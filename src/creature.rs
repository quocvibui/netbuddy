//! Pixel-art tamagotchi buddy — monochrome edition.
//!
//! Every shape is drawn on an integer pixel grid using small rectangles,
//! giving a chunky retro LCD / Game Boy feel.  Pure black & white palette.
//! DNA-driven uniqueness ensures every machine gets a distinct creature.

use nannou::prelude::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::gui::{COL_BLACK, COL_WHITE};

// ── DNA: unique identity per machine ─────────────────────────────────

pub struct CreatureDna {
    seed: u64,
    #[allow(dead_code)]
    pub body_roundness: f32,
    pub body_squish: f32,
    pub ear_style: u8,       // 0..5 (none, round, pointy, antenna, floppy, horns)
    pub eye_size: f32,
    pub eye_spacing: f32,
    pub eye_style: u8,       // 0..3 (round, wide, narrow, dot)
    pub mouth_style: u8,     // 0..3 (smile, cat, beak, snaggle)
    pub tail_style: u8,      // 0..4 (none, curl, spike, fluff, long)
    pub pattern_style: u8,   // 0..4 (none, stripes, spots, checker, half)
    pub limb_style: u8,      // 0..3 (none, stubby, long, flipper)
    pub marking_style: u8,   // 0..3 (none, belly patch, back stripe, mask)
    pub cheek_size: f32,
    pub body_w_bias: f32,    // 0.8..1.2 — wide vs narrow body
    pub head_bump: f32,      // 0..1 — how pronounced the head bump is
}

impl CreatureDna {
    pub fn from_machine() -> Self {
        // TODO: restore machine-unique seed for release
        // let mut data = String::new();
        // data.push_str(&hostname::get().unwrap_or_default().to_string_lossy());
        // data.push_str(&whoami::username());
        // let mut h = DefaultHasher::new();
        // data.hash(&mut h);
        // Self::from_seed(h.finish())

        // Temporary: random seed each launch to preview different designs
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        eprintln!("creature seed: {seed}");
        Self::from_seed(seed)
    }

    fn from_seed(seed: u64) -> Self {
        let f = |off: u32, lo: f32, hi: f32| -> f32 {
            lo + ((seed >> off) & 0xFF) as f32 / 255.0 * (hi - lo)
        };
        let b = |off: u32, modulo: u8| -> u8 {
            ((seed >> off) % modulo as u64) as u8
        };
        Self {
            seed,
            body_roundness: f(0, 0.6, 1.0),
            body_squish: f(8, 0.75, 1.0),
            ear_style: b(16, 6),
            eye_size: f(20, 0.12, 0.22),
            eye_spacing: f(28, 0.15, 0.32),
            eye_style: b(18, 4),
            mouth_style: b(24, 4),
            tail_style: b(26, 5),
            pattern_style: b(30, 5),
            limb_style: b(34, 4),
            marking_style: b(38, 4),
            cheek_size: f(52, 0.04, 0.12),
            body_w_bias: f(40, 0.85, 1.2),
            head_bump: f(48, 0.0, 1.0),
        }
    }

    /// Deterministic pseudo-random float [0,1) from DNA + index.
    fn r(&self, idx: usize) -> f32 {
        let mut h = DefaultHasher::new();
        (self.seed, idx).hash(&mut h);
        (h.finish() & 0xFFFF) as f32 / 65535.0
    }
}

// ── Reactive state ───────────────────────────────────────────────────

pub struct CreatureState {
    pub activity: f32,
    pub data_volume: f32,
    pub burst: f32,
    pub diversity: f32,
    pub time_of_day: f32,
    pub t: f32,
    pub content_vibe: ContentVibe,
}

/// What kind of content the user is browsing — affects creature posture.
#[derive(Clone, Copy, Default)]
pub enum ContentVibe {
    #[default]
    Neutral,
    Code,       // focused, still
    Social,     // bouncy, excited
    News,       // alert, attentive
    Shopping,   // curious, leaning
    Video,      // relaxed, chill
    Reading,    // calm, thoughtful
}

impl CreatureState {
    pub fn energy(&self) -> f32 {
        let base =
            self.activity * 0.4 + self.burst * 0.3 + self.diversity * 0.2 + self.data_volume * 0.1;
        let time_mod = 0.65 + 0.35 * (self.time_of_day * TAU - PI / 2.0).sin();
        (base * time_mod).clamp(0.0, 1.0)
    }

    pub fn sleepy(&self) -> bool {
        let night = self.time_of_day < 0.25 || self.time_of_day > 0.92;
        night && self.activity < 0.15
    }
}

// ── Dot drawing primitives ───────────────────────────────────────────
//
// Halftone style: every "pixel" is a round dot centered on a grid cell.
// Shading is expressed through dot SIZE — bigger = darker, smaller = lighter.
// This gives a newspaper/risograph print feel.

/// Grid spacing — distance between dot centers.
const PX: f32 = 5.0;

/// Default dot radius as fraction of grid cell (1.0 = full size, touching neighbors).
const DOT_FULL: f32 = 0.48;

/// Draw a single dot at grid position (gx, gy).
/// `size` is 0.0..1.0 where 1.0 = full size dot, 0.0 = invisible.
fn dot(draw: &Draw, gx: i32, gy: i32, origin: Point2, col: Srgb<u8>, size: f32) {
    if size <= 0.05 {
        return;
    }
    let x = origin.x + gx as f32 * PX;
    let y = origin.y + gy as f32 * PX;
    let r = PX * DOT_FULL * size;
    draw.ellipse().x_y(x, y).w_h(r * 2.0, r * 2.0).color(col);
}

/// Standard full-size dot — the default "pixel".
fn px(draw: &Draw, gx: i32, gy: i32, origin: Point2, col: Srgb<u8>) {
    dot(draw, gx, gy, origin, col, 1.0);
}

/// Draw a filled dot-art ellipse on the grid.
fn px_ellipse(draw: &Draw, cx: i32, cy: i32, rx: i32, ry: i32, origin: Point2, col: Srgb<u8>) {
    if rx <= 0 || ry <= 0 {
        return;
    }
    for dy in -ry..=ry {
        for dx in -rx..=rx {
            let nx = dx as f32 / rx as f32;
            let ny = dy as f32 / ry as f32;
            let d = nx * nx + ny * ny;
            if d <= 1.0 {
                // Dots get smaller toward the edge — halftone fade
                let edge_fade = 1.0 - (d - 0.7).max(0.0) / 0.3;
                dot(draw, cx + dx, cy + dy, origin, col, edge_fade);
            }
        }
    }
}

/// Halftone-shaded ellipse: dot SIZE varies instead of color swapping.
/// `density` 0.0 = sparse small dots, 1.0 = all full-size dots.
fn px_ellipse_dithered(
    draw: &Draw,
    cx: i32, cy: i32, rx: i32, ry: i32,
    origin: Point2,
    col_a: Srgb<u8>, col_b: Srgb<u8>,
    density: f32,
) {
    #[rustfmt::skip]
    const BAYER: [[f32; 4]; 4] = [
        [ 0.0/16.0,  8.0/16.0,  2.0/16.0, 10.0/16.0],
        [12.0/16.0,  4.0/16.0, 14.0/16.0,  6.0/16.0],
        [ 3.0/16.0, 11.0/16.0,  1.0/16.0,  9.0/16.0],
        [15.0/16.0,  7.0/16.0, 13.0/16.0,  5.0/16.0],
    ];
    if rx <= 0 || ry <= 0 {
        return;
    }
    for dy in -ry..=ry {
        for dx in -rx..=rx {
            let nx = dx as f32 / rx as f32;
            let ny = dy as f32 / ry as f32;
            if nx * nx + ny * ny <= 1.0 {
                let bx = ((cx + dx).rem_euclid(4)) as usize;
                let by = ((cy + dy).rem_euclid(4)) as usize;
                let bayer_val = BAYER[by][bx];
                if bayer_val < density {
                    // Secondary color — draw as smaller dot for texture
                    dot(draw, cx + dx, cy + dy, origin, col_b, 0.5 + density * 0.3);
                } else {
                    dot(draw, cx + dx, cy + dy, origin, col_a, 1.0);
                }
            }
        }
    }
}

fn px_hline(draw: &Draw, x0: i32, x1: i32, y: i32, origin: Point2, col: Srgb<u8>) {
    for x in x0..=x1 {
        px(draw, x, y, origin, col);
    }
}

fn px_rect(draw: &Draw, x: i32, y: i32, w: i32, h: i32, origin: Point2, col: Srgb<u8>) {
    for dy in 0..h {
        for dx in 0..w {
            px(draw, x + dx, y + dy, origin, col);
        }
    }
}

// ── Main draw entry point ────────────────────────────────────────────

pub fn draw_creature(
    draw: &Draw,
    dna: &CreatureDna,
    cs: &CreatureState,
    center: Point2,
    size: f32,
) {
    let e = cs.energy();
    let t = cs.t;

    // Bounce — varies by content vibe
    let (bounce_speed, bounce_amp) = match cs.content_vibe {
        ContentVibe::Social => (2.0 + e * 2.0, (2.0 + e * 4.0) as i32),
        ContentVibe::Code => (0.6, 1i32),
        ContentVibe::Video => (0.8 + e * 0.5, (1.0 + e * 1.5) as i32),
        ContentVibe::Reading => (0.7, (1.0 + e) as i32),
        _ => (1.2 + e * 1.5, (1.0 + e * 3.0) as i32),
    };
    let bounce_y = ((t * bounce_speed).sin() * bounce_amp as f32).round() as i32;

    let scale = (size / PX / 2.0) as i32;
    let body_rx = ((scale as f32 * 0.38) * dna.body_w_bias) as i32;
    let body_ry = (body_rx as f32 * dna.body_squish * 0.85) as i32;

    let origin = pt2(center.x, center.y + bounce_y as f32 * PX);

    // ── Shadow ───────────────────────────────────────────────────
    let shadow_y = -(body_ry + 3 + bounce_amp);
    let shadow_rx = body_rx - 1;
    px_ellipse(draw, 0, shadow_y, shadow_rx, 1, center, COL_BLACK);

    // ── Tail (behind body) ───────────────────────────────────────
    draw_px_tail(draw, dna, origin, body_rx, body_ry, t, e);

    // ── Ears (behind body) ───────────────────────────────────────
    draw_px_ears(draw, dna, cs, origin, body_rx, body_ry, t);

    // ── Limbs (behind body) ─────────────────────────────────────
    draw_px_limbs(draw, dna, origin, body_rx, body_ry, t, e);

    // ── Body ─────────────────────────────────────────────────────
    px_ellipse(draw, 0, 0, body_rx, body_ry, origin, COL_WHITE);

    // Head bump — makes some creatures look more head-heavy
    if dna.head_bump > 0.4 {
        let bump_r = (body_rx as f32 * 0.7 * dna.head_bump) as i32;
        let bump_y = (body_ry as f32 * 0.6) as i32;
        px_ellipse(draw, 0, bump_y, bump_r, (bump_r as f32 * 0.7) as i32, origin, COL_WHITE);
    }

    // Outline — 1px black border on the body for that pixel art pop
    draw_px_outline(draw, 0, 0, body_rx, body_ry, origin);

    // ── Pattern overlay ─────────────────────────────────────────
    draw_px_pattern(draw, dna, origin, body_rx, body_ry);

    // ── Markings ────────────────────────────────────────────────
    draw_px_markings(draw, dna, origin, body_rx, body_ry);

    // ── Cheeks ──────────────────────────────────────────────────
    let cheek_r = (size * dna.cheek_size / PX) as i32;
    let cheek_r = cheek_r.max(1);
    let cheek_gx = (body_rx as f32 * 0.55) as i32;
    let cheek_gy = -(body_ry / 5);
    // Cheeks — dithered B&W for texture
    px_ellipse_dithered(draw, -cheek_gx, cheek_gy, cheek_r, cheek_r - 1, origin, COL_WHITE, COL_BLACK, 0.25);
    px_ellipse_dithered(draw, cheek_gx, cheek_gy, cheek_r, cheek_r - 1, origin, COL_WHITE, COL_BLACK, 0.25);

    // ── Eyes ─────────────────────────────────────────────────────
    draw_px_eyes(draw, dna, cs, origin, body_rx, body_ry, t, e);

    // ── Mouth ────────────────────────────────────────────────────
    draw_px_mouth(draw, dna, cs, origin, body_rx, body_ry, e);

    // ── Sparkles (when active) ───────────────────────────────────
    if e > 0.15 {
        draw_px_sparkles(draw, dna, origin, body_rx, body_ry, t, e);
    }

    // ── Sleepy Z's ───────────────────────────────────────────────
    if cs.sleepy() {
        draw_px_zzz(draw, origin, body_rx, body_ry, t);
    }

    // ── Vibe indicator (small icon near creature) ────────────────
    draw_vibe_indicator(draw, cs, origin, body_rx, body_ry);
}

// ── Outline — draws a 1px border around the body ellipse ─────────────

fn draw_px_outline(draw: &Draw, cx: i32, cy: i32, rx: i32, ry: i32, origin: Point2) {
    if rx <= 0 || ry <= 0 {
        return;
    }
    for dy in -ry..=ry {
        for dx in -rx..=rx {
            let nx = dx as f32 / rx as f32;
            let ny = dy as f32 / ry as f32;
            let d = nx * nx + ny * ny;
            if d > 0.82 && d <= 1.0 {
                // Dots get smaller toward the outer edge — halftone fade out
                let t = (d - 0.82) / 0.18;
                let size = 0.9 - t * 0.4;
                dot(draw, cx + dx, cy + dy, origin, COL_BLACK, size);
            }
        }
    }
}

// ── Pattern overlay ──────────────────────────────────────────────────

fn draw_px_pattern(draw: &Draw, dna: &CreatureDna, origin: Point2, brx: i32, bry: i32) {
    match dna.pattern_style {
        1 => {
            // Horizontal stripes
            let stripe_gap = 3 + (dna.r(5000) * 2.0) as i32;
            for dy in -bry..=bry {
                if (dy.rem_euclid(stripe_gap)) == 0 {
                    for dx in -brx..=brx {
                        let nx = dx as f32 / brx as f32;
                        let ny = dy as f32 / bry as f32;
                        if nx * nx + ny * ny <= 0.82 {
                            px(draw, dx, dy, origin, COL_BLACK);
                        }
                    }
                }
            }
        }
        2 => {
            // Spots — scattered dark dots
            let spot_count = 4 + (dna.r(5100) * 4.0) as usize;
            for i in 0..spot_count {
                let angle = dna.r(5200 + i) * TAU;
                let dist = dna.r(5300 + i) * 0.6;
                let sx = (angle.cos() * dist * brx as f32).round() as i32;
                let sy = (angle.sin() * dist * bry as f32).round() as i32;
                let sr = 1 + (dna.r(5400 + i) * 1.5) as i32;
                px_ellipse(draw, sx, sy, sr, sr, origin, COL_BLACK);
            }
        }
        3 => {
            // Checkerboard dither
            for dy in -bry..=bry {
                for dx in -brx..=brx {
                    let nx = dx as f32 / brx as f32;
                    let ny = dy as f32 / bry as f32;
                    if nx * nx + ny * ny <= 0.7 && (dx + dy) % 2 == 0 {
                        px(draw, dx, dy, origin, COL_WHITE);
                    }
                }
            }
        }
        4 => {
            // Half body — bottom half darker
            for dy in -bry..0 {
                for dx in -brx..=brx {
                    let nx = dx as f32 / brx as f32;
                    let ny = dy as f32 / bry as f32;
                    if nx * nx + ny * ny <= 0.82 {
                        px(draw, dx, dy, origin, COL_WHITE);
                    }
                }
            }
        }
        _ => {} // no pattern
    }
}

// ── Markings ─────────────────────────────────────────────────────────

fn draw_px_markings(draw: &Draw, dna: &CreatureDna, origin: Point2, brx: i32, bry: i32) {
    match dna.marking_style {
        1 => {
            // Belly patch — lighter oval in center-bottom
            let belly_rx = (brx as f32 * 0.45) as i32;
            let belly_ry = (bry as f32 * 0.35) as i32;
            px_ellipse(draw, 0, -(bry / 4), belly_rx, belly_ry, origin, COL_WHITE);
        }
        2 => {
            // Back stripe — dark line down the center top
            for dy in (bry / 4)..=bry {
                let nx = 0.0f32;
                let ny = dy as f32 / bry as f32;
                if nx * nx + ny * ny <= 1.0 {
                    px(draw, 0, dy, origin, COL_BLACK);
                }
            }
        }
        3 => {
            // Eye mask — dark band across eye area
            let mask_y = bry / 5;
            let mask_h = 2;
            for dy in (mask_y - mask_h)..=(mask_y + mask_h) {
                for dx in -(brx - 2)..=(brx - 2) {
                    let nx = dx as f32 / brx as f32;
                    let ny = dy as f32 / bry as f32;
                    if nx * nx + ny * ny <= 0.85 {
                        px(draw, dx, dy, origin, COL_BLACK);
                    }
                }
            }
        }
        _ => {}
    }
}

// ── Ears ─────────────────────────────────────────────────────────────

fn draw_px_ears(
    draw: &Draw, dna: &CreatureDna, _cs: &CreatureState,
    origin: Point2, brx: i32, bry: i32, t: f32,
) {
    match dna.ear_style {
        0 => {} // no ears
        1 => {
            // Round ears
            let ear_r = (brx as f32 * 0.3) as i32;
            let ear_r = ear_r.max(2);
            let ex = (brx as f32 * 0.55) as i32;
            let ey = bry + ear_r / 2;
            px_ellipse(draw, -ex, ey, ear_r, ear_r, origin, COL_WHITE);
            px_ellipse(draw, ex, ey, ear_r, ear_r, origin, COL_WHITE);
            // Inner ear
            let ir = (ear_r as f32 * 0.5) as i32;
            let ir = ir.max(1);
            px_ellipse(draw, -ex, ey, ir, ir, origin, COL_BLACK);
            px_ellipse(draw, ex, ey, ir, ir, origin, COL_BLACK);
        }
        2 => {
            // Pointy ears — triangular
            let ear_h = (bry as f32 * 0.7) as i32;
            let ear_h = ear_h.max(3);
            let base_x = (brx as f32 * 0.45) as i32;
            for side in [-1i32, 1] {
                for row in 0..ear_h {
                    let width = ear_h - row;
                    let y = bry + row;
                    for dx in 0..width {
                        let col = if dx == 0 && row > ear_h / 3 { COL_BLACK } else { COL_WHITE };
                        px(draw, side * base_x + side * dx, y, origin, col);
                    }
                }
                // Tip pixel
                px(draw, side * base_x, bry + ear_h - 1, origin, COL_BLACK);
            }
        }
        3 => {
            // Antenna — pixel stalks with bobbing tips
            let stalk_h = (bry as f32 * 0.9) as i32;
            let stalk_h = stalk_h.max(4);
            let wobble = ((t * 2.0).sin() * 1.5).round() as i32;
            for side in [-1i32, 1] {
                let sx = side * (brx / 3);
                for row in 0..stalk_h {
                    let drift = if row > stalk_h / 2 {
                        side * wobble * (row - stalk_h / 2) / stalk_h
                    } else {
                        0
                    };
                    px(draw, sx + drift, bry + row, origin, COL_BLACK);
                }
                // Ball on top
                let tip_x = sx + side * wobble;
                let tip_y = bry + stalk_h;
                px_rect(draw, tip_x, tip_y, 2, 2, origin, COL_WHITE);
                // Outline the ball
                px(draw, tip_x - 1, tip_y, origin, COL_BLACK);
                px(draw, tip_x + 2, tip_y, origin, COL_BLACK);
                px(draw, tip_x, tip_y + 2, origin, COL_BLACK);
                px(draw, tip_x + 1, tip_y + 2, origin, COL_BLACK);
            }
        }
        4 => {
            // Floppy ears — droop down the sides
            let ear_len = (bry as f32 * 0.8) as i32;
            let ear_len = ear_len.max(3);
            let sway = ((t * 1.2).sin() * 1.0).round() as i32;
            for side in [-1i32, 1] {
                let base_x = (brx as f32 * 0.7) as i32;
                let base_y = bry / 3;
                for row in 0..ear_len {
                    let droop = row * row / (ear_len * 2); // quadratic droop
                    let x = side * base_x + side * (row / 3) + sway;
                    let y = base_y - droop;
                    px(draw, x, y, origin, COL_WHITE);
                    px(draw, x + side, y, origin, COL_WHITE);
                    // outline
                    px(draw, x + side * 2, y, origin, COL_BLACK);
                }
            }
        }
        _ => {
            // Horns — short, angular
            let horn_h = (bry as f32 * 0.5) as i32;
            let horn_h = horn_h.max(3);
            for side in [-1i32, 1] {
                let base_x = (brx as f32 * 0.3) as i32;
                for row in 0..horn_h {
                    let x = side * (base_x + row);
                    let y = bry + row;
                    px(draw, x, y, origin, COL_BLACK);
                    if row < horn_h - 1 {
                        px(draw, x, y + 1, origin, COL_BLACK);
                    }
                }
            }
        }
    }
}

// ── Tail ─────────────────────────────────────────────────────────────

fn draw_px_tail(
    draw: &Draw, dna: &CreatureDna, origin: Point2,
    brx: i32, bry: i32, t: f32, _e: f32,
) {
    let wag = ((t * 1.8).sin() * 2.0).round() as i32;

    match dna.tail_style {
        1 => {
            // Curl tail — spiral outward
            let len = 6 + (dna.r(6000) * 4.0) as i32;
            let side = if dna.r(6001) > 0.5 { 1i32 } else { -1 };
            for i in 0..len {
                let angle = i as f32 * 0.5;
                let r = 2.0 + i as f32 * 0.4;
                let tx = side * (brx + (angle.cos() * r).round() as i32);
                let ty = -(bry / 3) + (angle.sin() * r).round() as i32 + wag;
                px(draw, tx, ty, origin, COL_BLACK);
            }
        }
        2 => {
            // Spike tail — zigzag
            let side = if dna.r(6010) > 0.5 { 1i32 } else { -1 };
            let spikes = 3 + (dna.r(6011) * 2.0) as i32;
            for i in 0..spikes {
                let tx = side * (brx + i * 2);
                let ty_base = -(bry / 4) + wag;
                let spike_up = if i % 2 == 0 { 2 } else { -1 };
                px(draw, tx, ty_base + spike_up, origin, COL_BLACK);
                px(draw, tx, ty_base, origin, COL_BLACK);
                px(draw, tx + side, ty_base, origin, COL_BLACK);
            }
        }
        3 => {
            // Fluff tail — puffy ball
            let side = if dna.r(6020) > 0.5 { 1i32 } else { -1 };
            let fluff_x = side * (brx + 3);
            let fluff_y = -(bry / 4) + wag;
            let fluff_r = 2 + (dna.r(6021) * 2.0) as i32;
            px_ellipse_dithered(draw, fluff_x, fluff_y, fluff_r, fluff_r, origin, COL_WHITE, COL_BLACK, 0.3);
            // outline a few pixels
            px(draw, fluff_x - fluff_r - 1, fluff_y, origin, COL_BLACK);
            px(draw, fluff_x + fluff_r + 1, fluff_y, origin, COL_BLACK);
        }
        4 => {
            // Long tail — curves down
            let side = if dna.r(6030) > 0.5 { 1i32 } else { -1 };
            for i in 0..8 {
                let tx = side * (brx + i);
                let droop = (i * i) / 6;
                let ty = -(bry / 3) - droop + wag;
                px(draw, tx, ty, origin, COL_BLACK);
            }
        }
        _ => {} // no tail
    }
}

// ── Limbs ────────────────────────────────────────────────────────────

fn draw_px_limbs(
    draw: &Draw, dna: &CreatureDna, origin: Point2,
    brx: i32, bry: i32, t: f32, e: f32,
) {
    let walk_phase = (t * (1.0 + e * 2.0)).sin();

    match dna.limb_style {
        1 => {
            // Stubby feet — two small rectangles
            let foot_w = 3;
            let foot_h = 2;
            let spread = (brx as f32 * 0.4) as i32;
            let base_y = -(bry + foot_h);
            let bob_l = (walk_phase * 1.0).round() as i32;
            let bob_r = (-walk_phase * 1.0).round() as i32;
            px_rect(draw, -spread - 1, base_y + bob_l, foot_w, foot_h, origin, COL_BLACK);
            px_rect(draw, spread - 1, base_y + bob_r, foot_w, foot_h, origin, COL_BLACK);
        }
        2 => {
            // Long legs — thin stalks with feet
            let leg_len = 4 + (dna.r(7000) * 3.0) as i32;
            let spread = (brx as f32 * 0.3) as i32;
            let bob_l = (walk_phase * 1.5).round() as i32;
            let bob_r = (-walk_phase * 1.5).round() as i32;
            for side in [-1i32, 1] {
                let lx = side * spread;
                let bob = if side < 0 { bob_l } else { bob_r };
                for i in 0..leg_len {
                    px(draw, lx, -(bry + i) + bob, origin, COL_BLACK);
                }
                // Foot
                px(draw, lx - 1, -(bry + leg_len) + bob, origin, COL_BLACK);
                px(draw, lx + 1, -(bry + leg_len) + bob, origin, COL_BLACK);
            }
        }
        3 => {
            // Flippers — short wide appendages
            let spread = (brx as f32 * 0.6) as i32;
            let flip = (walk_phase * 1.0).round() as i32;
            for side in [-1i32, 1] {
                let fx = side * spread;
                let base_y = -(bry / 2);
                px(draw, fx + side, base_y + flip, origin, COL_BLACK);
                px(draw, fx + side * 2, base_y + flip, origin, COL_BLACK);
                px(draw, fx + side * 2, base_y - 1 + flip, origin, COL_BLACK);
                px(draw, fx + side * 3, base_y + flip, origin, COL_BLACK);
            }
        }
        _ => {} // no limbs — blob mode
    }
}

// ── Eyes ─────────────────────────────────────────────────────────────

fn draw_px_eyes(
    draw: &Draw, dna: &CreatureDna, cs: &CreatureState,
    origin: Point2, brx: i32, bry: i32, t: f32, e: f32,
) {
    let eye_r = ((dna.eye_size * 28.0) as i32).clamp(3, 6);
    let eye_gx = (brx as f32 * dna.eye_spacing * 1.8) as i32;
    let eye_gx = eye_gx.max(eye_r + 1);
    let eye_gy = bry / 5;

    let blink_cycle = ((t * 0.4).sin() * 8.0).clamp(-1.0, 1.0);
    let blinking = blink_cycle > 0.85;
    let sleepy = cs.sleepy();

    // Pupil tracking — adjusts with content vibe
    let look_speed = match cs.content_vibe {
        ContentVibe::Code => 0.15,
        ContentVibe::Reading => 0.2,
        _ => 0.3,
    };
    let look_x = ((t * look_speed).sin() * 1.5 * (0.3 + e)).round() as i32;
    let look_y = ((t * (look_speed + 0.1) + 1.0).cos() * 1.0).round() as i32;

    for side in [-1i32, 1] {
        let ex = side * eye_gx;

        if blinking || sleepy {
            let line_w = eye_r;
            px_hline(draw, ex - line_w, ex + line_w, eye_gy, origin, COL_BLACK);
            if sleepy {
                px_hline(draw, ex - line_w + 1, ex + line_w - 1, eye_gy + 1, origin, COL_BLACK);
            }
        } else {
            match dna.eye_style {
                0 => {
                    // Round eyes
                    let ey_r = (eye_r as f32 * 0.7) as i32;
                    let ey_r = ey_r.max(2);
                    // White sclera with black outline
                    px_ellipse(draw, ex, eye_gy, eye_r, ey_r, origin, COL_BLACK);
                    px_ellipse(draw, ex, eye_gy, eye_r - 1, ey_r - 1, origin, COL_WHITE);
                    // Pupil
                    let pupil_r = (eye_r as f32 * 0.35) as i32;
                    let pupil_r = pupil_r.max(1);
                    px_ellipse(draw, ex + look_x, eye_gy + look_y, pupil_r, pupil_r, origin, COL_BLACK);
                    // Highlight
                    px(draw, ex + look_x - 1, eye_gy + 1, origin, COL_WHITE);
                }
                1 => {
                    // Wide eyes — bigger, rounder, more expressive
                    let ey_r = eye_r;
                    px_ellipse(draw, ex, eye_gy, eye_r + 1, ey_r, origin, COL_BLACK);
                    px_ellipse(draw, ex, eye_gy, eye_r, ey_r - 1, origin, COL_WHITE);
                    let pupil_r = (eye_r as f32 * 0.4) as i32;
                    let pupil_r = pupil_r.max(1);
                    px_ellipse(draw, ex + look_x, eye_gy + look_y, pupil_r, pupil_r, origin, COL_BLACK);
                    px(draw, ex + look_x - 1, eye_gy + 1, origin, COL_WHITE);
                }
                2 => {
                    // Narrow/slit eyes
                    let ey_r = (eye_r as f32 * 0.4) as i32;
                    let ey_r = ey_r.max(1);
                    px_ellipse(draw, ex, eye_gy, eye_r, ey_r, origin, COL_BLACK);
                    px_ellipse(draw, ex, eye_gy, eye_r - 1, ey_r.max(1) - 1, origin, COL_WHITE);
                    // Slit pupil
                    px(draw, ex + look_x, eye_gy, origin, COL_BLACK);
                    px(draw, ex + look_x, eye_gy + 1, origin, COL_BLACK);
                    px(draw, ex + look_x, eye_gy - 1, origin, COL_BLACK);
                }
                _ => {
                    // Dot eyes — simple, cute
                    let dot_r = (eye_r as f32 * 0.5) as i32;
                    let dot_r = dot_r.max(2);
                    px_ellipse(draw, ex, eye_gy, dot_r, dot_r, origin, COL_BLACK);
                    px(draw, ex - 1, eye_gy + 1, origin, COL_WHITE);
                }
            }
        }
    }
}

// ── Mouth ────────────────────────────────────────────────────────────

fn draw_px_mouth(
    draw: &Draw, dna: &CreatureDna, cs: &CreatureState,
    origin: Point2, brx: i32, bry: i32, e: f32,
) {
    let mouth_y = -(bry / 4);
    let mouth_w = (brx as f32 * 0.25 + e * brx as f32 * 0.15) as i32;
    let mouth_w = mouth_w.max(1);

    if cs.sleepy() {
        px_hline(draw, -mouth_w, mouth_w, mouth_y, origin, COL_BLACK);
        return;
    }

    if e > 0.5 {
        // Excited — open mouth
        let mh = (mouth_w as f32 * 0.6) as i32;
        let mh = mh.max(1);
        px_ellipse(draw, 0, mouth_y - 1, mouth_w, mh, origin, COL_BLACK);
        return;
    }

    match dna.mouth_style {
        0 => {
            // Classic smile
            px_hline(draw, -mouth_w, mouth_w, mouth_y, origin, COL_BLACK);
            if mouth_w > 1 {
                px(draw, -mouth_w - 1, mouth_y + 1, origin, COL_BLACK);
                px(draw, mouth_w + 1, mouth_y + 1, origin, COL_BLACK);
            }
        }
        1 => {
            // Cat mouth — w shape
            px(draw, 0, mouth_y, origin, COL_BLACK);
            px(draw, -1, mouth_y + 1, origin, COL_BLACK);
            px(draw, 1, mouth_y + 1, origin, COL_BLACK);
            px(draw, -2, mouth_y, origin, COL_BLACK);
            px(draw, 2, mouth_y, origin, COL_BLACK);
        }
        2 => {
            // Beak — small triangle
            px(draw, 0, mouth_y, origin, COL_BLACK);
            px(draw, -1, mouth_y + 1, origin, COL_BLACK);
            px(draw, 1, mouth_y + 1, origin, COL_BLACK);
            px(draw, 0, mouth_y - 1, origin, COL_BLACK);
        }
        _ => {
            // Snaggle tooth — smile with a tooth pixel
            px_hline(draw, -mouth_w, mouth_w, mouth_y, origin, COL_BLACK);
            if mouth_w > 1 {
                px(draw, -mouth_w - 1, mouth_y + 1, origin, COL_BLACK);
                px(draw, mouth_w + 1, mouth_y + 1, origin, COL_BLACK);
            }
            // Single tooth
            px(draw, 1, mouth_y - 1, origin, COL_WHITE);
        }
    }
}

// ── Sparkles ─────────────────────────────────────────────────────────

fn draw_px_sparkles(
    draw: &Draw, dna: &CreatureDna, origin: Point2,
    brx: i32, bry: i32, t: f32, e: f32,
) {
    let count = (e * 5.0) as usize + 2;

    for i in 0..count {
        let angle = dna.r(i + 3000) * TAU + t * (0.3 + dna.r(i + 3100) * 0.4);
        let dist = (brx + bry) as f32 * 0.8 + dna.r(i + 3200) * brx as f32 * 0.5;
        let gx = (angle.cos() * dist).round() as i32;
        let gy = (angle.sin() * dist).round() as i32;

        let pulse = ((t * (2.0 + dna.r(i + 3300) * 2.0) + i as f32).sin() + 1.0) / 2.0;
        if pulse < 0.3 {
            continue;
        }

        // Halftone sparkle — center big, arms smaller, pulsing size
        dot(draw, gx, gy, origin, COL_BLACK, pulse);
        dot(draw, gx - 1, gy, origin, COL_BLACK, pulse * 0.6);
        dot(draw, gx + 1, gy, origin, COL_BLACK, pulse * 0.6);
        dot(draw, gx, gy - 1, origin, COL_BLACK, pulse * 0.6);
        dot(draw, gx, gy + 1, origin, COL_BLACK, pulse * 0.6);
    }
}

// ── Sleepy Z's ───────────────────────────────────────────────────────

fn draw_px_zzz(draw: &Draw, origin: Point2, brx: i32, bry: i32, t: f32) {
    let base_x = brx + 3;
    let base_y = bry + 2;
    let float_y = ((t * 0.8).sin() * 2.0).round() as i32;
    let phase = (t * 0.5).sin() * 0.5 + 0.5;

    // Small Z — smaller dots, fading
    if phase > 0.2 {
        let zx = base_x;
        let zy = base_y + float_y;
        let s = 0.5;
        for x in zx..=zx + 2 { dot(draw, x, zy + 2, origin, COL_BLACK, s); }
        dot(draw, zx + 1, zy + 1, origin, COL_BLACK, s);
        for x in zx..=zx + 2 { dot(draw, x, zy, origin, COL_BLACK, s); }
    }

    // Bigger Z — bigger dots
    if phase > 0.5 {
        let zx = base_x + 4;
        let zy = base_y + float_y + 4;
        let s = 0.75;
        for x in zx..=zx + 3 { dot(draw, x, zy + 3, origin, COL_BLACK, s); }
        dot(draw, zx + 2, zy + 2, origin, COL_BLACK, s);
        dot(draw, zx + 1, zy + 1, origin, COL_BLACK, s);
        for x in zx..=zx + 3 { dot(draw, x, zy, origin, COL_BLACK, s); }
    }
}

// ── Vibe indicator — small icon showing browsing context ─────────────

fn draw_vibe_indicator(
    draw: &Draw, cs: &CreatureState, origin: Point2,
    brx: i32, bry: i32,
) {
    let ix = -(brx + 5);
    let iy = bry + 2;

    match cs.content_vibe {
        ContentVibe::Code => {
            // < > brackets
            px(draw, ix, iy + 1, origin, COL_BLACK);
            px(draw, ix - 1, iy, origin, COL_BLACK);
            px(draw, ix, iy - 1, origin, COL_BLACK);
            px(draw, ix + 3, iy + 1, origin, COL_BLACK);
            px(draw, ix + 4, iy, origin, COL_BLACK);
            px(draw, ix + 3, iy - 1, origin, COL_BLACK);
        }
        ContentVibe::Social => {
            // Heart
            px(draw, ix, iy, origin, COL_BLACK);
            px(draw, ix + 2, iy, origin, COL_BLACK);
            px(draw, ix - 1, iy + 1, origin, COL_BLACK);
            px(draw, ix + 1, iy + 1, origin, COL_BLACK);
            px(draw, ix + 3, iy + 1, origin, COL_BLACK);
            px(draw, ix, iy + 2, origin, COL_BLACK);
            px(draw, ix + 2, iy + 2, origin, COL_BLACK);
            px(draw, ix + 1, iy + 3, origin, COL_BLACK);
        }
        ContentVibe::News => {
            // Exclamation mark
            px(draw, ix + 1, iy + 3, origin, COL_BLACK);
            px(draw, ix + 1, iy + 2, origin, COL_BLACK);
            px(draw, ix + 1, iy + 1, origin, COL_BLACK);
            px(draw, ix + 1, iy - 1, origin, COL_BLACK);
        }
        ContentVibe::Shopping => {
            // Dollar sign / bag
            px(draw, ix + 1, iy + 2, origin, COL_BLACK);
            px(draw, ix, iy + 1, origin, COL_BLACK);
            px(draw, ix + 1, iy, origin, COL_BLACK);
            px(draw, ix + 2, iy - 1, origin, COL_BLACK);
            px(draw, ix + 1, iy - 2, origin, COL_BLACK);
        }
        ContentVibe::Video => {
            // Play triangle
            px(draw, ix, iy + 2, origin, COL_BLACK);
            px(draw, ix, iy + 1, origin, COL_BLACK);
            px(draw, ix, iy, origin, COL_BLACK);
            px(draw, ix, iy - 1, origin, COL_BLACK);
            px(draw, ix + 1, iy + 1, origin, COL_BLACK);
            px(draw, ix + 1, iy, origin, COL_BLACK);
            px(draw, ix + 2, iy, origin, COL_BLACK);
        }
        ContentVibe::Reading => {
            // Open book
            px_hline(draw, ix, ix + 3, iy + 2, origin, COL_BLACK);
            px(draw, ix, iy + 1, origin, COL_BLACK);
            px(draw, ix + 3, iy + 1, origin, COL_BLACK);
            px_hline(draw, ix, ix + 3, iy, origin, COL_BLACK);
        }
        ContentVibe::Neutral => {}
    }
}
