/* themes.c - the built-in themes and the config's own */
#include "editor.h"

/* The built-in ones, the first is the default. theme = "NAME" in the config
 * picks one, or one of the config's own:
 *
 *   themes = {
 *           { name = "mine", base = "light", colors = { bg = { 255, 250, 240 }, types = { int = { 0, 128, 0 } } } },
 *   }
 *
 * which is its base (dark if it has none) with the colors it sets. Then
 * colors.* in the config changes the ones it sets, whatever the theme. */
const struct ThemesState themes[] = {
        {
        "dark",
        {
        .bg            = { 24, 24, 27, 255 },
        .grid          = { 39, 39, 42, 255 },
        .grid_major    = { 63, 63, 70, 255 },
        .bar           = { 15, 15, 17, 255 },
        .bar_border    = { 63, 63, 70, 255 },
        .bar_text      = { 161, 161, 170, 255 },
        .square        = { 102, 191, 255, 255 },
        .square_border = { 15, 15, 17, 255 },
        .square_text   = { 15, 15, 17, 255 },
        .name          = { 161, 161, 170, 255 },
        .pin_hover     = { 250, 204, 21, 255 },
        .selection     = { 250, 204, 21, 255 },
        .flash         = { 253, 224, 71, 255 },
        .output        = { 20, 20, 23, 255 },
        .output_text   = { 228, 228, 231, 255 },
        .error         = { 239, 68, 68, 255 },
        .error_text    = { 255, 255, 255, 255 },
        .field         = { 244, 244, 245, 255 },
        .types         = {
        [LI_NONE]   = { 113, 113, 122, 255 },
        [LI_TICK]   = { 228, 228, 231, 255 },
        [LI_ANY]    = { 161, 161, 170, 255 },
        [LI_BOOL]   = { 248, 113, 113, 255 },
        [LI_INT]    = { 74, 222, 128, 255 },
        [LI_FLOAT]  = { 190, 242, 100, 255 },
        [LI_STRING] = { 244, 114, 182, 255 },
        [LI_PTR]    = { 192, 132, 252, 255 },
        [LI_LIST]   = { 56, 189, 248, 255 },
        [LI_MAP]    = { 251, 146, 60, 255 },
        [LI_REF]    = { 45, 212, 191, 255 },
        [LI_BYTES]  = { 148, 163, 184, 255 },
        },
        .menu        = { 39, 39, 42, 255 },
        .menu_border = { 63, 63, 70, 255 },
        .menu_hover  = { 63, 63, 70, 255 },
        .menu_text   = { 228, 228, 231, 255 },
        },
        },
        {
        "light",
        {
        .bg            = { 250, 250, 250, 255 },
        .grid          = { 234, 234, 237, 255 },
        .grid_major    = { 212, 212, 216, 255 },
        .bar           = { 244, 244, 245, 255 },
        .bar_border    = { 212, 212, 216, 255 },
        .bar_text      = { 82, 82, 91, 255 },
        .square        = { 96, 165, 250, 255 },
        .square_border = { 39, 39, 42, 255 },
        .square_text   = { 9, 9, 11, 255 },
        .name          = { 82, 82, 91, 255 },
        .pin_hover     = { 234, 88, 12, 255 },
        .selection     = { 234, 88, 12, 255 },
        .flash         = { 245, 158, 11, 255 },
        .output        = { 244, 244, 245, 255 },
        .output_text   = { 39, 39, 42, 255 },
        .error         = { 220, 38, 38, 255 },
        .error_text    = { 255, 255, 255, 255 },
        .field         = { 255, 255, 255, 255 },
        .types         = {
        [LI_NONE]   = { 161, 161, 170, 255 },
        [LI_TICK]   = { 63, 63, 70, 255 },
        [LI_ANY]    = { 113, 113, 122, 255 },
        [LI_BOOL]   = { 220, 38, 38, 255 },
        [LI_INT]    = { 22, 163, 74, 255 },
        [LI_FLOAT]  = { 101, 163, 13, 255 },
        [LI_STRING] = { 219, 39, 119, 255 },
        [LI_PTR]    = { 147, 51, 234, 255 },
        [LI_LIST]   = { 2, 132, 199, 255 },
        [LI_MAP]    = { 234, 88, 12, 255 },
        [LI_REF]    = { 13, 148, 136, 255 },
        [LI_BYTES]  = { 71, 85, 105, 255 },
        },
        .menu        = { 255, 255, 255, 255 },
        .menu_border = { 212, 212, 216, 255 },
        .menu_hover  = { 228, 228, 231, 255 },
        .menu_text   = { 24, 24, 27, 255 },
        },
        },
        {
        "nord",
        {
        .bg            = { 46, 52, 64, 255 },
        .grid          = { 59, 66, 82, 255 },
        .grid_major    = { 67, 76, 94, 255 },
        .bar           = { 36, 41, 51, 255 },
        .bar_border    = { 67, 76, 94, 255 },
        .bar_text      = { 173, 181, 196, 255 },
        .square        = { 136, 192, 208, 255 },
        .square_border = { 36, 41, 51, 255 },
        .square_text   = { 46, 52, 64, 255 },
        .name          = { 173, 181, 196, 255 },
        .pin_hover     = { 235, 203, 139, 255 },
        .selection     = { 235, 203, 139, 255 },
        .flash         = { 235, 203, 139, 255 },
        .output        = { 41, 46, 57, 255 },
        .output_text   = { 229, 233, 240, 255 },
        .error         = { 191, 97, 106, 255 },
        .error_text    = { 236, 239, 244, 255 },
        .field         = { 236, 239, 244, 255 },
        .types         = {
        [LI_NONE]   = { 97, 110, 136, 255 },
        [LI_TICK]   = { 229, 233, 240, 255 },
        [LI_ANY]    = { 129, 161, 193, 255 },
        [LI_BOOL]   = { 191, 97, 106, 255 },
        [LI_INT]    = { 163, 190, 140, 255 },
        [LI_FLOAT]  = { 143, 188, 187, 255 },
        [LI_STRING] = { 180, 142, 173, 255 },
        [LI_PTR]    = { 208, 135, 112, 255 },
        [LI_LIST]   = { 94, 129, 172, 255 },
        [LI_MAP]    = { 208, 135, 112, 255 },
        [LI_REF]    = { 143, 188, 187, 255 },
        [LI_BYTES]  = { 216, 222, 233, 255 },
        },
        .menu        = { 59, 66, 82, 255 },
        .menu_border = { 76, 86, 106, 255 },
        .menu_hover  = { 76, 86, 106, 255 },
        .menu_text   = { 236, 239, 244, 255 },
        },
        },
        {
        "gruvbox",
        {
        .bg            = { 40, 40, 40, 255 },
        .grid          = { 50, 48, 47, 255 },
        .grid_major    = { 80, 73, 69, 255 },
        .bar           = { 29, 32, 33, 255 },
        .bar_border    = { 80, 73, 69, 255 },
        .bar_text      = { 168, 153, 132, 255 },
        .square        = { 131, 165, 152, 255 },
        .square_border = { 29, 32, 33, 255 },
        .square_text   = { 29, 32, 33, 255 },
        .name          = { 168, 153, 132, 255 },
        .pin_hover     = { 250, 189, 47, 255 },
        .selection     = { 250, 189, 47, 255 },
        .flash         = { 254, 128, 25, 255 },
        .output        = { 32, 32, 32, 255 },
        .output_text   = { 235, 219, 178, 255 },
        .error         = { 251, 73, 52, 255 },
        .error_text    = { 251, 241, 199, 255 },
        .field         = { 251, 241, 199, 255 },
        .types         = {
        [LI_NONE]   = { 124, 111, 100, 255 },
        [LI_TICK]   = { 235, 219, 178, 255 },
        [LI_ANY]    = { 168, 153, 132, 255 },
        [LI_BOOL]   = { 251, 73, 52, 255 },
        [LI_INT]    = { 184, 187, 38, 255 },
        [LI_FLOAT]  = { 142, 192, 124, 255 },
        [LI_STRING] = { 211, 134, 155, 255 },
        [LI_PTR]    = { 254, 128, 25, 255 },
        [LI_LIST]   = { 131, 165, 152, 255 },
        [LI_MAP]    = { 254, 128, 25, 255 },
        [LI_REF]    = { 142, 192, 124, 255 },
        [LI_BYTES]  = { 189, 174, 147, 255 },
        },
        .menu        = { 60, 56, 54, 255 },
        .menu_border = { 80, 73, 69, 255 },
        .menu_hover  = { 80, 73, 69, 255 },
        .menu_text   = { 235, 219, 178, 255 },
        },
        },
};
const int themes_count = countof(themes);

/* Names of the config's own themes, for Settings */
UserThemesDa user_themes;

/* Colors of theme NAME into OUT: a built-in one, or one of the themes list
 * of CONF (NULL: none). False if there's none called that. */
bool
theme_colors(Conf conf, const char *name, Colors *out, int depth)
{
        const char *str, *base = themes[0].name;
        int n = 0;

        for (int i = 0; i < countof(themes); i++) {
                if (strcmp(themes[i].name, name)) continue;
                *out = themes[i].colors;
                return true;
        }
        /* A base of a base... up to some, not forever if they loop */
        if (!conf || depth > 8 || Conf_get_len(conf, &n, "themes") != CONF_OK) return false;
        for (int t = 1; t <= n; t++) {
                if (Conf_get_str(conf, &str, "themes.%d.name", t) != CONF_OK || strcmp(str, name)) continue;
                Conf_get_str(conf, &base, "themes.%d.base", t);
                if (!theme_colors(conf, base, out, depth + 1)) {
                        fprintf(stderr, "config: theme %s: no theme called %s to start from\n", name, base);
                        *out = themes[0].colors;
                }
                for (int i = 0; i < settings_count; i++) {
                        const Setting *s = &settings[i];
                        if (s->type != SET_COLOR) continue;
                        char path[128];
                        snprintf(path, sizeof path, "themes.%d.%s", t, s->key);
                        conf_color(conf, (Color *) ((char *) out + ((char *) s->val - (char *) &cfg.colors)), path);
                }
                return true;
        }
        return false;
}

/* The names in the themes list of CONF, for Settings */
void
user_themes_read(Conf conf)
{
        const char *str;
        int n = 0;

        Da_foreach(t, user_themes)
        {
                free(*t);
        }
        user_themes.count = 0;
        if (!conf || Conf_get_len(conf, &n, "themes") != CONF_OK) return;
        for (int t = 1; t <= n; t++) {
                if (Conf_get_str(conf, &str, "themes.%d.name", t) == CONF_OK) Da_append(&user_themes, strdup(str));
        }
}
