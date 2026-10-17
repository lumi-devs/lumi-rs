//! In-process rate limiting helper built on `governor`.

use std::hash::Hash;
use std::num::NonZeroU32;

use governor::clock::DefaultClock;
use governor::state::keyed::DashMapStateStore;
use governor::{Quota, RateLimiter as Governor};

/// Per-key token-bucket limiter; `check` returns `false` when a key is over budget.
#[derive(Debug)]
pub struct RateLimiter<K>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
{
    inner: Governor<K, DashMapStateStore<K>, DefaultClock>,
}

impl<K> RateLimiter<K>
where
    K: Hash + Eq + Clone + Send + Sync + 'static,
{
    /// Allow `per_second` events per key, with bursts up to `burst`.
    #[must_use]
    pub fn new(per_second: u32, burst: u32) -> Self {
        let replenish = NonZeroU32::new(per_second).unwrap_or(NonZeroU32::MIN);
        let burst = NonZeroU32::new(burst).unwrap_or(NonZeroU32::MIN);
        Self {
            inner: Governor::dashmap(Quota::per_second(replenish).allow_burst(burst)),
        }
    }

    /// Returns `true` if `key` may proceed right now.
    pub fn check(&self, key: &K) -> bool {
        self.inner.check_key(key).is_ok()
    }
}
