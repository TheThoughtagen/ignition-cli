//! `targets/<env>.yaml`: where a rendered tree is deployed.
//!
//! A node reaches its host through `ssh` (SF-10) or `local` (D-02), never
//! both; a node with neither can still be read through the config API.
//!
//! The `ssh` block (SF-10) is parsed now so strict parsing does not reject
//! it when Phase 22 starts reading it. Credentials never live here:
//! `profile` names an `ign` profile (its secret chain holds the token) and
//! `key_env` names an environment variable, never a key.

use serde::Deserialize;

use super::component_problem;
use super::yamlio::{SchemaVersion, strict_parse};
use crate::error::{CoreError, DeployFinding};

/// The parsed `targets/<env>.yaml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// Always 1; any other value is refused by [`SchemaVersion`].
    pub schema_version: SchemaVersion,
    /// Gateway directory name under `gateways/` (render, Phase 21). A single
    /// safe path component.
    pub gateway: String,
    /// Deployment mode directory name under `modes/` (render, Phase 21). A
    /// single safe path component.
    pub mode: String,
    /// The nodes this environment deploys to.
    pub nodes: Nodes,
}

/// The nodes of one environment.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Nodes {
    /// The master (or only) gateway node.
    pub master: NodeTarget,
    /// The redundant backup node, when the environment has one.
    #[serde(default)]
    pub backup: Option<NodeTarget>,
}

/// One gateway node (consumed by Phases 22 and 25).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeTarget {
    /// An `ign` profile NAME. The credential stays in the profile's secret
    /// chain.
    pub profile: String,
    /// SSH access for host-level steps (SF-10, decided in Phase 22).
    #[serde(default)]
    pub ssh: Option<Ssh>,
    /// The gateway data directory on the runner's own filesystem (D-02).
    /// Mutually exclusive with `ssh`; selects the local-filesystem transport.
    #[serde(default)]
    pub local: Option<Local>,
}

/// Local-filesystem access to a node's gateway data directory (D-02).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Local {
    /// The gateway data directory on the runner's own filesystem; a relative
    /// value resolves against `--repo`.
    pub data_dir: String,
}

/// SSH access to a node's host (SF-10).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ssh {
    /// Host name or address.
    pub host: String,
    /// TCP port; the SSH default applies when absent.
    #[serde(default)]
    pub port: Option<u16>,
    /// Login user.
    pub user: String,
    /// Path to a private key file. Mutually exclusive with `key_env`.
    #[serde(default)]
    pub key_file: Option<String>,
    /// Name of an environment variable holding a key. A NAME, never a key.
    /// Mutually exclusive with `key_file`; neither means agent auth.
    #[serde(default)]
    pub key_env: Option<String>,
    /// Host operating system.
    pub os: TargetOs,
    /// The gateway's data directory on that host.
    pub data_dir: String,
    /// Repo-relative path of the reviewed `known_hosts` file that pins this
    /// host's key (D-01). Required: `ign` passes a copy of it as
    /// `UserKnownHostsFile` with `StrictHostKeyChecking=yes` and never reads
    /// the runner's own `~/.ssh/known_hosts`.
    pub known_hosts: String,
    /// Seconds ssh may take to connect (`ConnectTimeout`); 1..=120, default
    /// [`DEFAULT_CONNECT_TIMEOUT_S`] (D-04).
    #[serde(default)]
    pub connect_timeout_s: Option<u32>,
    /// Seconds one ssh or sftp invocation may run before it is killed;
    /// 1..=3600, default [`DEFAULT_OP_TIMEOUT_S`] (D-04).
    #[serde(default)]
    pub op_timeout_s: Option<u32>,
}

/// Default `ssh.connect_timeout_s`. Kept at 10 by OQ5-4: the largest of 60
/// measured sftp sessions (runs A and B, 22-LIVE-CAPTURES.md) took 3.0 s.
pub const DEFAULT_CONNECT_TIMEOUT_S: u32 = 10;
/// Default `ssh.op_timeout_s`: 1050 s. OQ5-4 (22-LIVE-CAPTURES.md): at least
/// three times the larger of the read p95 and the commit p95 over both OQ5
/// runs (348.2 s, a 152-file commit), rounded up to 30 s. It was 120 s, which
/// a measured 152-file commit (234 to 348 s) and read (155 to 221 s) both
/// exceeded.
pub const DEFAULT_OP_TIMEOUT_S: u32 = 1050;
/// Accepted `ssh.connect_timeout_s` range.
pub const CONNECT_TIMEOUT_RANGE_S: std::ops::RangeInclusive<u32> = 1..=120;
/// Accepted `ssh.op_timeout_s` range.
pub const OP_TIMEOUT_RANGE_S: std::ops::RangeInclusive<u32> = 1..=3600;

/// Host operating system of an SSH node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetOs {
    /// A Windows host.
    Windows,
    /// A Linux host.
    Linux,
}

impl Target {
    /// Parse and validate `bytes` (labelled `file` in errors).
    pub fn parse(file: &str, bytes: &[u8]) -> Result<Target, CoreError> {
        let target: Target = strict_parse(file, bytes)?;
        target.validate(file)?;
        Ok(target)
    }

    fn validate(&self, file: &str) -> Result<(), CoreError> {
        let mut findings = Vec::new();
        let mut push = |message: String| {
            findings.push(DeployFinding {
                file: file.to_string(),
                line: None,
                column: None,
                message,
            });
        };

        for (field, value) in [("gateway", &self.gateway), ("mode", &self.mode)] {
            if let Some(why) = component_problem(value) {
                push(format!("{field} {value:?}: {why}"));
            }
        }
        let backup = self.nodes.backup.as_ref().map(|n| ("nodes.backup", n));
        for (label, node) in std::iter::once(("nodes.master", &self.nodes.master)).chain(backup) {
            if node.profile.is_empty() {
                push(format!("{label}.profile: must not be empty"));
            }
            if node.ssh.is_some() && node.local.is_some() {
                push(format!("{label}: set ssh or local, not both"));
            }
            if let Some(local) = &node.local
                && local.data_dir.is_empty()
            {
                push(format!("{label}.local.data_dir: must not be empty"));
            }
            let Some(ssh) = &node.ssh else { continue };
            for (field, value) in [
                ("host", &ssh.host),
                ("user", &ssh.user),
                ("data_dir", &ssh.data_dir),
            ] {
                if value.is_empty() {
                    push(format!("{label}.ssh.{field}: must not be empty"));
                }
            }
            if ssh.key_file.is_some() && ssh.key_env.is_some() {
                push(format!(
                    "{label}.ssh: set key_file or key_env, not both (neither means agent auth)"
                ));
            }
            // host and user become ssh argv: a leading `-` would be read as an
            // option (`-oProxyCommand=...`), whitespace and control characters
            // split or smuggle arguments, and `@` in user or host moves the
            // user@host boundary (T-22-17).
            for (field, value) in [("host", &ssh.host), ("user", &ssh.user)] {
                if value.is_empty() {
                    continue;
                }
                if value.starts_with('-') {
                    push(format!(
                        "{label}.ssh.{field}: must not start with `-` (it would be read as an ssh option)"
                    ));
                }
                if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
                    push(format!(
                        "{label}.ssh.{field}: must not contain whitespace or control characters"
                    ));
                }
                if value.contains('@') {
                    push(format!("{label}.ssh.{field}: must not contain `@`"));
                }
            }
            if let Some(problem) = known_hosts_problem(&ssh.known_hosts) {
                push(format!("{label}.ssh.known_hosts: {problem}"));
            }
            if let Some(n) = ssh.connect_timeout_s
                && !CONNECT_TIMEOUT_RANGE_S.contains(&n)
            {
                push(format!(
                    "{label}.ssh.connect_timeout_s: {n} is outside {}..={} seconds",
                    CONNECT_TIMEOUT_RANGE_S.start(),
                    CONNECT_TIMEOUT_RANGE_S.end()
                ));
            }
            if let Some(n) = ssh.op_timeout_s
                && !OP_TIMEOUT_RANGE_S.contains(&n)
            {
                push(format!(
                    "{label}.ssh.op_timeout_s: {n} is outside {}..={} seconds",
                    OP_TIMEOUT_RANGE_S.start(),
                    OP_TIMEOUT_RANGE_S.end()
                ));
            }
        }

        if findings.is_empty() {
            Ok(())
        } else {
            Err(CoreError::deploy_findings(findings))
        }
    }
}

/// Why `path` is not a usable repo-relative `known_hosts` path, or `None`.
///
/// It must be relative (no leading `/`, no drive prefix, no backslash) and
/// every `/` component must pass [`component_problem`], so `..`, an empty
/// component and a dot-leading name are all refused.
fn known_hosts_problem(path: &str) -> Option<String> {
    if path.is_empty() {
        return Some("must not be empty".to_string());
    }
    if path.starts_with('/') {
        return Some("must be relative to the repo root, not absolute".to_string());
    }
    if path.contains('\\') {
        return Some("must use `/` separators, not `\\`".to_string());
    }
    if path.as_bytes().get(1) == Some(&b':') {
        return Some("must be relative to the repo root, not a drive path".to_string());
    }
    for part in path.split('/') {
        if let Some(why) = component_problem(part) {
            return Some(format!("component {part:?} {why}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn findings(e: CoreError) -> Vec<DeployFinding> {
        match e {
            CoreError::DeployTreeInvalid(d) => d.findings,
            other => panic!("expected deploy_tree_invalid, got {other}"),
        }
    }

    #[test]
    fn full_target_parses() {
        let yaml = "schema_version: 1\ngateway: plant-a\nmode: prod\nnodes:\n  master:\n    profile: plant-a-master\n    ssh:\n      host: 10.0.0.5\n      port: 2222\n      user: svc\n      key_file: /keys/id_ed25519\n      os: windows\n      data_dir: 'C:\\Program Files\\Inductive Automation\\Ignition\\data'\n      known_hosts: ssh/plant-a.known_hosts\n  backup:\n    profile: plant-a-backup\n    ssh:\n      host: 10.0.0.6\n      user: svc\n      key_env: PLANT_A_KEY\n      os: linux\n      data_dir: /usr/local/bin/ignition/data\n      known_hosts: ssh/plant-a.known_hosts\n";
        let t = Target::parse("targets/prod.yaml", yaml.as_bytes()).expect("parses");
        assert_eq!((t.gateway.as_str(), t.mode.as_str()), ("plant-a", "prod"));
        let master = t.nodes.master.ssh.expect("master ssh");
        assert_eq!(master.port, Some(2222));
        assert_eq!(master.os, TargetOs::Windows);
        assert_eq!(master.key_file.as_deref(), Some("/keys/id_ed25519"));
        assert!(master.data_dir.starts_with("C:\\"));
        let backup = t.nodes.backup.expect("backup");
        assert_eq!(backup.profile, "plant-a-backup");
        let bssh = backup.ssh.expect("backup ssh");
        assert_eq!((bssh.os, bssh.port), (TargetOs::Linux, None));
        assert_eq!(bssh.key_env.as_deref(), Some("PLANT_A_KEY"));
    }

    #[test]
    fn minimal_target_parses() {
        let t = Target::parse(
            "targets/dev.yaml",
            b"schema_version: 1\ngateway: g\nmode: dev\nnodes:\n  master:\n    profile: dev\n",
        )
        .expect("parses");
        assert_eq!(t.nodes.backup, None);
        assert_eq!(t.nodes.master.ssh, None);
        assert_eq!(t.nodes.master.local, None);
    }

    #[test]
    fn local_node_parses() {
        let yaml = "schema_version: 1\ngateway: g\nmode: dev\nnodes:\n  master:\n    profile: dev\n    local:\n      data_dir: /var/ignition/data\n";
        let t = Target::parse("targets/dev.yaml", yaml.as_bytes()).expect("parses");
        let local = t.nodes.master.local.expect("local");
        assert_eq!(local.data_dir, "/var/ignition/data");
    }

    #[test]
    fn ssh_and_local_together_are_refused() {
        let yaml = "schema_version: 1\ngateway: g\nmode: m\nnodes:\n  master:\n    profile: p\n    local:\n      data_dir: /d\n  backup:\n    profile: q\n    local:\n      data_dir: /d\n    ssh:\n      host: h\n      user: u\n      os: linux\n      data_dir: /d\n      known_hosts: kh\n";
        let all: Vec<String> =
            findings(Target::parse("targets/x.yaml", yaml.as_bytes()).expect_err("both"))
                .into_iter()
                .map(|f| f.message)
                .collect();
        assert_eq!(all, vec!["nodes.backup: set ssh or local, not both"]);
    }

    #[test]
    fn empty_local_data_dir_is_refused() {
        let yaml = "schema_version: 1\ngateway: g\nmode: m\nnodes:\n  master:\n    profile: p\n    local:\n      data_dir: ''\n";
        let all: Vec<String> =
            findings(Target::parse("targets/x.yaml", yaml.as_bytes()).expect_err("empty"))
                .into_iter()
                .map(|f| f.message)
                .collect();
        assert_eq!(all, vec!["nodes.master.local.data_dir: must not be empty"]);
    }

    #[test]
    fn target_unknown_ssh_field_is_positioned() {
        let yaml = "schema_version: 1\ngateway: g\nmode: m\nnodes:\n  master:\n    profile: p\n    ssh:\n      host: h\n      user: u\n      os: linux\n      data_dir: /d\n      bogus: 1\n      known_hosts: kh\n";
        let f = findings(Target::parse("targets/x.yaml", yaml.as_bytes()).expect_err("unknown"))
            .remove(0);
        assert_eq!((f.line, f.column), (Some(12), Some(7)));
        assert!(f.message.contains("bogus"), "{}", f.message);
    }

    #[test]
    fn target_os_rejects_unknown() {
        let yaml = "schema_version: 1\ngateway: g\nmode: m\nnodes:\n  master:\n    profile: p\n    ssh:\n      host: h\n      user: u\n      os: macos\n      data_dir: /d\n      known_hosts: kh\n";
        let f =
            findings(Target::parse("targets/x.yaml", yaml.as_bytes()).expect_err("os")).remove(0);
        assert_eq!(f.line, Some(10));
        assert!(f.column.is_some());
        assert!(f.message.contains("unknown variant"), "{}", f.message);
        assert!(!f.message.contains("macos"), "{}", f.message);
        assert!(
            f.message.contains("expected `windows` or `linux`"),
            "{}",
            f.message
        );
    }

    #[test]
    fn target_semantic_findings() {
        let yaml = "schema_version: 1\ngateway: ../x\nmode: .hidden\nnodes:\n  master:\n    profile: ''\n    ssh:\n      host: h\n      user: u\n      key_file: /k\n      key_env: K\n      os: linux\n      data_dir: ''\n      known_hosts: kh\n";
        let found =
            findings(Target::parse("targets/x.yaml", yaml.as_bytes()).expect_err("invalid"));
        let all: Vec<&str> = found.iter().map(|f| f.message.as_str()).collect();
        let has = |needle: &str| all.iter().any(|m| m.contains(needle));
        assert!(has("gateway") && has("path separator"), "{all:?}");
        assert!(has("mode") && has("start with `.`"), "{all:?}");
        assert!(has("nodes.master.profile"), "{all:?}");
        assert!(has("key_file or key_env"), "{all:?}");
        assert!(has("nodes.master.ssh.data_dir"), "{all:?}");
    }

    fn ssh_yaml(extra: &str) -> String {
        format!(
            "schema_version: 1\ngateway: g\nmode: m\nnodes:\n  master:\n    profile: p\n    ssh:\n      host: h\n      user: u\n      os: linux\n      data_dir: /d\n{extra}"
        )
    }

    fn messages(yaml: &str) -> Vec<String> {
        findings(Target::parse("targets/x.yaml", yaml.as_bytes()).expect_err("refused"))
            .into_iter()
            .map(|f| f.message)
            .collect()
    }

    #[test]
    fn target_ssh_fields() {
        // known_hosts is required (strict parse error, positioned)
        let f = findings(
            Target::parse("targets/x.yaml", ssh_yaml("").as_bytes()).expect_err("missing"),
        )
        .remove(0);
        assert!(f.message.contains("known_hosts"), "{}", f.message);

        // timeouts default when absent and parse when present
        let t = Target::parse(
            "targets/x.yaml",
            ssh_yaml("      known_hosts: ssh/kh\n").as_bytes(),
        )
        .expect("parses");
        let ssh = t.nodes.master.ssh.expect("ssh");
        assert_eq!((ssh.connect_timeout_s, ssh.op_timeout_s), (None, None));
        assert_eq!(DEFAULT_CONNECT_TIMEOUT_S, 10);
        assert_eq!(DEFAULT_OP_TIMEOUT_S, 1050);
        assert!(OP_TIMEOUT_RANGE_S.contains(&DEFAULT_OP_TIMEOUT_S));
        let t = Target::parse(
            "targets/x.yaml",
            ssh_yaml(
                "      known_hosts: ssh/kh\n      connect_timeout_s: 120\n      op_timeout_s: 1\n",
            )
            .as_bytes(),
        )
        .expect("edges parse");
        let ssh = t.nodes.master.ssh.expect("ssh");
        assert_eq!(
            (ssh.connect_timeout_s, ssh.op_timeout_s),
            (Some(120), Some(1))
        );

        // known_hosts shapes
        for bad in [
            "/etc/ssh/kh",
            "C:/kh",
            "a\\\\b",
            "../kh",
            "a/../kh",
            "a//kh",
            ".hidden/kh",
            "dir/.kh",
            "''",
        ] {
            let all = messages(&ssh_yaml(&format!("      known_hosts: {bad}\n")));
            assert!(
                all.iter()
                    .any(|m| m.starts_with("nodes.master.ssh.known_hosts:")),
                "{bad}: {all:?}"
            );
        }

        // timeouts out of range name the field
        for (field, value) in [
            ("connect_timeout_s", 0),
            ("connect_timeout_s", 121),
            ("op_timeout_s", 0),
            ("op_timeout_s", 3601),
        ] {
            let all = messages(&ssh_yaml(&format!(
                "      known_hosts: kh\n      {field}: {value}\n"
            )));
            assert!(
                all.iter()
                    .any(|m| m.starts_with(&format!("nodes.master.ssh.{field}:"))),
                "{field}={value}: {all:?}"
            );
        }
    }

    #[test]
    fn ssh_host_and_user_cannot_be_read_as_options() {
        let cases = [
            ("host", "-oProxyCommand=x"),
            ("user", "-l"),
            ("user", "a@b"),
            ("host", "'a b'"),
            ("host", "\"a\\tb\""),
            ("user", "'a b'"),
            ("host", "a@b"),
        ];
        for (field, value) in cases {
            let other = if field == "host" {
                "user: u"
            } else {
                "host: h"
            };
            let yaml = format!(
                "schema_version: 1\ngateway: g\nmode: m\nnodes:\n  master:\n    profile: p\n    ssh:\n      {field}: {value}\n      {other}\n      os: linux\n      data_dir: /d\n      known_hosts: kh\n"
            );
            let all = messages(&yaml);
            assert!(
                all.iter()
                    .any(|m| m.starts_with(&format!("nodes.master.ssh.{field}:"))),
                "{field}={value}: {all:?}"
            );
        }
    }
}
