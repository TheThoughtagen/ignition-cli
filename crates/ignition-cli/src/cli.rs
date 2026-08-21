//! CLI definition: the five global args (defined exactly once, here) and the
//! subcommand enum.
//!
//! Placement rules (Phase 1 research, Pattern 1):
//! - Globals set the `global` arg attribute so they propagate to every
//!   subcommand; subcommand structs never redeclare them.
//! - Never mark a global arg `required` (clap rejects required globals).
//! - Flags-only, zero interactive prompts — ever (research anti-pattern:
//!   "Prompting ever").

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

    /// Manage gateway profiles
    #[command(arg_required_else_help = true)]
    Profile(ProfileArgs),

    /// Interactive TUI cockpit
    #[cfg(feature = "tui")]
    Tui,
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
