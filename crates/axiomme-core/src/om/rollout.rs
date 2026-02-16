#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OmRolloutProfile {
    Baseline,
    ObserverOnly,
    FullModel,
}

fn parse_om_rollout_profile(raw: Option<&str>) -> Option<OmRolloutProfile> {
    let token = raw?.trim();
    if token.is_empty() {
        return None;
    }
    match token.to_ascii_lowercase().as_str() {
        "baseline" | "off" | "deterministic" => Some(OmRolloutProfile::Baseline),
        "observer_only" | "observer-only" => Some(OmRolloutProfile::ObserverOnly),
        "full_model" | "full-model" | "full" | "model" => Some(OmRolloutProfile::FullModel),
        _ => None,
    }
}

pub fn resolve_observer_model_enabled(explicit_flag: bool, rollout_profile: Option<&str>) -> bool {
    match parse_om_rollout_profile(rollout_profile) {
        Some(OmRolloutProfile::Baseline) => false,
        Some(OmRolloutProfile::ObserverOnly | OmRolloutProfile::FullModel) => true,
        None => explicit_flag,
    }
}

pub fn resolve_reflector_model_enabled(explicit_flag: bool, rollout_profile: Option<&str>) -> bool {
    match parse_om_rollout_profile(rollout_profile) {
        Some(OmRolloutProfile::Baseline | OmRolloutProfile::ObserverOnly) => false,
        Some(OmRolloutProfile::FullModel) => true,
        None => explicit_flag,
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_observer_model_enabled, resolve_reflector_model_enabled};

    #[test]
    fn observer_resolution_handles_profiles_and_fallback() {
        assert!(resolve_observer_model_enabled(false, Some("observer_only")));
        assert!(resolve_observer_model_enabled(false, Some("full_model")));
        assert!(!resolve_observer_model_enabled(true, Some("baseline")));
        assert!(resolve_observer_model_enabled(true, Some("unknown")));
        assert!(!resolve_observer_model_enabled(false, Some("unknown")));
    }

    #[test]
    fn reflector_resolution_handles_profiles_and_fallback() {
        assert!(!resolve_reflector_model_enabled(
            true,
            Some("observer_only")
        ));
        assert!(resolve_reflector_model_enabled(false, Some("full_model")));
        assert!(!resolve_reflector_model_enabled(true, Some("baseline")));
        assert!(resolve_reflector_model_enabled(true, Some("unknown")));
        assert!(!resolve_reflector_model_enabled(false, Some("unknown")));
    }
}
