// ── Remote cove via SSH ──
//
// `cove vps [host] [dir]` SSHes to `host` and execs `cove` on the remote.
// Real connection details (HostName, User, IdentityFile, Port) live in
// `~/.ssh/config` — cove only ever sees the alias name.
//
// Defaults (laptop-side env vars, both optional):
//   COVE_DEFAULT_VPS         — host alias to use when none is passed
//   COVE_DEFAULT_REMOTE_DIR  — directory to cd into on the remote before cove
//
// If COVE_DEFAULT_REMOTE_DIR is unset, cove runs from ssh's default cwd
// (typically $HOME) on the remote — no `cd` is performed. This avoids
// baking a workspace layout into the cove binary.

use std::process::Command;

pub fn run(host: Option<&str>, dir: Option<&str>) -> Result<(), String> {
    let host = resolve_host(host)?;
    let remote_cmd = build_remote_cmd(resolve_dir(dir).as_deref());

    // -t forces tty allocation so the remote tmux/cove can render.
    let status = Command::new("ssh")
        .args(["-t", &host, &remote_cmd])
        .status()
        .map_err(|e| format!("ssh: {e}"))?;

    if !status.success() {
        return Err(format!("ssh {host}: exited with status {status}"));
    }
    Ok(())
}

/// Resolve the SSH host: explicit arg → `$COVE_DEFAULT_VPS` → error.
fn resolve_host(host: Option<&str>) -> Result<String, String> {
    if let Some(h) = host {
        return Ok(h.to_string());
    }
    match std::env::var("COVE_DEFAULT_VPS") {
        Ok(v) if !v.is_empty() => Ok(v),
        _ => Err("no host given and COVE_DEFAULT_VPS is not set.\n  \
             Set up an alias in ~/.ssh/config and either pass it explicitly \
             (`cove vps myhost`) or export COVE_DEFAULT_VPS=myhost in your shell."
            .to_string()),
    }
}

/// Resolve the remote dir: explicit arg → `$COVE_DEFAULT_REMOTE_DIR` → None.
/// `None` means "don't cd — run cove from ssh's default cwd ($HOME)".
fn resolve_dir(dir: Option<&str>) -> Option<String> {
    if let Some(d) = dir {
        return Some(d.to_string());
    }
    std::env::var("COVE_DEFAULT_REMOTE_DIR")
        .ok()
        .filter(|s| !s.is_empty())
}

/// Compose the remote shell command. Skips the `cd` entirely when no dir is set.
fn build_remote_cmd(dir: Option<&str>) -> String {
    match dir {
        Some(d) => format!("cd {d} && exec cove"),
        None => "exec cove".to_string(),
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialize env-var mutations across tests so they don't race.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn explicit_host_wins_over_env() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("COVE_DEFAULT_VPS", "envhost") };
        let resolved = resolve_host(Some("explicithost")).unwrap();
        unsafe { std::env::remove_var("COVE_DEFAULT_VPS") };
        assert_eq!(resolved, "explicithost");
    }

    #[test]
    fn falls_back_to_env() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("COVE_DEFAULT_VPS", "envhost") };
        let resolved = resolve_host(None).unwrap();
        unsafe { std::env::remove_var("COVE_DEFAULT_VPS") };
        assert_eq!(resolved, "envhost");
    }

    #[test]
    fn errors_when_no_host_and_no_env() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("COVE_DEFAULT_VPS") };
        let err = resolve_host(None).unwrap_err();
        assert!(err.contains("COVE_DEFAULT_VPS"));
    }

    #[test]
    fn empty_env_treated_as_unset() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("COVE_DEFAULT_VPS", "") };
        let err = resolve_host(None).unwrap_err();
        unsafe { std::env::remove_var("COVE_DEFAULT_VPS") };
        assert!(err.contains("COVE_DEFAULT_VPS"));
    }

    #[test]
    fn dir_explicit_wins_over_env() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("COVE_DEFAULT_REMOTE_DIR", "/from/env") };
        let resolved = resolve_dir(Some("/from/arg"));
        unsafe { std::env::remove_var("COVE_DEFAULT_REMOTE_DIR") };
        assert_eq!(resolved.as_deref(), Some("/from/arg"));
    }

    #[test]
    fn dir_falls_back_to_env() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::set_var("COVE_DEFAULT_REMOTE_DIR", "~/workspace") };
        let resolved = resolve_dir(None);
        unsafe { std::env::remove_var("COVE_DEFAULT_REMOTE_DIR") };
        assert_eq!(resolved.as_deref(), Some("~/workspace"));
    }

    #[test]
    fn dir_none_when_unset() {
        let _g = ENV_LOCK.lock().unwrap();
        unsafe { std::env::remove_var("COVE_DEFAULT_REMOTE_DIR") };
        assert!(resolve_dir(None).is_none());
    }

    #[test]
    fn remote_cmd_with_dir_includes_cd() {
        assert_eq!(
            build_remote_cmd(Some("~/workspace")),
            "cd ~/workspace && exec cove"
        );
    }

    #[test]
    fn remote_cmd_without_dir_skips_cd() {
        assert_eq!(build_remote_cmd(None), "exec cove");
    }
}
