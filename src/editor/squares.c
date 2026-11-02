/* squares.c - squares: made from a module, freed, disconnected */
#include "editor.h"

static bool
square_open(Square *s, Module *m)
{
        char path[PATH_MAX];

        if (m->builtin) {
                s->fn = *m->builtin;
                return true;
        }
        if (!tmp_dir_make()) return false;

        snprintf(path, sizeof path, "%s/%d-%s.so", tmp_dir, s->id, m->label);
        if (!copy_file(m->lib ? m->lib : m->path, path)) {
                fprintf(stderr, "modules: could not copy %s to %s\n", m->lib ? m->lib : m->path, path);
                remove(path);
                return false;
        }
        s->handle = module_open(path, &s->fn);
        /* Once open it's mapped in memory, the file is not needed */
        remove(path);
        return s->handle != NULL;
}

/* A new square of M with id ID (a new one if ID < 0), centered at CENTER */
Square *
square_new_id(Module *m, Vector2 center, int id)
{
        Square *s = calloc(1, sizeof *s);
        s->id     = id >= 0 ? id : next_id++;
        s->module = m;
        s->ir     = -1;
        squares_version++;
        if (s->id >= next_id) next_id = s->id + 1;
        if (!square_open(s, m)) {
                free(s);
                return NULL;
        }
        s->fn.init(s);
        /* Let the module apply the defaults the same way it applies changes */
        for (int i = 0; s->fn.param && i < s->params.count; i++)
                s->fn.param(s, i);

        s->pos = Vector2SubtractValue(center, square_side(s) / 2);
        Da_append(&squares, s);
        last_module = m;
        return s;
}

Square *
square_new(Module *m, Vector2 center)
{
        return square_new_id(m, center, -1);
}

void
pins_free(PinList *l)
{
        Da_foreach(p, *l)
        {
                free(p->name);
                free(p->str);
                for (int i = 0; i < p->option_count; i++)
                        free(p->options[i]);
                free(p->options);
        }
        Da_destroy(l);
}

void
square_free(Square *s)
{
        if (panel.square == s) panel_close();
        if (field_edit.square == s) field_edit.square = NULL;
        if (s->fn.destroy) s->fn.destroy(s);
        pins_free(&s->inputs);
        pins_free(&s->was);
        Da_foreach(p, s->outputs)
        {
                value_drop(p->value); /* li_set's copy */
        }
        pins_free(&s->outputs);
        Da_foreach(p, s->params)
        {
                free(p->name);
                free(p->str);
        }
        Da_destroy(&s->params);
        if (s->handle) dlclose(s->handle);
        free(s);
        squares_version++;
}

void
square_disconnect_inputs(int id, void *arg)
{
        (void) arg;
        for (int i = links.count - 1; i >= 0; i--) {
                if (links.items[i].to == id) Da_remove(&links, i);
        }
}

void
square_disconnect_outputs(int id, void *arg)
{
        (void) arg;
        for (int i = links.count - 1; i >= 0; i--) {
                if (links.items[i].from == id) Da_remove(&links, i);
        }
}

void
square_disconnect_all(int id, void *arg)
{
        square_disconnect_inputs(id, arg);
        square_disconnect_outputs(id, arg);
}

void
square_configure(int id, void *arg)
{
        (void) arg;
        Square *s = square_get(id);
        if (s) panel_open(s);
}

void
square_delete(int id, void *arg)
{
        Square *s = square_get(id);
        if (!s) return;
        square_disconnect_all(id, arg);
        Da_remove(&squares, square_index(s));
        square_free(s);
}
