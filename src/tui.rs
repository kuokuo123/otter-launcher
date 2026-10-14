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
use ratatui_textarea::{Input, TextArea};
use std::{
    borrow::Cow,
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

// Mouse interaction for the suggestion list rendered by the most recent draw.
// Store the value returned by `draw()` and forward `Event::Mouse` to `handle_mouse()` along with a persistent `SuggestionMouseState`.
// Enable Crossterm mouse capture while the TUI is active.
// Each suggestion in this interface occupies one terminal row.
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
    let mut visible_suggestions: Vec<&Module> = Vec::new();

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
                visible_suggestions = matching_suggestions(&textarea, suggestions);
                matched_input.resize_with(textarea.lines().len(), String::new);
                for (cached, line) in matched_input.iter_mut().zip(textarea.lines()) {
                    cached.clone_from(line);
                }
            }

            normalize_selection(&mut suggestion_state, visible_suggestions.len());
            let mut suggestion_mouse: Option<SuggestionMouse> = None;

            // Draw the interface
            terminal.draw(|frame| {
                viewport_bottom = frame.area().bottom();
                suggestion_mouse = draw(
                    frame,
                    image.as_mut(),
                    &textarea,
                    &visible_suggestions,
                    &mut suggestion_state,
                    header,
                );
            })?;

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
                        SuggestionMouseAction::Ignored => continue,
                    }
                }
                Event::Resize(..) => {
                    suggestion_mouse_state.reset();
                    continue;
                }
                _ => continue,
            };

            if key.kind != KeyEventKind::Press {
                continue;
            }

            let control = key.modifiers.contains(KeyModifiers::CONTROL);
            let alt = key.modifiers.contains(KeyModifiers::ALT);
            let line_len = textarea.lines().len();

            match (key.code, control) {
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
                (KeyCode::Up | KeyCode::Char('k' | 'p'), true) => match suggestion_mode == "list" {
                    true => {
                        select_up(
                            &mut suggestion_state,
                            visible_suggestions.len(),
                            suggestion_direction,
                        );
                        continue;
                    }
                    false => (),
                },
                (KeyCode::Down | KeyCode::Char('j' | 'n'), true) => {
                    match suggestion_mode == "list" {
                        true => {
                            select_down(
                                &mut suggestion_state,
                                visible_suggestions.len(),
                                suggestion_direction,
                            );
                            continue;
                        }
                        false => (),
                    }
                }
                (KeyCode::Up, false) => match suggestion_mode == "list" && line_len <= 1 {
                    true => {
                        select_up(
                            &mut suggestion_state,
                            visible_suggestions.len(),
                            suggestion_direction,
                        );
                        continue;
                    }
                    false => (),
                },
                (KeyCode::Down, false) => match suggestion_mode == "list" && line_len <= 1 {
                    true => {
                        select_down(
                            &mut suggestion_state,
                            visible_suggestions.len(),
                            suggestion_direction,
                        );
                        continue;
                    }
                    false => (),
                },
                (KeyCode::Char('v'), _)
                    if suggestion_mode == "list" && !vi_mode && line_len <= 1 =>
                {
                    let Some(view) = suggestion_mouse.as_ref() else {
                        continue;
                    };

                    let page = usize::from(view.list_area.height).max(1);
                    let last = view.item_count.saturating_sub(1);
                    let max_offset = view.item_count.saturating_sub(page);
                    let offset = suggestion_state.offset().min(max_offset);

                    let forward = matches!(
                        (key.code, view.direction, alt, control),
                        (KeyCode::Char('v'), ListDirection::TopToBottom, false, true)
                            | (KeyCode::Char('v'), ListDirection::BottomToTop, true, _)
                    );
                    let edge = if forward { last } else { 0 };

                    let (selected, offset) = match suggestion_state.selected() {
                        None => {
                            let page_start = if forward {
                                offset.saturating_add(page).min(max_offset)
                            } else {
                                max_offset
                            };

                            (Some(page_start), page_start)
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

                    suggestion_state.select(selected);
                    *suggestion_state.offset_mut() = offset;
                    tab_original = None;
                    external_edit_pending = false;
                    continue;
                }
                (KeyCode::PageUp | KeyCode::PageDown, _)
                    if suggestion_mode == "list" && line_len <= 1 =>
                {
                    let Some(view) = suggestion_mouse.as_ref() else {
                        continue;
                    };

                    let page = usize::from(view.list_area.height).max(1);
                    let last = view.item_count.saturating_sub(1);
                    let max_offset = view.item_count.saturating_sub(page);
                    let offset = suggestion_state.offset().min(max_offset);

                    let forward = matches!(
                        (key.code, view.direction),
                        (KeyCode::PageDown, ListDirection::TopToBottom)
                            | (KeyCode::PageUp, ListDirection::BottomToTop)
                    );
                    let edge = if forward { last } else { 0 };

                    let (selected, offset) = match suggestion_state.selected() {
                        None => {
                            let page_start = if forward {
                                offset.saturating_add(page).min(max_offset)
                            } else {
                                max_offset
                            };

                            (Some(page_start), page_start)
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

                    suggestion_state.select(selected);
                    *suggestion_state.offset_mut() = offset;
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

// Function that draws the interface
pub fn draw(
    frame: &mut Frame,
    image: Option<&mut StatefulProtocol>,
    textarea: &TextArea<'_>,
    suggestions: &[&Module],
    suggestion_state: &mut ListState,
    header: &str,
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
    let prompt_sign = cfg.interface.prompt_sign.as_deref().unwrap();
    let header_color = parse_color(cfg.interface.header_color.as_deref(), Color::Blue);
    let prompt_sign_color = parse_color(cfg.interface.prompt_sign_color.as_deref(), Color::Blue);

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

    // Prepare header
    let header_text = header.into_text().expect("failed to parse header ANSI");
    let header_height = if !header.is_empty() {
        u16::try_from(header_text.lines.len()).unwrap_or(0)
    } else {
        0
    };

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
        let label = cfg.interface.separator_label.as_deref().unwrap();
        let separator_width = usize::from(separator_area.width.saturating_sub(1));
        frame.render_widget(
            Paragraph::new(format!(
                "{label}{}",
                "─".repeat(separator_width.saturating_sub(label.len())),
            ))
            .style(Style::default().fg(Color::DarkGray)),
            separator_area,
        );
    }

    frame.render_widget(
        Paragraph::new(header_text).style(
            Style::default()
                .fg(header_color)
                .add_modifier(Modifier::BOLD),
        ),
        header_area,
    );

    // Parse the prompt once; ANSI escape codes do not contribute to its width.
    let prompt_text = prompt_sign
        .into_text()
        .expect("failed to parse prompt ANSI");

    let prompt_sign_width = u16::try_from(prompt_text.width()).unwrap_or(u16::MAX);

    let [prefix_area, textarea_area] =
        Layout::horizontal([Constraint::Length(prompt_sign_width), Constraint::Fill(1)])
            .areas(input_area);

    frame.render_widget(
        Paragraph::new(prompt_text).style(
            Style::default()
                .fg(prompt_sign_color)
                .add_modifier(Modifier::BOLD),
        ),
        prefix_area,
    );

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

    // List-specific options and styles are only needed in list mode.
    let selection_prefix = cfg
        .interface
        .selection_prefix
        .as_deref()
        .unwrap()
        .into_text()
        .expect("");
    let list_prefix = cfg
        .interface
        .list_prefix
        .as_deref()
        .unwrap()
        .into_text()
        .expect("");
    let prefix_padding = cfg.interface.prefix_padding.unwrap();
    let prefix_style = Style::default().fg(parse_color(
        cfg.interface.prefix_color.as_deref(),
        Color::Yellow,
    ));
    let description_style = Style::default().fg(parse_color(
        cfg.interface.description_color.as_deref(),
        Color::Reset,
    ));
    let indicator_style = Style::default().fg(parse_color(
        cfg.interface.indicator_color.as_deref(),
        Color::Red,
    ));
    let list_marker_style = Style::default().fg(parse_color(
        cfg.interface.list_prefix_color.as_deref(),
        Color::Reset,
    ));
    let selected_marker_style = Style::default().fg(parse_color(
        cfg.interface.selection_prefix_color.as_deref(),
        Color::Red,
    ));
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
            &selection_prefix
        } else {
            &list_prefix
        };

        let marker_style = if selected {
            selected_marker_style
        } else {
            list_marker_style
        };

        // Make ANSI color code compatible with marker
        let mut spans: Vec<Span<'_>> = marker
            .lines
            .first()
            .map(|line| line.spans.clone())
            .unwrap_or_default();

        for span in &mut spans {
            span.style = marker_style.patch(span.style);
        }

        spans.extend([
            Span::styled(
                padded_prefix(module.prefix.as_str(), prefix_padding),
                prefix_style,
            ),
            Span::raw(" "),
            Span::styled(indicator, indicator_style),
            Span::styled(module.description.as_str(), description_style),
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
    let inverse_selection = if cfg.interface.inverse_selection.unwrap() {
        Style::default()
            .bg(Color::Black)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().add_modifier(Modifier::BOLD)
    };
    let suggestion_list = List::new(items)
        .direction(suggestion_direction)
        .highlight_style(inverse_selection);

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
    if width == 0 || (prefix.is_ascii() && prefix.len() >= width) {
        Cow::Borrowed(prefix)
    } else {
        Cow::Owned(format!("{prefix:<width$}"))
    }
}

// Most command prefixes and descriptions are already lowercase ASCII. Keep the original Unicode conversion for every non-ASCII string.
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

// Function to match user input with suggestions
pub fn matching_suggestions<'a>(
    textarea: &TextArea<'_>,
    suggestions: &[&'a Module],
) -> Vec<&'a Module> {
    if suggestions.is_empty() {
        return Vec::new();
    }

    // A single-line input can be borrowed instead of joined into a new String.
    let input: Cow<'_, str> = match textarea.lines() {
        [line] => Cow::Borrowed(line.as_str()),
        lines => Cow::Owned(lines.join(" ")),
    };

    if config().interface.suggestion_mode.as_deref().unwrap() != "list" {
        let query = lowercase(input.split_whitespace().next().unwrap_or(""));
        if query.is_empty() {
            return suggestions.to_vec();
        }

        return suggestions
            .iter()
            .copied()
            .filter(|module| lowercase(&module.prefix).starts_with(query.as_ref()))
            .collect();
    }

    let query = lowercase(input.as_ref());
    let query_prefix = query.split_whitespace().next().unwrap_or("");
    if query_prefix.is_empty() {
        return suggestions.to_vec();
    }

    let query_is_first_word = query.as_ref() == query_prefix;
    let matcher = SkimMatcherV2::default();
    let mut prefix_matches = Vec::new();
    let mut whole_input_matches: Vec<(i64, &'a Module)> = Vec::new();
    let mut first_word_matches: Vec<(i64, &'a Module)> = Vec::new();

    for module in suggestions.iter().copied() {
        let prefix = lowercase(&module.prefix);

        // Preserve prefix matches in their original order.
        if prefix.starts_with(query_prefix) {
            prefix_matches.push(module);
            continue;
        }

        let description = lowercase(&module.description);
        let whole_score = matcher
            .fuzzy_match(prefix.as_ref(), query.as_ref())
            .max(matcher.fuzzy_match(description.as_ref(), query.as_ref()));

        if let Some(score) = whole_score {
            whole_input_matches.push((score, module));
            continue;
        }

        // A failed single-word query would produce the same failed score again.
        if !query_is_first_word {
            let first_word_score = matcher
                .fuzzy_match(prefix.as_ref(), query_prefix)
                .max(matcher.fuzzy_match(description.as_ref(), query_prefix));
            if let Some(score) = first_word_score {
                first_word_matches.push((score, module));
            }
        }
    }

    // Stable sorting retains the original order when fuzzy scores tie.
    whole_input_matches.sort_by(|a, b| b.0.cmp(&a.0));
    first_word_matches.sort_by(|a, b| b.0.cmp(&a.0));
    prefix_matches.extend(whole_input_matches.into_iter().map(|(_, module)| module));
    prefix_matches.extend(first_word_matches.into_iter().map(|(_, module)| module));
    prefix_matches
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
