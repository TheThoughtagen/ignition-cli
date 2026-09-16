//! `ign mcp serve` — the Model Context Protocol over newline-delimited
//! JSON-RPC 2.0 on stdio (Phase 14, 14-01).
//!
//! The protocol stream IS the stdout product (the `mcp` routes.rs row
//! is OutOfBand — completions/api-call genre): this module speaks JSON-RPC
//! 2.0 to an MCP client and never touches the envelope-render chassis.
//! Contracts this module owns:
//!
//! - **SC-1 (catalog)**: the tool catalog is DERIVED from
//!   `Cli::command()` at runtime — every non-hidden clap leaf except
//!   the exclusion set below, with clap-derived names/descriptions/
//!   inputSchemas. Nothing here is hand-written; the parity test in
//!   this file proves the derivation bidirectionally against a fresh
//!   independent walk, so catalog drift from the CLI surface is
//!   structurally impossible.
//! - **SC-2 (confirm gate)**: every leaf whose clap path appears in
//!   [`crate::GUARDED_OPS`] carries a synthetic REQUIRED
//!   `confirm: boolean` schema property; the dispatch bridge sets
//!   `--yes` ONLY from that field. `IGNITION_YES` (merged by
//!   `apply_env_defaults` for the human CLI) can never reach the
//!   protocol path — the bridge bypasses `apply_env_defaults` for the
//!   inner parse by construction.
//! - **Purity**: compact serialization only (`serde_json::to_string`)
//!   — one message per line, never pretty-printed (embedded newlines
//!   are a spec violation and a protocol death). Diagnostics ride the
//!   stderr-only tracing init in `main` — stdout carries protocol
//!   messages and nothing else.

// TEMP (14-01): the serve loop + dispatch bridge arrive in this
// plan's next task — until they consume the catalog, tolerate
// dead code so the Task-1 commit stays clippy-clean. REMOVED by
// Task 2.
#![allow(dead_code)]

use std::sync::Arc;

use clap::CommandFactory;
use serde_json::{Value, json};

use ignition_cli::cli::Cli;

use crate::GUARDED_OPS;

/// The single served MCP protocol revision (2025-06-18 spec,
/// planner-locked pin; OQ5). Negotiation per spec: a requested version
/// EQUAL to this is echoed; anything else (older, newer, absent)
/// answers with the server's latest — this one revision.
pub(crate) const PROTOCOL_VERSION: &str = "2025-06-18";

/// Leaf paths EXCLUDED from the tool catalog (planner-locked set):
/// self-referential or non-agentic verbs whose product is not a tool
/// call — completions writes shell script, `mcp`/`lsp` ARE protocol
/// servers, `tui` owns the terminal, and `edit` hands the terminal to
/// a child $EDITOR (meaningless as a tool call). Matched against ANY
/// path segment so the set works at every depth.
pub(crate) const EXCLUDED_LEAVES: &[&str] = &["completions", "mcp", "lsp", "tui", "edit"];

/// One executable argument of a tool leaf, pre-resolved from clap
/// reflection so the dispatch bridge never touches the clap tree at
/// call time (owned data, `Arc`-shared into spawned tasks).
#[derive(Debug, Clone)]
pub(crate) struct ArgSpec {
    /// Token key: the positional's id (e.g. `name`) or the flag's long
    /// name with dashes preserved (e.g. `collision-policy`).
    pub key: String,
    /// Positional → raw tokens in definition order; flag → `--long=…`.
    pub positional: bool,
    /// `ArgAction::SetTrue` → boolean property, emitted as `--long`.
    pub boolean: bool,
    /// `ArgAction::Append` (repeatable) → the argument's array is
    /// spread into repeated `--long=…` tokens.
    pub repeatable: bool,
}

/// One derived tool: the wire entry (name/description/schema) plus the
/// execution table the bridge needs. Both halves come from the SAME
/// clap walk — they cannot disagree.
#[derive(Debug, Clone)]
pub(crate) struct CatalogEntry {
    /// Tool name: leaf path with spaces → `_` (e.g. `tags_browse`).
    pub name: String,
    /// The clap leaf path (e.g. `tags browse`).
    pub path: String,
    /// The clap doc comment (`get_about`) — already written for agents.
    pub description: String,
    /// JSON Schema object derived from the leaf's clap args.
    pub schema: Value,
    pub args: Vec<ArgSpec>,
}

/// The full tool catalog. `Arc`-shared into spawned `tools/call`
/// tasks (clap `Command` borrows cannot cross `'static`).
pub(crate) type Catalog = Arc<Vec<CatalogEntry>>;

/// Tool-name encoding: leaf path with spaces replaced by underscores
/// (JSON-schema-safe, stable — the ONLY transformation between the
/// clap path and the wire name).
pub(crate) fn tool_name_for_path(path: &str) -> String {
    path.replace(' ', "_")
}

pub(crate) fn path_is_excluded(path: &str) -> bool {
    path.split(' ')
        .any(|segment| EXCLUDED_LEAVES.contains(&segment))
}

/// THE catalog builder: walk `Cli::command()` (the tui_coverage.rs
/// recursion, extended with the hidden/exclusion filters) and derive
/// every tool. The parity test runs an INDEPENDENT walk of the same
/// tree and pins bidirectional set equality.
pub(crate) fn build_catalog() -> Catalog {
    let mut entries = Vec::new();
    walk_catalog(&Cli::command(), "", &mut entries);
    Arc::new(entries)
}

/// The catalog walk: skip clap's auto `help` everywhere, skip hidden
/// commands (and their whole subtrees), and apply the leaf rule the
/// route registry uses — a node REQUIRES representation when it has no
/// real children OR its bare form is invocable (subcommand optional).
/// Row-requiring leaves become tools; required-subcommand groups
/// recurse only.
fn walk_catalog(cmd: &clap::Command, prefix: &str, out: &mut Vec<CatalogEntry>) {
    for sub in cmd.get_subcommands() {
        if sub.get_name() == "help" || sub.is_hide_set() {
            continue;
        }
        let path = if prefix.is_empty() {
            sub.get_name().to_string()
        } else {
            format!("{prefix} {}", sub.get_name())
        };
        let has_real_children = sub.get_subcommands().any(|c| c.get_name() != "help");
        if (!has_real_children || !sub.is_subcommand_required_set()) && !path_is_excluded(&path) {
            out.push(leaf_entry(&path, sub));
        }
        walk_catalog(sub, &path, out);
    }
}

/// Derive one leaf's tool entry. Schema rules (all clap-derived):
/// positionals → string property named by id (repeatable ones → arrays
/// of string); long flags → properties keyed by the long name with
/// dashes preserved; `ArgAction::SetTrue` → boolean;
/// `get_possible_values()` → enum; `is_required_set()` → required.
/// GLOBAL args (profile/json/compact/yes/verbose) are EXCLUDED — their
/// effect in the protocol path is either forced by the bridge
/// (json/compact), overridden by the ambient profile flag (profile),
/// confirm-gated (`yes` — replaced by the synthetic `confirm`
/// property), or stderr-only noise (verbose). A leaf present in
/// GUARDED_OPS gains the synthetic REQUIRED `confirm` boolean — the
/// catalog reads ONLY that const (single source, SC-2).
fn leaf_entry(path: &str, cmd: &clap::Command) -> CatalogEntry {
    let guarded = GUARDED_OPS
        .iter()
        .any(|(guarded_path, _)| *guarded_path == path);
    let mut properties = serde_json::Map::new();
    let mut required: Vec<Value> = Vec::new();
    let mut args = Vec::new();

    for arg in cmd.get_arguments() {
        // Hidden (auto help/version) and GLOBAL args never become
        // per-call properties (see the fn doc for the globals rule).
        if arg.is_hide_set() || arg.is_global_set() {
            continue;
        }
        let id = arg.get_id().as_str().to_string();
        if id == "help" || id == "version" {
            continue;
        }
        let positional = arg.is_positional();
        let key = if positional {
            id.clone()
        } else {
            arg.get_long().unwrap_or(id.as_str()).to_string()
        };
        let action = arg.get_action();
        let boolean = matches!(action, clap::ArgAction::SetTrue);
        let repeatable = matches!(action, clap::ArgAction::Append);
        args.push(ArgSpec {
            key: key.clone(),
            positional,
            boolean,
            repeatable,
        });

        let mut prop = serde_json::Map::new();
        let help = arg
            .get_long_help()
            .or_else(|| arg.get_help())
            .map(|styled| styled.to_string())
            .unwrap_or_default();
        if !help.is_empty() {
            prop.insert("description".into(), Value::String(help));
        }
        if boolean {
            prop.insert("type".into(), Value::String("boolean".into()));
        } else {
            let enum_values: Vec<Value> = arg
                .get_possible_values()
                .iter()
                .filter(|pv| !pv.is_hide_set())
                .map(|pv| Value::String(pv.get_name().to_string()))
                .collect();
            if repeatable {
                let mut item = serde_json::Map::new();
                item.insert("type".into(), Value::String("string".into()));
                if !enum_values.is_empty() {
                    item.insert("enum".into(), Value::Array(enum_values));
                }
                prop.insert("type".into(), Value::String("array".into()));
                prop.insert("items".into(), Value::Object(item));
            } else {
                prop.insert("type".into(), Value::String("string".into()));
                if !enum_values.is_empty() {
                    prop.insert("enum".into(), Value::Array(enum_values));
                }
            }
        }
        if arg.is_required_set() {
            required.push(Value::String(key.clone()));
        }
        properties.insert(key, Value::Object(prop));
    }

    if guarded {
        properties.insert(
            "confirm".into(),
            json!({
                "type": "boolean",
                "description": "Explicit confirmation for this destructive operation — the ONLY \
                               way --yes is reachable over MCP. Omitted or false returns the \
                               refusal envelope as an error tool result."
            }),
        );
        required.push(Value::String("confirm".into()));
    }

    let mut schema = serde_json::Map::new();
    schema.insert("type".into(), Value::String("object".into()));
    schema.insert("properties".into(), Value::Object(properties));
    schema.insert("required".into(), Value::Array(required));

    CatalogEntry {
        name: tool_name_for_path(path),
        path: path.to_string(),
        description: cmd
            .get_about()
            .map(|styled| styled.to_string())
            .unwrap_or_default(),
        schema: Value::Object(schema),
        args,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::GUARDED_OPS;

    /// An INDEPENDENT leaf walk of the clap tree (the tui_coverage.rs
    /// recursion verbatim) with the SAME filters the builder applies —
    /// hidden-skip, help-skip, exclusion set, tool-name encoding. Any
    /// divergence between this walk and `build_catalog`'s is a bug in
    /// one of them.
    fn independent_leaf_names() -> BTreeSet<String> {
        fn walk(cmd: &clap::Command, prefix: &str, out: &mut BTreeSet<String>) {
            for sub in cmd.get_subcommands() {
                if sub.get_name() == "help" || sub.is_hide_set() {
                    continue;
                }
                let path = if prefix.is_empty() {
                    sub.get_name().to_string()
                } else {
                    format!("{prefix} {}", sub.get_name())
                };
                let has_real_children = sub.get_subcommands().any(|c| c.get_name() != "help");
                if (!has_real_children || !sub.is_subcommand_required_set())
                    && !path_is_excluded(&path)
                {
                    out.insert(tool_name_for_path(&path));
                }
                walk(sub, &path, out);
            }
        }
        let mut out = BTreeSet::new();
        walk(&Cli::command(), "", &mut out);
        out
    }

    /// SC-1's structural fact, machine-proven: the derived catalog and
    /// an independent clap-tree walk are the SAME set — a leaf without
    /// a tool and a tool without a leaf both fail, and a silently
    /// skipped subtree cannot hide (cardinality pin).
    #[test]
    fn catalog_names_match_the_clap_tree_bidirectionally() {
        let catalog: BTreeSet<String> = build_catalog().iter().map(|e| e.name.clone()).collect();
        let tree = independent_leaf_names();
        assert!(!catalog.is_empty(), "the catalog derived the tree");
        let missing: Vec<_> = tree.difference(&catalog).collect();
        assert!(
            missing.is_empty(),
            "clap leaves with no tool entry (builder filter bug): {missing:#?}"
        );
        let extra: Vec<_> = catalog.difference(&tree).collect();
        assert!(
            extra.is_empty(),
            "tool entries with no clap leaf (hand-written drift): {extra:#?}"
        );
        assert_eq!(catalog.len(), tree.len(), "bidirectional set equality");
    }

    /// SC-2's catalog half: every GUARDED_OPS leaf advertises a
    /// REQUIRED `confirm` boolean; a read-only verb advertises none.
    #[test]
    fn guarded_leaves_carry_required_confirm_and_readonly_leaves_do_not() {
        let catalog = build_catalog();

        let delete = catalog
            .iter()
            .find(|e| e.name == "project_delete")
            .expect("project_delete derives from the clap tree");
        let delete_props = delete.schema.get("properties").expect("schema object");
        let confirm = delete_props
            .get("confirm")
            .expect("guarded leaf carries confirm");
        assert_eq!(
            confirm.get("type").and_then(Value::as_str),
            Some("boolean"),
            "confirm is a boolean property"
        );
        let delete_required = delete
            .schema
            .get("required")
            .and_then(Value::as_array)
            .expect("required array");
        assert!(
            delete_required.iter().any(|v| v == "confirm"),
            "confirm is REQUIRED on project_delete: {delete_required:?}"
        );

        let status = catalog
            .iter()
            .find(|e| e.name == "status")
            .expect("status derives from the clap tree");
        assert!(
            status
                .schema
                .get("properties")
                .and_then(|props| props.get("confirm"))
                .is_none(),
            "the read-only status verb carries no confirm property"
        );

        // And EVERY guarded leaf that is CATALOG-ELIGIBLE advertises
        // it (the registry → catalog direction; the drift test in
        // main.rs pins the dispatch-site → registry direction). One
        // GUARDED_OPS member is deliberately absent from the catalog:
        // `edit` — its guard is real (the push gate) but its product
        // is a child-$EDITOR terminal handoff, excluded as a tool call
        // by the planner-locked set. Pin that intersection so neither
        // half can drift silently.
        for (path, _) in GUARDED_OPS {
            let name = tool_name_for_path(path);
            if path_is_excluded(path) {
                assert_eq!(
                    *path, "edit",
                    "only edit may be both guarded and catalog-excluded"
                );
                assert!(
                    !catalog.iter().any(|e| e.name == name),
                    "excluded leaf {name} must not appear in the catalog"
                );
                continue;
            }
            let entry = catalog
                .iter()
                .find(|e| e.name == name)
                .unwrap_or_else(|| panic!("GUARDED_OPS leaf {path:?} missing from the catalog"));
            assert!(
                entry
                    .schema
                    .get("properties")
                    .and_then(|props| props.get("confirm"))
                    .is_some(),
                "guarded leaf {name} must carry the confirm property"
            );
        }
    }

    /// The catalog's name encoding is injective over the leaf paths —
    /// two distinct leaves mapping to one tool name would let
    /// tools/call dispatch the WRONG verb.
    #[test]
    fn tool_name_encoding_is_injective_over_the_clap_tree() {
        let catalog = build_catalog();
        let mut names: Vec<&str> = catalog.iter().map(|e| e.name.as_str()).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "tool names must be unique");
    }
}
