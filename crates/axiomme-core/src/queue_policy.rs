use crate::uri::Scope;

pub(crate) fn should_retry_event(event_type: &str, attempt: u32) -> bool {
    let max_attempts = match event_type {
        "semantic_scan" => 5,
        "qdrant_ensure_collection_failed" => 12,
        "qdrant_upsert_failed" | "qdrant_delete_failed" => 12,
        _ => 3,
    };
    attempt < max_attempts
}

pub(crate) fn retry_backoff_seconds(event_type: &str, attempt: u32, event_id: i64) -> i64 {
    let capped_exp = attempt.saturating_sub(1).min(6);
    let base = 1_i64 << capped_exp;
    let max = match event_type {
        "semantic_scan" => 60,
        "qdrant_ensure_collection_failed" => 300,
        "qdrant_upsert_failed" | "qdrant_delete_failed" => 300,
        _ => 30,
    };
    let baseline = base.min(max);
    let jitter_bound = (baseline / 4).max(1);
    let jitter_seed = format!("{event_type}:{attempt}:{event_id}");
    let hash = blake3::hash(jitter_seed.as_bytes());
    let bytes = hash.as_bytes();
    let rand = u16::from_be_bytes([bytes[0], bytes[1]]) as i64;
    let jitter = rand % (jitter_bound + 1);
    (baseline + jitter).min(max)
}

pub(crate) fn default_scope_set() -> Vec<Scope> {
    vec![
        Scope::Resources,
        Scope::User,
        Scope::Agent,
        Scope::Session,
        Scope::Temp,
        Scope::Queue,
    ]
}

pub(crate) fn push_drift_sample(sample: &mut Vec<String>, uri: &str, max: usize) {
    if sample.len() < max {
        sample.push(uri.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_retry_event_uses_event_specific_caps() {
        assert!(should_retry_event("semantic_scan", 1));
        assert!(should_retry_event("semantic_scan", 4));
        assert!(!should_retry_event("semantic_scan", 5));

        assert!(should_retry_event("qdrant_upsert_failed", 11));
        assert!(!should_retry_event("qdrant_upsert_failed", 12));

        assert!(should_retry_event("unknown", 2));
        assert!(!should_retry_event("unknown", 3));
    }

    #[test]
    fn retry_backoff_seconds_is_deterministic_and_bounded() {
        let a = retry_backoff_seconds("semantic_scan", 3, 101);
        let b = retry_backoff_seconds("semantic_scan", 3, 101);
        assert_eq!(a, b);
        assert!(a >= 4);
        assert!(a <= 60);
    }

    #[test]
    fn default_scope_set_contains_all_expected_scopes() {
        let scopes = default_scope_set();
        assert_eq!(scopes.len(), 6);
        assert!(scopes.contains(&Scope::Resources));
        assert!(scopes.contains(&Scope::User));
        assert!(scopes.contains(&Scope::Agent));
        assert!(scopes.contains(&Scope::Session));
        assert!(scopes.contains(&Scope::Temp));
        assert!(scopes.contains(&Scope::Queue));
    }

    #[test]
    fn push_drift_sample_respects_cap() {
        let mut sample = vec!["a".to_string()];
        push_drift_sample(&mut sample, "b", 2);
        push_drift_sample(&mut sample, "c", 2);
        assert_eq!(sample, vec!["a".to_string(), "b".to_string()]);
    }
}
