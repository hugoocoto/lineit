//! Finding the square a bug is in, like `git bisect`: a run is a DAG of
//! events (every square that ran, in the order it ran), and the user is asked
//! about points in it ("are the values right after this one?") until one
//! event is left.
//!
//! The DAG is given as the parents of each event, so this knows nothing about
//! traces or squares. Branches that run in parallel aren't before or after
//! each other: a good answer on one says nothing about the other.

use std::collections::HashSet;

/// An event, by its place in the order they ran (0 is the first).
pub type EventId = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// The state after the event is right: it and all before it are fine.
    Good,
    /// The state after the event is wrong: the bug is it or one before it.
    Bad,
    /// Can't tell; the event isn't asked again.
    Skip,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Is the state after this event right?
    Ask(EventId),
    /// The bug is in this event.
    Found(EventId),
    /// It's one of these (some were skipped), in event order.
    Range(Vec<EventId>),
    /// Everything was good: no square that ran is to blame.
    AllGood,
}

/// A bisect in progress.
pub struct Bisect {
    parents: Vec<Vec<EventId>>,
    /// Where it starts: `None` is the end of the run.
    start: Option<EventId>,
    log: Vec<(EventId, Answer)>,
    // what follows is worked out from the log after each change
    cands: Vec<EventId>,
    state: State,
}

impl Bisect {
    /// A bisect over the events whose parents are PARENTS (all of them
    /// before the event). BAD is the event known to be wrong, or `None` for
    /// the end of the run: then every event is a candidate.
    pub fn new(parents: Vec<Vec<EventId>>, bad: Option<EventId>) -> Bisect {
        for (e, ps) in parents.iter().enumerate() {
            assert!(ps.iter().all(|&p| p < e), "the parents of event {e} must be earlier events");
        }
        if let Some(b) = bad {
            assert!(b < parents.len(), "event {b} doesn't exist");
        }
        let mut b = Bisect { parents, start: bad, log: Vec::new(), cands: Vec::new(), state: State::AllGood };
        b.update();
        b
    }

    pub fn state(&self) -> State {
        self.state.clone()
    }

    /// Answers the question of `state()`; does nothing unless it's `Ask`.
    pub fn answer(&mut self, a: Answer) {
        if let State::Ask(e) = self.state {
            self.log.push((e, a));
            self.update();
        }
    }

    /// Takes the last answer back; false if there was none.
    pub fn undo(&mut self) -> bool {
        let had = self.log.pop().is_some();
        if had {
            self.update();
        }
        had
    }

    /// What's still a candidate, in event order.
    pub fn candidates(&self) -> Vec<EventId> {
        self.cands.clone()
    }

    /// Roughly how many more questions (log2 of the candidates, rounded up).
    pub fn steps_left(&self) -> usize {
        match self.cands.len() {
            0 | 1 => 0,
            n => (usize::BITS - (n - 1).leading_zeros()) as usize,
        }
    }

    /// The answers so far, in order.
    pub fn log(&self) -> &[(EventId, Answer)] {
        &self.log
    }

    /// The point known to be bad now: the start, or the last bad answer.
    fn bad(&self) -> Option<EventId> {
        self.log.iter().rev().find(|(_, a)| *a == Answer::Bad).map(|(e, _)| *e).or(self.start)
    }

    /// Works out the candidates and the state from the log.
    fn update(&mut self) {
        let n = self.parents.len();
        // good: the events fine for sure (a set closed under parents)
        let mut good = vec![false; n];
        for &(e, a) in &self.log {
            if a == Answer::Good {
                self.mark(e, &mut good);
            }
        }
        let skipped: HashSet<EventId> = self.log.iter().filter(|(_, a)| *a == Answer::Skip).map(|(e, _)| *e).collect();
        let bad = self.bad();
        // a path to a good event goes only through good ones, so walking
        // back from the bad point without entering good ones finds them all
        self.cands = match bad {
            None => (0..n).filter(|&e| !good[e]).collect(),
            Some(b) => {
                let mut seen = vec![false; n];
                let mut stack = vec![b];
                seen[b] = true;
                while let Some(e) = stack.pop() {
                    for &p in &self.parents[e] {
                        if !good[p] && !seen[p] {
                            seen[p] = true;
                            stack.push(p);
                        }
                    }
                }
                (0..n).filter(|&e| seen[e] && !good[e]).collect()
            }
        };
        self.state = if self.cands.is_empty() {
            State::AllGood
        } else {
            match self.pick(bad, &skipped) {
                Some(e) => State::Ask(e),
                None if self.cands.len() == 1 && bad.is_some() => State::Found(self.cands[0]),
                None => State::Range(self.cands.clone()),
            }
        };
    }

    /// Marks E and everything before it.
    fn mark(&self, e: EventId, set: &mut [bool]) {
        let mut stack = vec![e];
        while let Some(e) = stack.pop() {
            if !set[e] {
                set[e] = true;
                stack.extend(self.parents[e].iter().copied().filter(|&p| !set[p]));
            }
        }
    }

    /// The candidate that halves the rest best (the earliest if tied): the
    /// one where the smaller of "it and its ancestors" and "the others" is
    /// the largest. Not a skipped one, and not the bad point.
    fn pick(&self, bad: Option<EventId>, skipped: &HashSet<EventId>) -> Option<EventId> {
        let m = self.cands.len();
        // where each candidate is in cands (events not there are good)
        let mut at = vec![usize::MAX; self.parents.len()];
        for (i, &e) in self.cands.iter().enumerate() {
            at[e] = i;
        }
        // the candidate parents of each candidate
        let ps: Vec<Vec<usize>> = self
            .cands
            .iter()
            .map(|&e| self.parents[e].iter().filter(|&&p| at[p] != usize::MAX).map(|&p| at[p]).collect())
            .collect();
        // how many are yet to use each row, to drop it when no one does
        let mut users = vec![0usize; m];
        for p in ps.iter().flatten() {
            users[*p] += 1;
        }
        // rows[i]: which candidates are i or before it. Only kept while
        // someone yet to come needs it, and only from its first word with a
        // bit, so a run with many entries doesn't need a square of memory.
        let mut rows: Vec<Option<Row>> = (0..m).map(|_| None).collect();
        let mut best: Option<(usize, EventId)> = None;
        for (i, &e) in self.cands.iter().enumerate() {
            let lo = ps[i].iter().filter_map(|&p| rows[p].as_ref()).map(|r| r.lo).min().unwrap_or(i / 64).min(i / 64);
            let mut row = Row { lo, words: vec![0; i / 64 - lo + 1] };
            row.set(i);
            for &p in &ps[i] {
                if let Some(r) = &rows[p] {
                    row.or(r);
                }
                users[p] -= 1;
                if users[p] == 0 {
                    rows[p] = None;
                }
            }
            let a = row.count();
            if users[i] > 0 {
                rows[i] = Some(row);
            }
            if skipped.contains(&e) || bad == Some(e) {
                continue;
            }
            let score = a.min(m - a);
            if best.is_none_or(|(s, _)| score > s) {
                best = Some((score, e));
            }
        }
        best.map(|(_, e)| e)
    }
}

/// A set of small numbers kept from its word LO on (the words before are empty).
struct Row {
    lo: usize,
    words: Vec<u64>,
}

impl Row {
    fn set(&mut self, i: usize) {
        self.words[i / 64 - self.lo] |= 1 << (i % 64);
    }

    fn or(&mut self, o: &Row) {
        let at = o.lo - self.lo;
        for (d, s) in self.words[at..].iter_mut().zip(&o.words) {
            *d |= s;
        }
    }

    fn count(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asks until it ends, answering with ORACLE; how many questions it took.
    fn run(b: &mut Bisect, oracle: impl Fn(EventId) -> Answer) -> usize {
        let mut asked = 0;
        while let State::Ask(e) = b.state() {
            b.answer(oracle(e));
            asked += 1;
            assert!(asked < 10_000, "doesn't end");
        }
        asked
    }

    fn line(n: usize) -> Vec<Vec<EventId>> {
        (0..n).map(|e| if e == 0 { vec![] } else { vec![e - 1] }).collect()
    }

    /// A tiny random generator, so the tests are always the same.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self, below: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % below as u64) as usize
        }
    }

    impl Bisect {
        /// Answers about E, whatever is asked (to set up a case).
        fn answer_at(&mut self, e: EventId, a: Answer) {
            self.log.push((e, a));
            self.update();
        }
    }

    #[test]
    fn line_finds_the_first_bad() {
        for k in [0, 1, 17, 63, 64, 99] {
            let mut b = Bisect::new(line(100), None);
            let asked = run(&mut b, |e| if e >= k { Answer::Bad } else { Answer::Good });
            assert_eq!(b.state(), State::Found(k), "k = {k}");
            assert!(asked <= 8, "{asked} questions for k = {k}");
        }
    }

    #[test]
    fn fan_out_eliminates_the_other_branch() {
        //   0 -> 1 -> 2 \
        //    \-> 3 -> 4 -> 5 (they meet)
        let parents = vec![vec![], vec![0], vec![1], vec![0], vec![3], vec![2, 4]];
        let mut b = Bisect::new(parents, None);
        assert_eq!(b.candidates(), vec![0, 1, 2, 3, 4, 5]);
        // the bug is event 2: bad after 2 and 5, good elsewhere
        run(&mut b, |e| if e == 2 || e == 5 { Answer::Bad } else { Answer::Good });
        assert_eq!(b.state(), State::Found(2));
        // the other branch is out
        assert!(b.candidates().iter().all(|e| ![3, 4, 5].contains(e)));
    }

    #[test]
    fn good_on_one_branch_says_nothing_about_the_other() {
        let parents = vec![vec![], vec![0], vec![0], vec![1, 2]];
        let mut b = Bisect::new(parents, Some(3));
        b.answer_at(1, Answer::Good);
        // 0 and 1 are fine, 2 and 3 stay
        assert_eq!(b.candidates(), vec![2, 3]);
        assert_eq!(b.state(), State::Ask(2));
        b.answer(Answer::Good);
        assert_eq!(b.state(), State::Found(3));
    }

    #[test]
    fn entries_that_never_meet() {
        let mut b = Bisect::new(vec![vec![], vec![], vec![], vec![]], None);
        assert_eq!(b.candidates(), vec![0, 1, 2, 3]);
        // each one is alone, so none halves better: the earliest
        assert_eq!(b.state(), State::Ask(0));
        b.answer(Answer::Good);
        b.answer(Answer::Good);
        assert_eq!(b.candidates(), vec![2, 3]);
        b.answer(Answer::Bad);
        assert_eq!(b.state(), State::Found(2));
    }

    #[test]
    fn skip_gives_a_range() {
        let mut b = Bisect::new(line(4), Some(3));
        b.answer(Answer::Skip);
        let skipped = b.log()[0].0;
        let State::Ask(e) = b.state() else { panic!("{:?}", b.state()) };
        assert_ne!(e, 3);
        assert_ne!(e, skipped);
        run(&mut b, |_| Answer::Skip);
        assert_eq!(b.state(), State::Range(vec![0, 1, 2, 3]));
        // a good answer before the skipped ones narrows it
        let mut b = Bisect::new(line(4), Some(3));
        b.answer_at(0, Answer::Good);
        b.answer_at(1, Answer::Skip);
        b.answer_at(2, Answer::Skip);
        assert_eq!(b.state(), State::Range(vec![1, 2, 3]));
    }

    #[test]
    fn undo() {
        let mut b = Bisect::new(line(8), None);
        assert!(!b.undo());
        let first = b.state();
        b.answer(Answer::Good);
        b.answer(Answer::Bad);
        assert_eq!(b.log().len(), 2);
        assert!(b.undo());
        assert!(b.undo());
        assert!(!b.undo());
        assert_eq!(b.state(), first);
        assert!(b.log().is_empty());
        assert_eq!(b.candidates().len(), 8);
    }

    #[test]
    fn all_good() {
        let mut b = Bisect::new(line(5), None);
        run(&mut b, |_| Answer::Good);
        assert_eq!(b.state(), State::AllGood);
        assert!(b.candidates().is_empty());
        // answering does nothing now
        b.answer(Answer::Bad);
        assert_eq!(b.state(), State::AllGood);
        assert!(b.undo());
        assert_ne!(b.state(), State::AllGood);
        assert_eq!(Bisect::new(vec![], None).state(), State::AllGood);
    }

    #[test]
    fn bad_in_the_middle() {
        let mut b = Bisect::new(line(10), Some(5));
        assert_eq!(b.candidates(), vec![0, 1, 2, 3, 4, 5]);
        run(&mut b, |e| if e >= 2 { Answer::Bad } else { Answer::Good });
        assert_eq!(b.state(), State::Found(2));
        // only the bad point
        assert_eq!(Bisect::new(vec![vec![]], Some(0)).state(), State::Found(0));
    }

    #[test]
    fn steps_left() {
        assert_eq!(Bisect::new(line(1), Some(0)).steps_left(), 0);
        assert_eq!(Bisect::new(line(2), Some(1)).steps_left(), 1);
        assert_eq!(Bisect::new(line(5), Some(4)).steps_left(), 3);
        assert_eq!(Bisect::new(line(8), None).steps_left(), 3);
    }

    #[test]
    fn random_dags_end_at_the_bug() {
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        for round in 0..300 {
            let n = 1 + rng.next(60);
            let parents: Vec<Vec<EventId>> =
                (0..n).map(|e| if e == 0 { vec![] } else { (0..rng.next(4)).map(|_| rng.next(e)).collect() }).collect();
            let bug = rng.next(n);
            // the event and what is before it, as the oracle sees it
            let mut anc: Vec<HashSet<EventId>> = Vec::new();
            for (e, ps) in parents.iter().enumerate() {
                let mut s: HashSet<EventId> = HashSet::from([e]);
                for &p in ps {
                    s.extend(anc[p].iter().copied());
                }
                anc.push(s);
            }
            let oracle = |e: EventId| if anc[e].contains(&bug) { Answer::Bad } else { Answer::Good };
            // from the end of the run, and from a bad event
            let bads: Vec<EventId> = (0..n).filter(|&e| anc[e].contains(&bug)).collect();
            let start = if round % 2 == 0 { None } else { Some(bads[rng.next(bads.len())]) };
            let mut b = Bisect::new(parents, start);
            run(&mut b, oracle);
            assert_eq!(b.state(), State::Found(bug), "round {round}");
            assert!(b.log().iter().all(|(_, a)| *a != Answer::Skip));
        }
    }

    #[test]
    fn big_runs_are_fast() {
        let mut b = Bisect::new(line(50_000), None);
        let t = std::time::Instant::now();
        let asked = run(&mut b, |e| if e >= 31_337 { Answer::Bad } else { Answer::Good });
        assert_eq!(b.state(), State::Found(31_337));
        assert!(asked <= 17);
        let per = t.elapsed() / asked as u32;
        // generous, for a slow CI machine
        assert!(per.as_millis() < 500, "{per:?} a question");
        // entries that never meet don't need a bit for every pair
        let b = Bisect::new(vec![vec![]; 50_000], None);
        assert_eq!(b.state(), State::Ask(0));
    }
}
