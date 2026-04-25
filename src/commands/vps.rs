// ── Remote cove via SSH ──
//
// `cove vps [host] [dir]` SSHes to `host` and execs `cove` on the remote.
// Real connection details (HostName, User, IdentityFile, Port) live in
// `~/.ssh/config` — cove only ever sees the alias name.
//
// If `host` is omitted, falls back to `$COVE_DEFAULT_VPS` so the common
// case is just `cove vps`.

use std::process::Command;

const DEFAULT_DIR: &str = "~/workspace/projects";

pub fn run(host: Option<&str>, dir: Option<&str>) -> Result<(), String> {
    let host = resolve_host(host)?;
    let dir = dir.unwrap_or(DEFAULT_DIR);
    let remote_cmd = format!("cd {dir} && exec cove");

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
}
