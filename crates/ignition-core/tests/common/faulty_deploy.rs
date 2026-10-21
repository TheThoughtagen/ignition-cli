//! `FaultyDeploy`: a gateway-call fault wrapper for any [`DeployApi`]
//! (Phase 23 review fix CR-01).
//!
//! `FaultyHost` fails whole host calls; this does the same one level over, for
//! the REST calls the engine makes after a commit (the lock read, the scan,
//! the verify's read back). A rule fails the next `times` calls of one method
//! with a `network_error`, and only once the wrapper is *armed*: a test arms
//! it from a `FaultyHost` hook that runs after the commit returned, so the
//! preflights, the lock acquire and the commit all see a healthy gateway.
//!
//! A clone shares every rule with the original, so a test keeps one clone to
//! read [`FaultyDeploy::calls`] after handing another to the engine.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use ignition_core::client::DeployApi;
use ignition_core::client::config_resources::{
    MutationOutcome, ResourceCreate, ResourceName, ResourceRecord, ResourceUpdate,
};
use ignition_core::client::deploy_api::SecretNameListing;
use ignition_core::client::mode::{ModeCreate, ModeRecord};
use ignition_core::client::query::{ListEnvelope, ListQuery};
use ignition_core::client::scan::{LockObservation, ScanKind, ScanStatus};
use ignition_core::error::CoreError;

#[derive(Default)]
struct State {
    armed: bool,
    /// Calls still to fail, by method name.
    fails: BTreeMap<&'static str, usize>,
    /// Calls that failed, by method name.
    failed: BTreeMap<&'static str, usize>,
}

/// The rules, shared between the wrapper and the test.
#[derive(Clone, Default)]
pub struct DeployFaults {
    state: Arc<Mutex<State>>,
}

impl DeployFaults {
    /// Fail the next `times` calls of `method` once armed.
    pub fn fail(self, method: &'static str, times: usize) -> Self {
        self.state().fails.insert(method, times);
        self
    }

    /// From now on the rules apply.
    pub fn arm(&self) {
        self.state().armed = true;
    }

    /// How many calls of `method` were failed so far.
    pub fn failed(&self, method: &'static str) -> usize {
        self.state().failed.get(method).copied().unwrap_or(0)
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("faulty deploy state")
    }

    fn hit(&self, method: &'static str) -> Result<(), CoreError> {
        let mut state = self.state();
        if !state.armed {
            return Ok(());
        }
        let Some(left) = state.fails.get_mut(method) else {
            return Ok(());
        };
        if *left == 0 {
            return Ok(());
        }
        *left -= 1;
        *state.failed.entry(method).or_insert(0) += 1;
        Err(CoreError::Network {
            url: format!("http://faulty-gateway.invalid/{method}"),
            source: None,
            observation: None,
        })
    }
}

/// Wraps `D`, failing calls named by its [`DeployFaults`].
pub struct FaultyDeploy<D: DeployApi> {
    inner: D,
    faults: DeployFaults,
}

impl<D: DeployApi> FaultyDeploy<D> {
    pub fn new(inner: D, faults: DeployFaults) -> Self {
        FaultyDeploy { inner, faults }
    }
}

#[async_trait::async_trait]
impl<D: DeployApi> DeployApi for FaultyDeploy<D> {
    async fn resource_names(
        &self,
        module: &str,
        ty: &str,
    ) -> Result<ListEnvelope<ResourceName>, CoreError> {
        self.faults.hit("resource_names")?;
        self.inner.resource_names(module, ty).await
    }

    async fn resource_list(
        &self,
        module: &str,
        ty: &str,
        q: &ListQuery,
    ) -> Result<ListEnvelope<ResourceRecord>, CoreError> {
        self.faults.hit("resource_list")?;
        self.inner.resource_list(module, ty, q).await
    }

    async fn resource_find(
        &self,
        module: &str,
        ty: &str,
        name: &str,
        collection: Option<&str>,
    ) -> Result<ResourceRecord, CoreError> {
        self.faults.hit("resource_find")?;
        self.inner.resource_find(module, ty, name, collection).await
    }

    async fn resource_singleton(
        &self,
        module: &str,
        ty: &str,
        collection: Option<&str>,
    ) -> Result<ResourceRecord, CoreError> {
        self.faults.hit("resource_singleton")?;
        self.inner.resource_singleton(module, ty, collection).await
    }

    async fn resource_create(
        &self,
        module: &str,
        ty: &str,
        items: &[ResourceCreate],
    ) -> Result<MutationOutcome, CoreError> {
        self.faults.hit("resource_create")?;
        self.inner.resource_create(module, ty, items).await
    }

    async fn resource_update(
        &self,
        module: &str,
        ty: &str,
        items: &[ResourceUpdate],
    ) -> Result<MutationOutcome, CoreError> {
        self.faults.hit("resource_update")?;
        self.inner.resource_update(module, ty, items).await
    }

    async fn scan_lock_acquire(
        &self,
        kind: ScanKind,
        acquire_s: u64,
        hold_s: u64,
    ) -> Result<(), CoreError> {
        self.faults.hit("scan_lock_acquire")?;
        self.inner.scan_lock_acquire(kind, acquire_s, hold_s).await
    }

    async fn scan_lock_info(&self, kind: ScanKind) -> Result<LockObservation, CoreError> {
        self.faults.hit("scan_lock_info")?;
        self.inner.scan_lock_info(kind).await
    }

    async fn scan_status(&self, kind: ScanKind) -> Result<ScanStatus, CoreError> {
        self.faults.hit("scan_status")?;
        self.inner.scan_status(kind).await
    }

    async fn scan_request(&self, kind: ScanKind) -> Result<(), CoreError> {
        self.faults.hit("scan_request")?;
        self.inner.scan_request(kind).await
    }

    async fn modes(&self) -> Result<ListEnvelope<ModeRecord>, CoreError> {
        self.faults.hit("modes")?;
        self.inner.modes().await
    }

    async fn mode_create(&self, body: &ModeCreate) -> Result<ModeRecord, CoreError> {
        self.faults.hit("mode_create")?;
        self.inner.mode_create(body).await
    }

    async fn deployment_mode(&self) -> Result<Option<String>, CoreError> {
        self.faults.hit("deployment_mode")?;
        self.inner.deployment_mode().await
    }

    async fn restart_pending(&self) -> Result<Vec<String>, CoreError> {
        self.faults.hit("restart_pending")?;
        self.inner.restart_pending().await
    }

    async fn redundancy_resync(&self) -> Result<(), CoreError> {
        self.faults.hit("redundancy_resync")?;
        self.inner.redundancy_resync().await
    }

    async fn secret_names(&self, provider: &str) -> Result<SecretNameListing, CoreError> {
        self.faults.hit("secret_names")?;
        self.inner.secret_names(provider).await
    }
}
