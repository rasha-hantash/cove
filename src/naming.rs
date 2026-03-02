// ── Window naming logic ──
//
// Builds tmux window names in the format: {base}-{branch}
// Worktree sessions get a (wt) suffix.

use std::process::Command;

/// Replace problematic characters and clean up the name for tmux.
pub fn sanitize_name(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| if matches!(c, '.' | ':' | '/') { '-' } else { c })
        .collect();

    // Collapse consecutive dashes, strip leading/trailing dashes
    let collapsed = replaced
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    // Truncate to 30 chars on a dash boundary if possible
    if collapsed.len() <= 30 {
        collapsed
    } else {
        match collapsed[..30].rfind('-') {
            Some(pos) if pos > 10 => collapsed[..pos].to_string(),
            _ => collapsed[..30].to_string(),
        }
    }
}

/// Get the current git branch name, or None if not a git repo.
pub fn git_branch(dir: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["-C", dir, "rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        None
    } else {
        Some(branch)
    }
}

/// Check if the directory is inside a git worktree (not the main working tree).
pub fn is_worktree(dir: &str) -> bool {
    let git_dir = Command::new("git")
        .args(["-C", dir, "rev-parse", "--git-dir"])
        .output()
        .ok();
    let common_dir = Command::new("git")
        .args(["-C", dir, "rev-parse", "--git-common-dir"])
        .output()
        .ok();

    match (git_dir, common_dir) {
        (Some(gd), Some(cd)) if gd.status.success() && cd.status.success() => {
            let gd_str = String::from_utf8_lossy(&gd.stdout).trim().to_string();
            let cd_str = String::from_utf8_lossy(&cd.stdout).trim().to_string();
            // In a worktree, git-dir is something like ../.git/worktrees/name
            // while git-common-dir is the main ../.git
            gd_str != cd_str
        }
        _ => false,
    }
}

/// Build the full window name: {base}-{branch} with optional (wt) suffix.
/// Falls back to just {base} if not a git repo.
pub fn build_window_name(base: &str, dir: &str) -> String {
    let branch = git_branch(dir);
    let wt = is_worktree(dir);

    match branch {
        Some(branch) => {
            let raw = format!("{base}-{branch}");
            let name = sanitize_name(&raw);
            if wt { format!("{name}(wt)") } else { name }
        }
        None => sanitize_name(base),
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_name() {
        assert_eq!(sanitize_name("cove-main"), "cove-main");
    }

    #[test]
    fn replaces_dots_colons_slashes() {
        assert_eq!(sanitize_name("feature/add-auth"), "feature-add-auth");
        assert_eq!(sanitize_name("v1.2.3"), "v1-2-3");
        assert_eq!(sanitize_name("host:port"), "host-port");
    }

    #[test]
    fn collapses_consecutive_dashes() {
        assert_eq!(sanitize_name("a--b---c"), "a-b-c");
        assert_eq!(sanitize_name("a/../b"), "a-b");
    }

    #[test]
    fn strips_leading_trailing_dashes() {
        assert_eq!(sanitize_name("-hello-"), "hello");
        assert_eq!(sanitize_name("---test---"), "test");
    }

    #[test]
    fn truncates_long_names() {
        let long = "a".repeat(50);
        let result = sanitize_name(&long);
        assert!(result.len() <= 30);
    }

    #[test]
    fn truncates_on_dash_boundary() {
        // 35 chars total: "abcdefghijklmno-pqrstuvwxyz-12345"
        let name = "abcdefghijklmno-pqrstuvwxyz-12345";
        let result = sanitize_name(name);
        assert!(result.len() <= 30);
        // Should truncate at the last dash within 30 chars
        assert_eq!(result, "abcdefghijklmno-pqrstuvwxyz");
    }

    #[test]
    fn empty_string() {
        assert_eq!(sanitize_name(""), "");
    }

    #[test]
    fn only_special_chars() {
        assert_eq!(sanitize_name("..."), "");
    }

    #[test]
    fn build_name_no_git() {
        // /tmp is not a git repo
        let name = build_window_name("myapp", "/tmp");
        assert_eq!(name, "myapp");
    }
}
