/* packages.c - packages: modules in a .li */
#include "editor.h"

/* A .li is a zip with a module built for one or more platforms, its source
 * and its license (tools/lineit-pack makes them, see its --howto):
 *
 *   manifest.lua   name, version, license, api (LI_VERSION it was built for) and
 *                  platforms: platform -> library inside, like
 *                  ["linux-x86_64"] = "lib/linux-x86_64.so"
 *   lib/...        the libraries
 *   src/           its C
 *   Makefile       builds it from src/, in the package: lineit-so (the
 *                  library), lineit-o (one object, for Compile, see
 *                  package_object) and lineit-libs (prints what that
 *                  object needs to link: -lcurl...)
 *   LICENSE        for people, not read here
 *
 * manifest.lua also has the SHA-256 of every other file in it (sha256,
 * written by lineit-pack). Nothing in a package is used until it matches:
 * a file that doesn't, or that isn't in the list, and the package isn't
 * loaded, built or compiled. It finds a broken or changed file, not who
 * made it: whoever can change the files can change manifest.lua too.
 *
 * The library for this platform is taken out to tmp_dir once, and squares
 * copy it from there like any other .so. A package without this platform
 * is built from its src/ with its Makefile, once (see package_build). */

#if defined(__linux__)
#define LI_OS "linux"
#elif defined(__APPLE__)
#define LI_OS "macos"
#elif defined(_WIN32)
#define LI_OS "windows"
#else
#define LI_OS "unknown"
#endif
#if defined(__x86_64__) || defined(_M_X64)
#define LI_ARCH "x86_64"
#elif defined(__aarch64__) || defined(_M_ARM64)
#define LI_ARCH "arm64"
#else
#define LI_ARCH "unknown"
#endif
#define LI_PLATFORM LI_OS "-" LI_ARCH

/* Where the central directory of the zip ZIP (SIZE bytes) starts, in *AT,
 * and how many members it has. False if it isn't a zip. */
static bool
zip_directory(const unsigned char *zip, size_t size, size_t *at, unsigned *count)
{
        /* The end record is last, maybe after a comment of up to 64K */
        for (size_t i = size >= 22 ? size - 22 : 0; size >= 22; i--) {
                if (le32(zip + i) == 0x06054b50) {
                        *at    = le32(zip + i + 16);
                        *count = le16(zip + i + 10);
                        return true;
                }
                if (i == 0 || size - i > 22 + 0xffff) break;
        }
        return false;
}

typedef Da(char *) NameList;

/* The names of the members of ZIP, added to NAMES (free them) */
static void
zip_names(const unsigned char *zip, size_t size, NameList *names)
{
        size_t at;
        unsigned n;
        if (!zip_directory(zip, size, &at, &n)) return;
        for (unsigned e = 0; e < n; e++) {
                if (at + 46 > size || le32(zip + at) != 0x02014b50) return;
                unsigned nlen = le16(zip + at + 28);
                if (at + 46 + nlen > size) return;
                Da_append(names, strndup((const char *) zip + at + 46, nlen));
                at += 46 + nlen + le16(zip + at + 30) + le16(zip + at + 32);
        }
}

/* Member NAME of the zip ZIP (SIZE bytes), in a new buffer of *LEN bytes.
 * NULL if it isn't there or is broken. Stored and deflated members, no
 * zip64: modules are small. */
static unsigned char *
zip_member(const unsigned char *zip, size_t size, const char *name, size_t *len)
{
        size_t at;
        unsigned n;
        if (!zip_directory(zip, size, &at, &n)) return NULL;
        for (unsigned e = 0; e < n; e++) {
                if (at + 46 > size || le32(zip + at) != 0x02014b50) return NULL;
                unsigned method   = le16(zip + at + 10);
                unsigned long crc = le32(zip + at + 16);
                size_t csize      = le32(zip + at + 20);
                size_t usize      = le32(zip + at + 24);
                unsigned nlen     = le16(zip + at + 28);
                size_t local      = le32(zip + at + 42);
                bool match        = at + 46 + nlen <= size && nlen == strlen(name) && !memcmp(zip + at + 46, name, nlen);
                at += 46 + nlen + le16(zip + at + 30) + le16(zip + at + 32);
                if (!match) continue;

                if (local + 30 > size || le32(zip + local) != 0x04034b50) return NULL;
                size_t data = local + 30 + le16(zip + local + 26) + le16(zip + local + 28);
                if (data + csize > size) return NULL;

                unsigned char *out = malloc(usize ? usize : 1);
                bool ok            = false;
                if (method == 0 && csize == usize) {
                        memcpy(out, zip + data, usize);
                        ok = true;
                } else if (method == 8) {
                        z_stream z = { .next_in = (unsigned char *) zip + data, .avail_in = csize, .next_out = out, .avail_out = usize };
                        ok         = inflateInit2(&z, -MAX_WBITS) == Z_OK && inflate(&z, Z_FINISH) == Z_STREAM_END && z.total_out == usize;
                        inflateEnd(&z);
                }
                if (!ok || crc32(0, out, usize) != crc) {
                        free(out);
                        return NULL;
                }
                *len = usize;
                return out;
        }
        return NULL;
}

/* Whether NAME, a path in a zip, stays in the folder it's taken out to */
static bool
zip_name_safe(const char *name)
{
        if (*name == '/' || strchr(name, '\\')) return false;
        for (const char *p = name; (p = strstr(p, "..")); p += 2)
                if ((p == name || p[-1] == '/') && (p[2] == '/' || !p[2])) return false;
        return true;
}

typedef struct Sum {
        char *file;
        char sum[65];
} Sum;

typedef Da(Sum) SumList;

static void
sums_free(SumList *sums)
{
        Da_foreach(s, *sums)
        {
                free(s->file);
        }
        Da_destroy(sums);
}

static bool package_manifest(const unsigned char *zip, size_t size, Conf *conf, char *why, int why_size);
static bool package_sum_ok(SumList *sums, const char *name, const void *data, size_t len, char *why, int why_size);

/* The sha256 list of the manifest of the package ZIP, in SUMS (free them
 * with sums_free). False with the reason in WHY if there's none, if it
 * doesn't name every file of the package but manifest.lua, and only them,
 * or if a file doesn't match it. */
static bool
package_sums(const unsigned char *zip, size_t size, SumList *sums, char *why, int why_size)
{
        NameList names = { 0 };
        Conf conf;
        int n   = 0;
        bool ok = false;

        if (!package_manifest(zip, size, &conf, why, why_size)) return false;
        if (Conf_get_len(conf, &n, "sha256") != CONF_OK || n <= 0) {
                snprintf(why, why_size, "no sha256 in its manifest.lua, make it again with tools/lineit-pack");
                Conf_close(conf);
                return false;
        }
        for (int i = 1; i <= n; i++) {
                const char *file, *sum;
                if (Conf_get_str(conf, &file, "sha256.%d.file", i) != CONF_OK ||
                    Conf_get_str(conf, &sum, "sha256.%d.sum", i) != CONF_OK || strlen(sum) != 64 ||
                    strspn(sum, "0123456789abcdef") != 64) {
                        snprintf(why, why_size, "entry %d of sha256 in its manifest.lua is broken", i);
                        Conf_close(conf);
                        goto done;
                }
                Sum s = { .file = strdup(file) };
                memcpy(s.sum, sum, sizeof s.sum);
                Da_append(sums, s);
        }
        Conf_close(conf);

        int files = 0;
        zip_names(zip, size, &names);
        Da_foreach(name, names)
        {
                size_t len = strlen(*name);
                if (!len || (*name)[len - 1] == '/' || !strcmp(*name, "manifest.lua")) continue;
                bool listed = false;
                Da_foreach(s, *sums)
                {
                        listed = listed || !strcmp(s->file, *name);
                }
                if (!listed) {
                        snprintf(why, why_size, "%s is not in the sha256 of its manifest.lua", *name);
                        goto done;
                }
                size_t mlen;
                unsigned char *data = zip_member(zip, size, *name, &mlen);
                bool match          = data && package_sum_ok(sums, *name, data, mlen, why, why_size);
                free(data);
                if (!match) {
                        if (!data) snprintf(why, why_size, "%s is broken", *name);
                        goto done;
                }
                files++;
        }
        if (files != sums->count) {
                snprintf(why, why_size, "the sha256 of its manifest.lua names files it doesn't have");
                goto done;
        }
        ok = true;
done:
        Da_foreach(name, names)
        {
                free(*name);
        }
        Da_destroy(&names);
        if (!ok) sums_free(sums);
        return ok;
}

/* Whether the LEN bytes of DATA, the file NAME of a package, are what its
 * SUMS say. False with the reason in WHY if not. */
static bool
package_sum_ok(SumList *sums, const char *name, const void *data, size_t len, char *why, int why_size)
{
        char hex[65];
        Da_foreach(s, *sums)
        {
                if (strcmp(s->file, name)) continue;
                if (sha256_hex(data, len, hex) && !strcmp(hex, s->sum)) return true;
                snprintf(why, why_size, "%s doesn't match its SHA-256 in manifest.lua: broken or changed", name);
                return false;
        }
        snprintf(why, why_size, "%s is not in the sha256 of its manifest.lua", name);
        return false;
}

/* Takes the package ZIP out to DIR, but lib/ (built libraries). False with
 * the reason in WHY if it can't, or if it has no Makefile to build it. */
bool
package_extract(const unsigned char *zip, size_t zsize, const char *dir, char *why, int why_size)
{
        NameList names = { 0 };
        SumList sums   = { 0 };
        char path[PATH_MAX * 2];
        bool ok = false, makefile = false;

        if (!package_sums(zip, zsize, &sums, why, why_size)) return false;
        zip_names(zip, zsize, &names);
        Da_foreach(name, names)
        {
                const char *n = *name;
                size_t len    = strlen(n);
                if (!strncmp(n, "lib/", 4) || n[len - 1] == '/') continue;
                if (!zip_name_safe(n)) {
                        snprintf(why, why_size, "%s goes out of its folder", n);
                        goto done;
                }
                size_t mlen;
                unsigned char *data = zip_member(zip, zsize, n, &mlen);
                if (!data) {
                        snprintf(why, why_size, "%s is broken", n);
                        goto done;
                }
                snprintf(path, sizeof path, "%s/%s", dir, n);
                char *slash = strrchr(path, '/');
                *slash      = '\0';
                bool saved  = mkdir_p(path);
                *slash      = '/';
                saved       = saved && write_if_changed(path, data, mlen);
                free(data);
                if (!saved) {
                        snprintf(why, why_size, "can't write %s", path);
                        goto done;
                }
                makefile = makefile || !strcmp(n, "Makefile");
        }
        ok = makefile;
        if (!ok) snprintf(why, why_size, "it has no Makefile, make it again with tools/lineit-pack");
done:
        Da_foreach(name, names)
        {
                free(*name);
        }
        Da_destroy(&names);
        sums_free(&sums);
        return ok;
}

/* Runs rule RULE of the Makefile of the package taken out to DIR, as
 * tools/lineit-pack --howto says, with OUT, what it says to the output */
bool
package_make(const char *dir, const char *rule, const char *out)
{
        char qdir[PATH_MAX * 4 + 8], qout[PATH_MAX * 4 + 8], qinc[PATH_MAX * 4 + 8], qcc[1024], qflags[1024];
        char rel[PATH_MAX], include[PATH_MAX], cmd[PATH_MAX * 16];

        /* lineit.h, with the runtime of Compile: `make` puts it there */
        snprintf(rel, sizeof rel, "%s/include", cfg.compile.runtime);
        if (!realpath(rel, include)) snprintf(include, sizeof include, "%s", rel);
        shell_quote(qdir, sizeof qdir, dir);
        shell_quote(qout, sizeof qout, out);
        shell_quote(qinc, sizeof qinc, include);
        shell_quote(qcc, sizeof qcc, cfg.compile.cc);
        shell_quote(qflags, sizeof qflags, cfg.compile.cflags);
        remove(out);
        /* In the package: its Makefile can use paths relative to it. The cd
         * is the shell's, the editor stays where it is. */
        snprintf(cmd, sizeof cmd, "cd %s && make -s %s OUT=%s LINEIT_INCLUDE=%s CC=%s CFLAGS=%s 2>&1", qdir, rule, qout, qinc,
                 qcc, qflags);
        return run_to_output(cmd) && FileExists(out);
}

/* The manifest.lua of the package ZIP, open in CONF (close it). False with
 * the reason in WHY if it has none. */
static bool
package_manifest(const unsigned char *zip, size_t size, Conf *conf, char *why, int why_size)
{
        static int n;
        char tmp[PATH_MAX];
        size_t len;
        unsigned char *member = zip_member(zip, size, "manifest.lua", &len);

        if (!member) {
                snprintf(why, why_size, "not a package: no manifest.lua");
                return false;
        }
        /* conf.h only reads files */
        snprintf(tmp, sizeof tmp, "%s/pkg-%d-manifest.lua", tmp_dir, n++);
        bool ok = SaveFileData(tmp, member, len) && Conf_open(conf, tmp) == CONF_OK;
        free(member);
        remove(tmp);
        if (!ok) snprintf(why, why_size, "broken manifest.lua");
        return ok;
}

/* The package at PATH (ZIP, SIZE bytes) has no library for this platform:
 * build it from its src/, with its Makefile (lineit-so), to modules.cache.
 * Built once: the cache is by the package's CRC, so a new one builds again.
 * Returns a copy of it in tmp_dir (to free and remove), or NULL with the
 * reason in WHY. */
char *
package_build(const char *path, const unsigned char *zip, size_t size, char *why, int why_size)
{
        static int n;
        char cache[PATH_MAX], base[PATH_MAX * 2], dir[PATH_MAX * 2 + 8], so[PATH_MAX * 2 + 8], tmp[PATH_MAX];
        unsigned long crc = crc32(0, zip, size);

        /* Absolute: make runs in the package */
        if (!mkdir_p(cfg.modules.cache) || !realpath(cfg.modules.cache, cache)) {
                snprintf(why, why_size, "not built for " LI_PLATFORM ", and can't make %s (modules.cache)", cfg.modules.cache);
                return NULL;
        }
        snprintf(base, sizeof base, "%s/%s-%08lx", cache, GetFileNameWithoutExt(path), crc);
        snprintf(dir, sizeof dir, "%s.build", base);
        snprintf(so, sizeof so, "%s.so", base);
        if (!FileExists(so)) {
                fprintf(stderr, "modules: %s is not built for " LI_PLATFORM ", building it in %s\n", path, dir);
                output_add(TextFormat("%s is not built for " LI_PLATFORM ", building it", path), true);
                if (!package_extract(zip, size, dir, why, why_size)) return NULL;
                if (!package_make(dir, "lineit-so", so)) {
                        snprintf(why, why_size, "not built for " LI_PLATFORM ", and make lineit-so failed in %s", dir);
                        return NULL;
                }
        }
        snprintf(tmp, sizeof tmp, "%s/pkg-built-%d.so", tmp_dir, n++);
        if (!copy_file(so, tmp)) {
                snprintf(why, why_size, "can't copy %s", so);
                return NULL;
        }
        return strdup(tmp);
}

/* Take the library for this platform out of the package at PATH (or build
 * it, see package_build). Returns where it went (to free and remove), or
 * NULL with the reason in WHY. */
char *
package_lib(const char *path, char *why, int why_size)
{
        static int n;
        int size              = 0;
        unsigned char *zip    = LoadFileData(path, &size);
        size_t len            = 0;
        unsigned char *member = NULL;
        char tmp[PATH_MAX], member_name[PATH_MAX] = "";
        char *lib = NULL;
        const char *str;
        Conf conf;
        Version api;
        SumList sums = { 0 };
        bool has_api = false;

        if (!zip || !tmp_dir_make()) {
                snprintf(why, why_size, "can't read it");
                goto done;
        }
        if (!package_manifest(zip, size, &conf, why, why_size)) goto done;
        has_api = conf_version(conf, "api", &api, why, why_size);
        if (Conf_get_str(conf, &str, "platforms.%s", LI_PLATFORM) == CONF_OK) snprintf(member_name, sizeof member_name, "%s", str);
        Conf_close(conf);

        if (!has_api) goto done; /* why says it */
        if (!version_ok(api, "package", why, why_size)) goto done;
        /* All of it, even when its library is built already (the cache is
         * by its CRC): one that doesn't match isn't used at all. From here
         * on, its files are what manifest.lua says. */
        if (!package_sums(zip, size, &sums, why, why_size)) goto done;
        if (!*member_name) {
                lib = package_build(path, zip, size, why, why_size);
                goto done;
        }
        if (!(member = zip_member(zip, size, member_name, &len))) {
                snprintf(why, why_size, "%s missing or broken in it", member_name);
                goto done;
        }
        snprintf(tmp, sizeof tmp, "%s/pkg-%d.so", tmp_dir, n++);
        if (!SaveFileData(tmp, member, len)) {
                remove(tmp);
                snprintf(why, why_size, "can't write %s", tmp);
                goto done;
        }
        lib = strdup(tmp);
done:
        free(member);
        sums_free(&sums);
        UnloadFileData(zip);
        return lib;
}
