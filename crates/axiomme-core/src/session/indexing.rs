use std::path::Path;

use chrono::Utc;
use uuid::Uuid;

use crate::error::Result;
use crate::fs::LocalContextFs;
use crate::index::InMemoryIndex;
use crate::uri::{AxiomUri, Scope};

pub(super) fn ensure_directory_record(
    fs: &LocalContextFs,
    index: &mut InMemoryIndex,
    uri: &AxiomUri,
) -> Result<()> {
    if index.get(&uri.to_string()).is_some() {
        return Ok(());
    }

    let path = fs.resolve_uri(uri);
    if !Path::new(&path).exists() {
        fs.create_dir_all(uri, true)?;
    }

    if !fs.abstract_path(uri).exists() || !fs.overview_path(uri).exists() {
        fs.write_tiers(
            uri,
            &format!(
                "Directory {}",
                uri.last_segment().unwrap_or_else(|| uri.scope().as_str())
            ),
            &format!("# Overview\n\nURI: {uri}"),
        )?;
    }

    let abstract_text = fs.read_abstract(uri)?;
    let overview_text = fs.read_overview(uri)?;

    index.upsert(crate::models::IndexRecord {
        id: Uuid::new_v4().to_string(),
        uri: uri.to_string(),
        parent_uri: uri.parent().map(|p| p.to_string()),
        is_leaf: false,
        context_type: if matches!(uri.scope(), Scope::User | Scope::Agent) {
            "memory".to_string()
        } else {
            "resource".to_string()
        },
        name: uri
            .last_segment()
            .unwrap_or_else(|| uri.scope().as_str())
            .to_string(),
        abstract_text,
        content: overview_text,
        tags: vec![],
        updated_at: Utc::now(),
        depth: uri.segments().len(),
    });

    Ok(())
}
