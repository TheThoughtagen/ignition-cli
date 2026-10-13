//! Path vocabulary for declared trees: [`RelPath`], collection paths and
//! resource keys.
//!
//! A *repo-relative* path is `gateways/<g>/resources/<module>/<type>/...`;
//! its *collection path* drops the `resources/` prefix and is what the
//! gateway holds under its resource collection directory
//! (`<module>/<type>/...`). All paths are `/`-separated UTF-8 and compare as
//! raw bytes: no normalization, no case folding (DRND-01 encoding edge).

/// A validated `/`-separated relative path.
///
/// The constructor is the only way to build one, so every `RelPath` is free
/// of a leading `/`, backslashes, NUL bytes and empty, `.` or `..`
/// components. `Ord` is the inner `String`'s byte order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelPath(String);

impl RelPath {
    /// Validate `s` as a relative path.
    pub fn new(s: &str) -> Result<RelPath, String> {
        if s.is_empty() {
            return Err("path must not be empty".to_string());
        }
        if s.starts_with('/') {
            return Err("path must not start with `/`".to_string());
        }
        if s.contains('\\') {
            return Err("path must not contain a backslash".to_string());
        }
        if s.contains('\0') {
            return Err("path must not contain a NUL byte".to_string());
        }
        for component in s.split('/') {
            match component {
                "" => return Err("path must not contain an empty component".to_string()),
                "." | ".." => return Err("path must not contain `.` or `..`".to_string()),
                _ => {}
            }
        }
        Ok(RelPath(s.to_string()))
    }

    /// The path as a `/`-joined string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The `/`-separated components.
    pub fn components(&self) -> std::str::Split<'_, char> {
        self.0.split('/')
    }

    /// The remainder after `prefix` plus one `/`, when this path starts with
    /// that whole component prefix and has something left.
    pub fn strip_prefix(&self, prefix: &str) -> Option<RelPath> {
        let rest = self.0.strip_prefix(prefix)?.strip_prefix('/')?;
        RelPath::new(rest).ok()
    }
}

impl std::fmt::Display for RelPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The identity of one gateway resource.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceKey {
    /// Module id, e.g. `ignition`.
    pub module: String,
    /// Resource type, e.g. `database-translator`.
    pub ty: String,
    /// Resource name; `None` for a singleton. Orders before any name.
    pub name: Option<String>,
}

/// `resources/<m>/<t>/...` to `<m>/<t>/...`.
///
/// Refuses a path that does not start with `resources/` and one with fewer
/// than three remaining components (a file directly under `resources/` or
/// `resources/<module>/` is not a resource file).
pub fn collection_path(repo_rel: &RelPath) -> Result<RelPath, String> {
    let rest = repo_rel
        .strip_prefix("resources")
        .ok_or_else(|| "not under `resources/`".to_string())?;
    if rest.components().count() < 3 {
        return Err(
            "a resource file must sit at `resources/<module>/<type>/<file>` or deeper".to_string(),
        );
    }
    Ok(rest)
}

/// The resource a collection path belongs to: module and type from
/// components 1 and 2, the name from component 3 only when a 4th exists
/// (otherwise component 3 is a file of a singleton).
pub fn resource_key(collection: &RelPath) -> ResourceKey {
    let parts: Vec<&str> = collection.components().collect();
    ResourceKey {
        module: parts.first().copied().unwrap_or_default().to_string(),
        ty: parts.get(1).copied().unwrap_or_default().to_string(),
        name: if parts.len() >= 4 {
            Some(parts[2].to_string())
        } else {
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rp(s: &str) -> RelPath {
        RelPath::new(s).unwrap()
    }

    #[test]
    fn valid_paths_round_trip() {
        for ok in ["a", "a/b", "résumé/x", "A/a", "a b/c.json"] {
            assert_eq!(RelPath::new(ok).unwrap().as_str(), ok);
        }
    }

    #[test]
    fn hostile_paths_are_refused() {
        for bad in [
            "", "/abs", "a\\b", "a//b", "a/", "./a", "a/./b", "a/..", "../a", "a\0b",
        ] {
            assert!(RelPath::new(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn ordering_is_utf8_byte_order() {
        let mut v = [rp("b"), rp("a"), rp("B"), rp("é"), rp("Z")];
        v.sort();
        let got: Vec<&str> = v.iter().map(RelPath::as_str).collect();
        assert_eq!(got, ["B", "Z", "a", "b", "é"]);
    }

    #[test]
    fn strip_prefix_needs_a_whole_component() {
        assert_eq!(
            rp("resources/a/b").strip_prefix("resources"),
            Some(rp("a/b"))
        );
        assert_eq!(rp("resourcesx/a").strip_prefix("resources"), None);
        assert_eq!(rp("resources").strip_prefix("resources"), None);
    }

    #[test]
    fn collection_path_requires_three_components() {
        assert_eq!(
            collection_path(&rp("resources/ignition/x/config.json")).unwrap(),
            rp("ignition/x/config.json")
        );
        assert!(collection_path(&rp("resources/config.json")).is_err());
        assert!(collection_path(&rp("resources/ignition/config.json")).is_err());
        assert!(collection_path(&rp("other/ignition/x/y")).is_err());
    }

    #[test]
    fn resource_key_names_only_with_a_fourth_component() {
        let singleton = resource_key(&rp("ignition/general-alarm-settings/config.json"));
        assert_eq!(singleton.name, None);
        let named = resource_key(&rp("ignition/database-translator/POSTGRES/config.json"));
        assert_eq!(named.module, "ignition");
        assert_eq!(named.ty, "database-translator");
        assert_eq!(named.name.as_deref(), Some("POSTGRES"));
    }

    #[test]
    fn singleton_key_orders_before_named_of_same_type() {
        let a = ResourceKey {
            module: "m".into(),
            ty: "t".into(),
            name: None,
        };
        let b = ResourceKey {
            module: "m".into(),
            ty: "t".into(),
            name: Some("A".into()),
        };
        assert!(a < b);
    }
}
