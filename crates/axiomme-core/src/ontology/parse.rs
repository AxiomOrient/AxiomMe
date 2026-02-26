use crate::error::{AxiomError, Result};

use super::model::OntologySchemaV1;

pub fn parse_schema_v1(raw: &str) -> Result<OntologySchemaV1> {
    serde_json::from_str::<OntologySchemaV1>(raw).map_err(|err| {
        AxiomError::OntologyViolation(format!("ontology schema parse failed: {err}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_schema_v1_rejects_unknown_fields() {
        let raw = r#"{
            "version": 1,
            "object_types": [],
            "link_types": [],
            "action_types": [],
            "invariants": [],
            "unknown": true
        }"#;
        let err = parse_schema_v1(raw).expect_err("unknown fields must fail");
        assert!(matches!(err, AxiomError::OntologyViolation(_)));
    }
}
