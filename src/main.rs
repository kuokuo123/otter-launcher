mod config;
mod exec;
mod interface;
mod vim;

use crate::config::*;
use crate::exec::*;
use crate::interface::*;
use crate::vim::{Mode, Transition, Vim};
use image::ImageReader;
use ratatui::{
    TerminalOptions, Viewport,
    crossterm::{
        event::{
            self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
            KeyModifiers,
        },
        execute,
    },
    style::Style,
    widgets::{ListDirection, ListState},
};
use ratatui_image::{
    picker::{Picker, ProtocolType},
    protocol::StatefulProtocol,
};
use ratatui_textarea::{Input, TextArea};
use shellexpand;
use std::{error::Error, io};
use urlencoding::encode;

fn main() -> Result<(), Box<dyn Error>> {
    // Load config: check if alternative config is provided and if arguments are provided
    let cli = parse_arguments()?;
    init_config(cli.config_path.as_deref())?;
    let cli_arguments = cli.cli_arguments;
    let cfg = config();

    // Expand header and cache it for later rendering
    let header = if cli_arguments.is_none() {
        expand_header(cfg.interface.header.as_deref().unwrap())?
    } else {
        String::new()
    };

    // Prepare placeholders for the main loop
    let mut submission: Option<String> = None;
    let mut loop_switch = false;

    // Main loop
    loop {
        // Enter TUI only if no arguments are passed
        if !cli_arguments.is_none() {
            submission = cli_arguments.clone();
        } else {
            // Load image
            let mut image: Option<StatefulProtocol> = match cfg
                .image
                .path
                .as_deref()
                .filter(|path| !path.trim().is_empty())
            {
                Some(image_path) => {
                    let expanded_path = shellexpand::full_with_context_no_errors(
                        &image_path,
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
                        "symbol" => ProtocolType::Halfblocks,
                        _ => ProtocolType::Halfblocks,
                    });
                    let protocol = picker.new_resize_protocol(source.clone());

                    Some(protocol)
                }
                None => None,
            };

            // Initialize editor
            let vi_mode = cfg.general.vi_mode.unwrap();
            //let placeholder = cfg.interface.placeholder.as_deref().unwrap();
            let mut textarea = TextArea::default();
            textarea.set_cursor_line_style(Style::default());
            textarea.set_block(Mode::Insert.block());
            textarea.set_cursor_style(Mode::Insert.cursor_style());
            let mut external_edit_pending = false;
            let mut vim = Vim::new(Mode::Insert);
            let mut tab_original: Option<String> = None;
            //textarea.set_placeholder_text(placeholder);
            //textarea.set_placeholder_style(Style::default().fg(Color::Black));

            // Initialize suggestions
            let suggestions: Vec<&Module> = cfg.modules.iter().collect();
            let mut suggestion_state = ListState::default();
            let mut suggestion_mouse_state = SuggestionMouseState::default();
            let suggestion_direction =
                if cfg.interface.suggestion_position.as_deref().unwrap() == "top" {
                    ListDirection::BottomToTop
                } else {
                    ListDirection::TopToBottom
                };

            // Set up viewport
            let height = cfg.general.height.unwrap();
            let mut terminal = ratatui::try_init_with_options(TerminalOptions {
                viewport: Viewport::Inline(height),
            })?;
            let mut viewport_bottom = 0;

            // Let the keypresses flow
            let _run_result = (|| -> io::Result<()> {
                execute!(io::stdout(), EnableMouseCapture)?;
                loop {
                    let visible_suggestions = matching_suggestions(&textarea, &suggestions);
                    let mut suggestion_mouse = None;
                    normalize_selection(&mut suggestion_state, visible_suggestions.len());

                    terminal.draw(|frame| {
                        viewport_bottom = frame.area().bottom();

                        suggestion_mouse = draw(
                            frame,
                            image.as_mut(),
                            &textarea,
                            &visible_suggestions,
                            &mut suggestion_state,
                            &header,
                        );
                    })?;

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

                                SuggestionMouseAction::Selected
                                | SuggestionMouseAction::Scrolled => {
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

                    match key.code {
                        KeyCode::Up => {
                            select_up(
                                &mut suggestion_state,
                                visible_suggestions.len(),
                                suggestion_direction,
                            );
                            continue;
                        }
                        KeyCode::Down => {
                            select_down(
                                &mut suggestion_state,
                                visible_suggestions.len(),
                                suggestion_direction,
                            );
                            continue;
                        }
                        KeyCode::Tab => {
                            // Second Tab: restore the original input.
                            if let Some(original) = tab_original.take() {
                                textarea.cancel_selection();
                                textarea.select_all();
                                textarea.cut();
                                textarea.insert_str(&original);

                                suggestion_state.select(None);
                                continue;
                            }

                            // First Tab: complete using the selected or first suggestion.
                            let module = suggestion_state
                                .selected()
                                .and_then(|index| visible_suggestions.get(index))
                                .or_else(|| visible_suggestions.first())
                                .copied();

                            if let Some(module) = module {
                                let prompt = textarea.lines().join("\n");

                                let updated = if module.with_argument.unwrap_or(false) {
                                    // Argument-taking module: replace only the first word.
                                    let input = prompt.trim_start();
                                    let first_word_end =
                                        input.find(char::is_whitespace).unwrap_or(input.len());

                                    let remaining = &input[first_word_end..];

                                    if remaining.is_empty() {
                                        format!("{} ", module.prefix)
                                    } else {
                                        format!("{}{}", module.prefix, remaining)
                                    }
                                } else {
                                    // Non-argument-taking module: replace the entire input.
                                    module.prefix.clone()
                                };

                                tab_original = Some(prompt);

                                textarea.cancel_selection();
                                textarea.select_all();
                                textarea.cut();
                                textarea.insert_str(&updated);

                                suggestion_state.select(None);
                            }

                            continue;
                        }

                        // Submit the selected suggestion.
                        KeyCode::Enter => {
                            if key.modifiers.contains(KeyModifiers::ALT) {
                                let prefix =
                                    config().general.alternative_module.as_deref().unwrap_or("");

                                submission =
                                    Some(format!("{} {}", prefix, textarea.lines().join("\n")));

                                return Ok(());
                            }

                            let prompt = textarea.lines().join("\n");

                            let selected_module = suggestion_state
                                .selected()
                                .and_then(|index| visible_suggestions.get(index))
                                .copied();

                            match selected_module {
                                // Nothing selected: submit the input unchanged.
                                None => {
                                    submission = Some(prompt);
                                    return Ok(());
                                }

                                // Argument-taking module: replace the first word.
                                Some(module) if module.with_argument.unwrap_or(false) => {
                                    let input = prompt.trim_start();

                                    let first_word_end =
                                        input.find(char::is_whitespace).unwrap_or(input.len());

                                    // Preserve everything after the first word.
                                    let remaining = &input[first_word_end..];

                                    let updated = if remaining.is_empty() {
                                        format!("{} ", module.prefix)
                                    } else {
                                        format!("{}{}", module.prefix, remaining)
                                    };

                                    // Update the existing textarea, retaining its styling.
                                    textarea.cancel_selection();
                                    textarea.select_all();
                                    textarea.cut();
                                    textarea.insert_str(&updated);

                                    tab_original = Some(prompt);

                                    // The next Enter submits the edited input.
                                    suggestion_state.select(None);
                                    continue;
                                }

                                // Module without arguments: submit only its prefix.
                                Some(module) => {
                                    submission = Some(module.prefix.clone());
                                    return Ok(());
                                }
                            }
                        }
                        _ => {}
                    }

                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        match key.code {
                            KeyCode::Char('c') | KeyCode::Char('d') => {
                                return Ok(());
                            }
                            KeyCode::Char('j') | KeyCode::Char('n') => {
                                select_down(
                                    &mut suggestion_state,
                                    visible_suggestions.len(),
                                    suggestion_direction,
                                );
                                continue;
                            }
                            KeyCode::Char('k') | KeyCode::Char('p') => {
                                select_up(
                                    &mut suggestion_state,
                                    visible_suggestions.len(),
                                    suggestion_direction,
                                );
                                continue;
                            }
                            _ => {}
                        }
                        if !vi_mode {
                            let external_editor = external_edit_pending;
                            external_edit_pending = false;

                            match key.code {
                                KeyCode::Char('u') => {
                                    textarea.delete_line_by_head();
                                    continue;
                                }
                                KeyCode::Char('e') => {
                                    if external_editor {
                                        execute!(io::stdout(), DisableMouseCapture)?;
                                        let result =
                                            vim::edit_externally(&mut terminal, &mut textarea);
                                        let mouse_result =
                                            execute!(io::stdout(), EnableMouseCapture);

                                        result?;
                                        mouse_result?;
                                        continue;
                                    }
                                }
                                KeyCode::Char('x') => {
                                    external_edit_pending = true;
                                    continue;
                                }
                                //KeyCode::Char('k') => {
                                //    textarea.delete_line_by_end();
                                //    continue;
                                //}
                                _ => {}
                            }

                            if key.code == KeyCode::Esc {
                                return Ok(());
                            }
                        }
                    }

                    let input: Input = key.into();

                    // Use emacs keybinds if not set with vi_mode
                    if !vi_mode {
                        textarea.input(input);
                        continue;
                    }

                    // Use vi keybinds if it comes through
                    vim = match vim.transition(input, &mut textarea) {
                        Transition::Mode(mode) if vim.mode != mode => {
                            textarea.set_block(mode.block());
                            textarea.set_cursor_style(mode.cursor_style());
                            // Preserve numeric counts and pending command state.
                            vim.with_mode(mode)
                        }
                        Transition::EditExternally => {
                            vim::edit_externally(&mut terminal, &mut textarea)?;
                            vim
                        }
                        Transition::Pending(input) => vim.with_pending(input),
                        Transition::Nop | Transition::Mode(_) => vim,
                        Transition::Quit => return Ok(()),
                    };
                }
            })();

            // Set cursor to viwport end
            if viewport_bottom > 0 {
                terminal.set_cursor_position((0, viewport_bottom))?;
            }

            // Clean up ratatui
            let mouse_result = execute!(io::stdout(), DisableMouseCapture);
            let restore_result = ratatui::try_restore();

            mouse_result?;
            restore_result?;
            _run_result?;
        }

        // Cancelled: exit main before running any module.
        if submission.is_none() {
            std::process::exit(0);
        }

        // Match the prompted with configured modules
        let prompt = submission.as_deref().unwrap_or_default().to_string();
        let prompted_prfx = prompt.split_whitespace().next().unwrap_or("");
        let module_prfx = config()
            .modules
            .iter()
            .find(|module| remove_ascii(&module.prefix) == prompted_prfx);

        match module_prfx {
            // If user input starts with some module prefixes
            Some(module) => {
                // Determine whether the prompt should be urlencoded
                let argument = if module.url_encode.unwrap_or(false) {
                    encode(prompt.trim_start_matches(prompted_prfx).trim_start()).to_string()
                } else {
                    prompt
                        .trim_start_matches(prompted_prfx)
                        .trim_start()
                        .to_string()
                };

                // Condition 1: when the selected module runs with arguement
                if module.with_argument.unwrap_or(false) {
                    if module.unbind_proc.unwrap_or(false) {
                        let _ = run_module_command_unbind_proc(module.cmd.replace("{}", &argument));
                    } else {
                        let _ = run_module_command(module.cmd.replace("{}", &argument));
                    }
                // Condition 2: when user input is exactly the same as the no-arg module
                } else if remove_ascii(&module.prefix) == prompt.trim_end() {
                    if module.unbind_proc.unwrap_or(false) {
                        let _ = run_module_command_unbind_proc(module.cmd.to_owned());
                    } else {
                        let _ = run_module_command(module.cmd.to_owned());
                    }
                // Condition 3: when no-arg modules is running with arguement
                } else {
                    if let Some(module_name) = cfg.general.default_module.as_ref() {
                        run_designated_module(prompt, module_name);
                    }
                }
            }
            // If user input doesn't start with some module prefixes
            _ => {
                // Condition 1: when user input is empty, run the empty module
                if prompt.is_empty() {
                    if let Some(module_name) = cfg.general.empty_module.as_ref() {
                        run_designated_module(prompt, module_name);
                    }
                // Condition 2: when helper keyword is passed, open cheatsheet
                } else if Some(prompt.trim_end()) == cfg.general.cheatsheet_entry.as_deref() {
                    let _ = cheat_sheet();
                    loop_switch = true;
                // Condition 3: when no module is matched, run the default module
                } else {
                    if let Some(module_name) = cfg.general.default_module.as_ref() {
                        run_designated_module(prompt, module_name);
                    }
                }
            }
        }
        // Quit the process if not instructed to loop
        if !loop_switch {
            break;
        }

        // Reset placeholders for the main loop
        submission = None;
        loop_switch = false;
    }

    Ok(())
}
