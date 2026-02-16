use std::collections::HashSet;

use chrono::{Duration, TimeZone, Utc};

use crate::om::{
    OmObservationChunk, OmOriginType, OmPendingMessage, OmRecord, OmScope,
    ResolvedObservationConfig, ResolvedReflectionConfig,
};

use super::*;

fn chunk(seq: u32, msg_tokens: u32, obs_tokens: u32, ids: &[&str]) -> OmObservationChunk {
    OmObservationChunk {
        id: format!("chunk-{seq}"),
        record_id: "record-1".to_string(),
        seq,
        cycle_id: format!("cycle-{seq}"),
        observations: format!("obs-{seq}"),
        token_count: obs_tokens,
        message_tokens: msg_tokens,
        message_ids: ids.iter().copied().map(str::to_string).collect(),
        last_observed_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
            + Duration::minutes(i64::from(seq)),
        created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
            + Duration::minutes(i64::from(seq)),
    }
}

mod activation;
mod observer;
mod reflection;
mod scope;
