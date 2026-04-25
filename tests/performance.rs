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
