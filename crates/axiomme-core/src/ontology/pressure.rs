use serde::{Deserialize, Serialize};

use super::model::OntologySchemaV1;
use crate::error::{AxiomError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OntologyV2PressurePolicy {
    pub min_action_types: usize,
    pub min_invariants: usize,
    pub min_action_invariant_total: usize,
    pub min_link_types_per_object_basis_points: u32,
}

impl Default for OntologyV2PressurePolicy {
    fn default() -> Self {
        Self {
            min_action_types: 3,
            min_invariants: 3,
            min_action_invariant_total: 5,
            min_link_types_per_object_basis_points: 15_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OntologyV2PressureReport {
    pub schema_version: u32,
    pub object_type_count: usize,
    pub link_type_count: usize,
    pub action_type_count: usize,
    pub invariant_count: usize,
    pub action_invariant_total: usize,
    pub link_types_per_object_basis_points: u32,
    pub v2_candidate: bool,
    pub trigger_reasons: Vec<String>,
    pub policy: OntologyV2PressurePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OntologyV2PressureTrendPolicy {
    pub min_samples: usize,
    pub consecutive_v2_candidate: usize,
}

impl Default for OntologyV2PressureTrendPolicy {
    fn default() -> Self {
        Self {
            min_samples: 3,
            consecutive_v2_candidate: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OntologyV2PressureTrendStatus {
    InsufficientSamples,
    Monitor,
    TriggerV2Design,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OntologyV2PressureSample {
    pub sample_id: String,
    pub generated_at_utc: String,
    pub v2_candidate: bool,
    #[serde(default)]
    pub trigger_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OntologyV2PressureTrendReport {
    pub total_samples: usize,
    pub consecutive_v2_candidate_tail: usize,
    pub trigger_v2_design: bool,
    pub status: OntologyV2PressureTrendStatus,
    pub policy: OntologyV2PressureTrendPolicy,
    pub latest_sample_id: Option<String>,
    pub latest_generated_at_utc: Option<String>,
    pub latest_v2_candidate: Option<bool>,
    pub samples: Vec<OntologyV2PressureSample>,
}

pub fn validate_v2_pressure_trend_policy(
    policy: OntologyV2PressureTrendPolicy,
) -> Result<OntologyV2PressureTrendPolicy> {
    if policy.min_samples == 0 {
        return Err(AxiomError::OntologyViolation(
            "ontology trend policy min_samples must be >= 1".to_string(),
        ));
    }
    if policy.consecutive_v2_candidate == 0 {
        return Err(AxiomError::OntologyViolation(
            "ontology trend policy consecutive_v2_candidate must be >= 1".to_string(),
        ));
    }
    Ok(policy)
}

#[must_use]
pub fn evaluate_v2_pressure(
    schema: &OntologySchemaV1,
    policy: OntologyV2PressurePolicy,
) -> OntologyV2PressureReport {
    let object_type_count = schema.object_types.len();
    let link_type_count = schema.link_types.len();
    let action_type_count = schema.action_types.len();
    let invariant_count = schema.invariants.len();
    let action_invariant_total = action_type_count.saturating_add(invariant_count);
    let link_types_per_object_basis_points = if object_type_count == 0 {
        0
    } else {
        ((link_type_count as u128 * 10_000_u128) / object_type_count as u128) as u32
    };

    let mut trigger_reasons = Vec::<String>::new();
    if action_type_count >= policy.min_action_types {
        trigger_reasons.push(format!(
            "action_type_count({action_type_count}) >= min_action_types({})",
            policy.min_action_types
        ));
    }
    if invariant_count >= policy.min_invariants {
        trigger_reasons.push(format!(
            "invariant_count({invariant_count}) >= min_invariants({})",
            policy.min_invariants
        ));
    }
    if action_invariant_total >= policy.min_action_invariant_total {
        trigger_reasons.push(format!(
            "action_invariant_total({action_invariant_total}) >= min_action_invariant_total({})",
            policy.min_action_invariant_total
        ));
    }
    if link_types_per_object_basis_points >= policy.min_link_types_per_object_basis_points {
        trigger_reasons.push(format!(
            "link_types_per_object_basis_points({link_types_per_object_basis_points}) >= min_link_types_per_object_basis_points({})",
            policy.min_link_types_per_object_basis_points
        ));
    }

    OntologyV2PressureReport {
        schema_version: schema.version,
        object_type_count,
        link_type_count,
        action_type_count,
        invariant_count,
        action_invariant_total,
        link_types_per_object_basis_points,
        v2_candidate: !trigger_reasons.is_empty(),
        trigger_reasons,
        policy,
    }
}

#[must_use]
pub fn evaluate_v2_pressure_trend(
    mut samples: Vec<OntologyV2PressureSample>,
    policy: OntologyV2PressureTrendPolicy,
) -> OntologyV2PressureTrendReport {
    samples.sort_by(|a, b| {
        a.generated_at_utc
            .cmp(&b.generated_at_utc)
            .then_with(|| a.sample_id.cmp(&b.sample_id))
    });

    let total_samples = samples.len();
    let mut consecutive_v2_candidate_tail = 0_usize;
    for sample in samples.iter().rev() {
        if sample.v2_candidate {
            consecutive_v2_candidate_tail = consecutive_v2_candidate_tail.saturating_add(1);
        } else {
            break;
        }
    }

    let trigger_v2_design = total_samples >= policy.min_samples
        && consecutive_v2_candidate_tail >= policy.consecutive_v2_candidate;
    let status = if total_samples < policy.min_samples {
        OntologyV2PressureTrendStatus::InsufficientSamples
    } else if trigger_v2_design {
        OntologyV2PressureTrendStatus::TriggerV2Design
    } else {
        OntologyV2PressureTrendStatus::Monitor
    };

    let latest = samples.last();
    OntologyV2PressureTrendReport {
        total_samples,
        consecutive_v2_candidate_tail,
        trigger_v2_design,
        status,
        policy,
        latest_sample_id: latest.map(|x| x.sample_id.clone()),
        latest_generated_at_utc: latest.map(|x| x.generated_at_utc.clone()),
        latest_v2_candidate: latest.map(|x| x.v2_candidate),
        samples,
    }
}

#[cfg(test)]
mod tests {
    use crate::ontology::parse_schema_v1;

    use super::*;

    fn schema_with_counts(
        object_type_count: usize,
        link_type_count: usize,
        action_type_count: usize,
        invariant_count: usize,
    ) -> OntologySchemaV1 {
        let mut object_types = Vec::new();
        for index in 0..object_type_count {
            object_types.push(format!(
                r#"{{
                    "id": "obj_{index}",
                    "uri_prefixes": ["axiom://resources/obj_{index}"],
                    "required_tags": [],
                    "allowed_scopes": ["resources"]
                }}"#
            ));
        }

        let mut link_types = Vec::new();
        for index in 0..link_type_count {
            let from = if object_type_count == 0 {
                "obj_0".to_string()
            } else {
                format!("obj_{}", index % object_type_count)
            };
            let to = if object_type_count == 0 {
                "obj_0".to_string()
            } else {
                format!("obj_{}", (index + 1) % object_type_count)
            };
            link_types.push(format!(
                r#"{{
                    "id": "link_{index}",
                    "from_types": ["{from}"],
                    "to_types": ["{to}"],
                    "min_arity": 2,
                    "max_arity": 8,
                    "symmetric": false
                }}"#
            ));
        }

        let mut action_types = Vec::new();
        for index in 0..action_type_count {
            action_types.push(format!(
                r#"{{
                    "id": "action_{index}",
                    "input_contract": "json-schema",
                    "effects": ["enqueue"],
                    "queue_event_type": "ontology_action_{index}"
                }}"#
            ));
        }

        let mut invariants = Vec::new();
        for index in 0..invariant_count {
            invariants.push(format!(
                r#"{{
                    "id": "invariant_{index}",
                    "rule": "rule_{index}",
                    "severity": "warn",
                    "message": "message_{index}"
                }}"#
            ));
        }

        let raw = format!(
            r#"{{
                "version": 1,
                "object_types": [{}],
                "link_types": [{}],
                "action_types": [{}],
                "invariants": [{}]
            }}"#,
            object_types.join(","),
            link_types.join(","),
            action_types.join(","),
            invariants.join(","),
        );
        parse_schema_v1(&raw).expect("schema parse")
    }

    #[test]
    fn evaluate_v2_pressure_triggers_when_action_threshold_is_crossed() {
        let schema = schema_with_counts(2, 2, 3, 0);
        let report = evaluate_v2_pressure(&schema, OntologyV2PressurePolicy::default());
        assert!(report.v2_candidate);
        assert!(
            report
                .trigger_reasons
                .iter()
                .any(|reason| reason.contains("action_type_count"))
        );
    }

    #[test]
    fn evaluate_v2_pressure_triggers_when_combined_threshold_is_crossed() {
        let schema = schema_with_counts(2, 1, 2, 3);
        let policy = OntologyV2PressurePolicy {
            min_action_types: 10,
            min_invariants: 10,
            min_action_invariant_total: 5,
            min_link_types_per_object_basis_points: u32::MAX,
        };
        let report = evaluate_v2_pressure(&schema, policy);
        assert!(report.v2_candidate);
        assert!(
            report
                .trigger_reasons
                .iter()
                .any(|reason| reason.contains("action_invariant_total"))
        );
    }

    #[test]
    fn evaluate_v2_pressure_can_stay_false_when_under_all_thresholds() {
        let schema = schema_with_counts(4, 2, 1, 1);
        let policy = OntologyV2PressurePolicy {
            min_action_types: 3,
            min_invariants: 3,
            min_action_invariant_total: 6,
            min_link_types_per_object_basis_points: 10_000,
        };
        let report = evaluate_v2_pressure(&schema, policy);
        assert!(!report.v2_candidate);
        assert!(report.trigger_reasons.is_empty());
    }

    #[test]
    fn evaluate_v2_pressure_trend_reports_insufficient_samples_before_threshold() {
        let report = evaluate_v2_pressure_trend(
            vec![OntologyV2PressureSample {
                sample_id: "s1".to_string(),
                generated_at_utc: "2026-02-23T00:00:00Z".to_string(),
                v2_candidate: true,
                trigger_reasons: vec!["action".to_string()],
            }],
            OntologyV2PressureTrendPolicy {
                min_samples: 2,
                consecutive_v2_candidate: 2,
            },
        );
        assert_eq!(
            report.status,
            OntologyV2PressureTrendStatus::InsufficientSamples
        );
        assert!(!report.trigger_v2_design);
    }

    #[test]
    fn evaluate_v2_pressure_trend_triggers_on_consecutive_tail() {
        let report = evaluate_v2_pressure_trend(
            vec![
                OntologyV2PressureSample {
                    sample_id: "s1".to_string(),
                    generated_at_utc: "2026-02-21T00:00:00Z".to_string(),
                    v2_candidate: false,
                    trigger_reasons: Vec::new(),
                },
                OntologyV2PressureSample {
                    sample_id: "s2".to_string(),
                    generated_at_utc: "2026-02-22T00:00:00Z".to_string(),
                    v2_candidate: true,
                    trigger_reasons: vec!["a".to_string()],
                },
                OntologyV2PressureSample {
                    sample_id: "s3".to_string(),
                    generated_at_utc: "2026-02-23T00:00:00Z".to_string(),
                    v2_candidate: true,
                    trigger_reasons: vec!["b".to_string()],
                },
                OntologyV2PressureSample {
                    sample_id: "s4".to_string(),
                    generated_at_utc: "2026-02-24T00:00:00Z".to_string(),
                    v2_candidate: true,
                    trigger_reasons: vec!["c".to_string()],
                },
            ],
            OntologyV2PressureTrendPolicy {
                min_samples: 3,
                consecutive_v2_candidate: 3,
            },
        );
        assert_eq!(
            report.status,
            OntologyV2PressureTrendStatus::TriggerV2Design
        );
        assert!(report.trigger_v2_design);
        assert_eq!(report.consecutive_v2_candidate_tail, 3);
    }

    #[test]
    fn evaluate_v2_pressure_trend_monitors_when_tail_is_not_consecutive_enough() {
        let report = evaluate_v2_pressure_trend(
            vec![
                OntologyV2PressureSample {
                    sample_id: "s1".to_string(),
                    generated_at_utc: "2026-02-21T00:00:00Z".to_string(),
                    v2_candidate: true,
                    trigger_reasons: vec!["a".to_string()],
                },
                OntologyV2PressureSample {
                    sample_id: "s2".to_string(),
                    generated_at_utc: "2026-02-22T00:00:00Z".to_string(),
                    v2_candidate: true,
                    trigger_reasons: vec!["b".to_string()],
                },
                OntologyV2PressureSample {
                    sample_id: "s3".to_string(),
                    generated_at_utc: "2026-02-23T00:00:00Z".to_string(),
                    v2_candidate: false,
                    trigger_reasons: Vec::new(),
                },
            ],
            OntologyV2PressureTrendPolicy {
                min_samples: 3,
                consecutive_v2_candidate: 2,
            },
        );
        assert_eq!(report.status, OntologyV2PressureTrendStatus::Monitor);
        assert!(!report.trigger_v2_design);
        assert_eq!(report.consecutive_v2_candidate_tail, 0);
    }

    #[test]
    fn validate_v2_pressure_trend_policy_rejects_zero_min_samples() {
        let policy = OntologyV2PressureTrendPolicy {
            min_samples: 0,
            consecutive_v2_candidate: 3,
        };
        let error = validate_v2_pressure_trend_policy(policy).expect_err("must fail");
        assert_eq!(error.code(), "ONTOLOGY_VIOLATION");
    }

    #[test]
    fn validate_v2_pressure_trend_policy_rejects_zero_consecutive_threshold() {
        let policy = OntologyV2PressureTrendPolicy {
            min_samples: 3,
            consecutive_v2_candidate: 0,
        };
        let error = validate_v2_pressure_trend_policy(policy).expect_err("must fail");
        assert_eq!(error.code(), "ONTOLOGY_VIOLATION");
    }
}
