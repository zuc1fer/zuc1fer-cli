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

// ---------------------------------------------------------------------------
// Ouroboros rendering – original size logic
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// "ophis" pixel-art bitmap with per-pixel gradient shading
// ---------------------------------------------------------------------------

/// 6×12 pixel font (lower 6 bits of each byte).
const FONT_H: usize = 12;

const O: [u8; FONT_H] = [
    0x1E, 0x33, 0x33, 0x33, 0x33, 0x33, 0x33, 0x33, 0x33, 0x33, 0x33, 0x1E,
];
const P: [u8; FONT_H] = [
    0x3F, 0x33, 0x33, 0x33, 0x3F, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x30,
];
const H: [u8; FONT_H] = [
    0x33, 0x33, 0x33, 0x33, 0x3F, 0x33, 0x33, 0x33, 0x33, 0x33, 0x33, 0x33,
];
const I: [u8; FONT_H] = [
    0x3F, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x0C, 0x3F,
];
const S: [u8; FONT_H] = [
    0x3F, 0x33, 0x30, 0x30, 0x3F, 0x03, 0x03, 0x03, 0x03, 0x03, 0x33, 0x3F,
];

const CHAR_W: usize = 6;

struct LetterDef {
    offset: usize,
    data: &'static [u8; FONT_H],
}

const LETTERS: [LetterDef; 5] = [
    LetterDef {
        offset: 0,
        data: &O,
    },
    LetterDef {
        offset: 7,
        data: &P,
    },
    LetterDef {
        offset: 14,
        data: &H,
    },
    LetterDef {
        offset: 21,
        data: &I,
    },
    LetterDef {
        offset: 28,
        data: &S,
    },
];

const BITMAP_W: usize = 34;

fn build_bitmap() -> Vec<Vec<bool>> {
    let mut bm = vec![vec![false; BITMAP_W]; FONT_H];
    for ld in &LETTERS {
        for y in 0..FONT_H {
            let row = ld.data[y];
            for x in 0..CHAR_W {
                if row & (0x20 >> x) != 0 {
                    bm[y][ld.offset + x] = true;
                }
            }
        }
    }
    bm
}

fn is_edge(bm: &[Vec<bool>], x: usize, y: usize) -> bool {
    if x == 0 || x + 1 >= BITMAP_W || y == 0 || y + 1 >= FONT_H {
        return true;
    }
    !bm[y][x - 1] || !bm[y][x + 1] || !bm[y - 1][x] || !bm[y + 1][x]
}

fn pixel_color(_x: usize, y: usize, edge: bool) -> Rgb {
    let t = y as f64 / (FONT_H - 1) as f64;
    let bright = (1.0 - t * 0.35) * if edge { 0.70 } else { 1.0 };
    Rgb {
        r: (160.0 * bright) as u8,
        g: (120.0 * bright) as u8,
        b: (245.0 * bright) as u8,
    }
}

fn render_ophis_art() -> Vec<Line<'static>> {
    let bm = build_bitmap();
    let char_rows = FONT_H / 2;
    let mut lines = Vec::with_capacity(char_rows);

    for cy in 0..char_rows {
        let mut spans = Vec::with_capacity(BITMAP_W);
        for cx in 0..BITMAP_W {
            let u = bm[cy * 2][cx];
            let l = bm[cy * 2 + 1][cx];

            match (u, l) {
                (false, false) => spans.push(Span::raw(" ")),
                (true, false) => {
                    let c = pixel_color(cx, cy * 2, is_edge(&bm, cx, cy * 2));
                    spans.push(Span::styled(
                        "\u{2580}",
                        Style::default()
                            .fg(Color::Rgb(c.r, c.g, c.b))
                            .bg(Color::Reset),
                    ));
                }
                (false, true) => {
                    let c = pixel_color(cx, cy * 2 + 1, is_edge(&bm, cx, cy * 2 + 1));
                    spans.push(Span::styled(
                        "\u{2584}",
                        Style::default()
                            .fg(Color::Rgb(c.r, c.g, c.b))
                            .bg(Color::Reset),
                    ));
                }
                (true, true) => {
                    let uc = pixel_color(cx, cy * 2, is_edge(&bm, cx, cy * 2));
                    let lc = pixel_color(cx, cy * 2 + 1, is_edge(&bm, cx, cy * 2 + 1));
                    spans.push(Span::styled(
                        "\u{2580}",
                        Style::default()
                            .fg(Color::Rgb(uc.r, uc.g, uc.b))
                            .bg(Color::Rgb(lc.r, lc.g, lc.b)),
                    ));
                }
            }
        }
        lines.push(Line::from(spans));
    }

    lines
}

// ---------------------------------------------------------------------------
// Left column: art + text as pre-styled Lines
// ---------------------------------------------------------------------------

const LEFT_WIDTH: u16 = 36;
const TAGLINE: &str = "\u{27B3}  a recursive coding agent  \u{27B3}";
const HELP: &str = "type a message to begin  \u{00B7}  /help  \u{00B7}  /models";

fn build_left_column() -> Vec<Line<'static>> {
    let mut col: Vec<Line<'static>> = Vec::with_capacity(20);

    let accent_st = Style::default().fg(theme::ACCENT);
    let dim_st = Style::default().fg(theme::TEXT_DIM);
    let accent_dim_st = Style::default().fg(theme::ACCENT_DIM);

    col.extend(render_ophis_art());
    col.push(Line::from(""));
    col.push(Line::from(Span::styled(TAGLINE.to_string(), accent_st)));
    col.push(Line::from(""));

    col.push(Line::from(Span::styled(
        "an AI-native coding companion for your terminal.",
        dim_st,
    )));
    col.push(Line::from(Span::styled(
        "works with any model, on any codebase.",
        dim_st,
    )));

    col.push(Line::from(""));
    col.push(Line::from(Span::styled("crafted by zuc1fer", dim_st)));
    col.push(Line::from(Span::styled(
        "zuc1fer.business@gmail.com",
        accent_dim_st,
    )));
    col.push(Line::from(Span::styled(
        "https://t.me/zuc1fer",
        accent_dim_st,
    )));
    col.push(Line::from(Span::styled(
        "https://github.com/zuc1fer",
        accent_dim_st,
    )));

    col.push(Line::from(""));
    col.push(Line::from(Span::styled(HELP.to_string(), dim_st)));

    col
}

fn line_padded_to(line: &Line<'static>, target: u16) -> Line<'static> {
    let w: u16 = line
        .spans
        .iter()
        .map(|s| s.width() as u16)
        .sum::<u16>()
        .min(target);
    if w >= target {
        return line.clone();
    }
    let mut spans = line.spans.clone();
    spans.push(Span::raw(" ".repeat((target - w) as usize)));
    Line::from(spans)
}

// ---------------------------------------------------------------------------
// Public splash entry point
// ---------------------------------------------------------------------------

pub fn splash_display(term_width: u16, term_height: u16) -> Vec<Line<'static>> {
    let left = build_left_column();
    let left_count = left.len();

    let img_col_w: u32 = (term_width as u32).saturating_sub(LEFT_WIDTH as u32 + 4);

    if img_col_w < 24 || term_width < 54 {
        let mut out: Vec<Line<'static>> = Vec::with_capacity(left_count);
        for line in &left {
            out.push(line_padded_to(line, LEFT_WIDTH));
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

    let total_rows = left_count.max(right_count);
    let right_pad_top = (left_count as u16).saturating_sub(right_count as u16) / 2;

    let mut result: Vec<Line<'static>> = Vec::with_capacity(total_rows);

    for i in 0..total_rows {
        let mut spans: Vec<Span<'static>> = Vec::new();

        if i < left_count {
            let line = line_padded_to(&left[i], LEFT_WIDTH);
            spans.extend(line.spans);
        } else {
            spans.push(Span::raw(" ".repeat(LEFT_WIDTH as usize)));
        }

        spans.push(Span::raw("  "));

        let img_i = (i as u16).wrapping_sub(right_pad_top) as usize;
        if img_i < right_count {
            spans.extend(right[img_i].spans.clone());
        }

        result.push(Line::from(spans));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ophis_art_has_correct_width() {
        let lines = render_ophis_art();
        for (i, line) in lines.iter().enumerate() {
            let w: usize = line.spans.iter().map(|s| s.width()).sum();
            assert_eq!(
                w, BITMAP_W,
                "ophis art row {i} is {w} cols wide, expected {BITMAP_W}"
            );
        }
        assert_eq!(lines.len(), FONT_H / 2);
    }

    #[test]
    fn splash_does_not_panic() {
        let r = splash_display(120, 40);
        assert!(!r.is_empty());
    }

    #[test]
    fn splash_fallback_narrow() {
        let r = splash_display(50, 40);
        assert!(!r.is_empty());
    }
}
