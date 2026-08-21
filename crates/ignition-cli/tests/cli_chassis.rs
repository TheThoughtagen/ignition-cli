//! Binary-level chassis tests: --help flags, --version, fresh-install
//! `version`, env application, usage-error exit 2, stdout/stderr separation.
//!
//! Every test sets `IGNITION_CLI_CONFIG` to an isolated path (research
//! Pitfall 3: `directories` ignores XDG on macOS, so tests must never touch
//! the real config dir). The path deliberately does not exist — chassis tests
//! must pass on a fresh install with no config file.

use assert_cmd::Command;
use predicates::str::contains;

fn ign() -> Command {
    let mut cmd = Command::cargo_bin("ign").expect("binary 'ign' not found");
    let isolated = std::env::temp_dir().join(format!("ign-chassis-{}.toml", std::process::id()));
    cmd.env("IGNITION_CLI_CONFIG", isolated);
    cmd
}

#[test]
fn help_lists_all_five_global_flags() {
    ign()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("--profile"))
        .stdout(contains("--json"))
        .stdout(contains("--compact"))
        .stdout(contains("--yes"))
        .stdout(contains("--verbose"));
}

#[test]
fn version_flag_exits_zero() {
    ign()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn version_subcommand_works_without_config() {
    // Fresh-install truth: no config file exists at all and the command still
    // succeeds and prints the CLI version.
    ign()
        .arg("version")
        .assert()
        .success()
        .stdout(contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn env_json_flag_is_accepted() {
    ign()
        .env("IGNITION_JSON", "1")
        .arg("version")
        .assert()
        .success();
}

#[test]
fn env_yes_flag_is_accepted() {
    ign()
        .env("IGNITION_YES", "1")
        .arg("version")
        .assert()
        .success();
}

#[test]
fn unknown_flag_exits_two() {
    // clap renders the usage error itself (try_parse → e.exit()); proves the
    // wiring, not an ign-authored error path.
    ign().arg("--bogus").assert().failure().code(2);
}

#[test]
fn verbose_keeps_stdout_version_line_only() {
    // Diagnostics belong on stderr; stdout must still parse as exactly the
    // version line.
    ign()
        .arg("version")
        .arg("-v")
        .assert()
        .success()
        .stdout(format!(
            "ign {} (ignition-cli)\n",
            env!("CARGO_PKG_VERSION")
        ));
}

// ---------------------------------------------------------------------------
// Completions (CORE-07)
// ---------------------------------------------------------------------------

/// All three shells generate; each output carries its shell-appropriate
/// marker (bash: `_ign` function prefix, zsh: `#compdef ign`, fish:
/// `complete -c ign`).
#[test]
fn completions_bash_generate() {
    ign()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(contains("_ign"));
}

#[test]
fn completions_zsh_generate() {
    ign()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(contains("#compdef ign"));
}

#[test]
fn completions_fish_generate() {
    ign()
        .args(["completions", "fish"])
        .assert()
        .success()
        .stdout(contains("complete -c ign"));
}

/// Bare `completions` (no shell) is a clap usage error → exit 2
/// (`arg_required_else_help`).
#[test]
fn completions_without_shell_exit_2() {
    ign()
        .args(["completions"])
        .assert()
        .failure()
        .code(2)
        .stderr(contains("SHELL"));
}

/// The ONE sanctioned stdout exception: the script prints RAW even under
/// `--json` (shells source stdout — never an envelope).
#[test]
fn completions_ignore_json_flag() {
    ign()
        .args(["completions", "bash", "--json"])
        .assert()
        .success()
        .stdout(contains("_ign"))
        .stdout(contains("complete -F _ign"));
}
