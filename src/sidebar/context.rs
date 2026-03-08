// ── Background context generation for sessions ──
//
// Reads Claude session JSONL files directly, extracts conversation text,
// then calls a local Ollama instance to generate a summary.
// Results flow back via mpsc channel so the sidebar event loop never blocks.

use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::events;
use crate::paths;
use crate::sidebar::state::WindowState;
use crate::tmux::WindowInfo;

// ── Types ──

type GeneratorFn = Arc<dyn Fn(&str, &str) -> Result<String, String> + Send + Sync>;

pub struct ContextManager {
    contexts: HashMap<String, String>,
    in_flight: HashSet<String>,
    failed: HashMap<String, Instant>,
    /// Last error message per window — displayed in the UI when context generation fails.
    errors: HashMap<String, String>,
    tx: mpsc::Sender<(String, Result<String, String>)>,
    rx: mpsc::Receiver<(String, Result<String, String>)>,
    generator: GeneratorFn,
    prev_selected_name: Option<String>,
    /// Previous state per window — used to detect transitions and invalidate cache.
    prev_states: HashMap<String, WindowState>,
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

const API_TIMEOUT: Duration = Duration::from_secs(30);
const RETRY_COOLDOWN: Duration = Duration::from_secs(30);

// ── Diagnostic Logging ──

fn log_context(msg: &str) {
    let path = paths::state_dir().join("context.log");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = writeln!(f, "[{ts}] {msg}");
    }
}

// ── Public API ──

impl Default for ContextManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextManager {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            contexts: HashMap::new(),
            in_flight: HashSet::new(),
            failed: HashMap::new(),
            errors: HashMap::new(),
            tx,
            rx,
            generator: Arc::new(generate_context),
            prev_selected_name: None,
            prev_states: HashMap::new(),
        }
    }

    pub fn with_generator(
        generator_fn: impl Fn(&str, &str) -> Result<String, String> + Send + Sync + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            contexts: HashMap::new(),
            in_flight: HashSet::new(),
            failed: HashMap::new(),
            errors: HashMap::new(),
            tx,
            rx,
            generator: Arc::new(generator_fn),
            prev_selected_name: None,
            prev_states: HashMap::new(),
        }
    }

    /// Drain completed context results from background threads.
    pub fn drain(&mut self) {
        while let Ok((name, result)) = self.rx.try_recv() {
            self.in_flight.remove(&name);
            match result {
                Ok(context) => {
                    self.errors.remove(&name);
                    self.contexts.insert(name, context);
                }
                Err(reason) => {
                    log_context(&format!("failed for {name}: {reason}"));
                    self.errors.insert(name.clone(), reason);
                    self.failed.insert(name, Instant::now());
                }
            }
        }
    }

    /// Get the context for a window, if available.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.contexts.get(name).map(String::as_str)
    }

    /// Get the last error for a window, if generation failed.
    pub fn get_error(&self, name: &str) -> Option<&str> {
        self.errors.get(name).map(String::as_str)
    }

    /// Whether a context request is currently running for this window.
    pub fn is_loading(&self, name: &str) -> bool {
        self.in_flight.contains(name)
    }

    /// Run one tick of context orchestration.
    ///
    /// - Drains completed results from background threads
    /// - Invalidates cache on state transitions (Working → settled)
    /// - Requests context for the selected window (lazy — only when settled)
    /// - On selection change: refreshes old session (if settled), requests new session
    pub fn tick(
        &mut self,
        windows: &[WindowInfo],
        states: &HashMap<u32, WindowState>,
        selected: usize,
        pane_id_for: &impl Fn(u32) -> Option<String>,
        cwd_for: &impl Fn(u32) -> Option<String>,
    ) {
        // Drain completed context results first so they're available this tick
        self.drain();

        // Detect state transitions and invalidate stale context.
        // When a session transitions from Working to a settled state (Idle/Asking/Waiting),
        // the conversation has new content — clear cache so fresh context is generated.
        if let Some(win) = windows.get(selected) {
            let state = states
                .get(&win.index)
                .copied()
                .unwrap_or(WindowState::Fresh);
            let prev = self
                .prev_states
                .get(&win.name)
                .copied()
                .unwrap_or(WindowState::Fresh);

            if prev == WindowState::Working && is_settled(state) {
                self.contexts.remove(&win.name);
                self.failed.remove(&win.name);
            }

            self.prev_states.insert(win.name.clone(), state);
        }

        // Request context for the selected window only (lazy generation).
        // Only fire for settled states — Working is too early (conversation incomplete,
        // subprocess may fail), and failures trigger a 30s retry cooldown that blocks
        // subsequent attempts when the session actually reaches Idle.
        if let Some(win) = windows.get(selected) {
            let state = states
                .get(&win.index)
                .copied()
                .unwrap_or(WindowState::Fresh);
            if is_settled(state) {
                let pane_id = pane_id_for(win.index).unwrap_or_default();
                let cwd = cwd_for(win.index).unwrap_or_else(|| win.pane_path.clone());
                self.request(&win.name, &cwd, &pane_id);
            }
        }

        // Track selection changes and manage context generation
        let current_name = windows.get(selected).map(|w| w.name.clone());
        if current_name != self.prev_selected_name {
            // Refresh context for old session — only if it's in a settled state
            if let Some(ref prev_name) = self.prev_selected_name {
                if let Some(prev_win) = windows.iter().find(|w| w.name == *prev_name) {
                    let state = states
                        .get(&prev_win.index)
                        .copied()
                        .unwrap_or(WindowState::Fresh);
                    if is_settled(state) {
                        let pane_id = pane_id_for(prev_win.index).unwrap_or_default();
                        let cwd =
                            cwd_for(prev_win.index).unwrap_or_else(|| prev_win.pane_path.clone());
                        self.refresh(&prev_win.name, &cwd, &pane_id);
                    }
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

    /// Force-refresh context for a window (clears cache/failed/errors, respects in_flight).
    pub fn refresh(&mut self, name: &str, cwd: &str, pane_id: &str) {
        if self.in_flight.contains(name) {
            return;
        }
        self.contexts.remove(name);
        self.failed.remove(name);
        self.errors.remove(name);
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
            let result = generator(&cwd, &pane_id);
            let _ = tx.send((name, result));
        });
    }
}

/// A settled state means Claude has finished a turn and the conversation has content
/// worth summarizing. Only generate context in these states — not during Working
/// (conversation incomplete) or Fresh/Done (no activity / session ended).
fn is_settled(state: WindowState) -> bool {
    matches!(
        state,
        WindowState::Idle | WindowState::Asking | WindowState::Waiting
    )
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
    let session_id = match events::find_session_id(pane_id) {
        Some(id) => id,
        None => {
            log_context(&format!("no session_id for pane_id={pane_id}"));
            return None;
        }
    };
    let project_dir = claude_project_dir(cwd);
    let path = project_dir.join(format!("{session_id}.jsonl"));
    if path.exists() {
        Some(path)
    } else {
        log_context(&format!("JSONL not found: {}", path.display()));
        None
    }
}

// ── JSONL Parsing ──

/// Extract conversation text from a Claude session JSONL file.
/// Returns a compact representation of user/assistant messages, truncated
/// to fit within CONVERSATION_BUDGET.
fn extract_conversation(path: &Path) -> Option<String> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            log_context(&format!(
                "read session file failed: {}: {e}",
                path.display()
            ));
            return None;
        }
    };
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

fn generate_context(cwd: &str, pane_id: &str) -> Result<String, String> {
    let session_path = find_session_file(cwd, pane_id)
        .ok_or_else(|| format!("no session file: cwd={cwd} pane_id={pane_id}"))?;
    let conversation = extract_conversation(&session_path)
        .ok_or_else(|| format!("no conversation: path={}", session_path.display()))?;

    let prompt = format!("{SUMMARY_PROMPT}\n\nConversation:\n{conversation}");
    call_ollama(&prompt)
}

const OLLAMA_DEFAULT_MODEL: &str = "llama3.2";

fn call_ollama(prompt: &str) -> Result<String, String> {
    let model =
        std::env::var("COVE_OLLAMA_MODEL").unwrap_or_else(|_| OLLAMA_DEFAULT_MODEL.to_string());

    let config = ureq::Agent::config_builder()
        .timeout_global(Some(API_TIMEOUT))
        .build();
    let agent: ureq::Agent = config.into();

    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "stream": false
    });

    let mut response = agent
        .post("http://localhost:11434/api/chat")
        .send_json(&body)
        .map_err(|e| {
            if e.to_string().contains("Connection refused") {
                "Ollama not connected".to_string()
            } else {
                format!("Ollama request failed: {e}")
            }
        })?;

    let response_text = response
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("read response failed: {e}"))?;

    let parsed: serde_json::Value =
        serde_json::from_str(&response_text).map_err(|e| format!("parse response failed: {e}"))?;

    let text = parsed
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|t| t.as_str())
        .ok_or_else(|| "no text in Ollama response".to_string())?;

    let text = text.trim().to_string();
    if text.is_empty() {
        Err("Ollama returned empty text".to_string())
    } else {
        Ok(text)
    }
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
        impl Fn(&str, &str) -> Result<String, String> + Send + Sync + 'static,
        Arc<Mutex<Vec<(String, String)>>>,
    ) {
        let calls: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let calls_clone = Arc::clone(&calls);
        let generator = move |cwd: &str, pane_id: &str| -> Result<String, String> {
            calls_clone
                .lock()
                .unwrap()
                .push((cwd.to_string(), pane_id.to_string()));
            Ok(format!("context for {pane_id}"))
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

    fn no_cwd(_idx: u32) -> Option<String> {
        None
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
        mgr.tick(&windows, &states, 0, &pane_ids(&panes), &no_cwd);

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
        mgr.tick(&windows, &states, 0, &pane_ids(&panes), &no_cwd);
        assert!(calls.lock().unwrap().is_empty());

        // Tick 2: switch to session-2 (also Fresh)
        mgr.tick(&windows, &states, 1, &pane_ids(&panes), &no_cwd);

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
        mgr.tick(&windows, &states, 0, &pane_ids(&panes), &no_cwd);
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
        mgr.tick(&windows, &states, 1, &pane_ids(&panes), &no_cwd);
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
        let slow_generator = move |_cwd: &str, _pane_id: &str| -> Result<String, String> {
            barrier_clone.wait();
            Ok("generated context".to_string())
        };
        let mut mgr = ContextManager::with_generator(slow_generator);

        let windows = vec![win(1, "active-session"), win(2, "new-session")];
        let states: HashMap<u32, WindowState> = [(1, WindowState::Idle), (2, WindowState::Fresh)]
            .into_iter()
            .collect();
        let panes: HashMap<u32, String> =
            [(1, "%0".into()), (2, "%1".into())].into_iter().collect();

        // Tick 1: active-session selected — starts context generation (blocked on barrier)
        mgr.tick(&windows, &states, 0, &pane_ids(&panes), &no_cwd);
        // Context is in-flight but blocked, so is_loading should be true
        assert!(
            mgr.is_loading("active-session"),
            "should be loading while generator is running"
        );

        // Tick 2: switch to new-session — triggers refresh for active-session
        // But active-session is still in-flight (blocked), so refresh is a no-op
        mgr.tick(&windows, &states, 1, &pane_ids(&panes), &no_cwd);

        // Tick 3: switch back to active-session — should still show loading
        mgr.tick(&windows, &states, 0, &pane_ids(&panes), &no_cwd);
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

    // ── Scenario 5: Working state should NOT fire context generation ──
    //
    // During Working state, the conversation is incomplete — Claude hasn't
    // responded yet. Firing the generator here risks failure (session file not
    // ready) which triggers a 30s retry cooldown, blocking context generation
    // when the session eventually reaches Idle.
    #[test]
    fn test_working_state_does_not_fire_context() {
        let (generator, calls) = mock_generator();
        let mut mgr = ContextManager::with_generator(generator);

        let windows = vec![win(1, "session-1")];
        let states: HashMap<u32, WindowState> = [(1, WindowState::Working)].into_iter().collect();
        let panes: HashMap<u32, String> = [(1, "%0".into())].into_iter().collect();

        // Tick with Working state selected
        mgr.tick(&windows, &states, 0, &pane_ids(&panes), &no_cwd);

        // Generator should NOT fire during Working
        assert!(
            calls.lock().unwrap().is_empty(),
            "should not fire context during Working"
        );
        assert!(!mgr.is_loading("session-1"));
        assert!(mgr.get("session-1").is_none());
    }

    // ── Scenario 6: Full user flow — Fresh → Working → Idle ──
    //
    // Simulates the exact user experience:
    // 1. Session starts (Fresh) → no context
    // 2. User asks a question (Working) → no context fires
    // 3. Claude responds (Idle) → context fires and resolves
    #[test]
    fn test_full_user_flow_fresh_working_idle() {
        let (generator, calls) = mock_generator();
        let mut mgr = ContextManager::with_generator(generator);

        let windows = vec![win(1, "my-session")];
        let panes: HashMap<u32, String> = [(1, "%0".into())].into_iter().collect();

        // Phase 1: Fresh — no context activity
        let states_fresh: HashMap<u32, WindowState> =
            [(1, WindowState::Fresh)].into_iter().collect();
        mgr.tick(&windows, &states_fresh, 0, &pane_ids(&panes), &no_cwd);
        assert!(calls.lock().unwrap().is_empty(), "Fresh: no generator call");
        assert!(mgr.get("my-session").is_none(), "Fresh: no context");
        assert!(!mgr.is_loading("my-session"), "Fresh: not loading");

        // Phase 2: Working — still no context activity
        let states_working: HashMap<u32, WindowState> =
            [(1, WindowState::Working)].into_iter().collect();
        mgr.tick(&windows, &states_working, 0, &pane_ids(&panes), &no_cwd);
        assert!(
            calls.lock().unwrap().is_empty(),
            "Working: no generator call"
        );
        assert!(mgr.get("my-session").is_none(), "Working: no context");
        assert!(!mgr.is_loading("my-session"), "Working: not loading");

        // Phase 3: Idle — context fires
        let states_idle: HashMap<u32, WindowState> = [(1, WindowState::Idle)].into_iter().collect();
        mgr.tick(&windows, &states_idle, 0, &pane_ids(&panes), &no_cwd);

        // spawn() sets in_flight synchronously before the thread runs
        assert!(
            mgr.is_loading("my-session"),
            "Idle: loading while in-flight"
        );

        // Let background thread complete and drain results
        drain_with_wait(&mut mgr);

        // Generator should have been called (check after drain — calls is populated in thread)
        assert_eq!(
            calls.lock().unwrap().len(),
            1,
            "Idle: generator called once"
        );

        // Context should now be available
        assert!(
            !mgr.is_loading("my-session"),
            "Idle: not loading after drain"
        );
        assert_eq!(
            mgr.get("my-session"),
            Some("context for %0"),
            "Idle: context populated"
        );
    }

    // ── Scenario 7: State transition invalidates stale cache ──
    //
    // After context is cached for a session, a new Working → Idle transition
    // should invalidate the cache so fresh context is generated.
    #[test]
    fn test_working_to_idle_invalidates_cache() {
        let (generator, calls) = mock_generator();
        let mut mgr = ContextManager::with_generator(generator);

        let windows = vec![win(1, "my-session")];
        let panes: HashMap<u32, String> = [(1, "%0".into())].into_iter().collect();

        // First round: Idle → generates and caches context
        let states_idle: HashMap<u32, WindowState> = [(1, WindowState::Idle)].into_iter().collect();
        mgr.tick(&windows, &states_idle, 0, &pane_ids(&panes), &no_cwd);
        drain_with_wait(&mut mgr);
        assert_eq!(mgr.get("my-session"), Some("context for %0"));
        assert_eq!(calls.lock().unwrap().len(), 1);

        // Second round: user asks another question → Working
        let states_working: HashMap<u32, WindowState> =
            [(1, WindowState::Working)].into_iter().collect();
        mgr.tick(&windows, &states_working, 0, &pane_ids(&panes), &no_cwd);
        // Context still cached from first round (not cleared during Working)
        assert_eq!(mgr.get("my-session"), Some("context for %0"));

        // Third round: Claude responds → Idle again
        // The Working → Idle transition should invalidate the cache
        calls.lock().unwrap().clear();
        mgr.tick(&windows, &states_idle, 0, &pane_ids(&panes), &no_cwd);

        // spawn() is synchronous — should be in-flight (cache was cleared by transition)
        assert!(
            mgr.is_loading("my-session"),
            "should re-request after Working → Idle transition"
        );

        drain_with_wait(&mut mgr);

        // Generator should have been called again
        assert_eq!(
            calls.lock().unwrap().len(),
            1,
            "should have called generator after Working → Idle transition"
        );
        assert_eq!(
            mgr.get("my-session"),
            Some("context for %0"),
            "fresh context should be available"
        );
    }

    // ── Scenario 8: Failed generator during Idle retries correctly ──
    //
    // Verifies the 30s cooldown behavior: if the generator fails during Idle,
    // the retry cooldown prevents immediate re-requests (as expected).
    // But a state transition (Working → Idle) should clear the cooldown.
    #[test]
    fn test_failed_generator_cooldown_cleared_on_transition() {
        let call_count = Arc::new(Mutex::new(0u32));
        let call_count_clone = Arc::clone(&call_count);
        // Generator fails on first call, succeeds on second
        let generator = move |_cwd: &str, _pane_id: &str| -> Result<String, String> {
            let mut count = call_count_clone.lock().unwrap();
            *count += 1;
            if *count == 1 {
                Err("simulated failure".to_string())
            } else {
                Ok("generated context".to_string())
            }
        };
        let mut mgr = ContextManager::with_generator(generator);

        let windows = vec![win(1, "my-session")];
        let panes: HashMap<u32, String> = [(1, "%0".into())].into_iter().collect();

        // Tick 1: Idle → generator fires and fails → enters 30s cooldown
        let states_idle: HashMap<u32, WindowState> = [(1, WindowState::Idle)].into_iter().collect();
        mgr.tick(&windows, &states_idle, 0, &pane_ids(&panes), &no_cwd);
        drain_with_wait(&mut mgr);
        assert!(mgr.get("my-session").is_none(), "first attempt should fail");
        assert_eq!(*call_count.lock().unwrap(), 1);

        // Tick 2: Still Idle → cooldown blocks retry
        mgr.tick(&windows, &states_idle, 0, &pane_ids(&panes), &no_cwd);
        drain_with_wait(&mut mgr);
        assert_eq!(
            *call_count.lock().unwrap(),
            1,
            "cooldown should block retry"
        );

        // Simulate new activity: Working → Idle transition clears cooldown
        let states_working: HashMap<u32, WindowState> =
            [(1, WindowState::Working)].into_iter().collect();
        mgr.tick(&windows, &states_working, 0, &pane_ids(&panes), &no_cwd);
        mgr.tick(&windows, &states_idle, 0, &pane_ids(&panes), &no_cwd);
        drain_with_wait(&mut mgr);

        // Should have retried and succeeded
        assert_eq!(
            *call_count.lock().unwrap(),
            2,
            "should retry after state transition"
        );
        assert_eq!(
            mgr.get("my-session"),
            Some("generated context"),
            "second attempt should succeed"
        );
    }
}
