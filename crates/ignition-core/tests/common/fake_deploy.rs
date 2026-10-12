//! One stateful in-memory gateway implementing the whole `DeployApi`
//! (20-05). Phase 23's apply engine tests against it instead of a live
//! gateway.
//!
//! A fake that encodes assumptions as facts turns false beliefs into green
//! tests (threat T-20-25), so two rules hold here:
//!
//! - Every decision the library already makes as a pure function is taken
//!   by calling that function: `lock_verdict` decides whether the lock is
//!   free, `scan_complete` decides when a scan has finished,
//!   `normalize_deployment_mode` reads the active mode and
//!   `validate_segment` guards every path segment. Nothing is re-derived.
//! - Every behaviour cites the `20-LIVE-CAPTURES.md` Decision it copies.
//!   Where the captures are silent the fake says so below instead of
//!   inventing a behaviour; 20-06 runs `deploy_scenario::scenario` against
//!   the live gateway and diffs the trace against this fake's.
//!
//! The fake's errors are the ones the real client yields for the same wire
//! outcome: `scan_lock_held` and `resource_signature_rejected` are typed,
//! a duplicate create or mode create is `internal` (the real client maps a
//! 409 outside the scan-lock route to `internal`), an unknown update name
//! is `not_found`.
//!
//! Where the captures are silent:
//! - Which copy a collection-less PUT updates: the fake updates the
//!   effective copy (active mode's collection, else `core`). 20-06 settles
//!   it live.
//! - `remaining` right after an acquire: the live gateway reads
//!   `holdTimeout - 1` (D9, C1b); the fake reads `holdTimeout` and counts
//!   one gateway second per info read, so a hold of 3 reads 3, 2, 1, then
//!   free. The trace compares only held-or-free, never the number.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use ignition_core::client::DeployApi;
use ignition_core::client::config_resources::{
    MutationOutcome, ResourceCreate, ResourceName, ResourceRecord, ResourceUpdate,
    list_query_pairs, resource_find_path, validate_segment,
};
use ignition_core::client::eam::ResourceChange;
use ignition_core::client::mode::{ModeCreate, ModeRecord, normalize_deployment_mode};
use ignition_core::client::query::{ListEnvelope, ListMetadata, ListQuery};
use ignition_core::client::scan::{
    LockObservation, LockVerdict, ScanKind, ScanLockInfo, ScanStatus, lock_verdict, scan_complete,
};
use ignition_core::error::{CoreError, SignatureReason};

/// Where the fake pretends to live; only ever shown in error endpoints.
const FAKE_BASE: &str = "http://fake-gateway.invalid";

/// First `lastScanTimestamp` the fake reports (the epoch-ms base of the
/// 8.3.6 C1i capture neighbourhood).
const SCAN_BASE_TS: i64 = 1_791_584_841_451;

/// Status reads a triggered scan answers `scanActive: true` for before it
/// reports inactive (C1i: two active polls, then complete).
const SCAN_ACTIVE_READS: u32 = 2;

/// One value per scan kind; `ScanKind` is not `Hash`, so a pair, not a map.
#[derive(Default)]
struct PerKind<T> {
    config: T,
    projects: T,
}

impl<T> PerKind<T> {
    fn get_mut(&mut self, kind: ScanKind) -> &mut T {
        match kind {
            ScanKind::Config => &mut self.config,
            ScanKind::Projects => &mut self.projects,
        }
    }
}

/// A held scan-lock.
struct Lock {
    actor: String,
    remaining: i64,
    timestamp: i64,
}

/// One kind's scan state.
struct ScanState {
    active: bool,
    reads_left: u32,
    timestamp: i64,
    /// The reading taken at trigger time; completion is judged against it
    /// by the library's own `scan_complete`.
    before: Option<ScanStatus>,
}

impl Default for ScanState {
    fn default() -> Self {
        Self {
            active: false,
            reads_left: 0,
            timestamp: SCAN_BASE_TS,
            before: None,
        }
    }
}

#[derive(Default)]
struct FakeState {
    locks: PerKind<Option<Lock>>,
    scans: PerKind<ScanState>,
    /// Insertion order is gateway order.
    resources: Vec<ResourceRecord>,
    modes: Vec<ModeRecord>,
    /// The raw `deploymentMode` string the gateway would send.
    deployment_mode: Option<String>,
    /// Source of signatures and lock timestamps.
    counter: u64,
}

impl FakeState {
    fn next_signature(&mut self) -> String {
        self.counter += 1;
        // 64 hex characters, like a real signature, from a counter.
        format!("{:064x}", self.counter)
    }

    fn active_mode(&self) -> Option<String> {
        normalize_deployment_mode(self.deployment_mode.clone())
    }

    /// The copy a read or write targets (C-list, D7): `Some(c)` is exactly
    /// collection `c`; `None` is the effective view, the active mode's
    /// copy when it exists, else `core`.
    fn locate(
        &self,
        resource_type: &str,
        name: Option<&str>,
        collection: Option<&str>,
    ) -> Option<usize> {
        let same = |rec: &ResourceRecord| {
            rec.resource_type == resource_type && rec.name.as_deref() == name
        };
        let in_collection = |wanted: &str| {
            self.resources
                .iter()
                .position(|rec| same(rec) && rec.collection.as_deref() == Some(wanted))
        };
        match collection {
            Some(c) => in_collection(c),
            None => self
                .active_mode()
                .and_then(|mode| in_collection(&mode))
                .or_else(|| in_collection("core")),
        }
    }

    /// Every collection holding `name` of `resource_type`, in insertion
    /// order (C-list: `modes` of a names item).
    fn collections_of(&self, resource_type: &str, name: Option<&str>) -> Vec<String> {
        self.resources
            .iter()
            .filter(|rec| rec.resource_type == resource_type && rec.name.as_deref() == name)
            .filter_map(|rec| rec.collection.clone())
            .collect()
    }

    fn record_at(&self, index: usize) -> ResourceRecord {
        let mut record = self.resources[index].clone();
        record.collections = self.collections_of(&record.resource_type, record.name.as_deref());
        record
    }
}

/// The in-memory gateway. Hand out one [`FakeDeploy`] per API token.
#[derive(Clone)]
pub struct FakeGateway {
    state: Arc<Mutex<FakeState>>,
}

impl Default for FakeGateway {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeGateway {
    /// A gateway with no lock, no scan history, no resources, no modes and
    /// no deployment mode (C-null (no mode): `""`).
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeState {
                deployment_mode: Some(String::new()),
                ..FakeState::default()
            })),
        }
    }

    /// A client for the API token `token_name`. Its actor is
    /// `api-token:<token_name>:127.0.0.1`, the C1c format.
    pub fn client(&self, token_name: &str) -> FakeDeploy {
        FakeDeploy {
            state: Arc::clone(&self.state),
            actor: format!("api-token:{token_name}:127.0.0.1"),
        }
    }

    /// Advance the gateway clock: every held lock loses `seconds` of its
    /// hold and is freed at zero (D9: expiry reads as 204).
    pub fn tick(&self, seconds: i64) {
        let mut state = self.state.lock().expect("fake state");
        for kind in [ScanKind::Config, ScanKind::Projects] {
            let slot = state.locks.get_mut(kind);
            if let Some(lock) = slot {
                lock.remaining -= seconds;
                if lock.remaining <= 0 {
                    *slot = None;
                }
            }
        }
    }

    /// Set the active deployment mode as the gateway would report it:
    /// `None` is `""` (D5).
    pub fn set_deployment_mode(&self, mode: Option<String>) {
        self.state.lock().expect("fake state").deployment_mode = Some(mode.unwrap_or_default());
    }

    /// Seed a singleton resource (C-list "GET singleton") in `collection`.
    pub fn seed_singleton(
        &self,
        module: &str,
        ty: &str,
        collection: &str,
        mut record: ResourceRecord,
    ) {
        let mut state = self.state.lock().expect("fake state");
        record.resource_type = format!("{module}/{ty}");
        record.name = None;
        record.collection = Some(collection.to_string());
        if record.signature.is_none() {
            record.signature = Some(state.next_signature());
        }
        state.resources.push(record);
    }
}

/// One API token's view of the fake gateway.
#[derive(Clone)]
pub struct FakeDeploy {
    state: Arc<Mutex<FakeState>>,
    actor: String,
}

impl FakeDeploy {
    fn state(&self) -> MutexGuard<'_, FakeState> {
        self.state.lock().expect("fake state")
    }
}

fn endpoint(path: &str) -> Option<String> {
    Some(format!("{FAKE_BASE}{path}"))
}

fn lock_observation(lock: &Option<Lock>) -> LockObservation {
    match lock {
        None => LockObservation::Free,
        Some(lock) => LockObservation::Held(ScanLockInfo {
            timestamp: Some(lock.timestamp),
            remaining: lock.remaining,
            actor: Some(lock.actor.clone()),
            extra: BTreeMap::new(),
        }),
    }
}

fn change(record: &ResourceRecord, new_signature: &str) -> ResourceChange {
    ResourceChange {
        name: record.name.clone().unwrap_or_default(),
        resource_type: record.resource_type.clone(),
        collection: record.collection.clone().unwrap_or_default(),
        new_signature: Some(new_signature.to_string()),
    }
}

fn outcome(changes: Vec<ResourceChange>) -> MutationOutcome {
    MutationOutcome {
        success: true,
        changes,
        problem: None,
    }
}

/// The `internal` a real client reports for a 409 outside the scan-lock
/// route (classify's fallback arm).
fn conflict(path: &str, detail: &str) -> CoreError {
    CoreError::Internal(format!(
        "unexpected HTTP 409 Conflict from {FAKE_BASE}{path}{detail}"
    ))
}

fn envelope<T>(items: Vec<T>) -> ListEnvelope<T> {
    let count = items.len() as i64;
    ListEnvelope {
        items,
        metadata: ListMetadata {
            total: count,
            matching: count,
            limit: -1,
            offset: 0,
        },
    }
}

/// The `field[op]=value` pairs of a list call that are filters (everything
/// that is not `limit`, `offset`, `sortBy` or `search`).
fn filter_pairs(pairs: &[(String, String)]) -> Vec<(&str, &str)> {
    pairs
        .iter()
        .filter(|(key, _)| key.contains('['))
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect()
}

#[async_trait::async_trait]
impl DeployApi for FakeDeploy {
    async fn resource_names(
        &self,
        module: &str,
        ty: &str,
    ) -> Result<ListEnvelope<ResourceName>, CoreError> {
        validate_segment("module", module)?;
        validate_segment("type", ty)?;
        let resource_type = format!("{module}/{ty}");
        let state = self.state();
        let mut seen: Vec<String> = Vec::new();
        let mut items = Vec::new();
        for rec in &state.resources {
            let Some(name) = rec.name.clone() else {
                continue;
            };
            if rec.resource_type != resource_type || seen.contains(&name) {
                continue;
            }
            seen.push(name.clone());
            items.push(ResourceName {
                modes: state.collections_of(&resource_type, Some(&name)),
                name,
                enabled: rec.enabled,
                extra: BTreeMap::new(),
            });
        }
        Ok(envelope(items))
    }

    async fn resource_list(
        &self,
        module: &str,
        ty: &str,
        q: &ListQuery,
    ) -> Result<ListEnvelope<ResourceRecord>, CoreError> {
        validate_segment("module", module)?;
        validate_segment("type", ty)?;
        // The library's own filter rule (D7: `field[op]=value`, never
        // `filter=`), so a malformed filter is refused the same way.
        let pairs = list_query_pairs(q)?;
        let resource_type = format!("{module}/{ty}");
        let state = self.state();
        // D7: the list is the EFFECTIVE view only, one item per name.
        let mut seen: Vec<String> = Vec::new();
        let mut records = Vec::new();
        for rec in &state.resources {
            let Some(name) = rec.name.clone() else {
                continue;
            };
            if rec.resource_type != resource_type || seen.contains(&name) {
                continue;
            }
            seen.push(name.clone());
            if let Some(index) = state.locate(&resource_type, Some(&name), None) {
                records.push(state.record_at(index));
            }
        }
        // D7: the gateway matches a filter against the effective item. The
        // fake supports `eq` on `name` and `collection` only and refuses
        // anything else loudly, never by ignoring it.
        for (key, wanted) in filter_pairs(&pairs) {
            match key {
                "name[eq]" => records.retain(|rec| rec.name.as_deref() == Some(wanted)),
                "collection[eq]" => {
                    records.retain(|rec| rec.collection.as_deref() == Some(wanted));
                }
                other => {
                    return Err(CoreError::Internal(format!(
                        "FakeDeploy supports only name[eq] and collection[eq] list filters, \
                         got {other}"
                    )));
                }
            }
        }
        let matching = records.len() as i64;
        let offset = usize::try_from(q.offset).unwrap_or(0);
        let mut page: Vec<ResourceRecord> = records.into_iter().skip(offset).collect();
        if let Ok(limit) = usize::try_from(q.limit) {
            page.truncate(limit);
        }
        Ok(ListEnvelope {
            items: page,
            metadata: ListMetadata {
                total: matching,
                matching,
                limit: q.limit,
                offset: q.offset,
            },
        })
    }

    async fn resource_find(
        &self,
        module: &str,
        ty: &str,
        name: &str,
        collection: Option<&str>,
    ) -> Result<ResourceRecord, CoreError> {
        // The library's own path builder validates the segments and the name.
        let path = resource_find_path(module, ty, name)?;
        let resource_type = format!("{module}/{ty}");
        let state = self.state();
        // C-list (D7): `None` is the effective view, `Some(c)` that copy.
        state
            .locate(&resource_type, Some(name), collection)
            .map(|index| state.record_at(index))
            .ok_or(CoreError::NotFound {
                endpoint: endpoint(&path),
            })
    }

    async fn resource_singleton(
        &self,
        module: &str,
        ty: &str,
        collection: Option<&str>,
    ) -> Result<ResourceRecord, CoreError> {
        validate_segment("module", module)?;
        validate_segment("type", ty)?;
        let resource_type = format!("{module}/{ty}");
        let state = self.state();
        state
            .locate(&resource_type, None, collection)
            .map(|index| state.record_at(index))
            .ok_or(CoreError::NotFound {
                endpoint: endpoint(&format!("/data/api/v1/resources/singleton/{module}/{ty}")),
            })
    }

    async fn resource_create(
        &self,
        module: &str,
        ty: &str,
        items: &[ResourceCreate],
    ) -> Result<MutationOutcome, CoreError> {
        validate_segment("module", module)?;
        validate_segment("type", ty)?;
        if items.is_empty() {
            return Err(CoreError::InvalidInput {
                reason: "resource create needs at least one item".to_string(),
            });
        }
        if items.iter().any(|item| item.name.is_empty()) {
            return Err(CoreError::InvalidInput {
                reason: "resource name must not be empty".to_string(),
            });
        }
        let path = format!("/data/api/v1/resources/{module}/{ty}");
        let resource_type = format!("{module}/{ty}");
        let mut state = self.state();
        // D10: a create needs only name and config; the collection defaults
        // to `core`. A duplicate is a 409, so nothing is applied unless
        // every item is new (the gateway's push is atomic).
        for item in items {
            let target = item.collection.as_deref().unwrap_or("core");
            if state
                .locate(&resource_type, Some(&item.name), Some(target))
                .is_some()
            {
                return Err(conflict(&path, ""));
            }
        }
        let mut changes = Vec::new();
        for item in items {
            let signature = state.next_signature();
            let record = ResourceRecord {
                resource_type: resource_type.clone(),
                name: Some(item.name.clone()),
                description: item.description.clone(),
                enabled: Some(item.enabled.unwrap_or(true)),
                version: Some(1),
                collection: Some(
                    item.collection
                        .clone()
                        .unwrap_or_else(|| "core".to_string()),
                ),
                collections: Vec::new(),
                signature: Some(signature.clone()),
                config: item.config.clone(),
                backup_config: item.backup_config.clone(),
                attributes: None,
                extra: BTreeMap::new(),
            };
            changes.push(change(&record, &signature));
            state.resources.push(record);
        }
        Ok(outcome(changes))
    }

    async fn resource_update(
        &self,
        module: &str,
        ty: &str,
        items: &[ResourceUpdate],
    ) -> Result<MutationOutcome, CoreError> {
        validate_segment("module", module)?;
        validate_segment("type", ty)?;
        if items.is_empty() {
            return Err(CoreError::InvalidInput {
                reason: "resource update needs at least one item".to_string(),
            });
        }
        let path = format!("/data/api/v1/resources/{module}/{ty}");
        let resource_type = format!("{module}/{ty}");
        let mut state = self.state();
        // Resolve and check every item first: a refusal leaves the whole
        // batch unapplied (D3: "zero partial effect").
        let mut targets = Vec::new();
        for item in items {
            let index = state
                .locate(&resource_type, Some(&item.name), item.collection.as_deref())
                // D10: PUT of an unknown name is a bare 404.
                .ok_or(CoreError::NotFound {
                    endpoint: endpoint(&path),
                })?;
            // D3: a signature that is not the copy's current one is the
            // stale refusal (500 on the wire).
            if state.resources[index].signature.as_deref() != Some(item.signature()) {
                return Err(CoreError::ResourceSignatureRejected {
                    name: item.name.clone(),
                    reason: SignatureReason::Stale,
                    endpoint: endpoint(&path),
                });
            }
            targets.push(index);
        }
        let mut changes = Vec::new();
        for (item, index) in items.iter().zip(targets) {
            // Every successful write rotates the signature (D3, D10).
            let signature = state.next_signature();
            let record = &mut state.resources[index];
            record.config = item.config.clone();
            if item.enabled.is_some() {
                record.enabled = item.enabled;
            }
            if item.description.is_some() {
                record.description = item.description.clone();
            }
            if item.backup_config.is_some() {
                record.backup_config = item.backup_config.clone();
            }
            record.signature = Some(signature.clone());
            changes.push(change(record, &signature));
        }
        Ok(outcome(changes))
    }

    async fn scan_lock_acquire(
        &self,
        kind: ScanKind,
        _acquire_s: u64,
        hold_s: u64,
    ) -> Result<(), CoreError> {
        let mut state = self.state();
        state.counter += 1;
        let timestamp = 380_000_000 + (state.counter as i64) * 61_000;
        let slot = state.locks.get_mut(kind);
        // The library's own verdict decides whether the lock is taken.
        // D1 + D2: any held lock refuses, the holder's own token included;
        // the gateway never waits out `acquireTimeout` (D1: immediate 409).
        if lock_verdict(&lock_observation(slot), 0) != LockVerdict::NotHeld {
            return Err(CoreError::ScanLockHeld {
                actor: slot.as_ref().map(|lock| lock.actor.clone()),
                kind: kind.as_str().to_string(),
                endpoint: endpoint(&format!("/data/api/v1/scan-lock/{}", kind.as_str())),
            });
        }
        *slot = Some(Lock {
            actor: self.actor.clone(),
            remaining: i64::try_from(hold_s).unwrap_or(i64::MAX),
            timestamp,
        });
        Ok(())
    }

    async fn scan_lock_info(&self, kind: ScanKind) -> Result<LockObservation, CoreError> {
        let mut state = self.state();
        let slot = state.locks.get_mut(kind);
        let observation = lock_observation(slot);
        // D9: one gateway second elapses per read; at zero the lock is gone
        // and the next read is a 204 (Free).
        if let Some(lock) = slot {
            lock.remaining -= 1;
            if lock.remaining <= 0 {
                *slot = None;
            }
        }
        Ok(observation)
    }

    async fn scan_status(&self, kind: ScanKind) -> Result<ScanStatus, CoreError> {
        let mut state = self.state();
        let scan = state.scans.get_mut(kind);
        let reading = |active: bool, timestamp: i64| ScanStatus {
            scan_active: active,
            last_scan_timestamp: timestamp,
            last_scan_duration: Some(1000),
            extra: BTreeMap::new(),
        };
        if !scan.active {
            return Ok(reading(false, scan.timestamp));
        }
        // C1i: a triggered scan reads active for a few polls, then inactive
        // with the timestamp advanced. The library's own `scan_complete`
        // decides when the candidate reading ends the scan.
        let candidate = if scan.reads_left > 0 {
            scan.reads_left -= 1;
            reading(true, scan.timestamp)
        } else {
            reading(false, scan.timestamp + 1)
        };
        let finished = scan
            .before
            .as_ref()
            .is_some_and(|before| scan_complete(before, &candidate));
        if finished {
            scan.active = false;
            scan.timestamp = candidate.last_scan_timestamp;
            scan.before = None;
        }
        Ok(candidate)
    }

    async fn scan_request(&self, kind: ScanKind) -> Result<(), CoreError> {
        let mut state = self.state();
        // D4: any actor's scan POST is accepted whether or not another
        // actor holds the lock; a second POST while one runs is a no-op.
        let scan = state.scans.get_mut(kind);
        if !scan.active {
            scan.active = true;
            scan.reads_left = SCAN_ACTIVE_READS;
            scan.before = Some(ScanStatus {
                scan_active: false,
                last_scan_timestamp: scan.timestamp,
                last_scan_duration: None,
                extra: BTreeMap::new(),
            });
        }
        // D12 (and D4 for a non-holder): a scan releases the lock at
        // trigger time, for the holder and for any other actor alike.
        *state.locks.get_mut(kind) = None;
        Ok(())
    }

    async fn modes(&self) -> Result<ListEnvelope<ModeRecord>, CoreError> {
        // C-mode: insertion order, the standard envelope.
        Ok(envelope(self.state().modes.clone()))
    }

    async fn mode_create(&self, body: &ModeCreate) -> Result<ModeRecord, CoreError> {
        validate_segment("mode name", &body.name)?;
        let mut state = self.state();
        if state.modes.iter().any(|mode| mode.name == body.name) {
            // D6: the duplicate is a 409 Jetty page, `internal` client-side.
            return Err(conflict(
                "/data/api/v1/mode",
                " (gateway error page: Error 409: Mode already exists)",
            ));
        }
        let record = ModeRecord {
            name: body.name.clone(),
            title: Some(body.title.clone()),
            description: Some(body.description.clone()),
            resource_count: Some(0),
            extra: BTreeMap::new(),
        };
        // T-20-26: creating a mode only defines it; `deployment_mode` is
        // not touched.
        state.modes.push(record.clone());
        Ok(record)
    }

    async fn deployment_mode(&self) -> Result<Option<String>, CoreError> {
        Ok(self.state().active_mode())
    }
}
