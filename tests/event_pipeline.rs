// ── Cross-module event flow tests ──
//
// Tests the pipeline: write_event_to() → load_latest_events() / find_session_id_in()
// Verifies events written by the hook handler are correctly read by the sidebar.

mod helpers;

use cove_cli::events;
use cove_cli::sidebar::state;

#[test]
fn write_then_read_last_event() {
    let dir = tempfile::tempdir().unwrap();

    // Write 3 events for the same session/pane
    events::write_event_to(dir.path(), "session-a", "/project", "%0", "working").unwrap();
    events::write_event_to(dir.path(), "session-a", "/project", "%0", "waiting").unwrap();
    events::write_event_to(dir.path(), "session-a", "/project", "%0", "idle").unwrap();

    let latest = state::load_latest_events(dir.path());
    assert_eq!(latest.get("%0").map(String::as_str), Some("idle"));
}

#[test]
fn pane_id_dedup_across_files() {
    let dir = tempfile::tempdir().unwrap();

    // Two files with the same pane_id but different timestamps
    helpers::write_event_line(dir.path(), "old-session", "asking", "/old", "%5", 1000);
    helpers::write_event_line(dir.path(), "new-session", "working", "/new", "%5", 2000);

    let latest = state::load_latest_events(dir.path());
    assert_eq!(latest.len(), 1);
    // Higher timestamp wins
    assert_eq!(latest["%5"], "working");
}

#[test]
fn purge_then_fresh_state() {
    let dir = tempfile::tempdir().unwrap();

    // Write events for pane %3
    helpers::write_event_line(dir.path(), "session-x", "working", "/project", "%3", 1000);
    helpers::write_event_line(dir.path(), "session-x", "idle", "/project", "%3", 1001);

    // Also write events for pane %0 (should survive purge)
    helpers::write_event_line(dir.path(), "session-y", "asking", "/other", "%0", 2000);

    // Purge pane %3
    state::purge_events_for_pane_in(dir.path(), "%3");

    let latest = state::load_latest_events(dir.path());
    assert!(latest.get("%3").is_none(), "purged pane should be gone");
    assert_eq!(latest.get("%0").map(String::as_str), Some("asking"));
}

#[test]
fn session_end_event() {
    let dir = tempfile::tempdir().unwrap();

    helpers::write_event_sequence(
        dir.path(),
        "session-end-test",
        &[
            ("working", "/project", "%1", 1000),
            ("idle", "/project", "%1", 1001),
            ("end", "/project", "%1", 1002),
        ],
    );

    let latest = state::load_latest_events(dir.path());
    assert_eq!(latest["%1"], "end");
}

#[test]
fn find_session_id_by_pane() {
    let dir = tempfile::tempdir().unwrap();

    // 3 sessions with different pane_ids
    helpers::write_event_line(dir.path(), "sess-aaa", "working", "/a", "%0", 1000);
    helpers::write_event_line(dir.path(), "sess-bbb", "idle", "/b", "%3", 1000);
    helpers::write_event_line(dir.path(), "sess-ccc", "asking", "/c", "%5", 1000);

    assert_eq!(
        events::find_session_id_in(dir.path(), "%3"),
        Some("sess-bbb".to_string())
    );
    assert_eq!(
        events::find_session_id_in(dir.path(), "%0"),
        Some("sess-aaa".to_string())
    );
    assert_eq!(
        events::find_session_id_in(dir.path(), "%5"),
        Some("sess-ccc".to_string())
    );
    assert_eq!(events::find_session_id_in(dir.path(), "%99"), None);
}

#[test]
fn find_session_id_recycled_pane() {
    let dir = tempfile::tempdir().unwrap();

    // Two files with the same pane_id — most recent session should win
    helpers::write_event_line(dir.path(), "old-sess", "idle", "/old", "%2", 1000);
    helpers::write_event_line(dir.path(), "new-sess", "working", "/new", "%2", 2000);

    let found = events::find_session_id_in(dir.path(), "%2").unwrap();
    assert_eq!(found, "new-sess");
}

#[test]
fn idle_suppression_before_working() {
    let dir = tempfile::tempdir().unwrap();

    // Only an idle event (no working event yet) — has_working_event should be false
    helpers::write_event_line(dir.path(), "fresh-sess", "idle", "/project", "%0", 1000);

    let path = dir.path().join("fresh-sess.jsonl");
    let content = std::fs::read_to_string(&path).unwrap();
    let has_working = content
        .lines()
        .any(|line| line.contains(r#""state":"working""#));
    assert!(!has_working, "no working event should exist yet");

    // After adding a working event, it should be found
    helpers::write_event_line(dir.path(), "fresh-sess", "working", "/project", "%0", 1001);
    let content = std::fs::read_to_string(&path).unwrap();
    let has_working = content
        .lines()
        .any(|line| line.contains(r#""state":"working""#));
    assert!(has_working, "working event should now exist");
}
