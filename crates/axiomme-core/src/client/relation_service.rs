use crate::error::{AxiomError, Result};
use crate::models::{ContextHit, FindResult, RelationLink, RelationSummary};
use crate::uri::AxiomUri;

use super::AxiomMe;

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

        let normalized_uris = uris
            .into_iter()
            .map(|uri| AxiomUri::parse(&uri).map(|parsed| parsed.to_string()))
            .collect::<Result<Vec<_>>>()?;

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
    ) -> Result<()> {
        self.enrich_hits_with_relations(&mut result.query_results, max_per_hit)?;
        self.enrich_hits_with_relations(&mut result.memories, max_per_hit)?;
        self.enrich_hits_with_relations(&mut result.resources, max_per_hit)?;
        self.enrich_hits_with_relations(&mut result.skills, max_per_hit)?;
        Ok(())
    }

    fn enrich_hits_with_relations(
        &self,
        hits: &mut [ContextHit],
        max_per_hit: usize,
    ) -> Result<()> {
        for hit in hits {
            hit.relations = self.collect_relations_for_hit(&hit.uri, max_per_hit)?;
        }
        Ok(())
    }

    fn collect_relations_for_hit(
        &self,
        hit_uri: &str,
        max_per_hit: usize,
    ) -> Result<Vec<RelationSummary>> {
        if max_per_hit == 0 {
            return Ok(Vec::new());
        }

        let parsed = AxiomUri::parse(hit_uri)?;
        let hit_uri = parsed.to_string();
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
        let mut seen = std::collections::HashSet::<String>::new();

        for owner in owner_candidates {
            let relations = match self.fs.read_relations(&owner) {
                Ok(items) => items,
                Err(AxiomError::Validation(_)) => continue,
                Err(err) => return Err(err),
            };
            for relation in relations {
                if !relation.uris.iter().any(|uri| uri == &hit_uri) {
                    continue;
                }
                for related in &relation.uris {
                    if related == &hit_uri {
                        continue;
                    }
                    let key = format!("{}|{}", related, relation.reason);
                    if seen.insert(key) {
                        out.push(RelationSummary {
                            uri: related.clone(),
                            reason: relation.reason.clone(),
                        });
                    }
                }
            }
        }

        out.sort_by(|a, b| a.uri.cmp(&b.uri).then_with(|| a.reason.cmp(&b.reason)));
        out.truncate(max_per_hit);
        Ok(out)
    }
}
