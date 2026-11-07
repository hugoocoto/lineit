//! Branches in a compiled program.
//!
//! A tick output wired to several squares runs each of them as a branch, on
//! its own thread (the first one on the thread that fanned out), and goes on
//! once they've all finished. Squares that two or more branches of one
//! fan-out can reach are its *meets*, worked out when compiling: a branch
//! that gets to one stops there instead, and once they're all done
//! [`fan_out`] says which meets were reached, for the caller to run once.
//!
//! Each thread keeps the fan-outs it's inside of, innermost last, so that a
//! branch of a fan-out inside another one stops at the outer one's meets
//! too.

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
    std::thread::scope(|scope| {
        for b in branches.iter().skip(1) {
            let inside = inside.clone();
            std::thread::Builder::new()
                .stack_size(16 << 20)
                .spawn_scoped(scope, move || {
                    INSIDE.with(|i| *i.borrow_mut() = inside);
                    b();
                })
                .expect("lineit: couldn't start a thread for a branch");
        }
        if let Some(first) = branches.first() {
            let outer = INSIDE.with(|i| std::mem::replace(&mut *i.borrow_mut(), inside));
            first();
            INSIDE.with(|i| *i.borrow_mut() = outer);
        }
    });
    meets.iter().zip(&m.arrived).filter(|(_, a)| a.load(Ordering::Relaxed)).map(|(h, _)| *h).collect()
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
}
