//! Bisect in the editor: the run is recorded once, on a thread of its own
//! (see [`lineit_core::trace`]), and then the bar asks about points of it
//! until one run of one square is left (see [`lineit_core::bisect`]). It
//! belongs to the tab it was started in, and the trace is of the canvas as
//! it was: an edit ends it.

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;
use lineit_core::bisect::{Answer, Bisect, State};
use lineit_core::model::{Canvas, SquareId};
use lineit_core::trace::{self, Event, EventId, Trace};
use lineit_rt::Value;

use super::tab::Tab;

/// How many squares that ran a recording keeps (a loop that never ends, a clock).
pub const LIMIT: usize = trace::LIMIT;

type Recorded = Arc<Mutex<Option<Result<Trace, String>>>>;

/// A bisect of a tab: being recorded, then being asked.
pub struct Bisecting {
    /// The canvas the run is of: an edit to it ends the bisect.
    canvas: Canvas,
    /// The tab's `files_generation` then: a module file that changes ends it too.
    files_generation: u64,
    /// The generation of the tab the canvas was last compared at.
    checked: u64,
    phase: Phase,
}

enum Phase {
    /// The run is going on a thread. HERE: the bad point is the last run of
    /// this square, not the end of the run.
    Recording {
        stop: Arc<AtomicBool>,
        done: Recorded,
        here: Option<SquareId>,
    },
    Asking(Box<Asking>),
}

impl Drop for Bisecting {
    /// Ending it while the run is recorded stops the recording.
    fn drop(&mut self) {
        if let Phase::Recording { stop, .. } = &self.phase {
            stop.store(true, Ordering::Relaxed);
        }
    }
}

/// What the recording came to.
pub enum Outcome {
    /// It's the questions now. The run's error, if it ended with one, and
    /// how many squares it was cut at, if it was.
    Asking { error: Option<String>, cut: Option<usize> },
    /// There's nothing to ask about: why.
    Failed(String),
}

/// The questions of a recorded run.
pub struct Asking {
    pub trace: Trace,
    bisect: Bisect,
    // what follows is worked out from the bisect after each change
    state: State,
    /// The squares that have a candidate run.
    candidates: BTreeSet<SquareId>,
    /// What the run printed up to the point of the question.
    printed: Vec<String>,
}

/// What the canvas draws of a bisect.
pub struct Draw {
    /// The squares with the question's outline (all of them, in a range).
    pub outlined: BTreeSet<SquareId>,
    /// The squares that have a candidate run: the others are dimmed.
    pub candidates: BTreeSet<SquareId>,
    /// Written next to a square: what it and the canvas had after it ran.
    pub note: Option<(SquareId, Vec<String>)>,
}

impl Bisecting {
    /// Records the run of PROG, on a thread. HERE: the bad point is the last
    /// run of this square.
    fn start(
        prog: lineit_core::ir::Program,
        dir: PathBuf,
        canvas: Canvas,
        generation: u64,
        files_generation: u64,
        here: Option<SquareId>,
        ctx: &egui::Context,
    ) -> Bisecting {
        let stop = Arc::new(AtomicBool::new(false));
        let done: Recorded = Arc::default();
        let (s, d, c) = (stop.clone(), done.clone(), ctx.clone());
        std::thread::spawn(move || {
            let r = trace::record(prog, dir, LIMIT, s);
            *d.lock().unwrap() = Some(r);
            c.request_repaint();
        });
        Bisecting { canvas, files_generation, checked: generation, phase: Phase::Recording { stop, done, here } }
    }

    pub fn asking(&self) -> Option<&Asking> {
        match &self.phase {
            Phase::Asking(a) => Some(a),
            Phase::Recording { .. } => None,
        }
    }

    pub fn asking_mut(&mut self) -> Option<&mut Asking> {
        match &mut self.phase {
            Phase::Asking(a) => Some(a),
            Phase::Recording { .. } => None,
        }
    }

    /// Takes the recording if it's done.
    pub fn poll(&mut self) -> Option<Outcome> {
        let Phase::Recording { done, here, .. } = &self.phase else { return None };
        let r = done.lock().unwrap().take()?;
        let here = *here;
        let outcome = match r {
            Err(e) => Outcome::Failed(e),
            Ok(trace) => {
                let (error, cut) = (trace.error.clone(), trace.truncated.then_some(trace.events.len()));
                match Asking::new(trace, here) {
                    Ok(a) => {
                        self.phase = Phase::Asking(Box::new(a));
                        Outcome::Asking { error, cut }
                    }
                    Err(e) => Outcome::Failed(e),
                }
            }
        };
        Some(outcome)
    }
}

impl Asking {
    /// HERE: the bad point is the last run of this square (it must have run).
    fn new(trace: Trace, here: Option<SquareId>) -> Result<Asking, String> {
        if trace.events.is_empty() {
            return Err("no square ran: there's nothing to bisect".into());
        }
        let bad = match here {
            Some(s) => match trace.events.iter().rposition(|e| e.square == s) {
                Some(e) => Some(e),
                None => return Err(format!("square {s} didn't run: there's no point in the run to start from")),
            },
            None => None,
        };
        let bisect = Bisect::new(trace.parents(), bad);
        let mut a = Asking { trace, bisect, state: State::AllGood, candidates: BTreeSet::new(), printed: Vec::new() };
        a.refresh();
        Ok(a)
    }

    /// Works out what's shown from the bisect.
    fn refresh(&mut self) {
        self.state = self.bisect.state();
        self.candidates = self.bisect.candidates().into_iter().map(|e| self.trace.events[e].square).collect();
        self.printed = match &self.state {
            State::Ask(e) | State::Found(e) => self.printed_to(*e),
            _ => self.printed_to(self.trace.events.len() - 1),
        };
    }

    /// What the run printed up to the event E, it included.
    fn printed_to(&self, e: EventId) -> Vec<String> {
        self.trace.events[..=e].iter().flat_map(|e| e.printed.iter().cloned()).collect()
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn answer(&mut self, a: Answer) {
        self.bisect.answer(a);
        self.refresh();
    }

    /// Takes the last answer back; false if there was none.
    pub fn undo(&mut self) -> bool {
        let had = self.bisect.undo();
        self.refresh();
        had
    }

    pub fn can_undo(&self) -> bool {
        !self.bisect.log().is_empty()
    }

    /// The question is open: Good, Bad or Skip answer it.
    pub fn is_question(&self) -> bool {
        matches!(self.state, State::Ask(_))
    }

    /// The event the question (or the answer) is about.
    pub fn event(&self) -> Option<&Event> {
        match self.state {
            State::Ask(e) | State::Found(e) => Some(&self.trace.events[e]),
            _ => None,
        }
    }

    /// The square to show: the one of the question, or the bug; the first
    /// of a range.
    pub fn shown_square(&self) -> Option<SquareId> {
        match &self.state {
            State::Ask(_) | State::Found(_) => self.event().map(|e| e.square),
            State::Range(es) => es.first().map(|e| self.trace.events[*e].square),
            State::AllGood => None,
        }
    }

    /// What the run printed up to the point of the question (all of it, when
    /// there's none).
    pub fn printed(&self) -> &[String] {
        &self.printed
    }

    /// How many times the square of an event ran in all.
    fn runs_of(&self, e: &Event) -> usize {
        self.trace.events.iter().filter(|x| x.square == e.square).count()
    }

    /// "square 4 math/add, run 3 of 5", run 1-based.
    fn place(&self, e: &Event) -> String {
        format!("square {} {}, run {} of {}", e.square, e.module, e.run + 1, self.runs_of(e))
    }

    /// What the bar says.
    pub fn summary(&self) -> String {
        match &self.state {
            State::Ask(_) => {
                let e = self.event().expect("a question has an event");
                let more = match self.bisect.steps_left() {
                    0 => "the last one".to_string(),
                    n => format!("about {n} more"),
                };
                format!("Bisect: {} · {more} · are the values right after it?", self.place(e))
            }
            State::Found(_) => {
                let e = self.event().expect("an answer has an event");
                format!("the bug is in square {} ({}), run {}", e.square, e.module, e.run + 1)
            }
            State::Range(es) => {
                let squares: BTreeSet<SquareId> = es.iter().map(|e| self.trace.events[*e].square).collect();
                let list: Vec<String> = squares.iter().map(|s| s.to_string()).collect();
                format!(
                    "can't tell: the bug is in one of {} runs, in squares {} (Undo takes an answer back)",
                    es.len(),
                    list.join(", ")
                )
            }
            State::AllGood => "every answer was good: no square that ran is to blame (Undo takes one back)".into(),
        }
    }

    /// The values the event showed, by square and pin (with those of the
    /// squares it ran inside of: a loop's index), and the canvas's
    /// variables after it (as a run that paused there has them).
    pub fn values(&self) -> Option<(HashMap<(SquareId, String), Value>, Vec<(String, Value)>)> {
        let e = self.event()?;
        let inside = e.inside.iter().map(|(sq, pin, v)| ((*sq, pin.clone()), v.clone()));
        let values = inside.chain(e.values.iter().map(|(pin, v)| ((e.square, pin.clone()), v.clone()))).collect();
        Some((values, e.vars.clone()))
    }

    /// What the canvas draws.
    pub fn draw(&self) -> Draw {
        let outlined = match &self.state {
            State::Range(es) => es.iter().map(|e| self.trace.events[*e].square).collect(),
            _ => self.event().map(|e| e.square).into_iter().collect(),
        };
        let note = self.event().map(|e| {
            let list = |kv: &[(String, Value)]| -> String {
                kv.iter().map(|(k, v)| format!("{k} = {}", short(&v.text(), 40))).collect::<Vec<_>>().join(", ")
            };
            let mut lines = vec![format!("after run {}", e.run + 1)];
            if !e.values.is_empty() {
                lines.push(list(&e.values));
            }
            for (sq, pin, v) in &e.inside {
                lines.push(format!("in square {sq}: {pin} = {}", short(&v.text(), 40)));
            }
            if !e.vars.is_empty() {
                lines.push(format!("vars: {}", list(&e.vars)));
            }
            (e.square, lines)
        });
        Draw { outlined, candidates: self.candidates.clone(), note }
    }
}

fn short(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() <= max { s } else { format!("{}…", s.chars().take(max).collect::<String>()) }
}

impl Tab {
    /// Starts a bisect: records the run, then asks. HERE: the bad point is
    /// the last run of this square, not the end of the run. Says why it
    /// can't, if it can't.
    pub fn bisect_start(&mut self, here: Option<SquareId>, ctx: &egui::Context) -> Result<(), String> {
        if self.is_running() {
            return Err("stop the run first".into());
        }
        let Some(prog) = self.lowered.program.clone() else {
            let n = self.lowered.diags.len();
            return Err(format!("it can't run: {n} problem{} (in red)", if n == 1 { "" } else { "s" }));
        };
        self.status = None;
        self.bisect = Some(Bisecting::start(
            prog,
            self.dir(),
            self.canvas.clone(),
            self.generation,
            self.files_generation,
            here,
            ctx,
        ));
        Ok(())
    }

    /// Takes the recording if it's done: the error of the run is told like
    /// Run tells it; a recording with nothing to ask about ends the bisect.
    /// Whether it got to the questions just now.
    pub fn bisect_poll(&mut self) -> bool {
        let Some(outcome) = self.bisect.as_mut().and_then(Bisecting::poll) else { return false };
        match outcome {
            Outcome::Asking { error, cut } => {
                self.status = match (error, cut) {
                    (Some(e), _) => {
                        self.output.push(format!("lineit: {e}"));
                        Some((e, true))
                    }
                    (None, Some(n)) => {
                        Some((format!("the run went on: only its first {n} squares are recorded"), false))
                    }
                    (None, None) => None,
                };
                true
            }
            Outcome::Failed(e) => {
                self.bisect = None;
                self.output.push(format!("lineit: {e}"));
                self.status = Some((e, true));
                false
            }
        }
    }

    /// An edit to the canvas since the recording (moving squares isn't one,
    /// as in a run) ends the bisect, and the bar says why.
    pub fn bisect_check_edit(&mut self) {
        let Some(b) = &mut self.bisect else { return };
        if b.checked == self.generation {
            return;
        }
        b.checked = self.generation;
        let edited =
            self.files_generation > b.files_generation || !lineit_core::diff::diff(&b.canvas, &self.canvas).is_empty();
        if edited {
            self.bisect = None;
            self.status = Some(("bisect ended: the canvas changed, and the run was recorded before".into(), false));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineit_core::registry::Registry;
    use std::sync::Arc;

    /// An entry, three prints in a row, and a last one: squares 2, 3, 4, 5.
    fn chain() -> Canvas {
        let src = r#"return { lineit = 1, squares = {
            { id = 1, module = "flow/entry" },
            { id = 2, module = "io/print", fields = { value = "a" } },
            { id = 3, module = "io/print", fields = { value = "b" } },
            { id = 4, module = "io/print", fields = { value = "c" } },
            { id = 5, module = "io/print", fields = { value = "d" } },
            }, links = { { 1, "then", 2, "tick" }, { 2, "then", 3, "tick" },
            { 3, "then", 4, "tick" }, { 4, "then", 5, "tick" } } }"#;
        lineit_core::file::read_canvas(src, "chain").unwrap()
    }

    fn tab(c: Canvas) -> Tab {
        Tab::new(c, None, &Arc::new(Registry::builtin()))
    }

    /// Starts a bisect of the tab and waits for the questions.
    fn started(t: &mut Tab, here: Option<SquareId>) {
        t.bisect_start(here, &egui::Context::default()).unwrap();
        for _ in 0..200 {
            t.bisect_poll();
            if t.bisect.as_ref().is_some_and(|b| b.asking().is_some()) {
                return;
            }
            assert!(t.bisect.is_some(), "the bisect ended: {:?}", t.status);
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        panic!("it didn't record");
    }

    fn asking(t: &mut Tab) -> &mut Asking {
        match &mut t.bisect.as_mut().unwrap().phase {
            Phase::Asking(a) => a,
            Phase::Recording { .. } => panic!("still recording"),
        }
    }

    fn question(t: &mut Tab) -> SquareId {
        let a = asking(t);
        assert!(a.is_question());
        a.event().unwrap().square
    }

    #[test]
    fn it_asks_about_a_square_that_ran() {
        let mut t = tab(chain());
        started(&mut t, None);
        let a = asking(&mut t);
        // the entry, then 2 3 4 5: of five in a row, the earliest of the middle ones is asked
        assert_eq!(a.trace.events.len(), 5);
        assert_eq!(a.event().unwrap().square, 2);
        assert!(a.summary().contains("square 2 io/print, run 1 of 1"), "{}", a.summary());
        // what it printed up to there, and the squares that can still be it
        assert_eq!(a.printed(), ["a"]);
        assert_eq!(a.draw().outlined, BTreeSet::from([2]));
        assert_eq!(a.draw().candidates, BTreeSet::from([1, 2, 3, 4, 5]));
        assert_eq!(a.draw().note.unwrap().0, 2);
    }

    #[test]
    fn answers_move_the_question_and_end_in_the_bug() {
        let mut t = tab(chain());
        started(&mut t, None);
        // 2 and 3 are fine: the bug is after them
        asking(&mut t).answer(Answer::Good);
        assert_eq!(question(&mut t), 3);
        asking(&mut t).answer(Answer::Good);
        assert_eq!(question(&mut t), 4);
        assert_eq!(asking(&mut t).printed(), ["a", "b", "c"]);
        assert_eq!(asking(&mut t).draw().candidates, BTreeSet::from([4, 5]));
        // 4 is wrong: it is the bug
        asking(&mut t).answer(Answer::Bad);
        let a = asking(&mut t);
        assert_eq!(a.state(), &State::Found(3));
        assert_eq!(a.shown_square(), Some(4));
        assert!(a.summary().starts_with("the bug is in square 4 (io/print), run 1"), "{}", a.summary());
        assert!(!a.is_question());
        // it keeps what it showed, and the outline
        assert!(a.values().is_some());
        assert_eq!(a.draw().outlined, BTreeSet::from([4]));
    }

    #[test]
    fn undo_goes_back_a_question() {
        let mut t = tab(chain());
        started(&mut t, None);
        asking(&mut t).answer(Answer::Bad);
        assert_ne!(question(&mut t), 2);
        assert!(asking(&mut t).can_undo());
        assert!(asking(&mut t).undo());
        assert_eq!(question(&mut t), 2);
        assert!(!asking(&mut t).can_undo());
        assert!(!asking(&mut t).undo());
    }

    #[test]
    fn everything_good_says_so() {
        let mut t = tab(chain());
        started(&mut t, None);
        // 2, 3, 4 and then the last one
        for _ in 0..4 {
            asking(&mut t).answer(Answer::Good);
        }
        let a = asking(&mut t);
        assert_eq!(a.state(), &State::AllGood);
        assert!(a.summary().starts_with("every answer was good"));
        assert!(a.undo());
        assert!(a.is_question());
    }

    #[test]
    fn up_to_here_starts_at_the_last_run_of_the_square() {
        let mut t = tab(chain());
        started(&mut t, Some(4));
        // events 0..=3 are candidates: 4 is bad and isn't asked
        let a = asking(&mut t);
        assert_eq!(a.draw().candidates, BTreeSet::from([1, 2, 3, 4]));
        assert_eq!(a.event().unwrap().square, 2);
        // 2 and 3 good: 4 itself is the bug
        a.answer(Answer::Good);
        assert_eq!(a.event().unwrap().square, 3);
        a.answer(Answer::Good);
        assert_eq!(a.state(), &State::Found(3));
        assert_eq!(a.shown_square(), Some(4));
    }

    #[test]
    fn a_square_that_didnt_run_says_so() {
        let mut c = chain();
        c.add(lineit_core::model::Square::new(9, "io/print", 0.0, 0.0));
        let mut t = tab(c);
        t.bisect_start(Some(9), &egui::Context::default()).unwrap();
        for _ in 0..200 {
            t.bisect_poll();
            if t.bisect.is_none() {
                let (msg, err) = t.status.clone().unwrap();
                assert!(err && msg.contains("square 9 didn't run"), "{msg}");
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        panic!("it didn't end");
    }

    #[test]
    fn an_edit_ends_it_and_moving_a_square_doesnt() {
        let reg = Arc::new(Registry::builtin());
        let mut t = tab(chain());
        started(&mut t, None);
        t.canvas.squares[2].x += 40.0;
        t.update(&reg, false, false);
        t.bisect_check_edit();
        assert!(t.bisect.is_some());
        t.canvas.squares[2].fields.insert("value".into(), "z".into());
        t.update(&reg, false, false);
        t.bisect_check_edit();
        assert!(t.bisect.is_none());
        assert!(t.status.as_ref().is_some_and(|(m, _)| m.starts_with("bisect ended")));
    }

    #[test]
    fn a_canvas_with_problems_gives_the_error_run_gives() {
        let mut c = chain();
        c.add(lineit_core::model::Square::new(9, "no/such", 0.0, 0.0));
        let mut t = tab(c);
        let e = t.bisect_start(None, &egui::Context::default()).unwrap_err();
        assert!(e.starts_with("it can't run: "), "{e}");
        assert!(t.bisect.is_none());
    }

    #[test]
    fn a_run_that_ends_with_an_error_is_told() {
        let src = r#"return { lineit = 1, squares = {
            { id = 1, module = "flow/entry" },
            { id = 2, module = "io/print", fields = { value = "before" } },
            { id = 3, module = "code/function", params = { code = "function boom()\n  error('no')\nend\n" } },
            { id = 4, module = "io/print", fields = { value = "after" } },
        }, links = { { 1, "then", 2, "tick" }, { 2, "then", 3, "tick" }, { 3, "then", 4, "tick" } } }"#;
        let mut t = tab(lineit_core::file::read_canvas(src, "err").unwrap());
        started(&mut t, None);
        let (msg, err) = t.status.clone().expect("the error of the run");
        assert!(err && msg.contains("no"), "{msg}");
        assert!(t.output.iter().any(|l| l.starts_with("lineit: ") && l.contains("no")));
        // the square that failed is the last of the run
        assert_eq!(asking(&mut t).trace.events.last().unwrap().square, 3);
    }

    #[test]
    fn a_branch_that_runs_in_parallel_isnt_before_or_after() {
        // an entry fans out to two prints: good on one says nothing of the other
        let src = r#"return { lineit = 1, squares = {
            { id = 1, module = "flow/entry" },
            { id = 2, module = "io/print", fields = { value = "a" } },
            { id = 3, module = "io/print", fields = { value = "b" } },
            }, links = { { 1, "then", 2, "tick" }, { 1, "then", 3, "tick" } } }"#;
        let mut t = tab(lineit_core::file::read_canvas(src, "fan").unwrap());
        started(&mut t, None);
        let first = question(&mut t);
        asking(&mut t).answer(Answer::Good);
        // the other branch is still a candidate
        let a = asking(&mut t);
        assert!(a.draw().candidates.iter().any(|s| *s != first && *s != 1), "{:?}", a.draw().candidates);
    }

    /// The canvas draws under a bisect, in every theme, with an outline that
    /// is none of the other outlines' color.
    #[test]
    fn the_canvas_draws_with_a_bisect() {
        use super::super::canvas::{self, CanvasState, Ctx};
        use super::super::theme::Theme;
        let mut t = tab(chain());
        started(&mut t, None);
        asking(&mut t).answer(Answer::Good);
        let reg = Registry::builtin();
        for name in ["dark", "light", "nord", "gruvbox"] {
            let mut cfg = lineit_core::config::Config::defaults();
            cfg.set("theme", lineit_core::config::SValue::Text(name.into()));
            let theme = Theme::from_config(&cfg);
            for other in [theme.step, theme.selection, theme.error, theme.border] {
                assert_ne!(theme.bisect, other, "{name}");
            }
            let mut state = CanvasState::default();
            let ctx = egui::Context::default();
            for _ in 0..3 {
                let a = t.bisect.as_ref().and_then(|b| b.asking()).unwrap();
                let (values, draw) = (a.values().map(|v| v.0).unwrap_or_default(), a.draw());
                ctx.run_ui(egui::RawInput::default(), |ui| {
                    let mut cx = Ctx {
                        tab: &mut t,
                        tab_id: 1,
                        reg: &reg,
                        theme: &theme,
                        cfg: &cfg,
                        state: &mut state,
                        actions: Vec::new(),
                        values: values.clone(),
                        current: BTreeSet::new(),
                        states: HashMap::new(),
                        checks: HashMap::new(),
                        vars: Vec::new(),
                        pending: None,
                        bisect: Some(Draw {
                            outlined: draw.outlined.clone(),
                            candidates: draw.candidates.clone(),
                            note: draw.note.clone(),
                        }),
                    };
                    canvas::show(ui, &mut cx);
                })
                .drop_without_applying_deltas();
            }
            assert!(state.rects.contains_key(&3), "{name}: the squares were drawn");
        }
    }
}
