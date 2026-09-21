//! Pinned third-party `.modl` module artifacts (Phase 15) — the registry
//! seam ([`ModuleSpec`], [`spec_for`]) and the on-disk cache ([`cache_root`],
//! [`module_cache_dir`], [`cached_entry`]) that [`fetch`] verifies into.
//!
//! **Security boundary (read this before touching `fetch.rs`):** the
//! sha256 check this module performs proves ONE thing — that the bytes on
//! disk are byte-for-byte what the release feed currently serves for a
//! given digest. It does NOT prove the module is validly signed by its
//! author, and it is not a substitute for one. Ignition's own module-scan
//! owns the signature check, at gateway load time, entirely outside
//! `ign`'s control. An attacker who can both upload a malicious `.modl`
//! AND control what digest the feed reports for it defeats this check by
//! construction — that supply-chain trust boundary is accepted, not
//! mitigated, here (see the phase's threat register, T-15-07).

pub mod fetch;

use std::path::{Path, PathBuf};

use crate::error::CoreError;

/// One third-party module's release-feed coordinates: where its releases
/// live (`repo`) and how a version string becomes a release tag / asset
/// filename (`tag_template` / `asset_template`, both substituting the
/// literal token `{version}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleSpec {
    /// Short, filesystem- and URL-safe identifier (`git`) — also the cache
    /// subdirectory name under `<cache_root>/modules/`.
    pub id: &'static str,
    /// `owner/repo` on GitHub.
    pub repo: &'static str,
    /// The release tag shape, e.g. `v{version}`.
    pub tag_template: &'static str,
    /// The release asset filename shape, e.g. `Git-{version}-signed.modl`.
    pub asset_template: &'static str,
}

impl ModuleSpec {
    /// The release tag for `version` (substitutes `{version}` into
    /// [`Self::tag_template`]).
    pub fn tag(&self, version: &str) -> String {
        self.tag_template.replace("{version}", version)
    }

    /// The expected release asset filename for `version` (substitutes
    /// `{version}` into [`Self::asset_template`]).
    pub fn asset_name(&self, version: &str) -> String {
        self.asset_template.replace("{version}", version)
    }
}

/// The `ignition-git-module` release feed (live-verified shape in
/// 15-RESEARCH.md: tag `v{version}`, asset `Git-{version}-signed.modl`).
pub const GIT_MODULE: ModuleSpec = ModuleSpec {
    id: "git",
    repo: "WhiskeyHouse/ignition-git-module",
    tag_template: "v{version}",
    asset_template: "Git-{version}-signed.modl",
};

/// The module registry — one entry today. Phase 16's second module
/// registers through this seam; nothing else grows here in Phase 15.
pub const MODULES: &[ModuleSpec] = &[GIT_MODULE];

/// Look up a registered module by [`ModuleSpec::id`].
pub fn spec_for(id: &str) -> Option<&'static ModuleSpec> {
    MODULES.iter().find(|spec| spec.id == id)
}

/// Validate a module version string BEFORE it becomes a filesystem path
/// segment or URL component (ASVS V5, T-15-03): 1–64 characters from
/// `[A-Za-z0-9._+-]`, with no `..` window anywhere (path-traversal
/// refusal — this also catches a lone `..`). Exit 2 `invalid_input` —
/// a caller-supplied bad value is the usage class, so no new slug is
/// warranted (the frozen taxonomy's existing precedent).
pub fn validate_version(version: &str) -> Result<(), CoreError> {
    let len_ok = (1..=64).contains(&version.len());
    let chars_ok = version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'));
    let no_traversal = !version.as_bytes().windows(2).any(|pair| pair == b"..");
    if len_ok && chars_ok && no_traversal {
        Ok(())
    } else {
        Err(CoreError::InvalidInput {
            reason: format!(
                "module version {version:?} is not a safe version string (expected \
                 1-64 characters of [A-Za-z0-9._+-], no \"..\")"
            ),
        })
    }
}

/// Validate a module id string the same way (T-15-03): 1–32 characters
/// from `[a-z0-9-]`. Exit 2 `invalid_input`.
pub fn validate_module_id(id: &str) -> Result<(), CoreError> {
    let len_ok = (1..=32).contains(&id.len());
    let chars_ok = id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if len_ok && chars_ok {
        Ok(())
    } else {
        Err(CoreError::InvalidInput {
            reason: format!(
                "module id {id:?} is not a safe identifier (expected 1-32 \
                 characters of [a-z0-9-])"
            ),
        })
    }
}

/// The module artifact cache root: `IGNITION_CLI_CACHE` env override
/// first, else the platform cache dir (`directories::ProjectDirs::
/// from("", "", "ignition-cli").cache_dir()`) — mirrors
/// [`crate::config::config_path`]'s `IGNITION_CLI_CONFIG` shape exactly,
/// including its `expect` on a missing home directory. The env override
/// is what lets tests point at a `tempfile::TempDir` instead of the
/// developer's real cache.
pub fn cache_root() -> PathBuf {
    std::env::var_os("IGNITION_CLI_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let dirs = directories::ProjectDirs::from("", "", "ignition-cli")
                .expect("no home directory discoverable");
            dirs.cache_dir().to_path_buf()
        })
}

/// The per-module cache directory: `<root>/modules/<module_id>`.
pub fn module_cache_dir(root: &Path, module_id: &str) -> PathBuf {
    root.join("modules").join(module_id)
}

/// A verified artifact already sitting in the cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedArtifact {
    /// Full path to the cached `.modl` file.
    pub path: PathBuf,
    /// The lowercase hex sha256 digest encoded in the filename.
    pub digest_sha256: String,
    /// On-disk size in bytes.
    pub bytes: u64,
}

/// `true` when `s` is exactly 64 lowercase hex characters — the full
/// digest, never a truncated prefix or an uppercase rendering (D-08).
fn is_full_lowercase_sha256_hex(s: &str) -> bool {
    s.len() == 64
        && s.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// Look up a cached, verified artifact for `module_id`/`version` — keyed
/// by VERSION ALONE (D-08): a hit never needs the feed, which is what
/// makes SC-3 (no network on a repeat fetch) and SC-4 (offline-with-cache
/// succeeds) possible. Accepts only filenames that strip to EXACTLY
/// `<version>-` + 64 lowercase hex + `.modl` — no regex, just
/// `strip_prefix`/`strip_suffix` plus a length + hex-digit check, which
/// also disambiguates a prerelease version whose own string contains a
/// hyphen (e.g. `2.3.4-rc1` never satisfies the fixed-length digest
/// check when only `2.3.4` was requested).
///
/// A missing cache directory is `None`, never an error. Selection is
/// DETERMINISTIC — never "whatever `read_dir` yielded first": the newest
/// entry by modified time wins, ties broken by filename order. One
/// version can legitimately own two entries once 15-02's
/// accept-the-change override persists a second digest alongside the
/// first.
pub fn cached_entry(root: &Path, module_id: &str, version: &str) -> Option<CachedArtifact> {
    let dir = module_cache_dir(root, module_id);
    let entries = std::fs::read_dir(&dir).ok()?;
    let prefix = format!("{version}-");

    let mut candidates: Vec<(std::time::SystemTime, String, PathBuf, u64)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(rest) = file_name.strip_prefix(&prefix) else {
            continue;
        };
        let Some(digest) = rest.strip_suffix(".modl") else {
            continue;
        };
        if !is_full_lowercase_sha256_hex(digest) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let modified = metadata.modified().unwrap_or(std::time::UNIX_EPOCH);
        candidates.push((modified, file_name.to_string(), path, metadata.len()));
    }

    // Newest modified time wins; ties broken by filename order — never
    // directory-iteration order, which is unspecified.
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    candidates
        .into_iter()
        .next()
        .map(|(_, file_name, path, bytes)| {
            let digest = file_name
                .strip_prefix(&prefix)
                .and_then(|rest| rest.strip_suffix(".modl"))
                .expect("candidate matched the prefix/suffix check above")
                .to_string();
            CachedArtifact {
                path,
                digest_sha256: digest,
                bytes,
            }
        })
}

#[cfg(test)]
mod tests {
    use super::{
        GIT_MODULE, MODULES, cached_entry, module_cache_dir, spec_for, validate_module_id,
        validate_version,
    };

    #[test]
    fn module_spec_substitutes_version() {
        assert_eq!(GIT_MODULE.tag("2.3.4"), "v2.3.4");
        assert_eq!(GIT_MODULE.asset_name("2.3.4"), "Git-2.3.4-signed.modl");
    }

    #[test]
    fn registry_finds_git_module() {
        assert!(MODULES.contains(&GIT_MODULE));
        assert_eq!(spec_for("git").map(|spec| spec.id), Some("git"));
        assert!(spec_for("nope").is_none());
    }

    // cache_root()'s IGNITION_CLI_CACHE env-override seam is exercised by
    // `cache_root_honours_env_override` in
    // tests/module_fetch_contract.rs (D-08) — kept there, not duplicated
    // here, per the plan's file assignment.

    #[test]
    fn cached_entry_is_none_for_missing_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(cached_entry(dir.path(), "git", "2.3.4").is_none());
    }

    #[test]
    fn module_cache_dir_shape() {
        let root = std::path::Path::new("/tmp/cache-root");
        assert_eq!(
            module_cache_dir(root, "git"),
            std::path::PathBuf::from("/tmp/cache-root/modules/git")
        );
    }

    #[test]
    fn validate_version_accepts_well_formed() {
        assert!(validate_version("2.3.4").is_ok());
        assert!(validate_version("2.3.4-rc1+build.1").is_ok());
    }

    #[test]
    fn validate_version_rejects_hostile_input() {
        for bad in [
            "../escape",
            "a/b",
            "with\0null",
            "back\\slash",
            "2..3",
            "",
            &"x".repeat(65),
        ] {
            let err = validate_version(bad).expect_err(&format!("{bad:?} must be refused"));
            assert_eq!(err.code(), "invalid_input");
            assert_eq!(err.exit_code(), 2);
        }
    }

    #[test]
    fn validate_module_id_accepts_registry_entries() {
        for spec in MODULES {
            assert!(
                validate_module_id(spec.id).is_ok(),
                "{} must validate",
                spec.id
            );
        }
    }

    #[test]
    fn validate_module_id_rejects_hostile_input() {
        for bad in ["../nope", "a/b", "UPPER", "", &"x".repeat(33)] {
            let err = validate_module_id(bad).expect_err(&format!("{bad:?} must be refused"));
            assert_eq!(err.code(), "invalid_input");
            assert_eq!(err.exit_code(), 2);
        }
    }
}
