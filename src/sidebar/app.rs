// ── Sidebar application ──

use std::collections::HashMap;
use std::io::{self, stdout};

use crossterm::cursor;
use crossterm::execute;
use crossterm::terminal::{self, DisableLineWrap, EnableLineWrap};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::sidebar::context::ContextManager;
use crate::sidebar::event::{self, Action};
use crate::sidebar::state::{StateDetector, WindowState};
use crate::sidebar::ui::SidebarWidget;
use crate::tmux::{self, WindowInfo};

// ── Types ──

struct SidebarApp {
    windows: Vec<WindowInfo>,
    states: HashMap<u32, WindowState>,
    selected: usize,
    tick: u64,
    detector: StateDetector,
    context_mgr: ContextManager,
    prev_selected_name: Option<String>,
}

// ── Constants ──

const REFRESH_EVERY: u64 = 2;

// ── Public API ──

pub fn run() -> Result<(), String> {
    // No alternate screen — render in-place in tmux pane (matches bash behavior)
    let mut stdout = stdout();
    execute!(stdout, cursor::Hide, DisableLineWrap).map_err(|e| format!("terminal: {e}"))?;
    terminal::enable_raw_mode().map_err(|e| format!("terminal: {e}"))?;

    let result = run_loop();

    // Cleanup
    terminal::disable_raw_mode().ok();
    execute!(stdout, cursor::Show, EnableLineWrap).ok();

    result
}

// ── Helpers ──

fn run_loop() -> Result<(), String> {
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).map_err(|e| format!("terminal: {e}"))?;

    let mut app = SidebarApp {
        windows: Vec::new(),
        states: HashMap::new(),
        selected: 0,
        tick: 0,
        detector: StateDetector::new(),
        context_mgr: ContextManager::new(),
        prev_selected_name: None,
    };

    loop {
        // Refresh window list periodically
        if app.tick % REFRESH_EVERY == 0 {
            refresh_windows(&mut app);
        }

        // Detect states every tick
        app.states = app.detector.detect(&app.windows);

        // Prefetch context for all non-fresh sessions
        for win in &app.windows {
            let state = app
                .states
                .get(&win.index)
                .copied()
                .unwrap_or(WindowState::Fresh);
            if state != WindowState::Fresh {
                let pane_id = app.detector.pane_id(win.index).unwrap_or("");
                app.context_mgr.request(&win.name, &win.pane_path, pane_id);
            }
        }

        // Drain completed context results from background threads
        app.context_mgr.drain();

        // Track selection changes and manage context generation
        let current_name = app.windows.get(app.selected).map(|w| w.name.clone());
        if current_name != app.prev_selected_name {
            // Refresh context for old session (we're switching away from it)
            if let Some(ref prev_name) = app.prev_selected_name {
                if let Some(prev_win) = app.windows.iter().find(|w| w.name == *prev_name) {
                    let pane_id = app.detector.pane_id(prev_win.index).unwrap_or("");
                    app.context_mgr
                        .refresh(&prev_win.name, &prev_win.pane_path, pane_id);
                }
            }
            // Request context for new session (no-op if already cached)
            if let Some(win) = app.windows.get(app.selected) {
                let pane_id = app.detector.pane_id(win.index).unwrap_or("");
                app.context_mgr.request(&win.name, &win.pane_path, pane_id);
            }
            app.prev_selected_name = current_name;
        }

        // Prepare context for rendering
        let context = app
            .windows
            .get(app.selected)
            .and_then(|win| app.context_mgr.get(&win.name));
        let context_loading = app
            .windows
            .get(app.selected)
            .is_some_and(|win| app.context_mgr.is_loading(&win.name));

        // Render
        terminal
            .draw(|frame| {
                let area = frame.area();
                let widget = SidebarWidget {
                    windows: &app.windows,
                    states: &app.states,
                    selected: app.selected,
                    tick: app.tick,
                    context,
                    context_loading,
                };
                frame.render_widget(widget, area);
            })
            .map_err(|e| format!("render: {e}"))?;

        // Handle events
        let actions = event::poll();
        let mut moved = false;

        for action in actions {
            match action {
                Action::Up => {
                    if app.selected > 0 {
                        app.selected -= 1;
                        moved = true;
                    }
                }
                Action::Down => {
                    if app.selected + 1 < app.windows.len() {
                        app.selected += 1;
                        moved = true;
                    }
                }
                Action::Select => {
                    if let Some(win) = app.windows.get(app.selected) {
                        let _ = tmux::select_window(win.index);
                        refresh_windows(&mut app);
                        app.tick = 0;
                        continue;
                    }
                }
                Action::Quit => return Ok(()),
                Action::Tick => {}
            }
        }

        // Single tmux call after all queued keys are processed
        if moved {
            if let Some(win) = app.windows.get(app.selected) {
                let _ = tmux::select_window_sidebar(win.index);
            }
            // Skip next refresh so select-window has time to take effect
            app.tick = 1;
        } else {
            app.tick += 1;
        }
    }
}

fn refresh_windows(app: &mut SidebarApp) {
    if let Ok(windows) = tmux::list_windows() {
        // Sync selected to the tmux-active window
        let active_pos = windows.iter().position(|w| w.is_active).unwrap_or(0);

        app.selected = active_pos;
        app.windows = windows;

        // Clamp
        if app.selected >= app.windows.len() && !app.windows.is_empty() {
            app.selected = app.windows.len() - 1;
        }
    }
}
