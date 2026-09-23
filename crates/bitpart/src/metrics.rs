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

type MetricsMap = HashMap<String, (String, Arc<ChannelMetrics>)>;

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
pub struct ChannelMetrics {
    sent: AtomicU64,
    received: AtomicU64,
    status: AtomicU8,
}

impl Default for ChannelMetrics {
    fn default() -> Self {
        Self {
            sent: AtomicU64::new(0),
            received: AtomicU64::new(0),
            status: AtomicU8::new(ConnectionStatus::Unlinked.as_u8()),
        }
    }
}

impl ChannelMetrics {
    pub fn incr_sent(&self) {
        self.sent.fetch_add(1, Ordering::Relaxed);
    }

    pub fn incr_received(&self) {
        self.received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_status(&self, status: ConnectionStatus) {
        self.status.store(status.as_u8(), Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> ChannelMetricsSnapshot {
        ChannelMetricsSnapshot {
            sent: self.sent.load(Ordering::Relaxed),
            received: self.received.load(Ordering::Relaxed),
            status: ConnectionStatus::from_u8(self.status.load(Ordering::Relaxed)),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChannelMetricsSnapshot {
    pub sent: u64,
    pub received: u64,
    pub status: ConnectionStatus,
}

#[derive(Debug, Clone, Default)]
pub struct MetricsRegistry {
    channels: Arc<RwLock<MetricsMap>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChannelStatus {
    pub channel_id: String,
    #[serde(flatten)]
    pub metrics: ChannelMetricsSnapshot,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_or_create(&self, channel: &str, bot_id: &str) -> Arc<ChannelMetrics> {
        if let Some((_, metrics)) = self.channels.read().unwrap().get(channel) {
            return metrics.clone();
        }
        let mut channels = self.channels.write().unwrap();
        channels
            .entry(channel.to_owned())
            .or_insert_with(|| (bot_id.to_owned(), Arc::default()))
            .1
            .clone()
    }

    pub fn remove(&self, channel: &str) {
        self.channels.write().unwrap().remove(channel);
    }

    pub fn for_channels(&self, channels: &[(String, String)]) -> Vec<ChannelStatus> {
        let registry = self.channels.read().unwrap();
        channels
            .iter()
            .map(|(row_id, channel_id)| ChannelStatus {
                channel_id: channel_id.clone(),
                metrics: registry
                    .get(row_id)
                    .map(|(_, m)| m.snapshot())
                    .unwrap_or_else(|| ChannelMetrics::default().snapshot()),
            })
            .collect()
    }

    pub fn snapshot_all(&self) -> Vec<(String, String, ChannelMetricsSnapshot)> {
        self.channels
            .read()
            .unwrap()
            .iter()
            .map(|(channel, (bot_id, m))| (bot_id.clone(), channel.clone(), m.snapshot()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_of_one_bot_are_tracked_separately() {
        let registry = MetricsRegistry::new();
        let a = registry.get_or_create("row-a", "bot");
        let b = registry.get_or_create("row-b", "bot");
        a.incr_sent();
        a.set_status(ConnectionStatus::Linked);
        b.set_status(ConnectionStatus::Failing);

        let status = registry.for_channels(&[
            ("row-a".to_owned(), "signal".to_owned()),
            ("row-b".to_owned(), "backup".to_owned()),
            ("row-c".to_owned(), "never-started".to_owned()),
        ]);
        assert_eq!(status[0].channel_id, "signal");
        assert_eq!(status[0].metrics.sent, 1);
        assert_eq!(status[0].metrics.status, ConnectionStatus::Linked);
        assert_eq!(status[1].metrics.sent, 0);
        assert_eq!(status[1].metrics.status, ConnectionStatus::Failing);
        assert_eq!(status[2].metrics.status, ConnectionStatus::Unlinked);
    }

    #[test]
    fn same_channel_returns_same_metrics() {
        let registry = MetricsRegistry::new();
        registry.get_or_create("row-a", "bot").incr_received();
        registry.get_or_create("row-a", "bot").incr_received();
        let all = registry.snapshot_all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].0, "bot");
        assert_eq!(all[0].1, "row-a");
        assert_eq!(all[0].2.received, 2);
    }

    #[test]
    fn removed_channel_is_no_longer_reported() {
        let registry = MetricsRegistry::new();
        registry.get_or_create("row-a", "bot");
        registry.remove("row-a");
        assert!(registry.snapshot_all().is_empty());
    }
}
