extern crate serde;
extern crate urlencoding;
extern crate inquire;
extern crate toml;
extern crate fuzzy_matcher;
extern crate regex;

/* Note for Modified Crate
The inquire crate was modified with three files for better ui:
    1. src/ui/api/render_config.rs: list_prefix was added into struct RenderConfig, following some definitions in several blocks
    2. src/ui/backend: [] was removed from fn render_help_message; list_prefix was added to fn print_option_prefix
    3. src/input/action.rs was modified in fn from_key for ctrl+k & ctrl+u keybinds
*/

use std::{str::from_utf8, fs, env, path::Path, error::Error, process, process::Command};
use inquire::{autocompletion::{Autocomplete, Replacement}, CustomUserError, Text, ui::{RenderConfig, Styled, StyleSheet, Attributes, IndexPrefix}};
use serde::Deserialize;
use toml::from_str;
use urlencoding::encode;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use regex::Regex;

// Define config structure
#[derive(Deserialize, Default)]
struct Config {
    #[serde(default)]
    general: General,
    #[serde(default)]
    interface: Interface,
    #[serde(default)]
    modules: Vec<Module>,
}

#[derive(Deserialize, Default)]
struct General {
    default_module: Option<String>,
    empty_module: Option<String>,
    exec_cmd: Option<String>,
    show_suggestion: Option<bool>,
}

#[derive(Deserialize, Default)]
struct Interface {
    header: Option<String>,
    header_cmd: Option<String>,
    header_cmd_trimmed_lines: Option<usize>,
    prompt_prefix: Option<String>,
    list_prefix: Option<String>,
    highlighted_prefix: Option<String>,
    scroll_up_prefix: Option<String>,
    scroll_down_prefix: Option<String>,
    help_message: Option<String>,
    suggestion_lines: Option<usize>,
    place_holder: Option<String>,
}

#[derive(Deserialize)]
struct Module {
    description: String,
    prefix: String,
    cmd: String,
    with_argument: Option<bool>,
    url_encode: Option<bool>,
    prehook: Option<String>,
    callback: Option<String>,
}

// Define suggestion autocompleter
#[derive(Clone, Default)]
struct SuggestionCompleter {
    input: String,
    hints: Vec<String>,
}

impl SuggestionCompleter {
    fn update_input(&mut self, input: &str) -> Result<(), CustomUserError> {
        if input == self.input && !self.hints.is_empty() {
            return Ok(());
        }
        self.input = input.to_owned();
        self.hints.clear();

        let config = read_config();
        let mut input_hint: Vec<String> = config
            .unwrap()
            .modules
            .iter()
            .map(|module| module.prefix.clone() + " " + &module.description
            )
            .collect();
        input_hint.sort();

        for entry in input_hint {
            let hint = entry;
            let hint_str = hint;

            self.hints.push(hint_str);
        }
        Ok(())
    }

    fn fuzzy_sort(&self, input: &str) -> Vec<(String, i64)> {
        let mut matches: Vec<(String, i64)> = self
            .hints
            .iter()
            .filter_map(|hint| {
                SkimMatcherV2::default()
                    .smart_case()
                    .fuzzy_match( &remove_ascii(hint), input)
                    //match prefix only: .fuzzy_match( &remove_ascii(hint).split_whitespace().next()?, input)
                    .map(|score| (hint.clone(), score))
            })
            .collect();

        matches.sort_by(|a, b| b.1.cmp(&a.1));
        matches
    }
}

impl Autocomplete for SuggestionCompleter {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        self.update_input(input)?;

        let matches = self.fuzzy_sort(input);
        Ok(matches.into_iter()
            .take(15)
            .map(|(hint, _)| hint)
            .collect())
    }

    fn get_completion(
        &mut self,
        input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<Replacement, CustomUserError> {
        self.update_input(input)?;

        Ok(if let Some(suggestion) = highlighted_suggestion {
            Replacement::Some(suggestion)
        } else {
            let matches = self.fuzzy_sort(input);
                matches
                    .first()
                    .map(|(hint, _)| Replacement::Some(
                        remove_ascii(hint)
                            .split_whitespace()
                            .next()?
                            .to_string()))
                    .unwrap_or(Replacement::None)
        })
    }
}

// function to read from TOML Config
fn read_config() -> Result<Config, Box<dyn Error>> {
    let home_dir = env::var("HOME").unwrap_or_else(|_| String::from("/"));
    let xdg_config_path = format!("{}/.config/otter-launcher/config.toml", home_dir);

    // fallback from xdg_config_path to /etc
    let config_file: &str;
    if Path::new(&xdg_config_path).exists() {
        config_file = &xdg_config_path;
    } else {
        config_file = "/etc/otter-launcher/config.toml";
    }

    let contents = fs::read_to_string(config_file)
        .unwrap_or_else(|_| "".to_string());
    let config: Config = from_str(&contents)?;
    Ok(config)
}

// function to remove ascii color code from &str
fn remove_ascii(input: &str) -> String {
    let re = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(input, "").to_string()
}

// function to run module.cmd
fn run_module_command(mod_cmd_arg: &str, mut shell_cmd: Command) {
    let mut run_module_cmd = shell_cmd.arg(mod_cmd_arg)
        .spawn()
        .expect("Failed to launch callback...");
    let _ = run_module_cmd.wait().expect("Callback process wasn't running");
}

// function to run callback and prehook
fn callback(cb_cmd_arg: &Option<String>, mut shell_cmd: Command) {
    if cb_cmd_arg.is_some() {
        let mut callback = shell_cmd.arg(cb_cmd_arg.clone().unwrap())
            .spawn()
            .expect("Failed to launch callback cmd...");
        let _ = callback.wait().expect("Callback cmd wasn't running");
    }
}

// main function
fn main() {
    // comparing prompt with loaded configs
    match read_config() {
        Ok(config) => {
            // load header and header_cmd from config file
            let prompt_header = config
                .interface.header
                .unwrap_or("".to_string());
            let header_cmd = config
                .interface.header_cmd
                .unwrap_or("".to_string());
            if !header_cmd.is_empty() {
                let output = Command::new("sh")
                    .arg("-c")
                    .arg(header_cmd)
                    .output()
                    .expect("Failed to launch header command...");
                if output.status.success() {
                    let prompt_header = from_utf8(&output.stdout).unwrap();
                    let lines: Vec<&str> = prompt_header.lines().collect();
                    let remove_lines_count = config
                        .interface.header_cmd_trimmed_lines
                        .unwrap_or(0);
                    if lines.len() > remove_lines_count {
                        let remaining_lines = &lines[..lines.len() - remove_lines_count];
                        for line in remaining_lines {
                        println!("{}", line);
                        }
                    } else {
                        println!("{}", prompt_header.trim_end());
                    }
                } else {
                    eprintln!("Header command failed with status: {}", output.status);
                }
            }

            // getting prompt from user input, and set up input interface as per the config file
            let prompt = Text {
                message: &config
                    .interface.prompt_prefix
                    .unwrap_or(" \x1b[34m \x1b[0m otter-launcher \x1b[34m>\x1b[0m".to_string()),
                initial_value: None,
                default: None,
                autocompleter: if config
                    .general.show_suggestion
                    .unwrap_or(false) == true {
                    Some(Box::new(SuggestionCompleter::default()))
                } else {
                    None
                },
                placeholder: Some(
                    &config
                    .interface.place_holder
                    .unwrap_or("type and search..."
                        .to_string())
                ),
                formatter: Text::DEFAULT_FORMATTER,
                validators: Vec::new(),
                page_size: config
                        .interface.suggestion_lines
                        .unwrap_or(1),
                render_config:
                    RenderConfig {
                        prompt_prefix: Styled::new(&prompt_header),
                        selected_option: Some(StyleSheet::new().with_attr(Attributes::BOLD)),
                        option_index_prefix: IndexPrefix::SpacePadded,
                        highlighted_option_prefix: Styled::new(
                                &config
                                .interface.highlighted_prefix
                                .unwrap_or("  \x1b[31m >\x1b[0m".to_string())),
                        scroll_down_prefix: Styled::new(
                                &config
                                .interface.scroll_down_prefix
                                .unwrap_or("  \x1b[31m #\x1b[0m".to_string())),
                        scroll_up_prefix: Styled::new(
                                &config
                                .interface.scroll_up_prefix
                                .unwrap_or("  \x1b[31m #\x1b[0m".to_string())),
                        list_prefix: Styled::new(
                                &config
                                .interface.list_prefix
                                .unwrap_or("    ".to_string())),
                        ..Default::default()
                    },
                help_message: Some(
                    &config
                    .interface.help_message
                    .unwrap_or("".to_string())
                ),
            }.prompt()
                .unwrap_or_else(|_err|{
                    String::from("otter_magic_canceled_and_quit")
                }).to_string();
            
            let prompt = remove_ascii(&prompt);

            // format the shell command by which module.cmd are launched
            let exec_cmd = config
                .general.exec_cmd.unwrap_or("sh -c".to_string());
            let mut cmd_parts = exec_cmd.split_whitespace();
            let exec_cmd_base = cmd_parts.next().expect("No exec_cmd found");
            let exec_cmd_args: Vec<&str> = cmd_parts.collect();
            let mut shell_cmd = Command::new(exec_cmd_base);
            let mut shell_prehook = Command::new(exec_cmd_base);
            let mut shell_callback = Command::new(exec_cmd_base);
            for arg in &exec_cmd_args {
                shell_cmd.arg(arg);
                shell_prehook.arg(arg);
                shell_callback.arg(arg);
            }

            // matching the prompted prefix with module prefixes to decide what to do
            let prompted_prfx = prompt
                .split_whitespace()
                .next()
                .unwrap_or("");
            let module_prfx = config
                .modules
                .iter()
                .find(|module| remove_ascii( &module.prefix ) == prompted_prfx);

            match module_prfx {
                // if user input starts with some module prefixes
                Some(module) => {
                    // determine whether the prompt should be urlencoded
                    let argument = if module.url_encode
                        .unwrap_or(false) == true {
                            encode(prompt
                                .trim_start_matches(prompted_prfx)
                                .trim_start_matches(" ")
                            ).to_string()
                        } else {
                            prompt
                            .trim_start_matches(prompted_prfx)
                            .trim_start_matches(" ")
                            .to_string()
                        };

                    // Condition 1: when the selected module runs with arguement
                    if module.with_argument.unwrap_or(false) == true {
                        callback(&module.prehook, shell_prehook);
                        run_module_command(
                            &format!("{}", module.cmd.replace("{}", &argument)),
                            shell_cmd);
                        callback(&module.callback, shell_callback);
                    // Condition 2: when user input is exactly the same as the no-arg module
                    } else if remove_ascii( &module.prefix ) == prompt {
                        callback(&module.prehook, shell_prehook);
                        run_module_command(&module.cmd, shell_cmd);
                        callback(&module.callback, shell_callback);
                    // Condition 3: when the selected module is selected by suggestion (prompt=prefix+desc)
                    } else if remove_ascii( &module.prefix ) + " " + &module.description == prompt {
                        callback(&module.prehook, shell_prehook);
                        run_module_command(&module.cmd, shell_cmd);
                        callback(&module.callback, shell_callback);
                    // Condition 4: when no-arg modules is running with arguement
                    } else {
                        let defaultmodule = config
                            .general.default_module
                            .unwrap_or("".to_string());
                        // Test if default module is set
                        if defaultmodule.is_empty() {
                            run_module_command(
                                &format!(
                                    "setsid -f xdg-open https://www.google.com/search?q='{}'", prompt),
                                shell_cmd);
                        } else {
                            let default_module = config
                                .modules
                                .iter()
                                .find(|module| 
                                    remove_ascii( &module.prefix ) == defaultmodule);
                            let prompt_wo_prefix = if default_module
                                .unwrap()
                                .url_encode.unwrap_or(false) == true {
                                    encode(&prompt).to_string()
                            } else {
                                prompt
                            };
                            run_module_command(
                                &format!("{}", &default_module
                                    .unwrap()
                                    .cmd
                                    .replace("{}", &prompt_wo_prefix)),
                                shell_cmd);
                        }
                    }
                },
                // if user input doesn't start with some module prefixes
                None => {
                    // Condition 1: when user input is empty (and no module selected), run the empty module
                    if prompt.is_empty() {
                        let emptymodule = config
                            .general.empty_module
                            .unwrap_or("".to_string());
                        if emptymodule.is_empty() {
                            process::exit(0);
                        } else {
                            let empty_module = config
                                .modules
                                .iter()
                                .find(|module| remove_ascii( &module.prefix ) == emptymodule);
                            callback(&empty_module.unwrap().prehook, shell_prehook);
                            run_module_command(
                                &format!("{}", &empty_module
                                    .unwrap()
                                    .cmd.replace("{}", "")),
                                shell_cmd);
                            callback(&empty_module.unwrap().callback, shell_callback);
                        }
                    // Condition 2: when canceled with esc (thus no module selected), exit
                    } else if prompt == "otter_magic_canceled_and_quit" {
                        process::exit(0);
                    // Condition 3: when no module is matched, run the default module
                    } else {
                        let defaultmodule = config
                            .general.default_module
                            .unwrap_or("".to_string());
                        // test if default module is set
                        if defaultmodule.is_empty() {
                            run_module_command(
                                &format!(
                                    "setsid -f xdg-open https://www.google.com/search?q='{}'", prompt),
                                shell_cmd);
                        } else {
                            let default_module = config
                                .modules
                                .iter()
                                .find(|module| 
                                    remove_ascii( &module.prefix ) == defaultmodule);
                            let prompt_wo_prefix = if default_module
                                .unwrap()
                                .url_encode.unwrap_or(false) == true {
                                    encode(&prompt).to_string()
                            } else {
                                prompt
                            };
                            callback(&default_module.unwrap().prehook, shell_prehook);
                            run_module_command(
                                &format!("{}", &default_module
                                    .unwrap()
                                    .cmd
                                    .replace("{}", &prompt_wo_prefix)),
                                shell_cmd);
                            callback(&default_module.unwrap().callback, shell_callback);
                        }
                    }
                }
            }
        },
        // if there's something wrong with the config
        Err(e) => println!("Error reading config.toml: {}", e),
    }
}
