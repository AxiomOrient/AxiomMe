use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct LoadMarkdownQuery {
    pub uri: String,
}

#[derive(Debug, Deserialize)]
pub struct LoadDocumentQuery {
    pub uri: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveMarkdownRequest {
    pub uri: String,
    pub content: String,
    pub expected_etag: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PreviewRequest {
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct PreviewResponse {
    pub html: String,
}

#[derive(Debug, Serialize)]
pub struct WebDocumentResponse {
    pub uri: String,
    pub content: String,
    pub etag: String,
    pub updated_at: String,
    pub format: String,
    pub editable: bool,
}

#[derive(Debug, Deserialize)]
pub struct FsListQuery {
    pub uri: String,
    pub recursive: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct FsTreeQuery {
    pub uri: String,
}

#[derive(Debug, Deserialize)]
pub struct FsMkdirRequest {
    pub uri: String,
}

#[derive(Debug, Deserialize)]
pub struct FsMoveRequest {
    pub from_uri: String,
    pub to_uri: String,
}

#[derive(Debug, Deserialize)]
pub struct FsDeleteRequest {
    pub uri: String,
    pub recursive: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct FsListResponse {
    pub uri: String,
    pub entries: Vec<axiomme_core::models::Entry>,
}

#[derive(Debug, Serialize)]
pub struct FsMutationResponse {
    pub status: String,
    pub uri: Option<String>,
    pub from_uri: Option<String>,
    pub to_uri: Option<String>,
}
