use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cove", about = "Claude Code session manager", version)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Cli {
    /// Session name (default behavior: start or resume a session)
    pub name: Option<String>,

    /// Working directory
    pub dir: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// List active sessions
    #[command(alias = "ls")]
    List,
    /// Kill a single session tab
    Kill {
        /// Session name to kill
        name: String,
    },
    /// Kill all sessions
    AllKill,
    /// Reattach to existing session
    Resume,
    /// Interactive session navigator (launched by start)
    Sidebar,
    /// Handle Claude Code hook events (called by hooks, not directly)
    Hook {
        #[command(subcommand)]
        event: HookEvent,
    },
    /// Install Claude Code hooks for session status detection
    Init,
}

#[derive(Subcommand)]
pub enum HookEvent {
    /// Claude received a user prompt (UserPromptSubmit hook)
    UserPrompt,
    /// Claude finished responding (Stop hook)
    Stop,
    /// Claude is about to show an AskUserQuestion (PreToolUse hook) [legacy]
    Ask,
    /// User answered an AskUserQuestion (PostToolUse hook) [legacy]
    AskDone,
    /// Claude is about to use a tool (PreToolUse hook, wildcard matcher)
    PreTool,
    /// Claude finished using a tool (PostToolUse hook, wildcard matcher)
    PostTool,
}
