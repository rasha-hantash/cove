# Context Retriever — Design Plan

## Problem

Cove's sidebar needs to show a 1-2 sentence summary of what each Claude session is working on. The original approach used `claude -c -p` which resumes the full session context (~600KB for a typical session with images), making it extremely slow (30+ seconds). Sessions are actively running, so `--resume` can't be used either — it would conflict with the live session.

## Solution: Direct JSONL Parsing

Read the Claude session JSONL file directly from disk (doesn't interfere with the running session). Extract only user/assistant text messages, skipping images, `tool_use`, `tool_result`, and progress entries. Truncate individual messages to 300 chars and keep recent messages within a 6KB budget. Call `claude -p` with a fresh, compact prompt (no `-c`, no `--resume`) to generate a summary via Haiku.

## Data Flow

1. Sidebar selects a session — gets the window's tmux `pane_id` from `StateDetector`
2. `ContextManager.request()` spawns a background thread
3. Thread scans `~/.cove/events/*.jsonl` to find the `session_id` matching the `pane_id`
4. Derives the Claude project path: `~/.claude/projects/{cwd with / replaced by -}/`
5. Reads `{session_id}.jsonl`, parses user/assistant messages
6. Builds a compact prompt (~6KB) and calls `claude -p "..." --max-turns 1 --model haiku`
7. Result flows back via mpsc channel — sidebar renders it at the bottom of the panel

## UX Behavior

- **Session switching:** old session's context refreshes, new session shows "loading..." if not cached
- **Caching:** context is cached per window name — only regenerated on session switch
- **Retry cooldown:** failed generations enter a 30-second retry cooldown before retrying
- **Subprocess timeout:** 30-second timeout prevents hangs from stalled `claude` calls
- **Layout:** context block is pinned to the bottom of the sidebar with a separator line

## Performance

| Metric                      | Old approach                  | New approach               |
| --------------------------- | ----------------------------- | -------------------------- |
| Prompt size                 | ~600KB (full session context) | ~6KB (extracted text)      |
| API call latency            | 30+ seconds                   | 3-5 seconds                |
| Hook overhead per tool call | —                             | ~90ms (negligible)         |
| JSONL parsing               | —                             | <10ms even for large files |

## Files Changed

- **`src/sidebar/context.rs`** — Core logic: JSONL parsing, session lookup, context generation
- **`src/sidebar/state.rs`** — Added `pane_id` storage to `StateDetector`
- **`src/sidebar/app.rs`** — Wired context manager with `pane_id` passing
- **`src/sidebar/ui.rs`** — Context block + loading indicator rendering
