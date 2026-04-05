//! Tamagotchi-style GUI — fully pixel-art, no smooth anything.
//! Status bar uses bitmap_font; speech text uses VT323 pixel TTF.
//! Pure black & white only. No anti-aliasing. Cmd-Q or Escape to quit.

use std::sync::mpsc::{Receiver, Sender};

use chrono::Timelike;
use nannou::prelude::*;
use nannou::text::Font;

use crate::bitmap_font;
use crate::creature::{self, ContentVibe, CreatureDna, CreatureState};
use crate::state::{InsightStatus, ModelStatus, SharedState};

// ── Pure black & white ──────────────────────────────────────────────

pub const COL_BLACK: Srgb<u8> = Srgb { red: 0,   green: 0,   blue: 0,   standard: std::marker::PhantomData };
pub const COL_WHITE: Srgb<u8> = Srgb { red: 255, green: 255, blue: 255, standard: std::marker::PhantomData };

/// Square window size (width = height).
const WIN_SIZE: u32 = 480;

/// UI pixel size — matches creature PX for unified grid feel.
const UI_PX: f32 = 4.0;

/// Text pixel size for main text.
const TEXT_PX: f32 = 3.0;

/// Smaller text pixel size for hints.
const TEXT_PX_SM: f32 = 2.0;

pub struct Model {
    state: SharedState,
    insight_rx: Receiver<String>,
    trigger_tx: Sender<()>,
    t: f32,
    dna: CreatureDna,
    smooth_activity: f32,
    smooth_volume: f32,
    smooth_burst: f32,
    smooth_diversity: f32,
    content_vibe: ContentVibe,
    vt323_font: Font,
}

pub fn run_gui(state: SharedState, insight_rx: Receiver<String>, trigger_tx: Sender<()>) {
    INIT_DATA.with(|cell| {
        *cell.borrow_mut() = Some((state, insight_rx, trigger_tx));
    });
    nannou::app(model_fn).update(update).run();
}

thread_local! {
    static INIT_DATA: std::cell::RefCell<Option<(SharedState, Receiver<String>, Sender<()>)>> =
        std::cell::RefCell::new(None);
}

fn model_fn(app: &App) -> Model {
    let window_id = app.new_window()
        .size(WIN_SIZE, WIN_SIZE)
        .decorations(false)
        .transparent(true)
        .title("netmind")
        .view(view)
        .key_pressed(key_pressed)
        .mouse_pressed(mouse_pressed)
        .build()
        .unwrap();

    app.set_exit_on_escape(false);
    fix_macos_transparency(app, window_id);

    let (state, insight_rx, trigger_tx) =
        INIT_DATA.with(|cell| cell.borrow_mut().take().expect("init data missing"));

    let font_bytes = include_bytes!("../assets/VT323-Regular.ttf");
    let vt323_font = Font::from_bytes(font_bytes.as_ref()).expect("failed to load VT323 font");

    Model {
        state, insight_rx, trigger_tx, t: 0.0,
        dna: CreatureDna::from_machine(),
        smooth_activity: 0.0, smooth_volume: 0.0,
        smooth_burst: 0.0, smooth_diversity: 0.0,
        content_vibe: ContentVibe::Neutral,
        vt323_font,
    }
}

#[cfg(target_os = "macos")]
fn fix_macos_transparency(app: &App, window_id: nannou::window::Id) {
    use cocoa::appkit::NSWindow;
    use cocoa::base::{id, YES, NO};
    use objc::{msg_send, sel, sel_impl};
    use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};

    let window = app.window(window_id).expect("window not found");
    let raw = window.winit_window().raw_window_handle();

    if let RawWindowHandle::AppKit(handle) = raw {
        unsafe {
            let ns_window: id = handle.ns_window as id;

            // Transparent window — no OS border at all
            ns_window.setOpaque_(NO);
            ns_window.setBackgroundColor_(cocoa::appkit::NSColor::clearColor(std::ptr::null_mut()));
            ns_window.setHasShadow_(NO);

            // Allow dragging the window by clicking anywhere on it
            let _: () = msg_send![ns_window, setMovableByWindowBackground: YES];

            // Kill CALayer corner radius — prevents black bleed at edges
            let content_view: id = ns_window.contentView();
            let _: () = msg_send![content_view, setWantsLayer: YES];
            let layer: id = msg_send![content_view, layer];
            let _: () = msg_send![layer, setCornerRadius: 0.0_f64];
            let _: () = msg_send![layer, setMasksToBounds: YES];
            let _: () = msg_send![layer, setBorderWidth: 0.0_f64];
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn fix_macos_transparency(_app: &App, _window_id: nannou::window::Id) {}

fn key_pressed(app: &App, _m: &mut Model, key: Key) {
    if matches!(key, Key::Escape | Key::Q) { app.quit(); }
}

fn update(_app: &App, m: &mut Model, upd: Update) {
    let dt = upd.since_last.as_secs_f32();
    m.t += dt;

    while let Ok(insight) = m.insight_rx.try_recv() {
        m.content_vibe = detect_vibe(&insight);
        if let Ok(mut st) = m.state.lock() {
            st.latest_insight = Some(insight);
            st.insight_status = InsightStatus::Done;
            st.last_insight_time = Some(now_secs());
        }
    }

    if let Ok(st) = m.state.lock() {
        m.smooth_activity += ((st.requests_per_sec() / 5.0).min(1.0) - m.smooth_activity) * dt * 3.0;
        m.smooth_volume += (((st.total_bytes as f64 / 1e6) as f32 / 50.0).min(1.0) - m.smooth_volume) * dt * 2.0;
        m.smooth_burst += (st.burst_intensity() - m.smooth_burst) * dt * 4.0;
        m.smooth_diversity += (st.domain_diversity() - m.smooth_diversity) * dt * 1.5;

        if m.content_vibe as u8 == 0 {
            m.content_vibe = detect_vibe_from_domains(&st.recent_domains);
        }
    }
}

fn detect_vibe(text: &str) -> ContentVibe {
    let lower = text.to_lowercase();
    if lower.contains("code") || lower.contains("github") || lower.contains("stack overflow")
        || lower.contains("programming") || lower.contains("developer") || lower.contains("api")
        || lower.contains("rust") || lower.contains("python") || lower.contains("javascript")
    {
        ContentVibe::Code
    } else if lower.contains("twitter") || lower.contains("reddit") || lower.contains("instagram")
        || lower.contains("social") || lower.contains("tiktok") || lower.contains("facebook")
        || lower.contains("discord") || lower.contains("threads")
    {
        ContentVibe::Social
    } else if lower.contains("news") || lower.contains("article") || lower.contains("report")
        || lower.contains("breaking") || lower.contains("headline") || lower.contains("politics")
    {
        ContentVibe::News
    } else if lower.contains("buy") || lower.contains("shop") || lower.contains("price")
        || lower.contains("cart") || lower.contains("amazon") || lower.contains("deal")
    {
        ContentVibe::Shopping
    } else if lower.contains("youtube") || lower.contains("video") || lower.contains("watch")
        || lower.contains("stream") || lower.contains("netflix") || lower.contains("twitch")
    {
        ContentVibe::Video
    } else if lower.contains("blog") || lower.contains("essay") || lower.contains("wiki")
        || lower.contains("research") || lower.contains("paper") || lower.contains("book")
    {
        ContentVibe::Reading
    } else {
        ContentVibe::Neutral
    }
}

fn detect_vibe_from_domains(domains: &[String]) -> ContentVibe {
    for d in domains.iter().rev().take(3) {
        let lower = d.to_lowercase();
        if lower.contains("github") || lower.contains("stackoverflow") || lower.contains("gitlab") {
            return ContentVibe::Code;
        }
        if lower.contains("twitter") || lower.contains("reddit") || lower.contains("instagram") {
            return ContentVibe::Social;
        }
        if lower.contains("youtube") || lower.contains("twitch") || lower.contains("netflix") {
            return ContentVibe::Video;
        }
        if lower.contains("amazon") || lower.contains("ebay") || lower.contains("shopify") {
            return ContentVibe::Shopping;
        }
    }
    ContentVibe::Neutral
}

// ── Pixel-art UI primitives ─────────────────────────────────────────

fn ui_rect(draw: &Draw, x: f32, y: f32, w: f32, h: f32, col: Srgb<u8>) {
    draw.rect().x_y(x, y).w_h(w, h).color(col);
}

fn ui_border(draw: &Draw, x: f32, y: f32, w: f32, h: f32, col: Srgb<u8>) {
    let hw = w / 2.0;
    let hh = h / 2.0;
    draw.rect().x_y(x, y + hh - UI_PX / 2.0).w_h(w, UI_PX).color(col);
    draw.rect().x_y(x, y - hh + UI_PX / 2.0).w_h(w, UI_PX).color(col);
    draw.rect().x_y(x - hw + UI_PX / 2.0, y).w_h(UI_PX, h).color(col);
    draw.rect().x_y(x + hw - UI_PX / 2.0, y).w_h(UI_PX, h).color(col);
}

fn ui_status_dot(draw: &Draw, x: f32, y: f32, col: Srgb<u8>) {
    draw.rect().x_y(x, y).w_h(UI_PX * 2.0, UI_PX * 2.0).color(col);
}

// ── View ─────────────────────────────────────────────────────────────

fn view(app: &App, m: &Model, frame: Frame) {
    let draw = app.draw();
    let st = m.state.lock().unwrap();
    let win = app.window_rect();

    // Transparent clear — the OS window is fully see-through, no border
    frame.clear(nannou::color::rgba(0.0, 0.0, 0.0, 0.0));

    let panel_w = win.w();
    let panel_h = win.h();

    // Draw our own opaque white background — slightly inset to avoid
    // any edge pixel that might show the 1px OS border
    ui_rect(&draw, 0.0, 0.0, panel_w, panel_h, COL_WHITE);

    // Outer black border
    ui_border(&draw, 0.0, 0.0, panel_w, panel_h, COL_BLACK);
    // Inner border for retro double-line look
    ui_border(&draw, 0.0, 0.0, panel_w - UI_PX * 2.0, panel_h - UI_PX * 2.0, COL_BLACK);

    // ── Status bar (top) ────────────────────────────────────────
    let top_y = panel_h / 2.0 - UI_PX * 5.0;
    let left_x = -panel_w / 2.0 + UI_PX * 4.0;
    let right_x = panel_w / 2.0 - UI_PX * 4.0;

    // Divider line — 1px thick black line under status
    let divider_y = top_y - UI_PX * 4.0;
    ui_rect(&draw, 0.0, divider_y, panel_w - UI_PX * 4.0, UI_PX, COL_BLACK);

    // Proxy status square + port — left
    let pcol = if st.proxy_active { COL_BLACK } else { COL_WHITE };
    ui_status_dot(&draw, left_x + UI_PX, top_y, pcol);
    if st.proxy_active {
        // Draw a white inner pixel to make an "outlined" square when active
    }
    let port_str = format!(":{}", st.proxy_port);
    bitmap_font::draw_text(&draw, &port_str, left_x + UI_PX * 4.0, top_y + TEXT_PX * 2.5, TEXT_PX, COL_BLACK);

    // Page count — center
    let pg_str = format!("{} PG", st.page_count);
    bitmap_font::draw_text_centered(&draw, &pg_str, 0.0, top_y, TEXT_PX, COL_BLACK);

    // Model status — right
    let mtxt = match &st.model_status {
        ModelStatus::Loading => {
            let frames = [".  ", " . ", "  ."];
            frames[(m.t * 3.0) as usize % 3].to_string()
        }
        ModelStatus::Ready => "OK".to_string(),
        ModelStatus::Error(_) => "ERR".to_string(),
    };
    let mcol = match &st.model_status {
        ModelStatus::Ready => COL_BLACK,
        _ => COL_BLACK,
    };
    ui_status_dot(&draw, right_x - UI_PX * 14.0, top_y, mcol);
    bitmap_font::draw_text_right(&draw, &mtxt, right_x, top_y, TEXT_PX, COL_BLACK);

    // ── Layout: bottom section first to know creature bounds ─────
    let bottom_edge = -panel_h / 2.0 + UI_PX * 3.0;

    // ESC hint — very bottom
    let esc_cy = bottom_edge + TEXT_PX_SM * 3.0;
    bitmap_font::draw_text_centered(&draw, "ESC", 0.0, esc_cy, TEXT_PX_SM, COL_BLACK);

    // Ask button — just above ESC
    let btn_h = UI_PX * 6.0;
    let btn_w = UI_PX * 18.0;
    let btn_cy = esc_cy + TEXT_PX_SM * 5.0 + UI_PX * 1.0 + btn_h / 2.0;

    let is_gen = st.insight_status == InsightStatus::Generating;
    ui_rect(&draw, 0.0, btn_cy, btn_w, btn_h, COL_BLACK);
    let btn_label = if is_gen { "..." } else { "ASK" };
    bitmap_font::draw_text_centered(&draw, btn_label, 0.0, btn_cy, TEXT_PX, COL_WHITE);

    // ── Speech text — directly on white bg, no box ──────────────
    let text_margin = UI_PX * 6.0;
    let text_max_w = panel_w - text_margin * 2.0;
    let text_area_bot = btn_cy + btn_h / 2.0 + UI_PX * 2.0;
    let text_area_top = text_area_bot + UI_PX * 20.0;
    let text_cy = (text_area_bot + text_area_top) / 2.0;

    let speech_text = match (&st.model_status, &st.insight_status, &st.latest_insight) {
        (ModelStatus::Loading, _, _) => {
            let dots = ".".repeat((m.t * 2.0) as usize % 4);
            format!("loading brain{dots}")
        }
        (ModelStatus::Error(_), _, _) => "brain broke :(".to_string(),
        (_, InsightStatus::Generating, _) => {
            let dots = ".".repeat((m.t * 2.5) as usize % 4);
            format!("thinking{dots}")
        }
        (_, _, Some(insight_text)) => insight_text.clone(),
        _ => ". . .".to_string(),
    };

    // Dynamic font sizing: shorter text gets bigger font
    let char_count = speech_text.len();
    let font_size = if char_count < 40 { 24.0 }
        else if char_count < 80 { 20.0 }
        else if char_count < 120 { 16.0 }
        else { 14.0 };

    draw.text(&speech_text)
        .font(m.vt323_font.clone())
        .font_size(font_size as u32)
        .color(COL_BLACK)
        .x_y(0.0, text_cy)
        .w(text_max_w)
        .align_text_middle_y()
        .center_justify();

    // ── Creature — fills space between status bar and text ───────
    let creature_top = divider_y - UI_PX * 2.0;
    let creature_bot = text_area_top + UI_PX * 2.0;
    let creature_cy = (creature_top + creature_bot) / 2.0;
    let creature_zone = creature_top - creature_bot;
    let creature_size = creature_zone.min(panel_w - UI_PX * 8.0) * 1.2;

    let hour = chrono::Local::now().hour() as f32;
    let cs = CreatureState {
        activity: m.smooth_activity,
        data_volume: m.smooth_volume,
        burst: m.smooth_burst,
        diversity: m.smooth_diversity,
        time_of_day: hour / 24.0,
        t: m.t,
        content_vibe: m.content_vibe,
    };
    creature::draw_creature(&draw, &m.dna, &cs, pt2(0.0, creature_cy), creature_size);

    drop(st);
    draw.to_frame(app, &frame).unwrap();
}

// ── Mouse ────────────────────────────────────────────────────────────

fn mouse_pressed(app: &App, m: &mut Model, _button: MouseButton) {
    let mouse = app.mouse.position();
    let win = app.window_rect();
    let panel_h = win.h();
    let bottom_edge = -panel_h / 2.0 + UI_PX * 3.0;
    let esc_cy = bottom_edge + TEXT_PX_SM * 3.0;
    let btn_h = UI_PX * 6.0;
    let btn_cy = esc_cy + TEXT_PX_SM * 5.0 + UI_PX * 1.0 + btn_h / 2.0;
    let btn_w = UI_PX * 18.0;
    let btn = geom::Rect::from_x_y_w_h(0.0, btn_cy, btn_w, btn_h);
    if btn.contains(mouse) {
        let mut st = m.state.lock().unwrap();
        if st.insight_status == InsightStatus::Generating { return; }
        st.insight_status = InsightStatus::Generating;
        drop(st);
        let _ = m.trigger_tx.send(());
    }
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
