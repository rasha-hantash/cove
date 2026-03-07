# Brain-OS Provenance — Inline Citations from Session Transcripts

## What this is

A system that links brain-os convention docs to the Claude Code session transcripts that produced them. Every piece of knowledge gets inline citations pointing to session transcripts — like a research paper citing its sources. This enables a `/doctor` command to trace, validate, enrich, and reorganize knowledge with full context.

## First principles

1. **Traceability** — every piece of knowledge should be connected to its source conversation
2. **Reliability** — capture should be visible and never silently fail
3. **Accessibility** — source conversations should be viewable from the brain-os frontend, not buried in local files
4. **Signal over noise** — capture anything that saves future-you real time, whether it's a one-sentence gotcha or a multi-paragraph pattern. The quality bar is "is this true and useful?" not "is this long enough?"
5. **Auditability** — you should be able to assess knowledge health: what's well-sourced, what's thin, what's stale

## Why

Convention docs today are statements without provenance. When a paragraph says "use XDG_CONFIG_HOME for config files," there's no way to know: what conversation produced this? Was there more nuance discussed but not captured? Is this still accurate given what we've learned since?

With provenance:

- `/doctor` can pull up the original conversation to expand thin entries
- Confusing paragraphs can be traced back to the discussion that inspired them
- Reorganization is safe — citations travel with the text
- Quality is auditable — entries without citations are flagged

## How it works

### Inline citations via markdown footnotes

Convention docs use standard markdown footnotes that reference session transcripts:

```markdown
## Isolating app instances with XDG

Override XDG vars to run an app with a completely separate config.[^1]

**Use cases:**

- Purpose-built tool configs — e.g., a review-only Neovim instance[^1]
- Testing app configs without affecting your daily setup[^2]

[^1]: session:07e94319:247:45-187 "Cove neovim explorer — XDG isolation discussion"

[^2]: session:a3b1c9d2:85:0-312 "Testing configs without affecting daily setup"
```

**Citation format:** `session:<prefix>:<lines>[:<start>-<end>] "<description>"`

- `prefix`: first 8 chars of the session UUID (enough to uniquely identify — 4.3 billion possibilities, collision essentially impossible for a personal knowledge base)
- `lines`: single line number (`247`) or line range (`247-250`). Points to the specific JSONL entry or range of entries that produced the insight.
- `start-end` (optional): character range for precise highlighting. For a single line, highlights within that entry's text. For a line range, `start` = char position in the first line's text, `end` = char position in the last line's text, all intermediate lines highlighted in full (text-selection model). Omit for whole-entry citations.
- `description`: brief human-readable note about what this citation covers. Also serves as a search term when browsing the transcript.

**Citation precision levels:**

| Form                     | Example                           | Highlights                                                                        |
| ------------------------ | --------------------------------- | --------------------------------------------------------------------------------- |
| Single entry, exact span | `session:07e94319:247:45-187`     | Chars 45-187 of line 247                                                          |
| Single entry, whole      | `session:07e94319:247`            | All text in line 247                                                              |
| Multi-entry, whole       | `session:07e94319:247-250`        | All text in lines 247-250                                                         |
| Multi-entry, partial     | `session:07e94319:247-250:45-187` | From char 45 of line 247 through char 187 of line 250 (248-249 fully highlighted) |

**Multiple footnotes** can also compose citations: `[^1][^2]` where each footnote points to a different entry. Use whichever is cleaner — a single line-range citation or multiple footnotes.

**How text content is extracted for character offsets:** Each JSONL entry has a `message.content` field that's either a string or an array of content blocks. Extract all `text`-type blocks and concatenate them — character offsets reference this concatenated text. This extraction method is the same in the capture process and the frontend, so highlights always align.

**Why this is stable:** JSONL files are strictly append-only — entries are never edited, deleted, or reordered. Rewind/undo adds new entries referencing the rewind point, it doesn't modify existing lines. Line numbers and character offsets within each line are permanent and stable, like page numbers and coordinates in a printed book.

**How citations are captured:**

- **Exit capture (`claude -p`):** `brain-os-capture.py` receives `session_id` and `transcript_path` directly from the hook payload (stdin JSON) or from cove's `--session-id` flag — no JSONL discovery needed. The script passes filtered entries with their original line numbers and extracted text to `claude -p`. The extraction prompt says "cite the line number and character range of the text that produced each insight." The LLM sees `[line 247, chars 0-500] user: "what about XDG isolation? I want to..."` and cites `:247:45-187` for the specific span.
- **Organic capture (mid-session):** Claude has full conversational context and knows what's worth capturing. This is the only path that needs JSONL discovery (exit paths get it handed to them). There is no `$CLAUDE_SESSION_ID` env var — Claude finds its JSONL by: (1) encode the CWD by replacing `/` with `-` to get the project directory, (2) find the most recently modified `.jsonl` in `~/.claude/projects/<encoded-path>/`. **Worktree caveat:** JSONL files live at the project root's encoded path, not the worktree's. If no JSONL is found at the encoded CWD, strip `/.claude/worktrees/<name>` from the CWD and try the project root (e.g., `~/workspace/.../cove/.claude/worktrees/feature-x` → strip → `~/workspace/.../cove` → encode → find JSONL). This is deterministic for cove sessions. Claude reads near the tail to find the relevant entry and its line number. Character offsets are best-effort.

### Resolving a citation

**V1 (local):** Scan `~/.claude/projects/` for a file matching `<prefix>*.jsonl` -> jump to cited line -> highlight characters `start-end`.

**V2 (GitHub archive):** Clone/pull the `brain-os-transcripts` repo, same local resolution.

**V3 (cloud):** R2 + Cloudflare Worker API: `GET /api/sessions/07e94319?line=247&start=45&end=187` returns the cited line with highlight range and surrounding entries.

### JSONL transcript structure

Session transcripts live at `~/.claude/projects/<encoded-project-path>/<session-id>.jsonl`. Each line is a JSON object:

```
Type               | Typical % | Contains
-------------------|-----------|------------------------------------------
progress           | ~56%      | Hook events, tool execution progress (noise)
assistant          | ~22%      | Claude's responses + tool calls
user               | ~16%      | Your messages + tool results
system             | ~3%       | System prompts, context injection
file-history       | ~2%       | File snapshots before edits
queue-operation    | <1%       | Internal queue operations
```

Key fields per entry: `type`, `sessionId`, `cwd`, `gitBranch`, `message.content`, `timestamp`.

**Actual sizes observed:**

| Metric                              | Value                     |
| ----------------------------------- | ------------------------- |
| Typical session                     | 1-5MB, 500-2000 lines     |
| Largest session                     | 17MB, 74,424 lines        |
| Meaningful text entries per session | ~83 out of ~1000 (8%)     |
| Total JSONL on disk                 | ~396MB across ~5500 files |

**Persistence:** JSONL files survive `/clear`, context compaction, and session exit. Claude Code may auto-delete after 30 days — the archive (GitHub or R2) provides permanent storage.

**Location pattern:** `~/.claude/projects/-Users-rashasaadeh-workspace-personal-explorations-<repo>/<session-uuid>.jsonl`

### Filtering JSONL for extraction

Before passing a transcript to `claude -p` for learnings extraction, filter out noise:

```python
# Keep only entries with actual conversation content, preserving line numbers
KEEP_TYPES = {"user", "assistant"}

def filter_transcript(jsonl_path):
    meaningful = []
    for line_num, line in enumerate(open(jsonl_path), 1):
        entry = json.loads(line)
        if entry.get("type") not in KEEP_TYPES:
            continue
        content = entry.get("message", {}).get("content", "")
        # Skip tool-only entries (no text content)
        if isinstance(content, list):
            has_text = any(b.get("type") == "text" and len(b.get("text", "")) > 20
                         for b in content if isinstance(b, dict))
            if not has_text:
                continue
        entry["_line_number"] = line_num  # preserve for citation
        meaningful.append(entry)
    return meaningful
```

Line numbers are preserved through filtering so `claude -p` can cite the original JSONL position (e.g., `[line 247] user: "what about XDG isolation?"`). These line numbers are permanent — JSONL is append-only.

This reduces a typical 1000-line transcript to ~83 meaningful entries (~50-100KB of text). Even the largest session (74K lines) becomes manageable for `claude -p`'s context window after filtering.

For very long sessions, chunk by compact-summary boundaries if present in the JSONL, or by time windows (e.g., 30-minute blocks).

---

## Capturing provenance

### Two capture paths — both read from JSONL

Both paths extract from the same JSONL transcript. The difference is context and timing:

**1. During the session (organic):** When Claude proactively notices something worth capturing, or when the user asks to record a learning, Claude writes directly to brain-os convention docs with footnote citations. Claude has full conversational context — it knows what was important, what was nuanced, what the user really cared about. This produces the highest quality entries.

**2. At session exit (systematic):** A capture script reads the full JSONL transcript (filtered), passes it to `claude -p`, which extracts any uncaptured insights and writes them to brain-os with citations. This catches things that weren't captured organically — including tiny nuggets ("Airtable caps at 250k records per base regardless of plan") that Claude didn't flag mid-session.

Both paths can produce signal. Organic capture has better context; exit capture has the full session view and catches things missed in the moment.

**Critical constraint — no nested Claude:** Organic capture and the `/capture` skill must NEVER call `claude -p`. Claude does the work inline using its own context. Only exit paths (cove kill script, SessionEnd hook) use `claude -p`, and only because Claude has already exited or because the capture runs outside the Claude process (cove is Rust, not Claude).

### Cove kill flow — synchronous capture

When the user runs `cove kill <name>` or `cove all-kill`:

```
cove kill <name>
  1. Run capture script: `brain-os-capture.py --session-id $ID`
  2. Script reads + filters the JSONL transcript
  3. Script runs `claude -p` with extraction prompt + filtered transcript
  4. Show progress in terminal: "Analyzing session for learnings..."
  5. If learnings found:
     a. Show summary:
        "Found 3 learnings:
           - XDG Base Directory conventions -> unix/xdg-conventions.md
           - Neovim isolation pattern -> claude/claude.md
           - JSONL transcript persistence -> claude/claude.md"
     b. Write to brain-os convention docs with citations
     c. Create PR via gt
     d. Show: "PR created: https://app.graphite.com/github/pr/..."
  6. If no learnings: Show "No new learnings detected."
  7. Show: "Ready to exit. Press Enter to close or Ctrl-C to cancel."
  8. User confirms -> send /exit -> kill window
```

**Key design decisions:**

- **Capture logic is a standalone Python script**, not Rust code in cove. `brain-os-capture.py` is the single source of truth for extraction logic. Cove just calls it as a subprocess and displays output. This means the capture logic can be iterated on without recompiling Rust, and the same script works for standalone sessions (SessionEnd hook).
- **Synchronous, before /exit:** The capture completes fully before Claude Code is told to exit. No race conditions, no detached processes.
- **Visible UX:** The terminal shows progress and results. You see exactly what's being captured and where.
- **User confirmation:** The session doesn't die until you press Enter. You can review the PR link, verify the learnings, or Ctrl-C to cancel.
- **Compact summaries are in the JSONL:** If compaction happened during the session, the compact summary is already in the transcript. The extraction step reads it as part of the filtered JSONL — no special handling needed.
- **`claude -p` is not nested:** Cove (Rust binary) spawns `brain-os-capture.py` which spawns `claude -p`. The Claude session in the tmux pane is a separate, independent process. The `claude -p` reads a file and talks to the API independently — not nested.

### For `cove all-kill`

Runs the capture flow for each session sequentially (each session gets its own marker file before `/exit`):

```
cove all-kill
  Analyzing session "auth-api"...
    Found 2 learnings -> PR created: ...
  Analyzing session "frontend"...
    No new learnings detected.
  Analyzing session "debug-panel"...
    Found 1 learning -> PR created: ...

  All learnings captured. Ready to exit 3 sessions.
  Press Enter to close all, or Ctrl-C to cancel.
```

### Standalone Claude Code sessions (no cove)

For sessions opened with plain `claude` in a terminal, there's no cove to orchestrate the exit. Two mechanisms cover this:

**`/capture` skill (manual, best quality):** Type `/capture` anytime during a session. Claude reads its own JSONL, extracts insights, writes to brain-os with citations, creates PR — all within the conversation where Claude has full context. This works in both cove and standalone sessions. Does NOT call `claude -p` — Claude does the work inline.

**SessionEnd hook (safety net, automatic):** A simplified `brain-os-capture.py` that runs `claude -p` to extract insights and create a PR on session exit. No interactive confirmation (hooks can't do that), but it catches sessions where you forgot to `/capture`. Replaces the current `capture-learnings.py`.

### Avoiding double capture (cove + SessionEnd hook)

When `cove kill` runs, it captures learnings synchronously, then sends `/exit`. The `/exit` triggers the SessionEnd hook — which would capture again. To prevent this, cove writes a marker file that the SessionEnd hook checks:

```rust
// cove kill.rs — after capture completes, before /exit:
// Write marker file that the SessionEnd hook checks
std::fs::write(format!("/tmp/cove-captured-{session_id}"), "").ok();
```

```python
# SessionEnd hook (brain-os-capture.py) — check for marker:
marker = f"/tmp/cove-captured-{session_id}"
if os.path.exists(marker):
    os.unlink(marker)
    sys.exit(0)  # cove already captured, skip
```

The flow:

- **`cove kill`**: capture -> write marker -> `/exit` -> SessionEnd hook sees marker -> skips
- **Standalone `claude`**: no marker exists -> SessionEnd hook runs capture
- **`/capture` skill**: runs mid-session -> SessionEnd hook still fires at exit, but the extraction prompt checks existing brain-os docs for overlap and skips learnings that are already present (acceptable duplication risk — worst case a learning is proposed twice and the PR review catches it)

### What about /clear?

No trigger on `/clear`. JSONL persists on disk and the archive captures it permanently. If insights weren't captured during the session, they'll be caught at session exit or by `/doctor` later. No urgency at context boundaries.

---

## The `/doctor` command

A skill that scans brain-os convention docs, validates provenance, and surfaces quality issues.

### V1 checks (ship first)

**1. Uncited sections**

- Scan all convention docs for paragraphs/sections with no footnote citations
- Report: "These sections have no provenance — consider adding citations or flagging as manually authored"
- Severity: info (not all content needs citations — manually written docs are fine)

**2. Orphaned citations**

- Check that referenced JSONL files exist (scan `~/.claude/projects/` for matching prefix)
- Check the archive (GitHub repo) if not found locally
- Validate that cited line numbers are within the file's range and character offsets are within the entry's text
- Report: "Citation session:abc12345:999:0-50 — JSONL file not found on disk or in archive" or "line 999 but file only has 620 lines"
- Severity: warning

### Nice-to-have checks (future)

**3. Thin entries**

- Identify convention doc sections that are suspiciously short (< 3 sentences)
- For sections WITH citations: pull up the cited transcript at the cited line, check if more detail was discussed but not captured
- Suggest: "This section cites session:07e94319:247:45-187 — the original discussion included [X, Y, Z] that could be added"
- Severity: suggestion

**4. Stale entries**

- Flag entries whose citations reference sessions older than a configurable threshold (e.g., 90 days) based on JSONL file modification time
- Suggest: "This was written 3 months ago based on session:abc123. Still accurate?"
- Severity: info

**5. Duplicate content**

- Fuzzy match across all convention docs for similar paragraphs
- Suggest consolidation if two docs cover the same topic
- Severity: suggestion

**6. Topic organization**

- Identify entries that might belong in a different directory
- Suggest: "This section in unix/terminals.md is really about tmux hooks — should it move to claude/claude.md?"
- Severity: suggestion

### Output format

```
brain-os doctor report
======================

WARNINGS (1):
  unix/xdg-conventions.md:
    - Citation session:abc12345:99:0-200 — JSONL file not found on disk or in archive

INFO:
  12 docs scanned, 47 citations found, 3 sections uncited

  Sections without citations:
    rust/rust-conventions.md: "Error Handling" (added manually?)
    frontend-conventions.md: "React Query Factories" (added manually?)
    logging-conventions.md: "Structured JSON" (added manually?)
```

---

## CI/CD check

A GitHub Action on brain-os PRs that runs a lightweight provenance check. **Warning only, never blocking.**

```yaml
# .github/workflows/provenance-check.yml
name: Provenance Check
on:
  pull_request:
    paths: ["**/*.md", "!*-plan.md"]

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Check provenance
        run: python3 scripts/check-provenance.py
        continue-on-error: true # never blocks merge
```

**What the CI script checks (subset of /doctor — no local JSONL access):**

1. **New content without citations:** Diff the PR, find added paragraphs in convention docs that have no footnote citations. Post a comment: "New content added without provenance citations. Consider adding `[^N]: session:...` references."
2. **Malformed citations:** Regex-validate that all `[^N]: session:` footnotes match the expected format. Regex: `session:[a-f0-9]{8}:\d+(-\d+)?(:\d+-\d+)? ".+"`.
3. **Citation count summary:** "This PR adds 3 sections with 5 citations across 2 docs."

The CI check CAN'T validate that JSONL files exist (they're local/archived, not in the brain-os repo). That's `/doctor`'s job.

---

## Transcript storage

### V1: Local files, .gitignored

Transcripts stay at `~/.claude/projects/.../*.jsonl`. The brain-os web UI (runs locally) reads from the local filesystem via an API route.

Add to brain-os `.gitignore`:

```
# Session transcripts — stored locally or in archive, not in this repo
.transcripts/
*.jsonl
```

Citations resolve locally: `session:07e94319:247:45-187` -> scan `~/.claude/projects/` for a file matching `07e94319*.jsonl` -> jump to line 247 -> highlight characters 45-187.

### V2: GitHub archive (permanent storage)

A private GitHub repo `brain-os-transcripts` stores archived session JSONLs. **Only sessions captured going forward** — the existing ~5500 JSONL files are not backfilled (a separate ingestion script can handle historical sessions later if needed). Only sessions that produced learnings get archived (not every idle debugging session).

**Why GitHub over R2 for V2:**

- Zero infrastructure to manage (no bucket, no API tokens, no Worker)
- Free for private repos up to 500MB (archiving only cited sessions: estimated ~40-80MB)
- Git-native: `git clone`, `git pull`, standard tooling
- The brain-os frontend (running locally) reads from the local clone
- Upgrade to R2 (V3) when/if the repo outgrows GitHub or the frontend goes cloud

**Archive flow** (runs as part of `brain-os-capture.py` after extraction):

```bash
# Copy the cited session JSONL to the archive repo
cp "$jsonl_path" ~/workspace/brain-os-transcripts/sessions/${session_id}.jsonl

# Commit and push
cd ~/workspace/brain-os-transcripts
git add sessions/${session_id}.jsonl
git commit -m "archive: session ${session_id:0:8} from ${project}"
git push
```

**Resolving a citation from the archive:**

```bash
# Find session by prefix
ls ~/workspace/brain-os-transcripts/sessions/07e94319*.jsonl
# -> sessions/07e94319-d9ba-4194-9872-de89b2fb8faf.jsonl
```

### V3: Cloudflare R2 + Worker (cloud access)

When the brain-os frontend moves beyond localhost, or when the GitHub archive outgrows the free tier, migrate to R2:

- **R2 bucket:** `brain-os-transcripts`, S3-compatible, 10GB free forever, zero egress
- **Cloudflare Worker:** Proxy between frontend and R2. Handles auth (R2 binding, no credentials exposed), CORS (Worker sets headers), and search (Worker parses JSONL server-side, returns matching entries for a search term)
- **Upload:** `aws s3 cp` with `--endpoint-url` pointing to R2. Custom metadata: project, branch, created-at
- **Resolution:** Worker's `ListObjects` with prefix filter finds session by 8-char prefix

R2 details (bucket structure, Worker code, wrangler.toml, CORS approach) are documented in the git history of this plan file for when V3 is needed.

---

## Changes to existing system

### 1. Create `brain-os-capture.py` — the single capture script

**File:** `~/.claude/hooks/brain-os-capture.py` (replaces `capture-learnings.py`)

**Config constants at the top of the script:**

```python
BRAIN_OS_PATH = os.path.expanduser("~/workspace/personal/explorations/brain-os")
CLAUDE_PROJECTS_DIR = os.path.expanduser("~/.claude/projects")
```

A standalone Python script that:

- Finds the session JSONL:
  - **SessionEnd hook:** Claude Code passes `session_id` and `transcript_path` directly in stdin JSON — no discovery needed
  - **Cove kill (`--session-id`):** globs `~/.claude/projects/*/<session-id>.jsonl` across all project directories (the encoded path varies per repo)
- Reads + filters the JSONL transcript (keeps only user/assistant entries with text)
- Passes filtered transcript to `claude -p` with extraction prompt
- Writes extracted learnings to brain-os convention docs (at `BRAIN_OS_PATH`) with footnote citations
- Creates PR via `gt create` + `gt submit`
- Archives the JSONL to the transcript repo (V2)
- Returns summary for display

This single script is called by:

- **Cove kill:** `Command::new("python3").arg("brain-os-capture.py").arg("--session-id").arg(id)` — cove shows output, waits for completion
- **SessionEnd hook:** same script, runs automatically after Claude exits
- Both paths use `claude -p` (safe — not nested, either cove calls it or Claude is already dead)

### 2. Deprecate learnings-capturer agent + old capture-learnings.py

**Remove:**

- `~/.claude/agents/learnings-capturer.md` — no longer needed
- `capture-learnings.py` SessionEnd hook — replaced by `brain-os-capture.py`
- Pre-compact hook's "ACTION REQUIRED: launch learnings-capturer" section
- `capture-learnings.py` entry from `~/.claude/settings.json` SessionEnd hooks

**Keep:**

- `cove hook session-end` in SessionEnd hooks (for cove's own event tracking)
- Proactive capture triggers in CLAUDE.md — "surface insights during the session, write to brain-os with citations when something is worth capturing"

### 3. Update brain-os CLAUDE.md

Add citation instructions to the "Adding knowledge" section (already partially done in brain-os PR #32):

```markdown
## Adding knowledge

Write directly to the appropriate convention doc. Include inline footnote
citations pointing to the session transcript that produced the insight.

Citation format: `[^N]: session:<prefix>:<lines>[:<start>-<end>] "<description>"`
Lines can be single (`247`) or range (`247-250`). Character range is optional.

Example: `[^1]: session:07e94319:247:45-187 "XDG isolation for purpose-built Neovim"`
Example: `[^2]: session:07e94319:240-248 "Full XDG isolation discussion"`
```

### 4. Create `/doctor` skill

A new skill at `~/.claude/skills/doctor.md`. V1 runs two checks: uncited sections and orphaned citations. Resolves citations by scanning local JSONL files and the GitHub archive.

### 5. Create CI check script

`brain-os/scripts/check-provenance.py` — lightweight CI check for PRs (warning only, never blocking).

### 6. Update brain-os hook

The `brain-os-context.py` TF-IDF hook should strip footnotes from injected content to save context window space. Citations are useful in docs on disk but not needed during active coding sessions.

### 7. Add `cove kill` capture integration

**Files:** `cove/src/commands/kill.rs`

Minimal change — before `graceful_exit()`, call `brain-os-capture.py` as a subprocess:

```rust
// In kill.rs, before graceful_exit:
let status = Command::new("python3")
    .arg(capture_script_path)
    .arg("--session-id")
    .arg(&session_id)
    .stdout(Stdio::inherit())  // show output in terminal
    .stderr(Stdio::inherit())
    .status()?;

// Write marker to prevent double-capture
std::fs::write(format!("/tmp/cove-captured-{}", session_id), "").ok();

// Wait for user confirmation
println!("Press Enter to close or Ctrl-C to cancel.");
let _ = std::io::stdin().read_line(&mut String::new());

// Then proceed with existing graceful_exit + kill_window
```

No new `capture.rs` module needed — just a subprocess call.

---

## Implementation phases

### Phase 1: Capture script + citation convention + cove integration

- New `brain-os-capture.py` — JSONL filter, `claude -p` extraction, brain-os writer, PR creation
- Wire into `cove kill` as subprocess call (few lines of Rust)
- Wire as SessionEnd hook (replaces `capture-learnings.py`)
- Marker file (`/tmp/cove-captured-{session_id}`) prevents double-capture
- Remove pre-compact hook's ACTION REQUIRED section
- Create `/capture` skill (`~/.claude/skills/capture.md`) for manual mid-session use (Claude writes inline, no `claude -p`)
- Add `.gitignore` entries to brain-os for transcript files
- Update brain-os CLAUDE.md with full citation format spec
- `/doctor` V1: uncited sections + orphaned citations (scan `~/.claude/projects/`)
- Test: `cove kill <name>` shows learnings, creates PR, waits for confirmation
- Test: standalone `claude` session triggers SessionEnd capture on exit
- Test: `cove kill` + SessionEnd hook doesn't double-capture

### Phase 2: GitHub transcript archive (V2)

- Create private repo `brain-os-transcripts`
- Add archive step to `brain-os-capture.py` (copy cited JSONL, commit, push)
- Update `/doctor` to check archive when local files are missing
- brain-os frontend reads from local clone of archive repo

### Phase 3: `/doctor` enhancements

- Thin entry detection (pull up cited transcript, check for uncaptured detail)
- Stale entry detection (flag citations > 90 days old)
- Duplicate content detection (fuzzy match across docs)
- Topic organization suggestions

### Phase 4: CI check + brain-os frontend

- `brain-os/scripts/check-provenance.py` — warn on PRs with uncited new content
- Brain-os frontend: browse convention docs, click citations to view source transcript
- Frontend reads from local archive clone (V2) or R2 Worker API (V3)

### Phase 5 (future): R2 + Cloudflare Worker (V3)

- Migrate transcript archive from GitHub to R2 when size or access patterns demand it
- Deploy Cloudflare Worker for cloud-accessible transcript API
- Update frontend to use Worker endpoints

---

## Resolved design decisions

- **Line numbers + character ranges in citations (bounding box for plain text).** JSONL is strictly append-only — entries are never edited, deleted, or reordered. Line numbers and character offsets are permanent and stable. Citations support single lines (`session:prefix:247:45-187`) and line ranges (`session:prefix:247-250:45-187`) with optional character offsets. For line ranges, start char applies to the first line, end char to the last line, intermediate lines are fully highlighted (text-selection model). Like bounding boxes for PDFs — file -> line(s) -> character range.
- **Only archive future sessions.** The existing ~5500 JSONL files are not backfilled into the archive. A separate ingestion script can handle historical sessions later if needed. Going forward, only sessions that produce learnings are archived.
- **Capture logic is Python, not Rust.** A standalone `brain-os-capture.py` script is easier to iterate on than a Rust module. Cove just calls it as a subprocess. The same script works for both cove kill and SessionEnd hooks.
- **GitHub archive before R2.** Zero infrastructure. Free for private repos up to 500MB. Only archive cited sessions (~40-80MB). Upgrade to R2 when needed.
- **Both capture paths are valuable.** Organic (mid-session) has better context; exit (systematic) has the full session view. Both can produce signal — a one-sentence nugget caught at exit is just as valuable as a detailed entry written mid-session.
- **JSONL filtering makes chunking tractable.** A 1000-line transcript filters down to ~83 meaningful entries (~50-100KB). Even the largest session (74K lines, 17MB) becomes manageable after filtering.
- **No nested Claude.** Organic capture and `/capture` skill: Claude writes inline. Exit capture: `claude -p` is a separate process (cove spawns it, or it runs after Claude exits). Never call `claude -p` from inside a running Claude session.
- **Prefix collision is non-issue.** 8 hex chars = 4.3 billion possibilities. Hundreds of personal sessions = essentially zero collision risk.
- **`claude -p` returns structured JSON, Python handles file I/O.** Follows the proven pattern from `capture-learnings.py`: `claude -p` (with `--no-session-persistence`, `capture_output=True`) returns JSON with learning text, target file, section, citation, and a verbatim source quote. The Python script appends to convention docs, computes character offsets from the quote via `str.find()`, manages footnote numbering, and runs git operations. `claude -p` never touches the filesystem directly.
- **Single extraction prompt, not two-step.** The current `capture-learnings.py` uses a two-step extract→assess pipeline (two `claude -p` calls, ~60-120s). The new system uses a single prompt that handles extraction, assessment, and routing together. The two-step split was needed when output was unstructured learning files in a staging directory. Now the output is structured JSON with clear fields (target file, section, verbatim quote, confidence), so a single well-designed prompt handles it. Dedup is handled by passing existing brain-os doc headings into the prompt. Signal density comes from prompt design — explicit examples of both tiny nuggets and larger patterns, plus the instruction "a one-sentence gotcha is just as valuable as a multi-paragraph pattern." Saves 30-60 seconds per session.
- **Per-doc, append-only footnote numbering.** Standard markdown footnotes (`[^1]`, `[^2]`, ...) with append-only numbering: `max(existing_footnote_numbers) + 1`. No renumbering, no globally unique IDs. Standard markdown renders correctly everywhere (GitHub, mdx). PR collision risk is minimal in a personal knowledge base — if it happens, it's a trivial merge conflict. A single footnote can be referenced from multiple places in the doc (standard markdown behavior). Implementation: `re.findall(r'\[\^(\d+)\]', content)` → take max → increment.
- **Strip footnotes during hook injection.** `brain-os-context.py` strips both inline markers (`[^1]`) and footnote definitions (`[^1]: session:...`) before injecting excerpts into Claude sessions. The hook injects section excerpts (max 800 chars), not full files — footnote definitions at the bottom of the file would already be orphaned in the excerpt. Orphaned `[^1]` markers without definitions are confusing noise. Citations live on disk for `/doctor` and the frontend — they don't need to be in the injected context. Implementation: two regex substitutions in `extract_relevant_section()`.

## Open questions

- **claude -p performance:** Extraction takes ~30-60 seconds per session. Acceptable for `cove kill` but may feel slow for `cove all-kill` with many sessions. Parallelize?
- **Cross-project citations:** A learning from a cove session might belong in `rust/rust-conventions.md`. Should `cove kill` write to brain-os from any project's session?
- **GitHub archive size management:** At ~40-80MB for cited sessions, the 500MB limit is comfortable. But if usage grows, when to trigger the R2 migration?
- **Character offset precision:** LLMs are imprecise at counting characters. For exit capture, `claude -p` should return a **verbatim quote** from the source text (not character offsets). The Python script post-processes quotes via `str.find()` to compute exact character offsets — same approach PDF annotation tools use. For organic capture, Claude attempts best-effort character ranges; `/doctor` can validate and correct offsets later.

---

## Progress

- [x] Deprecate claude-learnings in brain-os CLAUDE.md (PR #32)
- [x] Update ~/.claude/CLAUDE.md capture triggers (dotfiles PR #63)
- [ ] Phase 1: Capture script + citation convention + cove integration
- [ ] Phase 2: GitHub transcript archive
- [ ] Phase 3: `/doctor` enhancements
- [ ] Phase 4: CI check + brain-os frontend
- [ ] Phase 5: R2 + Cloudflare Worker (future)
