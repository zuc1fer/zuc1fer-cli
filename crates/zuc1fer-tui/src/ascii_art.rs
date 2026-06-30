use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::theme;

// Ouroboros — the serpent devouring its own tail: recursion, cycles,
// self-reference. A coding agent that works on its own source.
const OUROBOROS: &str = r#"
            ╓▄▄▄▄▄▄╖
         ▄█▀      ▀█▄
       ▟▀            ▀▙
      █▘              ▝█
     ▐▌                ▐▌
     ▐▌                ▐▌
      █▖              ▗█
       ▜▄            ▄▛
     ◣●▙ ▀█▄▄▄▄▄▄█▀
      ▀▀
"#;

const WORDMARK: &str = r#"
███████╗██╗   ██╗ ██████╗ ██╗███████╗███████╗██████╗
╚══███╔╝██║   ██║██╔════╝███║██╔════╝██╔════╝██╔══██╗
  ███╔╝ ██║   ██║██║      ╚██║█████╗  █████╗  ██████╔╝
 ███╔╝  ██║   ██║██║       ██║██╔══╝  ██╔══╝  ██╔══██╗
███████╗╚██████╔╝╚██████╗  ██║██║     ███████╗██║  ██║
╚══════╝ ╚═════╝  ╚═════╝  ╚═╝╚═╝     ╚══════╝╚═╝  ╚═╝
"#;

/// Lines for the empty-state splash: a glowing purple Ouroboros over the
/// wordmark, with a tagline and hints. Centered by the caller.
pub fn splash_lines() -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    let ring: Vec<&str> = OUROBOROS.lines().filter(|l| !l.trim().is_empty()).collect();
    let n = ring.len().max(1);
    for (i, row) in ring.iter().enumerate() {
        let color = if i * 3 < n {
            theme::ACCENT_LIGHT
        } else if i * 3 < n * 2 {
            theme::ACCENT
        } else {
            theme::ACCENT_DIM
        };
        lines.push(Line::from(Span::styled(
            (*row).to_string(),
            Style::default().fg(color),
        )));
    }

    lines.push(Line::from(""));

    let wm: Vec<&str> = WORDMARK.lines().filter(|l| !l.is_empty()).collect();
    let wm_w = wm.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    for row in wm {
        let mut s = (*row).to_string();
        s.push_str(&" ".repeat(wm_w.saturating_sub(row.chars().count())));
        lines.push(Line::from(Span::styled(
            s,
            Style::default()
                .fg(theme::ACCENT_LIGHT)
                .add_modifier(Modifier::BOLD),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "⟳  a recursive coding agent  ⟳",
        Style::default().fg(theme::ACCENT),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "type a message to begin   ·   /help   ·   /models",
        Style::default().fg(theme::TEXT_DIM),
    )));

    lines
}
