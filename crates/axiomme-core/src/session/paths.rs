use std::path::PathBuf;

use crate::error::Result;
use crate::uri::{AxiomUri, Scope};

use super::Session;

impl Session {
    pub(super) fn session_uri(&self) -> Result<AxiomUri> {
        AxiomUri::root(Scope::Session).join(&self.session_id)
    }

    pub(super) fn messages_path(&self) -> Result<PathBuf> {
        Ok(self
            .fs
            .resolve_uri(&self.session_uri()?)
            .join("messages.jsonl"))
    }

    pub(super) fn meta_path(&self) -> Result<PathBuf> {
        Ok(self.fs.resolve_uri(&self.session_uri()?).join(".meta.json"))
    }

    pub(super) fn relations_path(&self) -> Result<PathBuf> {
        Ok(self
            .fs
            .resolve_uri(&self.session_uri()?)
            .join(".relations.json"))
    }
}
