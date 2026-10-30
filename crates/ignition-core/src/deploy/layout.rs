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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
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

/// The four tag types the gateway writes under `ignition/` for a tag
/// provider, in the order a provider is built (21-02 capture, decision CD5).
pub const TAG_TYPES: [&str; 4] = [
    "tag-provider",
    "tag-group",
    "tag-type-definition",
    "tag-definition",
];

/// Gateway-relative path to collection path.
///
/// Two layouts are accepted:
///
/// - `resources/<m>/<t>/...` to `<m>/<t>/...`. Refuses a path with fewer than
///   three remaining components (a file directly under `resources/` or
///   `resources/<module>/` is not a resource file).
/// - `tags/<P>/<type>/<rest>` to `ignition/<type>/<P>/<rest>` for the four
///   [`TAG_TYPES`]. This is decision **CD5** of `21-LIVE-CAPTURES.md`,
///   captured on an 8.3.6 gateway: the type and provider segments swap
///   (confirming RESEARCH assumption A5, so CD5 and the research guess
///   agree). Folders nest as directories; `tag-group` entries are named
///   resources (`<P>/<Group>/{config,resource}.json`) while
///   `tag-type-definition` and `tag-definition` carry `udts.json`,
///   `tags.json` and `unary-resource.json`.
///
/// Two layouts CD5 left unexercised were captured later on an 8.3.6 Docker
/// rig and the Windows VM (`261030-8m8-LIVE-CAPTURES.md`) and are mapped
/// read-write like folder tags:
///
/// - **CD5a**, root-level tags: `tags/<P>/tag-definition/tags.json` (and
///   `udts.json`) to `ignition/tag-definition/<P>/tags.json`. The gateway
///   writes them beside the provider-root `unary-resource.json`, so they
///   join the provider-root unit `ignition/tag-definition/<P>`.
/// - **CD5b**, nested type folders: `tags/<P>/tag-type-definition/A/B/...`
///   to `ignition/tag-type-definition/<P>/A/B/...`. A type in `_types_/A/B`
///   lands in `A/B/` (no `_types_` directory exists); every folder carries
///   its own marker, a folder holding only sub-folders with `files: []`, so
///   each folder is its own unit.
///
/// One write, change or delete of either layout takes effect after one
/// config scan, as for folder tags (CD5-CTL). Render checks each marker's
/// `files` against its member files (CD5c) and refuses a folder named like a
/// member file (CD5d); both live in render, not here. The one layout still
/// unexercised is a collection other than the default one, which this
/// mapping cannot express (it has no collection concept).
pub fn collection_path(repo_rel: &RelPath) -> Result<RelPath, String> {
    if let Some(rest) = repo_rel.strip_prefix("tags") {
        return tag_collection_path(&rest);
    }
    let rest = repo_rel
        .strip_prefix("resources")
        .ok_or_else(|| "not under `resources/` or `tags/`".to_string())?;
    if rest.components().count() < 3 {
        return Err(
            "a resource file must sit at `resources/<module>/<type>/<file>` or deeper".to_string(),
        );
    }
    Ok(rest)
}

/// Collection path to gateway-relative repo path: the exact inverse of
/// [`collection_path`] (CD5), used by `deploy pull` to place live files.
///
/// - `ignition/<tagtype>/<P>/<rest>` with `tagtype` one of [`TAG_TYPES`]
///   becomes `tags/<P>/<tagtype>/<rest>`;
/// - any other path with at least three components becomes
///   `resources/<collection>`.
///
/// Fail-closed: a tag type with no provider or no file below it, an
/// `ignition/tag-*` type that is not one of the four tag types, and a path
/// with fewer than three components are errors. Every path this function
/// accepts maps back through [`collection_path`] (root-level tags CD5a and
/// nested type folders CD5b included). Messages carry no names.
pub fn repo_path(collection: &RelPath) -> Result<RelPath, String> {
    let parts: Vec<&str> = collection.components().collect();
    if parts.len() < 3 {
        return Err(
            "a resource file must sit at `<module>/<type>/<file>` or deeper in a collection"
                .to_string(),
        );
    }
    if parts[0] == "ignition" {
        let ty = parts[1];
        if TAG_TYPES.contains(&ty) {
            if parts.len() < 4 {
                return Err(
                    "a tag file must sit at `ignition/<tag type>/<provider>/<file>` or deeper"
                        .to_string(),
                );
            }
            return RelPath::new(&format!("tags/{}/{ty}/{}", parts[2], parts[3..].join("/")));
        }
        if ty.starts_with("tag-") {
            return Err(format!(
                "an `ignition/tag-*` type that is not one of the tag types ({})",
                TAG_TYPES.join(", ")
            ));
        }
    }
    RelPath::new(&format!("resources/{collection}"))
}

/// `<P>/<type>/<rest>` (the part after `tags/`) to `ignition/<type>/<P>/<rest>`.
fn tag_collection_path(rest: &RelPath) -> Result<RelPath, String> {
    let parts: Vec<&str> = rest.components().collect();
    if parts.len() < 3 {
        return Err("a tag file must sit at `tags/<provider>/<type>/<file>` or deeper".to_string());
    }
    let (provider, ty, tail) = (parts[0], parts[1], &parts[2..]);
    if !TAG_TYPES.contains(&ty) {
        return Err(format!(
            "`{ty}` is not a tag type (expected one of: {})",
            TAG_TYPES.join(", ")
        ));
    }
    // Any tail maps: root-level tags (CD5a) and nested type folders (CD5b)
    // were captured and are accepted (261030-8m8-LIVE-CAPTURES.md).
    RelPath::new(&format!("ignition/{ty}/{provider}/{}", tail.join("/")))
}

/// The tag provider directory a gateway-relative `tags/<P>/...` path sits in.
pub fn tag_provider(repo_rel: &RelPath) -> Option<&str> {
    let mut parts = repo_rel.components();
    if parts.next() != Some("tags") {
        return None;
    }
    parts.next()
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
    fn tags_map_per_cd5() {
        let cases = [
            (
                "tags/P/tag-provider/config.json",
                "ignition/tag-provider/P/config.json",
            ),
            (
                "tags/P/tag-group/Default/resource.json",
                "ignition/tag-group/P/Default/resource.json",
            ),
            (
                "tags/P/tag-type-definition/udts.json",
                "ignition/tag-type-definition/P/udts.json",
            ),
            (
                "tags/P/tag-definition/unary-resource.json",
                "ignition/tag-definition/P/unary-resource.json",
            ),
            (
                "tags/P/tag-definition/FolderA/Sub/tags.json",
                "ignition/tag-definition/P/FolderA/Sub/tags.json",
            ),
            // CD5a (261030-8m8): root-level tags join the provider-root unit
            (
                "tags/P/tag-definition/tags.json",
                "ignition/tag-definition/P/tags.json",
            ),
            (
                "tags/P/tag-definition/udts.json",
                "ignition/tag-definition/P/udts.json",
            ),
            // CD5b (261030-8m8): nested type folders, each its own unit
            (
                "tags/P/tag-type-definition/A/B/udts.json",
                "ignition/tag-type-definition/P/A/B/udts.json",
            ),
            (
                "tags/P/tag-type-definition/A/B/unary-resource.json",
                "ignition/tag-type-definition/P/A/B/unary-resource.json",
            ),
            (
                "tags/P/tag-type-definition/A/unary-resource.json",
                "ignition/tag-type-definition/P/A/unary-resource.json",
            ),
            // `_types_` is never a directory (CD5b); a folder of that name is
            // a plain nested folder
            (
                "tags/P/tag-type-definition/_types_/udts.json",
                "ignition/tag-type-definition/P/_types_/udts.json",
            ),
        ];
        for (from, to) in cases {
            assert_eq!(collection_path(&rp(from)).unwrap(), rp(to), "{from}");
        }
        assert_eq!(tag_provider(&rp("tags/P/tag-group/x")), Some("P"));
        assert_eq!(tag_provider(&rp("resources/a/b/c")), None);
    }

    #[test]
    fn unmapped_tag_layouts_stay_fail_closed() {
        // CD5a and CD5b lifted the root-level and nested refusals; what has
        // no tag type or no file below the type is still refused
        for bad in [
            "tags/P/other/x.json",
            "tags/P/x.json",
            "tags/x.json",
            "tags/P/tag-definition",
        ] {
            assert!(collection_path(&rp(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn repo_path_inverts_collection_path() {
        // resource and tag collection paths, including nested tag folders
        let cases = [
            "ignition/general-alarm-settings/config.json",
            "ignition/database-translator/POSTGRES/config.json",
            "ignition/database-translator/POSTGRES/resource.json",
            "com.example.mod/thing/A B/x/y.json",
            "ignition/tag-provider/P/config.json",
            "ignition/tag-group/P/Default/resource.json",
            "ignition/tag-type-definition/P/udts.json",
            "ignition/tag-definition/P/unary-resource.json",
            "ignition/tag-definition/P/FolderA/Sub/tags.json",
            // CD5a and CD5b (261030-8m8)
            "ignition/tag-definition/P/tags.json",
            "ignition/tag-definition/P/udts.json",
            "ignition/tag-type-definition/P/A/B/udts.json",
            "ignition/tag-type-definition/P/A/B/unary-resource.json",
            "ignition/tag-type-definition/P/A/unary-resource.json",
        ];
        for collection in cases {
            let c = rp(collection);
            let repo = repo_path(&c).unwrap_or_else(|e| panic!("{collection}: {e}"));
            assert_eq!(
                collection_path(&repo).unwrap(),
                c,
                "{collection} via {repo}"
            );
        }
        assert_eq!(
            repo_path(&rp("ignition/tag-group/P/Default/resource.json")).unwrap(),
            rp("tags/P/tag-group/Default/resource.json")
        );
        assert_eq!(
            repo_path(&rp("ignition/database-translator/X/config.json")).unwrap(),
            rp("resources/ignition/database-translator/X/config.json")
        );
    }

    #[test]
    fn repo_path_refuses_what_has_no_layout() {
        // too short for a resource file
        assert!(repo_path(&rp("ignition/config.json")).is_err());
        assert!(repo_path(&rp("config.json")).is_err());
        // a tag type with no provider or no file below it
        assert!(repo_path(&rp("ignition/tag-definition/P")).is_err());
        assert!(repo_path(&rp("ignition/tag-definition/x.json")).is_err());
        // a tag-looking type that is not one of the four tag types
        assert!(repo_path(&rp("ignition/tag-other/P/x.json")).is_err());
    }

    use proptest::prelude::*;

    proptest! {
        /// CD5a/CD5b (261030-8m8): for every tag type and any folder chain
        /// of 1 to 6 segments below the provider, `collection_path` and
        /// `repo_path` are exact inverses. No tag layout is refused any more,
        /// so nothing is assumed out.
        #[test]
        fn cd5_tag_paths_round_trip(
            provider in "[A-Za-z0-9_]{1,12}",
            ty in prop::sample::select(TAG_TYPES.to_vec()),
            tail in prop::collection::vec("[A-Za-z0-9_]{1,12}", 1..=6),
        ) {
            let repo = rp(&format!("tags/{provider}/{ty}/{}", tail.join("/")));
            let collection = collection_path(&repo).expect("every tag layout maps");
            prop_assert_eq!(
                collection.as_str(),
                format!("ignition/{ty}/{provider}/{}", tail.join("/"))
            );
            prop_assert_eq!(&repo_path(&collection).expect("inverse"), &repo);
            prop_assert_eq!(&collection_path(&repo_path(&collection).unwrap()).unwrap(), &collection);
        }
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
