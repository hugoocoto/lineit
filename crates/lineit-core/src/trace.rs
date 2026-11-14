//! Records a run: each square that ran, with what came before it, what it
//! showed and printed. [`record`] runs the program with Step's turns (the
//! branches take turns on one thread), so what each square did is known
//! exactly. Bisect asks about the events.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use lineit_rt::Value;

use crate::interp::{Interp, Turn};
use crate::ir::Program;
use crate::model::SquareId;

pub type EventId = usize;

/// How many squares that ran a recording keeps, unless told otherwise: in
/// the editor and in `lineit bisect`.
pub const LIMIT: usize = 100_000;

/// A square that ran.
#[derive(Clone, Debug)]
pub struct Event {
    pub node: crate::ir::NodeId,
    /// The square of the canvas (the top-level one, for squares inside a subgraph: NodeInfo.square).
    pub square: SquareId,
    pub module: String,
    /// How many times this square ran before this one (0: the first). A
    /// subgraph's squares count together, as they are one square of the canvas.
    pub run: usize,
    /// The events just before it, causally: the one before it in its branch, or for the first square of a branch the event that fanned it out, or after a meet, the last events of the branches that met. Earlier events only (indexes < this one).
    pub parents: Vec<EventId>,
    /// The values shown on this square after it ran (its outputs, from the slots that `shows` it: see World::shown), by pin name.
    pub values: Vec<(String, Value)>,
    /// The canvas's variables after it ran (Program::vars).
    pub vars: Vec<(String, Value)>,
    /// What it printed.
    pub printed: Vec<String>,
    /// The values shown on the squares it ran inside of, when it started,
    /// by square and pin: a loop's `index` for the squares of its body.
    pub inside: Vec<(SquareId, String, Value)>,
}

#[derive(Clone, Debug, Default)]
pub struct Trace {
    pub events: Vec<Event>,
    /// It stopped at the limit before the program ended.
    pub truncated: bool,
    /// The program ended with this error (after the last event).
    pub error: Option<String>,
}

impl Trace {
    /// The parents of each event, for `bisect::Bisect::new`.
    pub fn parents(&self) -> Vec<Vec<EventId>> {
        self.events.iter().map(|e| e.parents.clone()).collect()
    }
}

/// Where a branch is in the trace.
#[derive(Default)]
struct Branch {
    /// The events just before its next one.
    last: Vec<EventId>,
    /// The branches it fanned out, while it waits for them, and what was
    /// before the fan-out.
    children: Vec<usize>,
    forked: Vec<EventId>,
}

struct Recorder {
    trace: Trace,
    limit: usize,
    /// Per interpreter branch (task), until all are done.
    branches: Vec<Branch>,
    /// What was before the branches that start now (clocks tick, the
    /// entries run again): the last events of the ones that ended.
    before: Vec<EventId>,
    runs: HashMap<SquareId, usize>,
    /// Per square, the slots that show its outputs, by pin.
    shown: HashMap<SquareId, Vec<(String, usize)>>,
    /// Per branch about to take its turn: what it's inside of.
    inside: HashMap<usize, Vec<(SquareId, String, Value)>>,
}

fn merge(into: &mut Vec<EventId>, from: &[EventId]) {
    into.extend_from_slice(from);
    into.sort_unstable();
    into.dedup();
}

impl Recorder {
    fn new(prog: &Program, limit: usize) -> Recorder {
        let mut shown: HashMap<SquareId, Vec<(String, usize)>> = HashMap::new();
        for (i, s) in prog.slots.iter().enumerate() {
            if let Some((sq, pin)) = &s.shows {
                shown.entry(*sq).or_default().push((pin.clone(), i));
            }
        }
        Recorder {
            trace: Trace::default(),
            limit,
            branches: Vec::new(),
            before: Vec::new(),
            runs: HashMap::new(),
            shown,
            inside: HashMap::new(),
        }
    }

    /// Right before the turn of branch I: the values on the squares it's
    /// inside of (in it and the branches that fanned it out) are what they
    /// are for the square it runs now. After the turn they may be the next
    /// ones already (the next index of a loop).
    fn before(&mut self, it: &Interp, i: usize) {
        let mut tasks = vec![i];
        while let Some(p) = it.task(*tasks.last().unwrap()).parent {
            tasks.push(p);
        }
        let mut squares: Vec<SquareId> = Vec::new();
        for t in tasks.iter().rev() {
            for n in it.task(*t).inside {
                let sq = it.program().nodes[n as usize].square;
                if !squares.contains(&sq) {
                    squares.push(sq);
                }
            }
        }
        let mut inside = Vec::new();
        for sq in squares {
            for (pin, s) in self.shown.get(&sq).into_iter().flatten() {
                inside.push((sq, pin.clone(), it.w.slot(*s)));
            }
        }
        self.inside.insert(i, inside);
    }

    /// Right after the turn of branch I: it ran a square, or not.
    fn after(&mut self, it: &Interp, i: usize) {
        // the branches that start with the step (clocks) are there already
        while self.branches.len() < it.task_count() && it.task(self.branches.len()).parent.is_none() {
            self.branches.push(Branch { last: self.before.clone(), ..Branch::default() });
        }
        let output = it.w.take_output();
        let mut inside = self.inside.remove(&i).unwrap_or_default();
        match it.task(i).ran {
            Some(node) if self.trace.events.len() < self.limit => {
                let info = &it.program().nodes[node as usize];
                inside.retain(|(sq, _, _)| *sq != info.square);
                let run = self.runs.entry(info.square).or_default();
                let values = self
                    .shown
                    .get(&info.square)
                    .map_or_else(Vec::new, |slots| slots.iter().map(|(pin, s)| (pin.clone(), it.w.slot(*s))).collect());
                self.trace.events.push(Event {
                    node,
                    square: info.square,
                    module: info.module.clone(),
                    run: *run,
                    parents: std::mem::take(&mut self.branches[i].last),
                    values,
                    vars: it.w.variables(),
                    printed: output,
                    inside,
                });
                *run += 1;
                self.branches[i].last = vec![self.trace.events.len() - 1];
            }
            Some(_) => self.trace.truncated = true,
            // it printed in the squares it ran before: the last one
            None => {
                if let Some(e) = self.branches[i].last.last() {
                    self.trace.events[*e].printed.extend(output);
                }
            }
        }
        // the branches it fanned out start where it is
        while self.branches.len() < it.task_count() {
            let p = it.task(self.branches.len()).parent.unwrap_or(i);
            let last = self.branches[p].last.clone();
            let child = self.branches.len();
            let b = &mut self.branches[p];
            b.forked = last.clone();
            b.children.push(child);
            self.branches.push(Branch { last, ..Branch::default() });
        }
        // the ones that were waiting and aren't: after the last events of
        // the branches they fanned out, those that ran something. Children
        // have higher indexes, so the inner fan-outs are done first.
        for p in (0..self.branches.len()).rev() {
            if self.branches[p].children.is_empty() || it.task(p).waiting {
                continue;
            }
            let children = std::mem::take(&mut self.branches[p].children);
            let forked = std::mem::take(&mut self.branches[p].forked);
            let mut last = Vec::new();
            for c in &children {
                if self.branches[*c].last != forked {
                    merge(&mut last, &self.branches[*c].last);
                }
            }
            self.branches[p].last = if last.is_empty() { forked } else { last };
        }
        if it.is_idle() {
            let mut end = Vec::new();
            for (i, b) in self.branches.iter().enumerate() {
                if it.task(i).parent.is_none() {
                    merge(&mut end, &b.last);
                }
            }
            self.before = end;
            self.branches.clear();
        }
    }
}

/// Runs PROG from the start, in turns, recording each square that runs, up to LIMIT events (clocks tick at once, as Step does; a program with clocks runs until the limit or flow/stop). STOP ends it early from another thread (the editor's Stop): then truncated.
pub fn record(prog: Program, base_dir: PathBuf, limit: usize, stop: Arc<AtomicBool>) -> Result<Trace, String> {
    let mut it = Interp::new(prog, base_dir)?;
    it.set_stop(stop.clone());
    let mut rec = Recorder::new(it.program(), limit);
    let mut error = it.start().err();
    while error.is_none() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if rec.trace.events.len() >= limit {
            rec.trace.truncated |= !(it.is_idle() && !it.has_clocks());
            break;
        }
        match it.step_with(|it, i, turn| match turn {
            Turn::Before => rec.before(it, i),
            Turn::After => rec.after(it, i),
        }) {
            Ok(crate::interp::Status::Idle) if !it.has_clocks() => break,
            Ok(_) => {}
            Err(e) => error = Some(e),
        }
    }
    let mut trace = rec.trace;
    trace.error = error;
    // `flow/stop` sets the flag too: that's the program ending
    trace.truncated |= stop.load(Ordering::Relaxed) && !it.w.ended.load(Ordering::Relaxed);
    Ok(trace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;

    fn lower(src: &str, dir: &std::path::Path) -> Program {
        let canvas = crate::file::read_canvas(src, "trace").unwrap();
        let mut reg = Registry::builtin();
        reg.load_dir(&dir.join("modules"), "");
        let l = crate::lower::lower(&canvas, &reg);
        l.program.unwrap_or_else(|| panic!("{:?}", l.diags))
    }

    fn root() -> PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn rec_src(src: &str, limit: usize) -> Trace {
        record(lower(src, &root().join("tests")), std::env::temp_dir(), limit, Arc::default()).unwrap()
    }

    fn rec(name: &str, limit: usize) -> Trace {
        let dir = root().join("tests");
        let src = std::fs::read_to_string(dir.join(name)).unwrap();
        record(lower(&src, &dir), dir, limit, Arc::default()).unwrap()
    }

    fn squares(t: &Trace) -> Vec<SquareId> {
        t.events.iter().map(|e| e.square).collect()
    }

    fn int_var(e: &Event, name: &str) -> i64 {
        e.vars.iter().find(|(n, _)| n == name).map(|(_, v)| v.int()).unwrap()
    }

    fn printed(t: &Trace) -> Vec<String> {
        t.events.iter().flat_map(|e| e.printed.clone()).collect()
    }

    #[test]
    fn parents_are_always_earlier_events() {
        for n in ["branches.lua", "clock.lua", "fizzbuzz.lua", "parallel.lua", "modules.lua", "vars.lua"] {
            let t = rec(n, 200);
            for (i, e) in t.events.iter().enumerate() {
                assert!(e.parents.iter().all(|p| *p < i), "{n}: event {i} has parents {:?}", e.parents);
                assert!(e.parents.windows(2).all(|w| w[0] < w[1]), "{n}: {i}: sorted, no repeats");
            }
        }
    }

    #[test]
    fn a_fan_out_and_its_meet() {
        let t = rec("branches.lua", 100);
        assert_eq!(squares(&t), [1, 3, 4, 5, 7, 8, 9]);
        // the three pushes come from the entry; the print (the meet) after all three
        let want: Vec<Vec<EventId>> = vec![vec![], vec![0], vec![0], vec![0], vec![1, 2, 3], vec![4], vec![5]];
        assert_eq!(t.parents(), want);
        assert!(!t.truncated && t.error.is_none());
        // each push saw the list grow by one, and the print saw all three
        let len = |i: usize| match &t.events[i].vars[0].1 {
            Value::List(l) => l.len(),
            v => panic!("{v:?}"),
        };
        assert_eq!([len(0), len(1), len(2), len(3), len(4)], [0, 1, 2, 3, 3]);
        assert_eq!(t.events[4].printed, ["3"]);
        assert_eq!(t.events[6].printed, ["after"]);
        assert!(t.events[..4].iter().all(|e| e.printed.is_empty()));
    }

    const NESTED: &str = r#"return { lineit = 1, squares = {
        { id = 1, module = "flow/entry" },
        { id = 2, module = "io/print", fields = { value = "a" } },
        { id = 3, module = "io/print", fields = { value = "b" } },
        { id = 4, module = "io/print", fields = { value = "c" } },
        { id = 5, module = "io/print", fields = { value = "d" } },
        { id = 6, module = "io/print", fields = { value = "inner" } },
        { id = 7, module = "io/print", fields = { value = "outer" } },
        { id = 8, module = "io/print", fields = { value = "end" } },
    }, links = {
        { 1, "then", 2, "tick" }, { 1, "then", 3, "tick" },
        { 2, "then", 4, "tick" }, { 2, "then", 5, "tick" },
        { 4, "then", 6, "tick" }, { 5, "then", 6, "tick" },
        { 6, "then", 7, "tick" }, { 3, "then", 7, "tick" },
        { 7, "then", 8, "tick" },
    } }"#;

    #[test]
    fn nested_fan_outs() {
        let t = rec_src(NESTED, 100);
        assert_eq!(squares(&t), [1, 2, 3, 4, 5, 6, 7, 8]);
        // c and d come from a; the inner meet after both; the outer one after b and the inner meet
        let want: Vec<Vec<EventId>> = vec![vec![], vec![0], vec![0], vec![1], vec![1], vec![3, 4], vec![2, 5], vec![6]];
        assert_eq!(t.parents(), want);
        assert_eq!(printed(&t), ["a", "b", "c", "d", "inner", "outer", "end"]);
    }

    #[test]
    fn a_meet_reached_by_a_branch_that_ran_nothing() {
        // 1 fans out to 2 and 3; 2 goes on to 3, the meet: its own branch (just that) has no event
        let src = r#"return { lineit = 1, squares = {
            { id = 1, module = "flow/entry" },
            { id = 2, module = "io/print", fields = { value = "a" } },
            { id = 3, module = "io/print", fields = { value = "m" } },
        }, links = { { 1, "then", 2, "tick" }, { 1, "then", 3, "tick" }, { 2, "then", 3, "tick" } } }"#;
        let t = rec_src(src, 100);
        assert_eq!(squares(&t), [1, 2, 3]);
        // only the branch that ran a square is a parent, not the entry again
        let want: Vec<Vec<EventId>> = vec![vec![], vec![0], vec![1]];
        assert_eq!(t.parents(), want);
    }

    #[test]
    fn two_branches_that_step_together_keep_their_own_values() {
        // both loops add to ::n by turns, one event each: what each event shows is the state
        // right after it, not after the other branch's turn
        let t = rec("parallel.lua", 12);
        assert!(t.truncated);
        assert_eq!(squares(&t)[..5], [1, 4, 5, 8, 9]);
        let n: Vec<i64> = t.events.iter().map(|e| int_var(e, "::n")).collect();
        assert_eq!(n[3..], [1, 2, 3, 4, 5, 6, 7, 8, 9]);
        let p: Vec<Vec<EventId>> = t.parents()[..7].to_vec();
        assert_eq!(p, [vec![], vec![0], vec![0], vec![1], vec![2], vec![3], vec![4]]);
        // the same square, again: its runs count, and its branch's events chain
        assert_eq!((t.events[5].square, t.events[5].run), (8, 1));
        assert_eq!((t.events[6].square, t.events[6].run), (9, 1));
    }

    #[test]
    fn a_loop_runs_its_squares_again_and_counts() {
        let t = rec("fizzbuzz.lua", 1000);
        assert!(!t.truncated);
        // the loop is one event; the body runs for each of the 15 values
        assert_eq!(t.events.iter().filter(|e| e.square == 2).count(), 1);
        let ifelse: Vec<_> = t.events.iter().filter(|e| e.square == 5).collect();
        assert_eq!(ifelse.iter().map(|e| e.run).collect::<Vec<_>>(), (0..15).collect::<Vec<_>>());
        // each comes after the last square of the iteration before
        for e in &ifelse[1..] {
            assert_eq!(e.parents.len(), 1);
            assert!([6, 10, 14, 15].contains(&t.events[e.parents[0]].square));
        }
        let want = std::fs::read_to_string(root().join("tests/fizzbuzz.out")).unwrap();
        assert_eq!(printed(&t).join("\n") + "\n", want);
        let fizz: Vec<_> = t.events.iter().filter(|e| e.square == 10).collect();
        assert_eq!(fizz.len(), 4);
        assert!(fizz.iter().all(|e| e.printed == ["Fizz"]));
    }

    #[test]
    fn the_squares_of_a_loop_body_have_the_index_of_their_iteration() {
        let t = rec("fizzbuzz.lua", 1000);
        let index =
            |e: &Event| e.inside.iter().find(|(sq, pin, _)| *sq == 2 && pin == "index").map(|(_, _, v)| v.int());
        // the loop's own event isn't inside of itself
        assert_eq!(index(t.events.iter().find(|e| e.square == 2).unwrap()), None);
        // each iteration starts at its ifelse: 1 to 15, and what comes after
        // it in the iteration has the same index, the last square too
        let mut i = 0;
        for e in t.events.iter().filter(|e| e.square != 1 && e.square != 2) {
            if e.square == 5 {
                i += 1;
            }
            assert_eq!(index(e), Some(i), "square {} run {}", e.square, e.run);
        }
        assert_eq!(i, 15);
        // FizzBuzz on 15, Fizz on 3, a number on 1
        let printed_at = |n: i64| {
            t.events.iter().filter(|e| index(e) == Some(n)).flat_map(|e| e.printed.clone()).collect::<Vec<_>>()
        };
        assert_eq!(printed_at(15), ["FizzBuzz"]);
        assert_eq!(printed_at(3), ["Fizz"]);
        assert_eq!(printed_at(1), ["1"]);
    }

    #[test]
    fn branches_fanned_out_of_a_loop_body_have_its_index() {
        let t = rec_src(
            r#"return { lineit = 1, squares = {
                { id = 1, module = "flow/entry" },
                { id = 2, module = "flow/loop", fields = { from = "0", to = "3" } },
                { id = 3, module = "io/print" },
                { id = 4, module = "io/print" },
            }, links = {
                { 1, "then", 2, "tick" }, { 2, "body", 3, "tick" }, { 2, "body", 4, "tick" },
                { 2, "index", 3, "value" }, { 2, "index", 4, "value" },
            } }"#,
            100,
        );
        let got: Vec<_> = t
            .events
            .iter()
            .filter(|e| e.square == 3 || e.square == 4)
            .map(|e| {
                (e.square, e.inside.iter().find(|(sq, _, _)| *sq == 2).map(|(_, _, v)| v.int()), e.printed.clone())
            })
            .collect();
        for (sq, index, printed) in &got {
            assert_eq!(Some(vec![index.unwrap().to_string()]), Some(printed.clone()), "square {sq}");
        }
        assert_eq!(got.len(), 6);
    }

    #[test]
    fn a_loop_of_wires_has_the_variable_of_each_run() {
        let t = rec("countdown.lua", 11);
        assert!(t.truncated);
        // set, ifelse, set, ifelse...
        assert_eq!(squares(&t), [1, 4, 6, 4, 6, 4, 6, 4, 6, 4, 6]);
        let sets: Vec<_> = t.events.iter().filter(|e| e.square == 4).collect();
        let got: Vec<_> = sets.iter().map(|e| (e.run, int_var(e, "n"))).collect();
        assert_eq!(got, [(0, 99999), (1, 99998), (2, 99997), (3, 99996), (4, 99995)]);
    }

    #[test]
    fn a_clock_ticks_one_after_the_other() {
        let t = rec("clock.lua", 20);
        assert!(t.truncated, "a clock goes on until the limit");
        // each tick: the clock's tick (square 1), the counter, the ifelse
        assert_eq!(squares(&t)[..6], [1, 2, 4, 1, 2, 4]);
        // a tick comes after the last event of the tick before: the count it keeps comes from there
        let want: Vec<Vec<EventId>> = vec![vec![], vec![0], vec![1], vec![2], vec![3], vec![4]];
        assert_eq!(t.parents()[..6], want);
        let counter: Vec<_> = t.events.iter().filter(|e| e.square == 2).collect();
        let counts: Vec<_> = counter.iter().map(|e| (e.run, e.values[0].1.int())).collect();
        assert_eq!(counts[..4], [(0, 1), (1, 2), (2, 3), (3, 4)]);
        // the third tick prints
        let p: Vec<_> = t.events.iter().filter(|e| !e.printed.is_empty()).collect();
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].square, p[0].printed.clone()), (5, vec!["three ticks".to_string()]));
    }

    #[test]
    fn squares_of_a_subgraph_are_events_of_its_square() {
        let t = rec("modules.lua", 100);
        let src = std::fs::read_to_string(root().join("tests/modules.lua")).unwrap();
        let canvas = crate::file::read_canvas(&src, "m").unwrap();
        for e in &t.events {
            assert!(canvas.squares.iter().any(|s| s.id == e.square), "square {} isn't on the canvas", e.square);
        }
        // `test/accumulate` (squares 5 and 6): its inner squares ran as 5, then 5 again, then 6
        let acc: Vec<_> = t.events.iter().filter(|e| e.square == 5 || e.square == 6).collect();
        assert_eq!(acc.iter().map(|e| (e.square, e.run)).collect::<Vec<_>>(), [(5, 0), (5, 1), (6, 0)]);
        assert!(acc.iter().all(|e| e.module == "var/set"), "the module is the inner square's");
        // a Lua module shows its outputs
        let tally = t.events.iter().find(|e| e.square == 11).unwrap();
        assert_eq!(tally.values, [("count".to_string(), Value::Int(1))]);
        let want = std::fs::read_to_string(root().join("tests/modules.out")).unwrap();
        assert_eq!(printed(&t).join("\n") + "\n", want);
    }

    #[test]
    fn the_limit() {
        // branches.lua has 7 events
        let t = rec("branches.lua", 7);
        assert_eq!((t.events.len(), t.truncated), (7, false));
        let t = rec("branches.lua", 6);
        assert_eq!((t.events.len(), t.truncated), (6, true));
        // inside a step: the three pushes take turns in one
        let t = rec("branches.lua", 2);
        assert_eq!((t.events.len(), t.truncated), (2, true));
        let t = rec("branches.lua", 0);
        assert_eq!((t.events.len(), t.truncated), (0, true));
    }

    #[test]
    fn an_error_ends_the_trace_and_is_in_it() {
        let src = r#"return { lineit = 1, squares = {
            { id = 1, module = "flow/entry" },
            { id = 2, module = "io/print", fields = { value = "before" } },
            { id = 3, module = "code/function", params = { code = "function boom()\n  error('no')\nend\n" } },
            { id = 4, module = "io/print", fields = { value = "after" } },
        }, links = { { 1, "then", 2, "tick" }, { 2, "then", 3, "tick" }, { 3, "then", 4, "tick" } } }"#;
        let t = rec_src(src, 100);
        // the square that failed is the last event
        assert_eq!(squares(&t), [1, 2, 3]);
        assert!(t.error.as_deref().is_some_and(|e| e.contains("no")), "{:?}", t.error);
        assert!(!t.truncated);
        assert_eq!(t.events[1].printed, ["before"]);
    }

    #[test]
    fn stopping() {
        // from another thread (already, here): truncated
        let dir = root().join("tests");
        let src = std::fs::read_to_string(dir.join("clock.lua")).unwrap();
        let t = record(lower(&src, &dir), dir, 100, Arc::new(AtomicBool::new(true))).unwrap();
        assert!(t.truncated && t.events.is_empty());
        // `flow/stop` is the program ending, not a cut
        let src = r#"return { lineit = 1, squares = {
            { id = 1, module = "flow/entry" },
            { id = 2, module = "io/print", fields = { value = "once" } },
            { id = 3, module = "flow/stop" },
            { id = 4, module = "io/print", fields = { value = "never" } },
        }, links = { { 1, "then", 2, "tick" }, { 2, "then", 3, "tick" }, { 3, "then", 4, "tick" } } }"#;
        let t = rec_src(src, 100);
        assert_eq!(squares(&t), [1, 2, 3]);
        assert!(!t.truncated && t.error.is_none());
    }
}
