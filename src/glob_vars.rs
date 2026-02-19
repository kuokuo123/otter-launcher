// the library for config file format, and setting global variables accordingly

use serde::Deserialize;
use std::{
    env, fs,
    path::Path,
    sync::{Mutex, OnceLock},
};

// function to initialize a mutex as per the config file
pub fn init_statics<T: Clone>(
    cell: &OnceLock<Mutex<T>>,
    config_value: Option<T>,
    default_value: T,
) {
    let value = config_value.unwrap_or(default_value);
    let _ = cell.set(Mutex::new(value));
}

// function to retrieve a cached value with a default
pub fn cached_statics<T: Clone, F: FnOnce() -> T>(cell: &OnceLock<Mutex<T>>, default_fn: F) -> T {
    let m = cell.get_or_init(|| Mutex::new(default_fn()));
    m.lock().unwrap().clone()
}

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
    let home_dir = env::var("HOME").unwrap_or_else(|_| "/".to_string());
    let xdg_config_path = format!("{}/.config/otter-launcher/config.toml", home_dir);

    let config_file = if Path::new(&xdg_config_path).exists() {
        xdg_config_path
    } else {
        "/etc/otter-launcher/config.toml".to_string()
    };

    let configs = fs::read_to_string(config_file)?;

    Ok(toml::from_str(&configs)?)
}

#[inline]
pub fn config() -> &'static Config {
    CONFIG.get_or_init(|| load_config().unwrap())
}

// use oncelock mutex to make important variables globally accessible (repeatedly used config values, list selection, and completion related stuff)
// define config variables as statics
pub static HEADER_CMD: OnceLock<Mutex<String>> = OnceLock::new();
pub static OVERLAY_CMD: OnceLock<Mutex<String>> = OnceLock::new();
pub static SUGGESTION_MODE: OnceLock<Mutex<String>> = OnceLock::new();
pub static LOOP_MODE: OnceLock<Mutex<bool>> = OnceLock::new();
pub static CALLBACK: OnceLock<Mutex<String>> = OnceLock::new();
pub static CHEATSHEET_ENTRY: OnceLock<Mutex<String>> = OnceLock::new();
pub static CHEATSHEET_VIEWER: OnceLock<Mutex<String>> = OnceLock::new();
pub static EXTERNAL_EDITOR: OnceLock<Mutex<String>> = OnceLock::new();
pub static VI_MODE: OnceLock<Mutex<bool>> = OnceLock::new();
pub static ESC_TO_ABORT: OnceLock<Mutex<bool>> = OnceLock::new();
pub static CLEAR_SCREEN_AFTER_EXECUTION: OnceLock<Mutex<bool>> = OnceLock::new();
pub static HEADER_CMD_TRIMMED_LINES: OnceLock<Mutex<usize>> = OnceLock::new();
pub static DELAY_STARTUP: OnceLock<Mutex<usize>> = OnceLock::new();
pub static OVERLAY_TRIMMED_LINES: OnceLock<Mutex<usize>> = OnceLock::new();
pub static OVERLAY_HEIGHT: OnceLock<Mutex<usize>> = OnceLock::new();
pub static HEADER: OnceLock<Mutex<String>> = OnceLock::new();
pub static SEPARATOR: OnceLock<Mutex<String>> = OnceLock::new();
pub static FOOTER: OnceLock<Mutex<String>> = OnceLock::new();
pub static EXEC_CMD: OnceLock<Mutex<String>> = OnceLock::new();
pub static DEFAULT_MODULE: OnceLock<Mutex<String>> = OnceLock::new();
pub static EMPTY_MODULE: OnceLock<Mutex<String>> = OnceLock::new();
pub static EMPTY_MODULE_MESSAGE: OnceLock<Mutex<String>> = OnceLock::new();
pub static DEFAULT_MODULE_MESSAGE: OnceLock<Mutex<String>> = OnceLock::new();
pub static SUGGESTION_LINES: OnceLock<Mutex<usize>> = OnceLock::new();
pub static PREFIX_PADDING: OnceLock<Mutex<usize>> = OnceLock::new();
pub static SELECTION_INDEX: OnceLock<Mutex<usize>> = OnceLock::new();
pub static SELECTION_SPAN: OnceLock<Mutex<usize>> = OnceLock::new();
pub static HINT_SPAN: OnceLock<Mutex<usize>> = OnceLock::new();
pub static HINT_BENCHMARK: OnceLock<Mutex<usize>> = OnceLock::new();
pub static LIST_PREFIX: OnceLock<Mutex<String>> = OnceLock::new();
pub static SELECTION_PREFIX: OnceLock<Mutex<String>> = OnceLock::new();
pub static PREFIX_COLOR: OnceLock<Mutex<String>> = OnceLock::new();
pub static DESCRIPTION_COLOR: OnceLock<Mutex<String>> = OnceLock::new();
pub static PLACE_HOLDER: OnceLock<Mutex<String>> = OnceLock::new();
pub static PLACE_HOLDER_COLOR: OnceLock<Mutex<String>> = OnceLock::new();
pub static HINT_COLOR: OnceLock<Mutex<String>> = OnceLock::new();
pub static INDICATOR_WITH_ARG_MODULE: OnceLock<Mutex<String>> = OnceLock::new();
pub static INDICATOR_NO_ARG_MODULE: OnceLock<Mutex<String>> = OnceLock::new();
pub static FILTERED_HINT_COUNT: OnceLock<Mutex<usize>> = OnceLock::new();
pub static HEADER_LINE_COUNT: OnceLock<Mutex<usize>> = OnceLock::new();
pub static COMPLETION_CANDIDATE: OnceLock<Mutex<String>> = OnceLock::new();
pub static LAYOUT_RIGHTWARD: OnceLock<Mutex<usize>> = OnceLock::new();
pub static LAYOUT_DOWNWARD: OnceLock<Mutex<usize>> = OnceLock::new();
pub static OVERLAY_RIGHTWARD: OnceLock<Mutex<usize>> = OnceLock::new();
pub static OVERLAY_DOWNWARD: OnceLock<Mutex<usize>> = OnceLock::new();
pub static CUSTOMIZED_LIST_ORDER: OnceLock<Mutex<bool>> = OnceLock::new();
pub static OVERLAY_LINES: OnceLock<Mutex<String>> = OnceLock::new();
pub static CELL_HEIGHT: OnceLock<usize> = OnceLock::new();
pub static SEPARATOR_COUNT: OnceLock<Mutex<usize>> = OnceLock::new();
pub static CTRLX_LOCK: OnceLock<Mutex<usize>> = OnceLock::new();

// function to initialize all statics
pub fn init_all_statics() {
    init_statics(
        &EXEC_CMD,
        config().general.exec_cmd.clone(),
        "sh -c".to_string(),
    );
    init_statics(
        &EXTERNAL_EDITOR,
        config().general.external_editor.clone(),
        "".to_string(),
    );
    init_statics(
        &DEFAULT_MODULE,
        config().general.default_module.clone(),
        "".to_string(),
    );
    init_statics(
        &EMPTY_MODULE,
        config().general.empty_module.clone(),
        "".to_string(),
    );
    init_statics(
        &CHEATSHEET_ENTRY,
        config().general.cheatsheet_entry.clone(),
        "?".to_string(),
    );
    init_statics(
        &CHEATSHEET_VIEWER,
        config().general.cheatsheet_viewer.clone(),
        "less -R; clear".to_string(),
    );
    init_statics(&VI_MODE, config().general.vi_mode, false);
    init_statics(&ESC_TO_ABORT, config().general.esc_to_abort, true);
    init_statics(&LOOP_MODE, config().general.loop_mode, false);
    init_statics(
        &CLEAR_SCREEN_AFTER_EXECUTION,
        config().general.clear_screen_after_execution,
        false,
    );
    init_statics(&CALLBACK, config().general.callback.clone(), "".to_string());
    init_statics(&DELAY_STARTUP, config().general.delay_startup, 0);
    init_statics(
        &HEADER_CMD,
        config().interface.header_cmd.clone(),
        "".to_string(),
    );
    init_statics(
        &OVERLAY_CMD,
        config().overlay.overlay_cmd.clone(),
        "".to_string(),
    );
    init_statics(
        &HEADER_CMD_TRIMMED_LINES,
        config().interface.header_cmd_trimmed_lines,
        0,
    );
    init_statics(
        &OVERLAY_TRIMMED_LINES,
        config().overlay.overlay_trimmed_lines,
        0,
    );
    init_statics(&OVERLAY_HEIGHT, config().overlay.overlay_height, 0);
    init_statics(
        &HEADER,
        config().interface.header.clone(),
        "otter-launcher: ".to_string(),
    );
    init_statics(
        &SEPARATOR,
        config().interface.separator.clone(),
        "".to_string(),
    );
    init_statics(&FOOTER, config().interface.footer.clone(), "".to_string());
    init_statics(
        &LIST_PREFIX,
        config().interface.list_prefix.clone(),
        "".to_string(),
    );
    init_statics(
        &SELECTION_PREFIX,
        config().interface.selection_prefix.clone(),
        ">".to_string(),
    );
    init_statics(
        &PLACE_HOLDER,
        config().interface.place_holder.clone(),
        "type something".to_string(),
    );
    init_statics(
        &INDICATOR_WITH_ARG_MODULE,
        config().interface.indicator_with_arg_module.clone(),
        "".to_string(),
    );
    init_statics(
        &INDICATOR_NO_ARG_MODULE,
        config().interface.indicator_no_arg_module.clone(),
        "".to_string(),
    );
    init_statics(
        &SUGGESTION_MODE,
        config().interface.suggestion_mode.clone(),
        "list".to_string(),
    );
    init_statics(&SUGGESTION_LINES, config().interface.suggestion_lines, 1);
    init_statics(
        &DEFAULT_MODULE_MESSAGE,
        config().interface.default_module_message.clone(),
        "".to_string(),
    );
    init_statics(
        &EMPTY_MODULE_MESSAGE,
        config().interface.empty_module_message.clone(),
        "".to_string(),
    );
    init_statics(&PREFIX_PADDING, config().interface.prefix_padding, 0);
    init_statics(
        &PREFIX_COLOR,
        config().interface.prefix_color.clone(),
        "".to_string(),
    );
    init_statics(
        &DESCRIPTION_COLOR,
        config().interface.description_color.clone(),
        "\x1b[39m".to_string(),
    );
    init_statics(
        &PLACE_HOLDER_COLOR,
        config().interface.place_holder_color.clone(),
        "\x1b[30m".to_string(),
    );
    init_statics(
        &HINT_COLOR,
        config().interface.hint_color.clone(),
        "\x1b[30m".to_string(),
    );
    init_statics(
        &LAYOUT_RIGHTWARD,
        config().interface.move_interface_right,
        0,
    );
    init_statics(&LAYOUT_DOWNWARD, config().interface.move_interface_down, 0);
    init_statics(&OVERLAY_RIGHTWARD, config().overlay.move_overlay_right, 0);
    init_statics(&OVERLAY_DOWNWARD, config().overlay.move_overlay_down, 0);
    init_statics(
        &CUSTOMIZED_LIST_ORDER,
        config().interface.customized_list_order,
        false,
    );
}
