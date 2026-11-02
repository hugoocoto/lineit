/* module_api.c - the module API (lineit.h): pins, values, text, params */
#include "editor.h"

static int
pin_add(PinList *l, const char *name, Li_Type type)
{
        Da_append(l, (PinInfo) { .name = strdup(name ? name : ""), .type = type });
        return l->count - 1;
}

/* Set the typed value of P, copying strings */
void
pin_store(PinInfo *p, Li_Value value)
{
        char *old = p->str;
        /* Strings and variable names are kept here */
        p->str   = value.type == LI_STRING || value.type == LI_REF ? strdup(value.s ? value.s : "") : NULL;
        p->value = value;
        if (p->str) p->value.s = p->str;
        free(old);
}

int
li_input(Li_Node *n, const char *name, Li_Type type)
{
        n->side = 0;
        return pin_add(&n->inputs, name, type);
}

/* Every pin goes, for the module to say them again: what a param
 * changed (ffi/call's prototype). Typed values stay with the inputs of the
 * same name and type said again. */
void
li_pins_reset(Li_Node *n)
{
        Da_foreach(p, n->outputs)
        {
                value_drop(p->value);
        }
        pins_free(&n->outputs);
        pins_free(&n->was);
        n->was    = n->inputs; /* li_input_value takes their values back */
        n->inputs = (PinList) { 0 };
        n->side   = 0;
}

int
li_input_value(Li_Node *n, const char *name, Li_Type type, Li_Value value)
{
        int i      = li_input(n, name, type);
        PinInfo *p = &n->inputs.items[i];
        p->kind    = PIN_VALUE;
        /* The one before li_pins_reset, if it's the same */
        Da_foreach(w, n->was)
        {
                if (w->kind == PIN_VALUE && w->type == type && !strcmp(w->name, name)) value = w->value;
        }
        pin_store(p, value);
        return i;
}

int
li_input_options(Li_Node *n, const char *name, const char *options, int chosen)
{
        int i      = li_input(n, name, LI_INT);
        PinInfo *p = &n->inputs.items[i];
        char *list = strdup(options ? options : "");
        char *save;

        p->kind = PIN_OPTIONS;
        for (char *o = strtok_r(list, "|", &save); o; o = strtok_r(NULL, "|", &save)) {
                p->options                    = realloc(p->options, (p->option_count + 1) * sizeof *p->options);
                p->options[p->option_count++] = strdup(o);
        }
        free(list);
        p->value = li_int(Clamp(chosen, 0, Max(p->option_count - 1, 0)));
        return i;
}

const char *
li_option(Li_Node *n, int input)
{
        PinInfo *p = pin_info(n, PART_INPUT, input);
        if (!p || p->kind != PIN_OPTIONS || p->value.i < 0 || p->value.i >= p->option_count) return "";
        return p->options[p->value.i];
}

int
li_output(Li_Node *n, const char *name, Li_Type type)
{
        n->side = 0;
        return pin_add(&n->outputs, name, type);
}

/* li_get and li_tick follow the wires of the program, see program.c */

void
li_output_type(Li_Node *n, int output, Li_Type type)
{
        PinInfo *out = pin_info(n, PART_OUTPUT, output);
        if (out && out->type != type) {
                out->type = type;
                n->side   = 0; /* its field may change */
        }
}

void
li_input_type(Li_Node *n, int input, Li_Type type)
{
        PinInfo *in = pin_info(n, PART_INPUT, input);
        if (in && in->type != type) {
                in->type = type;
                n->side  = 0;
        }
}

void
li_set(Li_Node *n, int output, Li_Value value)
{
        /* A copy of its own: the module's may not live long, and readers in
         * other branches may read it while the module makes the next one */
        PinInfo *out = pin_info(n, PART_OUTPUT, output);
        if (!out) return;
        Li_Value old = out->value;
        out->value   = value_dup(value);
        value_drop(old);
}

void *
li_data(Li_Node *n)
{
        return n->data;
}

void
li_set_data(Li_Node *n, void *data)
{
        n->data = data;
}

void
li_text(Li_Node *n, const char *fmt, ...)
{
        va_list ap;
        va_start(ap, fmt);
        vsnprintf(n->text, sizeof n->text, fmt, ap);
        va_end(ap);
}

void
li_print(Li_Node *n, const char *fmt, ...)
{
        char buf[1024];
        va_list ap;
        (void) n;
        va_start(ap, fmt);
        vsnprintf(buf, sizeof buf, fmt, ap);
        va_end(ap);
        /* In the panel and the terminal in the same order, whatever branch */
        pthread_mutex_lock(&output_lock);
        output_append(buf, false);
        printf("%s\n", buf);
        pthread_mutex_unlock(&output_lock);
}

double
li_time(void)
{
        return GetTime();
}

int
li_param(Li_Node *n, const char *name, Li_Value value)
{
        if (value.type != LI_INT && value.type != LI_FLOAT && value.type != LI_BOOL && value.type != LI_STRING) {
                fprintf(stderr, "%s: param %s must be int, float, bool or string\n", n->module->name, name);
                return -1;
        }

        Param p = { .name = strdup(name ? name : ""), .value = value };
        if (value.type == LI_STRING) {
                p.str     = strdup(value.s ? value.s : "");
                p.value.s = p.str;
        }
        Da_append(&n->params, p);
        return n->params.count - 1;
}

Li_Value
li_get_param(Li_Node *n, int param)
{
        if (param < 0 || param >= n->params.count) return (Li_Value) { .type = LI_NONE };
        return n->params.items[param].value;
}
