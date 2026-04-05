#![allow(unexpected_cfgs)]
//! netmind — always-on browsing companion.
//!
//! Architecture: nannou GUI on the main thread, tokio runtime on a
//! background thread.  The two sides communicate through std::sync::mpsc
//! channels because nannou's model callback is !Send.
//!
//! Boot sequence:
//!   1. Init logging (all crates silenced except `netmind`)
//!   2. Load config from `netmind.toml`
//!   3. Spawn tokio thread → proxy, model loader, store ingestion, insight timer
//!   4. Run nannou GUI on main thread (required by macOS/wgpu)

mod bitmap_font;
mod config;
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

use crate::config::Config;
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

    let config = Config::load();
    info!("config: auto_response={}, interval={}s", config.auto_response, config.auto_response_interval);

    let state: SharedState = Arc::new(Mutex::new(AppState::default()));
    {
        let mut st = state.lock().unwrap();
        st.auto_response = config.auto_response;
        st.auto_response_interval = config.auto_response_interval;
    }

    let llm: Arc<Mutex<Option<LlmEngine>>> = Arc::new(Mutex::new(None));

    // std::sync::mpsc channels bridge tokio ↔ nannou (can't use tokio::mpsc
    // across the nannou model boundary because it's !Send).
    let (insight_tx, insight_rx) = mpsc::channel::<String>();
    let (trigger_tx, trigger_rx) = mpsc::channel::<()>();    // "ask buddy" button

    let bg_state = state.clone();
    let bg_llm = llm.clone();
    let bg_config = config;

    // tokio runtime must live on a background thread — nannou owns main.
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        rt.block_on(async move {
            run_backend(bg_state, bg_llm, insight_tx, trigger_rx, bg_config).await;
        });
    });

    gui::run_gui(state, insight_rx, trigger_tx);

    // Hard-exit after the GUI closes.  llama.cpp registers a global Metal
    // device whose C++ destructor crashes during normal process teardown
    // (ggml_metal_device_free → ggml_abort).  std::process::exit() still
    // runs __cxa_finalize / atexit handlers, so it crashes too.
    //
    // _exit() is the POSIX immediate-terminate syscall — no atexit, no
    // C++ destructors, no flush.  This is the only way to avoid the
    // macOS "quit unexpectedly" dialog.
    #[cfg(unix)]
    unsafe {
        libc::_exit(0);
    }
    #[cfg(not(unix))]
    std::process::exit(0);
}

/// Drives all async subsystems: proxy, model, store, and insight timer.
async fn run_backend(
    state: SharedState,
    llm: Arc<Mutex<Option<LlmEngine>>>,
    insight_tx: mpsc::Sender<String>,
    trigger_rx: mpsc::Receiver<()>,
    config: Config,
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
    // Checks every second: if auto_response is on and enough time has
    // elapsed, generates a new insight.  Also polls the ASK button
    // trigger channel for on-demand generation.
    let interval_secs = config.auto_response_interval.max(5); // minimum 5s

    loop {
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Check for manual ASK button trigger
        if trigger_rx.try_recv().is_ok() {
            try_generate_insight(&store, &llm, &state, &insight_tx).await;
            continue;
        }

        // Check for auto-response timer
        let should_auto = {
            let st = state.lock().unwrap();
            if !st.auto_response {
                continue;
            }
            if st.insight_status == InsightStatus::Generating {
                continue;
            }
            match st.last_insight_time {
                Some(last) => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    now.saturating_sub(last) >= interval_secs
                }
                None => true, // No insight yet — generate one
            }
        };

        if should_auto {
            try_generate_insight(&store, &llm, &state, &insight_tx).await;
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
