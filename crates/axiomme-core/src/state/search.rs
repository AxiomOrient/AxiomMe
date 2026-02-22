use chrono::Utc;
use rusqlite::{OptionalExtension, params, types::Type};

use crate::error::Result;
use crate::mime::infer_mime;
use crate::models::IndexRecord;

use super::SqliteStateStore;

impl SqliteStateStore {
    pub fn list_search_documents(&self) -> Result<Vec<IndexRecord>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                r"
                SELECT
                  d.uri,
                  d.parent_uri,
                  d.is_leaf,
                  d.context_type,
                  d.name,
                  d.abstract_text,
                  d.content,
                  d.updated_at,
                  d.depth,
                  COALESCE(
                    (
                        SELECT group_concat(tag, ' ')
                        FROM search_doc_tags t
                        WHERE t.doc_id = d.id
                    ),
                    d.tags_text,
                    ''
                  ) AS tags_text
                FROM search_docs d
                ORDER BY d.depth ASC, d.uri ASC
                ",
            )?;
            let rows = stmt.query_map([], |row| {
                let uri = row.get::<_, String>(0)?;
                let updated_raw = row.get::<_, String>(7)?;
                let updated_at = parse_required_rfc3339(7, &updated_raw)?;
                let tags = row
                    .get::<_, String>(9)?
                    .split_whitespace()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();

                Ok(IndexRecord {
                    id: blake3::hash(uri.as_bytes()).to_hex().to_string(),
                    uri,
                    parent_uri: row.get(1)?,
                    is_leaf: row.get::<_, i64>(2)? != 0,
                    context_type: row.get(3)?,
                    name: row.get(4)?,
                    abstract_text: row.get(5)?,
                    content: row.get(6)?,
                    tags,
                    updated_at,
                    depth: i64_to_usize_saturating(row.get::<_, i64>(8)?),
                })
            })?;

            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    pub fn clear_search_index(&self) -> Result<()> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM search_doc_tags", [])?;
            conn.execute("DELETE FROM search_docs", [])?;
            Ok(())
        })
    }

    pub fn upsert_search_document(&self, record: &IndexRecord) -> Result<()> {
        let tags = normalize_tags(&record.tags);
        let tags_text = tags.join(" ");
        let mime = infer_mime(record);
        self.with_tx(|tx| {
            tx.execute(
                r"
                INSERT INTO search_docs(
                    uri, parent_uri, is_leaf, context_type, name, abstract_text, content,
                    tags_text, mime, updated_at, depth
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ON CONFLICT(uri) DO UPDATE SET
                  parent_uri=excluded.parent_uri,
                  is_leaf=excluded.is_leaf,
                  context_type=excluded.context_type,
                  name=excluded.name,
                  abstract_text=excluded.abstract_text,
                  content=excluded.content,
                  tags_text=excluded.tags_text,
                  mime=excluded.mime,
                  updated_at=excluded.updated_at,
                  depth=excluded.depth
                ",
                params![
                    record.uri.as_str(),
                    record.parent_uri.as_deref(),
                    bool_to_i64(record.is_leaf),
                    record.context_type.as_str(),
                    record.name.as_str(),
                    record.abstract_text.as_str(),
                    record.content.as_str(),
                    tags_text,
                    mime,
                    record.updated_at.to_rfc3339(),
                    usize_to_i64_saturating(record.depth),
                ],
            )?;

            let doc_id: i64 = tx.query_row(
                "SELECT id FROM search_docs WHERE uri = ?1",
                params![record.uri],
                |row| row.get(0),
            )?;

            tx.execute(
                "DELETE FROM search_doc_tags WHERE doc_id = ?1",
                params![doc_id],
            )?;
            for tag in &tags {
                tx.execute(
                    "INSERT OR IGNORE INTO search_doc_tags(doc_id, tag) VALUES (?1, ?2)",
                    params![doc_id, tag],
                )?;
            }

            Ok(())
        })
    }

    pub fn remove_search_document(&self, uri: &str) -> Result<()> {
        self.with_tx(|tx| {
            let doc_id = tx
                .query_row(
                    "SELECT id FROM search_docs WHERE uri = ?1",
                    params![uri],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;

            if let Some(doc_id) = doc_id {
                tx.execute(
                    "DELETE FROM search_doc_tags WHERE doc_id = ?1",
                    params![doc_id],
                )?;
                tx.execute("DELETE FROM search_docs WHERE id = ?1", params![doc_id])?;
            }

            Ok(())
        })
    }

    pub fn remove_search_documents_with_prefix(&self, uri_prefix: &str) -> Result<()> {
        self.with_tx(|tx| {
            let mut stmt = tx.prepare(
                r"
                SELECT id FROM search_docs
                WHERE uri = ?1 OR uri LIKE ?2
                ",
            )?;
            let rows = stmt.query_map(params![uri_prefix, format!("{uri_prefix}/%")], |row| {
                row.get::<_, i64>(0)
            })?;
            let mut doc_ids = Vec::<i64>::new();
            for row in rows {
                doc_ids.push(row?);
            }
            drop(stmt);

            for doc_id in doc_ids {
                tx.execute(
                    "DELETE FROM search_doc_tags WHERE doc_id = ?1",
                    params![doc_id],
                )?;
                tx.execute("DELETE FROM search_docs WHERE id = ?1", params![doc_id])?;
            }

            Ok(())
        })
    }
}

fn parse_required_rfc3339(idx: usize, raw: &str) -> rusqlite::Result<chrono::DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|x| x.with_timezone(&Utc))
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(idx, Type::Text, Box::new(err)))
}

const fn bool_to_i64(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn usize_to_i64_saturating(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn i64_to_usize_saturating(value: i64) -> usize {
    if value <= 0 {
        0
    } else {
        usize::try_from(value).unwrap_or(usize::MAX)
    }
}

fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut out = tags
        .iter()
        .map(|x| x.trim().to_lowercase())
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}
