// library for customized rustyline helper, which provides hints and selection list (through rustyline highlighter) to the rustyline editor

use rustyline::{
    Context,
    completion::{Completer, Pair},
    highlight::Highlighter,
    hint::{Hint, Hinter},
};
use rustyline_derive::{Helper, Validator};
use std::{borrow::Cow, error::Error, process::Command, str::from_utf8, sync::Mutex};

use crate::glob_vars::*;
use crate::graphics::*;
use crate::mod_exec::*;

// define the structure of every formatted hint
pub struct ModuleHint {
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
pub struct OtterHelper {
    pub hints: Vec<ModuleHint>,
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
            let output = shell_cmd.arg(&overlay_cmd).output().ok()?;
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

// function to format vec<hints> according to configured modules, and to provide them to hinter
pub fn map_hints() -> Result<Vec<ModuleHint>, Box<dyn Error>> {
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
