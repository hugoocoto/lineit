/* modules.c - modules: finding, loading and checking them */
#include "editor.h"

/* dlopen PATH and look up the callbacks. NULL if it fails or has no
 * module_init. */
void *
module_open(const char *path, ModuleFns *fn)
{
        void *handle = dlopen(path, RTLD_NOW | RTLD_LOCAL);
        if (!handle) {
                fprintf(stderr, "modules: %s\n", dlerror());
                return NULL;
        }

        fn->init    = (void (*)(Li_Node *)) dlsym(handle, "module_init");
        fn->tick    = (void (*)(Li_Node *, int)) dlsym(handle, "module_tick");
        fn->eval    = (void (*)(Li_Node *)) dlsym(handle, "module_eval");
        fn->frame   = (void (*)(Li_Node *)) dlsym(handle, "module_frame");
        fn->destroy = (void (*)(Li_Node *)) dlsym(handle, "module_destroy");
        fn->param   = (void (*)(Li_Node *, int)) dlsym(handle, "module_param");
        fn->save    = (const char *(*)(Li_Node *) ) dlsym(handle, "module_save");
        fn->load    = (void (*)(Li_Node *, const char *)) dlsym(handle, "module_load");
        fn->draw    = (void (*)(Li_Node *)) dlsym(handle, "module_draw");
        fn->start   = (void (*)(Li_Node *)) dlsym(handle, "module_start");
        fn->emit    = (const char *(*)(Li_Node *) ) dlsym(handle, "module_emit");
        fn->libs    = (const char *(*)(void) ) dlsym(handle, "module_libs");
        if (!fn->init) {
                fprintf(stderr, "modules: %s has no module_init\n", path);
                dlclose(handle);
                return NULL;
        }
        return handle;
}

/* Whether the .so at PATH loads and has module_init. From a copy, like the
 * squares, so dlopen doesn't hand back one it has already loaded. */
static bool
module_check(const char *path)
{
        static int n;
        char copy[PATH_MAX];
        ModuleFns fn;

        if (!tmp_dir_make()) return false;
        snprintf(copy, sizeof copy, "%s/check-%d.so", tmp_dir, n++);
        bool ok = copy_file(path, copy);
        void *h = ok ? module_open(copy, &fn) : NULL;
        remove(copy);
        if (h) dlclose(h);
        return h != NULL;
}

/* module_menu with the folders of the modules that are there */
static void
module_menu_build(void)
{
        menu_free(&module_menu);
        Da_foreach(mp, modules)
        {
                Module *mod = *mp;
                Menu *m     = &module_menu;
                char *name  = strdup(mod->name);
                char *save, *next;
                char *part = strtok_r(name, "/", &save);

                if (mod->gone) {
                        free(name);
                        continue;
                }
                while ((next = strtok_r(NULL, "/", &save))) {
                        m    = menu_submenu(m, part);
                        part = next;
                }
                Da_append(m, (MenuItem) { .label = mod->label, .fn = canvas_add_module, .arg = mod });
                free(name);
        }
}

/* Where modules are: modules.path, read when lineit starts, then
 * modules.user, as the config says now. A module in both is the first's. */
static const char *modules_dir;

int
modules_dirs(const char *dirs[2])
{
        dirs[0] = modules_dir;
        dirs[1] = cfg.modules.user;
        return dirs[1] && *dirs[1] && strcmp(dirs[0], dirs[1]) ? 2 : 1;
}

/* "DIR/math/add.so" -> "math/add", to free. EXT: the length of its extension. */
char *
module_name_of(const char *path, const char *dir, size_t ext)
{
        const char *rel = path + strlen(dir);
        while (*rel == '/')
                rel++;
        return strndup(rel, strlen(rel) - ext);
}

/* PATH, found in DIR (modules_dirs()[DI]): a new module, a rebuilt one or
 * one that hasn't changed. See modules_scan. */
static void
module_scan_file(const char *path, const char *dir, int di, bool reload, long long now, bool *menu_changed)
{
        if (is_type_file(path)) return; /* types_scan's */
        long long mtime = file_mtime(path);
        bool settled    = !reload || now - mtime > MODULE_SETTLE_NS;

        char *name    = module_name_of(path, dir, strlen(GetFileExtension(path)));
        Module *m     = module_find(name);
        bool graph    = IsFileExtension(path, ".lua");
        bool package  = IsFileExtension(path, ".li");
        char *lib     = NULL; /* of the package */
        char why[256] = "doesn't load";

        if (m && m->dir != di) { /* the one in the first dir */
                free(name);
                return;
        }
        if (m) {
                m->seen = true;
                if (m->gone) {
                        m->gone       = false;
                        *menu_changed = true;
                }
                free(name);
                if (mtime == m->mtime || !settled) return;
                m->mtime = mtime; /* a broken build is tried again when it changes */
                if ((package && !(lib = package_lib(path, why, sizeof why))) ||
                    (graph && !canvas_check(path, why, sizeof why))) {
                        set_status("Module %s: %s, keeping the old one", m->name, why);
                        free(lib);
                        return;
                }
                if (!graph && !module_check(lib ? lib : path)) {
                        set_status("Module %s doesn't load, keeping the old one", m->name);
                        if (lib) remove(lib);
                        free(lib);
                        return;
                }
                if (lib) {
                        if (m->lib) remove(m->lib);
                        free(m->lib);
                        m->lib = lib;
                }
                module_reload_squares(m);
                set_status("Reloaded %s", m->name);
                return;
        }

        if (!settled) {
                free(name);
                return;
        }
        if (package) lib = package_lib(path, why, sizeof why);
        if ((package && !lib) || (graph && !canvas_check(path, why, sizeof why)) ||
            (!graph && !module_check(lib ? lib : path))) {
                fprintf(stderr, "modules: %s skipped: %s\n", path, why);
                if (reload) set_status("Module %s: %s", name, why);
                if (lib) remove(lib);
                free(lib);
                free(name);
                return;
        }
        m          = calloc(1, sizeof *m);
        m->lib     = lib;
        m->path    = strdup(path);
        m->builtin = graph ? &graph_fns : NULL;
        m->name    = name;
        m->label   = strrchr(m->name, '/') ? strrchr(m->name, '/') + 1 : m->name;
        m->mtime   = mtime;
        m->seen    = true;
        m->dir     = di;
        Da_append(&modules, m);
        *menu_changed = true;
        if (reload) set_status("Added module %s", m->name);
}

/* Look at every .so, .lua and .li in the modules dirs. The first time, load them all. Later
 * (RELOAD), add new ones, swap rebuilt ones in their squares, and take
 * removed ones out of the menu. */
void
modules_scan(bool reload)
{
        bool menu_changed = !reload;
        long long now     = now_ns();

        const char *dirs[2];
        int ndirs = modules_dirs(dirs);
        if (!reload && !DirectoryExists(modules_dir)) fprintf(stderr, "modules: %s not found\n", modules_dir);

        Da_foreach(mp, modules)

        {
                (*mp)->seen = !(*mp)->path; /* built-in, not in a dir */
        }
        types_scan(reload, &menu_changed); /* first: modules may use them */
        for (int di = 0; di < ndirs; di++) {
                if (!DirectoryExists(dirs[di])) continue;
                FilePathList files = LoadDirectoryFilesEx(dirs[di], ".so;.lua;.li", true);
                for (unsigned i = 0; i < files.count; i++)
                        module_scan_file(files.paths[i], dirs[di], di, reload, now, &menu_changed);
                UnloadDirectoryFiles(files);
        }

        /* Removed: their squares keep their copies, so they still work */
        Da_foreach(mp, modules)
        {
                if (!(*mp)->seen && !(*mp)->gone) {
                        (*mp)->gone  = true;
                        menu_changed = true;
                        set_status("Removed module %s", (*mp)->name);
                }
        }

        if (!menu_changed) return;
        qsort(modules.items, modules.count, sizeof *modules.items, module_cmp);
        module_menu_build();
        if (!last_module || last_module->gone) {
                last_module = NULL;
                Da_foreach(mp, modules)
                {
                        if (!last_module && !(*mp)->gone) last_module = *mp;
                }
        }
}

/* Load every module under DIR (.so, .lua subgraphs and .li packages), and build module_menu with the same folders */
void
load_modules(const char *dir)
{
        modules_dir = dir;
        graph_builtins();
        modules_scan(false);
}

void
unload_modules(void)
{
        menu_free(&module_menu);
        Da_destroy(&canvas_menu);
        Da_destroy(&square_menu);
        Da_destroy(&options_menu);
        Da_destroy(&search_menu);
        types_free();
        Da_foreach(mp, modules)
        {
                if ((*mp)->lib) remove((*mp)->lib);
                free((*mp)->lib);
                free((*mp)->path);
                free((*mp)->name);
                free(*mp);
        }
        Da_destroy(&modules);
        last_module = NULL;
        /* The copies are removed as soon as they are open */
        if (*tmp_dir) rmdir(tmp_dir);
}

Module *
module_find(const char *name)
{
        Da_foreach(mp, modules)
        {
                if (strcmp((*mp)->name, name) == 0) return *mp;
        }
        return NULL;
}

/* M was rebuilt: the squares of the canvas shown get new copies of it. Everything else in them
 * stays: position, params, typed inputs, wires, state, selection. */
static void
module_reload_squares_here(Module *m)
{
        Da(int) selected = { 0 };
        bool any         = false;

        Da_foreach(sp, squares)
        {
                any = any || square_uses(*sp, m);
                if ((*sp)->selected) Da_append(&selected, (*sp)->id);
        }
        if (!any) {
                Da_destroy(&selected);
                return;
        }

        char *text = canvas_text(WRITE_STATE);
        /* Close the old copies: restoring makes new ones from the new build */
        for (int i = squares.count - 1; i >= 0; i--) {
                if (square_uses(squares.items[i], m)) square_delete(squares.items[i]->id, NULL);
        }
        canvas_read_text(text, READ_RESTORE);
        free(text);

        Da_foreach(id, selected)
        {
                Square *s = square_get(*id);
                if (s) s->selected = true;
        }
        Da_destroy(&selected);
}

/* M was rebuilt: in every tab */
void
module_reload_squares(Module *m)
{
        if (!tabs.count) {
                module_reload_squares_here(m);
                return;
        }
        for (int t = 0; t < tabs.count; t++) {
                tab_visit(t);
                module_reload_squares_here(m);
                tab_visit(t);
        }
}
