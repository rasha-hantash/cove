# Cove IDE View — Neovim as File Explorer & App Runner

## What this is

A `C-a e` hotkey that opens a purpose-built Neovim instance via `display-popup`, giving you a read-only IDE view of the **highlighted branch** in the Cove sidebar. If you've selected a different session/branch in the sidebar, Neovim opens in that branch's working directory (or worktree) — so you can explore any session's files, not just the active Claude pane. Browse files with syntax highlighting, run apps in integrated terminals, and (future) step through code with a debugger.

This is NOT the diff viewer. The diff viewer (`C-a r`) is a custom ratatui TUI as designed in `command-r-pr-review-plan.md`. This is a companion tool for when you want to explore the project, read code for context, or launch dev servers alongside Claude.

## Workflows

### Explore files while Claude works

```
Claude is coding in the left pane
  → Highlight a session in the Cove sidebar (or leave the current one selected)
  → C-a e
  → Neovim opens in the highlighted session's working directory / worktree
  → File tree on the left (oil.nvim), file viewer on the right
  → Navigate directories, open files, read code with syntax highlighting
  → q → back to Claude
```

This means you can explore **any session's files** — not just the one in the active Claude pane. If session "auth-api" is running in a worktree at `~/workspace/cove/.claude/worktrees/auth-api/`, highlighting it and pressing `C-a e` opens Neovim there.

### Run apps alongside Claude

```
C-a e → open IDE view
  → :terminal in a split
  → Run `uv run python main.py` in one terminal
  → Open another split: `bun dev`
  → Toggle back to Claude with q (terminals keep running? No — popup closes them)
```

**Important limitation:** `display-popup` kills child processes on close. For persistent app runners, the existing terminal pane (bottom-right) or a dedicated tmux window is better. The IDE view is for quick exploration and short-lived runs. See Open Questions for alternatives.

### Future: Debugger integration

```
C-a e → open IDE view
  → Claude has identified a bug location
  → DAP (Debug Adapter Protocol) via nvim-dap
  → Set breakpoints, step through code
  → Inspect variables at the exact failure point
  → Take control of the debugger manually if needed
  → q → back to Claude with your findings
```

This depends on an MCP debugger plugin being available. nvim-dap supports Python (debugpy), Rust (codelldb), JavaScript (vscode-js-debug), and most other languages.

---

## Tmux mechanics

Same `display-popup` approach as the review TUI:

```bash
# In tmux.conf:
bind e run-shell 'cove ide --highlighted'
# cove ide reads the highlighted session from sidebar state,
# resolves its cwd/worktree path, and runs:
#   tmux display-popup -E -B -w 70% -h 100% -x 0 -y 0 \
#     -d "{session_cwd}" \
#     "XDG_CONFIG_HOME=$HOME/.cove/config XDG_DATA_HOME=$HOME/.cove/data \
#      XDG_STATE_HOME=$HOME/.cove/state XDG_CACHE_HOME=$HOME/.cove/cache \
#      nvim"
```

Key details:

- `cove ide --highlighted` resolves the highlighted session's working directory from sidebar state (falls back to active pane's cwd if no session is highlighted)
- `-B` removes popup border for seamless view switch
- XDG vars isolate this Neovim instance from your personal config (see XDG section)
- `q` exits Neovim → popup closes → Claude is right there
- Claude's process keeps running underneath, uninterrupted

---

## Isolated Neovim config via XDG

The IDE view uses a **separate, minimal Neovim config** so it doesn't conflict with your personal Neovim setup. This is achieved by overriding XDG environment variables:

```
~/.cove/
  config/nvim/          ← XDG_CONFIG_HOME → Neovim looks here for init.lua
    init.lua            ← Minimal config: lazy.nvim + oil + treesitter
    lua/
      plugins.lua       ← Plugin specs
  data/nvim/            ← Plugin installations (lazy.nvim downloads)
  state/nvim/           ← Shada, undo files
  cache/nvim/           ← Tree-sitter parsers, plugin cache
```

Your personal `~/.config/nvim/` is completely untouched. Two separate Neovim "personalities."

### Minimal init.lua (~40 lines)

```lua
-- Bootstrap lazy.nvim
local lazypath = vim.fn.stdpath("data") .. "/lazy/lazy.nvim"
if not vim.uv.fs_stat(lazypath) then
  vim.fn.system({ "git", "clone", "--filter=blob:none",
    "https://github.com/folke/lazy.nvim.git", lazypath })
end
vim.opt.rtp:prepend(lazypath)

-- Settings
vim.g.mapleader = " "
vim.opt.number = true
vim.opt.relativenumber = true
vim.opt.termguicolors = true
vim.opt.signcolumn = "yes"

-- Plugins
require("lazy").setup({
  -- File explorer
  { "stevearc/oil.nvim", config = function()
    require("oil").setup({ view_options = { show_hidden = true } })
    vim.keymap.set("n", "<leader>e", "<cmd>Oil<cr>")
  end },

  -- Syntax highlighting
  { "nvim-treesitter/nvim-treesitter", build = ":TSUpdate", config = function()
    require("nvim-treesitter.configs").setup({
      ensure_installed = { "rust", "python", "typescript", "lua", "json", "toml", "markdown" },
      highlight = { enable = true },
    })
  end },

  -- Catppuccin (match Cove's palette)
  { "catppuccin/nvim", name = "catppuccin", config = function()
    vim.cmd.colorscheme("catppuccin-mocha")
  end },

  -- Future: debugger
  -- { "mfussenegger/nvim-dap" },
  -- { "rcarriga/nvim-dap-ui" },
})

-- Quick exit
vim.keymap.set("n", "q", "<cmd>qa!<cr>", { desc = "Exit IDE view" })
```

### First launch

First time `C-a e` is pressed, lazy.nvim bootstraps itself and downloads plugins (~5 seconds). Subsequent launches are instant.

**Installation step:** `cove` could detect missing config on first run and copy the default from its own assets, or we ship the config as part of `cove init`.

---

## What this is NOT

- **Not a code editor** — Claude writes code, you review it. This is for reading and navigating.
- **Not the diff viewer** — `C-a r` opens the ratatui review TUI for diffs and comments. `C-a e` is for file exploration.
- **Not a persistent workspace** — the popup closes when you press `q`. For long-running terminals, use the existing terminal pane or a dedicated tmux window.

---

## Implementation phases

### Phase 1: File explorer

- tmux keybinding (`C-a e`) in dotfiles/tmux/tmux.conf
- Minimal Neovim config at `~/.cove/config/nvim/init.lua`
- oil.nvim for file navigation
- Tree-sitter for syntax highlighting
- Catppuccin Mocha theme
- `q` to exit

**This gets you `C-a e` → browse files with syntax highlighting → `q` back to Claude.**

### Phase 2: Integrated terminals

- Keybind to open terminal splits within the IDE view
- Pre-configured terminal commands (e.g., `<leader>t` opens a terminal at project root)
- Investigate persistent terminals that survive popup close (see Open Questions)

### Phase 3: Debugger integration

- nvim-dap setup for Python, Rust, JavaScript
- DAP UI (nvim-dap-ui) for variable inspection, call stack, breakpoints
- Integration with Claude — Claude sets breakpoints via MCP, you step through
- Keybinds: F5 continue, F10 step over, F11 step in, F12 step out

---

## Files involved

### Cove (~/workspace/personal/explorations/cove/)

| File                   | Change                                                                            |
| ---------------------- | --------------------------------------------------------------------------------- |
| `assets/nvim/init.lua` | **New** — default Neovim config shipped with cove                                 |
| `src/commands/ide.rs`  | **New** — `cove ide` command: detect session, ensure config exists, launch Neovim |
| `src/cli.rs`           | Add `Ide` subcommand                                                              |
| `src/main.rs`          | Wire to `ide::run()`                                                              |

### Dotfiles (~/workspace/personal/dotfiles/)

| File             | Change                       |
| ---------------- | ---------------------------- |
| `tmux/tmux.conf` | Add `bind e run-shell '...'` |

### Cove config (~/.cove/)

| Path                   | Purpose                                              |
| ---------------------- | ---------------------------------------------------- |
| `config/nvim/init.lua` | Neovim config (copied from cove assets on first run) |
| `data/nvim/`           | Plugin installations                                 |
| `state/nvim/`          | Session state                                        |
| `cache/nvim/`          | Tree-sitter parsers                                  |

---

## Open questions

- **Persistent terminals:** `display-popup` kills child processes on close. Options: (a) accept this limitation — IDE view is for quick exploration, not long-running servers; (b) use a hidden tmux window instead of a popup for the IDE view, switching with `C-a e`; (c) launch terminals in the existing terminal pane before opening the IDE view.
- **Neovim as a dependency:** Should `cove` check for Neovim on startup and warn if missing? Or is this an optional feature that gracefully degrades?
- **Config updates:** When cove ships a new default config, how to update `~/.cove/config/nvim/init.lua` without overwriting user customizations? Version stamp in a comment? Separate `defaults.lua` that users can override?
- **MCP debugger bridge:** How would Claude communicate breakpoint locations and debug commands to the Neovim DAP instance? Shared file? MCP tool that writes to a known path?

---

## Progress

- [ ] Phase 1: tmux keybinding + minimal Neovim config + oil.nvim + tree-sitter
- [ ] Phase 2: Integrated terminal splits
- [ ] Phase 3: nvim-dap debugger integration
