//! `lineit bisect FILE`: records a run, then asks about points of it ("are the
//! values right after this square ran?") until one run of one square is left
//! (see `lineit_core::bisect`).

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use lineit_core::bisect::{Answer, Bisect, State};
use lineit_core::ir::Program;
use lineit_core::trace::{self, Event, EventId, Trace};
use lineit_rt::Value;

/// How many events a trace records unless `--limit` says.
pub const DEFAULT_LIMIT: usize = lineit_core::trace::LIMIT;

/// A value is cut to this many characters.
const VALUE_WIDTH: usize = 60;

/// Candidates listed when it ends in a range.
const RANGE_SHOWN: usize = 20;

pub struct Options {
    pub limit: usize,
    /// `--bad` as typed: `SQUARE` or `SQUARE:RUN`.
    pub bad: Option<String>,
    /// `--answers` as typed: g, b and s (and u) to start with.
    pub answers: String,
    /// The canvas as typed, for the line that replays the answers.
    pub file: String,
}

/// Records PROG and asks the questions on INPUT, writing to OUT.
pub fn run(
    prog: Program,
    dir: PathBuf,
    opts: &Options,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<(), String> {
    let trace = trace::record(prog, dir, opts.limit, Default::default())?;
    let bad = match &opts.bad {
        Some(spec) => Some(bad_event(&trace, spec)?),
        None => None,
    };
    let mut b = Bisect::new(trace.parents(), bad);
    // the answers given to start with
    for (i, c) in opts.answers.chars().filter(|c| !c.is_whitespace()).enumerate() {
        let a = match c.to_ascii_lowercase() {
            'g' => Some(Answer::Good),
            'b' => Some(Answer::Bad),
            's' => Some(Answer::Skip),
            'u' => None,
            _ => return Err(format!("--answers: `{c}` isn't g, b, s or u (good, bad, skip, undo)")),
        };
        let ok = match a {
            Some(a) if matches!(b.state(), State::Ask(_)) => {
                b.answer(a);
                true
            }
            Some(_) => false,
            None => b.undo(),
        };
        if !ok {
            let why = if a.is_some() { "it was done already" } else { "nothing to undo" };
            return Err(format!("--answers: answer {} (`{c}`): {why}", i + 1));
        }
    }
    session(&trace, &mut b, opts, input, out).map_err(|e| e.to_string())
}

/// The event `--bad SQUARE[:RUN]` means: run RUN (from 1) of the square, or its last.
fn bad_event(trace: &Trace, spec: &str) -> Result<EventId, String> {
    let (sq, run) = match spec.split_once(':') {
        Some((s, r)) => (s, Some(r)),
        None => (spec, None),
    };
    let id: u64 = sq.parse().map_err(|_| format!("--bad {spec}: want a square id, or SQUARE:RUN"))?;
    let runs: Vec<EventId> = (0..trace.events.len()).filter(|&e| trace.events[e].square == id).collect();
    if runs.is_empty() {
        let cut = if trace.truncated { " in the part recorded (see --limit)" } else { "" };
        return Err(format!("--bad {spec}: square {id} didn't run{cut}"));
    }
    let Some(run) = run else { return Ok(*runs.last().unwrap()) };
    let n: usize = run.parse().map_err(|_| format!("--bad {spec}: `{run}` isn't a run number"))?;
    match n.checked_sub(1).and_then(|n| runs.get(n)) {
        Some(e) => Ok(*e),
        None => Err(format!("--bad {spec}: square {id} ran {} time{} (runs 1 to {0})", runs.len(), plural(runs.len()))),
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// `square 4 (math/add), run 3 of 5`
fn what(trace: &Trace, e: EventId) -> String {
    let ev = &trace.events[e];
    let total = trace.events.iter().filter(|x| x.square == ev.square).count();
    format!("square {} ({}), run {} of {total}", ev.square, ev.module, ev.run + 1)
}

/// A value on one line, cut if long.
fn value(v: &Value) -> String {
    let t = v.text().replace('\r', "").replace('\n', "\\n");
    if t.chars().count() > VALUE_WIDTH { format!("{}...", t.chars().take(VALUE_WIDTH).collect::<String>()) } else { t }
}

fn pairs(list: &[&(String, Value)]) -> String {
    list.iter().map(|(n, v)| format!("{n} = {}", value(v))).collect::<Vec<_>>().join(", ")
}

/// What an event shows: its values, the variables that changed since the
/// event before it (all, for the first), what it printed.
fn details(trace: &Trace, e: EventId, out: &mut impl Write) -> io::Result<()> {
    let ev: &Event = &trace.events[e];
    if !ev.values.is_empty() {
        writeln!(out, "  values: {}", pairs(&ev.values.iter().collect::<Vec<_>>()))?;
    }
    for (sq, pin, v) in &ev.inside {
        let module = trace.events.iter().find(|e| e.square == *sq).map_or("", |e| e.module.as_str());
        writeln!(out, "  inside square {sq} ({module}): {pin} = {}", value(v))?;
    }
    let before = ev.parents.last().map(|&p| &trace.events[p].vars);
    let changed: Vec<_> = ev
        .vars
        .iter()
        .filter(|(n, v)| before.is_none_or(|b| b.iter().find(|(m, _)| m == n).is_none_or(|(_, w)| w != v)))
        .collect();
    if !ev.vars.is_empty() {
        match (changed.is_empty(), before.is_some()) {
            (true, _) => writeln!(out, "  variables (none changed): {}", pairs(&ev.vars.iter().collect::<Vec<_>>()))?,
            (false, true) => writeln!(out, "  variables changed: {}", pairs(&changed))?,
            (false, false) => writeln!(out, "  variables: {}", pairs(&changed))?,
        }
    }
    for line in &ev.printed {
        writeln!(out, "  printed: {line}")?;
    }
    Ok(())
}

/// A word in the shell: quoted if it needs it.
fn quoted(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || "-_./:=+,@%".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// The command that gets to where the answers are.
fn replay(b: &Bisect, opts: &Options) -> String {
    let mut s = format!("lineit bisect {}", quoted(&opts.file));
    if opts.limit != DEFAULT_LIMIT {
        s.push_str(&format!(" --limit {}", opts.limit));
    }
    if let Some(bad) = &opts.bad {
        s.push_str(&format!(" --bad {}", quoted(bad)));
    }
    if !b.log().is_empty() {
        let letters: String = b
            .log()
            .iter()
            .map(|(_, a)| match a {
                Answer::Good => 'g',
                Answer::Bad => 'b',
                Answer::Skip => 's',
            })
            .collect();
        s.push_str(&format!(" --answers {letters}"));
    }
    s
}

fn session(
    trace: &Trace,
    b: &mut Bisect,
    opts: &Options,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> io::Result<()> {
    let printed: Vec<&String> = trace.events.iter().flat_map(|e| &e.printed).collect();
    if printed.is_empty() {
        writeln!(out, "the run printed nothing")?;
    } else {
        writeln!(out, "the run printed:")?;
        for l in printed {
            writeln!(out, "  {l}")?;
        }
    }
    if let Some(e) = &trace.error {
        writeln!(out, "the run ended with an error: {e}")?;
    }
    writeln!(out, "{} square run{} recorded", trace.events.len(), plural(trace.events.len()))?;
    if trace.truncated {
        writeln!(
            out,
            "the run was cut at {} events: what came later isn't in it (--limit N records more)",
            opts.limit
        )?;
    }
    if trace.events.is_empty() {
        return writeln!(out, "no square ran, so there's nothing to bisect");
    }
    loop {
        match b.state() {
            State::Ask(e) => {
                writeln!(out)?;
                writeln!(out, "{}; about {} question{} left", what(trace, e), b.steps_left(), plural(b.steps_left()))?;
                details(trace, e, out)?;
                // asked again on input that isn't an answer
                loop {
                    write!(out, "good, bad, skip, undo or quit? [g/b/s/u/q] ")?;
                    out.flush()?;
                    let mut line = String::new();
                    if input.read_line(&mut line)? == 0 {
                        writeln!(out)?;
                        return quit(b, opts, out);
                    }
                    match line.trim().to_lowercase().as_str() {
                        "g" | "good" => b.answer(Answer::Good),
                        "b" | "bad" => b.answer(Answer::Bad),
                        "s" | "skip" => b.answer(Answer::Skip),
                        "u" | "undo" => {
                            if !b.undo() {
                                writeln!(out, "nothing to undo")?;
                                continue;
                            }
                        }
                        "q" | "quit" => return quit(b, opts, out),
                        other => {
                            writeln!(out, "`{other}`: not an answer")?;
                            continue;
                        }
                    }
                    break;
                }
            }
            State::Found(e) => {
                writeln!(out)?;
                writeln!(out, "the bug is in {}", what(trace, e))?;
                details(trace, e, out)?;
                break;
            }
            State::Range(es) => {
                writeln!(out)?;
                writeln!(out, "can't tell: the bug is in one of these {} (some were skipped)", es.len())?;
                for &e in es.iter().take(RANGE_SHOWN) {
                    writeln!(out, "  {}", what(trace, e))?;
                }
                if es.len() > RANGE_SHOWN {
                    writeln!(out, "  ... and {} more", es.len() - RANGE_SHOWN)?;
                }
                break;
            }
            State::AllGood => {
                writeln!(out)?;
                writeln!(
                    out,
                    "every point was good: no square that ran is to blame. The bug may be in what was expected, or in something that didn't run"
                )?;
                break;
            }
        }
    }
    writeln!(out, "to replay: {}", replay(b, opts))
}

/// Stops with the answers so far said, to go on from there.
fn quit(b: &Bisect, opts: &Options, out: &mut impl Write) -> io::Result<()> {
    if b.log().is_empty() {
        return writeln!(out, "stopped");
    }
    writeln!(out, "stopped; to go on: {}", replay(b, opts))
}
