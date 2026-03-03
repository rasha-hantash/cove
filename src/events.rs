// ── Shared event helpers ──
//
// Functions for reading and writing Cove session event files.
// Used by hook handler (writing), sidebar (reading), and kill (writing end events).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Path to the Cove events directory (~/.cove/events/).
pub fn events_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".cove").join("events")
}

/// Append a state event to the session's event file.
pub fn write_event(session_id: &str, cwd: &str, pane_id: &str, state: &str) -> Result<(), String> {
    write_event_to(&events_dir(), session_id, cwd, pane_id, state)
}

/// Append a state event to a session file in the given directory.
pub fn write_event_to(
    dir: &Path,
    session_id: &str,
    cwd: &str,
    pane_id: &str,
    state: &str,
) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("create events dir: {e}"))?;

    let path = dir.join(format!("{session_id}.jsonl"));
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("open event file: {e}"))?;

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let line = format!(r#"{{"state":"{state}","cwd":"{cwd}","pane_id":"{pane_id}","ts":{ts}}}"#);
    writeln!(file, "{line}").map_err(|e| format!("write event: {e}"))?;

    Ok(())
}

/// Find the Claude session_id for a given tmux pane_id by scanning cove event files.
/// Returns the session_id from the file whose last event matches the pane_id
/// with the highest timestamp (handles pane_id recycling).
pub fn find_session_id(pane_id: &str) -> Option<String> {
    find_session_id_in(&events_dir(), pane_id)
}

/// Find the Claude session_id for a given pane_id by scanning event files in the given directory.
pub fn find_session_id_in(dir: &Path, pane_id: &str) -> Option<String> {
    let entries = fs::read_dir(dir).ok()?;

    let mut best: Option<(String, u64)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let content = fs::read_to_string(&path).unwrap_or_default();
        let last_line = content.lines().rev().find(|l| !l.trim().is_empty());
        if let Some(line) = last_line {
            if let Ok(event) = serde_json::from_str::<serde_json::Value>(line) {
                if event.get("pane_id").and_then(|v| v.as_str()) == Some(pane_id) {
                    let ts = event.get("ts").and_then(|v| v.as_u64()).unwrap_or(0);
                    if best.as_ref().is_none_or(|(_, prev_ts)| ts > *prev_ts) {
                        if let Some(sid) = path.file_stem().and_then(|s| s.to_str()) {
                            best = Some((sid.to_string(), ts));
                        }
                    }
                }
            }
        }
    }

    best.map(|(sid, _)| sid)
}
