//! `ForbidWrites<T>`: a [`DeployApi`] wrapper that forwards every read and
//! panics on every write (22-10).
//!
//! `deploy plan`, `pull` and `verify` never write to the gateway. Wrapping the
//! fake (or any other `DeployApi`) in this double turns an accidental write
//! into a test failure instead of a silent state change. The six write
//! methods are the lock acquire, the scan trigger, the two resource writes,
//! the mode create and the redundancy resync. `restart_pending` and
//! `secret_names` are reads and are forwarded.

use ignition_core::client::DeployApi;
use ignition_core::client::config_resources::{
    MutationOutcome, ResourceCreate, ResourceName, ResourceRecord, ResourceUpdate,
};
use ignition_core::client::deploy_api::SecretNameListing;
use ignition_core::client::mode::{ModeCreate, ModeRecord};
use ignition_core::client::query::{ListEnvelope, ListQuery};
use ignition_core::client::scan::{LockObservation, ScanKind, ScanStatus};
use ignition_core::error::CoreError;

/// Forwards reads to `.0`, panics on writes.
pub struct ForbidWrites<T>(pub T);

fn refuse(method: &str) -> ! {
    panic!("plan, pull and verify must not write: {method}")
}

#[async_trait::async_trait]
impl<T: DeployApi> DeployApi for ForbidWrites<T> {
    async fn resource_names(
        &self,
        module: &str,
        ty: &str,
    ) -> Result<ListEnvelope<ResourceName>, CoreError> {
        self.0.resource_names(module, ty).await
    }

    async fn resource_list(
        &self,
        module: &str,
        ty: &str,
        q: &ListQuery,
    ) -> Result<ListEnvelope<ResourceRecord>, CoreError> {
        self.0.resource_list(module, ty, q).await
    }

    async fn resource_find(
        &self,
        module: &str,
        ty: &str,
        name: &str,
        collection: Option<&str>,
    ) -> Result<ResourceRecord, CoreError> {
        self.0.resource_find(module, ty, name, collection).await
    }

    async fn resource_singleton(
        &self,
        module: &str,
        ty: &str,
        collection: Option<&str>,
    ) -> Result<ResourceRecord, CoreError> {
        self.0.resource_singleton(module, ty, collection).await
    }

    async fn resource_create(
        &self,
        _module: &str,
        _ty: &str,
        _items: &[ResourceCreate],
    ) -> Result<MutationOutcome, CoreError> {
        refuse("resource_create")
    }

    async fn resource_update(
        &self,
        _module: &str,
        _ty: &str,
        _items: &[ResourceUpdate],
    ) -> Result<MutationOutcome, CoreError> {
        refuse("resource_update")
    }

    async fn scan_lock_acquire(
        &self,
        _kind: ScanKind,
        _acquire_s: u64,
        _hold_s: u64,
    ) -> Result<(), CoreError> {
        refuse("scan_lock_acquire")
    }

    async fn scan_lock_info(&self, kind: ScanKind) -> Result<LockObservation, CoreError> {
        self.0.scan_lock_info(kind).await
    }

    async fn scan_status(&self, kind: ScanKind) -> Result<ScanStatus, CoreError> {
        self.0.scan_status(kind).await
    }

    async fn scan_request(&self, _kind: ScanKind) -> Result<(), CoreError> {
        refuse("scan_request")
    }

    async fn modes(&self) -> Result<ListEnvelope<ModeRecord>, CoreError> {
        self.0.modes().await
    }

    async fn mode_create(&self, _body: &ModeCreate) -> Result<ModeRecord, CoreError> {
        refuse("mode_create")
    }

    async fn deployment_mode(&self) -> Result<Option<String>, CoreError> {
        self.0.deployment_mode().await
    }

    async fn restart_pending(&self) -> Result<Vec<String>, CoreError> {
        self.0.restart_pending().await
    }

    async fn redundancy_resync(&self) -> Result<(), CoreError> {
        refuse("redundancy_resync")
    }

    async fn secret_names(&self, provider: &str) -> Result<SecretNameListing, CoreError> {
        self.0.secret_names(provider).await
    }
}
