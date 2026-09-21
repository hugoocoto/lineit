#include "conf.h"
#include "cum.h"
#include "flag.h"
#include "raylib.h"

#include <libgen.h>
#include <unistd.h>

#ifndef VERSION
#define VERSION "unknown"
#endif

typedef struct Object {
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
                        SQUARE,
                } type;
                union shape_data {
                        struct {
                                int width, height;
                        } square;
                } as;
                void (*draw)(struct Object *);                       // custom draw function - required
                int (*point_collide)(int x, int y, struct Object *); // true if x, y collide with the object
        } shape;
        Da(struct Object *) children;
} Object;

struct Globals {
        struct Window {
                int width;
                int heigh;
                char *title;
                Color background;
        } window;
        Da(Object *) object_list;
        struct mouse_state {
                Object *grabbed;
                Vector2 offset;
        } mouse_state;
        struct camera {
                int x;
                int y;
                int use_global_geometry; // where to use screen geometry or
                                         // window geometry
        } camera;
} g = {
        .window.width               = 300,
        .window.heigh               = 300,
        .window.title               = "Title",
        .window.background          = BLACK,
        .camera.use_global_geometry = false,
};

static inline int
to_world_x(int x)
{
        return x + (g.camera.use_global_geometry ? GetWindowPosition().x : 0) + g.camera.x;
}

static inline int
to_world_y(int y)
{
        return y + (g.camera.use_global_geometry ? GetWindowPosition().y : 0) + g.camera.y;
}

static inline int
to_window_x(int x)
{
        return x - (g.camera.use_global_geometry ? GetWindowPosition().x : 0) - g.camera.x;
}

static inline int
to_window_y(int y)
{
        return y - (g.camera.use_global_geometry ? GetWindowPosition().y : 0) - g.camera.y;
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
square_point_collide(int x, int y, Object *o)
{
        return CheckCollisionPointRec((Vector2) { x, y },
                                      (Rectangle) {
                                      .x      = o->position.x,
                                      .y      = o->position.y,
                                      .height = o->shape.as.square.height,
                                      .width  = o->shape.as.square.width,
                                      });
}

void
square_draw(Object *o)
{
        DrawRectangle(to_window_x(o->position.x),
                      to_window_y(o->position.y),
                      o->shape.as.square.width,
                      o->shape.as.square.height,
                      o->properties.color);
}

Object *
new_square(int l)
{
        Object *o                 = new_empty_object();
        o->shape.type             = SQUARE;
        o->shape.as.square.width  = l;
        o->shape.as.square.height = l;
        o->shape.draw             = square_draw;
        o->shape.point_collide    = square_point_collide;
        return o;
}

void
add_object(Object *o)
{
        Da_append(&g.object_list, o);
}

void
inner_render_objects(Da(struct Object *) * _o)
{
        static int cum_x = 0;
        static int cum_y = 0;
        if (_o) Da_foreach(o, *_o)
                {
                        if ((*o)->properties.skip_render) continue;
                        if ((*o)->shape.draw == NULL) {
                                printf("Error: object does not have draw method\n");
                                continue;
                        }
                        (*o)->shape.draw(*o);
                        if ((*o)->children.count > 0) {
                                cum_x += (*o)->position.x;
                                cum_y += (*o)->position.y;
                                inner_render_objects((void *) &(*o)->children);
                                cum_x -= (*o)->position.x;
                                cum_y -= (*o)->position.y;
                        }
                }
}

void
render_objects()
{
        inner_render_objects((void *) &g.object_list);
}

Object *
get_object_at_xy(int x, int y)
{
        Da_foreach(o, g.object_list)
        {
                if ((*o)->shape.point_collide == NULL) continue;
                if ((*o)->shape.point_collide(x, y, *o)) return *o;
        }
        return NULL;
}

void
process_events()
{
        int mouse_x = to_world_x(GetMouseX());
        int mouse_y = to_world_y(GetMouseY());

        if (g.mouse_state.grabbed == NULL && IsMouseButtonDown(MOUSE_LEFT_BUTTON)) {
                Object *o = get_object_at_xy(mouse_x, mouse_y);
                if (o != NULL) {
                        g.mouse_state.grabbed = o;
                        g.mouse_state.offset  = (Vector2) {
                                .x = o->position.x - mouse_x,
                                .y = o->position.y - mouse_y,
                        };
                }
        }

        if (g.mouse_state.grabbed != NULL && IsMouseButtonReleased(MOUSE_LEFT_BUTTON)) {
                g.mouse_state.grabbed = NULL;
        }

        if (g.mouse_state.grabbed) {
                g.mouse_state.grabbed->position.x = mouse_x + g.mouse_state.offset.x;
                g.mouse_state.grabbed->position.y = mouse_y + g.mouse_state.offset.y;
        }
}

void
loop()
{
        SetConfigFlags(FLAG_WINDOW_RESIZABLE | FLAG_VSYNC_HINT);
        InitWindow(g.window.width, g.window.heigh, g.window.title);

        Object *square = new_square(100);
        add_object(square);

        while (!WindowShouldClose()) {
                process_events();
                BeginDrawing();
                ClearBackground(g.window.background);
                DrawText("Hello", 0, 0, 20, WHITE);
                render_objects();
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
