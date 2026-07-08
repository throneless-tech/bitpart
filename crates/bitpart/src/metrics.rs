// Bitpart
// Copyright (C) 2025 Throneless Tech

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.

// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    /// Not registered/linked to the Signal server yet.
    Unlinked,
    /// Linked and actively receiving.
    Linked,
    /// Linked previously but the connection is currently failing.
    Failing,
}

impl ConnectionStatus {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Linked,
            2 => Self::Failing,
            _ => Self::Unlinked,
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            Self::Unlinked => 0,
            Self::Linked => 1,
            Self::Failing => 2,
        }
    }
}

#[derive(Debug)]
pub struct BotMetrics {
    sent: AtomicU64,
    received: AtomicU64,
    status: AtomicU8,
}

impl Default for BotMetrics {
    fn default() -> Self {
        Self {
            sent: AtomicU64::new(0),
            received: AtomicU64::new(0),
            status: AtomicU8::new(ConnectionStatus::Unlinked.as_u8()),
        }
    }
}

impl BotMetrics {
    pub fn incr_sent(&self) {
        self.sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn incr_received(&self) {
        self.received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_status(&self, status: ConnectionStatus) {
        self.status.store(status.as_u8(), Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> BotMetricsSnapshot {
        BotMetricsSnapshot {
            sent: self.sent.load(Ordering::Relaxed),
            received: self.received.load(Ordering::Relaxed),
            status: ConnectionStatus::from_u8(self.status.load(Ordering::Relaxed)),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BotMetricsSnapshot {
    pub sent: u64,
    pub received: u64,
    pub status: ConnectionStatus,
}

#[derive(Debug, Clone, Default)]
pub struct MetricsRegistry {
    bots: Arc<RwLock<HashMap<String, Arc<BotMetrics>>>>,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_or_create(&self, bot_id: &str) -> Arc<BotMetrics> {
        if let Some(metrics) = self.bots.read().unwrap().get(bot_id) {
            return metrics.clone();
        }
        let mut bots = self.bots.write().unwrap();
        bots.entry(bot_id.to_owned()).or_default().clone()
    }

    pub fn snapshot(&self, bot_id: &str) -> Option<BotMetricsSnapshot> {
        self.bots.read().unwrap().get(bot_id).map(|m| m.snapshot())
    }

    pub fn snapshot_all(&self) -> Vec<(String, BotMetricsSnapshot)> {
        self.bots
            .read()
            .unwrap()
            .iter()
            .map(|(id, m)| (id.clone(), m.snapshot()))
            .collect()
    }
}
