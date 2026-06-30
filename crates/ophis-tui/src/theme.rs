use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders};

// Dark, cyberpunk-purple palette.
pub const ACCENT: Color = Color::Rgb(160, 120, 245); // primary purple
pub const ACCENT_LIGHT: Color = Color::Rgb(196, 158, 255); // bright purple (highlights)
pub const ACCENT_DIM: Color = Color::Rgb(108, 88, 150); // muted purple (borders, chrome)
pub const SURFACE: Color = Color::Rgb(22, 15, 36); // deep background
pub const SURFACE_LIGHT: Color = Color::Rgb(40, 28, 62); // raised background
pub const TEXT: Color = Color::Rgb(224, 219, 235); // primary text
pub const TEXT_DIM: Color = Color::Rgb(142, 134, 162); // secondary text
pub const SUCCESS: Color = Color::Rgb(118, 214, 152);
pub const WARN: Color = Color::Rgb(240, 200, 92);
pub const ERROR: Color = Color::Rgb(236, 96, 124);

pub fn accent() -> Style {
    Style::default().fg(ACCENT)
}

pub fn accent_bold() -> Style {
    Style::default()
        .fg(ACCENT_LIGHT)
        .add_modifier(Modifier::BOLD)
}

pub fn text() -> Style {
    Style::default().fg(TEXT)
}

pub fn dim() -> Style {
    Style::default().fg(TEXT_DIM)
}

/// Selected row in any list / picker.
pub fn selection() -> Style {
    Style::default()
        .fg(SURFACE)
        .bg(ACCENT_LIGHT)
        .add_modifier(Modifier::BOLD)
}

/// Rounded, purple-bordered floating panel used by every modal.
pub fn modal_block(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .title(Line::from(format!(" ◆ {title} ")).style(accent_bold()))
        .style(Style::default().bg(SURFACE))
}

/// Left-edge border used by the sidebar panels.
pub fn panel_block() -> Block<'static> {
    Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(ACCENT_DIM))
}
