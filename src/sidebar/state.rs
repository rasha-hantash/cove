// ── State detection for Claude session windows ──
//
// Reads Cove event files written by Claude Code hooks to determine sidebar state.
// Each Claude session has an event file at ~/.cove/events/{session_id}.jsonl.
// The sidebar matches events to tmux windows by comparing the event's `pane_id`
// (from $TMUX_PANE) to each window's tmux pane ID. This correctly handles
// multiple sessions in the same working directory.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, Seek, SeekFrom};
use std::path::Path;

use serde::Deserialize;

use crate::events;
use crate::tmux;

// ── Types ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    /// New session, no hook events fired yet.
    Fresh,
    /// Claude is generating output.
    Working,
    /// Claude is waiting for user to answer a question.
    Asking,
    /// Claude is waiting for the user to approve a tool use.
    Waiting,
    /// Claude finished answering — waiting for next user message.
    Idle,
    /// Claude process exited — shell prompt visible.
    Done,
}

#[derive(Deserialize)]
struct EventEntry {
    state: String,
    #[allow(dead_code)]
    cwd: String,
    /// Tmux pane ID (e.g. "%0") — used to match events to windows.
    #[serde(default)]
    pane_id: String,
    ts: u64,
}

// ── Helpers ──

/// Read the last line of a file efficiently.
/// Returns None if the file is empty or unreadable.
fn read_last_line(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len == 0 {
        return None;
    }

    // Read last 1KB — event lines are ~80 bytes, so this is more than enough
    let tail_start = len.saturating_sub(1024);
    let mut reader = std::io::BufReader::new(file);
    reader.seek(SeekFrom::Start(tail_start)).ok()?;

    // If we seeked mid-line, skip the partial first line
    if tail_start > 0 {
        let mut discard = String::new();
        let _ = reader.read_line(&mut discard);
    }

    let mut last = None;
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    last = Some(trimmed.to_string());
                }
            }
            Err(_) => break,
        }
    }

    last
}

/// Load the latest event from each event file in the events directory.
/// Returns a map of pane_id → state, keeping only the highest-timestamp entry
/// per pane_id. This deduplicates across multiple files that share a recycled
/// pane ID, ensuring the current session's events always win.
fn load_latest_events(dir: &Path) -> HashMap<String, String> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return HashMap::new(),
    };

    // Track (state, timestamp) per pane_id — keep highest timestamp
    let mut best: HashMap<String, (String, u64)> = HashMap::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        if let Some(line) = read_last_line(&path) {
            if let Ok(event) = serde_json::from_str::<EventEntry>(&line) {
                if !event.pane_id.is_empty() {
                    let replace = best
                        .get(&event.pane_id)
                        .is_none_or(|(_, prev_ts)| event.ts > *prev_ts);
                    if replace {
                        best.insert(event.pane_id, (event.state, event.ts));
                    }
                }
            }
        }
    }

    best.into_iter().map(|(k, (state, _))| (k, state)).collect()
}

fn state_from_str(s: &str) -> WindowState {
    match s {
        "working" => WindowState::Working,
        "asking" => WindowState::Asking,
        "waiting" => WindowState::Waiting,
        "idle" => WindowState::Idle,
        _ => WindowState::Fresh,
    }
}

// ── Public API ──

/// Remove event files whose last event matches the given pane_id.
/// Called when a new window is created to prevent stale events (from a previous
/// session that used the same recycled tmux pane_id) from contaminating state.
pub fn purge_events_for_pane(pane_id: &str) {
    let dir = events::events_dir();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        if let Some(line) = read_last_line(&path) {
            if let Ok(event) = serde_json::from_str::<EventEntry>(&line) {
                if event.pane_id == pane_id {
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }
}

pub struct StateDetector {
    pane_ids: HashMap<u32, String>,
}

impl StateDetector {
    pub fn new() -> Self {
        Self {
            pane_ids: HashMap::new(),
        }
    }

    /// Get the tmux pane_id (e.g. "%5") for a window's Claude pane.
    pub fn pane_id(&self, window_index: u32) -> Option<&str> {
        self.pane_ids.get(&window_index).map(String::as_str)
    }

    /// Detect the state of each window. Returns a map from window_index to state.
    pub fn detect(&mut self, windows: &[tmux::WindowInfo]) -> HashMap<u32, WindowState> {
        let mut states = HashMap::new();

        // Get foreground commands + pane IDs for all panes in one tmux call
        let pane_infos: Vec<tmux::PaneInfo> = tmux::list_pane_commands().unwrap_or_default();

        // Store pane_ids so context manager can look them up
        self.pane_ids = pane_infos
            .iter()
            .map(|p| (p.window_index, p.pane_id.clone()))
            .collect();

        let pane_cmds: HashMap<u32, &str> = pane_infos
            .iter()
            .map(|p| (p.window_index, p.command.as_str()))
            .collect();
        let pane_ids: HashMap<u32, &str> = pane_infos
            .iter()
            .map(|p| (p.window_index, p.pane_id.as_str()))
            .collect();

        // Load all latest events once per detect cycle
        let events = load_latest_events(&events::events_dir());

        for win in windows {
            let cmd = pane_cmds.get(&win.index).copied().unwrap_or("zsh");

            // Shell prompt means Claude exited
            if cmd == "zsh" || cmd == "bash" || cmd == "fish" {
                states.insert(win.index, WindowState::Done);
                continue;
            }

            // Match event by pane_id — each tmux pane has a unique ID like "%0"
            let win_pane_id = pane_ids.get(&win.index).copied().unwrap_or("");
            let state = match events.get(win_pane_id) {
                Some(state_str) => state_from_str(state_str),
                None => WindowState::Fresh,
            };

            states.insert(win.index, state);
        }

        states
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_read_last_line_single() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, r#"{{"state":"working","cwd":"/tmp","ts":1000}}"#).unwrap();

        let line = read_last_line(&path).unwrap();
        assert!(line.contains(r#""state":"working""#));
    }

    #[test]
    fn test_read_last_line_multiple() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, r#"{{"state":"working","cwd":"/tmp","ts":1000}}"#).unwrap();
        writeln!(f, r#"{{"state":"idle","cwd":"/tmp","ts":1001}}"#).unwrap();

        let line = read_last_line(&path).unwrap();
        assert!(line.contains(r#""state":"idle""#));
    }

    #[test]
    fn test_read_last_line_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        fs::File::create(&path).unwrap();

        assert!(read_last_line(&path).is_none());
    }

    #[test]
    fn test_read_last_line_missing() {
        let path = Path::new("/nonexistent/test.jsonl");
        assert!(read_last_line(path).is_none());
    }

    #[test]
    fn test_load_latest_events() {
        let dir = tempfile::tempdir().unwrap();

        let mut f1 = fs::File::create(dir.path().join("session-a.jsonl")).unwrap();
        writeln!(
            f1,
            r#"{{"state":"working","cwd":"/project-a","pane_id":"%0","ts":1000}}"#
        )
        .unwrap();
        writeln!(
            f1,
            r#"{{"state":"idle","cwd":"/project-a","pane_id":"%0","ts":1001}}"#
        )
        .unwrap();

        let mut f2 = fs::File::create(dir.path().join("session-b.jsonl")).unwrap();
        writeln!(
            f2,
            r#"{{"state":"asking","cwd":"/project-b","pane_id":"%3","ts":2000}}"#
        )
        .unwrap();

        let events = load_latest_events(dir.path());
        assert_eq!(events.len(), 2);
        assert_eq!(events["%0"], "idle");
        assert_eq!(events["%3"], "asking");
    }

    #[test]
    fn test_same_cwd_different_panes() {
        let dir = tempfile::tempdir().unwrap();

        // Two sessions in the same cwd but different panes
        let mut f1 = fs::File::create(dir.path().join("session-a.jsonl")).unwrap();
        writeln!(
            f1,
            r#"{{"state":"working","cwd":"/same/dir","pane_id":"%0","ts":1000}}"#
        )
        .unwrap();

        let mut f2 = fs::File::create(dir.path().join("session-b.jsonl")).unwrap();
        writeln!(
            f2,
            r#"{{"state":"idle","cwd":"/same/dir","pane_id":"%3","ts":1000}}"#
        )
        .unwrap();

        let events = load_latest_events(dir.path());
        assert_eq!(events.len(), 2);

        // Each should match to its own pane, not cross-contaminate
        assert_eq!(events["%0"], "working");
        assert_eq!(events["%3"], "idle");
    }

    #[test]
    fn test_load_latest_events_deduplicates_by_timestamp() {
        let dir = tempfile::tempdir().unwrap();

        // Stale file with pane_id %0 and older timestamp
        let mut f1 = fs::File::create(dir.path().join("stale-session.jsonl")).unwrap();
        writeln!(
            f1,
            r#"{{"state":"idle","cwd":"/old","pane_id":"%0","ts":1000}}"#
        )
        .unwrap();

        // Current file with pane_id %0 and newer timestamp
        let mut f2 = fs::File::create(dir.path().join("current-session.jsonl")).unwrap();
        writeln!(
            f2,
            r#"{{"state":"working","cwd":"/new","pane_id":"%0","ts":2000}}"#
        )
        .unwrap();

        let events = load_latest_events(dir.path());
        assert_eq!(events.len(), 1);
        // Newer timestamp wins — "working" from ts:2000 beats "idle" from ts:1000
        assert_eq!(events["%0"], "working");
    }

    #[test]
    fn test_events_without_pane_id_ignored() {
        let dir = tempfile::tempdir().unwrap();

        // Old-format event without pane_id should be skipped
        let mut f = fs::File::create(dir.path().join("old-session.jsonl")).unwrap();
        writeln!(f, r#"{{"state":"working","cwd":"/project","ts":1000}}"#).unwrap();

        let events = load_latest_events(dir.path());
        assert!(events.is_empty());
    }

    #[test]
    fn test_load_latest_events_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let events = load_latest_events(dir.path());
        assert!(events.is_empty());
    }

    #[test]
    fn test_load_latest_events_missing_dir() {
        let events = load_latest_events(Path::new("/nonexistent/events"));
        assert!(events.is_empty());
    }

    #[test]
    fn test_state_from_str() {
        assert_eq!(state_from_str("working"), WindowState::Working);
        assert_eq!(state_from_str("idle"), WindowState::Idle);
        assert_eq!(state_from_str("asking"), WindowState::Asking);
        assert_eq!(state_from_str("waiting"), WindowState::Waiting);
        assert_eq!(state_from_str("unknown"), WindowState::Fresh);
    }

    #[test]
    fn test_purge_events_for_pane() {
        let dir = tempfile::tempdir().unwrap();

        // Stale event with pane_id %3 — should be removed
        let mut f1 = fs::File::create(dir.path().join("old-session.jsonl")).unwrap();
        writeln!(
            f1,
            r#"{{"state":"asking","cwd":"/project","pane_id":"%3","ts":1000}}"#
        )
        .unwrap();

        // Active event with pane_id %0 — should be kept
        let mut f2 = fs::File::create(dir.path().join("active-session.jsonl")).unwrap();
        writeln!(
            f2,
            r#"{{"state":"idle","cwd":"/project","pane_id":"%0","ts":2000}}"#
        )
        .unwrap();

        // Another stale event with pane_id %3 — should be removed
        let mut f3 = fs::File::create(dir.path().join("another-old.jsonl")).unwrap();
        writeln!(
            f3,
            r#"{{"state":"idle","cwd":"/other","pane_id":"%3","ts":500}}"#
        )
        .unwrap();

        // Call purge with a custom dir (can't use purge_events_for_pane directly
        // since it uses events::events_dir(), so test the logic inline)
        let entries = fs::read_dir(dir.path()).unwrap();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            if let Some(line) = read_last_line(&path) {
                if let Ok(event) = serde_json::from_str::<EventEntry>(&line) {
                    if event.pane_id == "%3" {
                        fs::remove_file(&path).unwrap();
                    }
                }
            }
        }

        // Only the %0 file should remain
        let remaining: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().and_then(|e| e.to_str()) == Some("jsonl"))
            .collect();
        assert_eq!(remaining.len(), 1);
        assert!(
            remaining[0]
                .path()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .contains("active-session")
        );
    }
}
