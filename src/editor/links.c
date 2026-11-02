/* links.c - links: which wires are allowed, and making them */
#include "editor.h"

/* Index of the last link plugged into input PIN of square ID, or -1 */
int
link_to(int id, int pin)
{
        for (int i = links.count - 1; i >= 0; i--) {
                if (links.items[i].to == id && links.items[i].to_pin == pin) return i;
        }
        return -1;
}

bool
pin_connected(Square *s, Part part, int pin)
{
        Da_foreach(l, links)
        {
                if (part == PART_INPUT && l->to == s->id && l->to_pin == pin) return true;
                if (part == PART_OUTPUT && l->from == s->id && l->from_pin == pin) return true;
        }
        return false;
}

/* Ticks only go to ticks. Data goes to the same type, or from/to LI_ANY. */
bool
types_compatible(Li_Type out, Li_Type in)
{
        /* A variable only goes to what reads or changes one */
        if (out == LI_TICK || in == LI_TICK || out == LI_REF || in == LI_REF) return out == in;
        /* As in C, li_get makes an int a float, and a bool an int or the
         * other way (0 is false) */
        if ((out == LI_INT && in == LI_FLOAT) || (out == LI_BOOL && in == LI_INT) || (out == LI_INT && in == LI_BOOL))
                return true;
        /* Bytes are a buffer: where a ptr goes, their data does (ffi/) */
        if (out == LI_BYTES && in == LI_PTR) return true;
        return out == in || out == LI_ANY || in == LI_ANY;
}

/* Any output can be wired to any input of another square. Wires between
 * incompatible types are kept, but carry nothing and show an error. */
bool
link_allowed(Square *from, int from_pin, Square *to, int to_pin)
{
        PinInfo *in = pin_info(to, PART_INPUT, to_pin);
        return pin_info(from, PART_OUTPUT, from_pin) && in && in->kind != PIN_OPTIONS && from != to;
}

bool
link_valid(Link k)
{
        Square *from = square_get(k.from);
        Square *to   = square_get(k.to);
        if (!from || !to) return false;
        return types_compatible(pin_info(from, PART_OUTPUT, k.from_pin)->type,
                                pin_info(to, PART_INPUT, k.to_pin)->type);
}

void
link_add(Square *from, int from_pin, Square *to, int to_pin)
{
        int l;

        if (!link_allowed(from, from_pin, to, to_pin)) return;

        /* Data inputs take one wire, tick inputs can merge many */
        if (pin_info(to, PART_INPUT, to_pin)->type != LI_TICK) {
                while ((l = link_to(to->id, to_pin)) >= 0)
                        Da_remove(&links, l);
        }
        Da_foreach(k, links)
        {
                if (k->from == from->id && k->from_pin == from_pin && k->to == to->id && k->to_pin == to_pin)
                        return;
        }
        Da_append(&links, (Link) { .from = from->id, .from_pin = from_pin, .to = to->id, .to_pin = to_pin });
}

/* A param can change the pins of S (li_pins_reset, ffi/call): the wires
 * to ones that aren't there anymore, or can't take one, go */
void
links_check(Square *s)
{
        for (int i = links.count - 1; i >= 0; i--) {
                Link k = links.items[i];
                if (k.from == s->id && k.from_pin >= s->outputs.count)
                        Da_remove(&links, i);
                else if (k.to == s->id && (k.to_pin >= s->inputs.count || s->inputs.items[k.to_pin].kind == PIN_OPTIONS))
                        Da_remove(&links, i);
        }
}
