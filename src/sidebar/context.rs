// ── Background context generation for sessions ──
//
// Spawns `claude -c -p` in a background thread to summarize what the user
// was working on in each session. Results flow back via mpsc channel so the
// sidebar event loop never blocks.

use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::sync::mpsc;
use std::thread;

// ── Types ──

pub struct ContextManager {
    contexts: HashMap<String, String>,
    in_flight: HashSet<String>,
    failed: HashSet<String>,
    tx: mpsc::Sender<(String, String)>,
    rx: mpsc::Receiver<(String, String)>,
}

// ── Constants ──

const PROMPT: &str = "\
Summarize what you were just working on in 1-2 concise sentences. \
Be specific about the feature, bug, or task. Output only the summary, nothing else.";

// ── Public API ──

impl ContextManager {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            contexts: HashMap::new(),
            in_flight: HashSet::new(),
            failed: HashSet::new(),
            tx,
            rx,
        }
    }

    /// Drain completed context results from background threads.
    pub fn drain(&mut self) {
        while let Ok((name, context)) = self.rx.try_recv() {
            self.in_flight.remove(&name);
            if context.is_empty() {
                self.failed.insert(name);
            } else {
                self.contexts.insert(name, context);
            }
        }
    }

    /// Get the context for a window, if available.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.contexts.get(name).map(String::as_str)
    }

    /// Whether a context request is currently running for this window.
    pub fn is_loading(&self, name: &str) -> bool {
        self.in_flight.contains(name)
    }

    /// Request context generation for a window (no-op if cached, in flight, or failed).
    pub fn request(&mut self, name: &str, cwd: &str) {
        if self.contexts.contains_key(name)
            || self.in_flight.contains(name)
            || self.failed.contains(name)
        {
            return;
        }
        self.spawn(name, cwd);
    }

    /// Force-refresh context for a window (clears cache/failed, respects in_flight).
    pub fn refresh(&mut self, name: &str, cwd: &str) {
        if self.in_flight.contains(name) {
            return;
        }
        self.contexts.remove(name);
        self.failed.remove(name);
        self.spawn(name, cwd);
    }

    fn spawn(&mut self, name: &str, cwd: &str) {
        self.in_flight.insert(name.to_string());
        let tx = self.tx.clone();
        let name = name.to_string();
        let cwd = cwd.to_string();
        thread::spawn(move || {
            let context = generate_context(&cwd).unwrap_or_default();
            let _ = tx.send((name, context));
        });
    }
}

// ── Helpers ──

fn generate_context(cwd: &str) -> Option<String> {
    let output = Command::new("claude")
        .args([
            "-c",
            "-p",
            PROMPT,
            "--max-turns",
            "1",
            "--fork-session",
            "--model",
            "haiku",
        ])
        .current_dir(cwd)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}
