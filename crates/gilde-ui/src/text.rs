//! Text layout and drawing, after `FUN_0045f3a0`.
//!
//! Words are separated by spaces and placed with [`WORD_SPACING`]; a line that
//! would exceed the box width breaks before the word, or inside it at a soft
//! hyphen `~` / hyphen `-` if the first part still fits (the `~` then renders as
//! `-`). `~` is otherwise invisible. Flag 8 centres each line, 0x10 aligns right.

use crate::assets::{Font, WORD_SPACING};
use crate::draw::DrawList;

pub const ALIGN_CENTER: u32 = 8;
pub const ALIGN_RIGHT: u32 = 0x10;

pub struct Line {
    pub text: Vec<u8>,
    pub width: f32,
}

fn encode(s: &str) -> Vec<u8> {
    s.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect()
}

pub fn layout(font: &Font, text: &str, max_w: f32) -> Vec<Line> {
    let bytes = encode(text);
    let mut lines = Vec::new();
    for para in bytes.split(|&c| c == b'\n') {
        let mut cur: Vec<u8> = Vec::new();
        let mut cur_w = 0.0f32;
        for word in para.split(|&c| c == b' ').filter(|w| !w.is_empty()) {
            let ww = font.word_width(word);
            let add = if cur.is_empty() { ww } else { WORD_SPACING + ww };
            if cur.is_empty() || cur_w + add <= max_w {
                if !cur.is_empty() {
                    cur.push(b' ');
                }
                cur.extend_from_slice(word);
                cur_w += add;
                continue;
            }
            // Try to hyphenate at the last ~ or - that still fits.
            let mut split = None;
            for (i, &c) in word.iter().enumerate().rev() {
                if c == b'~' || c == b'-' {
                    let mut head = word[..i].to_vec();
                    head.push(b'-');
                    if cur_w + WORD_SPACING + font.word_width(&head) <= max_w {
                        split = Some((head, word[i + 1..].to_vec()));
                        break;
                    }
                }
            }
            match split {
                Some((head, tail)) => {
                    cur.push(b' ');
                    cur.extend_from_slice(&head);
                    let w = cur_w + WORD_SPACING + font.word_width(&head);
                    lines.push(Line { text: std::mem::take(&mut cur), width: w });
                    cur_w = font.word_width(&tail);
                    cur = tail;
                }
                None => {
                    lines.push(Line { text: std::mem::take(&mut cur), width: cur_w });
                    cur = word.to_vec();
                    cur_w = ww;
                }
            }
        }
        lines.push(Line { text: cur, width: cur_w });
    }
    lines
}

/// Draw text into a box; returns the height used.
pub fn draw(out: &mut DrawList, font: &Font, text: &str, x: f32, y: f32, w: f32, flags: u32, tint: [u8; 4]) -> f32 {
    let lh = font.line_height();
    let lines = layout(font, text, w);
    for (i, line) in lines.iter().enumerate() {
        let mut cx = if flags & ALIGN_CENTER != 0 && line.width <= w {
            x + ((w - line.width) / 2.0).floor()
        } else if flags & ALIGN_RIGHT != 0 {
            x + w - line.width
        } else {
            x
        };
        let cy = y + i as f32 * lh;
        for (k, &c) in line.text.iter().enumerate() {
            match c {
                b'~' => {}
                b' ' => cx += WORD_SPACING,
                _ => {
                    if k > 0 {
                        cx -= font.kern(c);
                    }
                    if let Some(g) = font.glyph(c) {
                        out.sprite(font.sprite(c), cx, cy, g.width as f32, g.height as f32, tint);
                    }
                    cx += font.advance(c);
                }
            }
        }
    }
    lines.len() as f32 * lh
}
