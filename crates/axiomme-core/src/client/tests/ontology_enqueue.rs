use super::*;

fn write_schema_with_enqueue_action(app: &AxiomMe) {
    let schema_uri =
        AxiomUri::parse(crate::ontology::ONTOLOGY_SCHEMA_URI_V1).expect("parse schema uri");
    app.fs
        .write(
            &schema_uri,
            r#"{
              "version": 1,
              "object_types": [
                {
                  "id": "resource_doc",
                  "uri_prefixes": ["axiom://resources/docs"],
                  "allowed_scopes": ["resources"]
                }
              ],
              "link_types": [],
              "action_types": [
                {
                  "id": "sync_doc",
                  "input_contract": "json-object",
                  "effects": ["enqueue"],
                  "queue_event_type": "semantic_scan"
                }
              ],
              "invariants": []
            }"#,
            true,
        )
        .expect("write schema");
}

#[test]
fn enqueue_ontology_action_moves_validation_and_queue_write_into_core_api() {
    let temp = tempdir().expect("tempdir");
    let app = AxiomMe::new(temp.path()).expect("app");
    app.initialize().expect("initialize");
    write_schema_with_enqueue_action(&app);

    let (event_id, target_uri, report) = app
        .enqueue_ontology_action(
            crate::ontology::ONTOLOGY_SCHEMA_URI_V1,
            "axiom://resources/docs/a.md",
            "sync_doc",
            "semantic_scan",
            serde_json::json!({
                "uri": "axiom://resources/docs/a.md"
            }),
        )
        .expect("enqueue ontology action");

    assert!(event_id > 0, "event id must be positive row id");
    assert_eq!(target_uri, "axiom://resources/docs/a.md");
    assert_eq!(report.action_id, "sync_doc");
    assert_eq!(report.queue_event_type, "semantic_scan");

    let queued = app
        .state
        .get_outbox_event(event_id)
        .expect("load queued event")
        .expect("queued event exists");
    assert_eq!(queued.event_type, "semantic_scan");
    assert_eq!(queued.uri, "axiom://resources/docs/a.md");

    let payload = queued.payload_json.as_object().expect("payload object");
    assert_eq!(payload.get("schema_version"), Some(&serde_json::json!(1)));
    assert_eq!(
        payload.get("action_id"),
        Some(&serde_json::json!("sync_doc"))
    );
}
