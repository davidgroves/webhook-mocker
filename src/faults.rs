use std::time::Duration;

use rand::Rng;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(default)]
pub struct FaultProfile {
    /// Force this HTTP status when set.
    pub status: Option<u16>,
    /// Fixed delay applied before responding.
    pub delay_ms: Option<u64>,
    /// Random delay in [0, delay_random_ms].
    pub delay_random_ms: Option<u64>,
    /// When set with force_429, return Slack-style rate limit.
    pub force_429: bool,
    /// Retry-After seconds when force_429 is true.
    pub retry_after_secs: Option<u64>,
    /// Fail this percentage of requests (0-100) with status or 500.
    pub fail_percent: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct FaultDecision {
    pub delay: Duration,
    pub override_status: Option<u16>,
    pub retry_after_secs: Option<u64>,
}

impl FaultProfile {
    pub fn decide(&self) -> FaultDecision {
        let mut delay_ms = self.delay_ms.unwrap_or(0);
        if let Some(max) = self.delay_random_ms
            && max > 0
        {
            delay_ms = delay_ms.saturating_add(rand::rng().random_range(0..=max));
        }

        let mut override_status = self.status;
        let mut retry_after_secs = None;

        if self.force_429 {
            override_status = Some(429);
            retry_after_secs = Some(self.retry_after_secs.unwrap_or(1));
        }

        if let Some(pct) = self.fail_percent
            && pct > 0
            && rand::rng().random_range(0..100) < pct
        {
            override_status = Some(override_status.unwrap_or(500));
        }

        FaultDecision {
            delay: Duration::from_millis(delay_ms),
            override_status,
            retry_after_secs,
        }
    }
}
