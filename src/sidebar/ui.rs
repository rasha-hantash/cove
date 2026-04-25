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

        // Left column: sessions
        let body_bottom = area.y + area.height;
        for (i, win) in self.windows.iter().enumerate() {
            let y = body_start + i as u16;
            if y >= body_bottom {
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

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    use crate::tmux::WindowInfo;

    fn test_win(index: u32, name: &str) -> WindowInfo {
        WindowInfo {
            index,
            name: name.to_string(),
            is_active: false,
            pane_path: format!("/project/{name}"),
        }
    }

    fn buf_lines(buf: &Buffer, area: Rect) -> Vec<String> {
        (area.y..area.y + area.height)
            .map(|y| {
                (area.x..area.x + area.width)
                    .map(|x| buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
                    .collect::<String>()
            })
            .collect()
    }

    fn buf_contains(buf: &Buffer, area: Rect, needle: &str) -> bool {
        buf_lines(buf, area)
            .iter()
            .any(|line| line.contains(needle))
    }

    #[test]
    fn test_render_session_name() {
        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);

        let windows = vec![test_win(1, "my-session")];
        let states: HashMap<u32, WindowState> = [(1, WindowState::Idle)].into_iter().collect();

        let widget = SidebarWidget {
            windows: &windows,
            states: &states,
            selected: 0,
            tick: 0,
        };
        widget.render(area, &mut buf);

        assert!(
            buf_contains(&buf, area, "my-session"),
            "should show session name"
        );
    }
}
