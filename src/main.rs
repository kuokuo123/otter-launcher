// otter-launcher's main flow

mod glob_vars;
mod graphics;
mod helper;
mod keybinds;
mod mod_exec;

use glob_vars::*;
use keybinds::*;
use mod_exec::*;
use std::{
    io::Write,
    process,
    process::{Command, Stdio},
    sync::Mutex,
};
use terminal_size::{Width, terminal_size};
use urlencoding::encode;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // initializing global variables
    init_all_statics();

    // initializing menu selection index
    *SELECTION_INDEX
        .get_or_init(|| Mutex::new(0))
        .lock()
        .unwrap() = 0;

    // rustyline editor setup
    let mut rl = customized_rustyline_editor()?;

    // run overlay_cmd and cache the ouput, if any
    get_overlay_lines();

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
            // mannually add in neccesary shell vars for expansion
            shell_cmd.env(
                "COLUMNS",
                terminal_size()
                    .map(|(Width(w), _)| w.to_string())
                    .unwrap_or_else(|| "80".to_string()),
            );
            shell_cmd.env(
                "HOSTNAME",
                hostname::get()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            );
            shell_cmd.env(
                "HOST",
                hostname::get()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            );
            let status = shell_cmd
                .arg(&header_cmd)
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()?;

            if !status.success() {
                eprintln!("header_cmd failed to run with status: {}", status);
            }
            println!("\x1b[{}A", remove_lines_count + 1)
        }

        // print header
        let config_header = cached_statics(&HEADER, || "sh -c".to_string());
        let expanded_header = expand_env_vars(&config_header);
        let header_lines: Vec<&str> = expanded_header.split('\n').collect();

        // set up variables to form the header
        let layout_down_string = if layout_down > 0 {
            format!("{}", "\n".repeat(layout_down))
        } else {
            String::new()
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

        let prompt: String;

        // if prompted from cli, output directly without entering rustyline editor
        if let Some(cli_prompt) = CLI_PROMPT.get() {
            prompt = cli_prompt.to_owned();
        } else {
            match rl.readline(&header) {
                Ok(line) => {
                    prompt = line;
                }
                Err(_) => {
                    process::exit(0);
                }
            }
        }

        // flow switches setup
        let mut loop_switch = cached_statics(&LOOP_MODE, || false);

        // clear screen if clear_screen_after_execution is on
        if cached_statics(&CLEAR_SCREEN_AFTER_EXECUTION, || false) {
            print!("\x1B[2J\x1B[1;1H");
            std::io::stdout().flush()?
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
                    run_designated_module(prompt, cached_statics(&DEFAULT_MODULE, || String::new()))
                }
            }
            // if user input doesn't start with some module prefixes
            _ => {
                // Condition 1: when user input is empty, run the empty module
                if prompt.is_empty() {
                    run_designated_module(prompt, cached_statics(&EMPTY_MODULE, || String::new()))
                // Condition 2: when helper keyword is passed, open cheatsheet
                } else if prompt.trim_end() == cached_statics(&CHEATSHEET_ENTRY, || "?".to_string())
                {
                    let _ = cheat_sheet();
                    loop_switch = true;
                // Condition 3: when no module is matched, run the default module
                } else {
                    run_designated_module(prompt, cached_statics(&DEFAULT_MODULE, || String::new()))
                }
            }
        }

        // run general.callback
        let callback = cached_statics(&CALLBACK, || String::new());
        if !callback.is_empty() {
            let _ = run_module_command_unbind_proc(callback);
        }

        // if not in loop_mode, quit the process
        if !loop_switch {
            break Ok(());
        }
    }
}
