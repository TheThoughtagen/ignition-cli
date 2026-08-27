//! Shell completions — `clap_complete::aot` runtime generation (CORE-07).
//!
//! Completions print to stdout **regardless of `--json`** — the ONE
//! sanctioned success-path exception (research Pattern 7): shells source
//! the script from stdout, so it must never be JSON-wrapped. See the
//! contract note in `render.rs` and the README.
//!
//! Import from `aot` — the old `shells`/`generator` paths are deprecated;
//! the `unstable-dynamic` engine (`COMPLETE=$SHELL`) stays out (flag-gated
//! upstream). Generated from the LIVE clap definition, so the script can
//! never drift from the actual flags.

use clap::CommandFactory;
use clap_complete::aot::{Shell, generate};

use ignition_cli::cli::Cli;

/// Generate the completion script for `shell` into a String (rendered to
/// stdout by `render_ok` — the sanctioned exception).
pub fn completions(shell: Shell) -> String {
    let mut command = Cli::command();
    // `generate` writes to an `io::Write`; `String` only implements
    // `fmt::Write`, so buffer as bytes and convert.
    let mut buffer = Vec::new();
    generate(shell, &mut command, "ign", &mut buffer);
    String::from_utf8(buffer).expect("completion scripts are UTF-8")
}
