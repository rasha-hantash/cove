> **Archived 2026-03-17** — One-time research, useful output already captured

# Creative Design Toolkit — Research & Application Plan for Cove

## Context

You have a rich set of UI/UX and agentic engineering tools to evaluate, and a concrete design target: cove's Review TUI (`command-r-pr-review-plan.md`). The goal is to figure out which tools help you creatively design cove's next features, and critically — how to get designs into a format Claude can consume without relying on screenshots.

The knowledge flywheel system design plan has been saved separately to `~/workspace/personal/explorations/day-plan-2026-03-08.md`.

---

## Tool Research Summary

### Tier 1: Directly Useful for Cove Design

| Tool                                                        | What It Is                                                                                                                                                                             | Why It Matters for Cove                                                                                                                                                                                            |
| ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **[paper.design](https://paper.design)**                    | Design canvas that exports as real HTML/CSS. Has **bidirectional MCP server** (24 tools) — Claude can read AND write to your designs.                                                  | Best "design-to-Claude" pipeline. Prototype cove's review TUI layout visually, then Claude reads the structured design via MCP and translates to ratatui constraints. No screenshots needed.                       |
| **[impeccable.style](https://impeccable.style)**            | Prompt enhancement layer — 17 design commands (`/polish`, `/audit`, `/distill`, `/bolder`) that give you design vocabulary for working with AI. Works with Claude Code.                | Install once, use everywhere. When building cove's TUI, commands like `/polish` and `/audit` let you refine visual hierarchy, spacing, and color without needing design expertise.                                 |
| **[difit](https://github.com/yoshiko-pg/difit)**            | CLI diff viewer with GitHub-like web UI. Auto-collapses generated files, severity annotations, "Copy Prompt" button that exports comments as structured prompts with file:line format. | **Direct design inspiration** for cove's review TUI. Study its UX patterns: file collapsing heuristics, side-by-side vs inline toggle, comment-to-prompt export format. These are exactly the patterns cove needs. |
| **[Refactor UI](https://online.fliphtml5.com/uejlb/wnsd/)** | Book by Tailwind CSS creator on UI design principles — spacing, color, typography, visual hierarchy.                                                                                   | Universal design principles that apply to TUIs. ratatui has the same constraints: limited space, need for visual hierarchy, contrast ratios matter even more in terminals.                                         |
| **Excalidraw MCP** (already installed)                      | Diagram tool where Claude can read/write structured JSON.                                                                                                                              | Use for architecture diagrams and layout mockups. Claude can export cove's module map or TUI layouts as Excalidraw diagrams, then iterate on them.                                                                 |

### Tier 2: Useful for Exploration / Inspiration

| Tool                                                                   | What It Is                                                                                                                                       | Relevance                                                                                                                                                                               |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **[visual-explainer](https://github.com/nicobailon/visual-explainer)** | Agent skill that transforms terminal output into styled HTML pages with Mermaid diagrams. Has `/diff-review` and `/plan-review` commands.        | Could generate visual docs for cove's architecture. The `/diff-review` command is interesting — compare with cove's review TUI approach.                                                |
| **[refero.design](https://refero.design)**                             | UI/UX design inspiration gallery.                                                                                                                | Browse for diff viewer and code review UI patterns. Study how production tools (GitHub, GitLab, Bitbucket) handle inline comments, severity indicators, file navigation.                |
| **[Delphi Design](https://design.delphi.ai)**                          | Delphi AI's public design system — OKLCH color palette, Pythia/Inter typography, component docs.                                                 | Study as a well-crafted design system example. The OKLCH color approach and their "Greco-Futurism" aesthetic could inspire cove's color palette (currently Catppuccin Mocha).           |
| **[beads](https://github.com/steveyegge/beads)**                       | Steve Yegge's agent memory/issue tracker — git-backed, graph-oriented, with semantic memory decay. Anthropic's tasks feature was inspired by it. | Not a design tool but highly relevant to cove's agentic workflow. Study how it handles task state, memory decay, and multi-agent coordination. Could inform cove's sidebar state model. |

### Tier 3: Interesting but Lower Priority for Cove

| Tool                                                 | What It Is                                                                                      | Notes                                                                                                                                                                     |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **[Google Antigravity](https://antigravity.google)** | Google's agentic IDE (modified VSCode fork). AI Studio → Antigravity pipeline.                  | Study the "design in AI Studio, build in Antigravity" workflow pattern. But it's a full IDE, not directly applicable to cove's TUI design.                                |
| **[tambo.co](https://tambo.co)**                     | Generative UI SDK for React — register components with Zod schemas, AI picks and streams props. | Interesting "component registry" pattern but web-only. The concept of "register UI components, let AI compose them" could theoretically apply to ratatui widgets someday. |
| **[entire.io](https://entire.io)**                   | CLI that captures AI agent sessions alongside git commits.                                      | Observability, not design. Worth setting up but not for the creative design workflow.                                                                                     |

---

## The Core Question: How Does Claude Digest Designs?

Screenshots are lossy — Claude sees pixels, not structure. Here's the hierarchy from best to worst:

### 1. Structured Design Data (best)

**paper.design MCP** — Claude reads your design as structured objects: layers, CSS properties, layout constraints, colors. No interpretation needed. This is the gold standard.

**Excalidraw JSON** — Claude reads/writes diagram elements as typed JSON (rectangles, arrows, text with positions). Good for layout prototyping.

**Design tokens (JSON/YAML)** — Express colors, spacing, typography as tokens:

```json
{
  "colors": {
    "diff-add": "#a6e3a1",
    "diff-remove": "#f38ba8",
    "comment-border": "#fab387"
  },
  "spacing": { "gutter": 4, "line-padding": 1, "comment-indent": 2 },
  "layout": {
    "diff-panel": "60%",
    "nav-panel": "40%",
    "file-list-height": "30%"
  }
}
```

Claude can consume this perfectly and translate to ratatui `Color` constants and `Layout` constraints.

### 2. Structured Text Descriptions (good)

**ASCII mockups** — What the cove review plan already does. Claude understands these natively. Combined with impeccable.style's design vocabulary, you can describe design intent precisely:

```
The diff viewer needs more vertical rhythm — group hunks with 1-line spacing,
use a subtle background tint on changed regions rather than just +/- coloring.
The file list should have tighter leading to fit more entries without scrolling.
```

**ratatui Layout DSL** — Describe layouts directly in ratatui's constraint language:

```rust
Layout::vertical([
    Constraint::Length(3),   // header: branch info
    Constraint::Min(10),     // diff viewer (fills remaining)
    Constraint::Length(1),   // status bar
])
```

### 3. Reference Screenshots with Context (okay)

If you must use images, pair them with structured annotations:

- "This is difit's file collapse behavior — I want this pattern"
- "This is GitHub's inline comment UX — the hover-to-comment trigger"

### 4. Raw Screenshots (worst)

Claude guesses at structure, colors, spacing. Avoid.

---

## Recommended Workflow for Designing Cove's Review TUI

### Step 1: Install impeccable.style (5 min)

It's a Claude Code skill — gives you design vocabulary commands. Install it, then every design conversation benefits.

### Step 2: Study difit + refero.design for patterns (30 min)

- Clone difit, run it on a real repo, note UX patterns that work
- Browse refero.design for diff viewer / code review inspiration
- Document patterns as structured notes (not screenshots): "difit collapses lock files by default, uses severity badges left-aligned"

### Step 3: Prototype in paper.design (1h)

- Open paper.design, create a canvas for the review TUI
- Lay out the panels: diff viewer (60%), branch nav (top-right), file list (bottom-right)
- Use real content — paste actual diff output, real branch names
- Connect Paper's MCP server to Claude Code
- Claude reads the design via MCP, you iterate: "make the comment bubbles more distinct", "tighten the file list spacing"

### Step 4: Extract design tokens (15 min)

From the Paper prototype, extract a `design-tokens.json` for cove:

- Color palette (map to ratatui `Color` values and Catppuccin Mocha equivalents)
- Spacing rules (gutter widths, padding, separator styles)
- Component patterns (comment bubble shape, severity badge style, file entry format)

### Step 5: Translate to ratatui (ongoing, during implementation)

With design tokens in hand, Claude translates directly to ratatui code:

- `Color` constants in `colors.rs`
- `Layout` constraints in `review/ui.rs`
- Widget rendering in diff viewer, branch nav, file list modules

The paper.design MCP stays connected — as you implement, you can go back to the canvas, adjust, and Claude sees the changes.

### Step 6: Read Refactor UI chapters on spacing + color (30 min)

Skim the chapters on vertical rhythm, color contrast, and visual hierarchy. Apply to the ratatui TUI — these principles work the same in terminals as on the web, just with fewer pixels.

---

## Design-to-Code Pipeline Summary

```
paper.design (visual prototype)
    │
    ├── MCP → Claude reads structured design data
    │
    ├── Extract → design-tokens.json
    │           (colors, spacing, layout constraints)
    │
    ▼
impeccable.style vocabulary
    │
    ├── /polish — refine visual hierarchy
    ├── /audit — check contrast, spacing, consistency
    │
    ▼
ratatui implementation
    │
    ├── colors.rs ← design token colors
    ├── review/ui.rs ← layout constraints
    ├── review/diff.rs ← diff rendering patterns (from difit study)
    │
    ▼
Excalidraw MCP (architecture docs)
    │
    └── Module diagrams, data flow visualizations
```

---

## Beads: Separate Evaluation Track

Beads deserves its own evaluation as an agentic engineering tool, not a design tool:

- **What it does**: Git-backed issue tracker where agents query `bd ready --json` for unblocked tasks. Semantic memory decay compacts old closed tasks. Cell-level merge prevents conflicts in multi-agent work.
- **How it relates to cove**: Cove tracks session state via JSONL events. Beads tracks task state via a Dolt (versioned SQL) database. The "memory decay" pattern (summarize old tasks to save context) is directly applicable to cove's event files, which currently grow unbounded.
- **Evaluation plan**: Install beads, use it in a cove development session, compare with current JSONL event tracking. Does it solve problems cove doesn't? Would cove benefit from integrating beads as a task backend?

---

## Time Blocks (revised for design focus)

| Block | Time  | What                                                   |
| ----- | ----- | ------------------------------------------------------ |
| 1     | 5min  | Install impeccable.style                               |
| 2     | 30min | Study difit + browse refero.design for patterns        |
| 3     | 1h    | Prototype cove review TUI in paper.design, connect MCP |
| 4     | 15min | Extract design tokens from prototype                   |
| 5     | 30min | Read Refactor UI chapters (spacing, color, hierarchy)  |
| 6     | 30min | Evaluate beads for agentic engineering                 |
| 7     | 30min | Try visual-explainer on cove's architecture            |
| 8     | 15min | Skim Delphi design system for color/typography ideas   |
| 9     | 15min | Set up entire.io on cove repo                          |

**Total: ~3.5h** — leaves room for the knowledge flywheel work from the earlier plan.

---

## Verification

- [ ] impeccable.style installed and working in Claude Code
- [ ] difit cloned and tested on a real repo — patterns documented
- [ ] paper.design prototype of review TUI created, MCP connected
- [ ] design-tokens.json extracted from prototype
- [ ] Refactor UI chapters read, key principles noted
- [ ] beads evaluated, outcome documented
- [ ] visual-explainer tried on cove architecture
- [ ] At least one Excalidraw diagram of cove's review TUI data flow created

## Progress

- [x] Block 1: impeccable.style setup (2026-03-09 — added to extraKnownMarketplaces in settings.json)
- [ ] Block 2: difit + refero.design study
- [x] Block 3: paper.design prototype (2026-03-09 — 9 artboards created for Review TUI, MCP connected and used extensively)
- [ ] Block 4: Design token extraction (Catppuccin Mocha palette documented in brain-os design/tui-design-workflow.md)
- [ ] Block 5: Refactor UI reading
- [ ] Block 6: Beads evaluation
- [ ] Block 7: visual-explainer trial
- [ ] Block 8: Delphi design system skim
- [ ] Block 9: entire.io setup
