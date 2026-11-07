//! Vim mode, as a state machine: keys in, actions out. It knows nothing of
//! the canvas or of egui; the editor (`app/vimctl.rs`) turns keys into
//! [`Input`]s, feeds them to [`Vim::key`] and does what the [`Act`]s say.
//!
//! The modes are Normal, Visual, Pin (wiring from the keyboard) and a
//! command line (`:` and `/`). Typing in a field is Insert: that's egui's
//! text box, which has the keyboard until Esc or Enter, so it isn't here.

/// A key, as the machine sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    Char(char),
    Ctrl(char),
    Esc,
    Enter,
    Tab,
    BackTab,
    Backspace,
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Left,
    Down,
    Up,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Visual,
    /// Wiring: the editor keeps what stage it's in.
    Pin,
    /// The command line, after `:` or `/`.
    Line(char),
}

/// What the editor has to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Move(Dir, usize),
    /// To the square downstream (or upstream) along the wires.
    Follow(bool, usize),
    /// To the next (or previous) square wired from the same square.
    Sibling(i32),
    /// To the start (false) or the end (true) of the chain.
    Chain(bool),
    First,
    /// The last square, or the nth.
    Last(Option<usize>),
    /// The next (positive) or previous squares in order.
    Reading(i32),
    Search(String),
    SearchStep(i32, usize),
    /// Moves the selected squares by grid steps.
    Shove(Dir, usize),
    /// Types in a field: the nth, or the first.
    Edit(Option<usize>),
    AddFree,
    AddAfter,
    AddBefore,
    Delete,
    Yank,
    Paste(bool),
    Undo(usize),
    Redo(usize),
    VisualOn,
    VisualChain,
    VisualOff,
    /// Esc in Normal: the selection is the focused square again.
    Escape,
    PinStart,
    PinPick(usize),
    PinCycle(i32),
    PinAccept,
    PinRemove,
    PinCancel,
    OpenSubgraph,
    TabNext(usize),
    TabPrev(usize),
    TabGo(usize),
    Pan(Dir, usize),
    PanHalf(bool),
    Center,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    FitAll,
    Ex(String),
    /// Tab on the command line: the editor completes it.
    Complete,
}

pub struct Vim {
    pub mode: Mode,
    count: Option<usize>,
    pending: Option<char>,
    /// The text of the command line.
    pub line: String,
    history: Vec<String>,
    /// Where Up and Down are in the history (None: at the line being typed).
    recall: Option<usize>,
}

impl Default for Vim {
    fn default() -> Self {
        Vim::new()
    }
}

fn digit(c: char) -> Option<usize> {
    c.to_digit(10).map(|d| d as usize)
}

impl Vim {
    pub fn new() -> Vim {
        Vim { mode: Mode::Normal, count: None, pending: None, line: String::new(), history: Vec::new(), recall: None }
    }

    /// Back to Normal, with nothing half typed.
    pub fn reset(&mut self) {
        self.mode = Mode::Normal;
        self.count = None;
        self.pending = None;
        self.line.clear();
        self.recall = None;
    }

    /// The count and the half-typed command, as the bar shows them: `2d`.
    pub fn pending_text(&self) -> String {
        let mut s = self.count.map(|n| n.to_string()).unwrap_or_default();
        if let Some(p) = self.pending {
            s.push(p);
        }
        s
    }

    fn take_count(&mut self) -> Option<usize> {
        self.count.take()
    }

    pub fn key(&mut self, k: Input) -> Vec<Act> {
        match self.mode {
            Mode::Line(c) => self.line_key(c, k),
            Mode::Pin => self.pin_key(k),
            Mode::Normal | Mode::Visual => self.normal_key(k),
        }
    }

    /// The motions Normal, Visual and Pin share. None: not one.
    fn motion(&mut self, k: Input, n: usize) -> Option<Vec<Act>> {
        let dir = match k {
            Input::Char('h') | Input::Left => Some(Dir::Left),
            Input::Char('j') | Input::Down => Some(Dir::Down),
            Input::Char('k') | Input::Up => Some(Dir::Up),
            Input::Char('l') | Input::Right => Some(Dir::Right),
            _ => None,
        };
        if let Some(d) = dir {
            return Some(vec![Act::Move(d, n)]);
        }
        Some(match k {
            Input::Char('w') => vec![Act::Follow(true, n)],
            Input::Char('b') => vec![Act::Follow(false, n)],
            Input::Char(';') => vec![Act::Sibling(n as i32)],
            Input::Char(',') => vec![Act::Sibling(-(n as i32))],
            _ => return None,
        })
    }

    fn normal_key(&mut self, k: Input) -> Vec<Act> {
        let visual = self.mode == Mode::Visual;
        // the second key of `dd`, `gt`, `zz`...
        if let Some(p) = self.pending.take() {
            let n = self.take_count();
            let one = n.unwrap_or(1);
            return match (p, k) {
                ('d', Input::Char('d')) if !visual => vec![Act::Delete],
                ('y', Input::Char('y')) => self.after_operator(Act::Yank),
                ('g', Input::Char('g')) => vec![Act::First],
                ('g', Input::Char('t')) if !visual => match n {
                    Some(n) => vec![Act::TabGo(n)],
                    None => vec![Act::TabNext(1)],
                },
                ('g', Input::Char('T')) if !visual => vec![Act::TabPrev(one)],
                ('g', Input::Char('d')) if !visual => vec![Act::OpenSubgraph],
                ('z', Input::Char('h')) => vec![Act::Pan(Dir::Left, one)],
                ('z', Input::Char('l')) => vec![Act::Pan(Dir::Right, one)],
                ('z', Input::Char('j')) => vec![Act::Pan(Dir::Down, one)],
                ('z', Input::Char('k')) => vec![Act::Pan(Dir::Up, one)],
                ('z', Input::Char('z')) => vec![Act::Center],
                ('z', Input::Char('f')) => vec![Act::FitAll],
                _ => vec![],
            };
        }
        // a count: 0 is a count's digit only after another
        if let Input::Char(c) = k
            && let Some(d) = digit(c)
            && (d != 0 || self.count.is_some())
        {
            self.count = Some((self.count.unwrap_or(0) * 10 + d).min(9999));
            return vec![];
        }
        let n = self.count.unwrap_or(1);
        if matches!(k, Input::Esc | Input::Ctrl('[')) {
            self.count = None;
            if visual {
                self.mode = Mode::Normal;
                return vec![Act::VisualOff];
            }
            return vec![Act::Escape];
        }
        if let Some(acts) = self.motion(k, n) {
            self.count = None;
            return acts;
        }
        match k {
            Input::Char(c @ ('d' | 'y' | 'g' | 'z')) => {
                // in Visual, d and y act at once
                if visual && c == 'd' {
                    return self.after_operator(Act::Delete);
                }
                if visual && c == 'y' {
                    return self.after_operator(Act::Yank);
                }
                self.pending = Some(c);
                vec![]
            }
            Input::Char('x') => self.after_operator(Act::Delete),
            Input::Char('H') => self.done(vec![Act::Shove(Dir::Left, n)]),
            Input::Char('J') => self.done(vec![Act::Shove(Dir::Down, n)]),
            Input::Char('K') => self.done(vec![Act::Shove(Dir::Up, n)]),
            Input::Char('L') => self.done(vec![Act::Shove(Dir::Right, n)]),
            Input::Char('0') => self.done(vec![Act::Chain(false)]),
            Input::Char('$') => self.done(vec![Act::Chain(true)]),
            Input::Char('G') => {
                let c = self.take_count();
                vec![Act::Last(c)]
            }
            Input::Tab => self.done(vec![Act::Reading(n as i32)]),
            Input::BackTab => self.done(vec![Act::Reading(-(n as i32))]),
            Input::Char('n') => self.done(vec![Act::SearchStep(1, n)]),
            Input::Char('N') => self.done(vec![Act::SearchStep(-1, n)]),
            Input::Char('/') => self.line_start('/'),
            Input::Char(':') => self.line_start(':'),
            Input::Char('+') => self.done(vec![Act::ZoomIn]),
            Input::Char('-') => self.done(vec![Act::ZoomOut]),
            Input::Char('=') => self.done(vec![Act::ZoomReset]),
            Input::Ctrl('e') => self.done(vec![Act::Pan(Dir::Down, n)]),
            Input::Ctrl('y') => self.done(vec![Act::Pan(Dir::Up, n)]),
            Input::Ctrl('d') => self.done(vec![Act::PanHalf(true)]),
            Input::Ctrl('u') => self.done(vec![Act::PanHalf(false)]),
            Input::Char('v') => {
                self.count = None;
                if visual {
                    self.mode = Mode::Normal;
                    vec![Act::VisualOff]
                } else {
                    self.mode = Mode::Visual;
                    vec![Act::VisualOn]
                }
            }
            Input::Char('V') => {
                self.count = None;
                self.mode = Mode::Visual;
                vec![Act::VisualChain]
            }
            _ if visual => {
                self.count = None;
                vec![]
            }
            Input::Char('i') | Input::Enter => {
                let c = self.take_count();
                vec![Act::Edit(c)]
            }
            Input::Char('a') => self.done(vec![Act::AddFree]),
            Input::Char('o') => self.done(vec![Act::AddAfter]),
            Input::Char('O') => self.done(vec![Act::AddBefore]),
            Input::Char('p') => self.done(vec![Act::Paste(true)]),
            Input::Char('P') => self.done(vec![Act::Paste(false)]),
            Input::Char('u') => self.done(vec![Act::Undo(n)]),
            Input::Ctrl('r') => self.done(vec![Act::Redo(n)]),
            Input::Char('c') => {
                self.count = None;
                self.mode = Mode::Pin;
                vec![Act::PinStart]
            }
            _ => {
                self.count = None;
                vec![]
            }
        }
    }

    /// An operator that ended Visual (Visual: back to Normal).
    fn after_operator(&mut self, act: Act) -> Vec<Act> {
        self.count = None;
        if self.mode == Mode::Visual {
            self.mode = Mode::Normal;
            return vec![act, Act::VisualOff];
        }
        vec![act]
    }

    fn done(&mut self, acts: Vec<Act>) -> Vec<Act> {
        self.count = None;
        acts
    }

    fn line_start(&mut self, c: char) -> Vec<Act> {
        self.count = None;
        self.mode = Mode::Line(c);
        self.line.clear();
        self.recall = None;
        vec![]
    }

    fn pin_key(&mut self, k: Input) -> Vec<Act> {
        match k {
            Input::Esc | Input::Ctrl('[') => {
                self.mode = Mode::Normal;
                vec![Act::PinCancel]
            }
            Input::Char(c @ '1'..='9') => vec![Act::PinPick(digit(c).unwrap_or(1) - 1)],
            Input::Tab => vec![Act::PinCycle(1)],
            Input::BackTab => vec![Act::PinCycle(-1)],
            Input::Enter => vec![Act::PinAccept],
            Input::Char('x') => vec![Act::PinRemove],
            _ => self.motion(k, 1).unwrap_or_default(),
        }
    }

    fn line_key(&mut self, c: char, k: Input) -> Vec<Act> {
        match k {
            Input::Esc | Input::Ctrl('[') | Input::Ctrl('c') => {
                self.mode = Mode::Normal;
                self.line.clear();
                vec![]
            }
            Input::Enter => {
                let line = std::mem::take(&mut self.line);
                self.mode = Mode::Normal;
                if c == '/' {
                    return vec![if line.is_empty() { Act::SearchStep(1, 1) } else { Act::Search(line) }];
                }
                let line = line.trim().to_string();
                if line.is_empty() {
                    return vec![];
                }
                if self.history.last() != Some(&line) {
                    self.history.push(line.clone());
                }
                vec![Act::Ex(line)]
            }
            Input::Backspace => {
                if self.line.pop().is_none() {
                    self.mode = Mode::Normal;
                }
                vec![]
            }
            Input::Ctrl('u') => {
                self.line.clear();
                vec![]
            }
            Input::Ctrl('w') => {
                let t = self.line.trim_end().len();
                self.line.truncate(t);
                match self.line.rfind(' ') {
                    Some(i) => self.line.truncate(i + 1),
                    None => self.line.clear(),
                }
                vec![]
            }
            Input::Char(ch) => {
                self.line.push(ch);
                vec![]
            }
            Input::Tab if c == ':' => vec![Act::Complete],
            Input::Up | Input::Ctrl('p') if c == ':' => {
                let i = match self.recall {
                    None => self.history.len().checked_sub(1),
                    Some(i) => Some(i.saturating_sub(1)),
                };
                if let Some(i) = i {
                    self.recall = Some(i);
                    self.line = self.history[i].clone();
                }
                vec![]
            }
            Input::Down | Input::Ctrl('n') if c == ':' => {
                if let Some(i) = self.recall {
                    if i + 1 < self.history.len() {
                        self.recall = Some(i + 1);
                        self.line = self.history[i + 1].clone();
                    } else {
                        self.recall = None;
                        self.line.clear();
                    }
                }
                vec![]
            }
            _ => vec![],
        }
    }

    /// The keys of the Settings tab, where the rows are the settings.
    pub fn settings_key(&mut self, k: Input) -> Vec<SAct> {
        if let Mode::Line(_) = self.mode {
            // the command line works there too
            return self.line_key(':', k).into_iter().filter_map(SAct::from_act).collect();
        }
        if let Some(p) = self.pending.take() {
            return match (p, k) {
                ('g', Input::Char('g')) => vec![SAct::First],
                ('g', Input::Char('t')) => match self.take_count() {
                    Some(n) => vec![SAct::Ex(format!("tabnext {n}"))],
                    None => vec![SAct::Ex("tabnext".into())],
                },
                ('g', Input::Char('T')) => vec![SAct::Ex("tabprev".into())],
                (']', Input::Char(']')) => vec![SAct::Section(1)],
                ('[', Input::Char('[')) => vec![SAct::Section(-1)],
                _ => vec![],
            };
        }
        if let Input::Char(c) = k
            && let Some(d) = digit(c)
            && (d != 0 || self.count.is_some())
        {
            self.count = Some((self.count.unwrap_or(0) * 10 + d).min(9999));
            return vec![];
        }
        let n = self.count.unwrap_or(1);
        // a count waits for the second key of `3gt`
        if !matches!(k, Input::Char('g' | ']' | '[')) {
            self.count = None;
        }
        match k {
            Input::Char('j') | Input::Down | Input::Ctrl('n') | Input::Tab => vec![SAct::Rows(n as i32)],
            Input::Char('k') | Input::Up | Input::Ctrl('p') | Input::BackTab => vec![SAct::Rows(-(n as i32))],
            Input::Char('l') | Input::Right => vec![SAct::Change(n as i32)],
            Input::Char('h') | Input::Left => vec![SAct::Change(-(n as i32))],
            Input::Char(' ') => vec![SAct::Toggle],
            Input::Enter | Input::Char('i') => vec![SAct::Activate],
            Input::Char('r') => vec![SAct::Reset],
            Input::Char('/') => vec![SAct::Search],
            Input::Char('G') => vec![SAct::Last],
            Input::Char(c @ ('g' | ']' | '[')) => {
                self.pending = Some(c);
                vec![]
            }
            Input::Char(':') => {
                self.line_start(':');
                vec![]
            }
            _ => vec![],
        }
    }
}

/// What a key does in the Settings tab.
#[derive(Clone, Debug, PartialEq)]
pub enum SAct {
    Rows(i32),
    /// A number up or down by its step, a theme to the next.
    Change(i32),
    /// A checkbox flips.
    Toggle,
    /// A checkbox flips; a text takes the keyboard.
    Activate,
    Reset,
    Search,
    First,
    Last,
    Section(i32),
    Ex(String),
    Complete,
}

impl SAct {
    fn from_act(a: Act) -> Option<SAct> {
        match a {
            Act::Ex(l) => Some(SAct::Ex(l)),
            Act::Complete => Some(SAct::Complete),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------- Ex

/// A command typed after `:`.
#[derive(Clone, Debug, PartialEq)]
pub enum Ex {
    Write(Option<String>),
    Quit {
        force: bool,
    },
    QuitAll {
        force: bool,
    },
    WriteQuit,
    Edit(String),
    TabNew,
    TabNext(Option<usize>),
    TabPrev,
    TabClose {
        force: bool,
    },
    /// `:3`: the third square.
    Goto(usize),
    Run,
    Step,
    Stop,
    Compile,
    Output,
    ClearOutput,
    Add(String),
    Set(SetOp),
    Settings,
    Subgraph,
    Disconnect,
    Dup,
    OpenSubgraph,
    Undo,
    Redo,
    Help,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SetOp {
    Show(String),
    Assign(String, String),
    On(String),
    Toggle(String),
}

/// The commands, to complete and to list.
pub const COMMANDS: &[&str] = &[
    "add",
    "compile",
    "disconnect",
    "dup",
    "edit",
    "help",
    "output",
    "clear",
    "quit",
    "qall",
    "redo",
    "run",
    "set",
    "settings",
    "step",
    "stop",
    "subgraph",
    "tabclose",
    "tabnew",
    "tabnext",
    "tabprev",
    "open",
    "undo",
    "write",
    "wq",
];

pub fn parse_ex(line: &str) -> Result<Ex, String> {
    let line = line.trim().trim_start_matches(':').trim();
    let (head, rest) = match line.find(char::is_whitespace) {
        Some(i) => (&line[..i], line[i..].trim()),
        None => (line, ""),
    };
    if let Ok(n) = head.parse::<usize>() {
        return Ok(Ex::Goto(n));
    }
    let (name, force) = match head.strip_suffix('!') {
        Some(n) => (n, true),
        None => (head, false),
    };
    let no_args =
        |ex: Ex| -> Result<Ex, String> { if rest.is_empty() { Ok(ex) } else { Err(format!(":{name} takes nothing")) } };
    match name {
        "w" | "write" => Ok(Ex::Write((!rest.is_empty()).then(|| rest.to_string()))),
        "q" | "quit" | "close" => no_args(Ex::Quit { force }),
        "qa" | "qall" | "quitall" => no_args(Ex::QuitAll { force }),
        "wq" | "x" | "xit" => no_args(Ex::WriteQuit),
        "e" | "edit" if rest.is_empty() => Err(":edit needs a file".into()),
        "e" | "edit" => Ok(Ex::Edit(rest.to_string())),
        "tabnew" | "tabe" | "tabedit" | "new" => no_args(Ex::TabNew),
        "tabn" | "tabnext" => match rest {
            "" => Ok(Ex::TabNext(None)),
            n => n.parse().map(|n| Ex::TabNext(Some(n))).map_err(|_| format!("{n}: a tab number")),
        },
        "tabp" | "tabprev" | "tabprevious" | "tabN" => no_args(Ex::TabPrev),
        "tabc" | "tabclose" => no_args(Ex::TabClose { force }),
        "run" | "r" => no_args(Ex::Run),
        "step" => no_args(Ex::Step),
        "stop" => no_args(Ex::Stop),
        "compile" | "make" => no_args(Ex::Compile),
        "output" | "out" => no_args(Ex::Output),
        "clear" | "cl" => no_args(Ex::ClearOutput),
        "add" | "a" if rest.is_empty() => Err(":add needs a module (math/add)".into()),
        "add" | "a" => Ok(Ex::Add(rest.to_string())),
        "set" | "se" => parse_set(rest).map(Ex::Set),
        "settings" => no_args(Ex::Settings),
        "subgraph" | "sub" => no_args(Ex::Subgraph),
        "disconnect" | "disc" => no_args(Ex::Disconnect),
        "dup" | "duplicate" => no_args(Ex::Dup),
        "open" | "gd" => no_args(Ex::OpenSubgraph),
        "undo" => no_args(Ex::Undo),
        "redo" => no_args(Ex::Redo),
        "help" | "h" | "?" => no_args(Ex::Help),
        "" => Err("type a command (:help lists the keys)".into()),
        _ => Err(format!("not a command: {head}")),
    }
}

fn parse_set(arg: &str) -> Result<SetOp, String> {
    if arg.is_empty() {
        return Err(":set needs a setting: :set key=value, key, nokey, key! or key?".into());
    }
    if let Some((k, v)) = arg.split_once('=') {
        return Ok(SetOp::Assign(k.trim().to_string(), v.trim().to_string()));
    }
    if let Some(k) = arg.strip_suffix('!') {
        return Ok(SetOp::Toggle(k.to_string()));
    }
    if let Some(k) = arg.strip_suffix('?') {
        return Ok(SetOp::Show(k.to_string()));
    }
    // `no` before a name that is a setting's own isn't a negation: the
    // editor resolves that, since it knows the names
    Ok(SetOp::On(arg.to_string()))
}

/// What Tab does on the command line: the command, or the setting, that
/// LINE starts, if only one does or they share more. KEYS are the settings'.
pub fn complete_ex(line: &str, keys: &[&str]) -> Option<String> {
    let line = line.trim_start();
    let (head, rest) = match line.split_once(' ') {
        Some((h, r)) => (h, Some(r)),
        None => (line, None),
    };
    let common = |names: Vec<&str>, prefix: &str| -> Option<String> {
        let mut it = names.into_iter().filter(|n| n.starts_with(prefix));
        let first = it.next()?;
        let mut common = first.to_string();
        for n in it {
            while !n.starts_with(&common) {
                common.pop();
            }
        }
        Some(common)
    };
    match rest {
        None => common(COMMANDS.to_vec(), head),
        Some(r) if matches!(head, "set" | "se") => {
            let (neg, name) = match r.strip_prefix("no") {
                Some(n) if !keys.iter().any(|k| k.starts_with(r)) => ("no", n),
                _ => ("", r),
            };
            let mut names: Vec<&str> = keys.to_vec();
            // the short names too: `grid` for `canvas.grid`
            names.extend(keys.iter().filter_map(|k| k.split_once('.').map(|(_, s)| s)));
            common(names, name).map(|c| format!("{head} {neg}{c}"))
        }
        Some(_) => None,
    }
}

/// The keys, as `:help` lists them.
pub const HELP: &str = "\
NORMAL (the focused square has a ring; counts work: 3j)
  h j k l, arrows    the nearest square left, down, up, right
  w / b              downstream / upstream along the wires
  ; and ,            the next / previous square wired from the same one
  0 / $              start / end of the chain
  gg / G / 3G        first / last / third square (left to right)
  Tab / Shift+Tab    next / previous square in that order
  /text  n  N        find squares (module, field values, notes)
  H J K L            move the selected squares by a grid step
  i  Enter  3i       type in a field (the first, or the third)
  a                  add a square (module search), not wired
  o / O              add a square after / before this one, wired to it
  dd  x              delete          yy  copy         p / P  paste right / below
  u  Ctrl+R          undo / redo
  v / V              Visual: extend by moving / select the whole chain
  c                  Pin: wire from the keyboard (see below)
  gd                 open the subgraph this square is
  gt  gT  3gt        next / previous / third tab
  Ctrl+E/Y  Ctrl+D/U pan down / up; half a page down / up
  zh zl zj zk        pan sideways / vertically     zz  center on the focus
  + - =              zoom in / out / reset         zf  fit everything
  :                  command line        Esc  cancels, F5 F10 Shift+F5 run

VISUAL   moving extends the selection (back shrinks it); d x y H J K L; Esc

PIN      c, then 1-9 picks a pin of the square (Tab cycles, Enter takes the
         marked one); h j k l w b go to the other square; its pins are
         numbered the same way: the digit makes the wire.
         x removes the wires of the pin picked. Esc cancels.

SETTINGS TAB   j k move, Space/Enter flip, h l change numbers and themes,
         i types in a text, r resets, / searches, ]] [[ sections, gg G

COMMANDS (Tab completes)
  :w [file]  :q  :q!  :qa  :wq  :e file  :tabnew  :tabn [N]  :tabp  :tabc
  :run  :step  :stop  :compile  :output  :clear  :3 (the third square)
  :add module  :set key=value | key | nokey | key! | key?
  :settings  :subgraph  :open  :disconnect  :dup  :undo  :redo  :help
";

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(v: &mut Vim, s: &str) -> Vec<Act> {
        s.chars().flat_map(|c| v.key(Input::Char(c))).collect()
    }

    #[test]
    fn counts_and_motions() {
        let mut v = Vim::new();
        assert_eq!(keys(&mut v, "3j"), vec![Act::Move(Dir::Down, 3)]);
        assert_eq!(keys(&mut v, "12l"), vec![Act::Move(Dir::Right, 12)]);
        // 0 is a motion alone, a digit after another
        assert_eq!(keys(&mut v, "0"), vec![Act::Chain(false)]);
        assert_eq!(keys(&mut v, "10k"), vec![Act::Move(Dir::Up, 10)]);
        assert_eq!(v.key(Input::Down), vec![Act::Move(Dir::Down, 1)]);
        // the count is spent
        assert_eq!(keys(&mut v, "j"), vec![Act::Move(Dir::Down, 1)]);
    }

    #[test]
    fn operators_and_chords() {
        let mut v = Vim::new();
        assert_eq!(keys(&mut v, "dd"), vec![Act::Delete]);
        assert_eq!(keys(&mut v, "yy"), vec![Act::Yank]);
        assert_eq!(keys(&mut v, "x"), vec![Act::Delete]);
        assert_eq!(keys(&mut v, "gt"), vec![Act::TabNext(1)]);
        assert_eq!(keys(&mut v, "gT"), vec![Act::TabPrev(1)]);
        assert_eq!(keys(&mut v, "3gt"), vec![Act::TabGo(3)]);
        assert_eq!(keys(&mut v, "2gT"), vec![Act::TabPrev(2)]);
        assert_eq!(keys(&mut v, "gg"), vec![Act::First]);
        assert_eq!(keys(&mut v, "G"), vec![Act::Last(None)]);
        assert_eq!(keys(&mut v, "4G"), vec![Act::Last(Some(4))]);
        assert_eq!(keys(&mut v, "gd"), vec![Act::OpenSubgraph]);
        assert_eq!(keys(&mut v, "zz"), vec![Act::Center]);
        // a wrong second key drops the first
        assert_eq!(keys(&mut v, "dj"), vec![]);
        assert_eq!(keys(&mut v, "j"), vec![Act::Move(Dir::Down, 1)]);
        assert_eq!(keys(&mut v, "u"), vec![Act::Undo(1)]);
        assert_eq!(v.key(Input::Ctrl('r')), vec![Act::Redo(1)]);
        assert_eq!(keys(&mut v, "3u"), vec![Act::Undo(3)]);
    }

    #[test]
    fn pending_is_shown() {
        let mut v = Vim::new();
        keys(&mut v, "2d");
        assert_eq!(v.pending_text(), "2d");
        v.key(Input::Esc);
        assert_eq!(v.pending_text(), "");
    }

    #[test]
    fn escape_in_every_mode() {
        let mut v = Vim::new();
        // Normal: the first clears what's half typed, the next is the Esc
        keys(&mut v, "3d");
        assert_eq!(v.key(Input::Esc), vec![]);
        assert_eq!(v.pending_text(), "");
        assert_eq!(v.key(Input::Esc), vec![Act::Escape]);
        // Visual
        assert_eq!(keys(&mut v, "v"), vec![Act::VisualOn]);
        assert_eq!(v.mode, Mode::Visual);
        assert_eq!(v.key(Input::Esc), vec![Act::VisualOff]);
        assert_eq!(v.mode, Mode::Normal);
        // Pin
        assert_eq!(keys(&mut v, "c"), vec![Act::PinStart]);
        assert_eq!(v.mode, Mode::Pin);
        assert_eq!(v.key(Input::Esc), vec![Act::PinCancel]);
        assert_eq!(v.mode, Mode::Normal);
        // the command line
        keys(&mut v, ":wri");
        assert_eq!(v.mode, Mode::Line(':'));
        assert_eq!(v.key(Input::Esc), vec![]);
        assert_eq!(v.mode, Mode::Normal);
        assert_eq!(v.line, "");
    }

    #[test]
    fn visual_operators() {
        let mut v = Vim::new();
        keys(&mut v, "v");
        assert_eq!(keys(&mut v, "j"), vec![Act::Move(Dir::Down, 1)]);
        assert_eq!(keys(&mut v, "d"), vec![Act::Delete, Act::VisualOff]);
        assert_eq!(v.mode, Mode::Normal);
        keys(&mut v, "v");
        assert_eq!(keys(&mut v, "y"), vec![Act::Yank, Act::VisualOff]);
        keys(&mut v, "V");
        assert_eq!(v.mode, Mode::Visual);
        assert_eq!(keys(&mut v, "H"), vec![Act::Shove(Dir::Left, 1)]);
        assert_eq!(keys(&mut v, "v"), vec![Act::VisualOff]);
        // what Normal has alone is not there
        keys(&mut v, "v");
        assert_eq!(keys(&mut v, "i"), vec![]);
        assert_eq!(keys(&mut v, "o"), vec![]);
    }

    #[test]
    fn pin_sequence() {
        let mut v = Vim::new();
        assert_eq!(keys(&mut v, "c"), vec![Act::PinStart]);
        assert_eq!(keys(&mut v, "2"), vec![Act::PinPick(1)]);
        assert_eq!(keys(&mut v, "l"), vec![Act::Move(Dir::Right, 1)]);
        assert_eq!(keys(&mut v, "w"), vec![Act::Follow(true, 1)]);
        assert_eq!(v.key(Input::Tab), vec![Act::PinCycle(1)]);
        assert_eq!(v.key(Input::BackTab), vec![Act::PinCycle(-1)]);
        assert_eq!(keys(&mut v, "x"), vec![Act::PinRemove]);
        assert_eq!(v.key(Input::Enter), vec![Act::PinAccept]);
        // still Pin until the editor says it's over
        assert_eq!(v.mode, Mode::Pin);
        // no counts, no operators
        assert_eq!(keys(&mut v, "dd"), vec![]);
        v.key(Input::Esc);
        assert_eq!(v.mode, Mode::Normal);
    }

    #[test]
    fn command_line() {
        let mut v = Vim::new();
        keys(&mut v, ":w foo");
        assert_eq!(v.line, "w foo");
        assert_eq!(v.key(Input::Enter), vec![Act::Ex("w foo".into())]);
        assert_eq!(v.mode, Mode::Normal);
        // backspace, and out on an empty line
        keys(&mut v, ":ab");
        v.key(Input::Backspace);
        assert_eq!(v.line, "a");
        v.key(Input::Backspace);
        v.key(Input::Backspace);
        assert_eq!(v.mode, Mode::Normal);
        // history
        keys(&mut v, ":run");
        v.key(Input::Enter);
        keys(&mut v, ":");
        v.key(Input::Up);
        assert_eq!(v.line, "run");
        v.key(Input::Up);
        assert_eq!(v.line, "w foo");
        v.key(Input::Down);
        assert_eq!(v.line, "run");
        v.key(Input::Down);
        assert_eq!(v.line, "");
        v.key(Input::Esc);
        // Ctrl+W takes a word
        keys(&mut v, ":set grid");
        v.key(Input::Ctrl('w'));
        assert_eq!(v.line, "set ");
        v.key(Input::Esc);
        // Tab asks for completion
        keys(&mut v, ":ru");
        assert_eq!(v.key(Input::Tab), vec![Act::Complete]);
    }

    #[test]
    fn search_line() {
        let mut v = Vim::new();
        assert_eq!(keys(&mut v, "/add"), vec![]);
        assert_eq!(v.key(Input::Enter), vec![Act::Search("add".into())]);
        // Enter on nothing repeats
        keys(&mut v, "/");
        assert_eq!(v.key(Input::Enter), vec![Act::SearchStep(1, 1)]);
        assert_eq!(keys(&mut v, "2n"), vec![Act::SearchStep(1, 2)]);
        assert_eq!(keys(&mut v, "N"), vec![Act::SearchStep(-1, 1)]);
    }

    #[test]
    fn adding_and_editing() {
        let mut v = Vim::new();
        assert_eq!(keys(&mut v, "a"), vec![Act::AddFree]);
        assert_eq!(keys(&mut v, "o"), vec![Act::AddAfter]);
        assert_eq!(keys(&mut v, "O"), vec![Act::AddBefore]);
        assert_eq!(keys(&mut v, "i"), vec![Act::Edit(None)]);
        assert_eq!(keys(&mut v, "3i"), vec![Act::Edit(Some(3))]);
        assert_eq!(v.key(Input::Enter), vec![Act::Edit(None)]);
        assert_eq!(keys(&mut v, "p"), vec![Act::Paste(true)]);
        assert_eq!(keys(&mut v, "P"), vec![Act::Paste(false)]);
    }

    #[test]
    fn settings_keys() {
        let mut v = Vim::new();
        assert_eq!(v.settings_key(Input::Char('j')), vec![SAct::Rows(1)]);
        assert_eq!(v.settings_key(Input::Char('3')), vec![]);
        assert_eq!(v.settings_key(Input::Char('k')), vec![SAct::Rows(-3)]);
        assert_eq!(v.settings_key(Input::Char('l')), vec![SAct::Change(1)]);
        assert_eq!(v.settings_key(Input::Char(' ')), vec![SAct::Toggle]);
        assert_eq!(v.settings_key(Input::Char(']')), vec![]);
        assert_eq!(v.settings_key(Input::Char(']')), vec![SAct::Section(1)]);
        assert_eq!(v.settings_key(Input::Char('g')), vec![]);
        assert_eq!(v.settings_key(Input::Char('g')), vec![SAct::First]);
        // tabs are reached from there too
        v.settings_key(Input::Char('g'));
        assert_eq!(v.settings_key(Input::Char('t')), vec![SAct::Ex("tabnext".into())]);
        v.settings_key(Input::Char('2'));
        v.settings_key(Input::Char('g'));
        assert_eq!(v.settings_key(Input::Char('t')), vec![SAct::Ex("tabnext 2".into())]);
        v.settings_key(Input::Char('g'));
        assert_eq!(v.settings_key(Input::Char('T')), vec![SAct::Ex("tabprev".into())]);
        // the command line
        v.settings_key(Input::Char(':'));
        v.settings_key(Input::Char('q'));
        assert_eq!(v.settings_key(Input::Enter), vec![SAct::Ex("q".into())]);
    }

    #[test]
    fn ex_parsing() {
        assert_eq!(parse_ex("w"), Ok(Ex::Write(None)));
        assert_eq!(parse_ex("w a/b.lua"), Ok(Ex::Write(Some("a/b.lua".into()))));
        assert_eq!(parse_ex("q"), Ok(Ex::Quit { force: false }));
        assert_eq!(parse_ex("q!"), Ok(Ex::Quit { force: true }));
        assert_eq!(parse_ex("qa!"), Ok(Ex::QuitAll { force: true }));
        assert_eq!(parse_ex("wq"), Ok(Ex::WriteQuit));
        assert_eq!(parse_ex("e foo.lua"), Ok(Ex::Edit("foo.lua".into())));
        assert!(parse_ex("e").is_err());
        assert_eq!(parse_ex("tabnew"), Ok(Ex::TabNew));
        assert_eq!(parse_ex("tabn"), Ok(Ex::TabNext(None)));
        assert_eq!(parse_ex("tabn 2"), Ok(Ex::TabNext(Some(2))));
        assert_eq!(parse_ex("tabc!"), Ok(Ex::TabClose { force: true }));
        assert_eq!(parse_ex(":12"), Ok(Ex::Goto(12)));
        assert_eq!(parse_ex("add math/add"), Ok(Ex::Add("math/add".into())));
        assert!(parse_ex("add").is_err());
        assert_eq!(parse_ex("set grid"), Ok(Ex::Set(SetOp::On("grid".into()))));
        assert_eq!(parse_ex("set nogrid"), Ok(Ex::Set(SetOp::On("nogrid".into()))));
        assert_eq!(parse_ex("set grid!"), Ok(Ex::Set(SetOp::Toggle("grid".into()))));
        assert_eq!(parse_ex("set grid?"), Ok(Ex::Set(SetOp::Show("grid".into()))));
        assert_eq!(parse_ex("set theme=nord"), Ok(Ex::Set(SetOp::Assign("theme".into(), "nord".into()))));
        assert_eq!(
            parse_ex("set font.name = Iosevka NFM"),
            Ok(Ex::Set(SetOp::Assign("font.name".into(), "Iosevka NFM".into())))
        );
        assert!(parse_ex("set").is_err());
        assert!(parse_ex("run now").is_err());
        assert!(parse_ex("frobnicate").is_err());
        assert!(parse_ex("").is_err());
        for (c, e) in [("run", Ex::Run), ("step", Ex::Step), ("stop", Ex::Stop), ("compile", Ex::Compile)] {
            assert_eq!(parse_ex(c), Ok(e));
        }
        assert_eq!(parse_ex("help"), Ok(Ex::Help));
    }

    #[test]
    fn every_command_parses() {
        // a name in COMMANDS that isn't a command would complete to nothing
        for c in COMMANDS {
            let line = match *c {
                "add" => "add x".to_string(),
                "edit" => "edit x".to_string(),
                "set" => "set x".to_string(),
                c => c.to_string(),
            };
            assert!(parse_ex(&line).is_ok(), "{c}");
        }
    }

    #[test]
    fn completion() {
        let keys = ["theme", "canvas.grid", "canvas.grid_step", "canvas.snap"];
        assert_eq!(complete_ex("ru", &keys), Some("run".into()));
        // tabnew and tabnext share more than was typed
        assert_eq!(complete_ex("tabn", &keys), Some("tabne".into()));
        assert_eq!(complete_ex("set can", &keys), Some("set canvas.".into()));
        assert_eq!(complete_ex("set canvas.gr", &keys), Some("set canvas.grid".into()));
        assert_eq!(complete_ex("set nocanvas.sn", &keys), Some("set nocanvas.snap".into()));
        assert_eq!(complete_ex("set sn", &keys), Some("set snap".into()));
        assert_eq!(complete_ex("zzz", &keys), None);
    }
}
