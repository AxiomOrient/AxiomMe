use serde_json::Value;
use thiserror::Error;

use super::contract::{
    OM_PROMPT_CONTRACT_NAME, OM_PROMPT_CONTRACT_VERSION, OM_PROTOCOL_VERSION,
    OmObserverPromptContractV2, OmPromptRequestKind, OmReflectorPromptContractV2,
};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OmPromptContractParseError {
    #[error("invalid json: {reason}")]
    InvalidJson { reason: String },
    #[error("missing required field: {field}")]
    MissingRequiredField { field: String },
    #[error("invalid field type: {field}")]
    InvalidFieldType { field: String },
    #[error("contract name mismatch: expected `{expected}`, got `{actual}`")]
    ContractNameMismatch { expected: String, actual: String },
    #[error("contract version mismatch: expected `{expected}`, got `{actual}`")]
    ContractVersionMismatch { expected: String, actual: String },
    #[error("protocol version mismatch: expected `{expected}`, got `{actual}`")]
    ProtocolVersionMismatch { expected: String, actual: String },
    #[error("request kind mismatch: expected `{expected}`, got `{actual}`")]
    RequestKindMismatch { expected: String, actual: String },
    #[error("invalid contract payload: {reason}")]
    InvalidPayload { reason: String },
}

fn request_kind_name(kind: OmPromptRequestKind) -> &'static str {
    match kind {
        OmPromptRequestKind::ObserverSingle => "observer_single",
        OmPromptRequestKind::ObserverMulti => "observer_multi",
        OmPromptRequestKind::Reflector => "reflector",
    }
}

fn field_path(path: &[&str]) -> String {
    path.join(".")
}

fn lookup_field<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cursor = root;
    for key in path {
        cursor = cursor.as_object()?.get(*key)?;
    }
    Some(cursor)
}

fn ensure_required_field<'a>(
    root: &'a Value,
    path: &[&str],
) -> Result<&'a Value, OmPromptContractParseError> {
    let field = field_path(path);
    let value = lookup_field(root, path).ok_or_else(|| {
        OmPromptContractParseError::MissingRequiredField {
            field: field.clone(),
        }
    })?;
    if value.is_null() {
        return Err(OmPromptContractParseError::MissingRequiredField { field });
    }
    Ok(value)
}

fn ensure_string_field(root: &Value, path: &[&str]) -> Result<String, OmPromptContractParseError> {
    let field = field_path(path);
    let value = ensure_required_field(root, path)?;
    value
        .as_str()
        .map(ToString::to_string)
        .ok_or(OmPromptContractParseError::InvalidFieldType { field })
}

fn parse_contract_json(contract_json: &str) -> Result<Value, OmPromptContractParseError> {
    serde_json::from_str::<Value>(contract_json).map_err(|error| {
        OmPromptContractParseError::InvalidJson {
            reason: error.to_string(),
        }
    })
}

fn validate_common_contract_header(
    value: &Value,
    expected_request_kind: Option<OmPromptRequestKind>,
) -> Result<(), OmPromptContractParseError> {
    let required_header_fields: &[&[&str]] = &[
        &["header"],
        &["header", "contract_name"],
        &["header", "contract_version"],
        &["header", "protocol_version"],
        &["header", "request_kind"],
        &["header", "scope"],
        &["header", "scope_key"],
    ];
    for path in required_header_fields {
        ensure_required_field(value, path)?;
    }

    let contract_name = ensure_string_field(value, &["header", "contract_name"])?;
    if contract_name != OM_PROMPT_CONTRACT_NAME {
        return Err(OmPromptContractParseError::ContractNameMismatch {
            expected: OM_PROMPT_CONTRACT_NAME.to_string(),
            actual: contract_name,
        });
    }

    let contract_version = ensure_string_field(value, &["header", "contract_version"])?;
    if contract_version != OM_PROMPT_CONTRACT_VERSION {
        return Err(OmPromptContractParseError::ContractVersionMismatch {
            expected: OM_PROMPT_CONTRACT_VERSION.to_string(),
            actual: contract_version,
        });
    }

    let protocol_version = ensure_string_field(value, &["header", "protocol_version"])?;
    if protocol_version != OM_PROTOCOL_VERSION {
        return Err(OmPromptContractParseError::ProtocolVersionMismatch {
            expected: OM_PROTOCOL_VERSION.to_string(),
            actual: protocol_version,
        });
    }

    if let Some(expected_kind) = expected_request_kind {
        let actual_kind = ensure_string_field(value, &["header", "request_kind"])?;
        let expected_kind_name = request_kind_name(expected_kind);
        if actual_kind != expected_kind_name {
            return Err(OmPromptContractParseError::RequestKindMismatch {
                expected: expected_kind_name.to_string(),
                actual: actual_kind,
            });
        }
    }

    Ok(())
}

pub fn parse_observer_prompt_contract_v2(
    contract_json: &str,
    expected_request_kind: Option<OmPromptRequestKind>,
) -> Result<OmObserverPromptContractV2, OmPromptContractParseError> {
    let value = parse_contract_json(contract_json)?;
    validate_common_contract_header(&value, expected_request_kind)?;

    let required_paths: &[&[&str]] = &[
        &["known_message_ids"],
        &["has_other_conversation_context"],
        &["skip_continuation_hints"],
        &["limits"],
        &["limits", "max_output_tokens"],
        &["output_contract"],
        &["output_contract", "format"],
        &["output_contract", "required_sections"],
        &["output_contract", "continuation_enabled"],
    ];
    for path in required_paths {
        ensure_required_field(&value, path)?;
    }

    serde_json::from_value::<OmObserverPromptContractV2>(value).map_err(|error| {
        OmPromptContractParseError::InvalidPayload {
            reason: error.to_string(),
        }
    })
}

pub fn parse_reflector_prompt_contract_v2(
    contract_json: &str,
) -> Result<OmReflectorPromptContractV2, OmPromptContractParseError> {
    let value = parse_contract_json(contract_json)?;
    validate_common_contract_header(&value, Some(OmPromptRequestKind::Reflector))?;

    let required_paths: &[&[&str]] = &[
        &["generation_count"],
        &["compression_level"],
        &["skip_continuation_hints"],
        &["limits"],
        &["limits", "max_output_tokens"],
        &["output_contract"],
        &["output_contract", "format"],
        &["output_contract", "required_sections"],
        &["output_contract", "continuation_enabled"],
    ];
    for path in required_paths {
        ensure_required_field(&value, path)?;
    }

    serde_json::from_value::<OmReflectorPromptContractV2>(value).map_err(|error| {
        OmPromptContractParseError::InvalidPayload {
            reason: error.to_string(),
        }
    })
}
