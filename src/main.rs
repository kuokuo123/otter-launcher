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

extern crate regex;
extern crate rustyline;
extern crate rustyline_derive;
extern crate serde;
extern crate toml;
extern crate urlencoding;

use once_cell::sync::Lazy;
use rustyline::{
    completion::{Completer, Pair},
    config::Configurer,
    highlight::Highlighter,
    hint::{Hint, Hinter},
    history::DefaultHistory,
    Cmd, ConditionalEventHandler, Context, EditMode, Editor, Event, EventContext, EventHandler,
    KeyCode, KeyEvent, Modifiers, Movement, RepeatCount,
};
use rustyline_derive::{Helper, Validator};
use serde::Deserialize;
use std::{
    borrow::Cow,
    env,
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process,
    process::{Command, Stdio},
    str::from_utf8,
    sync::Mutex,
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
}

#[derive(Deserialize, Default)]
struct Interface {
    header: Option<String>,
    header_cmd: Option<String>,
    header_cmd_trimmed_lines: Option<usize>,
    header_concatenate: Option<bool>,
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
}

#[derive(Deserialize, Clone)]
struct Module {
    description: String,
    prefix: String,
    cmd: String,
    with_argument: Option<bool>,
    url_encode: Option<bool>,
}

// load toml config
static CONFIG: Lazy<Config> = Lazy::new(|| {
    let home_dir = env::var("HOME").unwrap_or_else(|_| String::from("/"));
    let xdg_config_path = format!("{}/.config/otter-launcher/config.toml", home_dir);
    // fallback from xdg_config_path to /etc
    let config_file: &str = if Path::new(&xdg_config_path).exists() {
        &xdg_config_path
    } else {
        "/etc/otter-launcher/config.toml"
    };
    let contents = std::fs::read_to_string(config_file).unwrap_or_else(|_| String::new());

    toml::from_str(&contents).expect("cannot read contents from config_file")
});

// use lazy mutex to make important variables globally accessible (repeatedly used config values, list selection, and completion related stuff)
// define config variables as statics
static HEADER_CMD: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static SUGGESTION_MODE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static LOOP_MODE: Lazy<Mutex<Option<bool>>> = Lazy::new(|| Mutex::new(None));
static CALLBACK: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static CHEATSHEET_ENTRY: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static CHEATSHEET_VIEWER: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static EXTERNAL_EDITOR: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static VI_MODE: Lazy<Mutex<Option<bool>>> = Lazy::new(|| Mutex::new(None));
static ESC_TO_ABORT: Lazy<Mutex<Option<bool>>> = Lazy::new(|| Mutex::new(None));
static CLEAR_SCREEN_AFTER_EXECUTION: Lazy<Mutex<Option<bool>>> = Lazy::new(|| Mutex::new(None));
static HEADER_CMD_TRIMMED_LINES: Lazy<Mutex<Option<usize>>> = Lazy::new(|| Mutex::new(None));
static HEADER: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static HEADER_CONCATENATE: Lazy<Mutex<Option<bool>>> = Lazy::new(|| Mutex::new(None));
static EXEC_CMD: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static DEFAULT_MODULE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static EMPTY_MODULE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static EMPTY_MODULE_MESSAGE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static DEFAULT_MODULE_MESSAGE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static SUGGESTION_LINES: Lazy<Mutex<Option<usize>>> = Lazy::new(|| Mutex::new(None));
static PREFIX_PADDING: Lazy<Mutex<Option<usize>>> = Lazy::new(|| Mutex::new(None));
static SELECTION_INDEX: Lazy<Mutex<usize>> = Lazy::new(|| Mutex::new(0));
static SELECTION_SPAN: Lazy<Mutex<usize>> = Lazy::new(|| Mutex::new(0));
static HINT_SPAN: Lazy<Mutex<usize>> = Lazy::new(|| Mutex::new(0));
static HINT_BENCHMARK: Lazy<Mutex<usize>> = Lazy::new(|| Mutex::new(0));
static LIST_PREFIX: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static SELECTION_PREFIX: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static PREFIX_COLOR: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static DESCRIPTION_COLOR: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static PLACE_HOLDER: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static PLACE_HOLDER_COLOR: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static HINT_COLOR: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static INDICATOR_WITH_ARG_MODULE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static INDICATOR_NO_ARG_MODULE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));
static FILTERED_HINT_COUNT: Lazy<Mutex<usize>> = Lazy::new(|| Mutex::new(0));
static COMPLETION_CANDIDATE: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

//░█░█░▀█▀░█▀█░▀█▀░░░▄▀░░░░█▀▀░█▀█░█▄█░█▀█░█░░░█▀▀░▀█▀░▀█▀░█▀█░█▀█
//░█▀█░░█░░█░█░░█░░░░▄█▀░░░█░░░█░█░█░█░█▀▀░█░░░█▀▀░░█░░░█░░█░█░█░█
//░▀░▀░▀▀▀░▀░▀░░▀░░░░░▀▀░░░▀▀▀░▀▀▀░▀░▀░▀░░░▀▀▀░▀▀▀░░▀░░▀▀▀░▀▀▀░▀░▀

// define the structure of every formatted hint
#[derive(Hash, Debug, PartialEq, Eq)]
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
    fn suffix(&self, strip_chars: usize) -> Self {
        Self {
            display: self.display.to_owned(),
            completion: strip_chars,
            w_arg: self.w_arg,
        }
    }
}

// define how the chosen hint is presented and completed in the rustyline editor
impl Hint for ModuleHint {
    // text to display when hint is active
    fn display(&self) -> &str {
        if cached_statics(&SUGGESTION_MODE, "list".to_string()) == "hint" {
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
        let com_candidate = cached_statics(&COMPLETION_CANDIDATE, "".to_string());
        if cached_statics(&SUGGESTION_MODE, "".to_string()) == "hint".to_string() {
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
            if line.is_empty() && *SELECTION_INDEX.lock().unwrap() == 0 {
                // when empty, complete with empty module
                let cand = vec![Pair {
                    display: "".to_string(),
                    replacement: cached_statics(&EMPTY_MODULE, "".to_string()) + " ",
                }];
                *SELECTION_INDEX.lock().unwrap() = 0;
                Ok((0, cand))
            } else if com_candidate == " " {
                // when no module is matched, complete with default module
                let cand = vec![Pair {
                    display: "".to_string(),
                    replacement: cached_statics(&DEFAULT_MODULE, "".to_string()) + " ",
                }];
                *SELECTION_INDEX.lock().unwrap() = 0;
                Ok((0, cand))
            } else {
                // normal behavior
                let cand = vec![Pair {
                    display: "".to_string(),
                    replacement: com_candidate,
                }];
                *SELECTION_INDEX.lock().unwrap() = 0;
                Ok((0, cand))
            }
        }
    }
}

// the coloring functionality of OtterHelper
impl Highlighter for OtterHelper {
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        let default_module_message = cached_statics(&DEFAULT_MODULE_MESSAGE, "".to_string());
        let empty_module_message = cached_statics(&EMPTY_MODULE_MESSAGE, "".to_string());
        let description_color = cached_statics(&DESCRIPTION_COLOR, "".to_string());
        let place_holder = cached_statics(&PLACE_HOLDER, "type and search...".to_string());
        let place_holder_color = cached_statics(&PLACE_HOLDER_COLOR, "\x1b[30m".to_string());
        let hint_color = cached_statics(&HINT_COLOR, "\x1b[30m".to_string());
        let suggestion_mode = cached_statics(&SUGGESTION_MODE, "list".to_string());
        let list_prefix = cached_statics(&LIST_PREFIX, "".to_string());
        let selection_prefix = cached_statics(&SELECTION_PREFIX, ">".to_string());
        let prefix_color = cached_statics(&PREFIX_COLOR, "".to_string());
        let prefix_width = cached_statics(&PREFIX_PADDING, 0);
        let suggestion_lines = cached_statics(&SUGGESTION_LINES, 0);
        let mut selection_index = SELECTION_INDEX.lock().unwrap();
        let mut selection_span = SELECTION_SPAN.lock().unwrap();
        let mut hint_benchmark = HINT_BENCHMARK.lock().unwrap();
        let filtered_hint_count = FILTERED_HINT_COUNT.lock().unwrap();

        if suggestion_mode == "hint" {
            format!("{}{}{}{}", "\x1b[0m", hint_color, hint, "\x1b[0m").into()
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
            // if suggestion_mode is list
            hint.lines()
                .enumerate()
                .map(|(index, line)| {
                    if index == *selection_index && *selection_index > 0 {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            format!(
                                "{}{}{:prefix_width$}{} {}{}{}",
                                selection_prefix,
                                prefix_color,
                                parts[0],
                                "\x1b[0m",
                                description_color,
                                parts[1..].join(" "),
                                "\x1b[0m"
                            )
                        } else {
                            line.to_string()
                        }
                    } else if line == place_holder {
                        format!("{}{}{}", place_holder_color, place_holder, "\x1b[0m")
                    } else if (default_module_message.contains(line)
                        || empty_module_message.contains(line))
                        && !line.is_empty()
                    {
                        line.to_string()
                    } else {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            format!(
                                "{}{}{:prefix_width$}{} {}{}{}",
                                list_prefix,
                                prefix_color,
                                parts[0],
                                "\x1b[0m",
                                description_color,
                                parts[1..].join(" "),
                                "\x1b[0m"
                            )
                        } else {
                            line.to_string()
                        }
                    }
                })
                .collect::<Vec<String>>()
                .join("\n")
                .into()
        }
    }
}

// the hint providing functionality of OtterHelper
// select a hint for OtterHelper to pass into rustyline prompt editor (from a vector of all formatted hints)
impl Hinter for OtterHelper {
    type Hint = ModuleHint;
    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<ModuleHint> {
        let suggestion_mode = cached_statics(&SUGGESTION_MODE, "list".to_string());
        let place_holder = cached_statics(&PLACE_HOLDER, "type and search...".to_string());
        let cheatsheet_entry = cached_statics(&CHEATSHEET_ENTRY, "?".to_string());
        let indicator_no_arg_module = cached_statics(&INDICATOR_NO_ARG_MODULE, "".to_string());
        let suggestion_lines = cached_statics(&SUGGESTION_LINES, 1);
        let hint_benchmark = *HINT_BENCHMARK.lock().unwrap();

        *HINT_SPAN.lock().unwrap() = self.hints.len();

        // hint mode behavior
        if suggestion_mode == "hint" {
            if line.is_empty() {
                // when nothing is typed
                *COMPLETION_CANDIDATE.lock().unwrap() = None;
                Some(ModuleHint {
                    display: place_holder,
                    completion: 0,
                    w_arg: None,
                })
            } else if line.trim_end() == cheatsheet_entry {
                // when cheatsheet_entry is typed
                *COMPLETION_CANDIDATE.lock().unwrap() = Some("?".to_string());
                Some(ModuleHint {
                    display: format!(
                        "{} {}{}",
                        cheatsheet_entry, indicator_no_arg_module, "cheat sheet"
                    )
                    .to_string(),
                    completion: pos,
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
                                *COMPLETION_CANDIDATE.lock().unwrap() = Some(
                                    i.display
                                        .split_whitespace()
                                        .next()
                                        .unwrap_or("")
                                        .to_string(),
                                );
                                // provide the found hint
                                Some(i.suffix(pos))
                            } else {
                                *COMPLETION_CANDIDATE.lock().unwrap() = None;
                                None
                            }
                        })
                        .next()
                        .unwrap_or(ModuleHint {
                            display: "\x1b[0m".to_string(),
                            completion: 0,
                            w_arg: None,
                        }),
                )
            }
        } else {
            // list mode behavior
            let e_module = cached_statics(&EMPTY_MODULE_MESSAGE, "".to_string());
            let d_module = cached_statics(&DEFAULT_MODULE_MESSAGE, "".to_string());
            let selection_index = SELECTION_INDEX.lock().unwrap();

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

            // sort list items alphebetically
            aggregated_lines.sort_unstable();
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
            *FILTERED_HINT_COUNT.lock().unwrap() = filtered_items.len();

            // Check if there are enough filtered items after the skip
            let agg_line =
                if hint_benchmark + suggestion_lines > *FILTERED_HINT_COUNT.lock().unwrap() {
                    // If not enough, default to taking from the start
                    let join_range = &filtered_items
                        [..usize::min(suggestion_lines, *FILTERED_HINT_COUNT.lock().unwrap())];
                    join_range.join("\n")
                } else {
                    // If there are enough to take
                    let join_range = filtered_items
                        .iter()
                        .skip(hint_benchmark)
                        .take(suggestion_lines)
                        .copied()
                        .collect::<Vec<_>>();
                    join_range.join("\n")
                };

            // set completion candidate according to list selection index
            *COMPLETION_CANDIDATE.lock().unwrap() = Some(if *selection_index == 0 {
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
            });

            // format the aggregated hint lines as the single hint object to be presented
            if line.is_empty() {
                // if nothing has been typed
                Some(ModuleHint {
                    display: format!(
                        "{}{}",
                        // show place holder first
                        place_holder,
                        // if empty module is not set
                        if e_module.is_empty() {
                            if agg_line.is_empty() {
                                "".to_string()
                            } else {
                                format!("\n{}", agg_line)
                            }
                        } else {
                            // if empty module is set
                            format!("\n{}", e_module)
                        },
                    ),
                    completion: pos,
                    w_arg: None,
                })
            } else {
                // if something is typed
                Some(ModuleHint {
                    display: (if line.trim_end() == cheatsheet_entry {
                        *COMPLETION_CANDIDATE.lock().unwrap() = Some("? ".to_string());
                        format!(
                            "\n{} {} {}",
                            cheatsheet_entry, indicator_no_arg_module, "cheat sheet"
                        )
                    // if no module is matched
                    } else if agg_line.is_empty() {
                        // check if default module message is set
                        if d_module.is_empty() {
                            "\x1b[0m".to_string()
                        } else {
                            format!("\n{}", d_module)
                        }
                    // if some module is matched
                    } else {
                        format!("\n{}", agg_line)
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
        {
            let editor = cached_statics(&EXTERNAL_EDITOR, "".to_string());
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

            let exec_cmd = cached_statics(&EXEC_CMD, "sh -c".to_string());
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

struct ListItemUp;
impl ConditionalEventHandler for ListItemUp {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        let mut selection_index = SELECTION_INDEX.lock().unwrap();
        let mut hint_benchmark = HINT_BENCHMARK.lock().unwrap();
        let selection_span = SELECTION_SPAN.lock().unwrap();
        let suggestion_lines = cached_statics(&SUGGESTION_LINES, 0);
        let filtered_hint_count = FILTERED_HINT_COUNT.lock().unwrap();

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
        let selection_span = SELECTION_SPAN.lock().unwrap();
        let suggestion_lines = cached_statics(&SUGGESTION_LINES, 0);
        let hint_span = HINT_SPAN.lock().unwrap();
        let mut selection_index = SELECTION_INDEX.lock().unwrap();
        let mut hint_benchmark = HINT_BENCHMARK.lock().unwrap();
        let filtered_hint_count = FILTERED_HINT_COUNT.lock().unwrap();

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
            let selection_span = SELECTION_SPAN.lock().unwrap();
            let suggestion_lines = cached_statics(&SUGGESTION_LINES, 0);
            let hint_span = HINT_SPAN.lock().unwrap();
            let mut selection_index = SELECTION_INDEX.lock().unwrap();
            let mut hint_benchmark = HINT_BENCHMARK.lock().unwrap();
            let filtered_hint_count = FILTERED_HINT_COUNT.lock().unwrap();

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
            let mut selection_index = SELECTION_INDEX.lock().unwrap();
            let mut hint_benchmark = HINT_BENCHMARK.lock().unwrap();
            let selection_span = SELECTION_SPAN.lock().unwrap();
            let suggestion_lines = cached_statics(&SUGGESTION_LINES, 0);
            let filtered_hint_count = FILTERED_HINT_COUNT.lock().unwrap();

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
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        if *SELECTION_INDEX.lock().unwrap() == 0 {
            Some(Cmd::AcceptLine)
        } else {
            let com_candidate = cached_statics(&COMPLETION_CANDIDATE, "".to_string())
                .split_whitespace()
                .next()?
                .to_string();
            let target_module = CONFIG
                .modules
                .iter()
                .find(|module| remove_ascii(&module.prefix) == com_candidate)
                .unwrap();
            Some(if target_module.with_argument.unwrap_or(false) == false {
                run_designated_module("".to_string(), com_candidate);
                if cached_statics(&LOOP_MODE, false) == true {
                    *SELECTION_INDEX.lock().unwrap() = 0;
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

struct ListItemSelect;
impl ConditionalEventHandler for ListItemSelect {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        _ctx: &EventContext,
    ) -> Option<Cmd> {
        if *SELECTION_INDEX.lock().unwrap() == 0 {
            Some(Cmd::Complete)
        } else {
            let com_candidate = cached_statics(&COMPLETION_CANDIDATE, "".to_string())
                .split_whitespace()
                .next()?
                .to_string();
            let target_module = CONFIG
                .modules
                .iter()
                .find(|module| remove_ascii(&module.prefix) == com_candidate)
                .unwrap();
            Some(if target_module.with_argument.unwrap_or(false) == false {
                run_designated_module("".to_string(), com_candidate);
                if cached_statics(&LOOP_MODE, false) == true {
                    *SELECTION_INDEX.lock().unwrap() = 0;
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
        *SELECTION_INDEX.lock().unwrap() = 0;
        *HINT_BENCHMARK.lock().unwrap() = 0;
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
        let suggestion_lines = cached_statics(&SUGGESTION_LINES, 0);
        let mut hint_benchmark = HINT_BENCHMARK.lock().unwrap();
        let hint_span = HINT_SPAN.lock().unwrap();
        *hint_benchmark = *hint_span - suggestion_lines;
        *SELECTION_INDEX.lock().unwrap() = *SELECTION_SPAN.lock().unwrap();
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
            *SELECTION_INDEX.lock().unwrap() = 0;
            *HINT_BENCHMARK.lock().unwrap() = 0;
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
            let mut hint_benchmark = HINT_BENCHMARK.lock().unwrap();
            *hint_benchmark = *HINT_SPAN.lock().unwrap() - cached_statics(&SUGGESTION_LINES, 0);
            *SELECTION_INDEX.lock().unwrap() = *SELECTION_SPAN.lock().unwrap();
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
            let suggestion_lines = cached_statics(&SUGGESTION_LINES, 0);
            let mut hint_benchmark = HINT_BENCHMARK.lock().unwrap();
            if *hint_benchmark >= suggestion_lines {
                *hint_benchmark -= suggestion_lines / 2;
            } else if suggestion_lines >= *hint_benchmark {
                *hint_benchmark = 0;
                *SELECTION_INDEX.lock().unwrap() = 0;
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
            let suggestion_lines = cached_statics(&SUGGESTION_LINES, 0);
            let mut hint_benchmark = HINT_BENCHMARK.lock().unwrap();
            let hint_span = HINT_SPAN.lock().unwrap();
            if *hint_span - suggestion_lines > *hint_benchmark {
                *hint_benchmark += suggestion_lines / 2;
            } else {
                *hint_benchmark = *hint_span - suggestion_lines;
                *SELECTION_INDEX.lock().unwrap() = *SELECTION_SPAN.lock().unwrap();
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
        let suggestion_lines = cached_statics(&SUGGESTION_LINES, 0);
        let mut hint_benchmark = HINT_BENCHMARK.lock().unwrap();
        let hint_span = HINT_SPAN.lock().unwrap();
        if *hint_span - suggestion_lines > *hint_benchmark {
            *hint_benchmark += suggestion_lines;
        } else {
            *hint_benchmark = *hint_span - suggestion_lines;
            *SELECTION_INDEX.lock().unwrap() = *SELECTION_SPAN.lock().unwrap();
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
        let suggestion_lines = cached_statics(&SUGGESTION_LINES, 0);
        let mut hint_benchmark = HINT_BENCHMARK.lock().unwrap();
        if *hint_benchmark >= suggestion_lines {
            *hint_benchmark -= suggestion_lines;
        } else if suggestion_lines >= *hint_benchmark {
            *hint_benchmark = 0;
            *SELECTION_INDEX.lock().unwrap() = 0;
        }
        Some(Cmd::Repaint)
    }
}

//░█▀▀░█░█░█▀█░█▀▀░▀█▀░▀█▀░█▀█░█▀█░█▀▀
//░█▀▀░█░█░█░█░█░░░░█░░░█░░█░█░█░█░▀▀█
//░▀░░░▀▀▀░▀░▀░▀▀▀░░▀░░▀▀▀░▀▀▀░▀░▀░▀▀▀

// function to initialize a lazy mutex as per the config file
fn init_statics<T: Clone>(
    lazy_value: &Lazy<Mutex<Option<T>>>,
    config_value: Option<T>,
    default_value: T,
) {
    let value = config_value.unwrap_or(default_value);
    let mut lock = lazy_value.lock().unwrap();
    *lock = Some(value);
}
// function to retrieve a cached value with a default
fn cached_statics<T: Clone>(lazy_value: &Lazy<Mutex<Option<T>>>, default_value: T) -> T {
    let lock = lazy_value.lock().unwrap();
    lock.clone().unwrap_or(default_value)
}

// function to format vec<hints> according to configured modules, and to provide them to hinter
fn map_hints() -> Result<Vec<ModuleHint>, Box<dyn Error>> {
    let indicator_with_arg_module = &cached_statics(&INDICATOR_WITH_ARG_MODULE, "".to_string());
    let indicator_no_arg_module = &cached_statics(&INDICATOR_NO_ARG_MODULE, "".to_string());

    let set = CONFIG
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

// function to run module.cmd
fn run_module_command(mod_cmd_arg: &str) {
    // format the shell command by which the module commands are launched
    let exec_cmd = cached_statics(&EXEC_CMD, "sh -c".to_string());
    let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
    let mut shell_cmd = Command::new(cmd_parts[0]);
    for arg in &cmd_parts[1..] {
        shell_cmd.arg(arg);
    }
    // run module cmd
    shell_cmd
        .arg(mod_cmd_arg)
        .spawn()
        .expect("failed to launch callback...")
        .wait()
        .expect("failed to wail for callback execution...");
}

// function to run empty & default modules
fn run_designated_module(prompt: String, prfx: String) {
    // test if the designated module is set
    if prfx.is_empty() {
        println!("{}", prompt)
    } else {
        // if set
        // find the designated module
        let target_module = CONFIG
            .modules
            .iter()
            .find(|module| remove_ascii(&module.prefix) == prfx)
            .unwrap();
        // whether to use url encoding
        let prompt_wo_prefix = if target_module.url_encode.unwrap_or(false) {
            encode(&prompt).to_string()
        } else {
            prompt
        };
        // run the module's command
        run_module_command(
            &target_module
                .cmd
                .replace("{}", &prompt_wo_prefix)
                .to_string(),
        );
    }
}

// function to run general.callback
fn general_callback() {
    // check if general.callback if set
    let callback = cached_statics(&CALLBACK, "".to_string());
    if !callback.is_empty() {
        // format exec_cmd
        let exec_cmd = cached_statics(&EXEC_CMD, "sh -c".to_string());
        let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
        let mut cb_cmd = Command::new(cmd_parts[0]);
        for arg in &cmd_parts[1..] {
            cb_cmd.arg(arg);
        }
        // run callback
        cb_cmd
            .arg(callback)
            .spawn()
            .expect("failed to launch general.callback")
            .wait()
            .expect("failed to wait the execution of general.callback");
    }
}

//░█▀▀░█░░░█▀█░█░█░░░█▀▀░█▀█░█▀█░▀█▀░█▀▄░█▀█░█░░
//░█▀▀░█░░░█░█░█▄█░░░█░░░█░█░█░█░░█░░█▀▄░█░█░█░░
//░▀░░░▀▀▀░▀▀▀░▀░▀░░░▀▀▀░▀▀▀░▀░▀░░▀░░▀░▀░▀▀▀░▀▀▀

// main function
fn main() {
    //initializing global variables
    init_statics(
        &EXEC_CMD,
        CONFIG.general.exec_cmd.clone(),
        "sh -c".to_string(),
    );
    init_statics(
        &EXTERNAL_EDITOR,
        CONFIG.general.external_editor.clone(),
        "".to_string(),
    );
    init_statics(
        &DEFAULT_MODULE,
        CONFIG.general.default_module.clone(),
        "".to_string(),
    );
    init_statics(
        &EMPTY_MODULE,
        CONFIG.general.empty_module.clone(),
        "".to_string(),
    );
    init_statics(
        &CHEATSHEET_ENTRY,
        CONFIG.general.cheatsheet_entry.clone(),
        "?".to_string(),
    );
    init_statics(
        &CHEATSHEET_VIEWER,
        CONFIG.general.cheatsheet_viewer.clone(),
        "less -R; clear".to_string(),
    );
    init_statics(&VI_MODE, CONFIG.general.vi_mode, false);
    init_statics(&ESC_TO_ABORT, CONFIG.general.esc_to_abort, true);
    init_statics(&LOOP_MODE, CONFIG.general.loop_mode, false);
    init_statics(
        &CLEAR_SCREEN_AFTER_EXECUTION,
        CONFIG.general.clear_screen_after_execution,
        false,
    );
    init_statics(&CALLBACK, CONFIG.general.callback.clone(), "".to_string());
    init_statics(
        &HEADER_CMD,
        CONFIG.interface.header_cmd.clone(),
        "".to_string(),
    );
    init_statics(
        &HEADER_CMD_TRIMMED_LINES,
        CONFIG.interface.header_cmd_trimmed_lines,
        0,
    );
    init_statics(
        &HEADER,
        CONFIG.interface.header.clone(),
        "\x1b[34;1m \x1b[0m otter-launcher \x1b[34;1m>\x1b[0;1m ".to_string(),
    );
    init_statics(
        &HEADER_CONCATENATE,
        CONFIG.interface.header_concatenate,
        false,
    );
    init_statics(
        &LIST_PREFIX,
        CONFIG.interface.list_prefix.clone(),
        "".to_string(),
    );
    init_statics(
        &SELECTION_PREFIX,
        CONFIG.interface.selection_prefix.clone(),
        ">".to_string(),
    );
    init_statics(
        &PLACE_HOLDER,
        CONFIG.interface.place_holder.clone(),
        "type and search".to_string(),
    );
    init_statics(
        &INDICATOR_WITH_ARG_MODULE,
        CONFIG.interface.indicator_with_arg_module.clone(),
        "".to_string(),
    );
    init_statics(
        &INDICATOR_NO_ARG_MODULE,
        CONFIG.interface.indicator_no_arg_module.clone(),
        "".to_string(),
    );
    init_statics(
        &SUGGESTION_MODE,
        CONFIG.interface.suggestion_mode.clone(),
        "list".to_string(),
    );
    init_statics(&SUGGESTION_LINES, CONFIG.interface.suggestion_lines, 1);
    init_statics(
        &DEFAULT_MODULE_MESSAGE,
        CONFIG.interface.default_module_message.clone(),
        "".to_string(),
    );
    init_statics(
        &EMPTY_MODULE_MESSAGE,
        CONFIG.interface.empty_module_message.clone(),
        "".to_string(),
    );
    init_statics(&PREFIX_PADDING, CONFIG.interface.prefix_padding, 0);
    init_statics(
        &PREFIX_COLOR,
        CONFIG.interface.prefix_color.clone(),
        "".to_string(),
    );
    init_statics(
        &DESCRIPTION_COLOR,
        CONFIG.interface.description_color.clone(),
        "".to_string(),
    );
    init_statics(
        &PLACE_HOLDER_COLOR,
        CONFIG.interface.place_holder_color.clone(),
        "\x1b[30m".to_string(),
    );
    init_statics(
        &HINT_COLOR,
        CONFIG.interface.hint_color.clone(),
        "\x1b[30m".to_string(),
    );

    // rustyline editor setup
    *SELECTION_INDEX.lock().unwrap() = 0;
    let mut rl: Editor<OtterHelper, DefaultHistory> = Editor::new().unwrap();
    // set OtterHelper as hint and completion provider
    rl.set_helper(Some(OtterHelper {
        hints: map_hints().expect("failed to provide hints"),
    }));

    // check if esc_to_abort is on
    if cached_statics(&ESC_TO_ABORT, true) {
        rl.bind_sequence(
            KeyEvent::new('\x1b', Modifiers::NONE),
            EventHandler::Simple(Cmd::Interrupt),
        );
    }

    // check if vi_mode is on, and set up keybinds accordingly
    if cached_statics(&VI_MODE, false) {
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
        if !cached_statics(&EXTERNAL_EDITOR, "".to_string()).is_empty() {
            rl.bind_sequence(
                KeyEvent::new('v', Modifiers::NONE),
                EventHandler::Conditional(Box::from(ExternalEditor)),
            );
        }
    } else {
        // emacs bindings
        rl.bind_sequence(
            KeyEvent::new('n', Modifiers::CTRL),
            EventHandler::Conditional(Box::from(ListItemDown)),
        );
        rl.bind_sequence(
            KeyEvent::new('p', Modifiers::CTRL),
            EventHandler::Conditional(Box::from(ListItemUp)),
        );
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
        if !cached_statics(&EXTERNAL_EDITOR, "".to_string()).is_empty() {
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
        KeyEvent::new('j', Modifiers::CTRL),
        EventHandler::Conditional(Box::from(ListItemDown)),
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
        KeyEvent::new('k', Modifiers::CTRL),
        EventHandler::Conditional(Box::from(ListItemUp)),
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
        KeyEvent::new('l', Modifiers::CTRL),
        EventHandler::Conditional(Box::from(ListItemSelect)),
    );
    rl.bind_sequence(
        KeyEvent(KeyCode::Right, Modifiers::NONE),
        EventHandler::Simple(Cmd::Move(Movement::ForwardChar(1))),
    );

    // start the flow
    loop {
        // print from header commands
        let header_cmd = cached_statics(&HEADER_CMD, "".to_string());
        // format exec_cmd
        let exec_cmd = cached_statics(&EXEC_CMD, "sh -c".to_string());
        let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
        let mut shell_cmd = Command::new(cmd_parts[0]);
        for arg in &cmd_parts[1..] {
            shell_cmd.arg(arg);
        }
        // run header_cmd and pipe the output
        let output = shell_cmd
            .arg(&header_cmd)
            .output()
            .expect("Failed to launch header command...");
        // trim stdout according to toml config
        let remove_lines_count = cached_statics(&HEADER_CMD_TRIMMED_LINES, 0);
        let stdout = from_utf8(&output.stdout).unwrap();
        let lines: Vec<&str> = stdout.lines().collect();
        let remaining_lines = if lines.len() >= remove_lines_count {
            lines[..lines.len() - remove_lines_count].join("\n") + "\x1b[?25h"
        } else {
            "not enough lines of header_cmd output to be trimmed".to_string()
        };
        // check if header_cmd and header should be concatenated, form header content accordingly
        let concatenated_header = format!(
            "{}{}{}",
            remaining_lines,
            if cached_statics(&HEADER_CONCATENATE, false) || header_cmd.is_empty() {
                ""
            } else {
                "\n"
            },
            cached_statics(&HEADER, "".to_string())
        );

        // run rustyline with configured header
        let prompt = rl.readline(&concatenated_header);
        match prompt {
            Ok(_) => {}
            Err(_) => {
                process::exit(0);
            }
        }
        let prompt = prompt.expect("failed to read prompt");

        // flow switches setup
        let mut loop_switch = cached_statics(&LOOP_MODE, false);
        let clear_switch = cached_statics(&CLEAR_SCREEN_AFTER_EXECUTION, false);

        // clear screen if clear_screen_after_execution is on
        if clear_switch {
            print!("\x1B[2J\x1B[1;1H");
            std::io::stdout().flush().expect("failed to flush stdout");
        }

        // matching the prompted prefix with module prefixes to decide what to do
        let prompted_prfx = prompt.split_whitespace().next().unwrap_or("");
        let module_prfx = CONFIG
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
                    run_module_command(&module.cmd.replace("{}", &argument).to_string());
                // Condition 2: when user input is exactly the same as the no-arg module
                } else if remove_ascii(&module.prefix) == prompt.trim_end() {
                    run_module_command(&module.cmd);
                // Condition 3: when no-arg modules is running with arguement
                } else {
                    run_designated_module(prompt, cached_statics(&DEFAULT_MODULE, "".to_string()))
                }
            }
            // if user input doesn't start with some module prefixes
            _ => {
                // Condition 1: when user input is empty, run the empty module
                if prompt.is_empty() {
                    run_designated_module(prompt, cached_statics(&EMPTY_MODULE, "".to_string()))
                // Condition 2: when helper keyword is passed, open cheatsheet in less
                } else if prompt.trim_end() == cached_statics(&CHEATSHEET_ENTRY, "?".to_string()) {
                    // setup variables
                    let prefix_color = cached_statics(&PREFIX_COLOR, "".to_string());
                    let description_color = cached_statics(&DESCRIPTION_COLOR, "".to_string());
                    let indicator_with_arg_module =
                        &cached_statics(&INDICATOR_WITH_ARG_MODULE, "".to_string());
                    let indicator_no_arg_module =
                        &cached_statics(&INDICATOR_NO_ARG_MODULE, "".to_string());
                    // run general.cheatsheet.viewer
                    let exec_cmd = cached_statics(&EXEC_CMD, "sh -c".to_string());
                    let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
                    let mut shell_cmd = Command::new(cmd_parts[0]);
                    for arg in &cmd_parts[1..] {
                        shell_cmd.arg(arg);
                    }
                    let mut child = shell_cmd
                        .arg(cached_statics(
                            &CHEATSHEET_VIEWER,
                            "less -R; clear".to_string(),
                        ))
                        .stdin(Stdio::piped())
                        .spawn();
                    if let Ok(ref mut child) = child {
                        if let Some(stdin) = child.stdin.as_mut() {
                            // Format cheat sheet
                            let mapped_modules = CONFIG
                                .modules
                                .iter()
                                .map(|module| {
                                    let arg_indicator = if module.with_argument == Some(true) {
                                        indicator_with_arg_module
                                    } else {
                                        indicator_no_arg_module
                                    };
                                    let width = CONFIG
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
                    run_designated_module(prompt, cached_statics(&DEFAULT_MODULE, "".to_string()))
                }
            }
        }

        // run general.callback
        general_callback();
        // if not in loop_mode, quit the process
        if !loop_switch {
            break;
        }
    }
}
