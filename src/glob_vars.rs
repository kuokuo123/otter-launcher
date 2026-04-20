// the library for config file format, and setting global variables accordingly

use crate::mod_exec::{print_help, print_version};
use serde::Deserialize;
use std::{
    env, fs,
    path::Path,
    sync::{
        LazyLock, OnceLock, RwLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

// Define config structure
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub interface: Interface,
    pub overlay: Overlay,
    pub modules: Vec<Module>,
}

#[derive(Deserialize, Default)]
pub struct General {
    pub default_module: Option<String>,
    pub empty_module: Option<String>,
    pub exec_cmd: Option<String>,
    pub esc_to_abort: Option<bool>,
    pub cheatsheet_entry: Option<String>,
    pub cheatsheet_viewer: Option<String>,
    pub vi_mode: Option<bool>,
    pub clear_screen_after_execution: Option<bool>,
    pub loop_mode: Option<bool>,
    pub callback: Option<String>,
    pub external_editor: Option<String>,
    pub delay_startup: Option<usize>,
}

#[derive(Deserialize, Default)]
pub struct Interface {
    pub header: Option<String>,
    pub header_cmd: Option<String>,
    pub header_cmd_trimmed_lines: Option<usize>,
    pub separator: Option<String>,
    pub footer: Option<String>,
    pub list_prefix: Option<String>,
    pub selection_prefix: Option<String>,
    pub place_holder: Option<String>,
    pub default_module_message: Option<String>,
    pub empty_module_message: Option<String>,
    pub suggestion_mode: Option<String>,
    pub suggestion_lines: Option<usize>,
    pub indicator_no_arg_module: Option<String>,
    pub indicator_with_arg_module: Option<String>,
    pub prefix_padding: Option<usize>,
    pub prefix_color: Option<String>,
    pub description_color: Option<String>,
    pub place_holder_color: Option<String>,
    pub hint_color: Option<String>,
    pub move_interface_right: Option<usize>,
    pub move_interface_down: Option<usize>,
    pub customized_list_order: Option<bool>,
}

#[derive(Deserialize, Default)]
pub struct Overlay {
    pub overlay_cmd: Option<String>,
    pub overlay_trimmed_lines: Option<usize>,
    pub overlay_height: Option<usize>,
    pub move_overlay_right: Option<usize>,
    pub move_overlay_down: Option<usize>,
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

// load toml config
static CONFIG: OnceLock<Config> = OnceLock::new();

fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    let user_config_path = USER_CONFIG_PATH.get_or_init(|| String::new());
    let home_dir = env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let xdg_config_path = format!("{}/.config/otter-launcher/config.toml", home_dir);

    let config_file = if !user_config_path.is_empty() {
        user_config_path.to_string()
    } else if Path::new(&xdg_config_path).exists() {
        xdg_config_path
    } else {
        "/etc/otter-launcher/config.toml".to_string()
    };

    // if config_file not exist
    let configs = match fs::read_to_string(&config_file) {
        Ok(file_content) => file_content,
        Err(e) => {
            eprintln!(
                "Could not read the configuration file at '{}'.",
                config_file
            );
            eprintln!("OS error: {}", e);
            std::process::exit(1);
        }
    };

    // if config_file cannot be parsed
    let parsed_config = match toml::from_str(&configs) {
        Ok(config_data) => config_data,
        Err(e) => {
            eprintln!(
                "The configuration file at '{}' is not correctly formatted.",
                config_file
            );
            eprintln!("TOML parser error: {}", e);
            std::process::exit(1);
        }
    };

    Ok(parsed_config)
}

#[inline]
pub fn config() -> &'static Config {
    CONFIG.get_or_init(|| load_config().unwrap())
}

// use oncelock and atomics to make important variables globally accessible (repeatedly used config values, list selection, and completion related stuff)
// define config variables as statics
pub static HEADER_CMD: OnceLock<String> = OnceLock::new();
pub static OVERLAY_CMD: OnceLock<String> = OnceLock::new();
pub static SUGGESTION_MODE: OnceLock<String> = OnceLock::new();
pub static LOOP_MODE: AtomicBool = AtomicBool::new(false);
pub static CALLBACK: OnceLock<String> = OnceLock::new();
pub static CHEATSHEET_ENTRY: OnceLock<String> = OnceLock::new();
pub static CHEATSHEET_VIEWER: OnceLock<String> = OnceLock::new();
pub static EXTERNAL_EDITOR: OnceLock<String> = OnceLock::new();
pub static VI_MODE: AtomicBool = AtomicBool::new(false);
pub static ESC_TO_ABORT: AtomicBool = AtomicBool::new(false);
pub static CLEAR_SCREEN_AFTER_EXECUTION: AtomicBool = AtomicBool::new(false);
pub static HEADER_CMD_TRIMMED_LINES: AtomicUsize = AtomicUsize::new(0);
pub static DELAY_STARTUP: AtomicUsize = AtomicUsize::new(0);
pub static OVERLAY_TRIMMED_LINES: AtomicUsize = AtomicUsize::new(0);
pub static OVERLAY_HEIGHT: AtomicUsize = AtomicUsize::new(0);
pub static HEADER: OnceLock<String> = OnceLock::new();
pub static SEPARATOR: OnceLock<String> = OnceLock::new();
pub static FOOTER: OnceLock<String> = OnceLock::new();
pub static EXEC_CMD: OnceLock<String> = OnceLock::new();
pub static DEFAULT_MODULE: OnceLock<String> = OnceLock::new();
pub static EMPTY_MODULE: OnceLock<String> = OnceLock::new();
pub static EMPTY_MODULE_MESSAGE: OnceLock<String> = OnceLock::new();
pub static DEFAULT_MODULE_MESSAGE: OnceLock<String> = OnceLock::new();
pub static SUGGESTION_LINES: AtomicUsize = AtomicUsize::new(0);
pub static PREFIX_PADDING: AtomicUsize = AtomicUsize::new(0);
pub static SELECTION_INDEX: AtomicUsize = AtomicUsize::new(0);
pub static SELECTION_SPAN: AtomicUsize = AtomicUsize::new(0);
pub static HINT_SPAN: AtomicUsize = AtomicUsize::new(0);
pub static HINT_BENCHMARK: AtomicUsize = AtomicUsize::new(0);
pub static LIST_PREFIX: OnceLock<String> = OnceLock::new();
pub static SELECTION_PREFIX: OnceLock<String> = OnceLock::new();
pub static PREFIX_COLOR: OnceLock<String> = OnceLock::new();
pub static DESCRIPTION_COLOR: OnceLock<String> = OnceLock::new();
pub static PLACE_HOLDER: OnceLock<String> = OnceLock::new();
pub static PLACE_HOLDER_COLOR: OnceLock<String> = OnceLock::new();
pub static HINT_COLOR: OnceLock<String> = OnceLock::new();
pub static INDICATOR_WITH_ARG_MODULE: OnceLock<String> = OnceLock::new();
pub static INDICATOR_NO_ARG_MODULE: OnceLock<String> = OnceLock::new();
pub static FILTERED_HINT_COUNT: AtomicUsize = AtomicUsize::new(0);
pub static HEADER_LINE_COUNT: AtomicUsize = AtomicUsize::new(0);
pub static COMPLETION_CANDIDATE: LazyLock<RwLock<String>> =
    LazyLock::new(|| RwLock::new(String::new()));
pub static LAYOUT_RIGHTWARD: AtomicUsize = AtomicUsize::new(0);
pub static LAYOUT_DOWNWARD: AtomicUsize = AtomicUsize::new(0);
pub static OVERLAY_RIGHTWARD: AtomicUsize = AtomicUsize::new(0);
pub static OVERLAY_DOWNWARD: AtomicUsize = AtomicUsize::new(0);
pub static CUSTOMIZED_LIST_ORDER: AtomicBool = AtomicBool::new(false);
pub static CELL_HEIGHT: AtomicUsize = AtomicUsize::new(0);
pub static SEPARATOR_COUNT: AtomicUsize = AtomicUsize::new(0);
pub static CTRLX_LOCK: AtomicUsize = AtomicUsize::new(0);
pub static OVERLAY_LINES_CACHE: OnceLock<String> = OnceLock::new();
pub static USER_CONFIG_PATH: OnceLock<String> = OnceLock::new();
pub static CLI_PROMPT: OnceLock<String> = OnceLock::new();

// macro to initialize onelock as per the config file
macro_rules! init_lock {
    // for custom default string
    ($lock:expr, $field:expr, $default:expr) => {
        $lock.get_or_init(|| $field.clone().unwrap_or_else(|| $default.to_string()));
    };
    // for empty string default
    ($lock:expr, $field:expr) => {
        $lock.get_or_init(|| $field.clone().unwrap_or_default());
    };
}

// function to initialize all statics
pub fn init_all_statics() {
    // if launched with arguments, act accordingly
    let mut args = env::args().skip(1);
    if let Some(arg) = args.next() {
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
                let path = args.next().unwrap_or_else(|| String::new());
                USER_CONFIG_PATH.get_or_init(|| path);

                let remaining_args: Vec<_> = args.collect();
                if !remaining_args.is_empty() {
                    CLI_PROMPT.get_or_init(|| remaining_args.join(" "));
                }
            }
            _ => {
                let mut full_args = vec![arg];
                full_args.extend(args);

                CLI_PROMPT.get_or_init(|| full_args.join(" "));
            }
        }
    };

    // initialize global vars
    init_lock!(EXEC_CMD, config().general.exec_cmd, "sh -c");
    init_lock!(EXTERNAL_EDITOR, config().general.external_editor);
    init_lock!(DEFAULT_MODULE, config().general.default_module);
    init_lock!(EMPTY_MODULE, config().general.empty_module);
    init_lock!(CHEATSHEET_ENTRY, config().general.cheatsheet_entry);
    init_lock!(
        CHEATSHEET_VIEWER,
        config().general.cheatsheet_viewer,
        "less -R; clear"
    );
    init_lock!(HEADER, config().interface.header, "otter-launcher: ");
    init_lock!(SEPARATOR, config().interface.separator);
    init_lock!(FOOTER, config().interface.footer);
    init_lock!(LIST_PREFIX, config().interface.list_prefix, " ");
    init_lock!(SELECTION_PREFIX, config().interface.selection_prefix, ">");
    init_lock!(
        PLACE_HOLDER,
        config().interface.place_holder,
        "type & search"
    );
    init_lock!(
        INDICATOR_WITH_ARG_MODULE,
        config().interface.indicator_with_arg_module
    );
    init_lock!(
        INDICATOR_NO_ARG_MODULE,
        config().interface.indicator_no_arg_module
    );
    init_lock!(SUGGESTION_MODE, config().interface.suggestion_mode, "list");
    init_lock!(
        DEFAULT_MODULE_MESSAGE,
        config().interface.default_module_message,
        "list"
    );
    init_lock!(
        EMPTY_MODULE_MESSAGE,
        config().interface.empty_module_message
    );
    init_lock!(PREFIX_COLOR, config().interface.prefix_color);
    init_lock!(
        DESCRIPTION_COLOR,
        config().interface.description_color,
        "\x1b[39m"
    );
    init_lock!(
        PLACE_HOLDER_COLOR,
        config().interface.place_holder_color,
        "\x1b[30m"
    );
    init_lock!(HINT_COLOR, config().interface.hint_color, "\x1b[30m");
    VI_MODE.store(config().general.vi_mode.unwrap_or(false), Ordering::Relaxed);
    ESC_TO_ABORT.store(
        config().general.esc_to_abort.unwrap_or(true),
        Ordering::Relaxed,
    );
    LOOP_MODE.store(
        config().general.loop_mode.unwrap_or(false),
        Ordering::Relaxed,
    );
    CLEAR_SCREEN_AFTER_EXECUTION.store(
        config()
            .general
            .clear_screen_after_execution
            .unwrap_or(false),
        Ordering::Relaxed,
    );
    DELAY_STARTUP.store(
        config().general.delay_startup.unwrap_or(0),
        Ordering::Relaxed,
    );
    HEADER_CMD_TRIMMED_LINES.store(
        config().interface.header_cmd_trimmed_lines.unwrap_or(0),
        Ordering::Relaxed,
    );
    OVERLAY_TRIMMED_LINES.store(
        config().overlay.overlay_trimmed_lines.unwrap_or(0),
        Ordering::Relaxed,
    );
    OVERLAY_HEIGHT.store(
        config().overlay.overlay_height.unwrap_or(0),
        Ordering::Relaxed,
    );
    SUGGESTION_LINES.store(
        config().interface.suggestion_lines.unwrap_or(4),
        Ordering::Relaxed,
    );
    PREFIX_PADDING.store(
        config().interface.prefix_padding.unwrap_or(0),
        Ordering::Relaxed,
    );
    LAYOUT_RIGHTWARD.store(
        config().interface.move_interface_right.unwrap_or(0),
        Ordering::Relaxed,
    );
    LAYOUT_DOWNWARD.store(
        config().interface.move_interface_down.unwrap_or(0),
        Ordering::Relaxed,
    );
    OVERLAY_RIGHTWARD.store(
        config().overlay.move_overlay_right.unwrap_or(0),
        Ordering::Relaxed,
    );
    OVERLAY_DOWNWARD.store(
        config().overlay.move_overlay_down.unwrap_or(0),
        Ordering::Relaxed,
    );
    CUSTOMIZED_LIST_ORDER.store(
        config().interface.customized_list_order.unwrap_or(false),
        Ordering::Relaxed,
    );
}
