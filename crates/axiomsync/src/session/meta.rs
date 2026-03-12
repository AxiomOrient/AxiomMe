use std::fs;

use crate::error::Result;
use crate::models::SessionMeta;

use super::Session;

impl Session {
    pub(super) fn read_meta(&self) -> Result<SessionMeta> {
        let content = fs::read_to_string(self.meta_path()?)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub(super) fn touch_meta<F>(&self, mutate: F) -> Result<()>
    where
        F: FnOnce(&mut SessionMeta),
    {
        let mut meta = self.read_meta()?;
        mutate(&mut meta);
        fs::write(self.meta_path()?, serde_json::to_string_pretty(&meta)?)?;
        Ok(())
    }
}
