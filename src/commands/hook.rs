// ── Claude Code hook handler ──
//
// Called by Claude Code hooks to write Cove state events.
// Reads JSON from stdin, determines state, appends to ~/.cove/events/{session_id}.jsonl.
//
// Hook → state mapping:
//   UserPromptSubmit                       → working
//   PreToolUse (asking tools)              → asking
//   PreToolUse (other tools)               → waiting
//   PostToolUse                            → working
//   Stop                                   → idle

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::cli::HookEvent;
use crate::naming;
use crate::tmux;

// ── Types ──

#[derive(Deserialize)]
struct HookInput {
    session_id: String,
    cwd: String,
    /// Tool name from PreToolUse/PostToolUse payloads (e.g. "Bash", "AskUserQuestion").
    #[serde(default)]
    tool_name: String,
}

// ── Helpers ──

fn events_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".cove").join("events")
}

/// Append a state event to the session's event file.
fn write_event(session_id: &str, cwd: &str, pane_id: &str, state: &str) -> Result<(), String> {
    let dir = events_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("create events dir: {e}"))?;

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

/// Check if the session's event file contains at least one "working" entry,
/// proving the user has submitted a prompt in this session.
fn has_working_event(session_id: &str) -> bool {
    has_working_event_in(session_id, &events_dir())
}

fn has_working_event_in(session_id: &str, dir: &Path) -> bool {
    let path = dir.join(format!("{session_id}.jsonl"));
    fs::read_to_string(path)
        .map(|content| {
            content
                .lines()
                .any(|line| line.contains(r#""state":"working""#))
        })
        .unwrap_or(false)
}

// ── Constants ──

/// Tools that represent Claude asking the user a question (not a permission prompt).
const ASKING_TOOLS: &[&str] = &["AskUserQuestion", "ExitPlanMode", "EnterPlanMode"];

// ── Public API ──

pub fn run(event: HookEvent) -> Result<(), String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| format!("read stdin: {e}"))?;

    let hook: HookInput =
        serde_json::from_str(&input).map_err(|e| format!("parse hook input: {e}"))?;

    let state = match event {
        HookEvent::UserPrompt | HookEvent::AskDone | HookEvent::PostTool => "working",
        HookEvent::Stop => "idle",
        HookEvent::Ask => "asking",
        HookEvent::PreTool => {
            if ASKING_TOOLS.contains(&hook.tool_name.as_str()) {
                "asking"
            } else {
                "waiting"
            }
        }
    };

    // Suppress the initial "idle" on session startup — only write it after
    // the user has submitted at least one prompt (i.e. a "working" event exists).
    if state == "idle" && !has_working_event(&hook.session_id) {
        return Ok(());
    }

    // $TMUX_PANE uniquely identifies which tmux pane Claude is running in.
    // This lets the sidebar distinguish sessions even when they share a cwd.
    let pane_id = std::env::var("TMUX_PANE").unwrap_or_default();

    write_event(&hook.session_id, &hook.cwd, &pane_id, state)?;

    // Auto-update window name if the branch changed (e.g. Claude switched branches
    // or created a worktree). Silently ignore errors — hooks must never block Claude.
    let _ = maybe_rename_window(&hook.cwd, &pane_id);

    Ok(())
}

/// Recompute the window name from the stored base + current git branch.
/// Renames the window if the name has drifted.
fn maybe_rename_window(cwd: &str, pane_id: &str) -> Result<(), String> {
    let base = tmux::get_window_option(pane_id, "@cove_base")?;
    let expected = naming::build_window_name(&base, cwd);
    let current = tmux::get_window_name(pane_id)?;

    if current != expected {
        tmux::rename_window(pane_id, &expected)?;
    }
    Ok(())
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_write_event_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let events = dir.path().join("events");

        fs::create_dir_all(&events).unwrap();
        let path = events.join("test-session.jsonl");

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .unwrap();
        writeln!(file, r#"{{"state":"working","cwd":"/tmp","ts":1234}}"#).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains(r#""state":"working""#));
        assert!(content.contains(r#""cwd":"/tmp""#));
    }

    #[test]
    fn test_has_working_event_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!has_working_event_in("no-such-session", dir.path()));
    }

    #[test]
    fn test_has_working_event_with_working() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test-session.jsonl");

        fs::write(
            &path,
            r#"{"state":"working","cwd":"/tmp","pane_id":"%1","ts":1000}
{"state":"idle","cwd":"/tmp","pane_id":"%1","ts":1001}
"#,
        )
        .unwrap();

        assert!(has_working_event_in("test-session", dir.path()));
    }
}
