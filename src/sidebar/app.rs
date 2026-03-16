// ── Sidebar application ──

use std::collections::HashMap;
use std::io::{self, stdout};
use std::time::Instant;

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

    let mut last_refresh = Instant::now();
    let refresh_interval = std::time::Duration::from_secs(2);
    let mut needs_render = true;

    loop {
        // Refresh window list periodically (every 2s wall-clock, not every N ticks)
        if last_refresh.elapsed() >= refresh_interval {
            refresh_windows(&mut app);
            last_refresh = Instant::now();
            needs_render = true;
        }

        // Only do expensive work (state detection, context, render) when needed
        if needs_render {
            // Detect states
            let new_states = app.detector.detect(&app.windows);

            // Detect Working → Idle transitions and play notification
            for (idx, new_state) in &new_states {
                if let Some(old_state) = app.states.get(idx) {
                    if *old_state == WindowState::Working && *new_state == WindowState::Idle {
                        // Play notification sound in background thread
                        std::thread::spawn(|| {
                            let home = std::env::var("HOME").unwrap_or_default();
                            let path = format!("{home}/.claude/assets/audio/beep.mp3");
                            let _ = std::process::Command::new("afplay")
                                .arg(&path)
                                .stdout(std::process::Stdio::null())
                                .stderr(std::process::Stdio::null())
                                .spawn();
                        });
                    }
                }
            }

            app.states = new_states;

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

            needs_render = false;
        }

        // Block for input (500ms timeout when idle — ~2 wakeups/sec instead of 10)
        let actions = event::poll();
        let mut moved = false;

        for action in &actions {
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
                        last_refresh = Instant::now();
                        app.tick = 0;
                        needs_render = true;
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
            app.tick = 1;
            needs_render = true;
        } else {
            app.tick += 1;
            // Trigger periodic render for context loading spinners
            if app.tick % 4 == 0 {
                needs_render = true;
            }
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
