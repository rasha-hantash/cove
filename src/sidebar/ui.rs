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

            let env_glyph = env_glyph(win);
            let label = snake_label(&win.name);
            let name_style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(colors::OVERLAY)
            };

            let mut spans = vec![
                Span::raw(" "),
                Span::raw(env_glyph),
                Span::raw(" "),
                Span::styled(label.clone(), name_style),
            ];

            // State glyph right-aligned against the legend column.
            if let Some(glyph) = state_span(state, self.tick) {
                let used = 1 + env_glyph.chars().count() + 1 + label.chars().count();
                let glyph_width = glyph_visual_width(state);
                let pad = (right_col as usize).saturating_sub(used + glyph_width + 1);
                spans.push(Span::raw(" ".repeat(pad)));
                spans.push(glyph);
            }

            let line = Line::from(spans);
            buf.set_line(area.x, y, &line, right_col);
        }

        // Right column: legend (independent positioning)
        for (i, entry) in LEGEND.iter().enumerate() {
            let ly = body_start + i as u16;
            if ly >= body_bottom {
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

/// Two-cell environment glyph (docker/ssh) or a 2-cell blank for native sessions.
fn env_glyph(win: &WindowInfo) -> &'static str {
    if win.is_docker {
        "\u{1F433}" // 🐳
    } else if win.is_ssh {
        "\u{1F310}" // 🌐
    } else {
        "  " // 2 cells of padding so columns align across native/docker/ssh rows
    }
}

/// Lowercase + non-alphanumeric → '_' + collapse repeats.
/// Returns empty string if input is empty (no fallback label).
pub fn snake_label(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_underscore = true; // suppress leading underscores
    for c in input.chars() {
        let mapped = if c.is_alphanumeric() {
            c.to_ascii_lowercase()
        } else {
            '_'
        };
        if mapped == '_' {
            if prev_underscore {
                continue;
            }
            prev_underscore = true;
        } else {
            prev_underscore = false;
        }
        out.push(mapped);
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

/// Renderable state glyph or None for non-actionable states (Fresh, Done).
fn state_span(state: WindowState, tick: u64) -> Option<Span<'static>> {
    match state {
        WindowState::Working => {
            let frame = SPINNER[tick as usize % SPINNER.len()];
            Some(Span::styled(
                frame.to_string(),
                Style::default().fg(colors::LAVENDER),
            ))
        }
        WindowState::Idle => Some(Span::styled(
            "\u{25CF}", // ●
            Style::default().fg(colors::GREEN),
        )),
        WindowState::Asking | WindowState::Waiting => {
            Some(Span::raw("\u{2753}")) // ❓
        }
        WindowState::Fresh | WindowState::Done => None,
    }
}

/// Visual cell-width of the state glyph for layout math.
fn glyph_visual_width(state: WindowState) -> usize {
    match state {
        WindowState::Working => 1, // braille spinner = 1 cell
        WindowState::Idle => 1,    // ● = 1 cell
        WindowState::Asking | WindowState::Waiting => 2, // ❓ = 2 cells
        WindowState::Fresh | WindowState::Done => 0,
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
            is_docker: false,
            is_ssh: false,
        }
    }

    #[test]
    fn snake_label_basic() {
        assert_eq!(
            snake_label("Run Claude Code on VPS with Paper Desktop"),
            "run_claude_code_on_vps_with_paper_desktop"
        );
    }

    #[test]
    fn snake_label_collapses_repeats_and_trims() {
        assert_eq!(snake_label("  hello---world  "), "hello_world");
    }

    #[test]
    fn snake_label_empty() {
        assert_eq!(snake_label(""), "");
    }

    #[test]
    fn snake_label_only_separators() {
        assert_eq!(snake_label("---"), "");
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
    fn renders_snake_label() {
        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);

        let windows = vec![test_win(1, "Run Claude Code on VPS")];
        let states: HashMap<u32, WindowState> = [(1, WindowState::Idle)].into_iter().collect();
        let widget = SidebarWidget {
            windows: &windows,
            states: &states,
            selected: 0,
            tick: 0,
        };
        widget.render(area, &mut buf);

        assert!(
            buf_contains(&buf, area, "run_claude_code_on_vps"),
            "should snake-case the title"
        );
    }

    #[test]
    fn renders_docker_glyph() {
        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);

        let mut win = test_win(1, "feature");
        win.is_docker = true;
        let windows = vec![win];
        let states: HashMap<u32, WindowState> = [(1, WindowState::Idle)].into_iter().collect();
        let widget = SidebarWidget {
            windows: &windows,
            states: &states,
            selected: 0,
            tick: 0,
        };
        widget.render(area, &mut buf);

        assert!(
            buf_contains(&buf, area, "\u{1F433}"),
            "docker glyph should render"
        );
    }
}
