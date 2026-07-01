use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::theme;

const RAW_DATA: &[u8] = include_bytes!("../ouroboros_raw.dat");

#[derive(Clone, Copy)]
struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

fn get_source() -> (u32, u32, &'static [u8]) {
    let w = u32::from_le_bytes(RAW_DATA[0..4].try_into().unwrap());
    let h = u32::from_le_bytes(RAW_DATA[4..8].try_into().unwrap());
    (w, h, &RAW_DATA[8..])
}

fn avg_block(x: u32, y: u32, out_w: u32, out_h: u32, src_w: u32, src_h: u32, pixels: &[u8]) -> Rgb {
    let xs = x * src_w / out_w;
    let xe = ((x + 1) * src_w / out_w).max(xs + 1);
    let ys = y * src_h / out_h;
    let ye = ((y + 1) * src_h / out_h).max(ys + 1);

    let mut r = 0u32;
    let mut g = 0u32;
    let mut b = 0u32;
    let mut count = 0u32;

    for sy in ys..ye {
        for sx in xs..xe {
            let off = ((sy * src_w + sx) * 3) as usize;
            r += pixels[off] as u32;
            g += pixels[off + 1] as u32;
            b += pixels[off + 2] as u32;
            count += 1;
        }
    }

    Rgb {
        r: (r / count) as u8,
        g: (g / count) as u8,
        b: (b / count) as u8,
    }
}

const BG_THRESHOLD: u16 = 200;

fn is_bg(c: Rgb) -> bool {
    (c.r as u16 + c.g as u16 + c.b as u16) / 3 > BG_THRESHOLD
}

fn render_ouroboros(out_w: u32, out_h: u32) -> Vec<Line<'static>> {
    let (src_w, src_h, pixels) = get_source();
    let out_scanlines = out_h * 2;
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(out_h as usize);

    for cy in 0..out_h {
        let mut spans = Vec::with_capacity(out_w as usize);
        for cx in 0..out_w {
            let u_orig = avg_block(cx, cy * 2, out_w, out_scanlines, src_w, src_h, pixels);
            let l_orig = avg_block(cx, cy * 2 + 1, out_w, out_scanlines, src_w, src_h, pixels);

            let u_bg = is_bg(u_orig);
            let l_bg = is_bg(l_orig);

            let u_inv = Rgb {
                r: 255 - u_orig.r,
                g: 255 - u_orig.g,
                b: 255 - u_orig.b,
            };
            let l_inv = Rgb {
                r: 255 - l_orig.r,
                g: 255 - l_orig.g,
                b: 255 - l_orig.b,
            };

            let style = |c: Rgb| Style::default().fg(Color::Rgb(c.r, c.g, c.b));

            match (u_bg, l_bg) {
                (true, true) => spans.push(Span::raw(" ")),
                (false, true) => {
                    spans.push(Span::styled("\u{2580}", style(u_inv).bg(Color::Reset)));
                }
                (true, false) => {
                    spans.push(Span::styled("\u{2584}", style(l_inv).bg(Color::Reset)));
                }
                (false, false) => {
                    spans.push(Span::styled(
                        "\u{2580}",
                        style(u_inv).bg(Color::Rgb(l_inv.r, l_inv.g, l_inv.b)),
                    ));
                }
            }
        }
        lines.push(Line::from(spans));
    }

    lines
}

/// 7-row detailed ASCII art for "ophis". Each row is exactly 39 columns.
const OPHIS_ART: [&str; 7] = [
    " █████  ██████  ██   ██ ███████ ██████ ",
    "██   ██ ██   ██ ██   ██    ██  ██     ",
    "██   ██ ██   ██ ██   ██    ██  ██     ",
    "██   ██ ██████  ███████    ██  ██████ ",
    "██   ██ ██     ██   ██    ██       ██",
    "██   ██ ██     ██   ██    ██       ██",
    " █████  ██     ██   ██ ███████ ██████ ",
];

const LEFT_WIDTH: u16 = 42;

const ART_WIDTH: usize = 39;

const TAGLINE: &str = "\u{27B3}  a recursive coding agent  \u{27B3}";

fn pad_art(s: &str) -> String {
    let mut out = String::with_capacity(ART_WIDTH);
    out.push_str(s);
    while out.chars().count() < ART_WIDTH {
        out.push(' ');
    }
    out
}

fn build_left_column() -> Vec<(String, Style)> {
    let mut col: Vec<(String, Style)> = Vec::with_capacity(20);

    let art_style = Style::default().fg(theme::ACCENT_LIGHT);

    for row in &OPHIS_ART {
        col.push((pad_art(row), art_style));
    }

    col.push((String::new(), Style::default()));

    col.push((TAGLINE.to_string(), Style::default().fg(theme::ACCENT)));

    col.push((String::new(), Style::default()));

    col.push((
        "created by zuc1fer".to_string(),
        Style::default().fg(theme::TEXT_DIM),
    ));
    col.push((
        "zuc1fer.business@gmail.com".to_string(),
        Style::default().fg(theme::ACCENT_DIM),
    ));
    col.push((
        "https://t.me/zuc1fer".to_string(),
        Style::default().fg(theme::ACCENT_DIM),
    ));
    col.push((
        "https://github.com/zuc1fer".to_string(),
        Style::default().fg(theme::ACCENT_DIM),
    ));

    col.push((String::new(), Style::default()));

    col.push((
        "type a message to begin  \u{00B7}  /help  \u{00B7}  /models".to_string(),
        Style::default().fg(theme::TEXT_DIM),
    ));

    col
}

pub fn splash_display(term_width: u16, term_height: u16) -> Vec<Line<'static>> {
    let left = build_left_column();
    let left_count = left.len() as u16;

    let img_col_w: u32 = (term_width as u32).saturating_sub(LEFT_WIDTH as u32 + 4);

    if img_col_w < 24 || term_width < 54 {
        let mut out: Vec<Line<'static>> = Vec::with_capacity(left_count as usize);
        for (text, style) in &left {
            if style.fg.is_some() {
                out.push(Line::from(Span::styled(text.clone(), *style)));
            } else {
                out.push(Line::from(""));
            }
        }
        return out;
    }

    let char_adj = 0.50;
    let max_h = (term_height as u32).saturating_sub(5).max(2);
    let max_w = img_col_w;
    let (src_w, src_h, _) = get_source();
    let src_ratio = src_w as f64 / src_h as f64;

    let w_from_h = (max_h as f64 / char_adj * src_ratio) as u32;
    let (out_w, out_h) = if w_from_h >= max_w.min(40) {
        (w_from_h.min(max_w).max(20), max_h)
    } else {
        let h_from_w = (max_w as f64 * char_adj / src_ratio).max(1.0) as u32;
        (max_w, h_from_w.min(max_h).max(2))
    };

    let right = render_ouroboros(out_w, out_h);
    let right_count = right.len();

    let total_rows = left_count.max(right_count as u16);
    let right_pad_top = left_count.saturating_sub(right_count as u16) / 2;

    let mut result: Vec<Line<'static>> = Vec::with_capacity(total_rows as usize);

    for i in 0..total_rows {
        let mut spans: Vec<Span<'static>> = Vec::new();

        if (i as usize) < left.len() {
            let (text, style) = &left[i as usize];
            if style.fg.is_some() {
                let padded = format!("{text:width$}", width = LEFT_WIDTH as usize);
                spans.push(Span::styled(padded, *style));
            } else {
                spans.push(Span::raw(" ".repeat(LEFT_WIDTH as usize)));
            }
        } else {
            spans.push(Span::raw(" ".repeat(LEFT_WIDTH as usize)));
        }

        spans.push(Span::raw("  "));

        let img_i = i.wrapping_sub(right_pad_top);
        if (img_i as usize) < right_count {
            spans.extend(right[img_i as usize].spans.clone());
        }

        result.push(Line::from(spans));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ophis_art_is_padded_to_39() {
        let col = build_left_column();
        for (i, (text, _)) in col.iter().enumerate().take(7) {
            assert_eq!(
                text.chars().count(),
                ART_WIDTH,
                "left column row {i} is {} chars wide, expected {ART_WIDTH}",
                text.chars().count()
            );
        }
    }

    #[test]
    fn splash_does_not_panic() {
        let result = splash_display(120, 40);
        assert!(!result.is_empty());
    }

    #[test]
    fn splash_fallback_narrow() {
        let result = splash_display(50, 40);
        assert!(!result.is_empty());
    }
}
