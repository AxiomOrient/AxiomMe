use std::collections::{HashMap, HashSet};

use crate::error::{AxiomError, Result};
use crate::uri::AxiomUri;

use super::model::{
    ActionTypeDef, InvariantDef, LinkTypeDef, ObjectTypeDef, OntologyActionRequestV1,
    OntologyActionValidationReport, OntologyInvariantCheckItem, OntologyInvariantCheckReport,
    OntologyInvariantCheckStatus, OntologyInvariantFailureKind, OntologyJsonValueKind,
    OntologySchemaV1,
};

const ONTOLOGY_SCHEMA_VERSION_V1: u32 = 1;

#[derive(Debug, Clone)]
struct UriPrefixRule {
    prefix: String,
    object_type_id: String,
}

#[derive(Debug, Clone)]
struct CompiledLinkType {
    def: LinkTypeDef,
    from_types: HashSet<String>,
    to_types: HashSet<String>,
    allowed_types: HashSet<String>,
}

impl CompiledLinkType {
    fn from_def(def: LinkTypeDef) -> Self {
        let from_types = def.from_types.iter().cloned().collect::<HashSet<_>>();
        let to_types = def.to_types.iter().cloned().collect::<HashSet<_>>();
        let allowed_types = from_types.union(&to_types).cloned().collect::<HashSet<_>>();
        Self {
            def,
            from_types,
            to_types,
            allowed_types,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompiledOntologySchema {
    object_types: HashMap<String, ObjectTypeDef>,
    link_types: HashMap<String, CompiledLinkType>,
    action_types: HashMap<String, ActionTypeDef>,
    invariants: Vec<InvariantDef>,
    uri_prefix_rules: Vec<UriPrefixRule>,
}

impl CompiledOntologySchema {
    fn resolve_object_type(&self, uri: &AxiomUri) -> Option<&str> {
        let target = uri.to_string();
        for rule in &self.uri_prefix_rules {
            if uri_matches_prefix(&target, &rule.prefix) {
                return Some(&rule.object_type_id);
            }
        }
        None
    }

    #[must_use]
    pub fn resolve_object_type_id(&self, uri: &AxiomUri) -> Option<&str> {
        self.resolve_object_type(uri)
    }

    #[must_use]
    pub fn link_type(&self, relation_id: &str) -> Option<&LinkTypeDef> {
        self.link_types
            .get(relation_id)
            .map(|compiled| &compiled.def)
    }

    #[must_use]
    pub fn action_type(&self, action_id: &str) -> Option<&ActionTypeDef> {
        self.action_types.get(action_id)
    }

    #[must_use]
    pub fn has_object_type(&self, object_type_id: &str) -> bool {
        self.object_types.contains_key(object_type_id)
    }

    #[must_use]
    pub fn has_link_type(&self, link_type_id: &str) -> bool {
        self.link_types.contains_key(link_type_id)
    }

    #[must_use]
    pub fn has_action_type(&self, action_type_id: &str) -> bool {
        self.action_types.contains_key(action_type_id)
    }

    #[must_use]
    pub fn invariants(&self) -> &[InvariantDef] {
        &self.invariants
    }
}

pub fn compile_schema(schema: OntologySchemaV1) -> Result<CompiledOntologySchema> {
    if schema.version != ONTOLOGY_SCHEMA_VERSION_V1 {
        return Err(AxiomError::OntologyViolation(format!(
            "ontology schema version mismatch: expected {ONTOLOGY_SCHEMA_VERSION_V1}, got {}",
            schema.version
        )));
    }

    let object_types_by_id = compile_object_types(&schema.object_types)?;
    let link_types = compile_link_types(&schema.link_types, &object_types_by_id)?;
    let action_types = compile_action_defs(&schema.action_types)?;
    let invariants = compile_invariant_defs(&schema.invariants)?;

    let mut uri_prefix_rules = Vec::<UriPrefixRule>::new();
    for object_type in &schema.object_types {
        for prefix in &object_type.uri_prefixes {
            let parsed = AxiomUri::parse(prefix).map_err(|err| {
                AxiomError::OntologyViolation(format!(
                    "invalid ontology object uri_prefix '{}': {err}",
                    prefix
                ))
            })?;
            let normalized_prefix = parsed.to_string();
            uri_prefix_rules.push(UriPrefixRule {
                prefix: normalized_prefix,
                object_type_id: object_type.id.clone(),
            });
        }
    }
    uri_prefix_rules.sort_by(|a, b| b.prefix.len().cmp(&a.prefix.len()));

    Ok(CompiledOntologySchema {
        object_types: object_types_by_id,
        link_types,
        action_types,
        invariants,
        uri_prefix_rules,
    })
}

pub fn validate_relation_link(
    schema: &CompiledOntologySchema,
    relation_id: &str,
    uris: &[AxiomUri],
) -> Result<()> {
    if schema.link_types.is_empty() {
        return Ok(());
    }

    let link = schema.link_types.get(relation_id).ok_or_else(|| {
        AxiomError::OntologyViolation(format!(
            "ontology link type is not declared: relation_id='{relation_id}'"
        ))
    })?;

    let arity = uris.len();
    if arity < link.def.min_arity || arity > link.def.max_arity {
        return Err(AxiomError::OntologyViolation(format!(
            "ontology relation arity is out of range: relation_id='{}' arity={} expected={}..={}",
            relation_id, arity, link.def.min_arity, link.def.max_arity
        )));
    }

    let mut resolved_types = Vec::<&str>::with_capacity(uris.len());
    for uri in uris {
        let object_type = schema.resolve_object_type(uri).ok_or_else(|| {
            AxiomError::OntologyViolation(format!(
                "ontology object type is not resolved for uri: relation_id='{}' uri='{}'",
                relation_id, uri
            ))
        })?;
        resolved_types.push(object_type);
    }

    let from_present = resolved_types
        .iter()
        .any(|object_type| link.from_types.contains(*object_type));
    let to_present = resolved_types
        .iter()
        .any(|object_type| link.to_types.contains(*object_type));
    if !from_present || !to_present {
        return Err(AxiomError::OntologyViolation(format!(
            "ontology relation endpoint type coverage mismatch: relation_id='{}' requires from={:?} to={:?}",
            relation_id, link.def.from_types, link.def.to_types
        )));
    }

    for (index, object_type) in resolved_types.iter().enumerate() {
        if !link.allowed_types.contains(*object_type) {
            return Err(AxiomError::OntologyViolation(format!(
                "ontology relation endpoint type is not allowed: relation_id='{}' endpoint_index={} type='{}'",
                relation_id, index, object_type
            )));
        }
    }

    Ok(())
}

pub fn validate_action_request(
    schema: &CompiledOntologySchema,
    request: &OntologyActionRequestV1,
) -> Result<OntologyActionValidationReport> {
    let action_id = request.action_id.trim();
    if action_id.is_empty() {
        return Err(AxiomError::OntologyViolation(
            "ontology action request action_id must not be empty".to_string(),
        ));
    }
    let queue_event_type = request.queue_event_type.trim();
    if queue_event_type.is_empty() {
        return Err(AxiomError::OntologyViolation(
            "ontology action request queue_event_type must not be empty".to_string(),
        ));
    }

    let action = schema.action_type(action_id).ok_or_else(|| {
        AxiomError::OntologyViolation(format!(
            "ontology action type is not declared: action_id='{action_id}'"
        ))
    })?;
    if action.queue_event_type != queue_event_type {
        return Err(AxiomError::OntologyViolation(format!(
            "ontology action queue_event_type mismatch: action_id='{action_id}' expected='{}' got='{}'",
            action.queue_event_type, queue_event_type
        )));
    }

    validate_action_input_contract(action.input_contract.as_str(), &request.input)?;
    Ok(OntologyActionValidationReport {
        action_id: action.id.clone(),
        queue_event_type: action.queue_event_type.clone(),
        input_contract: action.input_contract.clone(),
        input_kind: json_value_kind(&request.input),
    })
}

pub fn evaluate_invariants(schema: &CompiledOntologySchema) -> OntologyInvariantCheckReport {
    let mut items = Vec::<OntologyInvariantCheckItem>::with_capacity(schema.invariants().len());
    let mut passed = 0_usize;
    let mut failed = 0_usize;

    for invariant in schema.invariants() {
        let (status, failure_kind, failure_detail) = evaluate_invariant(schema, invariant);
        if status == OntologyInvariantCheckStatus::Pass {
            passed = passed.saturating_add(1);
        } else {
            failed = failed.saturating_add(1);
        }
        items.push(OntologyInvariantCheckItem {
            id: invariant.id.clone(),
            severity: invariant.severity.clone(),
            rule: invariant.rule.clone(),
            message: invariant.message.clone(),
            status,
            failure_kind,
            failure_detail,
        });
    }

    OntologyInvariantCheckReport {
        total: items.len(),
        passed,
        failed,
        items,
    }
}

fn compile_object_types(object_types: &[ObjectTypeDef]) -> Result<HashMap<String, ObjectTypeDef>> {
    let mut out = HashMap::<String, ObjectTypeDef>::new();

    for object_type in object_types {
        let id = object_type.id.trim();
        if id.is_empty() {
            return Err(AxiomError::OntologyViolation(
                "ontology object type id must not be empty".to_string(),
            ));
        }
        if object_type.uri_prefixes.is_empty() {
            return Err(AxiomError::OntologyViolation(format!(
                "ontology object type '{id}' must declare at least one uri_prefix"
            )));
        }
        if object_type.allowed_scopes.is_empty() {
            return Err(AxiomError::OntologyViolation(format!(
                "ontology object type '{id}' must declare at least one allowed scope"
            )));
        }
        if out.contains_key(id) {
            return Err(AxiomError::OntologyViolation(format!(
                "duplicate ontology object type id: {id}"
            )));
        }

        for prefix in &object_type.uri_prefixes {
            let parsed = AxiomUri::parse(prefix).map_err(|err| {
                AxiomError::OntologyViolation(format!(
                    "invalid ontology object uri_prefix '{}': {err}",
                    prefix
                ))
            })?;
            if !object_type.allowed_scopes.contains(&parsed.scope()) {
                return Err(AxiomError::OntologyViolation(format!(
                    "ontology object type '{id}' prefix scope '{}' is not in allowed_scopes",
                    parsed.scope()
                )));
            }
        }

        out.insert(id.to_string(), object_type.clone());
    }

    Ok(out)
}

fn compile_link_types(
    link_types: &[LinkTypeDef],
    object_types_by_id: &HashMap<String, ObjectTypeDef>,
) -> Result<HashMap<String, CompiledLinkType>> {
    let mut out = HashMap::<String, CompiledLinkType>::new();

    for link in link_types {
        let id = link.id.trim();
        if id.is_empty() {
            return Err(AxiomError::OntologyViolation(
                "ontology link type id must not be empty".to_string(),
            ));
        }
        if out.contains_key(id) {
            return Err(AxiomError::OntologyViolation(format!(
                "duplicate ontology link type id: {id}"
            )));
        }
        if link.from_types.is_empty() || link.to_types.is_empty() {
            return Err(AxiomError::OntologyViolation(format!(
                "ontology link type '{id}' must declare non-empty from_types and to_types"
            )));
        }
        if link.min_arity < 2 {
            return Err(AxiomError::OntologyViolation(format!(
                "ontology link type '{id}' min_arity must be >= 2"
            )));
        }
        if link.max_arity < link.min_arity {
            return Err(AxiomError::OntologyViolation(format!(
                "ontology link type '{id}' max_arity must be >= min_arity"
            )));
        }

        for object_id in link.from_types.iter().chain(link.to_types.iter()) {
            if !object_types_by_id.contains_key(object_id) {
                return Err(AxiomError::OntologyViolation(format!(
                    "ontology link type '{id}' references unknown object type '{object_id}'"
                )));
            }
        }

        out.insert(id.to_string(), CompiledLinkType::from_def(link.clone()));
    }

    Ok(out)
}

fn compile_action_defs(action_types: &[ActionTypeDef]) -> Result<HashMap<String, ActionTypeDef>> {
    let mut out = HashMap::<String, ActionTypeDef>::new();
    for action in action_types {
        let id = action.id.trim();
        if id.is_empty() {
            return Err(AxiomError::OntologyViolation(
                "ontology action type id must not be empty".to_string(),
            ));
        }
        if out.contains_key(id) {
            return Err(AxiomError::OntologyViolation(format!(
                "duplicate ontology action type id: {id}"
            )));
        }
        if action.input_contract.trim().is_empty() {
            return Err(AxiomError::OntologyViolation(format!(
                "ontology action type '{id}' input_contract must not be empty"
            )));
        }
        if let Err(detail) = parse_action_input_contract(action.input_contract.as_str()) {
            return Err(AxiomError::OntologyViolation(format!(
                "ontology action type '{id}' input_contract is unsupported: {detail}"
            )));
        }
        if action.queue_event_type.trim().is_empty() {
            return Err(AxiomError::OntologyViolation(format!(
                "ontology action type '{id}' queue_event_type must not be empty"
            )));
        }
        out.insert(id.to_string(), action.clone());
    }
    Ok(out)
}

fn compile_invariant_defs(invariants: &[InvariantDef]) -> Result<Vec<InvariantDef>> {
    let mut ids = HashSet::<String>::new();
    let mut out = Vec::<InvariantDef>::with_capacity(invariants.len());
    for invariant in invariants {
        let id = invariant.id.trim();
        if id.is_empty() {
            return Err(AxiomError::OntologyViolation(
                "ontology invariant id must not be empty".to_string(),
            ));
        }
        if !ids.insert(id.to_string()) {
            return Err(AxiomError::OntologyViolation(format!(
                "duplicate ontology invariant id: {id}"
            )));
        }
        if invariant.rule.trim().is_empty() {
            return Err(AxiomError::OntologyViolation(format!(
                "ontology invariant '{id}' rule must not be empty"
            )));
        }
        if invariant.severity.trim().is_empty() {
            return Err(AxiomError::OntologyViolation(format!(
                "ontology invariant '{id}' severity must not be empty"
            )));
        }
        if invariant.message.trim().is_empty() {
            return Err(AxiomError::OntologyViolation(format!(
                "ontology invariant '{id}' message must not be empty"
            )));
        }
        out.push(invariant.clone());
    }
    Ok(out)
}

fn validate_action_input_contract(contract: &str, input: &serde_json::Value) -> Result<()> {
    let contract = parse_action_input_contract(contract).map_err(|detail| {
        AxiomError::OntologyViolation(format!(
            "ontology action input contract is unsupported: {detail}"
        ))
    })?;
    let ActionInputContract::Strict(expected_kind) = contract else {
        return Ok(());
    };
    let actual_kind = json_value_kind(input);
    if actual_kind != expected_kind {
        return Err(AxiomError::OntologyViolation(format!(
            "ontology action input contract mismatch: expected='{}' actual='{}'",
            json_value_kind_label(expected_kind),
            json_value_kind_label(actual_kind)
        )));
    }
    Ok(())
}

enum ActionInputContract {
    Any,
    Strict(OntologyJsonValueKind),
}

fn parse_action_input_contract(contract: &str) -> std::result::Result<ActionInputContract, String> {
    if contract.eq_ignore_ascii_case("json-any") {
        return Ok(ActionInputContract::Any);
    }
    if contract.eq_ignore_ascii_case("json-null") {
        return Ok(ActionInputContract::Strict(OntologyJsonValueKind::Null));
    }
    if contract.eq_ignore_ascii_case("json-boolean") {
        return Ok(ActionInputContract::Strict(OntologyJsonValueKind::Boolean));
    }
    if contract.eq_ignore_ascii_case("json-number") {
        return Ok(ActionInputContract::Strict(OntologyJsonValueKind::Number));
    }
    if contract.eq_ignore_ascii_case("json-string") {
        return Ok(ActionInputContract::Strict(OntologyJsonValueKind::String));
    }
    if contract.eq_ignore_ascii_case("json-array") {
        return Ok(ActionInputContract::Strict(OntologyJsonValueKind::Array));
    }
    if contract.eq_ignore_ascii_case("json-object") {
        return Ok(ActionInputContract::Strict(OntologyJsonValueKind::Object));
    }
    Err(format!(
        "supported contracts are: json-any|json-null|json-boolean|json-number|json-string|json-array|json-object (got '{contract}')"
    ))
}

fn json_value_kind(value: &serde_json::Value) -> OntologyJsonValueKind {
    match value {
        serde_json::Value::Null => OntologyJsonValueKind::Null,
        serde_json::Value::Bool(_) => OntologyJsonValueKind::Boolean,
        serde_json::Value::Number(_) => OntologyJsonValueKind::Number,
        serde_json::Value::String(_) => OntologyJsonValueKind::String,
        serde_json::Value::Array(_) => OntologyJsonValueKind::Array,
        serde_json::Value::Object(_) => OntologyJsonValueKind::Object,
    }
}

const fn json_value_kind_label(kind: OntologyJsonValueKind) -> &'static str {
    match kind {
        OntologyJsonValueKind::Null => "json-null",
        OntologyJsonValueKind::Boolean => "json-boolean",
        OntologyJsonValueKind::Number => "json-number",
        OntologyJsonValueKind::String => "json-string",
        OntologyJsonValueKind::Array => "json-array",
        OntologyJsonValueKind::Object => "json-object",
    }
}

fn evaluate_invariant(
    schema: &CompiledOntologySchema,
    invariant: &InvariantDef,
) -> (
    OntologyInvariantCheckStatus,
    Option<OntologyInvariantFailureKind>,
    Option<String>,
) {
    let severity = invariant.severity.trim();
    if !is_supported_invariant_severity(severity) {
        return (
            OntologyInvariantCheckStatus::Fail,
            Some(OntologyInvariantFailureKind::InvalidSeverity),
            Some(format!(
                "unsupported invariant severity '{severity}' (expected: info|warn|error)"
            )),
        );
    }

    let parsed_rule = match parse_invariant_rule(invariant.rule.as_str()) {
        Ok(parsed) => parsed,
        Err(detail) => {
            return (
                OntologyInvariantCheckStatus::Fail,
                Some(OntologyInvariantFailureKind::UnsupportedRule),
                Some(detail),
            );
        }
    };
    if !parsed_rule.target_exists(schema) {
        return (
            OntologyInvariantCheckStatus::Fail,
            Some(OntologyInvariantFailureKind::MissingTarget),
            Some(format!(
                "invariant target is not declared in schema: {}",
                parsed_rule.target_id()
            )),
        );
    }

    (OntologyInvariantCheckStatus::Pass, None, None)
}

fn is_supported_invariant_severity(severity: &str) -> bool {
    severity.eq_ignore_ascii_case("info")
        || severity.eq_ignore_ascii_case("warn")
        || severity.eq_ignore_ascii_case("error")
}

enum ParsedInvariantRule<'a> {
    Object(&'a str),
    Link(&'a str),
    Action(&'a str),
}

impl ParsedInvariantRule<'_> {
    fn target_exists(&self, schema: &CompiledOntologySchema) -> bool {
        match self {
            Self::Object(target) => schema.has_object_type(target),
            Self::Link(target) => schema.has_link_type(target),
            Self::Action(target) => schema.has_action_type(target),
        }
    }

    fn target_id(&self) -> &str {
        match self {
            Self::Object(target) => target,
            Self::Link(target) => target,
            Self::Action(target) => target,
        }
    }
}

fn parse_invariant_rule(rule: &str) -> std::result::Result<ParsedInvariantRule<'_>, String> {
    let (kind, target_id) = rule
        .split_once(':')
        .ok_or_else(|| "invariant rule must use '<kind>:<target_id>' format".to_string())?;
    let target_id = target_id.trim();
    if target_id.is_empty() {
        return Err("invariant rule target_id must not be empty".to_string());
    }

    if kind.eq_ignore_ascii_case("object_type_declared") {
        return Ok(ParsedInvariantRule::Object(target_id));
    }
    if kind.eq_ignore_ascii_case("link_type_declared") {
        return Ok(ParsedInvariantRule::Link(target_id));
    }
    if kind.eq_ignore_ascii_case("action_type_declared") {
        return Ok(ParsedInvariantRule::Action(target_id));
    }
    Err(format!("unsupported invariant rule kind '{kind}'"))
}

fn uri_matches_prefix(uri: &str, prefix: &str) -> bool {
    uri == prefix
        || (uri.len() > prefix.len()
            && uri.starts_with(prefix)
            && uri.as_bytes().get(prefix.len()) == Some(&b'/'))
}

#[cfg(test)]
mod tests {
    use crate::ontology::parse_schema_v1;

    use super::*;

    fn schema_raw() -> &'static str {
        r#"{
            "version": 1,
            "object_types": [
                {
                    "id": "resource_doc",
                    "uri_prefixes": ["axiom://resources/docs"],
                    "required_tags": [],
                    "allowed_scopes": ["resources"]
                },
                {
                    "id": "resource_subdoc",
                    "uri_prefixes": ["axiom://resources/docs/security"],
                    "required_tags": [],
                    "allowed_scopes": ["resources"]
                }
            ],
            "link_types": [
                {
                    "id": "depends_on",
                    "from_types": ["resource_doc", "resource_subdoc"],
                    "to_types": ["resource_doc", "resource_subdoc"],
                    "min_arity": 2,
                    "max_arity": 8,
                    "symmetric": true
                }
            ],
            "action_types": [],
            "invariants": []
        }"#
    }

    fn schema_with_actions_and_invariants_raw() -> &'static str {
        r#"{
            "version": 1,
            "object_types": [{
                "id":"resource_doc",
                "uri_prefixes":["axiom://resources/docs"],
                "allowed_scopes":["resources"]
            }],
            "link_types": [{
                "id":"depends_on",
                "from_types":["resource_doc"],
                "to_types":["resource_doc"],
                "min_arity": 2,
                "max_arity": 8,
                "symmetric": false
            }],
            "action_types": [{
                "id":"sync_doc",
                "input_contract":"json-object",
                "effects":["enqueue"],
                "queue_event_type":"semantic_scan"
            }],
            "invariants": [
                {
                    "id":"inv_object_exists",
                    "rule":"object_type_declared:resource_doc",
                    "severity":"warn",
                    "message":"resource_doc must exist"
                },
                {
                    "id":"inv_action_exists",
                    "rule":"action_type_declared:sync_doc",
                    "severity":"error",
                    "message":"sync_doc must exist"
                },
                {
                    "id":"inv_missing_link",
                    "rule":"link_type_declared:missing_link",
                    "severity":"warn",
                    "message":"missing link for test"
                },
                {
                    "id":"inv_invalid_kind",
                    "rule":"invalid_rule:resource_doc",
                    "severity":"warn",
                    "message":"invalid kind for test"
                },
                {
                    "id":"inv_invalid_severity",
                    "rule":"object_type_declared:resource_doc",
                    "severity":"critical",
                    "message":"invalid severity for test"
                }
            ]
        }"#
    }

    #[test]
    fn compile_schema_rejects_unknown_object_type_reference() {
        let raw = r#"{
            "version": 1,
            "object_types": [{
                "id":"doc",
                "uri_prefixes":["axiom://resources/docs"],
                "allowed_scopes":["resources"]
            }],
            "link_types": [{
                "id":"rel",
                "from_types":["doc"],
                "to_types":["missing"]
            }],
            "action_types": [],
            "invariants": []
        }"#;
        let parsed = parse_schema_v1(raw).expect("parse schema");
        let err = compile_schema(parsed).expect_err("compile must fail");
        assert!(matches!(err, AxiomError::OntologyViolation(_)));
    }

    #[test]
    fn compile_schema_rejects_scope_mismatch_between_prefix_and_allowed_scopes() {
        let raw = r#"{
            "version": 1,
            "object_types": [{
                "id":"agent_doc",
                "uri_prefixes":["axiom://agent/docs"],
                "allowed_scopes":["resources"]
            }],
            "link_types": [],
            "action_types": [],
            "invariants": []
        }"#;
        let parsed = parse_schema_v1(raw).expect("parse schema");
        let err = compile_schema(parsed).expect_err("compile must fail");
        assert!(matches!(err, AxiomError::OntologyViolation(_)));
    }

    #[test]
    fn validate_relation_link_prefers_longest_matching_object_prefix() {
        let parsed = parse_schema_v1(schema_raw()).expect("parse schema");
        let schema = compile_schema(parsed).expect("compile schema");

        let uri =
            AxiomUri::parse("axiom://resources/docs/security/checklist.md").expect("parse uri");
        let resolved = schema.resolve_object_type(&uri).expect("resolve type");
        assert_eq!(resolved, "resource_subdoc");
    }

    #[test]
    fn validate_relation_link_enforces_declared_link_type_and_arity() {
        let parsed = parse_schema_v1(schema_raw()).expect("parse schema");
        let schema = compile_schema(parsed).expect("compile schema");
        let a = AxiomUri::parse("axiom://resources/docs/auth.md").expect("a");
        let b = AxiomUri::parse("axiom://resources/docs/security/hardening.md").expect("b");

        validate_relation_link(&schema, "depends_on", &[a.clone(), b.clone()])
            .expect("valid relation");

        let unknown = validate_relation_link(&schema, "missing_link", &[a.clone(), b.clone()])
            .expect_err("unknown link type must fail");
        assert!(matches!(unknown, AxiomError::OntologyViolation(_)));

        let arity = validate_relation_link(&schema, "depends_on", std::slice::from_ref(&a))
            .expect_err("arity must fail");
        assert!(matches!(arity, AxiomError::OntologyViolation(_)));
    }

    #[test]
    fn validate_relation_link_fails_when_endpoint_has_no_object_type() {
        let parsed = parse_schema_v1(schema_raw()).expect("parse schema");
        let schema = compile_schema(parsed).expect("compile schema");
        let a = AxiomUri::parse("axiom://resources/docs/auth.md").expect("a");
        let b = AxiomUri::parse("axiom://user/notes/personal.md").expect("b");

        let err = validate_relation_link(&schema, "depends_on", &[a, b]).expect_err("must fail");
        assert!(matches!(err, AxiomError::OntologyViolation(_)));
    }

    #[test]
    fn ontology_contract_probe_default_schema_is_compilable() {
        let parsed = parse_schema_v1(crate::ontology::DEFAULT_ONTOLOGY_SCHEMA_V1_JSON)
            .expect("parse default schema");
        assert_eq!(parsed.version, 1);
        let compiled = compile_schema(parsed).expect("compile default schema");
        let probe_uri = AxiomUri::parse("axiom://agent/ontology/schema.v1.json").expect("uri");
        assert!(compiled.resolve_object_type_id(&probe_uri).is_none());
    }

    #[test]
    fn validate_action_request_enforces_declared_action_and_contract() {
        let parsed = parse_schema_v1(schema_with_actions_and_invariants_raw()).expect("parse");
        let schema = compile_schema(parsed).expect("compile");
        let request = OntologyActionRequestV1 {
            action_id: "sync_doc".to_string(),
            queue_event_type: "semantic_scan".to_string(),
            input: serde_json::json!({ "uri": "axiom://resources/docs/a.md" }),
        };
        let report = validate_action_request(&schema, &request).expect("validate");
        assert_eq!(report.action_id, "sync_doc");
        assert_eq!(report.queue_event_type, "semantic_scan");
        assert_eq!(report.input_contract, "json-object");
        assert_eq!(report.input_kind, OntologyJsonValueKind::Object);

        let mismatch = validate_action_request(
            &schema,
            &OntologyActionRequestV1 {
                action_id: "sync_doc".to_string(),
                queue_event_type: "embedding_upsert".to_string(),
                input: serde_json::json!({}),
            },
        )
        .expect_err("queue_event_type mismatch must fail");
        assert!(matches!(mismatch, AxiomError::OntologyViolation(_)));

        let input_kind = validate_action_request(
            &schema,
            &OntologyActionRequestV1 {
                action_id: "sync_doc".to_string(),
                queue_event_type: "semantic_scan".to_string(),
                input: serde_json::json!("not-an-object"),
            },
        )
        .expect_err("json-object contract mismatch must fail");
        assert!(matches!(input_kind, AxiomError::OntologyViolation(_)));
    }

    #[test]
    fn evaluate_invariants_reports_supported_and_failing_items() {
        let parsed = parse_schema_v1(schema_with_actions_and_invariants_raw()).expect("parse");
        let schema = compile_schema(parsed).expect("compile");
        let report = evaluate_invariants(&schema);
        assert_eq!(report.total, 5);
        assert_eq!(report.passed, 2);
        assert_eq!(report.failed, 3);
        assert!(report.items.iter().any(|item| {
            item.id == "inv_missing_link"
                && item.status == OntologyInvariantCheckStatus::Fail
                && item.failure_kind == Some(OntologyInvariantFailureKind::MissingTarget)
        }));
        assert!(report.items.iter().any(|item| {
            item.id == "inv_invalid_kind"
                && item.status == OntologyInvariantCheckStatus::Fail
                && item.failure_kind == Some(OntologyInvariantFailureKind::UnsupportedRule)
        }));
        assert!(report.items.iter().any(|item| {
            item.id == "inv_invalid_severity"
                && item.status == OntologyInvariantCheckStatus::Fail
                && item.failure_kind == Some(OntologyInvariantFailureKind::InvalidSeverity)
        }));
    }

    #[test]
    fn compile_schema_rejects_unknown_action_input_contract() {
        let raw = r#"{
            "version": 1,
            "object_types": [{
                "id":"resource_doc",
                "uri_prefixes":["axiom://resources/docs"],
                "allowed_scopes":["resources"]
            }],
            "link_types": [],
            "action_types": [{
                "id":"sync_doc",
                "input_contract":"json-schema",
                "effects":["enqueue"],
                "queue_event_type":"semantic_scan"
            }],
            "invariants": []
        }"#;
        let parsed = parse_schema_v1(raw).expect("parse");
        let err = compile_schema(parsed).expect_err("compile must fail");
        assert!(matches!(err, AxiomError::OntologyViolation(_)));
    }

    #[test]
    fn validate_action_request_accepts_json_any_contract_for_arbitrary_input_kind() {
        let raw = r#"{
            "version": 1,
            "object_types": [{
                "id":"resource_doc",
                "uri_prefixes":["axiom://resources/docs"],
                "allowed_scopes":["resources"]
            }],
            "link_types": [],
            "action_types": [{
                "id":"sync_doc",
                "input_contract":"json-any",
                "effects":["enqueue"],
                "queue_event_type":"semantic_scan"
            }],
            "invariants": []
        }"#;
        let parsed = parse_schema_v1(raw).expect("parse");
        let schema = compile_schema(parsed).expect("compile");
        let report = validate_action_request(
            &schema,
            &OntologyActionRequestV1 {
                action_id: "sync_doc".to_string(),
                queue_event_type: "semantic_scan".to_string(),
                input: serde_json::json!("free-form"),
            },
        )
        .expect("validate");
        assert_eq!(report.input_contract, "json-any");
        assert_eq!(report.input_kind, OntologyJsonValueKind::String);
    }
}
