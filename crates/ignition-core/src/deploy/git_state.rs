//! The gateway's project repo as the git module leaves it, and what ign may do
//! about it (Phase 27).
//!
//! This module is pure: it holds the types a host read produces and the
//! classification of those types. Parsing the bytes a gateway host returns
//! (config, refs) is done by host git in [`super::git_remote`]; nothing here
//! runs git or touches a file.
//!
//! [`classify`] maps every `.git` shape a host can report to exactly one
//! [`GitStateKind`] and, for an unsupported shape, to exactly one word of
//! [`HOST_REASONS`]. The order of the checks is fixed and documented on
//! [`classify`]: the first match wins, so a repo that is both mid-merge and
//! has no remote reports `mid_operation`.
//!
//! No URL with userinfo leaves this module: [`GitState`] carries the redacted
//! URL and the normalised key only, and the `Debug` of [`GitRemoteConfig`]
//! redacts too.

use std::collections::BTreeMap;
use std::fmt;

use crate::deploy::transport::projects::GitEntry;
use crate::error::{CoreError, PreflightFailure};

/// Largest `HEAD` or `config` a host may report (bytes).
pub const GIT_SMALL_FILE_MAX: usize = 65_536;
/// Largest `packed-refs` a host may report (bytes).
pub const GIT_PACKED_REFS_MAX: usize = 8_388_608;
/// Most loose refs a host may report.
pub const GIT_LOOSE_REFS_MAX: usize = 4_096;
/// Largest `logs/HEAD` tail a host may report (bytes).
pub const GIT_LOG_TAIL_MAX: usize = 65_536;

/// The marker names a host reports on (presence only). A host never reports a
/// name outside this list.
pub const GIT_MARKERS: &[&str] = &[
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "rebase-merge",
    "rebase-apply",
    "index.lock",
    "HEAD.lock",
    "config.lock",
    "shallow",
    "objects/info/alternates",
];

/// Markers that mean an operation is in progress.
const MID_OPERATION_MARKERS: &[&str] = &[
    "MERGE_HEAD",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "rebase-merge",
    "rebase-apply",
];

/// Markers that mean another process holds a lock.
const LOCK_MARKERS: &[&str] = &["index.lock", "HEAD.lock", "config.lock"];

/// The reason words a host-file classification can produce. Every word has a
/// producer in [`classify`] and a test.
pub const HOST_REASONS: &[&str] = &[
    "not_directory",
    "symlink",
    "no_head",
    "detached_head",
    "unborn_head",
    "mid_operation",
    "locked",
    "no_remote",
    "multiple_remotes",
    "no_upstream",
    "shallow",
    "alternates",
];

/// The raw, capped answer a host gives about one `.git`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFiles {
    /// What the `.git` entry is.
    pub kind: GitEntry,
    /// The bytes of `HEAD`, when it exists.
    pub head: Option<Vec<u8>>,
    /// The bytes of `config`, when it exists.
    pub config: Option<Vec<u8>>,
    /// The bytes of `packed-refs`, when it exists.
    pub packed_refs: Option<Vec<u8>>,
    /// Every loose ref under `refs/`: full ref name and file bytes.
    pub loose_refs: Vec<(String, Vec<u8>)>,
    /// Which of [`GIT_MARKERS`] exist.
    pub markers: Vec<String>,
    /// Whether `index` exists.
    pub index_present: bool,
    /// The tail of `logs/HEAD`, when it exists.
    pub head_log_tail: Option<Vec<u8>>,
}

impl GitFiles {
    /// A host answer for an entry that is not a directory (nothing to read).
    pub fn without_contents(kind: GitEntry) -> GitFiles {
        GitFiles {
            kind,
            head: None,
            config: None,
            packed_refs: None,
            loose_refs: Vec::new(),
            markers: Vec::new(),
            index_present: false,
            head_log_tail: None,
        }
    }

    /// Refuse an answer that exceeds a cap, before anything is parsed
    /// (T-27-04).
    pub fn check_caps(&self) -> Result<(), CoreError> {
        let over = |what: &str, len: usize, max: usize| -> Result<(), CoreError> {
            if len > max {
                Err(state_failure(format!(
                    "the gateway repo's {what} is {len} bytes, over the {max} byte limit"
                )))
            } else {
                Ok(())
            }
        };
        over("HEAD", len_of(&self.head), GIT_SMALL_FILE_MAX)?;
        over("config", len_of(&self.config), GIT_SMALL_FILE_MAX)?;
        over(
            "packed-refs",
            len_of(&self.packed_refs),
            GIT_PACKED_REFS_MAX,
        )?;
        over(
            "logs/HEAD tail",
            len_of(&self.head_log_tail),
            GIT_LOG_TAIL_MAX,
        )?;
        if self.loose_refs.len() > GIT_LOOSE_REFS_MAX {
            return Err(state_failure(format!(
                "the gateway repo holds {} loose refs, over the {GIT_LOOSE_REFS_MAX} limit",
                self.loose_refs.len()
            )));
        }
        for (name, bytes) in &self.loose_refs {
            if bytes.len() > GIT_SMALL_FILE_MAX {
                return Err(state_failure(format!(
                    "the gateway repo's loose ref `{name}` is over the {GIT_SMALL_FILE_MAX} byte limit"
                )));
            }
        }
        Ok(())
    }

    /// Whether the host reported `marker`.
    fn has_marker(&self, marker: &str) -> bool {
        self.markers.iter().any(|m| m == marker)
    }
}

fn len_of(bytes: &Option<Vec<u8>>) -> usize {
    bytes.as_ref().map_or(0, Vec::len)
}

fn state_failure(detail: String) -> CoreError {
    CoreError::DeployPreflightFailed {
        failures: vec![PreflightFailure {
            check: "git_state".to_string(),
            detail: redact_text(&detail),
        }]
        .into(),
    }
}

/// One `[remote "<name>"]` section.
#[derive(Clone, PartialEq, Eq)]
pub struct GitRemoteConfig {
    /// `remote.<name>.url` (raw: may carry userinfo; never printed).
    pub url: String,
    /// Every `remote.<name>.fetch` refspec.
    pub fetch: Vec<String>,
}

impl fmt::Debug for GitRemoteConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GitRemoteConfig")
            .field("url", &redact_url(&self.url))
            .field("fetch", &self.fetch)
            .finish()
    }
}

/// One `[branch "<name>"]` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitBranchConfig {
    /// `branch.<name>.remote`, when set.
    pub remote: Option<String>,
    /// `branch.<name>.merge`, when set.
    pub merge: Option<String>,
}

/// The parts of a gateway `config` ign reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitConfig {
    /// Remotes by name.
    pub remotes: BTreeMap<String, GitRemoteConfig>,
    /// Branch sections by branch name.
    pub branches: BTreeMap<String, GitBranchConfig>,
}

/// What `HEAD` points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitHead {
    /// `ref: refs/heads/<name>`; `commit` is `None` while the branch is
    /// unborn.
    Branch {
        /// The short branch name.
        name: String,
        /// The branch tip, when it exists.
        commit: Option<String>,
    },
    /// A commit id.
    Detached {
        /// The commit id.
        commit: String,
    },
}

impl GitHead {
    /// The commit `HEAD` resolves to, when it is born.
    pub fn commit(&self) -> Option<&str> {
        match self {
            GitHead::Branch { commit, .. } => commit.as_deref(),
            GitHead::Detached { commit } => Some(commit),
        }
    }
}

/// The refs of a gateway repo, read through a scratch git dir.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRefs {
    /// `HEAD`.
    pub head: GitHead,
    /// Full ref name (`refs/heads/<b>`) to commit id.
    pub heads: BTreeMap<String, String>,
    /// Full ref name (`refs/remotes/<r>/<b>`) to commit id.
    pub remotes: BTreeMap<String, String>,
}

/// How the gateway's repo is to be treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitStateKind {
    /// No `.git`.
    Absent,
    /// A normal repo the module manages: a tracked branch with an upstream.
    Managed,
    /// A partial repo the module left behind that ign may complete (27-17,
    /// D-13): an unborn `HEAD`, no local branch, no index, exactly one remote
    /// and no mid-operation marker. It holds no commit, so there is no local
    /// work to lose; ign adopts it onto an explicitly named branch.
    Adoptable,
    /// A shape ign refuses; see [`GitState::reason`].
    Unsupported,
}

impl GitStateKind {
    /// The word reports and JSON use.
    pub fn as_str(self) -> &'static str {
        match self {
            GitStateKind::Absent => "absent",
            GitStateKind::Managed => "managed",
            GitStateKind::Adoptable => "adoptable",
            GitStateKind::Unsupported => "unsupported",
        }
    }
}

/// The one remote of a managed repo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRemote {
    /// The remote's name in the gateway config.
    pub name: String,
    /// Its URL with no userinfo.
    pub url_redacted: String,
    /// The normalised key two URLs of one repo share ([`url_key`]).
    pub url_key: String,
}

/// The classification of one gateway `.git`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitState {
    /// The class.
    pub kind: GitStateKind,
    /// One of [`HOST_REASONS`] when `kind` is `Unsupported`.
    pub reason: Option<String>,
    /// `HEAD`, when it could be read.
    pub head: Option<GitHead>,
    /// The one remote, when there is exactly one.
    pub remote: Option<GitRemote>,
    /// The tracked branch of a managed repo.
    pub tracked: Option<String>,
    /// The tracked remote's refs, by branch name (`production`, not the full
    /// ref name).
    pub remote_refs: BTreeMap<String, String>,
}

impl GitState {
    fn plain(kind: GitStateKind) -> GitState {
        GitState {
            kind,
            reason: None,
            head: None,
            remote: None,
            tracked: None,
            remote_refs: BTreeMap::new(),
        }
    }

    fn unsupported(reason: &'static str, head: Option<GitHead>) -> GitState {
        GitState {
            reason: Some(reason.to_string()),
            head,
            ..GitState::plain(GitStateKind::Unsupported)
        }
    }
}

/// Classify one gateway `.git`. `config` and `refs` are what host git parsed
/// from `files`; `refs` is `None` when `HEAD` is absent or unparseable.
///
/// Order, first match wins: absent; `symlink`; `not_directory`; `no_head`;
/// `mid_operation`; `locked`; `shallow`; `alternates`; `no_remote`;
/// `multiple_remotes`; `detached_head`; an unborn `HEAD` (adoptable when no
/// local branch and no index exists, else `unborn_head`); `no_upstream` (the
/// branch's `remote` is not the one remote, or its `merge` is not
/// `refs/heads/<branch>`); otherwise managed.
///
/// The adoptable shape is the one the module's `setupLocalRepo` leaves when
/// the remote has no `master` (G6): by the time the unborn `HEAD` is reached
/// the repo has no marker and exactly one remote, so only the absence of every
/// `refs/heads/*` (loose or packed) and of the index remains to be checked. It
/// says nothing about which branch to adopt onto: an unborn `HEAD` names JGit's
/// default (`master`), never intent.
pub fn classify(files: &GitFiles, config: Option<&GitConfig>, refs: Option<&GitRefs>) -> GitState {
    match files.kind {
        GitEntry::Absent => return GitState::plain(GitStateKind::Absent),
        GitEntry::Symlink => return GitState::unsupported("symlink", None),
        GitEntry::File | GitEntry::Other => return GitState::unsupported("not_directory", None),
        GitEntry::Directory => {}
    }
    let refs = match (&files.head, refs) {
        (Some(_), Some(refs)) => refs,
        _ => return GitState::unsupported("no_head", None),
    };
    let head = Some(refs.head.clone());
    if MID_OPERATION_MARKERS.iter().any(|m| files.has_marker(m)) {
        return GitState::unsupported("mid_operation", head);
    }
    if LOCK_MARKERS.iter().any(|m| files.has_marker(m)) {
        return GitState::unsupported("locked", head);
    }
    if files.has_marker("shallow") {
        return GitState::unsupported("shallow", head);
    }
    if files.has_marker("objects/info/alternates") {
        return GitState::unsupported("alternates", head);
    }
    let empty = GitConfig::default();
    let config = config.unwrap_or(&empty);
    let mut remotes = config.remotes.iter();
    let (remote_name, remote_config) = match (remotes.next(), remotes.next()) {
        (None, _) => return GitState::unsupported("no_remote", head),
        (Some(_), Some(_)) => return GitState::unsupported("multiple_remotes", head),
        (Some(one), None) => one,
    };
    let (branch, born) = match &refs.head {
        GitHead::Detached { .. } => return GitState::unsupported("detached_head", head),
        GitHead::Branch { name, commit } => (name, commit.is_some()),
    };
    if !born {
        // A repo with a local branch or an index holds work (or the trace of
        // it) that adoption would not account for: it stays refused.
        if !refs.heads.is_empty() || files.index_present {
            return GitState::unsupported("unborn_head", head);
        }
        return GitState {
            kind: GitStateKind::Adoptable,
            reason: None,
            head,
            remote: Some(remote_of(remote_name, remote_config)),
            tracked: None,
            remote_refs: remote_refs_of(refs, remote_name),
        };
    }
    // The tracked branch is the current branch's upstream (D-03, D-11): the
    // one remote AND the same-named branch there. `tracked` below is the local
    // name, which by this check is also the remote branch the module's pull
    // reads, so the gate and the advance name that branch (WR-A5).
    let want_merge = format!("refs/heads/{branch}");
    let upstream_ok = config.branches.get(branch).is_some_and(|b| {
        b.remote.as_deref() == Some(remote_name.as_str())
            && b.merge.as_deref() == Some(want_merge.as_str())
    });
    if !upstream_ok {
        return GitState::unsupported("no_upstream", head);
    }
    GitState {
        kind: GitStateKind::Managed,
        reason: None,
        head,
        remote: Some(remote_of(remote_name, remote_config)),
        tracked: Some(branch.clone()),
        remote_refs: remote_refs_of(refs, remote_name),
    }
}

fn remote_of(name: &str, config: &GitRemoteConfig) -> GitRemote {
    GitRemote {
        name: name.to_string(),
        url_redacted: redact_url(&config.url),
        url_key: url_key(&config.url),
    }
}

/// The remote's tracking refs by short branch name.
fn remote_refs_of(refs: &GitRefs, remote_name: &str) -> BTreeMap<String, String> {
    let prefix = format!("refs/remotes/{remote_name}/");
    refs.remotes
        .iter()
        .filter_map(|(name, id)| {
            name.strip_prefix(&prefix)
                .map(|short| (short.to_string(), id.clone()))
        })
        .collect()
}

/// Why `name` cannot be a branch name ign writes into a gateway repo's refs
/// and config, or `None` (T-27-38). Pure and conservative: a strict subset of
/// what git accepts, because the name becomes a path below `.git/refs/heads`
/// and a config subsection. Non-empty; only `A-Za-z0-9._/-`; at most 200
/// characters; no `..` or `//`; no leading `-` or `/`; no trailing `/`, `.` or
/// `.lock`; no component that starts with `.` or ends with `.lock`.
pub fn branch_name_problem(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        return Some("must not be empty");
    }
    if name.len() > 200 {
        return Some("must be at most 200 characters");
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'-'))
    {
        return Some("may hold only letters, digits and . _ / -");
    }
    if name.starts_with('-') {
        return Some("must not start with `-`");
    }
    if name.starts_with('/') || name.ends_with('/') || name.contains("//") {
        return Some("must not start or end with `/` or hold an empty component");
    }
    if name.contains("..") {
        return Some("must not contain `..`");
    }
    if name.ends_with('.') {
        return Some("must not end with `.`");
    }
    if name
        .split('/')
        .any(|part| part.starts_with('.') || part.ends_with(".lock"))
    {
        return Some("a component must not start with `.` or end with `.lock`");
    }
    None
}

/// `url` with its userinfo removed: `scheme://user:pass@host/x` and the
/// scp-like `user@host:path` both lose everything up to the last `@` of the
/// authority.
pub fn redact_url(url: &str) -> String {
    if let Some(at) = url.find("://") {
        let (scheme, rest) = url.split_at(at + 3);
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, tail) = rest.split_at(end);
        let host = authority
            .rfind('@')
            .map_or(authority, |i| &authority[i + 1..]);
        return format!("{scheme}{host}{tail}");
    }
    if let Some(colon) = url.find(':') {
        let head = &url[..colon];
        if !head.contains('/')
            && let Some(at) = head.rfind('@')
        {
            return format!("{}{}", &head[at + 1..], &url[colon..]);
        }
    }
    url.to_string()
}

/// Redact every URL-shaped token of free text (git's stderr, a path list):
/// a token holding `://`, or an `@` before a `:`, goes through
/// [`redact_url`]. Used on every message that may carry a gateway URL.
///
/// The userinfo of every `scheme://` authority is removed first, with the
/// authority running to the first `/`, `?`, `#` or whitespace: a quote mark
/// is legal in userinfo, so splitting at quotes first could leave part of a
/// password in a token with no `@` or `://` left to find.
pub fn redact_text(text: &str) -> String {
    let mut no_userinfo = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let (head, after) = rest.split_at(at + 3);
        no_userinfo.push_str(head);
        let end = after
            .find(|c: char| matches!(c, '/' | '?' | '#') || c.is_whitespace())
            .unwrap_or(after.len());
        let (authority, tail) = after.split_at(end);
        let host = authority
            .rfind('@')
            .map_or(authority, |i| &authority[i + 1..]);
        no_userinfo.push_str(host);
        rest = tail;
    }
    no_userinfo.push_str(rest);
    let text = no_userinfo.as_str();
    let mut out = String::with_capacity(text.len());
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String| {
        if token.contains("://") || (token.contains('@') && token.contains(':')) {
            out.push_str(&redact_url(token));
        } else {
            out.push_str(token);
        }
        token.clear();
    };
    for c in text.chars() {
        if c.is_whitespace() || matches!(c, '\'' | '"' | '`' | '<' | '>') {
            flush(&mut token, &mut out);
            out.push(c);
        } else {
            token.push(c);
        }
    }
    flush(&mut token, &mut out);
    out
}

/// The key two URLs of one repo share: no scheme, no userinfo, a lowercase
/// host, no trailing `/` or `.git`; the scp-like `host:org/repo` and
/// `https://host/org/repo` give the same `host/org/repo`.
pub fn url_key(url: &str) -> String {
    let stripped = redact_url(url);
    let host_and_path = match stripped.find("://") {
        Some(at) => stripped[at + 3..].to_string(),
        None => match stripped.find(':') {
            Some(colon) if !stripped[..colon].contains('/') => {
                format!("{}/{}", &stripped[..colon], &stripped[colon + 1..])
            }
            _ => stripped.clone(),
        },
    };
    let (host, path) = match host_and_path.find('/') {
        Some(slash) => (&host_and_path[..slash], &host_and_path[slash..]),
        None => (host_and_path.as_str(), ""),
    };
    let mut path = path.to_string();
    loop {
        let before = path.len();
        while path.ends_with('/') {
            path.pop();
        }
        if let Some(trimmed) = path.strip_suffix(".git") {
            path = trimmed.to_string();
        }
        if path.len() == before {
            break;
        }
    }
    format!("{}{}", host.to_ascii_lowercase(), path)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn files(kind: GitEntry) -> GitFiles {
        let mut f = GitFiles::without_contents(kind);
        if kind == GitEntry::Directory {
            f.head = Some(b"ref: refs/heads/production\n".to_vec());
            f.config = Some(Vec::new());
        }
        f
    }

    fn config(remotes: &[&str], upstream: Option<(&str, &str)>) -> GitConfig {
        let mut c = GitConfig::default();
        for name in remotes {
            c.remotes.insert(
                (*name).to_string(),
                GitRemoteConfig {
                    url: "https://user:secret@git.example/org/capapp.git".to_string(),
                    fetch: vec![format!("+refs/heads/*:refs/remotes/{name}/*")],
                },
            );
        }
        if let Some((branch, remote)) = upstream {
            c.branches.insert(
                branch.to_string(),
                GitBranchConfig {
                    remote: Some(remote.to_string()),
                    merge: Some(format!("refs/heads/{branch}")),
                },
            );
        }
        c
    }

    /// WR-A5 (27-27): the upstream holds only when its `merge` names the same
    /// branch on the one remote.
    #[test]
    fn classify_requires_merge_of_the_same_branch() {
        let dir = || files(GitEntry::Directory);
        let good_refs = refs(born());
        let with_merge = |merge: Option<&str>| {
            let mut c = config(&["capapp"], Some(("production", "capapp")));
            c.branches.get_mut("production").expect("branch").merge = merge.map(str::to_string);
            c
        };
        let other = classify(
            &dir(),
            Some(&with_merge(Some("refs/heads/other"))),
            Some(&good_refs),
        );
        assert_eq!(reason(&other), "no_upstream");
        let absent = classify(&dir(), Some(&with_merge(None)), Some(&good_refs));
        assert_eq!(reason(&absent), "no_upstream");
        let short = classify(
            &dir(),
            Some(&with_merge(Some("production"))),
            Some(&good_refs),
        );
        assert_eq!(reason(&short), "no_upstream");
        let same = classify(
            &dir(),
            Some(&with_merge(Some("refs/heads/production"))),
            Some(&good_refs),
        );
        assert_eq!(same.kind, GitStateKind::Managed, "{same:?}");
        assert_eq!(same.tracked.as_deref(), Some("production"));
    }

    fn refs(head: GitHead) -> GitRefs {
        let mut r = GitRefs {
            head,
            heads: BTreeMap::new(),
            remotes: BTreeMap::new(),
        };
        r.heads
            .insert("refs/heads/production".to_string(), SHA_A.to_string());
        r.remotes.insert(
            "refs/remotes/capapp/production".to_string(),
            SHA_B.to_string(),
        );
        r
    }

    fn born() -> GitHead {
        GitHead::Branch {
            name: "production".to_string(),
            commit: Some(SHA_A.to_string()),
        }
    }

    fn reason(state: &GitState) -> &str {
        assert_eq!(state.kind, GitStateKind::Unsupported, "{state:?}");
        state.reason.as_deref().expect("a reason")
    }

    #[test]
    fn classify_managed_module_clone() {
        let f = files(GitEntry::Directory);
        let c = config(&["capapp"], Some(("production", "capapp")));
        let r = refs(born());
        let state = classify(&f, Some(&c), Some(&r));
        assert_eq!(state.kind, GitStateKind::Managed);
        assert_eq!(state.reason, None);
        assert_eq!(state.tracked.as_deref(), Some("production"));
        let remote = state.remote.clone().expect("a remote");
        assert_eq!(remote.name, "capapp");
        assert_eq!(remote.url_redacted, "https://git.example/org/capapp.git");
        assert_eq!(remote.url_key, "git.example/org/capapp");
        assert_eq!(
            state.remote_refs.get("production").map(String::as_str),
            Some(SHA_B)
        );
        assert!(!format!("{state:?}").contains("secret"));
    }

    #[test]
    fn classify_reasons() {
        let good_config = config(&["capapp"], Some(("production", "capapp")));
        let good_refs = refs(born());
        let dir = || files(GitEntry::Directory);
        let marked = |marker: &str| {
            let mut f = dir();
            f.markers.push(marker.to_string());
            f
        };
        let mut seen: Vec<String> = Vec::new();
        let mut check = |label: &str, state: GitState, want: &str| {
            assert_eq!(reason(&state), want, "{label}");
            seen.push(want.to_string());
        };

        let symlink = files(GitEntry::Symlink);
        check(
            "symlink",
            classify(&symlink, Some(&good_config), Some(&good_refs)),
            "symlink",
        );
        for kind in [GitEntry::File, GitEntry::Other] {
            check(
                "not_directory",
                classify(&files(kind), None, None),
                "not_directory",
            );
        }
        let mut no_head = dir();
        no_head.head = None;
        check(
            "no_head absent",
            classify(&no_head, Some(&good_config), None),
            "no_head",
        );
        check(
            "no_head unparseable",
            classify(&dir(), Some(&good_config), None),
            "no_head",
        );
        for marker in [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "rebase-merge",
            "rebase-apply",
        ] {
            check(
                marker,
                classify(&marked(marker), Some(&good_config), Some(&good_refs)),
                "mid_operation",
            );
        }
        for marker in ["index.lock", "HEAD.lock", "config.lock"] {
            check(
                marker,
                classify(&marked(marker), Some(&good_config), Some(&good_refs)),
                "locked",
            );
        }
        check(
            "shallow",
            classify(&marked("shallow"), Some(&good_config), Some(&good_refs)),
            "shallow",
        );
        check(
            "alternates",
            classify(
                &marked("objects/info/alternates"),
                Some(&good_config),
                Some(&good_refs),
            ),
            "alternates",
        );
        check(
            "no_remote",
            classify(&dir(), Some(&config(&[], None)), Some(&good_refs)),
            "no_remote",
        );
        check(
            "multiple_remotes",
            classify(
                &dir(),
                Some(&config(&["a", "b"], Some(("production", "a")))),
                Some(&good_refs),
            ),
            "multiple_remotes",
        );
        check(
            "detached_head",
            classify(
                &dir(),
                Some(&good_config),
                Some(&refs(GitHead::Detached {
                    commit: SHA_A.to_string(),
                })),
            ),
            "detached_head",
        );
        check(
            "unborn_head",
            classify(
                &dir(),
                Some(&good_config),
                Some(&refs(GitHead::Branch {
                    name: "master".to_string(),
                    commit: None,
                })),
            ),
            "unborn_head",
        );
        check(
            "no_upstream none",
            classify(&dir(), Some(&config(&["capapp"], None)), Some(&good_refs)),
            "no_upstream",
        );
        check(
            "no_upstream other remote",
            classify(
                &dir(),
                Some(&config(&["capapp"], Some(("production", "elsewhere")))),
                Some(&good_refs),
            ),
            "no_upstream",
        );

        // Every published reason word has a producer and a test above.
        for word in HOST_REASONS {
            assert!(
                seen.iter().any(|s| s == word),
                "no classify_reasons case for `{word}`"
            );
        }
        // And nothing outside the table is produced.
        for word in &seen {
            assert!(HOST_REASONS.contains(&word.as_str()), "{word}");
        }
    }

    /// The partial repo `setupLocalRepo` leaves against a remote with no
    /// `master` (G6, both `git.yaml` variants): an unborn `master` HEAD, one
    /// remote named `origin`, every remote branch and tag already fetched into
    /// `packed-refs`, no local branch, no index, no `[branch]` section.
    fn g6_partial(variant_remote_refs: &[(&str, &str)]) -> (GitFiles, GitConfig, GitRefs) {
        let mut f = files(GitEntry::Directory);
        f.head = Some(b"ref: refs/heads/master\n".to_vec());
        f.index_present = false;
        let c = config(&["origin"], None);
        let mut r = GitRefs {
            head: GitHead::Branch {
                name: "master".to_string(),
                commit: None,
            },
            heads: BTreeMap::new(),
            remotes: BTreeMap::new(),
        };
        for (branch, id) in variant_remote_refs {
            r.remotes
                .insert(format!("refs/remotes/origin/{branch}"), (*id).to_string());
        }
        (f, c, r)
    }

    #[test]
    fn classify_adoptable_g6() {
        // The template and the production variants leave one shape.
        for refs_of in [
            &[("main", SHA_A), ("production", SHA_B)][..],
            &[("production", SHA_B), ("main", SHA_A)][..],
        ] {
            let (f, c, r) = g6_partial(refs_of);
            let state = classify(&f, Some(&c), Some(&r));
            assert_eq!(state.kind, GitStateKind::Adoptable, "{state:?}");
            assert_eq!(state.reason, None);
            assert_eq!(state.kind.as_str(), "adoptable");
            assert_eq!(
                state.tracked, None,
                "an unborn HEAD names no tracked branch"
            );
            assert_eq!(
                state.remote.as_ref().map(|r| r.name.as_str()),
                Some("origin")
            );
            assert_eq!(
                state.remote_refs.get("production").map(String::as_str),
                Some(SHA_B)
            );
            assert_eq!(state.remote_refs.len(), 2);
            assert!(!format!("{state:?}").contains("secret"));
        }
    }

    #[test]
    fn classify_unborn_not_adoptable() {
        // A local branch beside an unborn HEAD.
        let (f, c, mut r) = g6_partial(&[("production", SHA_B)]);
        r.heads
            .insert("refs/heads/production".to_string(), SHA_A.to_string());
        let state = classify(&f, Some(&c), Some(&r));
        assert_eq!(reason(&state), "unborn_head");
        // An index beside an unborn HEAD.
        let (mut f, c, r) = g6_partial(&[("production", SHA_B)]);
        f.index_present = true;
        let state = classify(&f, Some(&c), Some(&r));
        assert_eq!(reason(&state), "unborn_head");
        // The earlier reasons still win over adoption.
        let (mut f, c, r) = g6_partial(&[("production", SHA_B)]);
        f.markers.push("MERGE_HEAD".to_string());
        assert_eq!(reason(&classify(&f, Some(&c), Some(&r))), "mid_operation");
        let (f, _, r) = g6_partial(&[("production", SHA_B)]);
        assert_eq!(
            reason(&classify(&f, Some(&config(&["a", "b"], None)), Some(&r))),
            "multiple_remotes"
        );
        assert_eq!(
            reason(&classify(&f, Some(&config(&[], None)), Some(&r))),
            "no_remote"
        );
    }

    #[test]
    fn branch_names_are_checked_purely() {
        for ok in ["production", "main", "release/1.2", "a-b_c.d", "v1"] {
            assert_eq!(branch_name_problem(ok), None, "{ok}");
        }
        for bad in [
            "",
            "..",
            "a..b",
            "-x",
            "--upload-pack=x",
            "/x",
            "x/",
            "a//b",
            "x.",
            "x.lock",
            "a/b.lock",
            "a/.hidden",
            ".x",
            "a b",
            "a:b",
            "a~1",
            "a^",
            "a?b",
            "a*b",
            "a[b",
            "a\\b",
            "a@{b",
            "x\n",
            "\u{e9}",
        ] {
            assert!(branch_name_problem(bad).is_some(), "{bad:?}");
        }
        assert!(branch_name_problem(&"a".repeat(201)).is_some());
    }

    #[test]
    fn classify_absent() {
        let state = classify(&files(GitEntry::Absent), None, None);
        assert_eq!(state.kind, GitStateKind::Absent);
        assert_eq!(state.reason, None);
        assert_eq!(state.kind.as_str(), "absent");
    }

    #[test]
    fn redact_url_cases() {
        assert_eq!(
            redact_url("https://user:pass@host/org/repo.git"),
            "https://host/org/repo.git"
        );
        assert_eq!(
            redact_url("https://token@host:8443/org/repo.git"),
            "https://host:8443/org/repo.git"
        );
        assert_eq!(redact_url("user@host:org/repo.git"), "host:org/repo.git");
        assert_eq!(
            redact_url("https://host/org/repo.git"),
            "https://host/org/repo.git"
        );
        assert_eq!(redact_url("host:org/repo.git"), "host:org/repo.git");
        assert_eq!(redact_url("/srv/git/repo.git"), "/srv/git/repo.git");
        assert_eq!(
            redact_text("fatal: unable to access 'https://u:p@host/org/repo.git/': nope"),
            "fatal: unable to access 'https://host/org/repo.git/': nope"
        );
    }

    /// An apostrophe is legal in URI userinfo: it must not split the URL
    /// before its credentials are removed.
    #[test]
    fn redact_text_apostrophe_in_userinfo() {
        for text in [
            "fatal: unable to access 'https://alice:pa'ss@host/org/repo.git/': nope",
            "https://alice:pa'ss@host/org/repo.git",
            "\"https://al'ice:pass@host\": refused",
        ] {
            let out = redact_text(text);
            assert!(!out.contains("pa'ss") && !out.contains("ss@"), "{out}");
            assert!(!out.contains("alice") && !out.contains("al'ice"), "{out}");
            assert!(out.contains("host"), "{out}");
        }
        assert_eq!(
            redact_text("fatal: unable to access 'https://alice:pa'ss@host/org/repo.git/': nope"),
            "fatal: unable to access 'https://host/org/repo.git/': nope"
        );
    }

    #[test]
    fn url_key_cases() {
        let want = "host.example/org/repo";
        for url in [
            "https://host.example/org/repo",
            "HTTPS://Host.Example/org/repo.git",
            "https://user:pass@host.example/org/repo.git/",
            "ssh://git@host.example/org/repo.git",
            "git@host.example:org/repo.git",
            "host.example:org/repo",
        ] {
            assert_eq!(url_key(url), want, "{url}");
        }
        assert_ne!(url_key("https://host.example/org/other"), want);
    }

    #[test]
    fn caps_checked_first() {
        let mut f = files(GitEntry::Directory);
        assert!(f.check_caps().is_ok());
        f.config = Some(vec![b'x'; GIT_SMALL_FILE_MAX + 1]);
        let err = f.check_caps().expect_err("config over the cap");
        assert_eq!(err.code(), "deploy_preflight_failed");

        let mut f = files(GitEntry::Directory);
        f.loose_refs = (0..=GIT_LOOSE_REFS_MAX)
            .map(|i| (format!("refs/heads/b{i}"), SHA_A.as_bytes().to_vec()))
            .collect();
        assert!(f.check_caps().is_err());

        let mut f = files(GitEntry::Directory);
        f.packed_refs = Some(vec![b'x'; GIT_PACKED_REFS_MAX + 1]);
        assert!(f.check_caps().is_err());
    }
}
