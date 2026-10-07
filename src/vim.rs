use crate::config::config;
use arboard::Clipboard;
use ratatui::{
    DefaultTerminal,
    style::{Color, Modifier, Style},
    widgets::Block,
};
use ratatui_textarea::{CursorMove, Input, Key, Scrolling, TextArea};
use std::{fmt, io, io::Write};

// Bound numeric prefixes and the product of operator and motion counts.
const MAX_COUNT: usize = u16::MAX as usize;
type Position = (usize, usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Replace(bool), // true = replace once (r), false = overtype (R)
    Operator(char),
}

impl Mode {
    pub fn block<'a>(&self) -> Block<'a> {
        Block::default()
    }

    pub fn cursor_style(&self) -> Style {
        let color = match self {
            Self::Normal => Color::Reset,
            Self::Insert => Color::DarkGray,
            Self::Replace(_) => Color::Reset,
            Self::Operator(_) => Color::Reset,
        };
        Style::default().fg(color).add_modifier(Modifier::REVERSED)
    }
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Normal => write!(f, "NORMAL"),
            Self::Insert => write!(f, "INSERT"),
            Self::Replace(_) => write!(f, "REPLACE"),
            Self::Operator(c) => write!(f, "OPERATOR({})", c),
        }
    }
}

pub enum Transition {
    Nop,
    Mode(Mode),
    Pending(Input),
    EditExternally,
    Quit,
}

pub struct Vim {
    pub mode: Mode,
    pub pending: Input,
    count: Option<usize>,
    operator_count: Option<usize>,
    operator_start: Option<Position>,
    replace_count: usize,
    insert_repeat: usize,
    insert_keys: Vec<Input>,
    insert_newline: bool,
    clipboard: Option<Clipboard>,
    pub clipboard_error: Option<String>,
}

impl Vim {
    pub fn new(mode: Mode) -> Self {
        let (clipboard, clipboard_error) = match Clipboard::new() {
            Ok(clipboard) => (Some(clipboard), None),
            Err(err) => (None, Some(err.to_string())),
        };
        Self {
            mode,
            pending: Input::default(),
            count: None,
            operator_count: None,
            operator_start: None,
            replace_count: 1,
            insert_repeat: 1,
            insert_keys: Vec::new(),
            insert_newline: false,
            clipboard,
            clipboard_error,
        }
    }

    fn copy_to_clipboard(&mut self, textarea: &TextArea<'_>) {
        let Some(clipboard) = self.clipboard.as_mut() else {
            return;
        };

        self.clipboard_error = clipboard
            .set_text(textarea.yank_text().to_owned())
            .err()
            .map(|err| err.to_string());
    }

    fn load_clipboard(&mut self, textarea: &mut TextArea<'_>) {
        let Some(clipboard) = self.clipboard.as_mut() else {
            return;
        };

        match clipboard.get_text() {
            Ok(text) => {
                textarea.set_yank_text(text);
                self.clipboard_error = None;
            }
            Err(err) => {
                // Leave the internal yank buffer available as a fallback.
                self.clipboard_error = Some(err.to_string());
            }
        }
    }

    pub fn with_pending(mut self, pending: Input) -> Self {
        self.pending = pending;
        self
    }

    // Use this instead of Vim::new(mode) when applying Transition::Mode.
    // Counts, replacement state, and insert repetition must survive mode changes.
    pub fn with_mode(mut self, mode: Mode) -> Self {
        self.mode = mode;
        self
    }

    //pub fn is_before_line_end(textarea: &TextArea<'_>) -> bool {
    //    let (row, col) = cursor(textarea);
    //    col < line_len(textarea, row).saturating_sub(1)
    //}

    fn reset_command(&mut self) {
        self.count = None;
        self.operator_count = None;
        self.operator_start = None;
        self.pending = Input::default();
    }

    fn begin_insert(&mut self, repeat: usize, newline: bool) {
        self.insert_repeat = repeat;
        self.insert_newline = newline;
        self.insert_keys.clear();
    }

    fn finish_insert(&mut self, textarea: &mut TextArea<'_>, replace: bool) {
        // A count on i/a/I/A/o/O/R repeats the entered keys when Esc is pressed.
        for _ in 1..self.insert_repeat {
            if self.insert_newline {
                textarea.insert_newline();
            }
            for input in &self.insert_keys {
                if replace {
                    replace_key(textarea, input.clone());
                } else {
                    insert_key(textarea, input.clone());
                }
            }
        }
        self.insert_keys.clear();
        self.insert_repeat = 1;
        self.insert_newline = false;
    }

    fn finish_operator(&mut self, textarea: &mut TextArea<'_>, op: char) -> Transition {
        let start = self.operator_start;
        match op {
            'y' => {
                textarea.copy();
                self.copy_to_clipboard(textarea);
                textarea.cancel_selection();

                if let Some((row, col)) = start {
                    jump(textarea, row, col);
                }
            }
            'd' | 'c' => {
                textarea.cut();
                self.copy_to_clipboard(textarea);
                textarea.cancel_selection();
            }
            _ => textarea.cancel_selection(),
        }
        self.reset_command();
        if op == 'c' {
            self.begin_insert(1, false);
            Transition::Mode(Mode::Insert)
        } else {
            normal_cursor(textarea);
            Transition::Mode(Mode::Normal)
        }
    }

    // The caller still applies the returned Transition, as in the original API.
    // Keep the same Vim state; do not rebuild it with Vim::new after each key.
    pub fn transition(&mut self, input: Input, textarea: &mut TextArea<'_>) -> Transition {
        if input.key == Key::Null {
            return Transition::Nop;
        }

        let escape = input.key == Key::Esc || (input.ctrl && input.key == Key::Char('['));
        if escape {
            match self.mode {
                Mode::Insert => self.finish_insert(textarea, false),
                Mode::Replace(false) => self.finish_insert(textarea, true),
                _ => {}
            }
            self.reset_command();
            textarea.cancel_selection();
            normal_cursor(textarea);
            return Transition::Mode(Mode::Normal);
        }

        // Digits in Insert/Replace mode are text, not numeric prefixes.
        match self.mode {
            Mode::Insert => {
                if input.ctrl && input.key == Key::Char('c') {
                    self.begin_insert(1, false);
                    self.reset_command();
                    textarea.cancel_selection();
                    return Transition::Mode(Mode::Normal);
                }
                insert_key(textarea, input.clone());
                if self.insert_repeat > 1 {
                    self.insert_keys.push(input);
                }
                return Transition::Mode(Mode::Insert);
            }
            Mode::Replace(true) => {
                if let Input {
                    key: Key::Char(c),
                    ctrl: false,
                    alt: false,
                    ..
                } = input
                {
                    let (row, col) = cursor(textarea);
                    let n = self.replace_count;
                    // Like Vim, r does nothing if the whole count cannot fit.
                    if n <= line_len(textarea, row).saturating_sub(col) {
                        select_range(textarea, (row, col), (row, col + n));
                        textarea.cut();
                        self.copy_to_clipboard(textarea);
                        textarea.insert_str(c.to_string().repeat(n));
                        jump(textarea, row, col + n - 1);
                    }
                }
                self.replace_count = 1;
                self.reset_command();
                return Transition::Mode(Mode::Normal);
            }
            Mode::Replace(false) => {
                replace_key(textarea, input.clone());
                if self.insert_repeat > 1 {
                    self.insert_keys.push(input);
                }
                return Transition::Mode(Mode::Replace(false));
            }
            _ => {}
        }

        // 1..9 start a count. 0 extends a count, or moves to the line head.
        if !input.ctrl && !input.alt && self.pending.key == Key::Null {
            if let Key::Char(c @ '0'..='9') = input.key {
                if c != '0' || self.count.is_some() {
                    self.count = Some(
                        self.count
                            .unwrap_or(0)
                            .saturating_mul(10)
                            .saturating_add((c as u8 - b'0') as usize)
                            .min(MAX_COUNT),
                    );
                    return Transition::Nop;
                }
            }
        }

        // Keep counts intact while waiting for the second g in [count]gg.
        let plain = !input.ctrl && !input.alt;
        let gg = plain && input.key == Key::Char('g') && self.pending.key == Key::Char('g');
        if plain && input.key == Key::Char('g') && !gg {
            self.pending = input.clone();
            return Transition::Pending(input);
        }
        if self.pending.key == Key::Char('g') && !gg {
            // An unsupported g sequence cancels the whole pending command.
            self.reset_command();
            if matches!(self.mode, Mode::Operator(_)) {
                textarea.cancel_selection();
                return Transition::Mode(Mode::Normal);
            }
            return Transition::Nop;
        }
        self.pending = Input::default();

        let raw_count = self.count.take();
        let explicit_count = raw_count.is_some() || self.operator_count.is_some();
        let count = raw_count
            .unwrap_or(1)
            .saturating_mul(self.operator_count.unwrap_or(1))
            .min(MAX_COUNT);

        if let Mode::Operator(op) = self.mode {
            if plain && input.key == Key::Char(op) {
                // dd/yy/cc, including 2dd, d2d, and 2d3d.
                let row = cursor(textarea).0;
                let last = row
                    .saturating_add(count - 1)
                    .min(textarea.lines().len() - 1);
                select_lines(textarea, row, last, op);
                return self.finish_operator(textarea, op);
            }
        }

        let mut motion = if gg {
            Some(Motion::FirstLine)
        } else if plain {
            match input.key {
                Key::Char('h') => Some(Motion::BackAcrossLines),
                Key::Left => Some(Motion::Back),
                Key::Char('j') | Key::Down => Some(Motion::Down),
                Key::Char('k') | Key::Up => Some(Motion::Up),
                Key::Char('l') => Some(Motion::ForwardAcrossLines),
                Key::Right => Some(Motion::Forward),
                Key::Char('w') => Some(Motion::WordForward),
                Key::Char('e') => Some(Motion::WordEnd),
                Key::Char('b') => Some(Motion::WordBack),
                Key::Char('W') => Some(Motion::BigWordForward),
                Key::Char('E') => Some(Motion::BigWordEnd),
                Key::Char('B') => Some(Motion::BigWordBack),
                Key::Char('0') | Key::Home => Some(Motion::Head),
                Key::Char('^') => Some(Motion::FirstNonBlank),
                Key::Char('$') | Key::End => Some(Motion::LineEnd),
                Key::Char('G') => Some(Motion::LastLine),
                _ => None,
            }
        } else {
            None
        };

        // cw/cW on a nonblank change through the word end, leaving following spaces.
        let mut motion_count = count;
        if self.mode == Mode::Operator('c') {
            let (row, col) = cursor(textarea);
            if textarea.lines()[row]
                .chars()
                .nth(col)
                .is_some_and(|c| !c.is_whitespace())
            {
                let big = matches!(motion, Some(Motion::BigWordForward));
                if matches!(motion, Some(Motion::WordForward | Motion::BigWordForward))
                    && at_word_end(textarea, big)
                {
                    motion_count -= 1;
                }
                motion = match motion {
                    Some(Motion::WordForward) => Some(Motion::WordEnd),
                    Some(Motion::BigWordForward) => Some(Motion::BigWordEnd),
                    other => other,
                };
            }
        }

        if let Some(motion) = motion {
            let start = cursor(textarea);
            let operator = matches!(self.mode, Mode::Operator(_));
            move_motion(textarea, motion, motion_count, explicit_count, operator);
            if let Mode::Operator(op) = self.mode {
                if motion.linewise() {
                    let end = cursor(textarea).0;
                    select_lines(textarea, start.0.min(end), start.0.max(end), op);
                } else if motion.inclusive() {
                    // Extend only once, after the whole counted motion.
                    // Extending after each e/E would skip words for d2e/d2E.
                    let (row, col) = cursor(textarea);
                    let end = (col + 1).min(line_len(textarea, row));
                    jump(textarea, row, end);
                }
                return self.finish_operator(textarea, op);
            }
            if self.mode == Mode::Normal {
                normal_cursor(textarea);
            }
            return Transition::Nop;
        }

        // No valid motion: cancel the operator instead of leaving a stale count.
        if matches!(self.mode, Mode::Operator(_)) {
            self.reset_command();
            textarea.cancel_selection();
            return Transition::Mode(Mode::Normal);
        }

        if plain {
            if let Key::Char(op @ ('y' | 'd' | 'c')) = input.key {
                if self.mode == Mode::Normal {
                    self.operator_count = raw_count;
                    self.operator_start = Some(cursor(textarea));
                    textarea.start_selection();
                    return Transition::Mode(Mode::Operator(op));
                }
            }
        }

        self.reset_command();
        match input {
            Input {
                key: Key::Char('D' | 'C' | 'S'),
                ctrl: false,
                alt: false,
                ..
            } => {
                let (row, col) = cursor(textarea);
                let last = row
                    .saturating_add(count - 1)
                    .min(textarea.lines().len() - 1);
                let start = if input.key == Key::Char('S') {
                    (row, 0)
                } else {
                    (row, col)
                };
                let end = (last, line_len(textarea, last));
                select_range(textarea, start, end);
                textarea.cut();
                self.copy_to_clipboard(textarea);
                textarea.cancel_selection();
                if input.key == Key::Char('D') {
                    normal_cursor(textarea);
                    Transition::Mode(Mode::Normal)
                } else {
                    self.begin_insert(1, false);
                    Transition::Mode(Mode::Insert)
                }
            }
            Input {
                key: Key::Char('x' | 'X'),
                ctrl: false,
                alt: false,
                ..
            } => {
                let (row, col) = cursor(textarea);
                let (start, end) = if input.key == Key::Char('X') {
                    (col.saturating_sub(count), col)
                } else {
                    (col, col.saturating_add(count).min(line_len(textarea, row)))
                };
                if start < end {
                    select_range(textarea, (row, start), (row, end));
                    textarea.cut();
                    self.copy_to_clipboard(textarea);
                }
                textarea.cancel_selection();
                normal_cursor(textarea);
                Transition::Mode(Mode::Normal)
            }
            Input {
                key: Key::Char('p'),
                ctrl: false,
                alt: false,
                ..
            } => {
                // Read once, so counted pastes use the same clipboard contents.
                self.load_clipboard(textarea);

                for _ in 0..count {
                    if !textarea.paste() {
                        break;
                    }
                }

                textarea.cancel_selection();
                normal_cursor(textarea);
                Transition::Mode(Mode::Normal)
            }
            Input {
                key: Key::Char('u'),
                ctrl: false,
                alt: false,
                ..
            } => {
                textarea.cancel_selection();
                for _ in 0..count {
                    if !textarea.undo() {
                        break;
                    }
                }
                normal_cursor(textarea);
                Transition::Mode(Mode::Normal)
            }
            Input {
                key: Key::Char('r'),
                ctrl: true,
                alt: false,
                ..
            } => {
                textarea.cancel_selection();
                for _ in 0..count {
                    if !textarea.redo() {
                        break;
                    }
                }
                normal_cursor(textarea);
                Transition::Mode(Mode::Normal)
            }
            Input {
                key: Key::Char(c @ ('i' | 'a' | 'I' | 'A' | 'o' | 'O')),
                ctrl: false,
                alt: false,
                ..
            } => {
                textarea.cancel_selection();
                let (row, col) = cursor(textarea);
                let len = line_len(textarea, row);
                let first = first_nonblank(textarea, row);
                match c {
                    'a' => jump(textarea, row, (col + 1).min(len)),
                    'A' => jump(textarea, row, len),
                    'I' => jump(textarea, row, first),
                    'o' => {
                        jump(textarea, row, len);
                        textarea.insert_newline();
                    }
                    'O' => {
                        jump(textarea, row, 0);
                        textarea.insert_newline();
                        jump(textarea, row, 0);
                    }
                    _ => {}
                }
                self.begin_insert(count, matches!(c, 'o' | 'O'));
                Transition::Mode(Mode::Insert)
            }
            Input {
                key: Key::Char('J'),
                ctrl: false,
                alt: false,
                ..
            } => {
                let joins = count.max(2) - 1;
                for _ in 0..joins {
                    let row = cursor(textarea).0;
                    if row + 1 >= textarea.lines().len() {
                        break;
                    }
                    let len = line_len(textarea, row);
                    jump(textarea, row, len);
                    textarea.delete_next_char();
                    textarea.insert_char(' ');
                }
                normal_cursor(textarea);
                Transition::Mode(Mode::Normal)
            }
            Input {
                key: Key::Char('r'),
                ctrl: false,
                alt: false,
                ..
            } if self.mode == Mode::Normal => {
                self.replace_count = count;
                Transition::Mode(Mode::Replace(true))
            }
            Input {
                key: Key::Char('R'),
                ctrl: false,
                alt: false,
                ..
            } if self.mode == Mode::Normal => {
                self.begin_insert(count, false);
                Transition::Mode(Mode::Replace(false))
            }
            Input {
                key: Key::Char('v'),
                ctrl: false,
                alt: false,
                ..
            } if self.mode == Mode::Normal => {
                textarea.cancel_selection();
                Transition::EditExternally
            }
            Input {
                key: Key::Char('q'),
                ctrl: false,
                alt: false,
                ..
            } => Transition::Quit,
            Input {
                key: Key::Char(c @ ('e' | 'y')),
                ctrl: true,
                alt: false,
                ..
            } => {
                for _ in 0..count {
                    textarea.scroll((if c == 'e' { 1 } else { -1 }, 0));
                }
                Transition::Nop
            }
            Input {
                key: Key::Char(c @ ('d' | 'u')),
                ctrl: true,
                alt: false,
                ..
            } => {
                if explicit_count {
                    for _ in 0..count {
                        textarea.scroll((if c == 'd' { 1 } else { -1 }, 0));
                    }
                } else {
                    textarea.scroll(if c == 'd' {
                        Scrolling::HalfPageDown
                    } else {
                        Scrolling::HalfPageUp
                    });
                }
                Transition::Nop
            }
            Input {
                key: Key::Char(c @ ('f' | 'b')),
                ctrl: true,
                alt: false,
                ..
            } => {
                for _ in 0..count {
                    textarea.scroll(if c == 'f' {
                        Scrolling::PageDown
                    } else {
                        Scrolling::PageUp
                    });
                }
                Transition::Nop
            }
            _ => Transition::Nop,
        }
    }
}

#[derive(Clone, Copy)]
enum Motion {
    Back,
    BackAcrossLines,
    Down,
    Up,
    Forward,
    ForwardAcrossLines,
    WordForward,
    WordEnd,
    WordBack,
    BigWordForward,
    BigWordEnd,
    BigWordBack,
    Head,
    FirstNonBlank,
    LineEnd,
    FirstLine,
    LastLine,
}

impl Motion {
    fn inclusive(self) -> bool {
        matches!(self, Self::WordEnd | Self::BigWordEnd | Self::LineEnd)
    }

    fn linewise(self) -> bool {
        matches!(
            self,
            Self::Down | Self::Up | Self::FirstLine | Self::LastLine
        )
    }
}

// Works with tuple cursors and newer TextArea versions' DataCursor type.
fn cursor(textarea: &TextArea<'_>) -> Position {
    let position = textarea.cursor();
    (position.0, position.1)
}

fn line_len(textarea: &TextArea<'_>, row: usize) -> usize {
    textarea.lines()[row].chars().count()
}

fn first_nonblank(textarea: &TextArea<'_>, row: usize) -> usize {
    textarea.lines()[row]
        .chars()
        .position(|c| !c.is_whitespace())
        .unwrap_or(0)
}

fn at_word_end(textarea: &TextArea<'_>, big: bool) -> bool {
    let (row, col) = cursor(textarea);
    let mut chars = textarea.lines()[row].chars().skip(col);
    let Some(current) = chars.next() else {
        return true;
    };
    let Some(next) = chars.next() else {
        return true;
    };
    next.is_whitespace()
        || (!big
            && (current.is_alphanumeric() || current == '_')
                != (next.is_alphanumeric() || next == '_'))
}

fn jump(textarea: &mut TextArea<'_>, row: usize, col: usize) {
    textarea.move_cursor(CursorMove::Jump(
        u16::try_from(row).unwrap_or(u16::MAX),
        u16::try_from(col).unwrap_or(u16::MAX),
    ));
}

fn normal_cursor(textarea: &mut TextArea<'_>) {
    let (row, col) = cursor(textarea);
    let last = line_len(textarea, row).saturating_sub(1);
    if col > last {
        jump(textarea, row, last);
    }
}

fn select_range(textarea: &mut TextArea<'_>, start: Position, end: Position) {
    textarea.cancel_selection();
    jump(textarea, start.0, start.1);
    textarea.start_selection();
    jump(textarea, end.0, end.1);
}

fn select_lines(textarea: &mut TextArea<'_>, first: usize, last: usize, op: char) {
    let end = (last, line_len(textarea, last));
    if op == 'c' {
        // Leave an empty line for the replacement text from cc/cj/cG.
        select_range(textarea, (first, 0), end);
    } else if last + 1 < textarea.lines().len() {
        select_range(textarea, (first, 0), (last + 1, 0));
    } else if op == 'd' && first > 0 {
        // Deleting final lines also removes their preceding newline.
        let start = (first - 1, line_len(textarea, first - 1));
        select_range(textarea, start, end);
    } else {
        select_range(textarea, (first, 0), end);
    }
}

fn move_motion(
    textarea: &mut TextArea<'_>,
    motion: Motion,
    count: usize,
    explicit_count: bool,
    operator: bool,
) {
    let (row, col) = cursor(textarea);
    let last_row = textarea.lines().len() - 1;
    match motion {
        Motion::BackAcrossLines | Motion::ForwardAcrossLines if !operator => {
            let target = horizontal_target(textarea.lines(), (row, col), motion, count);
            jump(textarea, target.0, target.1);
        }
        // Keep the existing characterwise operator ranges on the current line.
        Motion::Back | Motion::BackAcrossLines => {
            jump(textarea, row, col.saturating_sub(count));
        }
        Motion::Forward | Motion::ForwardAcrossLines => {
            let len = line_len(textarea, row);
            let limit = if operator { len } else { len.saturating_sub(1) };
            jump(textarea, row, col.saturating_add(count).min(limit));
        }
        Motion::Down => jump(textarea, row.saturating_add(count).min(last_row), col),
        Motion::Up => jump(textarea, row.saturating_sub(count), col),
        Motion::Head => jump(textarea, row, 0),
        Motion::FirstNonBlank => {
            let col = first_nonblank(textarea, row);
            jump(textarea, row, col);
        }
        Motion::LineEnd => {
            let row = row.saturating_add(count - 1).min(last_row);
            let col = line_len(textarea, row).saturating_sub(1);
            jump(textarea, row, col);
        }
        Motion::FirstLine | Motion::LastLine => {
            let row = if explicit_count {
                (count - 1).min(last_row)
            } else if matches!(motion, Motion::FirstLine) {
                0
            } else {
                last_row
            };
            let col = first_nonblank(textarea, row);
            jump(textarea, row, col);
        }
        Motion::WordForward | Motion::WordEnd | Motion::WordBack => {
            let movement = match motion {
                Motion::WordForward => CursorMove::WordForward,
                Motion::WordEnd => CursorMove::WordEnd,
                _ => CursorMove::WordBack,
            };
            for step in 0..count {
                let before = cursor(textarea);
                textarea.move_cursor(movement);
                if operator
                    && matches!(motion, Motion::WordForward)
                    && step + 1 == count
                    && cursor(textarea).0 > before.0
                    && textarea.lines()[before.0]
                        .chars()
                        .nth(before.1)
                        .is_some_and(|c| !c.is_whitespace())
                {
                    let end = line_len(textarea, before.0);
                    jump(textarea, before.0, end);
                }
                if before == cursor(textarea) {
                    break;
                }
            }
        }
        Motion::BigWordForward | Motion::BigWordEnd | Motion::BigWordBack => {
            let target = big_word_target(textarea.lines(), (row, col), motion, count, operator);
            jump(textarea, target.0, target.1);
        }
    }
}

// Normal-mode h/l cross line boundaries without stopping after the last character.
// Work a line at a time so large counts do not repeatedly scan the same line.
fn horizontal_target(
    lines: &[String],
    (mut row, mut col): Position,
    motion: Motion,
    mut count: usize,
) -> Position {
    let forward = matches!(motion, Motion::ForwardAcrossLines);

    while count > 0 {
        if forward {
            let last_col = lines[row].chars().count().saturating_sub(1);
            let distance = last_col.saturating_sub(col);
            if count <= distance {
                col += count;
                break;
            }
            if row + 1 >= lines.len() {
                col = last_col;
                break;
            }
            count -= distance + 1;
            row += 1;
            col = 0;
        } else {
            if count <= col {
                col -= count;
                break;
            }
            if row == 0 {
                col = 0;
                break;
            }
            count -= col + 1;
            row -= 1;
            col = lines[row].chars().count().saturating_sub(1);
        }
    }

    (row, col)
}

// W/B/E use whitespace-delimited WORDs: punctuation stays in the same WORD.
// Positions are Unicode character indices, matching TextArea::cursor().
fn big_word_target(
    lines: &[String],
    mut pos: Position,
    motion: Motion,
    count: usize,
    operator: bool,
) -> Position {
    let lines: Vec<Vec<char>> = lines.iter().map(|line| line.chars().collect()).collect();
    for step in 0..count {
        let before = pos;
        match motion {
            Motion::BigWordForward => {
                if lines[pos.0].is_empty() {
                    if let Some(next) = next_position(&lines, pos) {
                        pos = next;
                    }
                } else {
                    while !blank_at(&lines, pos) {
                        match next_position(&lines, pos) {
                            Some(next) => pos = next,
                            None => break,
                        }
                    }
                }
                while blank_at(&lines, pos) && !lines[pos.0].is_empty() {
                    match next_position(&lines, pos) {
                        Some(next) => pos = next,
                        None => break,
                    }
                }
            }
            Motion::BigWordBack => {
                if let Some(previous) = previous_position(&lines, pos) {
                    pos = previous;
                }
                while blank_at(&lines, pos) && !lines[pos.0].is_empty() {
                    match previous_position(&lines, pos) {
                        Some(previous) => pos = previous,
                        None => break,
                    }
                }
                if !blank_at(&lines, pos) {
                    while let Some(previous) = previous_position(&lines, pos) {
                        if blank_at(&lines, previous) {
                            break;
                        }
                        pos = previous;
                    }
                }
            }
            Motion::BigWordEnd => {
                if let Some(next) = next_position(&lines, pos) {
                    pos = next;
                }
                while blank_at(&lines, pos) {
                    match next_position(&lines, pos) {
                        Some(next) => pos = next,
                        None => break,
                    }
                }
                while let Some(next) = next_position(&lines, pos) {
                    if blank_at(&lines, next) {
                        break;
                    }
                    pos = next;
                }
            }
            _ => unreachable!("expected a WORD motion"),
        }
        if operator
            && matches!(motion, Motion::BigWordForward)
            && step + 1 == count
            && pos.0 > before.0
            && !blank_at(&lines, before)
        {
            pos = (before.0, lines[before.0].len());
        }
        if before == pos {
            break;
        }
    }
    pos
}

fn blank_at(lines: &[Vec<char>], (row, col): Position) -> bool {
    lines[row].get(col).map_or(true, |c| c.is_whitespace())
}

fn next_position(lines: &[Vec<char>], (row, col): Position) -> Option<Position> {
    if col < lines[row].len() {
        Some((row, col + 1))
    } else if row + 1 < lines.len() {
        Some((row + 1, 0))
    } else {
        None
    }
}

fn previous_position(lines: &[Vec<char>], (row, col): Position) -> Option<Position> {
    if col > 0 {
        Some((row, col - 1))
    } else if row > 0 {
        Some((row - 1, lines[row - 1].len()))
    } else {
        None
    }
}

fn insert_key(textarea: &mut TextArea<'_>, input: Input) {
    if input.ctrl && input.key == Key::Char('u') {
        // Preserve the original custom Insert-mode Ctrl-U behavior.
        let row = cursor(textarea).0;
        let end = (row, line_len(textarea, row));
        select_range(textarea, (row, 0), end);
        textarea.cut();
        textarea.cancel_selection();
    } else {
        textarea.input(input);
    }
}

fn replace_key(textarea: &mut TextArea<'_>, input: Input) {
    if let Input {
        key: Key::Char(c),
        ctrl: false,
        alt: false,
        ..
    } = input
    {
        let (row, col) = cursor(textarea);
        if col < line_len(textarea, row) {
            textarea.delete_next_char();
        }
        textarea.insert_char(c);
    }
}

pub fn edit_externally(
    terminal: &mut DefaultTerminal,
    textarea: &mut TextArea<'_>,
) -> io::Result<()> {
    let editor = config().general.external_editor.as_deref().unwrap();
    if !editor.is_empty() {
        let text = textarea.lines().join("\n");
        let position = cursor(textarea);

        ratatui::restore();
        let result = (|| -> std::io::Result<String> {
            let file = tempfile::NamedTempFile::new()?;
            std::fs::write(file.path(), &text)?;

            let editor = config().general.external_editor.as_deref().unwrap();

            let mut exec_cmd = "sh -c".to_string();
            if let Some(module_name) = config().general.exec_cmd.as_ref() {
                exec_cmd = module_name.to_string()
            };
            let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
            let mut shell_cmd = std::process::Command::new(cmd_parts[0]);
            for arg in &cmd_parts[1..] {
                shell_cmd.arg(arg);
            }

            let _ = shell_cmd
                .arg(format!("{} {} 2>/dev/null", editor, file.path().display()))
                .status();

            std::fs::read_to_string(file.path())
        })();

        // Move cursor up one line to offset the line created by creating a viewport
        let mut stdout = io::stdout();
        write!(stdout, "\x1b[1A")?;

        // Restore the TUI before propagating an editor-launch or file-read error.
        *terminal = ratatui::try_init_with_options(ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Inline(config().general.height.unwrap()),
        })?;
        terminal.clear()?;

        let edited = result?;
        textarea.cancel_selection();
        if edited != text {
            // Replace the contents without rebuilding the styled textarea.
            textarea.select_all();
            textarea.insert_str(&edited.trim_end());
            textarea.cancel_selection();

            // Retain the old cursor position as far as the edited text permits.
            let row = position.0.min(textarea.lines().len() - 1);
            let col = position.1.min(line_len(textarea, row));
            jump(textarea, row, col);
        }
        normal_cursor(textarea);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> Input {
        Input {
            key: Key::Char(c),
            ..Input::default()
        }
    }

    fn send(vim: &mut Vim, textarea: &mut TextArea<'_>, input: Input) {
        match vim.transition(input, textarea) {
            Transition::Mode(mode) => vim.mode = mode,
            Transition::Pending(input) => vim.pending = input,
            Transition::Nop => {}
            Transition::EditExternally => panic!("unexpected external editor request"),
            Transition::Quit => panic!("unexpected quit"),
        }
    }

    fn keys(vim: &mut Vim, textarea: &mut TextArea<'_>, input: &str) {
        for c in input.chars() {
            send(vim, textarea, key(c));
        }
    }

    fn esc(vim: &mut Vim, textarea: &mut TextArea<'_>) {
        send(
            vim,
            textarea,
            Input {
                key: Key::Esc,
                ..Input::default()
            },
        );
    }

    #[test]
    fn delete_counts_before_and_after_operator() {
        for command in ["2dw", "d2w", "2d3w"] {
            let mut vim = Vim::new(Mode::Normal);
            let mut textarea = TextArea::from(["one two three four five six seven"]);
            keys(&mut vim, &mut textarea, command);
            let expected = if command == "2d3w" {
                "seven"
            } else {
                "three four five six seven"
            };
            assert_eq!(textarea.lines(), [expected], "{command}");
            assert_eq!(vim.mode, Mode::Normal);
        }
    }

    #[test]
    fn count_includes_word_end_only_once() {
        for command in ["2de", "d2e", "2dE", "d2E"] {
            let mut vim = Vim::new(Mode::Normal);
            let mut textarea = TextArea::from(["one two three"]);
            keys(&mut vim, &mut textarea, command);
            assert_eq!(textarea.lines(), [" three"], "{command}");
        }
    }

    #[test]
    fn count_survives_mode_builder() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["one two three"]);
        send(&mut vim, &mut textarea, key('2'));
        let transition = vim.transition(key('d'), &mut textarea);
        if let Transition::Mode(mode) = transition {
            vim = vim.with_mode(mode);
        }
        send(&mut vim, &mut textarea, key('w'));
        assert_eq!(textarea.lines(), ["three"]);
    }

    #[test]
    fn digits_and_zero_motion() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["abcdefghijklmnop"]);
        keys(&mut vim, &mut textarea, "10l");
        assert_eq!(textarea.cursor(), (0, 10));
        keys(&mut vim, &mut textarea, "0");
        assert_eq!(textarea.cursor(), (0, 0));
        keys(&mut vim, &mut textarea, "3l");
        assert_eq!(textarea.cursor(), (0, 3));
    }

    #[test]
    fn horizontal_motions_cross_line_boundaries() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["abc", "def"]);
        jump(&mut textarea, 0, 2);

        keys(&mut vim, &mut textarea, "l");
        assert_eq!(textarea.cursor(), (1, 0));
        keys(&mut vim, &mut textarea, "h");
        assert_eq!(textarea.cursor(), (0, 2));
        assert_eq!(textarea.lines(), ["abc", "def"]);
    }

    #[test]
    fn counted_horizontal_motions_cross_empty_lines_and_unicode() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["a🙂", "", "你好b"]);

        keys(&mut vim, &mut textarea, "4l");
        assert_eq!(textarea.cursor(), (2, 1));
        keys(&mut vim, &mut textarea, "4h");
        assert_eq!(textarea.cursor(), (0, 0));
        keys(&mut vim, &mut textarea, "2l");
        assert_eq!(textarea.cursor(), (1, 0));
        keys(&mut vim, &mut textarea, "2l");
        assert_eq!(textarea.cursor(), (2, 1));
    }

    #[test]
    fn horizontal_motions_stop_at_buffer_edges() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["abc", "", "de"]);

        keys(&mut vim, &mut textarea, "65535h");
        assert_eq!(textarea.cursor(), (0, 0));
        keys(&mut vim, &mut textarea, "65535l");
        assert_eq!(textarea.cursor(), (2, 1));
        keys(&mut vim, &mut textarea, "65535h");
        assert_eq!(textarea.cursor(), (0, 0));

        let mut textarea = TextArea::from(["", "", ""]);
        keys(&mut vim, &mut textarea, "9l");
        assert_eq!(textarea.cursor(), (2, 0));
        keys(&mut vim, &mut textarea, "9h");
        assert_eq!(textarea.cursor(), (0, 0));
    }

    #[test]
    fn horizontal_wrap_preserves_operator_ranges() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["abc", "def"]);
        jump(&mut textarea, 0, 1);
        keys(&mut vim, &mut textarea, "d9l");
        assert_eq!(textarea.lines(), ["a", "def"]);

        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["abc", "def"]);
        jump(&mut textarea, 1, 1);
        keys(&mut vim, &mut textarea, "d9h");
        assert_eq!(textarea.lines(), ["abc", "ef"]);
    }

    #[test]
    fn uppercase_words_keep_punctuation() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["foo-bar baz.qux fin"]);
        keys(&mut vim, &mut textarea, "W");
        assert_eq!(textarea.cursor(), (0, 8));
        keys(&mut vim, &mut textarea, "E");
        assert_eq!(textarea.cursor(), (0, 14));
        keys(&mut vim, &mut textarea, "B");
        assert_eq!(textarea.cursor(), (0, 8));
        keys(&mut vim, &mut textarea, "2B");
        assert_eq!(textarea.cursor(), (0, 0));
    }

    #[test]
    fn uppercase_words_cross_lines_and_use_character_indices() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["你好-世界", "", "  🙂-ok rest"]);
        keys(&mut vim, &mut textarea, "W");
        assert_eq!(textarea.cursor(), (1, 0));
        keys(&mut vim, &mut textarea, "W");
        assert_eq!(textarea.cursor(), (2, 2));
        keys(&mut vim, &mut textarea, "E");
        assert_eq!(textarea.cursor(), (2, 5));
        keys(&mut vim, &mut textarea, "2B");
        assert_eq!(textarea.cursor(), (1, 0));
    }

    #[test]
    fn uppercase_word_operators_and_change_counts() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["foo-bar baz.qux rest"]);
        keys(&mut vim, &mut textarea, "d2W");
        assert_eq!(textarea.lines(), ["rest"]);
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["foo-bar baz.qux rest"]);
        keys(&mut vim, &mut textarea, "2cW");
        assert_eq!(textarea.lines(), [" rest"]);
        assert_eq!(vim.mode, Mode::Insert);
    }

    #[test]
    fn uppercase_x_is_bounded_to_the_current_line() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["previous", "a🙂bc"]);
        jump(&mut textarea, 1, 3);
        keys(&mut vim, &mut textarea, "2X");
        assert_eq!(textarea.lines(), ["previous", "ac"]);
        assert_eq!(textarea.cursor(), (1, 1));
        keys(&mut vim, &mut textarea, "9X");
        assert_eq!(textarea.lines(), ["previous", "c"]);
        keys(&mut vim, &mut textarea, "X");
        assert_eq!(textarea.lines(), ["previous", "c"]);
    }

    #[test]
    fn x_can_delete_last_character_without_merging_lines() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["你好🙂", "next"]);
        jump(&mut textarea, 0, 2);
        keys(&mut vim, &mut textarea, "3x");
        assert_eq!(textarea.lines(), ["你好", "next"]);
    }

    #[test]
    fn doubled_operators_use_line_counts() {
        for command in ["2dd", "d2d"] {
            let mut vim = Vim::new(Mode::Normal);
            let mut textarea = TextArea::from(["one", "two", "three"]);
            keys(&mut vim, &mut textarea, command);
            assert_eq!(textarea.lines(), ["three"], "{command}");
        }
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["one", "two", "three"]);
        jump(&mut textarea, 1, 0);
        keys(&mut vim, &mut textarea, "2dd");
        assert_eq!(textarea.lines(), ["one"]);
    }

    #[test]
    fn yank_and_change_accept_counts() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["one two three"]);
        keys(&mut vim, &mut textarea, "y2w");
        assert_eq!(textarea.yank_text(), "one two ");
        assert_eq!(textarea.cursor(), (0, 0));
        keys(&mut vim, &mut textarea, "2cwX");
        assert_eq!(textarea.lines(), ["X three"]);
        esc(&mut vim, &mut textarea);
        assert_eq!(vim.mode, Mode::Normal);
    }

    #[test]
    fn escape_and_invalid_keys_clear_counts_and_selections() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["one two three"]);
        keys(&mut vim, &mut textarea, "2d3");
        esc(&mut vim, &mut textarea);
        keys(&mut vim, &mut textarea, "w");
        assert_eq!(textarea.cursor(), (0, 4));
        assert!(textarea.selection_range().is_none());
        keys(&mut vim, &mut textarea, "9z0l");
        assert_eq!(textarea.cursor(), (0, 1));
    }

    #[test]
    fn gg_and_g_use_one_based_line_counts() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["one", "  two", "three", "four"]);
        keys(&mut vim, &mut textarea, "3gg");
        assert_eq!(textarea.cursor(), (2, 0));
        keys(&mut vim, &mut textarea, "2G");
        assert_eq!(textarea.cursor(), (1, 2));
        keys(&mut vim, &mut textarea, "G");
        assert_eq!(textarea.cursor(), (3, 0));
        keys(&mut vim, &mut textarea, "gg");
        assert_eq!(textarea.cursor(), (0, 0));
    }

    #[test]
    fn r_replaces_a_count_and_accepts_a_literal_digit() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["abcde"]);
        keys(&mut vim, &mut textarea, "3r7");
        assert_eq!(textarea.lines(), ["777de"]);
        assert_eq!(textarea.cursor(), (0, 2));
        keys(&mut vim, &mut textarea, "4rX");
        assert_eq!(textarea.lines(), ["777de"]);
    }

    #[test]
    fn insert_counts_repeat_on_escape_and_digits_stay_literal() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::default();
        keys(&mut vim, &mut textarea, "3i12");
        assert_eq!(textarea.lines(), ["12"]);
        esc(&mut vim, &mut textarea);
        assert_eq!(textarea.lines(), ["121212"]);
        assert_eq!(vim.mode, Mode::Normal);
    }

    #[test]
    fn open_line_counts_repeat_the_entered_text_on_each_line() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["original"]);
        keys(&mut vim, &mut textarea, "2ohello");
        esc(&mut vim, &mut textarea);
        assert_eq!(textarea.lines(), ["original", "hello", "hello"]);
    }

    #[test]
    fn replace_mode_counts_repeat_on_escape() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["abcdefgh"]);
        keys(&mut vim, &mut textarea, "3Rxy");
        esc(&mut vim, &mut textarea);
        assert_eq!(textarea.lines(), ["xyxyxygh"]);
    }

    #[test]
    fn change_word_at_an_existing_word_end() {
        for command in ["cw", "cW"] {
            let mut vim = Vim::new(Mode::Normal);
            let mut textarea = TextArea::from(["a b c"]);
            keys(&mut vim, &mut textarea, command);
            assert_eq!(textarea.lines(), [" b c"], "{command}");
        }
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["a b c"]);
        keys(&mut vim, &mut textarea, "2cW");
        assert_eq!(textarea.lines(), [" c"]);
    }

    #[test]
    fn final_word_operator_does_not_delete_the_newline() {
        for command in ["d2w", "2dW"] {
            let mut vim = Vim::new(Mode::Normal);
            let mut textarea = TextArea::from(["one two", "three"]);
            keys(&mut vim, &mut textarea, command);
            assert_eq!(textarea.lines(), ["", "three"], "{command}");
        }
    }

    #[test]
    fn direct_commands_accept_counts() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["one", "two", "three", "four"]);
        keys(&mut vim, &mut textarea, "3J");
        assert_eq!(textarea.lines(), ["one two three", "four"]);
        jump(&mut textarea, 0, 4);
        keys(&mut vim, &mut textarea, "2D");
        assert_eq!(textarea.lines(), ["one "]);
        textarea.set_yank_text("a");
        keys(&mut vim, &mut textarea, "0");
        keys(&mut vim, &mut textarea, "3p");
        assert_eq!(textarea.lines(), ["aaaone "]);
        keys(&mut vim, &mut textarea, "2u");
        assert_eq!(textarea.lines(), ["aone "]);
        keys(&mut vim, &mut textarea, "2");
        send(
            &mut vim,
            &mut textarea,
            Input {
                key: Key::Char('r'),
                ctrl: true,
                ..Input::default()
            },
        );
        assert_eq!(textarea.lines(), ["aaaone "]);
    }

    #[test]
    fn normal_v_requests_external_editor_and_clears_count() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["one two three"]);
        keys(&mut vim, &mut textarea, "2");
        let transition = vim.transition(key('v'), &mut textarea);
        assert!(matches!(transition, Transition::EditExternally));
        assert_eq!(textarea.lines(), ["one two three"]);
        assert_eq!(vim.mode, Mode::Normal);
        assert!(textarea.selection_range().is_none());
        keys(&mut vim, &mut textarea, "w");
        assert_eq!(textarea.cursor(), (0, 4));
    }

    #[test]
    fn v_is_literal_in_insert_and_cancels_a_pending_operator() {
        let mut vim = Vim::new(Mode::Insert);
        let mut textarea = TextArea::default();
        keys(&mut vim, &mut textarea, "v");
        assert_eq!(textarea.lines(), ["v"]);
        assert_eq!(vim.mode, Mode::Insert);

        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["one two"]);
        keys(&mut vim, &mut textarea, "dv");
        assert_eq!(textarea.lines(), ["one two"]);
        assert_eq!(vim.mode, Mode::Normal);
        assert!(textarea.selection_range().is_none());
    }

    #[test]
    fn uppercase_v_does_not_start_selection() {
        let mut vim = Vim::new(Mode::Normal);
        let mut textarea = TextArea::from(["one two"]);
        assert!(matches!(
            vim.transition(key('V'), &mut textarea),
            Transition::Nop
        ));
        assert_eq!(vim.mode, Mode::Normal);
        assert!(textarea.selection_range().is_none());
    }
}
