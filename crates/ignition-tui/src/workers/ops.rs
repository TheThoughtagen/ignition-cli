//! The Projects screen's one-shot workers (06-05) — the browse half
//! (project list, project find, resources list, resource get) and
//! the action-menu fire helpers (new/copy/rename/set/delete/import/
//! export/put/delete/webdev verbs), each composing ignition-core
//! action fns AS-IS through [`super::spawn_action`]'s locked
//! result-modal display. The resource family's export-zip surgery
//! rides INSIDE the actions layer — invisible here, exactly as
//! designed.

use std::sync::Arc;

use ignition_core::actions;
use ignition_core::client::GatewayApi;
use ignition_core::client::ReqwestGatewayApi;
use ignition_core::error::CoreError;

use crate::event::AppEvent;
use crate::state::AppState;

/// One-shot project list (the Projects screen's entry load and the
/// mutation refresh trigger): reports [`AppEvent::ProjectsList`]
/// era-stamped. Busy-guarded so repeated entries cannot stack.
pub fn spawn_project_list(state: &mut AppState) {
    if state.projects.list_busy {
        return;
    }
    let Some(client) = state.client.as_ref().map(|handle| handle.0.clone()) else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    state.projects.list_busy = true;
    let era = state.era;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let result = actions::projects::projects(&*client)
                .await
                .map(|result| result.projects)
                .map_err(|err| err.to_string());
            let _ = tx.send(AppEvent::ProjectsList { era, result });
        });
    }
}

/// One-shot project find (the detail pane's record read — the CLI
/// family's own read-back source; there is no `project get` leaf,
/// the find IS the detail): reports [`AppEvent::ProjectGet`]
/// carrying the name (the pane's identity; a closed pane's late
/// result drops at the name lookup).
pub fn spawn_project_get(state: &mut AppState, name: &str) {
    let Some(client) = state.client.as_ref().map(|handle| handle.0.clone()) else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    let name = name.to_string();
    let era = state.era;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let result = client
                .project_find(&name)
                .await
                .map_err(|err| err.to_string());
            let _ = tx.send(AppEvent::ProjectGet { era, name, result });
        });
    }
}

/// One-shot resources list (the detail pane's drill-down half — the
/// action owns the export-zip surgery): reports
/// [`AppEvent::ResourcesList`] carrying the project (the pane's
/// identity).
pub fn spawn_resources_list(state: &mut AppState, project: &str) {
    let Some(client) = state.client.as_ref().map(|handle| handle.0.clone()) else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    let project = project.to_string();
    let era = state.era;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let result = actions::resources::resources_list(&*client, &project, None)
                .await
                .map(|result| {
                    result
                        .resources
                        .into_iter()
                        .filter_map(|entry| entry.path)
                        .collect::<Vec<String>>()
                })
                .map_err(|err| err.to_string());
            let _ = tx.send(AppEvent::ResourcesList {
                era,
                project,
                result,
            });
        });
    }
}

/// One-shot resource get (the drill-down's content preview):
/// reports [`AppEvent::ResourceGet`] stamped with the resource-open's
/// `seq` — the request-id gate that drops gets for left/replaced
/// panes. Binary fencing rides the action's own exit-6 refusal —
/// surfaced as the Error state verbatim.
pub fn spawn_resource_get(state: &mut AppState, seq: u64, project: &str, path: &str) {
    let Some(client) = state.client.as_ref().map(|handle| handle.0.clone()) else {
        return;
    };
    let Some(tx) = state.events_tx.clone() else {
        return;
    };
    let (project, path) = (project.to_string(), path.to_string());
    let era = state.era;
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let result = actions::resources::resource_get(&*client, &project, &path)
                .await
                .map_err(|err| err.to_string());
            let _ = tx.send(AppEvent::ResourceGet { era, seq, result });
        });
    }
}

/// Read raw file bytes for a spawned projects form worker — the
/// import zip and the resource-put content source, with the cockpit
/// difference pinned in [`super::watch::read_json_file`]: `-` (stdin)
/// is REFUSED (the alternate screen owns the terminal input). Runs
/// ONLY inside spawned workers (I/O lives in workers).
pub async fn read_file_bytes(file: &str) -> Result<Vec<u8>, CoreError> {
    if file == "-" {
        return Err(CoreError::InvalidInput {
            reason: "stdin (-) is not available in the TUI — pass a file path \
                     (the pipe form is CLI-only)"
                .to_string(),
        });
    }
    tokio::fs::read(file)
        .await
        .map_err(|err| CoreError::InvalidInput {
            reason: format!("cannot read {file}: {err}"),
        })
}

// ---- Action-menu fire helpers (06-05 Task 2) ----
//
// One helper per project/resource/webdev menu verb, each firing the
// ignition-core action fn AS-IS through [`super::spawn_action`] (the
// locked busy guard + pretty-JSON result modal). The Confirm-gated
// verbs fire ONLY from update's execute_pending — the TUI owns their
// `--yes`, these arms stay unguarded exactly like the actions.

/// `ign project new` — name + the optional title (the common-field
/// form; every other flag stays on the CLI form).
pub fn fire_project_new(state: &mut AppState, name: String, title: Option<String>) {
    if let Some(client) = client_arc(state) {
        super::spawn_action(state, "project new", async move {
            actions::projects::project_new(
                &*client,
                &name,
                &actions::projects::NewOptions {
                    enabled: true,
                    title,
                    ..Default::default()
                },
            )
            .await
        });
    }
}

/// `ign project copy` — copy all resources src → dst.
pub fn fire_project_copy(state: &mut AppState, src: String, dst: String) {
    if let Some(client) = client_arc(state) {
        super::spawn_action(state, "project copy", async move {
            actions::projects::project_copy(&*client, &src, &dst).await
        });
    }
}

/// `ign project rename` — native rename old → new.
pub fn fire_project_rename(state: &mut AppState, old: String, new: String) {
    if let Some(client) = client_arc(state) {
        super::spawn_action(state, "project rename", async move {
            actions::projects::project_rename(&*client, &old, &new).await
        });
    }
}

/// `ign project set` — the modify body from `Some`-fields ONLY (the
/// parsed `FIELD=VALUE` line; absent fields ride nothing).
pub fn fire_project_set(state: &mut AppState, name: String, opts: actions::projects::SetOptions) {
    if let Some(client) = client_arc(state) {
        super::spawn_action(state, "project set", async move {
            actions::projects::project_set(&*client, &name, &opts).await
        });
    }
}

/// `ign project delete` — the CONFIRMED arm (the TUI owned the
/// `--yes`; the action runs unguarded).
pub fn fire_project_delete(state: &mut AppState, name: String) {
    if let Some(client) = client_arc(state) {
        super::spawn_action(state, "project delete", async move {
            actions::projects::project_delete(&*client, &name).await
        });
    }
}

/// `ign project import` — the zip bytes are read INSIDE the worker
/// (the import guards — magic/structure/size — ride the action).
pub fn fire_project_import(
    state: &mut AppState,
    name: String,
    file: String,
    policy: actions::projects::CollisionPolicy,
) {
    if let Some(client) = client_arc(state) {
        super::spawn_action(state, "project import", async move {
            let zip = read_file_bytes(&file).await?;
            actions::projects::project_import(&*client, &name, zip, policy).await
        });
    }
}

/// `ign project export` — the streaming export to the given path
/// (completion lands in the result modal).
pub fn fire_project_export(state: &mut AppState, name: String, out: String) {
    if let Some(client) = client_arc(state) {
        let out = std::path::PathBuf::from(out);
        super::spawn_action(state, "project export", async move {
            actions::projects::project_export(&*client, &name, Some(&out)).await
        });
    }
}

/// `ign resource put` — the CONFIRMED arm; the content bytes are
/// read INSIDE the worker.
pub fn fire_resource_put(state: &mut AppState, project: String, path: String, file: String) {
    if let Some(client) = client_arc(state) {
        super::spawn_action(state, "resource put", async move {
            let input = read_file_bytes(&file).await?;
            actions::resources::resource_put(&*client, &project, &path, input).await
        });
    }
}

/// `ign resource delete` — the CONFIRMED arm.
pub fn fire_resource_delete(state: &mut AppState, project: String, path: String) {
    if let Some(client) = client_arc(state) {
        super::spawn_action(state, "resource delete", async move {
            actions::resources::resource_delete(&*client, &project, &path).await
        });
    }
}

/// `ign webdev deploy` — deliberately NOT Confirm-gated (the 05-03
/// decision: the dedicated ign-cli project is CLI-OWNED, born from
/// the deploy zip and overwrite-replaced every deploy; user projects
/// are never touched). Fires with the CLI defaults — target
/// `ign-cli`, no scriptExec (scriptExec rides the CLI form's explicit
/// flags). The action owns the profile-local secret lifecycle.
pub fn fire_webdev_deploy(state: &mut AppState) {
    let (Some(client), Some(profile)) = (client_arc(state), state.profile.clone()) else {
        return;
    };
    super::spawn_action(state, "webdev deploy", async move {
        actions::webdev::webdev_deploy(
            &*client,
            "ign-cli",
            false, // --with-script-exec stays a CLI-form flag
            false, // --rotate-secret ditto
            &ignition_core::config::config_path(),
            &profile,
        )
        .await
    });
}

/// `ign webdev status` — the version-handshake sweep (a READ; the
/// per-route degradation matrix lands in the result modal as data).
/// The profile's stored webdev-route secret is loaded INSIDE the
/// worker (local file I/O, like the action's own config reads).
pub fn fire_webdev_status(state: &mut AppState) {
    let (Some(client), Some(profile)) = (client_arc(state), state.profile.clone()) else {
        return;
    };
    super::spawn_action(state, "webdev status", async move {
        let path = ignition_core::config::config_path();
        let secret = ignition_core::config::load(&path).ok().and_then(|config| {
            config
                .profiles
                .get(&profile)
                .and_then(|entry| entry.webdev_secret.clone())
        });
        actions::webdev::webdev_status(&*client, "ign-cli", secret.as_deref()).await
    });
}

/// `ign script run <CODE>` (07-03, SCRPT-01) — gateway-side Python
/// through the secret-gated scriptExec route. UNGATED (CLI parity —
/// no --yes exists: the deploy flag IS the opt-in). The config is
/// loaded INSIDE the worker (the fire_webdev_status precedent) so
/// the action's secret gate sees the persisted store; a missing
/// secret surfaces the action's own `script_exec_not_configured`
/// refusal in the result modal.
pub fn fire_script_run(state: &mut AppState, code: String) {
    let (Some(client), Some(profile)) = (client_arc(state), state.profile.clone()) else {
        return;
    };
    super::spawn_action(state, "script run", async move {
        let config = ignition_core::config::load(&ignition_core::config::config_path())?;
        actions::script::script_run(&*client, &config, &profile, "ign-cli", &code).await
    });
}

/// `ign lint <PATHS>` (07-04, INTR-02) — the LOCAL delegation: NO
/// client, NO profile (spawn_action without a client arc — the
/// first clientless dashboard worker). Ungated, unstrict (the
/// doctor posture: findings + child exit land in the result modal
/// as data; `--strict` and `--` passthrough stay CLI forms).
pub fn fire_lint(state: &mut AppState, paths: String) {
    let paths: Vec<String> = paths.split_whitespace().map(str::to_string).collect();
    super::spawn_action(state, "lint", async move {
        actions::lint::lint_run(&paths, false, &[]).await
    });
}

/// `ign project diff` (07-01) — the cross-gateway read. The TWO
/// per-side clients are rebuilt INSIDE the worker from the named
/// profiles (`context::rebuild` — the same public building blocks
/// the opening resolution composes; each side's secret chain
/// resolves independently), so the cockpit's own single-client world
/// is untouched. NO confirm gate — a read.
pub fn fire_project_diff(
    state: &mut AppState,
    profile_a: String,
    profile_b: String,
    project: String,
) {
    super::spawn_action(state, "project diff", async move {
        let (_name_a, _url_a, api_a) = crate::context::rebuild(&profile_a)?;
        let (_name_b, _url_b, api_b) = crate::context::rebuild(&profile_b)?;
        actions::projects::project_diff(&*api_a, &*api_b, &project, &profile_a, &profile_b).await
    });
}

/// `ign project sync` (07-01) — the CONFIRMED promotion arm (the TUI
/// owned the `--yes`; the action runs unguarded). Per-side clients
/// rebuilt INSIDE the worker exactly like the diff twin; the form's
/// explicit resource list is the selection (`--all-changed` and
/// `--delete` stay CLI forms, `?`-named).
pub fn fire_project_sync(
    state: &mut AppState,
    profile_a: String,
    profile_b: String,
    project: String,
    resources: Vec<String>,
) {
    super::spawn_action(state, "project sync", async move {
        let (_name_a, _url_a, api_a) = crate::context::rebuild(&profile_a)?;
        let (_name_b, _url_b, api_b) = crate::context::rebuild(&profile_b)?;
        let selection = actions::projects::SyncSelection {
            resources,
            all_changed: false,
        };
        actions::projects::project_sync(
            &*api_a, &*api_b, &project, &selection, false, &profile_a, &profile_b,
        )
        .await
    });
}

/// The state's client Arc, cloned out of the handle (the watch.rs
/// helper's shape).
fn client_arc(state: &AppState) -> Option<Arc<ReqwestGatewayApi>> {
    state.client.as_ref().map(|handle| handle.0.clone())
}

#[cfg(test)]
mod tests {
    use super::{
        fire_project_new, fire_webdev_deploy, read_file_bytes, spawn_project_get,
        spawn_project_list, spawn_resource_get, spawn_resources_list,
    };
    use crate::event::AppEvent;
    use crate::state::AppState;

    fn armed_state() -> AppState {
        let mut state = AppState::new();
        state.client = Some(crate::state::ClientHandle(std::sync::Arc::new(
            ignition_core::client::ReqwestGatewayApi::for_tests("http://127.0.0.1:1/", None),
        )));
        state.events_tx = Some(tokio::sync::mpsc::unbounded_channel().0);
        // The webdev verbs key off the active profile (the secret's
        // config slot) — arm it like production.
        state.profile = Some("dev".into());
        state
    }

    /// The browse rails stand alone without a runtime: the list load
    /// arms its busy guard (and refuses to stack); get/list/get
    /// transitions never panic.
    #[test]
    fn projects_one_shot_rails_stand_alone_without_a_runtime() {
        let mut state = armed_state();

        spawn_project_list(&mut state);
        assert!(state.projects.list_busy, "list load armed");
        spawn_project_list(&mut state);
        assert!(state.projects.list_busy, "busy guard refuses to stack");

        spawn_project_get(&mut state, "PlantFloor");
        spawn_resources_list(&mut state, "PlantFloor");
        spawn_resource_get(&mut state, 1, "PlantFloor", "views/root.json");
    }

    /// The menu fire helpers transition the busy guard only (nothing
    /// spawns outside a runtime) — the state-machine half of the
    /// result-modal contract.
    #[test]
    fn menu_fire_helpers_arm_the_label_without_a_runtime() {
        let mut state = armed_state();
        fire_project_new(&mut state, "scratch".into(), None);
        assert_eq!(state.dashboard.in_flight, Some("project new"));

        let mut again = armed_state();
        fire_webdev_deploy(&mut again);
        assert_eq!(
            again.dashboard.in_flight,
            Some("webdev deploy"),
            "deploy fires WITHOUT a Confirm (the 05-03 CLI-owned-project decision)"
        );
    }

    /// The list load against a dead endpoint degrades to the Err
    /// payload — era-stamped, data never a panic (the alarms worker
    /// test's shape).
    #[tokio::test]
    async fn project_list_degrades_to_err_on_a_dead_endpoint() {
        let mut state = armed_state();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        state.events_tx = Some(tx);
        spawn_project_list(&mut state);

        let event = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("list settles within 5s")
            .expect("worker holds the sender");
        match event {
            AppEvent::ProjectsList { era, result } => {
                assert_eq!(era, state.era, "era-stamped");
                assert!(result.is_err(), "dead endpoint degrades to Err");
            }
            other => panic!("expected ProjectsList, got {other:?}"),
        }
    }

    /// The resource get stamps its seq (the request-id) — the gate
    /// update drops stale gets through.
    #[tokio::test]
    async fn resource_get_stamps_the_request_id() {
        let mut state = armed_state();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        state.events_tx = Some(tx);
        spawn_resource_get(&mut state, 9, "PlantFloor", "views/root.json");

        let event = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("get settles within 5s")
            .expect("worker holds the sender");
        match event {
            AppEvent::ResourceGet { era, seq, result } => {
                assert_eq!(era, state.era, "era-stamped");
                assert_eq!(seq, 9, "seq-stamped (the request-id)");
                assert!(result.is_err(), "dead endpoint degrades to Err");
            }
            other => panic!("expected ResourceGet, got {other:?}"),
        }
    }

    /// The byte-source helper refuses stdin (the cockpit owns the
    /// terminal input) and surfaces read failures honestly.
    #[tokio::test]
    async fn read_file_bytes_refuses_stdin_and_missing_files() {
        let stdin = read_file_bytes("-").await.expect_err("stdin refuses");
        assert!(stdin.to_string().contains("not available in the TUI"));

        let missing = read_file_bytes("/nonexistent/definitely-missing.zip")
            .await
            .expect_err("missing file refuses");
        assert!(missing.to_string().contains("cannot read"));
    }
}
