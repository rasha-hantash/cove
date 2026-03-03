use crate::colors::*;
use crate::events;
use crate::tmux;

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

pub fn run(name: &str) -> Result<(), String> {
    if !tmux::has_session() {
        println!("{ANSI_OVERLAY}No active cove session.{ANSI_RESET}");
        return Err(String::new());
    }

    write_end_event(name);
    tmux::kill_window(name)?;
    println!("Killed: {ANSI_PEACH}{name}{ANSI_RESET}");
    Ok(())
}

pub fn run_all() -> Result<(), String> {
    if !tmux::has_session() {
        println!("{ANSI_OVERLAY}No active cove session.{ANSI_RESET}");
        return Err(String::new());
    }

    if let Ok(windows) = tmux::list_windows() {
        for win in &windows {
            write_end_event(&win.name);
        }
    }
    tmux::kill_session()?;
    println!("Killed all sessions.");
    Ok(())
}
