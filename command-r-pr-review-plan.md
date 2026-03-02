# Cove Review TUI — Design Doc

## What this is

A terminal-based code review tool built into Cove. Press `C-a r` in any Claude Code session to replace the Claude pane with a review TUI. Review diffs, see mesa.dev comments inline, leave your own inline comments, and submit feedback back to Claude.

## Workflows

### Pre-push: Review before triggering mesa.dev

```
You're coding with Claude
  → C-a r
  → Review TUI shows diff of current branch (stack-aware, diffs against parent)
  → You review the diff, leave inline comments on lines that need fixing
  → S (submit) → comments piped to Claude as a prompt
  → q → back to Claude, it addresses your comments
  → Once satisfied, gt submit → optionally trigger mesa.dev review
```

The pre-push review lets you catch issues yourself before spending money on mesa.dev. Fix the obvious stuff first, then trigger the paid reviewer on cleaner code.

### Post-push: Handle mesa.dev review

```
You push via gt submit
  → Claude triggers mesa.dev review (manual trigger, not automatic)
  → TUI shows polling indicator: "Waiting for mesa.dev..."
  → Comments arrive → Claude analyzes each one (Phase 2)
  → C-a r
  → Bottom-right tab switches to Mesa Analysis (scrollable comments + recommendations)
  → Bottom-right feedback input → type your response
  → Enter → feedback submitted, input freezes
  → q → back to Claude, it's addressing your feedback
```

---

## TUI Layout

### Full tmux window (with review active)

```
┌─────────────────────────────────────────────┬──────────────────┐
│                                             │                  │
│              REVIEW TUI (C-a r)             │   Cove Sidebar   │
│           replaces Claude pane              │   (sessions)     │
│               see below                     │                  │
│                                             ├──────────────────┤
│                                             │                  │
│                                             │   Terminal       │
│                                             │   (mini shell)   │
│                                             │                  │
└─────────────────────────────────────────────┴──────────────────┘
  70% width                                     30% width
```

### Phase 1 layout

The Phase 1 UI adapts based on context. Before a PR exists (pre-push), you see diffs and your own inline comments. After a PR is pushed and mesa.dev comments exist (post-push), those comments also appear inline on the diff.

**Pre-push state** (no PR yet — reviewing your own code):

```
┌──────────────────────────────────────┬──────────────────────────┐
│                                      │  BRANCH / STACK NAV      │
│         DIFF VIEWER                  │                          │
│                                      │  ▸ feat/auth             │
│  src/lib.rs                          │    ├ add-model       ✓   │
│  ─────────────────────────           │    ├ add-api         ●←  │
│  42  │ -  fn process() {             │    └ add-ui          ○   │
│  42  │ +  fn process() -> Result {   │                          │
│  43  │ +      let data = fetch()?;   │  ▸ feat/dashboard        │
│  44  │    }                          │    ├ add-route       ●   │
│      │                               │    └ add-widgets     ○   │
│      │  ┌─ you ───────────────────┐  │                          │
│      │  │ "Use anyhow here"       │  │    fix/login-bug   [wt] │
│      │  └─────────────────────────┘  │                          │
│      │                               ├──────────────────────────┤
│  src/api.rs                          │  FILES CHANGED (3)       │
│  ─────────────────────────           │                          │
│  15  │ +  pub fn new_endpoint() {    │  ▸ src/lib.rs       +15 │
│  16  │ +      todo!()                │    src/api.rs         +8 │
│      │                               │    src/types.rs       +3 │
└──────────────────────────────────────┴──────────────────────────┘
 [c]omment  [j/k] scroll  [Tab] panel  [S]ubmit  [q]uit
```

**Post-push state** (PR exists — mesa.dev comments appear inline):

```
┌──────────────────────────────────────┬──────────────────────────┐
│                                      │  BRANCH / STACK NAV      │
│         DIFF VIEWER                  │                          │
│                                      │  ▸ feat/auth      PR #14 │
│  src/lib.rs                          │    ├ add-model       ✓   │
│  ─────────────────────────           │    ├ add-api         ●←  │
│  42  │ -  fn process() {             │    └ add-ui          ○   │
│  42  │ +  fn process() -> Result {   │                          │
│  43  │ +      let data = fetch()?;   │  ▸ feat/dashboard PR #15 │
│  44  │    }                          │    ├ add-route       ●   │
│      │                               │    └ add-widgets     ○   │
│      │  ┌─ mesa [high] ───────────┐  │                          │
│      │  │ "No error handling for  │  │    fix/login-bug   [wt] │
│      │  │  the fetch() call"      │  │                          │
│      │  └─────────────────────────┘  ├──────────────────────────┤
│      │                               │  FILES CHANGED (3)       │
│      │  ┌─ you ───────────────────┐  │                          │
│      │  │ "Use anyhow here"       │  │  ▸ src/lib.rs       +15 │
│      │  └─────────────────────────┘  │    src/api.rs         +8 │
│      │                               │    src/types.rs       +3 │
│  src/api.rs                          │                          │
│  ─────────────────────────           │                          │
│  15  │ +  pub fn new_endpoint() {    │                          │
│  16  │ +      todo!()                │                          │
│      │                               │                          │
└──────────────────────────────────────┴──────────────────────────┘
 [c]omment  [j/k] scroll  [Tab] panel  [S]ubmit  [q]uit
```

### Phase 2+ layout (bottom-right gains tabs)

```
┌──────────────────────────────────────┬──────────────────────────┐
│                                      │  BRANCH / STACK NAV      │
│         DIFF VIEWER                  │                          │
│                                      │  ▸ feat/auth      PR #14 │
│  (same as Phase 1)                   │    ├ add-model       ✓   │
│                                      │    ├ add-api         ●←  │
│                                      │    └ add-ui          ○   │
│                                      │                          │
│                                      ├──────────────────────────┤
│                                      │  [Files] [Mesa Analysis] │
│                                      │                          │
│                                      │  ┌─ 1. src/lib:42 ─────┐│
│                                      │  │ [high] "No error    ││
│                                      │  │ handling for fetch()"││
│                                      │  │ claude: ADDRESS      ││
│                                      │  │ "Valid — wrap in     ││
│                                      │  │ Result with ?"       ││
│                                      │  └──────────────────────┘│
│                                      │                          │
│                                      │  ┌─ 2. src/api:15 ─────┐│
│                                      │  │ [low] "Add docs"    ││
│                                      │  │ claude: REJECT       ││
│                                      │  │ "Self-explanatory."  ││
│                                      │  └──────────────────────┘│
│                                      │                          │
│                                      ├──────────────────────────┤
│                                      │  YOUR FEEDBACK:          │
│                                      │  ┌──────────────────────┐│
│                                      │  │ Address 1, skip 2.  ││
│                                      │  └──────────────────────┘│
│                                      │  [Enter] submit          │
└──────────────────────────────────────┴──────────────────────────┘
 [c]omment  [j/k] scroll  [Tab] file/mesa  [S]ubmit  [q]uit
```

After submitting feedback, the input area freezes:

```
├──────────────────────────┤
│  ✓ Feedback submitted    │
│  [q] back to Claude      │
└──────────────────────────┘
```

### Key layout details

- **Left (60%)**: Diff viewer — unified diff, file-by-file. Mesa.dev comments rendered inline under target lines with severity (low/medium/high). Your comments rendered as `you` tags. Scrollable.
- **Top-right**: Branch/stack navigator. Shows Graphite stacks with entries, standalone branches, worktree indicators. Arrow keys + Enter to switch branches, diff viewer updates.
- **Bottom-right (Phase 1)**: File list — changed files with line counts. Arrow keys or Tab to switch files, diff viewer jumps to that file.
- **Bottom-right (Phase 2+)**: Tabbed panel — `[Files]` tab (same file list) and `[Mesa Analysis]` tab (scrollable mesa.dev comments with Claude's recommendations + fixed feedback input at bottom).

### Branch navigator details

The top-right panel shows all branches and stacks:

```
  BRANCHES
  ────────────────────────
  repo: rasha-hantash/cove

  Stacks (Graphite)
  ▸ feat/auth                        PR #14
    ├── 01-add-user-model        ✓   merged
    ├── 02-add-auth-api          ●←  current (you are here)
    └── 03-add-auth-ui           ○   pending review

  ▸ feat/dashboard                   PR #15
    ├── 01-add-dashboard-route   ●   in review
    └── 02-add-widgets           ○   draft

  Branches
    fix/login-bug                [wt] worktree active
    chore/cleanup                     standalone branch

  ● current  ✓ merged  ○ pending
  [wt] = running in a worktree
  ← = your active branch
```

- **Graphite stacks**: grouped, showing position in stack, PR number, entry status
- **Standalone branches**: not part of any stack
- **Worktree indicator**: `[wt]` means this branch has an active git worktree (another Claude session might be working on it)
- **Current marker**: `←` shows which branch the active Claude session is on
- **Navigation**: Arrow keys to highlight, Enter to switch — diff viewer updates to show that branch's changes

### File list details

```
  FILES CHANGED (3)
  ────────────────────────
  ▸ src/lib.rs          +15 -3
    src/api.rs           +8 -0
    src/types.rs         +3 -1

  ▸ = currently viewing
  Tab / ↑↓ to switch
```

Selecting a file scrolls the diff viewer to that file. The `▸` marker tracks which file is in view.

---

## Data flow

### How the review TUI gets its data

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│   Cove events    │     │   Git / Graphite │     │   GitHub API    │
│   ~/.cove/       │     │   (local repo)   │     │   (gh api)      │
│   events/*.jsonl │     │                  │     │                 │
└────────┬────────┘     └────────┬─────────┘     └────────┬────────┘
         │                       │                         │
         ▼                       ▼                         ▼
    pane → session          git diff              mesa.dev comments
    detect cwd + branch     branch list           (raw, Phase 1)
                            stack info            + AI analysis
                                                  (Phase 2)
         │                       │                         │
         └───────────────────────┼─────────────────────────┘
                                 ▼
                          ┌──────────────┐
                          │  Review TUI  │
                          │  (ratatui)   │
                          └──────┬───────┘
                                 │
                    on submit: tmux send-keys
                                 │
                                 ▼
                          ┌──────────────┐
                          │ Claude pane  │
                          │ (receives    │
                          │  feedback)   │
                          └──────────────┘
```

### Mesa.dev comment fetching

**Phase 1 (raw comments):**

The review TUI directly calls `gh api repos/{owner}/{repo}/pulls/{pr}/comments` to fetch mesa.dev comments. No AI analysis — just renders the raw comments inline on the diff with their severity level (low/medium/high).

**Phase 2 (analysis + polling):**

After `gt submit`, Claude triggers mesa.dev review (manual, not automatic) and spawns a background polling process. The TUI shows a status indicator:

```
  ┌────────────────────────────────┐
  │  ⟳ Polling mesa.dev... (2:30)  │
  └────────────────────────────────┘
```

When comments arrive, Claude analyzes each one:

- Should it be addressed? Why/why not?
- Classify: address / partially address / reject
- Write proposed approach if recommending address

Analysis written to `~/.cove/reviews/{pr_number}.json`:

```json
{
  "pr_number": 16,
  "branch": "03-01-feat_add-auth-api",
  "repo": "rasha-hantash/pancake",
  "analyzed_at": "2026-03-01T12:30:00Z",
  "comments": [
    {
      "id": "gh-comment-123",
      "author": "mesa[bot]",
      "file": "src/lib.rs",
      "line": 42,
      "severity": "high",
      "body": "No error handling for the fetch() call",
      "recommendation": "address",
      "reasoning": "Valid — fetch() can fail on network timeout...",
      "proposed_fix": "Wrap in Result, propagate with ?"
    }
  ]
}
```

The sub-agent **never touches code**. It only analyzes and writes recommendations. You decide what gets fixed.

---

## Tmux mechanics: view switching

`C-a r` switches the left 70% from the Claude view to the review view. Claude keeps running underneath.

```bash
# In tmux.conf:
bind r run-shell 'tmux display-popup -E -B -w 70% -h 100% -x 0 -y 0 -d "#{pane_current_path}" "cove review-tui --pane #{pane_id}"'
```

The UX should feel like the Claude pane's content **seamlessly switches** to the review view — not like a dialog or floating window popped up. Key details:

- `-B` removes the popup border entirely. Combined with `-w 70% -h 100% -x 0 -y 0`, the popup covers the Claude pane pixel-for-pixel. Visually, the content just changes.
- `-d "#{pane_current_path}"` sets the popup's working directory to the active pane's cwd (popup cwd is unpredictable otherwise)
- `--pane #{pane_id}` passes the Claude pane's ID to the TUI so it knows where to send feedback (since `$TMUX_PANE` is empty inside a popup — the popup isn't a real pane)
- Claude's process is never interrupted — it keeps running underneath
- When `cove review-tui` exits (user presses `q`), the overlay closes and Claude is right there
- **Note**: while the review TUI is active, keyboard input goes to the review TUI only. The sidebar and terminal panes are still visible but not interactive until you exit the review (`q`)

If `-B` isn't supported (requires tmux 3.3+), fall back to `set -g popup-border-lines none` in tmux.conf.

This is a seamless view switch — press `C-a r` to review, `q` to go back to Claude.

---

## PR number detection

The review TUI needs to know the PR number to fetch mesa.dev comments. Rather than requiring the user to provide it, the system detects it automatically.

**Approach: Cove hook watches for PR creation events**

Cove already hooks into Claude Code events via `UserPromptSubmit`, `Stop`, `PreToolUse`, and `PostToolUse`. The `PostToolUse` hook receives tool output — when Claude runs `gt submit` or `gh pr create`, the output contains the PR URL/number.

Detection flow:

```
Claude runs `gt submit` or `gh pr create`
  → PostToolUse hook fires with tool output
  → cove hook post_tool_use receives JSON via stdin
  → Parse output for PR URL pattern (github.com/.../pull/\d+)
  → Write PR number to event JSONL: {"state": "pr_created", "pr_number": 16, ...}
  → Review TUI reads this from events, knows the PR number
```

This is **agnostic to GitHub vs Graphite** — it just pattern-matches the PR URL from command output. Works with `gt submit`, `gh pr create`, or any tool that outputs a PR link.

If no PR has been created yet (pre-push workflow), the TUI simply doesn't fetch mesa.dev comments and the branch navigator shows branches without PR numbers.

---

## Comment submission

### Inline comments (Phase 1 — from the diff viewer)

Press `c` on a diff line to leave a comment (tagged as `you`). Press `S` to submit all inline comments. The TUI formats them and sends to Claude via `tmux send-keys -t :.1`:

```
Please address these review comments:

src/lib.rs:42 — Use anyhow here instead of std Result
src/lib.rs:78 — Rename `x` to something descriptive
src/api.rs:20 — This endpoint needs auth middleware
```

### Mesa.dev feedback (Phase 2 — from the bottom-right input)

Your single free-form response to the mesa.dev analysis. The TUI sends Claude the full context:

```
Mesa.dev left 3 comments on PR #16. Here's my analysis and the user's feedback:

1. src/lib.rs:42 — [high] "No error handling for fetch()"
   My recommendation: ADDRESS — fetch() can fail on timeout
2. src/api.rs:15 — [low] "Add documentation"
   My recommendation: REJECT — name is self-explanatory
3. src/lib.rs:44 — [medium] "Use anyhow::Result"
   My recommendation: ADDRESS — aligns with project conventions

User's feedback: "Address 1 and 3 but use anyhow. Skip 2."

Please address the comments according to the user's feedback.
```

### Mechanics

1. On submit, feedback is sent to Claude pane via `tmux send-keys -t :.1`
2. For mesa.dev feedback, the input freezes to "✓ Feedback submitted"
3. Press `q` → review closes → back to Claude → Claude is already processing

---

## Implementation phases

### Phase 1: Core review TUI

The UI adapts based on context — pre-push (no PR) shows diffs + your inline comments; post-push (PR exists) also shows raw mesa.dev comments inline.

- `cove review-tui` command — ratatui app
- Diff viewer: parse unified diff, render with +/- coloring
- Branch/stack navigator (top-right): Graphite stacks with entries, standalone branches, worktree indicators, arrow+Enter to switch
- File list (bottom-right): changed files with line counts, Tab/arrow to switch
- PR number detection: parse `PostToolUse` hook output for PR URLs, store in event JSONL
- Mesa.dev comments (post-push only): raw fetch via `gh api` when PR number is known, rendered inline with severity (low/medium/high)
- Inline commenting: press `c` on a line, type comment (tagged as `you`)
- Submit (`S`): format inline comments, send via `tmux send-keys`
- Tmux binding: `bind r display-popup ...`

**This gets you `C-a r` → review diffs → navigate branches/stacks → leave comments → submit to Claude → `q` back.**

### Phase 2: Mesa.dev analysis + feedback loop

- Manual mesa.dev trigger from Claude (not automatic on push)
- Polling indicator in TUI ("Waiting for mesa.dev...")
- Background sub-agent analyzes mesa.dev comments (never touches code)
- Write analysis JSON to `~/.cove/reviews/`
- Bottom-right gains tabs: `[Files]` and `[Mesa Analysis]`
- Mesa Analysis tab: scrollable comment cards with recommendations
- Fixed feedback input at bottom of Mesa Analysis tab
- Submit feedback → piped to Claude via `tmux send-keys`
- Input freezes to "✓ Feedback submitted" after submit

### Phase 3: Polish

- Cove sidebar "review ready" indicator when mesa.dev analysis is available
- Syntax highlighting in diff viewer (tree-sitter or simple keyword coloring — colors keywords like `fn`, `let`, `Result` as in an editor)
- Persistent review state: inline comments survive across `C-a r` toggles (saved to disk, restored on re-open)

---

## Key design decisions

1. **TUI in Cove, not a separate app** — stays in the terminal, no context switching.
2. **View switching via `display-popup`** — overlays the review TUI on the Claude pane without killing the process. `q` to switch back. Simple and reversible.
3. **Sub-agent analyzes, human decides** — mesa.dev comments get AI-powered recommendations but the user approves every action. Full transparency.
4. **Manual mesa.dev trigger** — not automatic on push. Saves money by letting you review code yourself first, then trigger the paid reviewer on cleaner code.
5. **`tmux send-keys` for submission** — zero-infrastructure way to pipe review comments back to Claude. No custom hooks, no IPC, no file watchers.
6. **Stack-aware diffing** — Graphite stacks diff against parent branch, not main. Standalone branches diff against detected base.
7. **Bottom-right evolves across phases** — Phase 1: file list. Phase 2: tabbed (files + mesa analysis). Same space, progressive enhancement.

---

## Files involved

### Cove (~/workspace/personal/cove/)

| File                     | Change                                                                                                                      |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------- |
| `src/cli.rs`             | Add `ReviewTui` subcommand                                                                                                  |
| `src/main.rs`            | Wire to `review::run()`                                                                                                     |
| `src/commands/mod.rs`    | Add `pub mod review;`                                                                                                       |
| `src/commands/review.rs` | **New** — detect session, launch TUI                                                                                        |
| `src/review/mod.rs`      | **New** — ratatui app, event loop                                                                                           |
| `src/review/diff.rs`     | **New** — diff parser (unified → structured)                                                                                |
| `src/review/ui.rs`       | **New** — ratatui widgets (diff, branch info, file list)                                                                    |
| `src/review/comments.rs` | **New** — inline comment model, submission formatting                                                                       |
| `src/review/mesa.rs`     | **New** — Phase 1: fetch raw comments via `gh api`, render inline. Phase 2: read analysis JSON, render in Mesa Analysis tab |
| `src/review/branches.rs` | **New** — branch/stack detection, diff base resolution                                                                      |
| `src/events.rs`          | **New** — extract shared helpers from `sidebar/state.rs`                                                                    |

### Dotfiles (~/workspace/personal/dotfiles/)

| File             | Change                                                                                                                                   |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `tmux/tmux.conf` | Add `bind r run-shell 'tmux display-popup -E -B -w 70% -h 100% -x 0 -y 0 -d "#{pane_current_path}" "cove review-tui --pane #{pane_id}"'` |

---

## Open questions

- **Diff base detection**: For non-Graphite branches, how to detect the right base? `git merge-base HEAD main`? Configurable?
- **Large diffs**: Should we collapse unchanged regions (context folding)?
- **Mesa.dev trigger**: What does the manual trigger look like? A Claude command? A `gh` workflow dispatch? Need to check mesa.dev's API.
- **Multiple PRs**: If a stack has 3 PRs and mesa.dev reviewed all of them, how to navigate between reviews in the Mesa Analysis tab?
- **Sub-agent lifetime**: Does the mesa.dev polling agent live per-PR or per-session? What happens if you push a new commit to the same PR?

---

## Risks and assumptions

Technical risks identified before implementation. Each has been investigated and marked with validation status.

### tmux `display-popup` — all validated ✅

| Risk                           | Detail                                                                  | Status                                                                              |
| ------------------------------ | ----------------------------------------------------------------------- | ----------------------------------------------------------------------------------- |
| **PTY inside popup**           | ratatui needs raw mode + alternate screen inside the popup PTY          | ✅ tmux 3.6a, `$TERM=tmux-256color` inside popup. Should work.                      |
| **Popup sizing**               | `-w 70% -h 100%` percentage-based sizing requires tmux 3.3+             | ✅ Confirmed on tmux 3.6a. Min version: 3.3.                                        |
| **Popup vs. pane coordinates** | `-x 0 -y 0` positioning with `status-position top` — could misalign     | ❓ Needs visual test. Adjust `-y` offset if needed.                                 |
| **Mouse events in popup**      | Does `display-popup` forward mouse events?                              | ✅ `set -g mouse on` is enabled, events propagate.                                  |
| **Cross-pane tmux commands**   | Can a process inside popup run `tmux send-keys` targeting parent panes? | ✅ Confirmed: `send-keys`, `list-panes`, `capture-pane` all work from inside popup. |
| **`$TMUX_PANE` empty**         | Popup isn't a real pane — env var is empty                              | ✅ Resolved: binding passes `--pane #{pane_id}` as CLI arg.                         |
| **Popup cwd unpredictable**    | Popup doesn't reliably inherit active pane's cwd                        | ✅ Resolved: binding uses `-d "#{pane_current_path}"`.                              |
| **Borderless (`-B` flag)**     | Needed for seamless view-switch UX                                      | ✅ Supported in tmux 3.6a. Man page confirms `[-BCEkN]` flags.                      |

### ratatui — not yet validated

| Risk                          | Detail                                                                                               | How to validate                                                                                                     |
| ----------------------------- | ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| **Multi-panel layout**        | 3 panels with independent scroll, focus tracking. No built-in panel/focus manager.                   | Study `gitui` patterns for focus ring. Pull technical-rag context before writing `ui.rs`.                           |
| **Inline comment insertion**  | Mixed stream of diff hunks + comment blocks with different styling.                                  | Prototype `Vec<DiffElement>` where `DiffElement` is `Line(...)` or `Comment(...)`. Test with ratatui `List` widget. |
| **Text input widget**         | ratatui has no built-in text input. Need `tui-textarea` or custom.                                   | Evaluate `tui-textarea` crate for single-line (Phase 1) and multi-line (Phase 2) input.                             |
| **Large diff performance**    | 5,000+ line diff could be sluggish.                                                                  | Benchmark with synthetic diff. Switch to windowed rendering if needed.                                              |
| **Alternate screen in popup** | Sidebar uses no alternate screen; review TUI should. Both run in separate PTYs — shouldn't conflict. | Validate: open sidebar → `C-a r` → `q` → confirm sidebar didn't glitch.                                             |

### Graphite CLI — validated ✅

| Risk                     | Detail                                                 | Status                                                                                                                                                         |
| ------------------------ | ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Stack detection**      | Need branch graph with parent relationships and order. | ✅ `gt state` returns structured JSON with `parents`, `trunk`, `needs_restack` per branch. (`gt log --json` doesn't exist, `gt stack` is not an info command.) |
| **Diff base resolution** | Need parent branch for stack-aware diffing.            | ✅ `gt parent` returns parent branch name as plain text. `refs/branch-metadata/<branch>` has `parentBranchName` in JSON.                                       |
| **PR numbers**           | Need PR number per branch — not in `gt state` JSON.    | ✅ `gt branch info --branch <name>` outputs `PR #N (Draft) ...`. Extract with regex `PR #(\d+)`. N subprocess calls for N branches — cache results.            |

### PostToolUse hook for PR detection — validated ✅

| Risk                    | Detail                                                                        | Status                                                                                                                                    |
| ----------------------- | ----------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| **Hook JSON schema**    | Does PostToolUse include tool output?                                         | ✅ Yes — `tool_input.command` and `tool_output.stdout/stderr` are in the payload. Confirmed by existing `detect-git-init.py` hook.        |
| **MCP vs. bash output** | `gt submit` may run as MCP tool (Graphite MCP) with different JSON structure. | ❓ Not yet tested. PostToolUse hooks only fire for specific matchers — MCP tool calls may not trigger Bash-matched hooks. Need to verify. |
| **Timing**              | Is full output captured by the time the hook fires?                           | ❓ Not yet tested with multi-branch stacks. Validate during E2E spike.                                                                    |

### `tmux send-keys` for feedback — not yet validated

| Risk                   | Detail                                                            | How to validate                                                                             |
| ---------------------- | ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| **Multi-line input**   | Special characters (`"`, `'`, `$`, backticks) might get mangled.  | Test during E2E spike. Fall back to `tmux load-buffer` + `tmux paste-buffer` if unreliable. |
| **Claude input state** | Sending while Claude is mid-output would appear in wrong context. | Check Claude state from event JSONL before sending. Only send when "idle" or "asking".      |

### mesa.dev API — validated ✅

| Risk               | Detail                                                               | Status                                                                                                                      |
| ------------------ | -------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| **Comment format** | Which API endpoint? Review comments, issue comments, or proprietary? | ✅ Inline review comments on `pulls/{pr}/comments`. Description summaries (separate feature) on `issues/{pr}/comments`.     |
| **Severity field** | Structured field, label, or embedded in body?                        | ✅ Embedded as markdown image: `![High\|Medium\|Low](url)` at start of body. Extract with regex `!\[(High\|Medium\|Low)\]`. |
| **Rate limits**    | Polling too frequently could hit GitHub's 5,000/hour limit.          | Use 30s polling interval + conditional requests (`If-None-Match` / ETag).                                                   |

---

## Validated data sources

Reference for implementation — confirmed APIs and data formats.

### PostToolUse hook payload (for PR detection)

The Cove hook currently only parses `session_id` and `cwd`, but the Claude Code payload includes more:

```python
tool_input = event.get("tool_input", {})   # has "command" for Bash tools
tool_output = event.get("tool_output", {})  # has "stdout" and "stderr"
```

PR detection: match `tool_input.command` for `gt submit` / `gh pr create`, extract PR URLs from `tool_output.stdout`.

### Graphite CLI data sources

| Data needed                        | Source                                          | Format                              |
| ---------------------------------- | ----------------------------------------------- | ----------------------------------- |
| Full branch graph with parents     | `gt state`                                      | JSON                                |
| Parent of current branch           | `gt parent`                                     | plain text (single line)            |
| Per-branch metadata (SHAs, parent) | `git cat-file -p refs/branch-metadata/<branch>` | JSON                                |
| PR number per branch               | `gt branch info --branch <name>`                | human text, needs regex `PR #(\d+)` |

Note: `gt log --json` doesn't exist, `gt stack` is not an info command. `gt state` is the only JSON source.

### Mesa.dev comment format

Mesa.dev works via the GitHub App — sync the repo on mesa.dev's web UI, no local config needed. Inline review comments appear on PRs with substantive code changes.

**Primary endpoint:** `GET repos/{owner}/{repo}/pulls/{pr}/comments` — filter by `user.login == "mesa-dot-dev[bot]"`

```json
{
  "user": { "login": "mesa-dot-dev[bot]" },
  "path": "src-tauri/src/lib.rs",
  "line": 122,
  "side": "RIGHT",
  "diff_hunk": "@@ -0,0 +1,485 @@ ...",
  "body": "![Medium](https://mesa-production-.../medium-severity-v2.svg)\n\nWeak hash function: Using DefaultHasher...",
  "pull_request_review_id": 3786367781
}
```

**Severity:** Markdown image at start of body: `![High|Medium|Low](url)`. Extract with regex `!\[(High|Medium|Low)\]`. Review text follows after two newlines.

**Other endpoints:** `pulls/{pr}/reviews` has summary analysis; `issues/{pr}/comments` has PR description summaries (separate feature, not code review).

**Remaining action:** Sync the cove repo on mesa.dev's web UI (currently only active on pancake).

### Minor Cove bugs to fix

- **JSON escaping**: `write_event()` uses raw format string — `cwd` with `"` chars corrupts JSON. Fix: use `serde_json::to_string()`.
- **No file locking**: JSONL writes have no advisory locking. Mitigation: skip unparseable lines (sidebar already does this).

---

## E2E validation spike — Phase 1 data layer

Validate every external dependency end-to-end before writing UI code. This is a single Rust binary (`cove review-spike --pane <id>`) that runs each data source, prints what it got, and reports pass/fail. No ratatui, no layout, no styling — just the data layer proving it can fetch everything the TUI will need.

### Test 1: Session context detection

**What to test:** Given a `--pane` arg (e.g., `%1`), can we find the matching Cove session and extract `cwd`?

```
Input:  --pane %1
Action: Scan ~/.cove/events/*.jsonl → read last line of each → find entry where pane_id == "%1"
Output: ✅ Session found: session_id=abc123, cwd=/Users/.../workspace/personal/cove
   or: ❌ No session found for pane %1
```

**How to run:** Launch from `tmux display-popup` with `--pane #{pane_id}` in binding. Confirm it finds the active Claude session's event file.

### Test 2: Branch + parent detection

**What to test:** From the detected `cwd`, can we get the current branch and its parent?

```
Action: git -C {cwd} branch --show-current
        gt parent (run from cwd)
Output: ✅ Branch: 03-01-docs_add_command-r_pr_review_tui_design_doc
        ✅ Parent: main
   or: ❌ git branch failed (detached HEAD? not a git repo?)
```

### Test 3: Diff against parent

**What to test:** Can we get the diff between the current branch and its parent?

```
Action: git -C {cwd} diff {parent}..HEAD
Output: ✅ Diff: 2 files changed, 695 insertions
        Print first 20 lines of raw diff to stdout
   or: ❌ git diff failed (parent branch doesn't exist locally?)
```

**Edge cases to watch for:**

- Parent branch not fetched locally → need `git fetch origin {parent}` first?
- Detached HEAD → `git branch --show-current` returns empty
- Binary files in diff → unified diff won't show content

### Test 4: Stack graph

**What to test:** Can we parse `gt state` JSON and reconstruct the branch tree?

```
Action: gt state (run from cwd, capture stdout, parse JSON)
Output: ✅ Stack graph parsed: 10 branches, 1 trunk
        Print tree structure:
          main (trunk)
          ├── feat/auth
          │   ├── add-model
          │   └── add-api ← current
          └── docs/review-tui
   or: ❌ gt state failed or JSON parse error
```

**What to check:** Does every non-trunk branch have a `parents` array? Are parent refs resolvable? Do `needs_restack` flags appear correctly?

### Test 5: PR number extraction

**What to test:** Can we regex-parse PR numbers from `gt branch info`?

```
Action: gt branch info --branch {current} (capture stdout)
        Regex: PR #(\d+)
Output: ✅ PR #9 found for branch 03-01-docs_add_command-r_pr_review_tui_design_doc
   or: ✅ No PR yet (pre-push state) — this is valid
   or: ❌ gt branch info failed or regex didn't match despite PR existing
```

**Batch test:** Run for all branches in `gt state` to validate N subprocess calls work reliably.

### Test 6: Mesa.dev comments

**What to test:** Can we fetch and parse mesa.dev inline review comments for a PR?

**Format already validated** on pancake PR #4. Mesa.dev posts line-level review comments on `pulls/{pr}/comments` with severity as `![High|Medium|Low]` markdown image at the start of the body.

```
Action: Detect repo owner/name from git remote
        gh api repos/{owner}/{repo}/pulls/{pr}/comments
          → filter user.login == "mesa-dot-dev[bot]"
          → for each: extract path, line, severity (regex !\[(High|Medium|Low)\]), body text
Output: ✅ Found 10 mesa inline comments on PR #4
        ✅ Parsed severity: 2 High, 8 Medium
        ✅ Each comment has path + line number for inline rendering
        Print: file, line, severity, first 80 chars of body
   or: ✅ No PR number known — skip mesa check (valid for pre-push)
   or: ✅ 0 mesa comments — PR may be docs-only or mesa not installed on repo
   or: ❌ gh api failed (auth? rate limit? repo not found?)
```

**Prerequisite:** Sync the cove repo on mesa.dev's web UI (currently only active on pancake). No local config needed.

### Test 7: Send-keys roundtrip

**What to test:** Can we send text from inside the popup to the Claude pane?

```
Action: Check Claude state from event JSONL (should be "idle" or "asking")
        tmux send-keys -t {pane} "# review-tui test message — please ignore" Enter
Output: ✅ Claude pane state: idle — safe to send
        ✅ send-keys executed (exit code 0)
   or: ❌ Claude pane state: working — NOT safe to send (would interrupt)
   or: ❌ send-keys failed (pane doesn't exist? wrong target?)
```

**Important:** Use a comment-prefixed message (`#`) so Claude ignores it if the test accidentally fires during a real session.

### Test 8: PostToolUse PR detection hook

**What to test:** Does the PostToolUse hook payload include `tool_output.stdout` with PR URLs when running `gt submit`?

```
Action: Add a temporary PostToolUse(Bash) hook that dumps raw stdin JSON to /tmp/post-tool-use-dump.json
        Trigger a Bash tool call in Claude (e.g., run `echo hello`)
        Read /tmp/post-tool-use-dump.json
Output: ✅ Payload includes tool_input.command and tool_output.stdout
        Print: full JSON schema with field names and types
   or: ❌ Payload is missing tool_output (would need alternative PR detection)
```

**Already partially validated:** The existing `detect-git-init.py` hook parses `tool_output.stdout` successfully, confirming the field exists for Bash tool calls.

### Test 9: ratatui inside display-popup

**What to test:** Can a ratatui app render correctly inside a borderless `display-popup`?

```
Action: Build a minimal ratatui app that:
        1. Enters alternate screen + raw mode
        2. Renders a frame with colored text (Catppuccin palette)
        3. Queries terminal size via terminal.size()
        4. Waits for a single keypress (confirms keyboard events arrive)
        5. Exits cleanly (restores terminal state)
        Run it inside: tmux display-popup -E -B -w 70% -h 100% -x 0 -y 0 'target/debug/ratatui-spike'
Output: ✅ Terminal size: 131x50 (should be ~70% of window width)
        ✅ Keyboard event received: 'q'
        ✅ Clean exit (no terminal corruption)
   or: ❌ Terminal size wrong (popup not reporting dimensions correctly)
   or: ❌ No keyboard events (input not forwarded to popup)
   or: ❌ Terminal corrupted after exit (alternate screen not restored)
```

### Test 10: Sidebar survives popup lifecycle

**What to test:** Does opening and closing the review popup glitch the Cove sidebar (which runs its own ratatui instance in a separate pane)?

```
Action: Capture sidebar pane content: tmux capture-pane -t :.2 -p > /tmp/sidebar-before.txt
        Open popup with ratatui spike → press q to exit
        Capture again: tmux capture-pane -t :.2 -p > /tmp/sidebar-after.txt
        Compare: diff /tmp/sidebar-before.txt /tmp/sidebar-after.txt
Output: ✅ Sidebar output identical before and after (no glitch)
   or: ❌ Sidebar output differs (terminal state leaked between PTYs)
```

### Test 11: Terminal capabilities inside popup

**What to test:** Does the popup PTY support the color depth and features ratatui needs?

```
Action: From inside the popup, check:
        - echo $TERM (expect: tmux-256color)
        - tput colors (expect: 256)
        - printf "\x1b[38;2;255;0;0mTRUECOLOR\x1b[0m" (expect: red text if truecolor works)
Output: ✅ TERM=tmux-256color, 256 colors, truecolor supported
   or: ⚠️ No truecolor — fall back to 256-color Catppuccin palette
   or: ❌ TERM not set or < 256 colors (ratatui styling will break)
```

### Test 12: Send-keys with special characters

**What to test:** Does `tmux send-keys` correctly deliver multi-line text with special characters?

```
Action: Send a test payload from inside the popup:
        tmux send-keys -t {pane} '# Test: "quotes" $dollar `backticks` and
        # second line with (parens) & ampersand' Enter
        Wait 500ms
        Capture pane content: tmux capture-pane -t {pane} -p | tail -5
        Compare sent text vs captured text
Output: ✅ All characters arrived intact
   or: ❌ Characters mangled — switch to tmux load-buffer + paste-buffer approach
```

**If send-keys fails:** Fall back to `tmux load-buffer - <<< "$text" && tmux paste-buffer -t {pane}` which bypasses keystroke simulation entirely.

### Test 13: Popup dimensions match expected layout

**What to test:** Does the popup's terminal size correctly reflect the 70% width we requested?

```
Action: From inside the popup, query terminal dimensions:
        - stty size (rows cols)
        - tput cols / tput lines
        Compare to expected: cols should be ~70% of tmux window width
        Also query the full window: tmux display-message -p '#{window_width}'
Output: ✅ Window: 188 cols → Popup: ~131 cols (70%) ✓
        ✅ Rows match full window height
   or: ❌ Popup dimensions wrong (layout calculations will be off)
```

### Running the spike

```bash
# 1. Build the spike binary
cd ~/workspace/personal/cove
cargo build  # spike is a subcommand of cove

# 2. Run data layer tests directly (no popup needed for tests 1-8)
./target/debug/cove review-spike --pane %1

# 3. Run rendering tests inside display-popup (tests 9-13)
tmux display-popup -E -B -w 70% -h 100% -x 0 -y 0 \
  -d "#{pane_current_path}" \
  "./target/debug/cove review-spike --pane #{pane_id} --rendering-tests"
```

### Pass criteria

All 13 tests print their output and exit. No full TUI, no multi-panel layout — just targeted validations. If any test fails, the error tells us exactly what to fix before building the UI.

| Test                       | Must pass for Phase 1         | Must pass for Phase 2 |
| -------------------------- | ----------------------------- | --------------------- |
| 1. Session context         | ✅                            | ✅                    |
| 2. Branch + parent         | ✅                            | ✅                    |
| 3. Diff against parent     | ✅                            | ✅                    |
| 4. Stack graph             | ✅                            | ✅                    |
| 5. PR number               | ✅                            | ✅                    |
| 6. Mesa.dev comments       | ✅                            | ✅                    |
| 7. Send-keys roundtrip     | ✅                            | ✅                    |
| 8. PostToolUse hook        | ❌ (on-demand fallback works) | ✅                    |
| 9. ratatui in popup        | ✅                            | ✅                    |
| 10. Sidebar survives popup | ✅                            | ✅                    |
| 11. Terminal capabilities  | ✅                            | ✅                    |
| 12. Special char send-keys | ✅                            | ✅                    |
| 13. Popup dimensions       | ✅                            | ✅                    |

### What this skips

- Multi-panel layout (3-panel split with focus management)
- Inline comment model + rendering
- Diff parsing into structured hunks (the spike just prints raw diff)
- Text input widgets
- Mesa.dev analysis sub-agent (Phase 2)

### What requires visual validation (by you)

These can't be automated — you need to look at the screen:

- Does `-x 0 -y 0` align exactly with the Claude pane (no gap, no status bar overlap)?
- Does the borderless popup feel like a seamless view switch?
- Do Catppuccin colors render correctly inside the popup?
