// ── Shared test helpers for integration tests ──

#![allow(dead_code)]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

/// Write a single JSONL event line to a session file.
pub fn write_event_line(
    dir: &Path,
    session_id: &str,
    state: &str,
    cwd: &str,
    pane_id: &str,
    ts: u64,
) {
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(format!("{session_id}.jsonl"));
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap();
    let line = format!(r#"{{"state":"{state}","cwd":"{cwd}","pane_id":"{pane_id}","ts":{ts}}}"#);
    writeln!(file, "{line}").unwrap();
}

/// Write multiple events to build a realistic session file.
pub fn write_event_sequence(dir: &Path, session_id: &str, events: &[(&str, &str, &str, u64)]) {
    for (state, cwd, pane_id, ts) in events {
        write_event_line(dir, session_id, state, cwd, pane_id, *ts);
    }
}

/// Build a tmux `list-windows` output string from window data.
/// Each tuple: (index, name, is_active, pane_path)
pub fn fake_window_output(windows: &[(u32, &str, bool, &str)]) -> String {
    windows
        .iter()
        .map(|(idx, name, active, path)| {
            format!(
                "{}|{}|{}|{}",
                idx,
                name,
                if *active { "1" } else { "0" },
                path
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build a tmux `list-panes` output string from pane data.
/// Each tuple: (window_index, pane_index, command, pane_id)
pub fn fake_pane_output(panes: &[(u32, u32, &str, &str)]) -> String {
    panes
        .iter()
        .map(|(win_idx, pane_idx, cmd, pane_id)| format!("{win_idx}|{pane_idx}|{cmd}|{pane_id}"))
        .collect::<Vec<_>>()
        .join("\n")
}
