use std::fmt::{Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{AxiomError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    pub fn as_str(&self) -> &'static str {
        match self {
            Scope::Resources => "resources",
            Scope::User => "user",
            Scope::Agent => "agent",
            Scope::Session => "session",
            Scope::Temp => "temp",
            Scope::Queue => "queue",
        }
    }

    pub fn is_internal(&self) -> bool {
        matches!(self, Scope::Temp | Scope::Queue)
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
            "resources" => Ok(Scope::Resources),
            "user" => Ok(Scope::User),
            "agent" => Ok(Scope::Agent),
            "session" => Ok(Scope::Session),
            "temp" => Ok(Scope::Temp),
            "queue" => Ok(Scope::Queue),
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
    pub fn root(scope: Scope) -> Self {
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

    pub fn scope(&self) -> Scope {
        self.scope.clone()
    }

    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn join(&self, child: &str) -> Result<Self> {
        let child_segments = normalize_segments(child)?;
        let mut segments = self.segments.clone();
        segments.extend(child_segments);
        Ok(Self {
            scope: self.scope.clone(),
            segments,
        })
    }

    pub fn child(&self, child: impl Into<String>) -> Result<Self> {
        self.join(&child.into())
    }

    pub fn parent(&self) -> Option<Self> {
        if self.segments.is_empty() {
            None
        } else {
            Some(Self {
                scope: self.scope.clone(),
                segments: self.segments[..self.segments.len() - 1].to_vec(),
            })
        }
    }

    pub fn last_segment(&self) -> Option<&str> {
        self.segments.last().map(String::as_str)
    }

    pub fn starts_with(&self, other: &Self) -> bool {
        self.scope == other.scope
            && self.segments.len() >= other.segments.len()
            && self
                .segments
                .iter()
                .zip(other.segments.iter())
                .all(|(a, b)| a == b)
    }

    pub fn to_string_uri(&self) -> String {
        if self.segments.is_empty() {
            format!("axiom://{}", self.scope)
        } else {
            format!("axiom://{}/{}", self.scope, self.segments.join("/"))
        }
    }

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
}
