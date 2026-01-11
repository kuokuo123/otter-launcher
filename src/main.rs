//
//
//   ██████╗ ████████╗████████╗███████╗██████╗
//  ██╔═══██╗╚══██╔══╝╚══██╔══╝██╔════╝██╔══██╗
//  ██║   ██║   ██║      ██║   █████╗  ██████╔╝█████╗
//  ██║   ██║   ██║      ██║   ██╔══╝  ██╔══██╗╚════╝
//  ╚██████╔╝   ██║      ██║   ███████╗██║  ██║
//   ╚═════╝    ╚═╝      ╚═╝   ╚══════╝╚═╝  ╚═╝
//  ██╗      █████╗ ██╗   ██╗███╗   ██╗ ██████╗██╗  ██╗███████╗██████╗
//  ██║     ██╔══██╗██║   ██║████╗  ██║██╔════╝██║  ██║██╔════╝██╔══██╗
//  ██║     ███████║██║   ██║██╔██╗ ██║██║     ███████║█████╗  ██████╔╝
//  ██║     ██╔══██║██║   ██║██║╚██╗██║██║     ██╔══██║██╔══╝  ██╔══██╗
//  ███████╗██║  ██║╚██████╔╝██║ ╚████║╚██████╗██║  ██║███████╗██║  ██║
//  ╚══════╝╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═══╝ ╚═════╝╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝
//
// Terminal shell script launcher, written in rust
// Source Code: https://github.com/kuokuo123/otter-launcher
// GPL 3.0

//░█▀▀░█▀▄░█▀█░▀█▀░█▀▀░█▀▀
//░█░░░█▀▄░█▀█░░█░░█▀▀░▀▀█
//░▀▀▀░▀░▀░▀░▀░░▀░░▀▀▀░▀▀▀

use rustyline::{
    Cmd, ConditionalEventHandler, Context, EditMode, Editor, Event, EventContext, EventHandler,
    KeyCode, KeyEvent, Modifiers, Movement, RepeatCount,
    completion::{Completer, Pair},
    config::Configurer,
    highlight::Highlighter,
    hint::{Hint, Hinter},
    history::DefaultHistory,
};
use rustyline_derive::{Helper, Validator};
use serde::Deserialize;
use std::{
    borrow::Cow,
    collections::HashMap,
    env,
    error::Error,
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    path::Path,
    process::{self, Command, Stdio},
    str::from_utf8,
    sync::{Mutex, OnceLock},
    thread,
    time::Duration,
};
use urlencoding::encode;

//░█▀▀░█▀█░█▀█░█▀▀░▀█▀░█▀▀░░░█▀▀░▀█▀░█▀▄░█░█░█▀▀░▀█▀░█░█░█▀▄░█▀▀
//░█░░░█░█░█░█░█▀▀░░█░░█░█░░░▀▀█░░█░░█▀▄░█░█░█░░░░█░░█░█░█▀▄░█▀▀
//░▀▀▀░▀▀▀░▀░▀░▀░░░▀▀▀░▀▀▀░░░▀▀▀░░▀░░▀░▀░▀▀▀░▀▀▀░░▀░░▀▀▀░▀░▀░▀▀▀

// Define config structure
#[derive(Deserialize, Default)]
#[serde(default)]
struct Config {
    general: General,
    interface: Interface,
    overlay: Overlay,
    modules: Vec<Module>,
}

#[derive(Deserialize, Default)]
struct General {
    default_module: Option<String>,
    empty_module: Option<String>,
    exec_cmd: Option<String>,
    esc_to_abort: Option<bool>,
    cheatsheet_entry: Option<String>,
    cheatsheet_viewer: Option<String>,
    vi_mode: Option<bool>,
    clear_screen_after_execution: Option<bool>,
    loop_mode: Option<bool>,
    callback: Option<String>,
    external_editor: Option<String>,
    delay_startup: Option<usize>,
}

#[derive(Deserialize, Default)]
struct Interface {
    header: Option<String>,
    header_cmd: Option<String>,
    header_cmd_trimmed_lines: Option<usize>,
    separator: Option<String>,
    footer: Option<String>,
    list_prefix: Option<String>,
    selection_prefix: Option<String>,
    place_holder: Option<String>,
    default_module_message: Option<String>,
    empty_module_message: Option<String>,
    suggestion_mode: Option<String>,
    suggestion_lines: Option<usize>,
    indicator_no_arg_module: Option<String>,
    indicator_with_arg_module: Option<String>,
    prefix_padding: Option<usize>,
    prefix_color: Option<String>,
    description_color: Option<String>,
    place_holder_color: Option<String>,
    hint_color: Option<String>,
    move_interface_right: Option<usize>,
    move_interface_down: Option<usize>,
    customized_list_order: Option<bool>,
}

#[derive(Deserialize, Default)]
struct Overlay {
    overlay_cmd: Option<String>,
    overlay_trimmed_lines: Option<usize>,
    overlay_height: Option<usize>,
    move_overlay_right: Option<usize>,
    move_overlay_down: Option<usize>,
}

#[derive(Deserialize, Clone)]
struct Module {
    description: String,
    prefix: String,
    cmd: String,
    with_argument: Option<bool>,
    url_encode: Option<bool>,
    unbind_proc: Option<bool>,
}

// load toml config
static CONFIG: OnceLock<Config> = OnceLock::new();

fn load_config() -> Config {
    let home_dir = env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let xdg_config_path = format!("{}/.config/otter-launcher/config.toml", home_dir);

    let config_file = if Path::new(&xdg_config_path).exists() {
        xdg_config_path
    } else {
        "/etc/otter-launcher/config.toml".to_string()
    };

    let contents = fs::read_to_string(config_file).unwrap_or_default();
    toml::from_str(&contents).expect("cannot read contents from config_file")
}

#[inline]
fn config() -> &'static Config {
    CONFIG.get_or_init(load_config)
}

// use oncelock mutex to make important variables globally accessible (repeatedly used config values, list selection, and completion related stuff)
// define config variables as statics
static HEADER_CMD: OnceLock<Mutex<String>> = OnceLock::new();
static OVERLAY_CMD: OnceLock<Mutex<String>> = OnceLock::new();
static SUGGESTION_MODE: OnceLock<Mutex<String>> = OnceLock::new();
static LOOP_MODE: OnceLock<Mutex<bool>> = OnceLock::new();
static CALLBACK: OnceLock<Mutex<String>> = OnceLock::new();
static CHEATSHEET_ENTRY: OnceLock<Mutex<String>> = OnceLock::new();
static CHEATSHEET_VIEWER: OnceLock<Mutex<String>> = OnceLock::new();
static EXTERNAL_EDITOR: OnceLock<Mutex<String>> = OnceLock::new();
static VI_MODE: OnceLock<Mutex<bool>> = OnceLock::new();
static ESC_TO_ABORT: OnceLock<Mutex<bool>> = OnceLock::new();
static CLEAR_SCREEN_AFTER_EXECUTION: OnceLock<Mutex<bool>> = OnceLock::new();
static HEADER_CMD_TRIMMED_LINES: OnceLock<Mutex<usize>> = OnceLock::new();
static DELAY_STARTUP: OnceLock<Mutex<usize>> = OnceLock::new();
static OVERLAY_TRIMMED_LINES: OnceLock<Mutex<usize>> = OnceLock::new();
static OVERLAY_HEIGHT: OnceLock<Mutex<usize>> = OnceLock::new();
static HEADER: OnceLock<Mutex<String>> = OnceLock::new();
static SEPARATOR: OnceLock<Mutex<String>> = OnceLock::new();
static FOOTER: OnceLock<Mutex<String>> = OnceLock::new();
static EXEC_CMD: OnceLock<Mutex<String>> = OnceLock::new();
static DEFAULT_MODULE: OnceLock<Mutex<String>> = OnceLock::new();
static EMPTY_MODULE: OnceLock<Mutex<String>> = OnceLock::new();
static EMPTY_MODULE_MESSAGE: OnceLock<Mutex<String>> = OnceLock::new();
static DEFAULT_MODULE_MESSAGE: OnceLock<Mutex<String>> = OnceLock::new();
static SUGGESTION_LINES: OnceLock<Mutex<usize>> = OnceLock::new();
static PREFIX_PADDING: OnceLock<Mutex<usize>> = OnceLock::new();
static SELECTION_INDEX: OnceLock<Mutex<usize>> = OnceLock::new();
static SELECTION_SPAN: OnceLock<Mutex<usize>> = OnceLock::new();
static HINT_SPAN: OnceLock<Mutex<usize>> = OnceLock::new();
static HINT_BENCHMARK: OnceLock<Mutex<usize>> = OnceLock::new();
static LIST_PREFIX: OnceLock<Mutex<String>> = OnceLock::new();
static SELECTION_PREFIX: OnceLock<Mutex<String>> = OnceLock::new();
static PREFIX_COLOR: OnceLock<Mutex<String>> = OnceLock::new();
static DESCRIPTION_COLOR: OnceLock<Mutex<String>> = OnceLock::new();
static PLACE_HOLDER: OnceLock<Mutex<String>> = OnceLock::new();
static PLACE_HOLDER_COLOR: OnceLock<Mutex<String>> = OnceLock::new();
static HINT_COLOR: OnceLock<Mutex<String>> = OnceLock::new();
static INDICATOR_WITH_ARG_MODULE: OnceLock<Mutex<String>> = OnceLock::new();
static INDICATOR_NO_ARG_MODULE: OnceLock<Mutex<String>> = OnceLock::new();
static FILTERED_HINT_COUNT: OnceLock<Mutex<usize>> = OnceLock::new();
static HEADER_LINE_COUNT: OnceLock<Mutex<usize>> = OnceLock::new();
static COMPLETION_CANDIDATE: OnceLock<Mutex<String>> = OnceLock::new();
static LAYOUT_RIGHTWARD: OnceLock<Mutex<usize>> = OnceLock::new();
static LAYOUT_DOWNWARD: OnceLock<Mutex<usize>> = OnceLock::new();
static OVERLAY_RIGHTWARD: OnceLock<Mutex<usize>> = OnceLock::new();
static OVERLAY_DOWNWARD: OnceLock<Mutex<usize>> = OnceLock::new();
static CUSTOMIZED_LIST_ORDER: OnceLock<Mutex<bool>> = OnceLock::new();
static OVERLAY_LINES: OnceLock<Mutex<String>> = OnceLock::new();
static CELL_HEIGHT: OnceLock<usize> = OnceLock::new();
static SEPARATOR_COUNT: OnceLock<Mutex<usize>> = OnceLock::new();
static CTRLX_LOCK: OnceLock<Mutex<usize>> = OnceLock::new();

//░█░█░▀█▀░█▀█░▀█▀░░░▄▀░░░░█▀▀░█▀█░█▄█░█▀█░█░░░█▀▀░▀█▀░▀█▀░█▀█░█▀█
//░█▀█░░█░░█░█░░█░░░░▄█▀░░░█░░░█░█░█░█░█▀▀░█░░░█▀▀░░█░░░█░░█░█░█░█
//░▀░▀░▀▀▀░▀░▀░░▀░░░░░▀▀░░░▀▀▀░▀▀▀░▀░▀░▀░░░▀▀▀░▀▀▀░░▀░░▀▀▀░▀▀▀░▀░▀

// define the structure of every formatted hint
struct ModuleHint {
    display: String,
    completion: usize,
    w_arg: Option<bool>,
}

// define the functions for struct ModuleHint
impl ModuleHint {
    fn new(text: &str, completion: &str, w_arg: Option<bool>) -> Self {
        assert!(text.starts_with(completion));
        Self {
            display: text.into(),
            completion: completion.len(),
            w_arg: w_arg,
        }
    }
    fn suffix(&self, strip_chars: usize, padded_line_count: usize, footer: &str) -> Self {
        Self {
            display: self.display.to_owned() + &"\n ".repeat(padded_line_count) + footer,
            completion: strip_chars,
            w_arg: self.w_arg,
        }
    }
}

// define how the chosen hint is presented and completed in the rustyline editor
impl Hint for ModuleHint {
    // text to display when hint is active
    fn display(&self) -> &str {
        if cached_statics(&SUGGESTION_MODE, || "list".to_string()) == "hint" {
            // hint mode
            &self.display[self.completion..]
        } else {
            // list mode
            &self.display
        }
    }
    // hint completing function required by rustyline, not in use
    fn completion(&self) -> Option<&str> {
        None
    }
}

// define the helper that provide hints, highlights to the rustyline editor
#[derive(Helper, Validator)]
struct OtterHelper {
    hints: Vec<ModuleHint>,
}

// the completion functionality of OtterHelper
impl Completer for OtterHelper {
    type Candidate = Pair;
    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let com_candidate = cached_statics(&COMPLETION_CANDIDATE, || "".to_string());
        if cached_statics(&SUGGESTION_MODE, || "".to_string()) == "hint".to_string() {
            // define the behavior of completion in hint mode
            if pos <= com_candidate.len() && pos > 0 {
                // disable completion when the input texts is longer than the matched module prefix
                let cand = vec![Pair {
                    display: "".to_string(),
                    replacement: com_candidate + " ",
                }];
                Ok((0, cand))
            } else {
                // normal behavior
                let cand = vec![Pair {
                    display: "".to_string(),
                    replacement: "".to_string(),
                }];
                Ok((pos, cand))
            }
        } else {
            // the behavior in list mode
            if line.is_empty()
                && *SELECTION_INDEX
                    .get_or_init(|| Mutex::new(0))
                    .lock()
                    .unwrap()
                    == 0
            {
                // when empty, complete with empty module
                let cand = vec![Pair {
                    display: "".to_string(),
                    replacement: cached_statics(&EMPTY_MODULE, || "".to_string()) + " ",
                }];
                *SELECTION_INDEX
                    .get_or_init(|| Mutex::new(0))
                    .lock()
                    .unwrap() = 0;
                Ok((0, cand))
            } else if com_candidate == " " {
                // when no module is matched, complete with default module
                let cand = vec![Pair {
                    display: "".to_string(),
                    replacement: cached_statics(&DEFAULT_MODULE, || "".to_string()) + " ",
                }];
                *SELECTION_INDEX
                    .get_or_init(|| Mutex::new(0))
                    .lock()
                    .unwrap() = 0;
                Ok((0, cand))
            } else if pos == line.len() {
                // normal behavior
                let cand = vec![Pair {
                    display: "".to_string(),
                    replacement: com_candidate,
                }];
                *SELECTION_INDEX
                    .get_or_init(|| Mutex::new(0))
                    .lock()
                    .unwrap() = 0;
                Ok((0, cand))
            } else {
                let cand = vec![Pair {
                    display: "".to_string(),
                    replacement: "".to_string(),
                }];
                *SELECTION_INDEX
                    .get_or_init(|| Mutex::new(0))
                    .lock()
                    .unwrap() = 0;
                Ok((pos, cand))
            }
        }
    }
}

// the coloring functionality of OtterHelper
impl Highlighter for OtterHelper {
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        let default_module_message = cached_statics(&DEFAULT_MODULE_MESSAGE, || "".to_string());
        let empty_module_message = cached_statics(&EMPTY_MODULE_MESSAGE, || "".to_string());
        let description_color = cached_statics(&DESCRIPTION_COLOR, || "\x1b[39m".to_string());
        let place_holder = cached_statics(&PLACE_HOLDER, || "type something".to_string());
        let place_holder_color = cached_statics(&PLACE_HOLDER_COLOR, || "\x1b[30m".to_string());
        let hint_color = cached_statics(&HINT_COLOR, || "\x1b[30m".to_string());
        let suggestion_mode = cached_statics(&SUGGESTION_MODE, || "list".to_string());
        let list_prefix = cached_statics(&LIST_PREFIX, || "".to_string());
        let selection_prefix = cached_statics(&SELECTION_PREFIX, || ">".to_string());
        let prefix_color = cached_statics(&PREFIX_COLOR, || "".to_string());
        let prefix_width = cached_statics(&PREFIX_PADDING, || 0);
        let suggestion_lines = cached_statics(&SUGGESTION_LINES, || 0);
        let mut selection_index = SELECTION_INDEX
            .get_or_init(|| Mutex::new(0))
            .lock()
            .unwrap();
        let mut selection_span = SELECTION_SPAN.get_or_init(|| Mutex::new(0)).lock().unwrap();
        let mut hint_benchmark = HINT_BENCHMARK.get_or_init(|| Mutex::new(0)).lock().unwrap();
        let filtered_hint_count = FILTERED_HINT_COUNT
            .get_or_init(|| Mutex::new(0))
            .lock()
            .unwrap();
        let separator_count = SEPARATOR_COUNT
            .get_or_init(|| Mutex::new(0))
            .lock()
            .unwrap();
        let layout_right = cached_statics(&LAYOUT_RIGHTWARD, || 0);
        let overlay_lines = cached_statics(&OVERLAY_LINES, || "".to_string());
        let overlay_right = cached_statics(&OVERLAY_RIGHTWARD, || 0);
        let overlay_down_cached = cached_statics(&OVERLAY_DOWNWARD, || 0);
        let overlay_up = format!(
            "\x1b[{}A",
            hint.lines().collect::<Vec<&str>>().len()
                + *HEADER_LINE_COUNT
                    .get_or_init(|| Mutex::new(0))
                    .lock()
                    .unwrap()
                - 2
        );
        let overlay_down = if overlay_down_cached == 0 {
            String::new()
        } else {
            format!("\x1b[{}B", overlay_down_cached)
        };

        if suggestion_mode == "hint" {
            (format!(
                "\x1b[0m{}{}\x1b[0m\x1b[s{}{}\x1b[{}G",
                hint_color,
                hint,
                overlay_up,
                overlay_down,
                overlay_right + 1
            ) + &overlay_lines
                + "\x1b[u\x1b[?25h")
                .into()
        } else {
            // shrink selection span if filtered_hint_count shrinks
            if suggestion_lines >= *filtered_hint_count {
                *selection_span = *filtered_hint_count;
            } else {
                // or set it the same as the page length
                *selection_span = suggestion_lines;
            }

            // set selection index back to 0 if it is beyond the range of filtered items
            if *hint_benchmark > *filtered_hint_count || *selection_index > *filtered_hint_count {
                *hint_benchmark = 0;
                *selection_index = 0;
            }

            // format every line
            let aggregated_hint_lines = hint
                .lines()
                .enumerate()
                .map(|(index, line)| {
                    if index == *selection_index + *separator_count && *selection_index > 0 {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            format!(
                                "\x1B[{}G{}{}{:prefix_width$} {}{}{}",
                                layout_right + 1,
                                selection_prefix,
                                prefix_color,
                                parts[0],
                                description_color,
                                parts[1..].join(" "),
                                "\x1b[0m"
                            )
                        } else {
                            format!("\x1b[{}G{}", layout_right + 1, line)
                        }
                    } else if line == place_holder {
                        format!("{}{}{}", place_holder_color, place_holder, "\x1b[0m")
                    } else if (default_module_message.contains(line)
                        || empty_module_message.contains(line))
                        && !line.is_empty()
                    {
                        format!("\x1b[{}G{}", layout_right + 1, line)
                    } else if index <= *separator_count {
                        line.to_string()
                    } else if index > *separator_count + *selection_span
                        && cached_statics(&FOOTER, || String::new()).contains(line)
                    {
                        line.to_string()
                    } else {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            format!(
                                "\x1B[{}G{}{}{:prefix_width$} {}{}{}",
                                layout_right + 1,
                                list_prefix,
                                prefix_color,
                                parts[0],
                                description_color,
                                parts[1..].join(" "),
                                "\x1b[0m"
                            )
                        } else {
                            format!("\x1b[{}G{}", layout_right + 1, line)
                        }
                    }
                })
                .collect::<Vec<String>>()
                .join("\n")
                + &format!(
                    "\x1b[s{}{}\x1b[{}G",
                    overlay_up,
                    overlay_down,
                    overlay_right + 1
                )
                + &overlay_lines
                + "\x1b[u\x1b[?25h";

            return aggregated_hint_lines.into();
        }
    }
}

// the hint providing functionality of OtterHelper
// select a hint for OtterHelper to pass into rustyline prompt editor (from a vector of all formatted hints)
impl Hinter for OtterHelper {
    type Hint = ModuleHint;
    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<ModuleHint> {
        *HINT_SPAN.get_or_init(|| Mutex::new(0)).lock().unwrap() = self.hints.len();
        let suggestion_mode = cached_statics(&SUGGESTION_MODE, || "list".to_string());
        let place_holder = cached_statics(&PLACE_HOLDER, || "type something".to_string());
        let cheatsheet_entry = cached_statics(&CHEATSHEET_ENTRY, || "?".to_string());
        let indicator_no_arg_module = cached_statics(&INDICATOR_NO_ARG_MODULE, || "".to_string());
        let suggestion_lines = cached_statics(&SUGGESTION_LINES, || 1);
        let hint_benchmark = *HINT_BENCHMARK.get_or_init(|| Mutex::new(0)).lock().unwrap();
        let overlay_down = cached_statics(&OVERLAY_DOWNWARD, || 0);
        let header_line_count = *HEADER_LINE_COUNT
            .get_or_init(|| Mutex::new(0))
            .lock()
            .unwrap();

        // form separator lines, if any
        let mut separator_lines = cached_statics(&SEPARATOR, || String::new());
        if separator_lines.is_empty() {
            separator_lines = "".to_string();
            *SEPARATOR_COUNT
                .get_or_init(|| Mutex::new(0))
                .lock()
                .unwrap() = 0;
        } else {
            let expanded_separator = expand_env_vars(&separator_lines);
            let prepared_separator_lines: Vec<&str> = expanded_separator.split('\n').collect();
            *SEPARATOR_COUNT
                .get_or_init(|| Mutex::new(0))
                .lock()
                .unwrap() = prepared_separator_lines.len();
            separator_lines = format!("\n{}", prepared_separator_lines.join("\n"));
        }

        // form footer lines, if any
        let mut footer_lines = cached_statics(&FOOTER, || String::new());
        if footer_lines.is_empty() {
            footer_lines = "".to_string();
        } else {
            let expanded_footer = expand_env_vars(&footer_lines);
            let prepared_footer_lines: Vec<&str> = expanded_footer.split('\n').collect();
            footer_lines = format!("\x1b[0m\n{}", prepared_footer_lines.join("\n"));
        }

        // print from overlay commands, if any
        let overlay_cmd = cached_statics(&OVERLAY_CMD, || "".to_string());
        let overlay_lines = if !overlay_cmd.is_empty() {
            let overlay_right = cached_statics(&OVERLAY_RIGHTWARD, || 0);
            let exec_cmd = cached_statics(&EXEC_CMD, || "sh -c".to_string());
            let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
            let mut shell_cmd = Command::new(cmd_parts[0]);
            for arg in &cmd_parts[1..] {
                shell_cmd.arg(arg);
            }
            let output = shell_cmd
                .arg(&overlay_cmd)
                .output()
                .expect("Failed to launch overlay command...");
            let remove_lines_count = cached_statics(&OVERLAY_TRIMMED_LINES, || 0);
            let overlay_cmd_stdout = from_utf8(&output.stdout).unwrap();
            let lines: Vec<&str> = overlay_cmd_stdout.lines().collect();
            let lines_count = lines.len();
            if lines_count > remove_lines_count {
                lines[..lines_count - remove_lines_count]
                    .join(&format!("\n\x1b[{}G", overlay_right + 1))
            } else {
                "not enough lines of overlay_cmd output to be trimmed".to_string()
            }
        } else {
            "".to_string()
        };

        // store overlay lines into universial var, prep for highlighter use
        *OVERLAY_LINES
            .get_or_init(|| Mutex::new("".to_string()))
            .lock()
            .unwrap() = overlay_lines.clone();

        // measure overlay row height, using either kitty or sixel or raw lines
        let overlay_height_cached = cached_statics(&OVERLAY_HEIGHT, || 0);
        let overlay_height = if overlay_height_cached == 0 {
            let overlay_line_count = overlay_lines.lines().count();
            if let Some(r) = kitty_rows(&overlay_lines) {
                r + overlay_line_count - 1
            } else if let Some(r) = sixel_rows(&overlay_lines) {
                // convert pixels -> terminal rows using ceil
                let term_cell_height = term_cell_height_cached().unwrap_or(22);
                r * 6 / term_cell_height + overlay_line_count - 1
            } else {
                overlay_line_count
            }
        } else {
            let overlay_line_count = overlay_lines.lines().count();
            if overlay_height_cached >= overlay_line_count {
                overlay_height_cached
            } else {
                overlay_line_count
            }
        };

        // calculate overlay padding, to maintain layout when printing at window bottom
        let mut padded_line_count = if overlay_height + overlay_down > header_line_count {
            overlay_height - header_line_count + overlay_down
        } else {
            0
        };

        // hint mode behavior
        if suggestion_mode == "hint" {
            let foot_lines_hint_mode = footer_lines.lines().collect::<Vec<&str>>().join("\n");
            if line.is_empty() {
                // when nothing is typed
                *COMPLETION_CANDIDATE
                    .get_or_init(|| Mutex::new("".to_string()))
                    .lock()
                    .unwrap() = "".to_string();
                Some(ModuleHint {
                    display: format!(
                        "{}{}{}",
                        place_holder,
                        "\n ".repeat(padded_line_count),
                        foot_lines_hint_mode
                    ),
                    completion: 0,
                    w_arg: None,
                })
            } else if line.trim_end() == cheatsheet_entry {
                // when cheatsheet_entry is typed
                *COMPLETION_CANDIDATE
                    .get_or_init(|| Mutex::new("".to_string()))
                    .lock()
                    .unwrap() = "?".to_string();
                Some(ModuleHint {
                    display: format!(
                        "{} {}{}{}{}",
                        cheatsheet_entry,
                        indicator_no_arg_module,
                        "cheat sheet",
                        "\n ".repeat(padded_line_count),
                        foot_lines_hint_mode
                    )
                    .to_string(),
                    completion: line.len(),
                    w_arg: None,
                })
            } else {
                // when something's typed
                Some(
                    self.hints
                        .iter()
                        .filter_map(|i| {
                            let adjusted_line = &line.replace(" ", "\n");

                            // match typed texts with hint objectss
                            if !adjusted_line.trim_end().is_empty()
                                && remove_ascii(&i.display).starts_with(adjusted_line.trim_end())
                            {
                                // set the first matched prefix as completion candidate
                                *COMPLETION_CANDIDATE
                                    .get_or_init(|| Mutex::new("".to_string()))
                                    .lock()
                                    .unwrap() = i
                                    .display
                                    .split_whitespace()
                                    .next()
                                    .unwrap_or("")
                                    .to_string();
                                // provide the found hint
                                Some(i.suffix(line.len(), padded_line_count, &foot_lines_hint_mode))
                            } else {
                                *COMPLETION_CANDIDATE
                                    .get_or_init(|| Mutex::new("".to_string()))
                                    .lock()
                                    .unwrap() = "".to_string();
                                None
                            }
                        })
                        .next()
                        .unwrap_or(ModuleHint {
                            display: format!(
                                "\x1b[0m{}{}",
                                "\n ".repeat(padded_line_count),
                                foot_lines_hint_mode
                            ),
                            completion: 0,
                            w_arg: None,
                        }),
                )
            }
        } else {
            // list mode behavior
            let e_module =
                expand_env_vars(&cached_statics(&EMPTY_MODULE_MESSAGE, || "".to_string()));
            let d_module =
                expand_env_vars(&cached_statics(&DEFAULT_MODULE_MESSAGE, || "".to_string()));
            let selection_index = SELECTION_INDEX
                .get_or_init(|| Mutex::new(0))
                .lock()
                .unwrap();

            // aggregate all the matched hint objects to form a single line that is presented as a list
            let mut aggregated_lines = self
                .hints
                .iter()
                .filter_map(|i| {
                    // set different behavior for modules with/without arguments
                    let adjusted_line = if i.w_arg.unwrap_or(false) {
                        if line.contains(" ") {
                            line.split_whitespace().next().unwrap_or("").to_owned() + " "
                        } else {
                            line.to_string()
                        }
                    } else {
                        line.replace(" ", "\n")
                    };

                    if remove_ascii(&i.display).contains(&adjusted_line.trim_end()) {
                        Some(i.display.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<&str>>(); // Collect the filtered results into a vector

            if cached_statics(&CUSTOMIZED_LIST_ORDER, || false) == false {
                // sort list items alphebetically
                aggregated_lines.sort_unstable();
            }
            // partition list items into those that start with input texts and others
            let partitioned_lines =
                aggregated_lines
                    .into_iter()
                    .partition::<Vec<&str>, _>(|display| {
                        display.starts_with(&line.split_whitespace().next().unwrap_or(""))
                    });
            // move the first group forward
            let mut filtered_items = partitioned_lines.0;
            filtered_items.extend(partitioned_lines.1);

            // make the number of filtered items globally accessible
            *FILTERED_HINT_COUNT
                .get_or_init(|| Mutex::new(0))
                .lock()
                .unwrap() = filtered_items.len();

            // Check if there are enough filtered items after the skip
            let agg_line = if hint_benchmark + suggestion_lines
                > *FILTERED_HINT_COUNT
                    .get_or_init(|| Mutex::new(0))
                    .lock()
                    .unwrap()
            {
                // If not enough, default to taking from the start
                let join_range = &filtered_items[..usize::min(
                    suggestion_lines,
                    *FILTERED_HINT_COUNT
                        .get_or_init(|| Mutex::new(0))
                        .lock()
                        .unwrap(),
                )];
                join_range.join("\n")
            } else {
                // If there are enough to take
                let join_range = filtered_items
                    .iter()
                    .skip(hint_benchmark)
                    .take(suggestion_lines)
                    .copied()
                    .collect::<Vec<_>>();

                // calculate overlay padding, to maintain layout when printing at window bottom
                let join_range_count = join_range.len();

                padded_line_count = if overlay_height + overlay_down
                    > header_line_count
                        + join_range_count
                        + *SEPARATOR_COUNT
                            .get_or_init(|| Mutex::new(0))
                            .lock()
                            .unwrap()
                {
                    overlay_height + overlay_down
                        - header_line_count
                        - join_range_count
                        - *SEPARATOR_COUNT
                            .get_or_init(|| Mutex::new(0))
                            .lock()
                            .unwrap()
                } else {
                    0
                };
                join_range.join("\n")
            };

            // set completion candidate according to list selection index
            *COMPLETION_CANDIDATE
                .get_or_init(|| Mutex::new("".to_string()))
                .lock()
                .unwrap() = if *selection_index == 0 {
                agg_line
                    .lines()
                    .nth(0)
                    .unwrap_or("")
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string()
                    + " "
            } else {
                agg_line
                    .lines()
                    .nth(*selection_index - 1)
                    .unwrap_or("")
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_string()
                    + " "
            };

            // format the aggregated hint lines as the single hint object to be presented
            if line.is_empty() {
                // if nothing has been typed
                Some(ModuleHint {
                    display: format!(
                        "{}{}{}{}",
                        // show place holder first
                        place_holder,
                        separator_lines,
                        // if empty module is set && no module selected
                        if !e_module.is_empty() && *selection_index == 0 {
                            // calculate overlay padding, to maintain layout when printing at window bottom
                            let empty_message_count = e_module.lines().count();
                            let padded_line_count_local = if overlay_height + overlay_down
                                > header_line_count
                                    + empty_message_count
                                    + *SEPARATOR_COUNT
                                        .get_or_init(|| Mutex::new(0))
                                        .lock()
                                        .unwrap()
                            {
                                overlay_height + overlay_down
                                    - header_line_count
                                    - empty_message_count
                                    - *SEPARATOR_COUNT
                                        .get_or_init(|| Mutex::new(0))
                                        .lock()
                                        .unwrap()
                            } else {
                                0
                            };
                            // if empty module is set
                            format!("\n{}{}", e_module, "\n ".repeat(padded_line_count_local))
                        } else {
                            if agg_line.is_empty() {
                                format!("{}", "\x1b[0mn")
                            } else {
                                format!("\n{}{}", agg_line, "\n ".repeat(padded_line_count))
                            }
                        },
                        footer_lines
                    ),
                    completion: pos,
                    w_arg: None,
                })
            } else {
                // if something is typed
                let agg_count = agg_line.lines().collect::<Vec<&str>>().len();
                Some(ModuleHint {
                    display: (if line.trim_end() == cheatsheet_entry {
                        *COMPLETION_CANDIDATE
                            .get_or_init(|| Mutex::new("".to_string()))
                            .lock()
                            .unwrap() = "? ".to_string();
                        let cheatsheet_count = cheatsheet_entry.lines().count();
                        let padded_line_count_local = if overlay_height
                            + overlay_down
                            + *SEPARATOR_COUNT
                                .get_or_init(|| Mutex::new(0))
                                .lock()
                                .unwrap()
                            > header_line_count + cheatsheet_count
                        {
                            overlay_height + overlay_down
                                - header_line_count
                                - cheatsheet_count
                                - *SEPARATOR_COUNT
                                    .get_or_init(|| Mutex::new(0))
                                    .lock()
                                    .unwrap()
                        } else {
                            0
                        };
                        format!(
                            "{}\n{} {} {}{}{}",
                            separator_lines,
                            cheatsheet_entry,
                            indicator_no_arg_module,
                            "cheat sheet",
                            "\n ".repeat(padded_line_count_local),
                            footer_lines
                        )
                    // if no module is matched
                    } else if agg_line.is_empty() {
                        // check if default module message is set
                        if d_module.is_empty() {
                            format!("\x1b[0m{}", separator_lines)
                        } else {
                            let default_message_count = d_module.lines().count();
                            let padded_line_count_local = if overlay_height + overlay_down
                                > header_line_count
                                    + default_message_count
                                    + *SEPARATOR_COUNT
                                        .get_or_init(|| Mutex::new(0))
                                        .lock()
                                        .unwrap()
                            {
                                overlay_height + overlay_down
                                    - header_line_count
                                    - default_message_count
                                    - *SEPARATOR_COUNT
                                        .get_or_init(|| Mutex::new(0))
                                        .lock()
                                        .unwrap()
                            } else {
                                0
                            };
                            format!(
                                "{}\n{}{}{}",
                                separator_lines,
                                d_module,
                                "\n ".repeat(padded_line_count_local),
                                footer_lines
                            )
                        }
                    // if some module is matched
                    } else {
                        let padded_line_count_local = if overlay_height + overlay_down
                            > header_line_count
                                + agg_count
                                + *SEPARATOR_COUNT
                                    .get_or_init(|| Mutex::new(0))
                                    .lock()
                                    .unwrap()
                        {
                            overlay_height + overlay_down
                                - header_line_count
                                - agg_count
                                - *SEPARATOR_COUNT
                                    .get_or_init(|| Mutex::new(0))
                                    .lock()
                                    .unwrap()
                        } else {
                            0
                        };
                        format!(
                            "{}\n{}{}{}",
                            separator_lines,
                            agg_line,
                            "\n ".repeat(padded_line_count_local),
                            footer_lines
                        )
                    })
                    .to_string(),
                    completion: pos,
                    w_arg: None,
                })
            }
        }
    }
}

//░█░█░█▀▀░█░█░█▀▄░▀█▀░█▀█░█▀▄░▀█▀░█▀█░█▀▀░█▀▀
//░█▀▄░█▀▀░░█░░█▀▄░░█░░█░█░█░█░░█░░█░█░█░█░▀▀█
//░▀░▀░▀▀▀░░▀░░▀▀░░▀▀▀░▀░▀░▀▀░░▀▀▀░▀░▀░▀▀▀░▀▀▀

struct ExternalEditor;
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
            || ctx.mode() == rustyline::EditMode::Emacs
                && *CTRLX_LOCK.get_or_init(|| Mutex::new(0)).lock().unwrap() == 1
        {
            let editor = cached_statics(&EXTERNAL_EDITOR, || "".to_string());
            let mut file_path = env::temp_dir();
            file_path.push("otter-launcher");
            // Write the current line into the temporary file
            {
                let file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&file_path);

                write!(file.expect("cannot write to temp file"), "{}", ctx.line())
                    .expect("failed when writing to the temp file");
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

struct CTRLX;
impl ConditionalEventHandler for CTRLX {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        let mut ctrlx_lock = CTRLX_LOCK.get_or_init(|| Mutex::new(0)).lock().unwrap();
        if *ctrlx_lock == 0 {
            *ctrlx_lock = 1;
            thread::spawn(|| {
                thread::sleep(Duration::from_millis(1500));
                *CTRLX_LOCK.get().unwrap().lock().unwrap() = 0;
            });
        };
        None
    }
}

struct ListItemUp;
impl ConditionalEventHandler for ListItemUp {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        let mut selection_index = SELECTION_INDEX
            .get_or_init(|| Mutex::new(0))
            .lock()
            .unwrap();
        let mut hint_benchmark = HINT_BENCHMARK.get_or_init(|| Mutex::new(0)).lock().unwrap();
        let selection_span = SELECTION_SPAN.get_or_init(|| Mutex::new(0)).lock().unwrap();
        let suggestion_lines = cached_statics(&SUGGESTION_LINES, || 0);
        let filtered_hint_count = FILTERED_HINT_COUNT
            .get_or_init(|| Mutex::new(0))
            .lock()
            .unwrap();

        if *selection_index > 1 {
            *selection_index -= 1;
        } else if *selection_index == 1 {
            if *hint_benchmark == 0 {
                *selection_index = 0;
            } else {
                *hint_benchmark -= 1;
            }
        } else if *selection_index == 0 {
            if *filtered_hint_count > suggestion_lines {
                *selection_index = *selection_span;
                *hint_benchmark = *filtered_hint_count - suggestion_lines;
            } else {
                *selection_index = *selection_span;
            }
        }
        Some(Cmd::Repaint)
    }
}

struct ListItemDown;
impl ConditionalEventHandler for ListItemDown {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        let selection_span = SELECTION_SPAN.get_or_init(|| Mutex::new(0)).lock().unwrap();
        let suggestion_lines = cached_statics(&SUGGESTION_LINES, || 0);
        let hint_span = HINT_SPAN.get_or_init(|| Mutex::new(0)).lock().unwrap();
        let mut selection_index = SELECTION_INDEX
            .get_or_init(|| Mutex::new(0))
            .lock()
            .unwrap();
        let mut hint_benchmark = HINT_BENCHMARK.get_or_init(|| Mutex::new(0)).lock().unwrap();
        let filtered_hint_count = FILTERED_HINT_COUNT
            .get_or_init(|| Mutex::new(0))
            .lock()
            .unwrap();

        if *hint_benchmark <= *hint_span - suggestion_lines {
            if suggestion_lines == *selection_span {
                if *selection_index < *selection_span {
                    *selection_index += 1;
                } else if *selection_index == *selection_span {
                    if *hint_benchmark < *filtered_hint_count - suggestion_lines {
                        *hint_benchmark += 1;
                    } else {
                        *hint_benchmark = 0;
                        *selection_index = 0;
                    }
                }
            } else if *selection_index < *selection_span {
                *selection_index += 1;
            } else if *selection_index == *selection_span {
                *selection_index = 0;
                *hint_benchmark = 0;
            }
        } else if *hint_benchmark == *hint_span - suggestion_lines {
            *selection_index = 0;
            *hint_benchmark = 0;
        }
        Some(Cmd::Repaint)
    }
}

struct ViListItemJ;
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
            let selection_span = SELECTION_SPAN.get_or_init(|| Mutex::new(0)).lock().unwrap();
            let suggestion_lines = cached_statics(&SUGGESTION_LINES, || 0);
            let hint_span = HINT_SPAN.get_or_init(|| Mutex::new(0)).lock().unwrap();
            let mut selection_index = SELECTION_INDEX
                .get_or_init(|| Mutex::new(0))
                .lock()
                .unwrap();
            let mut hint_benchmark = HINT_BENCHMARK.get_or_init(|| Mutex::new(0)).lock().unwrap();
            let filtered_hint_count = FILTERED_HINT_COUNT
                .get_or_init(|| Mutex::new(0))
                .lock()
                .unwrap();

            if *hint_benchmark <= *hint_span - suggestion_lines {
                if suggestion_lines == *selection_span {
                    if *selection_index < *selection_span {
                        *selection_index += 1;
                    } else if *selection_index == *selection_span {
                        if *hint_benchmark < *filtered_hint_count - suggestion_lines {
                            *hint_benchmark += 1;
                        } else {
                            *hint_benchmark = 0;
                            *selection_index = 0;
                        }
                    }
                } else if *selection_index < *selection_span {
                    *selection_index += 1;
                } else if *selection_index == *selection_span {
                    *selection_index = 0;
                    *hint_benchmark = 0;
                }
            } else if *hint_benchmark == *hint_span - suggestion_lines {
                *selection_index = 0;
                *hint_benchmark = 0;
            }
            Some(Cmd::Repaint)
        } else {
            None
        }
    }
}

struct ViListItemK;
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
            let mut selection_index = SELECTION_INDEX
                .get_or_init(|| Mutex::new(0))
                .lock()
                .unwrap();
            let mut hint_benchmark = HINT_BENCHMARK.get_or_init(|| Mutex::new(0)).lock().unwrap();
            let selection_span = SELECTION_SPAN.get_or_init(|| Mutex::new(0)).lock().unwrap();
            let suggestion_lines = cached_statics(&SUGGESTION_LINES, || 0);
            let filtered_hint_count = FILTERED_HINT_COUNT
                .get_or_init(|| Mutex::new(0))
                .lock()
                .unwrap();

            if *selection_index > 1 {
                *selection_index -= 1;
            } else if *selection_index == 1 {
                if *hint_benchmark == 0 {
                    *selection_index = 0;
                } else {
                    *hint_benchmark -= 1;
                }
            } else if *selection_index == 0 {
                if *filtered_hint_count > suggestion_lines {
                    *selection_index = *selection_span;
                    *hint_benchmark = *filtered_hint_count - suggestion_lines;
                } else {
                    *selection_index = *selection_span;
                }
            }
            Some(Cmd::Repaint)
        } else {
            None
        }
    }
}

struct ListItemEnter;
impl ConditionalEventHandler for ListItemEnter {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        ctx: &EventContext,
    ) -> Option<Cmd> {
        if *SELECTION_INDEX
            .get_or_init(|| Mutex::new(0))
            .lock()
            .unwrap()
            == 0
        {
            Some(Cmd::AcceptLine)
        } else {
            let com_candidate = cached_statics(&COMPLETION_CANDIDATE, || "".to_string())
                .split_whitespace()
                .next()?
                .to_string();
            let target_module = config()
                .modules
                .iter()
                .find(|module| remove_ascii(&module.prefix) == com_candidate)
                .unwrap();
            Some(if target_module.with_argument.unwrap_or(false) == false {
                run_designated_module("".to_string(), com_candidate);
                if cached_statics(&LOOP_MODE, || false) == true {
                    *SELECTION_INDEX
                        .get_or_init(|| Mutex::new(0))
                        .lock()
                        .unwrap() = 0;
                    Cmd::Replace(Movement::WholeBuffer, Some("".to_string()))
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

struct ListItemTab;
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

struct ListItemSelect;
impl ConditionalEventHandler for ListItemSelect {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        if *SELECTION_INDEX
            .get_or_init(|| Mutex::new(0))
            .lock()
            .unwrap()
            == 0
        {
            Some(Cmd::Complete)
        } else {
            let com_candidate = cached_statics(&COMPLETION_CANDIDATE, || "".to_string())
                .split_whitespace()
                .next()?
                .to_string();
            let target_module = config()
                .modules
                .iter()
                .find(|module| remove_ascii(&module.prefix) == com_candidate)
                .unwrap();
            Some(if target_module.with_argument.unwrap_or(false) == false {
                run_designated_module("".to_string(), com_candidate);
                if cached_statics(&LOOP_MODE, || false) == true {
                    *SELECTION_INDEX
                        .get_or_init(|| Mutex::new(0))
                        .lock()
                        .unwrap() = 0;
                    Cmd::Replace(Movement::WholeBuffer, Some("".to_string()))
                } else {
                    Cmd::Interrupt
                }
            } else {
                Cmd::Complete
            })
        }
    }
}

struct ListHome;
impl ConditionalEventHandler for ListHome {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        *SELECTION_INDEX
            .get_or_init(|| Mutex::new(0))
            .lock()
            .unwrap() = 0;
        *HINT_BENCHMARK.get_or_init(|| Mutex::new(0)).lock().unwrap() = 0;
        Some(Cmd::Repaint)
    }
}

struct ListEnd;
impl ConditionalEventHandler for ListEnd {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        let suggestion_lines = cached_statics(&SUGGESTION_LINES, || 0);
        let mut hint_benchmark = HINT_BENCHMARK.get_or_init(|| Mutex::new(0)).lock().unwrap();
        let hint_span = HINT_SPAN.get_or_init(|| Mutex::new(0)).lock().unwrap();
        *hint_benchmark = *hint_span - suggestion_lines;
        *SELECTION_INDEX
            .get_or_init(|| Mutex::new(0))
            .lock()
            .unwrap() = *SELECTION_SPAN.get_or_init(|| Mutex::new(0)).lock().unwrap();
        Some(Cmd::Repaint)
    }
}

struct ViListGgHome;
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
            *SELECTION_INDEX
                .get_or_init(|| Mutex::new(0))
                .lock()
                .unwrap() = 0;
            *HINT_BENCHMARK.get_or_init(|| Mutex::new(0)).lock().unwrap() = 0;
            Some(Cmd::Repaint)
        } else {
            None
        }
    }
}

struct ViListGEnd;
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
            let mut hint_benchmark = HINT_BENCHMARK.get_or_init(|| Mutex::new(0)).lock().unwrap();
            *hint_benchmark = *HINT_SPAN.get_or_init(|| Mutex::new(0)).lock().unwrap()
                - cached_statics(&SUGGESTION_LINES, || 0);
            *SELECTION_INDEX
                .get_or_init(|| Mutex::new(0))
                .lock()
                .unwrap() = *SELECTION_SPAN.get_or_init(|| Mutex::new(0)).lock().unwrap();
            Some(Cmd::Repaint)
        } else {
            None
        }
    }
}

struct ViListCtrlU;
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
            let suggestion_lines = cached_statics(&SUGGESTION_LINES, || 0);
            let mut hint_benchmark = HINT_BENCHMARK.get_or_init(|| Mutex::new(0)).lock().unwrap();
            if *hint_benchmark >= suggestion_lines {
                *hint_benchmark -= suggestion_lines / 2;
            } else if suggestion_lines >= *hint_benchmark {
                *hint_benchmark = 0;
                *SELECTION_INDEX
                    .get_or_init(|| Mutex::new(0))
                    .lock()
                    .unwrap() = 0;
            }
            Some(Cmd::Repaint)
        } else {
            None
        }
    }
}

struct ViListCtrlD;
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
            let suggestion_lines = cached_statics(&SUGGESTION_LINES, || 0);
            let mut hint_benchmark = HINT_BENCHMARK.get_or_init(|| Mutex::new(0)).lock().unwrap();
            let hint_span = HINT_SPAN.get_or_init(|| Mutex::new(0)).lock().unwrap();
            if *hint_span - suggestion_lines > *hint_benchmark {
                *hint_benchmark += suggestion_lines / 2;
            } else {
                *hint_benchmark = *hint_span - suggestion_lines;
                *SELECTION_INDEX
                    .get_or_init(|| Mutex::new(0))
                    .lock()
                    .unwrap() = *SELECTION_SPAN.get_or_init(|| Mutex::new(0)).lock().unwrap();
            }
            Some(Cmd::Repaint)
        } else {
            None
        }
    }
}

struct ListPageDown;
impl ConditionalEventHandler for ListPageDown {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        let suggestion_lines = cached_statics(&SUGGESTION_LINES, || 0);
        let mut hint_benchmark = HINT_BENCHMARK.get_or_init(|| Mutex::new(0)).lock().unwrap();
        let hint_span = HINT_SPAN.get_or_init(|| Mutex::new(0)).lock().unwrap();
        if *hint_span - suggestion_lines > *hint_benchmark {
            *hint_benchmark += suggestion_lines;
        } else {
            *hint_benchmark = *hint_span - suggestion_lines;
            *SELECTION_INDEX
                .get_or_init(|| Mutex::new(0))
                .lock()
                .unwrap() = *SELECTION_SPAN.get_or_init(|| Mutex::new(0)).lock().unwrap();
        }
        Some(Cmd::Repaint)
    }
}

struct ListPageUp;
impl ConditionalEventHandler for ListPageUp {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        let suggestion_lines = cached_statics(&SUGGESTION_LINES, || 0);
        let mut hint_benchmark = HINT_BENCHMARK.get_or_init(|| Mutex::new(0)).lock().unwrap();
        if *hint_benchmark >= suggestion_lines {
            *hint_benchmark -= suggestion_lines;
        } else if suggestion_lines >= *hint_benchmark {
            *hint_benchmark = 0;
            *SELECTION_INDEX
                .get_or_init(|| Mutex::new(0))
                .lock()
                .unwrap() = 0;
        }
        Some(Cmd::Repaint)
    }
}

//░█▀▀░█░█░█▀█░█▀▀░▀█▀░▀█▀░█▀█░█▀█░█▀▀
//░█▀▀░█░█░█░█░█░░░░█░░░█░░█░█░█░█░▀▀█
//░▀░░░▀▀▀░▀░▀░▀▀▀░░▀░░▀▀▀░▀▀▀░▀░▀░▀▀▀

// function to initialize a mutex as per the config file
fn init_statics<T: Clone>(cell: &OnceLock<Mutex<T>>, config_value: Option<T>, default_value: T) {
    let value = config_value.unwrap_or(default_value);
    let _ = cell.set(Mutex::new(value));
}
// function to retrieve a cached value with a default
fn cached_statics<T: Clone, F: FnOnce() -> T>(cell: &OnceLock<Mutex<T>>, default_fn: F) -> T {
    let m = cell.get_or_init(|| Mutex::new(default_fn()));
    m.lock().unwrap().clone()
}

// function to print help
fn print_help() {
    println!("\x1b[4motter-launcher:\x1b[0m");
    println!("A terminal script launcher featuring vi & emacs keybinds. Repo: https://github.com/kuokuo123/otter-launcher");
    println!();
    println!("\x1b[4mUsage:\x1b[0m");
    println!("otter-launcher [OPTIONS] [ARGUMENTS]...");
    println!();
    println!("\x1b[4mOptions:\x1b[0m");
    println!("  -h, --help     Show help");
    println!("  -v, --version  Show version");
    println!();
    println!("\x1b[4mBehavior:\x1b[0m");
    println!( "  1. Without OPTIONS nor ARGUMENTS, TUI interface will be shown for interacting with configured modules.");
    println!( "  2. If OPTIONS are given, only help or version messages will be shown.");
    println!( "  3. If ARGUMENTS are given without OPTIONS, ARGUMENTS are taken as a direct user prompt. All configured modules are effective without entering the TUI interface.");
}

// function to print version
fn print_version() {
    println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
}

// function to format vec<hints> according to configured modules, and to provide them to hinter
fn map_hints() -> Result<Vec<ModuleHint>, Box<dyn Error>> {
    let indicator_with_arg_module = &cached_statics(&INDICATOR_WITH_ARG_MODULE, || "".to_string());
    let indicator_no_arg_module = &cached_statics(&INDICATOR_NO_ARG_MODULE, || "".to_string());

    let set = config()
        .modules
        .iter()
        .map(|module| {
            let arg_indicator = if module.with_argument == Some(true) {
                indicator_with_arg_module
            } else {
                indicator_no_arg_module
            };

            let hint_string = format!(
                "{} {}{}",
                remove_ascii(&module.prefix),
                arg_indicator,
                &module.description
            );
            ModuleHint::new(&hint_string, &hint_string, module.with_argument)
        })
        .collect::<Vec<_>>();
    Ok(set)
}

// function to remove ascii color code from &str
fn remove_ascii(text: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;]*[A-Za-z]").unwrap();
    re.replace_all(text, "").to_string()
}

// function to expand env
fn expand_env_vars(input: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // look for $(
        if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '(' {
            // find matching )
            let mut depth = 1;
            let mut j = i + 2; // start after "$("
            while j < chars.len() && depth > 0 {
                match chars[j] {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }

            if depth == 0 {
                // command is between i+2 and j-1
                let command: String = chars[i + 2..j - 1].iter().collect();
                let output = run_subshell(&command);
                result.push_str(&output);
                i = j;
                continue;
            } else {
                // no matching closing ), treat "$(" literally
                result.push(chars[i]);
                i += 1;
                continue;
            }
        }

        // not a subshell, just copy
        result.push(chars[i]);
        i += 1;
    }

    // Now handle $VARS (but not numeric like $1)
    let var_re = regex::Regex::new(r"\$([A-Za-z_][A-Za-z0-9_]*)").unwrap();
    var_re
        .replace_all(&result, |caps: &regex::Captures| {
            env::var(&caps[1]).unwrap_or_default()
        })
        .into_owned()
}

fn run_subshell(cmd: &str) -> String {
    let exec_cmd = cached_statics(&EXEC_CMD, || "sh -c".to_string());
    let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
    let mut shell_cmd = Command::new(cmd_parts[0]);
    for arg in &cmd_parts[1..] {
        shell_cmd.arg(arg);
    }
    match shell_cmd.arg(cmd).output() {
        Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
        Err(_) => String::new(),
    }
}

// function to run module.cmd
fn run_module_command(mod_cmd_arg: String) {
    // format the shell command by which the module commands are launched
    let exec_cmd = cached_statics(&EXEC_CMD, || "sh -c".to_string());
    let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
    let mut shell_cmd = Command::new(cmd_parts[0]);
    for arg in &cmd_parts[1..] {
        shell_cmd.arg(arg);
    }
    // run module cmd
    shell_cmd.arg(mod_cmd_arg);
    if cached_statics(&LOOP_MODE, || false) {
        shell_cmd.stderr(Stdio::null());
    }
    shell_cmd
        .spawn()
        .expect("failed to launch run_module_command()")
        .wait()
        .expect("failed to wait for run_module_command()");
}

fn run_module_command_unbind_proc(mod_cmd_arg: String) {
    // format the shell command by which the module commands are launched
    let mut shell_cmd = Command::new("setsid");
    shell_cmd.arg("-f");

    let exec_cmd = cached_statics(&EXEC_CMD, || "sh -c".to_string());
    let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
    for arg in &cmd_parts[0..] {
        shell_cmd.arg(arg);
    }

    // run module cmd
    shell_cmd
        .arg(mod_cmd_arg)
        .spawn()
        .expect("failed to launch run_module_command_unbind_proc()")
        .wait()
        .expect("failed to wait for run_module_command_unbind_proc()");
}

// function to run empty & default modules
fn run_designated_module(prompt: String, prfx: String) {
    // test if the designated module is set
    if prfx.is_empty() {
        println!("{}", prompt)
    } else {
        // set a fallback module to prevent panic when no module is found
        let fallback = Module {
            description: "".to_string(),
            prefix: "".to_string(),
            cmd: "printf 'no default_module or empty_module found\n'".to_string(),
            with_argument: None,
            url_encode: None,
            unbind_proc: None,
        };

        // find the designated module
        let target_module = config()
            .modules
            .iter()
            .find(|module| remove_ascii(&module.prefix) == prfx)
            .unwrap_or(&fallback);
        // whether to use url encoding
        let prompt_wo_prefix = if target_module.url_encode.unwrap_or(false) {
            encode(&prompt).to_string()
        } else {
            prompt
        };

        // run the module's command
        if target_module.unbind_proc.unwrap_or(false) {
            run_module_command_unbind_proc(
                target_module
                    .cmd
                    .replace("{}", &prompt_wo_prefix)
                    .to_string(),
            );
        } else {
            run_module_command(
                target_module
                    .cmd
                    .replace("{}", &prompt_wo_prefix)
                    .to_string(),
            );
        }
    }
}

// function to measure kitty image height
fn kitty_rows(s: &str) -> Option<usize> {
    // match: ESC_G ... (terminated by ST `ESC\` or ST 0x9c, or BEL 0x07)
    // captures the body (params[,;]data...), enabling parsing params before the first ;
    let re = regex::Regex::new(r"\x1b_G(?P<body>.*?)(?:\x1b\\|\x9c|\x07)").ok()?;
    let mut id_to_rows: HashMap<String, usize> = HashMap::new();
    let mut anon_images = 0usize;

    for caps in re.captures_iter(s) {
        let body = match caps.name("body") {
            Some(m) => m.as_str(),
            None => continue,
        };

        // params are before the first ; (then optional base64 data after ';')
        let params_str = body.split_once(';').map(|(p, _)| p).unwrap_or(body);

        let mut img_id: Option<String> = None;
        let mut rows: Option<usize> = None;

        for part in params_str.split(',') {
            if let Some((k, v)) = part.split_once('=') {
                match k {
                    "i" => img_id = Some(v.to_string()),
                    "r" => rows = v.parse::<usize>().ok(),
                    _ => {}
                }
            }
        }

        if let Some(r) = rows {
            if let Some(id) = img_id {
                // for the same image id, keep the largest r seen
                id_to_rows
                    .entry(id)
                    .and_modify(|x| {
                        if r > *x {
                            *x = r
                        }
                    })
                    .or_insert(r);
            } else {
                // no explicit id — treat as its own image
                anon_images += r;
            }
        }
    }

    let sum_ids: usize = id_to_rows.values().copied().sum();
    let total = sum_ids + anon_images;
    if total > 0 { Some(total) } else { None }
}

// functions to measure sixel graphics height (raster row, not terminal row, string between ESC P and ST)
fn sixel_block_raster_rows(body: &str) -> Option<usize> {
    // sixel data starts after the first 'q'
    let idx = body.find('q')?;
    let data = &body[idx + 1..];
    if data.is_empty() {
        return Some(0);
    }
    // '-' advances to next sixel row; '$' is carriage return (same row)
    Some(1 + data.bytes().filter(|&b| b == b'-').count())
}

// sum of raster rows across all sixel images found in `s`.
// returns None if no sixel blocks are present.
fn sixel_rows(s: &str) -> Option<usize> {
    // match DCS ... ST: ESC P ... (terminated by ESC\ or 0x9C)
    let re = regex::Regex::new(r"(?s)\x1bP(?P<body>.*?)(?:\x1b\\|\x9c)").ok()?;
    let mut total: usize = 0;
    let mut found = false;

    for caps in re.captures_iter(s) {
        if let Some(body) = caps.name("body") {
            if let Some(rows) = sixel_block_raster_rows(body.as_str()) {
                total += rows;
                found = true;
            }
        }
    }

    if found { Some(total) } else { None }
}

// functions to get term cell height, just for converting sixel rows to terminal rows
fn term_cell_height_cached() -> std::io::Result<usize> {
    if let Some(h) = CELL_HEIGHT.get() {
        return Ok(*h);
    }
    let h = terminal_cell_height_px()?;
    CELL_HEIGHT.set(h).ok();
    Ok(h)
}

fn write_csi_and_read<'a>(
    out: &mut dyn Write,
    inp: &mut dyn Read,
    csi: &[u8],
    buf: &'a mut [u8],
) -> io::Result<&'a str> {
    out.write_all(csi)?;
    out.flush()?;
    let n = inp.read(buf)?;
    std::str::from_utf8(&buf[..n]).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn parse_csi_numbers(s: &str, expected_leading: &str) -> Option<Vec<usize>> {
    let rest = s.strip_prefix("\x1b[")?;
    let rest = rest.strip_suffix('t')?;
    let mut it = rest.split(';');
    if it.next()? != expected_leading {
        return None;
    }
    let mut out = Vec::new();
    for p in it {
        out.push(p.parse().ok()?);
    }
    Some(out)
}

fn terminal_cell_height_px() -> io::Result<usize> {
    // use /dev/tty-style stdin/stdout, pure std I/O
    let mut out = io::stdout();
    let mut inp = io::stdin();

    // 1) try CSI 16 t : "report cell size in pixels" -> ESC [ 16 t?
    // many terminals reply as: ESC [ 6 ; <height_px> ; <width_px> t
    {
        let mut buf = [0u8; 128];
        if let Ok(s) = write_csi_and_read(&mut out, &mut inp, b"\x1b[16t", &mut buf) {
            if let Some(nums) = parse_csi_numbers(s, "6") {
                if nums.len() >= 2 && nums[0] > 0 {
                    return Ok(nums[0]); // height in pixels per cell
                }
            }
        }
    }

    // 2) fallback: derive from window pixel height / rows
    //    CSI 14 t -> report window size in pixels: ESC [ 4 ; <height_px> ; <width_px> t
    //    CSI 18 t -> report text area size in characters: ESC [ 8 ; <rows> ; <cols> t
    let mut height_px: Option<usize> = None;
    let mut rows: Option<usize> = None;

    {
        let mut buf = [0u8; 128];
        if let Ok(s) = write_csi_and_read(&mut out, &mut inp, b"\x1b[14t", &mut buf) {
            if let Some(nums) = parse_csi_numbers(s, "4") {
                if nums.len() >= 2 && nums[0] > 0 {
                    height_px = Some(nums[0]);
                }
            }
        }
    }
    {
        let mut buf = [0u8; 128];
        if let Ok(s) = write_csi_and_read(&mut out, &mut inp, b"\x1b[18t", &mut buf) {
            if let Some(nums) = parse_csi_numbers(s, "8") {
                if nums.len() >= 2 && nums[0] > 0 {
                    rows = Some(nums[0]);
                }
            }
        }
    }

    if let (Some(hpx), Some(r)) = (height_px, rows) {
        if r > 0 {
            let per_cell = (hpx + r - 1) / r;
            return Ok(per_cell);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::Other,
        "could not determine cell height in pixels (ANSI queries failed)",
    ))
}

//░█▀▀░█░░░█▀█░█░█░░░█▀▀░█▀█░█▀█░▀█▀░█▀▄░█▀█░█░░
//░█▀▀░█░░░█░█░█▄█░░░█░░░█░█░█░█░░█░░█▀▄░█░█░█░░
//░▀░░░▀▀▀░▀▀▀░▀░▀░░░▀▀▀░▀▀▀░▀░▀░░▀░░▀░▀░▀▀▀░▀▀▀

// main function
fn main() {
    //initializing global variables
    init_statics(
        &EXEC_CMD,
        config().general.exec_cmd.clone(),
        "sh -c".to_string(),
    );
    init_statics(
        &EXTERNAL_EDITOR,
        config().general.external_editor.clone(),
        "".to_string(),
    );
    init_statics(
        &DEFAULT_MODULE,
        config().general.default_module.clone(),
        "".to_string(),
    );
    init_statics(
        &EMPTY_MODULE,
        config().general.empty_module.clone(),
        "".to_string(),
    );
    init_statics(
        &CHEATSHEET_ENTRY,
        config().general.cheatsheet_entry.clone(),
        "?".to_string(),
    );
    init_statics(
        &CHEATSHEET_VIEWER,
        config().general.cheatsheet_viewer.clone(),
        "less -R; clear".to_string(),
    );
    init_statics(&VI_MODE, config().general.vi_mode, false);
    init_statics(&ESC_TO_ABORT, config().general.esc_to_abort, true);
    init_statics(&LOOP_MODE, config().general.loop_mode, false);
    init_statics(
        &CLEAR_SCREEN_AFTER_EXECUTION,
        config().general.clear_screen_after_execution,
        false,
    );
    init_statics(&CALLBACK, config().general.callback.clone(), "".to_string());
    init_statics(&DELAY_STARTUP, config().general.delay_startup, 0);
    init_statics(
        &HEADER_CMD,
        config().interface.header_cmd.clone(),
        "".to_string(),
    );
    init_statics(
        &OVERLAY_CMD,
        config().overlay.overlay_cmd.clone(),
        "".to_string(),
    );
    init_statics(
        &HEADER_CMD_TRIMMED_LINES,
        config().interface.header_cmd_trimmed_lines,
        0,
    );
    init_statics(
        &OVERLAY_TRIMMED_LINES,
        config().overlay.overlay_trimmed_lines,
        0,
    );
    init_statics(&OVERLAY_HEIGHT, config().overlay.overlay_height, 0);
    init_statics(
        &HEADER,
        config().interface.header.clone(),
        "otter-launcher: ".to_string(),
    );
    init_statics(
        &SEPARATOR,
        config().interface.separator.clone(),
        "".to_string(),
    );
    init_statics(&FOOTER, config().interface.footer.clone(), "".to_string());
    init_statics(
        &LIST_PREFIX,
        config().interface.list_prefix.clone(),
        "".to_string(),
    );
    init_statics(
        &SELECTION_PREFIX,
        config().interface.selection_prefix.clone(),
        ">".to_string(),
    );
    init_statics(
        &PLACE_HOLDER,
        config().interface.place_holder.clone(),
        "type something".to_string(),
    );
    init_statics(
        &INDICATOR_WITH_ARG_MODULE,
        config().interface.indicator_with_arg_module.clone(),
        "".to_string(),
    );
    init_statics(
        &INDICATOR_NO_ARG_MODULE,
        config().interface.indicator_no_arg_module.clone(),
        "".to_string(),
    );
    init_statics(
        &SUGGESTION_MODE,
        config().interface.suggestion_mode.clone(),
        "list".to_string(),
    );
    init_statics(&SUGGESTION_LINES, config().interface.suggestion_lines, 1);
    init_statics(
        &DEFAULT_MODULE_MESSAGE,
        config().interface.default_module_message.clone(),
        "".to_string(),
    );
    init_statics(
        &EMPTY_MODULE_MESSAGE,
        config().interface.empty_module_message.clone(),
        "".to_string(),
    );
    init_statics(&PREFIX_PADDING, config().interface.prefix_padding, 0);
    init_statics(
        &PREFIX_COLOR,
        config().interface.prefix_color.clone(),
        "".to_string(),
    );
    init_statics(
        &DESCRIPTION_COLOR,
        config().interface.description_color.clone(),
        "\x1b[39m".to_string(),
    );
    init_statics(
        &PLACE_HOLDER_COLOR,
        config().interface.place_holder_color.clone(),
        "\x1b[30m".to_string(),
    );
    init_statics(
        &HINT_COLOR,
        config().interface.hint_color.clone(),
        "\x1b[30m".to_string(),
    );
    init_statics(
        &LAYOUT_RIGHTWARD,
        config().interface.move_interface_right,
        0,
    );
    init_statics(&LAYOUT_DOWNWARD, config().interface.move_interface_down, 0);
    init_statics(&OVERLAY_RIGHTWARD, config().overlay.move_overlay_right, 0);
    init_statics(&OVERLAY_DOWNWARD, config().overlay.move_overlay_down, 0);
    init_statics(
        &CUSTOMIZED_LIST_ORDER,
        config().interface.customized_list_order,
        false,
    );

    // rustyline editor setup
    *SELECTION_INDEX
        .get_or_init(|| Mutex::new(0))
        .lock()
        .unwrap() = 0;
    let mut rl: Editor<OtterHelper, DefaultHistory> = Editor::new().unwrap();
    // set OtterHelper as hint and completion provider
    rl.set_helper(Some(OtterHelper {
        hints: map_hints().expect("failed to provide hints"),
    }));

    // check if esc_to_abort is on
    if cached_statics(&ESC_TO_ABORT, || true) {
        rl.bind_sequence(
            KeyEvent::new('\x1b', Modifiers::NONE),
            EventHandler::Simple(Cmd::Interrupt),
        );
        rl.set_keyseq_timeout(Some(0));
    }

    // check if vi_mode is on, and set up keybinds accordingly
    if cached_statics(&VI_MODE, || false) {
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
        if !cached_statics(&EXTERNAL_EDITOR, || "".to_string()).is_empty() {
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
        if !cached_statics(&EXTERNAL_EDITOR, || "".to_string()).is_empty() {
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

    // start the flow
    loop {
        // delay startup if configured
        let delay_startup = cached_statics(&DELAY_STARTUP, || 0);
        if delay_startup > 0 {
            std::thread::sleep(std::time::Duration::from_millis(
                delay_startup.try_into().unwrap(),
            ));
        }

        // moving layout around
        let layout_right = cached_statics(&LAYOUT_RIGHTWARD, || 0);
        let layout_down = cached_statics(&LAYOUT_DOWNWARD, || 0);

        // print from header commands
        let remove_lines_count = cached_statics(&HEADER_CMD_TRIMMED_LINES, || 0);
        let header_cmd = cached_statics(&HEADER_CMD, String::new);
        if !header_cmd.is_empty() {
            let exec_cmd = cached_statics(&EXEC_CMD, || "sh -c".to_string());
            let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
            let mut shell_cmd = Command::new(cmd_parts[0]);
            for arg in &cmd_parts[1..] {
                shell_cmd.arg(arg);
            }
            let status = shell_cmd
                .arg(&header_cmd)
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .expect("Failed to launch header command...");

            if !status.success() {
                eprintln!("header_cmd failed to run with status: {}", status);
            }
            println!("\x1b[{}A", remove_lines_count + 1)
        }

        // print header
        let config_header = cached_statics(&HEADER, || "sh -c".to_string());
        let expanded_header = expand_env_vars(&config_header);
        let header_lines: Vec<&str> = expanded_header.split('\n').collect();

        // variables to form the header
        let layout_down_string = if layout_down > 0 {
            format!("{}", "\n".repeat(layout_down))
        } else {
            "".to_string()
        };
        let layout_right_padding = format!("\x1b[{}G", layout_right + 1);
        let repeated_spaces = " ".repeat(layout_right);
        let padded_lines: Vec<String> = header_lines
            .iter()
            .map(|line| {
                format!(
                    "{}{}{}{}",
                    layout_right_padding, repeated_spaces, layout_right_padding, line
                )
            })
            .collect();
        let aligned_header = padded_lines.join("\n");

        let header = format!("{}{}", layout_down_string, aligned_header,);

        // calculate header lines and make it globaly accesible
        *HEADER_LINE_COUNT
            .get_or_init(|| Mutex::new(0))
            .lock()
            .unwrap() = header.lines().count();

        // if launched with arguments, do not enter rustyline editor
        let args: Vec<String> = env::args().skip(1).collect();
        let prompt = if let Some(arg) = args.first() {
            match arg.as_str() {
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                "-v" | "--version" => {
                    print_version();
                    std::process::exit(0);
                }
                _ => Ok(args.join(" ")),
            }
        } else {
            // if launched without arguments, run rustyline with configured header
            rl.readline(&header)
        };

        match prompt {
            Ok(_) => {}
            Err(_) => {
                process::exit(0);
            }
        }
        let prompt = prompt.expect("failed to read prompt");

        // flow switches setup
        let mut loop_switch = cached_statics(&LOOP_MODE, || false);

        // clear screen if clear_screen_after_execution is on
        if cached_statics(&CLEAR_SCREEN_AFTER_EXECUTION, || false) {
            print!("\x1B[2J\x1B[1;1H");
            std::io::stdout().flush().expect("failed to flush stdout")
        };

        // matching the prompted prefix with module prefixes to decide what to do
        let prompted_prfx = prompt.split_whitespace().next().unwrap_or("");
        let module_prfx = config()
            .modules
            .iter()
            .find(|module| remove_ascii(&module.prefix) == prompted_prfx);

        match module_prfx {
            // if user input starts with some module prefixes
            Some(module) => {
                // determine whether the prompt should be urlencoded
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
                        run_module_command_unbind_proc(module.cmd.replace("{}", &argument));
                    } else {
                        run_module_command(module.cmd.replace("{}", &argument));
                    }
                // Condition 2: when user input is exactly the same as the no-arg module
                } else if remove_ascii(&module.prefix) == prompt.trim_end() {
                    if module.unbind_proc.unwrap_or(false) {
                        run_module_command_unbind_proc(module.cmd.to_owned());
                    } else {
                        run_module_command(module.cmd.to_owned());
                    }
                // Condition 3: when no-arg modules is running with arguement
                } else {
                    run_designated_module(
                        prompt,
                        cached_statics(&DEFAULT_MODULE, || "".to_string()),
                    )
                }
            }
            // if user input doesn't start with some module prefixes
            _ => {
                // Condition 1: when user input is empty, run the empty module
                if prompt.is_empty() {
                    run_designated_module(prompt, cached_statics(&EMPTY_MODULE, || "".to_string()))
                // Condition 2: when helper keyword is passed, open cheatsheet in less
                } else if prompt.trim_end() == cached_statics(&CHEATSHEET_ENTRY, || "?".to_string())
                {
                    // setup variables
                    let prefix_color = cached_statics(&PREFIX_COLOR, || "".to_string());
                    let description_color = cached_statics(&DESCRIPTION_COLOR, || "".to_string());
                    let indicator_with_arg_module =
                        &cached_statics(&INDICATOR_WITH_ARG_MODULE, || "".to_string());
                    let indicator_no_arg_module =
                        &cached_statics(&INDICATOR_NO_ARG_MODULE, || "".to_string());
                    // run general.cheatsheet.viewer
                    let exec_cmd = cached_statics(&EXEC_CMD, || "sh -c".to_string());
                    let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
                    let mut shell_cmd = Command::new(cmd_parts[0]);
                    for arg in &cmd_parts[1..] {
                        shell_cmd.arg(arg);
                    }
                    let mut child = shell_cmd
                        .arg(cached_statics(&CHEATSHEET_VIEWER, || {
                            "less -R; clear".to_string()
                        }))
                        .stdin(Stdio::piped())
                        .spawn();
                    if let Ok(ref mut child) = child {
                        if let Some(stdin) = child.stdin.as_mut() {
                            // Format cheat sheet
                            let mapped_modules = config()
                                .modules
                                .iter()
                                .map(|module| {
                                    let arg_indicator = if module.with_argument == Some(true) {
                                        indicator_with_arg_module
                                    } else {
                                        indicator_no_arg_module
                                    };
                                    let width = config()
                                        .modules
                                        .iter()
                                        .map(|line| remove_ascii(&line.prefix).len())
                                        .max()
                                        .unwrap_or(0);
                                    format!(
                                        "    {}{:width$}{} {}{}{}{}",
                                        prefix_color,
                                        &module.prefix,
                                        "\x1b[0m",
                                        description_color,
                                        arg_indicator,
                                        &module.description,
                                        "\x1b[0m"
                                    )
                                })
                                .collect::<Vec<String>>()
                                .join("\n");
                            match stdin.write_all(
                                format!(
                                    "\n  {}{}{}",
                                    prefix_color, "Configured Modules:\n\n\x1b[0m", mapped_modules
                                )
                                .as_bytes(),
                            ) {
                                Ok(_) => {}
                                Err(e) => {
                                    eprintln!("Error writing to stdin of child process: {}", e);
                                }
                            }
                        }
                    }
                    child
                        .expect("failed to pipe cheatsheet into viewer")
                        .wait()
                        .expect("failed to wait for the execution of cheatsheet_viewer");
                    loop_switch = true;
                // Condition 3: when no module is matched, run the default module
                } else {
                    run_designated_module(
                        prompt,
                        cached_statics(&DEFAULT_MODULE, || "".to_string()),
                    )
                }
            }
        }

        // run general.callback
        let callback = cached_statics(&CALLBACK, || "".to_string());
        if !callback.is_empty() {
            run_module_command_unbind_proc(callback)
        }

        // if not in loop_mode, quit the process
        if !loop_switch {
            break;
        }
    }
}
