# Cove + Claude Code tmux Team Agent Integration Plan

**Date:** 2026-03-04
**Status:** Research complete, implementation pending

## Background

Claude Code's experimental team feature (`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`) spawns teammate agents as new tmux windows via `tmux new-window claude ...`. The agent processes become children of the tmux server, not the parent claude process.

This creates an accidental synergy with cove: agent windows spawned inside the `cove` tmux session automatically inherit cove's session-level hooks (70/30 layout enforcement, pane respawn, etc.) — neither tool explicitly knows about the other.

## Current State

### What already works (no code changes needed)

- **Hooks fire for agents** — cove's hooks are global in `~/.claude/settings.json` with wildcard matchers (`"*"`). Agent processes inherit them.
- **Each agent gets its own JSONL event file** — separate `session_id`, correct `$TMUX_PANE` captured. Files land in `~/.cove/events/{agent_session_id}.jsonl`.
- **Pane ID matching works** — the sidebar loads all `.jsonl` files and matches by `pane_id`. If an agent window is in the `cove` session, its state transitions (Working/Idle/Asking) are already tracked.
- **Layout enforcement applies** — session-level `window-layout-changed` hook resizes agent windows to 70/30 automatically.

### What doesn't work

- **Sidebar only shows cove-created windows** — `tmux.rs:list_windows()` queries the `"cove"` session, so agent windows appear in the list, but cove treats them as `Fresh` (no name, no context). They're visible but not meaningfully tracked.
- **No agent vs. lead distinction** — all windows look the same in the sidebar. No way to tell which are agents and which are user-started sessions.
- **No hierarchical view** — agents are flat in the window list, with no parent-child relationship visible.
- **No team lifecycle awareness** — cove doesn't know when a team is active, how many agents are running, or when they shut down.

## Key Metadata Available

### tmux-level

| Metadata      | Source                 | Example                     | Notes                          |
| ------------- | ---------------------- | --------------------------- | ------------------------------ |
| Window name   | `#{window_name}`       | `dotfiles-improvement-main` | Contains branch/project        |
| Custom option | `@cove_base`           | `dotfiles-improvement`      | Set by cove on window creation |
| Pane ID       | `#{pane_id}`           | `%3`                        | Unique per pane                |
| Pane title    | `#{pane_title}`        | `✳ Claude Code`             | Agent windows show this        |
| Working dir   | `#{pane_current_path}` | `/path/to/project`          |                                |
| Process PID   | `#{pane_pid}`          | `8996`                      | Claude process PID             |

### File-level

| File          | Location                                      | Contents                                                         |
| ------------- | --------------------------------------------- | ---------------------------------------------------------------- |
| Team config   | `~/.claude/teams/{name}/config.json`          | Agent roster: `leadAgentId`, `agents[]` with `name`, `agentType` |
| Agent inboxes | `~/.claude/teams/{name}/inboxes/{agent}.json` | Message queue: `shutdown_request`, task assignments              |
| Event logs    | `~/.cove/events/{session_id}.jsonl`           | Per-session state events with `pane_id`                          |

### Environment variables

- `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` — in tmux global env when teams are enabled
- `CLAUDECODE=1` — set in all Claude Code processes
- `TMUX_PANE=%N` — unique per pane, already used for state matching

## Implementation Plan

### Phase 1: Detect agent windows

**Goal:** Distinguish cove-created windows from agent-spawned windows in the sidebar.

**Approach:** Cove already sets `@cove_base` on windows it creates via `new_window()`. Agent windows created by Claude Code won't have this option set (or it'll have a different value). Detection:

1. When listing windows, also read `@cove_base` for each window
2. Windows without `@cove_base` (or with an unrecognized value) that have a running `claude` process are likely agent windows
3. Cross-reference with `~/.claude/teams/*/config.json` to confirm team membership

**Sidebar display:** Show agent windows with a distinct indicator — e.g., `⚡` prefix or dimmed color — to distinguish them from user-started sessions.

### Phase 2: Team lifecycle awareness

**Goal:** Know when a team is active and which agents belong to it.

**Approach:** Watch `~/.claude/teams/` directory:

1. On sidebar tick, scan for team config files
2. Parse `config.json` to get agent roster
3. Match agent names to tmux windows (by window name or pane title)
4. Detect team shutdown via `SessionEnd` events or inbox `shutdown_request` messages

**Sidebar display:** Group agent windows under their lead session. Show agent count: `my-feature (2 agents)`.

### Phase 3: Hierarchical session view

**Goal:** Show parent-child relationship between lead and agent sessions.

**Approach:**

1. Indent agent windows under their lead in the sidebar list
2. Aggregate state: if any agent is Working, show the lead as "delegating"
3. Collapse/expand agent sub-list (keyboard shortcut)

### Phase 4: Cross-session context

**Goal:** Surface agent activity in the lead session's context summary.

**Approach:**

1. Read agent event files to get their current task/state
2. Include agent summaries in the lead session's context line
3. Example: `Lead: coordinating 2 agents — @backend (fixing auth) @frontend (building UI)`

## Architecture Constraint

The sidebar hardcodes `SESSION = "cove"` in `tmux.rs`. This means agent detection only works if Claude Code spawns agents **within the same `cove` session**. If agents end up in a separate tmux session, the sidebar won't see them. This is the current behavior (agents spawn in the active session), but worth monitoring if Claude Code changes its spawning strategy.

## Open Questions

1. **Does Claude Code always spawn agents in the current session?** — If it creates a new session, cove won't see the agents. Need to verify this is stable behavior.
2. **Can cove set `@cove_base` on agent windows retroactively?** — If we detect an untagged window, can we tag it without disrupting the agent?
3. **Should cove auto-create the 3-pane layout for agent windows?** — Currently the layout enforcement hook handles this, but the sidebar pane and terminal pane aren't explicitly created by cove for agent windows.
4. **Team config file stability** — Is `~/.claude/teams/` a stable API or an implementation detail that might change?

## Related

- `brain-os/claude-learnings/2026-03-04-claude-code-tmux-team-agents.md` — original discovery and exploration ideas
- `brain-os/claude-learnings/2026-03-03-claudecode-env-nested-sessions.md` — `CLAUDECODE=1` env var blocking nested sessions
