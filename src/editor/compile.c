/* compile.c - Compile: the program, as C */
#include "editor.h"

/* Compile (in the bar, or lineit --compile FILE) lowers the canvas to a
 * program, like Run, and writes it as C in compile.output/NAME.build/, NAME
 * the canvas file's:
 *
 *   nI.c      one per node: its module's C (modules.source/NAME.c),
 *             included with the callbacks renamed (module_tick is
 *             nI_tick). Each is its own object, so each square has its own
 *             globals, like its own copy of the .so in the editor.
 *   nI.o      of packages instead: the package, taken out to packages/,
 *             builds its src/ as one object with its own Makefile
 *             (lineit-o, see package_object), and each square gets a copy
 *             of it, with compile.objcopy, the callbacks renamed and
 *             everything else made local: its own globals too. What it
 *             needs to link (lineit-libs) goes to the program's link.
 *   main.c    the nodes, N[i], and the wires as functions: nI_inK gives
 *             input K of node I its value (pulling it from the output it
 *             comes from, or a constant if it's typed), nI_tickK calls the
 *             module_tick of every input output K ticks (through rt_tick,
 *             with rt_fork if there's more than one: see src/branches.h). li_get
 *             and li_tick are calls to them. Then setup (init, params,
 *             options and state, as they are now), start and frame.
 *   Makefile  builds compile.output/NAME with compile.cc, with the runtime
 *             (src/rt, built by `make` to compile.runtime).
 *
 * Pure squares whose module has module_emit (see lineit.h), but packages,
 * are inline
 * instead: no nI.c, no module_eval. Their C, from module_emit, is in
 * main.c, as nI_outK, and the inputs that read them call it right away,
 * so the C compiler sees through them (constants fold, calls go away).
 * Squares that read each other in a loop are left as calls: inline, the
 * loop would never end.
 *
 * Then it runs make there, with what it says in the output panel. The
 * program does what Run does, without the editor: li_text shows nothing,
 * li_print prints to stdout. If no square has module_frame it ends after
 * start, if not it runs frames (60 a second) until it's killed.
 *
 * Modules don't need anything for this, their C is already what runs. But
 * everything in them but the callbacks has to be static: there's a copy of
 * them per square in one program. */

/* What Compile knows of each node, by index */
typedef struct CompileNode {
        char *src;  /* its module's C file */
        char *obj;  /* or, for packages, its object, relative to the build dir */
        char *libs; /* and what it needs to link, lineit-libs */
        char *emit; /* module_emit, expanded. NULL: not inline */
} CompileNode;

static Da(CompileNode) compiled;

/* The C callbacks that the program calls */
static const char *const callback_names[] = { "init", "tick", "eval", "frame", "param", "load", "start" };
/* And the ones it doesn't, but may be in a module */
static const char *const callback_others[] = { "draw", "destroy", "save", "emit", "libs" };

static bool
node_is_make(Node *x)
{
        return x->square->module->builtin == &type_make_fns;
}

static bool
node_is_break(Node *x)
{
        return x->square->module->builtin == &type_break_fns;
}

/* Whether node I is inline, see module_emit */
static bool
node_inline(int i)
{
        return compiled.items[i].emit != NULL;
}

/* Whether node I has its own nI.c, with callback NAME in it */
static bool
node_has(int i, const char *name)
{
        Node *x = &prog.nodes.items[i];
        if (node_is_make(x) || node_is_break(x) || node_inline(i)) return false;
        ModuleFns *f = &x->square->fn;
        void *fns[]  = { f->init, f->tick, f->eval, f->frame, f->param, f->load, f->start };
        for (int c = 0; c < countof(callback_names); c++)
                if (!strcmp(callback_names[c], name)) return fns[c] != NULL;
        return false;
}

/* Whether input K of S gets a function in main.c: options are in the node,
 * and ticks carry no value */
static bool
input_has_get(Square *s, int k)
{
        PinInfo *in = &s->inputs.items[k];
        return in->kind != PIN_OPTIONS && in->type != LI_TICK;
}

/* The C function that is module_eval of node I, "NULL" if it has none */
static const char *
node_eval(int i, char *buf, int size)
{
        Node *x = &prog.nodes.items[i];
        if (node_is_make(x)) return "rt_make_eval";
        if (node_is_break(x)) return "rt_break_eval";
        if (!node_has(i, "eval")) return "NULL";
        snprintf(buf, size, "n%d_eval", i);
        return buf;
}

static void
c_string(FILE *f, const char *s)
{
        fputc('"', f);
        for (const unsigned char *c = (const unsigned char *) (s ? s : ""); *c; c++) {
                if (*c == '"' || *c == '\\')
                        fprintf(f, "\\%c", *c);
                else if (*c >= 0x20 && *c < 0x7f)
                        fputc(*c, f);
                else
                        fprintf(f, "\\%03o", *c);
        }
        fputc('"', f);
}

/* V as a C expression */
/* rt_tick of target T: its node's tick, or nothing if it has none */
static bool node_has(int i, const char *fn);

static void
c_tick(FILE *f, Target t)
{
        Node *x    = &prog.nodes.items[t.node];
        bool merge = t.pin < 32 && (x->merge & 1u << t.pin);
        if (node_has(t.node, "tick"))
                fprintf(f, "        rt_tick(&N[%d], n%d_tick, %d, %d, %s);\n", t.node, t.node, t.pin, t.node, merge ? "true" : "false");
}

static void
c_value(FILE *f, Li_Value v)
{
        switch (v.type) {
        case LI_BOOL: fprintf(f, "li_bool(%s)", v.b ? "true" : "false"); break;
        case LI_INT:
                if (v.i == LONG_MIN)
                        fprintf(f, "li_int(-%ldL - 1)", LONG_MAX);
                else
                        fprintf(f, "li_int(%ldL)", v.i);
                break;
        case LI_FLOAT:
                if (isnan(v.f))
                        fprintf(f, "li_float(__builtin_nan(\"\"))");
                else if (isinf(v.f))
                        fprintf(f, "li_float(%s__builtin_inf())", v.f < 0 ? "-" : "");
                else
                        fprintf(f, "li_float(%a)", v.f); /* exact */
                break;
        case LI_STRING:
                fprintf(f, "li_string(");
                c_string(f, v.s);
                fprintf(f, ")");
                break;
        case LI_REF:
                fprintf(f, "li_var_ref(");
                c_string(f, v.s);
                fprintf(f, ")");
                break;
        default: fprintf(f, "(Li_Value) { .type = LI_NONE }"); break;
        }
}

/* The package M built for Compile: taken out to DIR (but lib/), and its
 * Makefile's lineit-o run there to make OBJ. What it needs to link
 * (lineit-libs) in LIBS. False, saying why in the output, if it can't be. */
static bool
package_object(Module *m, const char *dir, const char *obj, char *libs, int libs_size)
{
        int zsize          = 0;
        unsigned char *zip = LoadFileData(m->path, &zsize);
        char why[512]      = "", qdir[PATH_MAX * 4 + 8], cmd[PATH_MAX * 4 + 64];
        bool ok            = false;

        if (!zip) {
                snprintf(why, sizeof why, "can't read it");
                goto done;
        }
        if (!package_extract(zip, zsize, dir, why, sizeof why)) goto done;
        /* Its own build: whatever src/ has, as one object */
        if (!package_make(dir, "lineit-o", obj)) {
                snprintf(why, sizeof why, "make lineit-o failed in %s", dir);
                goto done;
        }
        /* stdout only: it's the flags, what make says goes to stderr */
        shell_quote(qdir, sizeof qdir, dir);
        snprintf(cmd, sizeof cmd, "cd %s && make -s lineit-libs", qdir);
        if (!run_to_buf(cmd, libs, libs_size)) {
                snprintf(why, sizeof why, "make lineit-libs failed in %s", dir);
                goto done;
        }
        ok = true;
done:
        if (!ok) output_add(TextFormat("compile: package %s: %s", m->name, why), true);
        if (zip) UnloadFileData(zip);
        return ok;
}

/* TEXT from module_emit of node I, with $_ and $source replaced (see
 * lineit.h). A new string. */
static char *
emit_expand(const char *text, int i, const char *src)
{
        char *out;
        size_t len;
        FILE *f = open_memstream(&out, &len);
        for (const char *p = text; *p; p++) {
                if (p[0] == '$' && p[1] == '_') {
                        fprintf(f, "n%d", i);
                } else if (!strncmp(p, "$source", 7)) {
                        fputs(src, f);
                        p += 6;
                } else {
                        fputc(*p, f);
                }
        }
        fclose(f);
        return out;
}

/* Whether node I reads itself through inline nodes. SEEN: the ones
 * visited from node FROM. */
static bool
inline_loop(int from, int i, bool *seen)
{
        Node *x = &prog.nodes.items[i];
        for (int k = 0; k < x->square->inputs.count; k++) {
                Source src = prog.sources.items[x->inputs + k];
                if (src.kind != SRC_OUTPUT || !node_inline(src.node)) continue;
                if (src.node == from) return true;
                if (seen[src.node]) continue;
                seen[src.node] = true;
                if (inline_loop(from, src.node, seen)) return true;
        }
        return false;
}

/* The scope of node X as a number: 0 the canvas's, then one per subgraph
 * square, in the order they come */
typedef Da(Graph *) GraphList;

static int
scope_index(GraphList *scopes, Graph *g)
{
        if (!g) return 0;
        for (int i = 0; i < scopes->count; i++)
                if (scopes->items[i] == g) return i + 1;
        Da_append(scopes, g);
        return scopes->count;
}

static void
compile_main(FILE *f, const char *name, const char *from)
{
        GraphList scopes = { 0 };
        char eval[32];
        bool frames = false, any_inline = false;

        fprintf(f, "/* %s, compiled by lineit from %s. See src/editor/compile.c:\n", name, from);
        fprintf(f, " * N[I] is node I, its module is in nI.c (or below, as nI_outK, if it's\n");
        fprintf(f, " * inline), and its wires are nI_inK (where input K gets its value) and\n");
        fprintf(f, " * nI_tickK (what output K ticks). */\n");
        fprintf(f, "#include \"rt.h\"\n\n");

        fprintf(f, "/* The modules: in nI.c, and the inline ones below */\n");
        for (int i = 0; i < prog.nodes.count; i++) {
                Square *s = prog.nodes.items[i].square;
                for (int c = 0; c < countof(callback_names); c++) {
                        const char *cb = callback_names[c];
                        if (!node_has(i, cb)) continue;
                        if (!strcmp(cb, "tick") || !strcmp(cb, "param"))
                                fprintf(f, "void n%d_%s(Li_Node *n, int i);\n", i, cb);
                        else if (!strcmp(cb, "load"))
                                fprintf(f, "void n%d_%s(Li_Node *n, const char *state);\n", i, cb);
                        else
                                fprintf(f, "void n%d_%s(Li_Node *n);\n", i, cb);
                }
                for (int k = 0; node_inline(i) && k < s->outputs.count; k++)
                        fprintf(f, "static Li_Value n%d_out%d(void) __attribute__((unused)); /* maybe nothing reads it */\n", i, k);
        }
        fprintf(f, "\nstatic Li_Node N[%d];\n", prog.nodes.count ? prog.nodes.count : 1);

        /* The wires */
        for (int i = 0; i < prog.nodes.count; i++) {
                Node *x   = &prog.nodes.items[i];
                Square *s = x->square;
                fprintf(f, "\n/* N[%d]: square %d, %s%s */\n", i, s->id, s->module->name, node_inline(i) ? ", inline" : "");

                for (int k = 0; k < s->inputs.count; k++) {
                        PinInfo *in = &s->inputs.items[k];
                        Source src  = prog.sources.items[x->inputs + k];
                        if (!input_has_get(s, k)) continue;
                        fprintf(f, "static Li_Value n%d_in%d(void) { return ", i, k);
                        if (src.kind == SRC_OUTPUT && node_inline(src.node)) {
                                fprintf(f, "n%d_out%d();", src.node, src.pin);
                        } else if (src.kind == SRC_OUTPUT) {
                                fprintf(f, "rt_pull(&N[%d], %s, %d, %d);", src.node, node_eval(src.node, eval, sizeof eval), src.pin, src.node);
                        } else if (src.kind == SRC_TYPED) {
                                c_value(f, pin_info(src.square, PART_INPUT, src.pin)->value);
                                fprintf(f, ";");
                        } else {
                                fprintf(f, "(Li_Value) { .type = LI_NONE };");
                        }
                        fprintf(f, *in->name ? " } /* %s */\n" : " }\n", in->name);
                }
                if (node_inline(i)) continue; /* nothing reads them through li_get */

                for (int k = 0; k < s->outputs.count; k++) {
                        Out o = prog.outs.items[x->outputs + k];
                        if (!o.count) continue;
                        /* A fan-out: a branch per target, see branches.h */
                        for (int t = o.first; o.count > 1 && t < o.first + o.count; t++) {
                                fprintf(f, "static void n%d_tick%d_%d(void *arg)\n{\n        (void) arg;\n", i, k, t - o.first);
                                c_tick(f, prog.targets.items[t]);
                                fprintf(f, "}\n");
                        }
                        fprintf(f, "static void n%d_tick%d(void)", i, k);
                        fprintf(f, *s->outputs.items[k].name ? " /* %s */\n{\n" : "\n{\n", s->outputs.items[k].name);
                        if (o.count == 1) {
                                c_tick(f, prog.targets.items[o.first]);
                        } else {
                                fprintf(f, "        static const Sched_Task tasks[] = {\n");
                                for (int t = o.first; t < o.first + o.count; t++)
                                        fprintf(f, "                { n%d_tick%d_%d, NULL, %d, %d },\n", i, k, t - o.first,
                                                prog.targets.items[t].node, prog.targets.items[t].pin);
                                fprintf(f, "        };\n        rt_fork(tasks, %d);\n", o.count);
                        }
                        fprintf(f, "}\n");
                }
                if (s->inputs.count) {
                        fprintf(f, "static Li_Value (*const n%d_get[])(void) = {", i);
                        for (int k = 0; k < s->inputs.count; k++) {
                                if (!input_has_get(s, k))
                                        fprintf(f, "%sNULL", k ? ", " : " ");
                                else
                                        fprintf(f, "%sn%d_in%d", k ? ", " : " ", i, k);
                        }
                        fprintf(f, " };\n");
                }
                if (s->outputs.count) {
                        fprintf(f, "static void (*const n%d_ticks[])(void) = {", i);
                        for (int k = 0; k < s->outputs.count; k++) {
                                Out o = prog.outs.items[x->outputs + k];
                                if (o.count)
                                        fprintf(f, "%sn%d_tick%d", k ? ", " : " ", i, k);
                                else
                                        fprintf(f, "%sNULL", k ? ", " : " ");
                        }
                        fprintf(f, " };\n");
                }
        }

        /* Inline squares, as their modules wrote them */
        for (int i = 0; i < prog.nodes.count; i++) {
                if (!node_inline(i)) continue;
                if (!any_inline) fprintf(f, "\n/* ---- Inline squares, from their module_emit ---- */\n");
                any_inline = true;
                fprintf(f, "\n/* N[%d]: square %d, %s */\n%s", i, prog.nodes.items[i].square->id,
                        prog.nodes.items[i].square->module->name, compiled.items[i].emit);
        }

        fprintf(f, "\nstatic Li_Node N[%d] = {\n", prog.nodes.count ? prog.nodes.count : 1);
        for (int i = 0; i < prog.nodes.count; i++) {
                Node *x   = &prog.nodes.items[i];
                Square *s = x->square;
                fprintf(f, "        { .module = ");
                c_string(f, s->module->name);
                fprintf(f, ", .square = %d, .scope = %d", s->id, scope_index(&scopes, x->scope));
                if (node_is_make(x) || node_is_break(x)) fprintf(f, ", .type = %d", (int) s->module->type);
                if (s->inputs.count && !node_inline(i)) fprintf(f, ", .get = n%d_get, .get_count = %d", i, s->inputs.count);
                if (s->outputs.count && !node_inline(i)) fprintf(f, ", .tick = n%d_ticks, .tick_count = %d", i, s->outputs.count);
                fprintf(f, " },\n");
        }
        fprintf(f, "};\n\n");
        fprintf(f, "const int rt_scope_count = %d;\n\n", scopes.count + 1);

        /* Every type, so their ids are the editor's */
        fprintf(f, "const Rt_Type rt_types[] = {\n");
        for (int t = 0; t < types.count; t++) {
                TypeDef *d = types.items[t];
                fprintf(f, "        { ");
                c_string(f, d->name);
                fprintf(f, ", %d, (const char *const[]) {", d->fields.count);
                for (int k = 0; k < d->fields.count; k++) {
                        fprintf(f, "%s", k ? ", " : " ");
                        c_string(f, d->fields.items[k].name);
                }
                fprintf(f, "%s}, (const char *const[]) {", d->fields.count ? " " : " NULL ");
                for (int k = 0; k < d->fields.count; k++) {
                        fprintf(f, "%s", k ? ", " : " ");
                        c_string(f, d->fields.items[k].type ? d->fields.items[k].type : "");
                }
                fprintf(f, "%s} },\n", d->fields.count ? " " : " NULL ");
        }
        if (!types.count) fprintf(f, "        { 0 },\n");
        fprintf(f, "};\nconst int rt_type_count = %d;\n\n", types.count);

        /* As the squares are now: params, options and state */
        fprintf(f, "static void\nsetup(void)\n{\n");
        for (int i = 0; i < prog.nodes.count; i++) {
                Node *x   = &prog.nodes.items[i];
                Square *s = x->square;
                if (node_inline(i)) continue;
                fprintf(f, "        rt_scope(N[%d].scope);\n", i);
                if (node_is_make(x) || node_is_break(x))
                        fprintf(f, "        rt_pins(&N[%d], %d, %d);\n", i, s->inputs.count, s->outputs.count);
                else if (node_has(i, "init"))
                        fprintf(f, "        n%d_init(&N[%d]);\n", i, i);
                for (int k = 0; k < s->inputs.count; k++) {
                        PinInfo *in = &s->inputs.items[k];
                        if (in->kind == PIN_OPTIONS) fprintf(f, "        rt_option(&N[%d], %d, %ld);\n", i, k, in->value.i);
                }
                for (int k = 0; k < s->params.count; k++) {
                        fprintf(f, "        rt_param(&N[%d], %d, ", i, k);
                        c_value(f, s->params.items[k].value);
                        fprintf(f, ");\n");
                        if (node_has(i, "param")) fprintf(f, "        n%d_param(&N[%d], %d);\n", i, i, k);
                }
                const char *state = s->fn.save && node_has(i, "load") ? s->fn.save(s) : NULL;
                if (state) {
                        fprintf(f, "        n%d_load(&N[%d], ", i, i);
                        c_string(f, state);
                        fprintf(f, ");\n");
                }
        }
        fprintf(f, "        rt_scope(0);\n}\n\n");

        /* The ticks between nodes, for merges (see branches.h) */
        fprintf(f, "const int rt_node_count = %d;\nconst int rt_next_first[] = {", prog.nodes.count);
        int next = 0;
        for (int i = 0; i < prog.nodes.count; i++) {
                fprintf(f, "%s%d", i ? ", " : " ", next);
                Node *x = &prog.nodes.items[i];
                for (int k = 0; k < x->square->outputs.count; k++)
                        next += prog.outs.items[x->outputs + k].count;
        }
        fprintf(f, "%s%d };\nconst int rt_next[] = {", prog.nodes.count ? ", " : " ", next);
        for (int i = 0; i < prog.nodes.count; i++) {
                Node *x = &prog.nodes.items[i];
                for (int k = 0; k < x->square->outputs.count; k++) {
                        Out o = prog.outs.items[x->outputs + k];
                        for (int t = o.first; t < o.first + o.count; t++)
                                fprintf(f, " %d,", prog.targets.items[t].node);
                }
        }
        fprintf(f, " -1 };\n\n");

        /* The starts, and the clocks of a frame: a branch each, in order */
        const char *each[] = { "start", "frame" };
        for (int e = 0; e < 2; e++) {
                int count = 0;
                for (int i = 0; i < prog.nodes.count; i++) {
                        if (!node_has(i, each[e]) || (e == 0 && square_is_ticked(prog.nodes.items[i].square))) continue;
                        fprintf(f, "static void\n%s_%d(void *arg)\n{\n        (void) arg;\n        rt_start(&N[%d], n%d_%s, %d);\n}\n\n",
                                each[e], i, i, i, each[e], i);
                        count++;
                }
                if (e == 1) frames = count > 0;
                if (e == 1 && !frames) break;
                fprintf(f, "static void\n%s(void)\n{\n", each[e]);
                /* var/var says what its variable starts as, and squares are
                 * set up, before the branches start, like Run does */
                for (int i = 0; e == 0 && i < prog.nodes.count; i++) {
                        if (is_decl(prog.nodes.items[i].square) && node_has(i, "eval"))
                                fprintf(f, "        rt_pull(&N[%d], n%d_eval, 0, %d);\n", i, i, i);
                        if (node_has(i, "start") && square_is_ticked(prog.nodes.items[i].square))
                                fprintf(f, "        rt_call(&N[%d], n%d_start);\n", i, i);
                }
                if (count) {
                        fprintf(f, "        static const Sched_Task tasks[] = {\n");
                        for (int i = 0; i < prog.nodes.count; i++)
                                if (node_has(i, each[e]) && !(e == 0 && square_is_ticked(prog.nodes.items[i].square)))
                                        fprintf(f, "                { %s_%d, NULL, %d, -1 },\n", each[e], i, i);
                        fprintf(f, "        };\n        rt_run(tasks, %d);\n", count);
                }
                fprintf(f, "}\n\n");
        }

        fprintf(f, "int\nmain(int argc, char **argv)\n{\n");
        fprintf(f, "        return rt_main(argc, argv, setup, start, %s);\n}\n", frames ? "frame" : "NULL");
        Da_destroy(&scopes);
}

static void
compiled_free(void)
{
        Da_foreach(c, compiled)
        {
                free(c->src);
                free(c->obj);
                free(c->libs);
                free(c->emit);
        }
        compiled.count = 0;
}

/* Where the C of each node is, and which are inline, to `compiled`. Its
 * packages are taken out to DIR/packages and built there. False, saying
 * why in the output, if some module has no C. */
static bool
compile_sources(const char *dir)
{
        char path[PATH_MAX * 2], src[PATH_MAX];
        bool ok = true;

        for (int i = 0; i < prog.nodes.count; i++) {
                Node *x   = &prog.nodes.items[i];
                Module *m = x->square->module;
                Da_append(&compiled, (CompileNode) { 0 });
                if (node_is_make(x) || node_is_break(x)) continue;

                /* The same module as one before: the same C (or none, already said) */
                int same = -1;
                for (int j = 0; j < i && same < 0; j++)
                        if (prog.nodes.items[j].square->module == m) same = j;
                if (same >= 0) {
                        CompileNode *before   = &compiled.items[same];
                        compiled.items[i].src = before->src ? strdup(before->src) : NULL;
                        compiled.items[i].obj = before->obj ? strdup(before->obj) : NULL;
                        /* Its libraries are already in the link, with the first */
                } else if (m->lib) {
                        char obj[PATH_MAX * 2], libs[1024];
                        snprintf(path, sizeof path, "%s/packages/%s", dir, m->name);
                        snprintf(obj, sizeof obj, "%s.o", path);
                        if (package_object(m, path, obj, libs, sizeof libs)) {
                                compiled.items[i].obj  = strdup(TextFormat("packages/%s.o", m->name));
                                compiled.items[i].libs = strdup(libs);
                        }
                } else {
                        /* With lineit's (modules.source), or the user's own, next to its .so */
                        snprintf(path, sizeof path, "%s/%s.c", m->dir ? cfg.modules.user : cfg.modules.source, m->name);
                        if (!m->builtin && realpath(path, src)) {
                                compiled.items[i].src = strdup(src);
                                /* What it needs to link, if it says (module_libs) */
                                const char *libs = x->square->fn.libs ? x->square->fn.libs() : NULL;
                                if (libs) compiled.items[i].libs = strdup(libs);
                        } else
                                output_add(TextFormat("compile: square %d: no C for %s, it should be %s", x->square->id, m->name, path), true);
                }
                if (!compiled.items[i].src && !compiled.items[i].obj) {
                        ok = false;
                        continue;
                }
                if (!compiled.items[i].src) continue; /* a package: not inline */

                /* Inline if it's pure and says how */
                ModuleFns *fn    = &x->square->fn;
                const char *text = fn->emit && !fn->tick && !fn->frame && !fn->start ? fn->emit(x->square) : NULL;
                if (text) compiled.items[i].emit = emit_expand(text, i, compiled.items[i].src);
        }

        /* Not the ones that read themselves */
        bool *seen = calloc(prog.nodes.count + 1, sizeof *seen);
        for (int i = 0; i < prog.nodes.count; i++) {
                if (!node_inline(i)) continue;
                memset(seen, 0, prog.nodes.count * sizeof *seen);
                if (inline_loop(i, i, seen)) {
                        free(compiled.items[i].emit);
                        compiled.items[i].emit = NULL;
                }
        }
        free(seen);
        return ok;
}

/* Writes FILE of DIR from F, an open_memstream of *TEXT and *LEN (they are
 * only set when it's closed) */
static bool
write_text(const char *dir, const char *file, FILE *f, char **text, size_t *len)
{
        char path[PATH_MAX * 2];
        fclose(f);
        snprintf(path, sizeof path, "%s/%s", dir, file);
        bool ok = write_if_changed(path, *text, *len);
        free(*text);
        return ok;
}

/* The canvas shown, compiled to compile.output/NAME, its path in BIN.
 * What went wrong goes to the output panel. */
bool
compile_canvas(const char *name, char *bin, int size)
{
        char dir[PATH_MAX], out[PATH_MAX], rt[PATH_MAX], file[64];
        char base[128], *text;
        const char *from = canvas_path ? canvas_path : "a canvas never saved";
        size_t len;
        bool ok = true;
        FILE *f;

        const char *bad = decls_check();
        if (bad) {
                output_add(TextFormat("compile: the squares of variable %s say different things", bad), true);
                return false;
        }
        program_lower();
        squares_settle();
        program_lower();
        file_name(base, sizeof base, name);
        if (!mkdir_p(cfg.compile.output) || !realpath(cfg.compile.output, out)) {
                output_add(TextFormat("compile: can't make %s", cfg.compile.output), true);
                return false;
        }
        if (!realpath(cfg.compile.runtime, rt) || !FileExists(TextFormat("%s/liblineit-rt.a", rt))) {
                output_add(TextFormat("compile: no runtime in %s (compile.runtime), build it with make", cfg.compile.runtime), true);
                return false;
        }
        snprintf(dir, sizeof dir, "%s/%s.build", out, base);
        snprintf(bin, size, "%s/%s", out, base);
        if (!mkdir_p(dir)) {
                output_add(TextFormat("compile: can't make %s", dir), true);
                return false;
        }
        if (!compile_sources(dir)) {
                compiled_free();
                return false;
        }

        /* What an older compile left that isn't used now */
        for (int i = 0; i < prog.nodes.count + 1024; i++) {
                bool c = i < prog.nodes.count && compiled.items[i].src && !node_inline(i);
                bool o = c || (i < prog.nodes.count && compiled.items[i].obj);
                if (i >= prog.nodes.count && !FileExists(TextFormat("%s/n%d.o", dir, i))) break;
                if (!c) remove(TextFormat("%s/n%d.c", dir, i));
                if (!c) remove(TextFormat("%s/n%d.d", dir, i));
                if (!o) remove(TextFormat("%s/n%d.o", dir, i));
        }

        /* One file per node that isn't inline, with its module in it */
        for (int i = 0; i < prog.nodes.count; i++) {
                Square *s = prog.nodes.items[i].square;
                if (!compiled.items[i].src || node_inline(i)) continue;
                f = open_memstream(&text, &len);
                fprintf(f, "/* N[%d] of %s: square %d, %s. Made by lineit's Compile. */\n", i, base, s->id, s->module->name);
                for (int c = 0; c < countof(callback_names); c++)
                        fprintf(f, "#define module_%s n%d_%s\n", callback_names[c], i, callback_names[c]);
                /* Not called, but they may be there */
                for (int c = 0; c < countof(callback_others); c++)
                        fprintf(f, "#define module_%s n%d_%s\n", callback_others[c], i, callback_others[c]);
                fprintf(f, "#include ");
                c_string(f, compiled.items[i].src);
                fprintf(f, "\n");
                snprintf(file, sizeof file, "n%d.c", i);
                ok = write_text(dir, file, f, &text, &len) && ok;
        }

        f = open_memstream(&text, &len);
        compile_main(f, base, from);
        ok = write_text(dir, "main.c", f, &text, &len) && ok;

        f = open_memstream(&text, &len);
        fprintf(f, "# %s, compiled by lineit from %s: make builds it again\n", base, from);
        fprintf(f, "CC = %s\nCFLAGS = %s\nOBJCOPY = %s\nRT = %s\n\n", cfg.compile.cc, cfg.compile.cflags, cfg.compile.objcopy, rt);
        fprintf(f, "OBJS = main.o");
        for (int i = 0; i < prog.nodes.count; i++)
                if ((compiled.items[i].src && !node_inline(i)) || compiled.items[i].obj) fprintf(f, " n%d.o", i);
        /* What the packages need, from their lineit-libs */
        fprintf(f, "\nLIBS =");
        for (int i = 0; i < prog.nodes.count; i++)
                if (compiled.items[i].libs && *compiled.items[i].libs) fprintf(f, " %s", compiled.items[i].libs);
        fprintf(f, "\n\n../%s: $(OBJS) $(RT)/liblineit-rt.a\n", base);
        fprintf(f, "\t$(CC) $(CFLAGS) $(OBJS) $(RT)/liblineit-rt.a $(LIBS) -lm -o $@\n\n");
        fprintf(f, "%%.o: %%.c\n\t$(CC) $(CFLAGS) -MMD -MP -I$(RT)/include -c $< -o $@\n");
        /* A header gone since the last build (a .d still has it): rebuild, not fail */
        fprintf(f, "%%.h: ;\n");

        /* Packages: a copy of their object per square, only the callbacks
         * global, renamed */
        fprintf(f, "\nKEEP =");
        for (int c = 0; c < countof(callback_names); c++)
                fprintf(f, " --keep-global-symbol=module_%s", callback_names[c]);
        for (int c = 0; c < countof(callback_others); c++)
                fprintf(f, " --keep-global-symbol=module_%s", callback_others[c]);
        fprintf(f, "\n");
        for (int i = 0; i < prog.nodes.count; i++) {
                if (!compiled.items[i].obj) continue;
                fprintf(f, "\nn%d.o: %s\n\t$(OBJCOPY) $(KEEP) $< $@\n\t$(OBJCOPY)", i, compiled.items[i].obj);
                for (int c = 0; c < countof(callback_names); c++)
                        fprintf(f, " --redefine-sym module_%s=n%d_%s", callback_names[c], i, callback_names[c]);
                for (int c = 0; c < countof(callback_others); c++)
                        fprintf(f, " --redefine-sym module_%s=n%d_%s", callback_others[c], i, callback_others[c]);
                fprintf(f, " $@\n");
        }
        fprintf(f, "\n-include $(OBJS:.o=.d)\n");
        ok = write_text(dir, "Makefile", f, &text, &len) && ok;
        compiled_free();
        if (!ok) {
                output_add(TextFormat("compile: can't write in %s", dir), true);
                return false;
        }

        /* Build it, with what make and the compiler say in the output */
        char quoted[PATH_MAX * 4 + 8], cmd[PATH_MAX * 4 + 64];
        shell_quote(quoted, sizeof quoted, dir);
        snprintf(cmd, sizeof cmd, "cd %s && make -s -j%ld 2>&1", quoted, sysconf(_SC_NPROCESSORS_ONLN));
        return run_to_output(cmd);
}

/* lineit --compile PATH: Compile without the editor. Prints where the
 * program went, or what went wrong. */
int
compile_file(const char *config, const char *path)
{
        char bin[PATH_MAX];
        bool ok = false;

        SetTraceLogLevel(LOG_WARNING);
        config_start(config);
        /* No window: no fonts, squares are measured as 0, they're not shown */
        load_modules(cfg.modules.path);
        canvas_path = path;
        if (!canvas_read_file(path, READ_LOAD))
                output_add(TextFormat("compile: can't read %s", path), true);
        else
                ok = compile_canvas(GetFileNameWithoutExt(path), bin, sizeof bin);
        Da_foreach(l, output_panel.lines)
        {
                fprintf(stderr, "%s\n", l->text);
        }
        if (ok) printf("%s\n", bin);
        canvas_clear(-1, NULL);
        unload_modules();
        canvas_path = NULL;
        return ok ? 0 : 1;
}

/* Whether S is of M, or is a subgraph with one of M inside */
bool
square_uses(Square *s, Module *m)
{
        if (s->module == m) return true;
        if (!s->graph) return false;
        Da_foreach(sp, s->graph->squares)
        {
                if (square_uses(*sp, m)) return true;
        }
        return false;
}
