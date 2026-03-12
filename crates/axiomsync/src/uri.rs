use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{AxiomError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Resources,
    User,
    Agent,
    Session,
    Temp,
    Queue,
}

impl Scope {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Resources => "resources",
            Self::User => "user",
            Self::Agent => "agent",
            Self::Session => "session",
            Self::Temp => "temp",
            Self::Queue => "queue",
        }
    }

    #[must_use]
    pub const fn is_internal(&self) -> bool {
        matches!(self, Self::Temp | Self::Queue)
    }
}

impl Display for Scope {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Scope {
    type Err = AxiomError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "resources" => Ok(Self::Resources),
            "user" => Ok(Self::User),
            "agent" => Ok(Self::Agent),
            "session" => Ok(Self::Session),
            "temp" => Ok(Self::Temp),
            "queue" => Ok(Self::Queue),
            _ => Err(AxiomError::InvalidScope(s.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AxiomUri {
    scope: Scope,
    segments: Vec<String>,
}

impl AxiomUri {
    #[must_use]
    pub const fn root(scope: Scope) -> Self {
        Self {
            scope,
            segments: Vec::new(),
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        if !value.starts_with("axiom://") {
            return Err(AxiomError::InvalidUri(value.to_string()));
        }
        let tail = &value[8..];
        if tail.is_empty() {
            return Err(AxiomError::InvalidUri(value.to_string()));
        }

        let mut parts = tail.splitn(2, '/');
        let scope_raw = parts
            .next()
            .ok_or_else(|| AxiomError::InvalidUri(value.to_string()))?;
        let scope = Scope::from_str(scope_raw)?;

        let segments = if let Some(path) = parts.next() {
            normalize_segments(path)?
        } else {
            Vec::new()
        };

        Ok(Self { scope, segments })
    }

    #[must_use]
    pub const fn scope(&self) -> Scope {
        self.scope
    }

    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    #[must_use]
    pub const fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn join(&self, child: &str) -> Result<Self> {
        let child_segments = normalize_segments(child)?;
        let mut segments = self.segments.clone();
        segments.extend(child_segments);
        Ok(Self {
            scope: self.scope,
            segments,
        })
    }

    pub fn child(&self, child: impl Into<String>) -> Result<Self> {
        self.join(&child.into())
    }

    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        if self.segments.is_empty() {
            None
        } else {
            Some(Self {
                scope: self.scope,
                segments: self.segments[..self.segments.len() - 1].to_vec(),
            })
        }
    }

    #[must_use]
    pub fn last_segment(&self) -> Option<&str> {
        self.segments.last().map(String::as_str)
    }

    #[must_use]
    pub fn starts_with(&self, other: &Self) -> bool {
        self.scope == other.scope
            && self.segments.len() >= other.segments.len()
            && self
                .segments
                .iter()
                .zip(other.segments.iter())
                .all(|(a, b)| a == b)
    }

    #[must_use]
    pub fn to_string_uri(&self) -> String {
        if self.segments.is_empty() {
            format!("axiom://{}", self.scope)
        } else {
            format!("axiom://{}/{}", self.scope, self.segments.join("/"))
        }
    }

    #[must_use]
    pub fn path(&self) -> String {
        self.segments.join("/")
    }
}

impl Display for AxiomUri {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_string_uri())
    }
}

impl FromStr for AxiomUri {
    type Err = AxiomError;

    fn from_str(s: &str) -> Result<Self> {
        Self::parse(s)
    }
}

pub(crate) fn uri_equivalent(expected: &str, actual: &str) -> bool {
    if expected == actual {
        return true;
    }

    let Ok(expected_uri) = AxiomUri::parse(expected) else {
        return false;
    };
    let Ok(actual_uri) = AxiomUri::parse(actual) else {
        return false;
    };
    if expected_uri.scope != actual_uri.scope {
        return false;
    }

    normalize_duplicate_leaf_segments(expected_uri.segments)
        == normalize_duplicate_leaf_segments(actual_uri.segments)
}

fn normalize_duplicate_leaf_segments(mut segments: Vec<String>) -> Vec<String> {
    while segments.len() >= 2 {
        let last_index = segments.len() - 1;
        if segments[last_index] != segments[last_index - 1] {
            break;
        }
        segments.pop();
    }
    segments
}

fn normalize_segments(raw_path: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for segment in raw_path.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            return Err(AxiomError::PathTraversal(raw_path.to_string()));
        }
        if segment.contains('\\') {
            return Err(AxiomError::InvalidUri(raw_path.to_string()));
        }
        out.push(segment.to_string());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_root_uri() {
        let uri = AxiomUri::parse("axiom://resources").expect("parse failed");
        assert_eq!(uri.scope(), Scope::Resources);
        assert!(uri.is_root());
        assert_eq!(uri.to_string(), "axiom://resources");
    }

    #[test]
    fn normalize_path() {
        let uri = AxiomUri::parse("axiom://resources//a///b/./c").expect("parse failed");
        assert_eq!(uri.to_string(), "axiom://resources/a/b/c");
    }

    #[test]
    fn reject_traversal() {
        let err = AxiomUri::parse("axiom://resources/a/../b").expect_err("must fail");
        assert!(matches!(err, AxiomError::PathTraversal(_)));
    }

    #[test]
    fn reject_unknown_scope() {
        let err = AxiomUri::parse("axiom://unknown/path").expect_err("must fail");
        assert!(matches!(err, AxiomError::InvalidScope(_)));
    }

    #[test]
    fn join_rejects_traversal_segments() {
        let root = AxiomUri::parse("axiom://resources").expect("parse failed");
        let err = root.join("../outside").expect_err("must fail");
        assert!(matches!(err, AxiomError::PathTraversal(_)));
    }

    #[test]
    fn join_and_parent() {
        let root = AxiomUri::parse("axiom://user").expect("parse failed");
        let child = root.join("memories/profile").expect("join failed");
        assert_eq!(child.to_string(), "axiom://user/memories/profile");
        let parent = child.parent().expect("missing parent");
        assert_eq!(parent.to_string(), "axiom://user/memories");
    }

    #[test]
    fn uri_equivalent_treats_duplicate_leaf_path_as_same_resource() {
        assert!(uri_equivalent(
            "axiom://resources/docs/guide.md",
            "axiom://resources/docs/guide.md/guide.md"
        ));
        assert!(uri_equivalent(
            "axiom://resources/docs/guide.md/guide.md",
            "axiom://resources/docs/guide.md"
        ));
    }

    #[test]
    fn uri_equivalent_rejects_different_scope_or_path() {
        assert!(!uri_equivalent(
            "axiom://resources/docs/guide.md",
            "axiom://queue/docs/guide.md/guide.md"
        ));
        assert!(!uri_equivalent(
            "axiom://resources/docs/guide.md",
            "axiom://resources/docs/other.md"
        ));
    }
}
