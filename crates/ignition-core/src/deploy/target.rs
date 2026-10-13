//! `targets/<env>.yaml`: where a rendered tree is deployed.
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
}

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
        }

        if findings.is_empty() {
            Ok(())
        } else {
            Err(CoreError::deploy_findings(findings))
        }
    }
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
        let yaml = "schema_version: 1\ngateway: plant-a\nmode: prod\nnodes:\n  master:\n    profile: plant-a-master\n    ssh:\n      host: 10.0.0.5\n      port: 2222\n      user: svc\n      key_file: /keys/id_ed25519\n      os: windows\n      data_dir: 'C:\\Program Files\\Inductive Automation\\Ignition\\data'\n  backup:\n    profile: plant-a-backup\n    ssh:\n      host: 10.0.0.6\n      user: svc\n      key_env: PLANT_A_KEY\n      os: linux\n      data_dir: /usr/local/bin/ignition/data\n";
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
    }

    #[test]
    fn target_unknown_ssh_field_is_positioned() {
        let yaml = "schema_version: 1\ngateway: g\nmode: m\nnodes:\n  master:\n    profile: p\n    ssh:\n      host: h\n      user: u\n      os: linux\n      data_dir: /d\n      bogus: 1\n";
        let f = findings(Target::parse("targets/x.yaml", yaml.as_bytes()).expect_err("unknown"))
            .remove(0);
        assert_eq!((f.line, f.column), (Some(12), Some(7)));
        assert!(f.message.contains("bogus"), "{}", f.message);
    }

    #[test]
    fn target_os_rejects_unknown() {
        let yaml = "schema_version: 1\ngateway: g\nmode: m\nnodes:\n  master:\n    profile: p\n    ssh:\n      host: h\n      user: u\n      os: macos\n      data_dir: /d\n";
        let f =
            findings(Target::parse("targets/x.yaml", yaml.as_bytes()).expect_err("os")).remove(0);
        assert_eq!(f.line, Some(10));
        assert!(f.column.is_some());
        assert!(f.message.contains("unknown variant"), "{}", f.message);
    }

    #[test]
    fn target_semantic_findings() {
        let yaml = "schema_version: 1\ngateway: ../x\nmode: .hidden\nnodes:\n  master:\n    profile: ''\n    ssh:\n      host: h\n      user: u\n      key_file: /k\n      key_env: K\n      os: linux\n      data_dir: ''\n";
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
}
