//! Persistent page store backed by sled (embedded key-value DB).
//!
//! Each intercepted page is stored as a JSON-serialized PageEntry,
//! keyed by timestamp (big-endian u64) so iteration order is
//! chronological.  FIFO compaction kicks in when disk usage exceeds
//! 5 GB, evicting the oldest entries until we're back under 4 GB.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

const MAX_DB_SIZE: u64 = 5_368_709_120; // 5 GB — triggers compaction
const COMPACT_TARGET: u64 = 4_294_967_296; // 4 GB — compaction stops here

/// One captured page from the proxy.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PageEntry {
    pub url: String,
    pub body: String,       // truncated to 2000 chars by proxy
    pub timestamp: u64,     // unix epoch seconds
}

/// Thin wrapper around a sled::Db.
#[derive(Clone)]
pub struct Store {
    db: sled::Db,
}

impl Store {
    /// Open (or create) the database in `./netbuddy_data/`.
    pub fn open() -> Result<Self> {
        let db = sled::open("netbuddy_data")?;
        info!("opened sled db with {} entries", db.len());
        Ok(Self { db })
    }

    /// Persist a page entry.  Flushes immediately, then compacts if needed.
    pub fn save(&self, entry: &PageEntry) -> Result<()> {
        let key = entry.timestamp.to_be_bytes();
        let value = serde_json::to_vec(entry)?;
        self.db.insert(key, value)?;
        self.db.flush()?;
        self.compact_if_needed()?;
        Ok(())
    }

    /// Return the `n` most recent entries, newest first.
    pub fn get_recent(&self, n: usize) -> Vec<PageEntry> {
        self.db
            .iter()
            .rev()
            .take(n)
            .filter_map(|item| {
                let (_, v) = item.ok()?;
                serde_json::from_slice(&v).ok()
            })
            .collect()
    }

    /// Total number of stored pages.
    pub fn page_count(&self) -> usize {
        self.db.len()
    }

    /// Approximate on-disk size in bytes.
    pub fn size_bytes(&self) -> u64 {
        self.db.size_on_disk().unwrap_or(0)
    }

    /// FIFO eviction: delete oldest entries until we're under COMPACT_TARGET.
    /// Called after every save, but only does work when over MAX_DB_SIZE.
    fn compact_if_needed(&self) -> Result<()> {
        let size = self.size_bytes();
        if size <= MAX_DB_SIZE {
            return Ok(());
        }
        info!("db size {size} exceeds limit, compacting...");
        let mut deleted = 0u64;
        for item in self.db.iter() {
            if self.size_bytes() < COMPACT_TARGET {
                break;
            }
            if let Ok((key, _)) = item {
                self.db.remove(key)?;
                deleted += 1;
            }
        }
        warn!("compacted: removed {deleted} oldest entries");
        Ok(())
    }
}
