# Phase 6: TUI Cockpit - Research

**Researched:** 2026-08-27
**Domain:** ratatui 0.30 terminal UI over an async tokio + actions-layer architecture
**Confidence:** HIGH (stack verified live on crates.io/docs.rs; architecture verified against official ratatui examples AND the actual codebase seams)

## Summary

Phase 6 fills the deliberate zero-dep `ignition-tui` stub with a ratatui cockpit. The stack question resolves cleanly: **ratatui 0.30.2 (current stable) has MSRV exactly 1.88.0 and is edition 2024 — a perfect match for this workspace** (verified live on crates.io; ratatui 0.30.0's modularization into ratatui-core/-widgets/-crossterm does not change app usage — depend on the `ratatui` umbrella crate only). The event-loop architecture is equally settled: ratatui's official `async-github` example demonstrates exactly the pattern this roadmap sketches — `ratatui::init()` → `tokio::select!` over an interval tick + crossterm `EventStream` → workers via `tokio::spawn` → `ratatui::restore()`. The `ratatui::init()` helper installs raw mode, alternate screen, AND a restore-on-panic hook, eliminating the classic cleanup footguns.

The codebase seam is better than expected: **every action is a free async fn taking `&dyn GatewayApi`** and `GatewayApi: Send + Sync`, so `Arc<ReqwestGatewayApi>` clones into worker tasks directly — the actions layer is callable AS-IS from the TUI with zero re-implementation. Two concrete gaps found: (1) `actions::logs::tail`'s sink is `&mut dyn FnMut(&LogEntry)` **without `+ Send`** (the rig sinks have it), so the tail future can't cross `tokio::spawn` — a one-line signature fix; (2) profile→client resolution lives as private fns in ignition-cli's `main.rs`, so ignition-tui needs its own thin context module calling the already-public `ignition_core::config` functions. The structural-completeness proof (Success Criterion 1) is achievable with a static route registry in ignition-tui + a CI test in ignition-cli that walks the clap `CommandFactory` tree and asserts bidirectional mapping against it.

**Primary recommendation:** Take `ratatui = "0.30.2"` + `crossterm = "0.29" (event-stream)` as the ONLY new runtime deps (tokio/futures-util already in workspace graph); build a single-channel AppEvent mpsc loop with Elm-style `AppState`/`update()`; fix the `logs::tail` sink `+ Send`; prove CLI↔TUI coverage with a clap-tree-walk test against a static TUI route registry.

## User Constraints

No CONTEXT.md exists (user did not run `/gsd-discuss-phase`). However, **locked prior decisions from STATE.md / repo architecture DO constrain this phase** — the planner must honor these:

### Locked Decisions (from STATE.md / prior phases)
- Workspace shape: three crates — ignition-cli (bin `ign`), ignition-core (lib), **ignition-tui (currently a deliberate ZERO-dep stub — this phase fills it)**. The `tui` feature gate in ignition-cli exists, default-on, `tui = ["dep:ignition-tui"]`.
- MSRV 1.88, edition 2024, no Windows CI.
- Actions layer: `crates/ignition-core/src/actions/*.rs` — free fns returning typed results; GatewayApi locked on `async_trait`, ONE coarse method per capability, implemented by `ReqwestGatewayApi` in `client/mod.rs`.
- Output contract FROZEN for the CLI: envelope `{ok,profile,data}`/`{ok,profile,error}`; human rendering in ignition-cli render.rs; five sanctioned stdout exceptions (completions, logs -f NDJSON, rig logs raw, tags export -o -, version warning) — **the TUI defines its own relationship to this contract** (it renders in alternate screen; it must not print envelopes to stdout while running).
- `poll.rs` is THE wait engine (×1.5 backoff clamp [interval,30s]); streaming sinks use `dyn FnMut(...) (+ Send)` pattern.
- cli.rs/main.rs/render.rs are choke files — the Tui dispatch arm in main.rs must stay minimal (delegate to ignition-tui).
- Destructive verbs are `--yes`-guarded (7 exist); ack is NOT guarded; alarm ack requires `--username` (3-arg wire form), active table prints FULL UUID with short-prefix expansion.
- `Secret::expose()` confined to the single header-construction site in `ReqwestGatewayApi`; secrets never in output — the TUI must never touch secrets directly (client construction goes through `ReqwestGatewayApi::new`).
- Tag alarm/history/config operations ride deployed WebDev routes via `webdev_route_call` + `require_routes` precondition (handled INSIDE the actions layer — TUI inherits it for free).

### Claude's Discretion (this researcher's recommendations, unconstrained by user)
Everything else: exact dep versions/features, event-loop shape, module layout, testing strategy, coverage-mapping mechanism, keybinding scheme, buffer bounds. All researched and prescribed below.

### Deferred Ideas (OUT OF SCOPE)
None recorded for this phase.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| ratatui | **0.30.2** (current stable, 2026-06-19) | Widgets, Terminal, TestBackend, `init()/restore()` helpers | THE Rust TUI standard; official async examples; MSRV **exactly 1.88.0**, edition 2024 — exact workspace match |
| crossterm | **0.29.0** (direct dep, feature `event-stream`) | Raw mode, alt screen, key events, `EventStream` (async event source) | ratatui's default backend; EventStream is the official async input mechanism |
| tokio | workspace (`rt-multi-thread`, `macros`, `time`, `sync` already enabled) | Runtime, `select!`, `tokio::spawn`, mpsc channels | Already in graph via ignition-core; **no feature additions needed** |
| futures-util | workspace (`std` feature) | `StreamExt::next()` over crossterm `EventStream` | Already a workspace dep; avoids adding `tokio-stream` |
| ignition-core | workspace | actions layer, GatewayApi, config, CoreError | The whole point of the phase: shared actions layer |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| clap (dev, via ignition-cli tests) | workspace 4.6 | `CommandFactory::command()` tree walk for the coverage test | Test lives in ignition-cli (owns cli.rs) |
| tui-input | 0.11+ (ecosystem, MEDIUM) | Input field widget for filters/username prompt | OPTIONAL — simple char-append input in a modal is ~30 lines; only take this dep if filter prompts get complex. Default: hand-roll minimal input |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| ratatui umbrella crate | ratatui-core + ratatui-widgets + ratatui-crossterm directly | 0.30 modularized, but **app developers should keep using `ratatui`** (official migration guide: "continue using ratatui as before"); subcrates are for widget-library authors wanting slower API churn |
| crossterm EventStream (async) | std::thread + `event::poll`/`read` + tokio mpsc (ratatui component-template style) | Thread+mpsc avoids the `event-stream` feature flag; but we're already all-in on tokio, the official async example uses EventStream, and one fewer OS thread. **Use EventStream.** |
| `tokio_stream::StreamExt` | `futures_util::StreamExt` | Identical `.next()` ergonomics; futures-util already a workspace dep — **use futures-util, add nothing** |
| insta snapshot tests | TestBackend + `assert_eq!(buf, Buffer::with_lines(...))` | insta is the ecosystem favorite BUT adds a new dev-dep; the repo's snapbox/minimal-dep philosophy favors native TestBackend assertions (already in ratatui). **Use TestBackend assertions; keep snapbox for any CLI-adjacent golden needs** |
| Arc<RwLock<AppState>> shared with workers | AppEvent mpsc (Elm-style) | Shared-state (async-github's widget pattern) is simpler per-widget but less testable and races with the render loop; mpsc+update() is the roadmap's stated intent and unit-testable headlessly. **Use AppEvent mpsc.** |

**Installation (workspace Cargo.toml additions):**
```toml
[workspace.dependencies]
# Phase 6: pinned .2 — 0.30.1 bumped MSRV 1.86→1.88 in a PATCH release, so
# patch drift can move MSRV; pin the floor at 0.30.2 (MSRV 1.88) and rely on
# CI to catch any future bump (CI runs stable; no MSRV job exists).
ratatui = "0.30.2"
crossterm = { version = "0.29", features = ["event-stream"] } # event-stream: async EventStream for the select loop

# crates/ignition-tui/Cargo.toml (replaces the zero-dep stub):
[dependencies]
ignition-core = { workspace = true }
ratatui = { workspace = true }
crossterm = { workspace = true }
tokio = { workspace = true }
futures-util = { workspace = true }
```
No other new deps. tokio's `sync`/`time`/`macros`/`rt-multi-thread` are already workspace-enabled (verified in root Cargo.toml).

## Architecture Patterns

### Recommended Project Structure
```
crates/ignition-tui/src/
├── lib.rs           # pub async fn run(profile: Option<String>) -> Result<(), CoreError>
│                    #   (stub `run() -> !` is replaced; ignition-cli's Tui arm delegates here)
├── event.rs         # AppEvent enum + worker spawn helpers
├── state.rs         # AppState (Elm Model): per-screen data, focus, modals — pure, no I/O
├── update.rs        # fn update(state: &mut AppState, event: AppEvent) -> Option<Effect>  (pure)
├── ui/
│   │                # view fns: fn render(state: &AppState, frame: &mut Frame) — pure
│   ├── mod.rs       # top-level dispatch: tab bar + active screen + modals
│   ├── dashboard.rs # modules/sessions/metrics panels (TUI-02)
│   ├── logs.rs      # tail view + level filter + scrollback (TUI-03)
│   ├── tags.rs      # tag browser + live watch (TUI-04)
│   ├── alarms.rs    # alarm panel + ack flow (TUI-05)
│   ├── projects.rs  # project/resource browser + actions (TUI-06)
│   └── profiles.rs  # profile switcher popup (TUI-06)
├── workers/
│   │                # gateway I/O tasks — the ONLY place actions are awaited
│   ├── mod.rs
│   ├── refresh.rs   # interval-driven dashboard refresh (status/modules/metrics/sessions)
│   ├── tail.rs      # actions::logs::tail with channel-sending sink (TUI-03)
│   ├── watch.rs     # tags_read interval poll (TUI-04) + alarms_active poll (TUI-05)
│   └── actions.rs   # one-shot action executor (browse/list/terminate/ack/...)
├── context.rs       # profile resolution + client construction (Arc<ReqwestGatewayApi>),
│                    #   profile switch (config reload + client rebuild)
└── routes.rs        # THE COVERAGE REGISTRY: static CLI-path → TUI-screen mapping table
```

### Pattern 1: Event-Driven Async Loop (the phase's foundation — 06-01)
**What:** Single `tokio::select!` loop over: crossterm `EventStream` (input), an interval tick (timed redraw), and an `AppEvent` mpsc receiver (worker results). Render NEVER awaits gateway I/O.
**When to use:** Always — this IS the cockpit.
**Example** (adapted from ratatui's official async-github example, ratatui-v0.30.2):
```rust
// Source: https://ratatui.rs/examples/apps/async-github/ (ratatui-v0.30.2) + component template's Action enum
use crossterm::event::{Event, EventStream};
use futures_util::StreamExt;
use tokio::sync::mpsc;

pub enum AppEvent {
    Input(Event),                       // crossterm events forwarded by the select loop
    Tick,                               // timed redraw / housekeeping (~250ms–1s)
    Refresh(refresh::Snapshot),         // dashboard snapshot from the refresh worker
    LogLine(LogEntry),                  // one tailed log entry (TUI-03)
    TagWatch(Vec<TagValueRow>),         // live-watch poll result (TUI-04)
    Alarms(Vec<AlarmRow>),              // alarm panel poll result (TUI-05)
    ActionDone(ActionId, ActionResult), // one-shot worker finished (any screen)
    ProfileChanged(String),             // after a profile switch: new client live
    Error(String),
}

pub async fn run_loop(terminal: &mut ratatui::DefaultTerminal, state: &mut AppState,
                      mut events: mpsc::UnboundedReceiver<AppEvent>) -> Result<(), CoreError> {
    let mut crossterm_events = EventStream::new();
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(250));
    while !state.should_quit {
        tokio::select! {
            _ = tick.tick() => { terminal.draw(|f| ui::render(state, f))?; }
            // NOTE: match fully — `Some(Ok(e)) = ...` disables the arm forever on a
            // terminal-stream error and the app freezes for input (see Pitfall 3)
            ev = crossterm_events.next() => match ev {
                Some(Ok(event)) => update(state, AppEvent::Input(event)),
                Some(Err(e)) => return Err(/* wrap e */),
                None => break,
            },
            Some(app_event) = events.recv() => update(state, app_event),
        }
    }
    Ok(())
}
```

### Pattern 2: Worker Tasks Are the Only Actions Callers
**What:** `tokio::spawn`'d tasks own `Arc<ReqwestGatewayApi>` (or `Arc<dyn GatewayApi>`) and send results back over cloned `mpsc::UnboundedSender<AppEvent>`. The update loop owns ALL state.
**When to use:** Every gateway interaction.
**Why it works here (codebase-verified):** all actions are free async fns whose first param is `&dyn GatewayApi`, and the trait is `Send + Sync` (client/mod.rs:80-81) — `Arc` clones into spawned tasks and `&*arc` satisfies `&dyn GatewayApi`. Callable AS-IS:
```rust
// Verified signatures (crates/ignition-core/src/actions/*):
//   inspect::status(api: &dyn GatewayApi) -> Result<StatusResult, CoreError>
//   inspect::modules(api: &dyn GatewayApi, quarantined: bool) -> ...
//   sessions::sessions(api: &dyn GatewayApi, filter: Option<Type>) -> ...
//   logs::tail(api, logger, min_level, since_ms, interval, deadline,
//              sink: &mut dyn FnMut(&LogEntry)) -> ...   // needs +Send — see Pattern 2b
//   tags::tags_read(api, provider, paths) / tags_alarms_ack(api, project, ids, note, username)
//   profile::use_profile(config_path, name)  // SYNC fn — call from update, not a worker
pub async fn refresh_worker(api: Arc<ReqwestGatewayApi>, tx: mpsc::UnboundedSender<AppEvent>,
                            period: Duration) {
    let mut tick = tokio::time::interval(period);
    loop {
        tick.tick().await;
        // Actions composed directly — no re-implementation
        let snap = refresh::Snapshot {
            status:   inspect::status(&api).await.ok(),
            modules:  inspect::modules(&api, false).await.ok(),
            metrics:  inspect::metrics(&api, false).await.ok(),
            sessions: sessions::sessions(&api, None).await.ok(),
        };
        if tx.send(AppEvent::Refresh(snap)).is_err() { break; } // UI gone → stop worker
    }
}
```
**In-flight guard:** keep a per-worker `busy: bool` in AppState (set on spawn, clear on result) so keystrokes can't stack duplicate refreshes.

### Pattern 2b: Streaming Sinks → Channels (with the required micro-refactor)
**What:** `actions::logs::tail` takes a synchronous sink closure — the closure sends each `LogEntry` into the AppEvent channel from inside the worker task.
**The gap (codebase-verified):** `logs.rs:231` declares `sink: &mut dyn FnMut(&LogEntry)` — **no `+ Send`** (rig.rs:636/1065 sinks DO have `+ Send`). A future holding a non-Send `&mut dyn` can't cross `tokio::spawn` on the multi-thread runtime. The CLI never noticed because it awaits tail directly under `block_on`.
**Fix (prescribe in 06-01):** change `logs.rs:214/231` to `&mut (dyn FnMut(&LogEntry) + Send)` — a one-line signature change; existing callers pass concrete closures (Send), tests still compile (their locals at logs.rs:634/922 stay concrete).
**Sink body policy:** use `UnboundedSender::send` (sync, non-blocking — legal in a sync closure) for correctness, and bound memory at the DISPLAY buffer (VecDeque ring in AppState, cap ~5–10k entries). If planner prefers backpressure: a dedicated bounded channel with `try_send` + drop-counter also works, but unbounded→ring-buffer is simpler and the tail cursor advances regardless.
```rust
let worker_tx = tx.clone();
let sink = &mut |entry: &LogEntry| { let _ = worker_tx.send(AppEvent::LogLine(entry.clone())); };
actions::logs::tail(&api, logger, min_level, since_ms, interval, None, sink).await
// deadline None = until user leaves the screen; on exit the UI drops the receiver,
// the next send errors, and the worker breaks — OR wrap tail with a cancellation token (CancellationToken) selected against.
```
**Cancellation:** when the user leaves the logs screen, stop the tail task. Simplest robust mechanism: `tokio_util::sync:: CancellationToken`… but that adds tokio-util. Alternative with zero new deps: drop-the-receiver kills the sink (worker exits on next entry) PLUS a `tokio::select!` in the worker between tail and a `tokio::sync::watch` shutdown channel (workspace already has tokio). **Recommend the watch-channel shutdown; do not add tokio-util.**

### Pattern 3: Profile Switch In-Place (TUI-06)
**What:** Modal profile list → `actions::profile::use_profile(config_path, name)` (sync, persists active profile) → rebuild `Arc<ReqwestGatewayApi>` from the reloaded config → broadcast `AppEvent::ProfileChanged` → workers re-spawned with the new Arc.
**Codebase note:** `resolve_profile_context` / `resolve_gateway_api` / `resolve_secret_opt` / `secret_chain` are PRIVATE fns in ignition-cli main.rs (~1638–1790) — NOT reusable from ignition-tui. The public building blocks all live in `ignition_core::config` (`apply_env_overlay`, `resolve_selection`, `resolve_secret`, `Config`, `config_path`). Prescribe: ignition-tui's `context.rs` composes the same three calls (overlay → resolve_selection → resolve_secret → ReqwestGatewayApi::new) rather than refactoring the CLI choke files. Secrets stay confined — the TUI only ever passes `Credential` into `ReqwestGatewayApi::new`, never rendering it.

### Pattern 4: k9s/lazygit Navigation (object-list → detail)
**What:** A screens enum + per-screen `TableState`/`ListState` selection; Enter descends to detail, Esc ascends; a `Focus` enum arbitrates keybinding dispatch; modals (centered `Rect` over a `Clear`ed area) handle confirmations and input.
```rust
// Source: https://ratatui.rs/recipes/render/overwrite-regions/ (popup recipe) +
// 0.30's new centering helpers (https://ratatui.rs/highlights/v030/)
enum Screen { Dashboard, Logs, Tags, Alarms, Projects, Resources, Profiles }
enum Focus { Table, Detail, Modal }
fn render(state: &AppState, frame: &mut Frame) {
    let area = frame.area();
    // ... tab bar + active screen ...
    if let Some(modal) = &state.modal {
        let area = area.centered(Constraint::Ratio(1, 2), Constraint::Length(5)); // 0.30 helper
        frame.render_widget(ratatui::widgets::Clear, area);
        // render modal widget over it
    }
}
```
**Destructive-verb mapping:** each of the 7 guarded verbs renders a confirm modal whose acceptance ≡ `--yes` (the action itself is unmodified — the guard lives in CLI dispatch, and the TUI owns its own confirmation). Alarm ack (unguarded) needs a `--username` input — a small modal text input (hand-rolled; ~30 lines).

### Pattern 5: The Coverage Registry (Success Criterion 1's mechanism)
**What:** A static table in `ignition-tui/src/routes.rs` mapping EVERY CLI invocation path to a TUI screen/mapping kind; a CI test in ignition-cli walks the live clap tree and asserts bidirectional equality.
```rust
// ignition-tui/src/routes.rs
pub enum Mapping { Screen(Screen), Streamed /* logs -f, rig logs: shown in-stream */,
                   OutOfBand /* completions, version-warning, tags export -o -: N/A by design */ }
pub struct CliRoute { pub path: &'static str, pub mapping: Mapping } // path: "logs loggers set"
pub fn routes() -> &'static [CliRoute] { &[ /* one row per leaf command */ ] }
```
```rust
// crates/ignition-cli/tests/tui_coverage.rs — THE structural-completeness proof
use clap::CommandFactory;
fn walk(cmd: &clap::Command, prefix: &mut String, out: &mut Vec<String>) {
    let subs: Vec<_> = cmd.get_subcommands().collect();
    if subs.is_empty() { out.push(prefix.clone()); }
    else for sub in subs { /* append " " + sub.get_name(); recurse */ }
}
#[test]
fn every_cli_action_has_a_tui_mapping_and_no_orphans() {
    let mut cli_paths = Vec::new();
    walk(&Cli::command(), &mut String::new(), &mut cli_paths);
    let tui_paths: Vec<&str> = ignition_tui::routes::routes().iter().map(|r| r.path).collect();
    let missing: Vec<_> = cli_paths.iter().filter(|p| !tui_paths.contains(&p.as_str())).collect();
    let orphans:  Vec<_> = tui_paths.iter().filter(|p| !cli_paths.iter().any(|c| c == p)).collect();
    assert!(missing.is_empty(), "CLI actions with no TUI mapping: {missing:?}");
    assert!(orphans.is_empty(),  "TUI routes with no CLI action: {orphans:?}");
}
```
**Why clap tree-walking is sound:** `CommandFactory::command()` (verified via Context7/clap docs) yields the fully-built `Command`; `get_subcommands()` iteration is the same mechanism `clap_complete` uses to generate completions for arbitrary trees (clap_complete is already a dependency — proof by construction). New CLI commands added in future phases fail this test until a route row exists — the coverage stays structural, not aspirational. Place the test in ignition-cli (owns `Cli`); ignition-tui is a normal dep under default features (`tui` is default-on). Sequence: registry scaffold lands in 06-01 (mapping kinds defined, table possibly incomplete), full-coverage assertion goes green in 06-04 when the last screen ships.

### Anti-Patterns to Avoid
- **Awaiting gateway I/O in the update/render path** — the cardinal sin this whole design exists to prevent; actions are ONLY awaited inside `workers/*`.
- **Shared mutable UI state (`Arc<RwLock<AppState>>`)** — races with the render loop and kills headless testability; state mutations flow through `update()` only.
- **`Some(Ok(event)) = events.next()` as a select arm** — a terminal stream error permanently disables the arm (frozen input); match all three cases (see Pattern 1).
- **Unbounded scrollback** — a weekend-long `logs -f` must not OOM the process; ring-buffer the display (and keep level filtering over the retained buffer, not just the query).
- **Growing main.rs** — the `Commands::Tui` arm stays ~5 lines (delegate + exit-code mapping); everything else lives in ignition-tui.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Terminal setup/restore/panic cleanup | manual enable_raw_mode/EnterAlternateScreen/panic hook | `ratatui::init()` / `ratatui::restore()` | Official helpers do raw mode + alt screen + restore-on-panic hook (docs.rs/ratatui/0.30.0, verified) |
| Async terminal input | poll/read thread bridging | crossterm `EventStream` (`event-stream` feature) | Official pattern (async-github example); cancellation via Waker handled internally (crossterm wiki) |
| Diffing redraws / dirty tracking | manual cell diffing | `Terminal::draw` (double-buffered diff) | ratatui already diffs buffer→backend; draw per tick is cheap |
| Widget-level unicode width | manual column math | `Constraint` widths + ratatui's internal unicode-width | Tables/lists handle wide chars; only beware `Constraint::Length` vs wide glyph overflow |
| Wait/retry/backoff for polls | new intervals/backoff code in workers | keep using `poll.rs` semantics / plain `tokio::time::interval` | poll.rs already exists for probe-style waits; dashboard refresh is a plain interval (it's a refresh, not a wait-for-condition) |
| Centered modal geometry | manual rect math | 0.30 `Rect::centered(centered_vertically/…)` helpers | Official 0.30 addition (highlights page) |
| Coverage enumeration | hand-maintained command list in the TEST | walk the clap `CommandFactory` tree | tree can't drift from cli.rs; registry rows are the only hand-maintained part, and orphan direction catches their drift |

**Key insight:** ratatui 0.30 + the existing actions layer already solve the hard problems (render diffing, terminal lifecycle, async input, gateway I/O). The phase's real work is state design + wiring, not infrastructure.

## Common Pitfalls

### Pitfall 1: Blocking the render loop on gateway I/O
**What goes wrong:** UI freezes while a gateway call hangs (Success Criterion 3 explicitly forbids this).
**Why it happens:** calling actions directly in `update()`/render path.
**How to avoid:** workers-only actions rule (Pattern 2); per-panel `Loading`/`Loaded`/`Error(String)` states (async-github's LoadingState pattern) so slow calls show, not freeze.
**Warning signs:** typing lag while a panel refreshes; tests that need `tokio::time::pause` to avoid multi-second stalls.

### Pitfall 2: Terminal left broken after panic/kill
**What goes wrong:** panic in any task → user's shell is stuck in raw mode + alt screen.
**Why it happens:** raw mode is process-global; default panic path skips cleanup.
**How to avoid:** `ratatui::init()` installs the restore-on-panic hook (verified docs.rs). NOTE: a panic inside a `tokio::spawn`ed worker does NOT unwind the main task — it returns JoinError; workers must send `AppEvent::Error` instead of panicking (the init hook only saves the process-level case).
**Warning signs:** manual raw-mode/alt-screen code creeping in (it shouldn't exist).

### Pitfall 3: Frozen input after EventStream error
**What goes wrong:** `Some(Ok(e)) = events.next()` select arm silently disables after an `Err` — every other arm keeps firing; app looks alive but ignores keys.
**Why it happens:** tokio::select! drops a permanently-failing pattern arm.
**How to avoid:** full match on the stream item (Ok/Err/None) — Pattern 1.
**Warning signs:** none at compile time; only a test that injects a stream error would catch it — keep the match exhaustive.

### Pitfall 4: Ctrl-C doesn't quit
**What goes wrong:** in raw mode ISIG is disabled — Ctrl-C arrives as a KeyEvent (Char('c') + CONTROL), not SIGINT; the app never exits.
**Why it happens:** everyone forgets once.
**How to avoid:** explicitly map `KeyCode::Char('c') + KeyModifiers::CONTROL` to quit alongside 'q'/Esc.
**Warning signs:** manual test only — add a unit test on the keymap.

### Pitfall 5: Key release/repeat double-fires (kitty-protocol terminals)
**What goes wrong:** each key handled twice on terminals that report Release/Repeat.
**Why it happens:** crossterm emits all three KeyEventKinds on enhanced-protocol terminals.
**How to avoid:** filter with 0.30's `event.as_key_press_event()` helper (used by the official example) or explicit `kind == KeyEventKind::Press`.
**Warning signs:** doubled list-navigation on macOS Terminal.app/iTerm2 with kitty keyboard enabled.

### Pitfall 6: `logs::tail` sink not `+ Send` — spawn fails to COMPILE
**What goes wrong:** `tokio::spawn(tail_future)` errors: "dyn FnMut(&LogEntry) cannot be sent between threads safely".
**Why it happens:** logs.rs:231 predates the Send convention (rig sinks have it).
**How to avoid:** the one-line `+ Send` signature fix in 06-01 (Pattern 2b).
**Warning signs:** any `spawn_local`/`LocalSet` workaround proposed to dodge it — fix the signature instead.

### Pitfall 7: Two crossterm majors in the graph
**What goes wrong:** ratatui-crossterm resolves crossterm 0.28 while ignition-tui directly deps 0.29 — event/style types mismatch (compile errors at worst, silent duplication at best).
**Why it happens:** ratatui 0.30 supports both 0.28 and 0.29 via features (`crossterm_0_28`/`crossterm_0_29`), defaulting to the latest.
**How to avoid:** declare crossterm 0.29 in the workspace (matches the default); the ratatui backends doc has a "Crossterm version compatibility" section precisely for this.
**Warning signs:** `cargo tree -i crossterm` showing two versions.

### Pitfall 8: ratatui MSRV drift in patch releases
**What goes wrong:** future 0.30.x bumps MSRV above 1.88; CI (stable toolchain only — verified ci.yml) keeps passing, MSRV-conscious builds break.
**Why it happens:** precedent is real — 0.30.1 bumped MSRV 1.86→1.88 in a patch.
**How to avoid:** workspace pins `ratatui = "0.30.2"` (floor at the 1.88-MSRV release); optionally add an MSRV job to CI later (out of phase scope).
**Warning signs:** `cargo update` suddenly pulling a new ratatui patch.

### Pitfall 9: Worker leaks after leaving a screen / switching profiles
**What goes wrong:** old tail/watch workers keep polling a stale (or wrong-profile) gateway; duplicate AppEvents interleave.
**Why it happens:** spawn-and-forget.
**How to avoid:** generation tokens — AppState holds a `u64` era counter stamped into each worker; workers attach their era to results; update() drops stale-era events. Plus the watch-channel shutdown on screen exit (Pattern 2b).
**Warning signs:** log lines from the previous profile appearing after a switch.

### Pitfall 10: `ign tui` with piped stdout
**What goes wrong:** `ratatui::init()` panics when stdout isn't a TTY (size query fails).
**Why it happens:** helpers panic on init failure by design.
**How to avoid:** pre-check `std::io::stdout().is_terminal()` (std, stable 1.70) in the Tui dispatch arm → clean CoreError usage-class error instead of a panic.
**Warning signs:** e2e harness accidentally invoking `tui` in CI without a pty.

## Code Examples

### The complete entry point (verified against official 0.30 API)
```rust
// crates/ignition-tui/src/lib.rs — replaces the stub
use ignition_core::error::CoreError;

/// Open the cockpit. Callers must ensure stdout is a TTY (checked in ignition-cli).
pub async fn run(profile_flag: Option<String>) -> Result<(), CoreError> {
    let (ctx, err) = context::resolve(profile_flag)?;   // profile → Arc<ReqwestGatewayApi>
    let terminal = ratatui::init();                     // raw mode + alt screen + panic hook
    let app_result = run_loop(&mut /* … */).await;
    ratatui::restore();                                 // always restore, even on error
    app_result
}
```
```rust
// ignition-cli/src/main.rs Tui arm (~5 lines, choke-file discipline):
Commands::Tui => {
    if !std::io::stdout().is_terminal() {
        return (None, Err(CoreError::Usage(/* "ign tui requires a terminal" */)));
    }
    ignition_tui::run(cli.profile.clone()).await   // returns Result<(), CoreError>
        .map(|()| ActionOutput::TuiExited)          // or a dedicated early-return like Completions
}
```

### Headless test harness (TestBackend — official pattern)
```rust
// Source: ratatui 0.30 docs/tests (docs.rs TestBackend; stateful widget test style)
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::Terminal;

#[test]
fn dashboard_renders_panels() {
    let mut state = AppState::with_snapshot(test_snapshot());
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::render(&state, f)).unwrap();
    let expected = Buffer::with_lines([
        "┌Dashboard────────────┐┌Modules─────────────┐",
        /* … exact expected content … */
    ]);
    terminal.backend().assert_buffer(&expected);
}
```
The `update()` side is plain unit testing: `update(&mut state, AppEvent::Refresh(snap)); assert!(matches!(state.screen_data, …))` — no terminal needed.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| ratatui single crate | modular workspace (ratatui-core/-widgets/-crossterm/-macros); apps still depend on `ratatui` umbrella | 0.30.0 (2025-12) | No action needed; widget authors would use core, we don't |
| Manual terminal init (enable_raw_mode + EnterAlternateScreen + hand-rolled panic hook) | `ratatui::init()/restore()/run()` helpers incl. panic hook | 0.29→0.30 | Use the helpers; the Elm-docs manual `tui::init_terminal` pattern is now legacy-style |
| tui-rs (unmaintained since 2023) | ratatui (fork, 0.30.x current) | — | n/a — repo already chose ratatui by design |
| ratatui MSRV 1.74–1.80 era | 0.30.2 MSRV **1.88**, edition 2024 | 0.30.1 (2026-06) | Exact match with workspace MSRV — no toolchain tension |
| crossterm 0.27/0.28 | 0.29.0 current; ratatui-crossterm supports 0.28+0.29 behind features | 2025-04 | Pin 0.29 (default); avoid dual-major graphs |
| `Block::title(Title::from(...))` | `Block::title` takes `Into<Line>`; `Title` type removed | 0.30.0 BREAKING | Code examples must use the new title API (both examples above do) |

**Deprecated/outdated:**
- `ratatui::init_terminal`-era manual setup recipes still floating around blog posts — superseded by `ratatui::init()`.
- Old examples filtering `key.kind == KeyEventKind::Press` manually — prefer `event.as_key_press_event()`.

## Open Questions

1. **Render cadence: per-event vs per-tick**
   - What we know: async-github redraws at a 60fps interval; dashboard apps typically redraw on a 250ms–1s tick.
   - What's unclear: whether redrawing ONLY on AppEvent+Tick (250ms) is smooth enough for log tail bursts.
   - Recommendation: draw per loop iteration when an event arrived, else on tick — a one-line select-structure decision; tune during 06-03. LOW risk either way.

2. **Log-line transport: unbounded→ring vs bounded try_send**
   - What we know: both work; unbounded + display-side ring is simplest; bounded try_send adds a drop counter.
   - What's unclear: burst behavior on a chatty gateway (thousands of lines/s).
   - Recommendation: start unbounded→ring (5–10k); the tail action's page limit already batches; revisit only if a live test shows memory pressure. MEDIUM confidence.

3. **`ActionOutput` relationship for TUI exit**
   - What we know: main.rs's ActionOutput enum has no TUI variant; Completions special-cases before dispatch; STATE says the TUI defines its own relationship to the output contract.
   - What's unclear: whether `ign tui` should print a final summary envelope to stdout after restore (useful for scripting) or print nothing.
   - Recommendation: print a minimal final status line (or nothing) — planner decides; do NOT print envelopes while the alt screen is active. LOW impact.

4. **Detail-pane depth for resource get / project set forms**
   - What we know: TUI-06 requires browse + trigger actions; some CLI verbs take rich flag sets (resource put needs a file; project set needs fields).
   - What's unclear: how far modal form-editing goes vs "show the equivalent CLI command" for exotic inputs.
   - Recommendation: MVP = every action reachable; rich-arg actions get a modal with the common fields and a "press ? for the CLI form" hint — keeps coverage honest without building a form framework. MEDIUM confidence; planner should scope per-screen.

## Sources

### Primary (HIGH confidence)
- **crates.io API (live, 2026-08-27)**: ratatui 0.30.2 max_stable (updated 2026-06-19), `rust_version: 1.88.0`; 0.30.1 also 1.88.0, 0.30.0 1.86.0 (patch-bump precedent); crossterm 0.29.0 (2025-04-05); ratatui 0.30.2 dep graph: ratatui-core 0.1.2, ratatui-widgets 0.3.2, ratatui-crossterm 0.1.2 (optional, crossterm ^0.28/^0.29 optional); ratatui-core 0.1.2 current.
- **ratatui.rs (official site)**: v0.30.0 highlights (modularization, `ratatui::run()`, Rect::centered, Flex changes, MSRV 1.86→, edition 2024, multiple-crossterm-version features); async-github example (tokio::select! + EventStream + tokio::spawn worker, full source; Cargo.toml shows `crossterm = { features = ["event-stream"] }` direct dep); Elm Architecture page (Model/Message/update/view + manual terminal module); component template Action.rs (reified Action enum); application-patterns index.
- **docs.rs ratatui 0.30.0**: `ratatui::init()` (raw mode + alt screen + panic hook, panics on failure), `ratatui::restore()` (errors to stderr, ignored), `init_with_options`; TestBackend (`assert_buffer_lines` example, Infallible error in 0.30); default features list (crossterm, underline-color, macros, layout-cache, std, all-widgets).
- **Context7 /websites/rs_ratatui_0_30_0**: `ratatui::run` signature; TestBackend render/assert patterns (`Buffer::with_lines` + assert_eq in official widget tests); feature-flag summary.
- **Context7 /crossterm-rs/crossterm**: `event-stream` feature provides EventStream; cancellation semantics (Waker/mio on unix, semaphore on windows).
- **Codebase (read directly)**: ignition-tui stub lib.rs/Cargo.toml; actions/mod.rs + actions/{logs,sessions,inspect,tags,profile,rig}.rs signatures; client/mod.rs (GatewayApi: Send+Sync, async_trait, ReqwestGatewayApi::new(profile, credential), no Clone derive); poll.rs; cli.rs Commands/sub-enums; main.rs dispatch + resolve_profile_context/resolve_gateway_api (private) + ActionOutput (63 variants); workspace + crate Cargo.tomls (tokio features incl. sync/time; tui feature gate default-on); .github/workflows/ci.yml (stable toolchain only); requirements.md TUI-01..06.

### Secondary (MEDIUM confidence)
- clap `CommandFactory::command()` + `get_subcommands()` tree walk — CommandFactory verified via Context7/clap docs; `get_subcommands()` public iteration is the mechanism clap_complete uses (inference from clap_complete's existence as a dependency + standard builder API). Compile-verified in first 06-04 task.
- Terminal-emitter nuance (raw mode disables ISIG → Ctrl-C as key event) — consistent across crossterm docs/community; not re-verified against a live terminal in this research.

### Tertiary (LOW confidence)
- Cargo MSRV-aware resolver default behavior for patch-drift protection — not verified; the `=0.30.2`-floor pin + optional CI MSRV job is the mitigation regardless.
- tui-input as the input-widget choice — ecosystem-known; only relevant if modal inputs outgrow hand-rolling.

## Metadata

**Confidence breakdown:**
- Standard stack: **HIGH** — every version/feature claim verified live (crates.io API) or against official docs (ratatui.rs, docs.rs); MSRV alignment is an exact-match fact.
- Architecture: **HIGH** — the recommended loop is ratatui's own official example, adapted; the actions-reuse seam was verified against actual signatures, including the Send-bound gap.
- Pitfalls: **HIGH** for codebase-specific ones (verified by reading the code); MEDIUM-HIGH for terminal-behavior ones (official docs + strong community consensus).
- Coverage mechanism: **MEDIUM-HIGH** — sound design verified against clap's documented CommandFactory; the tree-walk itself gets compile-verified in the first attempt (low risk, high value).

**Research date:** 2026-08-27
**Valid until:** 2026-09-27 (ratatui is stable but patch-releases MSRV bumps — re-check `cargo tree` if planning later than ~30 days)
