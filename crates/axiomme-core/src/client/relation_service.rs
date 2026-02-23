use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::Arc;

use crate::error::{AxiomError, Result};
use crate::models::{ContextHit, FindResult, RelationLink, RelationSummary};
use crate::ontology::{
    CompiledOntologySchema, ONTOLOGY_SCHEMA_URI_V1, compile_schema, parse_schema_v1,
    validate_relation_link,
};
use crate::uri::AxiomUri;

use super::{AxiomMe, OntologySchemaCacheEntry, OntologySchemaFingerprint};

impl AxiomMe {
    pub fn relations(&self, owner_uri: &str) -> Result<Vec<RelationLink>> {
        let owner = AxiomUri::parse(owner_uri)?;
        self.fs.read_relations(&owner)
    }

    pub fn link(
        &self,
        owner_uri: &str,
        relation_id: &str,
        uris: Vec<String>,
        reason: &str,
    ) -> Result<RelationLink> {
        let owner = AxiomUri::parse(owner_uri)?;
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
        self.maybe_validate_relation_link_ontology(relation_id, &parsed_uris)?;
        let normalized_uris = parsed_uris.iter().map(ToString::to_string).collect();

        let next = RelationLink {
            id: relation_id.to_string(),
            uris: normalized_uris,
            reason: reason.to_string(),
        };

        let mut existing = self.fs.read_relations(&owner)?;
        if let Some(record) = existing.iter_mut().find(|record| record.id == next.id) {
            *record = next.clone();
        } else {
            existing.push(next.clone());
        }
        self.fs.write_relations(&owner, &existing, false)?;
        Ok(next)
    }

    pub fn unlink(&self, owner_uri: &str, relation_id: &str) -> Result<bool> {
        let owner = AxiomUri::parse(owner_uri)?;
        let relation_id = relation_id.trim();
        if relation_id.is_empty() {
            return Err(AxiomError::Validation(
                "relation id must not be empty".to_string(),
            ));
        }

        let mut existing = self.fs.read_relations(&owner)?;
        let before = existing.len();
        existing.retain(|record| record.id != relation_id);
        if existing.len() == before {
            return Ok(false);
        }
        self.fs.write_relations(&owner, &existing, false)?;
        Ok(true)
    }

    pub(super) fn enrich_find_result_relations(
        &self,
        result: &mut FindResult,
        max_per_hit: usize,
        typed_edge_enrichment: bool,
    ) -> Result<()> {
        let ontology_schema = if typed_edge_enrichment {
            self.load_relation_ontology_schema()?
        } else {
            None
        };
        let ontology_schema = ontology_schema.as_deref();
        let mut owner_relations_cache = HashMap::<AxiomUri, Arc<Vec<RelationLink>>>::new();
        self.enrich_hits_with_relations(
            &mut result.query_results,
            max_per_hit,
            ontology_schema,
            &mut owner_relations_cache,
        )?;
        self.enrich_hits_with_relations(
            &mut result.memories,
            max_per_hit,
            ontology_schema,
            &mut owner_relations_cache,
        )?;
        self.enrich_hits_with_relations(
            &mut result.resources,
            max_per_hit,
            ontology_schema,
            &mut owner_relations_cache,
        )?;
        self.enrich_hits_with_relations(
            &mut result.skills,
            max_per_hit,
            ontology_schema,
            &mut owner_relations_cache,
        )?;
        Ok(())
    }

    fn enrich_hits_with_relations(
        &self,
        hits: &mut [ContextHit],
        max_per_hit: usize,
        ontology_schema: Option<&CompiledOntologySchema>,
        owner_relations_cache: &mut HashMap<AxiomUri, Arc<Vec<RelationLink>>>,
    ) -> Result<()> {
        for hit in hits {
            hit.relations = self.collect_relations_for_hit(
                &hit.uri,
                max_per_hit,
                ontology_schema,
                owner_relations_cache,
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
    ) -> Result<Vec<RelationSummary>> {
        if max_per_hit == 0 {
            return Ok(Vec::new());
        }

        let parsed = AxiomUri::parse(hit_uri)?;
        let hit_uri = parsed.to_string();
        let source_object_type = ontology_schema
            .and_then(|schema| schema.resolve_object_type_id(&parsed))
            .map(ToString::to_string);
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
                        let target_object_type = ontology_schema
                            .and_then(|schema| {
                                AxiomUri::parse(related)
                                    .ok()
                                    .and_then(|uri| schema.resolve_object_type_id(&uri))
                            })
                            .map(ToString::to_string);
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

        let loaded = match self.fs.read_relations(owner) {
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
