// ── XDG-compliant paths for cove ──
//
// Cove's data (event logs, context log) is operational state — safe to delete,
// regenerated on next session. Per XDG Base Directory spec, this goes in
// XDG_STATE_HOME/cove (~/.local/state/cove by default).
//
// On first run, migrates from the legacy ~/.cove/ directory if it exists.

use std::env;
use std::fs;
use std::path::PathBuf;

/// Legacy data directory (~/.cove/).
fn legacy_dir() -> PathBuf {
    let home = env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".cove")
}

/// XDG_STATE_HOME/cove — event logs, context log, session state.
pub fn state_dir() -> PathBuf {
    env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = env::var("HOME").unwrap_or_default();
            PathBuf::from(home).join(".local").join("state")
        })
        .join("cove")
}

/// XDG_STATE_HOME/cove/events/
pub fn events_dir() -> PathBuf {
    state_dir().join("events")
}

/// Migrate from ~/.cove/ to XDG_STATE_HOME/cove/ if needed.
///
/// Runs once — if the new path already exists, this is a no-op.
/// Moves the directory (rename is atomic on the same filesystem).
/// If rename fails (cross-device), falls back to keeping legacy path.
pub fn migrate_legacy() {
    let legacy = legacy_dir();
    let xdg = state_dir();

    // Nothing to migrate
    if !legacy.is_dir() {
        return;
    }

    // Already migrated (or user set XDG_STATE_HOME to something with existing data)
    if xdg.is_dir() {
        return;
    }

    // Ensure parent exists (~/.local/state/)
    if let Some(parent) = xdg.parent() {
        let _ = fs::create_dir_all(parent);
    }

    // Atomic rename (same filesystem)
    match fs::rename(&legacy, &xdg) {
        Ok(()) => {
            eprintln!(
                "cove: migrated {} → {}",
                legacy.display(),
                xdg.display()
            );
            // Leave a symlink at the old path for anything that might reference it
            #[cfg(unix)]
            {
                let _ = std::os::unix::fs::symlink(&xdg, &legacy);
            }
        }
        Err(e) => {
            // Cross-device or permission error — keep using legacy path
            eprintln!(
                "cove: could not migrate {} → {}: {e}",
                legacy.display(),
                xdg.display()
            );
            eprintln!("cove: continuing with {}", legacy.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_dir_respects_xdg_env() {
        unsafe { env::set_var("XDG_STATE_HOME", "/tmp/test-xdg-state") };
        let dir = state_dir();
        assert_eq!(dir, PathBuf::from("/tmp/test-xdg-state/cove"));
        unsafe { env::remove_var("XDG_STATE_HOME") };
    }

    #[test]
    fn events_dir_is_under_state() {
        unsafe { env::set_var("XDG_STATE_HOME", "/tmp/test-xdg-state") };
        let dir = events_dir();
        assert_eq!(dir, PathBuf::from("/tmp/test-xdg-state/cove/events"));
        unsafe { env::remove_var("XDG_STATE_HOME") };
    }

    #[test]
    fn migrate_noop_when_no_legacy() {
        unsafe { env::set_var("HOME", "/tmp/nonexistent-home-for-cove-test") };
        unsafe { env::set_var("XDG_STATE_HOME", "/tmp/nonexistent-xdg-for-cove-test") };
        migrate_legacy();
        unsafe { env::remove_var("XDG_STATE_HOME") };
    }
}
