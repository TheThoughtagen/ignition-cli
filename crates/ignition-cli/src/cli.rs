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

    /// Manage gateway projects: list with inheritance info, new, copy,
    /// rename, set (reparent), delete, export/import (ZIP)
    #[command(arg_required_else_help = true)]
    Project(ProjectArgs),

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

    /// Manage gateway profiles
    #[command(arg_required_else_help = true)]
    Profile(ProfileArgs),

    /// Interactive TUI cockpit
    #[cfg(feature = "tui")]
    Tui,
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
        /// name, else <name>.zip)
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Import a project from a ZIP archive
    Import {
        /// Project name to import as
        name: String,
        /// ZIP file path, or - to read the archive from stdin
        #[arg(long, value_name = "PATH")]
        file: String,
        /// Collision policy: abort refuses when the name exists
        /// (default); overwrite REPLACES the entire project —
        /// destructive, requires --yes. merge is Designer-only (not a
        /// value; the README documents why)
        #[arg(long, value_enum, default_value_t = CollisionPolicy::Abort)]
        collision_policy: CollisionPolicy,
    },
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
    /// routes (`ign webdev deploy`)
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
