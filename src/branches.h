/* branches.h - branches: tick chains that run in parallel, on threads.
 * Shared by the editor (src/main.c) and the programs Compile makes
 * (src/rt/rt.c), so both run a canvas the same way.
 *
 * Each start of a run (every entry, every clock in a frame) is a branch,
 * and so is each target of a tick output wired to more than one input: the
 * tick forks, and returns once all of them are done. Branches run at the
 * same time, each on a thread of its own (taken from a pool), so the order
 * of what they do between them isn't fixed.
 *
 * A square (a node) runs one tick or read at a time: a branch that gets to
 * one another branch is in waits for it. So modules work as they are.
 *
 * A tick input with more than one wire is a merge. Branches that reach it
 * wait there while a relative of theirs could still get there: a sibling
 * (forked together), or a sibling of their parent, of its parent... The
 * last one to get there runs the square, once, and the others go on
 * without it. The two sides of an ifelse meeting again don't wait: they're
 * one branch.
 *
 * Stepping, every branch stops before every square, and each step lets all
 * of them run one square, in lockstep. Nodes are numbered by whoever links
 * this. */

#ifndef BRANCHES_H_
#define BRANCHES_H_

#include <stdbool.h>

/* What a branch runs: FN(ARG). It starts at NODE: the square it runs
 * first, and for a fork the input PIN of it ticked (-1 for starts). */
typedef struct Sched_Task {
        void (*fn)(void *arg);
        void *arg;
        int node, pin;
} Sched_Task;

/* Given by whoever links it */
typedef struct Sched_Hooks {
        void *ctx;
        bool (*reach)(void *ctx, int from, int to); /* a tick from node FROM can get to TO, through one wire or more */
} Sched_Hooks;

typedef struct Sched Sched;

Sched *sched_new(Sched_Hooks hooks);
void sched_free(Sched *s);

/* Run N branches, siblings, until they're all done: true if they stopped
 * stepping instead (sched_next says where). From outside a branch. */
bool sched_run(Sched *s, const Sched_Task *tasks, int n);

/* Go on: one square in every branch if stepping, or to the end. True if
 * they stopped again. */
bool sched_resume(Sched *s);

void sched_step(Sched *s, bool on);
bool sched_busy(Sched *s);                     /* branches left: they stopped stepping */
int sched_next(Sched *s, int *nodes, int max); /* the nodes they stopped before, how many */
void sched_abort(Sched *s);                    /* the branches left run no more squares, and end */
bool sched_aborted(Sched *s);                  /* while they end: don't run squares */

/* In a branch. Outside one they do what the program would do with one
 * branch: fork runs the tasks one after another, merge says to run. */

/* Run N branches, children of this one, and return when they're done */
void sched_fork(Sched *s, const Sched_Task *tasks, int n);

/* About to tick input PIN of NODE, a merge: false if another runs it */
bool sched_merge(Sched *s, int node, int pin);

/* About to run NODE: stepping, where it stops; and NODE is this branch's
 * until sched_back, so no other runs it meanwhile. Returns the node it was
 * at, for sched_back. */
int sched_at(Sched *s, int node);
void sched_back(Sched *s, int was);

/* NODE is this branch's (or the main thread's, outside one) to read it,
 * until sched_unlock */
void sched_lock(Sched *s, int node);
void sched_unlock(Sched *s, int node);

#endif /* BRANCHES_H_ */
