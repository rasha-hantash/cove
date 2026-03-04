// ── CLI argument parsing tests ──
//
// Tests clap parsing of Cli struct without running any commands.

use clap::Parser;
use cove_cli::cli::{Cli, Command, HookEvent};

#[test]
fn no_args() {
    let cli = Cli::parse_from(["cove"]);
    assert!(cli.name.is_none());
    assert!(cli.dir.is_none());
    assert!(cli.command.is_none());
}

#[test]
fn session_name_only() {
    let cli = Cli::parse_from(["cove", "my-project"]);
    assert_eq!(cli.name.as_deref(), Some("my-project"));
    assert!(cli.dir.is_none());
    assert!(cli.command.is_none());
}

#[test]
fn session_name_and_dir() {
    let cli = Cli::parse_from(["cove", "my-project", "/path/to/dir"]);
    assert_eq!(cli.name.as_deref(), Some("my-project"));
    assert_eq!(cli.dir.as_deref(), Some("/path/to/dir"));
    assert!(cli.command.is_none());
}

#[test]
fn list_subcommand() {
    // `list` command
    let cli = Cli::parse_from(["cove", "list"]);
    assert!(matches!(cli.command, Some(Command::List)));

    // `ls` alias
    let cli = Cli::parse_from(["cove", "ls"]);
    assert!(matches!(cli.command, Some(Command::List)));
}

#[test]
fn hook_subcommands() {
    let cases: &[(&str, fn(&HookEvent) -> bool)] = &[
        ("user-prompt", |e| matches!(e, HookEvent::UserPrompt)),
        ("stop", |e| matches!(e, HookEvent::Stop)),
        ("ask", |e| matches!(e, HookEvent::Ask)),
        ("ask-done", |e| matches!(e, HookEvent::AskDone)),
        ("pre-tool", |e| matches!(e, HookEvent::PreTool)),
        ("post-tool", |e| matches!(e, HookEvent::PostTool)),
        ("session-end", |e| matches!(e, HookEvent::SessionEnd)),
    ];

    for (name, check) in cases {
        let cli = Cli::parse_from(["cove", "hook", name]);
        match &cli.command {
            Some(Command::Hook { event }) => {
                assert!(check(event), "hook {name} parsed to wrong variant");
            }
            other => panic!("expected Hook command for '{name}', got {other:?}"),
        }
    }
}
