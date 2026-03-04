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
    };

    loop {
        // Refresh window list periodically
        if app.tick % REFRESH_EVERY == 0 {
            refresh_windows(&mut app);
        }

        // Detect states every tick
        app.states = app.detector.detect(&app.windows);

        // Context orchestration: prefetch, drain, handle selection changes
        let detector = &app.detector;
        app.context_mgr.tick(
            &app.windows,
            &app.states,
            app.selected,
            &|idx| detector.pane_id(idx).map(str::to_string),
            &|idx| detector.cwd(idx).map(str::to_string),
        );

        // Prepare context for rendering
        let context = app
            .windows
            .get(app.selected)
            .and_then(|win| app.context_mgr.get(&win.name));
        let context_loading = app
            .windows
            .get(app.selected)
            .is_some_and(|win| app.context_mgr.is_loading(&win.name));
        let context_error = app
            .windows
            .get(app.selected)
            .and_then(|win| app.context_mgr.get_error(&win.name));

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
                    context_error,
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
