// ── System tests (require tmux) ──
//
// These tests create real tmux sessions and verify end-to-end behavior.
// They are #[ignore]d so they never run in CI.
// Run locally with: cargo test -- --ignored

use std::process::Command;
use std::time::Instant;

const TEST_SESSION: &str = "cove-test-system";

/// Helper: run a tmux command and return stdout.
fn tmux_run(args: &[&str]) -> String {
    let output = Command::new("tmux")
        .args(args)
        .output()
        .expect("tmux not found");
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Helper: check if tmux is available.
fn tmux_available() -> bool {
    Command::new("tmux")
        .args(["-V"])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Helper: clean up the test session.
fn cleanup() {
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", TEST_SESSION])
        .output();
}

#[test]
#[ignore]
fn tmux_create_and_list_windows() {
    if !tmux_available() {
        eprintln!("tmux not available, skipping");
        return;
    }
    cleanup();

    // Create a detached session with 3 windows
    let _ = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            TEST_SESSION,
            "-n",
            "win-a",
            ";",
            "new-window",
            "-t",
            TEST_SESSION,
            "-n",
            "win-b",
            ";",
            "new-window",
            "-t",
            TEST_SESSION,
            "-n",
            "win-c",
        ])
        .output()
        .expect("failed to create test session");

    let out = tmux_run(&[
        "list-windows",
        "-t",
        TEST_SESSION,
        "-F",
        "#{window_index}|#{window_name}|#{window_active}|#{pane_current_path}",
    ]);

    let windows = cove_cli::tmux::parse_window_list(&out);
    assert!(
        windows.len() >= 3,
        "expected 3 windows, got {}",
        windows.len()
    );

    let names: Vec<&str> = windows.iter().map(|w| w.name.as_str()).collect();
    assert!(names.contains(&"win-a"));
    assert!(names.contains(&"win-b"));
    assert!(names.contains(&"win-c"));

    cleanup();
}

#[test]
#[ignore]
fn tmux_select_window_round_trip() {
    if !tmux_available() {
        eprintln!("tmux not available, skipping");
        return;
    }
    cleanup();

    let _ = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            TEST_SESSION,
            "-n",
            "first",
            ";",
            "new-window",
            "-t",
            TEST_SESSION,
            "-n",
            "second",
            ";",
            "new-window",
            "-t",
            TEST_SESSION,
            "-n",
            "third",
        ])
        .output()
        .expect("failed to create test session");

    // Select the second window by index
    let out = tmux_run(&[
        "list-windows",
        "-t",
        TEST_SESSION,
        "-F",
        "#{window_index}|#{window_name}|#{window_active}|#{pane_current_path}",
    ]);
    let windows = cove_cli::tmux::parse_window_list(&out);
    let second = windows.iter().find(|w| w.name == "second").unwrap();

    let _ = Command::new("tmux")
        .args([
            "select-window",
            "-t",
            &format!("{TEST_SESSION}:{}", second.index),
        ])
        .output();

    // Verify it's now active
    let out = tmux_run(&[
        "list-windows",
        "-t",
        TEST_SESSION,
        "-F",
        "#{window_index}|#{window_name}|#{window_active}|#{pane_current_path}",
    ]);
    let windows = cove_cli::tmux::parse_window_list(&out);
    let active = windows.iter().find(|w| w.is_active).unwrap();
    assert_eq!(active.name, "second");

    cleanup();
}

#[test]
#[ignore]
fn tab_switch_timing() {
    if !tmux_available() {
        eprintln!("tmux not available, skipping");
        return;
    }
    cleanup();

    // Create 5 windows
    let _ = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            TEST_SESSION,
            "-n",
            "w0",
            ";",
            "new-window",
            "-t",
            TEST_SESSION,
            "-n",
            "w1",
            ";",
            "new-window",
            "-t",
            TEST_SESSION,
            "-n",
            "w2",
            ";",
            "new-window",
            "-t",
            TEST_SESSION,
            "-n",
            "w3",
            ";",
            "new-window",
            "-t",
            TEST_SESSION,
            "-n",
            "w4",
        ])
        .output()
        .expect("failed to create test session");

    let out = tmux_run(&["list-windows", "-t", TEST_SESSION, "-F", "#{window_index}"]);
    let indexes: Vec<u32> = out.lines().filter_map(|l| l.trim().parse().ok()).collect();

    // Switch through all windows and measure average time
    let start = Instant::now();
    for &idx in &indexes {
        let _ = Command::new("tmux")
            .args(["select-window", "-t", &format!("{TEST_SESSION}:{idx}")])
            .output();
    }
    let elapsed = start.elapsed();
    let avg_ms = elapsed.as_millis() as f64 / indexes.len() as f64;

    assert!(
        avg_ms < 100.0,
        "average tab switch took {avg_ms:.1}ms (threshold: 100ms)"
    );

    cleanup();
}
