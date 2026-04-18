// libray for vim and emacs keybinds, which are used in rutstyline editor

use crate::glob_vars::*;
use crate::helper::*;
use crate::mod_exec::*;
use rustyline::{Cmd, ConditionalEventHandler, Event, EventContext, Movement, RepeatCount};
use rustyline::{
    EditMode, Editor, EventHandler, KeyCode, KeyEvent, Modifiers, config::Configurer,
    history::DefaultHistory,
};
use std::sync::atomic::Ordering;
use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    process::Command,
    thread,
    time::Duration,
};

pub struct ExternalEditor;
impl ConditionalEventHandler for ExternalEditor {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        ctx: &EventContext,
    ) -> Option<Cmd> {
        if ctx.mode() == rustyline::EditMode::Vi
            && ctx.input_mode() == rustyline::InputMode::Command
            || ctx.mode() == rustyline::EditMode::Emacs && CTRLX_LOCK.load(Ordering::Relaxed) == 1
        {
            let editor = cached_statics(&EXTERNAL_EDITOR, || String::new());
            let mut file_path = env::temp_dir();
            file_path.push("otter-launcher");
            // Write the current line into the temporary file
            {
                let file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&file_path);

                write!(file.ok()?, "{}", ctx.line()).ok()?;
            }

            let exec_cmd = cached_statics(&EXEC_CMD, || "sh -c".to_string());
            let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
            let mut shell_cmd = Command::new(cmd_parts[0]);
            for arg in &cmd_parts[1..] {
                shell_cmd.arg(arg);
            }

            let _ = shell_cmd
                .arg(format!("{} {}", editor, &file_path.display().to_string()))
                .status();

            Some(Cmd::Replace(
                Movement::WholeLine,
                Some(
                    fs::read_to_string(&file_path)
                        .unwrap()
                        .trim_end()
                        .to_string(),
                ),
            ))
        } else {
            None
        }
    }
}

pub struct CTRLX;
impl ConditionalEventHandler for CTRLX {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        let ctrlx_lock = CTRLX_LOCK.load(Ordering::Relaxed);
        if ctrlx_lock == 0 {
            CTRLX_LOCK.store(1, Ordering::Relaxed);
            thread::spawn(|| {
                thread::sleep(Duration::from_millis(1500));
                CTRLX_LOCK.store(0, Ordering::Relaxed);
            });
        };
        None
    }
}

pub struct ListItemUp;
impl ConditionalEventHandler for ListItemUp {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        let selection_index = SELECTION_INDEX.load(Ordering::Relaxed);
        let hint_benchmark = HINT_BENCHMARK.load(Ordering::Relaxed);
        let selection_span = SELECTION_SPAN.load(Ordering::Relaxed);
        let suggestion_lines = SUGGESTION_LINES.load(Ordering::Relaxed);
        let filtered_hint_count = FILTERED_HINT_COUNT.load(Ordering::Relaxed);

        if selection_index > 1 {
            SELECTION_INDEX.fetch_sub(1, Ordering::Relaxed);
        } else if selection_index == 1 {
            if hint_benchmark == 0 {
                SELECTION_INDEX.store(0, Ordering::Relaxed);
            } else {
                HINT_BENCHMARK.fetch_sub(1, Ordering::Relaxed);
            }
        } else if selection_index == 0 {
            if filtered_hint_count > suggestion_lines {
                SELECTION_INDEX.store(selection_span, Ordering::Relaxed);
                HINT_BENCHMARK.store(filtered_hint_count - suggestion_lines, Ordering::Relaxed);
            } else {
                SELECTION_INDEX.store(selection_span, Ordering::Relaxed);
            }
        }
        Some(Cmd::Repaint)
    }
}

pub struct ListItemDown;
impl ConditionalEventHandler for ListItemDown {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        let selection_span = SELECTION_SPAN.load(Ordering::Relaxed);
        let suggestion_lines = SUGGESTION_LINES.load(Ordering::Relaxed);
        let hint_span = HINT_SPAN.load(Ordering::Relaxed);
        let selection_index = SELECTION_INDEX.load(Ordering::Relaxed);
        let hint_benchmark = HINT_BENCHMARK.load(Ordering::Relaxed);
        let filtered_hint_count = FILTERED_HINT_COUNT.load(Ordering::Relaxed);

        if hint_benchmark <= hint_span - suggestion_lines {
            if suggestion_lines == selection_span {
                if selection_index < selection_span {
                    SELECTION_INDEX.fetch_add(1, Ordering::Relaxed);
                } else if selection_index == selection_span {
                    if hint_benchmark < filtered_hint_count - suggestion_lines {
                        HINT_BENCHMARK.fetch_add(1, Ordering::Relaxed);
                    } else {
                        HINT_BENCHMARK.store(0, Ordering::Relaxed);
                        SELECTION_INDEX.store(0, Ordering::Relaxed);
                    }
                }
            } else if selection_index < selection_span {
                SELECTION_INDEX.fetch_add(1, Ordering::Relaxed);
            } else if selection_index == selection_span {
                SELECTION_INDEX.store(0, Ordering::Relaxed);
                HINT_BENCHMARK.store(0, Ordering::Relaxed);
            }
        } else if hint_benchmark == hint_span - suggestion_lines {
            SELECTION_INDEX.store(0, Ordering::Relaxed);
            HINT_BENCHMARK.store(0, Ordering::Relaxed);
        }
        Some(Cmd::Repaint)
    }
}

pub struct ViListItemJ;
impl ConditionalEventHandler for ViListItemJ {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        ctx: &EventContext,
    ) -> Option<Cmd> {
        if ctx.mode() == rustyline::EditMode::Vi
            && ctx.input_mode() == rustyline::InputMode::Command
        {
            let selection_span = SELECTION_SPAN.load(Ordering::Relaxed);
            let suggestion_lines = SUGGESTION_LINES.load(Ordering::Relaxed);
            let hint_span = HINT_SPAN.load(Ordering::Relaxed);
            let selection_index = SELECTION_INDEX.load(Ordering::Relaxed);
            let hint_benchmark = HINT_BENCHMARK.load(Ordering::Relaxed);
            let filtered_hint_count = FILTERED_HINT_COUNT.load(Ordering::Relaxed);

            if hint_benchmark <= hint_span - suggestion_lines {
                if suggestion_lines == selection_span {
                    if selection_index < selection_span {
                        SELECTION_INDEX.fetch_add(1, Ordering::Relaxed);
                    } else if selection_index == selection_span {
                        if hint_benchmark < filtered_hint_count - suggestion_lines {
                            HINT_BENCHMARK.fetch_add(1, Ordering::Relaxed);
                        } else {
                            HINT_BENCHMARK.store(0, Ordering::Relaxed);
                            SELECTION_INDEX.store(0, Ordering::Relaxed);
                        }
                    }
                } else if selection_index < selection_span {
                    SELECTION_INDEX.fetch_add(1, Ordering::Relaxed);
                } else if selection_index == selection_span {
                    SELECTION_INDEX.store(0, Ordering::Relaxed);
                    HINT_BENCHMARK.store(0, Ordering::Relaxed);
                }
            } else if hint_benchmark == hint_span - suggestion_lines {
                SELECTION_INDEX.store(0, Ordering::Relaxed);
                HINT_BENCHMARK.store(0, Ordering::Relaxed);
            }
            Some(Cmd::Repaint)
        } else {
            None
        }
    }
}

pub struct ViListItemK;
impl ConditionalEventHandler for ViListItemK {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        ctx: &EventContext,
    ) -> Option<Cmd> {
        if ctx.mode() == rustyline::EditMode::Vi
            && ctx.input_mode() == rustyline::InputMode::Command
        {
            let selection_index = SELECTION_INDEX.load(Ordering::Relaxed);
            let hint_benchmark = HINT_BENCHMARK.load(Ordering::Relaxed);
            let selection_span = SELECTION_SPAN.load(Ordering::Relaxed);
            let suggestion_lines = SUGGESTION_LINES.load(Ordering::Relaxed);
            let filtered_hint_count = FILTERED_HINT_COUNT.load(Ordering::Relaxed);

            if selection_index > 1 {
                SELECTION_INDEX.fetch_sub(1, Ordering::Relaxed);
            } else if selection_index == 1 {
                if hint_benchmark == 0 {
                    SELECTION_INDEX.store(0, Ordering::Relaxed);
                } else {
                    SELECTION_INDEX.fetch_sub(1, Ordering::Relaxed);
                }
            } else if selection_index == 0 {
                if filtered_hint_count > suggestion_lines {
                    SELECTION_INDEX.store(selection_span, Ordering::Relaxed);
                    HINT_BENCHMARK.store(filtered_hint_count - suggestion_lines, Ordering::Relaxed);
                } else {
                    SELECTION_INDEX.store(selection_span, Ordering::Relaxed);
                }
            }
            Some(Cmd::Repaint)
        } else {
            None
        }
    }
}

pub struct ListItemEnter;
impl ConditionalEventHandler for ListItemEnter {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        ctx: &EventContext,
    ) -> Option<Cmd> {
        if SELECTION_INDEX.load(Ordering::Relaxed) == 0 {
            Some(Cmd::AcceptLine)
        } else {
            let com_candidate = cached_statics(&COMPLETION_CANDIDATE, || String::new())
                .split_whitespace()
                .next()?
                .to_string();
            let target_module = config()
                .modules
                .iter()
                .find(|module| remove_ascii(&module.prefix) == com_candidate)
                .unwrap();
            Some(if target_module.with_argument.unwrap_or(false) == false {
                run_designated_module(String::new(), com_candidate);
                if LOOP_MODE.load(Ordering::Relaxed) == true {
                    SELECTION_INDEX.store(0, Ordering::Relaxed);
                    Cmd::Replace(Movement::WholeBuffer, Some(String::new()))
                } else {
                    Cmd::Interrupt
                }
            } else if ctx.pos() == ctx.line().len() {
                Cmd::Complete
            } else {
                Cmd::CompleteHint
            })
        }
    }
}

pub struct ListItemTab;
impl ConditionalEventHandler for ListItemTab {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        ctx: &EventContext,
    ) -> Option<Cmd> {
        Some(if ctx.pos() == ctx.line().len() {
            Cmd::Complete
        } else {
            Cmd::CompleteHint
        })
    }
}

pub struct ListItemSelect;
impl ConditionalEventHandler for ListItemSelect {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        if SELECTION_INDEX.load(Ordering::Relaxed) == 0 {
            Some(Cmd::Complete)
        } else {
            let com_candidate = cached_statics(&COMPLETION_CANDIDATE, || String::new())
                .split_whitespace()
                .next()?
                .to_string();
            let target_module = config()
                .modules
                .iter()
                .find(|module| remove_ascii(&module.prefix) == com_candidate)
                .unwrap();
            Some(if target_module.with_argument.unwrap_or(false) == false {
                run_designated_module(String::new(), com_candidate);
                if LOOP_MODE.load(Ordering::Relaxed) == true {
                    SELECTION_INDEX.store(0, Ordering::Relaxed);
                    Cmd::Replace(Movement::WholeBuffer, Some(String::new()))
                } else {
                    Cmd::Interrupt
                }
            } else {
                Cmd::Complete
            })
        }
    }
}

pub struct ListHome;
impl ConditionalEventHandler for ListHome {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        SELECTION_INDEX.store(0, Ordering::Relaxed);
        HINT_BENCHMARK.store(0, Ordering::Relaxed);
        Some(Cmd::Repaint)
    }
}

pub struct ListEnd;
impl ConditionalEventHandler for ListEnd {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        let suggestion_lines = SUGGESTION_LINES.load(Ordering::Relaxed);
        let hint_span = HINT_SPAN.load(Ordering::Relaxed);
        HINT_BENCHMARK.store(hint_span - suggestion_lines, Ordering::Relaxed);
        SELECTION_INDEX.store(SELECTION_SPAN.load(Ordering::Relaxed), Ordering::Relaxed);
        Some(Cmd::Repaint)
    }
}

pub struct ViListGgHome;
impl ConditionalEventHandler for ViListGgHome {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        ctx: &EventContext,
    ) -> Option<Cmd> {
        if ctx.mode() == rustyline::EditMode::Vi
            && ctx.input_mode() == rustyline::InputMode::Command
        {
            SELECTION_INDEX.store(0, Ordering::Relaxed);
            HINT_BENCHMARK.store(0, Ordering::Relaxed);
            Some(Cmd::Repaint)
        } else {
            None
        }
    }
}

pub struct ViListGEnd;
impl ConditionalEventHandler for ViListGEnd {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        ctx: &EventContext,
    ) -> Option<Cmd> {
        if ctx.mode() == rustyline::EditMode::Vi
            && ctx.input_mode() == rustyline::InputMode::Command
        {
            HINT_BENCHMARK.store(
                HINT_SPAN.load(Ordering::Relaxed) - SUGGESTION_LINES.load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
            SELECTION_INDEX.store(SELECTION_SPAN.load(Ordering::Relaxed), Ordering::Relaxed);
            Some(Cmd::Repaint)
        } else {
            None
        }
    }
}

pub struct ViListCtrlU;
impl ConditionalEventHandler for ViListCtrlU {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        ctx: &EventContext,
    ) -> Option<Cmd> {
        if ctx.mode() == rustyline::EditMode::Vi
            && ctx.input_mode() == rustyline::InputMode::Command
        {
            let suggestion_lines = SUGGESTION_LINES.load(Ordering::Relaxed);
            let hint_benchmark = HINT_BENCHMARK.load(Ordering::Relaxed);
            if hint_benchmark >= suggestion_lines {
                HINT_BENCHMARK.fetch_sub(suggestion_lines / 2, Ordering::Relaxed);
            } else if suggestion_lines >= hint_benchmark {
                HINT_BENCHMARK.store(0, Ordering::Relaxed);
                SELECTION_INDEX.store(0, Ordering::Relaxed);
            }
            Some(Cmd::Repaint)
        } else {
            None
        }
    }
}

pub struct ViListCtrlD;
impl ConditionalEventHandler for ViListCtrlD {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        ctx: &EventContext,
    ) -> Option<Cmd> {
        if ctx.mode() == rustyline::EditMode::Vi
            && ctx.input_mode() == rustyline::InputMode::Command
        {
            let suggestion_lines = SUGGESTION_LINES.load(Ordering::Relaxed);
            let hint_benchmark = HINT_BENCHMARK.load(Ordering::Relaxed);
            let hint_span = HINT_SPAN.load(Ordering::Relaxed);
            if hint_span - suggestion_lines > hint_benchmark {
                HINT_BENCHMARK.fetch_add(suggestion_lines / 2, Ordering::Relaxed);
            } else {
                HINT_BENCHMARK.store(hint_span - suggestion_lines, Ordering::Relaxed);
                SELECTION_INDEX.store(SELECTION_SPAN.load(Ordering::Relaxed), Ordering::Relaxed);
            }
            Some(Cmd::Repaint)
        } else {
            None
        }
    }
}

pub struct ListPageDown;
impl ConditionalEventHandler for ListPageDown {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        let suggestion_lines = SUGGESTION_LINES.load(Ordering::Relaxed);
        let hint_benchmark = HINT_BENCHMARK.load(Ordering::Relaxed);
        let hint_span = HINT_SPAN.load(Ordering::Relaxed);
        if hint_span - suggestion_lines > hint_benchmark {
            HINT_BENCHMARK.fetch_add(suggestion_lines, Ordering::Relaxed);
        } else {
            HINT_BENCHMARK.store(hint_span - suggestion_lines, Ordering::Relaxed);
            SELECTION_INDEX.store(SELECTION_SPAN.load(Ordering::Relaxed), Ordering::Relaxed);
        }
        Some(Cmd::Repaint)
    }
}

pub struct ListPageUp;
impl ConditionalEventHandler for ListPageUp {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        let suggestion_lines = SUGGESTION_LINES.load(Ordering::Relaxed);
        let hint_benchmark = HINT_BENCHMARK.load(Ordering::Relaxed);
        if hint_benchmark >= suggestion_lines {
            HINT_BENCHMARK.fetch_sub(suggestion_lines, Ordering::Relaxed);
        } else if suggestion_lines >= hint_benchmark {
            HINT_BENCHMARK.store(0, Ordering::Relaxed);
            SELECTION_INDEX.store(0, Ordering::Relaxed);
        }
        Some(Cmd::Repaint)
    }
}

pub fn customized_rustyline_editor()
-> Result<Editor<OtterHelper, DefaultHistory>, Box<dyn std::error::Error>> {
    let mut rl = Editor::new().unwrap();
    // set OtterHelper as hint and completion provider
    rl.set_helper(Some(OtterHelper {
        hints: map_hints()?,
    }));

    // check if esc_to_abort is on
    if ESC_TO_ABORT.load(Ordering::Relaxed) {
        rl.bind_sequence(
            KeyEvent::new('\x1b', Modifiers::NONE),
            EventHandler::Simple(Cmd::Interrupt),
        );
        rl.set_keyseq_timeout(Some(0));
    }

    // check if vi_mode is on, and set up keybinds accordingly
    if VI_MODE.load(Ordering::Relaxed) {
        rl.set_edit_mode(EditMode::Vi);
        // set vi bindings
        rl.bind_sequence(
            KeyEvent::new('G', Modifiers::NONE),
            EventHandler::Conditional(Box::from(ViListGEnd)),
        );
        rl.bind_sequence(
            KeyEvent::new('g', Modifiers::NONE),
            EventHandler::Conditional(Box::from(ViListGgHome)),
        );
        rl.bind_sequence(
            KeyEvent::new('j', Modifiers::NONE),
            EventHandler::Conditional(Box::from(ViListItemJ)),
        );
        rl.bind_sequence(
            KeyEvent::new('k', Modifiers::NONE),
            EventHandler::Conditional(Box::from(ViListItemK)),
        );
        rl.bind_sequence(
            KeyEvent::new('f', Modifiers::CTRL),
            EventHandler::Conditional(Box::from(ListPageDown)),
        );
        rl.bind_sequence(
            KeyEvent::new('d', Modifiers::CTRL),
            EventHandler::Conditional(Box::from(ViListCtrlD)),
        );
        rl.bind_sequence(
            KeyEvent::new('b', Modifiers::CTRL),
            EventHandler::Conditional(Box::from(ListPageUp)),
        );
        rl.bind_sequence(
            KeyEvent::new('u', Modifiers::CTRL),
            EventHandler::Conditional(Box::from(ViListCtrlU)),
        );
        if !cached_statics(&EXTERNAL_EDITOR, || String::new()).is_empty() {
            rl.bind_sequence(
                KeyEvent::new('v', Modifiers::NONE),
                EventHandler::Conditional(Box::from(ExternalEditor)),
            );
        }
    } else {
        // emacs bindings
        rl.bind_sequence(
            KeyEvent::new('<', Modifiers::ALT),
            EventHandler::Conditional(Box::from(ListHome)),
        );
        rl.bind_sequence(
            KeyEvent::new('>', Modifiers::ALT),
            EventHandler::Conditional(Box::from(ListEnd)),
        );
        rl.bind_sequence(
            KeyEvent::new('v', Modifiers::CTRL),
            EventHandler::Conditional(Box::from(ListPageDown)),
        );
        rl.bind_sequence(
            KeyEvent::new('v', Modifiers::ALT),
            EventHandler::Conditional(Box::from(ListPageUp)),
        );
        if !cached_statics(&EXTERNAL_EDITOR, || String::new()).is_empty() {
            rl.bind_sequence(
                KeyEvent::new('x', Modifiers::CTRL),
                EventHandler::Conditional(Box::from(CTRLX)),
            );
            rl.bind_sequence(
                KeyEvent::new('e', Modifiers::CTRL),
                EventHandler::Conditional(Box::from(ExternalEditor)),
            );
        }
    };

    // set shared keybinds (both vi and emacs) for list item selection
    rl.bind_sequence(
        KeyEvent::new('\r', Modifiers::NONE),
        EventHandler::Conditional(Box::from(ListItemEnter)),
    );
    rl.bind_sequence(
        KeyEvent::new('\r', Modifiers::ALT),
        EventHandler::Simple(Cmd::AcceptLine),
    );
    rl.bind_sequence(
        KeyEvent::new('\t', Modifiers::NONE),
        EventHandler::Conditional(Box::from(ListItemTab)),
    );
    rl.bind_sequence(
        KeyEvent::new('n', Modifiers::CTRL),
        EventHandler::Conditional(Box::from(ListItemDown)),
    );
    rl.bind_sequence(
        KeyEvent::new('p', Modifiers::CTRL),
        EventHandler::Conditional(Box::from(ListItemUp)),
    );
    rl.bind_sequence(
        KeyEvent(KeyCode::Down, Modifiers::NONE),
        EventHandler::Conditional(Box::from(ListItemDown)),
    );
    rl.bind_sequence(
        KeyEvent(KeyCode::Down, Modifiers::CTRL),
        EventHandler::Conditional(Box::from(ListItemDown)),
    );
    rl.bind_sequence(
        KeyEvent(KeyCode::Up, Modifiers::NONE),
        EventHandler::Conditional(Box::from(ListItemUp)),
    );
    rl.bind_sequence(
        KeyEvent(KeyCode::Up, Modifiers::CTRL),
        EventHandler::Conditional(Box::from(ListItemUp)),
    );
    rl.bind_sequence(
        KeyEvent(KeyCode::PageDown, Modifiers::NONE),
        EventHandler::Conditional(Box::from(ListPageDown)),
    );
    rl.bind_sequence(
        KeyEvent(KeyCode::PageUp, Modifiers::NONE),
        EventHandler::Conditional(Box::from(ListPageUp)),
    );
    rl.bind_sequence(
        KeyEvent(KeyCode::Right, Modifiers::CTRL),
        EventHandler::Conditional(Box::from(ListItemSelect)),
    );
    rl.bind_sequence(
        KeyEvent(KeyCode::Left, Modifiers::CTRL),
        EventHandler::Simple(Cmd::Kill(Movement::BackwardChar(1))),
    );
    rl.bind_sequence(
        KeyEvent(KeyCode::Right, Modifiers::NONE),
        EventHandler::Simple(Cmd::Move(Movement::ForwardChar(1))),
    );

    return Ok(rl);
}
