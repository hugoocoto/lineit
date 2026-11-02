/* input.c - the mouse on the canvas: dragging, selecting, the camera */
#include "editor.h"

/* What the left mouse button is currently dragging */
struct DragBoxState drag;

/* The selection box, world space */
Rectangle
drag_box(void)
{
        Vector2 a = drag.start, b = mouse_world();
        return (Rectangle) { fminf(a.x, b.x), fminf(a.y, b.y), fabsf(a.x - b.x), fabsf(a.y - b.y) };
}

/* Select what touches B (world space). ADD keeps what's selected. */
static void
select_in_box(Rectangle b, bool add)
{
        Da_foreach(sp, squares)
        {
                bool in         = b.width > 0 && b.height > 0 && CheckCollisionRecs(square_rect(*sp), b);
                (*sp)->selected = in || (add && (*sp)->selected);
        }
}

void
update_mouse(void)
{
        Vector2 mouse = GetMousePosition();
        Vector2 world = GetScreenToWorld2D(mouse, camera);

        if (IsMouseButtonPressed(MOUSE_BUTTON_LEFT) && CheckCollisionPointRec(mouse, canvas_rect())) {
                Hit h = square_hit(world);
                int l;

                switch (h.part) {
                case PART_OUTPUT:
                        drag.kind   = DRAG_WIRE;
                        drag.square = h.square;
                        drag.pin    = h.pin;
                        drag.picked = false;
                        break;

                case PART_INPUT: {
                        /* Pick up the wire plugged here, keeping its source */
                        if ((l = link_to(h.square->id, h.pin)) < 0) break;
                        Link k = links.items[l];
                        Da_remove(&links, l);
                        if ((drag.square = square_get(k.from))) {
                                drag.kind   = DRAG_WIRE;
                                drag.pin    = k.from_pin;
                                drag.picked = true;
                        }
                        break;
                }

                case PART_FIELD: {
                        PinInfo *p = &h.square->inputs.items[h.pin];
                        if (p->kind == PIN_OPTIONS) {
                                /* Its list, below the field */
                                Rectangle f        = field_rect(h.square, h.pin);
                                options_input      = h.pin;
                                options_menu.count = 0;
                                for (int k = 0; k < p->option_count; k++)
                                        Da_append(&options_menu, (MenuItem) { .label = p->options[k], .fn = option_choose, .arg = (void *) (intptr_t) k });
                                menu_open(&options_menu, h.square->id,
                                          GetWorldToScreen2D((Vector2) { f.x, f.y + f.height }, camera));
                        } else if (p->type == LI_BOOL) {
                                pin_store(p, li_bool(!(p->value.type == LI_BOOL && p->value.b)));
                        } else {
                                field_edit_start(h.square, h.pin, false);
                        }
                        break;
                }

                case PART_PARAM: {
                        Param *p = &h.square->params.items[h.pin];
                        if (p->value.type == LI_BOOL)
                                param_set_text(h.square, h.pin, p->value.b ? "false" : "true");
                        else
                                field_edit_start(h.square, h.pin, true);
                        break;
                }

                case PART_BODY: {
                        bool shift = IsKeyDown(KEY_LEFT_SHIFT) || IsKeyDown(KEY_RIGHT_SHIFT);

                        /* Double click opens the settings */
                        static Square *last;
                        static double last_time;
                        double now = GetTime();
                        if (h.square->params.count && h.square == last && now - last_time < 0.3) {
                                panel_open(h.square);
                                last = NULL;
                                break;
                        }
                        last      = h.square;
                        last_time = now;

                        /* Shift adds or removes it. Without it, clicking a
                         * selected square keeps the selection, to move it. */
                        if (shift) {
                                h.square->selected = !h.square->selected;
                                if (!h.square->selected) break;
                        } else if (!h.square->selected) {
                                select_all(false);
                                h.square->selected = true;
                        }

                        /* Move to the top of the draw order */
                        Da_remove(&squares, square_index(h.square));
                        /* It made room: the append doesn't grow it */
                        assert(squares.count < squares.capacity);
                        Da_append(&squares, h.square);
                        drag.kind = DRAG_SQUARE;
                        drag.last = world;
                        break;
                }

                case PART_NONE:
                        drag.kind  = DRAG_BOX;
                        drag.start = world;
                        break;
                }
        }

        if (drag.kind == DRAG_SQUARE) {
                Vector2 d = Vector2Subtract(world, drag.last);
                Da_foreach(sp, squares)
                {
                        if ((*sp)->selected) (*sp)->pos = Vector2Add((*sp)->pos, d);
                }
                drag.last = world;
        }

        if (IsMouseButtonReleased(MOUSE_BUTTON_LEFT)) {
                if (drag.kind == DRAG_WIRE) {
                        /* A new wire dropped on the empty canvas opens the
                         * module search there, to add one wired to it. One
                         * picked up from an input and dropped elsewhere, or
                         * dropped on a square, is removed. */
                        Hit h = square_hit(world);
                        if (h.part == PART_INPUT) {
                                link_add(drag.square, drag.pin, h.square, h.pin);
                        } else if (!h.square && !drag.picked && CheckCollisionPointRec(mouse, canvas_rect())) {
                                canvas_menu_build();
                                menu_search_open(&canvas_menu, mouse);
                                text_input_set(menu_search(), "");
                                menu.wire_from = drag.square->id;
                                menu.wire_pin  = drag.pin;
                        }
                }
                if (drag.kind == DRAG_BOX) {
                        /* Everything the box touches. A click (no box) just
                         * clears the selection. Shift adds to it. */
                        select_in_box(drag_box(), IsKeyDown(KEY_LEFT_SHIFT) || IsKeyDown(KEY_RIGHT_SHIFT));
                }
                drag.kind = DRAG_NONE;
        }
}

void
update_camera(void)
{
        Vector2 mouse = GetMousePosition();

        if (IsMouseButtonDown(MOUSE_BUTTON_RIGHT)) {
                Vector2 delta = Vector2Scale(GetMouseDelta(), -1.0f / camera.zoom);
                camera.target = Vector2Add(camera.target, delta);
        }

        /* Over an open menu the wheel scrolls it instead */
        bool over_menu = false;
        for (int k = 0; k < menu.depth; k++)
                over_menu = over_menu || CheckCollisionPointRec(mouse, menu_rect(k));
        if (menu.depth && menu.searchable) over_menu = over_menu || CheckCollisionPointRec(mouse, menu_search_rect());

        float wheel = GetMouseWheelMove();
        if (wheel != 0 && !over_menu && CheckCollisionPointRec(mouse, canvas_rect())) {
                /* Zoom around the cursor: keep the world point under it fixed */
                camera.target = GetScreenToWorld2D(mouse, camera);
                camera.offset = mouse;
                float scale   = cfg.canvas.zoom_speed * wheel;
                camera.zoom   = Clamp(expf(logf(camera.zoom) + scale),
                                      cfg.canvas.zoom_min, cfg.canvas.zoom_max);
        }
}
