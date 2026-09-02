//! Lightweight frecency store: remembers which peers you jump to and how
//! recently, so your most-used chats float to the top of the flat list.
//!
//! Keyed by peer id (as a string). Visit weight decays with a fixed half-life,
//! so a chat you used a lot last year ranks below one you used a few times this
//! week. Persisted as JSON next to the dialog cache.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config;

/// A visit's weight halves every this many days.
const HALF_LIFE_DAYS: f64 = 30.0;

#[derive(Default, Serialize, Deserialize)]
pub struct Store {
    /// peer id (as string) -> usage record.
    visits: HashMap<String, Record>,
}

#[derive(Serialize, Deserialize)]
struct Record {
    count: u32,
    /// Unix seconds of the last jump.
    last: i64,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Store {
    /// Load the store, or an empty one if it's missing or unreadable.
    pub fn load() -> Store {
        std::fs::read_to_string(config::frecency_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Record a jump to `id` and persist immediately (best-effort).
    pub fn bump(&mut self, id: i64) {
        let rec = self
            .visits
            .entry(id.to_string())
            .or_insert(Record { count: 0, last: 0 });
        rec.count = rec.count.saturating_add(1);
        rec.last = now();
        self.save();
    }

    /// Frecency score for `id`: visit count weighted by recency. 0 if unseen.
    pub fn score(&self, id: i64) -> f64 {
        match self.visits.get(&id.to_string()) {
            None => 0.0,
            Some(r) => {
                let age_days = (now() - r.last).max(0) as f64 / 86_400.0;
                r.count as f64 * 0.5f64.powf(age_days / HALF_LIFE_DAYS)
            }
        }
    }

    fn save(&self) {
        let path = config::frecency_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            std::fs::write(path, json).ok();
        }
    }
}
