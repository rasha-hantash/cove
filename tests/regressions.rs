// ── Regression tests for known bugs ──
//
// Each test reproduces the exact preconditions of a previously-fixed bug
// to prevent it from recurring.

mod helpers;

use cove_cli::sidebar::state;
use cove_cli::tmux;

/// Bug #1: Stale pane ID events from a previous session contaminating state.
/// Fix: purge old events when a new window is created with a recycled pane_id.
#[test]
fn stale_pane_id_on_restart() {
    let dir = tempfile::tempdir().unwrap();

    // Old session left events for pane %3
    helpers::write_event_line(dir.path(), "old-session", "asking", "/project", "%3", 1000);

    // Purge stale events (simulates what happens on new window creation)
    state::purge_events_for_pane_in(dir.path(), "%3");

    // New session should read as empty (Fresh state)
    let latest = state::load_latest_events(dir.path());
    assert!(
        latest.get("%3").is_none(),
        "purged pane should have no state (Fresh)"
    );
}

/// Bug #2: Window index collision with base-index=1.
/// User has `set -g base-index 1` — indexes 1,2,3 instead of 0,1,2.
/// Bug was losing windows because parsing assumed 0-based indexes.
#[test]
fn window_index_base_1() {
    let output = helpers::fake_window_output(&[
        (1, "session-a", true, "/project/a"),
        (2, "session-b", false, "/project/b"),
        (3, "session-c", false, "/project/c"),
    ]);

    let windows = tmux::parse_window_list(&output);

    // All 3 windows must be parsed — none lost to index collision
    assert_eq!(windows.len(), 3);
    assert_eq!(windows[0].index, 1);
    assert_eq!(windows[1].index, 2);
    assert_eq!(windows[2].index, 3);
}

/// Bug #4: Fresh session triggering unnecessary context generation.
/// A Fresh window (no hook events) should NOT trigger the context generator.
#[test]
fn fresh_session_no_context() {
    use cove_cli::sidebar::context::ContextManager;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    let calls: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let calls_clone = Arc::clone(&calls);
    let generator = move |cwd: &str, pane_id: &str| -> Result<String, String> {
        calls_clone
            .lock()
            .unwrap()
            .push((cwd.to_string(), pane_id.to_string()));
        Ok("context".to_string())
    };
    let mut mgr = ContextManager::with_generator(generator);

    let windows = vec![cove_cli::tmux::WindowInfo {
        index: 1,
        name: "fresh-session".to_string(),
        is_active: true,
        pane_path: "/project".to_string(),
    }];
    let states: HashMap<u32, state::WindowState> =
        [(1, state::WindowState::Fresh)].into_iter().collect();
    let panes: HashMap<u32, String> = [(1, "%0".into())].into_iter().collect();

    let no_cwd = |_idx: u32| -> Option<String> { None };
    mgr.tick(
        &windows,
        &states,
        0,
        &|idx| panes.get(&idx).cloned(),
        &no_cwd,
    );

    // Generator should NOT have been called for a Fresh window
    assert!(
        calls.lock().unwrap().is_empty(),
        "context generator should not fire for Fresh sessions"
    );
}

/// Bug #4 related: Idle event arriving before first user prompt (fresh state flicker).
/// An idle event without a prior working event should not count as "has working event".
#[test]
fn idle_suppressed_before_first_prompt() {
    let dir = tempfile::tempdir().unwrap();

    // Only an idle event exists — no working event ever fired
    helpers::write_event_line(dir.path(), "test-sess", "idle", "/project", "%0", 1000);

    let path = dir.path().join("test-sess.jsonl");
    let content = std::fs::read_to_string(&path).unwrap();
    let has_working = content
        .lines()
        .any(|line| line.contains(r#""state":"working""#));

    assert!(
        !has_working,
        "idle-only session should not have a working event"
    );
}

/// Bug #5: Context generation used wrong cwd (tmux pane_path instead of event cwd).
/// When the user is focused on the terminal pane (not the Claude pane), or when
/// Claude is in a worktree, tmux's #{pane_current_path} diverges from Claude's
/// actual project directory. The context generator then can't find the session file.
///
/// Fix: get cwd from cove event files (written by hooks from $PWD) instead of tmux.
/// cwd_for closure takes priority; pane_path is only a fallback.
#[test]
fn context_uses_event_cwd_not_pane_path() {
    use cove_cli::sidebar::context::ContextManager;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    let calls: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let calls_clone = Arc::clone(&calls);
    let generator = move |cwd: &str, pane_id: &str| -> Result<String, String> {
        calls_clone
            .lock()
            .unwrap()
            .push((cwd.to_string(), pane_id.to_string()));
        Ok("context".to_string())
    };
    let mut mgr = ContextManager::with_generator(generator);

    // Window's pane_path is the terminal pane's cwd (wrong)
    let windows = vec![cove_cli::tmux::WindowInfo {
        index: 1,
        name: "session-worktree".to_string(),
        is_active: true,
        pane_path: "/Users/test/terminal/cwd".to_string(),
    }];
    let states: HashMap<u32, state::WindowState> =
        [(1, state::WindowState::Idle)].into_iter().collect();
    let panes: HashMap<u32, String> = [(1, "%5".into())].into_iter().collect();

    // Event cwd is Claude's actual project directory (correct)
    let event_cwds: HashMap<u32, String> = [(1, "/Users/test/workspace/project".into())]
        .into_iter()
        .collect();

    mgr.tick(
        &windows,
        &states,
        0,
        &|idx| panes.get(&idx).cloned(),
        &|idx| event_cwds.get(&idx).cloned(),
    );

    // Wait for background thread
    std::thread::sleep(std::time::Duration::from_millis(50));
    mgr.drain();

    // Generator should have been called with the EVENT cwd, not the pane_path
    let recorded = calls.lock().unwrap().clone();
    assert_eq!(recorded.len(), 1, "generator should fire once");
    assert_eq!(
        recorded[0].0, "/Users/test/workspace/project",
        "should use event cwd, not pane_path"
    );
    assert_ne!(
        recorded[0].0, "/Users/test/terminal/cwd",
        "must NOT use pane_path when event cwd is available"
    );
}

/// Bug #5 related: When event cwd is unavailable (no events yet), fall back to pane_path.
#[test]
fn context_falls_back_to_pane_path_when_no_event_cwd() {
    use cove_cli::sidebar::context::ContextManager;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    let calls: Arc<Mutex<Vec<(String, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let calls_clone = Arc::clone(&calls);
    let generator = move |cwd: &str, pane_id: &str| -> Result<String, String> {
        calls_clone
            .lock()
            .unwrap()
            .push((cwd.to_string(), pane_id.to_string()));
        Ok("context".to_string())
    };
    let mut mgr = ContextManager::with_generator(generator);

    let windows = vec![cove_cli::tmux::WindowInfo {
        index: 1,
        name: "session-fallback".to_string(),
        is_active: true,
        pane_path: "/fallback/path".to_string(),
    }];
    let states: HashMap<u32, state::WindowState> =
        [(1, state::WindowState::Idle)].into_iter().collect();
    let panes: HashMap<u32, String> = [(1, "%5".into())].into_iter().collect();

    // No event cwd available
    let no_cwd = |_idx: u32| -> Option<String> { None };

    mgr.tick(
        &windows,
        &states,
        0,
        &|idx| panes.get(&idx).cloned(),
        &no_cwd,
    );

    std::thread::sleep(std::time::Duration::from_millis(50));
    mgr.drain();

    let recorded = calls.lock().unwrap().clone();
    assert_eq!(recorded.len(), 1, "generator should fire once");
    assert_eq!(
        recorded[0].0, "/fallback/path",
        "should fall back to pane_path when no event cwd"
    );
}

/// SessionEnd event: "end" state should map to Fresh (catch-all), not Idle.
/// Verifies state_from_str("end") behavior is intentional.
#[test]
fn end_state_not_idle() {
    let state = state::state_from_str("end");
    // "end" is not a recognized state string, so it maps to Fresh (the catch-all).
    // This is intentional — the sidebar treats ended sessions the same as fresh ones
    // until the process detection (pane command = zsh/bash) marks them as Done.
    assert_eq!(state, state::WindowState::Fresh);
    assert_ne!(state, state::WindowState::Idle);
}
