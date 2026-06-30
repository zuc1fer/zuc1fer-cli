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

pub fn splash_display(term_width: u16, term_height: u16) -> Vec<Line<'static>> {
    let (src_w, src_h, pixels) = get_source();

    let max_w = (term_width as u32).saturating_sub(4).max(20);
    let max_h = (term_height as u32).saturating_sub(5).max(2);

    let src_ratio = src_w as f64 / src_h as f64;
    let char_adj = 0.50;

    let w_from_h = (max_h as f64 / char_adj * src_ratio) as u32;
    let (out_w, out_h) = if w_from_h >= 40 {
        (w_from_h.min(max_w).max(20), max_h)
    } else {
        let h_from_w = (max_w as f64 * char_adj / src_ratio).max(1.0) as u32;
        (max_w, h_from_w.min(max_h).max(2))
    };

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

    let blank = Line::from("");

    lines.push(blank.clone());
    lines.push(Line::from(Span::styled(
        "\u{27B3}  a recursive coding agent  \u{27B3}",
        Style::default().fg(theme::ACCENT),
    )));
    lines.push(blank.clone());
    lines.push(Line::from(Span::styled(
        "type a message to begin   \u{00B7}   /help   \u{00B7}   /models",
        Style::default().fg(theme::TEXT_DIM),
    )));

    lines
}
