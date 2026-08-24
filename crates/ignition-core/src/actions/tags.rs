//! Tag actions (05-04, TAGS-01..04) — serde models OUT, no printing.
//!
//! TWO seams, one family (05-RESEARCH's crisp split):
//!
//! - **Providers** (TAGS-01) ride the NATIVE config-resource REST
//!   (`ignition/tag-provider` — healthier data: tagCount metrics,
//!   healthchecks, no deployed route needed). Delete is the
//!   find→signature→delete chain; a find miss refuses with the
//!   family-specific `provider_not_found` (exit 6) instead of a
//!   bare 404. The CLI layer guards the destructive verb
//!   (`--yes`, pre-resolution — the LOCKED shape).
//! - **browse/read/write** (TAGS-02/03/04) ride the deployed
//!   `tags` WebDev route through the 05-03 generic
//!   [`GatewayApi::webdev_route_call`] — every one runs the
//!   version precondition first (the 05-03 `webdev_precondition`):
//!   absent/mismatched routes refuse exit 6 naming
//!   `ign webdev deploy`. (Lands with the webdev seam half of this
//!   plan.)
//!
//! Two-layer naming: the client models stay wire-faithful; the
//! action results re-expose selected fields under unit-explicit
//! keys (`tag_count`, …) — the LOCKED convention.

use serde::Serialize;

use crate::client::GatewayApi;
use crate::client::query::ListQuery;
use crate::client::tags::{TagProviderCreate, TagProviderRecord};
use crate::error::CoreError;

/// `ign tags provider list` row — unit-explicit keys, ALL keys
/// always.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TagProviderRow {
    /// Provider name.
    pub name: String,
    /// Whether the provider resource is enabled.
    pub enabled: bool,
    /// `metrics.tagCount` when the gateway reported one.
    pub tag_count: Option<i64>,
    /// `healthchecks.status` when the gateway reported one.
    pub health: Option<String>,
    /// Gateway-managed providers (the built-in `System` provider —
    /// MANAGED-type; not user-deletable surface).
    pub managed: bool,
}

/// `ign tags provider list` result.
#[derive(Debug, Serialize)]
pub struct TagProvidersResult {
    /// One row per provider, gateway order.
    pub providers: Vec<TagProviderRow>,
}

/// `ign tags provider create` result — the Ok classification IS the
/// success contract (the project-create precedent: create response
/// bodies are opaque).
#[derive(Debug, Serialize)]
pub struct TagProviderCreateResult {
    /// The provider name created.
    pub name: String,
}

/// `ign tags provider delete` result.
#[derive(Debug, Serialize)]
pub struct TagProviderDeleteResult {
    /// The provider name deleted.
    pub deleted: String,
}

/// Map one wire record onto the unit-explicit row (two-layer
/// naming: raw pointers into the passthrough `metrics`/
/// `healthchecks`/`config` values, never interpreted).
fn provider_row(record: &TagProviderRecord) -> TagProviderRow {
    let tag_count = record.metrics.pointer("/tagCount").and_then(|v| v.as_i64());
    let health = record
        .healthchecks
        .pointer("/status")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    // The built-in System provider is MANAGED-type (research); a
    // MANAGED profile type flags it regardless of name.
    let managed = record.name == "System"
        || record
            .config
            .pointer("/profile/type")
            .and_then(|v| v.as_str())
            == Some("MANAGED");
    TagProviderRow {
        name: record.name.clone(),
        enabled: record.enabled,
        tag_count,
        health,
        managed,
    }
}

/// `ign tags provider list` — the native resource list (no deployed
/// routes involved).
pub async fn tag_provider_list(api: &dyn GatewayApi) -> Result<TagProvidersResult, CoreError> {
    let page = api.tag_provider_list(&ListQuery::default()).await?;
    Ok(TagProvidersResult {
        providers: page.items.iter().map(provider_row).collect(),
    })
}

/// `ign tags provider create NAME` — STANDARD profile only at MVP
/// (the create body is the fixed live-proven shape; DB-backed
/// providers are out of scope, README documents).
pub async fn tag_provider_create(
    api: &dyn GatewayApi,
    name: &str,
) -> Result<TagProviderCreateResult, CoreError> {
    api.tag_provider_create(&[TagProviderCreate::standard(name)])
        .await?;
    Ok(TagProviderCreateResult {
        name: name.to_string(),
    })
}

/// `ign tags provider delete NAME` — the find→signature→delete
/// chain: find carries the record (and its mutation signature);
/// delete embeds both on the path. A find miss refuses with the
/// family-specific `provider_not_found` (exit 6) over the bare 404.
pub async fn tag_provider_delete(
    api: &dyn GatewayApi,
    name: &str,
) -> Result<TagProviderDeleteResult, CoreError> {
    let record = api.tag_provider_find(name).await.map_err(|err| match err {
        CoreError::NotFound { .. } => CoreError::ProviderNotFound {
            name: name.to_string(),
            endpoint: err.endpoint(),
        },
        other => other,
    })?;
    let signature = record.signature.clone().ok_or_else(|| {
        CoreError::Internal(format!(
            "tag provider {name:?} find record carried no signature — \
             the delete chain needs one (unexpected wire shape)"
        ))
    })?;
    api.tag_provider_delete(name, &signature).await?;
    Ok(TagProviderDeleteResult {
        deleted: name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        TagProviderRecord, TagProvidersResult, TagProviderRow, provider_row, tag_provider_delete,
        tag_provider_list,
    };
    use crate::client::GatewayApi;
    use crate::client::query::{ListEnvelope, ListMetadata, ListQuery};
    use crate::client::tags::TagProviderCreate;
    use crate::error::CoreError;

    use std::sync::Mutex;

    /// A scripted double: the provider methods answer from fixtures
    /// (recorded so the chain can be asserted). Everything else is
    /// unreachable (the established action-double shape).
    struct TagsRig {
        providers: Vec<TagProviderRecord>,
        found: Mutex<Vec<String>>,
        deleted: Mutex<Vec<(String, String)>>,
        created: Mutex<Vec<serde_json::Value>>,
    }

    impl TagsRig {
        fn with(providers: Vec<TagProviderRecord>) -> Self {
            Self {
                providers,
                found: Mutex::new(Vec::new()),
                deleted: Mutex::new(Vec::new()),
                created: Mutex::new(Vec::new()),
            }
        }
    }

    fn record(name: &str, tag_count: i64, signature: Option<&str>) -> TagProviderRecord {
        TagProviderRecord {
            name: name.to_string(),
            enabled: true,
            config: serde_json::json!({"profile": {"type": "STANDARD"}}),
            metrics: serde_json::json!({"tagCount": tag_count}),
            healthchecks: serde_json::json!({"status": "OK"}),
            signature: signature.map(str::to_string),
            extra: Default::default(),
        }
    }

    fn page(items: Vec<TagProviderRecord>) -> ListEnvelope<TagProviderRecord> {
        let total = items.len() as i64;
        ListEnvelope {
            items,
            metadata: ListMetadata {
                total,
                matching: total,
                limit: -1,
                offset: 0,
            },
        }
    }

    #[async_trait::async_trait]
    impl GatewayApi for TagsRig {
        async fn tag_provider_list(
            &self,
            _query: &ListQuery,
        ) -> Result<ListEnvelope<TagProviderRecord>, CoreError> {
            Ok(page(self.providers.clone()))
        }
        async fn tag_provider_find(&self, name: &str) -> Result<TagProviderRecord, CoreError> {
            self.found.lock().unwrap().push(name.to_string());
            self.providers
                .iter()
                .find(|record| record.name == name)
                .cloned()
                .ok_or(CoreError::NotFound { endpoint: None })
        }
        async fn tag_provider_create(
            &self,
            body: &[TagProviderCreate],
        ) -> Result<(), CoreError> {
            let mut created = self.created.lock().unwrap();
            for record in body {
                created.push(serde_json::to_value(record).expect("serializes"));
            }
            Ok(())
        }
        async fn tag_provider_delete(
            &self,
            name: &str,
            signature: &str,
        ) -> Result<(), CoreError> {
            self.deleted
                .lock()
                .unwrap()
                .push((name.to_string(), signature.to_string()));
            Ok(())
        }
        async fn gateway_info(&self) -> Result<crate::client::version::GatewayInfo, CoreError> {
            unreachable!("not part of this action")
        }
        async fn overview(&self) -> Result<crate::client::status::Overview, CoreError> {
            unreachable!("not part of this action")
        }
        async fn status_ping(&self) -> Result<crate::client::status::StatusPing, CoreError> {
            unreachable!("not part of this action")
        }
        async fn modules(
            &self,
            _quarantined: bool,
            _query: &ListQuery,
        ) -> Result<ListEnvelope<crate::client::status::ModuleInfo>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn metrics_current(
            &self,
        ) -> Result<crate::client::metrics::CurrentGauges, CoreError> {
            unreachable!("not part of this action")
        }
        async fn metrics_historic(
            &self,
        ) -> Result<crate::client::metrics::PerformanceCharts, CoreError> {
            unreachable!("not part of this action")
        }
        async fn metrics_threads(&self) -> Result<crate::client::metrics::ThreadCounts, CoreError> {
            unreachable!("not part of this action")
        }
        async fn designers(
            &self,
            _query: &ListQuery,
        ) -> Result<ListEnvelope<crate::client::sessions::DesignerInfo>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn perspective_sessions(
            &self,
            _query: &ListQuery,
        ) -> Result<ListEnvelope<crate::client::sessions::PerspectiveSession>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn vision_clients(
            &self,
            _query: &ListQuery,
        ) -> Result<ListEnvelope<crate::client::sessions::VisionClient>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn terminate_perspective_session(
            &self,
            _id: &str,
            _message: Option<&str>,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn terminate_vision_client(&self, _id: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn prune_designer(&self, _id: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn database_connections(
            &self,
        ) -> Result<ListEnvelope<crate::client::connections::GatewayConnection>, CoreError>
        {
            unreachable!("not part of this action")
        }
        async fn opc_connections(
            &self,
        ) -> Result<ListEnvelope<crate::client::connections::GatewayConnection>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn logs(
            &self,
            _filter: &crate::client::logs::LogQuery,
        ) -> Result<ListEnvelope<crate::client::logs::LogEntry>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn logs_download(&self) -> Result<crate::client::logs::LogDownload, CoreError> {
            unreachable!("not part of this action")
        }
        async fn loggers(
            &self,
            _query: &ListQuery,
        ) -> Result<ListEnvelope<crate::client::logs::LoggerInfo>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn set_logger_level(&self, _logger: &str, _level: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn reset_logger_levels(&self) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn restart(&self) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn scan_projects(&self) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn security_properties(
            &self,
        ) -> Result<crate::client::restart::SecurityProperties, CoreError> {
            unreachable!("not part of this action")
        }
        async fn webdev_route_status(&self, _route: &str) -> Result<u16, CoreError> {
            unreachable!("not part of this action")
        }
        async fn webdev_route_call(
            &self,
            _project: &str,
            _route: &str,
            _body: &serde_json::Value,
            _extra_headers: &[(&str, &str)],
        ) -> Result<serde_json::Value, CoreError> {
            unreachable!("not part of this action")
        }
        async fn webdev_route_probe(
            &self,
            _project: &str,
            _route: &str,
            _extra_headers: &[(&str, &str)],
        ) -> Result<crate::client::webdev::RouteProbe, CoreError> {
            unreachable!("not part of this action")
        }
        async fn projects(
            &self,
            _query: &ListQuery,
        ) -> Result<ListEnvelope<crate::client::projects::ProjectRecord>, CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_find(
            &self,
            _name: &str,
        ) -> Result<crate::client::projects::ProjectRecord, CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_create(
            &self,
            _body: &crate::client::projects::ProjectCreate,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_copy(&self, _from: &str, _to: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_rename(&self, _name: &str, _new_name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_modify(
            &self,
            _name: &str,
            _body: &crate::client::projects::ProjectModify,
        ) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_delete(&self, _name: &str) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_export_to_file(
            &self,
            _name: &str,
            _out: &std::path::Path,
        ) -> Result<crate::client::projects::ExportMeta, CoreError> {
            unreachable!("not part of this action")
        }
        async fn project_import(
            &self,
            _name: &str,
            _zip: Vec<u8>,
            _overwrite: bool,
        ) -> Result<crate::client::projects::ImportOutcome, CoreError> {
            unreachable!("not part of this action")
        }
        async fn trial_status_wire(&self) -> Result<crate::client::trial::TrialWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn banners(&self) -> Result<crate::client::trial::BannerSet, CoreError> {
            unreachable!("not part of this action")
        }
        async fn trial_reset_wire(&self) -> Result<crate::client::trial::TrialWire, CoreError> {
            unreachable!("not part of this action")
        }
        async fn backup_download(
            &self,
            _out: &std::path::Path,
        ) -> Result<crate::client::projects::ExportMeta, CoreError> {
            unreachable!("not part of this action")
        }
        async fn backup_restore(&self, _gwbk: &std::path::Path) -> Result<(), CoreError> {
            unreachable!("not part of this action")
        }
    }

    /// The unit-explicit row mapping: tagCount/health pointered out
    /// of the passthrough values; System (and MANAGED-type
    /// providers) flagged managed.
    #[test]
    fn provider_rows_map_unit_explicit_keys() {
        let mut system = record("System", 3, None);
        system.config = serde_json::json!({"profile": {"type": "MANAGED"}});
        let rows: Vec<TagProviderRow> = [record("default", 12, None), system]
            .iter()
            .map(provider_row)
            .collect();
        assert_eq!(
            rows[0],
            TagProviderRow {
                name: "default".into(),
                enabled: true,
                tag_count: Some(12),
                health: Some("OK".into()),
                managed: false,
            }
        );
        assert!(rows[1].managed, "System is MANAGED-type");
        // A MANAGED profile type flags managed regardless of name.
        let mut managed = record("weird", 0, None);
        managed.config = serde_json::json!({"profile": {"type": "MANAGED"}});
        assert!(provider_row(&managed).managed);
        // Missing metrics/health degrade to None — all keys still
        // present.
        let bare = TagProviderRecord {
            name: "bare".into(),
            enabled: false,
            config: serde_json::Value::Null,
            metrics: serde_json::Value::Null,
            healthchecks: serde_json::Value::Null,
            signature: None,
            extra: Default::default(),
        };
        let row = provider_row(&bare);
        assert_eq!(row.tag_count, None);
        assert_eq!(row.health, None);
    }

    /// The delete chain: find → signature → delete, with the
    /// family-specific refusal when find misses.
    #[tokio::test]
    async fn provider_delete_chains_find_signature_delete() {
        let rig = TagsRig::with(vec![record("default", 12, Some("sig-42"))]);
        let result = tag_provider_delete(&rig, "default")
            .await
            .expect("chain deletes");
        assert_eq!(result.deleted, "default");
        assert_eq!(
            *rig.deleted.lock().unwrap(),
            vec![("default".into(), "sig-42".into())]
        );

        let rig = TagsRig::with(Vec::new());
        let err = tag_provider_delete(&rig, "ghost")
            .await
            .expect_err("find miss refuses");
        assert_eq!(err.code(), "provider_not_found");
        assert_eq!(err.exit_code(), 6);
        assert!(
            err.hint().unwrap().contains("ign tags provider list"),
            "hint names the fix: {err}"
        );
        assert!(rig.deleted.lock().unwrap().is_empty(), "zero deletes ran");
    }

    /// `tags provider list` maps every record into the agent shape.
    #[tokio::test]
    async fn provider_list_maps_rows() {
        let rig = TagsRig::with(vec![
            record("default", 12, None),
            record("System", 0, None),
        ]);
        let result: TagProvidersResult = tag_provider_list(&rig).await.expect("lists");
        assert_eq!(result.providers.len(), 2);
        assert_eq!(result.providers[1].name, "System");
        assert!(result.providers[1].managed);
    }
}
