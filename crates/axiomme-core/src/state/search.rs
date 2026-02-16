use chrono::Utc;
use rusqlite::{
    OptionalExtension, params, params_from_iter,
    types::{Type, Value},
};
use std::collections::HashSet;
use std::fmt::Write as _;

use crate::error::Result;
use crate::mime::infer_mime;
use crate::models::{IndexRecord, SearchFilter};

use super::{SqliteSearchHit, SqliteStateStore};

struct RawSearchRow {
    uri: String,
    context_type: String,
    abstract_text: String,
    content: Option<String>,
    rank: f64,
}

const MAX_NORMALIZED_FTS_TOKENS: usize = 48;

struct FtsQueryPlan {
    sql: String,
    values: Vec<Value>,
}

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
            conn.execute("DELETE FROM search_docs_fts", [])?;
            conn.execute("DELETE FROM search_docs", [])?;
            let _ = conn.execute(
                "INSERT INTO search_docs_fts(search_docs_fts) VALUES('optimize')",
                [],
            );
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

            tx.execute(
                "DELETE FROM search_docs_fts WHERE rowid = ?1",
                params![doc_id],
            )?;
            tx.execute(
                "INSERT INTO search_docs_fts(rowid, name, abstract_text, content, tags_text) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    doc_id,
                    record.name.as_str(),
                    record.abstract_text.as_str(),
                    record.content.as_str(),
                    tags.join(" ")
                ],
            )?;
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
                tx.execute(
                    "DELETE FROM search_docs_fts WHERE rowid = ?1",
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
                tx.execute(
                    "DELETE FROM search_docs_fts WHERE rowid = ?1",
                    params![doc_id],
                )?;
                tx.execute("DELETE FROM search_docs WHERE id = ?1", params![doc_id])?;
            }

            Ok(())
        })
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "sqlite retrieval keeps all query knobs explicit for deterministic call sites"
    )]
    pub fn search_documents_fts(
        &self,
        query: &str,
        target_prefix: Option<&str>,
        filter: Option<&SearchFilter>,
        max_depth: Option<usize>,
        query_normalizer_enabled: bool,
        limit: usize,
        min_match_tokens: Option<usize>,
    ) -> Result<Vec<SqliteSearchHit>> {
        let query_tokens = normalized_query_tokens(query);
        let min_match_tokens = resolve_min_match_tokens(min_match_tokens, query_tokens.len());
        let fts_query = normalize_fts_query(query, query_normalizer_enabled);
        if fts_query.is_empty() {
            return Ok(Vec::new());
        }
        let include_content = min_match_tokens.is_some();
        let query_plan = build_fts_query_plan(
            fts_query,
            target_prefix,
            filter,
            max_depth,
            limit,
            include_content,
        )?;

        self.with_conn(|conn| {
            let mut stmt = conn.prepare(&query_plan.sql)?;
            let mut raw = Vec::<RawSearchRow>::new();
            if include_content {
                let rows = stmt.query_map(params_from_iter(query_plan.values.iter()), |row| {
                    Ok(RawSearchRow {
                        uri: row.get::<_, String>(0)?,
                        context_type: row.get::<_, String>(1)?,
                        abstract_text: row.get::<_, String>(2)?,
                        content: row.get::<_, Option<String>>(3)?,
                        rank: row.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
                    })
                })?;
                for row in rows {
                    raw.push(row?);
                }
            } else {
                let rows = stmt.query_map(params_from_iter(query_plan.values.iter()), |row| {
                    Ok(RawSearchRow {
                        uri: row.get::<_, String>(0)?,
                        context_type: row.get::<_, String>(1)?,
                        abstract_text: row.get::<_, String>(2)?,
                        content: None,
                        rank: row.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
                    })
                })?;
                for row in rows {
                    raw.push(row?);
                }
            }
            if let Some(min_match_tokens) = min_match_tokens {
                raw.retain(|row| query_token_match_count(row, &query_tokens) >= min_match_tokens);
            }
            Ok(normalize_ranked_rows(raw))
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

fn normalize_ranked_rows(raw: Vec<RawSearchRow>) -> Vec<SqliteSearchHit> {
    if raw.is_empty() {
        return Vec::new();
    }

    let best_rank = raw
        .iter()
        .map(|row| row.rank)
        .filter(|rank| rank.is_finite())
        .fold(f64::INFINITY, f64::min);

    let mut out = Vec::with_capacity(raw.len());
    for row in raw {
        let score = normalize_sqlite_rank_score(row.rank, best_rank);
        out.push(SqliteSearchHit {
            uri: row.uri,
            score,
            context_type: row.context_type,
            abstract_text: row.abstract_text,
        });
    }
    out
}

const MIN_SQLITE_FTS_SCORE: f32 = 1e-4;

fn normalize_sqlite_rank_score(rank: f64, best_rank: f64) -> f32 {
    if !rank.is_finite() || !best_rank.is_finite() {
        return MIN_SQLITE_FTS_SCORE;
    }

    let delta = (rank - best_rank).max(0.0);
    let normalized = 1.0 / (1.0 + delta);
    f64_to_f32_saturating(normalized).max(MIN_SQLITE_FTS_SCORE)
}

fn push_sql_fragment(sql: &mut String, args: std::fmt::Arguments<'_>) -> Result<()> {
    sql.write_fmt(args).map_err(|_| {
        crate::error::AxiomError::Internal("failed to build FTS SQL query".to_string())
    })
}

fn build_fts_query_plan(
    fts_query: String,
    target_prefix: Option<&str>,
    filter: Option<&SearchFilter>,
    max_depth: Option<usize>,
    limit: usize,
    include_content: bool,
) -> Result<FtsQueryPlan> {
    let select_sql = if include_content {
        r"
            SELECT
              d.uri,
              d.context_type,
              d.abstract_text,
              d.content,
              bm25(search_docs_fts) AS rank
            FROM search_docs_fts
            JOIN search_docs d ON d.id = search_docs_fts.rowid
            WHERE search_docs_fts MATCH ?1
            "
    } else {
        r"
            SELECT
              d.uri,
              d.context_type,
              d.abstract_text,
              bm25(search_docs_fts) AS rank
            FROM search_docs_fts
            JOIN search_docs d ON d.id = search_docs_fts.rowid
            WHERE search_docs_fts MATCH ?1
            "
    };
    let mut sql = String::from(select_sql);
    let mut values = vec![Value::Text(fts_query)];
    let mut param_idx = 2usize;

    if let Some(prefix) = target_prefix {
        push_sql_fragment(
            &mut sql,
            format_args!(
                " AND (d.uri = ?{param_idx} OR d.uri LIKE ?{})",
                param_idx + 1
            ),
        )?;
        values.push(Value::Text(prefix.to_string()));
        values.push(Value::Text(format!("{prefix}/%")));
        param_idx += 2;
    }

    if let Some(mime) = filter
        .and_then(|x| x.mime.as_ref())
        .map(|x| x.trim().to_lowercase())
        .filter(|x| !x.is_empty())
    {
        push_sql_fragment(&mut sql, format_args!(" AND d.mime = ?{param_idx}"))?;
        values.push(Value::Text(mime));
        param_idx += 1;
    }

    if let Some(filter) = filter {
        for tag in normalize_tags(&filter.tags) {
            push_sql_fragment(
                &mut sql,
                format_args!(
                    " AND EXISTS (SELECT 1 FROM search_doc_tags t WHERE t.doc_id = d.id AND t.tag = ?{param_idx})"
                ),
            )?;
            values.push(Value::Text(tag));
            param_idx += 1;
        }
    }

    if let Some(max_depth) = max_depth {
        push_sql_fragment(&mut sql, format_args!(" AND d.depth <= ?{param_idx}"))?;
        values.push(Value::Integer(usize_to_i64_saturating(max_depth)));
        param_idx += 1;
    }

    push_sql_fragment(
        &mut sql,
        format_args!(" ORDER BY rank ASC, d.uri ASC LIMIT ?{param_idx}"),
    )?;
    values.push(Value::Integer(usize_to_i64_saturating(limit.max(1))));

    Ok(FtsQueryPlan { sql, values })
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "value is clamped to f32 bounds before conversion"
)]
fn f64_to_f32_saturating(value: f64) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    let bounded = value.clamp(f64::from(f32::MIN), f64::from(f32::MAX));
    bounded as f32
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

fn normalized_query_tokens(raw: &str) -> Vec<String> {
    let mut seen = HashSet::<String>::new();
    let mut out = Vec::<String>::new();
    for token in crate::embedding::tokenize_vec(raw) {
        let normalized = token.trim().to_lowercase();
        if normalized.is_empty() || !seen.insert(normalized.clone()) {
            continue;
        }
        out.push(normalized);
    }
    out
}

fn resolve_min_match_tokens(requested: Option<usize>, query_token_count: usize) -> Option<usize> {
    let requested = requested.filter(|value| *value > 1)?;
    if query_token_count == 0 {
        return None;
    }
    let effective = requested.min(query_token_count);
    (effective > 1).then_some(effective)
}

fn query_token_match_count(row: &RawSearchRow, query_tokens: &[String]) -> usize {
    if query_tokens.is_empty() {
        return 0;
    }

    let mut doc_tokens = crate::embedding::tokenize_vec(&row.abstract_text)
        .into_iter()
        .map(|token| token.trim().to_lowercase())
        .filter(|token| !token.is_empty())
        .collect::<HashSet<_>>();
    if let Some(content) = row.content.as_deref() {
        doc_tokens.extend(
            crate::embedding::tokenize_vec(content)
                .into_iter()
                .map(|token| token.trim().to_lowercase())
                .filter(|token| !token.is_empty()),
        );
    }

    query_tokens
        .iter()
        .filter(|token| doc_tokens.contains(token.as_str()))
        .count()
}

fn normalize_fts_query(raw: &str, query_normalizer_enabled: bool) -> String {
    let mut tokens = crate::embedding::tokenize_vec(raw);
    if tokens.is_empty() {
        return String::new();
    }

    if query_normalizer_enabled {
        tokens = normalize_fts_tokens(tokens);
    }

    tokens.join(" OR ")
}

#[cfg(test)]
fn query_normalizer_enabled_from(raw: Option<&str>) -> bool {
    !matches!(
        raw.map(|x| x.trim().to_ascii_lowercase()).as_deref(),
        Some("off" | "none" | "0" | "false")
    )
}

fn normalize_fts_tokens(tokens: Vec<String>) -> Vec<String> {
    let mut out = Vec::<String>::new();

    for token in tokens {
        push_normalized_token(&mut out, token.as_str(), MAX_NORMALIZED_FTS_TOKENS);
        for alias in lexical_aliases(token.as_str()) {
            push_normalized_token(&mut out, alias, MAX_NORMALIZED_FTS_TOKENS);
        }
        if out.len() >= MAX_NORMALIZED_FTS_TOKENS {
            break;
        }
    }
    out
}

fn push_normalized_token(out: &mut Vec<String>, token: &str, max_tokens: usize) {
    if out.len() >= max_tokens {
        return;
    }
    let normalized = token.trim().to_lowercase();
    if normalized.is_empty() {
        return;
    }
    if !out.iter().any(|value| value == &normalized) {
        out.push(normalized);
    }
}

fn lexical_aliases(token: &str) -> &'static [&'static str] {
    match token.trim().to_ascii_lowercase().as_str() {
        // auth / identity family
        "auth" | "oauth" | "authentication" | "authorize" | "authorization" | "login"
        | "signin" | "identity" | "token" | "인증" | "로그인" | "토큰" => &[
            "auth",
            "oauth",
            "authentication",
            "authorization",
            "login",
            "token",
            "인증",
        ],

        // system/runtime terms used in this project
        "queue" | "worker" | "replay" | "큐" | "재생" => &["queue", "worker", "replay", "큐"],
        "benchmark" | "perf" | "latency" | "벤치마크" | "성능" => {
            &["benchmark", "perf", "latency", "벤치마크", "성능"]
        }
        "session" | "memory" | "archive" | "세션" | "메모리" => {
            &["session", "memory", "archive", "세션", "메모리"]
        }

        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::{
        RawSearchRow, normalize_fts_tokens, query_normalizer_enabled_from, query_token_match_count,
        resolve_min_match_tokens,
    };

    #[test]
    fn fts_query_normalizer_can_be_disabled() {
        assert!(!query_normalizer_enabled_from(Some("off")));
        assert!(!query_normalizer_enabled_from(Some("false")));
        assert!(!query_normalizer_enabled_from(Some("none")));
        assert!(query_normalizer_enabled_from(None));
        assert!(query_normalizer_enabled_from(Some("basic")));
    }

    #[test]
    fn fts_query_expands_multilingual_aliases() {
        let tokens = normalize_fts_tokens(vec!["인증".to_string()]);
        assert!(tokens.iter().any(|x| x == "인증"));
        assert!(tokens.iter().any(|x| x == "auth"));
        assert!(tokens.iter().any(|x| x == "oauth"));
        assert!(tokens.iter().any(|x| x == "token"));
    }

    #[test]
    fn fts_query_expands_benchmark_aliases() {
        let tokens = normalize_fts_tokens(vec!["벤치마크".to_string()]);
        assert!(tokens.iter().any(|x| x == "benchmark"));
        assert!(tokens.iter().any(|x| x == "perf"));
        assert!(tokens.iter().any(|x| x == "latency"));
    }

    #[test]
    fn min_match_tokens_clamps_to_query_token_count() {
        assert_eq!(resolve_min_match_tokens(Some(2), 3), Some(2));
        assert_eq!(resolve_min_match_tokens(Some(99), 3), Some(3));
        assert_eq!(resolve_min_match_tokens(Some(2), 1), None);
        assert_eq!(resolve_min_match_tokens(Some(1), 3), None);
        assert_eq!(resolve_min_match_tokens(None, 3), None);
        assert_eq!(resolve_min_match_tokens(Some(2), 0), None);
    }

    #[test]
    fn query_token_match_count_uses_abstract_and_content_tokens() {
        let row = RawSearchRow {
            uri: "axiom://resources/a.md".to_string(),
            context_type: "resource".to_string(),
            abstract_text: "oauth flow".to_string(),
            content: Some("token exchange callback".to_string()),
            rank: 0.1,
        };
        let query_tokens = vec![
            "oauth".to_string(),
            "callback".to_string(),
            "missing".to_string(),
        ];
        assert_eq!(query_token_match_count(&row, &query_tokens), 2);
    }
}
