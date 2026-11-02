/* single.c - one editor: ./lineit --tab FILE opens FILE in the one running */
#include "editor.h"

int tab_socket = -1; /* listening, if this is the one */
static bool tab_socket_owned;
static char tab_socket_path[108]; /* sun_path */

static void
tab_socket_name(void)
{
        const char *dir = getenv("XDG_RUNTIME_DIR");
        if (dir && *dir)
                snprintf(tab_socket_path, sizeof tab_socket_path, "%s/lineit.sock", dir);
        else
                snprintf(tab_socket_path, sizeof tab_socket_path, "/tmp/lineit-%d.sock", (int) getuid());
}

static int
tab_socket_connect(void)
{
        struct sockaddr_un a = { .sun_family = AF_UNIX };
        int fd               = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
        snprintf(a.sun_path, sizeof a.sun_path, "%s", tab_socket_path);
        if (fd >= 0 && connect(fd, (struct sockaddr *) &a, sizeof a) == 0) return fd;
        if (fd >= 0) close(fd);
        return -1;
}

/* Give PATH to the editor running, if there is one. False if there isn't. */
bool
tab_socket_send(const char *path)
{
        char abs[2 * PATH_MAX], cwd[PATH_MAX];
        tab_socket_name();
        int fd = tab_socket_connect();
        if (fd < 0) return false;
        /* Absolute: it runs somewhere else. It may not exist yet. */
        if (!realpath(path, abs)) {
                if (*path == '/' || !getcwd(cwd, sizeof cwd))
                        snprintf(abs, sizeof abs, "%s", path);
                else
                        snprintf(abs, sizeof abs, "%s/%s", cwd, path);
        }
        size_t len = strlen(abs);
        bool ok    = write(fd, abs, len) == (ssize_t) len && write(fd, "\n", 1) == 1;
        close(fd);
        return ok;
}

/* Listen, unless another editor does. After a reload the old one hands
 * its socket over (LINEIT_SOCKET_FD). */
void
tab_socket_listen(void)
{
        const char *fd = getenv("LINEIT_SOCKET_FD");
        tab_socket_name();
        if (fd) {
                tab_socket       = atoi(fd);
                tab_socket_owned = true;
                unsetenv("LINEIT_SOCKET_FD");
                return;
        }

        int other = tab_socket_connect();
        if (other >= 0) {
                close(other);
                return;
        }
        struct sockaddr_un a = { .sun_family = AF_UNIX };
        snprintf(a.sun_path, sizeof a.sun_path, "%s", tab_socket_path);
        unlink(tab_socket_path); /* left by one that crashed */
        tab_socket = socket(AF_UNIX, SOCK_STREAM | SOCK_NONBLOCK, 0);
        if (tab_socket < 0 || bind(tab_socket, (struct sockaddr *) &a, sizeof a) != 0 || listen(tab_socket, 8) != 0) {
                if (tab_socket >= 0) close(tab_socket);
                tab_socket = -1;
                return;
        }
        tab_socket_owned = true;
}

/* Paths sent by ./lineit --tab, one per line */
void
tab_socket_poll(void)
{
        int c;
        if (tab_socket < 0) return;
        while ((c = accept(tab_socket, NULL, NULL)) >= 0) {
                char buf[PATH_MAX + 1];
                size_t len        = 0;
                ssize_t n         = 0;
                struct timeval tv = { .tv_usec = 200000 };
                setsockopt(c, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof tv);
                while (len < sizeof buf - 1 && (n = read(c, buf + len, sizeof buf - 1 - len)) > 0)
                        len += n;
                close(c);
                buf[len] = '\0';
                for (char *save, *line = strtok_r(buf, "\n", &save); line; line = strtok_r(NULL, "\n", &save))
                        tab_open(line);
                if (len) SetWindowFocused();
        }
}

void
tab_socket_close(void)
{
        if (tab_socket < 0) return;
        close(tab_socket);
        /* After a reload the new one has it */
        if (tab_socket_owned && !handed_over) unlink(tab_socket_path);
        tab_socket = -1;
}

bool
prompt_is_open(void)
{
        return prompt.open;
}
