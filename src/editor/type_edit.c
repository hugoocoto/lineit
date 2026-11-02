/* type_edit.c - New type and Edit type */
#include "editor.h"

/* Both ask for the fields in one line, "x:float y:float" (a field without
 * a type is any), and write the .type.lua (see types.c): a new one under
 * modules.user, like subgraphs, or the one
 * the type came from. Its squares pick it up right away. */

#define TYPE_FIELDS_HINT "fields as name:type, types: bool int float string any, or a custom type"

/* "x:float y:float" into OUT. False, with why in WHY, if it isn't fields.
 * SELF is the type being made: it can't be in itself. */
static bool
fields_parse(const char *text, const char *self, void *out, char *why, int size)
{
        Da(Field) *l = out;
        char buf[512];
        snprintf(buf, sizeof buf, "%s", text);
        for (char *save, *tok = strtok_r(buf, " ,\t", &save); tok; tok = strtok_r(NULL, " ,\t", &save)) {
                char *colon      = strchr(tok, ':');
                const char *type = colon ? colon + 1 : "any";
                if (colon) *colon = '\0';
                bool name_ok = *tok != '\0';
                for (char *c = tok; *c; c++)
                        name_ok = name_ok && (isalnum((unsigned char) *c) || *c == '_');
                bool dup = false;
                Da_foreach(f, *l)
                {
                        dup = dup || !strcmp(f->name, tok);
                }
                if (!name_ok || dup) {
                        snprintf(why, size, dup ? "two fields named %s" : "bad field name \"%s\" (letters, digits and _)", tok);
                        fields_free(l);
                        return false;
                }
                if (!strcmp(type, self) || !field_type_known(type, NULL, 0)) {
                        snprintf(why, size, "field %s: no type %s", tok, type);
                        fields_free(l);
                        return false;
                }
                Da_append(l, (Field) { strdup(tok), strdup(type) });
        }
        if (!l->count) {
                snprintf(why, size, "a type needs fields");
                return false;
        }
        return true;
}

static bool
type_write(const char *path, const char *name, void *fields, Color color)
{
        Da(Field) *l = fields;
        FILE *f      = fopen(path, "w");
        if (!f) return false;
        fprintf(f, "-- %s: a lineit type (see src/editor/types.c). Change it with Edit type...\n", name);
        fprintf(f, "-- on one of its make or break squares, or here.\nlineit_type = \"%s\"\nfields = {\n", LI_VERSION);
        Da_foreach(fl, *l)
        {
                fprintf(f, "        { name = ");
                lua_quote(f, fl->name);
                fprintf(f, ", type = ");
                lua_quote(f, fl->type);
                fprintf(f, " },\n");
        }
        fprintf(f, "}\ncolor = { %d, %d, %d }\n", color.r, color.g, color.b);
        return fclose(f) == 0;
}

/* "geo/point x:float y:float" */
static void
type_new_done(const char *text)
{
        char name[256], path[PATH_MAX], why[256];
        Da(Field) fields = { 0 };
        int len          = (int) strcspn(text, " \t");
        snprintf(name, sizeof name, "%.*s", len, text);

        bool ok = *name && name[0] != '/' && !strstr(name, "..");
        for (char *c = name; *c; c++)
                ok = ok && (isalnum((unsigned char) *c) || *c == '_' || *c == '/' || *c == '-');
        if (!ok) {
                set_status("Bad type name \"%s\": like geo/point", name);
                return;
        }
        if (type_from_name(name) >= 0) {
                set_status("There is a type %s already", name);
                return;
        }
        if (!fields_parse(text + len, name, &fields, why, sizeof why)) {
                set_status("Type %s: %s", name, why);
                return;
        }
        snprintf(path, sizeof path, "%s/%s.type.lua", cfg.modules.user, name);
        mkdir_parents(path);
        if (FileExists(path) || !type_write(path, name, &fields, type_color_for(path))) {
                set_status(FileExists(path) ? "%s already exists" : "Could not write %s", path);
                fields_free(&fields);
                return;
        }
        fields_free(&fields);
        modules_scan(false); /* now, not when it settles */
        char make[PATH_MAX], brk[PATH_MAX];
        type_module_name(name, "make", make, sizeof make);
        type_module_name(name, "break", brk, sizeof brk);
        set_status(module_find(make) ? "Made type %s: %s and %s are in the menu" : "Wrote %s, but it doesn't load",
                   name, make, brk);
}

void
type_new_ask(void)
{
        prompt_open("New type", "", type_new_done);
        prompt.no_complete = true;
        prompt.hint        = "name and fields, like geo/point x:float y:float";
}

/* Edit type...: the type of the square it was opened on */
static Li_Type type_editing;

static void
type_edit_done(const char *text)
{
        TypeDef *d = type_def(type_editing);
        char why[256], make[PATH_MAX];
        Da(Field) fields = { 0 };
        if (!d) return;
        type_module_name(d->name, "make", make, sizeof make);
        Module *m = module_find(make);
        if (!m || !fields_parse(text, d->name, &fields, why, sizeof why)) {
                if (m) set_status("Type %s: %s", d->name, why);
                return;
        }
        /* Through the link: to the file it was made from */
        bool ok = type_write(m->path, d->name, &fields, d->color);
        fields_free(&fields);
        if (!ok) {
                set_status("Could not write %s", m->path);
                return;
        }
        modules_scan(false); /* its squares change now */
}

void
type_edit(int id, void *arg)
{
        (void) arg;
        char text[512] = "";
        Square *s      = square_get(id);
        TypeDef *d     = s ? type_def(s->module->type) : NULL;
        if (!d) return;
        Da_foreach(f, d->fields)
        {
                size_t used = strlen(text);
                snprintf(text + used, sizeof text - used, "%s%s:%s", used ? " " : "", f->name, f->type);
        }
        type_editing = s->module->type;
        prompt_open(TextFormat("Type %s", d->name), text, type_edit_done);
        prompt.no_complete = true;
        prompt.hint        = TYPE_FIELDS_HINT;
}
