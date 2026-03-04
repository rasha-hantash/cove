// ── Performance tests ──
//
// Wall-clock latency assertions to catch O(n) regressions.
// Thresholds are generous — the goal is catching order-of-magnitude
// regressions, not microbenchmarking.

mod helpers;

use std::time::Instant;

use cove_cli::sidebar::state;

#[test]
fn state_detection_many_sessions() {
    let dir = tempfile::tempdir().unwrap();

    // Create 20 event files with different pane_ids
    for i in 0..20 {
        helpers::write_event_sequence(
            dir.path(),
            &format!("session-{i}"),
            &[
                ("working", "/project", &format!("%{i}"), 1000 + i as u64),
                ("idle", "/project", &format!("%{i}"), 1001 + i as u64),
            ],
        );
    }

    let start = Instant::now();
    let events = state::load_latest_events(dir.path());
    let elapsed = start.elapsed();

    assert_eq!(events.len(), 20);
    assert!(
        elapsed.as_millis() < 10,
        "load_latest_events with 20 files took {}ms (threshold: 10ms)",
        elapsed.as_millis()
    );
}

#[test]
fn state_detection_large_file() {
    let dir = tempfile::tempdir().unwrap();

    // Write 10,000 events to a single file
    for i in 0..10_000 {
        helpers::write_event_line(dir.path(), "big-session", "working", "/project", "%0", i);
    }
    // Final event is idle
    helpers::write_event_line(dir.path(), "big-session", "idle", "/project", "%0", 10_000);

    let path = dir.path().join("big-session.jsonl");

    let start = Instant::now();
    let line = state::read_last_line(&path);
    let elapsed = start.elapsed();

    assert!(line.is_some());
    assert!(line.unwrap().contains(r#""state":"idle""#));
    assert!(
        elapsed.as_millis() < 5,
        "read_last_line on 10K-line file took {}ms (threshold: 5ms)",
        elapsed.as_millis()
    );
}

#[test]
fn context_tick_latency() {
    use cove_cli::sidebar::context::ContextManager;
    use cove_cli::sidebar::state::WindowState;
    use cove_cli::tmux::WindowInfo;
    use std::collections::HashMap;

    // Mock generator that returns instantly
    let mgr_generator = |_cwd: &str, _pane_id: &str| -> Option<String> { None };
    let mut mgr = ContextManager::with_generator(mgr_generator);

    // Build 10 windows, all Fresh (no context generation should fire)
    let windows: Vec<WindowInfo> = (0..10)
        .map(|i| WindowInfo {
            index: i,
            name: format!("session-{i}"),
            is_active: i == 0,
            pane_path: format!("/project/{i}"),
        })
        .collect();
    let states: HashMap<u32, WindowState> = (0..10).map(|i| (i, WindowState::Fresh)).collect();
    let panes: HashMap<u32, String> = (0..10).map(|i| (i, format!("%{i}"))).collect();

    let start = Instant::now();
    mgr.tick(&windows, &states, 0, &|idx| panes.get(&idx).cloned());
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 1,
        "context tick with 10 Fresh windows took {}ms (threshold: 1ms)",
        elapsed.as_millis()
    );
}
