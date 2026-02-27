use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::Arc;

use crate::error::{AxiomError, Result};
use crate::models::{ContextHit, FindResult, RelationLink, RelationSummary};
use crate::ontology::{
    CompiledOntologySchema, ONTOLOGY_SCHEMA_URI_V1, compile_schema, parse_schema_v1,
    validate_relation_link,
};
use crate::relation_documents::{read_relations, write_relations};
use crate::uri::AxiomUri;

use super::{AxiomMe, OntologySchemaCacheEntry, OntologySchemaFingerprint};

impl AxiomMe {
    pub fn relations(&self, owner_uri: &str) -> Result<Vec<RelationLink>> {
        let owner = AxiomUri::parse(owner_uri)?;
        validate_relation_owner_scope(&owner)?;
        read_relations(&self.fs, &owner)
    }

    pub fn link(
        &self,
        owner_uri: &str,
        relation_id: &str,
        uris: Vec<String>,
        reason: &str,
    ) -> Result<RelationLink> {
        let owner = AxiomUri::parse(owner_uri)?;
        validate_relation_owner_scope(&owner)?;
        let relation_id = relation_id.trim();
        if relation_id.is_empty() {
            return Err(AxiomError::Validation(
                "relation id must not be empty".to_string(),
            ));
        }
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(AxiomError::Validation(
                "relation reason must not be empty".to_string(),
            ));
        }

        let parsed_uris = uris
            .into_iter()
            .map(|uri| AxiomUri::parse(&uri))
            .collect::<Result<Vec<_>>>()?;
        let parsed_uris = dedupe_relation_uris(parsed_uris);
        if parsed_uris.len() < 2 {
            return Err(AxiomError::Validation(
                "relation link requires at least two unique uris".to_string(),
            ));
        }
        self.maybe_validate_relation_link_ontology(relation_id, &parsed_uris)?;
        for uri in &parsed_uris {
            if !uri.starts_with(&owner) {
                return Err(AxiomError::Validation(format!(
                    "relation uri must be within owner subtree: owner={owner}, uri={uri}"
                )));
            }
        }
        let normalized_uris = parsed_uris.iter().map(ToString::to_string).collect();

        let next = RelationLink {
            id: relation_id.to_string(),
            uris: normalized_uris,
            reason: reason.to_string(),
        };

        let mut existing = read_relations(&self.fs, &owner)?;
        if let Some(record) = existing.iter_mut().find(|record| record.id == next.id) {
            *record = next.clone();
        } else {
            existing.push(next.clone());
        }
        write_relations(&self.fs, &owner, &existing, false)?;
        Ok(next)
    }

    pub fn unlink(&self, owner_uri: &str, relation_id: &str) -> Result<bool> {
        let owner = AxiomUri::parse(owner_uri)?;
        validate_relation_owner_scope(&owner)?;
        let relation_id = relation_id.trim();
        if relation_id.is_empty() {
            return Err(AxiomError::Validation(
                "relation id must not be empty".to_string(),
            ));
        }

        let mut existing = read_relations(&self.fs, &owner)?;
        let before = existing.len();
        existing.retain(|record| record.id != relation_id);
        if existing.len() == before {
            return Ok(false);
        }
        write_relations(&self.fs, &owner, &existing, false)?;
        Ok(true)
    }

    pub(super) fn enrich_find_result_relations(
        &self,
        result: &mut FindResult,
        max_per_hit: usize,
        typed_edge_enrichment: bool,
    ) -> Result<()> {
        let ontology_schema = if typed_edge_enrichment {
            self.load_relation_ontology_schema_for_enrichment()?
        } else {
            None
        };
        let ontology_schema = ontology_schema.as_deref();
        let mut owner_relations_cache = HashMap::<AxiomUri, Arc<Vec<RelationLink>>>::new();
        let mut object_type_cache = HashMap::<String, Option<String>>::new();
        self.enrich_hits_with_relations(
            &mut result.query_results,
            max_per_hit,
            ontology_schema,
            &mut owner_relations_cache,
            &mut object_type_cache,
        )?;
        Ok(())
    }

    fn load_relation_ontology_schema_for_enrichment(
        &self,
    ) -> Result<Option<Arc<crate::ontology::CompiledOntologySchema>>> {
        match self.load_relation_ontology_schema() {
            Ok(schema) => Ok(schema),
            // Relation enrichment is an optional read path. If ontology schema
            // is malformed, preserve base retrieval behavior instead of failing
            // the whole find/search request.
            Err(AxiomError::OntologyViolation(_)) => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn enrich_hits_with_relations(
        &self,
        hits: &mut [ContextHit],
        max_per_hit: usize,
        ontology_schema: Option<&CompiledOntologySchema>,
        owner_relations_cache: &mut HashMap<AxiomUri, Arc<Vec<RelationLink>>>,
        object_type_cache: &mut HashMap<String, Option<String>>,
    ) -> Result<()> {
        for hit in hits {
            hit.relations = self.collect_relations_for_hit(
                &hit.uri,
                max_per_hit,
                ontology_schema,
                owner_relations_cache,
                object_type_cache,
            )?;
        }
        Ok(())
    }

    fn collect_relations_for_hit(
        &self,
        hit_uri: &str,
        max_per_hit: usize,
        ontology_schema: Option<&CompiledOntologySchema>,
        owner_relations_cache: &mut HashMap<AxiomUri, Arc<Vec<RelationLink>>>,
        object_type_cache: &mut HashMap<String, Option<String>>,
    ) -> Result<Vec<RelationSummary>> {
        if max_per_hit == 0 {
            return Ok(Vec::new());
        }

        let parsed = AxiomUri::parse(hit_uri)?;
        let hit_uri = parsed.to_string();
        let source_object_type = if let Some(schema) = ontology_schema {
            if let Some(cached) = object_type_cache.get(hit_uri.as_str()) {
                cached.clone()
            } else {
                let resolved = schema
                    .resolve_object_type_id(&parsed)
                    .map(ToString::to_string);
                object_type_cache.insert(hit_uri.clone(), resolved.clone());
                resolved
            }
        } else {
            None
        };
        let mut owner_candidates = Vec::new();
        if self.fs.is_dir(&parsed) {
            owner_candidates.push(parsed.clone());
        }
        let mut cursor = parsed.parent();
        while let Some(parent) = cursor {
            owner_candidates.push(parent.clone());
            cursor = parent.parent();
        }
        let mut out = Vec::<RelationSummary>::new();
        let mut seen = HashSet::<String>::new();

        for owner in owner_candidates {
            let relations = self.load_owner_relations_cached(&owner, owner_relations_cache)?;
            for relation in relations.iter() {
                if !relation.uris.iter().any(|uri| uri == &hit_uri) {
                    continue;
                }
                for related in &relation.uris {
                    if related == &hit_uri {
                        continue;
                    }
                    let key = format!("{}|{}|{}", related, relation.id, relation.reason);
                    if seen.insert(key) {
                        let relation_type = ontology_schema
                            .and_then(|schema| schema.link_type(&relation.id))
                            .map(|def| def.id.clone());
                        let target_object_type = resolve_object_type_id_cached(
                            ontology_schema,
                            related,
                            object_type_cache,
                        );
                        out.push(RelationSummary {
                            uri: related.clone(),
                            reason: relation.reason.clone(),
                            relation_type,
                            source_object_type: source_object_type.clone(),
                            target_object_type,
                        });
                    }
                }
            }
        }

        out.sort_by(|a, b| a.uri.cmp(&b.uri).then_with(|| a.reason.cmp(&b.reason)));
        out.truncate(max_per_hit);
        Ok(out)
    }

    fn load_owner_relations_cached(
        &self,
        owner: &AxiomUri,
        owner_relations_cache: &mut HashMap<AxiomUri, Arc<Vec<RelationLink>>>,
    ) -> Result<Arc<Vec<RelationLink>>> {
        if let Some(cached) = owner_relations_cache.get(owner) {
            return Ok(Arc::clone(cached));
        }

        let loaded = match read_relations(&self.fs, owner) {
            Ok(items) => items,
            Err(AxiomError::Validation(_)) => Vec::new(),
            Err(err) => return Err(err),
        };
        let loaded = Arc::new(loaded);
        owner_relations_cache.insert(owner.clone(), Arc::clone(&loaded));
        Ok(loaded)
    }

    fn maybe_validate_relation_link_ontology(
        &self,
        relation_id: &str,
        uris: &[AxiomUri],
    ) -> Result<()> {
        let Some(compiled) = self.load_relation_ontology_schema()? else {
            return Ok(());
        };
        validate_relation_link(compiled.as_ref(), relation_id, uris)
    }

    fn load_relation_ontology_schema(
        &self,
    ) -> Result<Option<Arc<crate::ontology::CompiledOntologySchema>>> {
        let schema_uri = AxiomUri::parse(ONTOLOGY_SCHEMA_URI_V1).map_err(|err| {
            AxiomError::Internal(format!("invalid ontology schema URI constant: {err}"))
        })?;
        let Some(fingerprint) = self.read_ontology_schema_fingerprint(&schema_uri)? else {
            self.clear_cached_ontology_schema()?;
            return Ok(None);
        };
        if let Some(cached) = self.lookup_cached_ontology_schema(fingerprint)? {
            return Ok(Some(cached));
        }

        let raw = self.fs.read(&schema_uri)?;
        let parsed = parse_schema_v1(&raw)?;
        let compiled = Arc::new(compile_schema(parsed)?);
        self.store_cached_ontology_schema(fingerprint, Arc::clone(&compiled))?;
        Ok(Some(compiled))
    }

    fn read_ontology_schema_fingerprint(
        &self,
        schema_uri: &AxiomUri,
    ) -> Result<Option<OntologySchemaFingerprint>> {
        if !self.fs.exists(schema_uri) {
            return Ok(None);
        }
        let schema_path = self.fs.resolve_uri(schema_uri);
        let metadata = fs::metadata(&schema_path)?;
        Ok(Some(OntologySchemaFingerprint {
            modified: metadata.modified().ok(),
            len: metadata.len(),
        }))
    }

    fn lookup_cached_ontology_schema(
        &self,
        fingerprint: OntologySchemaFingerprint,
    ) -> Result<Option<Arc<crate::ontology::CompiledOntologySchema>>> {
        let cache = self
            .ontology_schema_cache
            .read()
            .map_err(|_| AxiomError::lock_poisoned("ontology schema cache"))?;
        Ok(cache.as_ref().and_then(|entry| {
            if entry.fingerprint == fingerprint {
                Some(Arc::clone(&entry.compiled))
            } else {
                None
            }
        }))
    }

    fn store_cached_ontology_schema(
        &self,
        fingerprint: OntologySchemaFingerprint,
        compiled: Arc<crate::ontology::CompiledOntologySchema>,
    ) -> Result<()> {
        let mut cache = self
            .ontology_schema_cache
            .write()
            .map_err(|_| AxiomError::lock_poisoned("ontology schema cache"))?;
        *cache = Some(OntologySchemaCacheEntry {
            fingerprint,
            compiled,
        });
        Ok(())
    }

    fn clear_cached_ontology_schema(&self) -> Result<()> {
        let mut cache = self
            .ontology_schema_cache
            .write()
            .map_err(|_| AxiomError::lock_poisoned("ontology schema cache"))?;
        *cache = None;
        Ok(())
    }
}

fn dedupe_relation_uris(uris: Vec<AxiomUri>) -> Vec<AxiomUri> {
    let mut out = Vec::with_capacity(uris.len());
    let mut seen = HashSet::<String>::new();
    for uri in uris {
        let key = uri.to_string();
        if seen.insert(key) {
            out.push(uri);
        }
    }
    out
}

fn validate_relation_owner_scope(owner: &AxiomUri) -> Result<()> {
    if owner.scope().is_internal() {
        return Err(AxiomError::PermissionDenied(format!(
            "internal scope is read-only for relation operations: {owner}"
        )));
    }
    Ok(())
}

fn resolve_object_type_id_cached(
    ontology_schema: Option<&CompiledOntologySchema>,
    uri: &str,
    object_type_cache: &mut HashMap<String, Option<String>>,
) -> Option<String> {
    let schema = ontology_schema?;
    if let Some(cached) = object_type_cache.get(uri) {
        return cached.clone();
    }
    let resolved = AxiomUri::parse(uri)
        .ok()
        .and_then(|parsed| schema.resolve_object_type_id(&parsed))
        .map(ToString::to_string);
    object_type_cache.insert(uri.to_string(), resolved.clone());
    resolved
}
