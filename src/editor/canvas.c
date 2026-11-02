/* canvas.c - the canvas: its squares, links and camera */
#include "editor.h"
Camera2D camera = { .zoom = 1.0f };
SquareList squares; /* draw order: last is on top. Inside a subgraph, its own (see subgraphs.c) */
LinkList links;
int next_id;
Module *last_module;           /* the last one placed, first in the canvas menu */
long long squares_version = 1; /* bumped when a square is made or freed, see program.c */

/* Area of the screen where the canvas is drawn (everything but the bar and
 * the output panel). */
Rectangle
canvas_rect(void)
{
        float top = tabs_height();
        return (Rectangle) { 0, top, GetScreenWidth(), output_rect().y - top };
}

Vector2
mouse_world(void)
{
        return GetScreenToWorld2D(GetMousePosition(), camera);
}

Square *
square_get(int id)
{
        Da_foreach(s, squares)
        {
                if ((*s)->id == id) return *s;
        }
        return NULL;
}

int
square_index(Square *s)
{
        for (int i = 0; i < squares.count; i++) {
                if (squares.items[i] == s) return i;
        }
        return -1;
}

void
canvas_add_module(int id, void *arg)
{
        (void) id;
        Square *s    = square_new(arg, menu.world);
        Square *from = menu.wire_from >= 0 ? square_get(menu.wire_from) : NULL;
        if (!s || !from) return;

        /* The wire it was opened with goes to its first input that takes it */
        Li_Type type = pin_info(from, PART_OUTPUT, menu.wire_pin)->type;
        for (int i = 0; i < s->inputs.count; i++) {
                if (s->inputs.items[i].kind != PIN_OPTIONS && types_compatible(type, s->inputs.items[i].type)) {
                        link_add(from, menu.wire_pin, s, i);
                        break;
                }
        }
}

void
canvas_reset_view(int id, void *arg)
{
        (void) id, (void) arg;
        camera = (Camera2D) { .zoom = 1.0f };
}

void
canvas_clear(int id, void *arg)
{
        (void) id, (void) arg;
        Da_foreach(s, squares)
        {
                square_free(*s);
        }
        squares.count = 0;
        links.count   = 0;
        vars_clear();
}
