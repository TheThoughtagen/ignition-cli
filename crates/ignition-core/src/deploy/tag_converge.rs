//! The backup's copy of a run's owned tags (Phase 26, 26-13, D-07 on pairs).

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use async_trait::async_trait;
    use serde_json::{Value, json};

    use crate::client::redundancy::RedundancyStatusWire;
    use crate::deploy::pair::{BackupPolicy, PairWaitStep, UnsyncedReason};
    use crate::error::CoreError;

    /// The master's export after a run: memory tags carrying live `value`,
    /// `timestamp` and `quality`, a UDT instance whose members carry values,
    /// a type whose member `value` is the type's default (configuration), and
    /// a node the run created.
    fn master() -> Value {
        json!({"name":"","tagType":"Provider","tags":[
            {"name":"_types_","tagType":"Folder","tags":[
                {"name":"Motor","tagType":"UdtType","tags":[
                    {"name":"Amps","tagType":"AtomicTag","dataType":"Float8","value":1.5}
                ]}
            ]},
            {"name":"Line1","tagType":"Folder","tags":[
                {"name":"T1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory",
                 "defaultValue":3,"value":9,"timestamp":1794179312490i64,"quality":"Good"},
                {"name":"T2","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory",
                 "documentation":"speed","value":4},
                {"name":"M1","tagType":"UdtInstance","typeId":"Motor","tags":[
                    {"name":"Amps","tagType":"AtomicTag","value":5,"quality":"Good"}
                ]},
                {"name":"Created","tagType":"AtomicTag","dataType":"Int4","defaultValue":7,"value":7}
            ]}
        ]})
    }

    /// The backup's export of the same configuration, as the pair capture
    /// showed it (F-P2): no live value on any AtomicTag, an instance member
    /// without its value, and children listed in another order (F-P1).
    fn backup() -> Value {
        json!({"name":"","tagType":"Provider","tags":[
            {"name":"Line1","tagType":"Folder","tags":[
                {"name":"Created","tagType":"AtomicTag","dataType":"Int4","defaultValue":7},
                {"name":"M1","tagType":"UdtInstance","typeId":"Motor","tags":[
                    {"name":"Amps","tagType":"AtomicTag"}
                ]},
                {"name":"T2","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory",
                 "documentation":"speed"},
                {"name":"T1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory",
                 "defaultValue":3}
            ]},
            {"name":"_types_","tagType":"Folder","tags":[
                {"name":"Motor","tagType":"UdtType","tags":[
                    {"name":"Amps","tagType":"AtomicTag","dataType":"Float8","value":1.5}
                ]}
            ]}
        ]})
    }

    /// `export` with `edit` applied to the node at `path` below `Line1`.
    fn edited(mut export: Value, name: &str, edit: impl FnOnce(&mut Value)) -> Value {
        let line = export["tags"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|n| n["name"] == "Line1")
            .unwrap();
        let node = line["tags"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|n| n["name"] == name)
            .unwrap();
        edit(node);
        export
    }

    #[test]
    fn exports_converged_without_values_on_every_node() {
        // F-P2: the master carries values, the backup none, created node
        // and instance members included.
        assert!(exports_converged(&master(), &backup()));
        assert!(exports_converged(&backup(), &master()));
    }

    #[test]
    fn exports_converged_keys_children_by_name() {
        // F-P1: the same children in another order.
        let mut reversed = master();
        for node in reversed["tags"].as_array_mut().unwrap() {
            if let Some(children) = node.get_mut("tags").and_then(Value::as_array_mut) {
                children.reverse();
            }
        }
        reversed["tags"].as_array_mut().unwrap().reverse();
        assert!(exports_converged(&master(), &reversed));
    }

    #[test]
    fn exports_converged_compares_configuration() {
        let doc = edited(backup(), "T2", |n| n["documentation"] = json!("other"));
        assert!(!exports_converged(&master(), &doc), "documentation");
        let default = edited(backup(), "T1", |n| n["defaultValue"] = json!(4));
        assert!(!exports_converged(&master(), &default), "defaultValue");
        let dropped = edited(backup(), "Created", |n| {
            n.as_object_mut().unwrap().remove("defaultValue");
        });
        assert!(!exports_converged(&master(), &dropped), "a dropped defaultValue");
        let mut missing = backup();
        missing["tags"][0]["tags"].as_array_mut().unwrap().remove(0);
        assert!(!exports_converged(&master(), &missing), "a missing node");
        let mut extra = backup();
        extra["tags"][0]["tags"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":"Extra","tagType":"AtomicTag"}));
        assert!(!exports_converged(&master(), &extra), "an extra node");
        // A type member's value is the type's default: configuration.
        let mut typed = backup();
        typed["tags"][1]["tags"][0]["tags"][0]["value"] = json!(2.5);
        assert!(!exports_converged(&master(), &typed), "a type member value");
        // Not a tag tree on either side is never converged.
        assert!(!exports_converged(&master(), &json!({"tags":[]})));
        assert!(!exports_converged(&json!(null), &backup()));
    }

    #[test]
    fn exports_converged_never_reads_a_backup_value() {
        // Whatever value the backup's export shows, it is not compared: the
        // backup is never a source of live values.
        let odd = edited(backup(), "T1", |n| {
            n["value"] = json!(123);
            n["timestamp"] = json!(1);
            n["quality"] = json!("Bad");
        });
        assert!(exports_converged(&master(), &odd));
        let member = edited(backup(), "M1", |n| n["tags"][0]["value"] = json!(77));
        assert!(exports_converged(&master(), &member));
    }

    fn policy(budget_ms: u64) -> BackupPolicy {
        BackupPolicy {
            budget: Duration::from_millis(budget_ms),
            interval: Duration::from_millis(5),
        }
    }

    fn look(differing: &[&str]) -> TagLook {
        TagLook {
            reachable: true,
            linked: true,
            exhausted: false,
            compared: true,
            differing: differing.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn look_reasons_follow_the_pair_precedence() {
        assert_eq!(look(&[]).reason(), None);
        assert_eq!(
            look(&["PlantA"]).reason(),
            Some(UnsyncedReason::ContentDiffers)
        );
        let unlinked = TagLook {
            linked: false,
            compared: false,
            ..look(&[])
        };
        assert_eq!(unlinked.reason(), Some(UnsyncedReason::NotSyncing));
        let exhausted = TagLook {
            exhausted: true,
            ..look(&["PlantA"])
        };
        assert_eq!(exhausted.reason(), Some(UnsyncedReason::SyncExhausted));
        let unreachable = TagLook {
            reachable: false,
            exhausted: true,
            ..look(&[])
        };
        assert_eq!(unreachable.reason(), Some(UnsyncedReason::Unreachable));
        // Linked but not compared yet is not converged.
        let unread = TagLook {
            compared: false,
            ..look(&[])
        };
        assert_eq!(unread.reason(), Some(UnsyncedReason::NotSyncing));
    }

    #[test]
    fn wait_step_of_a_look() {
        let p = policy(100);
        assert_eq!(
            tag_wait_step(&look(&[]), Duration::from_millis(500), &p),
            PairWaitStep::Converged
        );
        let exhausted = TagLook {
            exhausted: true,
            ..look(&["PlantA"])
        };
        assert_eq!(
            tag_wait_step(&exhausted, Duration::ZERO, &p),
            PairWaitStep::Exhausted
        );
        assert_eq!(
            tag_wait_step(&look(&["PlantA"]), Duration::from_millis(100), &p),
            PairWaitStep::TimedOut
        );
        assert_eq!(
            tag_wait_step(&look(&["PlantA"]), Duration::from_millis(98), &p),
            PairWaitStep::Sleep(Duration::from_millis(2))
        );
        assert_eq!(
            tag_wait_step(&look(&["PlantA"]), Duration::ZERO, &p),
            PairWaitStep::Sleep(Duration::from_millis(5))
        );
    }

    /// How the fake backup answers.
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Shape {
        /// The master's configuration (F-P1 and F-P2 shape).
        Synced,
        /// A stale copy until the master's resync, then synced.
        StaleUntilResync,
        /// A stale copy whatever happens.
        Stale,
        /// The tag export cannot be read.
        ExportFails,
        /// The status cannot be read.
        StatusFails,
        /// Not linked to its peer.
        Unlinked,
    }

    struct Backup {
        shape: Shape,
        exhausted: bool,
        resyncs: std::sync::Arc<AtomicUsize>,
        exports: Mutex<Vec<String>>,
        since: Mutex<Vec<i64>>,
    }

    impl Backup {
        fn new(shape: Shape, resyncs: &Resync) -> Self {
            Backup {
                shape,
                exhausted: false,
                resyncs: std::sync::Arc::clone(&resyncs.0),
                exports: Mutex::new(Vec::new()),
                since: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl BackupTags for Backup {
        async fn tag_export(&self, provider: &str) -> Result<Vec<u8>, CoreError> {
            self.exports.lock().unwrap().push(provider.to_string());
            let stale = edited(backup(), "T2", |n| n["documentation"] = json!("old"));
            let tree = match self.shape {
                Shape::Synced => backup(),
                Shape::StaleUntilResync if self.resyncs.load(Ordering::SeqCst) > 0 => backup(),
                Shape::StaleUntilResync | Shape::Stale | Shape::Unlinked => stale,
                Shape::ExportFails | Shape::StatusFails => {
                    return Err(CoreError::Internal("unreachable".into()));
                }
            };
            Ok(serde_json::to_vec(&tree).unwrap())
        }

        async fn status(&self) -> Result<RedundancyStatusWire, CoreError> {
            if self.shape == Shape::StatusFails {
                return Err(CoreError::Internal("unreachable".into()));
            }
            Ok(serde_json::from_value(json!({
                "role": "Backup", "activityLevel": "Cold",
                "peerConnected": self.shape != Shape::Unlinked,
                "syncPending": false, "lastSyncTimestamp": 1794179312498i64
            }))
            .unwrap())
        }

        async fn sync_exhausted(&self, since_ms: i64) -> Result<bool, CoreError> {
            self.since.lock().unwrap().push(since_ms);
            Ok(self.exhausted)
        }
    }

    #[derive(Default)]
    struct Resync(std::sync::Arc<AtomicUsize>);

    #[async_trait]
    impl MasterResync for Resync {
        async fn redundancy_resync(&self) -> Result<(), CoreError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn masters() -> BTreeMap<String, Value> {
        BTreeMap::from([("PlantA".to_string(), master())])
    }

    async fn converge(backup: &Backup, resync: &Resync, budget_ms: u64) -> (TagsBackupReport, Vec<String>) {
        let mut events = Vec::new();
        let report = converge_tags(
            &masters(),
            backup,
            Some(resync),
            policy(budget_ms),
            42,
            &mut |event: TagsBackupEvent| events.push(event.name().to_string()),
        )
        .await;
        (report, events)
    }

    #[tokio::test]
    async fn converged_backup_ends_at_once_without_a_resync() {
        let resync = Resync::default();
        let backup = Backup::new(Shape::Synced, &resync);
        let (report, events) = converge(&backup, &resync, 2000).await;
        assert!(report.converged, "{report:?}");
        assert_eq!(report.reason, None);
        assert!(!report.resync_sent);
        assert_eq!(report.providers, ["PlantA"]);
        assert!(report.differing.is_empty());
        assert_eq!(report.budget_s, 2);
        assert_eq!(resync.0.load(Ordering::SeqCst), 0);
        assert_eq!(events, ["backup_wait_started", "backup_verdict"]);
        assert_eq!(*backup.since.lock().unwrap(), [42], "the log anchor is the run's");
    }

    #[tokio::test]
    async fn unconverged_backup_gets_exactly_one_resync_then_a_second_budget() {
        let resync = Resync::default();
        let backup = Backup::new(Shape::Stale, &resync);
        let started = std::time::Instant::now();
        let (report, events) = converge(&backup, &resync, 60).await;
        assert!(!report.converged);
        assert_eq!(report.reason, Some(UnsyncedReason::ContentDiffers));
        assert!(report.resync_sent);
        assert_eq!(report.differing, ["PlantA"]);
        assert_eq!(resync.0.load(Ordering::SeqCst), 1, "exactly one resync");
        assert!(
            started.elapsed() >= Duration::from_millis(120),
            "two budgets were waited: {:?}",
            started.elapsed()
        );
        assert_eq!(
            events,
            ["backup_wait_started", "backup_resync", "backup_verdict"]
        );
    }

    #[tokio::test]
    async fn backup_converges_after_the_resync() {
        let resync = Resync::default();
        let backup = Backup::new(Shape::StaleUntilResync, &resync);
        let (report, _) = converge(&backup, &resync, 60).await;
        assert!(report.converged, "{report:?}");
        assert!(report.resync_sent);
        assert_eq!(resync.0.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn sync_exhausted_ends_the_wait_and_still_sends_one_resync() {
        let resync = Resync::default();
        let mut backup = Backup::new(Shape::Stale, &resync);
        backup.exhausted = true;
        let started = std::time::Instant::now();
        let (report, _) = converge(&backup, &resync, 5000).await;
        assert!(started.elapsed() < Duration::from_secs(2), "ended early");
        assert!(!report.converged);
        assert_eq!(report.reason, Some(UnsyncedReason::SyncExhausted));
        assert!(report.resync_sent);
        assert_eq!(resync.0.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn unreadable_backup_is_unreachable_never_an_error() {
        for shape in [Shape::ExportFails, Shape::StatusFails] {
            let resync = Resync::default();
            let backup = Backup::new(shape, &resync);
            let (report, _) = converge(&backup, &resync, 30).await;
            assert!(!report.converged);
            assert_eq!(report.reason, Some(UnsyncedReason::Unreachable));
            assert_eq!(resync.0.load(Ordering::SeqCst), 1);
        }
    }

    #[tokio::test]
    async fn unlinked_backup_is_not_syncing_and_its_export_is_read_once_at_the_end() {
        let resync = Resync::default();
        let backup = Backup::new(Shape::Unlinked, &resync);
        let (report, _) = converge(&backup, &resync, 30).await;
        assert_eq!(report.reason, Some(UnsyncedReason::NotSyncing));
        // Only the final evaluation reads the export of an unlinked backup.
        assert_eq!(*backup.exports.lock().unwrap(), ["PlantA"]);
    }

    #[tokio::test]
    async fn no_resync_surface_sends_nothing() {
        let resync = Resync::default();
        let backup = Backup::new(Shape::Stale, &resync);
        let report = converge_tags(
            &masters(),
            &backup,
            None,
            policy(20),
            0,
            &mut |_event: TagsBackupEvent| {},
        )
        .await;
        assert!(!report.converged);
        assert!(!report.resync_sent);
    }

    #[test]
    fn report_serializes_value_free() {
        let report = TagsBackupReport {
            converged: false,
            reason: Some(UnsyncedReason::ContentDiffers),
            budget_s: 300,
            elapsed_s: 600,
            resync_sent: true,
            providers: vec!["PlantA".into()],
            differing: vec!["PlantA".into()],
        };
        let value = serde_json::to_value(&report).unwrap();
        let mut keys: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "budget_s",
                "converged",
                "differing",
                "elapsed_s",
                "providers",
                "reason",
                "resync_sent"
            ]
        );
        assert_eq!(value["reason"], "content_differs");
    }
}
