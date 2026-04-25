// ── tmux Command wrappers ──

use std::process::Command;

// ── Types ──

pub struct WindowInfo {
    pub index: u32,
    pub name: String,
    pub is_active: bool,
    pub pane_path: String,
    pub is_docker: bool,
    pub is_ssh: bool,
}

// ── Helpers ──

fn tmux(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new("tmux").args(args).output()
}

fn tmux_ok(args: &[&str]) -> bool {
    tmux(args).is_ok_and(|o| o.status.success())
}

fn tmux_stdout(args: &[&str]) -> Result<String, String> {
    let output = tmux(args).map_err(|e| format!("tmux: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("tmux: {}", stderr.trim()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Check whether `dir` is inside a git repository.
fn is_git_repo(dir: &str) -> bool {
    Command::new("git")
        .args(["-C", dir, "rev-parse", "--is-inside-work-tree"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Build the claude command for a session.
/// Claude Code manages its own worktrees via EnterWorktree — cove doesn't force --worktree.
fn claude_cmd(name: &str, dir: &str, docker: bool) -> String {
    if docker {
        // In docker mode, pass --repo for git repos so the entrypoint clones
        // into /scratch/<repo> and cd's there. Non-git dirs just get plain claude.
        let repo_flag = if is_git_repo(dir) {
            let repo_name = std::path::Path::new(dir)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(name);
            format!(" --repo {repo_name}")
        } else {
            String::new()
        };
        format!(
            "cd ~/workspace/personal/explorations/claude-container && ./scripts/run.sh{repo_flag} claude"
        )
    } else {
        "claude".to_string()
    }
}

// ── Public API ──

pub const SESSION: &str = "cove";

pub fn has_session() -> bool {
    tmux_ok(&["has-session", "-t", SESSION])
}

pub fn list_windows() -> Result<Vec<WindowInfo>, String> {
    let out = tmux_stdout(&[
        "list-windows",
        "-t",
        SESSION,
        "-F",
        "#{window_index}|#{window_name}|#{window_active}|#{@cove_docker}|#{@cove_ssh}|#{pane_current_path}",
    ])?;
    Ok(parse_window_list(&out))
}

/// Parse tmux list-windows output into WindowInfo structs.
/// Format: "index|name|active|docker|ssh|path" per line. `path` is last so any
/// pipe characters inside it are preserved by splitn(6, '|').
pub fn parse_window_list(output: &str) -> Vec<WindowInfo> {
    let mut windows = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.splitn(6, '|').collect();
        if parts.len() < 6 {
            continue;
        }
        windows.push(WindowInfo {
            index: parts[0].parse().unwrap_or(0),
            name: parts[1].to_string(),
            is_active: parts[2] == "1",
            is_docker: !parts[3].is_empty(),
            is_ssh: !parts[4].is_empty(),
            pane_path: parts[5].to_string(),
        });
    }
    windows
}

/// List window names only (for duplicate checking).
pub fn list_window_names() -> Result<Vec<String>, String> {
    let out = tmux_stdout(&["list-windows", "-t", SESSION, "-F", "#{window_name}"])?;
    Ok(out.lines().map(|s| s.to_string()).collect())
}

pub fn is_inside_tmux() -> bool {
    std::env::var("TMUX").is_ok_and(|v| !v.is_empty())
}

pub fn new_session(name: &str, dir: &str, sidebar_bin: &str, docker: bool) -> Result<(), String> {
    let claude_cmd = claude_cmd(name, dir, docker);
    let status = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            SESSION,
            "-n",
            name,
            "-c",
            dir,
            ";",
            "set-option",
            "-w",
            "automatic-rename",
            "on",
            ";",
            "set-option",
            "-w",
            "automatic-rename-format",
            "#{pane_title}",
            ";",
            "set-option",
            "-w",
            "allow-rename",
            "on",
            ";",
            "set-option",
            "-w",
            "remain-on-exit",
            "on",
            ";",
            "set-hook",
            "pane-died",
            "respawn-pane",
            ";",
            "split-window",
            "-h",
            "-p",
            "30",
            "-c",
            dir,
            ";",
            "split-window",
            "-t",
            ".2",
            "-v",
            "-b",
            "-p",
            "50",
            sidebar_bin,
            ";",
            "select-pane",
            "-t",
            ".2",
            ";",
            "respawn-pane",
            "-t",
            ".1",
            "-k",
            &claude_cmd,
            ";",
            "set-hook",
            "-w",
            "window-layout-changed",
            "run-shell 'tmux resize-pane -t #{session_name}:#{window_index}.1 -x $((#{window_width} * 70 / 100))'",
        ])
        .status()
        .map_err(|e| format!("tmux: {e}"))?;

    if !status.success() {
        return Err("tmux new-session failed".to_string());
    }
    Ok(())
}

pub fn new_window(name: &str, dir: &str, docker: bool) -> Result<(), String> {
    // -a = insert AFTER the target window, not AT its index.
    // Without -a, `-t cove` resolves to the current window (e.g. cove:1)
    // and tmux tries to create at that exact index, causing "index N in use".
    let claude_cmd = claude_cmd(name, dir, docker);
    let status = Command::new("tmux")
        .args([
            "new-window",
            "-a",
            "-t",
            SESSION,
            "-n",
            name,
            "-c",
            dir,
            &claude_cmd,
        ])
        .status()
        .map_err(|e| format!("tmux: {e}"))?;

    if !status.success() {
        return Err("tmux new-window failed".to_string());
    }

    // Forward Claude Code's OSC 0/2 title onto the tmux window name.
    let target = format!("{SESSION}:{name}");
    let _ = tmux(&["set-option", "-w", "-t", &target, "automatic-rename", "on"]);
    let _ = tmux(&[
        "set-option",
        "-w",
        "-t",
        &target,
        "automatic-rename-format",
        "#{pane_title}",
    ]);
    let _ = tmux(&["set-option", "-w", "-t", &target, "allow-rename", "on"]);

    Ok(())
}

pub fn setup_layout(name: &str, dir: &str, sidebar_bin: &str) -> Result<(), String> {
    let win = format!("{SESSION}:{name}");
    let status = Command::new("tmux")
        .args([
            "set-option",
            "-w",
            "-t",
            &win,
            "remain-on-exit",
            "on",
            ";",
            "set-hook",
            "-w",
            "-t",
            &win,
            "pane-died",
            "respawn-pane",
            ";",
            "split-window",
            "-t",
            &win,
            "-h",
            "-p",
            "30",
            "-c",
            dir,
            ";",
            "split-window",
            "-t",
            &format!("{win}.2"),
            "-v",
            "-b",
            "-p",
            "50",
            sidebar_bin,
            ";",
            "select-pane",
            "-t",
            &format!("{win}.2"),
            ";",
            "set-hook",
            "-w",
            "-t",
            &win,
            "window-layout-changed",
            &format!(
                "run-shell 'tmux resize-pane -t {win}.1 -x $(( #{{window_width}} * 70 / 100 ))'"
            ),
        ])
        .status()
        .map_err(|e| format!("tmux: {e}"))?;

    if !status.success() {
        return Err("tmux setup-layout failed".to_string());
    }
    Ok(())
}

pub fn attach() -> Result<(), String> {
    let status = Command::new("tmux")
        .args(["attach", "-t", SESSION])
        .status()
        .map_err(|e| format!("tmux: {e}"))?;

    if !status.success() {
        return Err("tmux attach failed".to_string());
    }
    Ok(())
}

pub fn switch_client() -> Result<(), String> {
    let status = Command::new("tmux")
        .args(["switch-client", "-t", SESSION])
        .status()
        .map_err(|e| format!("tmux: {e}"))?;

    if !status.success() {
        return Err("tmux switch-client failed".to_string());
    }
    Ok(())
}

pub fn kill_window(name: &str) -> Result<(), String> {
    let target = format!("{SESSION}:{name}");
    tmux_stdout(&["kill-window", "-t", &target])?;
    Ok(())
}

pub fn kill_session() -> Result<(), String> {
    tmux_stdout(&["kill-session", "-t", SESSION])?;
    Ok(())
}

pub fn select_window(index: u32) -> Result<(), String> {
    let target = format!("{SESSION}:{index}");
    let status = Command::new("tmux")
        .args([
            "select-window",
            "-t",
            &target,
            ";",
            "select-pane",
            "-t",
            ":.1",
        ])
        .status()
        .map_err(|e| format!("tmux: {e}"))?;

    if !status.success() {
        return Err("tmux select-window failed".to_string());
    }
    Ok(())
}

/// Info about pane .1 in each window (for state detection).
pub struct PaneInfo {
    pub window_index: u32,
    pub command: String,
    /// Unique tmux pane identifier (e.g. "%0", "%3").
    pub pane_id: String,
}

/// Get the foreground command and pane ID of pane .1 in every window.
pub fn list_pane_commands() -> Result<Vec<PaneInfo>, String> {
    let out = tmux_stdout(&[
        "list-panes",
        "-s",
        "-t",
        SESSION,
        "-F",
        "#{window_index}|#{pane_index}|#{pane_current_command}|#{pane_id}",
    ])?;
    Ok(parse_pane_list(&out))
}

/// Parse tmux list-panes output into PaneInfo structs.
/// Format: "window_index|pane_index|command|pane_id" per line.
/// Only returns panes with pane_index=1 (the Claude pane).
pub fn parse_pane_list(output: &str) -> Vec<PaneInfo> {
    let mut panes = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.splitn(4, '|').collect();
        if parts.len() < 4 {
            continue;
        }
        // Only pane index 1 (the Claude pane)
        if parts[1] != "1" {
            continue;
        }
        panes.push(PaneInfo {
            window_index: parts[0].parse().unwrap_or(0),
            command: parts[2].to_string(),
            pane_id: parts[3].to_string(),
        });
    }
    panes
}

/// Get the pane_id (e.g. "%5") of pane .1 (the Claude pane) in a specific window.
pub fn get_claude_pane_id(window_name: &str) -> Result<String, String> {
    let target = format!("{SESSION}:{window_name}.1");
    let out = tmux_stdout(&["display-message", "-t", &target, "-p", "#{pane_id}"])?;
    Ok(out.trim().to_string())
}

pub fn set_window_option(window_name: &str, key: &str, value: &str) -> Result<(), String> {
    let target = format!("{SESSION}:{window_name}");
    tmux_stdout(&["set-option", "-w", "-t", &target, key, value])?;
    Ok(())
}

/// Send keys to the Claude pane (.1) of a window.
pub fn send_keys(window_name: &str, keys: &[&str]) -> Result<(), String> {
    let target = format!("{SESSION}:{window_name}.1");
    let mut args = vec!["send-keys", "-t", &target];
    args.extend_from_slice(keys);
    tmux_stdout(&args)?;
    Ok(())
}

/// Get the foreground command running in the Claude pane (.1).
pub fn pane_command(window_name: &str) -> Result<String, String> {
    let target = format!("{SESSION}:{window_name}.1");
    let out = tmux_stdout(&[
        "display-message",
        "-t",
        &target,
        "-p",
        "#{pane_current_command}",
    ])?;
    Ok(out.trim().to_string())
}

/// Remove the pane-died hook so Claude isn't respawned after exit.
pub fn disable_respawn(window_name: &str) -> Result<(), String> {
    let target = format!("{SESSION}:{window_name}");
    let _ = tmux_stdout(&["set-hook", "-u", "-w", "-t", &target, "pane-died"]);
    let _ = tmux_stdout(&["set-option", "-w", "-t", &target, "remain-on-exit", "off"]);
    Ok(())
}

pub fn select_window_sidebar(index: u32) -> Result<(), String> {
    let target = format!("{SESSION}:{index}");
    let status = Command::new("tmux")
        .args([
            "select-window",
            "-t",
            &target,
            ";",
            "select-pane",
            "-t",
            ":.2",
        ])
        .status()
        .map_err(|e| format!("tmux: {e}"))?;

    if !status.success() {
        return Err("tmux select-window failed".to_string());
    }
    Ok(())
}
