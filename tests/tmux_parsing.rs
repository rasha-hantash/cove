// ── tmux output parsing tests ──
//
// Tests parse_window_list() and parse_pane_list() with synthetic tmux output.
// No actual tmux process needed.

mod helpers;

use cove_cli::tmux;

#[test]
fn parse_window_list_basic() {
    let output = helpers::fake_window_output(&[
        (0, "cove-main", true, "/Users/dev/cove"),
        (1, "api-server", false, "/Users/dev/api"),
        (2, "frontend", false, "/Users/dev/web"),
    ]);

    let windows = tmux::parse_window_list(&output);
    assert_eq!(windows.len(), 3);

    assert_eq!(windows[0].index, 0);
    assert_eq!(windows[0].name, "cove-main");
    assert!(windows[0].is_active);
    assert_eq!(windows[0].pane_path, "/Users/dev/cove");

    assert_eq!(windows[1].index, 1);
    assert_eq!(windows[1].name, "api-server");
    assert!(!windows[1].is_active);

    assert_eq!(windows[2].index, 2);
    assert_eq!(windows[2].name, "frontend");
}

#[test]
fn parse_window_list_base_index_1() {
    // User has base-index 1 — indexes start at 1, not 0
    let output = helpers::fake_window_output(&[
        (1, "session-a", true, "/project/a"),
        (2, "session-b", false, "/project/b"),
        (3, "session-c", false, "/project/c"),
    ]);

    let windows = tmux::parse_window_list(&output);
    assert_eq!(windows.len(), 3);
    assert_eq!(windows[0].index, 1);
    assert_eq!(windows[1].index, 2);
    assert_eq!(windows[2].index, 3);
}

#[test]
fn parse_window_list_pipe_in_path() {
    // Path containing | shouldn't break splitn(4, '|')
    // splitn(4, ..) means the 4th part gets everything remaining
    let output = "0|test|1|/path/with|pipes|in|it";

    let windows = tmux::parse_window_list(output);
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].pane_path, "/path/with|pipes|in|it");
}

#[test]
fn parse_pane_list_filters_pane_1() {
    let output = helpers::fake_pane_output(&[
        (1, 0, "cove", "%0"),   // sidebar pane — should be filtered out
        (1, 1, "claude", "%1"), // Claude pane — should be kept
        (1, 2, "zsh", "%2"),    // terminal pane — should be filtered out
        (2, 0, "cove", "%3"),
        (2, 1, "claude", "%4"), // kept
        (2, 2, "zsh", "%5"),
    ]);

    let panes = tmux::parse_pane_list(&output);
    assert_eq!(panes.len(), 2);
    assert_eq!(panes[0].window_index, 1);
    assert_eq!(panes[0].pane_id, "%1");
    assert_eq!(panes[1].window_index, 2);
    assert_eq!(panes[1].pane_id, "%4");
}

#[test]
fn parse_pane_list_extracts_pane_id() {
    let output = helpers::fake_pane_output(&[(3, 1, "node", "%42")]);

    let panes = tmux::parse_pane_list(&output);
    assert_eq!(panes.len(), 1);
    assert_eq!(panes[0].pane_id, "%42");
    assert_eq!(panes[0].command, "node");
    assert_eq!(panes[0].window_index, 3);
}

#[test]
fn parse_empty_output() {
    assert!(tmux::parse_window_list("").is_empty());
    assert!(tmux::parse_pane_list("").is_empty());
}
