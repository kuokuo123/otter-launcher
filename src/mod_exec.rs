// functions to execute commands in otter-launcher

use crate::glob_vars::*;
use std::{
    env,
    io::Write,
    process::{Command, Stdio},
};
use terminal_size::{Width, terminal_size};
use urlencoding::encode;

// function to remove ascii color code from &str
pub fn remove_ascii(text: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;]*[A-Za-z]").unwrap();
    re.replace_all(text, "").to_string()
}

// function to run empty & default modules
pub fn run_designated_module(prompt: String, prfx: String) {
    // test if the designated module is set
    if prfx.is_empty() {
        println!("{}", prompt)
    } else {
        // set a fallback module to prevent panic when no module is found
        let fallback = Module {
            description: String::new(),
            prefix: String::new(),
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
            let _ = run_module_command_unbind_proc(
                target_module
                    .cmd
                    .replace("{}", &prompt_wo_prefix)
                    .to_string(),
            );
        } else {
            let _ = run_module_command(
                target_module
                    .cmd
                    .replace("{}", &prompt_wo_prefix)
                    .to_string(),
            );
        }
    }
}

// function to run module.cmd
pub fn run_module_command(mod_cmd_arg: String) -> Result<(), Box<dyn std::error::Error>> {
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
    shell_cmd.spawn()?.wait()?;
    Ok(())
}

pub fn run_module_command_unbind_proc(
    mod_cmd_arg: String,
) -> Result<(), Box<dyn std::error::Error>> {
    // format the shell command by which the module commands are launched
    let mut shell_cmd = Command::new("setsid");
    shell_cmd.arg("-f");

    let exec_cmd = cached_statics(&EXEC_CMD, || "sh -c".to_string());
    let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
    for arg in &cmd_parts[0..] {
        shell_cmd.arg(arg);
    }

    // run module cmd
    shell_cmd.arg(mod_cmd_arg).spawn()?.wait()?;
    Ok(())
}

// function to format and show cheat sheet
pub fn cheat_sheet() -> Result<(), Box<dyn std::error::Error>> {
    // setup variables
    let prefix_color = cached_statics(&PREFIX_COLOR, || String::new());
    let description_color = cached_statics(&DESCRIPTION_COLOR, || String::new());
    let indicator_with_arg_module = &cached_statics(&INDICATOR_WITH_ARG_MODULE, || String::new());
    let indicator_no_arg_module = &cached_statics(&INDICATOR_NO_ARG_MODULE, || String::new());
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
    child?.wait()?;
    Ok(())
}

// function to expand env and variables
pub fn expand_env_vars(input: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '(' {
            let mut depth = 1;
            let mut j = i + 2;

            while j < chars.len() && depth > 0 {
                match chars[j] {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                j += 1;
            }

            if depth == 0 {
                let command: String = chars[i + 2..j - 1].iter().collect();
                let output = run_subshell(&command);
                result.push_str(&output);
                i = j;
                continue;
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    let var_re = regex::Regex::new(r"\$([A-Za-z_][A-Za-z0-9_]*)").unwrap();
    var_re
        .replace_all(&result, |caps: &regex::Captures| {
            env::var(&caps[1]).unwrap_or_default()
        })
        .into_owned()
}

pub fn run_subshell(cmd: &str) -> String {
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

    match shell_cmd.arg(&cmd).output() {
        Ok(output) => {
            let mut s = String::from_utf8_lossy(&output.stdout).to_string();
            if s.ends_with('\n') {
                s.pop();
            }
            if s.ends_with('\r') {
                s.pop();
            }
            s
        }
        Err(_) => String::new(),
    }
}

// function to cache or load overlay_cmd output
pub fn get_overlay_lines() -> &'static str {
    OVERLAY_LINES_CACHE.get_or_init(|| {
        let overlay_cmd = cached_statics(&OVERLAY_CMD, String::new);

        if overlay_cmd.is_empty() {
            return String::new();
        }

        let overlay_right = cached_statics(&OVERLAY_RIGHTWARD, || 0);
        let exec_cmd = cached_statics(&EXEC_CMD, || "sh -c".to_string());
        
        let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
        let mut shell_cmd = std::process::Command::new(cmd_parts[0]);
        for arg in &cmd_parts[1..] {
            shell_cmd.arg(arg);
        }

        // execute overlay_cmd
        let output = match shell_cmd.arg(&overlay_cmd).output() {
            Ok(out) => out,
            Err(_) => return String::new(),
        };
        
        let overlay_cmd_stdout = std::str::from_utf8(&output.stdout).unwrap_or("");
        let remove_lines_count = cached_statics(&OVERLAY_TRIMMED_LINES, || 0);
        
        let lines: Vec<&str> = overlay_cmd_stdout.split('\n').collect();
        let lines_count = lines.len();
        
        if lines_count > remove_lines_count {
            lines[..lines_count - remove_lines_count]
                .join(&format!("\n\x1b[{}G", overlay_right + 1))
        } else {
            "not enough lines of overlay_cmd output to be trimmed".to_string()
        }
    })
}

// function to print help
pub fn print_help() {
    println!(
        "\
\x1b[4motter-launcher (ot):\x1b[0m

A modularized script launcher featuring vi & emacs keybinds, released under the GNU GPL v3.0 license.

\x1b[4mUsage:\x1b[0m

otter-launcher [OPTIONS] [ARGUMENTS]...

\x1b[4mOptions:\x1b[0m

  -c, --config <FILE>  The path to a config file 
  -h, --help           Show help
  -v, --version        Show version

\x1b[4mBehavior:\x1b[0m

1. TUI mode: Running without ARGUMENTS enters TUI interface, where user prompt launches the matched configured module by the first word.

2. CLI mode: ARGUMENTS are taken as a direct prompt without entering the TUI. All configured modules are effective.

\x1b[4mConfiguration:\x1b[0m

Modules are specified in a TOML config file, expected at one of the below positions by priorities:

1. PATH specified by the --config flag
2. $HOME/.config/otter-launcher/config.toml
3. /etc/otter-launcher/config.toml

The example config is in github repo: https://github.com/kuokuo123/otter-launcher"
    );
}

// function to print version
pub fn print_version() {
    println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
}
