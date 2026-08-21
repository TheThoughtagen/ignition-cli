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
