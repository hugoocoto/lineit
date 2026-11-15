//! Branches in a compiled program.
//!
//! A tick output wired to several squares runs each of them as a branch, in
//! parallel (the first one on the thread that fanned out), and goes on once
//! they've all finished. Squares that two or more branches of one fan-out
//! can reach are its *meets*, worked out when compiling: a branch that gets
//! to one stops there instead, and once they're all done [`fan_out`] says
//! which meets were reached, for the caller to run once.
//!
//! Each thread keeps the fan-outs it's inside of, innermost last, so that a
//! branch of a fan-out inside another one stops at the outer one's meets
//! too.
//!
//! With the `pool` feature the branches run on a pool of threads (rayon's),
//! one for each core, not on one thread each. A branch can wait for another
//! one (a `while` on a global that a sibling sets), so a pool that's out of
//! threads must not queue it: [`Pool`] counts the branches it has, started
//! or queued, and one past the number of threads gets a thread of its own,
//! as without the feature. A queued branch is then only stuck when every
//! thread is stuck in a branch of the pool, which takes more of them than
//! there are threads.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

struct Meets {
    ids: &'static [u32],
    arrived: Vec<AtomicBool>,
}

thread_local! {
    static INSIDE: RefCell<Vec<Arc<Meets>>> = const { RefCell::new(Vec::new()) };
}

/// What a thread was inside of before it ran a branch, put back when that's
/// done: a thread that waits for its own fan-out can run other branches
/// (rayon steals them), each inside of its own fan-outs.
struct Outer(Vec<Arc<Meets>>);

impl Drop for Outer {
    fn drop(&mut self) {
        INSIDE.with(|i| *i.borrow_mut() = std::mem::take(&mut self.0));
    }
}

/// Runs B inside of INSIDE.
fn branch(inside: &[Arc<Meets>], b: &(dyn Fn() + Sync)) {
    let _outer = Outer(INSIDE.with(|i| std::mem::replace(&mut *i.borrow_mut(), inside.to_vec())));
    b();
}

/// Called before running square handler H where it's a meet of some
/// fan-out: true if this branch should stop, because H is a meet of a
/// fan-out it's in (which will run it once it's done).
pub fn arrive(h: u32) -> bool {
    INSIDE.with(|inside| {
        for m in inside.borrow().iter().rev() {
            if let Ok(i) = m.ids.binary_search(&h) {
                m.arrived[i].store(true, Ordering::Relaxed);
                return true;
            }
        }
        false
    })
}

/// Runs BRANCHES, in parallel, and returns the meets (of MEETS, sorted)
/// that any of them reached.
pub fn fan_out(meets: &'static [u32], branches: &[&(dyn Fn() + Sync)]) -> Vec<u32> {
    let m = Arc::new(Meets { ids: meets, arrived: meets.iter().map(|_| AtomicBool::new(false)).collect() });
    let mut inside = INSIDE.with(|i| i.borrow().clone());
    inside.push(m.clone());
    run(&inside, branches);
    meets.iter().zip(&m.arrived).filter(|(_, a)| a.load(Ordering::Relaxed)).map(|(h, _)| *h).collect()
}

/// A thread for each of BRANCHES but the first, which runs here.
#[cfg(not(feature = "pool"))]
fn run(inside: &[Arc<Meets>], branches: &[&(dyn Fn() + Sync)]) {
    std::thread::scope(|scope| {
        for b in branches.iter().skip(1) {
            std::thread::Builder::new()
                .stack_size(OWN_STACK)
                .spawn_scoped(scope, move || branch(inside, *b))
                .expect("lineit: couldn't start a thread for a branch");
        }
        if let Some(first) = branches.first() {
            branch(inside, *first);
        }
    });
}

/// The stack of a thread that runs one branch (the first one has the
/// stack of the thread that fanned out).
const OWN_STACK: usize = 16 << 20;

#[cfg(feature = "pool")]
pub use pool::Pool;

#[cfg(feature = "pool")]
fn run(inside: &[Arc<Meets>], branches: &[&(dyn Fn() + Sync)]) {
    static POOL: std::sync::OnceLock<Pool> = std::sync::OnceLock::new();
    POOL.get_or_init(|| Pool::new(std::thread::available_parallelism().map_or(4, |n| n.get()))).run(inside, branches);
}

#[cfg(feature = "pool")]
mod pool {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// The stack of the threads of the pool. A thread that waits for its
    /// fan-out runs other branches on top of what it's running, and a tick
    /// chain is a deep call: as much as the interpreter gives a branch, not
    /// the main thread's. It's address space, not memory, until it's used.
    const STACK: usize = 64 << 20;

    pub struct Pool {
        threads: rayon::ThreadPool,
        /// Branches that run on THREADS, or wait for one.
        pub(super) live: AtomicUsize,
    }

    /// Takes a place of the pool back when dropped, even in a panic.
    struct Place<'a>(&'a AtomicUsize);

    impl Drop for Place<'_> {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    impl Pool {
        pub fn new(threads: usize) -> Pool {
            let threads = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .stack_size(STACK)
                .thread_name(|i| format!("lineit branch {i}"))
                .build()
                .expect("lineit: couldn't start the threads for the branches");
            Pool { threads, live: AtomicUsize::new(0) }
        }

        /// Takes a place for one more branch, if the pool has one.
        fn reserve(&self) -> bool {
            let n = self.threads.current_num_threads();
            let mut l = self.live.load(Ordering::SeqCst);
            while l < n {
                match self.live.compare_exchange_weak(l, l + 1, Ordering::SeqCst, Ordering::SeqCst) {
                    Ok(_) => return true,
                    Err(now) => l = now,
                }
            }
            false
        }

        pub(super) fn run(&self, inside: &[Arc<Meets>], branches: &[&(dyn Fn() + Sync)]) {
            std::thread::scope(|own| {
                // doesn't run the closure on a thread of the pool, which
                // would take the first branch off the caller's stack
                self.threads.in_place_scope(|queue| {
                    for b in branches.iter().skip(1) {
                        let b = *b;
                        if self.reserve() {
                            queue.spawn(move |_| {
                                let _place = Place(&self.live);
                                branch(inside, b);
                            });
                        } else {
                            std::thread::Builder::new()
                                .stack_size(OWN_STACK)
                                .spawn_scoped(own, move || branch(inside, b))
                                .expect("lineit: couldn't start a thread for a branch");
                        }
                    }
                    if let Some(first) = branches.first() {
                        branch(inside, *first);
                    }
                });
            });
        }
    }
}

/// Runs F on a thread with a big stack: tick chains are calls, and a long
/// chain of squares is a deep one.
pub fn main_thread(f: impl FnOnce() + Send) {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(256 << 20)
            .spawn_scoped(scope, f)
            .expect("lineit: couldn't start the main thread")
            .join()
            .unwrap_or_else(|_| std::process::exit(101));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// Fans out N copies of F, with the index of each.
    fn many(n: usize, f: impl Fn(usize) + Sync) {
        let fs: Vec<_> = (0..n).map(|i| (i, &f)).collect();
        let bs: Vec<Box<dyn Fn() + Sync>> = fs.iter().map(|&(i, f)| Box::new(move || f(i)) as _).collect();
        fan_out(&[], &bs.iter().map(|b| &**b).collect::<Vec<_>>());
    }

    /// Ends the process if what's tested hangs, which is what it's for.
    fn watchdog() {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_secs(120));
            eprintln!("a fan-out hung");
            std::process::exit(1);
        });
    }

    #[test]
    fn meets() {
        let reached = fan_out(&[7, 9], &[&|| assert!(arrive(7)), &|| assert!(!arrive(8))]);
        assert_eq!(reached, [7]);
        assert!(!arrive(7));
        let reached = fan_out(
            &[1],
            &[&|| {
                let inner = fan_out(&[2], &[&|| assert!(arrive(1)), &|| assert!(arrive(2))]);
                assert_eq!(inner, [2]);
            }],
        );
        assert_eq!(reached, [1]);
    }

    #[test]
    fn far_more_branches_than_cores() {
        let n = AtomicUsize::new(0);
        many(2000, |_| {
            n.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(n.load(Ordering::Relaxed), 2000);
    }

    /// Fan-outs inside fan-outs inside..., on a pool that's smaller than
    /// what's running.
    #[test]
    fn nested() {
        fn down(depth: usize, n: &AtomicUsize) {
            if depth == 0 {
                n.fetch_add(1, Ordering::Relaxed);
            } else {
                many(3, |_| down(depth - 1, n));
            }
        }
        let n = AtomicUsize::new(0);
        down(7, &n);
        assert_eq!(n.load(Ordering::Relaxed), 3usize.pow(7));
    }

    /// A thread that runs a branch while it waits for its own fan-out goes
    /// back to the fan-outs it was inside of after.
    #[test]
    fn meets_in_nested_fan_outs_on_busy_threads() {
        let reached = fan_out(
            &[100],
            &[&|| {
                many(64, |_| {
                    let inner = fan_out(&[200], &[&|| assert!(arrive(100)), &|| assert!(arrive(200))]);
                    assert_eq!(inner, [200]);
                    assert!(arrive(100));
                })
            }],
        );
        assert_eq!(reached, [100]);
        assert!(!arrive(100) && !arrive(200));
    }

    /// Branches that wait for a sibling, as a `while` on a global does:
    /// all of them have to run at once, however many threads there are.
    #[test]
    fn branches_that_wait_for_each_other() {
        watchdog();
        for (n, setter) in [(3, 0), (3, 1), (3, 2), (200, 0), (200, 100), (200, 199)] {
            let go = AtomicBool::new(false);
            let spun = AtomicUsize::new(0);
            many(n, |i| {
                if i == setter {
                    go.store(true, Ordering::SeqCst);
                } else {
                    while !go.load(Ordering::SeqCst) {
                        std::hint::spin_loop();
                    }
                    spun.fetch_add(1, Ordering::SeqCst);
                }
            });
            assert_eq!(spun.load(Ordering::SeqCst), n - 1);
        }
    }

    /// The order that hangs rayon's own scope on two threads: a spinner on
    /// each thread and the setter queued behind them.
    #[cfg(feature = "pool")]
    #[test]
    fn two_threads_and_branches_that_wait() {
        watchdog();
        let pool = Pool::new(2);
        for n in [3, 4, 9] {
            for setter in 0..n {
                let go = AtomicBool::new(false);
                let spun = AtomicUsize::new(0);
                let f = |i: usize| {
                    if i == setter {
                        go.store(true, Ordering::SeqCst);
                    } else {
                        while !go.load(Ordering::SeqCst) {
                            std::hint::spin_loop();
                        }
                        spun.fetch_add(1, Ordering::SeqCst);
                    }
                };
                let bs: Vec<Box<dyn Fn() + Sync>> = (0..n).map(|i| Box::new(move || f(i)) as _).collect();
                pool.run(&[], &bs.iter().map(|b| &**b).collect::<Vec<_>>());
                assert_eq!(spun.load(Ordering::SeqCst), n - 1);
                assert_eq!(pool.live.load(Ordering::SeqCst), 0);
            }
        }
    }
}
