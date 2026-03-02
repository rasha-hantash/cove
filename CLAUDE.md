# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Cove

Cove is a Rust CLI tool that manages multiple Claude Code sessions inside tmux. It creates a 3-pane layout per session (Claude pane at 70% width, sidebar + terminal at 30%) and uses Claude Code hooks to track session state in real-time.

## Build & Development Commands

```bash
cargo build                    # Debug build
cargo build --release          # Release build
cargo install --path .         # Install locally as `cove`
cargo test                     # Run all tests
cargo test state::tests        # Run a specific test module
cargo clippy -- -D warnings    # Lint (CI treats warnings as errors)
cargo fmt --check              # Check formatting
cargo fmt                      # Auto-format
```

## Architecture

### Data Flow

Claude Code hooks → `cove hook {event}` → writes JSONL to `~/.cove/events/{session_id}.jsonl` → sidebar reads last line per file → matches `pane_id` to tmux windows → renders state in TUI.

### Module Map

- **`cli.rs`** — clap definitions. `Cli` struct has optional positional args (`name`, `dir`) plus subcommands. `HookEvent` enum maps hook types to state transitions.
- **`tmux.rs`** — thin wrappers around `tmux` CLI. All tmux interaction goes through this module. Session group is always named `"cove"`. Key function: `new_session()` creates the full 3-pane layout in a single tmux command chain.
- **`commands/start.rs`** — entry point for creating sessions. Checks/prompts for hook installation, handles first-run vs. adding a window to an existing session.
- **`commands/init.rs`** — manages Claude Code hooks in `~/.claude/settings.json`. Installs 4 async hooks (UserPromptSubmit, Stop, PreToolUse, PostToolUse) that call `cove hook`.
- **`commands/hook.rs`** — hook handler. Reads JSON from stdin, maps event type to state string, appends JSONL event with `pane_id` from `$TMUX_PANE`.
- **`sidebar/state.rs`** — state detection. Reads last line of each `.jsonl` file, matches events to windows by `pane_id`. States: Fresh → Working → Asking → Idle → Done.
- **`sidebar/app.rs`** — ratatui event loop. Renders in-place (no alternate screen) inside a tmux pane.
- **`sidebar/ui.rs`** — ratatui widgets. Session list with status indicators (animated spinner for Working, static labels for other states).
- **`colors.rs`** — Catppuccin Mocha palette. Defines both ratatui `Color` constants and `ANSI_*` escape codes for CLI output.

## Pre-coding context gate

Before writing any Rust code for a new module or significant piece of work, **stop and ask the user for context first**. The prompt should be:

> Before I write code for this, please give me the relevant Rust context for: **[describe the specific piece of work]**

The user will query their technical-rag system (which has ingested Rust books) and provide best-practice guidance, patterns, and idioms relevant to that specific task. Wait for their response before writing any code.

This applies to new modules, non-trivial refactors, and any area where Rust-specific patterns matter (error handling, trait design, async, lifetimes, etc.). It does NOT apply to small mechanical changes like adding a clap variant or wiring a new subcommand.

### Key Design Decisions

- **Pane ID matching**: Events are matched to windows via `$TMUX_PANE` (unique per pane), not by `cwd`. This handles multiple sessions in the same directory.
- **JSONL state files**: Append-only log per session. Sidebar reads only the last line (seeks to last 1KB for efficiency).
- **No alternate screen**: The sidebar TUI renders in-place to work correctly within a tmux pane.
- **70/30 layout enforcement**: A `window-layout-changed` hook auto-resizes pane .1 to 70% width, preventing mouse drag from breaking the layout.
