//! `FaultyHost`: a call-granularity fault and hang wrapper for any
//! [`ApplyHost`] (Phase 23, D-01, research Pattern 7).
//!
//! Where `ApplyFault` on the local host fails one file move, `FaultyHost`
//! works one level up, on whole host calls, so the engine plans can:
//!
//! - fail `commit`, `restore` or `finalize` on their nth invocation (the inner
//!   host is not called, so the host state stays as it was),
//! - hang the nth invocation at an await point a test can abort,
//! - run an async hook after the nth invocation returned and before the
//!   caller sees its result (tick a fake gateway past the hold, rewrite a
//!   file, sleep past the hold), and
//! - read back the call sequence.
//!
//! Invocations are counted per call name from 1. A clone shares the wrapped
//! host and every rule and record with the original, so a test keeps one clone
//! to read [`FaultyHost::calls`] after boxing another into the engine.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use ignition_core::deploy::apply::host::{
    ApplyHost, BatchReport, BatchSpec, Prepared, ProbeReport,
};
use ignition_core::deploy::apply::record::RunRecord;
use ignition_core::deploy::apply::run_store::RunId;
use ignition_core::deploy::tree::RenderedTree;
use ignition_core::error::CoreError;
use tokio::sync::Notify;

/// An async hook run after a call returned.
type Hook = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

#[derive(Default)]
struct State {
    /// Invocations so far, by call name.
    counts: BTreeMap<&'static str, usize>,
    /// Every call, in order.
    log: Vec<&'static str>,
    fails: BTreeSet<(&'static str, usize)>,
    hangs: BTreeMap<(&'static str, usize), Arc<Notify>>,
    hooks: BTreeMap<(&'static str, usize), Vec<Hook>>,
}

/// What a call does, decided when it is entered.
enum Plan {
    Fail,
    Hang(Arc<Notify>),
    Run(Vec<Hook>),
}

/// Wraps `H`, delegating every method unless a rule names the call and its
/// invocation number.
pub struct FaultyHost<H: ApplyHost> {
    inner: Arc<H>,
    state: Arc<Mutex<State>>,
}

impl<H: ApplyHost> Clone for FaultyHost<H> {
    fn clone(&self) -> Self {
        FaultyHost {
            inner: self.inner.clone(),
            state: self.state.clone(),
        }
    }
}

impl<H: ApplyHost> FaultyHost<H> {
    pub fn new(inner: H) -> Self {
        FaultyHost {
            inner: Arc::new(inner),
            state: Arc::new(Mutex::new(State::default())),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("faulty host state")
    }

    /// Fail invocation `nth` (from 1) of `call` with `transport_failed`. The
    /// inner host is not called.
    pub fn fail(self, call: &'static str, nth: usize) -> Self {
        self.state().fails.insert((call, nth));
        self
    }

    /// Hang invocation `nth` of `call`: `notify` is signalled (a permit is
    /// stored, so a test that starts waiting later still sees it) and the call
    /// then waits forever at an await point, without calling the inner host.
    /// The test aborts the task that made the call.
    pub fn hang(self, call: &'static str, nth: usize, notify: Arc<Notify>) -> Self {
        self.state().hangs.insert((call, nth), notify);
        self
    }

    /// Run `hook` after invocation `nth` of `call` returned and before the
    /// caller sees its result (also when the inner call failed). Several hooks
    /// on one invocation run in the order they were added.
    pub fn after<F, Fut>(self, call: &'static str, nth: usize, hook: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let hook: Hook = Arc::new(move || Box::pin(hook()));
        self.state()
            .hooks
            .entry((call, nth))
            .or_default()
            .push(hook);
        self
    }

    /// Every call made so far, in order, by name.
    pub fn calls(&self) -> Vec<&'static str> {
        self.state().log.clone()
    }

    fn enter(&self, call: &'static str) -> Plan {
        let mut state = self.state();
        let nth = {
            let count = state.counts.entry(call).or_insert(0);
            *count += 1;
            *count
        };
        state.log.push(call);
        if state.fails.contains(&(call, nth)) {
            return Plan::Fail;
        }
        if let Some(notify) = state.hangs.get(&(call, nth)) {
            return Plan::Hang(notify.clone());
        }
        Plan::Run(state.hooks.get(&(call, nth)).cloned().unwrap_or_default())
    }

    async fn call<T, Fut>(
        &self,
        call: &'static str,
        run: impl FnOnce(Arc<H>) -> Fut,
    ) -> Result<T, CoreError>
    where
        Fut: Future<Output = Result<T, CoreError>>,
    {
        match self.enter(call) {
            Plan::Fail => Err(CoreError::TransportFailed {
                operation: call.to_string(),
                node: "faulty-host".to_string(),
                detail: "injected fault".to_string(),
            }),
            Plan::Hang(notify) => {
                notify.notify_one();
                std::future::pending::<Result<T, CoreError>>().await
            }
            Plan::Run(hooks) => {
                let out = run(self.inner.clone()).await;
                for hook in hooks {
                    hook().await;
                }
                out
            }
        }
    }
}

#[async_trait::async_trait]
impl<H: ApplyHost> ApplyHost for FaultyHost<H> {
    async fn probe(&self) -> Result<ProbeReport, CoreError> {
        self.call("probe", |inner| async move { inner.probe().await })
            .await
    }

    async fn prepare(&self, spec: &BatchSpec, tree: &RenderedTree) -> Result<Prepared, CoreError> {
        self.call(
            "prepare",
            |inner| async move { inner.prepare(spec, tree).await },
        )
        .await
    }

    async fn commit(&self, prepared: &Prepared) -> Result<BatchReport, CoreError> {
        self.call(
            "commit",
            |inner| async move { inner.commit(prepared).await },
        )
        .await
    }

    async fn restore(&self, record: &RunRecord) -> Result<BatchReport, CoreError> {
        self.call(
            "restore",
            |inner| async move { inner.restore(record).await },
        )
        .await
    }

    async fn fingerprint(
        &self,
        paths: &[String],
    ) -> Result<BTreeMap<String, Option<String>>, CoreError> {
        self.call("fingerprint", |inner| async move {
            inner.fingerprint(paths).await
        })
        .await
    }

    async fn finalize(&self, record: &RunRecord) -> Result<(), CoreError> {
        self.call(
            "finalize",
            |inner| async move { inner.finalize(record).await },
        )
        .await
    }

    async fn read_record(&self, run_id: &RunId) -> Result<Option<RunRecord>, CoreError> {
        self.call("read_record", |inner| async move {
            inner.read_record(run_id).await
        })
        .await
    }

    async fn fetch_snapshot(
        &self,
        run_id: &RunId,
        dest: &Path,
    ) -> Result<BTreeMap<String, String>, CoreError> {
        self.call("fetch_snapshot", |inner| async move {
            inner.fetch_snapshot(run_id, dest).await
        })
        .await
    }

    async fn discard_interrupted(&self, run_id: &RunId) -> Result<(), CoreError> {
        self.call("discard_interrupted", |inner| async move {
            inner.discard_interrupted(run_id).await
        })
        .await
    }
}
