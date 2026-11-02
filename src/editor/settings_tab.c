/* settings_tab.c - the Settings tab: every setting, searched and changed */
#include "editor.h"

/* File > Settings: every key of the config (see config.c), in a tab. Typing
 * searches them, the list on the left goes to a section. A value changed is
 * written to settings.lua and applied at once; Reset takes it out of there. */

static struct {
        TextInput search; /* typing goes here while no value is */
        int field;        /* setting being typed, -1 if none */
        TextInput text;
        bool error;   /* what's typed isn't valid */
        float scroll; /* of the list, pixels down */
} settings_ui = { .field = -1 };

bool
settings_shown(void)
{
        return tabs.count && tabs.items[tab_cur]->settings;
}

/* Run, Compile, Save... are for canvases */
bool
not_canvas(void)
{
        if (!settings_shown()) return false;
        set_status("Settings apply and are saved as they change");
        return true;
}

/* The settings tab, or a new one */
void
settings_open(void)
{
        for (int i = 0; i < tabs.count; i++) {
                if (!tabs.items[i]->settings) continue;
                tab_switch(i);
                return;
        }
        tab_add(NULL, true);
        settings_ui.scroll = 0;
}

/* "grid_step" for canvas.grid_step, as "Grid step" */
static void
setting_label(const Setting *s, char *buf, int size)
{
        const char *dot = strrchr(s->key, '.');
        snprintf(buf, size, "%s", dot ? dot + 1 : s->key);
        for (char *p = buf; *p; p++) {
                if (*p == '_') *p = ' ';
        }
        buf[0] = toupper((unsigned char) buf[0]);
}

/* The value of setting I as it's typed. A setting read at start shows what
 * it will have, from settings.lua. */
static void
setting_text(int i, char *buf, int size)
{
        const Setting *s = &settings[i];
        const char *over = setting_over[i];

        if (s->start && over && s->type == SET_STR) {
                /* The Lua string, unquoted */
                int n = 0;
                for (const char *p = over + 1; *p && *p != '"' && n < size - 1; p++)
                        buf[n++] = *p == '\\' && p[1] ? *++p : *p;
                buf[n] = '\0';
                return;
        }
        if (s->start && over) {
                snprintf(buf, size, "%s", over);
                return;
        }
        switch (s->type) {
        case SET_INT: snprintf(buf, size, "%d", *(int *) s->val); break;
        case SET_FLOAT: snprintf(buf, size, "%g", *(float *) s->val); break;
        case SET_BOOL: snprintf(buf, size, "%s", *(bool *) s->val ? "on" : "off"); break;
        case SET_STR: snprintf(buf, size, "%s", *(const char **) s->val); break;
        case SET_COLOR: {
                Color c = *(Color *) s->val;
                if (c.a == 255)
                        snprintf(buf, size, "%d, %d, %d", c.r, c.g, c.b);
                else
                        snprintf(buf, size, "%d, %d, %d, %d", c.r, c.g, c.b, c.a);
                break;
        }
        }
}

/* "r, g, b", "r g b a", "{ r, g, b }", "#rrggbb" or "#rrggbbaa" */
static bool
color_parse(const char *text, Color *c)
{
        int v[4] = { 0, 0, 0, 255 }, n = 0;
        char *end;

        while (isspace((unsigned char) *text))
                text++;
        if (*text == '#') {
                int len = 0;
                while (isxdigit((unsigned char) text[1 + len]))
                        len++;
                if ((len != 6 && len != 8) || text[1 + len]) return false;
                for (n = 0; n < len / 2; n++) {
                        char hex[3] = { text[1 + 2 * n], text[2 + 2 * n], '\0' };
                        v[n]        = (int) strtol(hex, NULL, 16);
                }
        } else {
                const char *p = text;
                for (n = 0; n < 4; n++) {
                        while (*p == ' ' || *p == ',' || *p == '{')
                                p++;
                        if (!isdigit((unsigned char) *p)) break;
                        long x = strtol(p, &end, 10);
                        if (x > 255) return false;
                        v[n] = (int) x;
                        p    = end;
                }
                while (*p == ' ' || *p == ',' || *p == '}')
                        p++;
                if (*p || n < 3) return false;
        }
        *c = (Color) { v[0], v[1], v[2], v[3] };
        return true;
}

/* A value of any type of setting */
typedef union SettingValue {
        int i;
        float f;
        bool b;
        const char *s;
        Color c;
} SettingValue;

/* TEXT as a value of S. False if it isn't one, or it's out of its range. */
static bool
setting_parse(const Setting *s, const char *text, SettingValue *v)
{
        char *end;
        bool limits = s->max > s->min;

        switch (s->type) {
        case SET_INT: {
                long n = strtol(text, &end, 10);
                if (end == text || *end || (limits && (n < s->min || n > s->max))) return false;
                v->i = (int) n;
                return true;
        }
        case SET_FLOAT: {
                double d = strtod(text, &end);
                if (end == text || *end || !isfinite(d) || (limits && (d < s->min || d > s->max))) return false;
                v->f = (float) d;
                return true;
        }
        case SET_STR: v->s = text; return *text || s->empty;
        case SET_COLOR: return color_parse(text, &v->c);
        case SET_BOOL: return false; /* toggled, not typed */
        }
        return false;
}

static int
setting_key_cmp(const void *a, const void *b)
{
        return strcmp(settings[*(const int *) a].key, settings[*(const int *) b].key);
}

/* settings.lua, from setting_over: nested tables like the config's */
static bool
settings_write(void)
{
        int order[SETTINGS_MAX], n = 0;
        char tmp[PATH_MAX + 8], open[4][64];
        int depth = 0;

        for (int i = 0; i < settings_count; i++) {
                if (setting_over[i]) order[n++] = i;
        }
        /* Sorted, the keys of a table are together */
        qsort(order, n, sizeof *order, setting_key_cmp);

        if (strchr(settings_path, '/') && !mkdir_p(GetDirectoryPath(settings_path))) return false;
        snprintf(tmp, sizeof tmp, "%s.tmp", settings_path);
        FILE *f = fopen(tmp, "w");
        if (!f) return false;
        fprintf(f, "-- What File > Settings in lineit changed. lineit writes it, and reads it\n"
                   "-- after %s: what's here wins. Remove a key to use the config's again.\n\n",
                GetFileName(config_path));

        for (int k = 0; k < n; k++) {
                char key[128], *parts[5], *save = NULL;
                int np = 0;
                snprintf(key, sizeof key, "%s", settings[order[k]].key);
                for (char *t = strtok_r(key, ".", &save); t && np < 5; t = strtok_r(NULL, ".", &save))
                        parts[np++] = t;
                if (!np) continue; /* no key */

                /* Close the tables it isn't in, open the ones it is */
                int same = 0;
                while (same < depth && same < np - 1 && !strcmp(open[same], parts[same]))
                        same++;
                while (depth > same) {
                        depth--;
                        fprintf(f, "%*s}%s\n", depth * 8, "", depth ? "," : "");
                }
                while (depth < np - 1 && depth < countof(open)) {
                        fprintf(f, "%*s%s = {\n", depth * 8, "", parts[depth]);
                        snprintf(open[depth], sizeof open[depth], "%s", parts[depth]);
                        depth++;
                }
                fprintf(f, "%*s%s = %s%s\n", depth * 8, "", parts[np - 1], setting_over[order[k]], depth ? "," : "");
        }
        while (depth > 0) {
                depth--;
                fprintf(f, "%*s}%s\n", depth * 8, "", depth ? "," : "");
        }
        if (fclose(f) != 0 || rename(tmp, settings_path) != 0) {
                remove(tmp);
                return false;
        }
        return true;
}

/* Setting I is VAL (its type) in settings.lua from now on, or isn't there
 * if VAL is NULL. It applies now. */
static void
settings_change(int i, const SettingValue *val)
{
        const Setting *s = &settings[i];
        char *lua        = val ? setting_lua(s, val) : NULL;

        if (val && !lua) return;
        free(setting_over[i]);
        setting_over[i] = lua;
        if (!settings_write()) {
                set_status("Could not write %s", settings_path);
                return;
        }
        /* Said now: the reload makes setting_over again */
        char said[256];
        if (!val)
                snprintf(said, sizeof said, "%s: reset%s", s->key, s->start ? ", from the next start" : "");
        else
                snprintf(said, sizeof said, "%s = %s%s", s->key, lua, s->start ? ", from the next start" : "");
        settings_mtime = file_mtime(settings_path);
        config_reload();
        set_status("%s", said);
}

/* Apply what's typed. On error it stays open. */
static bool
settings_edit_apply(void)
{
        SettingValue v;
        int i = settings_ui.field;

        if (i < 0) return true;
        if (!setting_parse(&settings[i], settings_ui.text.buf, &v)) {
                settings_ui.error = true;
                return false;
        }
        settings_ui.field = -1;
        settings_change(i, &v);
        return true;
}

/* Leaving the tab: what's typed is applied if it's valid */
void
settings_edit_end(void)
{
        if (!settings_edit_apply()) settings_ui.field = -1;
}

static void
settings_edit(int i)
{
        char buf[256];
        setting_text(i, buf, sizeof buf);
        text_input_set(&settings_ui.text, buf);
        settings_ui.field = i;
        settings_ui.error = false;
}

/* The theme list, opened by clicking its value */
static Menu theme_menu;

static void
theme_choose(int id, void *arg)
{
        (void) id;
        SettingValue v = { .s = arg };
        settings_change(0, &v);
}

static void
theme_menu_open(Rectangle at)
{
        theme_menu.count = 0;
        for (int i = 0; i < themes_count; i++)
                Da_append(&theme_menu, ((MenuItem) { .label = themes[i].name, .fn = theme_choose, .arg = (void *) themes[i].name }));
        Da_foreach(t, user_themes)
        {
                Da_append(&theme_menu, ((MenuItem) { .label = *t, .hint = "config", .fn = theme_choose, .arg = *t }));
        }
        menu_open(&theme_menu, -1, (Vector2) { at.x, at.y + at.height });
}

/* What the tab shows, from the top of the list: section and subsection
 * titles, and the settings that match the search */
typedef enum RowKind {
        ROW_SECTION,
        ROW_SUB,
        ROW_SETTING,
} RowKind;

typedef struct SettingsRow {
        RowKind kind;
        int i;      /* the setting, or the first one under the title */
        float y, h; /* in the list, before the scroll */
} SettingsRow;

typedef struct SettingsLayout {
        Rectangle nav;    /* sections, on the left (no width if narrow) */
        Rectangle search; /* above the list */
        Rectangle list;   /* where it scrolls */
        float x, w;       /* the column of the list */
        float pad, fs, small;
        SettingsRow rows[3 * SETTINGS_MAX];
        int count;
        float height; /* of all the rows */
} SettingsLayout;

static bool
setting_matches(const Setting *s, const char *q)
{
        char label[64];
        if (!*q) return true;
        setting_label(s, label, sizeof label);
        /* Not the titles: "int" would find all of Interface */
        return find_icase(s->key, q) >= 0 || find_icase(label, q) >= 0 || find_icase(s->help, q) >= 0;
}

static void
settings_layout(SettingsLayout *l)
{
        Rectangle a = canvas_rect();
        l->fs       = cfg.menu.font_size;
        l->small    = roundf(cfg.menu.font_size * 0.8f);
        l->pad      = Max(cfg.menu.padding, 4);

        float nav_w = a.width >= 720 ? fminf(240, a.width * 0.25f) : 0;
        l->nav      = (Rectangle) { a.x, a.y, nav_w, a.height };
        l->w        = fminf(a.width - nav_w - 4 * l->pad, 960);
        l->x        = a.x + nav_w + fmaxf(2 * l->pad, (a.width - nav_w - l->w) / 2);
        l->search   = (Rectangle) { l->x, a.y + 2 * l->pad, l->w, menu_item_height() };
        float top   = l->search.y + l->search.height + l->pad;
        l->list     = (Rectangle) { a.x + nav_w, top, a.width - nav_w, fmaxf(0, a.y + a.height - top) };

        const char *section = NULL, *sub = NULL;
        float y  = 0;
        l->count = 0;
        for (int i = 0; i < settings_count; i++) {
                const Setting *s = &settings[i];
                if (!setting_matches(s, settings_ui.search.buf)) continue;
                if (!section || strcmp(section, s->section)) {
                        float h             = l->fs * 1.4f + 3 * l->pad;
                        l->rows[l->count++] = (SettingsRow) { ROW_SECTION, i, y, h };
                        y += h;
                        section = s->section;
                        sub     = NULL;
                }
                if (!sub || strcmp(sub, s->sub)) {
                        float h             = l->fs + 2 * l->pad;
                        l->rows[l->count++] = (SettingsRow) { ROW_SUB, i, y, h };
                        y += h;
                        sub = s->sub;
                }
                float h             = l->fs + 2 * l->small + 4 * l->pad;
                l->rows[l->count++] = (SettingsRow) { ROW_SETTING, i, y, h };
                y += h;
        }
        l->height = y;
}

/* Row R on the screen */
static Rectangle
settings_row_rect(const SettingsLayout *l, const SettingsRow *r)
{
        return (Rectangle) { l->x, l->list.y + r->y - settings_ui.scroll, l->w, r->h };
}

/* Where the value of a setting in ROW is, and its Reset button */
static Rectangle
settings_value_rect(const SettingsLayout *l, Rectangle row)
{
        float w = fminf(300, l->w * 0.4f), h = menu_item_height();
        return (Rectangle) { row.x + row.width - w, row.y + (row.height - h) / 2, w, h };
}

static Rectangle
settings_reset_rect(const SettingsLayout *l, Rectangle row)
{
        Rectangle v = settings_value_rect(l, row);
        float w     = measure_text("Reset", l->small) + 2 * l->pad;
        return (Rectangle) { v.x - w - l->pad, v.y, w, v.height };
}

/* A color has its swatch at the left of the field */
static Rectangle
settings_field_rect(const SettingsLayout *l, const Setting *s, Rectangle row)
{
        Rectangle v = settings_value_rect(l, row);
        if (s->type != SET_COLOR) return v;
        return (Rectangle) { v.x + v.height + l->pad, v.y, v.width - v.height - l->pad, v.height };
}

/* Entries of the list on the left, one per title row. False past the last. */
static bool
settings_nav_rect(const SettingsLayout *l, int k, Rectangle *r)
{
        float y = l->nav.y + l->pad;
        for (int j = 0; j < l->count; j++) {
                const SettingsRow *row = &l->rows[j];
                if (row->kind == ROW_SETTING) continue;
                float h = (row->kind == ROW_SECTION ? l->fs : l->small) + 1.2f * l->pad;
                if (j == k) {
                        *r = (Rectangle) { l->nav.x, y, l->nav.width, h };
                        return true;
                }
                y += h;
        }
        return false;
}

/* The title shown at the top: the section or subsection being read */
static int
settings_current(const SettingsLayout *l)
{
        int cur = -1;
        for (int j = 0; j < l->count; j++) {
                if (l->rows[j].kind != ROW_SETTING && l->rows[j].y <= settings_ui.scroll + 1) cur = j;
        }
        return cur;
}

void
update_settings(void)
{
        static SettingsLayout l;
        bool changed  = false;
        Vector2 mouse = GetMousePosition();

        settings_layout(&l);

        /* Typing goes to the value being typed, or else to the search */
        if (settings_ui.field >= 0) {
                switch (text_input_update(&settings_ui.text, &changed)) {
                case TEXT_DONE:
                case TEXT_NEXT: settings_edit_apply(); break;
                case TEXT_CANCEL: settings_ui.field = -1; break;
                case TEXT_TYPING: break;
                }
                if (changed) settings_ui.error = false;
        } else {
                TextResult tr = text_input_update(&settings_ui.search, &changed);
                if (tr == TEXT_CANCEL && settings_ui.search.len) text_input_set(&settings_ui.search, "");
                if (changed || tr == TEXT_CANCEL) {
                        settings_ui.scroll = 0;
                        settings_layout(&l);
                }
        }

        float wheel = GetMouseWheelMove();
        if (wheel && CheckCollisionPointRec(mouse, l.list)) settings_ui.scroll -= wheel * 3 * (l.fs + 2 * l.small);
        settings_ui.scroll = Clamp(settings_ui.scroll, 0, fmaxf(0, l.height - l.list.height + 2 * l.pad));

        if (!IsMouseButtonPressed(MOUSE_BUTTON_LEFT)) return;

        /* Clicking away from the value being typed applies it, if it's valid */
        int typing = settings_ui.field;
        for (int j = 0; typing >= 0 && j < l.count; j++) {
                if (l.rows[j].kind != ROW_SETTING || l.rows[j].i != typing) continue;
                Rectangle f = settings_field_rect(&l, &settings[typing], settings_row_rect(&l, &l.rows[j]));
                if (CheckCollisionPointRec(mouse, f)) return;
        }
        settings_edit_end();

        for (int j = 0; j < l.count; j++) {
                Rectangle n;
                if (settings_nav_rect(&l, j, &n) && CheckCollisionPointRec(mouse, n)) {
                        settings_ui.scroll = l.rows[j].y;
                        return;
                }
        }

        if (!CheckCollisionPointRec(mouse, l.list)) return;
        for (int j = 0; j < l.count; j++) {
                const SettingsRow *r = &l.rows[j];
                Rectangle row        = settings_row_rect(&l, r);
                if (r->kind != ROW_SETTING || !CheckCollisionPointRec(mouse, row)) continue;

                const Setting *s = &settings[r->i];
                if (setting_over[r->i] && CheckCollisionPointRec(mouse, settings_reset_rect(&l, row))) {
                        settings_change(r->i, NULL);
                } else if (CheckCollisionPointRec(mouse, settings_value_rect(&l, row))) {
                        if (s->type == SET_BOOL) {
                                SettingValue v = { .b = !*(bool *) s->val };
                                settings_change(r->i, &v);
                        } else if (s->val == &cfg.theme) {
                                theme_menu_open(settings_value_rect(&l, row));
                        } else {
                                settings_edit(r->i);
                        }
                }
                return;
        }
}

static void
draw_settings_value(const SettingsLayout *l, int i, Rectangle row)
{
        const Setting *s = &settings[i];
        Rectangle v      = settings_value_rect(l, row);
        Rectangle f      = settings_field_rect(l, s, row);
        bool typing      = settings_ui.field == i;
        bool hover       = CheckCollisionPointRec(GetMousePosition(), v);
        float fs         = l->fs;
        char buf[256];

        if (s->type == SET_BOOL) {
                /* A switch at the right */
                bool on = *(bool *) s->val;
                float h = v.height * 0.7f, w = 2 * h;
                Rectangle t = { v.x + v.width - w, v.y + (v.height - h) / 2, w, h };
                DrawRectangleRounded(t, 1.0f, 12, on ? cfg.colors.square : cfg.colors.menu_border);
                DrawCircle(on ? t.x + w - h / 2 : t.x + h / 2, t.y + h / 2, h / 2 - 3, cfg.colors.field);
                const char *label = on ? "on" : "off";
                draw_text(label, t.x - measure_text(label, l->small) - l->pad, v.y + (v.height - l->small) / 2, l->small,
                          cfg.colors.name);
                return;
        }

        if (s->type == SET_COLOR) {
                Rectangle sw = { v.x, v.y, v.height, v.height };
                DrawRectangleRec(sw, *(Color *) s->val);
                DrawRectangleLinesEx(sw, 1.0f, cfg.colors.menu_border);
        }

        if (typing)
                snprintf(buf, sizeof buf, "%s%s", settings_ui.text.buf, fmod(GetTime(), 1.0) < 0.5 ? "_" : "");
        else
                setting_text(i, buf, sizeof buf);

        Color border = settings_ui.error && typing ? cfg.colors.error :
                       typing                      ? cfg.colors.pin_hover :
                       hover                       ? cfg.colors.name :
                                                     cfg.colors.menu_border;
        DrawRectangleRec(f, cfg.colors.field);
        DrawRectangleLinesEx(f, typing ? 2.0f : 1.0f, border);

        float room = f.width - 2 * l->pad;
        if (s->val == &cfg.theme) {
                /* It opens a list: an arrow down at the right */
                float a = fs * 0.5f, ax = f.x + f.width - l->pad - a, ay = f.y + f.height / 2;
                DrawTriangle((Vector2) { ax, ay - a / 3 }, (Vector2) { ax + a / 2, ay + a / 3 }, (Vector2) { ax + a, ay - a / 3 },
                             cfg.colors.square_text);
                room -= a + l->pad;
        }
        /* The end of it while it's typed, its start if not */
        float tw = measure_text(buf, fs);
        if (typing && tw > room) {
                Rectangle clip = GetCollisionRec(f, l->list);
                BeginScissorMode(clip.x, clip.y, clip.width, clip.height);
                draw_text(buf, f.x + l->pad - (tw - room), f.y + (f.height - fs) / 2, fs, cfg.colors.square_text);
                BeginScissorMode(l->list.x, l->list.y, l->list.width, l->list.height);
        } else {
                draw_text(text_fit(buf, fs, room), f.x + l->pad, f.y + (f.height - fs) / 2, fs, cfg.colors.square_text);
        }
}

void
draw_settings(void)
{
        static SettingsLayout l;
        Vector2 mouse = GetMousePosition();
        float pad, fs, small;

        settings_layout(&l);
        pad = l.pad, fs = l.fs, small = l.small;

        /* Sections on the left, the one being read marked */
        if (l.nav.width > 0) {
                int cur = settings_current(&l);
                DrawRectangleRec(l.nav, cfg.colors.bar);
                DrawLine(l.nav.x + l.nav.width, l.nav.y, l.nav.x + l.nav.width, l.nav.y + l.nav.height, cfg.colors.bar_border);
                BeginScissorMode(l.nav.x, l.nav.y, l.nav.width, l.nav.height);
                for (int j = 0; j < l.count; j++) {
                        Rectangle n;
                        if (!settings_nav_rect(&l, j, &n)) continue;
                        const SettingsRow *r = &l.rows[j];
                        const Setting *s     = &settings[r->i];
                        bool section         = r->kind == ROW_SECTION;
                        if (j == cur)
                                DrawRectangleRec(n, cfg.colors.menu);
                        else if (CheckCollisionPointRec(mouse, n))
                                DrawRectangleRec(n, Fade(cfg.colors.menu, 0.5f));
                        if (j == cur) DrawRectangle(n.x, n.y, 3, n.height, cfg.colors.square);
                        float size = section ? fs : small, x = n.x + (section ? 2 : 4) * pad;
                        draw_text(text_fit(section ? s->section : s->sub, size, n.x + n.width - x - pad), x,
                                  n.y + (n.height - size) / 2, size, section || j == cur ? cfg.colors.menu_text : cfg.colors.name);
                }
                EndScissorMode();
        }

        /* The search box: typing goes there while no value is typed */
        Rectangle sb   = l.search;
        bool searching = settings_ui.field < 0;
        DrawRectangleRec(sb, cfg.colors.field);
        DrawRectangleLinesEx(sb, searching ? 2.0f : 1.0f, searching ? cfg.colors.square : cfg.colors.menu_border);
        if (settings_ui.search.len)
                draw_text(TextFormat("%s%s", settings_ui.search.buf, searching && fmod(GetTime(), 1.0) < 0.5 ? "_" : ""),
                          sb.x + pad, sb.y + (sb.height - fs) / 2, fs, cfg.colors.square_text);
        else
                draw_text(searching && fmod(GetTime(), 1.0) < 0.5 ? "_ Type to search settings" : "  Type to search settings",
                          sb.x + pad, sb.y + (sb.height - fs) / 2, fs, Fade(cfg.colors.square_text, 0.5f));

        BeginScissorMode(l.list.x, l.list.y, l.list.width, l.list.height);
        for (int j = 0; j < l.count; j++) {
                const SettingsRow *r = &l.rows[j];
                Rectangle row        = settings_row_rect(&l, r);
                const Setting *s     = &settings[r->i];
                if (row.y + row.height < l.list.y || row.y > l.list.y + l.list.height) continue;

                if (r->kind == ROW_SECTION) {
                        float size = roundf(fs * 1.4f);
                        draw_text(s->section, row.x, row.y + 2 * pad, size, cfg.colors.menu_text);
                        DrawLine(row.x, row.y + row.height - 1, row.x + row.width, row.y + row.height - 1, cfg.colors.bar_border);
                        continue;
                }
                if (r->kind == ROW_SUB) {
                        draw_text(s->sub, row.x, row.y + pad, fs, cfg.colors.square);
                        continue;
                }

                if (CheckCollisionPointRec(mouse, row) && CheckCollisionPointRec(mouse, l.list))
                        DrawRectangleRec((Rectangle) { row.x - pad, row.y, row.width + 2 * pad, row.height },
                                         Fade(cfg.colors.menu, 0.5f));

                /* Name, what it does, and its key and where its value is from */
                char label[64];
                const char *from[] = {
                        [FROM_DEFAULT]  = "default",
                        [FROM_THEME]    = "from the theme",
                        [FROM_CONFIG]   = TextFormat("set in %s", GetFileName(config_path)),
                        [FROM_SETTINGS] = "set here",
                };
                setting_label(s, label, sizeof label);
                Rectangle reset = settings_reset_rect(&l, row);
                float room      = reset.x - row.x - pad;
                float y         = row.y + 2 * pad;
                draw_text(text_fit(label, fs, room), row.x, y, fs, cfg.colors.menu_text);
                draw_text(text_fit(s->help, small, room), row.x, y + fs + pad / 2, small, cfg.colors.name);
                draw_text(text_fit(TextFormat("%s  -  %s%s", s->key, from[(int) setting_from[r->i]],
                                              s->start ? ", read at start" : ""),
                                   small, room),
                          row.x, y + fs + small + pad, small, Fade(cfg.colors.name, 0.7f));

                if (setting_over[r->i]) {
                        bool hover = CheckCollisionPointRec(mouse, reset);
                        if (hover) DrawRectangleRec(reset, cfg.colors.menu_hover);
                        DrawRectangleLinesEx(reset, 1.0f, cfg.colors.menu_border);
                        draw_text("Reset", reset.x + pad, reset.y + (reset.height - small) / 2, small, cfg.colors.menu_text);
                }
                draw_settings_value(&l, r->i, row);
        }
        EndScissorMode();

        if (!l.count)
                draw_text("No setting matches that", l.x, l.list.y + 2 * pad, fs, cfg.colors.name);

        /* Where the view is, if it doesn't all fit */
        if (l.height > l.list.height) {
                float h = fmaxf(24, l.list.height * l.list.height / l.height);
                float y = l.list.y + (l.list.height - h) * settings_ui.scroll / fmaxf(1, l.height - l.list.height + 2 * pad);
                DrawRectangleRounded((Rectangle) { l.list.x + l.list.width - 8, y, 5, h }, 1.0f, 6, cfg.colors.menu_border);
        }
}
