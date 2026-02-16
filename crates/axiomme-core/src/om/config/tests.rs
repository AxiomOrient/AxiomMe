use serde_json::json;

use super::*;
use crate::om::OmScope;

#[test]
fn defaults_match_expected_thresholds_and_async_defaults() {
    let resolved = resolve_om_config(OmConfigInput::default()).expect("resolve");
    assert_eq!(resolved.observation.message_tokens_base, 30_000);
    assert_eq!(resolved.observation.max_tokens_per_batch, 10_000);
    assert_eq!(resolved.reflection.observation_tokens, 40_000);
    assert_eq!(resolved.observation.buffer_tokens, Some(6_000));
    assert_eq!(resolved.observation.buffer_activation, Some(0.8));
    assert_eq!(resolved.observation.block_after, Some(36_000));
    assert_eq!(resolved.reflection.buffer_activation, Some(0.5));
    assert_eq!(resolved.reflection.block_after, Some(48_000));
    assert!(!resolved.async_buffering_disabled);
}

#[test]
fn share_token_budget_rejects_default_async_buffering() {
    let input = OmConfigInput {
        share_token_budget: true,
        ..OmConfigInput::default()
    };
    let err = resolve_om_config(input).expect_err("must reject");
    assert_eq!(err, OmConfigError::ShareTokenBudgetRequiresAsyncDisabled);
}

#[test]
fn share_token_budget_allows_explicit_buffer_disable() {
    let input = OmConfigInput {
        share_token_budget: true,
        observation: ObservationConfigInput {
            buffer_tokens: Some(BufferTokensInput::Disabled),
            ..ObservationConfigInput::default()
        },
        ..OmConfigInput::default()
    };
    let resolved = resolve_om_config(input).expect("resolve");
    assert!(resolved.async_buffering_disabled);
    assert_eq!(resolved.observation.total_budget, Some(70_000));
    assert_eq!(resolved.observation.buffer_tokens, None);
    assert_eq!(resolved.observation.buffer_activation, None);
    assert_eq!(resolved.observation.dynamic_threshold(10_000), 60_000);
}

#[test]
fn resource_scope_disables_async_by_default_when_not_explicit() {
    let input = OmConfigInput {
        scope: OmScope::Resource,
        ..OmConfigInput::default()
    };
    let resolved = resolve_om_config(input).expect("resolve");
    assert!(resolved.async_buffering_disabled);
    assert_eq!(resolved.observation.buffer_tokens, None);
    assert_eq!(resolved.observation.buffer_activation, None);
    assert_eq!(resolved.reflection.buffer_activation, None);
    assert_eq!(resolved.reflection.block_after, None);
}

#[test]
fn resource_scope_rejects_explicit_async_buffering() {
    let input = OmConfigInput {
        scope: OmScope::Resource,
        observation: ObservationConfigInput {
            buffer_tokens: Some(BufferTokensInput::Ratio(0.2)),
            ..ObservationConfigInput::default()
        },
        ..OmConfigInput::default()
    };
    let err = resolve_om_config(input).expect_err("must reject");
    assert_eq!(err, OmConfigError::ResourceScopeAsyncBufferingUnsupported);
}

#[test]
fn resource_scope_allows_explicit_buffer_disable() {
    let input = OmConfigInput {
        scope: OmScope::Resource,
        observation: ObservationConfigInput {
            buffer_tokens: Some(BufferTokensInput::Disabled),
            buffer_activation: Some(0.8),
            ..ObservationConfigInput::default()
        },
        reflection: ReflectionConfigInput {
            buffer_activation: Some(0.5),
            ..ReflectionConfigInput::default()
        },
        ..OmConfigInput::default()
    };
    let resolved = resolve_om_config(input).expect("resolve");
    assert!(resolved.async_buffering_disabled);
    assert_eq!(resolved.observation.buffer_tokens, None);
    assert_eq!(resolved.observation.buffer_activation, None);
    assert_eq!(resolved.reflection.buffer_activation, None);
}

#[test]
fn invalid_buffer_activation_is_rejected() {
    let input = OmConfigInput {
        observation: ObservationConfigInput {
            buffer_activation: Some(0.0),
            ..ObservationConfigInput::default()
        },
        ..OmConfigInput::default()
    };
    let err = resolve_om_config(input).expect_err("must reject");
    assert_eq!(err, OmConfigError::InvalidObservationBufferActivation);
}

#[test]
fn observation_buffer_tokens_must_be_below_threshold() {
    let input = OmConfigInput {
        observation: ObservationConfigInput {
            message_tokens: Some(10_000),
            buffer_tokens: Some(BufferTokensInput::Absolute(10_000)),
            ..ObservationConfigInput::default()
        },
        ..OmConfigInput::default()
    };
    let err = resolve_om_config(input).expect_err("must reject");
    assert_eq!(
        err,
        OmConfigError::ObservationBufferTokensAtOrAboveThreshold
    );
}

#[test]
fn invalid_max_tokens_per_batch_is_rejected() {
    let input = OmConfigInput {
        observation: ObservationConfigInput {
            max_tokens_per_batch: Some(0),
            ..ObservationConfigInput::default()
        },
        ..OmConfigInput::default()
    };
    let err = resolve_om_config(input).expect_err("must reject");
    assert_eq!(err, OmConfigError::InvalidObservationMaxTokensPerBatch);
}

#[test]
fn non_finite_block_after_is_rejected() {
    let input = OmConfigInput {
        observation: ObservationConfigInput {
            block_after: Some(f32::INFINITY),
            ..ObservationConfigInput::default()
        },
        ..OmConfigInput::default()
    };
    let err = resolve_om_config(input).expect_err("must reject");
    assert_eq!(err, OmConfigError::InvalidObservationBlockAfter);
}

#[test]
fn block_after_multiplier_one_resolves_to_threshold() {
    let input = OmConfigInput {
        observation: ObservationConfigInput {
            message_tokens: Some(12_000),
            block_after: Some(1.0),
            ..ObservationConfigInput::default()
        },
        reflection: ReflectionConfigInput {
            observation_tokens: Some(20_000),
            block_after: Some(1.0),
            ..ReflectionConfigInput::default()
        },
        ..OmConfigInput::default()
    };
    let resolved = resolve_om_config(input).expect("resolve");
    assert_eq!(resolved.observation.block_after, Some(12_000));
    assert_eq!(resolved.reflection.block_after, Some(20_000));
}

#[test]
fn buffer_tokens_input_serialization_roundtrip_is_explicit_and_stable() {
    let cases = [
        BufferTokensInput::Disabled,
        BufferTokensInput::Absolute(42),
        BufferTokensInput::Ratio(0.25),
    ];
    for case in cases {
        let encoded = serde_json::to_value(case).expect("serialize");
        let decoded =
            serde_json::from_value::<BufferTokensInput>(encoded.clone()).expect("deserialize");
        assert_eq!(decoded, case);
    }

    assert_eq!(
        serde_json::to_value(BufferTokensInput::Disabled).expect("serialize"),
        json!({ "type": "disabled" })
    );
    assert_eq!(
        serde_json::to_value(BufferTokensInput::Absolute(10)).expect("serialize"),
        json!({ "type": "absolute", "value": 10 })
    );
    assert_eq!(
        serde_json::to_value(BufferTokensInput::Ratio(0.2)).expect("serialize"),
        json!({ "type": "ratio", "value": 0.2 })
    );
}

#[test]
fn buffer_tokens_input_deserialization_rejects_invalid_shapes() {
    let missing_value = serde_json::from_value::<BufferTokensInput>(json!({
        "type": "absolute"
    }));
    assert!(missing_value.is_err());

    let unknown_type = serde_json::from_value::<BufferTokensInput>(json!({
        "type": "unknown",
        "value": 1
    }));
    assert!(unknown_type.is_err());
}

#[test]
fn ratio_buffer_tokens_json_value_resolves_expected_interval_tokens() {
    let ratio = serde_json::from_value::<BufferTokensInput>(json!({
        "type": "ratio",
        "value": 0.2
    }))
    .expect("deserialize ratio");
    let input = OmConfigInput {
        observation: ObservationConfigInput {
            buffer_tokens: Some(ratio),
            ..ObservationConfigInput::default()
        },
        ..OmConfigInput::default()
    };
    let resolved = resolve_om_config(input).expect("resolve");
    assert_eq!(resolved.observation.buffer_tokens, Some(6_000));
}
