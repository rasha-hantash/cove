use clap::Parser;
use cove_cli::cli::{Cli, Command};
use cove_cli::{commands, naming, sidebar, tmux};

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Command::List) => commands::list::run(),
        Some(Command::Kill { name }) => commands::kill::run(&name),
        Some(Command::AllKill) => commands::kill::run_all(),
        Some(Command::Resume) => commands::resume::run(),
        Some(Command::Sidebar) => sidebar::app::run(),
        Some(Command::Hook { event }) => commands::hook::run(event),
        Some(Command::Init) => commands::init::run(),
        None => {
            // Default behavior: start a session or resume
            match cli.name {
                Some(name) => {
                    let dir = cli.dir.as_deref().unwrap_or(".");
                    let full = naming::build_window_name(&name, dir);
                    commands::start::run(&full, &name, Some(dir))
                }
                None => {
                    if tmux::has_session() {
                        commands::resume::run()
                    } else {
                        let base = std::env::current_dir()
                            .ok()
                            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                            .unwrap_or_else(|| "session".to_string());
                        let full = naming::build_window_name(&base, ".");
                        commands::start::run(&full, &base, Some("."))
                    }
                }
            }
        }
    };

    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
