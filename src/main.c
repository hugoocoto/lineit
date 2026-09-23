#include "conf.h"
#include "cum.h"
#include "flag.h"
#include "raylib.h"

#include <libgen.h>
#include <stdint.h>
#include <unistd.h>

#ifndef VERSION
#define VERSION "unknown"
#endif

#define DOUBLE_CLICK_MAX_DELTA 0.3

typedef struct Object Object;
typedef Da(Object *) Obj_List;

typedef enum {
        EV_PRESS,        // mouse button went down on this object
        EV_RELEASE,      // mouse button released (sent to the pressed object)
        EV_CLICK,        // press + release on the same object, without dragging
        EV_DOUBLE_CLICK, // second click on the same object soon after the first
                         // one (both EV_CLICKs are still sent before it)
        EV_DRAG,         // mouse moved while this object is pressed
        EV_HOVER_IN,     // mouse entered the object (does not bubble)
        EV_HOVER_OUT,    // mouse left the object (does not bubble)
} Event_Type;

typedef struct {
        Event_Type type;
        Vector2 mouse; // world coords
        Vector2 local; // mouse relative to the object that receives the event
        int button;    // MOUSE_BUTTON_* involved, -1 for hover events
} Event;

struct Object {
        struct position {
                int x;
                int y;
        } position;
        struct properties {
                Color color;
                int skip_render;
        } properties;
        struct state {
                int grabbed;
        } state;
        struct shape {
                enum shape_type {
                        INVALID = 0,
                        RECTANGLE,
                        TEXT,
                } type;
                union shape_data {
                        struct {
                                int width, height;
                        } rectangle;
                        struct {
                                const char *str;
                                int size;
                                int spacing;
                        } text;
                } as;
                void (*draw)(struct Object *);                       // custom draw function - required
                int (*point_collide)(int x, int y, struct Object *); // true if (x, y) collide with the object
        } shape;
        // handle an event. Return true if handled, false to let it bubble
        // up to the parent. NULL behaves as always returning false.
        int (*on_event)(struct Object *self, Event *e);
        void *data; // per-object user state
        Object *parent;
        Obj_List children;
};

struct Globals {
        struct Window {
                int width;
                int heigh;
                char *title;
                Color background;
        } window;
        Obj_List object_list;
        struct mouse_state {
                Object *hovered;        // topmost object under the mouse
                Object *pressed;        // object under the mouse when the button went down
                int pressed_button;     // MOUSE_BUTTON_* that pressed it
                int moved;              // mouse moved since press (cancels EV_CLICK)
                Vector2 last;           // mouse world position on the previous frame
                Vector2 offset;         // drag offset, used by draggable_on_event
                Object *last_click;     // candidate for EV_DOUBLE_CLICK
                double last_click_time; // GetTime() of last_click
        } mouse_state;
        struct camera {
                float x;
                float y;
                float zoom;
                int use_global_geometry; // where to use screen geometry or
                                         // window geometry
        } camera;
} g = {
        .window.width               = 300,
        .window.heigh               = 300,
        .window.title               = "Title",
        .window.background          = BLACK,
        .camera.zoom                = 1.0f,
        .camera.use_global_geometry = false,
};

static inline Camera2D
get_camera()
{
        Vector2 win = g.camera.use_global_geometry ? GetWindowPosition() : (Vector2) { 0 };
        return (Camera2D) {
                .offset   = (Vector2) { -win.x, -win.y },
                .target   = (Vector2) { g.camera.x, g.camera.y },
                .rotation = 0,
                .zoom     = g.camera.zoom,
        };
}

static inline Vector2
get_mouse_world()
{
        return GetScreenToWorld2D(GetMousePosition(), get_camera());
}

static inline void
attach(Object *self, Object *parent)
{
        Da_append(&parent->children, self);
        self->parent = parent;
}

// position of the object in world coords (positions are parent-relative).
Vector2
object_world_position(Object *o)
{
        Vector2 p = { 0 };
        for (; o; o = o->parent) {
                p.x += o->position.x;
                p.y += o->position.y;
        }
        return p;
}

Object *
new_empty_object()
{
        Object *o = Memdup((Object) { 0 });
        if (o == NULL) abort();
        o->properties.color = RED;
        return o;
}

int
rectangle_point_collide(int x, int y, Object *o)
{
        return CheckCollisionPointRec((Vector2) { x, y },
                                      (Rectangle) {
                                      .x      = o->position.x,
                                      .y      = o->position.y,
                                      .height = o->shape.as.rectangle.height,
                                      .width  = o->shape.as.rectangle.width,
                                      });
}

void
rectangle_draw(Object *o)
{
        DrawRectangle(o->position.x,
                      o->position.y,
                      o->shape.as.rectangle.width,
                      o->shape.as.rectangle.height,
                      o->properties.color);
}

Object *
new_rectangle(int w, int h)
{
        Object *o                    = new_empty_object();
        o->shape.type                = RECTANGLE;
        o->shape.as.rectangle.width  = w;
        o->shape.as.rectangle.height = h;
        o->shape.draw                = rectangle_draw;
        o->shape.point_collide       = rectangle_point_collide;
        return o;
}

void
text_draw(Object *o)
{
        DrawTextEx(GetFontDefault(), o->shape.as.text.str, (Vector2) { .x = o->position.x, .y = o->position.y }, o->shape.as.text.size, o->shape.as.text.spacing, o->properties.color);
}

Object *
new_text(const char *text)
{
        Object *o                = new_empty_object();
        o->shape.type            = TEXT;
        o->shape.as.text.str     = Strdup(text);
        o->shape.as.text.size    = 20;
        o->shape.as.text.spacing = 0;
        o->properties.color      = WHITE;
        o->shape.draw            = text_draw;
        o->shape.point_collide   = NULL; // do not add collider
        return o;
}


void
add_object(Object *o)
{
        Da_append(&g.object_list, o);
}

// move o to the end of the list it lives in (its parent's children, or the
// global object list), so it is rendered last and stays on top of its
// siblings.
void
raise_object(Object *o)
{
        Obj_List *list = o->parent ? &o->parent->children : &g.object_list;
        Da_foreach(it, *list)
        {
                if (*it != o) continue;
                Da_remove(list, Da_index(it, list));
                Da_append(list, o);
                return;
        }
}

void
inner_render_objects(Obj_List _o)
{
        static int cum_x = 0;
        static int cum_y = 0;
        Da_foreach(o, _o)
        {
                if ((*o)->properties.skip_render) continue;
                if ((*o)->shape.draw == NULL) {
                        printf("Error: object does not have draw method\n");
                        continue;
                }

                // draw self
                (*o)->position.x += cum_x;
                (*o)->position.y += cum_y;
                (*o)->shape.draw(*o);
                (*o)->position.x -= cum_x;
                (*o)->position.y -= cum_y;

                // draw children
                if ((*o)->children.count > 0) {
                        cum_x += (*o)->position.x;
                        cum_y += (*o)->position.y;
                        inner_render_objects((*o)->children);
                        cum_x -= (*o)->position.x;
                        cum_y -= (*o)->position.y;
                }
        }
}

void
render_objects()
{
        inner_render_objects(g.object_list);
}

Object *
get_object_at_xy_inner(Obj_List obj, int x, int y)
{
        // reverse of render order: objects drawn last are on top.
        Da_foreach_reverse(o, obj)
        {
                Object *c = *o;
                if (c->children.count > 0) {
                        // children positions are relative to their parent, so
                        // move the point into the parent's local space.
                        c = get_object_at_xy_inner(c->children,
                                                   x - c->position.x,
                                                   y - c->position.y);
                        // if get_object_at_xy_inner returns non null, the
                        // object that is returned already has point collider
                        // and the collider returns true.
                        if (c) return c;
                        c = *o; // came back if none of their childs collide.
                }
                if (c->shape.point_collide && c->shape.point_collide(x, y, c)) return c;
        }
        return NULL;
}

Object *
get_object_at_xy(int x, int y)
{
        return get_object_at_xy_inner(g.object_list, x, y);
}

// multiply zoom by factor keeping the world point under screen_point fixed.
void
zoom_at(Vector2 screen_point, float factor)
{
        Vector2 before = GetScreenToWorld2D(screen_point, get_camera());
        g.camera.zoom *= factor;
        if (g.camera.zoom < 0.1f) g.camera.zoom = 0.1f;
        if (g.camera.zoom > 10.0f) g.camera.zoom = 10.0f;
        Vector2 after = GetScreenToWorld2D(screen_point, get_camera());
        g.camera.x += before.x - after.x;
        g.camera.y += before.y - after.y;
}

void
process_zoom()
{
        const float step = 1.1f;

        // + and - zoom toward the window center. Read typed characters
        // instead of key codes so it works on any keyboard layout (and
        // with key repeat when held down).
        Vector2 center = { GetScreenWidth() / 2.0f, GetScreenHeight() / 2.0f };
        for (int c = GetCharPressed(); c != 0; c = GetCharPressed()) {
                if (c == '+') zoom_at(center, step);
                if (c == '-') zoom_at(center, 1 / step);
        }
}

// send the event to o only. Returns true if it was handled.
int
send_event(Object *o, Event *e)
{
        if (o == NULL || o->on_event == NULL) return 0;
        Vector2 p = object_world_position(o);
        e->local  = (Vector2) { e->mouse.x - p.x, e->mouse.y - p.y };
        return o->on_event(o, e);
}

// send the event to o, and bubble it up to the parents until one handles it.
// Returns the object that handled it, or NULL.
Object *
dispatch_event(Object *o, Event *e)
{
        for (; o; o = o->parent)
                if (send_event(o, e)) return o;
        return NULL;
}

void
process_mouse()
{
        struct mouse_state *ms = &g.mouse_state;
        Vector2 mouse          = get_mouse_world();
        Object *hit            = get_object_at_xy(mouse.x, mouse.y);
        Event e                = { .mouse = mouse, .button = -1 };

        if (hit != ms->hovered) {
                e.type = EV_HOVER_OUT;
                send_event(ms->hovered, &e);
                e.type = EV_HOVER_IN;
                send_event(hit, &e);
                ms->hovered = hit;
        }

        // only one button can be pressed at a time: other buttons are
        // ignored until the pressed one is released.
        if (ms->pressed == NULL && hit) {
                for (int b = MOUSE_BUTTON_LEFT; b <= MOUSE_BUTTON_MIDDLE; b++) {
                        if (!IsMouseButtonPressed(b)) continue;
                        ms->pressed        = hit;
                        ms->pressed_button = b;
                        ms->moved          = 0;
                        e.type             = EV_PRESS;
                        e.button           = b;
                        dispatch_event(hit, &e);
                        break;
                }
        }

        if (ms->pressed == NULL) {
                ms->last = mouse;
                return;
        }

        e.button = ms->pressed_button;

        if (mouse.x != ms->last.x || mouse.y != ms->last.y) {
                ms->moved = 1;
                e.type    = EV_DRAG;
                dispatch_event(ms->pressed, &e);
        }

        if (IsMouseButtonReleased(ms->pressed_button)) {
                e.type = EV_RELEASE;
                dispatch_event(ms->pressed, &e);
                if (!ms->moved && hit == ms->pressed) {
                        e.type = EV_CLICK;
                        dispatch_event(ms->pressed, &e);

                        double now = GetTime();
                        if (e.button != MOUSE_BUTTON_LEFT) {
                                ms->last_click = NULL;
                        } else if (ms->last_click == hit && now - ms->last_click_time <= DOUBLE_CLICK_MAX_DELTA) {
                                e.type = EV_DOUBLE_CLICK;
                                dispatch_event(ms->pressed, &e);
                                ms->last_click = NULL;
                        } else {
                                ms->last_click      = hit;
                                ms->last_click_time = now;
                        }
                }
                ms->pressed = NULL;
        }

        ms->last = mouse;
}

void
process_events()
{
        process_zoom();
        process_mouse();
}

int
on_event_default_dragable(Object *self, Event *e)
{
        if (e->button != MOUSE_BUTTON_LEFT) return 0;
        switch (e->type) {
        case EV_PRESS:
                g.mouse_state.offset = (Vector2) {
                        .x = self->position.x - e->mouse.x,
                        .y = self->position.y - e->mouse.y,
                };
                raise_object(self);
                return 1;
        case EV_DRAG:
                self->position.x = e->mouse.x + g.mouse_state.offset.x;
                self->position.y = e->mouse.y + g.mouse_state.offset.y;
                return 1;
        default:
                return 0;
        }
}

int
on_event_menu_button(Object *self, Event *e)
{
        switch (e->type) {
        case EV_HOVER_IN:
                self->properties.color = SKYBLUE;
                return 1;
        case EV_HOVER_OUT:
                self->properties.color = BLUE;
                return 1;
        case EV_CLICK:
                if (e->button == MOUSE_BUTTON_LEFT) {
                        int l                       = 100;
                        Object *rectangle           = new_rectangle(l, l);
                        rectangle->properties.color = RED;
                        rectangle->on_event         = on_event_default_dragable;
                        rectangle->position.x       = e->mouse.x - l / 2.0;
                        rectangle->position.y       = e->mouse.y - l / 2.0;
                        add_object(rectangle);
                        return 1;
                }
                return 0;
        case EV_DOUBLE_CLICK:
                return 1;
        default:
                return 0;
        }
}

void
init_menu_bar()
{
        int l = 40; // square side size
        int m = 5;  // margin
        int n = 7;  // number of squares

        Object *rectangle           = new_rectangle((l + m) * (n + 1) + m, 40 + 2 * m);
        rectangle->properties.color = WHITE;
        rectangle->on_event         = on_event_default_dragable;

        for (int i = 0; i < n; i++) {
                Object *square           = new_rectangle(l, l);
                square->properties.color = BLUE;
                square->on_event         = on_event_menu_button;
                square->data             = (void *) (intptr_t) i;
                square->position.x       = (l + m) * i + m;
                square->position.y       = m;
                Object *text             = new_text("?");
                text->position.x         = (l - text->shape.as.text.size) / 2;
                text->position.y         = (l - text->shape.as.text.size) / 2;
                attach(text, square);
                attach(square, rectangle);
        }

        add_object(rectangle);
}

void
loop()
{
        SetConfigFlags(FLAG_WINDOW_RESIZABLE | FLAG_VSYNC_HINT);
        InitWindow(g.window.width, g.window.heigh, g.window.title);

        init_menu_bar();

        while (!WindowShouldClose()) {
                process_events();
                BeginDrawing();
                ClearBackground(g.window.background);
                BeginMode2D(get_camera());
                render_objects();
                EndMode2D();
                EndDrawing();
        }
}

void
objects_free()
{
        Da_foreach(o, g.object_list) if (*o) free(*o);
        Da_destroy(&g.object_list);
}

int
main(int argc, char **argv)
{
        const char *f_version;

        flag_program();
        flag_add(&f_version, "--version");
        if (flag_parse(&argc, &argv)) {
                flag_show_help(STDERR_FILENO);
                exit(1);
        }

        if (f_version) {
                fprintf(stdout, "%s, version %s\n", basename(argv[0]), VERSION);
                fprintf(stdout, "Copyright (C) 2026 Hugo Coto Flórez.\n");
                fprintf(stdout, "License GPLv3+: GNU GPL version 3 or later <http://gnu.org/licenses/gpl.html>\n");
                fprintf(stdout, "\n");
                fprintf(stdout, "This is free software; you are free to change and redistribute it.\n");
                fprintf(stdout, "There is NO WARRANTY, to the extent permitted by law.\n");
                exit(0);
        }

        loop();

        flag_free();
        objects_free();
        return 0;
}
