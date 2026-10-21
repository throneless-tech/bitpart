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

use std::num::NonZeroU32;
use std::time::Duration;

use governor::{DefaultKeyedRateLimiter, Quota};
use uuid::Uuid;

fn quota(burst: u32, replenish_one_per: Duration) -> Quota {
    Quota::with_period(replenish_one_per)
        .expect("non-zero replenish period")
        .allow_burst(NonZeroU32::new(burst).expect("non-zero burst"))
}

#[derive(Debug, Clone, Copy)]
pub struct RateLimits {
    pub send_per_recipient: Quota,
    pub purge_per_user: Quota,
}

// Signal-Server RateLimiters.java defaults, each set a little under Signal's:
//   MESSAGES   bucket 60, 1 per 1s,    keyed by sender account + recipient
//   PRE_KEYS   bucket 6,  1 per 10min, keyed by sender device + recipient device
impl Default for RateLimits {
    fn default() -> Self {
        Self {
            send_per_recipient: quota(55, Duration::from_millis(1100)),
            purge_per_user: quota(4, Duration::from_secs(15 * 60)),
        }
    }
}

pub struct ChannelLimiters {
    pub send: DefaultKeyedRateLimiter<Uuid>,
    pub purge: DefaultKeyedRateLimiter<String>,
}

impl std::fmt::Debug for ChannelLimiters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelLimiters").finish_non_exhaustive()
    }
}

impl ChannelLimiters {
    pub fn new(limits: &RateLimits) -> Self {
        Self {
            send: DefaultKeyedRateLimiter::keyed(limits.send_per_recipient),
            purge: DefaultKeyedRateLimiter::keyed(limits.purge_per_user),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_stay_under_signal_limits() {
        let limits = RateLimits::default();
        assert!(limits.send_per_recipient.burst_size().get() < 60);
        assert!(limits.send_per_recipient.replenish_interval() > Duration::from_secs(1));
        assert!(limits.purge_per_user.burst_size().get() < 6);
        assert!(limits.purge_per_user.replenish_interval() > Duration::from_secs(10 * 60));
    }

    #[test]
    fn purge_limit_allows_a_small_burst_per_user() {
        let limiters = ChannelLimiters::new(&RateLimits::default());
        let user = "11111111-1111-1111-1111-111111111111".to_owned();
        let other = "22222222-2222-2222-2222-222222222222".to_owned();
        for _ in 0..4 {
            assert!(limiters.purge.check_key(&user).is_ok());
        }
        assert!(limiters.purge.check_key(&user).is_err());
        assert!(limiters.purge.check_key(&other).is_ok());
    }

    #[test]
    fn send_limit_is_per_recipient() {
        let limiters = ChannelLimiters::new(&RateLimits {
            send_per_recipient: quota(2, Duration::from_secs(3600)),
            ..RateLimits::default()
        });
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        assert!(limiters.send.check_key(&a).is_ok());
        assert!(limiters.send.check_key(&a).is_ok());
        assert!(limiters.send.check_key(&a).is_err());
        assert!(limiters.send.check_key(&b).is_ok());
    }
}
