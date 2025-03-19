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

extern crate serde;
extern crate urlencoding;
extern crate toml;
extern crate rustyline;
extern crate rustyline_derive;
extern crate regex;

use std::{str::from_utf8, env, path::Path, error::Error, process, process::Command, sync::Mutex, borrow::Cow};
use serde::Deserialize;
use once_cell::sync::Lazy;
use urlencoding::encode;
use rustyline::{EditMode, Context, Editor, KeyEvent, Modifiers, EventHandler, Cmd, history::DefaultHistory, hint::{Hint, Hinter}, config::Configurer, highlight::Highlighter};
use rustyline_derive::{Helper, Completer, Validator};
use regex::Regex;

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
    vi_mode: Option<bool>,
}

#[derive(Deserialize, Default)]
struct Interface {
    header: Option<String>,
    header_cmd: Option<String>,
    header_cmd_trimmed_lines: Option<usize>,
    prompt_prefix: Option<String>,
    list_prefix: Option<String>,
    place_holder: Option<String>,
    default_module_message: Option<String>,
    empty_module_message: Option<String>,
    suggestion_lines: Option<usize>,
    indicator_no_arg_module: Option<String>,
    indicator_with_arg_module: Option<String>,
    prefix_color: Option<String>,
    description_color: Option<String>,
    place_holder_color: Option<String>,
}

#[derive(Deserialize, Clone)]
struct Module {
    description: String,
    prefix: String,
    cmd: String,
    with_argument: Option<bool>,
    url_encode: Option<bool>,
    prehook: Option<String>,
    callback: Option<String>,
}

// Load toml config
static CONFIG: Lazy<Config> = Lazy::new(|| {
    let home_dir = env::var("HOME").unwrap_or_else(|_| String::from("/"));
    let xdg_config_path = format!("{}/.config/otter-launcher/config.toml", home_dir);

    // fallback from xdg_config_path to /etc
    let config_file: &str;
    if Path::new(&xdg_config_path).exists() {
        config_file = &xdg_config_path;
    } else {
        config_file = "/etc/otter-launcher/config.toml";
    }
    read_config(config_file).expect("")
});

fn read_config(file_path: &str) -> Result<Config, Box<dyn std::error::Error>> {
    let contents = std::fs::read_to_string(file_path)?;
    let config: Config = toml::from_str(&contents)?;
    Ok(config)
}

// Functions to load config values
// load at runtime
fn default_module() -> String {
    CONFIG.general.default_module.clone().unwrap_or("".to_string())
}
fn default_module_message() -> String {
    CONFIG.interface.default_module_message.clone().unwrap_or("".to_string())
}
fn empty_module() -> String {
    CONFIG.general.empty_module.clone().unwrap_or("".to_string())
}
fn empty_module_message() -> String {
    CONFIG.interface.empty_module_message.clone().unwrap_or("".to_string())
}
fn exec_cmd() -> String {
    CONFIG.general.exec_cmd.clone().unwrap_or("sh -c".to_string())
}
fn esc_to_abort() -> bool {
    CONFIG.general.esc_to_abort.clone().unwrap_or(true)
}
fn vi_mode() -> bool {
    CONFIG.general.vi_mode.clone().unwrap_or(false)
}
fn list_prefix() -> String {
    CONFIG.interface.list_prefix.clone().unwrap_or("".to_string())
}
fn place_holder() -> String {
    CONFIG.interface.place_holder.clone().unwrap_or("type and search...".to_string())
}
fn indicator_no_arg_module() -> String {
    CONFIG.interface.indicator_no_arg_module.clone().unwrap_or("".to_string())
}
fn indicator_with_arg_module() -> String {
    CONFIG.interface.indicator_with_arg_module.clone().unwrap_or("".to_string())
}
fn header_cmd() -> String {
    CONFIG.interface.header_cmd.clone().unwrap_or("".to_string())
}
fn header_cmd_trimmed_lines() -> usize {
    CONFIG.interface.header_cmd_trimmed_lines.unwrap_or(0)
}
fn header() -> String {
    CONFIG.interface.header.clone().unwrap_or("".to_string())
}
fn prompt_prefix() -> String {
    CONFIG.interface.prompt_prefix.clone().unwrap_or("\x1b[34m \x1b[0m otter-launcher \x1b[34m>\x1b[0m ".to_string())
}
fn prefix_color() -> String {
    CONFIG.interface.prefix_color.clone().unwrap_or("\x1b[90m".to_string())
}
fn description_color() -> String {
    CONFIG.interface.description_color.clone().unwrap_or("\x1b[90m".to_string())
}
fn place_holder_color() -> String {
    CONFIG.interface.place_holder_color.clone().unwrap_or("\x1b[90m".to_string())
}
// load and cache as statics
static SUGGESTION_LINES: Lazy<Mutex<Option<usize>>> = Lazy::new(|| Mutex::new(None));
fn init_suggestion_lines() {
    let suggestion = CONFIG.interface.suggestion_lines.clone().unwrap_or(1);
    let mut suggestion_lines = SUGGESTION_LINES.lock().unwrap();
    *suggestion_lines = Some(suggestion);
}
fn cached_suggestion_lines() -> usize {
    let suggestion_lines = SUGGESTION_LINES.lock().unwrap();
        suggestion_lines.clone().unwrap_or(1)
}

// Define the helper that provide hints, highlights to the rustyline editor
#[derive(Completer, Helper, Validator)]
struct OtterHelper {
    hints: Vec<ModuleHint>,
}

// Define the structure of every formatted hint
#[derive(Hash, Debug, PartialEq, Eq)]
struct ModuleHint {
    display: String,
    completion: usize,
    w_arg: Option<bool>,
}

// The coloring functionality of OtterHelper
impl Highlighter for OtterHelper {
    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        return hint
            .lines()
            .map(|line| {
                if line == "otter_magic_first_otter" {
                    place_holder_color() + &place_holder() + "\x1b[0m"
                } else if line == &empty_module_message() {
                    line.to_string()
                } else if line == &default_module_message() {
                    line.to_string()
                } else {
                    let mut parts = line.split_whitespace();
                    let prefix = parts.next().unwrap_or("");
                    let desc = parts.collect::<Vec<&str>>().join(" ");

                    format!("{}{}{}{} {}{}{}",
                        list_prefix(),
                        prefix_color(),
                        prefix,
                        "\x1b[0m",
                        description_color(),
                        desc,
                        "\x1b[0m")
                }
            })
            .collect::<Vec<String>>()
            .join("\n")
            .into();
    }
}

// the hint providing functionality of OtterHelper
// Select a hint for OtterHelper to pass into rustyline prompt editor (from a vector of all formatted hints)
impl Hinter for OtterHelper {
    type Hint = ModuleHint;
    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<ModuleHint> {
        let aggregated_hint_lines = self.hints
                .iter()
                .filter_map(|i| {
                    let line_wo_ascii = if i.w_arg.unwrap_or(false) == true {
                        remove_ascii(
                            &(line.split_whitespace()
                                .next()
                                .unwrap_or(""))
                        )
                    } else {
                        remove_ascii(&line.replace(" ", "\n"))
                    };

                    if remove_ascii(&i.display).starts_with( &line_wo_ascii ) {
                        Some(i.display.as_str())
                    } else {
                        None 
                    }
                })
                .take( cached_suggestion_lines() )
                .collect::<Vec<&str>>();

        let agg_line = aggregated_hint_lines.join("\n");
        let e_module = empty_module_message();
        let d_module = default_module_message();

        if line.is_empty() {
            Some( 
                ModuleHint {
                    display: format!(
                        "{}{}", 
                        "otter_magic_first_otter",
                        if e_module.is_empty() { 
                            if agg_line.is_empty() { 
                                "".to_string() 
                            } else { 
                                format!("\n{}", agg_line) 
                            } 
                        } else { 
                            format!("\n{}", e_module) 
                        },
                    ),
                    completion: pos,
                    w_arg: None,
                }.suffix(pos)
            )
        } else {
            Some( 
                ModuleHint {
                    display: format!(
                        "{}", 
                        if agg_line.is_empty() { 
                            if d_module.is_empty() { 
                                "".to_string() 
                            } else { 
                                format!("\n{}", d_module) } 
                        } else { 
                            format!("\n{}", agg_line) 
                        },
                    ),
                    completion: pos,
                    w_arg: None,
                }.suffix(pos)
            )
        }
    }
}

// Define the functions that hint objects can modify the value within it self
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
    // Text to display when hint is active
    fn display(&self) -> &str {
        &self.display
    }
    //Text to insert in line when right arrow is pressed
    fn completion(&self) -> Option<&str> {
        let prfx = self.display
            .trim_start_matches("\n")
            .trim_start_matches("otter_magic_first_otter")
            .trim_start_matches(&default_module_message())
            .split_whitespace()
            .next()
            .unwrap_or("");
        if prfx.len() > self.completion && self.completion > 0 {
            Some(&prfx[self.completion..])
        } else {
            None
        }
    }
}

// function to format vec<hints> according to configured modules, and to provide them to hinter
fn map_hints() -> Result<Vec<ModuleHint>, Box<dyn Error>> {
    let set = CONFIG
        .modules
        .iter()
        .map(|module| {
            let arg_indicator = 
                if module.with_argument == Some(true) {
                    indicator_with_arg_module()
                } else {
                    indicator_no_arg_module() };

            let hint_string = format!("{} {}{}",
                &module.prefix,
                arg_indicator,
                &module.description);
            ModuleHint:: new( &hint_string, &hint_string, module.with_argument)
        })
        .collect::<Vec<_>>();
    Ok(set)
}

// function to remove ascii color code from &str
fn remove_ascii(text: &str) -> String {
    let re = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(text, "").to_string()
}

// function to run module.cmd
fn run_module_command(mod_cmd_arg: &str, module: &Module) {
    // format the shell command by which the module commands are launched
    let exec_cmd = exec_cmd();
    let mut cmd_parts = exec_cmd.split_whitespace();
    let exec_cmd_base = cmd_parts.next().expect("No exec_cmd found");
    let exec_cmd_args: Vec<&str> = cmd_parts.collect();

    // run prehook is there is one
    if module.prehook.is_some() {
        let mut ph_cmd = Command::new(exec_cmd_base);
        for arg in &exec_cmd_args {
            ph_cmd.arg(arg);
        }
        let mut prehook = ph_cmd.arg(module.prehook.as_ref().unwrap())
            .spawn()
            .expect("Failed to launch prehook cmd...");
        let _ = prehook.wait().expect("Prehook cmd wasn't running");
    }

    // run the module cmd
    let mut shell_cmd = Command::new(exec_cmd_base);
    for arg in &exec_cmd_args {
        shell_cmd.arg(arg);
    }
    let mut run_module_cmd = shell_cmd.arg(mod_cmd_arg)
        .spawn()
        .expect("Failed to launch callback...");
    let _ = run_module_cmd.wait().expect("Module.cmd process wasn't running");

    // run callback if there is one
    if module.callback.is_some() {
        let mut cb_cmd = Command::new(exec_cmd_base);
        for arg in &exec_cmd_args {
            cb_cmd.arg(arg);
        }
        let mut callback = cb_cmd.arg(module.callback.as_ref().unwrap())
            .spawn()
            .expect("Failed to launch callback cmd...");
        let _ = callback.wait().expect("Callback cmd wasn't running");
    }
}

// function to run empty & default modules
fn run_designated_module(prompt: String, prfx: String) {
    // test if the designated module is set
    if prfx.is_empty() {
        println!("{}", prompt)
    } else {
    // if set
        // find the designated module
        let target_module = CONFIG.modules
            .iter()
            .find(|module| 
                remove_ascii(&module.prefix) == prfx);
        let target_module = target_module.unwrap();
        // whether to use url encoding
        let prompt_wo_prefix = if target_module
            .url_encode.unwrap_or(false) == true {
                encode(&prompt).to_string()
        } else {
            prompt
        };
        // run the module's command
        run_module_command(
            &format!("{}", target_module
                .cmd
                .replace("{}", &prompt_wo_prefix)),
            target_module);
    }
}

// main function
fn main() {
    //initializing static variables
    init_suggestion_lines();

    // print headers
    if !header_cmd().is_empty() {
        let output = Command::new("sh")
            .arg("-c")
            .arg(header_cmd())
            .output()
            .expect("Failed to launch header command...");

        if output.status.success() {
            let pprefix = from_utf8(&output.stdout).unwrap();
            let lines: Vec<&str> = pprefix.lines().collect();
            let remove_lines_count = header_cmd_trimmed_lines();

            if lines.len() > remove_lines_count {
                let remaining_lines = &lines[..lines.len() - remove_lines_count];
                for line in remaining_lines {
                    println!("{}\x1b[?25h", line);
                }
            } else {
                println!("{}", pprefix.trim_end());
            }
        } else {
            eprintln!("Header_cmd failed with status: {}", output.status);
        }
    }

    // read prompt using rustyline interactive shell
    let mut rl: Editor<OtterHelper, DefaultHistory> = Editor::new().unwrap();
    rl.set_helper(
        Some( OtterHelper {
            hints: map_hints().expect("Failed to provide hints") }
    ));
    rl.bind_sequence(KeyEvent::new('\t', Modifiers::NONE),
        EventHandler::Simple(Cmd::CompleteHint));
    // check if vi_mode is on
    if vi_mode() == true { rl.set_edit_mode(EditMode::Vi) };
    // check if esc_to_abort is on
    if esc_to_abort() == true {
        rl.bind_sequence(KeyEvent::new('\x1b', Modifiers::NONE),
            EventHandler::Simple(Cmd::Interrupt));
    }
    let prompt = rl.readline(&(header()+&prompt_prefix()));
    match prompt {
        Ok(_) => { },
        Err(_) => {
            //println!("{:?}", err);
            process::exit(0);
        }
    }
    let prompt = prompt.expect("");

    // matching the prompted prefix with module prefixes to decide what to do
    let prompted_prfx = prompt
        .split_whitespace()
        .next()
        .unwrap_or("");
    let module_prfx = CONFIG
        .modules
        .iter()
        .find(|module| remove_ascii(&module.prefix) == prompted_prfx);

    match module_prfx {
        // if user input starts with some module prefixes
        Some(module) => {
            // determine whether the prompt should be urlencoded
            let argument = if module.url_encode
                .unwrap_or(false) == true {
                    encode(prompt
                        .trim_start_matches(prompted_prfx)
                        .trim_start()
                    ).to_string()
                } else {
                    prompt
                    .trim_start_matches(prompted_prfx)
                    .trim_start()
                    .to_string()
                };

            // Condition 1: when the selected module runs with arguement
            if module.with_argument.unwrap_or(false) == true {
                run_module_command(
                    &format!("{}", module.cmd.replace("{}", &argument)),
                    module);
            // Condition 2: when user input is exactly the same as the no-arg module
            } else if remove_ascii(&module.prefix) == prompt {
                run_module_command(
                    &module.cmd,
                    module);
            // Condition 3: when no-arg modules is running with arguement
            } else {
                run_designated_module(
                    prompt,
                    default_module())
            }
        },
        // if user input doesn't start with some module prefixes
        None => {
            // Condition 1: when user input is empty, run the empty module
            if prompt.is_empty() {
                run_designated_module(
                    prompt,
                    empty_module())
            // Condition 2: when no module is matched, run the default module
            } else {
                run_designated_module(
                    prompt,
                    default_module())
            }
        }
    }
}
