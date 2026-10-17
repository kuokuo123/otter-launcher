use crate::config::*;
use crate::list::*;
use crate::vim::{Mode, Transition, Vim, edit_externally};

use ansi_to_tui::IntoText;
use image::ImageReader;
use ratatui::{
    Frame, TerminalOptions, Viewport,
    crossterm::{
        event::{
            self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
            KeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
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
use ratatui_textarea::{CursorMove, Input, Key, TextArea};
use std::{borrow::Cow, collections::HashMap, error::Error, io};

// Cache texts to save resources while building each frame's list item
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
    let general_height = cfg.general.height.unwrap();
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
    let mut terminal = if general_height > 0 {
        ratatui::try_init_with_options(TerminalOptions {
            viewport: Viewport::Inline(general_height),
        })?
    } else {
        // fullscreen if height is zero
        ratatui::try_init()?
    };

    let mut viewport_bottom = 0;
    let mut matched_input: Vec<String> = Vec::new();
    let mut visible_suggestions = Vec::with_capacity(suggestions.len());
    let mut suggestion_mouse = None;

    // The main loop
    let run_result = (|| -> io::Result<Option<String>> {
        execute!(
            io::stdout(),
            EnableMouseCapture,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        )?;

        loop {
            // Compare the lines and catches edits made by Vim or an external editor.
            if textarea.lines() != matched_input.as_slice() {
                suggestion_matcher.update(&textarea, &mut visible_suggestions);
                matched_input.resize_with(textarea.lines().len(), String::new);
                for (cached, line) in matched_input.iter_mut().zip(textarea.lines()) {
                    cached.clone_from(line);
                }
            }

            normalize_selection(&mut suggestion_state, visible_suggestions.len());

            // (Re)Draw the interface.
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

            // Read key events.
            let key = match event::read()? {
                // Keyboard events
                Event::Key(key) => key,
                // Mouse events
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
                        SuggestionMouseAction::Ignored => {
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
                    continue;
                }
            };

            // Set up function keys.
            let external_editor = std::mem::take(&mut external_edit_pending);
            let control = key.modifiers.contains(KeyModifiers::CONTROL);
            let alt = key.modifiers.contains(KeyModifiers::ALT);
            let shift = key.modifiers.contains(KeyModifiers::SHIFT);
            let line_len = textarea.lines().len();
            let vi_normal_mode = vim.mode == Mode::Normal && line_len <= 1;

            match (key.code, control) {
                (KeyCode::Char('f' | 'b' | 'd' | 'u'), true) if vi_normal_mode => {
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
                    if vi_normal_mode
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
                (KeyCode::Char('.'), _) if !vi_mode && alt && shift && line_len <= 1 => {
                    let Some(view) = suggestion_mouse.as_ref() else {
                        continue;
                    };
                    let last = view.direction == ListDirection::TopToBottom;
                    suggestion_state.select(if last {
                        visible_suggestions.len().checked_sub(1)
                    } else {
                        None
                    });
                }
                (KeyCode::Char(','), _) if !vi_mode && alt && shift && line_len <= 1 => {
                    let Some(view) = suggestion_mouse.as_ref() else {
                        continue;
                    };
                    let last = view.direction == ListDirection::BottomToTop;
                    suggestion_state.select(if last {
                        visible_suggestions.len().checked_sub(1)
                    } else {
                        None
                    });
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
                    if key.modifiers.contains(KeyModifiers::SHIFT) && vim.mode != Mode::Normal {
                        textarea.insert_newline();
                        continue;
                    }

                    if vim.mode == Mode::Normal && suggestion_state.selected() == None {
                        textarea.move_cursor(CursorMove::Down)
                    } else {
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

// Function to draw cached texts and parameters
fn draw_cached(
    frame: &mut Frame,
    image: Option<&mut StatefulProtocol>,
    textarea: &TextArea<'_>,
    suggestions: &[&Module],
    suggestion_state: &mut ListState,
    cache: &mut RenderCache<'_>,
) -> Option<SuggestionOperation> {
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
        Some(SuggestionOperation {
            scroll_area: inner,
            list_area,
            direction: suggestion_direction,
            item_count: suggestions.len(),
        })
    }
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

// Function to convert ANSI code into paragraph
fn marker_spans(marker: &str, style: Style) -> Vec<Span<'static>> {
    let mut spans = marker
        .into_text()
        .expect("failed to parse list/selection prefix ANSI")
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
