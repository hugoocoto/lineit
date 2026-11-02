/* version.c - versions of canvases, packages and lineit */
#include "editor.h"

/* "1.2.3" (or "1.2", "1") in V. False if it isn't one. */
static bool
version_parse(const char *s, Version *v)
{
        int *parts[] = { &v->major, &v->minor, &v->patch };
        *v           = (Version) { 0 };
        for (int i = 0; i < 3; i++) {
                if (!isdigit((unsigned char) *s)) return false;
                long n = 0;
                for (; isdigit((unsigned char) *s); s++) {
                        n = n * 10 + (*s - '0');
                        if (n > 1000000) return false;
                }
                *parts[i] = (int) n;
                if (!*s) return true;
                if (*s++ != '.') return false;
        }
        return false;
}

/* KEY of CONF as a version, a string: "0.0.1". False, with why in WHY, if
 * it has none. */
bool
conf_version(Conf conf, const char *key, Version *v, char *why, int size)
{
        const char *str;
        if (Conf_get_str(conf, &str, "%s", key) != CONF_OK) {
                snprintf(why, size, "it has no %s version (\"MAJOR.MINOR.PATCH\")", key);
                return false;
        }
        if (version_parse(str, v)) return true;
        snprintf(why, size, "its %s is \"%s\", not a version (MAJOR.MINOR.PATCH)", key, str);
        return false;
}

/* Whether something made for version MADE can be used by this lineit
 * (LI_VERSION). If not, why in WHY. WHAT is what it is: "canvas", "package"... */
bool
version_ok(Version made, const char *what, char *why, int size)
{
        const char *have = LI_VERSION;
        Version h;
        version_parse(have, &h);
        if (made.major != h.major) {
                snprintf(why, size, "%s made for lineit %d.%d.%d, this is %s: a different major, they don't work together",
                         what, made.major, made.minor, made.patch, have);
                return false;
        }
        if (made.minor > h.minor) {
                snprintf(why, size, "%s made for lineit %d.%d.%d, this is %s: update lineit", what, made.major, made.minor,
                         made.patch, have);
                return false;
        }
        return true;
}
ModulesDa modules; /* sorted by name */
