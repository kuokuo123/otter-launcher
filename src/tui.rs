use crate::config::*;
use crate::vim::{Mode, Transition, Vim, edit_externally};

use ansi_to_tui::IntoText;
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use image::ImageReader;
use ratatui::{
    Frame, TerminalOptions, Viewport,
    crossterm::{
        event::{
            self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
            KeyModifiers, KeyboardEnhancementFlags, MouseButton, MouseEvent, MouseEventKind,
            PushKeyboardEnhancementFlags,
        },
        execute,
    },
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, List, ListDirection, ListItem, ListState, Padding, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
};
use ratatui_image::{
    Resize, StatefulImage,
    picker::{Picker, ProtocolType},
    protocol::StatefulProtocol,
};
use ratatui_textarea::{Input, Key, TextArea};
use std::{
    borrow::Cow,
    collections::HashMap,
    error::Error,
    io,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionMouseAction {
    Ignored,
    Selected,
    Scrolled,
    Activate,
    Complete,
}

#[derive(Debug, Default)]
pub struct SuggestionMouseState {
    last_click: Option<SuggestionClick>,
}

#[derive(Debug)]
struct SuggestionClick {
    index: usize,
    area: Rect,
    column: u16,
    row: u16,
    at: Instant,
}

impl SuggestionMouseState {
    // Cancel a pending double-click when a key or resize changes the interface.
    pub fn reset(&mut self) {
        self.last_click = None;
    }

    fn click(
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
            self.last_click = Some(SuggestionClick {
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

// Mouse interaction for the suggestion list rendered by the most recent draw. Store the value returned by `draw()` and forward `Event::Mouse` to `handle_mouse()` along with a persistent `SuggestionMouseState`. Enable Crossterm mouse capture while the TUI is active. Each suggestion in this interface occupies one terminal row.
#[derive(Debug, Clone, Copy)]
pub struct SuggestionMouse {
    // Includes the scrollbar, allowing wheel scrolling over either area.
    scroll_area: Rect,
    // Excludes borders, padding, and the scrollbar column for click selection.
    list_area: Rect,
    direction: ListDirection,
    item_count: usize,
}

impl SuggestionMouse {
    // Keep keyboard paging and Emacs-style paging on the same state transition.
    fn page(&self, state: &mut ListState, forward: bool) {
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

    fn halfpage(&self, state: &mut ListState, forward: bool) {
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

    fn handle_mouse_at(
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

    fn item_at(&self, column: u16, row: u16, offset: usize) -> Option<usize> {
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

    fn contains(&self, area: Rect, column: u16, row: u16) -> bool {
        column >= area.x && column < area.right() && row >= area.y && row < area.bottom()
    }
}

// Function to launch the TUI interface
pub fn tui_loop(suggestions: &[&Module], header: &str) -> Result<Option<String>, Box<dyn Error>> {
    // Prepare config, image, and other variables
    let cfg = config();
    let mut image: Option<StatefulProtocol> = match cfg
        .image
        .path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
    {
        Some(image_path) => {
            let expanded_path = shellexpand::full_with_context_no_errors(
                image_path,
                || std::env::var("HOME").ok(),
                |name| std::env::var(name).ok(),
            );
            let source = ImageReader::open(expanded_path.as_ref())?
                .with_guessed_format()?
                .decode()?;

            let mut picker = Picker::from_query_stdio().map_err(io::Error::other)?;
            picker.set_protocol_type(match cfg.image.format.as_deref().unwrap() {
                "kitty" => ProtocolType::Kitty,
                "sixel" => ProtocolType::Sixel,
                "iterm2" => ProtocolType::Iterm2,
                _ => ProtocolType::Halfblocks,
            });

            Some(picker.new_resize_protocol(source))
        }
        None => None,
    };
    let vi_mode = cfg.general.vi_mode.unwrap();
    let suggestion_mode = cfg.interface.suggestion_mode.as_deref().unwrap();
    let list_mode = suggestion_mode == "list";
    let mut render_cache = RenderCache::new(header, suggestions);
    let mut suggestion_matcher = SuggestionMatcher::new(suggestions, list_mode);

    // Prepare editor settings
    let mut vim = Vim::new(Mode::Insert);
    let mut external_edit_pending = false;
    let mut tab_original: Option<String> = None;
    let mut suggestion_state = ListState::default();
    let mut suggestion_mouse_state = SuggestionMouseState::default();
    let suggestion_direction = if cfg.interface.suggestion_position.as_deref().unwrap() == "top" {
        ListDirection::BottomToTop
    } else {
        ListDirection::TopToBottom
    };

    // Prepare the editor
    let mut textarea = TextArea::default();
    textarea.set_cursor_line_style(Style::default());
    textarea.set_block(Mode::Insert.block());
    textarea.set_cursor_style(Mode::Insert.cursor_style());
    textarea.set_selection_style(Mode::Insert.cursor_style());

    // Prepare viewport
    let mut terminal = ratatui::try_init_with_options(TerminalOptions {
        viewport: Viewport::Inline(cfg.general.height.unwrap()),
    })?;
    let mut viewport_bottom = 0;
    let mut matched_input: Vec<String> = Vec::new();
    let mut visible_suggestions = Vec::with_capacity(suggestions.len());
    let mut suggestion_mouse = None;
    let mut redraw = true;

    // The main loop
    let run_result = (|| -> io::Result<Option<String>> {
        execute!(
            io::stdout(),
            EnableMouseCapture,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;

        loop {
            // Comparing the lines also catches edits made by Vim or an external editor.
            if textarea.lines() != matched_input.as_slice() {
                suggestion_matcher.update(&textarea, &mut visible_suggestions);
                redraw = true;
                matched_input.resize_with(textarea.lines().len(), String::new);
                for (cached, line) in matched_input.iter_mut().zip(textarea.lines()) {
                    cached.clone_from(line);
                }
            }

            normalize_selection(&mut suggestion_state, visible_suggestions.len());
            // Ignore releases and pointer movement without rebuilding the frame.
            if redraw {
                terminal.draw(|frame| {
                    viewport_bottom = frame.area().bottom();
                    suggestion_mouse = draw_cached(
                        frame,
                        image.as_mut(),
                        &textarea,
                        &visible_suggestions,
                        &mut suggestion_state,
                        &mut render_cache,
                    );
                })?;
            }
            redraw = true;

            let external_editor = std::mem::take(&mut external_edit_pending);

            // Keypress loop
            let key = match event::read()? {
                Event::Key(key) => {
                    suggestion_mouse_state.reset();
                    key
                }
                Event::Mouse(mouse) => {
                    let Some(view) = suggestion_mouse.as_ref() else {
                        suggestion_mouse_state.reset();
                        redraw = false;
                        continue;
                    };

                    match view.handle_mouse(
                        mouse,
                        &mut suggestion_state,
                        &mut suggestion_mouse_state,
                    ) {
                        SuggestionMouseAction::Activate => {
                            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
                        }
                        SuggestionMouseAction::Complete => {
                            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)
                        }
                        SuggestionMouseAction::Selected | SuggestionMouseAction::Scrolled => {
                            tab_original = None;
                            continue;
                        }
                        SuggestionMouseAction::Ignored => {
                            redraw = false;
                            continue;
                        }
                    }
                }
                Event::Resize(..) => {
                    suggestion_mouse_state.reset();
                    continue;
                }
                Event::FocusGained => continue,
                _ => {
                    redraw = false;
                    continue;
                }
            };

            if key.kind != KeyEventKind::Press {
                redraw = false;
                continue;
            }

            let control = key.modifiers.contains(KeyModifiers::CONTROL);
            let alt = key.modifiers.contains(KeyModifiers::ALT);
            let line_len = textarea.lines().len();
            let menu_normal =
                vi_mode && vim.mode == Mode::Normal && line_len <= 1 && list_mode && !alt;

            match (key.code, control) {
                (KeyCode::Char('f' | 'b' | 'd' | 'u'), true) if menu_normal => {
                    let Some(view) = suggestion_mouse.as_ref() else {
                        continue;
                    };

                    match key.code {
                        KeyCode::Char('f') => {
                            let forward = match view.direction {
                                ListDirection::TopToBottom => true,
                                _ => false,
                            };
                            view.page(&mut suggestion_state, forward);
                            continue;
                        }
                        KeyCode::Char('b') => {
                            let forward = match view.direction {
                                ListDirection::TopToBottom => false,
                                _ => true,
                            };
                            view.page(&mut suggestion_state, forward);
                            continue;
                        }
                        KeyCode::Char('d') => {
                            let forward = match view.direction {
                                ListDirection::TopToBottom => true,
                                _ => false,
                            };
                            view.halfpage(&mut suggestion_state, forward);
                            continue;
                        }
                        KeyCode::Char('u') => {
                            let forward = match view.direction {
                                ListDirection::TopToBottom => false,
                                _ => true,
                            };
                            view.halfpage(&mut suggestion_state, forward);
                            continue;
                        }
                        _ => {}
                    }

                    vim.reset_command();
                    tab_original = None;
                    external_edit_pending = false;
                    continue;
                }
                (KeyCode::Char('j' | 'k' | 'g' | 'G'), false)
                    if menu_normal
                        && (key.code != KeyCode::Char('g')
                            || vim.pending.key == Key::Char('g')) =>
                {
                    let Some(view) = suggestion_mouse.as_ref() else {
                        continue;
                    };
                    match key.code {
                        KeyCode::Char('j') => select_down(
                            &mut suggestion_state,
                            visible_suggestions.len(),
                            suggestion_direction,
                        ),
                        KeyCode::Char('k') => select_up(
                            &mut suggestion_state,
                            visible_suggestions.len(),
                            suggestion_direction,
                        ),
                        KeyCode::Char('g' | 'G') => {
                            let last = (key.code == KeyCode::Char('G'))
                                == (view.direction == ListDirection::TopToBottom);
                            suggestion_state.select(if last {
                                visible_suggestions.len().checked_sub(1)
                            } else {
                                None
                            });
                        }
                        _ => {}
                    }

                    vim.reset_command();
                    tab_original = None;
                    external_edit_pending = false;
                    continue;
                }
                (KeyCode::Char('u'), true) if !vi_mode => {
                    textarea.delete_line_by_head();
                    continue;
                }
                (KeyCode::Char('e'), true) if !vi_mode && external_editor => {
                    edit_externally(&mut terminal, &mut textarea)?;
                    continue;
                }
                (KeyCode::Esc, _) if !vi_mode => return Ok(None),
                (KeyCode::Char('d'), true) => {
                    if !vi_mode {
                        return Ok(None);
                    } else {
                        continue;
                    }
                }
                (KeyCode::Char('c'), true) => {
                    if !vi_mode {
                        textarea.copy();
                        vim.copy_to_clipboard(&textarea);
                        continue;
                    } else {
                        return Ok(None);
                    }
                }
                (KeyCode::Char('x' | 'X'), true) => {
                    if !vi_mode {
                        external_edit_pending = true;
                    }
                    textarea.cut();
                    vim.copy_to_clipboard(&textarea);
                    continue;
                }
                (KeyCode::Up | KeyCode::Char('k' | 'p'), true) | (KeyCode::Up, false)
                    if list_mode && (control || line_len <= 1) =>
                {
                    select_up(
                        &mut suggestion_state,
                        visible_suggestions.len(),
                        suggestion_direction,
                    );
                    continue;
                }
                (KeyCode::Down | KeyCode::Char('j' | 'n'), true) | (KeyCode::Down, false)
                    if list_mode && (control || line_len <= 1) =>
                {
                    select_down(
                        &mut suggestion_state,
                        visible_suggestions.len(),
                        suggestion_direction,
                    );
                    continue;
                }
                (KeyCode::Char('v'), _) if list_mode && !vi_mode && line_len <= 1 => {
                    let Some(view) = suggestion_mouse.as_ref() else {
                        continue;
                    };

                    let forward = matches!(
                        (key.code, view.direction, alt, control),
                        (KeyCode::Char('v'), ListDirection::TopToBottom, false, true)
                            | (KeyCode::Char('v'), ListDirection::BottomToTop, true, _)
                    );
                    view.page(&mut suggestion_state, forward);
                    tab_original = None;
                    external_edit_pending = false;
                    continue;
                }
                (KeyCode::PageUp | KeyCode::PageDown, _) if list_mode && line_len <= 1 => {
                    let Some(view) = suggestion_mouse.as_ref() else {
                        continue;
                    };

                    let forward = matches!(
                        (key.code, view.direction),
                        (KeyCode::PageDown, ListDirection::TopToBottom)
                            | (KeyCode::PageUp, ListDirection::BottomToTop)
                    );
                    view.page(&mut suggestion_state, forward);
                    tab_original = None;
                    external_edit_pending = false;
                    continue;
                }

                (KeyCode::Tab, _) => {
                    if let Some(original) = tab_original.take() {
                        replace_input(&mut textarea, &original);
                        suggestion_state.select(None);
                        continue;
                    }

                    let module = suggestion_state
                        .selected()
                        .and_then(|index| visible_suggestions.get(index))
                        .or_else(|| visible_suggestions.first())
                        .copied();

                    if let Some(module) = module {
                        tab_original = Some(complete_input(&mut textarea, module));
                        suggestion_state.select(None);
                    }
                    continue;
                }
                (KeyCode::Enter, _) => {
                    if key.modifiers.contains(KeyModifiers::ALT) {
                        let prefix = cfg.general.alternative_module.as_deref().unwrap_or("");
                        return Ok(Some(format!("{prefix} {}", textarea.lines().join("\n"))));
                    }
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        let prefix = cfg.general.control_module.as_deref().unwrap_or("");
                        return Ok(Some(format!("{prefix} {}", textarea.lines().join("\n"))));
                    }
                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                        textarea.insert_newline();
                        continue;
                    }

                    let module = suggestion_state
                        .selected()
                        .and_then(|index| visible_suggestions.get(index))
                        .copied();

                    match module {
                        None => return Ok(Some(textarea.lines().join("\n"))),
                        Some(module) if module.with_argument.unwrap_or(false) => {
                            tab_original = Some(complete_input(&mut textarea, module));
                            suggestion_state.select(None);
                            continue;
                        }
                        Some(module) => return Ok(Some(module.prefix.clone())),
                    }
                }
                _ => {}
            }

            let input: Input = key.into();
            if !vi_mode {
                textarea.input(input);
                continue;
            }

            vim = match vim.transition(input, &mut textarea) {
                Transition::Mode(mode) if vim.mode != mode => {
                    textarea.set_block(mode.block());
                    textarea.set_cursor_style(mode.cursor_style());
                    vim.with_mode(mode)
                }
                Transition::EditExternally => {
                    edit_externally(&mut terminal, &mut textarea)?;
                    vim
                }
                Transition::Pending(input) => vim.with_pending(input),
                Transition::Nop | Transition::Mode(_) => vim,
                Transition::Quit => return Ok(None),
            };
        }
    })();

    // Attempt all cleanup steps before returning any error.
    let cursor_result = if viewport_bottom > 0 {
        terminal.set_cursor_position((0, viewport_bottom))
    } else {
        Ok(())
    };
    let mouse_result = execute!(io::stdout(), DisableMouseCapture);
    let restore_result = ratatui::try_restore();

    cursor_result?;
    mouse_result?;
    restore_result?;
    Ok(run_result?)
}

// Configured text and styles stay fixed for one TUI session. Keep owned ANSI text here and borrow its spans while building each frame's list items.
struct RenderCache<'a> {
    header: Paragraph<'static>,
    header_height: u16,
    prompt: Paragraph<'static>,
    prompt_width: u16,
    list: Option<ListRenderCache<'a>>,
    separator: Option<(u16, Paragraph<'static>)>,
}

struct ListRenderCache<'a> {
    selection_prefix: Vec<Span<'static>>,
    list_prefix: Vec<Span<'static>>,
    padded_prefixes: HashMap<&'a str, String>,
    prefix_padding: usize,
    prefix_style: Style,
    description_style: Style,
    indicator_style: Style,
    highlight_style: Style,
}

impl<'a> RenderCache<'a> {
    fn new(header: &str, suggestions: &[&'a Module]) -> Self {
        let cfg = config();
        let header_text = header.into_text().expect("failed to parse header ANSI");
        let header_height = if header.is_empty() {
            0
        } else {
            u16::try_from(header_text.lines.len()).unwrap_or(0)
        };
        let prompt_text = cfg
            .interface
            .prompt_sign
            .as_deref()
            .unwrap()
            .into_text()
            .expect("failed to parse prompt ANSI");
        let prompt_width = u16::try_from(prompt_text.width()).unwrap_or(u16::MAX);

        let list = (cfg.interface.suggestion_mode.as_deref() != Some("hint")).then(|| {
            let prefix_padding = cfg.interface.prefix_padding.unwrap();
            ListRenderCache {
                selection_prefix: marker_spans(
                    cfg.interface.selection_prefix.as_deref().unwrap(),
                    Style::default().fg(parse_color(
                        cfg.interface.selection_prefix_color.as_deref(),
                        Color::Red,
                    )),
                ),
                list_prefix: marker_spans(
                    cfg.interface.list_prefix.as_deref().unwrap(),
                    Style::default().fg(parse_color(
                        cfg.interface.list_prefix_color.as_deref(),
                        Color::Reset,
                    )),
                ),
                padded_prefixes: suggestions
                    .iter()
                    .copied()
                    .filter_map(|module| {
                        let prefix = module.prefix.as_str();
                        match padded_prefix(prefix, prefix_padding) {
                            Cow::Owned(padded) => Some((prefix, padded)),
                            Cow::Borrowed(_) => None,
                        }
                    })
                    .collect(),
                prefix_padding,
                prefix_style: Style::default().fg(parse_color(
                    cfg.interface.prefix_color.as_deref(),
                    Color::Yellow,
                )),
                description_style: Style::default().fg(parse_color(
                    cfg.interface.description_color.as_deref(),
                    Color::Reset,
                )),
                indicator_style: Style::default().fg(parse_color(
                    cfg.interface.indicator_color.as_deref(),
                    Color::Red,
                )),
                highlight_style: if cfg.interface.inverse_selection.unwrap() {
                    Style::default()
                        .bg(Color::Black)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().add_modifier(Modifier::BOLD)
                },
            }
        });

        Self {
            header: Paragraph::new(header_text).style(
                Style::default()
                    .fg(parse_color(
                        cfg.interface.header_color.as_deref(),
                        Color::Blue,
                    ))
                    .add_modifier(Modifier::BOLD),
            ),
            header_height,
            prompt: Paragraph::new(prompt_text).style(
                Style::default()
                    .fg(parse_color(
                        cfg.interface.prompt_sign_color.as_deref(),
                        Color::Blue,
                    ))
                    .add_modifier(Modifier::BOLD),
            ),
            prompt_width,
            list,
            separator: None,
        }
    }

    fn draw_separator(&mut self, frame: &mut Frame, area: Rect) {
        if self.separator.as_ref().map(|(width, _)| *width) != Some(area.width) {
            let cfg = config();
            let label = cfg.interface.separator_label.as_deref().unwrap();
            let width = usize::from(area.width.saturating_sub(1));
            self.separator = Some((
                area.width,
                Paragraph::new(format!(
                    "{label}{}",
                    "─".repeat(width.saturating_sub(label.len())),
                ))
                .style(Style::default().fg(Color::DarkGray)),
            ));
        }
        if let Some((_, separator)) = &self.separator {
            frame.render_widget(separator, area);
        }
    }
}

fn marker_spans(marker: &str, style: Style) -> Vec<Span<'static>> {
    let mut spans = marker
        .into_text()
        .expect("failed to parse marker ANSI")
        .lines
        .into_iter()
        .next()
        .map(|line| line.spans)
        .unwrap_or_default();
    for span in &mut spans {
        span.style = style.patch(span.style);
    }
    spans
}

fn draw_cached(
    frame: &mut Frame,
    image: Option<&mut StatefulProtocol>,
    textarea: &TextArea<'_>,
    suggestions: &[&Module],
    suggestion_state: &mut ListState,
    cache: &mut RenderCache<'_>,
) -> Option<SuggestionMouse> {
    // Prepare variables from config
    let cfg = config();
    let has_image = image.is_some();
    let image_position = cfg.image.position.as_deref().unwrap();
    let image_width = cfg.image.width.unwrap_or(0);
    let suggestion_position = cfg.interface.suggestion_position.as_deref().unwrap();

    let suggestion_mode = cfg.interface.suggestion_mode.as_deref().unwrap();
    let separator = cfg.interface.separator.unwrap();
    let indicator_with_arg = cfg.interface.indicator_args.as_deref().unwrap();
    let indicator_no_arg = cfg.interface.indicator_bare.as_deref().unwrap();

    let [
        iface_pad_top,
        iface_pad_right,
        iface_pad_bottom,
        iface_pad_left,
    ] = cfg.interface.padding.unwrap();

    // Ratatui frame setup
    let (image_area, right) = if !has_image {
        // Preserve the existing margins when there is no image.
        let [_margin_left, content_area, _margin_right] = Layout::horizontal([
            Constraint::Length(iface_pad_left),
            Constraint::Fill(1),
            Constraint::Length(iface_pad_right),
        ])
        .areas(frame.area());

        let [_margin_top, content, _margin_bottom] = Layout::vertical([
            Constraint::Length(iface_pad_top),
            Constraint::Fill(1),
            Constraint::Length(iface_pad_bottom),
        ])
        .areas(content_area);

        (Rect::default(), content)
    } else {
        match image_position {
            "top" | "bottom" => {
                let constraints = if image_position == "top" {
                    [
                        Constraint::Length(image_width),
                        Constraint::Length(iface_pad_top),
                        Constraint::Fill(1),
                        Constraint::Length(iface_pad_bottom),
                    ]
                } else {
                    [
                        Constraint::Length(iface_pad_top),
                        Constraint::Fill(1),
                        Constraint::Length(iface_pad_bottom),
                        Constraint::Length(image_width),
                    ]
                };

                let [first, second, third, last] =
                    Layout::vertical(constraints).areas(frame.area());

                let (image_area, content_area) = if image_position == "top" {
                    (first, third)
                } else {
                    (last, second)
                };

                let [_margin_left, content, _margin_right] = Layout::horizontal([
                    Constraint::Length(iface_pad_left),
                    Constraint::Fill(1),
                    Constraint::Length(iface_pad_right),
                ])
                .areas(content_area);

                (image_area, content)
            }

            // Horizontal images: "left", "right", or an unrecognized position.
            _ => {
                let image_on_right = image_position == "right";
                let constraints = if image_on_right {
                    [
                        Constraint::Length(iface_pad_left),
                        Constraint::Fill(1),
                        Constraint::Length(iface_pad_right),
                        Constraint::Length(image_width),
                    ]
                } else {
                    [
                        Constraint::Length(image_width),
                        Constraint::Length(iface_pad_left),
                        Constraint::Fill(1),
                        Constraint::Length(iface_pad_right),
                    ]
                };
                let [first, second, third, last] =
                    Layout::horizontal(constraints).areas(frame.area());
                let (image_area, content_area) = if image_on_right {
                    (last, second)
                } else {
                    (first, third)
                };

                let [_margin_top, content, _margin_bottom] = Layout::vertical([
                    Constraint::Length(iface_pad_top),
                    Constraint::Fill(1),
                    Constraint::Length(iface_pad_bottom),
                ])
                .areas(content_area);

                (image_area, content)
            }
        }
    };

    // Render image
    if let Some(image) = image {
        let [top, right, bottom, left] = cfg.image.padding.unwrap();
        let inner_area = Block::default()
            .padding(Padding::new(left, right, top, bottom))
            .inner(image_area);

        frame.render_stateful_widget(
            StatefulImage::default().resize(Resize::Scale(None)),
            inner_area,
            image,
        );
    }

    let header_height = cache.header_height;

    // Form input field, separator area, and suggestion menu
    let content_height = u16::try_from(textarea.lines().len()).unwrap_or(u16::MAX);

    let max_input_height = right
        .height
        .saturating_sub(header_height)
        .saturating_sub(1)
        .max(3);

    let input_height = content_height.clamp(1, max_input_height);

    let suggestions_below_input = suggestion_position == "bottom";
    let separator_height = u16::from(separator && suggestion_mode == "list");

    let (suggestions_area, separator_area, header_area, input_area) = if suggestions_below_input {
        let [header_area, input_area, separator_area, suggestions_area] = Layout::vertical([
            Constraint::Length(header_height),
            Constraint::Length(input_height),
            Constraint::Length(separator_height),
            Constraint::Fill(1),
        ])
        .areas(right);

        (suggestions_area, separator_area, header_area, input_area)
    } else {
        let [suggestions_area, separator_area, header_area, input_area] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(separator_height),
            Constraint::Length(header_height),
            Constraint::Length(input_height),
        ])
        .areas(right);

        (suggestions_area, separator_area, header_area, input_area)
    };

    if !separator_area.is_empty() {
        cache.draw_separator(frame, separator_area);
    }

    frame.render_widget(&cache.header, header_area);

    let [prefix_area, textarea_area] =
        Layout::horizontal([Constraint::Length(cache.prompt_width), Constraint::Fill(1)])
            .areas(input_area);

    frame.render_widget(&cache.prompt, prefix_area);

    frame.render_widget(textarea, textarea_area);

    // Render placeholder
    if textarea.is_empty() {
        let inner = textarea
            .block()
            .map(|block| block.inner(textarea_area))
            .unwrap_or(textarea_area);

        if !inner.is_empty() {
            let placeholder_style = Style::default().fg(Color::Black).bg(Color::Reset);
            let first_char_style = Style::default()
                .bg(Color::Reset)
                .add_modifier(Modifier::REVERSED);
            let buffer = frame.buffer_mut();

            buffer.set_stringn(
                inner.x,
                inner.y,
                cfg.interface.placeholder.as_deref().unwrap(),
                usize::from(inner.width),
                placeholder_style,
            );

            // Apply the current cursor style to the first character.
            buffer[(inner.x, inner.y)].set_style(textarea.cursor_style().patch(first_char_style));
        }
    }

    // Draw suggestion area differently when hint mode is in use
    if suggestion_mode == "hint" {
        draw_hint_mode(
            frame,
            textarea,
            textarea_area,
            suggestions.first().copied(),
            indicator_with_arg,
            indicator_no_arg,
        );
        return None;
    }

    let Some(list) = cache.list.as_ref() else {
        return None;
    };
    let selected_index = suggestion_state.selected();

    // Form the suggestion items
    let items = suggestions.iter().enumerate().map(|(index, module)| {
        let selected = selected_index == Some(index);

        let indicator = if module.with_argument.unwrap() {
            indicator_with_arg
        } else {
            indicator_no_arg
        };

        let marker = if selected {
            &list.selection_prefix
        } else {
            &list.list_prefix
        };

        // Borrow cached ANSI text instead of cloning its owned strings per row.
        let mut spans = Vec::with_capacity(marker.len() + 4);
        spans.extend(
            marker
                .iter()
                .map(|span| Span::styled(span.content.as_ref(), span.style)),
        );
        let raw_prefix = module.prefix.as_str();
        let prefix = if list.prefix_padding == 0
            || (raw_prefix.is_ascii() && raw_prefix.len() >= list.prefix_padding)
        {
            Cow::Borrowed(raw_prefix)
        } else {
            list.padded_prefixes
                .get(raw_prefix)
                .map(|prefix| Cow::Borrowed(prefix.as_str()))
                .unwrap_or_else(|| padded_prefix(raw_prefix, list.prefix_padding))
        };

        spans.extend([
            Span::styled(prefix, list.prefix_style),
            Span::raw(" "),
            Span::styled(indicator, list.indicator_style),
            Span::styled(module.description.as_str(), list.description_style),
        ]);

        ListItem::new(Line::from(spans))
    });

    let inner = suggestions_area;
    let visible_rows = usize::from(inner.height);

    // Hide scrollbar if disabled
    let show_scrollbar = visible_rows > 0
        && inner.width > 1
        && suggestions.len() > visible_rows
        && cfg.interface.scrollbar.unwrap();

    let list_area = Rect {
        width: inner.width.saturating_sub(u16::from(show_scrollbar)),
        ..inner
    };

    let suggestion_direction = if suggestion_position == "top" {
        ListDirection::BottomToTop
    } else {
        ListDirection::TopToBottom
    };
    let suggestion_list = List::new(items)
        .direction(suggestion_direction)
        .highlight_style(list.highlight_style);

    frame.render_stateful_widget(suggestion_list, list_area, suggestion_state);

    if show_scrollbar {
        // Read the offset after rendering: List updates it to keep selection visible.
        let max_offset = suggestions.len() - visible_rows;
        let offset = suggestion_state.offset().min(max_offset);

        // Reverse the handle's movement for a bottom-to-top list.
        let position = if suggestion_direction == ListDirection::BottomToTop {
            max_offset - offset
        } else {
            offset
        };

        let scrollbar_area = Rect {
            x: inner.right() - 1,
            y: inner.y,
            width: 1,
            height: inner.height,
        };

        // Number of possible viewport positions, including both endpoints.
        let mut scrollbar_state = ScrollbarState::new(max_offset + 1)
            .position(position)
            .viewport_content_length(visible_rows);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(None)
            .thumb_symbol("│")
            .thumb_style(Style::default().fg(Color::DarkGray));

        frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
    }

    // These are absolute terminal coordinates, including the inline viewport's origin. Recreate them on every draw so terminal resizes stay accurate.
    if list_area.is_empty() || suggestions.is_empty() {
        None
    } else {
        Some(SuggestionMouse {
            scroll_area: inner,
            list_area,
            direction: suggestion_direction,
            item_count: suggestions.len(),
        })
    }
}

// Function to parse color to the Color object
fn parse_color(value: Option<&str>, fallback: Color) -> Color {
    value
        .and_then(|value| value.parse().ok())
        .unwrap_or(fallback)
}

// Function to pad prefix with spaces
fn padded_prefix(prefix: &str, width: usize) -> Cow<'_, str> {
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
fn lowercase(text: &str) -> Cow<'_, str> {
    if text.is_ascii() && !text.bytes().any(|byte| byte.is_ascii_uppercase()) {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.to_lowercase())
    }
}

// Function to form hint mode suggestions
fn hint_text<'a>(
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

// Function to render hint mode input field
fn draw_hint_mode(
    frame: &mut Frame,
    textarea: &TextArea<'_>,
    textarea_area: Rect,
    closest: Option<&Module>,
    indicator_with_arg: &str,
    indicator_no_arg: &str,
) {
    let Some(module) = closest else {
        return;
    };
    let [input] = textarea.lines() else {
        return;
    };

    // Reject hidden hints before measuring the input or reading module options.
    if input.starts_with(' ') || input.split_whitespace().take(2).count() != 1 {
        return;
    }

    let inner = textarea
        .block()
        .map(|block| block.inner(textarea_area))
        .unwrap_or(textarea_area);
    let input_width = Span::raw(input.as_str()).width();
    if inner.is_empty() || input_width >= usize::from(inner.width) {
        return;
    }

    let indicator = if module.with_argument.unwrap() {
        indicator_with_arg
    } else {
        indicator_no_arg
    };
    let Some(hint) = hint_text(input, &module.prefix, &module.description, indicator) else {
        return;
    };

    frame.buffer_mut().set_stringn(
        inner.x + input_width as u16,
        inner.y,
        hint.as_ref(),
        usize::from(inner.width) - input_width,
        Style::default().fg(Color::DarkGray),
    );
}

// Normalize immutable module text once and reuse fuzzy scoring buffers between input changes. The public matching helper below remains a one-shot wrapper.
struct SearchModule<'a> {
    module: &'a Module,
    prefix: Cow<'a, str>,
    description: Cow<'a, str>,
}

struct SuggestionMatcher<'a> {
    modules: Vec<SearchModule<'a>>,
    list_mode: bool,
    matcher: SkimMatcherV2,
    whole_input_matches: Vec<(i64, &'a Module)>,
    first_word_matches: Vec<(i64, &'a Module)>,
}

impl<'a> SuggestionMatcher<'a> {
    fn new(suggestions: &[&'a Module], list_mode: bool) -> Self {
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

    fn update(&mut self, textarea: &TextArea<'_>, matches: &mut Vec<&'a Module>) {
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

#[cfg(test)]
mod hint_tests {
    use super::*;

    #[test]
    fn spaces_and_partial_prefixes_keep_the_existing_hint_behavior() {
        let cases = [
            ("s", Some("h >run bash command")),
            ("sh", Some(" >run bash command")),
            ("SH", Some(" >run bash command")),
            ("sh ", Some(">run bash command")),
            ("s ", None),
            ("sh  ", None),
            ("sh   ", None),
            ("sh argument", None),
            (" sh", None),
            ("", None),
            ("   ", None),
        ];

        for (input, expected) in cases {
            assert_eq!(
                hint_text(input, "sh", "run bash command", ">").as_deref(),
                expected,
                "input: {input:?}",
            );
        }
    }

    #[test]
    fn empty_descriptions_keep_prefix_only_hints() {
        assert_eq!(hint_text("s", "sh", "", ">").as_deref(), Some("h"));
        assert_eq!(hint_text("s ", "sh", "", ">").as_deref(), Some("h"));
        assert_eq!(hint_text("sh", "sh", "", ">").as_deref(), None);
        assert_eq!(hint_text(" sh", "sh", "", ">").as_deref(), None);
    }

    #[test]
    fn unicode_suffixes_are_cut_at_character_boundaries() {
        assert_eq!(
            hint_text("開", "開啟", "browser", "").as_deref(),
            Some("啟 browser"),
        );
        assert_eq!(hint_text("i", "İx", "info", "").as_deref(), Some("x info"),);
        assert_eq!(
            hint_text("開啟\u{3000}", "開啟", "browser", "").as_deref(),
            Some("browser"),
        );
        assert_eq!(hint_text("sh\t ", "sh", "run", "").as_deref(), None);
    }
}

#[cfg(test)]
mod suggestion_mouse_tests {
    use super::*;
    use ratatui::{buffer::Buffer, crossterm::event::KeyModifiers, widgets::StatefulWidget};

    fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::empty(),
        }
    }

    fn view(direction: ListDirection, item_count: usize) -> SuggestionMouse {
        SuggestionMouse {
            scroll_area: Rect::new(7, 20, 11, 4),
            list_area: Rect::new(7, 20, 10, 4),
            direction,
            item_count,
        }
    }

    fn render(view: &SuggestionMouse, state: &mut ListState) -> Buffer {
        let mut buffer = Buffer::empty(view.scroll_area);
        let list = List::new((0..view.item_count).map(|index| index.to_string()))
            .direction(view.direction);
        StatefulWidget::render(list, view.list_area, &mut buffer, state);
        buffer
    }

    fn wheel_kinds(direction: ListDirection) -> (MouseEventKind, MouseEventKind) {
        if direction == ListDirection::TopToBottom {
            (MouseEventKind::ScrollDown, MouseEventKind::ScrollUp)
        } else {
            (MouseEventKind::ScrollUp, MouseEventKind::ScrollDown)
        }
    }

    #[test]
    fn paging_keeps_endpoint_deselection_and_none_to_page_navigation() {
        for direction in [ListDirection::TopToBottom, ListDirection::BottomToTop] {
            let view = view(direction, 12);
            let mut state = ListState::default();
            for (forward, selected, offset) in [
                (true, Some(4), 4),
                (true, Some(8), 8),
                (true, Some(11), 8),
                (true, None, 0),
                (false, Some(8), 8),
                (false, Some(4), 4),
                (false, Some(0), 0),
                (false, None, 0),
            ] {
                view.page(&mut state, forward);
                render(&view, &mut state);
                assert_eq!(state.selected(), selected);
                assert_eq!(state.offset(), offset);
            }
        }
    }

    #[test]
    fn paging_to_a_partial_last_page_selects_its_first_item() {
        let view = view(ListDirection::TopToBottom, 10);
        let mut state = ListState::default();
        view.page(&mut state, false);
        render(&view, &mut state);
        assert_eq!(state.selected(), Some(6));
        assert_eq!(state.offset(), 6);
    }

    #[test]
    fn a_single_click_selects_the_item_rendered_at_the_clicked_row() {
        for direction in [ListDirection::TopToBottom, ListDirection::BottomToTop] {
            let view = view(direction, 9);
            let mut state = ListState::default();
            let mut mouse_state = SuggestionMouseState::default();
            *state.offset_mut() = 2;
            let buffer = render(&view, &mut state);

            for row in 20..24 {
                let displayed_index = buffer[(7, row)].symbol().parse::<usize>().unwrap();
                assert_eq!(
                    view.handle_mouse(
                        mouse(MouseEventKind::Down(MouseButton::Left), 7, row),
                        &mut state,
                        &mut mouse_state,
                    ),
                    SuggestionMouseAction::Selected,
                );
                assert_eq!(state.selected(), Some(displayed_index));
            }
        }
    }

    #[test]
    fn a_double_click_activates_once_despite_release_and_movement_events() {
        for direction in [ListDirection::TopToBottom, ListDirection::BottomToTop] {
            let view = view(direction, 9);
            let mut state = ListState::default();
            let mut mouse_state = SuggestionMouseState::default();
            let now = Instant::now();
            *state.offset_mut() = 2;
            let buffer = render(&view, &mut state);
            let clicked_index = buffer[(7, 20)].symbol().parse::<usize>().unwrap();
            let down = mouse(MouseEventKind::Down(MouseButton::Left), 7, 20);

            assert_eq!(
                view.handle_mouse_at(down, &mut state, &mut mouse_state, now),
                SuggestionMouseAction::Selected,
            );
            for kind in [MouseEventKind::Up(MouseButton::Left), MouseEventKind::Moved] {
                assert_eq!(
                    view.handle_mouse_at(
                        mouse(kind, 7, 20),
                        &mut state,
                        &mut mouse_state,
                        now + Duration::from_millis(50),
                    ),
                    SuggestionMouseAction::Ignored,
                );
            }
            assert_eq!(
                view.handle_mouse_at(
                    down,
                    &mut state,
                    &mut mouse_state,
                    now + Duration::from_millis(100),
                ),
                SuggestionMouseAction::Activate,
            );
            assert_eq!(state.selected(), Some(clicked_index));
            assert_eq!(
                view.handle_mouse_at(
                    down,
                    &mut state,
                    &mut mouse_state,
                    now + Duration::from_millis(150),
                ),
                SuggestionMouseAction::Selected,
            );
        }
    }

    #[test]
    fn distant_slow_and_different_item_clicks_do_not_activate() {
        let view = view(ListDirection::TopToBottom, 9);
        let now = Instant::now();
        for (column, row, delay_ms) in [(7, 20, 401), (12, 20, 100), (7, 21, 100)] {
            let mut state = ListState::default();
            let mut mouse_state = SuggestionMouseState::default();
            view.handle_mouse_at(
                mouse(MouseEventKind::Down(MouseButton::Left), 7, 20),
                &mut state,
                &mut mouse_state,
                now,
            );
            assert_eq!(
                view.handle_mouse_at(
                    mouse(MouseEventKind::Down(MouseButton::Left), column, row),
                    &mut state,
                    &mut mouse_state,
                    now + Duration::from_millis(delay_ms),
                ),
                SuggestionMouseAction::Selected,
            );
        }
    }

    #[test]
    fn blank_rows_and_the_scrollbar_do_not_select_or_activate_items() {
        for direction in [ListDirection::TopToBottom, ListDirection::BottomToTop] {
            let view = view(direction, 2);
            let mut state = ListState::default();
            let mut mouse_state = SuggestionMouseState::default();
            let buffer = render(&view, &mut state);

            for row in 20..24 {
                let expected = if buffer[(7, row)].symbol() == " " {
                    SuggestionMouseAction::Ignored
                } else {
                    SuggestionMouseAction::Selected
                };
                assert_eq!(
                    view.handle_mouse(
                        mouse(MouseEventKind::Down(MouseButton::Left), 7, row),
                        &mut state,
                        &mut mouse_state,
                    ),
                    expected,
                );
            }
            assert_eq!(
                view.handle_mouse(
                    mouse(MouseEventKind::Down(MouseButton::Left), 17, 20),
                    &mut state,
                    &mut mouse_state,
                ),
                SuggestionMouseAction::Ignored,
            );
        }
    }

    #[test]
    fn wheel_wraps_both_endpoints_and_rendering_keeps_the_selection_visible() {
        for direction in [ListDirection::TopToBottom, ListDirection::BottomToTop] {
            let view = view(direction, 10);
            let mut state = ListState::default();
            let mut mouse_state = SuggestionMouseState::default();
            let (forward, backward) = wheel_kinds(direction);
            state.select(Some(9));
            render(&view, &mut state);
            assert_eq!(state.offset(), 6);

            assert_eq!(
                view.handle_mouse(mouse(forward, 17, 20), &mut state, &mut mouse_state),
                SuggestionMouseAction::Scrolled,
            );
            assert_eq!(state.selected(), Some(0));
            render(&view, &mut state);
            assert_eq!(state.offset(), 0);

            assert_eq!(
                view.handle_mouse(mouse(backward, 17, 20), &mut state, &mut mouse_state),
                SuggestionMouseAction::Scrolled,
            );
            assert_eq!(state.selected(), Some(9));
            render(&view, &mut state);
            assert_eq!(state.offset(), 6);
        }
    }

    #[test]
    fn wheel_cycles_even_when_all_items_fit_and_no_item_is_selected() {
        for direction in [ListDirection::TopToBottom, ListDirection::BottomToTop] {
            let view = view(direction, 3);
            let mut state = ListState::default();
            let mut mouse_state = SuggestionMouseState::default();
            let (forward, backward) = wheel_kinds(direction);

            for expected in [0, 1, 2, 0] {
                assert_eq!(
                    view.handle_mouse(mouse(forward, 8, 21), &mut state, &mut mouse_state),
                    SuggestionMouseAction::Scrolled,
                );
                render(&view, &mut state);
                assert_eq!(state.selected(), Some(expected));
                assert_eq!(state.offset(), 0);
            }
            for expected in [2, 1, 0, 2] {
                view.handle_mouse(mouse(backward, 8, 21), &mut state, &mut mouse_state);
                render(&view, &mut state);
                assert_eq!(state.selected(), Some(expected));
                assert_eq!(state.offset(), 0);
            }
            state.select(None);
            view.handle_mouse(mouse(backward, 8, 21), &mut state, &mut mouse_state);
            assert_eq!(state.selected(), Some(2));
        }
    }

    #[test]
    fn scrolling_or_explicit_reset_cancels_a_pending_double_click() {
        let view = view(ListDirection::TopToBottom, 1);
        let now = Instant::now();
        let down = mouse(MouseEventKind::Down(MouseButton::Left), 7, 20);
        for scroll_between in [false, true] {
            let mut state = ListState::default();
            let mut mouse_state = SuggestionMouseState::default();
            view.handle_mouse_at(down, &mut state, &mut mouse_state, now);
            if scroll_between {
                view.handle_mouse_at(
                    mouse(MouseEventKind::ScrollDown, 7, 20),
                    &mut state,
                    &mut mouse_state,
                    now + Duration::from_millis(50),
                );
            } else {
                mouse_state.reset();
            }
            assert_eq!(
                view.handle_mouse_at(
                    down,
                    &mut state,
                    &mut mouse_state,
                    now + Duration::from_millis(100),
                ),
                SuggestionMouseAction::Selected,
            );
            assert_eq!(state.selected(), Some(0));
        }
    }

    #[test]
    fn hidden_empty_and_outside_lists_ignore_mouse_events() {
        let view = view(ListDirection::TopToBottom, 10);
        let mut state = ListState::default();
        let mut mouse_state = SuggestionMouseState::default();
        state.select(Some(1));

        for (column, row) in [(6, 20), (18, 20), (7, 19), (7, 24)] {
            assert_eq!(
                view.handle_mouse(
                    mouse(MouseEventKind::ScrollDown, column, row),
                    &mut state,
                    &mut mouse_state,
                ),
                SuggestionMouseAction::Ignored,
            );
        }
        for unavailable in [
            SuggestionMouse {
                item_count: 0,
                ..view
            },
            SuggestionMouse {
                list_area: Rect::new(7, 20, 10, 0),
                ..view
            },
            SuggestionMouse {
                list_area: Rect::new(7, 20, 0, 4),
                ..view
            },
        ] {
            assert_eq!(
                unavailable.handle_mouse(
                    mouse(MouseEventKind::ScrollDown, 7, 20),
                    &mut state,
                    &mut mouse_state,
                ),
                SuggestionMouseAction::Ignored,
            );
        }
        assert_eq!(state.offset(), 0);
        assert_eq!(state.selected(), Some(1));
    }
}

#[cfg(test)]
mod matching_cache_tests {
    use super::*;

    fn module(prefix: &str, description: &str) -> Module {
        Module {
            prefix: prefix.to_owned(),
            description: description.to_owned(),
            cmd: String::new(),
            with_argument: Some(false),
            url_encode: None,
            unbind_proc: None,
        }
    }

    #[test]
    fn cached_matching_preserves_prefix_priority_ties_and_query_changes() {
        let modules = [
            module("y", "git"),
            module("gizmo", "tools"),
            module("git", "version control"),
            module("x", "git"),
        ];
        let suggestions: Vec<_> = modules.iter().collect();
        let mut matcher = SuggestionMatcher::new(&suggestions, true);
        let mut matches = Vec::new();

        for (input, expected) in [
            ("gi", vec!["gizmo", "git", "y", "x"]),
            ("git ignored", vec!["git", "y", "x"]),
            ("zzzz", vec![]),
            ("", vec!["y", "gizmo", "git", "x"]),
        ] {
            let textarea = TextArea::from([input]);
            matcher.update(&textarea, &mut matches);
            let prefixes: Vec<_> = matches
                .iter()
                .map(|module| module.prefix.as_str())
                .collect();
            assert_eq!(prefixes, expected, "input: {input:?}");
        }
    }

    #[test]
    fn cached_hint_matching_preserves_unicode_and_first_word_behavior() {
        let modules = [module("SH", ""), module("開啟", ""), module("İx", "")];
        let suggestions: Vec<_> = modules.iter().collect();
        let mut matcher = SuggestionMatcher::new(&suggestions, false);
        let mut matches = Vec::new();

        for (input, expected) in [
            ("SH argument", vec!["SH"]),
            ("開", vec!["開啟"]),
            ("I", vec!["İx"]),
            ("", vec!["SH", "開啟", "İx"]),
        ] {
            let textarea = TextArea::from([input]);
            matcher.update(&textarea, &mut matches);
            let prefixes: Vec<_> = matches
                .iter()
                .map(|module| module.prefix.as_str())
                .collect();
            assert_eq!(prefixes, expected, "input: {input:?}");
        }
    }

    #[test]
    fn cached_markers_preserve_ansi_colors_and_first_line_only() {
        let spans = marker_spans(
            "\x1b[32m▌\x1b[0m \nignored",
            Style::default().fg(Color::Red),
        );
        assert_eq!(spans[0].content.as_ref(), "▌");
        assert_eq!(spans[0].style.fg, Some(Color::Green));
        let text: String = spans.iter().map(|span| span.content.as_ref()).collect();
        assert_eq!(text, "▌ ");
    }
}

#[cfg(test)]
mod tests {
    use super::{Module, TextArea, complete_input, replace_input};

    fn module(with_argument: bool) -> Module {
        Module {
            description: String::new(),
            prefix: "sh".to_owned(),
            cmd: String::new(),
            with_argument: Some(with_argument),
            url_encode: None,
            unbind_proc: None,
        }
    }

    #[test]
    fn completion_preserves_argument_whitespace_and_unicode() {
        for (input, expected) in [
            ("s", "sh "),
            ("", "sh "),
            ("  s   echo hi", "sh   echo hi"),
            ("s\tfirst\nsecond", "sh\tfirst\nsecond"),
            ("検索\u{3000}日本語", "sh\u{3000}日本語"),
        ] {
            let mut textarea = TextArea::default();
            textarea.insert_str(input);
            let original = complete_input(&mut textarea, &module(true));
            assert_eq!(original, input);
            assert_eq!(textarea.lines().join("\n"), expected);

            replace_input(&mut textarea, &original);
            assert_eq!(textarea.lines().join("\n"), input);
        }
    }

    #[test]
    fn bare_completion_discards_arguments() {
        let mut textarea = TextArea::default();
        textarea.insert_str("  s   echo hi\nsecond");
        let original = complete_input(&mut textarea, &module(false));
        assert_eq!(original, "  s   echo hi\nsecond");
        assert_eq!(textarea.lines().join("\n"), "sh ");
    }
}
