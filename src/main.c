/* main.c - main: the window, and every frame */
#include "editor/editor.h"
#include "flag.h"
bool autosave; /* cfg.save.autosave at start, toggled in the File menu (see autosave.c) */

static void
init_raylib(void)
{
        SetConfigFlags(FLAG_WINDOW_RESIZABLE | FLAG_MSAA_4X_HINT | FLAG_VSYNC_HINT);
        SetTraceLogLevel(LOG_WARNING);
        InitWindow(cfg.window.width, cfg.window.height, cfg.window.title);
        SetExitKey(KEY_NULL);
        SetTargetFPS(cfg.window.fps);
}

int
main(int argc, char **argv)
{
        const char *f_version;
        const char *f_config;
        const char *f_file;
        const char *f_restore;
        const char *f_tab;
        const char *f_compile;

        flag_program();
        flag_add(&f_version, "--version", .help = "show the version and exit");
        flag_add(&f_config, "--config", "-c", .nargs = 1,
                 .help = "lua config file (default: config.lua next to lineit in its source tree, or $XDG_CONFIG_HOME/lineit/config.lua)");
        flag_add(&f_file, "--file", "-f", .nargs = 1,
                 .help = "canvas file to load at start, and to save to (default: save.path in the config)");
        flag_add(&f_tab, "--tab", "-t", .nargs = 1,
                 .help = "open the canvas file in a new tab of the editor running (or start one with it)");
        flag_add(&f_compile, "--compile", .nargs = 1,
                 .help = "compile the canvas file to a program (see compile in the config) and quit, no window");
        flag_add(&f_restore, "--restore", .nargs = 1,
                 .help = "session to carry on with (the editor uses it to reload itself when rebuilt)");
        if (flag_parse(&argc, &argv)) {
                flag_show_help(STDERR_FILENO);
                exit(1);
        }

        if (f_version) {
                fprintf(stdout, "lineit, version %s\n", LI_VERSION);
                fprintf(stdout, "Copyright (C) 2026 Hugo Coto Flórez.\n");
                fprintf(stdout, "License GPLv3+: GNU GPL version 3 or later <http://gnu.org/licenses/gpl.html>\n");
                fprintf(stdout, "\n");
                fprintf(stdout, "This is free software; you are free to change and redistribute it.\n");
                fprintf(stdout, "There is NO WARRANTY, to the extent permitted by law.\n");
                exit(0);
        }

        if (f_compile) return compile_file(f_config, f_compile);

        if (f_tab) {
                if (tab_socket_send(f_tab)) return 0;
                f_file = f_tab; /* none running: this one */
        }

        config_path    = config_start(f_config);
        config_mtime   = file_mtime(config_path);
        settings_mtime = file_mtime(settings_path);
        autosave       = cfg.save.autosave;
        exe_watch();
        if (f_restore) session_window(f_restore);
        init_raylib();
        load_font();
        load_modules(cfg.modules.path);

        tab_new(f_file ? f_file : cfg.save.path);
        tab_socket_listen();
        if (f_restore) {
                if (session_restore(f_restore))
                        set_status("Reloaded lineit");
                else
                        set_status("Could not restore %s", f_restore);
        }

        while (!handed_over && !quitting) {
                keys_poll();
                if (WindowShouldClose() && !ask.open) quit_request();
                tab_socket_poll();
                if (!settings_shown()) update_camera();
                /* Menus are drawn over everything, so they go first */
                if (!update_ask() && !update_prompt() && !update_field() && !update_panel() && !update_menu() && !update_output() &&
                    !update_bar() && !update_tabs()) {
                        update_shortcuts();
                        if (settings_shown())
                                update_settings();
                        else
                                update_mouse();
                }
                run_frame();
                decls_check();
                history_check();
                autosave_tick();
                hot_reload();

                BeginDrawing();
                ClearBackground(cfg.colors.bg);
                if (settings_shown()) {
                        draw_settings();
                } else {
                        draw_canvas();
                        draw_tooltip();
                        draw_panel();
                }
                draw_output();
                draw_tabs();
                draw_bar();
                draw_menu();
                draw_prompt();
                draw_ask();
                EndDrawing();
                exe_ready_check();
        }

        tab_socket_close();
        closed_tabs_free();
        /* Their squares may have something to close */
        for (int i = tabs.count - 1; i >= 0; i--) {
                if (i != tab_cur) tab_close(i, true);
        }
        canvas_clear(-1, NULL);
        history_destroy();
        sched_free(sched);
        unload_modules();
        unload_fonts();
        output_clear();
        Da_destroy(&output_panel.lines);
        CloseWindow();
        Da_destroy(&squares);
        Da_destroy(&links);
        program_free(&prog);
        Da_destroy(&levels);
        Da_destroy(&lower_path);
        flag_free();
        return 0;
}
