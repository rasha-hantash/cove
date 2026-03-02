// ── Background context generation for sessions ──
//
// Reads Claude session JSONL files directly, extracts conversation text,
// then calls `claude -p` with a compact prompt (no session resume).
// This avoids the ~600KB context reload that `claude -c` would trigger.
// Results flow back via mpsc channel so the sidebar event loop never blocks.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use std::{io, thread};

// ── Types ──

pub struct ContextManager {
    contexts: HashMap<String, String>,
    in_flight: HashSet<String>,
    failed: HashMap<String, Instant>,
    tx: mpsc::Sender<(String, String)>,
    rx: mpsc::Receiver<(String, String)>,
}

// ── Constants ──

const SUMMARY_PROMPT: &str = "\
Summarize what was being worked on in this conversation in 1-2 concise sentences. \
Be specific about the feature, bug, or task. Output only the summary, nothing else.";

/// Max chars of conversation text to include in the prompt.
const CONVERSATION_BUDGET: usize = 6000;

/// Max chars per individual message (truncate long code blocks, etc.).
const MESSAGE_TRUNCATE: usize = 300;

const SUBPROCESS_TIMEOUT: Duration = Duration::from_secs(30);
const RETRY_COOLDOWN: Duration = Duration::from_secs(30);

// ── Public API ──

impl ContextManager {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            contexts: HashMap::new(),
            in_flight: HashSet::new(),
            failed: HashMap::new(),
            tx,
            rx,
        }
    }

    /// Drain completed context results from background threads.
    pub fn drain(&mut self) {
        while let Ok((name, context)) = self.rx.try_recv() {
            self.in_flight.remove(&name);
            if context.is_empty() {
                self.failed.insert(name, Instant::now());
            } else {
                self.contexts.insert(name, context);
            }
        }
    }

    /// Get the context for a window, if available.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.contexts.get(name).map(String::as_str)
    }

    /// Whether a context request is currently running for this window.
    pub fn is_loading(&self, name: &str) -> bool {
        self.in_flight.contains(name)
    }

    /// Request context generation for a window (no-op if cached, in flight, or within retry cooldown).
    pub fn request(&mut self, name: &str, cwd: &str, pane_id: &str) {
        if self.contexts.contains_key(name) || self.in_flight.contains(name) {
            return;
        }
        if let Some(failed_at) = self.failed.get(name) {
            if failed_at.elapsed() < RETRY_COOLDOWN {
                return;
            }
        }
        self.failed.remove(name);
        self.spawn(name, cwd, pane_id);
    }

    /// Force-refresh context for a window (clears cache/failed, respects in_flight).
    pub fn refresh(&mut self, name: &str, cwd: &str, pane_id: &str) {
        if self.in_flight.contains(name) {
            return;
        }
        self.contexts.remove(name);
        self.failed.remove(name);
        self.spawn(name, cwd, pane_id);
    }

    fn spawn(&mut self, name: &str, cwd: &str, pane_id: &str) {
        self.in_flight.insert(name.to_string());
        let tx = self.tx.clone();
        let name = name.to_string();
        let cwd = cwd.to_string();
        let pane_id = pane_id.to_string();
        thread::spawn(move || {
            let context = generate_context(&cwd, &pane_id).unwrap_or_default();
            let _ = tx.send((name, context));
        });
    }
}

// ── Session Lookup ──

/// Find the Claude session_id for a given tmux pane_id by scanning cove event files.
fn find_session_id(pane_id: &str) -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let events_dir = PathBuf::from(&home).join(".cove").join("events");
    let entries = fs::read_dir(&events_dir).ok()?;

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

/// Derive the Claude project directory from a cwd.
fn claude_project_dir(cwd: &str) -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    let project_key = cwd.replace('/', "-");
    PathBuf::from(home)
        .join(".claude")
        .join("projects")
        .join(project_key)
}

/// Find the Claude session JSONL file for a given pane.
fn find_session_file(cwd: &str, pane_id: &str) -> Option<PathBuf> {
    let session_id = find_session_id(pane_id)?;
    let project_dir = claude_project_dir(cwd);
    let path = project_dir.join(format!("{session_id}.jsonl"));
    if path.exists() { Some(path) } else { None }
}

// ── JSONL Parsing ──

/// Extract conversation text from a Claude session JSONL file.
/// Returns a compact representation of user/assistant messages, truncated
/// to fit within CONVERSATION_BUDGET.
fn extract_conversation(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let mut messages = Vec::new();

    for line in content.lines() {
        let entry: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let entry_type = match entry.get("type").and_then(|t| t.as_str()) {
            Some(t) => t,
            None => continue,
        };

        if entry_type != "user" && entry_type != "assistant" {
            continue;
        }

        let message = match entry.get("message") {
            Some(m) => m,
            None => continue,
        };

        let role = message
            .get("role")
            .and_then(|r| r.as_str())
            .unwrap_or(entry_type);

        let content_arr = match message.get("content").and_then(|c| c.as_array()) {
            Some(arr) => arr,
            None => continue,
        };

        for item in content_arr {
            // Only extract text content — skip images, tool_use, tool_result
            if item.get("type").and_then(|t| t.as_str()) != Some("text") {
                continue;
            }
            if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                let text = text.trim();
                if text.is_empty() {
                    continue;
                }
                let truncated = if text.chars().count() > MESSAGE_TRUNCATE {
                    let t: String = text.chars().take(MESSAGE_TRUNCATE).collect();
                    format!("{t}\u{2026}")
                } else {
                    text.to_string()
                };
                let label = if role == "user" { "User" } else { "Assistant" };
                messages.push(format!("{label}: {truncated}"));
            }
        }
    }

    if messages.is_empty() {
        return None;
    }

    // Keep recent messages, truncated to fit within budget
    let mut kept = Vec::new();
    let mut total_len = 0;
    for msg in messages.iter().rev() {
        if total_len + msg.len() + 2 > CONVERSATION_BUDGET {
            break;
        }
        total_len += msg.len() + 2;
        kept.push(msg.as_str());
    }
    kept.reverse();

    Some(kept.join("\n\n"))
}

// ── Context Generation ──

fn generate_context(cwd: &str, pane_id: &str) -> Option<String> {
    let session_path = find_session_file(cwd, pane_id)?;
    let conversation = extract_conversation(&session_path)?;

    let prompt = format!("{SUMMARY_PROMPT}\n\nConversation:\n{conversation}");

    // Fresh claude -p call — no -c, no session resume, no large context reload
    let mut child = Command::new("claude")
        .args(["-p", &prompt, "--max-turns", "1", "--model", "haiku"])
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let deadline = Instant::now() + SUBPROCESS_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                thread::sleep(Duration::from_millis(200));
            }
            Err(_) => return None,
        }
    };

    if !status.success() {
        return None;
    }

    let mut stdout = child.stdout.take()?;
    let mut text = String::new();
    io::Read::read_to_string(&mut stdout, &mut text).ok()?;
    let text = text.trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_session_jsonl(dir: &Path, session_id: &str, messages: &[(&str, &str)]) -> PathBuf {
        let path = dir.join(format!("{session_id}.jsonl"));
        let mut f = fs::File::create(&path).unwrap();
        for (role, text) in messages {
            let entry = serde_json::json!({
                "type": role,
                "message": {
                    "role": role,
                    "content": [{"type": "text", "text": text}]
                }
            });
            writeln!(f, "{}", serde_json::to_string(&entry).unwrap()).unwrap();
        }
        path
    }

    #[test]
    fn test_extract_conversation_basic() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_session_jsonl(
            dir.path(),
            "test",
            &[
                ("user", "Fix the login bug"),
                ("assistant", "I'll look into the auth module"),
                ("user", "Also check the session handling"),
            ],
        );

        let result = extract_conversation(&path).unwrap();
        assert!(result.contains("User: Fix the login bug"));
        assert!(result.contains("Assistant: I'll look into the auth module"));
        assert!(result.contains("User: Also check the session handling"));
    }

    #[test]
    fn test_extract_conversation_skips_non_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jsonl");
        let mut f = fs::File::create(&path).unwrap();

        // User message with text
        writeln!(
            f,
            r#"{{"type":"user","message":{{"role":"user","content":[{{"type":"text","text":"hello"}}]}}}}"#
        )
        .unwrap();
        // Progress entry (should be skipped)
        writeln!(f, r#"{{"type":"progress","data":{{"hook":"test"}}}}"#).unwrap();
        // Assistant with tool_use (should be skipped)
        writeln!(
            f,
            r#"{{"type":"assistant","message":{{"role":"assistant","content":[{{"type":"tool_use","name":"Bash"}}]}}}}"#
        )
        .unwrap();

        let result = extract_conversation(&path).unwrap();
        assert!(result.contains("User: hello"));
        assert!(!result.contains("progress"));
        assert!(!result.contains("Bash"));
    }

    #[test]
    fn test_extract_conversation_truncates_long_messages() {
        let dir = tempfile::tempdir().unwrap();
        let long_text = "a".repeat(500);
        let path = write_session_jsonl(dir.path(), "test", &[("user", &long_text)]);

        let result = extract_conversation(&path).unwrap();
        // Should be truncated to MESSAGE_TRUNCATE + "…"
        assert!(result.chars().count() < 500);
        assert!(result.ends_with('\u{2026}'));
    }

    #[test]
    fn test_extract_conversation_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.jsonl");
        fs::File::create(&path).unwrap();

        assert!(extract_conversation(&path).is_none());
    }

    #[test]
    fn test_extract_conversation_respects_budget() {
        let dir = tempfile::tempdir().unwrap();
        let msg = "x".repeat(MESSAGE_TRUNCATE - 10);
        let messages: Vec<(&str, &str)> = (0..100).map(|_| ("user", msg.as_str())).collect();
        let path = write_session_jsonl(dir.path(), "test", &messages);

        let result = extract_conversation(&path).unwrap();
        assert!(result.len() <= CONVERSATION_BUDGET + 200); // some slack for labels
    }

    #[test]
    fn test_claude_project_dir() {
        let dir = claude_project_dir("/Users/test/workspace/myproject");
        assert!(
            dir.to_str()
                .unwrap()
                .ends_with("/.claude/projects/-Users-test-workspace-myproject")
        );
    }

    #[test]
    fn test_find_session_id_from_events() {
        let dir = tempfile::tempdir().unwrap();
        let events_path = dir.path().join("abc-123.jsonl");
        let mut f = fs::File::create(&events_path).unwrap();
        writeln!(
            f,
            r#"{{"state":"working","cwd":"/tmp","pane_id":"%5","ts":1000}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"state":"idle","cwd":"/tmp","pane_id":"%5","ts":1001}}"#
        )
        .unwrap();

        // Test the lookup logic directly (can't use find_session_id since it reads ~/.cove)
        let content = fs::read_to_string(&events_path).unwrap();
        let last_line = content
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap();
        let event: serde_json::Value = serde_json::from_str(last_line).unwrap();
        assert_eq!(event.get("pane_id").and_then(|v| v.as_str()), Some("%5"));
    }
}
