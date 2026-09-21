#include "raylib.h"


int
main(int argc, char **argv)
{
        InitWindow(800, 600, "Titulo");
        while (!WindowShouldClose()) {
                BeginDrawing();
                ClearBackground(RED);
                DrawText("Hello", 0, 0, 20, WHITE);
                EndDrawing();
        }
        return 0;
}
