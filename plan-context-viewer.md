# Context Viewer for CCS Sidebar

## Context

Claude Code loads context from multiple sources (global CLAUDE.md, project CLAUDE.md, memory files, brain-os backlinks), but there's no way to see what's active or discover available brain-os docs you're not using. You want a view in Cove that shows this and lets you add missing docs.

## Approach: Toggle Mode in Existing Sidebar

Add a second view mode to the CCS sidebar (the ratatui widget in pane .3). Press `Tab` to switch between **Sessions** view (current behavior) and **Context** view (new). No tmux layout changes needed.

## UI

### Sessions Mode (unchanged)

```
 3 sessions · ↑↓ navigate
────────────────────────────────
 ❯ my-project ⠹       ⌘+j claude
 · api-work   waiting… ⌘+m terminal
 · docs                ⌘+p sessions
                        ⌘+; exit
```

### Context Mode

```
 context · Tab sessions
────────────────────────────────
 ACTIVE
 ✓ ~/.claude/CLAUDE.md
 ✓ CLAUDE.md (project)
 ✓ MEMORY.md
 ✓ rust/rust-conventions.md
 ✓ rust/tauri.md
 ✓ frontend-conventions.md
 ✓ logging-conventions.md
 ✓ tanstack-router-guide.md
 ✓ claude/pr-review-monitor.md
────────────────────────────────
 AVAILABLE
 ❯ html-security.md
   rag-conventions.md
   python/python-conventions.md
   python/python-security-...
   unix/terminals.md
────────────────────────────────
 Enter: add to CLAUDE.md
```

## How It Works

### Context Detection

1. Get active window's `pane_path` (CWD) from `WindowInfo` — already available
2. Parse `~/.claude/CLAUDE.md` for brain-os backlinks using regex: `brain-os/([^\s\x60"]+\.md)` (handles both absolute and relative path styles)
3. If project has its own CLAUDE.md, parse that too
4. Check for project memory: encode CWD path (`/` → `-`) to find `~/.claude/projects/<encoded>/memory/MEMORY.md`
5. Scan `~/workspace/personal/brain-os/**/*.md`, excluding `papers/` and `claude-learnings/`
6. Diff scanned docs against backlinked set → "AVAILABLE" list

### Suggest Action (Enter on an unlinked doc)

When user presses Enter on an available doc:

1. Format backlink: `- **<Label>**: Read \`/Users/rashasaadeh/workspace/personal/brain-os/<path>\``
2. Copy formatted line to clipboard via `pbcopy`
3. Show flash confirmation: `"Copied backlink! Paste into CLAUDE.md"` (3 seconds)

Clipboard approach is safer than auto-editing CLAUDE.md — the Convention Docs section has a specific layout with context-dependent descriptions, and the user should choose where to place it and how to describe it.

### Refresh Strategy

- Scan on mode toggle (entering context view)
- Re-scan every 50 ticks (~5 seconds) while in context mode
- Re-scan on window switch

## Files to Modify

All in `~/workspace/personal/dotfiles/ccs/src/`:

| File                 | Change                                                                                                               |
| -------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `sidebar/context.rs` | **NEW** — `ContextInfo` struct, `scan_context()`, backlink parsing, brain-os scanning                                |
| `sidebar/app.rs`     | Add `ViewMode` enum, `context_selected`, `context: Option<ContextInfo>`, `flash_message`, toggle logic in event loop |
| `sidebar/event.rs`   | Add `ToggleMode` and `Suggest` actions, map `Tab` → toggle, `Enter` behavior changes per mode                        |
| `sidebar/ui.rs`      | Add context view rendering branch, flash message overlay                                                             |
| `sidebar/mod.rs`     | Export `context` module                                                                                              |
| `Cargo.toml`         | Add `serde` + `serde_json` if needed (currently only transitive)                                                     |

## Stack Structure (3 diffs)

1. **`feat: add context scanning module`** — `context.rs` with types + scanning logic
2. **`feat: add context viewer mode to sidebar`** — `app.rs`, `event.rs` mode switching + suggest action
3. **`feat: render context view in sidebar UI`** — `ui.rs` rendering, flash message, updated header/legend

## Verification

1. `cargo build` in `ccs/` — compiles cleanly
2. `ccs start test ~/workspace/personal/brain-os` — launch a session
3. Press `Tab` in sidebar — should toggle to context view showing active backlinks + available docs
4. Navigate to an available doc, press Enter — backlink line should be in clipboard
5. Press `Tab` again — should return to sessions view
6. Switch windows — context view should update to reflect new window's project
