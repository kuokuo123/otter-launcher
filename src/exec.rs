use crate::config::*;
use ratatui::{backend::IntoCrossterm, crossterm::style::SetForegroundColor, style::Color};
use std::{
    error::Error,
    io::Write,
    process::{Command, Stdio},
};
use terminal_size::{Width, terminal_size};
use urlencoding::encode;

// Function to remove ascii color code from &str
pub fn remove_ascii(text: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;]*[A-Za-z]").unwrap();
    re.replace_all(text, "").to_string()
}

// Function to run empty & default modules
pub fn run_designated_module(prompt: String, prfx: &str) {
    // Test if the designated module is set
    if prfx.is_empty() {
        println!("{}", prompt)
    } else {
        // Set a fallback module to prevent panic when no module is found
        let fallback = Module {
            description: String::new(),
            prefix: String::new(),
            cmd: "printf 'no default_module or empty_module found\n'".to_string(),
            with_argument: None,
            url_encode: None,
            unbind_proc: None,
        };

        // Find the designated module
        let target_module = config()
            .modules
            .iter()
            .find(|module| remove_ascii(&module.prefix) == prfx)
            .unwrap_or(&fallback);
        // Whether to use url encoding
        let prompt_wo_prefix = if target_module.url_encode.unwrap_or(false) {
            encode(&prompt).to_string()
        } else {
            prompt
        };

        // Run the module's command
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

// Function to run module.cmd
pub fn run_module_command(mod_cmd_arg: String) -> Result<(), Box<dyn std::error::Error>> {
    // format the shell command by which the module commands are launched
    let mut exec_cmd = "sh -c".to_string();
    if let Some(module_name) = config().general.exec_cmd.as_ref() {
        exec_cmd = module_name.to_string()
    };
    let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
    let mut shell_cmd = Command::new(cmd_parts[0]);
    for arg in &cmd_parts[1..] {
        shell_cmd.arg(arg);
    }
    // Run module cmd
    shell_cmd.arg(mod_cmd_arg);
    shell_cmd.spawn()?.wait()?;
    Ok(())
}

pub fn run_module_command_unbind_proc(
    mod_cmd_arg: String,
) -> Result<(), Box<dyn std::error::Error>> {
    // Format the shell command by which the module commands are launched
    let mut shell_cmd = Command::new("setsid");
    shell_cmd.arg("-f");

    let mut exec_cmd = "sh -c".to_string();
    if let Some(module_name) = config().general.exec_cmd.as_ref() {
        exec_cmd = module_name.to_string()
    };
    let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
    for arg in &cmd_parts[0..] {
        shell_cmd.arg(arg);
    }

    // Run module cmd
    shell_cmd.arg(mod_cmd_arg).spawn()?.wait()?;
    Ok(())
}

// Function to format and show cheat sheet
pub fn cheat_sheet() -> Result<(), Box<dyn std::error::Error>> {
    // Setup variables
    let cfg = config();
    let header_color = cfg
        .interface
        .header_color
        .as_deref()
        .and_then(|value| value.parse::<Color>().ok())
        .unwrap_or(Color::Blue);
    let prefix_color = cfg
        .interface
        .prefix_color
        .as_deref()
        .and_then(|value| value.parse::<Color>().ok())
        .unwrap_or(Color::Yellow);
    let description_color = cfg
        .interface
        .description_color
        .as_deref()
        .and_then(|value| value.parse::<Color>().ok())
        .unwrap_or(Color::Reset);
    let indicator_color = cfg
        .interface
        .indicator_color
        .as_deref()
        .and_then(|value| value.parse::<Color>().ok())
        .unwrap_or(Color::Red);
    let indicator_with_arg = cfg.interface.indicator_args.as_deref().unwrap();
    let indicator_no_arg = cfg.interface.indicator_bare.as_deref().unwrap();
    let cheatsheet_viewer = cfg.general.cheatsheet_viewer.as_deref().unwrap();

    // Run general.cheatsheet.viewer
    let mut exec_cmd = "sh -c".to_string();
    if let Some(module_name) = config().general.exec_cmd.as_ref() {
        exec_cmd = module_name.to_string()
    };
    let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
    let mut shell_cmd = Command::new(cmd_parts[0]);
    for arg in &cmd_parts[1..] {
        shell_cmd.arg(arg);
    }
    let mut child = shell_cmd
        .arg(cheatsheet_viewer)
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
                        indicator_with_arg
                    } else {
                        indicator_no_arg
                    };
                    let width = config()
                        .modules
                        .iter()
                        .map(|line| remove_ascii(&line.prefix).len())
                        .max()
                        .unwrap_or(0);
                    format!(
                        "    {}{:width$}{} {}{}{}{}{}",
                        SetForegroundColor(prefix_color.into_crossterm()),
                        &module.prefix,
                        "\x1b[0m",
                        SetForegroundColor(indicator_color.into_crossterm()),
                        arg_indicator,
                        SetForegroundColor(description_color.into_crossterm()),
                        &module.description,
                        "\x1b[0m"
                    )
                })
                .collect::<Vec<String>>()
                .join("\n");
            match stdin.write_all(
                format!(
                    "\n  {}{}{}",
                    SetForegroundColor(header_color.into_crossterm()),
                    "\x1b[1mCheatsheet\n\n\x1b[0m",
                    mapped_modules
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
pub fn expand_header(template: &str) -> Result<String, Box<dyn Error>> {
    // Plain text needs no shell expansion.
    let needs_shell = template.chars().any(|ch| matches!(ch, '$' | '`' | '\\'));
    if !needs_shell {
        return Ok(template.to_owned());
    }

    // Prepare header as shell command
    let mut delimiter = "_**_HDREND_#$_".to_owned();
    while template.lines().any(|line| line == delimiter) {
        delimiter.push('_');
    }
    let script = format!("cat <<{delimiter}\n{template}\n{delimiter}");

    // Run header through shell
    let mut exec_cmd = "sh -c".to_string();
    if let Some(module_name) = config().general.exec_cmd.as_ref() {
        exec_cmd = module_name.to_string()
    };
    let cmd_parts: Vec<&str> = exec_cmd.split_whitespace().collect();
    let mut shell_cmd = Command::new(cmd_parts[0]);
    for arg in &cmd_parts[1..] {
        shell_cmd.arg(arg);
    }

    // Calculate $COLUMNS
    let cfg = config();
    let iface_pad_right = cfg.interface.padding.unwrap()[1];
    let iface_pad_left = cfg.interface.padding.unwrap()[3];
    let image_pad_right = cfg.image.padding.unwrap()[1];
    let image_pad_left = cfg.image.padding.unwrap()[3];
    let columns = terminal_size()
        .map(|(Width(w), _)| w)
        .unwrap_or(80)
        .saturating_sub(iface_pad_right)
        .saturating_sub(iface_pad_left)
        .saturating_sub(image_pad_right)
        .saturating_sub(image_pad_left);

    shell_cmd.env("COLUMNS", columns.to_string());
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
    let output = shell_cmd.arg(script).output()?;

    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "interface.header expansion failed: {}",
            String::from_utf8_lossy(&output.stderr),
        ))
        .into());
    }

    let header = String::from_utf8(output.stdout)?;

    Ok(header)
}
