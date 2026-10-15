use crate::config::*;

use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use ratatui::{
    crossterm::event::{MouseButton, MouseEvent, MouseEventKind},
    layout::Rect,
    style::Color,
    widgets::{ListDirection, ListState},
};
use ratatui_textarea::TextArea;
use std::{
    borrow::Cow,
    time::{Duration, Instant},
};

// Struct for mouse actions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionMouseAction {
    Ignored,
    Selected,
    Scrolled,
    Activate,
    Complete,
}

#[derive(Debug)]
struct SuggestionMouseClick {
    index: usize,
    area: Rect,
    column: u16,
    row: u16,
    at: Instant,
}

#[derive(Debug, Default)]
pub struct SuggestionMouseState {
    last_click: Option<SuggestionMouseClick>,
}

impl SuggestionMouseState {
    // Cancel a pending double-click when a key or resize changes the interface.
    pub fn reset(&mut self) {
        self.last_click = None;
    }

    pub fn click(
        &mut self,
        index: usize,
        area: Rect,
        mouse: MouseEvent,
        now: Instant,
    ) -> SuggestionMouseAction {
        let double_click = self
            .last_click
            .as_ref()
            .map(|previous| {
                previous.index == index
                    && previous.area == area
                    && previous.row == mouse.row
                    && previous.column.abs_diff(mouse.column) <= 1
                    && now
                        .checked_duration_since(previous.at)
                        .map(|elapsed| elapsed <= Duration::from_millis(400))
                        .unwrap_or(false)
            })
            .unwrap_or(false);

        if double_click {
            // Pair clicks so a third click does not activate a second time.
            self.reset();
            SuggestionMouseAction::Activate
        } else {
            self.last_click = Some(SuggestionMouseClick {
                index,
                area,
                column: mouse.column,
                row: mouse.row,
                at: now,
            });
            SuggestionMouseAction::Selected
        }
    }
}

// Operations for the suggestion list rendered by the most recent draw. Store the value returned by `draw()` and forward `Event::Mouse` to `handle_mouse()` along with a persistent `SuggestionMouseState`. Enable Crossterm mouse capture while the TUI is active. Each suggestion in this interface occupies one terminal row.
#[derive(Debug, Clone, Copy)]
pub struct SuggestionOperation {
    // Includes the scrollbar, allowing wheel scrolling over either area.
    pub scroll_area: Rect,
    // Excludes borders, padding, and the scrollbar column for click selection.
    pub list_area: Rect,
    pub direction: ListDirection,
    pub item_count: usize,
}

impl SuggestionOperation {
    pub fn page(&self, state: &mut ListState, forward: bool) {
        if self.item_count == 0 || self.list_area.is_empty() {
            return;
        }

        let page = usize::from(self.list_area.height);
        let last = self.item_count - 1;
        let max_offset = self.item_count.saturating_sub(page);
        let offset = state.offset().min(max_offset);
        let edge = if forward { last } else { 0 };

        let (selected, offset) = match state.selected() {
            None => {
                let start = if forward {
                    offset.saturating_add(page).min(max_offset)
                } else {
                    max_offset
                };
                (Some(start), start)
            }
            Some(index) if index == edge => (None, 0),
            Some(index) if forward => (
                Some(index.saturating_add(page).min(last)),
                offset.saturating_add(page).min(max_offset),
            ),
            Some(index) => (
                Some(index.min(last).saturating_sub(page)),
                offset.saturating_sub(page),
            ),
        };

        state.select(selected);
        *state.offset_mut() = offset;
    }

    pub fn halfpage(&self, state: &mut ListState, forward: bool) {
        if self.item_count == 0 || self.list_area.is_empty() {
            return;
        }

        let visible_rows = usize::from(self.list_area.height);
        let step = (visible_rows / 2).max(2);
        let last = self.item_count - 1;

        // Keep the final page filled whenever enough items exist.
        let max_offset = self.item_count.saturating_sub(visible_rows);
        let offset = state.offset().min(max_offset);
        let edge = if forward { last } else { 0 };

        let (selected, offset) = match state.selected() {
            None if forward => {
                let index = offset.saturating_add(step).min(last);
                (Some(index), index.min(max_offset))
            }
            None => (Some(last), max_offset),

            Some(index) if index == edge => (None, 0),

            Some(index) if forward => (
                Some(index.saturating_add(step).min(last)),
                offset.saturating_add(step).min(max_offset),
            ),

            Some(index) => (
                Some(index.min(last).saturating_sub(step)),
                offset.saturating_sub(step),
            ),
        };

        state.select(selected);
        *state.offset_mut() = offset;
    }

    // Single-click selects; double-click activates; wheel steps cycle selection.
    pub fn handle_mouse(
        &self,
        mouse: MouseEvent,
        state: &mut ListState,
        mouse_state: &mut SuggestionMouseState,
    ) -> SuggestionMouseAction {
        self.handle_mouse_at(mouse, state, mouse_state, Instant::now())
    }

    pub fn handle_mouse_at(
        &self,
        mouse: MouseEvent,
        state: &mut ListState,
        mouse_state: &mut SuggestionMouseState,
        now: Instant,
    ) -> SuggestionMouseAction {
        if self.item_count == 0 || self.list_area.is_empty() {
            mouse_state.reset();
            return SuggestionMouseAction::Ignored;
        }

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let Some(index) = self.item_at(mouse.column, mouse.row, state.offset()) else {
                    mouse_state.reset();
                    return SuggestionMouseAction::Ignored;
                };
                state.select(Some(index));
                mouse_state.click(index, self.list_area, mouse, now)
            }
            MouseEventKind::Down(MouseButton::Right) => {
                return SuggestionMouseAction::Complete;
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                mouse_state.reset();
                if !self.contains(self.scroll_area, mouse.column, mouse.row) {
                    return SuggestionMouseAction::Ignored;
                }

                // Match the physical wheel direction for either list orientation.
                let increase_index = matches!(
                    (mouse.kind, self.direction),
                    (MouseEventKind::ScrollDown, ListDirection::TopToBottom)
                        | (MouseEventKind::ScrollUp, ListDirection::BottomToTop)
                );
                let selected = state.selected().filter(|&index| index < self.item_count);
                let next = match (selected, increase_index) {
                    (None, true) => 0,
                    (None, false) => self.item_count - 1,
                    (Some(index), true) => {
                        if index + 1 == self.item_count {
                            0
                        } else {
                            index + 1
                        }
                    }
                    (Some(0), false) => self.item_count - 1,
                    (Some(index), false) => index - 1,
                };

                state.select(Some(next));
                // List's next render scrolls automatically to show the selection.
                SuggestionMouseAction::Scrolled
            }
            MouseEventKind::Down(_) | MouseEventKind::Drag(_) => {
                mouse_state.reset();
                SuggestionMouseAction::Ignored
            }
            // A release or ordinary pointer movement between clicks is expected.
            _ => SuggestionMouseAction::Ignored,
        }
    }

    pub fn item_at(&self, column: u16, row: u16, offset: usize) -> Option<usize> {
        if !self.contains(self.list_area, column, row) {
            return None;
        }

        let row_index = match self.direction {
            ListDirection::TopToBottom => row - self.list_area.y,
            ListDirection::BottomToTop => self.list_area.bottom() - 1 - row,
        };
        let index = offset.checked_add(usize::from(row_index))?;
        (index < self.item_count).then_some(index)
    }

    pub fn contains(&self, area: Rect, column: u16, row: u16) -> bool {
        column >= area.x && column < area.right() && row >= area.y && row < area.bottom()
    }
}

// Function to parse color to ratatui's Color object
pub fn parse_color(value: Option<&str>, fallback: Color) -> Color {
    value
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

// Function to pad prefix with spaces
pub fn padded_prefix(prefix: &str, width: usize) -> Cow<'_, str> {
    if width == 0 {
        return Cow::Borrowed(prefix);
    }
    let length = if prefix.is_ascii() {
        prefix.len()
    } else {
        prefix.chars().count()
    };
    if length >= width {
        Cow::Borrowed(prefix)
    } else {
        Cow::Owned(format!("{prefix:<width$}"))
    }
}

// Function turn texts to lowercase; keep unicode conversion for every non-ASCII string.
pub fn lowercase(text: &str) -> Cow<'_, str> {
    if text.is_ascii() && !text.bytes().any(|byte| byte.is_ascii_uppercase()) {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.to_lowercase())
    }
}

// Function to form hint mode suggestions
pub fn hint_text<'a>(
    input: &str,
    prefix: &'a str,
    description: &str,
    indicator: &str,
) -> Option<Cow<'a, str>> {
    if input.starts_with(' ') {
        return None;
    }

    let mut words = input.split_whitespace();
    let first_word = words.next()?;
    if words.next().is_some() {
        return None;
    }

    // Two trailing whitespace characters are enough to hide the hint.
    let mut tail = input.chars().rev();
    let trailing_space = tail.next().is_some_and(char::is_whitespace);
    if trailing_space && tail.next().is_some_and(char::is_whitespace) {
        return None;
    }

    let prefix_matches = if prefix.is_ascii() && first_word.is_ascii() {
        prefix
            .get(..first_word.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(first_word))
    } else {
        lowercase(prefix).starts_with(lowercase(first_word).as_ref())
    };

    let suffix = if prefix_matches {
        let start = prefix
            .char_indices()
            .nth(first_word.chars().count())
            .map_or(prefix.len(), |(index, _)| index);
        &prefix[start..]
    } else {
        ""
    };

    let hint = if description.is_empty() {
        Cow::Borrowed(suffix)
    } else if trailing_space {
        if !suffix.is_empty() {
            return None;
        }
        Cow::Owned(format!("{indicator}{description}"))
    } else {
        Cow::Owned(format!("{suffix} {indicator}{description}"))
    };

    (!hint.is_empty()).then_some(hint)
}

// Normalize immutable module text once and reuse fuzzy scoring buffers between input changes. The public matching helper below remains a one-shot wrapper.
pub struct SearchModule<'a> {
    pub module: &'a Module,
    pub prefix: Cow<'a, str>,
    pub description: Cow<'a, str>,
}

pub struct SuggestionMatcher<'a> {
    pub modules: Vec<SearchModule<'a>>,
    pub list_mode: bool,
    pub matcher: SkimMatcherV2,
    pub whole_input_matches: Vec<(i64, &'a Module)>,
    pub first_word_matches: Vec<(i64, &'a Module)>,
}

impl<'a> SuggestionMatcher<'a> {
    pub fn new(suggestions: &[&'a Module], list_mode: bool) -> Self {
        Self {
            modules: suggestions
                .iter()
                .copied()
                .map(|module| SearchModule {
                    module,
                    prefix: lowercase(&module.prefix),
                    description: if list_mode {
                        lowercase(&module.description)
                    } else {
                        Cow::Borrowed(module.description.as_str())
                    },
                })
                .collect(),
            list_mode,
            matcher: SkimMatcherV2::default(),
            whole_input_matches: Vec::new(),
            first_word_matches: Vec::new(),
        }
    }

    pub fn update(&mut self, textarea: &TextArea<'_>, matches: &mut Vec<&'a Module>) {
        matches.clear();
        self.whole_input_matches.clear();
        self.first_word_matches.clear();
        if self.modules.is_empty() {
            return;
        }

        let input: Cow<'_, str> = match textarea.lines() {
            [line] => Cow::Borrowed(line.as_str()),
            lines => Cow::Owned(lines.join(" ")),
        };
        let query = if self.list_mode {
            lowercase(input.as_ref())
        } else {
            lowercase(input.split_whitespace().next().unwrap_or(""))
        };
        let query_prefix = query.split_whitespace().next().unwrap_or("");
        if query_prefix.is_empty() {
            matches.extend(self.modules.iter().map(|entry| entry.module));
            return;
        }

        if !self.list_mode {
            matches.extend(
                self.modules
                    .iter()
                    .filter(|entry| entry.prefix.starts_with(query.as_ref()))
                    .map(|entry| entry.module),
            );
            return;
        }

        let query_is_first_word = query.as_ref() == query_prefix;
        for entry in &self.modules {
            if entry.prefix.starts_with(query_prefix) {
                matches.push(entry.module);
                continue;
            }

            let whole_score = self
                .matcher
                .fuzzy_match(entry.prefix.as_ref(), query.as_ref())
                .max(
                    self.matcher
                        .fuzzy_match(entry.description.as_ref(), query.as_ref()),
                );
            if let Some(score) = whole_score {
                self.whole_input_matches.push((score, entry.module));
                continue;
            }

            if !query_is_first_word {
                let first_word_score = self
                    .matcher
                    .fuzzy_match(entry.prefix.as_ref(), query_prefix)
                    .max(
                        self.matcher
                            .fuzzy_match(entry.description.as_ref(), query_prefix),
                    );
                if let Some(score) = first_word_score {
                    self.first_word_matches.push((score, entry.module));
                }
            }
        }

        // Stable sorting retains configured order when fuzzy scores tie.
        self.whole_input_matches.sort_by(|a, b| b.0.cmp(&a.0));
        self.first_word_matches.sort_by(|a, b| b.0.cmp(&a.0));
        matches.extend(self.whole_input_matches.iter().map(|&(_, module)| module));
        matches.extend(self.first_word_matches.iter().map(|&(_, module)| module));
    }
}

// Functions for item selection
pub fn normalize_selection(state: &mut ListState, item_count: usize) {
    if item_count == 0
        || state
            .selected()
            .map(|index| index >= item_count)
            .unwrap_or(false)
    {
        state.select(None);
    }
}

fn select_next(state: &mut ListState, item_count: usize) {
    if item_count == 0 {
        state.select(None);
        return;
    }

    match state.selected() {
        None => state.select(Some(0)),
        Some(index) if index + 1 < item_count => state.select(Some(index + 1)),
        Some(_) => state.select(None),
    }
}

fn select_previous(state: &mut ListState, item_count: usize) {
    if item_count == 0 {
        state.select(None);
        return;
    }

    match state.selected() {
        Some(0) => state.select(None),
        Some(index) => state.select(Some(index - 1)),
        None => state.select(Some(item_count - 1)),
    }
}

pub fn select_down(state: &mut ListState, item_count: usize, direction: ListDirection) {
    match direction {
        ListDirection::TopToBottom => select_next(state, item_count),
        ListDirection::BottomToTop => select_previous(state, item_count),
    }
}

pub fn select_up(state: &mut ListState, item_count: usize, direction: ListDirection) {
    match direction {
        ListDirection::TopToBottom => select_previous(state, item_count),
        ListDirection::BottomToTop => select_next(state, item_count),
    }
}

// Function for tab completion
pub fn complete_input(textarea: &mut TextArea<'_>, module: &Module) -> String {
    let original = textarea.lines().join("\n");
    let remaining = if module.with_argument.unwrap_or(false) {
        let input = original.trim_start();
        let first_word_end = input.find(char::is_whitespace).unwrap_or(input.len());
        &input[first_word_end..]
    } else {
        ""
    };

    let remaining = if remaining.is_empty() { " " } else { remaining };
    let completed = format!("{}{remaining}", module.prefix);
    replace_input(textarea, &completed);
    original
}

// Function to replace user input
pub fn replace_input(textarea: &mut TextArea<'_>, input: &str) {
    textarea.cancel_selection();
    textarea.select_all();
    textarea.cut();
    textarea.insert_str(input);
}
