// ── Voice session launcher ──
//
// Launches Claude Code in a standalone Ghostty window instead of tmux.
// Voice mode (push-to-talk with spacebar) requires raw terminal key events
// that tmux cannot forward, so this bypasses tmux entirely.

use std::process::Command;

use crate::colors::*;

/// Locate the Ghostty binary.
fn find_ghostty() -> Result<String, String> {
    // Check PATH
    if let Ok(output) = Command::new("which").arg("ghostty").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(path);
            }
        }
    }

    // macOS app bundle fallback
    let app_bin = "/Applications/Ghostty.app/Contents/MacOS/ghostty";
    if std::path::Path::new(app_bin).exists() {
        return Ok(app_bin.to_string());
    }

    Err("Ghostty not found. Ensure 'ghostty' is in your PATH \
         or Ghostty.app is in /Applications."
        .to_string())
}

pub fn run(name: Option<&str>, dir: Option<&str>) -> Result<(), String> {
    let dir = dir.unwrap_or(".");
    let dir = std::fs::canonicalize(dir)
        .map_err(|e| format!("invalid directory '{dir}': {e}"))?
        .to_string_lossy()
        .to_string();

    let name = name.map(String::from).unwrap_or_else(|| {
        std::path::Path::new(&dir)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "voice".to_string())
    });

    let ghostty = find_ghostty()?;

    // Launch Ghostty with claude as the initial command.
    // Ghostty runs as a separate GUI process — it survives if cove or tmux exits.
    Command::new(&ghostty)
        .args([
            &format!("--title=cove voice: {name}"),
            &format!("--working-directory={dir}"),
            "-e",
            "claude",
        ])
        .spawn()
        .map_err(|e| format!("failed to launch Ghostty: {e}"))?;

    println!(
        "{ANSI_PEACH}cove voice:{ANSI_RESET} launched '{ANSI_BOLD}{name}{ANSI_RESET}' in Ghostty"
    );
    println!(
        "{ANSI_OVERLAY}Type /voice in Claude to enable push-to-talk (hold spacebar to record){ANSI_RESET}"
    );

    Ok(())
}
