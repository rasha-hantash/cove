use std::io::{self, BufRead};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::colors::*;
use crate::events;
use crate::tmux;

const GRACEFUL_TIMEOUT: Duration = Duration::from_secs(15);

/// Write an "end" event for a window's Claude pane before killing it.
/// All errors are silently swallowed — kill must never fail because of event writing.
fn write_end_event(window_name: &str) {
    let pane_id = match tmux::get_claude_pane_id(window_name) {
        Ok(id) => id,
        Err(_) => return,
    };
    let session_id = match events::find_session_id(&pane_id) {
        Some(id) => id,
        None => return,
    };
    let cwd = tmux::list_windows()
        .ok()
        .and_then(|wins| {
            wins.into_iter()
                .find(|w| w.name == window_name)
                .map(|w| w.pane_path)
        })
        .unwrap_or_default();
    let _ = events::write_event(&session_id, &cwd, &pane_id, "end");
}

/// Send /exit to Claude and wait for it to stop.
fn graceful_exit(window_name: &str) -> bool {
    // Prevent pane-died hook from respawning Claude after exit
    let _ = tmux::disable_respawn(window_name);

    // Interrupt any in-progress work, then send /exit
    let _ = tmux::send_keys(window_name, &["C-c"]);
    thread::sleep(Duration::from_millis(500));
    let _ = tmux::send_keys(window_name, &["/exit", "Enter"]);

    // Poll until Claude exits (pane_current_command changes from "claude")
    let start = Instant::now();
    while start.elapsed() < GRACEFUL_TIMEOUT {
        thread::sleep(Duration::from_secs(1));
        match tmux::pane_command(window_name) {
            Ok(cmd) if cmd != "claude" => return true,
            Err(_) => return true,
            _ => continue,
        }
    }
    false
}

/// Run the brain-os capture script for a window, returning the Claude session_id if captured.
fn run_capture(window_name: &str) -> Option<String> {
    let pane_id = tmux::get_claude_pane_id(window_name).ok()?;
    let session_id = events::find_session_id(&pane_id)?;
    let cwd = tmux::list_windows()
        .ok()
        .and_then(|wins| {
            wins.into_iter()
                .find(|w| w.name == window_name)
                .map(|w| w.pane_path)
        })
        .unwrap_or_default();

    let home = std::env::var("HOME").unwrap_or_default();
    let capture_script = std::path::PathBuf::from(home).join(".claude/hooks/brain-os-capture.py");

    if !capture_script.exists() {
        return None;
    }

    let status = Command::new("python3")
        .arg(&capture_script)
        .arg("--session-id")
        .arg(&session_id)
        .arg("--cwd")
        .arg(&cwd)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();

    match status {
        Ok(s) if s.success() => {
            // Write marker to prevent SessionEnd hook from double-capturing
            let _ = std::fs::write(format!("/tmp/cove-captured-{session_id}"), "");
            Some(session_id)
        }
        _ => None,
    }
}

pub fn run(name: &str, force: bool) -> Result<(), String> {
    if !tmux::has_session() {
        println!("{ANSI_OVERLAY}No active cove session.{ANSI_RESET}");
        return Err(String::new());
    }

    write_end_event(name);

    if !force {
        run_capture(name);

        println!("Press Enter to close {ANSI_PEACH}{name}{ANSI_RESET}, or Ctrl-C to cancel.");
        let _ = io::stdin().lock().read_line(&mut String::new());

        println!("Shutting down {ANSI_PEACH}{name}{ANSI_RESET} gracefully...");
        graceful_exit(name);
    }
    tmux::kill_window(name)?;
    println!("Killed: {ANSI_PEACH}{name}{ANSI_RESET}");
    Ok(())
}

pub fn run_all(force: bool) -> Result<(), String> {
    if !tmux::has_session() {
        println!("{ANSI_OVERLAY}No active cove session.{ANSI_RESET}");
        return Err(String::new());
    }

    let windows = tmux::list_windows().unwrap_or_default();
    for win in &windows {
        write_end_event(&win.name);
    }

    if !force {
        // Capture learnings from all sessions before exiting
        for win in &windows {
            run_capture(&win.name);
        }

        println!(
            "\nPress Enter to close {} session(s), or Ctrl-C to cancel.",
            windows.len()
        );
        let _ = io::stdin().lock().read_line(&mut String::new());

        println!("Shutting down {} session(s) gracefully...", windows.len());
        for win in &windows {
            let exited = graceful_exit(&win.name);
            let status = if exited { "exited" } else { "timed out" };
            println!("  {}: {status}", win.name);
        }
    }

    tmux::kill_session()?;
    println!("Killed all sessions.");
    Ok(())
}
