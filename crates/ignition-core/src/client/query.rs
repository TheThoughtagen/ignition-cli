//! The standard 8.3 list envelope + query (02-RESEARCH §Verified Endpoint
//! Catalog): every list-capable endpoint takes the same
//! `limit/offset/sortBy/search/filter` params and answers
//! `{items, metadata}` — ONE generic pair covers them all.
//!
//! `limit = -1` is the UI's "everything" convention (observed: unset
//! limit behaves as -1). Serde is deliberately tolerant: `metadata` may
//! carry extra keys (e.g. `metrics`) and omit fields — no
//! `deny_unknown_fields` anywhere, every metadata field carries
//! `#[serde(default)]`.

use serde::{Deserialize, Serialize};

/// The standard query params every 8.3 list endpoint accepts.
///
/// `Default` is the UI convention: `limit = -1` (all items), `offset = 0`,
/// no sort/search/filter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListQuery {
    /// Max items; `-1` = all (the gateway UI's convention).
    pub limit: i64,
    /// Skip the first `offset` items.
    pub offset: i64,
    /// Server-side sort, when used.
    pub sort_by: Option<String>,
    /// Server-side substring search, when used.
    pub search: Option<String>,
    /// Server-side filter expression, when used.
    pub filter: Option<String>,
}

impl Default for ListQuery {
    fn default() -> Self {
        Self {
            limit: -1,
            offset: 0,
            sort_by: None,
            search: None,
            filter: None,
        }
    }
}

impl ListQuery {
    /// Serialize into query pairs — `limit`/`offset` always present
    /// (explicit beats the gateway's unset-default ambiguity), optional
    /// keys only when `Some` (absent optionals are skipped, never sent
    /// as `null`/empty).
    pub fn to_query_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::with_capacity(5);
        pairs.push(("limit".to_string(), self.limit.to_string()));
        pairs.push(("offset".to_string(), self.offset.to_string()));
        if let Some(sort_by) = &self.sort_by {
            pairs.push(("sortBy".to_string(), sort_by.clone()));
        }
        if let Some(search) = &self.search {
            pairs.push(("search".to_string(), search.clone()));
        }
        if let Some(filter) = &self.filter {
            pairs.push(("filter".to_string(), filter.clone()));
        }
        pairs
    }
}

/// The `{items, metadata}` envelope every 8.3 list endpoint answers with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListEnvelope<T> {
    /// The page of items.
    pub items: Vec<T>,
    /// Pagination metadata.
    pub metadata: ListMetadata,
}

/// The `metadata` block of the list envelope (tolerant: fields may be
/// absent, extra keys like `metrics` are ignored).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListMetadata {
    /// Total items the collection holds.
    #[serde(default)]
    pub total: i64,
    /// Items matching the current query.
    #[serde(default)]
    pub matching: i64,
    /// Effective limit (`-1` = unlimited).
    #[serde(default)]
    pub limit: i64,
    /// Offset the page starts at.
    #[serde(default)]
    pub offset: i64,
}

#[cfg(test)]
mod tests {
    use super::{ListEnvelope, ListMetadata, ListQuery};

    /// Default = the UI convention; query pairs always carry limit=-1 and
    /// skip absent optionals entirely.
    #[test]
    fn query_pairs_skip_absent_optionals() {
        let pairs = ListQuery::default().to_query_pairs();
        assert_eq!(
            pairs,
            vec![
                ("limit".to_string(), "-1".to_string()),
                ("offset".to_string(), "0".to_string()),
            ],
            "default carries only limit=-1 and offset=0"
        );

        let full = ListQuery {
            limit: 200,
            offset: 40,
            sort_by: Some("timestamp".into()),
            search: Some("GatewayManager".into()),
            filter: None,
        };
        let pairs = full.to_query_pairs();
        assert_eq!(
            pairs,
            vec![
                ("limit".to_string(), "200".to_string()),
                ("offset".to_string(), "40".to_string()),
                ("sortBy".to_string(), "timestamp".to_string()),
                ("search".to_string(), "GatewayManager".to_string()),
            ],
            "present optionals serialize under their gateway-native names; filter skipped"
        );
    }

    /// The envelope tolerates extra metadata keys (live bodies carry
    /// `metrics`) and absent metadata fields.
    #[test]
    fn envelope_is_serde_tolerant() {
        let body = serde_json::json!({
            "items": [{"id": "mod-1"}],
            "metadata": {
                "total": 368,
                "matching": 368,
                "limit": -1,
                "offset": 0,
                "metrics": {"elapsedMs": 12}
            }
        });
        let page: ListEnvelope<serde_json::Value> =
            serde_json::from_value(body).expect("extra `metrics` key is tolerated");
        assert_eq!(page.items.len(), 1);
        assert_eq!(
            page.metadata,
            ListMetadata {
                total: 368,
                matching: 368,
                limit: -1,
                offset: 0,
            }
        );

        let sparse = serde_json::json!({"items": [], "metadata": {}});
        let page: ListEnvelope<serde_json::Value> =
            serde_json::from_value(sparse).expect("absent metadata fields default");
        assert_eq!(page.metadata.total, 0);
    }
}
