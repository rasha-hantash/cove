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
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use std::{io, thread};

use crate::events;
use crate::sidebar::state::WindowState;
use crate::tmux::WindowInfo;

// ── Types ──

type GeneratorFn = Arc<dyn Fn(&str, &str) -> Option<String> + Send + Sync>;

pub struct ContextManager {
    contexts: HashMap<String, String>,
    in_flight: HashSet<String>,
    failed: HashMap<String, Instant>,
    tx: mpsc::Sender<(String, String)>,
    rx: mpsc::Receiver<(String, String)>,
    generator: GeneratorFn,
    prev_selected_name: Option<String>,
}

// ── Constants ──

const SUMMARY_PROMPT: &str = "\
Summarize the overall goal and current state of this conversation in 1-2 sentences. \
What is the user trying to accomplish, and where are things at? \
Output only the summary, nothing else.";

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
            generator: Arc::new(generate_context),
            prev_selected_name: None,
        }
    }

    #[cfg(test)]
    pub fn with_generator(
        generator_fn: impl Fn(&str, &str) -> Option<String> + Send + Sync + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            contexts: HashMap::new(),
            in_flight: HashSet::new(),
            failed: HashMap::new(),
            tx,
            rx,
            generator: Arc::new(generator_fn),
            prev_selected_name: None,
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

    /// Run one tick of context orchestration.
    ///
    /// - Prefetches context for all non-Fresh sessions
    /// - Drains completed results from background threads
    /// - On selection change: refreshes old session (if active), requests new session (if active)
    pub fn tick(
        &mut self,
        windows: &[WindowInfo],
        states: &HashMap<u32, WindowState>,
        selected: usize,
        pane_id_for: &impl Fn(u32) -> Option<String>,
    ) {
        // Prefetch context for all non-fresh sessions
        for win in windows {
            let state = states
                .get(&win.index)
                .copied()
                .unwrap_or(WindowState::Fresh);
            if state != WindowState::Fresh {
                let pane_id = pane_id_for(win.index).unwrap_or_default();
                self.request(&win.name, &win.pane_path, &pane_id);
            }
        }

        // Drain completed context results from background threads
        self.drain();

        // Track selection changes and manage context generation
        let current_name = windows.get(selected).map(|w| w.name.clone());
        if current_name != self.prev_selected_name {
            // Refresh context for old session — only if it had activity (not Fresh)
            if let Some(ref prev_name) = self.prev_selected_name {
                if let Some(prev_win) = windows.iter().find(|w| w.name == *prev_name) {
                    let state = states
                        .get(&prev_win.index)
                        .copied()
                        .unwrap_or(WindowState::Fresh);
                    if state != WindowState::Fresh {
                        let pane_id = pane_id_for(prev_win.index).unwrap_or_default();
                        self.refresh(&prev_win.name, &prev_win.pane_path, &pane_id);
                    }
                }
            }
            // Request context for new session — only if it had activity (not Fresh)
            if let Some(win) = windows.get(selected) {
                let state = states
                    .get(&win.index)
                    .copied()
                    .unwrap_or(WindowState::Fresh);
                if state != WindowState::Fresh {
                    let pane_id = pane_id_for(win.index).unwrap_or_default();
                    self.request(&win.name, &win.pane_path, &pane_id);
                }
            }
            self.prev_selected_name = current_name;
        }
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
        let generator = Arc::clone(&self.generator);
        thread::spawn(move || {
            let context = generator(&cwd, &pane_id).unwrap_or_default();
            let _ = tx.send((name, context));
        });
    }
}

// ── Session Lookup ──

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
    let session_id = events::find_session_id(pane_id)?;
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

    // Fresh claude -p call — no -c, no session resume, no large context reload.
    // Must clear CLAUDECODE env var to avoid "nested session" detection,
    // since the sidebar itself runs inside a Claude Code session.
    let mut child = Command::new("claude")
        .args(["-p", &prompt, "--max-turns", "1", "--model", "haiku"])
        .current_dir(cwd)
        .env_remove("CLAUDECODE")
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

    // ── Context lifecycle integration tests ──
    //
    // These test the orchestration logic in tick(): when context generation
    // fires (or doesn't) based on session state and selection changes.

    use std::sync::Mutex;

    /// Build a mock generator that tracks calls and returns controlled results.
    fn mock_generator() -> (
        impl Fn(&str, &str) -> Option<String> + Send + Sync + 'static,
        Arc<Mutex<Vec<(String, String)>>>,
    ) {
        let calls: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let calls_clone = Arc::clone(&calls);
        let generator = move |cwd: &str, pane_id: &str| -> Option<String> {
            calls_clone
                .lock()
                .unwrap()
                .push((cwd.to_string(), pane_id.to_string()));
            Some(format!("context for {pane_id}"))
        };
        (generator, calls)
    }

    fn win(index: u32, name: &str) -> WindowInfo {
        WindowInfo {
            index,
            name: name.to_string(),
            is_active: false,
            pane_path: format!("/project/{name}"),
        }
    }

    fn pane_ids(map: &HashMap<u32, String>) -> impl Fn(u32) -> Option<String> + '_ {
        move |idx| map.get(&idx).cloned()
    }

    /// Drain results by waiting briefly for background threads to complete.
    fn drain_with_wait(mgr: &mut ContextManager) {
        // Background threads are fast (mock generator returns immediately),
        // but we need a brief pause for the thread to send its result.
        thread::sleep(Duration::from_millis(50));
        mgr.drain();
    }

    // ── Scenario 1: New session → no context action ──
    //
    // A brand-new session (Fresh state) should not trigger any context
    // generation. No "loading…", no failed generation.
    #[test]
    fn test_new_session_no_context_action() {
        let (generator, calls) = mock_generator();
        let mut mgr = ContextManager::with_generator(generator);

        let windows = vec![win(1, "session-1")];
        let states: HashMap<u32, WindowState> = [(1, WindowState::Fresh)].into_iter().collect();
        let panes: HashMap<u32, String> = [(1, "%0".into())].into_iter().collect();

        // Run a tick with the Fresh session selected
        mgr.tick(&windows, &states, 0, &pane_ids(&panes));

        // No generation should have been spawned
        assert!(calls.lock().unwrap().is_empty());
        assert!(!mgr.is_loading("session-1"));
        assert!(mgr.get("session-1").is_none());
    }

    // ── Scenario 2: Switch from new (no prompt) session to another new session → no context action ──
    //
    // Both sessions are Fresh. Switching between them should not trigger
    // context generation for either session.
    #[test]
    fn test_switch_between_fresh_sessions_no_context_action() {
        let (generator, calls) = mock_generator();
        let mut mgr = ContextManager::with_generator(generator);

        let windows = vec![win(1, "session-1"), win(2, "session-2")];
        let states: HashMap<u32, WindowState> = [(1, WindowState::Fresh), (2, WindowState::Fresh)]
            .into_iter()
            .collect();
        let panes: HashMap<u32, String> =
            [(1, "%0".into()), (2, "%1".into())].into_iter().collect();

        // Tick 1: session-1 selected (Fresh)
        mgr.tick(&windows, &states, 0, &pane_ids(&panes));
        assert!(calls.lock().unwrap().is_empty());

        // Tick 2: switch to session-2 (also Fresh)
        mgr.tick(&windows, &states, 1, &pane_ids(&panes));

        // No generation for either session
        assert!(calls.lock().unwrap().is_empty());
        assert!(!mgr.is_loading("session-1"));
        assert!(!mgr.is_loading("session-2"));
        assert!(mgr.get("session-1").is_none());
        assert!(mgr.get("session-2").is_none());
    }

    // ── Scenario 3: Switch from active session to new session → fire context for previous ──
    //
    // When switching away from a session that has had at least 1 conversation
    // turn (not Fresh), context generation should fire for that session.
    // The new Fresh session should NOT get context generated.
    #[test]
    fn test_switch_from_active_to_fresh_fires_context_for_previous() {
        let (generator, calls) = mock_generator();
        let mut mgr = ContextManager::with_generator(generator);

        let windows = vec![win(1, "active-session"), win(2, "new-session")];
        let states: HashMap<u32, WindowState> = [(1, WindowState::Idle), (2, WindowState::Fresh)]
            .into_iter()
            .collect();
        let panes: HashMap<u32, String> =
            [(1, "%0".into()), (2, "%1".into())].into_iter().collect();

        // Tick 1: active-session selected (Idle — has had activity)
        mgr.tick(&windows, &states, 0, &pane_ids(&panes));
        drain_with_wait(&mut mgr);

        // The prefetch loop should have requested context for the active session
        let call_count_after_tick1 = calls.lock().unwrap().len();
        assert!(
            call_count_after_tick1 > 0,
            "should request context for Idle session"
        );
        assert!(
            calls.lock().unwrap().iter().any(|(_, pid)| pid == "%0"),
            "should request for pane %0"
        );

        // Tick 2: switch to new-session (Fresh)
        calls.lock().unwrap().clear();
        mgr.tick(&windows, &states, 1, &pane_ids(&panes));
        // Wait for background thread to complete so calls are recorded
        drain_with_wait(&mut mgr);

        // Should fire refresh for old active-session (pane %0)
        // Should NOT fire for new-session (pane %1) since it's Fresh
        let tick2_calls = calls.lock().unwrap().clone();
        assert!(
            tick2_calls.iter().any(|(_, pid)| pid == "%0"),
            "should refresh context for previous active session"
        );
        assert!(
            !tick2_calls.iter().any(|(_, pid)| pid == "%1"),
            "should NOT request context for Fresh new session"
        );

        // new-session should have no loading state or context
        assert!(!mgr.is_loading("new-session"));
        assert!(mgr.get("new-session").is_none());
    }

    // ── Scenario 4: Switch back to previous session while context is pending → loading state ──
    //
    // After switching away from an active session (triggering refresh),
    // switching back before the generation completes should show loading state.
    #[test]
    fn test_switch_back_while_pending_shows_loading() {
        // Use a slow generator that blocks until signaled
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let barrier_clone = Arc::clone(&barrier);
        let slow_generator = move |_cwd: &str, _pane_id: &str| -> Option<String> {
            barrier_clone.wait();
            Some("generated context".to_string())
        };
        let mut mgr = ContextManager::with_generator(slow_generator);

        let windows = vec![win(1, "active-session"), win(2, "new-session")];
        let states: HashMap<u32, WindowState> = [(1, WindowState::Idle), (2, WindowState::Fresh)]
            .into_iter()
            .collect();
        let panes: HashMap<u32, String> =
            [(1, "%0".into()), (2, "%1".into())].into_iter().collect();

        // Tick 1: active-session selected — starts context generation (blocked on barrier)
        mgr.tick(&windows, &states, 0, &pane_ids(&panes));
        // Context is in-flight but blocked, so is_loading should be true
        assert!(
            mgr.is_loading("active-session"),
            "should be loading while generator is running"
        );

        // Tick 2: switch to new-session — triggers refresh for active-session
        // But active-session is still in-flight (blocked), so refresh is a no-op
        mgr.tick(&windows, &states, 1, &pane_ids(&panes));

        // Tick 3: switch back to active-session — should still show loading
        mgr.tick(&windows, &states, 0, &pane_ids(&panes));
        assert!(
            mgr.is_loading("active-session"),
            "should still be loading when switching back"
        );
        assert!(
            mgr.get("active-session").is_none(),
            "context should not be available yet"
        );

        // Unblock the generator
        barrier.wait();
        drain_with_wait(&mut mgr);

        // Now context should be available
        assert!(
            !mgr.is_loading("active-session"),
            "should not be loading after drain"
        );
        assert_eq!(
            mgr.get("active-session"),
            Some("generated context"),
            "context should be populated after drain"
        );
    }
}
