use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::theme;

/// Edge map stored at 400×192 (packed 1 bit/pixel) — enough detail
/// to look sharp on any terminal from 20 to 800+ columns wide.
const EDGE_MAP: &[u8] = include_bytes!("ouroboros_edge.dat");

fn unpack_edge_map(data: &[u8]) -> (u32, u32, Vec<u8>) {
    let (header, rest) = data.split_at(8);
    let w = u32::from_le_bytes(header[0..4].try_into().unwrap());
    let h = u32::from_le_bytes(header[4..8].try_into().unwrap());
    let mut out = Vec::with_capacity((w * h) as usize);
    for &byte in rest {
        for j in (0..8u32).rev() {
            out.push(if (byte >> j) & 1 == 1 { 255u8 } else { 0u8 });
        }
    }
    (w, h, out)
}

/// Render the Ouroboros ASCII art at the given terminal width.
fn render_ouroboros(term_width: u16) -> Vec<Line<'static>> {
    let (src_w, src_h, src) = unpack_edge_map(EDGE_MAP);

    // Output width: fill the terminal with 4-char padding on each side
    let out_w = (term_width as u32).saturating_sub(4).max(20).min(src_w * 2);
    let out_h = (out_w * src_h / src_w).max(1);

    let ramp: &[u8] = b" .:-=+*#%@";
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(out_h as usize);

    for y in 0..out_h {
        let sy = (y * src_h / out_h) as usize;
        let mut row = String::with_capacity(out_w as usize);
        for x in 0..out_w {
            let sx = (x * src_w / out_w) as usize;
            let p = src[sy * src_w as usize + sx];
            let idx = (p as usize * (ramp.len() - 1)) / 255;
            row.push(ramp[idx] as char);
        }

        let trimmed = row.trim_end();
        if trimmed.is_empty() {
            lines.push(Line::from(""));
        } else {
            lines.push(Line::from(Span::styled(
                trimmed.to_string(),
                Style::default().fg(theme::ACCENT_LIGHT),
            )));
        }
    }

    lines
}

/// Full splash page: Ouroboros art + branding tagline + hints.
pub fn splash_display(term_width: u16) -> Vec<Line<'static>> {
    let mut lines = render_ouroboros(term_width);

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
