use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OmScope {
    Session,
    Thread,
    Resource,
}

impl OmScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Thread => "thread",
            Self::Resource => "resource",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "session" => Some(Self::Session),
            "thread" => Some(Self::Thread),
            "resource" => Some(Self::Resource),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OmOriginType {
    Initial,
    Reflection,
}

impl OmOriginType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Initial => "initial",
            Self::Reflection => "reflection",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "initial" => Some(Self::Initial),
            "reflection" => Some(Self::Reflection),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmRecord {
    pub id: String,
    pub scope: OmScope,
    pub scope_key: String,
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub resource_id: Option<String>,
    pub generation_count: u32,
    pub last_applied_outbox_event_id: Option<i64>,
    pub origin_type: OmOriginType,
    pub active_observations: String,
    pub observation_token_count: u32,
    pub pending_message_tokens: u32,
    pub last_observed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_response: Option<String>,
    #[serde(default)]
    pub last_activated_message_ids: Vec<String>,
    #[serde(default)]
    pub observer_trigger_count_total: u32,
    #[serde(default)]
    pub reflector_trigger_count_total: u32,
    pub is_observing: bool,
    pub is_reflecting: bool,
    pub is_buffering_observation: bool,
    pub is_buffering_reflection: bool,
    pub last_buffered_at_tokens: u32,
    pub last_buffered_at_time: Option<DateTime<Utc>>,
    pub buffered_reflection: Option<String>,
    pub buffered_reflection_tokens: Option<u32>,
    pub buffered_reflection_input_tokens: Option<u32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmObservationChunk {
    pub id: String,
    pub record_id: String,
    pub seq: u32,
    pub cycle_id: String,
    pub observations: String,
    pub token_count: u32,
    pub message_tokens: u32,
    pub message_ids: Vec<String>,
    pub last_observed_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmThreadRefV2 {
    pub canonical_thread_id: String,
    pub scope: OmScope,
    pub scope_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OmContinuationSourceKind {
    ObserverLlm,
    ObserverDeterministic,
    Reflector,
    ExplicitUserTask,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmContinuationStateV2 {
    pub scope_key: String,
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_response: Option<String>,
    pub confidence_milli: u16,
    pub source_kind: OmContinuationSourceKind,
    #[serde(default)]
    pub source_message_ids: Vec<String>,
    pub updated_at_rfc3339: String,
    pub staleness_budget_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmContinuationCandidateV2 {
    pub scope_key: String,
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_response: Option<String>,
    pub confidence_milli: u16,
    pub source_kind: OmContinuationSourceKind,
    #[serde(default)]
    pub source_message_ids: Vec<String>,
    pub updated_at_rfc3339: String,
    pub staleness_budget_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContinuationPolicyV2 {
    pub min_confidence_milli_for_task: u16,
    pub min_confidence_milli_for_suggested_response: u16,
    pub preserve_existing_task_on_weaker_update: bool,
    pub only_improve_suggested_response: bool,
}

impl Default for ContinuationPolicyV2 {
    fn default() -> Self {
        Self {
            min_confidence_milli_for_task: 500,
            min_confidence_milli_for_suggested_response: 700,
            preserve_existing_task_on_weaker_update: true,
            only_improve_suggested_response: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OmObservationPriority {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OmObservationOriginKind {
    Observation,
    Chunk,
    Summary,
    Reflection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmObservationEntryV2 {
    pub entry_id: String,
    pub scope_key: String,
    pub thread_id: String,
    pub priority: OmObservationPriority,
    pub text: String,
    #[serde(default)]
    pub source_message_ids: Vec<String>,
    pub origin_kind: OmObservationOriginKind,
    pub created_at_rfc3339: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmReflectionResponseV2 {
    #[serde(default)]
    pub covers_entry_ids: Vec<String>,
    pub reflection_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_response: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OmDeterministicEvidenceKind {
    TaskSignal,
    ErrorSignal,
    ObservationLine,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmDeterministicEvidence {
    pub message_id: String,
    pub role: String,
    pub kind: OmDeterministicEvidenceKind,
    pub excerpt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmDeterministicObserverResponseV2 {
    pub observations: String,
    pub observation_token_count: u32,
    #[serde(default)]
    pub observed_message_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_response: Option<String>,
    pub confidence_milli: u16,
    #[serde(default)]
    pub evidence: Vec<OmDeterministicEvidence>,
}

pub const OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION: &str = "om-search-visible-snapshot-v2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmSearchVisibleSnapshotV2 {
    pub scope_key: String,
    #[serde(default)]
    pub activated_entry_ids: Vec<String>,
    #[serde(default)]
    pub buffered_entry_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_response: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendered_hint: Option<String>,
    pub materialized_at_rfc3339: String,
    pub snapshot_version: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub visible_entries: Vec<OmObservationEntryV2>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmHintPolicyV2 {
    pub max_lines: usize,
    pub max_chars: usize,
    pub reserve_current_task_line: bool,
    pub reserve_suggested_response_line: bool,
    pub high_priority_slots: usize,
    pub include_buffered_entries: bool,
}

impl Default for OmHintPolicyV2 {
    fn default() -> Self {
        Self {
            max_lines: 4,
            max_chars: 240,
            reserve_current_task_line: true,
            reserve_suggested_response_line: true,
            high_priority_slots: 1,
            include_buffered_entries: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OmRecordInvariantViolation {
    MissingScopeIdentifier {
        field: &'static str,
    },
    EmptyIdentifier {
        field: &'static str,
    },
    EmptyScopeKey,
    ScopeKeyPrefixMismatch {
        expected_prefix: &'static str,
    },
    ScopeKeyIdentifierMismatch {
        expected_identifier: String,
        actual_identifier: String,
    },
    EmptyBufferedReflection,
    BufferedReflectionMetadataWithoutText {
        field: &'static str,
    },
}

pub fn validate_om_record_invariants(record: &OmRecord) -> Vec<OmRecordInvariantViolation> {
    let mut violations = Vec::<OmRecordInvariantViolation>::new();

    let session_id = record.session_id.as_deref().map(str::trim);
    if session_id.is_some_and(str::is_empty) {
        violations.push(OmRecordInvariantViolation::EmptyIdentifier {
            field: "session_id",
        });
    }
    let thread_id = record.thread_id.as_deref().map(str::trim);
    if thread_id.is_some_and(str::is_empty) {
        violations.push(OmRecordInvariantViolation::EmptyIdentifier { field: "thread_id" });
    }
    let resource_id = record.resource_id.as_deref().map(str::trim);
    if resource_id.is_some_and(str::is_empty) {
        violations.push(OmRecordInvariantViolation::EmptyIdentifier {
            field: "resource_id",
        });
    }

    let (required_identifier, required_field, expected_prefix) = match record.scope {
        OmScope::Session => (
            session_id.filter(|value| !value.is_empty()),
            "session_id",
            "session:",
        ),
        OmScope::Thread => (
            thread_id.filter(|value| !value.is_empty()),
            "thread_id",
            "thread:",
        ),
        OmScope::Resource => (
            resource_id.filter(|value| !value.is_empty()),
            "resource_id",
            "resource:",
        ),
    };
    if required_identifier.is_none() {
        violations.push(OmRecordInvariantViolation::MissingScopeIdentifier {
            field: required_field,
        });
    }

    let scope_key = record.scope_key.trim();
    if scope_key.is_empty() {
        violations.push(OmRecordInvariantViolation::EmptyScopeKey);
    } else if !scope_key.starts_with(expected_prefix) {
        violations.push(OmRecordInvariantViolation::ScopeKeyPrefixMismatch { expected_prefix });
    } else if let Some(expected_identifier) = required_identifier {
        let actual_identifier = scope_key
            .strip_prefix(expected_prefix)
            .unwrap_or_default()
            .trim();
        if actual_identifier != expected_identifier {
            violations.push(OmRecordInvariantViolation::ScopeKeyIdentifierMismatch {
                expected_identifier: expected_identifier.to_string(),
                actual_identifier: actual_identifier.to_string(),
            });
        }
    }

    let has_buffered_reflection = match record.buffered_reflection.as_deref() {
        Some(value) => {
            if value.trim().is_empty() {
                violations.push(OmRecordInvariantViolation::EmptyBufferedReflection);
                false
            } else {
                true
            }
        }
        None => false,
    };
    if !has_buffered_reflection {
        if record.buffered_reflection_tokens.is_some() {
            violations.push(
                OmRecordInvariantViolation::BufferedReflectionMetadataWithoutText {
                    field: "buffered_reflection_tokens",
                },
            );
        }
        if record.buffered_reflection_input_tokens.is_some() {
            violations.push(
                OmRecordInvariantViolation::BufferedReflectionMetadataWithoutText {
                    field: "buffered_reflection_input_tokens",
                },
            );
        }
    }

    violations
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OmObservationEntryInvariantViolation {
    EmptyField { field: &'static str },
    InvalidRfc3339 { field: &'static str, value: String },
    EmptySourceMessageId,
    EmptySupersededBy,
}

pub fn validate_observation_entry_v2_invariants(
    entry: &OmObservationEntryV2,
) -> Vec<OmObservationEntryInvariantViolation> {
    let mut violations = Vec::<OmObservationEntryInvariantViolation>::new();
    if entry.entry_id.trim().is_empty() {
        violations.push(OmObservationEntryInvariantViolation::EmptyField { field: "entry_id" });
    }
    if entry.scope_key.trim().is_empty() {
        violations.push(OmObservationEntryInvariantViolation::EmptyField { field: "scope_key" });
    }
    if entry.thread_id.trim().is_empty() {
        violations.push(OmObservationEntryInvariantViolation::EmptyField { field: "thread_id" });
    }
    if entry.text.trim().is_empty() {
        violations.push(OmObservationEntryInvariantViolation::EmptyField { field: "text" });
    }
    if entry.created_at_rfc3339.trim().is_empty() {
        violations.push(OmObservationEntryInvariantViolation::EmptyField {
            field: "created_at_rfc3339",
        });
    } else if DateTime::parse_from_rfc3339(&entry.created_at_rfc3339).is_err() {
        violations.push(OmObservationEntryInvariantViolation::InvalidRfc3339 {
            field: "created_at_rfc3339",
            value: entry.created_at_rfc3339.clone(),
        });
    }
    if entry
        .source_message_ids
        .iter()
        .any(|id| id.trim().is_empty())
    {
        violations.push(OmObservationEntryInvariantViolation::EmptySourceMessageId);
    }
    if entry
        .superseded_by
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        violations.push(OmObservationEntryInvariantViolation::EmptySupersededBy);
    }
    violations
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OmSearchVisibleSnapshotInvariantViolation {
    EmptyField {
        field: &'static str,
    },
    InvalidRfc3339 {
        field: &'static str,
        value: String,
    },
    SnapshotVersionMismatch {
        expected: &'static str,
        actual: String,
    },
    EmptyActivatedEntryId,
    EmptyBufferedEntryId,
    VisibleEntryScopeMismatch {
        entry_id: String,
        scope_key: String,
    },
    VisibleEntryInvariant {
        entry_id: String,
        violation: OmObservationEntryInvariantViolation,
    },
}

pub fn validate_search_visible_snapshot_v2_invariants(
    snapshot: &OmSearchVisibleSnapshotV2,
) -> Vec<OmSearchVisibleSnapshotInvariantViolation> {
    let mut violations = Vec::<OmSearchVisibleSnapshotInvariantViolation>::new();
    if snapshot.scope_key.trim().is_empty() {
        violations
            .push(OmSearchVisibleSnapshotInvariantViolation::EmptyField { field: "scope_key" });
    }
    if snapshot.materialized_at_rfc3339.trim().is_empty() {
        violations.push(OmSearchVisibleSnapshotInvariantViolation::EmptyField {
            field: "materialized_at_rfc3339",
        });
    } else if DateTime::parse_from_rfc3339(&snapshot.materialized_at_rfc3339).is_err() {
        violations.push(OmSearchVisibleSnapshotInvariantViolation::InvalidRfc3339 {
            field: "materialized_at_rfc3339",
            value: snapshot.materialized_at_rfc3339.clone(),
        });
    }
    if snapshot.snapshot_version.trim().is_empty() {
        violations.push(OmSearchVisibleSnapshotInvariantViolation::EmptyField {
            field: "snapshot_version",
        });
    } else if snapshot.snapshot_version != OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION {
        violations.push(
            OmSearchVisibleSnapshotInvariantViolation::SnapshotVersionMismatch {
                expected: OM_SEARCH_VISIBLE_SNAPSHOT_V2_VERSION,
                actual: snapshot.snapshot_version.clone(),
            },
        );
    }
    if snapshot
        .activated_entry_ids
        .iter()
        .any(|entry_id| entry_id.trim().is_empty())
    {
        violations.push(OmSearchVisibleSnapshotInvariantViolation::EmptyActivatedEntryId);
    }
    if snapshot
        .buffered_entry_ids
        .iter()
        .any(|entry_id| entry_id.trim().is_empty())
    {
        violations.push(OmSearchVisibleSnapshotInvariantViolation::EmptyBufferedEntryId);
    }

    for entry in &snapshot.visible_entries {
        if entry.scope_key.trim() != snapshot.scope_key.trim() {
            violations.push(
                OmSearchVisibleSnapshotInvariantViolation::VisibleEntryScopeMismatch {
                    entry_id: entry.entry_id.clone(),
                    scope_key: entry.scope_key.clone(),
                },
            );
        }
        for violation in validate_observation_entry_v2_invariants(entry) {
            violations.push(
                OmSearchVisibleSnapshotInvariantViolation::VisibleEntryInvariant {
                    entry_id: entry.entry_id.clone(),
                    violation,
                },
            );
        }
    }

    violations
}

#[cfg(test)]
mod tests;
