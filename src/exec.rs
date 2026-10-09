use crate::config::{Module, config};
use ratatui::{
    backend::IntoCrossterm, crossterm::style::SetForegroundColor, style::Color, text::Line,
};
use regex::Regex;
use std::{
    borrow::Cow,
    error::Error,
    io::{self, BufWriter, Write},
    process::{Command, Stdio},
    sync::OnceLock,
};
use terminal_size::{Width, terminal_size};
use urlencoding::encode;

pub fn remove_ascii(text: &str) -> String {
    if !text.contains('\x1b') {
        return text.to_owned();
    }

    static ANSI_REGEX: OnceLock<Regex> = OnceLock::new();
    ANSI_REGEX
        .get_or_init(|| Regex::new(r"\x1b\[[0-9;]*[A-Za-z]").expect("valid ANSI escape pattern"))
        .replace_all(text, "")
        .into_owned()
}

// Function to tell what to do with the prompt
pub fn dispatch_prompt(prompt: String) -> bool {
    let cfg = config();
    // Match the first input word with module prefixes
    let first_word = prompt.split_whitespace().next().unwrap_or("");
    let matched = cfg.modules.iter().find_map(|module| {
        let prefix = remove_ascii(&module.prefix);
        if prefix == first_word {
            Some((module, prefix))
        } else {
            None
        }
    });

    match matched {
        // Condition 1: when the selected module runs with arguement
        Some((module, _)) if module.with_argument.unwrap_or(false) => {
            let argument = prompt.trim_start_matches(first_word).trim_start();
            execute_module(module, Some(argument));
        }
        // Condition 2: when user input is exactly the same as the no-arg module
        Some((module, prefix)) if prefix == prompt.trim_end() => {
            execute_module(module, None);
        }
        // Condition 3: when user input is empty, run the empty module
        None if prompt.is_empty() => {
            if let Some(prefix) = cfg.general.empty_module.as_deref() {
                run_designated_module(prompt, prefix);
            }
        }
        // Condition 4: cheatsheet keyword
        None if Some(prompt.trim_end()) == cfg.general.cheatsheet_entry.as_deref() => {
            let _ = cheat_sheet();
            return true;
        }
        // Condition 3: when no module is matched, run the default module
        _ => {
            if let Some(prefix) = cfg.general.default_module.as_deref() {
                run_designated_module(prompt, prefix);
            }
        }
    }

    false
}

// Function to find module by a given prefix, used for default and empty module
pub fn run_designated_module(prompt: String, prefix: &str) {
    if prefix.is_empty() {
        println!("{prompt}");
        return;
    }

    if let Some(module) = config()
        .modules
        .iter()
        .find(|module| remove_ascii(&module.prefix) == prefix)
    {
        // Designated modules receive the complete prompt, including its first word.
        execute_module(module, Some(&prompt));
    } else {
        if let Err(error) = shell_command(
            config().general.exec_cmd.as_deref(),
            "printf 'no default_module or empty_module found\n'",
            false,
        )
        .and_then(|mut built_command| built_command.status())
        {
            eprintln!("Failed to execute fallback: {error}");
        }
    }
}

// Function to execute a module's cmd
fn execute_module(module: &Module, argument: Option<&str>) {
    let command = match argument {
        Some(argument) => {
            let argument: Cow<'_, str> = if module.url_encode.unwrap_or(false) {
                encode(argument)
            } else {
                Cow::Borrowed(argument)
            };
            module.cmd.replace("{}", argument.as_ref())
        }
        None => module.cmd.to_owned(),
    };

    if let Err(error) = shell_command(
        config().general.exec_cmd.as_deref(),
        &command,
        module.unbind_proc.unwrap_or(false),
    )
    .and_then(|mut built_command| built_command.status())
    {
        eprintln!("Failed to execute module: {error}");
    }
}

// Function to build a process command
fn shell_command(exec_cmd: Option<&str>, script: &str, detached: bool) -> io::Result<Command> {
    let mut parts = exec_cmd.unwrap_or("sh -c").split_whitespace();
    let program = parts.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "general.exec_cmd must contain a program",
        )
    })?;

    let mut command = if detached {
        let mut command = Command::new("setsid");
        command.args(["-f", program]);
        command
    } else {
        Command::new(program)
    };
    command.args(parts).arg(script);
    Ok(command)
}

// Function to form and pipe cheat sheet to external pager
pub fn cheat_sheet() -> Result<(), Box<dyn Error>> {
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

    let mut child = shell_command(cfg.general.exec_cmd.as_deref(), cheatsheet_viewer, false)?
        .stdin(Stdio::piped())
        .spawn()?;

    let write_result = (|| -> io::Result<()> {
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("viewer stdin unavailable"))?;
        let mut output = BufWriter::new(stdin);

        // Strip ANSI codes and measure terminal columns once per module.
        let prefix_widths: Vec<usize> = cfg
            .modules
            .iter()
            .map(|module| Line::raw(remove_ascii(&module.prefix)).width())
            .collect();
        let width = prefix_widths.iter().copied().max().unwrap_or(0);

        write!(
            output,
            "\n  {}\x1b[1mCheatsheet\n\n\x1b[0m",
            SetForegroundColor(header_color.into_crossterm()),
        )?;

        for (index, (module, prefix_width)) in cfg.modules.iter().zip(prefix_widths).enumerate() {
            if index > 0 {
                writeln!(output)?;
            }
            let indicator = if module.with_argument.unwrap_or(false) {
                indicator_with_arg
            } else {
                indicator_no_arg
            };

            write!(
                output,
                "    {}{}{}\x1b[0m {}{}{}{}\x1b[0m",
                SetForegroundColor(prefix_color.into_crossterm()),
                module.prefix,
                " ".repeat(width.saturating_sub(prefix_width)),
                SetForegroundColor(indicator_color.into_crossterm()),
                indicator,
                SetForegroundColor(description_color.into_crossterm()),
                module.description,
            )?;
        }

        // The owned stdin closes when this closure returns, before waiting for the viewer.
        output.flush()
    })();

    if let Err(error) = write_result {
        eprintln!("Error writing to stdin of child process: {error}");
    }
    child.wait()?;
    Ok(())
}

pub fn expand_header(template: &str) -> Result<String, Box<dyn Error>> {
    if !template.chars().any(|ch| matches!(ch, '$' | '`' | '\\')) {
        return Ok(template.to_owned());
    }

    let mut delimiter = "_**_HDREND_#$_".to_owned();
    while template.lines().any(|line| line == delimiter) {
        delimiter.push('_');
    }
    let script = format!("cat <<{delimiter}\n{template}\n{delimiter}");

    let cfg = config();
    let mut shell_cmd = shell_command(cfg.general.exec_cmd.as_deref(), &script, false)?;
    let interface_padding = cfg.interface.padding.unwrap();
    let image_padding = cfg.image.padding.unwrap();
    let columns = terminal_size()
        .map(|(Width(width), _)| width)
        .unwrap_or(80)
        .saturating_sub(interface_padding[1])
        .saturating_sub(interface_padding[3])
        .saturating_sub(image_padding[1])
        .saturating_sub(image_padding[3]);
    let hostname = hostname::get()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    let output = shell_cmd
        .env("COLUMNS", columns.to_string())
        .env("HOSTNAME", &hostname)
        .env("HOST", &hostname)
        .output()?;

    if !output.status.success() {
        return Err(io::Error::other(format!(
            "interface.header expansion failed: {}",
            String::from_utf8_lossy(&output.stderr),
        ))
        .into());
    }

    Ok(String::from_utf8(output.stdout)?)
}

#[cfg(test)]
mod tests {
    use super::{Line, remove_ascii, shell_command};
    use std::{ffi::OsStr, io};

    #[test]
    fn configured_shell_preserves_the_script_as_one_argument() {
        let script = "printf '%s' 'a b'";
        let command = shell_command(Some("bash --noprofile -c"), script, false).unwrap();
        assert_eq!(command.get_program(), OsStr::new("bash"));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![OsStr::new("--noprofile"), OsStr::new("-c"), OsStr::new(script)],
        );
    }

    #[test]
    fn detached_shell_wraps_the_same_program_and_arguments() {
        let command = shell_command(None, "echo test", true).unwrap();
        assert_eq!(command.get_program(), OsStr::new("setsid"));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![OsStr::new("-f"), OsStr::new("sh"), OsStr::new("-c"), OsStr::new("echo test")],
        );
    }

    #[test]
    fn empty_shell_configuration_returns_an_error() {
        let error = shell_command(Some(" \t\n"), "echo test", false).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn colored_unicode_prefixes_have_their_visible_width() {
        let prefix = "\x1b[31m検索\x1b[0m";
        assert_eq!(remove_ascii(prefix), "検索");
        assert_eq!(Line::raw(remove_ascii(prefix)).width(), 4);
    }
}
