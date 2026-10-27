mod config;
mod exec;
mod tui;
mod list;
mod vim;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check arguments
    let cli = config::parse_arguments()?;
    let cli_arguments = cli.cli_arguments;

    // Check config file
    config::init_config(cli.config_path.as_deref())?;
    let cfg = config::config();

    // Expand header
    let expanded_header =
        match exec::expand_header(cfg.interface.header.as_deref().unwrap_or_default()) {
            Ok(expanded_header) => expanded_header,
            Err(err) => {
                // If failed, print error before initializing the TUI.
                eprintln!("Header expansion failed; using literal header: {err}");
                cfg.interface
                    .header
                    .as_deref()
                    .unwrap_or_default()
                    .to_owned()
            }
        };

    // Load suggestions from the default module, if enabled
    let default_suggestions = exec::default_module_suggestions().unwrap_or_else(|err| {
        eprintln!("Loading default module suggestions failed: {err}");
        Vec::new()
    });

    // Prepare header and suggested itmes accordingly
    let (suggestions, header): (Vec<&config::Module>, String) = if cli_arguments.is_none() {
        (
            cfg.modules.iter().chain(&default_suggestions).collect(),
            expanded_header,
        )
    } else {
        (Vec::new(), String::new())
    };

    // Start the flow
    loop {
        // With arguments -> submit directly
        // Without -> enter TUI
        let submission = match cli_arguments.as_ref() {
            Some(prompt) => Some(prompt.clone()),
            None => tui::tui_loop(&suggestions, &header)?,
        };

        // Leave if nothing prompted
        let Some(prompt) = submission else {
            return Ok(());
        };

        // Run modules by what is prompted
        if !exec::dispatch_prompt(prompt, &default_suggestions) {
            return Ok(());
        }
    }
}
