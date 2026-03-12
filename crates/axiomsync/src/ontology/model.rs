use serde::{Deserialize, Serialize};

use crate::uri::Scope;

pub const ONTOLOGY_SCHEMA_URI_V1: &str = "axiom://agent/ontology/schema.v1.json";
pub const DEFAULT_ONTOLOGY_SCHEMA_V1_JSON: &str = r#"{
  "version": 1,
  "object_types": [],
  "link_types": [],
  "action_types": [],
  "invariants": []
}"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct OntologySchemaV1 {
    pub version: u32,
    #[serde(default)]
    pub object_types: Vec<ObjectTypeDef>,
    #[serde(default)]
    pub link_types: Vec<LinkTypeDef>,
    #[serde(default)]
    pub action_types: Vec<ActionTypeDef>,
    #[serde(default)]
    pub invariants: Vec<InvariantDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ObjectTypeDef {
    pub id: String,
    #[serde(default)]
    pub uri_prefixes: Vec<String>,
    #[serde(default)]
    pub required_tags: Vec<String>,
    #[serde(default)]
    pub allowed_scopes: Vec<Scope>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LinkTypeDef {
    pub id: String,
    #[serde(default)]
    pub from_types: Vec<String>,
    #[serde(default)]
    pub to_types: Vec<String>,
    #[serde(default = "default_link_min_arity")]
    pub min_arity: usize,
    #[serde(default = "default_link_max_arity")]
    pub max_arity: usize,
    #[serde(default)]
    pub symmetric: bool,
}

impl Default for LinkTypeDef {
    fn default() -> Self {
        Self {
            id: String::new(),
            from_types: Vec::new(),
            to_types: Vec::new(),
            min_arity: default_link_min_arity(),
            max_arity: default_link_max_arity(),
            symmetric: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct ActionTypeDef {
    pub id: String,
    pub input_contract: String,
    #[serde(default)]
    pub effects: Vec<String>,
    pub queue_event_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
pub struct InvariantDef {
    pub id: String,
    pub rule: String,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OntologyActionRequestV1 {
    pub action_id: String,
    pub queue_event_type: String,
    #[serde(default)]
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OntologyJsonValueKind {
    Null,
    Boolean,
    Number,
    String,
    Array,
    Object,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OntologyActionValidationReport {
    pub action_id: String,
    pub queue_event_type: String,
    pub input_contract: String,
    pub input_kind: OntologyJsonValueKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OntologyInvariantCheckStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OntologyInvariantFailureKind {
    InvalidSeverity,
    UnsupportedRule,
    MissingTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OntologyInvariantCheckItem {
    pub id: String,
    pub severity: String,
    pub rule: String,
    pub message: String,
    pub status: OntologyInvariantCheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<OntologyInvariantFailureKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OntologyInvariantCheckReport {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub items: Vec<OntologyInvariantCheckItem>,
}

const fn default_link_min_arity() -> usize {
    2
}

const fn default_link_max_arity() -> usize {
    64
}
