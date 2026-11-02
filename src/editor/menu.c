/* menu.c - menus: the canvas menu, submenus and search */
#include "editor.h"

struct MenuState menu;

Menu module_menu; /* one entry per module, folders are submenus */
Menu search_menu; /* the modules that match the search */
Menu canvas_menu; /* rebuilt each time it opens */

static TextInput menu_search_text;

TextInput *
menu_search(void)
{
        return &menu_search_text;
}

/* Entry LABEL of M that opens a submenu, created if needed */
Menu *
menu_submenu(Menu *m, const char *label)
{
        Da_foreach(it, *m)
        {
                if (it->sub && strcmp(it->label, label) == 0) return it->sub;
        }
        Menu *sub = calloc(1, sizeof *sub);
        Da_append(m, (MenuItem) { .label = strdup(label), .sub = sub });
        return sub;
}

/* Free the submenus and labels menu_submenu created */
void
menu_free(Menu *m)
{
        Da_foreach(it, *m)
        {
                if (!it->sub) continue;
                menu_free(it->sub);
                free(it->sub);
                free((char *) it->label);
        }
        Da_destroy(m);
}

int
module_cmp(const void *a, const void *b)
{
        return strcmp((*(Module **) a)->name, (*(Module **) b)->name);
}

void
menu_open(Menu *m, int id, Vector2 pos)
{
        menu.depth      = 1;
        menu.level[0]   = (MenuLevel) { .menu = m, .pos = pos };
        menu.id         = id;
        menu.world      = GetScreenToWorld2D(pos, camera);
        menu.searchable = false;
        menu.selected   = -1;
        menu.wire_from  = -1;
}

float
menu_item_height(void)
{
        return cfg.menu.font_size + 2 * cfg.menu.padding;
}

/* Drawn at the right of the entries that open a submenu */
#define MENU_ARROW ">"

Rectangle
menu_rect(int lvl)
{
        Menu *m      = menu.level[lvl].menu;
        Vector2 pos  = menu.level[lvl].pos;
        float w      = 0;
        bool has_sub = false;

        Da_foreach(it, *m)
        {
                float hint = it->hint ? measure_text(it->hint, cfg.menu.font_size) + 4 * cfg.menu.padding : 0;
                w          = fmaxf(w, measure_text(it->label, cfg.menu.font_size) + hint);
                has_sub    = has_sub || it->sub;
        }
        w += 4 * cfg.menu.padding;
        if (has_sub) w += measure_text(MENU_ARROW, cfg.menu.font_size) + 2 * cfg.menu.padding;

        float h = menu_item_height();

        /* The search menu stays where menu_search_open put it, only as many
         * entries as fit below it are shown, and the rest scroll */
        if (lvl == 0 && menu.searchable) {
                int rows = Clamp((int) ((GetScreenHeight() - pos.y) / h), 1, Max(m->count, 1));
                return (Rectangle) { pos.x, pos.y, menu.search_w, rows * h };
        }

        /* Taller than the window: as many as fit, and the rest scroll */
        int rows    = Clamp((int) (GetScreenHeight() / h), 1, Max(m->count, 1));
        Rectangle r = { pos.x, pos.y, w, rows * h };

        /* Keep it inside the window. Submenus flip to the parent's left. */
        if (r.x + r.width > GetScreenWidth())
                r.x = lvl > 0 ? menu_rect(lvl - 1).x - r.width : GetScreenWidth() - r.width;
        r.y = fminf(r.y, GetScreenHeight() - r.height);
        r.x = fmaxf(r.x, 0);
        r.y = fmaxf(r.y, 0);
        return r;
}

/* Entries of level LVL shown at once */
static int
menu_rows(int lvl)
{
        return (int) roundf(menu_rect(lvl).height / menu_item_height());
}

/* Keep the scroll of LVL in range */
static void
menu_scroll_clamp(int lvl)
{
        MenuLevel *l = &menu.level[lvl];
        l->scroll    = Clamp(l->scroll, 0, Max(l->menu->count - menu_rows(lvl), 0));
}

/* Scroll LVL so entry I is shown */
static void
menu_scroll_to(int lvl, int i)
{
        MenuLevel *l = &menu.level[lvl];
        int rows     = menu_rows(lvl);
        if (i < l->scroll) l->scroll = i;
        if (i >= l->scroll + rows) l->scroll = i - rows + 1;
        menu_scroll_clamp(lvl);
}

/* The canvas menu with its search box, opened at POS. Its size and place
 * are fixed now, wide enough for any module name and with the search box at
 * the top, so typing doesn't move it. */
void
menu_search_open(Menu *m, Vector2 pos)
{
        float h = menu_item_height();
        float w = 260;

        menu_open(m, -1, pos);
        menu.searchable = true;
        Da_foreach(it, *m)
        {
                w = fmaxf(w, measure_text(it->label, cfg.menu.font_size) + (it->sub ? measure_text(MENU_ARROW, cfg.menu.font_size) + 2 * cfg.menu.padding : 0));
        }
        Da_foreach(mp, modules)
        {
                w = fmaxf(w, measure_text((*mp)->name, cfg.menu.font_size));
        }
        menu.search_w = w + 4 * cfg.menu.padding;

        /* Below the search box, and up if the menu doesn't fit under it,
         * with room for some results too */
        int rows = Clamp((int) ((GetScreenHeight() - h) / h), 1, Max(m->count, 8));
        pos.y += h;
        pos.y             = fmaxf(fminf(pos.y, GetScreenHeight() - rows * h), h);
        pos.x             = fmaxf(fminf(pos.x, GetScreenWidth() - menu.search_w), 0);
        menu.level[0].pos = pos;
}

/* The search box, above the canvas menu */
Rectangle
menu_search_rect(void)
{
        Rectangle r = menu_rect(0);
        return (Rectangle) { r.x, r.y - menu_item_height(), r.width, menu_item_height() };
}

/* NEEDLE as a subsequence of HAY, ignoring case: its letters in order, maybe
 * with others between ("l/" is in "list/push" and "logic/and"). Returns the
 * length of the shortest part of HAY that has it, -1 if none does. */
static int
fuzzy_span(const char *hay, const char *needle)
{
        int best = -1;
        if (!*needle) return 0;
        for (int start = 0; hay[start]; start++) {
                if (tolower((unsigned char) hay[start]) != tolower((unsigned char) *needle)) continue;
                const char *q = needle + 1;
                int i         = start + 1;
                for (; *q && hay[i]; i++)
                        if (tolower((unsigned char) hay[i]) == tolower((unsigned char) *q)) q++;
                if (*q) break; /* later starts can't have it either */
                if (best < 0 || i - start < best) best = i - start;
        }
        return best;
}

typedef struct SearchHit {
        int score; /* lower is better */
        Module *module;
} SearchHit;

static int
search_hit_cmp(const void *a, const void *b)
{
        const SearchHit *x = a, *y = b;
        if (x->score != y->score) return x->score - y->score;
        return strcmp(x->module->name, y->module->name);
}

/* search_menu = the modules matching what's typed: that name first, then
 * names starting with it, names with it, paths with it ("math" finds
 * math/add), and last paths with its letters in that order ("l/" finds
 * everything in folders with an l), tighter matches first */
static void
search_menu_build(void)
{
        Da(SearchHit) hits = { 0 };
        /* Words as typed match names with _ ("not equal" is not_equal) */
        char q[sizeof menu_search()->buf];
        snprintf(q, sizeof q, "%s", menu_search()->buf);
        for (char *c = q; *c; c++)
                if (*c == ' ') *c = '_';

        Da_foreach(mp, modules)
        {
                if ((*mp)->gone) continue;
                int at    = find_icase((*mp)->label, q);
                int span  = fuzzy_span((*mp)->name, q);
                int score = !strcasecmp((*mp)->label, q) ? 0 : /* exactly it */
                            at == 0                         ? 1 :
                            at > 0                          ? 2 :
                            find_icase((*mp)->name, q) >= 0 ? 3 :
                            span >= 0                       ? 4 + span :
                                                              -1;
                if (score >= 0) Da_append(&hits, (SearchHit) { score, *mp });
        }
        if (hits.count) qsort(hits.items, hits.count, sizeof *hits.items, search_hit_cmp);

        search_menu.count = 0;
        Da_foreach(h, hits)
        {
                Da_append(&search_menu, (MenuItem) { .label = h->module->name, .fn = canvas_add_module, .arg = h->module });
        }
        if (!search_menu.count) Da_append(&search_menu, (MenuItem) { .label = "No modules match" });
        Da_destroy(&hits);
}

/* Item under screen point P, deepest menu first, or -1. Its level goes to LVL. */
static int
menu_hit(Vector2 p, int *lvl)
{
        for (int k = menu.depth - 1; k >= 0; k--) {
                Rectangle r = menu_rect(k);
                if (!CheckCollisionPointRec(p, r)) continue;
                *lvl = k;
                return Min(menu.level[k].scroll + (int) ((p.y - r.y) / menu_item_height()), menu.level[k].menu->count - 1);
        }
        return -1;
}

/* Returns true if the menu consumed the input this frame. */
bool
update_menu(void)
{
        Vector2 mouse = GetMousePosition();

        if (IsMouseButtonPressed(MOUSE_BUTTON_RIGHT)) {
                menu.right_press = mouse;
                menu.depth       = 0;
        }

        /* A right click that did not move (not a pan) opens the menu */
        if (IsMouseButtonReleased(MOUSE_BUTTON_RIGHT) && !settings_shown() &&
            Vector2Distance(mouse, menu.right_press) < 4.0f &&
            CheckCollisionPointRec(mouse, canvas_rect())) {
                Hit h = square_hit(GetScreenToWorld2D(mouse, camera));
                if (h.square) {
                        /* Right click on a square that isn't selected selects only it */
                        if (!h.square->selected) {
                                select_all(false);
                                h.square->selected = true;
                        }
                        square_menu_build(h.square);
                        menu_open(&square_menu, h.square->id, mouse);
                } else {
                        canvas_menu_build();
                        /* The search box goes where it was clicked */
                        menu_search_open(&canvas_menu, mouse);
                        text_input_set(menu_search(), "");
                }
        }

        if (!menu.depth) return false;

        if (menu.searchable) {
                bool changed  = false;
                TextResult tr = text_input_update(menu_search(), &changed);
                if (changed) {
                        /* Typing shows the modules that match instead */
                        bool any = menu_search()->len > 0;
                        if (any) search_menu_build();
                        menu.level[0].menu   = any ? &search_menu : &canvas_menu;
                        menu.level[0].scroll = 0;
                        menu.depth           = 1;
                        menu.selected        = any ? 0 : -1;
                }

                /* Arrows, or C-n and C-p */
                Menu *m   = menu.level[0].menu;
                bool ctrl = IsKeyDown(KEY_LEFT_CONTROL) || IsKeyDown(KEY_RIGHT_CONTROL);
                bool down = key_pressed(KEY_DOWN) || IsKeyPressedRepeat(KEY_DOWN) ||
                            (ctrl && (key_pressed(KEY_N) || IsKeyPressedRepeat(KEY_N)));
                bool up   = key_pressed(KEY_UP) || IsKeyPressedRepeat(KEY_UP) ||
                            (ctrl && (key_pressed(KEY_P) || IsKeyPressedRepeat(KEY_P)));
                if (down) menu.selected = (menu.selected + 1) % m->count;
                if (up) menu.selected = (menu.selected <= 0 ? m->count : menu.selected) - 1;
                if (down || up) {
                        menu.depth = 1;
                        menu_scroll_to(0, menu.selected);
                }

                /* Esc closes it, and so does Enter with nothing typed */
                if (tr == TEXT_CANCEL || (tr == TEXT_DONE && !menu_search()->len)) {
                        menu.depth = 0;
                        return true;
                }
                if (tr == TEXT_DONE && menu.selected >= 0) {
                        MenuItem *it = &m->items[menu.selected];
                        if (it->fn) {
                                menu.depth = 0;
                                it->fn(menu.id, it->arg);
                        }
                        return true;
                }
                /* A submenu can be over it (flipped left): that one gets the click */
                int over_lvl = 0;
                bool on_sub  = menu_hit(mouse, &over_lvl) >= 0 && over_lvl > 0;
                if (IsMouseButtonPressed(MOUSE_BUTTON_LEFT) && !on_sub && CheckCollisionPointRec(mouse, menu_search_rect()))
                        return true;
        } else if (key_pressed(KEY_ESCAPE)) {
                menu.depth = 0;
                return true;
        }

        /* The wheel scrolls the menu under the mouse */
        float wheel = GetMouseWheelMove();
        for (int k = menu.depth - 1; wheel != 0 && k >= 0; k--) {
                if (!CheckCollisionPointRec(mouse, menu_rect(k))) continue;
                menu.level[k].scroll -= (int) (wheel * 3);
                menu_scroll_clamp(k);
                break;
        }

        int lvl      = 0;
        int i        = menu_hit(mouse, &lvl);
        MenuItem *it = i >= 0 ? &menu.level[lvl].menu->items[i] : NULL;

        /* Hovering an entry opens its submenu and closes deeper ones */
        if (it) {
                menu.depth = lvl + 1;
                if (it->sub && menu.depth < MENU_DEPTH) {
                        Rectangle r     = menu_rect(lvl);
                        MenuLevel *next = &menu.level[menu.depth++];
                        /* Still open: keep where it's scrolled to */
                        int scroll = next->menu == it->sub ? next->scroll : 0;
                        *next      = (MenuLevel) {
                                     it->sub, { r.x + r.width, r.y + (i - menu.level[lvl].scroll) * menu_item_height() }, scroll
                        };
                        menu_scroll_clamp(menu.depth - 1);
                }
        }

        if (IsMouseButtonPressed(MOUSE_BUTTON_LEFT)) {
                if (it && it->sub) return true; /* it's open already */
                menu.depth = 0;
                if (it && it->fn) it->fn(menu.id, it->arg);
                return true; /* clicking outside just closes it */
        }
        return true;
}

/* The search box of the canvas menu, on top of it */
static void
draw_menu_search(void)
{
        Rectangle b     = menu_search_rect();
        TextInput *t    = menu_search();
        const char *txt = t->len ? TextFormat("%s%s", t->buf, fmod(GetTime(), 1.0) < 0.5 ? "_" : "") : "Type to search modules";
        DrawRectangleRec(b, cfg.colors.field);
        DrawRectangleLinesEx(b, 1.0f, cfg.colors.menu_border);
        draw_text(txt, b.x + 2 * cfg.menu.padding, b.y + cfg.menu.padding, cfg.menu.font_size,
                  t->len ? cfg.colors.square_text : cfg.colors.name);
}

void
draw_menu(void)
{
        float h       = menu_item_height();
        int hover_lvl = -1; /* menu_hit leaves it if nothing is hit */
        int hover     = menu_hit(GetMousePosition(), &hover_lvl);

        for (int k = 0; k < menu.depth; k++) {
                Menu *m     = menu.level[k].menu;
                Rectangle r = menu_rect(k);

                int scroll = menu.level[k].scroll;
                int rows   = menu_rows(k);

                DrawRectangleRec(r, cfg.colors.menu);
                for (int i = scroll; i < m->count && i < scroll + rows; i++) {
                        MenuItem *it   = &m->items[i];
                        Rectangle item = { r.x, r.y + (i - scroll) * h, r.width, h };
                        float ty       = item.y + cfg.menu.padding;

                        /* Highlight the hovered entry, the one chosen with the
                         * arrows and the ones whose submenu is open */
                        bool open = it->sub && k + 1 < menu.depth && menu.level[k + 1].menu == it->sub;
                        bool keys = k == 0 && menu.searchable && i == menu.selected;
                        if ((k == hover_lvl && i == hover) || open || keys) DrawRectangleRec(item, cfg.colors.menu_hover);

                        draw_text(it->label, item.x + 2 * cfg.menu.padding, ty, cfg.menu.font_size, cfg.colors.menu_text);
                        if (it->hint) {
                                float hw = measure_text(it->hint, cfg.menu.font_size);
                                draw_text(it->hint, item.x + item.width - hw - 2 * cfg.menu.padding, ty,
                                          cfg.menu.font_size, cfg.colors.name);
                        }
                        if (it->sub) {
                                float aw = measure_text(MENU_ARROW, cfg.menu.font_size);
                                draw_text(MENU_ARROW, item.x + item.width - aw - 2 * cfg.menu.padding, ty,
                                          cfg.menu.font_size, cfg.colors.menu_text);
                        }
                }
                /* Some don't fit: a scrollbar on the right, as long as the
                 * part shown */
                if (m->count > rows) {
                        float th = fmaxf(r.height * rows / m->count, 8);
                        float ty = r.y + (r.height - th) * scroll / (m->count - rows);
                        DrawRectangleRounded((Rectangle) { r.x + r.width - 5, ty + 2, 3, th - 4 }, 1.0f, 4, cfg.colors.name);
                }
                DrawRectangleLinesEx(r, 1.0f, cfg.colors.menu_border);
                /* Before the submenus: they may be over it (flipped left) */
                if (k == 0 && menu.searchable) draw_menu_search();
        }
}
