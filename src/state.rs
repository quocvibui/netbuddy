//! Shared application state — the bridge between the tokio backend and
//! the nannou GUI.  Protected by `Arc<Mutex<_>>`.
//!
//! Tracks proxy status, model status, page counts, and reactive metrics
//! (request rate, burst intensity, domain diversity) that drive the
//! creature's visual behavior.

use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ModelStatus {
    Loading,
    Ready,
    Error(String), // kept for debug logging even when not displayed in UI
}

#[derive(Debug, Clone, PartialEq)]
pub enum InsightStatus {
    Idle,
    Generating,
    Done,
}

/// Central mutable state shared across all subsystems.
pub struct AppState {
    // ── Status indicators ────────────────────────────────────────────
    pub page_count: usize,
    pub db_size_bytes: u64,
    pub latest_insight: Option<String>,
    pub proxy_active: bool,
    pub proxy_port: u16,
    pub model_status: ModelStatus,
    pub insight_status: InsightStatus,
    pub last_insight_time: Option<u64>,

    // ── Reactive metrics (feed the creature animation) ───────────────
    pub total_requests: u64,
    pub total_bytes: u64,
    pub recent_request_timestamps: Vec<u64>, // unix ms, rolling 30 s window
    pub recent_domains: Vec<String>,         // last ~20 unique domains
}

impl AppState {
    /// Record one completed request.  Updates counters and the
    /// rolling 30-second timestamp window used for rate calculation.
    pub fn record_request(&mut self, body_bytes: usize) {
        self.total_requests += 1;
        self.total_bytes += body_bytes as u64;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.recent_request_timestamps.push(now_ms);
        let cutoff = now_ms.saturating_sub(30_000);
        self.recent_request_timestamps.retain(|&t| t > cutoff);
    }

    /// Track a visited domain.  Deduplicates consecutive repeats and
    /// keeps the 20 most recent entries.
    pub fn record_domain(&mut self, domain: String) {
        if self.recent_domains.last().map(|d| d.as_str()) != Some(&domain) {
            self.recent_domains.push(domain);
        }
        if self.recent_domains.len() > 20 {
            self.recent_domains.drain(0..self.recent_domains.len() - 20);
        }
    }

    /// Average requests per second over the last 30 s.
    pub fn requests_per_sec(&self) -> f32 {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let cutoff = now_ms.saturating_sub(30_000);
        let recent = self.recent_request_timestamps.iter()
            .filter(|&&t| t > cutoff).count();
        recent as f32 / 30.0
    }

    /// Content diversity: unique domains in the recent window (0..1).
    /// 15 unique domains = 1.0 (saturation point).
    pub fn domain_diversity(&self) -> f32 {
        let mut unique: Vec<&str> = self.recent_domains.iter().map(|s| s.as_str()).collect();
        unique.sort();
        unique.dedup();
        (unique.len() as f32 / 15.0).min(1.0)
    }

    /// Burst detector: ratio of last-5-second rate to last-30-second
    /// average.  Returns 0..1 where 1.0 means the current rate is >=3x
    /// the recent average (i.e. a sudden spike).
    pub fn burst_intensity(&self) -> f32 {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let cutoff_5 = now_ms.saturating_sub(5_000);
        let cutoff_30 = now_ms.saturating_sub(30_000);
        let recent_5 = self.recent_request_timestamps.iter().filter(|&&t| t > cutoff_5).count() as f32;
        let recent_30 = self.recent_request_timestamps.iter().filter(|&&t| t > cutoff_30).count() as f32;
        if recent_30 < 1.0 { return 0.0; }
        let ratio = (recent_5 / 5.0) / (recent_30 / 30.0);
        (ratio / 3.0).min(1.0)
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            page_count: 0,
            db_size_bytes: 0,
            latest_insight: None,
            proxy_active: false,
            proxy_port: 8080,
            model_status: ModelStatus::Loading,
            insight_status: InsightStatus::Idle,
            last_insight_time: None,
            total_requests: 0,
            total_bytes: 0,
            recent_request_timestamps: Vec::new(),
            recent_domains: Vec::new(),
        }
    }
}

pub type SharedState = Arc<Mutex<AppState>>;
