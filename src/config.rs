use serde::Deserialize;
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::OnceLock,
};

// Define config structure
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub interface: Interface,
    pub image: Image,
    pub modules: Vec<Module>,
}

#[derive(Deserialize, Default)]
pub struct General {
    pub height: Option<u16>,
    pub default_module: Option<String>,
    pub alternative_module: Option<String>,
    pub empty_module: Option<String>,
    pub exec_cmd: Option<String>,
    pub vi_mode: Option<bool>,
    pub cheatsheet_entry: Option<String>,
    pub cheatsheet_viewer: Option<String>,
    pub external_editor: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct Interface {
    pub padding: Option<[u16; 4]>,
    pub header: Option<String>,
    pub header_color: Option<String>,
    pub prompt_sign_color: Option<String>,
    pub prompt_sign: Option<String>,
    pub placeholder: Option<String>,
    pub separator: Option<bool>,
    pub separator_label: Option<String>,
    pub prefix_padding: Option<usize>,
    pub prefix_color: Option<String>,
    pub description_color: Option<String>,
    pub list_prefix: Option<String>,
    pub list_prefix_color: Option<String>,
    pub selection_prefix: Option<String>,
    pub selection_prefix_color: Option<String>,
    pub inverse_selection: Option<bool>,
    pub suggestion_mode: Option<String>,
    pub suggestion_position: Option<String>,
    pub indicator_bare: Option<String>,
    pub indicator_args: Option<String>,
    pub indicator_color: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct Image {
    pub format: Option<String>,
    pub path: Option<String>,
    pub width: Option<u16>,
    pub position: Option<String>,
    pub padding: Option<[u16; 4]>,
}

#[derive(Deserialize, Clone)]
pub struct Module {
    pub description: String,
    pub prefix: String,
    pub cmd: String,
    pub with_argument: Option<bool>,
    pub url_encode: Option<bool>,
    pub unbind_proc: Option<bool>,
}

// Config related functions
static CONFIG: OnceLock<Config> = OnceLock::new();

impl Config {
    fn apply_defaults(&mut self) {
        // General defaults — adjust these to your preferences.
        self.general.height.get_or_insert_with(|| 7);
        self.general.exec_cmd.get_or_insert("sh -c".to_string());
        self.general.vi_mode.get_or_insert_with(|| false);
        self.general
            .external_editor
            .get_or_insert_with(|| "".to_owned());
        self.general
            .cheatsheet_entry
            .get_or_insert_with(|| "?".to_owned());
        self.general
            .cheatsheet_viewer
            .get_or_insert_with(|| "less -R".to_owned());

        // Interface defaults.
        self.interface.padding.get_or_insert_with(|| [0, 0, 0, 0,]);
        self.interface.header.get_or_insert_with(|| "".to_owned());
        self.interface
            .header_color
            .get_or_insert_with(|| "blue".to_owned());
        self.interface.prompt_sign.get_or_insert_with(|| "> ".to_owned());
        self.interface
            .prompt_sign_color
            .get_or_insert_with(|| "blue".to_owned());
        self.interface.placeholder.get_or_insert_with(|| "type and search".to_owned());
        self.interface.inverse_selection.get_or_insert_with(|| true);
        self.interface.separator.get_or_insert_with(|| true);
        self.interface
            .separator_label
            .get_or_insert_with(|| "".to_owned());
        self.interface
            .suggestion_mode
            .get_or_insert_with(|| "list".to_owned());
        self.interface
            .suggestion_position
            .get_or_insert_with(|| "top".to_owned());
        self.interface
            .list_prefix
            .get_or_insert_with(|| "  ".to_owned());
        self.interface
            .selection_prefix
            .get_or_insert_with(|| "▌ ".to_owned());
        self.interface.prefix_padding.get_or_insert_with(|| 3);
        self.interface
            .indicator_args
            .get_or_insert_with(|| "".to_owned());
        self.interface
            .indicator_bare
            .get_or_insert_with(|| "".to_owned());
        self.interface
            .prefix_color
            .get_or_insert_with(|| "yellow".to_owned());
        self.interface
            .description_color
            .get_or_insert_with(|| "reset".to_owned());
        self.interface
            .indicator_color
            .get_or_insert_with(|| "red".to_owned());
        self.interface
            .list_prefix_color
            .get_or_insert_with(|| "reset".to_owned());
        self.interface
            .selection_prefix_color
            .get_or_insert_with(|| "red".to_owned());

        // Image defaults.
        self.image.format.get_or_insert_with(|| "symbol".to_owned());
        self.image.width.get_or_insert_with(|| 0);
        self.image.position.get_or_insert_with(|| "left".to_owned());
        self.image.padding.get_or_insert_with(|| [0, 0, 0, 0,]);

        // Module defaults.
        for module in &mut self.modules {
            module.with_argument.get_or_insert(false);
            module.url_encode.get_or_insert(false);
            module.unbind_proc.get_or_insert(false);
        }
    }
}

fn load_config(override_path: Option<&Path>) -> Result<Config, Box<dyn std::error::Error>> {
    let config_file = match override_path {
        Some(path) => path.to_path_buf(),

        None => {
            let home_dir = env::var("HOME").unwrap_or_else(|_| "/".to_owned());

            let user_path = PathBuf::from(home_dir).join(".config/otter-launcher/config.toml");

            if user_path.exists() {
                user_path
            } else {
                PathBuf::from("/etc/otter-launcher/config.toml")
            }
        }
    };

    let text = fs::read_to_string(&config_file).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "Could not read configuration '{}': {error}",
                config_file.display()
            ),
        )
    })?;

    let mut parsed: Config = toml::from_str(&text).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "Invalid TOML in configuration '{}': {error}",
                config_file.display()
            ),
        )
    })?;

    parsed.apply_defaults();

    Ok(parsed)
}

pub fn init_config(override_path: Option<&Path>) -> Result<(), Box<dyn std::error::Error>> {
    if CONFIG.get().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Configuration has already been initialized",
        )
        .into());
    }

    let loaded = load_config(override_path)?;

    CONFIG.set(loaded).map_err(|_| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Configuration has already been initialized",
        )
    })?;

    Ok(())
}

pub fn config() -> &'static Config {
    CONFIG
        .get()
        .expect("Call init_config() before accessing configuration")
}

// CLI functions
#[derive(Default)]
pub struct CliArguments {
    pub config_path: Option<PathBuf>,
    pub cli_arguments: Option<String>,
}

pub fn parse_arguments() -> io::Result<CliArguments> {
    let mut args = env::args().skip(1);
    let mut cli = CliArguments::default();
    let mut prompt_parts = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }

            "-v" | "--version" => {
                print_version();
                std::process::exit(0);
            }

            "-c" | "--config" => {
                let path = args.next().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "--config requires a file path")
                })?;

                if path.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "--config path cannot be empty",
                    ));
                }

                cli.config_path = Some(PathBuf::from(path));
            }

            // Treat everything after "--" as prompt text.
            "--" => {
                prompt_parts.extend(args);
                break;
            }

            // Once prompt text starts, preserve all remaining arguments.
            _ => {
                prompt_parts.push(arg);
                prompt_parts.extend(args);
                break;
            }
        }
    }

    if !prompt_parts.is_empty() {
        cli.cli_arguments = Some(prompt_parts.join(" "));
    }

    Ok(cli)
}

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

Modules are specified in a TOML config file, expected at one of the below positions by priority:

1. PATH specified by the --config flag
2. $HOME/.config/otter-launcher/config.toml
3. /etc/otter-launcher/config.toml

The example config is in github repo: https://github.com/kuokuo123/otter-launcher"
    );
}

pub fn print_version() {
    println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
}
