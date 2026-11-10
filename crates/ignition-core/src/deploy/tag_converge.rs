//! The backup's copy of a run's owned tags (Phase 26, 26-13, D-07 on pairs).
//!
//! On a redundant pair a tag-owning `ign deploy projects` run writes to the
//! master only. The tags land in `core`, and the gateway's own sync carries
//! them to the backup. After a green run, and after a verified rollback,
//! [`converge_tags`] reads the backup's REST export of every owned provider
//! until it holds the master's configuration, or says plainly that it does
//! not.
//!
//! ## What is compared
//!
//! [`exports_converged`] compares two gateway exports (research `## Revision
//! 3`, R3-DV part 5): AtomicTag `value`, `timestamp` and `quality` are
//! removed on every node of both sides outside `_types_/`, nodes the run
//! created included (R3-VK, [`super::tag_canon::strip_runtime_values`]); the
//! children of every `tags` array are keyed by name (R3-KEY); `defaultValue`
//! and every other property are compared as configuration. The pair capture
//! showed why: the backup's export holds no live value after a value-less
//! write (F-P2), and lists children in another order (F-P1). An exact or
//! `jq -S` compare never converges. No live value is ever read from the
//! backup: the comparison drops the backup's value keys unread.
//!
//! ## The wait
//!
//! The Phase 24 budget, resync and reason words ([`BackupPolicy`],
//! [`UnsyncedReason`], [`PairWaitStep`]):
//!
//! - each look reads the backup's redundancy status and its log (for the
//!   sync-exhausted line); when it is linked (peer connected, no sync
//!   pending) the owned providers' exports are read and compared;
//! - the backup has converged when it is linked and every export equals the
//!   master's. The proof is the content: a tag write the master took does not
//!   always move the backup's `lastSyncTimestamp` (a run that only writes its
//!   records sends none), so the file engine's timestamp clause is not used;
//! - when the first budget passes without convergence, exactly one resync
//!   request goes to the MASTER ([`MasterResync`]) and a second budget runs;
//!   the sync-exhausted line ends the wait at once and still sends the one
//!   resync; one final look names the reason;
//! - a read that cannot be answered is the reason `unreachable`, never an
//!   error.
//!
//! [`BackupTags`] has reads only, and the CLI implements it with the
//! read-only [`BackupNode`]. Nothing here writes to the backup (T-26-47); the
//! only write is the one resync to the master (T-26-49), and the master is
//! never restored for a backup that did not converge.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use super::pair::{BackupNode, BackupPolicy, PairWaitStep, UnsyncedReason};
use super::tag_canon::{CanonOpts, diff_properties, flatten_tree};
use crate::client::DeployApi;
use crate::client::redundancy::RedundancyStatusWire;
use crate::error::CoreError;

/// The backup node as the tag convergence reads it. Reads only.
#[async_trait]
pub trait BackupTags: Send + Sync {
    /// The backup's native export of `provider`, as its own bytes.
    async fn tag_export(&self, provider: &str) -> Result<Vec<u8>, CoreError>;
    /// The backup's redundancy status.
    async fn status(&self) -> Result<RedundancyStatusWire, CoreError>;
    /// Whether the backup's log says the gateway gave up syncing at or
    /// after `since_ms` (D-10 of Phase 24).
    async fn sync_exhausted(&self, since_ms: i64) -> Result<bool, CoreError>;
}

#[async_trait]
impl BackupTags for BackupNode<'_> {
    async fn tag_export(&self, provider: &str) -> Result<Vec<u8>, CoreError> {
        BackupNode::tag_export(self, provider).await
    }

    async fn status(&self) -> Result<RedundancyStatusWire, CoreError> {
        BackupNode::status(self).await
    }

    async fn sync_exhausted(&self, since_ms: i64) -> Result<bool, CoreError> {
        BackupNode::sync_exhausted(self, since_ms).await
    }
}

/// The one write of the wait: the resync request to the master.
#[async_trait]
pub trait MasterResync: Send + Sync {
    /// Ask the master to resynchronise its backup.
    async fn redundancy_resync(&self) -> Result<(), CoreError>;
}

/// [`MasterResync`] over the master's deploy surface.
pub struct DeployResync<'a>(pub &'a dyn DeployApi);

#[async_trait]
impl MasterResync for DeployResync<'_> {
    async fn redundancy_resync(&self) -> Result<(), CoreError> {
        self.0.redundancy_resync().await
    }
}

/// `data.tags.backup`: what the backup held at the end of the wait. Names,
/// counts and reason words only: no tag value, no gateway text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagsBackupReport {
    /// Every owned provider's export on the backup equals the master's.
    pub converged: bool,
    /// Why not, when it does not (`unreachable`, `sync_exhausted`,
    /// `not_syncing`, `content_differs`).
    pub reason: Option<UnsyncedReason>,
    /// The budget of one wait, in seconds.
    pub budget_s: u64,
    /// Seconds from the start of the wait to the verdict.
    pub elapsed_s: u64,
    /// Whether the one resync request was sent to the master.
    pub resync_sent: bool,
    /// The providers compared, byte-sorted.
    pub providers: Vec<String>,
    /// The providers whose backup export differed at the last look,
    /// byte-sorted.
    pub differing: Vec<String>,
}

/// A step of the wait, for the run's events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagsBackupEvent {
    /// The wait began.
    WaitStarted {
        /// The budget of one wait, in seconds.
        budget_s: u64,
    },
    /// The one resync request was sent to the master.
    Resync {
        /// The request was accepted.
        ok: bool,
        /// The error code when it was not.
        code: Option<String>,
    },
    /// The wait ended.
    Verdict {
        /// The backup converged.
        converged: bool,
        /// Why not.
        reason: Option<UnsyncedReason>,
    },
}

impl TagsBackupEvent {
    /// The event name (the file engine's words).
    pub fn name(&self) -> &'static str {
        match self {
            TagsBackupEvent::WaitStarted { .. } => "backup_wait_started",
            TagsBackupEvent::Resync { .. } => "backup_resync",
            TagsBackupEvent::Verdict { .. } => "backup_verdict",
        }
    }
}

/// Export against export: R3-VK and R3-KEY, `defaultValue` as configuration.
const EXPORTS: CanonOpts = CanonOpts {
    strip_values: true,
    created: false,
    exports: true,
};

/// Whether the backup's export `backup` holds the configuration of the
/// master's export `master`: the same node paths, each equal with AtomicTag
/// value keys removed on both sides (outside `_types_/`) and children keyed
/// by name. A side that is not a tag tree never converges.
pub fn exports_converged(master: &Value, backup: &Value) -> bool {
    let (Ok(master), Ok(backup)) = (
        flatten_tree(master, "master"),
        flatten_tree(backup, "backup"),
    ) else {
        return false;
    };
    master.len() == backup.len()
        && master.iter().all(|(path, node)| {
            backup
                .get(path)
                .is_some_and(|other| diff_properties(node, other, path, EXPORTS).is_empty())
        })
}

/// What one look at the backup found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagLook {
    /// The status and every export read were answered.
    pub reachable: bool,
    /// The backup sees its peer and has no sync queued.
    pub linked: bool,
    /// The log says the gateway gave up syncing.
    pub exhausted: bool,
    /// The exports were read and compared.
    pub compared: bool,
    /// The providers whose export differs, byte-sorted.
    pub differing: Vec<String>,
}

impl TagLook {
    /// Why the backup is not converged, or `None` when it is. First that
    /// holds: `unreachable`, `sync_exhausted`, `not_syncing` (not linked, or
    /// not compared yet), `content_differs`.
    pub fn reason(&self) -> Option<UnsyncedReason> {
        if self.reachable && self.linked && self.compared && self.differing.is_empty() {
            None
        } else if !self.reachable {
            Some(UnsyncedReason::Unreachable)
        } else if self.exhausted {
            Some(UnsyncedReason::SyncExhausted)
        } else if !self.linked || !self.compared {
            Some(UnsyncedReason::NotSyncing)
        } else {
            Some(UnsyncedReason::ContentDiffers)
        }
    }
}

/// What the wait does after `look`, `elapsed` into a budget: the decision of
/// [`super::pair::wait_step`] over a tag look. A budget equal to the elapsed
/// time has passed, and the last sleep never runs past the budget.
pub fn tag_wait_step(look: &TagLook, elapsed: Duration, policy: &BackupPolicy) -> PairWaitStep {
    if look.reason().is_none() {
        PairWaitStep::Converged
    } else if look.exhausted {
        PairWaitStep::Exhausted
    } else if elapsed >= policy.budget {
        PairWaitStep::TimedOut
    } else {
        PairWaitStep::Sleep(
            policy
                .interval
                .max(Duration::from_millis(1))
                .min(policy.budget - elapsed),
        )
    }
}

/// Wait for the backup to hold `master` (each owned provider's export on the
/// master, parsed, keyed by provider), and say how it went. `resync` is the
/// master's surface for the one resync (`None` sends none); `since_ms` is the
/// host's epoch-ms at the start of the run, the anchor of the sync-exhausted
/// line. `on_event` hears the wait start, the resync and the verdict. Never
/// an error: see the module doc.
pub async fn converge_tags(
    master: &BTreeMap<String, Value>,
    backup: &dyn BackupTags,
    resync: Option<&dyn MasterResync>,
    policy: BackupPolicy,
    since_ms: i64,
    on_event: &mut (dyn FnMut(TagsBackupEvent) + Send),
) -> TagsBackupReport {
    let budget_s = policy.budget.as_secs();
    let started = Instant::now();
    on_event(TagsBackupEvent::WaitStarted { budget_s });

    let (mut last, mut step) = wait_budget(master, backup, &policy, since_ms).await;
    let mut resync_sent = false;
    if step != PairWaitStep::Converged {
        // The first budget passed, or the gateway gave up syncing: the one
        // resync goes to the master, whatever its answer.
        if let Some(resync) = resync {
            let event = match resync.redundancy_resync().await {
                Ok(()) => TagsBackupEvent::Resync {
                    ok: true,
                    code: None,
                },
                Err(error) => TagsBackupEvent::Resync {
                    ok: false,
                    code: Some(error.code().to_string()),
                },
            };
            resync_sent = true;
            on_event(event);
        }
        if step == PairWaitStep::TimedOut {
            (last, step) = wait_budget(master, backup, &policy, since_ms).await;
        }
        if step != PairWaitStep::Converged {
            // One final look reads everything it can and names the reason.
            last = look(master, backup, since_ms, true).await;
        }
    }
    let reason = last.reason();
    on_event(TagsBackupEvent::Verdict {
        converged: reason.is_none(),
        reason,
    });
    TagsBackupReport {
        converged: reason.is_none(),
        reason,
        budget_s,
        elapsed_s: started.elapsed().as_secs(),
        resync_sent,
        providers: master.keys().cloned().collect(),
        differing: last.differing,
    }
}

/// Look until the backup converges, the gateway gives up syncing or the
/// budget passes.
async fn wait_budget(
    master: &BTreeMap<String, Value>,
    backup: &dyn BackupTags,
    policy: &BackupPolicy,
    since_ms: i64,
) -> (TagLook, PairWaitStep) {
    let started = Instant::now();
    loop {
        let found = look(master, backup, since_ms, false).await;
        match tag_wait_step(&found, started.elapsed(), policy) {
            PairWaitStep::Sleep(pause) => tokio::time::sleep(pause).await,
            step => return (found, step),
        }
    }
}

/// One look at the backup. The exports are read when it is linked, or
/// whenever its status is readable when `force` is set (the final look).
async fn look(
    master: &BTreeMap<String, Value>,
    backup: &dyn BackupTags,
    since_ms: i64,
    force: bool,
) -> TagLook {
    let unreachable = TagLook {
        reachable: false,
        linked: false,
        exhausted: false,
        compared: false,
        differing: Vec::new(),
    };
    let Ok(status) = backup.status().await else {
        return unreachable;
    };
    let linked = status.peer_connected && !status.sync_pending;
    // The log only feeds the early end; one that cannot be read is not the
    // gateway giving up.
    let exhausted = backup.sync_exhausted(since_ms).await.unwrap_or(false);
    if !(linked || force) {
        return TagLook {
            reachable: true,
            linked,
            exhausted,
            compared: false,
            differing: Vec::new(),
        };
    }
    let mut differing = Vec::new();
    for (provider, ours) in master {
        let Ok(bytes) = backup.tag_export(provider).await else {
            return TagLook {
                linked,
                exhausted,
                ..unreachable
            };
        };
        let equal = serde_json::from_slice::<Value>(&bytes)
            .is_ok_and(|theirs| exports_converged(ours, &theirs));
        if !equal {
            differing.push(provider.clone());
        }
    }
    TagLook {
        reachable: true,
        linked,
        exhausted,
        compared: true,
        differing,
    }
}

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
        assert!(
            !exports_converged(&master(), &dropped),
            "a dropped defaultValue"
        );
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

    async fn converge(
        backup: &Backup,
        resync: &Resync,
        budget_ms: u64,
    ) -> (TagsBackupReport, Vec<String>) {
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
        assert_eq!(
            *backup.since.lock().unwrap(),
            [42],
            "the log anchor is the run's"
        );
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
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
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
