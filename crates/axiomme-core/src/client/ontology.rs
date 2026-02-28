use crate::error::Result;
use crate::ontology::{
    OntologyActionRequestV1, OntologyActionValidationReport, compile_schema, parse_schema_v1,
    validate_action_request,
};
use crate::uri::AxiomUri;

use super::AxiomMe;

impl AxiomMe {
    pub fn enqueue_ontology_action(
        &self,
        schema_uri: &str,
        target_uri: &str,
        action_id: &str,
        queue_event_type: &str,
        input: serde_json::Value,
    ) -> Result<(i64, String, OntologyActionValidationReport)> {
        let raw = self.read(schema_uri)?;
        let parsed = parse_schema_v1(&raw)?;
        let compiled = compile_schema(parsed)?;
        let request = OntologyActionRequestV1 {
            action_id: action_id.to_string(),
            queue_event_type: queue_event_type.to_string(),
            input: input.clone(),
        };
        let report = validate_action_request(&compiled, &request)?;

        let target_uri = AxiomUri::parse(target_uri)?.to_string();
        let event_id = self.state.enqueue(
            report.queue_event_type.as_str(),
            target_uri.as_str(),
            serde_json::json!({
                "schema_version": 1,
                "action_id": report.action_id.clone(),
                "input": input,
            }),
        )?;

        Ok((event_id, target_uri, report))
    }
}
