use std::collections::HashSet;

use crate::error::{AxiomError, Result};
use crate::fs::LocalContextFs;
use crate::models::RelationLink;
use crate::uri::AxiomUri;

const RELATIONS_FILE_NAME: &str = ".relations.json";

fn relations_uri(owner: &AxiomUri) -> Result<AxiomUri> {
    owner.join(RELATIONS_FILE_NAME)
}

fn ensure_owner_is_directory(fs: &LocalContextFs, owner: &AxiomUri) -> Result<()> {
    let base = fs.resolve_uri(owner);
    if base.exists() && !base.is_dir() {
        return Err(AxiomError::Validation(format!(
            "relations owner must be directory: {owner}"
        )));
    }
    Ok(())
}

pub(crate) fn read_relations(fs: &LocalContextFs, owner: &AxiomUri) -> Result<Vec<RelationLink>> {
    ensure_owner_is_directory(fs, owner)?;
    let relation_uri = relations_uri(owner)?;
    if !fs.exists(&relation_uri) {
        return Ok(Vec::new());
    }
    let raw = fs.read(&relation_uri)?;
    let relations = serde_json::from_str::<Vec<RelationLink>>(&raw)
        .map_err(|err| AxiomError::Validation(format!("invalid relations schema: {err}")))?;
    validate_relations(&relations)?;
    Ok(relations)
}

pub(crate) fn write_relations(
    fs: &LocalContextFs,
    owner: &AxiomUri,
    relations: &[RelationLink],
    system: bool,
) -> Result<()> {
    ensure_owner_is_directory(fs, owner)?;
    validate_relations(relations)?;
    let mut canonical = relations.to_vec();
    canonical.sort_by(|a, b| a.id.cmp(&b.id));
    let payload = serde_json::to_string_pretty(&canonical)
        .map_err(|err| AxiomError::Validation(format!("invalid relations payload: {err}")))?;
    let relation_uri = relations_uri(owner)?;
    fs.write(&relation_uri, &payload, system)
}

pub(crate) fn validate_relations(relations: &[RelationLink]) -> Result<()> {
    let mut ids = HashSet::<String>::new();
    for relation in relations {
        let id = relation.id.trim();
        if id.is_empty() {
            return Err(AxiomError::Validation(
                "relation id must not be empty".to_string(),
            ));
        }
        if !ids.insert(id.to_string()) {
            return Err(AxiomError::Validation(format!(
                "duplicate relation id: {id}"
            )));
        }

        if relation.reason.trim().is_empty() {
            return Err(AxiomError::Validation(format!(
                "relation reason must not be empty: {id}"
            )));
        }

        if relation.uris.len() < 2 {
            return Err(AxiomError::Validation(format!(
                "relation must include at least 2 uris: {id}"
            )));
        }

        let mut unique_uris = HashSet::<String>::new();
        for uri in &relation.uris {
            let parsed = AxiomUri::parse(uri).map_err(|err| {
                AxiomError::Validation(format!("invalid relation uri in {id}: {err}"))
            })?;
            if !unique_uris.insert(parsed.to_string()) {
                return Err(AxiomError::Validation(format!(
                    "duplicate uri in relation {id}: {uri}"
                )));
            }
        }
    }
    Ok(())
}
