//! Config file support — reads `netbuddy.toml` from the working directory.
//!
//! If the file doesn't exist, defaults are used and a template is written
//! so users know what's configurable.

use serde::Deserialize;
use std::path::Path;
use tracing::info;

const CONFIG_PATH: &str = "netbuddy.toml";

/// All user-configurable settings.  Missing fields get defaults via serde.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Whether the buddy auto-generates messages on a timer.
    pub auto_response: bool,
    /// Seconds between auto-generated messages (when auto_response is on).
    pub auto_response_interval: u64,
    /// Maximum tokens the LLM generates per response.
    pub max_tokens: usize,
    /// Temperature for LLM sampling (higher = more creative).
    pub temperature: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            auto_response: true,
            auto_response_interval: 30,
            max_tokens: 40,
            temperature: 1.0,
        }
    }
}

impl Config {
    /// Load config from `netbuddy.toml`.  If the file doesn't exist, write
    /// a default template and return defaults.
    pub fn load() -> Self {
        let path = Path::new(CONFIG_PATH);

        if !path.exists() {
            let template = r#"# netbuddy configuration
# Edit this file and relaunch to apply changes.

# Auto-generate messages on a timer (true/false)
auto_response = true

# Seconds between auto-generated messages
auto_response_interval = 30

# Max tokens per LLM response (lower = shorter/faster)
max_tokens = 40

# Sampling temperature (0.0 = deterministic, 1.0+ = creative)
temperature = 1.0
"#;
            if let Err(e) = std::fs::write(path, template) {
                tracing::warn!("couldn't write default config: {e}");
            } else {
                info!("wrote default config to {CONFIG_PATH}");
            }
            return Self::default();
        }

        match std::fs::read_to_string(path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(cfg) => {
                    info!("loaded config from {CONFIG_PATH}");
                    cfg
                }
                Err(e) => {
                    tracing::error!("bad config file: {e} — using defaults");
                    Self::default()
                }
            },
            Err(e) => {
                tracing::error!("couldn't read config: {e} — using defaults");
                Self::default()
            }
        }
    }
}
