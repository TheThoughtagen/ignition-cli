//! CLI definition: the five global args (defined exactly once, here) and the
//! subcommand enum.
//!
//! Placement rules (Phase 1 research, Pattern 1):
//! - Globals set the `global` arg attribute so they propagate to every
//!   subcommand; subcommand structs never redeclare them.
//! - Never mark a global arg `required` (clap rejects required globals).
//! - Flags-only, zero interactive prompts — ever (research anti-pattern:
//!   "Prompting ever").

use std::path::PathBuf;

use clap::{ArgAction, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "ign",
    version,
    propagate_version = true,
    about = "Operate Ignition 8.3+ gateways from the terminal"
)]
pub struct Cli {
    /// Gateway profile to use (default: active profile in config)
    #[arg(long, global = true, value_name = "NAME")]
    pub profile: Option<String>,

    /// Machine-readable JSON output (stable field names)
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    pub json: bool,

    /// One-line compact JSON (implies --json)
    #[arg(long, global = true, action = ArgAction::SetTrue)]
    pub compact: bool,

    /// Non-interactive confirmation for destructive operations
    #[arg(long, short = 'y', global = true, action = ArgAction::SetTrue)]
    pub yes: bool,

    /// Increase diagnostics (-vv for HTTP trace) to stderr
    #[arg(long, short = 'v', global = true, action = ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Print version information (CLI always; gateway check when a profile resolves)
    Version,

    /// Generate shell completions (bash, zsh, fish, …) — the one stdout exception
    #[command(arg_required_else_help = true)]
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::aot::Shell,
    },

    /// Gateway status: identity, platform, uptime, license (incl. trial countdown)
    Status,

    /// List gateway modules (healthy by default)
    Modules {
        /// Show quarantined modules instead of healthy ones
        #[arg(long)]
        quarantined: bool,
    },

    /// Gateway performance metrics (current gauges + thread counts)
    Metrics {
        /// Include historic chart datapoints
        #[arg(long)]
        history: bool,
    },

    /// List gateway sessions (designers, Perspective, Vision) — or
    /// terminate one via the `terminate` subcommand
    Sessions(SessionsArgs),

    /// List database/OPC connections with healthcheck status as reported
    Connections {
        /// Filter to one connection family (default: both)
        #[arg(long, value_enum)]
        r#type: Option<ConnectionType>,
    },

    /// Query, tail, and download gateway logs; manage logger levels
    Logs(LogsArgs),

    /// Restart the gateway — destructive, refused without --yes;
    /// --wait polls until RUNNING
    Restart {
        /// Wait for the gateway to return to RUNNING (POST, then poll
        /// /StatusPing)
        #[arg(long)]
        wait: bool,
        /// Wait budget in seconds (default 300)
        #[arg(long, value_name = "SECS")]
        timeout: Option<u64>,
        /// Poll interval in seconds (default 2)
        #[arg(long, value_name = "SECS")]
        interval: Option<u64>,
    },

    /// Wait for a gateway state (readiness, restart completion, module)
    #[command(arg_required_else_help = true)]
    Wait(WaitArgs),

    /// Diagnose the gateway setup: URL, liveness, commissioning, auth,
    /// permissions, write, WebDev route, rig — exits 0 whenever the
    /// diagnosis completes (failing checks are data)
    Doctor {
        /// Probe write permission (POST scan/projects — a harmless
        /// project rescan)
        #[arg(long)]
        check_write: bool,
        /// Probe one WebDev route's presence (/system/webdev/<NAME>)
        #[arg(long, value_name = "NAME")]
        webdev_route: Option<String>,
    },

    /// Adopt this profile's gateway: native login → mint an
    /// Administrator-level API key (idempotent by name) → wire the
    /// gateway's read/write permissions → live-probe the key →
    /// persist the credential (OS keyring, env-var fallback). Re-run
    /// on an adopted gateway is an all-skip no-op. Composition flags
    /// ride the bootstrap: --project deploys the CLI's WebDev routes,
    /// --checkout lands every enabled project locally, --bake saves a
    /// restore-ready gwbk
    Adopt {
        /// Gateway login user for the native OIDC dance
        /// (default: admin)
        #[arg(long, value_name = "NAME")]
        user: Option<String>,

        /// The API-key name — the idempotency key (default: ign-cli)
        #[arg(long, value_name = "NAME")]
        key_name: Option<String>,

        /// The granted security level as a slash path
        /// (default: Authenticated/Roles/Administrator)
        #[arg(long, value_name = "PATH")]
        level: Option<String>,

        /// Deploy the CLI's WebDev routes (scriptExec on) into this
        /// project after the bootstrap
        #[arg(long, value_name = "NAME")]
        project: Option<String>,

        /// Ship the embedded TESTING bundle with the routes (the
        /// Jython framework + testing/run|tags routes + the smoke
        /// sentinel; the step asserts discover ≥1 module and a green
        /// smoke run — the empty-suite trap)
        #[arg(long, requires = "project")]
        testing: bool,

        /// Check out every enabled project into DIR/<project>
        /// (scripts decoded — grep/lint ready); existing targets skip
        #[arg(long, value_name = "DIR")]
        checkout: Option<String>,

        /// Download a roaming gwbk to FILE after everything landed —
        /// a gateway reset restored from it keeps the key and routes
        #[arg(long, value_name = "FILE")]
        bake: Option<String>,
    },

    /// Obtain a live gateway login session (the native OIDC dance) —
    /// returns the session cookie name/value, the CSRF token, the
    /// gateway URL, and a ready-to-use Playwright `storageState`
    /// document. The session material is emitted ONLY under the
    /// global --json flag; the human render prints the cookie name
    /// and the gateway URL and withholds the rest
    #[command(arg_required_else_help = true)]
    Session(SessionArgs),

    /// Manage gateway projects: list with inheritance info, new, copy,
    /// rename, set (reparent), delete, export/import (ZIP)
    #[command(arg_required_else_help = true)]
    Project(ProjectArgs),

    /// Check out a project's resources to a local tree, inspect drift,
    /// push guarded changes — the local edit loop over the
    /// project-export interchange (project RESOURCES only; tag values
    /// never appear in the tree — tags remain `ign tags` verbs)
    #[command(arg_required_else_help = true)]
    Workspace(WorkspaceArgs),

    /// Fetch a gateway resource, edit it in $EDITOR, push back
    /// (guarded) — the kubectl-edit loop over ONE resource:
    /// unchanged saves are clean no-ops (nothing pushed, never
    /// prompted), a gateway changed since fetch refuses (never
    /// force-pushed), a JSON-breaking save refuses fail-closed with
    /// the temp tree kept (path printed). Zero stdout in every mode —
    /// the editor owns the terminal (see README's edit section)
    Edit(EditArgs),

    /// Manage a project's individual resources: list, get, put,
    /// delete — the surgical edit loop (change one view/script
    /// without re-importing everything)
    #[command(arg_required_else_help = true)]
    Resource(ResourceArgs),

    /// Manage the CLI's own WebDev routes on the gateway: deploy the
    /// embedded bundle, verify the version handshake
    #[command(arg_required_else_help = true)]
    Webdev(WebdevArgs),

    /// Tag operations: manage providers (native REST) and browse /
    /// read / write tag values (the deployed WebDev routes — run
    /// `ign webdev deploy` first)
    #[command(arg_required_else_help = true)]
    Tags(TagsArgs),

    /// Manage a Docker compose rig: up/down/reset/status/logs/trial
    /// (snapshot arrives in a later plan) — docker-only, no profile
    /// needed
    #[command(arg_required_else_help = true)]
    Rig(RigArgs),

    /// Manage gateway backups (gwbk) on any profiled gateway:
    /// download (streamed) and restore (guarded)
    #[command(arg_required_else_help = true)]
    Backup(BackupArgs),

    /// EAM task orchestration (Enterprise Administration Module):
    /// history/definitions reads, guarded task create + force —
    /// every verb honestly reports the controller-mode state gate
    #[command(arg_required_else_help = true)]
    Eam(EamArgs),

    /// Run gateway-side Python (Jython) through the secret-gated
    /// scriptExec route — the opt-in is STRUCTURAL (`ign webdev
    /// deploy --with-script-exec` deploys the route + persists its
    /// secret); there is no --yes on this verb by design
    #[command(arg_required_else_help = true)]
    Script(ScriptArgs),

    /// Run the gateway's own Jython test suite through the deployed
    /// testing bundle — the agent/CI verb: a machine verdict without
    /// the Designer (`ign webdev deploy --with-testing` or
    /// `ign adopt --project NAME --testing` installs the bundle)
    #[command(arg_required_else_help = true)]
    Testing(TestingArgs),

    /// Lint local project files by delegating to ignition-lint (PATH
    /// discovery) — doctor posture: findings are DATA, exit 0
    /// whenever the tool ran; --strict passes the tool's exit code
    /// through for CI
    Lint(LintArgs),

    /// Browser end-to-end testing: diagnose the host + gateway a
    /// Playwright suite needs (`doctor`), and scaffold one that logs
    /// in through `ign session login` and talks to the gateway's
    /// testing routes (`init`). There is no `e2e run` — the
    /// scaffold's own `npm test` is the runner
    #[command(arg_required_else_help = true)]
    E2e(E2eArgs),

    /// Raw passthrough to any gateway REST endpoint — the escape
    /// hatch for the uncurated routes (the envelope's `data` is left
    /// gateway-verbatim; see README's documented contract exception)
    #[command(arg_required_else_help = true)]
    Api(ApiArgs),

    /// License inventory + trial state — the morning-check read
    /// (trial companion merged into one command)
    #[command(arg_required_else_help = true)]
    License(LicenseArgs),

    /// Gateway redundancy status — the morning-check read
    #[command(arg_required_else_help = true)]
    Redundancy(RedundancyArgs),

    /// Gateway Area Network (GAN) overview — the morning-check read
    #[command(arg_required_else_help = true)]
    Gan(GanArgs),

    /// Diagnostics support-bundle operations: generate, check
    /// status, download (streamed), wait — the support-bundle slice
    #[command(arg_required_else_help = true)]
    Diagnostics(DiagnosticsArgs),

    /// Manage gateway profiles
    #[command(arg_required_else_help = true)]
    Profile(ProfileArgs),

    /// Serve the Model Context Protocol over stdio (hidden; for MCP
    /// clients — the newline-delimited JSON-RPC 2.0 stream IS the
    /// stdout product; the routes.rs OutOfBand row lands atomically
    /// with this command, 08-06 contract)
    #[command(hide = true)]
    Mcp(McpArgs),

    /// Serve LSP over stdio (hidden; for ignition-nvim — the
    /// Content-Length-framed LSP stream IS the stdout product; the
    /// routes.rs OutOfBand row lands atomically with this command,
    /// 08-06 contract)
    #[command(hide = true)]
    Lsp,

    /// Interactive TUI cockpit
    #[cfg(feature = "tui")]
    Tui,
}

/// Hidden args for `ign mcp serve` (14-01). `serve` is a RESTRICTED
/// POSITIONAL — not a subcommand — so the clap walk (tui_coverage.rs)
/// yields exactly ONE row-requiring node, `mcp` itself: an `mcp
/// serve` subcommand leaf would need a second routes() row and extend
/// the pinned OutOfBand set past its four-member contract (an orphan
/// row fails the walk by design). clap's `value_parser = ["serve"]`
/// keeps every other spelling (`ign mcp`, `ign mcp bogus`) a
/// usage-class clap error — exit 2 BEFORE any protocol byte is
/// written. Zero stdout-facing behavior here — the mode owns its own
/// I/O entirely (`mcp::serve`).
#[derive(Debug, clap::Args)]
pub struct McpArgs {
    /// Start the stdio MCP server
    #[arg(value_parser = ["serve"], value_name = "SERVE")]
    pub serve: String,
}

/// Wait targets (02-05, HLTH-11). `gateway` and `restart` poll the
/// UNAUTHENTICATED /StatusPing (the dispatch builds a header-less
/// client for them — waiting must work when auth is broken); `module`
/// is an authed read.
#[derive(Debug, clap::Args)]
pub struct WaitArgs {
    #[command(subcommand)]
    pub command: WaitCmd,
}

#[derive(Debug, Subcommand)]
pub enum WaitCmd {
    /// Wait until the gateway reports RUNNING (unauthenticated
    /// StatusPing — works even when auth is broken or absent)
    Gateway {
        /// Poll interval in seconds
        #[arg(long, default_value_t = 2, value_name = "SECS")]
        interval: u64,
        /// Give up after this many seconds
        #[arg(long, default_value_t = 120, value_name = "SECS")]
        timeout: u64,
    },
    /// Wait for a restart to complete — restart-aware: shares
    /// `restart --wait`'s semantics (non-RUNNING observed once →
    /// RUNNING; a 5 s floor guards the all-RUNNING case)
    Restart {
        /// Poll interval in seconds
        #[arg(long, default_value_t = 2, value_name = "SECS")]
        interval: u64,
        /// Give up after this many seconds (default 300)
        #[arg(long, default_value_t = 300, value_name = "SECS")]
        timeout: u64,
    },
    /// Wait until a module reports ACTIVE
    Module {
        /// Module id (see `ign modules`)
        id: String,
        /// Poll interval in seconds
        #[arg(long, default_value_t = 2, value_name = "SECS")]
        interval: u64,
        /// Give up after this many seconds
        #[arg(long, default_value_t = 120, value_name = "SECS")]
        timeout: u64,
    },
}

/// Project subcommands (03-01, PROJ-01/02). `delete` is the family's
/// ONE destructive verb (`--yes`-guarded, exit 2 without); copy/
/// rename/set create or relabel — never destroy — so they carry NO
/// guard (planner decision per research).
#[derive(Debug, clap::Args)]
pub struct ProjectArgs {
    #[command(subcommand)]
    pub command: ProjectCommand,
}

#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// List every runnable project: name, title, enabled, parent,
    /// inheritable (inheritance info from the items themselves)
    List,
    /// Create a project (only provided fields ride the create body)
    New {
        /// Project name
        name: String,
        /// Display title
        #[arg(long, value_name = "TEXT")]
        title: Option<String>,
        /// Long description
        #[arg(long, value_name = "TEXT")]
        description: Option<String>,
        /// Parent project (inheritance)
        #[arg(long, value_name = "NAME")]
        parent: Option<String>,
        /// Mark this project eligible as a parent
        #[arg(long)]
        inheritable: bool,
        /// Create the project disabled
        #[arg(long)]
        disabled: bool,
    },
    /// Copy a project with all its resources
    Copy {
        /// Source project name
        src: String,
        /// Destination name (must not exist)
        dst: String,
    },
    /// Rename a project (native rename, not copy+delete)
    Rename {
        /// Current name
        old_name: String,
        /// New name
        new_name: String,
    },
    /// Set project fields — --parent IS the inheritance move (reparent)
    #[command(group(
        clap::ArgGroup::new("set_fields")
            .required(true)
            .multiple(true)
            .args(&["title", "description", "parent", "set_enabled", "disabled", "inheritable"])
    ))]
    Set {
        /// Project name
        name: String,
        /// Display title
        #[arg(long, value_name = "TEXT")]
        title: Option<String>,
        /// Long description
        #[arg(long, value_name = "TEXT")]
        description: Option<String>,
        /// Parent project (reparent)
        #[arg(long, value_name = "NAME")]
        parent: Option<String>,
        /// Enable the project
        #[arg(long, conflicts_with = "disabled")]
        set_enabled: bool,
        /// Disable the project
        #[arg(long)]
        disabled: bool,
        /// Whether this project may serve as a parent (true/false)
        #[arg(long, value_name = "BOOL")]
        inheritable: Option<bool>,
    },
    /// Delete a project — destructive, refused without --yes
    Delete {
        /// Project name
        name: String,
    },
    /// Export a project as a ZIP archive (streams to disk)
    Export {
        /// Project name
        name: String,
        /// Output file (default: the gateway's Content-Disposition
        /// name, else <name>.zip); with --decode-scripts, the output
        /// DIRECTORY (default <name>-export/)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
        /// Also decode embedded JSON scripts (Perspective view.json
        /// etc.) into editable `<member>.<n>.py` sidecars beside a
        /// copied member tree + scripts-manifest.json — the output is
        /// a DIRECTORY (script-python members are already plain .py
        /// and never decode; expressions pass through)
        #[arg(long)]
        decode_scripts: bool,
    },
    /// Import a project from a ZIP archive
    Import {
        /// Project name to import as
        name: String,
        /// ZIP file path, or - to read the archive from stdin; with
        /// --encode-scripts, a decoded export DIRECTORY (re-zipped
        /// with the sidecars spliced back before upload; stdin is
        /// invalid in that mode)
        #[arg(long, value_name = "PATH")]
        file: String,
        /// Collision policy: abort refuses when the name exists
        /// (default); overwrite REPLACES the entire project —
        /// destructive, requires --yes. merge is Designer-only (not a
        /// value; the README documents why)
        #[arg(long, value_enum, default_value_t = CollisionPolicy::Abort)]
        collision_policy: CollisionPolicy,
        /// --file points at a decoded export DIRECTORY (from
        /// `export --decode-scripts`): the sidecars are spliced back
        /// and the manifest stripped before the standard import path
        #[arg(long)]
        encode_scripts: bool,
    },
    /// Compare a project across two gateway profiles — statuses are
    /// B-relative-to-A (`added` = in B only, `removed` = in A only,
    /// `changed` = differing after resource.json normalization)
    Diff {
        /// Baseline profile (A)
        profile_a: String,
        /// Compared profile (B — the diff is B relative to A)
        profile_b: String,
        /// Project name
        #[arg(long, value_name = "NAME")]
        project: String,
    },
    /// Promote selected resources from profile A into profile B
    /// (direction is ALWAYS A→B) — destructive on B: the whole
    /// project is overwrite-imported, refused without --yes
    Sync {
        /// Source profile (A)
        profile_a: String,
        /// Target profile (B)
        profile_b: String,
        /// Project name
        #[arg(long, value_name = "NAME")]
        project: String,
        /// Resource user path to promote (repeatable)
        #[arg(long, value_name = "PATH")]
        resource: Vec<String>,
        /// Promote every resource the diff reports added or changed
        /// (never removed ones — deletion needs --delete)
        #[arg(long)]
        all_changed: bool,
        /// Also remove B's resources the diff reports removed
        /// (default: upsert-only, nothing is ever deleted)
        #[arg(long)]
        delete: bool,
    },
}

/// Workspace subcommands (13-07): the local edit loop. `checkout`
/// writes the mapped tree + records `.ign-workspace.json` (the
/// manifest IS the workspace identity — status/push read it and the
/// user never re-types the project); `status` reports PUSH-RELATIVE
/// drift (every row says what push would do); `push` is the guarded
/// splice of local edits into a FRESH gateway export.
#[derive(Debug, clap::Args)]
pub struct WorkspaceArgs {
    #[command(subcommand)]
    pub command: WorkspaceCommand,
}

#[derive(Debug, Subcommand)]
pub enum WorkspaceCommand {
    /// Check out a project's resources to a local tree — mapped
    /// paths + the recorded `.ign-workspace.json` manifest + an
    /// idempotent `.gitignore` (manifest committed; codec artifacts
    /// ignored). Read-only on the wire: one export GET, zero imports.
    Checkout {
        /// Project name to check out
        project: String,
        /// Target directory for the workspace tree (created if
        /// absent; an unrelated non-empty directory refuses — never
        /// clobbered)
        target: PathBuf,
        /// Decode embedded JSON scripts to editable .py sidecars
        /// (nvim-editable; encodes back cleanly — an unedited
        /// re-encode is byte-exact; `scripts-manifest.json` keys the
        /// splice)
        #[arg(long)]
        decode_scripts: bool,
    },
    /// Report workspace drift against the gateway — states are
    /// PUSH-RELATIVE (each row names what push would do: write /
    /// leave / refuse); the project comes from the workspace
    /// manifest, never re-typed. Read-only: one export GET.
    Status {
        /// Workspace root (default: the current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Push local edits to the gateway — guarded (refused without
    /// --yes; the refusal message IS the blast-radius preview);
    /// conflicts refuse EVEN WITH --yes (manual reconciliation);
    /// deletions need --delete (default: reported, skipped);
    /// untracked files are never imported; an empty selection
    /// writes nothing.
    Push {
        /// Workspace root (default: the current directory)
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Also delete gateway members removed locally (default:
        /// reported, skipped)
        #[arg(long)]
        delete: bool,
    },
}

/// `ign edit` args (13-08): the kubectl-edit loop over ONE resource.
/// The resource path scopes what $EDITOR opens (the whole tree still
/// decodes for encode-back context); the push rides the
/// project-import wire behind the ONE confirmation gate. `--yes` is
/// the GLOBAL flag (globals-once) — never redeclared here.
#[derive(Debug, clap::Args)]
pub struct EditArgs {
    /// Project containing the resource
    pub project: String,
    /// Resource user path to open in $EDITOR (e.g.
    /// `ignition/script-python/e2e/scratch` — `ign resource list`
    /// names the valid members)
    pub resource_path: String,
}

/// Resource subcommands (03-03, PROJ-05). `delete` is the family's
/// destructive verb (`--yes`-guarded, exit 2 without — the
/// sessions-terminate shape); `put` is an upsert (create-or-replace
/// ONE resource with explicit content) and stays friction-free per
/// the planner decision — agents pass exactly what they want written.
#[derive(Debug, clap::Args)]
pub struct ResourceArgs {
    #[command(subcommand)]
    pub command: ResourceCommand,
}

#[derive(Debug, Subcommand)]
pub enum ResourceCommand {
    /// List a project's resources (one path per line in human mode)
    List {
        /// Project name
        project: String,
        /// Only paths under this prefix (rides the wire as the
        /// server-side `path` filter)
        #[arg(long, value_name = "PREFIX")]
        prefix: Option<String>,
    },
    /// Read one resource: JSON pretty-printed, text raw — binary
    /// (data.bin-class) resources refuse with exit 6
    Get {
        /// Project name
        project: String,
        /// Resource path, slashes kept (e.g.
        /// `ignition/script-python/e2e/scratch`)
        path: String,
    },
    /// Write one resource (upsert: created if absent, replaced if
    /// present) — JSON if parseable (application/json), else UTF-8
    /// text (text/plain); binary-looking input refuses
    Put {
        /// Project name
        project: String,
        /// Resource path
        path: String,
        /// File to read the content from, or `-` for stdin
        #[arg(long, value_name = "PATH")]
        file: String,
    },
    /// Delete one resource — destructive, refused without --yes
    Delete {
        /// Project name
        project: String,
        /// Resource path
        path: String,
    },
}

/// Webdev subcommands (05-03, WEB-01/02). Deploy is deliberately NOT
/// `--yes`-guarded: the dedicated project (default `ign-cli`) is
/// CLI-OWNED — born from the first deploy zip and overwrite-replaced
/// on every deploy (replace-not-merge is the CONTRACT here; user
/// projects are never touched — README documents). scriptExec rides
/// only on explicit request, gated by a deploy-time generated shared
/// secret stored in the profile config at 0600.
#[derive(Debug, clap::Args)]
pub struct WebdevArgs {
    #[command(subcommand)]
    pub command: WebdevCommand,
}

#[derive(Debug, Subcommand)]
pub enum WebdevCommand {
    /// Deploy the embedded route bundle into the dedicated project
    /// (overwrite-replace — the CLI owns that project wholesale)
    Deploy {
        /// Target project (default ign-cli; the CLI owns it wholesale)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
        /// Also deploy the secret-gated scriptExec route (a fresh
        /// secret is generated and stored at 0600 when none exists)
        #[arg(long)]
        with_script_exec: bool,
        /// Generate a FRESH scriptExec secret before deploying (any
        /// route deployed with the old secret starts refusing)
        #[arg(long, requires = "with_script_exec")]
        rotate_secret: bool,
        /// Also deploy the embedded TESTING bundle — the Jython test
        /// framework (testing.runner & co., with the permanent
        /// testing.__tests__ smoke sentinel) and the testing/run +
        /// testing/tags WebDev routes
        #[arg(long)]
        with_testing: bool,
    },
    /// Probe every route's version handshake — a READ: exit 0
    /// whenever the sweep completes, per-route degradation is data
    Status {
        /// Target project (default ign-cli)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
    },
}

/// Tags args (05-04, TAGS-01..04) — the grouped-subfamily pattern:
/// `tags provider …` nests one level deeper (the family's native
/// REST half), browse/read/write ride the TOP level. The
/// webdev-dependent arms carry `--project` (default `ign-cli`, the
/// same default as deploy — the deployed routes live in that
/// project).
#[derive(Debug, clap::Args)]
pub struct TagsArgs {
    #[command(subcommand)]
    pub command: TagsCommand,
}

#[derive(Debug, Subcommand)]
pub enum TagsCommand {
    /// Manage tag providers (native config-resource REST — no
    /// deployed routes involved): list with tag counts + health,
    /// create a STANDARD provider, delete (guarded)
    #[command(subcommand)]
    Provider(TagsProviderCommand),

    /// Browse tags as a tree (Property children filtered by
    /// default) — providers appear at the root; needs the deployed
    /// routes (`ign webdev deploy`) — or run fully OFFLINE against
    /// an export via --from-export
    Browse {
        /// Tag path to browse from (default: the root — providers)
        path: Option<String>,
        /// Case-insensitive substring filter on name and full path
        #[arg(long, value_name = "SUBSTR")]
        filter: Option<String>,
        /// Include Property children (filtered out by default)
        #[arg(long)]
        include_properties: bool,
        /// Project holding the deployed routes (default ign-cli)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
        /// Browse an OFFLINE tag export instead of the gateway (no
        /// profile, no credential, no routes): the CLI's own `tags
        /// export` JSON, a legacy `<provider>.json` whole tree, or a
        /// git-module tags/ directory — mutually exclusive with the
        /// positional browse path
        #[arg(long, value_name = "PATH", conflicts_with_all = ["path"])]
        from_export: Option<PathBuf>,
    },
    /// Read one or more tag values (quality and timestamp included)
    /// — needs the deployed routes
    Read {
        /// Tag paths to read, e.g. `[default]T1` (one or more)
        #[arg(value_name = "PATH", required = true)]
        paths: Vec<String>,
        /// Project holding the deployed routes (default ign-cli)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
    },
    /// Write a value to a tag — the value is parsed as a JSON
    /// scalar (number/bool/null); anything unparseable is sent as a
    /// string; arrays/objects refuse — needs the deployed routes
    Write {
        /// Tag path to write, e.g. `[default]T1`
        path: String,
        /// Value to write: JSON scalar (42, 1.5, true), else the raw
        /// text is sent as a string
        #[arg(long, value_name = "V")]
        value: String,
        /// Project holding the deployed routes (default ign-cli)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
    },
    /// Tag CONFIGURATION CRUD (the surgical edit loop — JSON in,
    /// JSON out): get/create/edit/delete tag configs through the
    /// deployed tagConfig route; stringified values are re-parsed so
    /// agents see real JSON
    #[command(subcommand)]
    Config(TagsConfigCommand),
    /// UDT types and definitions (recursive: parameters + nested
    /// children) — needs the deployed routes
    #[command(subcommand)]
    Udt(TagsUdtCommand),
    /// Alarm operations: active status (with filters), journal
    /// history, acknowledge — needs the deployed routes
    #[command(subcommand)]
    Alarms(TagsAlarmsCommand),
    /// Tag history queries (historian-backed values) — needs the
    /// deployed routes; structurally safe on any rig (data requires
    /// a provisioned historian)
    #[command(subcommand)]
    History(TagsHistoryCommand),
    /// Export tag subtrees — json writes the gateway's native JSON
    /// interchange (the lossless round-trip); xml passes the
    /// gateway's own XML through byte-for-byte; csv is CLI-GENERATED
    /// and LOSSY (no alarms, legacy 48-column set, numeric enums —
    /// the gateway cannot export CSV)
    Export {
        /// Tag paths to export, e.g. `[default]P5` (one or more)
        #[arg(value_name = "PATH", required = true)]
        paths: Vec<String>,
        /// Output file (`-` = stdout — the raw payload, no envelope;
        /// default: `<last-path-segment>.<ext>` in the cwd, ext per
        /// format)
        #[arg(short = 'o', long, value_name = "FILE")]
        output: Option<PathBuf>,
        /// Payload format: json (the gateway's native interchange —
        /// default), xml (raw gateway bytes), or csv (CLI-generated
        /// and LOSSY: no alarms, legacy columns only, numeric enum
        /// coercion — what was dropped prints on stderr)
        #[arg(long, value_enum, default_value_t = TransferFormat::Json, value_name = "FORMAT")]
        format: TransferFormat,
        /// Project holding the deployed routes (default ign-cli)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
    },
    /// Import a tag export into a target provider — json is the
    /// native interchange; xml/csv ride the gateway's importTags
    /// passthrough. A loss scan reports what xml/csv would drop or
    /// coerce BEFORE the import and refuses exit 2 unless --yes.
    /// Collision behavior: abort (default) refuses on collisions;
    /// overwrite replaces them (destructive: requires --yes)
    Import {
        /// The export file to import (`-` = stdin)
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
        /// Target tag provider (must exist — `ign tags provider
        /// create NAME` first)
        #[arg(long, value_name = "NAME")]
        provider: String,
        /// Payload format: json (the gateway's native interchange —
        /// default), xml (gateway XML), or csv (legacy CSV — alarms
        /// never arrive; the loss scan reports the gap)
        #[arg(long, value_enum, default_value_t = TransferFormat::Json, value_name = "FORMAT")]
        format: TransferFormat,
        /// Collision policy: abort refuses when tags already exist
        /// (default); overwrite replaces them (destructive:
        /// requires --yes). merge is Designer-only (not a value)
        #[arg(long, value_enum, default_value_t = CollisionPolicy::Abort)]
        collision_policy: CollisionPolicy,
        /// Project holding the deployed routes (default ign-cli)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
    },
}

/// The bulk-transfer payload format (11-05) — the clap surface for
/// core's `ExportFormat`/`ImportFormat` (core keeps plain enums; the
/// From impls live beside this one, the CollisionPolicy precedent).
/// json is the default so every pre-11-05 invocation parses and
/// behaves byte-identically.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum TransferFormat {
    /// The gateway's JSON interchange (default — lossless)
    #[default]
    Json,
    /// Gateway XML — RAW-BYTE passthrough both directions (no
    /// parse/normalize in the transfer path)
    Xml,
    /// Legacy CSV — import: gateway passthrough (alarms never
    /// arrive); export: CLI-GENERATED and LOSSY (the gateway cannot
    /// export CSV)
    Csv,
}

impl std::fmt::Display for TransferFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Self::Json => "json",
            Self::Xml => "xml",
            Self::Csv => "csv",
        };
        f.write_str(label)
    }
}

/// `tags config …` — the configuration CRUD subfamily (05-05,
/// TAGS-05): the get→edit-file→write-back surgical loop. Definition
/// files are the configure shape (README's traps table: tagType
/// discriminator, nested children, alarms-as-LIST).
#[derive(Debug, Subcommand)]
pub enum TagsConfigCommand {
    /// Get a tag's configuration as (pretty) JSON — stringified
    /// value/defaultValue sub-dicts re-parsed into real JSON
    Get {
        /// Tag path, e.g. `[default]P5/T1`
        path: String,
        /// Project holding the deployed routes (default ign-cli)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
    },
    /// Create a tag from a JSON definition file (`-` = stdin) —
    /// aborts on an existing node (collision policy 'a')
    Create {
        /// Tag path to create, e.g. `[default]P5/T1`
        path: String,
        /// JSON definition file (`-` = stdin): `{tagType, value,
        /// alarms, …}` — see the README configure-shape table
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
        /// Project holding the deployed routes (default ign-cli)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
    },
    /// Edit a tag's configuration from a JSON definition file (`-` =
    /// stdin) — overwrites that single node (collision policy 'o')
    Edit {
        /// Tag path to edit, e.g. `[default]P5/T1`
        path: String,
        /// JSON definition file (`-` = stdin)
        #[arg(long, value_name = "FILE")]
        file: PathBuf,
        /// Project holding the deployed routes (default ign-cli)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
    },
    /// Delete tag configurations — destructive, refused without
    /// --yes (the guard fires before ANY resolution: zero network
    /// work)
    Delete {
        /// Tag paths to delete (one or more)
        #[arg(value_name = "PATH", required = true)]
        paths: Vec<String>,
        /// Project holding the deployed routes (default ign-cli)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
    },
}

/// `tags udt …` — the UDT subfamily (05-05, TAGS-06).
#[derive(Debug, Subcommand)]
pub enum TagsUdtCommand {
    /// List the provider's UDT types
    Types {
        /// Tag provider whose `_types_` folder to browse (default
        /// `default`)
        #[arg(long, default_value = "default", value_name = "NAME")]
        provider: String,
        /// Project holding the deployed routes (default ign-cli)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
    },
    /// Get a UDT definition (parameters + nested children, recursive)
    Def {
        /// UDT type name, e.g. `Motor`
        name: String,
        /// Tag provider (default `default`)
        #[arg(long, default_value = "default", value_name = "NAME")]
        provider: String,
        /// Project holding the deployed routes (default ign-cli)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum TagsProviderCommand {
    /// List the gateway's tag providers (tag counts + health)
    List,
    /// Create a STANDARD tag provider (DB-backed providers are out
    /// of scope at MVP)
    Create {
        /// Provider name to create
        name: String,
    },
    /// Delete a tag provider — destructive, refused without --yes
    Delete {
        /// Provider name to delete
        name: String,
    },
}

/// `tags alarms …` — the alarm subfamily (05-06, TAGS-07). History
/// needs a JOURNAL-PROVISIONED gateway (database connection +
/// alarm-journal profile + general-alarm-settings — default rigs
/// refuse exit 6 with the provisioning hint); acknowledge is the
/// gateway-scope 3-arg form, so `--username` is REQUIRED (no
/// default-guessing) and ack is deliberately NOT `--yes`-guarded
/// (acknowledging never un-acknowledges anything — a state-advancing
/// read-adjacent verb).
#[derive(Debug, Subcommand)]
pub enum TagsAlarmsCommand {
    /// List active alarms — eventId/source/state/priority/name
    Active {
        /// Filter by alarm source (e.g. `prov:default`)
        #[arg(long, value_name = "SOURCE")]
        source: Option<String>,
        /// Filter by priority (e.g. `High`)
        #[arg(long, value_name = "PRIORITY")]
        priority: Option<String>,
        /// Filter by state (e.g. `Active, Unacknowledged`)
        #[arg(long, value_name = "STATE")]
        state: Option<String>,
        /// Project holding the deployed routes (default ign-cli)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
    },
    /// Query alarm history — requires a journal-provisioned gateway
    /// (default rigs refuse with the provisioning hint naming the
    /// missing chain)
    History {
        /// Window start: RFC3339 timestamp or epoch-ms
        #[arg(long, value_name = "T")]
        start: String,
        /// Window end: RFC3339 timestamp or epoch-ms
        #[arg(long, value_name = "T")]
        end: String,
        /// Project holding the deployed routes (default ign-cli)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
    },
    /// Acknowledge alarms (explicit --username: the 3-arg wire form
    /// needs it). NOT --yes-guarded — acknowledging never
    /// un-acknowledges anything
    Ack {
        /// Alarm event ids to acknowledge (from `alarms active`)
        #[arg(value_name = "ID", required = true)]
        ids: Vec<String>,
        /// Acknowledgement note (default: empty)
        #[arg(long, value_name = "NOTE")]
        note: Option<String>,
        /// The username acknowledging (REQUIRED — the 3-arg wire
        /// form needs it; the CLI never guesses one)
        #[arg(long, value_name = "NAME")]
        username: String,
        /// Project holding the deployed routes (default ign-cli)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
    },
}

/// `tags history …` — the historian subfamily (05-06, TAGS-08).
#[derive(Debug, Subcommand)]
pub enum TagsHistoryCommand {
    /// Query historical tag values — t_stamp + one column per tag;
    /// structurally safe anywhere (data requires a provisioned
    /// historian)
    Query {
        /// Tag paths to query, e.g. `[default]T1` (one or more)
        #[arg(value_name = "PATH", required = true)]
        paths: Vec<String>,
        /// Window start: RFC3339 timestamp or epoch-ms
        #[arg(long, value_name = "T")]
        start: String,
        /// Window end: RFC3339 timestamp or epoch-ms
        #[arg(long, value_name = "T")]
        end: String,
        /// Maximum rows returned
        #[arg(long, value_name = "N")]
        return_size: Option<i64>,
        /// Aggregation mode (e.g. `LastValue`, `Average`, `MinMax`);
        /// default is the route's LastValue
        #[arg(long, value_name = "MODE")]
        aggregation: Option<String>,
        /// Project holding the deployed routes (default ign-cli)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
    },
}

/// Rig args (04-01, RIG-01): `--rig` rides the TOP level (the
/// SessionsArgs `--type` precedent) so every rig verb shares it. The
/// command enum carries ONLY the wired verbs — later plans extend it
/// one variant at a time (the established extend-per-plan chore; no
/// `unimplemented!()` stubs, every commit compiles clippy-clean).
#[derive(Debug, clap::Args)]
pub struct RigArgs {
    /// Rig to operate on (default: IGNITION_RIG, then [rig].default,
    /// then the cwd/convention scan)
    #[arg(long, value_name = "NAME")]
    pub rig: Option<String>,

    #[command(subcommand)]
    pub command: RigCommand,
}

#[derive(Debug, Subcommand)]
pub enum RigCommand {
    /// Bring the rig up (compose up -d --wait) and wait for the
    /// gateway: RUNNING, or uncommissioned-as-data (exit 0 + wizard
    /// hint in warnings)
    Up {
        /// Wait budget in seconds — BOTH compose's --wait-timeout and
        /// the commissioned probe deadline (default 300)
        #[arg(long, default_value_t = 300, value_name = "SECS")]
        timeout: u64,
    },
    /// Stop the rig (compose down --remove-orphans; volumes KEPT —
    /// `reset` owns the teardown half)
    Down,
    /// Tear the rig down AND remove its volumes (down -v
    /// --remove-orphans), then bring it back up fresh — destructive,
    /// refused without --yes; no stale project/trial state survives
    Reset {
        /// Wait budget in seconds — BOTH compose's --wait-timeout and
        /// the commissioned probe deadline (default 300)
        #[arg(long, default_value_t = 300, value_name = "SECS")]
        timeout: u64,
    },
    /// Structured status: services, ports, volumes (allowlist JSON;
    /// a down rig is exit-0 data)
    Status,
    /// Stream the rig's container logs (compose logs passthrough —
    /// raw lines, no envelope in any mode; the third streaming
    /// exception, README-documented)
    Logs {
        /// Lines to show from the end of each service's logs
        #[arg(long, default_value_t = 200, value_name = "N")]
        tail: u32,
        /// Follow: stream new lines as they occur (Ctrl-C stops —
        /// default process kill, no envelope)
        #[arg(short = 'f', long)]
        follow: bool,
        /// One service's logs only (see `ign rig status` for names)
        service: Option<String>,
    },
    /// Trial-license state: status is credential-free truth; reset
    /// (guarded) restarts an EXPIRED trial via the mechanism ladder
    /// (token-auth POST, else native gateway login)
    Trial(TrialArgs),
    /// Snapshot the rig's gateway: native gwbk (roaming backup,
    /// streamed) + per-project exports + manifest.json, composed in a
    /// timestamped directory — repeatable state
    Snapshot {
        /// Output directory (default:
        /// ./ign-rig-snapshots/<rig>-<yyyyMMdd-HHmmss>/)
        #[arg(short = 'o', long, value_name = "DIR")]
        output: Option<PathBuf>,
    },
    /// Restore a gwbk onto the rig's gateway — destructive, refused
    /// without --yes; synchronous restore + restart, then a witnessed
    /// RUNNING wait
    Restore {
        /// The gwbk file to restore (from `ign rig snapshot`)
        #[arg(long, value_name = "PATH")]
        file: PathBuf,
        /// Post-restore RUNNING wait budget in seconds (floored at
        /// 300 — the gateway restarts after a restore)
        #[arg(long, default_value_t = 300, value_name = "SECS")]
        timeout: u64,
    },
}

/// Trial subcommands (04-03, RIG-02/03). `status` reads the
/// unauthenticated trial endpoint + banners cross-check. `reset` is
/// the family's destructive verb (`--yes`-guarded, exit 2 without —
/// the reset precedent); the password NEVER rides a flag (env/secret
/// only — redaction discipline).
#[derive(Debug, clap::Args)]
pub struct TrialArgs {
    #[command(subcommand)]
    pub command: TrialCommand,
}

#[derive(Debug, Clone, Subcommand)]
pub enum TrialCommand {
    /// Show the trial state: licenseMode, trialState, seconds left,
    /// expired — plus the banners cross-check. No credential needed
    /// (the endpoints answer unauthenticated — fresh-rig friendly)
    Status,
    /// Reset an EXPIRED trial to a fresh window — destructive,
    /// refused without --yes. Mechanism ladder: API-token POST
    /// (IGNITION_TOKEN) → native gateway login (--user /
    /// IGNITION_USER + IGNITION_PASSWORD). Non-expired trials refuse
    /// (trial_not_expired)
    Reset {
        /// Gateway admin username for the login rung (password comes
        /// from IGNITION_PASSWORD — never a flag)
        #[arg(long, value_name = "NAME")]
        user: Option<String>,
    },
}

/// Backup subcommands (07-02, BKUP-01) — the standalone surface of
/// the Phase 4 gwbk wire. `restore` is the family's destructive verb
/// (the 8th `--yes`-guarded set member: it REPLACES this gateway's
/// state); `download` is a streamed read, unguarded.
#[derive(Debug, clap::Args)]
pub struct BackupArgs {
    #[command(subcommand)]
    pub command: BackupCommand,
}

#[derive(Debug, Subcommand)]
pub enum BackupCommand {
    /// Download a gwbk backup (streamed to disk)
    Download {
        /// Output file (default: the gateway's Content-Disposition
        /// name, else <profile>-backup.gwbk)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
        /// Backup type: roaming = portable across gateways (default);
        /// all includes gateway-specific state
        #[arg(long, value_enum, default_value_t = CliBackupType::Roaming)]
        r#type: CliBackupType,
    },
    /// Restore a gwbk onto THIS gateway — destructive, refused
    /// without --yes; the gateway restarts and blocks for minutes
    /// after the restore
    Restore {
        /// The gwbk file to restore (from `ign backup download` or
        /// `ign rig snapshot`)
        file: PathBuf,
    },
}

/// CLI value-enum mirror of the core `BackupType` (ignition-core
/// stays clap-free; `From` converts at the dispatch seam).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CliBackupType {
    /// Portable backup (cross-gateway)
    Roaming,
    /// Includes gateway-specific state
    All,
}

impl From<CliBackupType> for ignition_core::client::backup::BackupType {
    fn from(value: CliBackupType) -> Self {
        match value {
            CliBackupType::Roaming => Self::Roaming,
            CliBackupType::All => Self::All,
        }
    }
}

/// EAM subcommands (07-02, BKUP-02) — the read-heavy surface with
/// guarded writes. Every runtime verb honestly reports the
/// controller-mode state gate (`eam_not_controller` on a stock
/// gateway — the README documents the manual flip).
#[derive(Debug, clap::Args)]
pub struct EamArgs {
    #[command(subcommand)]
    pub command: EamCommand,
}

#[derive(Debug, Subcommand)]
pub enum EamCommand {
    /// EAM task run history (the gateway's own newest-first order)
    History {
        /// Max entries (default 200 — ALWAYS sent explicitly; the
        /// server default is unlimited)
        #[arg(long, value_name = "N")]
        limit: Option<u32>,
        /// Substring search over task names
        #[arg(long, value_name = "TEXT")]
        search: Option<String>,
    },
    /// Task definitions: bare `ign eam tasks` lists; with a name
    /// shows one definition + its scheduled state
    Tasks {
        /// One task definition's detail (omit to list all)
        name: Option<String>,
    },
    /// Task definition writes (create; force-dispatch) — `eam task`
    /// requires a subcommand (the `rig trial` shape)
    #[command(subcommand)]
    Task(EamTaskCommand),
}

/// `eam task …` — the guarded writes (07-02 Task 3; the five
/// lifecycle/mutation verbs arrive 10-04). `new` carries the typed
/// guard ladder (backup+OnDemand unguarded; mutating types and any
/// non-OnDemand schedule need --yes; restore/install/upgrade types
/// refuse outright — see README); `force` dispatches NOW, always
/// guarded; `suspend`/`resume`/`cancel`/`modify`/`delete` are the
/// two-tier guarded verbs (pure precheck → blast-radius preview
/// fetch → confirm-with-preview → action — the refusal message IS
/// the blast radius).
#[derive(Debug, Subcommand)]
pub enum EamTaskCommand {
    /// Create a task definition (scheduleMode defaults to OnDemand —
    /// never auto-fires)
    New {
        /// Task definition name (any resource name — e.g. nightly-backup)
        name: String,
        /// Task type — the openapi taxonomy, three classes with different guard consequences:
        ///
        /// - benign (no --yes with the default OnDemand schedule): eam_backup
        ///
        /// - mutating (need --yes — they act on their agent targets when dispatched):
        ///   eam_restart, eam_sendProject, eam_sendResource, eam_sendTags,
        ///   eam_activateLicense, eam_updateLicense, eam_unactivateLicense
        ///
        /// - refused (exit 6 eam_task_type_refused — fleet-destructive; run from the EAM console):
        ///   eam_restoreBackup, eam_installModules, eam_remoteUpgrade
        ///
        /// Any OTHER type fails safe to the --yes rung (unknown future
        /// types stay accepted, never silently unguarded).
        ///
        /// Example: ign eam task new nightly-backup eam_backup --target gw-a
        r#type: String,
        /// Target gateway name (repeatable; the GNET agent id)
        #[arg(long, value_name = "NAME")]
        target: Vec<String>,
        /// Setting as K=V with scalar auto-typing — bool/int ride
        /// typed, anything else stays a string (repeatable;
        /// arrays/objects need --definition)
        #[arg(long, value_name = "K=V")]
        setting: Vec<String>,
        /// Full-JSON settings file deep-merged over the composed
        /// `config.settings` (the typed/array settings path)
        #[arg(long, value_name = "PATH", conflicts_with = "setting")]
        definition: Option<PathBuf>,
        /// Schedule mode (default OnDemand — never auto-fires;
        /// Immediate/Scheduled/AtTime/AtDelay require --yes)
        #[arg(long, value_enum, default_value_t = ScheduleMode::OnDemand)]
        schedule_mode: ScheduleMode,
    },
    /// Force-dispatch a task NOW — destructive, refused without
    /// --yes (it dispatches to the agent targets immediately). The
    /// confirmation prompt (and every refusal) carries the
    /// blast-radius preview line.
    Force {
        /// Task definition name to dispatch
        name: String,
    },
    /// Suspend a task definition's scheduled dispatches — TASK-scoped
    /// runtime verb (there is NO agent-level suspend on the wire; an
    /// "agent" is suspended by suspending its tasks). Refused exit 2
    /// without --yes, the blast-radius preview in the refusal
    /// message; on a stock gateway the runtime seam honestly refuses
    /// `eam_not_controller` (exit 6).
    Suspend {
        /// Task definition name to suspend
        name: String,
    },
    /// Resume a suspended task definition — the suspend inverse
    /// (TASK-scoped, like suspend). Refused exit 2 without --yes
    /// with the blast-radius preview in the message.
    Resume {
        /// Task definition name to resume
        name: String,
    },
    /// Cancel a task's PENDING execution (TASK-scoped). Refused exit
    /// 2 without --yes with the blast-radius preview in the message;
    /// nothing pending is an honest no-op (`fired: false`), a
    /// `canCancel: false` row reports the gateway's own refusal.
    Cancel {
        /// Task definition whose pending execution to cancel
        name: String,
    },
    /// Rewrite targeted keys of a task definition — the full-record
    /// read-modify-write (every key the gateway answered rides back;
    /// only the targeted keys change). Rename is deliberately
    /// ABSENT (the wire answers PUT-rename with 404 — see README's
    /// create-new + delete-old composite). Refused exit 2 without
    /// --yes with the blast-radius preview in the message.
    ///
    /// Example: ign eam task modify nightly-backup --enable
    Modify {
        /// Task definition to modify
        name: String,
        /// Flip the definition's `enabled` key to true
        #[arg(long)]
        enable: bool,
        /// Flip the definition's `enabled` key to false
        /// (conflicts with --enable)
        #[arg(long, conflicts_with = "enable")]
        disable: bool,
        /// Rewrite `config.profile.scheduleMode` (e.g. OnDemand,
        /// Scheduled — the scheduleDetails string itself is out of
        /// scope; the read-back shows the landed pairing)
        #[arg(long, value_name = "MODE")]
        schedule_mode: Option<String>,
        /// Deep-merge a settings entry over the found
        /// `config.settings` (K=V with scalar auto-typing;
        /// repeatable — objects merge recursively, arrays/scalars
        /// replace)
        #[arg(long, value_name = "K=V")]
        setting: Vec<String>,
        /// Rewrite the definition's `description`
        #[arg(long, value_name = "TEXT")]
        description: Option<String>,
    },
    /// Delete a task definition — signature-keyed (the gateway
    /// refuses a stale signature), behind the same --yes guard.
    /// Refused exit 2 without --yes with the blast-radius preview
    /// in the message.
    Delete {
        /// Task definition to delete
        name: String,
    },
}

/// Schedule modes (the openapi taxonomy's user-facing subset —
/// SuspendedByFailover is system-owned, not a CLI value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ScheduleMode {
    /// Never fires on its own (force dispatches it)
    OnDemand,
    /// Fires immediately on create (requires --yes)
    Immediate,
    /// Fires on a schedule (requires --yes)
    Scheduled,
    /// Fires at an absolute time (requires --yes)
    AtTime,
    /// Fires after a delay (requires --yes)
    AtDelay,
}

impl ScheduleMode {
    /// The wire token.
    pub fn wire(self) -> &'static str {
        match self {
            Self::OnDemand => "OnDemand",
            Self::Immediate => "Immediate",
            Self::Scheduled => "Scheduled",
            Self::AtTime => "AtTime",
            Self::AtDelay => "AtDelay",
        }
    }
}

/// Script subcommands (07-03, SCRPT-01) — the smallest family: ONE
/// verb over the already-shipped, already-secured scriptExec route.
/// `script` requires a subcommand (the `rig trial` shape — no bare
/// row); the opt-in is STRUCTURAL (the route deploys only via
/// `ign webdev deploy --with-script-exec`, which persists the
/// secret at 0600), so `run` carries NO `--yes` guard by design —
/// the deploy flag IS the opt-in and agents need the verb
/// non-interactive (the research-adopted decision).
#[derive(Debug, clap::Args)]
pub struct ScriptArgs {
    #[command(subcommand)]
    pub command: ScriptCommand,
}

/// Testing subcommands (QUICK-p0g) — the standalone wrapper over the
/// gateway testing bundle's `testing/run` route. `testing` requires a
/// subcommand (the `script` shape — no bare row); there is no `--yes`
/// guard: running a test suite is a read-shaped operation from the
/// CLI's side and agents need the verb non-interactive.
#[derive(Debug, clap::Args)]
pub struct TestingArgs {
    #[command(subcommand)]
    pub command: TestingCommand,
}

/// `ign e2e` args (QUICK-tg4) — the browser-E2E pair.
#[derive(Debug, clap::Args)]
pub struct E2eArgs {
    #[command(subcommand)]
    pub command: E2eCmd,
}

#[derive(Debug, Subcommand)]
pub enum E2eCmd {
    /// Diagnose the browser-E2E setup: node (≥20), npm,
    /// @playwright/test in the scaffold, a downloaded chromium, the
    /// gateway's testing bundle, and the gateway's trial state.
    /// Read-only and offline-safe — **exits 0 whenever the diagnosis
    /// completes**, so every finding is a `checks[]` row rather than
    /// an exit code. The two gateway rows report `skip` when no
    /// profile resolves or no --project is given
    Doctor {
        /// The scaffold directory to inspect (default: ./e2e)
        #[arg(value_name = "DIR")]
        dir: Option<PathBuf>,
        /// The gateway project whose testing bundle is probed. No
        /// default — without it the bundle row honestly skips
        #[arg(long, value_name = "NAME")]
        project: Option<String>,
    },
}

/// `ign session` args (QUICK-tg4) — the SINGULAR verb family: one
/// gateway login session for this CLI to hand to a browser harness.
///
/// The plural `ign sessions` family (`SessionsArgs` / `SessionsCommand`)
/// is a different thing entirely: it LISTS the gateway's connected
/// designer/Perspective/Vision sessions. The names are deliberately
/// kept apart here so a future reader never merges them.
#[derive(Debug, clap::Args)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub command: SessionCmd,
}

#[derive(Debug, Subcommand)]
pub enum SessionCmd {
    /// Log in to the gateway and return the live session: the
    /// `webui-sid-*` cookie name and value, the CSRF token for
    /// `X-CSRF-Token`, the gateway URL, and `storage_state` — a
    /// Playwright `storageState` document ready to write straight to
    /// disk and point `use.storageState` at. The password is env-only
    /// (`IGNITION_PASSWORD`; missing exits 3, rejected exits 5). The
    /// cookie value and CSRF token are exposed ONLY under the global
    /// --json flag
    Login {
        /// Gateway login user (default: $IGNITION_USER, else admin)
        #[arg(long, value_name = "NAME")]
        user: Option<String>,
    },
}

/// The CLI-side `--format` enum (QUICK-p0g) — the clap `ValueEnum`
/// mirror of core's plain `TestingFormat`, the `TransferFormat`
/// precedent (core keeps framework-free enums; the `From` impl lives
/// beside this one). `get_possible_values()` is what feeds the MCP
/// tool schema a real enum instead of a free string.
///
/// EVERY value produces the SAME verdict and exit code: the route is
/// always asked for json and the junit/text reports are rendered
/// client-side, because the route's own junit/text answers stay HTTP
/// 200 and carry no counts — a passthrough would exit 0 on a red
/// suite.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum TestingFormatArg {
    /// Structured results only (default) — `data.report` stays empty
    #[default]
    Json,
    /// JUnit XML in `data.report`, for a CI test reporter
    Junit,
    /// Console-style text in `data.report`, for a human reading logs
    Text,
}

impl From<TestingFormatArg> for ignition_core::actions::testing::TestingFormat {
    fn from(value: TestingFormatArg) -> Self {
        match value {
            TestingFormatArg::Json => Self::Json,
            TestingFormatArg::Junit => Self::Junit,
            TestingFormatArg::Text => Self::Text,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum TestingCommand {
    /// Run the gateway-side Jython test suite and return the machine
    /// verdict (exit 0 green, exit 6 red with the FULL results still
    /// in the envelope)
    Run {
        /// The project whose testing bundle runs. REQUIRED — the
        /// bundle deploys per project and running a suite executes
        /// that project's gateway-side code, so there is
        /// deliberately no `ign-cli` default here
        #[arg(long, value_name = "NAME", required = true)]
        project: String,
        /// List the gateway's discovered test modules instead of
        /// running them
        #[arg(long, conflicts_with_all = ["module", "package"])]
        discover: bool,
        /// Run ONE dotted module (`proj.foo_test`); mutually
        /// exclusive with --package and --discover
        #[arg(long, value_name = "DOTTED", conflicts_with = "package")]
        module: Option<String>,
        /// Run every module under a package prefix (`proj.`);
        /// mutually exclusive with --module and --discover
        #[arg(long, value_name = "PREFIX")]
        package: Option<String>,
        /// Report rendered into `data.report`. The verdict and exit
        /// code are IDENTICAL in every format — the structured
        /// results always ride under `data.results`
        #[arg(long, value_name = "FORMAT", default_value = "json")]
        format: TestingFormatArg,
    },
}

/// `ign lint` args (07-04, INTR-02) — the ignition-lint delegation:
/// PATHS map to `--target <path>` pairs on the child's arg vector;
/// `--` passthrough args ride verbatim after them (power users:
/// `--profile`, `--checks`, `--fail-on`, …).
#[derive(Debug, clap::Args)]
pub struct LintArgs {
    /// Paths to lint (one --target pair each)
    #[arg(value_name = "PATH", required = true)]
    pub paths: Vec<String>,
    /// Exit with the linter's own exit code (CI mode). Default is
    /// the doctor posture: findings + child_exit_code + the parsed
    /// report ride as data and the command exits 0 whenever the
    /// tool RAN
    #[arg(long)]
    pub strict: bool,
    /// Extra args passed to ignition-lint verbatim (after --)
    #[arg(last = true, value_name = "ARGS")]
    pub passthrough: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub enum ScriptCommand {
    /// Execute gateway-side Python (Jython) — non-interactive, the
    /// route's entire purpose
    Run {
        /// Inline Python source (a one-liner's best form)
        #[arg(long, value_name = "PY")]
        code: Option<String>,
        /// Python source file (`-` reads stdin — the agent pipe
        /// path); giving both --code and --file refuses
        /// `invalid_input` (exit 2) pre-resolution
        #[arg(long, value_name = "PATH")]
        file: Option<String>,
        /// The deployed routes' project (default ign-cli — where
        /// `ign webdev deploy` put scriptExec)
        #[arg(long, default_value = "ign-cli", value_name = "NAME")]
        project: String,
    },
}

/// `ign api` args (09-03, EXT-01) — the raw-passthrough family. One
/// verb today (`call`); the escape hatch for the 80+ gateway endpoint
/// families no curated command covers.
#[derive(Debug, clap::Args)]
pub struct ApiArgs {
    #[command(subcommand)]
    pub command: ApiCommand,
}

#[derive(Debug, Subcommand)]
pub enum ApiCommand {
    /// Call any gateway REST endpoint raw: `--method`/`--path` are the
    /// request, `--data`/`--header`/`--query` shape it, and the
    /// envelope's `data.result.data` is the gateway's JSON VERBATIM
    /// (no field dropped, no value coerced, key order preserved — the
    /// README's documented contract exception). Auth-pattern headers
    /// (`Authorization`, `X-Ignition-API-Token`, `Cookie`) are
    /// refused pre-I/O — credentials come from the profile.
    Call(ApiCallArgs),
}

/// `ign api call` args (09-03, EXT-01). Arbitrary verbs accepted —
/// reqwest's `Method` parser validates (usage-class refusal for
/// garbage); `--data` is RAW TEXT on ANY method (GET/DELETE bodies
/// allowed — curl parity; the gateway's answer classifies); ONE query
/// mechanism (repeatable `--query k=v`, never `?` in `--path`).
#[derive(Debug, clap::Args)]
pub struct ApiCallArgs {
    /// HTTP method (GET, POST, PUT, DELETE, PATCH, HEAD, …)
    #[arg(long, value_name = "METHOD")]
    pub method: String,
    /// Absolute path on the gateway (must start with `/`; no `?`, no host)
    #[arg(long, value_name = "PATH")]
    pub path: String,
    /// Raw body text passthrough (rides ANY method — curl parity)
    #[arg(long, value_name = "TEXT")]
    pub data: Option<String>,
    /// Extra request header `Name: Value` (repeatable; split on the
    /// FIRST `:`; auth-pattern names are refused pre-I/O)
    #[arg(long, value_name = "NAME: VALUE")]
    pub header: Vec<String>,
    /// Query pair `k=v` (repeatable; split on the FIRST `=` — the
    /// ONE query mechanism)
    #[arg(long, value_name = "K=V")]
    pub query: Vec<String>,
}

/// `ign license` args (09-04, EXT-02) — the license morning-check
/// family. One verb today; room to grow (the uniform-leaf shape).
#[derive(Debug, clap::Args)]
pub struct LicenseArgs {
    #[command(subcommand)]
    pub command: LicenseCommand,
}

#[derive(Debug, Subcommand)]
pub enum LicenseCommand {
    /// License inventory + trial state in ONE command: the
    /// hardware-key item rows and effective stamp ride the
    /// `/licenses` read, the license mode + countdown ride the trial
    /// companion (the mode is NOT on the licenses payload — capture
    /// fact)
    Status,
}

/// `ign redundancy` args (09-04, EXT-02) — the redundancy
/// morning-check family.
#[derive(Debug, clap::Args)]
pub struct RedundancyArgs {
    #[command(subcommand)]
    pub command: RedundancyCommand,
}

#[derive(Debug, Subcommand)]
pub enum RedundancyCommand {
    /// The flat redundancy status: role, project state, peer
    /// connection, config access, sync/failover pending, uptime
    /// (ms since gateway start — capture-proven) and last-sync (the
    /// `-1` never-synced sentinel on fresh rigs)
    Status,
}

/// `ign gan` args (09-04, EXT-02) — the Gateway Area Network
/// morning-check family.
#[derive(Debug, clap::Args)]
pub struct GanArgs {
    #[command(subcommand)]
    pub command: GanCommand,
}

#[derive(Debug, Subcommand)]
pub enum GanCommand {
    /// The GAN overview: total/running connections, in/out byte
    /// rates, remote gateways — all zeros are healthy data on a
    /// non-GAN gateway (the capture IS the canonical shape)
    Status,
}

/// `ign diagnostics` args (09-05, EXT-02) — the support-bundle
/// family: `bundle` nests one level (the `tags provider` grouped-
/// subfamily pattern).
#[derive(Debug, clap::Args)]
pub struct DiagnosticsArgs {
    #[command(subcommand)]
    pub command: DiagnosticsCommand,
}

#[derive(Debug, Subcommand)]
pub enum DiagnosticsCommand {
    /// Support-bundle operations (generate / status / download /
    /// wait) — the state machine is ENCODED FROM LIVE CAPTURES:
    /// observed states are exactly `Generating` (mid-generation) and
    /// `Valid` (terminal/ready), PascalCase, both rigs
    #[command(subcommand)]
    Bundle(BundleCommand),
}

/// The four bundle verbs (09-05). Nothing here is destructive:
/// generate creates a support bundle, download writes a local file,
/// wait polls — none carry `--yes` (the backup-download posture).
#[derive(Debug, Subcommand)]
pub enum BundleCommand {
    /// Start bundle generation (POST, no body) — the 200 answer IS
    /// the fresh status (live capture: `{"state":"Generating"}`);
    /// generation takes ~2–6 s on a fresh rig, so pair with `wait`
    Generate,
    /// The bundle status: `state` rides the CAPTURED vocabulary
    /// (`Generating` / `Valid` — unknown future states pass through
    /// verbatim, never refused); `fileSize` (bytes) appears only
    /// when `Valid`
    Status,
    /// Download the bundle ZIP (streamed to disk; the request rides
    /// a 300 s per-request timeout — the 30 s client default would
    /// truncate MB-sized bundles)
    Download {
        /// Output file (default: the gateway's Content-Disposition
        /// name, else ignition-diagnostics-bundle-<unix_ts>.zip)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Poll the status until a terminal captured state (`Valid`)
    /// — an UNKNOWN state (outside the captured vocabulary) keeps
    /// polling honestly until the deadline; deadline expiry is
    /// exit 4 `network_error` (the restart-wait convention, no new
    /// slug) with the last observed state in the message
    Wait {
        /// Poll interval in seconds
        #[arg(long, default_value_t = 2, value_name = "SECS")]
        interval: u64,
        /// Give up after this many seconds (default 300 — mirrors
        /// restart --wait)
        #[arg(long, default_value_t = 300, value_name = "SECS")]
        timeout: u64,
    },
}

/// Profile subcommands (nested: a struct wrapper carrying the subcommand
/// enum, so `Commands::Profile` gets an `Args` payload).
#[derive(Debug, clap::Args)]
pub struct ProfileArgs {
    #[command(subcommand)]
    pub command: ProfileCmd,
}

/// Sessions args: the family filter rides the TOP level so bare
/// `ign sessions [--type X]` lists (must-have truth #1) while
/// `ign sessions terminate …` carries the destructive half.
#[derive(Debug, clap::Args)]
pub struct SessionsArgs {
    /// Filter to one session family (default: all three merged)
    #[arg(long, value_enum)]
    pub r#type: Option<SessionType>,

    #[command(subcommand)]
    pub command: Option<SessionsCmd>,
}

#[derive(Debug, Subcommand)]
pub enum SessionsCmd {
    /// Terminate (designer: prune / vision: close) a session —
    /// destructive, refused without --yes
    Terminate {
        /// Session family holding the id
        #[arg(long, value_enum)]
        r#type: SessionType,
        /// Session/client id to terminate (see `ign sessions`)
        #[arg(long)]
        id: String,
        /// Message shown to the session's user (Perspective only)
        #[arg(long)]
        message: Option<String>,
    },
}

/// Logs args: the query filters ride the TOP level so bare
/// `ign logs [-f]` lists/tails (must-have truth #1) while `download`
/// and the `loggers` subtree hang off the optional subcommand — the
/// SessionsArgs precedent.
#[derive(Debug, clap::Args)]
pub struct LogsArgs {
    /// Only entries from this logger (name prefix)
    #[arg(long, value_name = "NAME")]
    pub logger: Option<String>,
    /// Minimum level to include (server-side filter)
    #[arg(long, value_enum, value_name = "LEVEL")]
    pub min_level: Option<LogLevel>,
    /// Start from an absolute EPOCH-MS or a relative span (500ms, 30s,
    /// 5min, 2h) — parsed to epoch-ms at arg-parse time
    #[arg(
        long,
        value_name = "EPOCH_MS|Nms|Ns|Nmin|Nh",
        value_parser = parse_since_arg
    )]
    pub since: Option<i64>,
    /// Max entries — the server default is UNLIMITED, never used here
    #[arg(long, default_value_t = 200)]
    pub limit: i64,
    /// Follow: stream new entries as they occur (poll-based)
    #[arg(short = 'f', long)]
    pub follow: bool,
    /// Poll interval in seconds (follow mode)
    #[arg(long, default_value_t = 2, value_name = "SECS")]
    pub interval: u64,
    /// Stop after this many seconds (follow mode; default: until Ctrl-C)
    #[arg(long, value_name = "SECS")]
    pub timeout: Option<u64>,
    #[command(subcommand)]
    pub command: Option<LogsCmd>,
}

#[derive(Debug, Subcommand)]
pub enum LogsCmd {
    /// Download the log archive — a SQLite .idb, never a zip
    Download {
        /// Output file (default: the gateway's Content-Disposition
        /// name, else <profile>-logs-<ts>.idb)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// List loggers / manage logger levels
    Loggers(LoggersArgs),
}

#[derive(Debug, clap::Args)]
pub struct LoggersArgs {
    /// Substring search over logger names
    #[arg(long, value_name = "TEXT")]
    pub search: Option<String>,
    #[command(subcommand)]
    pub command: Option<LoggersCmd>,
}

#[derive(Debug, Subcommand)]
pub enum LoggersCmd {
    /// Set one logger's level — a mutation, refused without --yes
    Set {
        /// Logger name (see `ign logs loggers`)
        name: String,
        /// Level to set
        #[arg(value_enum)]
        level: LogLevel,
    },
    /// Reset ALL logger levels to defaults — refused without --yes
    Reset,
}

/// The seven spec-documented log levels (TRACE..OFF), value-enum form
/// for clap; [`LogLevel::wire`] yields the uppercase wire token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
    Off,
}

impl LogLevel {
    /// The uppercase wire token the gateway expects.
    pub fn wire(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Fatal => "FATAL",
            Self::Off => "OFF",
        }
    }
}

/// clap value parser delegating to the core `--since` grammar against
/// the current time (a relative span resolves at parse time) — invalid
/// specs are clap usage errors (exit 2) like any bad flag value.
fn parse_since_arg(spec: &str) -> Result<i64, String> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock is after the unix epoch")
        .as_millis() as i64;
    ignition_core::actions::logs::parse_since(spec, now_ms)
}

/// CLI value-enum mirrors of the core action enums (ignition-core stays
/// clap-free; `From` converts at the dispatch seam).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum SessionType {
    /// Designer sessions
    Designer,
    /// Perspective browser sessions
    Perspective,
    /// Vision clients
    Vision,
}

impl From<SessionType> for ignition_core::actions::sessions::SessionType {
    fn from(value: SessionType) -> Self {
        match value {
            SessionType::Designer => Self::Designer,
            SessionType::Perspective => Self::Perspective,
            SessionType::Vision => Self::Vision,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ConnectionType {
    /// Database connections
    Database,
    /// OPC connections
    Opc,
}

impl From<ConnectionType> for ignition_core::actions::connections::ConnectionType {
    fn from(value: ConnectionType) -> Self {
        match value {
            ConnectionType::Database => Self::Database,
            ConnectionType::Opc => Self::Opc,
        }
    }
}

/// Import collision policy (03-02). Exactly the two values REST
/// exposes — `merge` is the Designer import popup's vocabulary and is
/// NOT a value: clap's invalid-value error lists the two real
/// choices, and the README documents merge as Designer-only (the
/// sanctioned rejection-with-hint shape).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CollisionPolicy {
    /// Refuse when the project already exists (default)
    Abort,
    /// Replace the ENTIRE project (resources absent from the ZIP are
    /// deleted) — destructive, requires --yes
    Overwrite,
}

impl From<CollisionPolicy> for ignition_core::actions::projects::CollisionPolicy {
    fn from(value: CollisionPolicy) -> Self {
        match value {
            CollisionPolicy::Abort => Self::Abort,
            CollisionPolicy::Overwrite => Self::Overwrite,
        }
    }
}

#[derive(Debug, Subcommand)]
#[command(arg_required_else_help = true)]
pub enum ProfileCmd {
    /// Add (or overwrite) a gateway profile
    Add {
        /// Profile name
        name: String,
        /// Gateway base URL (e.g. http://localhost:9088)
        url: String,
        /// Optional display label
        #[arg(long, value_name = "TEXT")]
        label: Option<String>,
        /// Name of the env var holding the auth token
        #[arg(long, value_name = "VAR")]
        token_env: Option<String>,
        /// Keyring user string for the token (service is always ignition-cli)
        #[arg(long, value_name = "USER")]
        keyring: Option<String>,
        /// Name of the env var holding the basic-auth user (with --password-env)
        #[arg(long, value_name = "VAR")]
        user_env: Option<String>,
        /// Name of the env var holding the basic-auth password (with --user-env)
        #[arg(long, value_name = "VAR")]
        password_env: Option<String>,
        /// Make this profile the active one
        #[arg(long)]
        active: bool,
    },
    /// List configured profiles
    List,
    /// Switch the active profile
    Use {
        /// Profile name to activate
        name: String,
    },
}
