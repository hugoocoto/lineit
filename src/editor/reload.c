/* reload.c - reloading the editor itself, and the session it hands over */
#include "editor.h"

/* When the lineit binary is rebuilt, the editor hands over to the new one:
 * it writes everything to a session file (the canvas with its camera and
 * module state, plus the window, the selection, the output, undo and redo,
 * for every tab), and the socket of --tab,
 * starts the new binary with --restore on it, and waits for it to say it's
 * drawing. Then this one quits, and the new one carries on where it left. If
 * the new one crashes or doesn't answer, this one stays, and tries again when
 * the binary changes again. Running canvases are started again: variables
 * and lists are not in the session. */

static char exe_path[PATH_MAX]; /* this binary, as it was at start */
static long long exe_mtime;
static int ready_fd = -1; /* in the new one: where to say it's ready */
#define HANDOVER_TIMEOUT_MS 10000
#define READY_FRAMES 3 /* drawn before saying it's ready */

void
exe_watch(void)
{
        ssize_t n               = readlink("/proc/self/exe", exe_path, sizeof exe_path - 1);
        exe_path[n > 0 ? n : 0] = '\0';
        exe_mtime               = file_mtime(exe_path);

        /* Started by the old one: tell it once drawing works */
        const char *fd = getenv("LINEIT_READY_FD");
        if (fd) {
                ready_fd = atoi(fd);
                unsetenv("LINEIT_READY_FD");
        }
}

/* Call once per frame drawn */
void
exe_ready_check(void)
{
        static int frames;
        if (ready_fd < 0 || ++frames < READY_FRAMES) return;
        if (write(ready_fd, "k", 1) != 1) perror("lineit: ready");
        close(ready_fd);
        ready_fd = -1;
}

static void
session_write_list(FILE *f, const char *name, void *list)
{
        Da(char *) *l = list;
        fprintf(f, "        %s = {\n", name);
        Da_foreach(t, *l)
        {
                fprintf(f, "                ");
                lua_quote(f, *t);
                fprintf(f, ",\n");
        }
        fprintf(f, "        },\n");
}

static bool
session_write(const char *path)
{
        FILE *f = fopen(path, "w");
        if (!f) return false;
        Vector2 win = GetWindowPosition();

        fprintf(f, "-- lineit session, see --restore\nsession = {\n");
        fprintf(f, "        active = %d,\n", tab_cur + 1);
        fprintf(f, "        autosave = %s,\n", autosave ? "true" : "false");
        fprintf(f, "        output_open = %s,\n", output_panel.open ? "true" : "false");
        fprintf(f, "        window = { x = %d, y = %d, width = %d, height = %d },\n",
                (int) win.x, (int) win.y, GetScreenWidth(), GetScreenHeight());
        fprintf(f, "        output = {\n");
        Da_foreach(l, output_panel.lines)
        {
                fprintf(f, "                { info = %s, text = ", l->info ? "true" : "false");
                lua_quote(f, l->text);
                fprintf(f, " },\n");
        }
        fprintf(f, "        },\n        tabs = {\n");
        for (int i = 0; i < tabs.count; i++) {
                tab_visit(i);
                char *canvas = canvas_text(WRITE_CAMERA | WRITE_STATE);
                fprintf(f, "        {\n");
                if (tabs.items[i]->settings) fprintf(f, "        settings = true,\n");
                if (canvas_path) {
                        fprintf(f, "        path = ");
                        lua_quote(f, canvas_path);
                        fprintf(f, ",\n");
                }
                if (tabs.items[i]->saved) {
                        fprintf(f, "        saved = ");
                        lua_quote(f, tabs.items[i]->saved);
                        fprintf(f, ",\n");
                }
                fprintf(f, "        running = %s,\n        canvas = ", running ? "true" : "false");
                lua_quote(f, canvas);
                fprintf(f, ",\n        selected = {");
                Da_foreach(sp, squares)
                {
                        if ((*sp)->selected) fprintf(f, " %d,", (*sp)->id);
                }
                fprintf(f, " },\n");
                session_write_list(f, "undo", &history.undo);
                session_write_list(f, "redo", &history.redo);
                fprintf(f, "        },\n");
                free(canvas);
                tab_visit(i);
        }
        fprintf(f, "        },\n}\n");
        return fclose(f) == 0;
}

/* Start the new binary on a session of this one. True once it's running:
 * then this one has to quit. */
static bool
exe_handover(void)
{
        char session[PATH_MAX];
        const char *tmp = getenv("TMPDIR");
        int fds[2];

        snprintf(session, sizeof session, "%s/lineit-session-XXXXXX", tmp && *tmp ? tmp : "/tmp");
        int sfd = mkstemp(session);
        if (sfd < 0) return false;
        close(sfd);
        if (!session_write(session) || pipe(fds) != 0) {
                remove(session);
                return false;
        }
        fcntl(fds[0], F_SETFD, FD_CLOEXEC);

        fflush(stdout), fflush(stderr);
        pid_t pid = fork();
        if (pid == 0) {
                char fd[16];
                snprintf(fd, sizeof fd, "%d", fds[1]);
                setenv("LINEIT_READY_FD", fd, 1);
                if (tab_socket >= 0) {
                        snprintf(fd, sizeof fd, "%d", tab_socket);
                        setenv("LINEIT_SOCKET_FD", fd, 1);
                }
                /* Its default config is this one's, unless it wasn't there */
                if (FileExists(config_path))
                        execl(exe_path, exe_path, "--config", config_path, "--restore", session, (char *) NULL);
                else
                        execl(exe_path, exe_path, "--restore", session, (char *) NULL);
                perror("lineit: exec");
                _exit(127);
        }
        close(fds[1]);
        if (pid < 0) {
                close(fds[0]);
                remove(session);
                return false;
        }

        /* This window doesn't draw meanwhile: it's the old picture anyway */
        struct pollfd p = { .fd = fds[0], .events = POLLIN };
        char c;
        bool ok = poll(&p, 1, HANDOVER_TIMEOUT_MS) == 1 && read(fds[0], &c, 1) == 1;
        close(fds[0]);
        if (!ok) {
                kill(pid, SIGKILL);
                waitpid(pid, NULL, 0);
                remove(session); /* the new one removes it when it reads it */
        }
        return ok;
}

/* The binary changed: hand over to it */
static void
exe_reload(void)
{
        long long mtime = file_mtime(exe_path);
        if (mtime < 0 || mtime == exe_mtime || now_ns() - mtime < MODULE_SETTLE_NS) return;
        exe_mtime = mtime; /* a broken build is tried again when it changes */
        if (exe_handover())
                handed_over = true;
        else
                set_status("The new lineit didn't start, keeping this one");
}

static void
session_read_list(Conf conf, const char *key, void *list)
{
        Da(char *) *l = list;
        const char *str;
        int n = 0;
        Conf_get_len(conf, &n, "%s", key);
        for (int i = 1; i <= n; i++) {
                if (Conf_get_str(conf, &str, "%s.%d", key, i) == CONF_OK) Da_append(l, strdup(str));
        }
}

/* Before the window opens: its size */
void
session_window(const char *path)
{
        Conf conf;
        if (Conf_open(&conf, path) != CONF_OK) return;
        Conf_get_int(conf, &cfg.window.width, "session.window.width");
        Conf_get_int(conf, &cfg.window.height, "session.window.height");
        Conf_close(conf);
}

/* Everything else, once the window and the modules are there, into the
 * tab there is: the first one */
bool
session_restore(const char *path)
{
        Conf conf;
        const char *str;
        int b = 0, n = 0, x = 0, y = 0, active = 1;

        if (Conf_open(&conf, path) != CONF_OK) return false;
        if (Conf_get_int(conf, &x, "session.window.x") == CONF_OK && Conf_get_int(conf, &y, "session.window.y") == CONF_OK)
                SetWindowPosition(x, y);

        Conf_get_len(conf, &n, "session.output");
        for (int i = 1; i <= n; i++) {
                int info = 0;
                Conf_get_bool(conf, &info, "session.output.%d.info", i);
                if (Conf_get_str(conf, &str, "session.output.%d.text", i) == CONF_OK) output_add(str, info);
        }
        if (Conf_get_bool(conf, &b, "session.autosave") == CONF_OK) autosave = b;

        int ntabs = 0;
        Conf_get_len(conf, &ntabs, "session.tabs");
        Conf_get_int(conf, &active, "session.active");
        for (int t = 1; t <= ntabs; t++) {
                char key[64];
                b = 0;
                Conf_get_bool(conf, &b, "session.tabs.%d.settings", t);
                if (b && t > 1) {
                        tab_add(NULL, true);
                        continue;
                }
                if (b) {
                        /* The first one, made as a canvas */
                        tabs.items[tab_cur]->settings = true;
                        canvas_clear(-1, NULL);
                        history_reset();
                        tab_mark_saved();
                        continue;
                }
                if (t > 1) tab_new(NULL);

                if (Conf_get_str(conf, &str, "session.tabs.%d.canvas", t) == CONF_OK) {
                        char *canvas = strdup(str);
                        canvas_read_text(canvas, READ_LOAD);
                        free(canvas);
                }
                free((char *) canvas_path);
                canvas_path = Conf_get_str(conf, &str, "session.tabs.%d.path", t) == CONF_OK ? strdup(str) : NULL;

                n = 0;
                Conf_get_len(conf, &n, "session.tabs.%d.selected", t);
                for (int i = 1; i <= n; i++) {
                        int id    = -1;
                        Square *s = Conf_get_int(conf, &id, "session.tabs.%d.selected.%d", t, i) == CONF_OK ? square_get(id) : NULL;
                        if (s) s->selected = true;
                }

                history_reset();
                snprintf(key, sizeof key, "session.tabs.%d.undo", t);
                session_read_list(conf, key, &history.undo);
                snprintf(key, sizeof key, "session.tabs.%d.redo", t);
                session_read_list(conf, key, &history.redo);
                Tab *tab = tabs.items[tab_cur];
                free(tab->saved);
                tab->saved = Conf_get_str(conf, &str, "session.tabs.%d.saved", t) == CONF_OK ? strdup(str) : NULL;

                b = 0;
                Conf_get_bool(conf, &b, "session.tabs.%d.running", t);
                if (b) bar_run_stop();
        }

        /* After the tabs: Run opens it */
        if (Conf_get_bool(conf, &b, "session.output_open") == CONF_OK) output_panel.open = b;
        output_panel.unseen = 0;

        Conf_close(conf);
        remove(path);
        tab_switch(active - 1);
        tab_title();
        return true;
}

void
hot_reload(void)
{
        static double last;
        if (GetTime() - last < 0.5) return;
        last = GetTime();

        long long mtime = file_mtime(config_path), smtime = file_mtime(settings_path);
        if (mtime != config_mtime || smtime != settings_mtime) {
                bool there     = mtime >= 0 || (smtime >= 0 && config_mtime < 0);
                config_mtime   = mtime;
                settings_mtime = smtime;
                if (there) config_reload();
        }
        modules_scan(true);
        exe_reload();
}
