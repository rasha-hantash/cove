// ── Catppuccin Mocha palette ──

use ratatui::style::Color;

pub const LAVENDER: Color = Color::Rgb(180, 190, 254);
pub const BLUE: Color = Color::Rgb(137, 180, 250);
pub const PEACH: Color = Color::Rgb(250, 179, 135);
pub const OVERLAY: Color = Color::Rgb(108, 112, 134);
pub const GREEN: Color = Color::Rgb(166, 227, 161);
pub const SURFACE: Color = Color::Rgb(69, 71, 90);

// ── ANSI escape codes for non-ratatui output (CLI commands) ──

pub const ANSI_PEACH: &str = "\x1b[38;2;250;179;135m";
pub const ANSI_OVERLAY: &str = "\x1b[38;2;108;112;134m";
pub const ANSI_SURFACE: &str = "\x1b[38;2;69;71;90m";
pub const ANSI_SUBTEXT: &str = "\x1b[38;2;166;173;200m";
pub const ANSI_WHITE: &str = "\x1b[38;2;205;214;244m";
pub const ANSI_BOLD: &str = "\x1b[1m";
pub const ANSI_RESET: &str = "\x1b[0m";
