//! Bitmap font — every glyph is a 3×5 pixel grid drawn as rectangles.
//! No anti-aliasing, no smooth curves, pure pixel art text.
//! Supports uppercase A-Z, digits 0-9, and basic punctuation.

use nannou::prelude::*;

/// Each glyph is 3 wide × 5 tall. Stored as 5 rows of 3 bits each,
/// packed into a u16 (bit 14 = top-left, bit 0 = bottom-right).
/// Row order: top to bottom. Bit order within row: left to right.
const GLYPH_W: i32 = 3;
pub const GLYPH_H: i32 = 5;

/// Spacing between characters in grid cells.
const CHAR_SPACING: i32 = 1;

/// Total width of one character cell in grid units.
const CELL_W: i32 = GLYPH_W + CHAR_SPACING;

fn glyph_data(ch: char) -> u16 {
    match ch.to_ascii_uppercase() {
        'A' => 0b_010_111_101_111_101,
        'B' => 0b_110_101_110_101_110,
        'C' => 0b_011_100_100_100_011,
        'D' => 0b_110_101_101_101_110,
        'E' => 0b_111_100_110_100_111,
        'F' => 0b_111_100_110_100_100,
        'G' => 0b_011_100_101_101_011,
        'H' => 0b_101_101_111_101_101,
        'I' => 0b_111_010_010_010_111,
        'J' => 0b_001_001_001_101_010,
        'K' => 0b_101_101_110_101_101,
        'L' => 0b_100_100_100_100_111,
        'M' => 0b_101_111_111_101_101,
        'N' => 0b_101_111_111_111_101,
        'O' => 0b_010_101_101_101_010,
        'P' => 0b_110_101_110_100_100,
        'Q' => 0b_010_101_101_111_011,
        'R' => 0b_110_101_110_101_101,
        'S' => 0b_011_100_010_001_110,
        'T' => 0b_111_010_010_010_010,
        'U' => 0b_101_101_101_101_010,
        'V' => 0b_101_101_101_010_010,
        'W' => 0b_101_101_111_111_101,
        'X' => 0b_101_101_010_101_101,
        'Y' => 0b_101_101_010_010_010,
        'Z' => 0b_111_001_010_100_111,
        '0' => 0b_010_101_101_101_010,
        '1' => 0b_010_110_010_010_111,
        '2' => 0b_110_001_010_100_111,
        '3' => 0b_110_001_010_001_110,
        '4' => 0b_101_101_111_001_001,
        '5' => 0b_111_100_110_001_110,
        '6' => 0b_011_100_111_101_011,
        '7' => 0b_111_001_010_010_010,
        '8' => 0b_010_101_010_101_010,
        '9' => 0b_110_101_111_001_110,
        '.' => 0b_000_000_000_000_010,
        ',' => 0b_000_000_000_010_100,
        '!' => 0b_010_010_010_000_010,
        '?' => 0b_110_001_010_000_010,
        ':' => 0b_000_010_000_010_000,
        ';' => 0b_000_010_000_010_100,
        '-' => 0b_000_000_111_000_000,
        '+' => 0b_000_010_111_010_000,
        '=' => 0b_000_111_000_111_000,
        '\'' => 0b_010_010_000_000_000,
        '"' => 0b_101_101_000_000_000,
        '(' => 0b_010_100_100_100_010,
        ')' => 0b_010_001_001_001_010,
        '/' => 0b_001_001_010_100_100,
        '_' => 0b_000_000_000_000_111,
        '<' => 0b_001_010_100_010_001,
        '>' => 0b_100_010_001_010_100,
        '#' => 0b_101_111_101_111_101,
        '@' => 0b_010_101_111_100_011,
        '*' => 0b_101_010_111_010_101,
        '&' => 0b_010_101_010_101_011,
        '%' => 0b_101_001_010_100_101,
        '$' => 0b_011_110_010_011_110,
        '[' => 0b_110_100_100_100_110,
        ']' => 0b_011_001_001_001_011,
        '{' => 0b_011_010_110_010_011,
        '}' => 0b_110_010_011_010_110,
        '^' => 0b_010_101_000_000_000,
        '~' => 0b_000_011_110_000_000,
        '`' => 0b_100_010_000_000_000,
        _ => 0b_000_000_000_000_000, // space or unknown
    }
}

/// Draw a single character at grid position (gx, gy) = top-left corner.
/// `px_size` is the size of each pixel cell in screen coordinates.
fn draw_glyph(draw: &Draw, ch: char, gx: f32, gy: f32, px_size: f32, col: Srgb<u8>) {
    let bits = glyph_data(ch);
    if bits == 0 && ch != ' ' && ch != '\0' {
        return; // unknown char, skip
    }
    for row in 0..GLYPH_H {
        for col_idx in 0..GLYPH_W {
            let bit_pos = (GLYPH_H - 1 - row) * GLYPH_W + (GLYPH_W - 1 - col_idx);
            if bits & (1 << bit_pos) != 0 {
                let x = gx + col_idx as f32 * px_size;
                let y = gy - row as f32 * px_size;
                draw.rect().x_y(x, y).w_h(px_size, px_size).color(col);
            }
        }
    }
}

/// Draw a string of pixel text. Returns the total width in screen coords.
/// `origin` is the top-left corner of the first character.
pub fn draw_text(
    draw: &Draw,
    text: &str,
    origin_x: f32,
    origin_y: f32,
    px_size: f32,
    col: Srgb<u8>,
) -> f32 {
    let mut x = origin_x;
    for ch in text.chars() {
        draw_glyph(draw, ch, x, origin_y, px_size, col);
        x += CELL_W as f32 * px_size;
    }
    text.len() as f32 * CELL_W as f32 * px_size
}

/// Draw pixel text centered at (cx, cy).
pub fn draw_text_centered(
    draw: &Draw,
    text: &str,
    cx: f32,
    cy: f32,
    px_size: f32,
    col: Srgb<u8>,
) {
    let total_w = text.len() as f32 * CELL_W as f32 * px_size - CHAR_SPACING as f32 * px_size;
    let total_h = GLYPH_H as f32 * px_size;
    let start_x = cx - total_w / 2.0;
    let start_y = cy + total_h / 2.0;
    draw_text(draw, text, start_x, start_y, px_size, col);
}

/// Draw pixel text right-aligned, ending at (right_x, y).
pub fn draw_text_right(
    draw: &Draw,
    text: &str,
    right_x: f32,
    y: f32,
    px_size: f32,
    col: Srgb<u8>,
) {
    let total_w = text.len() as f32 * CELL_W as f32 * px_size - CHAR_SPACING as f32 * px_size;
    let start_x = right_x - total_w;
    let start_y = y + GLYPH_H as f32 * px_size / 2.0;
    draw_text(draw, text, start_x, start_y, px_size, col);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glyph_data_known_chars() {
        assert_ne!(glyph_data('A'), 0);
        assert_ne!(glyph_data('Z'), 0);
        assert_ne!(glyph_data('0'), 0);
        assert_ne!(glyph_data('.'), 0);
    }

    #[test]
    fn test_glyph_data_space() {
        assert_eq!(glyph_data(' '), 0);
    }

}
