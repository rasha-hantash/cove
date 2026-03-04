// ── ratatui rendering for sidebar ──

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::colors;
use crate::sidebar::state::WindowState;
use crate::tmux::WindowInfo;

// ── Types ──

/// Legend entry for keyboard shortcuts.
struct LegendEntry {
    key: &'static str,
    label: &'static str,
}

const LEGEND: &[LegendEntry] = &[
    LegendEntry {
        key: "\u{2318} + j",
        label: "claude",
    },
    LegendEntry {
        key: "\u{2318} + m",
        label: "terminal",
    },
    LegendEntry {
        key: "\u{2318} + p",
        label: "sessions",
    },
    LegendEntry {
        key: "\u{2318} + ;",
        label: "detach",
    },
];

pub struct SidebarWidget<'a> {
    pub windows: &'a [WindowInfo],
    pub states: &'a HashMap<u32, WindowState>,
    pub selected: usize,
    pub tick: u64,
    /// Context description for the selected session (if available).
    pub context: Option<&'a str>,
    /// Whether context is currently being generated for the selected session.
    pub context_loading: bool,
    /// Error message when context generation failed (e.g. "Ollama not connected").
    pub context_error: Option<&'a str>,
}

// ── Public API ──

impl Widget for SidebarWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let window_count = self.windows.len();

        // ── Header ──
        let plural = if window_count == 1 { "" } else { "s" };
        let header = Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!("{window_count} session{plural}"),
                Style::default().fg(colors::OVERLAY),
            ),
            Span::styled(" \u{00b7} ", Style::default().fg(colors::SURFACE)),
            Span::styled("\u{2191}\u{2193}", Style::default().fg(colors::BLUE)),
            Span::styled(" navigate", Style::default().fg(colors::OVERLAY)),
        ]);
        if area.height > 0 {
            buf.set_line(area.x, area.y, &header, area.width);
        }

        // ── Separator ──
        if area.height > 1 {
            let sep_row = area.y + 1;
            for x in area.x..area.x + area.width {
                buf.cell_mut((x, sep_row))
                    .map(|cell| cell.set_char('\u{2500}').set_fg(colors::SURFACE));
            }
        }

        // ── Body ──
        let body_start = area.y + 2;
        let right_col = area.width.saturating_sub(15);

        // Pre-calculate context block height so we can reserve space at the bottom
        let context_height = if let Some(context) = self.context {
            let max_width = (area.width as usize).saturating_sub(3);
            let text_lines = wrap_text(context, max_width, 3).len() as u16;
            1 + text_lines // dashed separator + text lines
        } else if self.context_loading || self.context_error.is_some() {
            2 // dashed separator + "loading…" or error message
        } else {
            0
        };

        // Left column: sessions
        let body_bottom = area.y + area.height;
        let session_bottom = body_bottom.saturating_sub(context_height);
        for (i, win) in self.windows.iter().enumerate() {
            let y = body_start + i as u16;
            if y >= session_bottom {
                break;
            }

            let state = self
                .states
                .get(&win.index)
                .copied()
                .unwrap_or(WindowState::Fresh);
            let is_selected = i == self.selected;

            let (bullet, name_style) = if is_selected {
                (
                    Span::styled("\u{276f}", Style::default().fg(Color::White)),
                    Style::default().fg(Color::White),
                )
            } else {
                (Span::raw(" "), Style::default().fg(colors::OVERLAY))
            };

            let mut spans = vec![
                Span::raw(" "),
                bullet,
                Span::raw(" "),
                Span::styled(&win.name, name_style),
            ];

            let status = status_text(state);
            if matches!(state, WindowState::Working) {
                // Spinner renders inline right after the name
                spans.push(status_span(state, self.tick));
            } else if !status.is_empty() {
                // Right-align status text against the legend column
                let name_width = 3 + win.name.len(); // " · " or " ❯ " prefix + name
                let status_width = status.chars().count() + 2; // 2 spaces before status
                let pad = (right_col as usize).saturating_sub(name_width + status_width);
                spans.push(Span::raw(" ".repeat(pad)));
                spans.push(status_span(state, self.tick));
            }

            let line = Line::from(spans);
            buf.set_line(area.x, y, &line, right_col);
        }

        // Right column: legend (independent positioning)
        for (i, entry) in LEGEND.iter().enumerate() {
            let ly = body_start + i as u16;
            if ly >= area.y + area.height {
                break;
            }
            let legend_line = Line::from(vec![
                Span::styled(entry.key, Style::default().fg(colors::BLUE)),
                Span::raw("  "),
                Span::styled(entry.label, Style::default().fg(colors::OVERLAY)),
            ]);
            buf.set_line(area.x + right_col, ly, &legend_line, area.width - right_col);
        }

        // Context block pinned to the bottom of the panel
        if context_height > 0 {
            let context_start = body_bottom.saturating_sub(context_height);
            if context_start >= body_start {
                if let Some(context) = self.context {
                    render_context_block(buf, area, context_start, right_col, context);
                } else if self.context_loading {
                    render_loading_block(buf, area, context_start);
                } else if let Some(error) = self.context_error {
                    render_error_block(buf, area, context_start, error);
                }
            }
        }
    }
}

// ── Helpers ──

const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

fn status_text(state: WindowState) -> &'static str {
    match state {
        WindowState::Working => "",
        WindowState::Asking => "waiting\u{2026}",
        WindowState::Waiting => "approve\u{2026}",
        WindowState::Idle => "your turn",
        WindowState::Done => "",
        WindowState::Fresh => "",
    }
}

fn status_span(state: WindowState, tick: u64) -> Span<'static> {
    match state {
        WindowState::Working => {
            let frame = SPINNER[tick as usize % SPINNER.len()];
            Span::styled(format!(" {frame}"), Style::default().fg(colors::LAVENDER))
        }
        WindowState::Idle => Span::styled(status_text(state), Style::default().fg(colors::GREEN)),
        WindowState::Waiting => Span::styled(
            status_text(state),
            Style::default()
                .fg(colors::PEACH)
                .add_modifier(Modifier::ITALIC),
        ),
        _ => Span::styled(
            status_text(state),
            Style::default()
                .fg(colors::OVERLAY)
                .add_modifier(Modifier::ITALIC),
        ),
    }
}

/// Render the dashed separator + context text below the selected session.
fn render_context_block(buf: &mut Buffer, area: Rect, mut y: u16, _right_col: u16, text: &str) {
    // Dashed separator (full width with 1-char padding on each side)
    if y < area.y + area.height {
        for x in (area.x + 1)..area.x + area.width.saturating_sub(1) {
            buf.cell_mut((x, y))
                .map(|cell| cell.set_char('\u{2500}').set_fg(colors::SURFACE));
        }
        y += 1;
    }

    // Word-wrapped context (max 3 lines)
    let max_width = (area.width as usize).saturating_sub(3);
    let lines = wrap_text(text, max_width, 3);
    for line_text in &lines {
        if y >= area.y + area.height {
            break;
        }
        let line = Line::from(vec![
            Span::raw(" "),
            Span::styled(line_text.clone(), Style::default().fg(colors::OVERLAY)),
        ]);
        buf.set_line(area.x, y, &line, area.width);
        y += 1;
    }
}

/// Render the dashed separator + "loading…" indicator.
fn render_loading_block(buf: &mut Buffer, area: Rect, mut y: u16) {
    // Dashed separator (full width with 1-char padding on each side)
    if y < area.y + area.height {
        for x in (area.x + 1)..area.x + area.width.saturating_sub(1) {
            buf.cell_mut((x, y))
                .map(|cell| cell.set_char('\u{2500}').set_fg(colors::SURFACE));
        }
        y += 1;
    }

    // Loading indicator
    if y < area.y + area.height {
        let line = Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "loading\u{2026}",
                Style::default()
                    .fg(colors::OVERLAY)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]);
        buf.set_line(area.x, y, &line, area.width);
    }
}

/// Render the dashed separator + error message (dimmed).
fn render_error_block(buf: &mut Buffer, area: Rect, mut y: u16, message: &str) {
    // Dashed separator (full width with 1-char padding on each side)
    if y < area.y + area.height {
        for x in (area.x + 1)..area.x + area.width.saturating_sub(1) {
            buf.cell_mut((x, y))
                .map(|cell| cell.set_char('\u{2500}').set_fg(colors::SURFACE));
        }
        y += 1;
    }

    // Error message
    if y < area.y + area.height {
        let line = Line::from(vec![
            Span::raw(" "),
            Span::styled(
                message.to_string(),
                Style::default()
                    .fg(colors::SURFACE)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]);
        buf.set_line(area.x, y, &line, area.width);
    }
}

/// Word-wrap text to fit within `max_width`, returning at most `max_lines` lines.
fn wrap_text(text: &str, max_width: usize, max_lines: usize) -> Vec<String> {
    if max_width == 0 || max_lines == 0 {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let word_width = word.chars().count();
        if current.is_empty() {
            if word_width > max_width {
                let truncated: String = word.chars().take(max_width.saturating_sub(1)).collect();
                current = format!("{truncated}\u{2026}");
                lines.push(current);
                current = String::new();
                if lines.len() >= max_lines {
                    break;
                }
                continue;
            }
            current = word.to_string();
        } else if current.chars().count() + 1 + word_width <= max_width {
            current.push(' ');
            current.push_str(word);
        } else {
            lines.push(current);
            if lines.len() >= max_lines {
                current = String::new();
                break;
            }
            current = word.to_string();
        }
    }

    if !current.is_empty() && lines.len() < max_lines {
        lines.push(current);
    }

    lines
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wrap_short_text() {
        let lines = wrap_text("hello world", 20, 3);
        assert_eq!(lines, vec!["hello world"]);
    }

    #[test]
    fn test_wrap_long_text() {
        let lines = wrap_text("Adding OAuth login flow with Google provider", 25, 3);
        assert_eq!(
            lines,
            vec!["Adding OAuth login flow", "with Google provider"]
        );
    }

    #[test]
    fn test_wrap_max_lines() {
        let lines = wrap_text("one two three four five six seven eight", 10, 2);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_wrap_empty() {
        let lines = wrap_text("", 20, 3);
        assert!(lines.is_empty());
    }

    #[test]
    fn test_wrap_zero_width() {
        let lines = wrap_text("hello", 0, 3);
        assert!(lines.is_empty());
    }

    // ── Render output validation tests ──
    //
    // These tests render the SidebarWidget to a ratatui Buffer and assert
    // that the actual output contains the expected text. This validates
    // the full rendering pipeline — layout math, height guards, and content.

    use crate::tmux::WindowInfo;

    fn test_win(index: u32, name: &str) -> WindowInfo {
        WindowInfo {
            index,
            name: name.to_string(),
            is_active: false,
            pane_path: format!("/project/{name}"),
        }
    }

    /// Extract all text from a ratatui Buffer as a vector of strings (one per row).
    fn buf_lines(buf: &Buffer, area: Rect) -> Vec<String> {
        (area.y..area.y + area.height)
            .map(|y| {
                (area.x..area.x + area.width)
                    .map(|x| buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
                    .collect::<String>()
            })
            .collect()
    }

    /// Check if any line in the buffer contains the given substring.
    fn buf_contains(buf: &Buffer, area: Rect, needle: &str) -> bool {
        buf_lines(buf, area)
            .iter()
            .any(|line| line.contains(needle))
    }

    #[test]
    fn test_render_no_context() {
        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);

        let windows = vec![test_win(1, "my-session")];
        let states: HashMap<u32, WindowState> = [(1, WindowState::Idle)].into_iter().collect();

        let widget = SidebarWidget {
            windows: &windows,
            states: &states,
            selected: 0,
            tick: 0,
            context: None,
            context_loading: false,
            context_error: None,
        };
        widget.render(area, &mut buf);

        // Should show session name but no context or loading
        assert!(
            buf_contains(&buf, area, "my-session"),
            "should show session name"
        );
        assert!(
            !buf_contains(&buf, area, "loading"),
            "should not show loading"
        );
    }

    #[test]
    fn test_render_loading_state() {
        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);

        let windows = vec![test_win(1, "my-session")];
        let states: HashMap<u32, WindowState> = [(1, WindowState::Idle)].into_iter().collect();

        let widget = SidebarWidget {
            windows: &windows,
            states: &states,
            selected: 0,
            tick: 0,
            context: None,
            context_loading: true,
            context_error: None,
        };
        widget.render(area, &mut buf);

        // Should show both session name and loading indicator
        assert!(
            buf_contains(&buf, area, "my-session"),
            "should show session name"
        );
        assert!(
            buf_contains(&buf, area, "loading\u{2026}"),
            "should show loading indicator"
        );
    }

    #[test]
    fn test_render_context_text() {
        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);

        let windows = vec![test_win(1, "my-session")];
        let states: HashMap<u32, WindowState> = [(1, WindowState::Idle)].into_iter().collect();
        let context_text = "Fixing auth bug in login flow";

        let widget = SidebarWidget {
            windows: &windows,
            states: &states,
            selected: 0,
            tick: 0,
            context: Some(context_text),
            context_loading: false,
            context_error: None,
        };
        widget.render(area, &mut buf);

        // Should show session name and context text
        assert!(
            buf_contains(&buf, area, "my-session"),
            "should show session name"
        );
        assert!(
            buf_contains(&buf, area, "Fixing auth bug"),
            "should show context text"
        );
        assert!(
            !buf_contains(&buf, area, "loading"),
            "should not show loading when context is available"
        );
    }

    #[test]
    fn test_render_context_not_swallowed_in_small_pane() {
        // Minimum viable pane: header + separator + 1 session + separator + loading = 5 rows
        let area = Rect::new(0, 0, 40, 5);
        let mut buf = Buffer::empty(area);

        let windows = vec![test_win(1, "sess")];
        let states: HashMap<u32, WindowState> = [(1, WindowState::Idle)].into_iter().collect();

        let widget = SidebarWidget {
            windows: &windows,
            states: &states,
            selected: 0,
            tick: 0,
            context: None,
            context_loading: true,
            context_error: None,
        };
        widget.render(area, &mut buf);

        // Loading should still render even in a small pane
        assert!(
            buf_contains(&buf, area, "loading\u{2026}"),
            "loading should render in 5-row pane: {:?}",
            buf_lines(&buf, area)
        );
    }

    #[test]
    fn test_render_context_swallowed_in_tiny_pane() {
        // Pane too small: header(1) + sep(1) = body_start at row 2, height 3 → body has 1 row
        // Context needs 2 rows (sep + loading), context_start = 3-2=1, body_start=2 → guard fails
        let area = Rect::new(0, 0, 40, 3);
        let mut buf = Buffer::empty(area);

        let windows = vec![test_win(1, "sess")];
        let states: HashMap<u32, WindowState> = [(1, WindowState::Idle)].into_iter().collect();

        let widget = SidebarWidget {
            windows: &windows,
            states: &states,
            selected: 0,
            tick: 0,
            context: None,
            context_loading: true,
            context_error: None,
        };
        widget.render(area, &mut buf);

        // In a 3-row pane, context guard correctly prevents rendering
        // (there's no space for it — this is expected behavior)
        assert!(
            !buf_contains(&buf, area, "loading\u{2026}"),
            "loading should NOT render in 3-row pane (no space)"
        );
    }
}
