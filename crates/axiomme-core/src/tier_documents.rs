use std::path::PathBuf;

use crate::error::{AxiomError, Result};
use crate::fs::LocalContextFs;
use crate::uri::AxiomUri;

const ABSTRACT_FILE_NAME: &str = ".abstract.md";
const OVERVIEW_FILE_NAME: &str = ".overview.md";

pub(crate) fn abstract_uri(uri: &AxiomUri) -> Result<AxiomUri> {
    uri.join(ABSTRACT_FILE_NAME)
}

pub(crate) fn overview_uri(uri: &AxiomUri) -> Result<AxiomUri> {
    uri.join(OVERVIEW_FILE_NAME)
}

pub(crate) fn abstract_path(fs: &LocalContextFs, uri: &AxiomUri) -> PathBuf {
    fs.resolve_uri(uri).join(ABSTRACT_FILE_NAME)
}

pub(crate) fn overview_path(fs: &LocalContextFs, uri: &AxiomUri) -> PathBuf {
    fs.resolve_uri(uri).join(OVERVIEW_FILE_NAME)
}

pub(crate) fn read_abstract(fs: &LocalContextFs, uri: &AxiomUri) -> Result<String> {
    let target_uri = abstract_uri(uri)?;
    if !fs.exists(&target_uri) {
        return Err(AxiomError::NotFound(format!("missing abstract for {uri}")));
    }
    if fs.is_dir(&target_uri) {
        return Err(AxiomError::Validation(format!(
            "abstract path is a directory: {uri}"
        )));
    }
    fs.read(&target_uri)
}

pub(crate) fn read_overview(fs: &LocalContextFs, uri: &AxiomUri) -> Result<String> {
    let target_uri = overview_uri(uri)?;
    if !fs.exists(&target_uri) {
        return Err(AxiomError::NotFound(format!("missing overview for {uri}")));
    }
    if fs.is_dir(&target_uri) {
        return Err(AxiomError::Validation(format!(
            "overview path is a directory: {uri}"
        )));
    }
    fs.read(&target_uri)
}

pub(crate) fn write_tiers(
    fs: &LocalContextFs,
    uri: &AxiomUri,
    abstract_md: &str,
    overview_md: &str,
    system: bool,
) -> Result<()> {
    fs.create_dir_all(uri, system)?;
    let abstract_uri = abstract_uri(uri)?;
    let overview_uri = overview_uri(uri)?;
    fs.write(&abstract_uri, abstract_md, system)?;
    fs.write(&overview_uri, overview_md, system)?;
    Ok(())
}
