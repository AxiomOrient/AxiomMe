use serde::{Deserialize, Serialize};

use crate::{OmObserverRequest, OmReflectorRequest};

pub const OM_PROMPT_CONTRACT_NAME: &str = "episodic.om.prompt";
pub const OM_PROMPT_CONTRACT_VERSION: &str = "2.0.0";
pub const OM_PROTOCOL_VERSION: &str = "om-v2";
pub const OM_PROMPT_CONTRACT_MARKERS_XML_BLOCK: &str = r#"<contract-name>episodic.om.prompt</contract-name>
<contract-version>2.0.0</contract-version>
<protocol-version>om-v2</protocol-version>"#;
pub const OM_PROMPT_CONTRACT_MARKERS_XML_INLINE: &str = "<contract-name>episodic.om.prompt</contract-name>, <contract-version>2.0.0</contract-version>, and <protocol-version>om-v2</protocol-version>";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OmPromptRequestKind {
    ObserverSingle,
    ObserverMulti,
    Reflector,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmPromptContractHeader {
    pub contract_name: String,
    pub contract_version: String,
    pub protocol_version: String,
    pub request_kind: OmPromptRequestKind,
    pub scope: String,
    pub scope_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmPromptLimitsV2 {
    pub max_output_tokens: u32,
    pub observation_max_chars: Option<usize>,
    pub reflection_max_chars: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmPromptOutputContractV2 {
    pub format: String,
    pub required_sections: Vec<String>,
    pub continuation_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmObserverPromptContractV2 {
    pub header: OmPromptContractHeader,
    pub known_message_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_thread_id: Option<String>,
    pub has_other_conversation_context: bool,
    pub skip_continuation_hints: bool,
    pub limits: OmPromptLimitsV2,
    pub output_contract: OmPromptOutputContractV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OmReflectorPromptContractV2 {
    pub header: OmPromptContractHeader,
    pub generation_count: u32,
    pub compression_level: u8,
    pub skip_continuation_hints: bool,
    pub limits: OmPromptLimitsV2,
    pub output_contract: OmPromptOutputContractV2,
}

#[must_use]
fn build_observer_prompt_contract(
    request: &OmObserverRequest,
    request_kind: OmPromptRequestKind,
    known_message_ids: &[String],
    skip_continuation_hints: bool,
    preferred_thread_id: Option<&str>,
    observation_max_chars: usize,
) -> OmObserverPromptContractV2 {
    let mut ids = known_message_ids
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();

    OmObserverPromptContractV2 {
        header: OmPromptContractHeader {
            contract_name: OM_PROMPT_CONTRACT_NAME.to_string(),
            contract_version: OM_PROMPT_CONTRACT_VERSION.to_string(),
            protocol_version: OM_PROTOCOL_VERSION.to_string(),
            request_kind,
            scope: request.scope.as_str().to_string(),
            scope_key: request.scope_key.clone(),
        },
        known_message_ids: ids,
        preferred_thread_id: preferred_thread_id.map(ToString::to_string),
        has_other_conversation_context: request
            .other_conversations
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty()),
        skip_continuation_hints,
        limits: OmPromptLimitsV2 {
            max_output_tokens: request.model.max_output_tokens,
            observation_max_chars: Some(observation_max_chars),
            reflection_max_chars: None,
        },
        output_contract: OmPromptOutputContractV2 {
            format: "xml".to_string(),
            required_sections: if skip_continuation_hints {
                vec![
                    "contract-name".to_string(),
                    "contract-version".to_string(),
                    "protocol-version".to_string(),
                    "observations".to_string(),
                ]
            } else {
                vec![
                    "contract-name".to_string(),
                    "contract-version".to_string(),
                    "protocol-version".to_string(),
                    "observations".to_string(),
                    "current-task".to_string(),
                    "suggested-response".to_string(),
                ]
            },
            continuation_enabled: !skip_continuation_hints,
        },
    }
}

#[must_use]
pub fn build_observer_prompt_contract_v2(
    request: &OmObserverRequest,
    known_message_ids: &[String],
    skip_continuation_hints: bool,
    preferred_thread_id: Option<&str>,
    observation_max_chars: usize,
) -> OmObserverPromptContractV2 {
    build_observer_prompt_contract(
        request,
        OmPromptRequestKind::ObserverSingle,
        known_message_ids,
        skip_continuation_hints,
        preferred_thread_id,
        observation_max_chars,
    )
}

#[must_use]
pub fn build_multi_thread_observer_prompt_contract_v2(
    request: &OmObserverRequest,
    known_message_ids: &[String],
    skip_continuation_hints: bool,
    preferred_thread_id: Option<&str>,
    observation_max_chars: usize,
) -> OmObserverPromptContractV2 {
    build_observer_prompt_contract(
        request,
        OmPromptRequestKind::ObserverMulti,
        known_message_ids,
        skip_continuation_hints,
        preferred_thread_id,
        observation_max_chars,
    )
}

#[must_use]
pub fn build_reflector_prompt_contract_v2(
    request: &OmReflectorRequest,
    compression_level: u8,
    skip_continuation_hints: bool,
    reflection_max_chars: usize,
) -> OmReflectorPromptContractV2 {
    OmReflectorPromptContractV2 {
        header: OmPromptContractHeader {
            contract_name: OM_PROMPT_CONTRACT_NAME.to_string(),
            contract_version: OM_PROMPT_CONTRACT_VERSION.to_string(),
            protocol_version: OM_PROTOCOL_VERSION.to_string(),
            request_kind: OmPromptRequestKind::Reflector,
            scope: request.scope.as_str().to_string(),
            scope_key: request.scope_key.clone(),
        },
        generation_count: request.generation_count,
        compression_level,
        skip_continuation_hints,
        limits: OmPromptLimitsV2 {
            max_output_tokens: request.model.max_output_tokens,
            observation_max_chars: None,
            reflection_max_chars: Some(reflection_max_chars),
        },
        output_contract: OmPromptOutputContractV2 {
            format: "xml".to_string(),
            required_sections: if skip_continuation_hints {
                vec![
                    "contract-name".to_string(),
                    "contract-version".to_string(),
                    "protocol-version".to_string(),
                    "observations".to_string(),
                ]
            } else {
                vec![
                    "contract-name".to_string(),
                    "contract-version".to_string(),
                    "protocol-version".to_string(),
                    "observations".to_string(),
                    "current-task".to_string(),
                    "suggested-response".to_string(),
                ]
            },
            continuation_enabled: !skip_continuation_hints,
        },
    }
}
