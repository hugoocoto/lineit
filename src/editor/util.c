/* util.c - files, processes and hashes */
#include "editor.h"

/* Where the copies of the modules go, created on first use */
char tmp_dir[PATH_MAX];

bool
copy_file(const char *from, const char *to)
{
        char buf[1 << 16];
        size_t n;
        bool ok   = false;
        FILE *in  = fopen(from, "rb");
        FILE *out = in ? fopen(to, "wb") : NULL;

        if (in && out) {
                /* A short read is the end, or an error: ferror says which */
                do {
                        n  = fread(buf, 1, sizeof buf, in);
                        ok = fwrite(buf, 1, n, out) == n;
                } while (ok && n == sizeof buf);
                ok = ok && !ferror(in);
        }
        if (out && fclose(out) != 0) ok = false;
        if (in) fclose(in);
        return ok;
}

/* Open a private copy of M for square S. dlopen on the same file again only
 * bumps a reference count, so every square would share the module's globals.
 * A copy is a different file, so it gets its own. */
bool
tmp_dir_make(void)
{
        if (*tmp_dir) return true;
        const char *tmp = getenv("TMPDIR");
        snprintf(tmp_dir, sizeof tmp_dir, "%s/lineit-XXXXXX", tmp && *tmp ? tmp : "/tmp");
        if (!mkdtemp(tmp_dir)) {
                perror("lineit: mkdtemp");
                *tmp_dir = '\0';
                return false;
        }
        return true;
}

/* Modification time of PATH in ns, -1 if it isn't there */
long long
file_mtime(const char *path)
{
        struct stat st;
        if (stat(path, &st) != 0) return -1;
        return st.st_mtim.tv_sec * 1000000000LL + st.st_mtim.tv_nsec;
}

long long
now_ns(void)
{
        struct timespec t;
        clock_gettime(CLOCK_REALTIME, &t);
        return t.tv_sec * 1000000000LL + t.tv_nsec;
}

unsigned
le16(const unsigned char *p)
{
        return p[0] | p[1] << 8;
}

unsigned long
le32(const unsigned char *p)
{
        return p[0] | p[1] << 8 | (unsigned long) p[2] << 16 | (unsigned long) p[3] << 24;
}

/* Writes the LEN bytes of DATA to PATH if they aren't there already, so
 * make only builds again what changed */
bool
write_if_changed(const char *path, const void *data, size_t len)
{
        int size           = 0;
        unsigned char *old = FileExists(path) ? LoadFileData(path, &size) : NULL;
        bool same          = old && (size_t) size == len && !memcmp(old, data, len);
        if (old) UnloadFileData(old);
        if (same) return true;
        FILE *f = fopen(path, "wb");
        if (!f) return false;
        bool ok = fwrite(data, 1, len, f) == len;
        return fclose(f) == 0 && ok;
}

/* DIR and the folders above it, made if they aren't there */
bool
mkdir_p(const char *dir)
{
        char buf[PATH_MAX];
        snprintf(buf, sizeof buf, "%s", dir);
        for (char *p = buf + 1; *p; p++) {
                if (*p != '/') continue;
                *p = '\0';
                if (mkdir(buf, 0755) && errno != EEXIST) return false;
                *p = '/';
        }
        return !mkdir(buf, 0755) || errno == EEXIST;
}

/* For the shell: 'it', quoted */
void
shell_quote(char *buf, int size, const char *s)
{
        int n = snprintf(buf, size, "'");
        for (; *s && n < size - 5; s++)
                n += *s == '\'' ? snprintf(buf + n, size - n, "'\\''") : snprintf(buf + n, size - n, "%c", *s);
        snprintf(buf + n, size - n, "'");
}

/* Runs CMD with the shell, what it says to the output panel. Whether it
 * went well. */
bool
run_to_output(const char *cmd)
{
        char line[1024];
        FILE *p = popen(cmd, "r");
        if (!p) return false;
        while (fgets(line, sizeof line, p)) {
                line[strcspn(line, "\n")] = '\0';
                output_add(line, true);
        }
        return pclose(p) == 0;
}

/* What CMD prints to stdout, in BUF, one line. False if it fails. */
bool
run_to_buf(const char *cmd, char *buf, int size)
{
        FILE *p = popen(cmd, "r");
        int n   = 0, c;
        if (!p) return false;
        while ((c = fgetc(p)) != EOF)
                if (n < size - 1) buf[n++] = c == '\n' ? ' ' : c;
        buf[n] = '\0';
        return pclose(p) == 0;
}

/* The SHA-256 of the LEN bytes of DATA, as sha256sum prints it */
bool
sha256_hex(const void *data, size_t len, char hex[65])
{
        unsigned char md[EVP_MAX_MD_SIZE];
        unsigned n = 0;

        if (!EVP_Digest(data, len, md, &n, EVP_sha256(), NULL) || n != 32) return false;
        for (unsigned i = 0; i < n; i++)
                snprintf(hex + 2 * i, 3, "%02x", md[i]);
        return true;
}

/* Position of NEEDLE in HAY ignoring case, -1 if it's not there */
int
find_icase(const char *hay, const char *needle)
{
        size_t n = strlen(needle);
        for (int i = 0; hay[i]; i++) {
                if (strncasecmp(hay + i, needle, n) == 0) return i;
        }
        return n == 0 ? 0 : -1;
}

/* NAME as a file name: letters, digits, - _ and . */
void
file_name(char *buf, int size, const char *name)
{
        int n = 0;
        for (; *name && n < size - 1; name++)
                buf[n++] = isalnum((unsigned char) *name) || strchr("-_.", *name) ? *name : '_';
        buf[n] = '\0';
        if (!n || buf[0] == '.') snprintf(buf, size, "program");
}

int
str_cmp(const void *a, const void *b)
{
        return strcmp(*(char **) a, *(char **) b);
}

/* mkdir -p of the folder of PATH */
void
mkdir_parents(const char *path)
{
        char dir[PATH_MAX];
        snprintf(dir, sizeof dir, "%s", path);
        for (char *p = dir + 1; *p; p++) {
                if (*p != '/') continue;
                *p = '\0';
                mkdir(dir, 0755);
                *p = '/';
        }
}
