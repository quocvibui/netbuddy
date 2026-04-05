#![allow(unexpected_cfgs)]
//! netmind — always-on browsing companion.
//!
//! Architecture: nannou GUI on the main thread, tokio runtime on a
//! background thread.  The two sides communicate through std::sync::mpsc
//! channels because nannou's model callback is !Send.
//!
//! Boot sequence:
//!   1. Init logging (all crates silenced except `netmind`)
//!   2. Spawn tokio thread → proxy, model loader, store ingestion, insight timer
//!   3. Run nannou GUI on main thread (required by macOS/wgpu)

mod bitmap_font;
mod creature;
mod gui;
mod insights;
mod llm;
mod proxy;
mod state;
mod store;

use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use tracing::info;

use crate::llm::LlmEngine;
use crate::state::{AppState, InsightStatus, ModelStatus, SharedState};
use crate::store::Store;

fn main() {
    // Silence wgpu/hudsucker/rustls noise; only show our own logs.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| {
                    "off,netmind=info"
                        .parse()
                        .unwrap()
                }),
        )
        .init();

    let state: SharedState = Arc::new(Mutex::new(AppState::default()));
    let llm: Arc<Mutex<Option<LlmEngine>>> = Arc::new(Mutex::new(None));

    // std::sync::mpsc channels bridge tokio ↔ nannou (can't use tokio::mpsc
    // across the nannou model boundary because it's !Send).
    let (insight_tx, insight_rx) = mpsc::channel::<String>();
    let (trigger_tx, trigger_rx) = mpsc::channel::<()>();    // "ask buddy" button

    let bg_state = state.clone();
    let bg_llm = llm.clone();

    // tokio runtime must live on a background thread — nannou owns main.
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        rt.block_on(async move {
            run_backend(bg_state, bg_llm, insight_tx, trigger_rx).await;
        });
    });

    gui::run_gui(state, insight_rx, trigger_tx);
}

/// Drives all async subsystems: proxy, model, store, and insight timer.
async fn run_backend(
    state: SharedState,
    llm: Arc<Mutex<Option<LlmEngine>>>,
    insight_tx: mpsc::Sender<String>,
    trigger_rx: mpsc::Receiver<()>,
) {
    let store = Store::open().expect("failed to open sled database");

    // Seed initial stats from whatever is already on disk.
    {
        let mut st = state.lock().unwrap();
        st.page_count = store.page_count();
        st.db_size_bytes = store.size_bytes();
    }

    // proxy → store pipeline: proxy sends PageEntry, ingestion task persists.
    let (entry_tx, mut entry_rx) = tokio::sync::mpsc::channel(256);

    // ── Proxy ────────────────────────────────────────────────────────
    let proxy_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = proxy::start_proxy(entry_tx, proxy_state).await {
            tracing::error!("proxy error: {e}");
        }
    });

    // ── Model loader ─────────────────────────────────────────────────
    // Downloads weights on first run (~350 MB), then loads from cache.
    let model_state = state.clone();
    let model_llm = llm.clone();
    tokio::spawn(async move {
        info!("starting model load...");
        let result = tokio::task::spawn_blocking(move || LlmEngine::load()).await;
        match result {
            Ok(Ok(engine)) => {
                *model_llm.lock().unwrap() = Some(engine);
                model_state.lock().unwrap().model_status = ModelStatus::Ready;
                info!("model loaded successfully");
            }
            Ok(Err(e)) => {
                let msg = format!("{e}");
                tracing::error!("model load failed: {msg}");
                model_state.lock().unwrap().model_status = ModelStatus::Error(msg);
            }
            Err(e) => {
                let msg = format!("task panicked: {e}");
                tracing::error!("{msg}");
                model_state.lock().unwrap().model_status = ModelStatus::Error(msg);
            }
        }
    });

    // ── Store ingestion ──────────────────────────────────────────────
    // Receives intercepted pages from proxy, persists to sled, updates stats.
    let ingest_store = store.clone();
    let ingest_state = state.clone();
    tokio::spawn(async move {
        while let Some(entry) = entry_rx.recv().await {
            let body_len = entry.body.len();
            if let Err(e) = ingest_store.save(&entry) {
                tracing::warn!("store save error: {e}");
            }
            let domain = entry.url.split("//")
                .nth(1)
                .and_then(|s| s.split('/').next())
                .unwrap_or("")
                .to_string();
            let mut st = ingest_state.lock().unwrap();
            st.page_count = ingest_store.page_count();
            st.db_size_bytes = ingest_store.size_bytes();
            st.record_request(body_len);
            if !domain.is_empty() {
                st.record_domain(domain);
            }
        }
    });

    // ── Insight timer ────────────────────────────────────────────────
    // Auto-generates every 30 min.  Also fires on-demand when the user
    // clicks "ask buddy" (polled via std::sync::mpsc at 50ms intervals
    // because tokio::select! can't await a std channel directly).
    let mut interval = tokio::time::interval(Duration::from_secs(1800));
    interval.tick().await; // discard the immediate first tick

    loop {
        tokio::select! {
            _ = interval.tick() => {
                try_generate_insight(&store, &llm, &state, &insight_tx).await;
            }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                if trigger_rx.try_recv().is_ok() {
                    try_generate_insight(&store, &llm, &state, &insight_tx).await;
                }
            }
        }
    }
}

/// Attempt one insight generation cycle.  No-ops if model isn't loaded yet.
async fn try_generate_insight(
    store: &Store,
    llm: &Arc<Mutex<Option<LlmEngine>>>,
    state: &SharedState,
    insight_tx: &mpsc::Sender<String>,
) {
    {
        let llm_guard = llm.lock().unwrap();
        if llm_guard.is_none() {
            info!("skipping insight generation — model not loaded yet");
            let mut st = state.lock().unwrap();
            st.insight_status = InsightStatus::Idle;
            return;
        }
    }

    {
        let mut st = state.lock().unwrap();
        st.insight_status = InsightStatus::Generating;
    }

    match insights::generate_insight(store, llm.clone()).await {
        Ok(insight) => {
            let _ = insight_tx.send(insight);
        }
        Err(e) => {
            tracing::error!("insight generation failed: {e}");
            let mut st = state.lock().unwrap();
            st.insight_status = InsightStatus::Idle;
        }
    }
}
