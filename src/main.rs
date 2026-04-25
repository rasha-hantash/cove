use clap::Parser;
use cove_cli::cli::{Cli, Command};
use cove_cli::{commands, paths, sidebar};

fn main() {
    paths::migrate_legacy();
    let cli = Cli::parse();

    let result = match cli.command {
        Some(Command::List) => commands::list::run(),
        Some(Command::Kill { name, force }) => commands::kill::run(&name, force),
        Some(Command::AllKill { force }) => commands::kill::run_all(force),
        Some(Command::Resume) => commands::resume::run(),
        Some(Command::Sidebar) => sidebar::app::run(),
        Some(Command::Hook { event }) => commands::hook::run(event),
        Some(Command::Init) => commands::init::run(),
        Some(Command::Voice { name, dir }) => commands::voice::run(name.as_deref(), dir.as_deref()),
        None => {
            // Default behavior: start a session or resume.
            // The window name is just a bootstrap label — Claude Code's OSC title
            // takes over the tmux window name once the session is alive.
            let docker = !cli.local;
            match cli.name {
                Some(name) => {
                    let dir = cli.dir.as_deref().unwrap_or(".");
                    commands::start::run(&name, Some(dir), docker)
                }
                None => {
                    let base = std::env::current_dir()
                        .ok()
                        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                        .unwrap_or_else(|| "session".to_string());
                    commands::start::run(&base, Some("."), docker)
                }
            }
        }
    };

    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
