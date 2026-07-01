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

/// Left-column data: (display text, optional style).
/// When style is `None` the line is blank.
const LEFT_ROWS: &[(&str, bool)] = &[
    ("█████ █████ ██ ██ █████ █████", true),
    ("██ ██ ██ ██ ██ ██  ██  ██   ", true),
    ("██ ██ █████ █████  ██  █████", true),
    ("██ ██ ██    ██ ██  ██     ██", true),
    ("█████ ██    ██ ██ █████ █████", true),
    ("", false),
    ("a recursive coding agent", true),
    ("", false),
    ("created by zuc1fer", true),
    ("zuc1fer.business@gmail.com", true),
    ("https://t.me/zuc1fer", true),
    ("https://github.com/zuc1fer", true),
    ("", false),
    (
        "type a message to begin  \u{00B7}  /help  \u{00B7}  /models",
        true,
    ),
];

const LEFT_WIDTH: u16 = 38;

fn left_line_style(is_art: bool) -> Style {
    if is_art {
        Style::default().fg(theme::ACCENT_LIGHT)
    } else {
        Style::default().fg(theme::TEXT_DIM)
    }
}

fn build_left_column() -> Vec<(String, Style)> {
    let mut col: Vec<(String, Style)> = Vec::with_capacity(LEFT_ROWS.len());

    let mut art_idx = 0;
    for (text, styled) in LEFT_ROWS {
        let style = if *styled {
            let is_art = art_idx < 5;
            art_idx += 1;
            left_line_style(is_art)
        } else {
            Style::default()
        };
        col.push((text.to_string(), style));
    }

    col
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

pub fn splash_display(term_width: u16, term_height: u16) -> Vec<Line<'static>> {
    let left = build_left_column();
    let left_row_count = left.len();

    let img_w: u32 = if term_width >= 52 {
        ((term_width as u32).saturating_sub(LEFT_WIDTH as u32 + 4))
            .min(28)
            .max(14)
    } else {
        0
    };

    if img_w < 14 {
        let mut out: Vec<Line<'static>> = Vec::with_capacity(left_row_count);
        for (text, style) in &left {
            if style.fg.is_some() {
                out.push(Line::from(Span::styled(text.clone(), *style)));
            } else {
                out.push(Line::from(""));
            }
        }
        return out;
    }

    let src_ratio = {
        let (w, h, _) = get_source();
        w as f64 / h as f64
    };
    let char_adj = 0.50;
    let max_h = (term_height as u32).saturating_sub(5).max(2);

    let out_w = img_w;
    let out_h = ((out_w as f64 * char_adj / src_ratio).max(2.0) as u32).min(max_h);

    let right = render_ouroboros(out_w, out_h);
    let right_row_count = right.len();

    let total_rows = left_row_count.max(right_row_count);
    let right_offset = left_row_count.saturating_sub(right_row_count) / 2;

    let mut result: Vec<Line<'static>> = Vec::with_capacity(total_rows);
    let gap = Span::raw("  ");

    for i in 0..total_rows {
        let mut spans: Vec<Span<'static>> = Vec::new();

        if i < left_row_count {
            let (text, style) = &left[i];
            if style.fg.is_some() {
                let padded = format!("{text:width$}", width = LEFT_WIDTH as usize);
                spans.push(Span::styled(padded, *style));
            } else {
                spans.push(Span::raw(" ".repeat(LEFT_WIDTH as usize)));
            }
        } else {
            spans.push(Span::raw(" ".repeat(LEFT_WIDTH as usize)));
        }

        spans.push(gap.clone());

        if i >= right_offset && (i - right_offset) < right_row_count {
            let img_idx = i - right_offset;
            spans.extend(right[img_idx].spans.clone());
        }

        result.push(Line::from(spans));
    }

    result
}
