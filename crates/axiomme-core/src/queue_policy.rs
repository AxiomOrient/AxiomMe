use crate::error::{AxiomError, OmInferenceFailureKind};
use crate::uri::Scope;

pub fn should_retry_event(event_type: &str, attempt: u32) -> bool {
    let max_attempts = match event_type {
        "semantic_scan" => 5,
        "om_reflect_requested" | "om_reflect_buffer_requested" | "om_observe_buffer_requested" => 6,
        _ => 3,
    };
    attempt < max_attempts
}

pub fn should_retry_event_error(event_type: &str, attempt: u32, err: &AxiomError) -> bool {
    if matches!(
        event_type,
        "om_reflect_requested" | "om_reflect_buffer_requested" | "om_observe_buffer_requested"
    ) && let AxiomError::OmInference { kind, .. } = err
    {
        return matches!(kind, OmInferenceFailureKind::Transient)
            && should_retry_event(event_type, attempt);
    }
    should_retry_event(event_type, attempt)
}

pub fn retry_backoff_seconds(event_type: &str, attempt: u32, event_id: i64) -> i64 {
    let capped_exp = attempt.saturating_sub(1).min(6);
    let base = 1_i64 << capped_exp;
    let max = match event_type {
        "semantic_scan" => 60,
        "om_reflect_requested" | "om_reflect_buffer_requested" | "om_observe_buffer_requested" => {
            120
        }
        _ => 30,
    };
    let baseline = base.min(max);
    let jitter_bound = (baseline / 4).max(1);
    let jitter_seed = format!("{event_type}:{attempt}:{event_id}");
    let hash = blake3::hash(jitter_seed.as_bytes());
    let bytes = hash.as_bytes();
    let rand = i64::from(u16::from_be_bytes([bytes[0], bytes[1]]));
    let jitter = rand % (jitter_bound + 1);
    (baseline + jitter).min(max)
}

pub fn default_scope_set() -> Vec<Scope> {
    vec![
        Scope::Resources,
        Scope::User,
        Scope::Agent,
        Scope::Session,
        Scope::Temp,
        Scope::Queue,
    ]
}

pub fn push_drift_sample(sample: &mut Vec<String>, uri: &str, max: usize) {
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

        assert!(should_retry_event("om_reflect_requested", 5));
        assert!(!should_retry_event("om_reflect_requested", 6));
        assert!(should_retry_event("om_reflect_buffer_requested", 5));
        assert!(!should_retry_event("om_reflect_buffer_requested", 6));
        assert!(should_retry_event("om_observe_buffer_requested", 5));
        assert!(!should_retry_event("om_observe_buffer_requested", 6));

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
    fn should_retry_event_error_uses_om_inference_taxonomy() {
        let transient = AxiomError::OmInference {
            inference_source: crate::error::OmInferenceSource::Reflector,
            kind: OmInferenceFailureKind::Transient,
            message: "timeout".to_string(),
        };
        let fatal = AxiomError::OmInference {
            inference_source: crate::error::OmInferenceSource::Reflector,
            kind: OmInferenceFailureKind::Fatal,
            message: "invalid endpoint".to_string(),
        };
        let schema = AxiomError::OmInference {
            inference_source: crate::error::OmInferenceSource::Reflector,
            kind: OmInferenceFailureKind::Schema,
            message: "bad json".to_string(),
        };

        assert!(should_retry_event_error(
            "om_reflect_requested",
            1,
            &transient
        ));
        assert!(!should_retry_event_error("om_reflect_requested", 1, &fatal));
        assert!(!should_retry_event_error(
            "om_reflect_buffer_requested",
            1,
            &schema
        ));
        assert!(should_retry_event_error(
            "om_observe_buffer_requested",
            1,
            &transient
        ));
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
