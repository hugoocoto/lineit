/* branches.c - see branches.h. Branches run on threads of a pool that
 * grows as needed (a branch waiting for its children keeps its thread), and
 * everything they share is in the Sched, under its mutex. */
#define _GNU_SOURCE
#include <pthread.h>

#include "branches.h"
#include "cum.h"

#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct Group Group;

typedef enum State {
        NEW,     /* made, its thread not there yet */
        RUN,     /* running */
        PARKED,  /* stepping: stopped before AT */
        BLOCKED, /* waiting for a node another branch has */
        JOIN,    /* waiting for its children */
        MERGE,   /* waiting at a merge for its relatives */
        DONE,
} State;

typedef struct Branch {
        Sched *sched;
        Sched_Task task;
        Group *group; /* its siblings, and its parent */
        State state;
        int at;                    /* the node it's running, or stopped before */
        Da(int) path;              /* every node it's in, outermost first: they all can tick more */
        int merge_node, merge_pin; /* where it waits, MERGE */
        long arrived;              /* when it got there: the last one runs it */
        bool skip;                 /* another runs the merge: go on without it */
        long gate;                 /* the step it may run, stepping */
        int wants;                 /* BLOCKED: the node */
        Group *kids;               /* JOIN: the children it waits for */
        pthread_cond_t wake;
} Branch;

/* Branches forked together: the children of PARENT, or the starts */
struct Group {
        Branch *parent; /* NULL for starts */
        Da(Branch *) items;
        int alive;
};

/* Who has a node: a branch, or the main thread (MAIN) */
typedef struct Owner {
        void *who;
        int count; /* it can take it again: a square that reads itself */
} Owner;

#define MAIN ((void *) 1)

struct Sched {
        Sched_Hooks h;
        pthread_mutex_t m;
        pthread_cond_t changed; /* for the main thread: a branch stopped, waits, or ended */
        Da(Branch *) branches;  /* alive */
        Da(Group *) groups;
        Da(Owner) owners; /* by node */
        bool step;
        atomic_bool abort; /* written with the lock, read by branches without it too (sched_aborted) */
        long gen;          /* steps given */
        long clock;
};

static _Thread_local Branch *cur; /* the branch the thread runs, NULL outside one */

/* ---- The pool ---- */

static pthread_mutex_t pool_m = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t pool_cv = PTHREAD_COND_INITIALIZER;
static Da(Branch *) queue;
static int waiting; /* threads waiting for a branch */

static void branch_main(Branch *b);

static void *
worker(void *arg)
{
        (void) arg;
        for (;;) {
                pthread_mutex_lock(&pool_m);
                while (!queue.count) {
                        waiting++;
                        pthread_cond_wait(&pool_cv, &pool_m);
                        waiting--;
                }
                Branch *b = queue.items[0];
                Da_remove(&queue, 0);
                pthread_mutex_unlock(&pool_m);
                branch_main(b);
        }
        return NULL;
}

/* B, on a thread: a waiting one, or a new one if there's none free */
static void
submit(Branch *b)
{
        pthread_mutex_lock(&pool_m);
        Da_append(&queue, b);
        if (queue.count > waiting) {
                pthread_t t;
                pthread_attr_t a;
                pthread_attr_init(&a);
                pthread_attr_setdetachstate(&a, PTHREAD_CREATE_DETACHED);
                pthread_attr_setstacksize(&a, 8 << 20); /* ticks are calls: chains go deep */
                if (pthread_create(&t, &a, worker, NULL)) fprintf(stderr, "branches: can't make a thread\n");
                pthread_attr_destroy(&a);
        } else {
                pthread_cond_signal(&pool_cv);
        }
        pthread_mutex_unlock(&pool_m);
}

/* ---- Sched ---- */

Sched *
sched_new(Sched_Hooks hooks)
{
        Sched *s = calloc(1, sizeof *s);
        s->h     = hooks;
        pthread_mutexattr_t a;
        pthread_mutexattr_init(&a);
        pthread_mutexattr_settype(&a, PTHREAD_MUTEX_RECURSIVE);
        pthread_mutex_init(&s->m, &a);
        pthread_mutexattr_destroy(&a);
        pthread_cond_init(&s->changed, NULL);
        return s;
}

static void
group_free(Group *g)
{
        Da_destroy(&g->items);
        free(g);
}

static void
branch_free(Branch *b)
{
        Da_destroy(&b->path);
        pthread_cond_destroy(&b->wake);
        free(b);
}

/* With the lock: nothing runs, and nothing will without being told */
static bool
quiet(Sched *s)
{
        Da_foreach(b, s->branches)
        {
                if ((*b)->state == NEW || (*b)->state == RUN) return false;
        }
        return true;
}

/* With the lock: B goes on (the main thread sees it running from now) */
static void
wake(Branch *b)
{
        b->state = RUN;
        pthread_cond_signal(&b->wake);
}

/* With the lock: the ones stopped stepping go on (or whatever's waiting,
 * if it's ending) */
static void
wake_parked(Sched *s)
{
        Da_foreach(b, s->branches)
        {
                State st = (*b)->state;
                if (st == PARKED || (s->abort && (st == BLOCKED || st == MERGE))) wake(*b);
        }
        pthread_cond_broadcast(&s->changed);
}

/* With the lock: nothing runs, and nothing can, but some aren't done (they
 * wait at merges for each other's squares...): they'd wait forever. The
 * ones at merges go on. */
static void
unstick(Sched *s)
{
        fprintf(stderr, "branches: every branch is waiting, letting them go on\n");
        Da_foreach(b, s->branches)
        {
                if ((*b)->state != MERGE) continue;
                (*b)->skip = false;
                wake(*b);
        }
}

/* With the lock: the done ones out */
static void
reap(Sched *s)
{
        int k = 0;
        Da_foreach(b, s->branches)
        {
                if ((*b)->state == DONE)
                        branch_free(*b);
                else
                        s->branches.items[k++] = *b;
        }
        s->branches.count = k;
        if (!k) {
                Da_foreach(g, s->groups)
                {
                        group_free(*g);
                }
                s->groups.count = 0;
        }
}

/* The main thread: until they're all done, or, stepping, until none runs */
static bool
wait_main(Sched *s)
{
        pthread_mutex_lock(&s->m);
        for (;;) {
                bool done = true;
                Da_foreach(b, s->branches)
                {
                        done = done && (*b)->state == DONE;
                }
                if (done) break;
                if (quiet(s)) {
                        bool parked = false;
                        Da_foreach(b, s->branches)
                        {
                                parked = parked || (*b)->state == PARKED;
                        }
                        if (s->step && !s->abort && parked) {
                                pthread_mutex_unlock(&s->m);
                                return true;
                        }
                        if (!parked) unstick(s);
                }
                pthread_cond_wait(&s->changed, &s->m);
        }
        reap(s);
        pthread_mutex_unlock(&s->m);
        return false;
}

void
sched_abort(Sched *s)
{
        if (!s) return;
        pthread_mutex_lock(&s->m);
        s->abort = true;
        Da_foreach(b, s->branches)
        {
                if ((*b)->state == MERGE) (*b)->skip = true;
        }
        wake_parked(s);
        pthread_mutex_unlock(&s->m);
        wait_main(s);
        s->abort = false;
}

bool
sched_aborted(Sched *s)
{
        return s->abort;
}

void
sched_free(Sched *s)
{
        if (!s) return;
        sched_abort(s);
        Da_destroy(&s->branches);
        Da_destroy(&s->groups);
        Da_destroy(&s->owners);
        pthread_cond_destroy(&s->changed);
        pthread_mutex_destroy(&s->m);
        free(s);
}

void
sched_step(Sched *s, bool on)
{
        pthread_mutex_lock(&s->m);
        s->step = on;
        if (!on) wake_parked(s);
        pthread_mutex_unlock(&s->m);
}

bool
sched_busy(Sched *s)
{
        pthread_mutex_lock(&s->m);
        bool busy = s->branches.count > 0;
        pthread_mutex_unlock(&s->m);
        return busy;
}

int
sched_next(Sched *s, int *nodes, int max)
{
        int n = 0;
        pthread_mutex_lock(&s->m);
        Da_foreach(b, s->branches)
        {
                if ((*b)->state == PARKED && n < max) nodes[n++] = (*b)->at;
        }
        pthread_mutex_unlock(&s->m);
        return n;
}

/* With the lock: a new branch for T, in G */
static Branch *
branch_new(Sched *s, Sched_Task t, Group *g)
{
        Branch *b = calloc(1, sizeof *b);
        b->sched  = s;
        b->task   = t;
        b->group  = g;
        b->state  = NEW;
        b->at     = t.node;
        b->gate   = s->gen;
        pthread_cond_init(&b->wake, NULL);
        Da_append(&g->items, b);
        g->alive++;
        Da_append(&s->branches, b);
        return b;
}

static Group *
group_new(Sched *s, Branch *parent)
{
        Group *g  = calloc(1, sizeof *g);
        g->parent = parent;
        Da_append(&s->groups, g);
        return g;
}

static void merges_check(Sched *s);

static void
branch_main(Branch *b)
{
        Sched *s = b->sched;
        cur      = b;
        pthread_mutex_lock(&s->m);
        b->state = RUN;
        pthread_mutex_unlock(&s->m);

        b->task.fn(b->task.arg);

        pthread_mutex_lock(&s->m);
        Group *g = b->group;
        b->state = DONE;
        g->alive--;
        /* Out of its group: it can't come to a merge anymore */
        for (int i = 0; i < g->items.count; i++) {
                if (g->items.items[i] != b) continue;
                Da_remove(&g->items, i);
                break;
        }
        merges_check(s);
        if (!g->alive && g->parent) wake(g->parent);
        pthread_cond_broadcast(&s->changed);
        pthread_mutex_unlock(&s->m);
        cur = NULL;
}

bool
sched_run(Sched *s, const Sched_Task *tasks, int n)
{
        if (!n) return false;
        pthread_mutex_lock(&s->m);
        Group *g = group_new(s, NULL);
        Branch *made[n];
        for (int i = 0; i < n; i++)
                made[i] = branch_new(s, tasks[i], g);
        pthread_mutex_unlock(&s->m);
        for (int i = 0; i < n; i++)
                submit(made[i]);
        return wait_main(s);
}

bool
sched_resume(Sched *s)
{
        pthread_mutex_lock(&s->m);
        s->gen++;
        wake_parked(s);
        pthread_mutex_unlock(&s->m);
        return wait_main(s);
}

/* With the lock: B waits on its condition while COND. It isn't running
 * meanwhile (ST), for the main thread to know; whoever wakes it says it
 * runs again (wake). */
#define WAIT_WHILE(s, b, st, cond)                              \
        do {                                                    \
                State was_ = (b)->state;                        \
                while (cond) {                                  \
                        (b)->state = (st);                      \
                        pthread_cond_broadcast(&(s)->changed);  \
                        pthread_cond_wait(&(b)->wake, &(s)->m); \
                }                                               \
                (b)->state = was_;                              \
        } while (0)

/* ---- Nodes: one branch in each at a time ---- */

/* With the lock: whoever has NODE, made if it isn't there */
static Owner *
owner(Sched *s, int node)
{
        while (s->owners.count <= node)
                Da_append(&s->owners, ((Owner) { 0 }));
        return &s->owners.items[node];
}

/* With the lock: waiting for NODE, B would wait for itself (the one that
 * has it waits for one B has...): then it doesn't wait */
static bool
would_loop(Sched *s, Branch *b, int node)
{
        void *who = owner(s, node)->who;
        for (int k = 0; k < s->branches.count && who && who != MAIN; k++) {
                if (who == b) return true;
                Branch *o = who;
                if (o->state != BLOCKED) return false;
                who = owner(s, o->wants)->who;
        }
        return false;
}

/* WHO is a branch forked (maybe through others) by A */
static bool
descends(void *who, Branch *a)
{
        if (who == MAIN) return false;
        for (Branch *b = who; b; b = b->group->parent)
                if (b->group->parent == a) return true;
        return false;
}

/* With the lock: NODE is WHO's now, if it can be. A node of a branch that
 * forked WHO is WHO's to use too: that branch waits for it, it doesn't run
 * the node (a loop's body reads its index), and it stays its. */
static bool
take(Sched *s, void *who, int node)
{
        Owner *o = owner(s, node);
        if (o->who && o->who != who) return o->who != MAIN && descends(who, o->who);
        o->who = who;
        o->count++;
        return true;
}

void
sched_lock(Sched *s, int node)
{
        Branch *b = cur;
        pthread_mutex_lock(&s->m);
        if (!b) {
                take(s, MAIN, node); /* branches don't run while the main thread reads */
        } else if (!take(s, b, node)) {
                if (would_loop(s, b, node)) {
                        static bool warned;
                        if (!warned) fprintf(stderr, "branches: two squares read each other at once, letting it go\n");
                        warned = true;
                } else {
                        b->wants = node;
                        WAIT_WHILE(s, b, BLOCKED, !take(s, b, node) && !s->abort);
                }
        }
        pthread_mutex_unlock(&s->m);
}

void
sched_unlock(Sched *s, int node)
{
        pthread_mutex_lock(&s->m);
        Owner *o = owner(s, node);
        if (o->who == (cur ? (void *) cur : MAIN) && --o->count <= 0) {
                o->who   = NULL;
                o->count = 0;
                /* Who was waiting for it */
                Da_foreach(b, s->branches)
                {
                        if ((*b)->state == BLOCKED && (*b)->wants == node) wake(*b);
                }
        }
        pthread_mutex_unlock(&s->m);
}

int
sched_at(Sched *s, int node)
{
        Branch *b = cur;
        if (!b) return -1;
        pthread_mutex_lock(&s->m);
        int was = b->at;
        b->at   = node;
        Da_append(&b->path, node);
        /* Stepping: until the next step lets it run one square */
        if (s->step && !s->abort) WAIT_WHILE(s, b, PARKED, s->step && b->gate >= s->gen && !s->abort);
        b->gate = s->gen;
        pthread_mutex_unlock(&s->m);
        sched_lock(s, node);
        return was;
}

void
sched_back(Sched *s, int was)
{
        Branch *b = cur;
        if (!b) return;
        int node = b->path.count ? b->path.items[b->path.count - 1] : -1;
        if (node >= 0) sched_unlock(s, node);
        pthread_mutex_lock(&s->m);
        if (b->path.count) b->path.count--;
        if (was >= 0) b->at = was;
        pthread_mutex_unlock(&s->m);
}

/* ---- Forks and merges ---- */

void
sched_fork(Sched *s, const Sched_Task *tasks, int n)
{
        Branch *p = cur;
        if (!p || n < 2) {
                for (int i = 0; i < n; i++)
                        tasks[i].fn(tasks[i].arg);
                return;
        }

        pthread_mutex_lock(&s->m);
        Group *g = group_new(s, p);
        Branch *made[n];
        for (int i = 0; i < n; i++)
                made[i] = branch_new(s, tasks[i], g);
        p->kids  = g;
        p->state = JOIN; /* from now: its thread runs the first child, not it */
        pthread_mutex_unlock(&s->m);
        /* The first on this thread: it would only wait for them meanwhile */
        for (int i = 1; i < n; i++)
                submit(made[i]);
        branch_main(made[0]);
        cur = p;

        pthread_mutex_lock(&s->m);
        WAIT_WHILE(s, p, JOIN, g->alive > 0);
        p->state = RUN;
        p->kids  = NULL;
        for (int i = 0; i < s->groups.count; i++) {
                if (s->groups.items[i] != g) continue;
                Da_remove(&s->groups, i);
                break;
        }
        group_free(g);
        reap(s);
        pthread_mutex_unlock(&s->m);
}

/* A tick from where O is, or from any square it's inside of (a loop around
 * it ticks more once its body is done), can get to NODE */
static bool
path_reaches(Sched *s, Branch *o, int node)
{
        if (s->h.reach(s->h.ctx, o->at, node)) return true;
        Da_foreach(x, o->path)
        {
                if (s->h.reach(s->h.ctx, *x, node)) return true;
        }
        return false;
}

/* One of G, or of their children, waits at input PIN of NODE */
static bool
kids_at(Group *g, int node, int pin)
{
        Da_foreach(op, g->items)
        {
                Branch *o = *op;
                if (o->state == MERGE && o->merge_node == node && o->merge_pin == pin) return true;
                if (o->state == JOIN && o->kids && kids_at(o->kids, node, pin)) return true;
        }
        return false;
}

static bool could_come(Sched *s, Branch *o, int node, int pin, bool stuck);

static bool
kids_could(Sched *s, Group *g, int node, int pin, bool stuck)
{
        Da_foreach(op, g->items)
        {
                if (could_come(s, *op, node, pin, stuck)) return true;
        }
        return false;
}

/* Whether O could still get to input PIN of NODE. STUCK: nothing can run,
 * so one waiting at another merge doesn't count (if it waited for these
 * too, none would ever go). */
static bool
could_come(Sched *s, Branch *o, int node, int pin, bool stuck)
{
        switch (o->state) {
        case DONE: return false;
        case MERGE:
                /* Here already, or elsewhere: from there once that one goes */
                if (stuck || (o->merge_node == node && o->merge_pin == pin)) return false;
                return s->h.reach(s->h.ctx, o->merge_node, node) || path_reaches(s, o, node);
        case JOIN:
                /* Through its children; or itself once they're done, unless one
                 * of them is there already for it */
                if (!o->kids) return path_reaches(s, o, node);
                return kids_could(s, o->kids, node, pin, stuck) ||
                       (!kids_at(o->kids, node, pin) && path_reaches(s, o, node));
        default:
                /* Not in its first square yet (just made, or its thread just
                 * took it): that's where it goes */
                if (!o->path.count && o->task.node == node && o->task.pin == pin) return true;
                return path_reaches(s, o, node);
        }
}

/* Some relative of W could still get to where it waits: its siblings, and
 * the siblings of its parent, of the parent's parent... (a branch of a
 * fan-out inside another branch gets there for that one) */
static bool
relatives_could(Sched *s, Branch *w, bool stuck)
{
        for (Branch *c = w; c; c = c->group->parent) {
                Da_foreach(op, c->group->items)
                {
                        if (*op != c && could_come(s, *op, w->merge_node, w->merge_pin, stuck)) return true;
                }
        }
        return false;
}

/* With the lock: a merge of W whose relatives can't get there anymore
 * goes: the last one to get there runs it, the others go on without.
 * True if it went. */
static bool
merge_go(Sched *s, Branch *w, bool stuck)
{
        if (w->state != MERGE || relatives_could(s, w, stuck)) return false;
        int node = w->merge_node, pin = w->merge_pin;
        Branch *last = NULL;
        Da_foreach(op, s->branches)
        {
                Branch *o = *op;
                if (o->state != MERGE || o->merge_node != node || o->merge_pin != pin) continue;
                if (!last || o->arrived > last->arrived) last = o;
                o->skip = true;
                wake(o);
        }
        if (last) last->skip = false; /* there's W at least */
        return true;
}

/* With the lock: the merges that can go, until none can. If none can and
 * nothing runs, it's stuck: then one waiting elsewhere doesn't count. */
static void
merges_check(Sched *s)
{
        for (bool stuck = false;;) {
                bool went = false;
                for (int i = 0; i < s->branches.count; i++)
                        went = merge_go(s, s->branches.items[i], stuck) || went;
                if (went) {
                        stuck = false;
                        continue;
                }
                if (stuck) return;
                Da_foreach(b, s->branches)
                {
                        State st = (*b)->state;
                        if (st == NEW || st == RUN || st == PARKED || st == BLOCKED) return; /* it can run: it'll come */
                }
                stuck = true;
        }
}

bool
sched_merge(Sched *s, int node, int pin)
{
        Branch *b = cur;
        if (!b) return true;
        pthread_mutex_lock(&s->m);
        b->merge_node = node;
        b->merge_pin  = pin;
        b->arrived    = ++s->clock;
        b->skip       = false;
        b->state      = MERGE;
        merges_check(s);
        while (b->state == MERGE && !s->abort) {
                pthread_cond_broadcast(&s->changed);
                pthread_cond_wait(&b->wake, &s->m);
        }
        b->state = RUN;
        bool run = !b->skip && !s->abort;
        pthread_mutex_unlock(&s->m);
        return run;
}
