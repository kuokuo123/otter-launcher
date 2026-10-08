use crate::config::*;

use ansi_to_tui::IntoText;
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use ratatui::{
    Frame,
    crossterm::event::{MouseButton, MouseEvent, MouseEventKind},
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListDirection, ListItem, ListState, Padding, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
};
use ratatui_image::{Resize, StatefulImage, protocol::StatefulProtocol};
use ratatui_textarea::TextArea;
use std::time::{Duration, Instant};

/// The event loop maps Activate to its existing Enter handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionMouseAction {
    Ignored,
    Selected,
    Scrolled,
    Activate,
    Complete,
}

/// Keep this outside the event loop; draw() only returns the current geometry.
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
    /// Cancel a pending double-click when a key or resize changes the interface.
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

/// Mouse interaction for the suggestion list rendered by the most recent draw.
///
/// Store the value returned by `draw()` and forward `Event::Mouse` to
/// `handle_mouse()` along with a persistent `SuggestionMouseState`.
/// Enable Crossterm mouse capture while the TUI is active.
/// Each suggestion in this interface occupies one terminal row.
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
    /// Single-click selects; double-click activates; wheel steps cycle selection.
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

pub fn draw(
    frame: &mut Frame,
    image: Option<&mut StatefulProtocol>,
    textarea: &TextArea<'_>,
    suggestions: &[&Module],
    suggestion_state: &mut ListState,
    header: &str,
) -> Option<SuggestionMouse> {
    let cfg = config();
    let has_image = image.is_some();
    let image_position = cfg.image.position.as_deref().unwrap();
    let image_width = cfg.image.width.unwrap_or(0);
    let suggestion_position = cfg.interface.suggestion_position.as_deref().unwrap();

    let suggestion_mode = cfg.interface.suggestion_mode.as_deref().unwrap();
    let separator = cfg.interface.separator.unwrap();
    let separator_label = cfg.interface.separator_label.as_deref().unwrap();
    let selection_prefix = cfg.interface.selection_prefix.as_deref().unwrap();
    let list_prefix = cfg.interface.list_prefix.as_deref().unwrap();
    let prefix_padding = cfg.interface.prefix_padding.unwrap();
    let indicator_with_arg = cfg.interface.indicator_args.as_deref().unwrap();
    let indicator_no_arg = cfg.interface.indicator_bare.as_deref().unwrap();
    let prompt_sign = config().interface.prompt_sign.as_deref().unwrap();
    let header_color = cfg
        .interface
        .header_color
        .as_deref()
        .and_then(|value| value.parse::<Color>().ok())
        .unwrap_or(Color::Blue);
    let prompt_sign_color = cfg
        .interface
        .prompt_sign_color
        .as_deref()
        .and_then(|value| value.parse::<Color>().ok())
        .unwrap_or(Color::Blue);
    let prefix_color = cfg
        .interface
        .prefix_color
        .as_deref()
        .and_then(|value| value.parse::<Color>().ok())
        .unwrap_or(Color::Yellow);
    let description_color = cfg
        .interface
        .description_color
        .as_deref()
        .and_then(|value| value.parse::<Color>().ok())
        .unwrap_or(Color::Reset);
    let indicator_color = cfg
        .interface
        .indicator_color
        .as_deref()
        .and_then(|value| value.parse::<Color>().ok())
        .unwrap_or(Color::Red);
    let list_prefix_color = cfg
        .interface
        .list_prefix_color
        .as_deref()
        .and_then(|value| value.parse::<Color>().ok())
        .unwrap_or(Color::Reset);
    let selection_prefix_color = cfg
        .interface
        .selection_prefix_color
        .as_deref()
        .and_then(|value| value.parse::<Color>().ok())
        .unwrap_or(Color::Red);

    let iface_pad_top = cfg.interface.padding.unwrap()[0];
    let iface_pad_right = cfg.interface.padding.unwrap()[1];
    let iface_pad_bottom = cfg.interface.padding.unwrap()[2];
    let iface_pad_left = cfg.interface.padding.unwrap()[3];

    let image_pad_top = cfg.image.padding.unwrap()[0];
    let image_pad_right = cfg.image.padding.unwrap()[1];
    let image_pad_bottom = cfg.image.padding.unwrap()[2];
    let image_pad_left = cfg.image.padding.unwrap()[3];

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

            "right" => {
                let [_margin_left, content_area, _margin_right, image_area] = Layout::horizontal([
                    Constraint::Length(iface_pad_left),
                    Constraint::Fill(1),
                    Constraint::Length(iface_pad_right),
                    Constraint::Length(image_width),
                ])
                .areas(frame.area());

                (image_area, content_area);

                let [_margin_top, content, _margin_bottom] = Layout::vertical([
                    Constraint::Length(iface_pad_top),
                    Constraint::Fill(1),
                    Constraint::Length(iface_pad_bottom),
                ])
                .areas(content_area);

                (image_area, content)
            }

            // "left", or an unrecognized position.
            _ => {
                let [image_area, _margin_left, content_area, _margin_right] = Layout::horizontal([
                    Constraint::Length(image_width),
                    Constraint::Length(iface_pad_left),
                    Constraint::Fill(1),
                    Constraint::Length(iface_pad_right),
                ])
                .areas(frame.area());

                (image_area, content_area);

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

    if let Some(image) = image {
        if image_area.width > 0 && image_area.height > 0 {
            // 1. Define the border block
            let block = Block::default()
                .borders(Borders::NONE)
                //.title(" Image ") // Optional title on the border
                .padding(Padding::new(
                    image_pad_left,
                    image_pad_right,
                    image_pad_top,
                    image_pad_bottom,
                )); // Optional: space between border line and image

            // 2. Calculate the area strictly inside the border line (and padding)
            let inner_area = block.inner(image_area);

            // 3. Draw the border block first
            frame.render_widget(block, image_area);

            // 4. Render the image ONLY within inner_area so it doesn't overwrite the border
            if inner_area.width > 0 && inner_area.height > 0 {
                frame.render_stateful_widget(
                    StatefulImage::default().resize(Resize::Scale(None)),
                    inner_area,
                    image,
                );
            }
        }
    }

    let header_text = header.into_text().expect("failed to parse header ANSI");

    let header_height = if !header.is_empty() {
        u16::try_from(header_text.lines.len()).unwrap_or(0)
    } else {
        0
    };
    let content_height = u16::try_from(textarea.lines().len())
        .unwrap_or(u16::MAX)
        .saturating_add(0);

    // Reserve the earlier header lines and one suggestion row.
    let max_input_height = right
        .height
        .saturating_sub(header_height)
        .saturating_sub(1)
        .max(3);

    let input_height = content_height.clamp(1, max_input_height);

    let suggestions_below_input = suggestion_position == "bottom";

    let (suggestions_area, separator_area, header_area, input_area) = if suggestions_below_input {
        let [header_area, input_area, separator_area, suggestions_area] = Layout::vertical([
            Constraint::Length(header_height),
            Constraint::Length(input_height),
            Constraint::Length(if separator && suggestion_mode == "list" {
                1
            } else {
                0
            }),
            Constraint::Fill(1),
        ])
        .areas(right);

        (suggestions_area, separator_area, header_area, input_area)
    } else {
        let [suggestions_area, separator_area, header_area, input_area] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(if separator && suggestion_mode == "list" {
                1
            } else {
                0
            }),
            Constraint::Length(header_height),
            Constraint::Length(input_height),
        ])
        .areas(right);

        (suggestions_area, separator_area, header_area, input_area)
    };

    let separator_width = separator_area.width.saturating_sub(1);

    frame.render_widget(
        Paragraph::new({
            let label = separator_label;
            format!(
                "{label}{}",
                "─".repeat(usize::from(separator_width).saturating_sub(label.len()))
            )
        })
        .style(Style::default().fg(Color::DarkGray)),
        separator_area,
    );

    frame.render_widget(
        Paragraph::new(header_text).style(
            Style::default()
                .fg(header_color)
                .add_modifier(Modifier::BOLD),
        ),
        header_area,
    );

    // Input field rendering
    let input_block = Block::default().borders(Borders::NONE);
    let input_inner = input_block.inner(input_area);

    frame.render_widget(input_block, input_area);

    // Leave at least one column for editable text.
    let prompt_text = prompt_sign
        .into_text()
        .expect("failed to parse prompt ANSI");

    let prompt_sign_width = u16::try_from(prompt_text.width()).unwrap_or(u16::MAX);

    let [prefix_area, textarea_area] =
        Layout::horizontal([Constraint::Length(prompt_sign_width), Constraint::Fill(1)])
            .areas(input_inner);

    frame.render_widget(
        Paragraph::new(prompt_text).style(
            Style::default()
                .fg(prompt_sign_color)
                .add_modifier(Modifier::BOLD),
        ),
        prefix_area,
    );

    frame.render_widget(
        Paragraph::new(prompt_sign.into_text().unwrap()).style(
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
            let placeholder_style = Style::default().fg(Color::Black);
            let buffer = frame.buffer_mut();

            buffer.set_stringn(
                inner.x,
                inner.y,
                cfg.interface.placeholder.as_deref().unwrap(),
                usize::from(inner.width),
                placeholder_style,
            );

            // Apply the current cursor style to the first character.
            buffer[(inner.x, inner.y)].set_style(placeholder_style.patch(textarea.cursor_style()));
        }
    }

    if Some(suggestion_mode) == Some("hint") {
        draw_inline_hint(frame, textarea, textarea_area, suggestions.first().copied());
        return None;
    }

    let items: Vec<ListItem<'_>> = suggestions
        .iter()
        .enumerate()
        .map(|(index, module)| {
            let selected = suggestion_state.selected() == Some(index);
            let marker = if selected {
                selection_prefix
            } else {
                list_prefix
            };

            let indicator = if module.with_argument.unwrap() {
                indicator_with_arg
            } else {
                indicator_no_arg
            };

            ListItem::new(Line::from(vec![
                Span::styled(
                    marker,
                    Style::default().fg(if selected {
                        selection_prefix_color
                    } else {
                        list_prefix_color
                    }),
                ),
                Span::styled(
                    format!("{:<width$}", module.prefix, width = prefix_padding),
                    Style::default().fg(prefix_color),
                ),
                Span::raw(" "),
                Span::styled(indicator, Style::default().fg(indicator_color)),
                Span::styled(
                    module.description.as_str(),
                    Style::default().fg(description_color),
                ),
            ]))
        })
        .collect();

    let suggestion_block = Block::default().border_style(Style::default().fg(Color::DarkGray));

    let suggestion_block = suggestion_block.borders(Borders::NONE);

    // Exclude the block's border from the available list space.
    let inner = suggestion_block.inner(suggestions_area);
    let visible_rows = usize::from(inner.height);

    let show_scrollbar = visible_rows > 0 && inner.width > 1 && suggestions.len() > visible_rows;

    // Render the block separately so the scrollbar shares its inner area.
    frame.render_widget(suggestion_block, suggestions_area);

    let list_area = Rect {
        width: inner.width.saturating_sub(u16::from(show_scrollbar)),
        ..inner
    };

    let suggestion_direction = if cfg.interface.suggestion_position.as_deref().unwrap() == "top" {
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

    // These are absolute terminal coordinates, including the inline viewport's
    // origin. Recreate them on every draw so terminal resizes stay accurate.
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

fn draw_inline_hint(
    frame: &mut Frame,
    textarea: &TextArea<'_>,
    textarea_area: Rect,
    closest: Option<&Module>,
) {
    let Some(module) = closest else {
        return;
    };

    // Only show hints for single-line input.
    if textarea.lines().len() != 1 {
        return;
    }

    let input = &textarea.lines()[0];
    let mut words = input.split_whitespace();

    let Some(first_word) = words.next() else {
        return;
    };

    // Hide as soon as a second word exists.
    if words.next().is_some() {
        return;
    }

    // Only show the hint while editing at the end of the input.
    if textarea.cursor() != (0, input.chars().count()) {
        return;
    }

    let trailing_space = input
        .chars()
        .last()
        .map(char::is_whitespace)
        .unwrap_or(false);

    let prefix_starts_with_word = module
        .prefix
        .to_lowercase()
        .starts_with(&first_word.to_lowercase());

    let hint = if trailing_space {
        // "sh " → "run bash command"
        module.description.clone()
    } else if prefix_starts_with_word {
        // "s" → "h run bash command"
        // "sh" → " run bash command"
        let suffix: String = module
            .prefix
            .chars()
            .skip(first_word.chars().count())
            .collect();

        if module.description.is_empty() {
            suffix
        } else {
            format!("{suffix} {}", module.description)
        }
    } else {
        // A fuzzy match may have no prefix suffix to append.
        format!(" {}", module.description)
    };

    // Respect the textarea's block borders/padding.
    let inner = textarea
        .block()
        .map(|block| block.inner(textarea_area))
        .unwrap_or(textarea_area);

    // Measure terminal columns rather than byte length.
    let input_width = Line::raw(input.as_str()).width();

    if inner.height == 0 || input_width >= usize::from(inner.width) {
        return;
    }

    let x = inner.x + input_width as u16;
    let available_width = usize::from(inner.width) - input_width;

    frame.buffer_mut().set_stringn(
        x,
        inner.y,
        hint,
        available_width,
        Style::default().fg(Color::DarkGray),
    );
}

pub fn matching_suggestions<'a>(
    textarea: &TextArea<'_>,
    suggestions: &[&'a Module],
) -> Vec<&'a Module> {
    let input = textarea.lines().join(" ");

    // List mode behavior
    if config().interface.suggestion_mode.as_deref().unwrap() == "list" {
        let query = input.to_lowercase();
        let query_prefix = query.split_whitespace().next().unwrap_or("");

        let matcher = SkimMatcherV2::default();

        let mut prefix_matches = Vec::new();
        let mut whole_input_matches: Vec<(i64, &'a Module)> = Vec::new();
        let mut first_word_matches: Vec<(i64, &'a Module)> = Vec::new();

        for module in suggestions.iter().copied() {
            let prefix = module.prefix.to_lowercase();

            // Group 1: prefix starts with the first word.
            if prefix.starts_with(query_prefix) {
                prefix_matches.push(module);
                continue;
            }

            let description = module.description.to_lowercase();

            // Group 2: fuzzy match against the entire input.
            let whole_score = matcher
                .fuzzy_match(&prefix, &query)
                .max(matcher.fuzzy_match(&description, &query));

            if let Some(score) = whole_score {
                whole_input_matches.push((score, module));
                continue;
            }

            // Group 3: fuzzy match against only the first word.
            let first_word_score = matcher
                .fuzzy_match(&prefix, query_prefix)
                .max(matcher.fuzzy_match(&description, query_prefix));

            if let Some(score) = first_word_score {
                first_word_matches.push((score, module));
            }
        }

        whole_input_matches.sort_by(|a, b| b.0.cmp(&a.0));
        first_word_matches.sort_by(|a, b| b.0.cmp(&a.0));

        prefix_matches.extend(whole_input_matches.into_iter().map(|(_, module)| module));
        prefix_matches.extend(first_word_matches.into_iter().map(|(_, module)| module));

        prefix_matches

    // Hint mode behaviour
    } else {
        let query = input.split_whitespace().next().unwrap_or("").to_lowercase();

        suggestions
            .iter()
            .copied()
            .filter(|module| {
                query.is_empty()
                    || module.prefix.to_lowercase().starts_with(&query)
            })
            .collect()
    }
}

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
