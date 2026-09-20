//! `ign e2e` (QUICK-tg4) — the browser-E2E scaffold pair.
//!
//! Two verbs, deliberately not three: [`e2e_doctor`] diagnoses the host
//! and gateway a Playwright suite needs, and `e2e_init` writes the
//! scaffold. There is NO `ign e2e run` — the scaffold's own `npm test`
//! is the runner, and wrapping it would put this CLI in the business of
//! proxying Playwright's reporter, flags, and exit codes.
//!
//! ## The doctor posture
//!
//! [`e2e_doctor`] returns `Ok` whenever the diagnosis COMPLETES. A
//! missing Node, an unreachable gateway, an undeployed testing bundle —
//! all of them are `checks[]` rows, never a `CoreError`. This is the
//! `actions::doctor` contract verbatim (exit 0 is "I looked"), and it
//! is what lets an agent read one envelope instead of branching on an
//! exit code before it can see the diagnosis.
//!
//! ## Environment injection
//!
//! Both actions take an [`E2eEnv`] rather than reading the process
//! environment themselves. `std::env::set_var` is `unsafe` in edition
//! 2024 and races under the workspace's parallel test runs, so the
//! contract tests build a temp-dir stub PATH and hand it in — the
//! `ComposeRunner` injection precedent.

use std::ffi::OsString;
use std::path::PathBuf;

use serde::Serialize;

use crate::actions::doctor::{CheckResult, CheckStatus};
use crate::actions::lint::find_on_path_in;
use crate::client::ReqwestGatewayApi;
use crate::client::webdev::{TestingProbe, testing_probe};
use crate::error::CoreError;

/// The minimum Node major this scaffold supports.
///
/// Playwright 1.5x requires Node 18+; 20 is the floor here because the
/// scaffold's `global-setup.mjs` and `lib/gateway.mjs` use built-in
/// `fetch` without a flag, which is only unflagged from 18 and only
/// non-experimental from 20.
pub const MIN_NODE_MAJOR: u32 = 20;

/// THE install hint for a missing or too-old Node.
///
/// A `pub const` rather than an inline string so `e2e init`'s exit-6
/// refusal (Task 3) carries the doctor's hint BYTE-IDENTICALLY — a
/// user who hits the refusal and then runs the doctor must not be
/// given two differently-worded versions of the same instruction.
pub const NODE_INSTALL_HINT: &str = "install Node 20 or newer — https://nodejs.org (LTS installer), \
     `brew install node` (macOS/Homebrew), or `nvm install --lts` (nvm)";

/// The Playwright browsers-cache override the scaffold and the doctor
/// both honor.
const BROWSERS_PATH_VAR: &str = "PLAYWRIGHT_BROWSERS_PATH";

/// Directory-entry prefixes a usable Chromium install produces.
const CHROMIUM_PREFIXES: &[&str] = &["chromium-", "chromium_headless_shell-"];

/// The injected host environment (D10) — everything these actions read
/// from outside their arguments.
#[derive(Debug, Clone, Default)]
pub struct E2eEnv {
    /// The `PATH` value tool discovery splits.
    pub path: Option<OsString>,
    /// Where Playwright keeps downloaded browsers.
    pub browsers_cache: Option<PathBuf>,
}

impl E2eEnv {
    /// Read the real process environment.
    ///
    /// The browsers cache follows Playwright's own resolution order
    /// (D1): `PLAYWRIGHT_BROWSERS_PATH` when set, else the per-OS
    /// default under the user's cache/local-data directory.
    pub fn from_process() -> Self {
        Self {
            path: std::env::var_os("PATH"),
            browsers_cache: default_browsers_cache(),
        }
    }
}

/// Playwright's per-OS default browsers cache.
///
/// `directories::BaseDirs` (already in the tree) rather than a
/// hand-rolled home-dir walk — zero new dependencies, and it gets
/// macOS's `~/Library/Caches` right without a `cfg` ladder for the
/// home directory itself.
fn default_browsers_cache() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os(BROWSERS_PATH_VAR) {
        return Some(PathBuf::from(explicit));
    }
    let base = directories::BaseDirs::new()?;
    let home = base.home_dir();
    #[cfg(target_os = "macos")]
    {
        Some(home.join("Library/Caches/ms-playwright"))
    }
    #[cfg(target_os = "windows")]
    {
        let _ = home;
        std::env::var_os("LOCALAPPDATA").map(|local| PathBuf::from(local).join("ms-playwright"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Some(home.join(".cache/ms-playwright"))
    }
}

/// `ign e2e doctor` options.
#[derive(Debug, Clone)]
pub struct E2eDoctorOptions {
    /// The scaffold directory whose `node_modules` is inspected.
    pub dir: PathBuf,
    /// The gateway project whose testing bundle is probed. NO default
    /// (D8) — without it the two gateway rows honestly `skip`.
    pub project: Option<String>,
}

/// `ign e2e doctor` output — the doctor-shaped row list.
#[derive(Debug, Serialize)]
pub struct E2eDoctorResult {
    /// The inspected scaffold directory.
    pub dir: String,
    /// The six rows, in a fixed order. Reuses
    /// [`crate::actions::doctor::CheckResult`] — there is deliberately
    /// no parallel row type, so every doctor in this CLI renders and
    /// parses identically.
    pub checks: Vec<CheckResult>,
}

/// `"v22.11.0\n"` → `Some(22)`; a bare `"22.11.0"` parses too.
/// Anything else is `None` (the `parse_compose_version` shape).
fn parse_node_major(stdout: &str) -> Option<u32> {
    let trimmed = stdout.trim();
    let version = trimmed.strip_prefix('v').unwrap_or(trimmed);
    version.split('.').next()?.parse().ok()
}

/// Does this cache directory hold a usable Chromium? (D1)
///
/// A pure filesystem read — deliberately NOT `npx playwright install
/// --dry-run`, which reaches for the npm registry on a host without the
/// package cached. That would break both the doctor's offline posture
/// and its "exit 0 whenever the diagnosis completes" contract, and the
/// `--dry-run` output is not a stable contract to parse either.
fn chromium_present(cache: &std::path::Path) -> bool {
    let Ok(entries) = std::fs::read_dir(cache) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        CHROMIUM_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
    })
}

fn row(name: &str, status: CheckStatus, detail: String, hint: Option<&str>) -> CheckResult {
    CheckResult {
        name: name.into(),
        status,
        detail,
        hint: hint.map(str::to_string),
    }
}

/// Row 1 — node: discovery, then a version probe through an ARG VECTOR
/// (never a shell string).
async fn check_node(env: &E2eEnv) -> CheckResult {
    let Some(path_var) = env.path.as_ref() else {
        return row(
            "node",
            CheckStatus::Fail,
            "no PATH is set — node cannot be discovered".into(),
            Some(NODE_INSTALL_HINT),
        );
    };
    let Some(node) = find_on_path_in(path_var, "node") else {
        return row(
            "node",
            CheckStatus::Fail,
            "node is not on PATH".into(),
            Some(NODE_INSTALL_HINT),
        );
    };
    let output = tokio::process::Command::new(&node)
        .args(["--version"])
        .output()
        .await;
    let stdout = match output {
        Ok(output) => String::from_utf8_lossy(&output.stdout).into_owned(),
        Err(err) => {
            return row(
                "node",
                CheckStatus::Fail,
                format!(
                    "node was found at {} but could not run: {err}",
                    node.display()
                ),
                Some(NODE_INSTALL_HINT),
            );
        }
    };
    match parse_node_major(&stdout) {
        Some(major) if major >= MIN_NODE_MAJOR => row(
            "node",
            CheckStatus::Ok,
            format!("{} ({})", stdout.trim(), node.display()),
            None,
        ),
        Some(major) => row(
            "node",
            CheckStatus::Fail,
            format!(
                "node {} is too old — major {major} < {MIN_NODE_MAJOR} ({})",
                stdout.trim(),
                node.display()
            ),
            Some(NODE_INSTALL_HINT),
        ),
        None => row(
            "node",
            CheckStatus::Warn,
            format!(
                "node at {} answered {:?} — the version could not be parsed",
                node.display(),
                stdout.trim()
            ),
            Some(NODE_INSTALL_HINT),
        ),
    }
}

/// Row 2 — npm: discovery only. The version is not probed: npm ships
/// WITH node, so a present node and an absent npm is a broken install,
/// not a version question.
fn check_npm(env: &E2eEnv) -> CheckResult {
    match env
        .path
        .as_ref()
        .and_then(|path| find_on_path_in(path, "npm"))
    {
        Some(npm) => row("npm", CheckStatus::Ok, npm.display().to_string(), None),
        None => row(
            "npm",
            CheckStatus::Fail,
            "npm is not on PATH".into(),
            Some(NODE_INSTALL_HINT),
        ),
    }
}

/// Row 3 — playwright: is the runner installed in the scaffold?
fn check_playwright(opts: &E2eDoctorOptions) -> CheckResult {
    let marker = opts
        .dir
        .join("node_modules")
        .join("@playwright")
        .join("test")
        .join("package.json");
    if marker.is_file() {
        row(
            "playwright",
            CheckStatus::Ok,
            format!("@playwright/test is installed in {}", opts.dir.display()),
            None,
        )
    } else {
        row(
            "playwright",
            CheckStatus::Warn,
            format!(
                "@playwright/test is not installed in {} (no node_modules/@playwright/test)",
                opts.dir.display()
            ),
            Some("run `ign e2e init <DIR> --yes` to scaffold and install"),
        )
    }
}

/// Row 4 — chromium: the browsers-cache scan (D1).
///
/// `warn`, never `fail`: a missing browser is one command away, and
/// grading it a failure would make a perfectly scaffoldable host read
/// as broken.
fn check_chromium(env: &E2eEnv) -> CheckResult {
    let Some(cache) = env.browsers_cache.as_ref() else {
        return row(
            "chromium",
            CheckStatus::Warn,
            "the Playwright browsers cache could not be located".into(),
            Some("run `ign e2e init <DIR> --browsers --yes`, or set PLAYWRIGHT_BROWSERS_PATH"),
        );
    };
    if chromium_present(cache) {
        row(
            "chromium",
            CheckStatus::Ok,
            format!("a chromium build is present in {}", cache.display()),
            None,
        )
    } else {
        row(
            "chromium",
            CheckStatus::Warn,
            format!("no chromium build found in {}", cache.display()),
            Some("run `ign e2e init <DIR> --browsers --yes` to download it"),
        )
    }
}

/// Row 5 — testing-bundle. A transport error becomes a `fail` ROW, not
/// a propagated `CoreError`: letting it escape would break the exit-0
/// contract and hide the five rows already gathered.
async fn check_bundle(api: Option<&ReqwestGatewayApi>, opts: &E2eDoctorOptions) -> CheckResult {
    let Some(project) = opts.project.as_deref() else {
        return row(
            "testing-bundle",
            CheckStatus::Skip,
            "no --project given — the gateway's testing bundle was not probed".into(),
            None,
        );
    };
    let Some(api) = api else {
        return row(
            "testing-bundle",
            CheckStatus::Skip,
            "no gateway profile resolved — the testing bundle was not probed".into(),
            None,
        );
    };
    let adopt_hint = "deploy the bundle: `ign webdev deploy --project NAME --with-testing` \
                      or `ign adopt --project NAME --testing`";
    match testing_probe(api, project).await {
        Ok(TestingProbe::Present { discovered }) => {
            let count = discovered
                .get("count")
                .and_then(serde_json::Value::as_u64)
                .or_else(|| {
                    discovered
                        .get("discovered_modules")
                        .and_then(serde_json::Value::as_array)
                        .map(|modules| modules.len() as u64)
                })
                .unwrap_or(0);
            row(
                "testing-bundle",
                CheckStatus::Ok,
                format!("deployed in {project} — {count} discovered module(s)"),
                None,
            )
        }
        Ok(TestingProbe::Absent) => row(
            "testing-bundle",
            CheckStatus::Fail,
            format!("the testing bundle is not deployed in {project}"),
            Some(adopt_hint),
        ),
        Ok(TestingProbe::Unlicensed) => row(
            "testing-bundle",
            CheckStatus::Fail,
            "the WebDev module is installed but unlicensed (HTTP 402)".into(),
            Some("license the WebDev module on the gateway"),
        ),
        Ok(TestingProbe::AuthGated { status }) => row(
            "testing-bundle",
            CheckStatus::Fail,
            format!("the testing route rejected this credential (HTTP {status})"),
            Some("check the profile's API token — `ign doctor` diagnoses auth"),
        ),
        Ok(TestingProbe::LazyCompile { .. }) => row(
            "testing-bundle",
            CheckStatus::Warn,
            format!(
                "the testing route in {project} answered 500 on first touch \
                 (WebDev lazy-compile) — re-run the doctor"
            ),
            Some("a freshly deployed route compiles on its first request; try again"),
        ),
        Err(err) => row(
            "testing-bundle",
            CheckStatus::Fail,
            format!("the testing bundle could not be probed: {err}"),
            Some(adopt_hint),
        ),
    }
}

/// Row 6 — trial. Same posture as row 5: a transport error is a row.
async fn check_trial(api: Option<&ReqwestGatewayApi>) -> CheckResult {
    let Some(api) = api else {
        return row(
            "trial",
            CheckStatus::Skip,
            "no gateway profile resolved — the trial state was not read".into(),
            None,
        );
    };
    match crate::actions::rig::trial_status(api).await {
        Ok(status) if status.expired => row(
            "trial",
            CheckStatus::Warn,
            format!(
                "the gateway's trial has EXPIRED ({}) — a browser run against it \
                 will hit the demo timeout",
                status.trial_state
            ),
            Some("reset it: `ign rig trial reset --yes`"),
        ),
        Ok(status) => row(
            "trial",
            CheckStatus::Ok,
            format!(
                "{} — {}s remaining",
                status.license_mode, status.trial_remaining_s
            ),
            None,
        ),
        Err(err) => row(
            "trial",
            CheckStatus::Fail,
            format!("the trial state could not be read: {err}"),
            Some("`ign doctor` diagnoses gateway reachability and auth"),
        ),
    }
}

/// THE diagnosis: six rows, fixed order, `Ok` whenever it completes.
///
/// `api` is `None` when no profile resolves — NOT an error for this
/// verb (`ign e2e doctor` must be useful on a host that has not been
/// configured yet, which is exactly when a developer runs it).
pub async fn e2e_doctor(
    api: Option<&ReqwestGatewayApi>,
    env: &E2eEnv,
    opts: &E2eDoctorOptions,
) -> Result<E2eDoctorResult, CoreError> {
    let checks = vec![
        check_node(env).await,
        check_npm(env),
        check_playwright(opts),
        check_chromium(env),
        check_bundle(api, opts).await,
        check_trial(api).await,
    ];
    Ok(E2eDoctorResult {
        dir: opts.dir.display().to_string(),
        checks,
    })
}

// ---------------------------------------------------------------------------
// `ign e2e init` — the scaffold write.
// ---------------------------------------------------------------------------

/// The marker file that identifies a directory as an e2e scaffold (and
/// therefore safe to re-init over).
const SCAFFOLD_MARKER: &str = "playwright.config.mjs";

/// The gateway URL baked into a scaffold generated with NO profile.
///
/// `ign e2e init` must work before a profile is configured — scaffolding
/// is something you do on a fresh machine — so the URL falls back to
/// Ignition's stock HTTP port. The scaffold reads `IGNITION_URL` first
/// in every place it uses this, so a wrong guess here costs one env var,
/// never a re-scaffold.
pub const DEFAULT_GATEWAY_URL: &str = "http://localhost:8088";

/// `ign e2e init` options.
///
/// There is deliberately NO `yes` field: `--yes`/`-y` is a GLOBAL clap
/// arg, and the confirmation guard lives at the dispatch site so the
/// `GUARDED_OPS` registry and the MCP catalog's synthetic `confirm`
/// property both see it (D3).
#[derive(Debug, Clone)]
pub struct E2eInitOptions {
    /// Where the scaffold lands.
    pub dir: PathBuf,
    /// The project whose testing routes the helpers call.
    pub project: String,
    /// The Perspective project the browser test navigates to.
    pub run_project: String,
    /// Baked in as the `IGNITION_URL` fallback.
    pub gateway_url: String,
    /// Also download the Chromium build.
    pub browsers: bool,
}

/// What happened to one scaffold member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum E2eFileStatus {
    /// The member was written.
    Written,
    /// The destination already existed and was left untouched.
    Skipped,
}

/// One scaffold member's outcome.
#[derive(Debug, Serialize)]
pub struct E2eFile {
    /// The member path, relative to the target directory.
    pub path: String,
    /// Written or skipped.
    pub status: E2eFileStatus,
}

/// A spawned installer's outcome. A non-zero exit is DATA, not an
/// error: a network-less host must still be left with a usable
/// scaffold on disk (the lint doctor posture).
#[derive(Debug, Serialize)]
pub struct E2eCommandRun {
    /// Whether the command was spawned at all.
    pub ran: bool,
    /// The child's exit code (`null` when a signal killed it).
    pub exit_code: Option<i32>,
}

/// `ign e2e init` output — ALL keys always present.
#[derive(Debug, Serialize)]
pub struct E2eInitResult {
    /// The target directory.
    pub dir: String,
    /// Every member with its status, in write order.
    pub files: Vec<E2eFile>,
    /// The `npm install` leg.
    pub npm_install: Option<E2eCommandRun>,
    /// The browser-download leg (`null` without `--browsers`).
    pub browsers: Option<E2eCommandRun>,
}

/// The `npm install` argv.
fn npm_install_argv() -> Vec<&'static str> {
    vec!["install"]
}

/// The Playwright browser-download argv.
fn browsers_argv() -> Vec<&'static str> {
    vec!["playwright", "install", "--with-deps", "chromium"]
}

/// Human-readable renderings of the two commands, for the preview.
fn command_lines(dir: &std::path::Path, browsers: bool) -> Vec<String> {
    let mut lines = vec![format!(
        "(in {}) npm {}",
        dir.display(),
        npm_install_argv().join(" ")
    )];
    if browsers {
        lines.push(format!(
            "(in {}) npx {}",
            dir.display(),
            browsers_argv().join(" ")
        ));
    }
    lines
}

/// GATE 1 — the foreign-directory refusal.
///
/// A non-empty target that is not already a scaffold is refused BEFORE
/// any write, naming a file it found. The `ensure_checkout_target`
/// message shape: a refusal that does not say what it saw leaves the
/// user guessing which directory they actually typed.
fn refuse_foreign_directory(dir: &std::path::Path) -> Result<(), CoreError> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(()); // absent (or unreadable) — the write step owns it
    };
    let names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    if names.is_empty() {
        return Ok(()); // empty dir — init owns it from here
    }
    if names.iter().any(|name| name == SCAFFOLD_MARKER) {
        return Ok(()); // an existing scaffold: re-init is idempotent
    }
    let mut found = names.clone();
    found.sort();
    Err(CoreError::InvalidInput {
        reason: format!(
            "target directory {} is not empty and is not an e2e scaffold (no {}) — \
             refusing to clobber it; it contains {}",
            dir.display(),
            SCAFFOLD_MARKER,
            found
                .iter()
                .take(5)
                .map(|name| format!("{name:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    })
}

/// GATE 2 — the node refusal.
fn refuse_without_node(env: &E2eEnv) -> Result<(), CoreError> {
    let found = env
        .path
        .as_ref()
        .and_then(|path| find_on_path_in(path, "node"));
    if found.is_none() {
        return Err(CoreError::NodeToolAbsent {
            tool: "node".into(),
        });
    }
    Ok(())
}

/// The confirmation preview (D3) — the refusal IS the dry run.
///
/// Runs gates 1 and 2 first so the foreign-directory and node refusals
/// WIN over the confirmation refusal (being told to pass `--yes` for a
/// write that would have been refused anyway is a wasted round trip),
/// and so the previewed statuses are accurate.
///
/// Pure apart from filesystem reads: it writes nothing and spawns
/// nothing.
pub fn e2e_init_preview(env: &E2eEnv, opts: &E2eInitOptions) -> Result<String, CoreError> {
    refuse_foreign_directory(&opts.dir)?;
    refuse_without_node(env)?;

    let mut lines = vec![
        format!(
            "scaffold a Playwright E2E suite into {}",
            opts.dir.display()
        ),
        String::new(),
        format!(
            "project: {}   run-project: {}   gateway: {}",
            opts.project, opts.run_project, opts.gateway_url
        ),
        String::new(),
        "files:".to_string(),
    ];
    for path in crate::e2e::member_paths() {
        let status = if opts.dir.join(path).exists() {
            "skip (exists)"
        } else {
            "write"
        };
        lines.push(format!("  {status:<14} {path}"));
    }
    lines.push(String::new());
    lines.push("commands:".to_string());
    for command in command_lines(&opts.dir, opts.browsers) {
        lines.push(format!("  {command}"));
    }
    // `require_confirmation` renders "<operation> is destructive; rerun
    // with --yes to confirm", concatenating its suffix directly onto
    // this string. Every other guarded site passes a short noun phrase,
    // so the suffix reads fine; a multi-line preview ending in a
    // command line does NOT — it produces
    // "npm install is destructive; rerun with --yes to confirm",
    // which reads as a claim about npm. The closing noun phrase gives
    // the suffix something sensible to attach to.
    lines.push(String::new());
    lines.push("this scaffold write".to_string());
    Ok(lines.join("\n"))
}

/// Spawn an installer in the target directory, streaming its output.
///
/// An ARG VECTOR, never a shell string (the compose seam precedent):
/// the target directory is operator-named and must never be able to
/// become shell syntax. stdout/stderr are inherited so the child
/// streams through live — the `rig logs` passthrough posture.
async fn run_installer(
    env: &E2eEnv,
    dir: &std::path::Path,
    tool: &str,
    args: &[&str],
) -> E2eCommandRun {
    let Some(binary) = env
        .path
        .as_ref()
        .and_then(|path| find_on_path_in(path, tool))
    else {
        eprintln!(
            "[ign e2e init] {tool} is not on PATH — skipping `{tool} {}`",
            args.join(" ")
        );
        return E2eCommandRun {
            ran: false,
            exit_code: None,
        };
    };
    eprintln!(
        "[ign e2e init] running: (in {}) {tool} {}",
        dir.display(),
        args.join(" ")
    );
    match tokio::process::Command::new(&binary)
        .args(args)
        .current_dir(dir)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await
    {
        Ok(status) => E2eCommandRun {
            ran: true,
            exit_code: status.code(),
        },
        Err(err) => {
            eprintln!("[ign e2e init] {tool} could not run: {err}");
            E2eCommandRun {
                ran: false,
                exit_code: None,
            }
        }
    }
}

/// THE scaffold write. Gates run in a fixed order and NONE may be
/// reordered.
///
/// Gates 1 and 2 are RE-RUN here even though `e2e_init_preview` already
/// ran them at the CLI seam: the action's own re-check is what keeps
/// in-process TUI and MCP callers honest (the `api call` convention),
/// because those callers do not pass through the dispatch arm.
pub async fn e2e_init(env: &E2eEnv, opts: &E2eInitOptions) -> Result<E2eInitResult, CoreError> {
    // 1. Foreign directory — before any write.
    refuse_foreign_directory(&opts.dir)?;
    // 2. Node — before any write.
    refuse_without_node(env)?;
    // A bad template edit must not write outside the target.
    crate::e2e::validate_member_paths()?;

    // 3. Write. NEVER overwrite: an existing member is recorded
    //    `skipped` and left byte-for-byte alone, which is what makes a
    //    second init safe to run over a suite someone has edited.
    std::fs::create_dir_all(&opts.dir).map_err(|err| {
        CoreError::Internal(format!("cannot create {}: {err}", opts.dir.display()))
    })?;
    let mut files = Vec::with_capacity(crate::e2e::E2E_TEMPLATE_FILES.len());
    for (member, body) in crate::e2e::E2E_TEMPLATE_FILES {
        let destination = opts.dir.join(member);
        if destination.exists() {
            files.push(E2eFile {
                path: (*member).to_string(),
                status: E2eFileStatus::Skipped,
            });
            continue;
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                CoreError::Internal(format!("cannot create {}: {err}", parent.display()))
            })?;
        }
        let rendered =
            crate::e2e::render_template(body, &opts.project, &opts.run_project, &opts.gateway_url);
        std::fs::write(&destination, rendered).map_err(|err| {
            CoreError::Internal(format!("cannot write {}: {err}", destination.display()))
        })?;
        files.push(E2eFile {
            path: (*member).to_string(),
            status: E2eFileStatus::Written,
        });
    }

    // 4. Install. A non-zero child is DATA — a host with no network
    //    still ends up with a scaffold it can install later.
    let npm_install = Some(run_installer(env, &opts.dir, "npm", &npm_install_argv()).await);

    // 5. Browsers, only on request.
    let browsers = if opts.browsers {
        Some(run_installer(env, &opts.dir, "npx", &browsers_argv()).await)
    } else {
        None
    };

    Ok(E2eInitResult {
        dir: opts.dir.display().to_string(),
        files,
        npm_install,
        browsers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_node_major_handles_the_known_shapes() {
        assert_eq!(parse_node_major("v22.11.0\n"), Some(22));
        assert_eq!(parse_node_major("22.11.0"), Some(22));
        assert_eq!(parse_node_major("  v20.0.0  "), Some(20));
        assert_eq!(parse_node_major("v18.20.4"), Some(18), "old still parses");
        assert_eq!(parse_node_major("not a version"), None);
        assert_eq!(parse_node_major(""), None);
        assert_eq!(parse_node_major("vX.Y.Z"), None);
    }

    #[test]
    fn chromium_cache_scan_matches_both_entry_prefixes() {
        let cache = tempfile::tempdir().expect("a temp dir");
        assert!(
            !chromium_present(cache.path()),
            "an empty cache holds no browser"
        );

        std::fs::create_dir(cache.path().join("ffmpeg-1011")).unwrap();
        assert!(
            !chromium_present(cache.path()),
            "ffmpeg alone is not chromium"
        );

        std::fs::create_dir(cache.path().join("chromium-1148")).unwrap();
        assert!(chromium_present(cache.path()), "the full build matches");

        let shell = tempfile::tempdir().expect("a temp dir");
        std::fs::create_dir(shell.path().join("chromium_headless_shell-1148")).unwrap();
        assert!(
            chromium_present(shell.path()),
            "the headless shell matches too"
        );

        assert!(
            !chromium_present(&cache.path().join("does-not-exist")),
            "a missing directory is simply absent, never a panic"
        );
    }
}
