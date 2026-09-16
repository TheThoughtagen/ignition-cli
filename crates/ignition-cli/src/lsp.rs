//! `ign lsp` — the LSP transport over stdio (14-03).
//!
//! Completes the transport pattern the MCP slice (14-01) proved, for the
//! second protocol consumer: OutOfBand seam, Session-only auth, cache-only
//! handlers.
//!
//! ## Ownership split (the Don't-Hand-Roll table)
//!
//! [`lsp_server`] owns the fiddly base protocol: Content-Length framing,
//! the initialize handshake, the shutdown handshake, and the stdio
//! reader/writer threads. WE own the sync dispatch loop on the main
//! thread — a plain `for msg in &connection.receiver` with NO runtime, NO
//! block_on, and NO network anywhere on the request path (SC-3's
//! structural half).
//!
//! ## The runtime story (the roadmap flag, resolved)
//!
//! `block_on` is legal on exactly ONE thread in this module: the
//! background refresher thread that owns the gateway cache (Task 2). The
//! LSP loop itself never touches a runtime — handlers read a TTL snapshot
//! behind an `Arc<RwLock<Arc<Snapshot>>>` and return.
//!
//! ## Capabilities: narrow BY DESIGN (composition, not replacement)
//!
//! Declared: `completion`, `hover`, full-text sync. NEVER claimed:
//! definition / codeAction / workspace symbols — Python ignition-lsp owns
//! those statics, and nvim merges completions across attached clients.
//! Doubling a capability makes nvim answer "go to definition" from two
//! servers and degrades the composition (research Anti-Pattern 5).
//!
//! ## Logging
//!
//! `init_tracing` (main.rs) already installed the stderr writer before
//! this seam runs — stderr doubles as the LSP log channel per the LSP
//! convention. No new logging surface; stdout carries ONLY framed
//! protocol bytes (a stray byte would kill the client).

use std::collections::BTreeMap;
use std::process::ExitCode;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    CompletionOptions, HoverProviderCapability, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind,
};

use ignition_core::client::GatewayApi as _;
use ignition_core::client::query::ListQuery;
use ignition_core::session::Session;

/// Serve LSP over stdio. The OutOfBand exit seam: SUCCESS on a clean
/// client-driven shutdown (or protocol-loop end), FAILURE with the error
/// on stderr for any protocol death. Nothing on the request path reads
/// config or the network — the ambient profile flag rides to the
/// GatewayCache refresher thread, whose failures degrade the snapshot
/// instead of the server.
pub fn serve(profile_flag: Option<&str>) -> ExitCode {
    match run(profile_flag) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            tracing::error!("lsp: protocol error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// The sync dispatch loop. Shape is FINAL — 14-04's handlers only fill
/// function bodies.
fn run(profile_flag: Option<&str>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (connection, io_threads) = Connection::stdio();
    let caps = serde_json::to_value(server_capabilities())?;
    let _init_params = connection.initialize(caps)?;
    tracing::info!("lsp: handshake complete — serving over stdio");

    // The gateway truth plane: a background refresher owns ALL network
    // and the module's only runtime; the loop below never blocks on
    // either. `kicks` is didSave's refresh request (14-04 sends;
    // the refresher drains), `snapshots` carries each published
    // snapshot version for diagnostics publishing (14-04 consumes).
    let (kicks_tx, kicks_rx) = std::sync::mpsc::channel::<()>();
    let (cache, snapshots_rx) = GatewayCache::spawn(profile_flag, kicks_rx);
    let _kicks_tx = kicks_tx; // consumed by 14-04's didSave handler

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                // The shutdown handshake is lsp-server's too; `true` here
                // means "client sent shutdown" — answer it, then exit the
                // loop cleanly (the client follows with `exit`).
                if connection.handle_shutdown(&req)? {
                    tracing::info!("lsp: shutdown handshake complete");
                    return Ok(());
                }
                let resp = handle_request(req, &cache);
                connection.sender.send(Message::Response(resp))?;
            }
            Message::Notification(note) => {
                // Spec: a bare `exit` notification terminates the server.
                // (`initialized` is consumed by connection.initialize.)
                if note.method == "exit" {
                    return Ok(());
                }
                handle_notification(note, &cache);
            }
            // We never call the client, so a Response cannot arrive; the
            // arm exists for Message exhaustiveness.
            Message::Response(_) => {}
        }
    }
    let _ = snapshots_rx; // consumed by 14-04's diagnostics publisher
    io_threads.join()?;
    Ok(())
}

/// THE narrow capability surface. Two providers + full-text sync, nothing
/// else: see the module docs for the composition contract (ignition-lsp
/// owns definition/codeAction/workspace symbols — never claimed here).
fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec!["/".into(), ".".into()]),
            ..Default::default()
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        ..Default::default()
    }
}

/// Request dispatch. Cache-only (SC-3): the body reads the TTL snapshot
/// behind the read lock and returns — zero network, zero runtime. Stubs
/// in 14-03; 14-04 fills the bodies against the GatewayCache.
fn handle_request(req: Request, cache: &GatewayCache) -> Response {
    // THE read path (SC-3's structural half): ONE Arc clone under the
    // lock, then the frame is read lock-free. The health/age log IS the
    // request-path evidence: what a request can know is what this frame
    // carries, nothing else.
    let snap = cache.snapshot();
    tracing::debug!(
        healthy = snap.healthy,
        providers = snap.providers.len(),
        tags = snap.tag_trees.values().map(Vec::len).sum::<usize>(),
        queries = snap.named_queries.values().map(Vec::len).sum::<usize>(),
        projects = snap.projects.len(),
        age = ?snap.fetched_at.elapsed(),
        "lsp: answering from cache snapshot (zero network on the request path)"
    );
    match req.method.as_str() {
        "textDocument/completion" => {
            Response::new_ok(req.id, Vec::<lsp_types::CompletionItem>::new())
        }
        "textDocument/hover" => Response::new_ok(req.id, serde_json::Value::Null),
        other => method_not_found(req.id, other),
    }
}

/// Notification dispatch — no-ops in 14-03 (the didSave→kick wire-up is
/// 14-04's; the kick channel already exists in `run`).
fn handle_notification(_note: Notification, _cache: &GatewayCache) {}

/// LSP error code -32601 for anything the narrow surface never claimed —
/// including definition/codeAction/workspace symbols, which stay
/// unanswered so nvim's OTHER attached client owns them.
fn method_not_found(id: RequestId, method: &str) -> Response {
    Response::new_err(
        id,
        -32601, // MethodNotFound
        format!(
            "ign lsp does not provide {method} (narrow-by-design surface: \
             completion + hover; statics belong to ignition-lsp)"
        ),
    )
}

// ---------------------------------------------------------------------------
// The gateway truth plane: TTL snapshot cache + background refresher
// ---------------------------------------------------------------------------

/// The refresh cadence — gateway truth is AT LEAST this stale for every
/// consumer (hover/diagnostic text carries the fetched-at age so the
/// staleness is stamped into the payload, Pitfall 4). A documented const:
/// UAT may tune it; the test pins the current value.
const LSP_CACHE_TTL: Duration = Duration::from_secs(30);

/// The project hosting the CLI's deployed WebDev routes — the same default
/// every `ign tags` verb uses (the routes live in that project by design,
/// `ign webdev deploy` deploys there). One const, one reason.
const TAGS_ROUTE_PROJECT: &str = "ign-cli";

/// An upper bound on tags browsed per provider per cycle — a pathological
/// tree (or a runaway recursive provider) cannot turn the refresher into
/// an infinite walker. The cap is a floor for honesty: a tree this big is
/// reported truncated, never silently cut.
const TAG_BROWSE_NODE_CAP: usize = 5_000;

/// One published frame of gateway truth. The ONLY shape LSP handlers may
/// read (SC-3): if a fact is not in here, a request cannot reach for it.
#[derive(Debug, Clone)]
struct Snapshot {
    /// Tag provider names (`[default]`, …) — the completion root.
    providers: Vec<String>,
    /// Provider → flattened tag paths (recursively browsed, properties
    /// excluded) — the completion tree and the tag-path diagnostic oracle.
    tag_trees: BTreeMap<String, Vec<String>>,
    /// Project → named-query resource paths (from the project export's
    /// member list) — completion for query references.
    named_queries: BTreeMap<String, Vec<String>>,
    /// Runnable project names.
    projects: Vec<String>,
    /// When this frame was fetched — the staleness stamp consumers show.
    fetched_at: Instant,
    /// True only when EVERY section of this frame fetched cleanly. A
    /// failed section keeps its PREVIOUS frame's data out — a partial
    /// snapshot beats none, and `healthy` says exactly how partial.
    healthy: bool,
}

impl Snapshot {
    /// The pre-first-publish frame: empty and explicitly unhealthy — no
    /// handler can mistake it for gateway truth.
    fn empty() -> Self {
        Self {
            providers: Vec::new(),
            tag_trees: BTreeMap::new(),
            named_queries: BTreeMap::new(),
            projects: Vec::new(),
            fetched_at: Instant::now(),
            healthy: false,
        }
    }
}

/// The TTL cache: `Arc<RwLock<Arc<Snapshot>>>` where the INNER Arc is the
/// handed-out frame. Readers clone the Arc under the lock (microseconds)
/// and then read without the lock; the refresher is the only writer.
/// The outer Arc lets the loop and the refresher thread share ONE cache.
#[derive(Clone)]
struct GatewayCache {
    inner: Arc<RwLock<Arc<Snapshot>>>,
}

impl GatewayCache {
    /// A cache seeded with [`Snapshot::empty`] — nothing is claimed as
    /// truth until the first publish.
    fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Arc::new(Snapshot::empty()))),
        }
    }

    /// THE read path: clone the current frame's Arc (the lock is held for
    /// the clone only) — handlers read the returned frame lock-free.
    fn snapshot(&self) -> Arc<Snapshot> {
        self.inner.read().expect("cache lock poisoned").clone()
    }

    /// THE write path (refresher only): swap the frame atomically. Readers
    /// holding the previous Arc keep a consistent old frame until they
    /// re-read — no torn reads, ever.
    fn publish(&self, snap: Snapshot) -> Arc<Snapshot> {
        let arc = Arc::new(snap);
        *self.inner.write().expect("cache lock poisoned") = Arc::clone(&arc);
        arc
    }
}

/// One populate cycle's raw sections. Each section is individually
/// fallible: a failed section stays empty and raises `failed_sections`
/// (partial snapshot beats none; `healthy` requires zero failures).
#[derive(Debug, Default, Clone)]
struct Collected {
    providers: Option<Vec<String>>,
    tag_trees: BTreeMap<String, Vec<String>>,
    named_queries: BTreeMap<String, Vec<String>>,
    projects: Option<Vec<String>>,
    failed_sections: u32,
}

/// The refresher's data seam — sync ON PURPOSE so the state-machine tests
/// inject fakes with no gateway and no runtime. Implementations run ONLY
/// on the refresher thread (never an LSP request); blocking here is
/// legal and expected.
trait SnapshotSource: Send + Sync + 'static {
    fn collect(&self) -> Collected;
}

/// Collapse one cycle's sections into a publishable frame: empty slots
/// where sections failed, `healthy` only when none did.
fn ingest(collected: &Collected) -> Snapshot {
    Snapshot {
        providers: collected.providers.clone().unwrap_or_default(),
        tag_trees: collected.tag_trees.clone(),
        named_queries: collected.named_queries.clone(),
        projects: collected.projects.clone().unwrap_or_default(),
        fetched_at: Instant::now(),
        healthy: collected.failed_sections == 0,
    }
}

/// One refresh cycle, PURE: collect → ingest → publish → announce. The
/// refresher thread calls it per cycle; the unit tests call it with fake
/// sources — identical code path either way.
fn refresh_once(
    source: &dyn SnapshotSource,
    cache: &GatewayCache,
    versions: &Sender<Arc<Snapshot>>,
) -> Arc<Snapshot> {
    let snap = ingest(&source.collect());
    let published = cache.publish(snap);
    // The diagnostics consumer (14-04) may be gone mid-teardown; a failed
    // announce is a no-op, never an error.
    let _ = versions.send(Arc::clone(&published));
    published
}

/// The live source: Session is the ONE client construction site (CORE-09)
/// and every section rides machinery the CLI already trusts — the native
/// provider/project lists, the deployed tags route's browse action, and
/// the project export zip's member list (research OQ3, planner-locked).
/// No new endpoint surface.
struct GatewaySource {
    /// `None` while the session has never resolved (or resolution keeps
    /// failing) — every cycle re-attempts resolution through the same
    /// construction site until it succeeds; a failed attempt publishes
    /// an unhealthy frame and the thread lives on.
    session: Option<Session>,
    /// The refresher thread's current-thread runtime, OWNED here so
    /// `Runtime::block_on` drives its reactor. (A cloned `Handle` would
    /// NOT: `Handle::block_on` into an idle current-thread runtime does
    /// not run its I/O driver — the connect future would hang forever.
    /// The runtime is moved back in on re-resolve; the live sections'
    /// futures are only ever polled on the refresher thread, never an
    /// LSP request — SC-3.)
    runtime: tokio::runtime::Runtime,
}

impl GatewaySource {
    /// Resolve the effective profile — ambient flag (the global
    /// `--profile`, IGNITION_PROFILE already folded by
    /// apply_env_defaults) — then config. Failure is DATA here (degrade
    /// soft), unlike the command chassis where it is exit 3.
    fn resolve(profile_flag: Option<&str>, runtime: tokio::runtime::Runtime) -> Self {
        let session = match Session::resolve(profile_flag) {
            Ok(session) => {
                tracing::info!(profile = %session.profile_name(), "lsp: gateway session resolved");
                Some(session)
            }
            Err(err) => {
                tracing::warn!("lsp: gateway session unresolved (editing degrades offline): {err}");
                None
            }
        };
        Self { session, runtime }
    }

    /// The async half: the four gateway reads, per-section fallible.
    /// Runs on the refresher thread's runtime ONLY.
    async fn fetch_sections(&self) -> Collected {
        let mut collected = Collected::default();
        let Some(session) = &self.session else {
            // Unresolved session: all four sections fail this cycle.
            collected.failed_sections = 4;
            return collected;
        };
        let api = session.api();

        // Section 1: providers (the native list).
        match api.tag_provider_list(&ListQuery::default()).await {
            Ok(page) => {
                collected.providers = Some(page.items.into_iter().map(|p| p.name).collect())
            }
            Err(err) => {
                tracing::warn!("lsp: provider section failed: {err}");
                collected.failed_sections += 1;
            }
        }

        // Section 2: tag trees (the deployed tags route's browse action,
        // recursed per provider — the SAME route `ign tags browse` uses;
        // one version precondition per cycle, not per node).
        if let Err(err) =
            ignition_core::actions::webdev::webdev_precondition(api, TAGS_ROUTE_PROJECT).await
        {
            tracing::warn!(
                "lsp: tags route precondition failed (deploy with `ign webdev deploy`?): {err}"
            );
            collected.failed_sections += 1;
        } else if let Some(providers) = &collected.providers {
            let mut tag_trees = BTreeMap::new();
            for provider in providers {
                let (paths, truncated) = browse_provider(api, provider, TAG_BROWSE_NODE_CAP).await;
                if truncated {
                    tracing::warn!(
                        "lsp: provider {provider} browse hit the {TAG_BROWSE_NODE_CAP}-node cap — tree truncated this cycle"
                    );
                }
                tag_trees.insert(provider.clone(), paths);
            }
            collected.tag_trees = tag_trees;
        } else {
            collected.failed_sections += 1;
        }

        // Section 3: projects (the native list) — needed by section 4.
        match api.projects(&ListQuery::default()).await {
            Ok(page) => collected.projects = Some(page.items.into_iter().map(|p| p.name).collect()),
            Err(err) => {
                tracing::warn!("lsp: projects section failed: {err}");
                collected.failed_sections += 1;
            }
        }

        // Section 4: named queries per project — the project export zip's
        // member list, `named-query/` prefixes (OQ3, planner-locked; the
        // CLI's own export path, no new endpoint). Skipped (and counted)
        // when the project list failed — there is nothing to enumerate.
        match &collected.projects {
            Some(projects) => {
                let mut named_queries = BTreeMap::new();
                for project in projects {
                    match ignition_core::actions::resources::export_zip_bytes(api, project).await {
                        Ok(zip) => {
                            let queries = ignition_core::client::resources::resource_members(&zip)
                                .unwrap_or_default()
                                .into_iter()
                                .filter(|path| path.starts_with("named-query/"))
                                .collect::<Vec<_>>();
                            named_queries.insert(project.clone(), queries);
                        }
                        Err(err) => {
                            tracing::warn!("lsp: named-query export for {project} failed: {err}");
                            collected.failed_sections += 1;
                        }
                    }
                }
                collected.named_queries = named_queries;
            }
            None => collected.failed_sections += 1,
        }

        collected
    }
}

impl SnapshotSource for GatewaySource {
    fn collect(&self) -> Collected {
        // `block_on` is legal HERE and ONLY here: the refresher thread
        // driving its OWN current-thread runtime, never an LSP request.
        // The LSP loop never touches a runtime (SC-3's structural
        // property).
        self.runtime.block_on(self.fetch_sections())
    }
}

/// Recursively browse one provider's tree over the deployed tags route —
/// the same `{"action":"browse","path":…}` call `ign tags browse` makes.
/// Returns the flattened tag paths (properties excluded) and whether the
/// node cap truncated the walk.
async fn browse_provider(
    api: &dyn ignition_core::client::GatewayApi,
    provider: &str,
    cap: usize,
) -> (Vec<String>, bool) {
    let mut paths = Vec::new();
    let mut queue = vec![format!("[{provider}]")];
    let mut truncated = false;
    while let Some(node) = queue.pop() {
        if paths.len() >= cap {
            truncated = true;
            break;
        }
        let body = serde_json::json!({"action": "browse", "path": node});
        let data = match api
            .webdev_route_call(TAGS_ROUTE_PROJECT, "tags", &body, &[])
            .await
        {
            Ok(data) => data,
            Err(err) => {
                tracing::warn!("lsp: browse {node} failed: {err}");
                continue;
            }
        };
        let Ok(entries) = serde_json::from_value::<Vec<ignition_core::client::tags::BrowseEntry>>(
            data["results"].clone(),
        ) else {
            tracing::warn!("lsp: browse {node} returned an unexpected shape");
            continue;
        };
        for entry in entries {
            if entry.tag_type == "Property" {
                continue;
            }
            if entry.has_children {
                queue.push(entry.full_path.clone());
            }
            paths.push(entry.full_path);
        }
    }
    (paths, truncated)
}

impl GatewayCache {
    /// Spawn the refresher thread: its OWN small current-thread runtime is
    /// built INSIDE the thread (the roadmap flag's resolution — `block_on`
    /// is legal THERE because it is not an LSP request), resolution rides
    /// [`GatewaySource::resolve`], the first populate publishes
    /// immediately, then one cycle per [`LSP_CACHE_TTL`]. Kicks are
    /// drained each cycle so a burst collapses to one refresh; every
    /// publish announces the new frame on the returned `versions`
    /// receiver (14-04's diagnostics publisher consumes it). The returned
    /// cache handle and the thread's share the ONE RwLock.
    fn spawn(profile_flag: Option<&str>, kicks: Receiver<()>) -> (Self, Receiver<Arc<Snapshot>>) {
        let (versions_tx, versions_rx) = std::sync::mpsc::channel::<Arc<Snapshot>>();
        let cache = Self::new();
        let thread_cache = cache.clone();
        let profile_flag = profile_flag.map(str::to_string);
        let spawned = thread::Builder::new()
            .name("ign-lsp-refresher".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("lsp refresher runtime");
                let mut source = GatewaySource::resolve(profile_flag.as_deref(), runtime);
                loop {
                    // Re-attempt resolution through the SAME construction
                    // site while it keeps failing (a gateway that comes
                    // back is seen next cycle; the happy path holds one
                    // client for the server's lifetime). The runtime moves
                    // back into the rebuilt source — the source owns it.
                    if source.session.is_none() {
                        source = GatewaySource::resolve(profile_flag.as_deref(), source.runtime);
                    }
                    // Drain kicks: a didSave burst collapses into THIS
                    // cycle's one refresh.
                    while kicks.try_recv().is_ok() {}
                    refresh_once(&source, &thread_cache, &versions_tx);
                    thread::sleep(LSP_CACHE_TTL);
                }
            });
        // The thread failing to spawn must not kill the server: an
        // unstarted refresher publishes nothing, the cache stays empty +
        // unhealthy, and handlers answer from that honestly.
        if let Err(err) = spawned {
            tracing::error!("lsp: refresher thread failed to spawn (cache stays unhealthy): {err}");
        }
        (cache, versions_rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_are_narrow_by_design() {
        let caps = server_capabilities();
        assert!(caps.completion_provider.is_some(), "completion claimed");
        assert!(caps.hover_provider.is_some(), "hover claimed");
        assert!(
            matches!(
                caps.text_document_sync,
                Some(lsp_types::TextDocumentSyncCapability::Kind(
                    lsp_types::TextDocumentSyncKind::FULL
                ))
            ),
            "full-text sync claimed"
        );
        // The composition anti-pattern is a CAPABILITY claim: these three
        // must stay absent so ignition-lsp's statics surface is untouched.
        assert!(
            caps.definition_provider.is_none(),
            "definition never claimed"
        );
        assert!(
            caps.code_action_provider.is_none(),
            "codeAction never claimed"
        );
        assert!(
            caps.workspace_symbol_provider.is_none(),
            "workspace symbols never claimed"
        );
    }

    #[test]
    fn completion_stub_returns_an_empty_list() {
        let cache = GatewayCache::new();
        let req = Request::new(
            RequestId::from(1),
            "textDocument/completion".into(),
            serde_json::Value::Null,
        );
        let resp = handle_request(req, &cache);
        assert_eq!(resp.id, RequestId::from(1));
        let result = resp
            .response_result
            .expect("completion stub answers without error");
        assert_eq!(result, serde_json::Value::Array(vec![]));
    }

    #[test]
    fn hover_stub_answers_null() {
        let cache = GatewayCache::new();
        let req = Request::new(
            RequestId::from(2),
            "textDocument/hover".into(),
            serde_json::Value::Null,
        );
        let resp = handle_request(req, &cache);
        let result = resp
            .response_result
            .expect("hover stub answers without error");
        assert_eq!(result, serde_json::Value::Null);
    }

    #[test]
    fn unclaimed_methods_refuse_method_not_found() {
        let cache = GatewayCache::new();
        for method in [
            "textDocument/definition",
            "textDocument/codeAction",
            "workspace/symbol",
            "totally/bogus",
        ] {
            let req = Request::new(RequestId::from(3), method.into(), serde_json::Value::Null);
            let resp = handle_request(req, &cache);
            let err = resp.response_result.expect_err("unclaimed method refuses");
            assert_eq!(err.code, -32601, "{method} refuses MethodNotFound");
        }
    }

    // ------------------------------------------------------------------
    // The gateway truth plane (Task 2): swap semantics, TTL pin,
    // degrade-soft state machine
    // ------------------------------------------------------------------

    /// A publishable frame with just the fields a test cares about.
    fn frame(providers: &[&str], healthy: bool) -> Snapshot {
        Snapshot {
            providers: providers.iter().map(|s| (*s).to_string()).collect(),
            tag_trees: BTreeMap::new(),
            named_queries: BTreeMap::new(),
            projects: Vec::new(),
            fetched_at: Instant::now(),
            healthy,
        }
    }

    /// A fake populate result: everything fetched cleanly.
    fn healthy_sections() -> Collected {
        Collected {
            providers: Some(vec!["default".into()]),
            tag_trees: BTreeMap::from([(
                "default".into(),
                vec!["[default]T1".into(), "[default]P5".into()],
            )]),
            named_queries: BTreeMap::from([("proj".into(), vec!["named-query/q".into()])]),
            projects: Some(vec!["proj".into()]),
            failed_sections: 0,
        }
    }

    /// A fake populate result: the gateway is unreachable — the live
    /// source's unresolved-session shape (all four sections failed).
    fn unreachable_sections() -> Collected {
        Collected {
            failed_sections: 4,
            ..Default::default()
        }
    }

    /// A fake populate result: ONE section failed (providers ok, tags
    /// not) — partial snapshot beats none, healthy says how partial.
    fn partial_sections() -> Collected {
        Collected {
            providers: Some(vec!["default".into()]),
            failed_sections: 1,
            ..Default::default()
        }
    }

    struct FakeSource(Collected);

    impl SnapshotSource for FakeSource {
        fn collect(&self) -> Collected {
            self.0.clone()
        }
    }

    #[test]
    fn ttl_const_is_pinned_at_thirty_seconds() {
        // The documented cadence — changing this needs a conscious edit
        // here (UAT may tune; the pin makes the change deliberate).
        assert_eq!(LSP_CACHE_TTL, Duration::from_secs(30));
    }

    #[test]
    fn snapshot_swap_semantics_readers_see_old_until_publish() {
        let cache = GatewayCache::new();

        // Before the first publish: empty + explicitly unhealthy — no
        // handler can mistake the seed for gateway truth.
        let seed = cache.snapshot();
        assert!(!seed.healthy);
        assert!(seed.providers.is_empty());

        // Publish frame A; a reader clones the current Arc.
        cache.publish(frame(&["default"], true));
        let held = cache.snapshot();
        assert_eq!(held.providers, ["default"]);
        assert!(held.healthy);

        // Publish frame B: the live view moves, the reader's clone does
        // not — no torn reads, ever.
        cache.publish(frame(&["other"], false));
        let now = cache.snapshot();
        assert_eq!(now.providers, ["other"]);
        assert!(!now.healthy);
        assert_eq!(held.providers, ["default"], "held readers keep frame A");
        // The staleness stamp is monotonic across publishes — 14-04's
        // hover text renders this age (Pitfall 4), so the pin guards it.
        assert!(held.fetched_at <= now.fetched_at);
    }

    #[test]
    fn refresh_once_announces_the_published_version() {
        let cache = GatewayCache::new();
        let (tx, rx) = std::sync::mpsc::channel();
        let published = refresh_once(&FakeSource(healthy_sections()), &cache, &tx);
        assert_eq!(cache.snapshot().providers, published.providers);
        let announced = rx.recv().expect("publish announces its version");
        assert_eq!(announced.providers, published.providers);
    }

    /// THE anti-sabotage pin, both directions: a failing source publishes
    /// an UNHEALTHY frame (editing offline degrades, never lies), and the
    /// same machinery RECOVERS to healthy the cycle the source succeeds —
    /// the server survives the outage and returns to truth.
    #[test]
    fn degrade_soft_failure_then_recovery() {
        let cache = GatewayCache::new();
        let (tx, _rx) = std::sync::mpsc::channel();

        // Sabotage: gateway unreachable → healthy:false, no fake data.
        let degraded = refresh_once(&FakeSource(unreachable_sections()), &cache, &tx);
        assert!(!degraded.healthy, "failure must mark the frame unhealthy");
        assert!(degraded.providers.is_empty());
        let live = cache.snapshot();
        assert!(!live.healthy);
        assert!(live.providers.is_empty());

        // Restore: the same cache, a succeeding source → healthy:true,
        // the sections land.
        let recovered = refresh_once(&FakeSource(healthy_sections()), &cache, &tx);
        assert!(recovered.healthy, "recovery must restore healthy");
        assert_eq!(recovered.providers, ["default"]);
        assert_eq!(
            recovered.tag_trees.get("default").map(Vec::as_slice),
            Some(&["[default]T1".to_string(), "[default]P5".to_string()][..])
        );
        assert_eq!(
            recovered.named_queries.get("proj").map(Vec::as_slice),
            Some(&["named-query/q".to_string()][..])
        );
        assert_eq!(recovered.projects, ["proj"]);
        let live = cache.snapshot();
        assert!(live.healthy);
        assert_eq!(live.providers, ["default"]);
    }

    /// One failing section never zeroes the others: providers survive,
    /// the failed section is empty, and healthy is false (all-sections-ok
    /// is the only healthy).
    #[test]
    fn partial_snapshot_keeps_the_surviving_sections() {
        let cache = GatewayCache::new();
        let (tx, _rx) = std::sync::mpsc::channel();
        let partial = refresh_once(&FakeSource(partial_sections()), &cache, &tx);
        assert!(!partial.healthy, "one failed section ⇒ not healthy");
        assert_eq!(partial.providers, ["default"], "surviving section kept");
        assert!(partial.tag_trees.is_empty(), "failed section empty");
        assert!(partial.named_queries.is_empty());
        assert!(partial.projects.is_empty());
    }

    /// The pre-first-publish seed cannot masquerade as truth: empty and
    /// unhealthy by construction (the refresher-spawn failure path leans
    /// on this — an unstarted refresher leaves this frame in place).
    #[test]
    fn seed_frame_is_empty_and_unhealthy() {
        let snap = Snapshot::empty();
        assert!(!snap.healthy);
        assert!(snap.providers.is_empty());
        assert!(snap.tag_trees.is_empty());
        assert!(snap.named_queries.is_empty());
        assert!(snap.projects.is_empty());
    }
}
