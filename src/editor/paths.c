/* paths.c - where lineit finds its config, modules and runtime */
#include "editor.h"

/* Where lineit finds its modules, its runtime and its config, if the config
 * doesn't say. It depends on where the binary is:
 *
 *   in the source tree   ./lineit, next to src/lineit.h: the tree, as make
 *                        leaves it, and its config.lua
 *   installed            PREFIX/bin/lineit, with PREFIX/lib/lineit (make
 *                        install, the AppImage): PREFIX/lib/lineit and
 *                        PREFIX/share/lineit for what comes with it, and
 *                        the XDG folders for the user's own: the config
 *                        ($XDG_CONFIG_HOME/lineit/config.lua), modules,
 *                        compiled programs and the package cache
 *   anywhere else        the built-in defaults, relative to the current
 *                        folder (the tests)
 *
 * A config.lua in the current folder is never read on its own: it's Lua, so
 * running lineit in a folder someone gave you would run their code. Use -c
 * for that one. */

static char config_default[PATH_MAX];

/* $VAR, or $HOME/HOME_DIR if it's not set, plus /lineit/WHAT, in BUF */
static const char *
xdg_path(char *buf, int size, const char *var, const char *home_dir, const char *what)
{
        const char *v    = getenv(var);
        const char *home = getenv("HOME");
        if (v && *v == '/')
                snprintf(buf, size, "%s/lineit/%s", v, what);
        else
                snprintf(buf, size, "%s/%s/lineit/%s", home ? home : "", home_dir, what);
        return buf;
}

/* Set the path defaults of cfg, before it's copied to cfg_defaults */
static void
paths_default(void)
{
        static char path[PATH_MAX], user[PATH_MAX], source[PATH_MAX], cache[PATH_MAX];
        static char output[PATH_MAX], runtime[PATH_MAX];
        char exe[PATH_MAX], dir[PATH_MAX];
        ssize_t n = readlink("/proc/self/exe", exe, sizeof exe - 1);

        xdg_path(config_default, sizeof config_default, "XDG_CONFIG_HOME", ".config", "config.lua");
        if (n <= 0) return;
        exe[n] = '\0';
        snprintf(dir, sizeof dir, "%s", exe);
        *strrchr(dir, '/') = '\0';

        if (FileExists(TextFormat("%s/src/lineit.h", dir))) {
                snprintf(path, sizeof path, "%s/build/modules", dir);
                snprintf(user, sizeof user, "%s/modules", dir);
                snprintf(source, sizeof source, "%s/modules", dir);
                snprintf(cache, sizeof cache, "%s/build/cache", dir);
                snprintf(output, sizeof output, "%s/build/bin", dir);
                snprintf(runtime, sizeof runtime, "%s/build/rt", dir);
                snprintf(config_default, sizeof config_default, "%s/config.lua", dir);
        } else if (DirectoryExists(TextFormat("%s/../lib/lineit", dir))) {
                char prefix[PATH_MAX];
                if (!realpath(TextFormat("%s/..", dir), prefix)) return;
                snprintf(path, sizeof path, "%s/lib/lineit/modules", prefix);
                snprintf(runtime, sizeof runtime, "%s/lib/lineit/rt", prefix);
                snprintf(source, sizeof source, "%s/share/lineit/modules", prefix);
                xdg_path(user, sizeof user, "XDG_DATA_HOME", ".local/share", "modules");
                xdg_path(output, sizeof output, "XDG_DATA_HOME", ".local/share", "bin");
                xdg_path(cache, sizeof cache, "XDG_CACHE_HOME", ".cache", "packages");
                xdg_path(config_default, sizeof config_default, "XDG_CONFIG_HOME", ".config", "config.lua");
        } else {
                xdg_path(config_default, sizeof config_default, "XDG_CONFIG_HOME", ".config", "config.lua");
                return;
        }
        cfg.modules.path    = path;
        cfg.modules.user    = user;
        cfg.modules.source  = source;
        cfg.modules.cache   = cache;
        cfg.compile.output  = output;
        cfg.compile.runtime = runtime;
}

/* settings.lua in the folder of the config at PATH */
static void
settings_path_set(const char *path)
{
        const char *slash = strrchr(path, '/');
        if (slash)
                snprintf(settings_path, sizeof settings_path, "%.*s/settings.lua", (int) (slash - path + 1) - 1, path);
        else
                snprintf(settings_path, sizeof settings_path, "settings.lua");
}

/* Paths and the config at start: the one given with -c (GIVEN), or the
 * default, which may not be there. Returns its path. */
const char *
config_start(const char *given)
{
        paths_default();
        cfg.colors       = themes[0].colors;
        cfg_defaults     = cfg;
        const char *path = given ? given : config_default;
        settings_path_set(path);
        load_config(path, given != NULL);
        return path;
}

/* The config at PATH, and settings.lua over it. Either can be missing (but
 * not PATH if REQUIRED). Returns false if PATH can't be read: missing, or
 * not valid Lua. */
bool
load_config(const char *path, bool required)
{
        Conf conf = NULL, over = NULL;
        bool ok = true;

        if ((required || FileExists(path)) && Conf_open(&conf, path) != CONF_OK) {
                fprintf(stderr, "config: could not load %s, using defaults\n", path);
                conf = NULL;
                ok   = false;
        }
        if (FileExists(settings_path) && Conf_open(&over, settings_path) != CONF_OK) {
                fprintf(stderr, "config: could not load %s, ignoring it\n", settings_path);
                over = NULL;
        }

        /* The theme first: the colors set after change some of it */
        if (conf) setting_read(conf, &settings[0], settings[0].key);
        if (over) setting_read(over, &settings[0], settings[0].key);
        if (!theme_colors(conf, cfg.theme, &cfg.colors, 0)) {
                fprintf(stderr, "config: no theme called %s, using %s\n", cfg.theme, themes[0].name);
                cfg.colors = themes[0].colors;
        }
        user_themes_read(conf);

        for (int i = 0; i < settings_count; i++) {
                const Setting *s = &settings[i];
                setting_from[i]  = s->type == SET_COLOR ? FROM_THEME : FROM_DEFAULT;
                free(setting_over[i]);
                setting_over[i] = NULL;
                if (conf && setting_read(conf, s, s->key)) setting_from[i] = FROM_CONFIG;
                if (over && setting_read(over, s, s->key)) {
                        setting_from[i] = FROM_SETTINGS;
                        setting_over[i] = setting_lua(s, s->val);
                }
        }

        if (conf) Conf_close(conf);
        if (over) Conf_close(over);

        if (cfg.canvas.grid_step <= 0) cfg.canvas.grid_step = 32.0f;
        if (cfg.canvas.grid_major <= 0) cfg.canvas.grid_major = 1;
        if (cfg.canvas.wire_curve_distance <= 0) cfg.canvas.wire_curve_distance = 1.0f;
        if (cfg.canvas.pin_spacing <= 0) cfg.canvas.pin_spacing = 24.0f;
        if (cfg.output.lines < 1) cfg.output.lines = 1;
        if (cfg.save.autosave_interval < 1) cfg.save.autosave_interval = 1;
        return ok;
}
