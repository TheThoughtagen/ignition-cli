//! `impl ProjectHost for SftpTransport` (Phase 25, 25-08): the host half of
//! `deploy projects` on a gateway reached over ssh.
//!
//! ```text
//! <data_dir>/projects/<project>/.git                          probed, never opened
//! ~/ign-deploy/<gateway>/projects/<project>/release.json      the release record
//! ~/ign-deploy/<gateway>/tag-providers/<provider>/record.json the tag record (26-06)
//! ~/ign-deploy/<gateway>/projects/<project>/journal.json      the run journal (26-12)
//! ~/ign-deploy/<gateway>/projects/<project>/pre-image/<name>  the run pre-image (26-12)
//! ```
//!
//! ## The `.git` probe is a `stat`, not a listing (K9)
//!
//! The 25-03 captures (`## C-H before grant`) showed that an sftp listing of a
//! directory the deploy account cannot traverse answers `not found`, the same
//! words as a path that does not exist, and that `test -e` answers false. Both
//! would read a denied `.git` as "no git module here" and let a deploy run
//! over a project the module still manages (research Pitfall 3, T-25-20). A
//! GNU `stat` names the cause (`No such file or directory` against
//! `Permission denied`), so the Linux probe is one fixed `sh` script over the
//! hardened exec channel that `stat`s the three levels and reports a word per
//! level:
//!
//! | word | meaning |
//! |---|---|
//! | `dir`, `file`, `symlink`, `other` | the entry exists; a symlink is reported, never followed (`stat` without `-L`) |
//! | `none` | `stat` said `No such file or directory` and nothing else |
//! | `skip` | not asked: an earlier level was absent or not a directory |
//!
//! Any other `stat` failure (denied, I/O error, not a directory, no `stat`)
//! makes the script exit non-zero with `stat`'s message on stderr, which the
//! shell turns into `transport_failed`. The script is fixed text; the three
//! paths and the interpreter name travel as separate single-quoted arguments
//! ([`RemoteCommand::posix`]), so no host or repo value is ever interpreted
//! by a shell.
//!
//! ## Windows hosts: `Get-Item -Force`
//!
//! A Windows node runs [`winps::GIT_ENTRY`](crate::deploy::transport::winps::GIT_ENTRY)
//! through `encoded_command`. It asks the same three levels with
//! `Get-Item -LiteralPath ... -Force`: `-Force` sees a Hidden `.git` (the
//! attribute `git init` and the Git module's own directories carry on
//! Windows), `Test-Path` and `[IO.Directory]::Exists` read a denied entry as
//! false (25-03, `## Host grant and prod git.yaml (25-03)`), and only an
//! `ObjectNotFound` error is `none`. A reparse point (junction or symlink) is
//! present and is never opened. The sftp listing is not used for the probe on
//! either family. The data directory and project name become native paths
//! here and travel as parameters of the fixed script, never in its text.
//!
//! ## The release record write (25-13)
//!
//! The one write `deploy projects` makes to a gateway host. The target is
//! `~/ign-deploy/<gateway>/projects/<project>/release.json` in the deploy
//! account's home, the file [`read_release_record`](ProjectHost::read_release_record)
//! reads, and nothing under the gateway's data directory (Decision B(i)). The
//! order is fixed:
//!
//! 1. Both names pass the checks the read applies, and the bytes must parse as
//!    a [`ReleaseRecord`] for that gateway and project.
//! 2. Every level from the home down is listed. A symlink or a non-directory
//!    on the way, a `release.json` that is not a regular file, or a leftover
//!    file of the temporary name refuses `transport_failed` naming the
//!    location, before any byte is written (T-25-41).
//! 3. Missing levels are made with sftp `mkdir`, shallowest first.
//! 4. The bytes go to `release.json.tmp-<run_id>` beside the record (the run id
//!    is the record's own, already held to `[A-Za-z0-9._-]{1,128}` by the
//!    parse), then replace the record in one atomic step: `mv -f --` (a
//!    `rename(2)`) on a Linux host, and on a Windows one the fixed
//!    `MoveFileEx(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)` script
//!    (`winps::RECORD_REPLACE`, 25-20; `Move-Item -Force` deletes before it
//!    renames on PowerShell 5.1 and was retired, G-25-4). A reader sees the
//!    old record or the new one, never a missing or half-written one (T-25-40).
//!    That is the guarantee of the atomic rename of NTFS on Windows and of
//!    `rename(2)` on Linux: the replace is atomic for readers and it is not a
//!    durable write (the temporary file is not flushed first). A sharing
//!    violation (Win32 error 32) or an access denial (Win32 error 5) is retried
//!    `RENAME_RETRIES` times a second apart, and error 5 also covers a
//!    permanent ACL denial, which therefore shows after about three seconds. A
//!    script that cannot run (not `FullLanguage`, or `Add-Type` failing) fails
//!    closed, and every failure of the script names its cause. A replace still
//!    blocked after the retries fails with `transport_failed` naming that
//!    cause, as step 5 describes.
//! 5. A failure after the `put` removes the temporary file and keeps the
//!    previous record untouched; after a verified import the run then ends
//!    with `record_written: false` and a warning.
//!
//! ## The tag record (26-06)
//!
//! `~/ign-deploy/<gateway>/tag-providers/<provider>/record.json` is read and
//! written by the same two helpers as the release record
//! (`read_home_file` and `write_home_record`), with [`TAG_RECORD_MAX_BYTES`]
//! and `record.json.tmp-<run_id>`. Its names pass
//! `tag_record_names_problem_on` for this host's family before any session:
//! a provider name holding an sftp glob character would be skipped by the
//! lenient home listing and so read as absent, which must never happen to a
//! record that decides what may be deleted (T-26-21).
//!
//! ## The project run files (26-12)
//!
//! `journal.json` and the fixed-name `pre-image/` files beside the release
//! record go through the same `read_home_file` and `write_home_record`, with
//! `<name>.tmp-<run_id>` siblings. `clear_journal` lists every level first
//! (a symlink or a non-regular journal refuses) and then removes the one
//! file; an absent journal is a success.

use super::{Budget, EntryKind, FoundFile, ReadBase, SftpTransport, windows_native_path};
use crate::deploy::apply::run_store::RunId;
use crate::deploy::component_problem;
use crate::deploy::git_advance::{GitWrite, GitWriteName};
use crate::deploy::git_state::{
    GIT_LOG_TAIL_MAX, GIT_LOOSE_REFS_MAX, GIT_MARKERS, GIT_PACKED_REFS_MAX, GitFiles,
};
use crate::deploy::ledger::{
    LedgerRoot, RELEASE_RECORD_FILE, RELEASE_RECORD_MAX_BYTES, git_save_location, git_save_rel,
    pre_image_location, pre_image_rel, project_journal_location, project_journal_rel,
    release_record_location, release_record_rel, tag_record_location, tag_record_rel,
};
use std::path::Path;

use crate::deploy::git_objects::{
    GIT_OBJECTS_FETCH_MAX_BYTES, GIT_OBJECTS_LIST_MAX_ENTRIES, GIT_OBJECTS_LOOSE_MAX,
    GIT_OBJECTS_PACKS_MAX, GitObjectEntry, GitObjectEntryKind, GitObjectFile, GitObjectList,
    check_object_rel,
};
use crate::deploy::project_run::{
    PRE_IMAGE_MAX_BYTES, PROJECT_JOURNAL_FILE, PROJECT_JOURNAL_MAX_BYTES, ProjectJournal,
};
use crate::deploy::projects::ReleaseRecord;
use crate::deploy::tag_record::{TAG_RECORD_FILE, TAG_RECORD_MAX_BYTES, TagRecord};
use crate::deploy::transport::batch::RENAME_RETRIES;
use crate::deploy::transport::projects::{
    GitEntry, GitHostRead, GitManifest, GitSize, GitStateWriter, ImportDir, ProjectHost,
    ProjectRunWriter, ReleaseRecordWriter, TagRecordWriter, check_pre_image_write, git_probe_name,
    project_run_names_problem_on, tag_record_names_problem_on,
};
use crate::deploy::transport::remote::{RemoteCommand, SftpOp, safe_remote_path};
use crate::deploy::transport::winps::{LONG_PATH_PREFIX, WinScript, windows_component_problem};
use crate::deploy::yamlio::YAML_MAX_BYTES;
use crate::error::CoreError;

/// The fixed `sh -c` text of the POSIX probe, one line (a remote command
/// argument may hold no control character). `$1` is `<data>/projects`, `$2`
/// the project directory and `$3` its `.git`. `LC_ALL=C` pins the English
/// `strerror` text the script matches. A `stat` failure other than a clean
/// `No such file or directory` is printed to stderr and exits 3.
pub(super) const GIT_PROBE_SH: &str = concat!(
    "LC_ALL=C; export LC_ALL; ",
    "t() { if o=$(stat -c %F -- \"$1\" 2>&1); then ",
    "case \"$o\" in directory) r=dir;; \"regular file\"|\"regular empty file\") r=file;; ",
    "\"symbolic link\") r=symlink;; *) r=other;; esac; ",
    "else case \"$o\" in *\": No such file or directory\") r=none;; ",
    "*) printf \"%s\\n\" \"$o\" >&2; exit 3;; esac; fi; }; ",
    "t \"$1\"; a=$r; b=skip; c=skip; ",
    "if [ \"$a\" = dir ]; then t \"$2\"; b=$r; ",
    "if [ \"$b\" = dir ]; then t \"$3\"; c=$r; fi; fi; ",
    "printf 'IGN-RESULT:{\"projects\":\"%s\",\"project\":\"%s\",\"git\":\"%s\"}\\n' \"$a\" \"$b\" \"$c\""
);

/// The `$0` the probe runs under, so a `stat` message names the script.
const GIT_PROBE_NAME: &str = "ign-git-probe";

// The git handover scripts (Phase 27, 27-08). Each is one line of fixed text;
// every path and name travels as a positional argument. Generated from one
// reviewed source and kept here as `concat!` lines so a test can run the very
// same text under the local `sh`.

pub(super) const GIT_MANIFEST_SH: &str = concat!(
    "LC_ALL=C; export LC_ALL; ",
    "for c in find wc awk sort tr ls; do command -v \"$c\" >/dev/null 2>&1 || { printf 'the host has no %s\\n' \"$c\" >&2; exit 3; }; done; ",
    "if command -v sha256sum >/dev/null 2>&1; then H=sha256sum; ",
    "elif command -v shasum >/dev/null 2>&1; then H='shasum -a 256'; ",
    "else echo 'the host has neither sha256sum nor shasum' >&2; exit 3; fi; ",
    "pr() { if o=$(ls -ld -- \"$1\" 2>&1); then return 0; fi; ",
    "case $o in *'No such file or directory') return 1;; esac; ",
    "printf '%s\\n' \"$o\" >&2; exit 3; }; ",
    "wk() { [ -d \"$1\" ] || { printf '%s is not a directory\\n' \"$1\" >&2; exit 4; }; ",
    "cd -- \"$1\" || exit 3; OI=$IFS; IFS=/; ",
    "for c in $2; do [ -d \"$c\" ] && [ ! -L \"$c\" ] || { printf '%s is not a real directory\\n' \"$c\" >&2; exit 4; }; ",
    "cd -- \"$c\" || exit 3; done; IFS=$OI; }; ",
    "ml() { while IFS= read -r l; do case $l in *' ./'*) s=${l%% ./*}; p=${l#*' ./'}; s=$((s)); ",
    "case $p in objects/*) printf '%s\\001%s\\n' \"$p\" \"$s\";; ",
    "*) h=$($H -- \"./$p\") || exit 3; printf '%s\\001%s\\n' \"$p\" \"${h%% *}\";; esac;; esac; done; }; ",
    "mf() { cd -- \"$1\" || exit 3; ",
    "x=$(find -P . \\( ! -type f ! -type d -o -name '*[[:cntrl:]]*' \\) -print 2>&1) || { printf '%s\\n' \"$x\" >&2; exit 3; }; ",
    "[ -z \"$x\" ] || { echo 'the tree holds a symlink, a special file or a name with a control character' >&2; exit 4; }; ",
    "a=$(find -P . -type f -exec wc -c {} + 2>&1) || { printf '%s\\n' \"$a\" >&2; exit 3; }; ",
    "w=$(printf '%s\\n' \"$a\" | awk '/ \\.\\// {n++; s=$0; sub(/^ */,\"\",s); b+=s+0} END {printf \"%d %.0f\", n, b}') || exit 3; ",
    "q=$(printf '%s\\n' \"$a\" | ml) || exit 3; ",
    "if [ -n \"$q\" ]; then d=$(printf '%s\\n' \"$q\" | sort | tr '\\001' '\\000' | $H) || exit 3; else d=$(: | $H) || exit 3; fi; ",
    "printf '%s %s\\n' \"$w\" \"${d%% *}\"; }; ",
    "wk \"$1\" \"$2\"; m=$(mf \"$PWD\") || exit $?; set -- $m; ",
    "printf 'IGN-RESULT:{\"files\":%s,\"bytes\":%s,\"digest\":\"%s\"}\\n' \"$1\" \"$2\" \"$3\"",
);

pub(super) const GIT_SIZE_SH: &str = concat!(
    "LC_ALL=C; export LC_ALL; ",
    "for c in find ls awk; do command -v \"$c\" >/dev/null 2>&1 || { printf 'the host has no %s\\n' \"$c\" >&2; exit 3; }; done; ",
    "wk() { [ -d \"$1\" ] || { printf '%s is not a directory\\n' \"$1\" >&2; exit 4; }; ",
    "cd -- \"$1\" || exit 3; OI=$IFS; IFS=/; ",
    "for c in $2; do [ -d \"$c\" ] && [ ! -L \"$c\" ] || { printf '%s is not a real directory\\n' \"$c\" >&2; exit 4; }; ",
    "cd -- \"$c\" || exit 3; done; IFS=$OI; }; ",
    "wk \"$1\" \"$2\"; ",
    "x=$(find -P . \\( ! -type f ! -type d -o -name '*[[:cntrl:]]*' \\) -print 2>&1) || { printf '%s\\n' \"$x\" >&2; exit 3; }; ",
    "[ -z \"$x\" ] || { echo 'the tree holds a symlink, a special file or a name with a control character' >&2; exit 4; }; ",
    "a=$(find -P . -type f -exec ls -ln {} + 2>&1) || { printf '%s\\n' \"$a\" >&2; exit 3; }; ",
    "w=$(printf '%s\\n' \"$a\" | awk '/^-/ {n++; b+=$5} END {printf \"%d %.0f\", n, b}') || exit 3; set -- $w; ",
    "printf 'IGN-RESULT:{\"files\":%s,\"bytes\":%s}\\n' \"$1\" \"$2\"",
);

/// The most object files a listing reports before it says `more` (a cut-off
/// listing is refused, never read as the whole repo): one past the loose and
/// pack caps together, so a repo over either cap is seen as over it.
const GIT_OBJECTS_REPORT_MAX: usize = GIT_OBJECTS_LOOSE_MAX + 2 * GIT_OBJECTS_PACKS_MAX + 1;

/// The object files of `<data>/projects/<p>/.git/objects` (27-11, D-24): one
/// line of fixed text, `$1` the data directory, `$2` the path
/// `projects/<p>/.git/objects` (every level a real directory), `$3` the most
/// entries it may walk, `$4` the most it reports. A symlink or a name with a
/// control character anywhere in the two levels below `objects/` refuses
/// (exit 4); only the files of the fixed shapes are reported, as the lines
/// `size objects/<name>` of one JSON string, `{"t":..,"n":..,"more":..}`.
/// Nothing is opened but the sizes (`wc -c` on a regular file asks the file
/// system), and no name is interpreted by a shell: the names are read from the
/// output of `find` by `awk`.
pub(super) const GIT_OBJECTS_SH: &str = concat!(
    "LC_ALL=C; export LC_ALL; ",
    "for c in find wc awk; do command -v \"$c\" >/dev/null 2>&1 || { printf 'the host has no %s\\n' \"$c\" >&2; exit 3; }; done; ",
    "wk() { [ -d \"$1\" ] || { printf '%s is not a directory\\n' \"$1\" >&2; exit 4; }; ",
    "cd -- \"$1\" || exit 3; OI=$IFS; IFS=/; ",
    "for c in $2; do [ -d \"$c\" ] && [ ! -L \"$c\" ] || { printf '%s is not a real directory\\n' \"$c\" >&2; exit 4; }; ",
    "cd -- \"$c\" || exit 3; done; IFS=$OI; }; ",
    "wk \"$1\" \"$2\"; ",
    "x=$(find -P . -maxdepth 2 \\( -type l -o -name '*[[:cntrl:]]*' \\) -print 2>&1) || { printf '%s\\n' \"$x\" >&2; exit 3; }; ",
    "[ -z \"$x\" ] || { echo 'objects holds a symlink or a name with a control character' >&2; exit 4; }; ",
    "a=$(find -P . -maxdepth 2 -type f -exec wc -c {} + 2>&1) || { printf '%s\\n' \"$a\" >&2; exit 3; }; ",
    "j=$(printf '%s\\n' \"$a\" | awk -v lim=\"$3\" -v cap=\"$4\" '",
    r#"BEGIN{printf "{\"t\":\""} "#,
    r#"/ \.\// {i=index($0," ./"); s=substr($0,1,i-1)+0; p=substr($0,i+3); n++; "#,
    r#"if(n>lim){print "objects holds more entries than a read may walk" > "/dev/stderr"; bad=4; exit} "#,
    r#"if(p ~ /[\\:]/){print "an object file has a name that is not a plain name" > "/dev/stderr"; bad=4; exit} "#,
    r#"if((length(p)==41 && p ~ /^[0-9a-f][0-9a-f]\/[0-9a-f]+$/) || ((length(p)==55 || length(p)==54) && p ~ /^pack\/pack-[0-9a-f]+\.(pack|idx)$/)) "#,
    r#"{if(e>=cap){m=1}else{e++; printf "%.0f objects/%s\\n", s, p}}} "#,
    r#"END{printf "\",\"n\":%d,\"more\":%d}\n", e, m; exit bad}"#,
    "') || exit $?; ",
    "printf 'IGN-RESULT:%s\\n' \"$j\"",
);

pub(super) const GIT_STATE_SH: &str = concat!(
    "LC_ALL=C; export LC_ALL; ",
    "for c in find wc head tail base64 tr ls; do command -v \"$c\" >/dev/null 2>&1 || { printf 'the host has no %s\\n' \"$c\" >&2; exit 3; }; done; ",
    "pr() { if o=$(ls -ld -- \"$1\" 2>&1); then return 0; fi; ",
    "case $o in *'No such file or directory') return 1;; esac; ",
    "printf '%s\\n' \"$o\" >&2; exit 3; }; ",
    "wk() { [ -d \"$1\" ] || { printf '%s is not a directory\\n' \"$1\" >&2; exit 4; }; ",
    "cd -- \"$1\" || exit 3; OI=$IFS; IFS=/; ",
    "for c in $2; do [ -d \"$c\" ] && [ ! -L \"$c\" ] || { printf '%s is not a real directory\\n' \"$c\" >&2; exit 4; }; ",
    "cd -- \"$c\" || exit 3; done; IFS=$OI; }; ",
    "wk \"$1\" \"$2\"; shift 2; set -f; NL=$(printf '\\nx'); NL=${NL%x}; ",
    "rd() { if [ -L \"$1\" ]; then printf '%s is a symlink\\n' \"$1\" >&2; exit 4; fi; ",
    "if pr \"$1\"; then [ -f \"$1\" ] || { printf '%s is not a regular file\\n' \"$1\" >&2; exit 4; }; ",
    "[ -r \"$1\" ] || { printf '%s cannot be read\\n' \"$1\" >&2; exit 3; }; ",
    "z=$(wc -c < \"$1\") || exit 3; z=$((z)); ",
    "b=$($3 -c \"$2\" < \"$1\" | base64 | tr -d '\\n'); printf '{\"n\":%s,\"b\":\"%s\"}' \"$z\" \"$b\"; ",
    "else printf null; fi; }; ",
    "printf 'IGN-RESULT:{\"head\":'; rd HEAD 65537 head; ",
    "printf ',\"config\":'; rd config 65537 head; ",
    "printf ',\"packed\":'; rd packed-refs 8388609 head; ",
    "printf ',\"log\":'; rd logs/HEAD 65536 tail; ",
    "printf ',\"index\":'; if pr index; then printf true; else printf false; fi; ",
    "printf ',\"markers\":['; s=''; for m in \"$@\"; do if pr \"$m\"; then printf '%s\"%s\"' \"$s\" \"$m\"; s=,; fi; done; printf ']'; ",
    "printf ',\"refs\":['; s=''; c=0; ",
    "for d in refs/heads refs/remotes; do if pr \"$d\"; then ",
    "[ -d \"$d\" ] && [ ! -L \"$d\" ] || { printf '%s is not a real directory\\n' \"$d\" >&2; exit 4; }; ",
    "x=$(find -P \"$d\" \\( ! -type f ! -type d -o -name '*[[:cntrl:]]*' \\) -print 2>&1) || { printf '%s\\n' \"$x\" >&2; exit 3; }; ",
    "[ -z \"$x\" ] || { printf '%s holds a symlink, a special file or a name with a control character\\n' \"$d\" >&2; exit 4; }; ",
    "L=$(find -P \"$d\" -type f 2>&1) || { printf '%s\\n' \"$L\" >&2; exit 3; }; ",
    "IFS=$NL; for r in $L; do c=$((c+1)); [ \"$c\" -le 4097 ] || break; ",
    "[ -r \"$r\" ] || { printf '%s cannot be read\\n' \"$r\" >&2; exit 3; }; ",
    "z=$(wc -c < \"$r\") || exit 3; z=$((z)); ",
    "b=$(head -c 65537 < \"$r\" | base64 | tr -d '\\n'); p=$(printf '%s' \"$r\" | base64 | tr -d '\\n'); ",
    "printf '%s{\"p\":\"%s\",\"n\":%s,\"b\":\"%s\"}' \"$s\" \"$p\" \"$z\" \"$b\"; s=,; done; unset IFS; ",
    "fi; done; printf ']}\\n'",
);

pub(super) const GIT_DIGEST_SH: &str = concat!(
    "LC_ALL=C; export LC_ALL; ",
    "for c in ls; do command -v \"$c\" >/dev/null 2>&1 || { printf 'the host has no %s\\n' \"$c\" >&2; exit 3; }; done; ",
    "if command -v sha256sum >/dev/null 2>&1; then H=sha256sum; ",
    "elif command -v shasum >/dev/null 2>&1; then H='shasum -a 256'; ",
    "else echo 'the host has neither sha256sum nor shasum' >&2; exit 3; fi; ",
    "pr() { if o=$(ls -ld -- \"$1\" 2>&1); then return 0; fi; ",
    "case $o in *'No such file or directory') return 1;; esac; ",
    "printf '%s\\n' \"$o\" >&2; exit 3; }; ",
    "wk() { [ -d \"$1\" ] || { printf '%s is not a directory\\n' \"$1\" >&2; exit 4; }; ",
    "cd -- \"$1\" || exit 3; OI=$IFS; IFS=/; ",
    "for c in $2; do [ -d \"$c\" ] && [ ! -L \"$c\" ] || { printf '%s is not a real directory\\n' \"$c\" >&2; exit 4; }; ",
    "cd -- \"$c\" || exit 3; done; IFS=$OI; }; ",
    "wk \"$1\" \"$2\"; f=$3; ",
    "if [ -L \"$f\" ]; then printf '%s is a symlink\\n' \"$f\" >&2; exit 4; fi; ",
    "if pr \"$f\"; then [ -f \"$f\" ] || { printf '%s is not a regular file\\n' \"$f\" >&2; exit 4; }; ",
    "h=$($H -- \"./$f\") || exit 3; printf 'IGN-RESULT:{\"digest\":\"%s\"}\\n' \"${h%% *}\"; ",
    "else printf 'IGN-RESULT:{\"digest\":null}\\n'; fi",
);

pub(super) const GIT_SAVE_SH: &str = concat!(
    "LC_ALL=C; export LC_ALL; ",
    "for c in find wc awk sort tr ls cp rm mkdir; do command -v \"$c\" >/dev/null 2>&1 || { printf 'the host has no %s\\n' \"$c\" >&2; exit 3; }; done; ",
    "if command -v sha256sum >/dev/null 2>&1; then H=sha256sum; ",
    "elif command -v shasum >/dev/null 2>&1; then H='shasum -a 256'; ",
    "else echo 'the host has neither sha256sum nor shasum' >&2; exit 3; fi; ",
    "pr() { if o=$(ls -ld -- \"$1\" 2>&1); then return 0; fi; ",
    "case $o in *'No such file or directory') return 1;; esac; ",
    "printf '%s\\n' \"$o\" >&2; exit 3; }; ",
    "wk() { [ -d \"$1\" ] || { printf '%s is not a directory\\n' \"$1\" >&2; exit 4; }; ",
    "cd -- \"$1\" || exit 3; OI=$IFS; IFS=/; ",
    "for c in $2; do [ -d \"$c\" ] && [ ! -L \"$c\" ] || { printf '%s is not a real directory\\n' \"$c\" >&2; exit 4; }; ",
    "cd -- \"$c\" || exit 3; done; IFS=$OI; }; ",
    "mk() { [ -d \"$1\" ] || { printf '%s is not a directory\\n' \"$1\" >&2; exit 4; }; ",
    "cd -- \"$1\" || exit 3; OI=$IFS; IFS=/; ",
    "for c in $2; do if [ -L \"$c\" ]; then printf '%s is a symlink\\n' \"$c\" >&2; exit 4; ",
    "elif [ -d \"$c\" ]; then :; ",
    "elif pr \"$c\"; then printf '%s is not a directory\\n' \"$c\" >&2; exit 4; ",
    "else mkdir -- \"$c\" || exit 3; fi; cd -- \"$c\" || exit 3; done; IFS=$OI; }; ",
    "ml() { while IFS= read -r l; do case $l in *' ./'*) s=${l%% ./*}; p=${l#*' ./'}; s=$((s)); ",
    "case $p in objects/*) printf '%s\\001%s\\n' \"$p\" \"$s\";; ",
    "*) h=$($H -- \"./$p\") || exit 3; printf '%s\\001%s\\n' \"$p\" \"${h%% *}\";; esac;; esac; done; }; ",
    "mf() { cd -- \"$1\" || exit 3; ",
    "x=$(find -P . \\( ! -type f ! -type d -o -name '*[[:cntrl:]]*' \\) -print 2>&1) || { printf '%s\\n' \"$x\" >&2; exit 3; }; ",
    "[ -z \"$x\" ] || { echo 'the tree holds a symlink, a special file or a name with a control character' >&2; exit 4; }; ",
    "a=$(find -P . -type f -exec wc -c {} + 2>&1) || { printf '%s\\n' \"$a\" >&2; exit 3; }; ",
    "w=$(printf '%s\\n' \"$a\" | awk '/ \\.\\// {n++; s=$0; sub(/^ */,\"\",s); b+=s+0} END {printf \"%d %.0f\", n, b}') || exit 3; ",
    "q=$(printf '%s\\n' \"$a\" | ml) || exit 3; ",
    "if [ -n \"$q\" ]; then d=$(printf '%s\\n' \"$q\" | sort | tr '\\001' '\\000' | $H) || exit 3; else d=$(: | $H) || exit 3; fi; ",
    "printf '%s %s\\n' \"$w\" \"${d%% *}\"; }; ",
    "umask 077; wk \"$1\" \"$2\"; src=$PWD; before=$(mf \"$src\") || exit $?; ",
    "mk \"$3\" \"$4\"; if pr repo; then echo 'the save already exists; a save is never overwritten' >&2; exit 4; fi; ",
    "mkdir -m 700 -- repo || { echo 'the save already exists; a save is never overwritten' >&2; exit 4; }; ",
    "sg() { [ ! -L \"$1\" ] && [ -d \"$1\" ] || { printf '%s is not a real directory\\n' \"$1\" >&2; exit 4; }; }; ",
    "sg repo; cp -a -- \"$src\"/. repo/ || { rm -rf -- repo; exit 3; }; ",
    "sg repo; after=$(mf repo) || { r=$?; rm -rf -- repo; exit $r; }; ",
    "[ \"$before\" = \"$after\" ] || { rm -rf -- repo; echo 'the saved copy does not match the .git it was taken from' >&2; exit 5; }; ",
    "set -- $after; ",
    "printf 'IGN-RESULT:{\"files\":%s,\"bytes\":%s,\"digest\":\"%s\"}\\n' \"$1\" \"$2\" \"$3\"",
);

pub(super) const GIT_RESTORE_SH: &str = concat!(
    "LC_ALL=C; export LC_ALL; ",
    "for c in find wc awk sort tr ls cp mv rm id mkdir; do command -v \"$c\" >/dev/null 2>&1 || { printf 'the host has no %s\\n' \"$c\" >&2; exit 3; }; done; ",
    "if command -v sha256sum >/dev/null 2>&1; then H=sha256sum; ",
    "elif command -v shasum >/dev/null 2>&1; then H='shasum -a 256'; ",
    "else echo 'the host has neither sha256sum nor shasum' >&2; exit 3; fi; ",
    "pr() { if o=$(ls -ld -- \"$1\" 2>&1); then return 0; fi; ",
    "case $o in *'No such file or directory') return 1;; esac; ",
    "printf '%s\\n' \"$o\" >&2; exit 3; }; ",
    "wk() { [ -d \"$1\" ] || { printf '%s is not a directory\\n' \"$1\" >&2; exit 4; }; ",
    "cd -- \"$1\" || exit 3; OI=$IFS; IFS=/; ",
    "for c in $2; do [ -d \"$c\" ] && [ ! -L \"$c\" ] || { printf '%s is not a real directory\\n' \"$c\" >&2; exit 4; }; ",
    "cd -- \"$c\" || exit 3; done; IFS=$OI; }; ",
    "ml() { while IFS= read -r l; do case $l in *' ./'*) s=${l%% ./*}; p=${l#*' ./'}; s=$((s)); ",
    "case $p in objects/*) printf '%s\\001%s\\n' \"$p\" \"$s\";; ",
    "*) h=$($H -- \"./$p\") || exit 3; printf '%s\\001%s\\n' \"$p\" \"${h%% *}\";; esac;; esac; done; }; ",
    "mf() { cd -- \"$1\" || exit 3; ",
    "x=$(find -P . \\( ! -type f ! -type d -o -name '*[[:cntrl:]]*' \\) -print 2>&1) || { printf '%s\\n' \"$x\" >&2; exit 3; }; ",
    "[ -z \"$x\" ] || { echo 'the tree holds a symlink, a special file or a name with a control character' >&2; exit 4; }; ",
    "a=$(find -P . -type f -exec wc -c {} + 2>&1) || { printf '%s\\n' \"$a\" >&2; exit 3; }; ",
    "w=$(printf '%s\\n' \"$a\" | awk '/ \\.\\// {n++; s=$0; sub(/^ */,\"\",s); b+=s+0} END {printf \"%d %.0f\", n, b}') || exit 3; ",
    "q=$(printf '%s\\n' \"$a\" | ml) || exit 3; ",
    "if [ -n \"$q\" ]; then d=$(printf '%s\\n' \"$q\" | sort | tr '\\001' '\\000' | $H) || exit 3; else d=$(: | $H) || exit 3; fi; ",
    "printf '%s %s\\n' \"$w\" \"${d%% *}\"; }; ",
    "wk \"$1\" \"$2\"; proj=$PWD; ",
    "if pr \"$proj/.git\"; then printf '%s/.git exists; it is never replaced\\n' \"$2\" >&2; exit 4; fi; ",
    "wk \"$3\" \"$4\"; [ -d repo ] && [ ! -L repo ] || { echo 'the save has no repo directory' >&2; exit 4; }; ",
    "saved=$(mf repo) || exit $?; stage=$proj/.ign-restore-$5; ",
    "sg() { [ ! -L \"$1\" ] && [ -d \"$1\" ] || { printf '%s is not a real directory\\n' \"$1\" >&2; exit 4; }; }; ",
    "mkdir -m 700 -- \"$stage\" || { printf '%s exists or cannot be created; it is left alone\\n' \"$stage\" >&2; exit 4; }; ",
    "sg \"$stage\"; cp -a -- repo/. \"$stage\"/ || { rm -rf -- \"$stage\"; exit 3; }; ",
    "gu=$(ls -ldn -- \"$proj\" | awk '{print $3}'); me=$(id -u); ",
    "if [ -n \"$gu\" ] && [ \"$gu\" != 0 ] && [ \"$gu\" != \"$me\" ]; then ",
    "if ! command -v setfacl >/dev/null 2>&1; then rm -rf -- \"$stage\"; echo 'the gateway account needs write access to the restored .git and the host has no setfacl' >&2; exit 6; fi; ",
    "sg \"$stage\"; setfacl -R -P -m \"u:$gu:rwX\" \"$stage\" && sg \"$stage\" && find -P \"$stage\" -type d -exec setfacl -d -m \"u:$gu:rwX\" {} + || { rm -rf -- \"$stage\"; echo 'setfacl failed on the restored .git' >&2; exit 6; }; ",
    "if command -v getfacl >/dev/null 2>&1; then e=$(getfacl -n -p -- \"$stage/config\" 2>/dev/null | grep \"^user:$gu:\"); ",
    "case $e in \"user:$gu:rw\"*) case $e in *'#effective:r--'*|*'#effective:---'*|*'#effective:-'*) rm -rf -- \"$stage\"; echo 'the gateway account has no effective write access on the restored .git' >&2; exit 6;; esac;; ",
    "*) rm -rf -- \"$stage\"; echo 'the gateway account has no ACL entry on the restored .git' >&2; exit 6;; esac; fi; fi; ",
    "sg \"$stage\"; staged=$(mf \"$stage\") || { r=$?; rm -rf -- \"$stage\"; exit $r; }; ",
    "[ \"$staged\" = \"$saved\" ] || { rm -rf -- \"$stage\"; echo 'the restored copy does not match the save; nothing was restored' >&2; exit 5; }; ",
    "if pr \"$proj/.git\"; then rm -rf -- \"$stage\"; printf '%s/.git appeared; it is never replaced\\n' \"$2\" >&2; exit 4; fi; ",
    "sg \"$stage\"; mv -- \"$stage\" \"$proj/.git\" || { rm -rf -- \"$stage\"; exit 3; }; ",
    "final=$(mf \"$proj/.git\") || exit $?; set -- $final; ",
    "printf 'IGN-RESULT:{\"files\":%s,\"bytes\":%s,\"digest\":\"%s\"}\\n' \"$1\" \"$2\" \"$3\"",
);

pub(super) const GIT_PROBE_W_SH: &str = concat!(
    "LC_ALL=C; export LC_ALL; ",
    "for c in ls rm id awk; do command -v \"$c\" >/dev/null 2>&1 || { printf 'the host has no %s\\n' \"$c\" >&2; exit 3; }; done; ",
    "pr() { if o=$(ls -ld -- \"$1\" 2>&1); then return 0; fi; ",
    "case $o in *'No such file or directory') return 1;; esac; ",
    "printf '%s\\n' \"$o\" >&2; exit 3; }; ",
    // D-G1: when the gateway runs as an account that is neither root nor the
    // deploy account, the restore gives it an explicit named ACL entry; the
    // probe file in `.git` proves that step can work (setfacl and getfacl
    // exist, the entry is accepted, its effective rights are rw-) before
    // anything is written.
    "ag() { [ -n \"$gu\" ] && [ \"$gu\" != 0 ] && [ \"$gu\" != \"$me\" ] || return 0; ",
    "if ! command -v setfacl >/dev/null 2>&1 || ! command -v getfacl >/dev/null 2>&1; then rm -f -- \"$1\"; printf 'the gateway account needs write access to the restored .git and the host has no setfacl or getfacl (checked in %s)\\n' \"$2\" >&2; exit 6; fi; ",
    "setfacl -m \"u:$gu:rw\" \"$1\" 2>/dev/null || { rm -f -- \"$1\"; printf 'setfacl failed on the probe file in %s\\n' \"$2\" >&2; exit 6; }; ",
    "e=$(getfacl -n -p \"$1\" 2>/dev/null | grep \"^user:$gu:\"); ",
    "case $e in \"user:$gu:rw\"*) case $e in *'#effective:r--'*|*'#effective:---'*|*'#effective:-'*) rm -f -- \"$1\"; printf 'the gateway account would have no effective write access on the restored .git (checked in %s)\\n' \"$2\" >&2; exit 6;; esac;; ",
    "*) rm -f -- \"$1\"; printf 'the gateway account has no ACL entry on the probe file in %s\\n' \"$2\" >&2; exit 6;; esac; }; ",
    "pw() { [ -d \"$1\" ] && [ ! -L \"$1\" ] || { printf '%s is not a real directory\\n' \"$2\" >&2; exit 4; }; ",
    "if pr \"$1/$3\"; then printf 'a probe file already exists in %s\\n' \"$2\" >&2; exit 4; fi; ",
    "( set -C; : > \"$1/$3\" ) 2>/dev/null || { printf 'a probe file cannot be created in %s\\n' \"$2\" >&2; exit 3; }; ",
    "if [ \"$4\" = acl ]; then ag \"$1/$3\" \"$2\"; fi; ",
    "rm -f -- \"$1/$3\" || { printf 'the probe file in %s cannot be removed\\n' \"$2\" >&2; exit 3; }; }; ",
    "[ -d \"$1\" ] || { printf '%s is not a directory\\n' \"$1\" >&2; exit 4; }; ",
    "gu=$(ls -ldn -- \"$1/$2\" 2>/dev/null | awk '{print $3}'); me=$(id -u); ",
    "pw \"$1/$2\" \"$2\" \"$3\"; pw \"$1/$2/.git\" \"$2/.git\" \"$3\" acl; printf 'IGN-RESULT:{\"ok\":1}\\n'",
);

pub(super) const GIT_LOCK_SH: &str = concat!(
    "LC_ALL=C; export LC_ALL; ",
    "for c in ls mkdir rm; do command -v \"$c\" >/dev/null 2>&1 || { printf 'the host has no %s\\n' \"$c\" >&2; exit 3; }; done; ",
    "pr() { if o=$(ls -ld -- \"$1\" 2>&1); then return 0; fi; ",
    "case $o in *'No such file or directory') return 1;; esac; ",
    "printf '%s\\n' \"$o\" >&2; exit 3; }; ",
    "wk() { [ -d \"$1\" ] || { printf '%s is not a directory\\n' \"$1\" >&2; exit 4; }; ",
    "cd -- \"$1\" || exit 3; OI=$IFS; IFS=/; ",
    "for c in $2; do [ -d \"$c\" ] && [ ! -L \"$c\" ] || { printf '%s is not a real directory\\n' \"$c\" >&2; exit 4; }; ",
    "cd -- \"$c\" || exit 3; done; IFS=$OI; }; ",
    "umask 022; wk \"$1\" \"$2\"; mode=$3; shift 3; set -f; NL=$(printf '\\nx'); NL=${NL%x}; ",
    "if [ \"$mode\" = unlock ]; then while [ $# -ge 2 ]; do l=$2; shift 2; ",
    "if [ -f \"$l\" ] && [ ! -L \"$l\" ]; then rm -f -- \"$l\" || exit 3; fi; done; printf 'IGN-RESULT:{\"ok\":1}\\n'; exit 0; fi; ",
    "ck() { p=''; OI=$IFS; IFS=/; for c in $1; do p=${p:+$p/}$c; ",
    "if [ -L \"$p\" ]; then printf '%s is a symlink\\n' \"$p\" >&2; exit 4; ",
    "elif [ -d \"$p\" ]; then :; ",
    "elif pr \"$p\"; then printf '%s is not a directory\\n' \"$p\" >&2; exit 4; ",
    "elif [ \"$2\" = m ]; then mkdir -- \"$p\" || exit 3; else break; fi; done; IFS=$OI; }; ",
    "n=$#; k=0; while [ $k -lt $n ]; do t=$1; l=$2; shift 2; set -- \"$@\" \"$t\" \"$l\"; k=$((k+2)); ",
    "case $t in */*) par=${t%/*};; *) par='';; esac; ck \"$par\" c; ",
    "if pr \"$t\"; then [ -f \"$t\" ] && [ ! -L \"$t\" ] || { printf '%s is not a regular file; it is left alone\\n' \"$t\" >&2; exit 4; }; fi; ",
    "if pr \"$l\"; then printf '%s exists; another writer holds it, nothing was written\\n' \"$l\" >&2; exit 4; fi; done; ",
    "made=''; k=0; while [ $k -lt $n ]; do t=$1; l=$2; shift 2; set -- \"$@\" \"$t\" \"$l\"; k=$((k+2)); ",
    "case $t in */*) par=${t%/*};; *) par='';; esac; ck \"$par\" m; ",
    "if ( set -C; : > \"$l\" ) 2>/dev/null; then made=\"$made$l$NL\"; ",
    "else IFS=$NL; for f in $made; do rm -f -- \"$f\"; done; unset IFS; printf '%s exists or cannot be created; nothing was written\\n' \"$l\" >&2; exit 4; fi; done; ",
    "printf 'IGN-RESULT:{\"ok\":1}\\n'",
);

pub(super) const GIT_REPLACE_SH: &str = concat!(
    "LC_ALL=C; export LC_ALL; ",
    "for c in ls mv wc; do command -v \"$c\" >/dev/null 2>&1 || { printf 'the host has no %s\\n' \"$c\" >&2; exit 3; }; done; ",
    "pr() { if o=$(ls -ld -- \"$1\" 2>&1); then return 0; fi; ",
    "case $o in *'No such file or directory') return 1;; esac; ",
    "printf '%s\\n' \"$o\" >&2; exit 3; }; ",
    "wk() { [ -d \"$1\" ] || { printf '%s is not a directory\\n' \"$1\" >&2; exit 4; }; ",
    "cd -- \"$1\" || exit 3; OI=$IFS; IFS=/; ",
    "for c in $2; do [ -d \"$c\" ] && [ ! -L \"$c\" ] || { printf '%s is not a real directory\\n' \"$c\" >&2; exit 4; }; ",
    "cd -- \"$c\" || exit 3; done; IFS=$OI; }; ",
    "wk \"$1\" \"$2\"; shift 2; ",
    "while [ $# -ge 3 ]; do t=$1; l=$2; z=$3; shift 3; ",
    "[ -f \"$l\" ] && [ ! -L \"$l\" ] || { printf '%s is not a regular file\\n' \"$l\" >&2; exit 4; }; ",
    "[ $(( $(wc -c < \"$l\") )) -eq \"$z\" ] || { printf '%s is not the size that was sent\\n' \"$l\" >&2; exit 5; }; ",
    "if pr \"$t\"; then [ -f \"$t\" ] && [ ! -L \"$t\" ] || { printf '%s is not a regular file; it is left alone\\n' \"$t\" >&2; exit 4; }; fi; ",
    "mv -f -- \"$l\" \"$t\" || { printf '%s cannot be put in place\\n' \"$t\" >&2; exit 3; }; done; ",
    "printf 'IGN-RESULT:{\"ok\":1}\\n'",
);

pub(super) const GIT_SAVE_RM_SH: &str = concat!(
    "LC_ALL=C; export LC_ALL; ",
    "for c in ls rm rmdir; do command -v \"$c\" >/dev/null 2>&1 || { printf 'the host has no %s\\n' \"$c\" >&2; exit 3; }; done; ",
    "pr() { if o=$(ls -ld -- \"$1\" 2>&1); then return 0; fi; ",
    "case $o in *'No such file or directory') return 1;; esac; ",
    "printf '%s\\n' \"$o\" >&2; exit 3; }; ",
    "case $2 in ign-deploy/*/git-save/\"$3\"/repo) ;; *) printf 'refusing %s: it does not end in git-save/%s/repo\\n' \"$2\" \"$3\" >&2; exit 4;; esac; ",
    "[ -d \"$1\" ] || { printf '%s is not a directory\\n' \"$1\" >&2; exit 4; }; ",
    "cd -- \"$1\" || exit 3; set -f; OI=$IFS; IFS=/; for c in $2; do ",
    "case $c in ''|.|..) echo 'refusing an empty, . or .. component' >&2; exit 4;; esac; ",
    "if [ -L \"$c\" ]; then printf '%s is a symlink\\n' \"$c\" >&2; exit 4; ",
    "elif [ -d \"$c\" ]; then :; ",
    "elif pr \"$c\"; then printf '%s is not a directory\\n' \"$c\" >&2; exit 4; ",
    "else printf 'IGN-RESULT:{\"removed\":0}\\n'; exit 0; fi; ",
    "if [ \"$c\" = repo ]; then break; fi; cd -- \"$c\" || exit 3; done; IFS=$OI; ",
    "rm -rf -- repo || { echo 'the save cannot be removed' >&2; exit 3; }; ",
    "cd .. || exit 3; rmdir -- \"$3\" 2>/dev/null; printf 'IGN-RESULT:{\"removed\":1}\\n'",
);

pub(super) const GIT_FREE_SH: &str = concat!(
    "LC_ALL=C; export LC_ALL; ",
    "for c in df awk; do command -v \"$c\" >/dev/null 2>&1 || { printf 'the host has no %s\\n' \"$c\" >&2; exit 3; }; done; ",
    "d=$1/$2; while [ ! -d \"$d\" ]; do case $d in */*) d=${d%/*}; [ -n \"$d\" ] || d=/;; *) exit 3;; esac; done; ",
    "f=$(df -P -k -- \"$d\" | awk 'NR==2 {printf \"%.0f\", $4*1024}') || exit 3; [ -n \"$f\" ] || exit 3; ",
    "printf 'IGN-RESULT:{\"free\":%s}\\n' \"$f\"",
);

/// The note `RemoteShell::require_success` puts in the detail of an sftp
/// failure that answered "not found" (`remote.rs`).
const NOT_FOUND_CLASS: &str = "(not found or unreadable)";

impl SftpTransport {
    /// The project name as one safe component on this host's family, or the
    /// refusal. Runs before any path is built (T-25-21, T-25-25).
    fn project_name_problem(&self, project: &str) -> Option<String> {
        if let Some(why) = component_problem(project) {
            return Some(format!("the project name {why}"));
        }
        if self.windows
            && let Some(why) = windows_component_problem(project)
        {
            return Some(format!("the project name: {why}"));
        }
        None
    }

    /// The POSIX probe: one `stat` script over the exec channel.
    async fn git_entry_posix(&self, project: &str) -> Result<GitEntry, CoreError> {
        self.entry_posix(project, ".git").await
    }

    /// The POSIX probe of `projects/<project>/<entry>`; `entry` is a fixed
    /// name (`.git` or an [`ImportDir`] name).
    async fn entry_posix(&self, project: &str, entry: &str) -> Result<GitEntry, CoreError> {
        let projects = self.abs(&["projects".to_string()]);
        let project_dir = format!("{projects}/{project}");
        let git = format!("{project_dir}/{entry}");
        let command = RemoteCommand::posix(&[
            "sh",
            "-c",
            GIT_PROBE_SH,
            GIT_PROBE_NAME,
            &projects,
            &project_dir,
            &git,
        ])
        .map_err(|why| self.fail(format!("the .git probe cannot be built: {why}")))?;
        let answer = self.shell.exec_posix("git_entry", &command).await?;
        let word = |key: &str| answer.get(key).and_then(|v| v.as_str()).unwrap_or("");

        // Level 1: `data/projects`. A missing or odd one means data_dir is not
        // the gateway data directory, or the account cannot see it: never
        // "no git".
        match word("projects") {
            "dir" => {}
            "none" => {
                return Err(
                    self.fail("projects does not exist; is data_dir the gateway data directory?")
                );
            }
            "symlink" => {
                return Err(self.fail("projects is a symlink; symlinks are refused"));
            }
            "file" | "other" => return Err(self.fail("projects is not a directory")),
            other => return Err(self.unexpected("projects", other)),
        }
        // Level 2: the project directory. Missing is the `create` case.
        match word("project") {
            "dir" => {}
            "none" => return Ok(GitEntry::Absent),
            "symlink" => {
                return Err(self.fail(format!(
                    "projects/{project} is a symlink; symlinks are refused"
                )));
            }
            "file" | "other" => {
                return Err(self.fail(format!("projects/{project} is not a directory")));
            }
            other => return Err(self.unexpected("project", other)),
        }
        // Level 3: `.git`, of any kind (K9).
        Ok(match word("git") {
            "none" => GitEntry::Absent,
            "dir" => GitEntry::Directory,
            "file" => GitEntry::File,
            "symlink" => GitEntry::Symlink,
            "other" => GitEntry::Other,
            other => return Err(self.unexpected("git", other)),
        })
    }

    /// The Windows probe: one `Get-Item -Force` script over the encoded
    /// PowerShell route.
    async fn git_entry_windows(&self, project: &str) -> Result<GitEntry, CoreError> {
        self.entry_windows(project, ".git").await
    }

    /// The Windows probe of `projects\<project>\<entry>`; `entry` is a fixed
    /// name (`.git` or an [`ImportDir`] name).
    async fn entry_windows(&self, project: &str, entry: &str) -> Result<GitEntry, CoreError> {
        let projects = windows_native_path(&self.abs(&["projects".to_string()]));
        let project_dir = format!("{projects}\\{project}");
        let git = format!("{project_dir}\\{entry}");
        let script = WinScript::git_entry(&projects, &project_dir, &git);
        let answer = self.shell.exec_powershell(&script).await?;
        windows_git_entry_of(&answer, project, entry).map_err(|why| self.fail(why))
    }

    /// An answer word the probe does not define is a failure, never a kind.
    fn unexpected(&self, level: &str, word: &str) -> CoreError {
        let shown: String = word
            .chars()
            .filter(|c| c.is_ascii_graphic())
            .take(20)
            .collect();
        self.fail(format!(
            "the .git probe answered {shown:?} for {level}, which it does not define"
        ))
    }
}

/// The [`GitEntry`] of a [`winps::GIT_ENTRY`] result, or why it cannot be
/// read. Anything the script does not define, a missing field included, is an
/// error: the result is never guessed into `Absent`.
///
/// `entry` is `.git` or an [`ImportDir`] name, below `projects\<project>`.
fn windows_git_entry_of(
    answer: &serde_json::Value,
    project: &str,
    entry: &str,
) -> Result<GitEntry, String> {
    let location = format!("projects/{project}/{entry}");
    let bad = |why: String| format!("{location}: the .git probe result is unusable ({why})");
    if answer.get("ok").and_then(|v| v.as_i64()) != Some(1) {
        return Err(bad("ok is not 1".to_string()));
    }
    let word = |key: &str| answer.get(key).and_then(|v| v.as_str());
    // Level 1: `data\projects`. A missing one means data_dir is not the
    // gateway data directory: never "no git".
    match word("projects") {
        Some("dir") => {}
        Some("none") => {
            return Err(format!(
                "{location}: projects does not exist; is data_dir the gateway data directory?"
            ));
        }
        Some("symlink") => {
            return Err(format!(
                "{location}: projects is a junction or symlink; symlinks are refused"
            ));
        }
        Some("file") => return Err(format!("{location}: projects is not a directory")),
        other => return Err(bad(format!("projects is {other:?}"))),
    }
    // Level 2: the project directory. Missing is the `create` case.
    match word("project") {
        Some("dir") => {}
        Some("none") => return Ok(GitEntry::Absent),
        Some("symlink") => {
            return Err(format!(
                "{location}: projects/{project} is a junction or symlink; symlinks are refused"
            ));
        }
        Some("file") => {
            return Err(format!("{location}: projects/{project} is not a directory"));
        }
        other => return Err(bad(format!("project is {other:?}"))),
    }
    // Level 3: `.git`, of any kind; a reparse point is present and not followed.
    let flag = |key: &str| match answer.get(key).and_then(|v| v.as_i64()) {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        other => Err(bad(format!("{key} is {other:?}"))),
    };
    match word("kind") {
        Some("none") => Ok(GitEntry::Absent),
        Some(kind @ ("dir" | "file")) => {
            let _hidden = flag("hidden")?;
            if flag("reparse")? {
                Ok(GitEntry::Symlink)
            } else if kind == "dir" {
                Ok(GitEntry::Directory)
            } else {
                Ok(GitEntry::File)
            }
        }
        other => Err(bad(format!("kind is {other:?}"))),
    }
}

#[async_trait::async_trait]
impl ProjectHost for SftpTransport {
    fn git(&self) -> Option<&dyn GitHostRead> {
        Some(self)
    }

    async fn git_entry(&self, project: &str) -> Result<GitEntry, CoreError> {
        if let Some(why) = self.project_name_problem(project) {
            return Err(self.fail(why));
        }
        if self.windows {
            return self.git_entry_windows(project).await;
        }
        self.git_entry_posix(project).await
    }

    async fn read_release_record(
        &self,
        gateway: &str,
        project: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let location = release_record_location(LedgerRoot::AccountHome, gateway, project);
        self.read_home_file(
            &release_record_rel(gateway, project),
            RELEASE_RECORD_MAX_BYTES,
            "the release record",
            [("gateway name", gateway), ("project name", project)],
            &|problem| CoreError::DeployRunInvalid {
                run_id: "unknown".to_string(),
                problem: format!("{location}: {problem}"),
            },
        )
        .await
    }

    async fn read_tag_record(
        &self,
        gateway: &str,
        provider: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let location = tag_record_location(LedgerRoot::AccountHome, gateway, provider);
        let bad = |problem: String| CoreError::DeployRunInvalid {
            run_id: "unknown".to_string(),
            problem: format!("{location}: {problem}"),
        };
        if let Some(why) = tag_record_names_problem_on(gateway, provider, Some(self.windows)) {
            return Err(bad(why));
        }
        self.read_home_file(
            &tag_record_rel(gateway, provider),
            TAG_RECORD_MAX_BYTES,
            "the tag record",
            [("gateway name", gateway), ("provider name", provider)],
            &bad,
        )
        .await
    }

    async fn read_journal(
        &self,
        gateway: &str,
        project: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let location = project_journal_location(LedgerRoot::AccountHome, gateway, project);
        let bad = |problem: String| CoreError::DeployRunInvalid {
            run_id: "unknown".to_string(),
            problem: format!("{location}: {problem}"),
        };
        if let Some(why) = project_run_names_problem_on(gateway, project, None, Some(self.windows))
        {
            return Err(bad(why));
        }
        self.read_home_file(
            &project_journal_rel(gateway, project),
            PROJECT_JOURNAL_MAX_BYTES,
            "the journal",
            [("gateway name", gateway), ("project name", project)],
            &bad,
        )
        .await
    }

    async fn read_pre_image(
        &self,
        gateway: &str,
        project: &str,
        name: &str,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let location = pre_image_location(LedgerRoot::AccountHome, gateway, project, name);
        let bad = |problem: String| CoreError::DeployRunInvalid {
            run_id: "unknown".to_string(),
            problem: format!("{location}: {problem}"),
        };
        if let Some(why) =
            project_run_names_problem_on(gateway, project, Some(name), Some(self.windows))
        {
            return Err(bad(why));
        }
        self.read_home_file(
            &pre_image_rel(gateway, project, name),
            PRE_IMAGE_MAX_BYTES,
            "the pre-image file",
            [("gateway name", gateway), ("project name", project)],
            &bad,
        )
        .await
    }
}

/// One record file below the account home, as the shared write sees it.
struct HomeRecord<'a> {
    /// The path below the home (`ign-deploy/...`).
    rel: &'a str,
    /// The record's file name (`release.json`, `record.json`).
    file: &'a str,
    /// The temporary sibling's name, `<file>.tmp-<run_id>`.
    temp_name: &'a str,
    /// How a refusal names the file (`the release record`).
    what: &'a str,
    /// The exec operation name of the POSIX replace.
    replace_op: &'a str,
    /// The location errors name (`~/ign-deploy/...`).
    location: &'a str,
}

impl SftpTransport {
    /// Walk the record path from the account home and say where it stops.
    ///
    /// `Ok(None)`: every level exists and `release.json` is a regular file.
    /// `Ok(Some(i))`: `parts[i]` and everything below it is absent. Anything
    /// that is not a plain directory on the way, a `release.json` that is not
    /// a regular file, and a leftover file named `temp` in the record's
    /// directory are refused as `transport_failed` naming `location`.
    async fn record_walk(
        &self,
        home: &str,
        parts: &[&str],
        temp: &str,
        location: &str,
        what: &str,
    ) -> Result<Option<usize>, CoreError> {
        let refuse = |why: &str| self.mirror_fail("write", format!("{location}: {why}"));
        // Only the fixed names of the path are looked up in the home, so names
        // that cannot ride a batch line are skipped, not refused.
        let mut budget = Budget::lenient();
        let mut current = home.to_string();
        for (index, name) in parts.iter().enumerate() {
            let last = index + 1 == parts.len();
            let mut listed = self
                .list_level_as("write", std::slice::from_ref(&current), &mut budget)
                .await
                .map_err(|e| named(e, location))?;
            let entries = listed.pop().unwrap_or_default();
            if last && entries.iter().any(|e| e.name == temp) {
                return Err(refuse(
                    "a file of the temporary name is already there; it is not touched",
                ));
            }
            let Some(entry) = entries.iter().find(|e| e.name == *name) else {
                return Ok(Some(index));
            };
            match (entry.kind, last) {
                (EntryKind::Symlink, _) => {
                    return Err(refuse(
                        "a path component is a symlink; symlinks are refused",
                    ));
                }
                (EntryKind::Dir, false) | (EntryKind::File, true) => {}
                (_, false) => return Err(refuse("a path component is not a directory")),
                (EntryKind::Dir, true) => return Err(refuse(&format!("{what} is a directory"))),
                (_, true) => {
                    return Err(refuse(&format!("{what} is not a regular file")));
                }
            }
            current = format!("{current}/{name}");
        }
        Ok(None)
    }

    /// Put `bytes` at `temp` over sftp, from a private local file.
    async fn put_record_temp(
        &self,
        bytes: &[u8],
        temp: &str,
        location: &str,
    ) -> Result<(), CoreError> {
        let dir = self.download_dir("write").map_err(|e| named(e, location))?;
        let local = dir.path().join("record");
        let written = {
            use std::io::Write;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options
                .open(&local)
                .and_then(|mut file| file.write_all(bytes).and_then(|()| file.flush()))
        };
        written.map_err(|e| {
            self.mirror_fail(
                "write",
                format!(
                    "{location}: the record cannot be staged locally ({:?})",
                    e.kind()
                ),
            )
        })?;
        self.run_batch(
            "write",
            vec![SftpOp::Put {
                local,
                remote: temp.to_string(),
            }],
        )
        .await
        .map_err(|e| named(e, location))
    }

    /// Remove `temp` after `err` and return `err`, saying in its detail that
    /// the temporary file `temp_name` may still be there when the removal
    /// failed. With `absent_is_fine` a removal that answers "not found" counts
    /// as done (the file was never created).
    async fn remove_temp_after(
        &self,
        err: CoreError,
        temp: &str,
        temp_name: &str,
        absent_is_fine: bool,
    ) -> CoreError {
        let cleanup = self
            .run_batch(
                "write",
                vec![SftpOp::Rm {
                    path: temp.to_string(),
                }],
            )
            .await;
        let leftover = match &cleanup {
            Ok(()) => false,
            Err(CoreError::TransportFailed { detail, .. }) => {
                !(absent_is_fine && detail.contains(NOT_FOUND_CLASS))
            }
            Err(_) => true,
        };
        match err {
            CoreError::TransportFailed {
                operation,
                node,
                detail,
            } if leftover => CoreError::TransportFailed {
                operation,
                node,
                detail: format!("{detail}; the temporary file {temp_name} may still be there"),
            },
            err => err,
        }
    }

    /// Replace `final_path` with `temp` in one step on this host's family.
    async fn replace_record(
        &self,
        temp: &str,
        final_path: &str,
        location: &str,
        op: &str,
    ) -> Result<(), CoreError> {
        if self.windows {
            // The sftp spelling `/C:/Users/<account>/...` becomes the native
            // one; both travel, with the retry count, as parameters of the
            // fixed atomic `MoveFileEx` script (base64 JSON), never in its
            // text. The script fails closed, so a blocked or impossible
            // replace is an error here and never a non-atomic fallback.
            let script = WinScript::record_replace(
                &windows_native_path(temp),
                &windows_native_path(final_path),
                RENAME_RETRIES,
            );
            let answer = self
                .shell
                .exec_powershell(&script)
                .await
                .map_err(|e| named(e, location))?;
            return if answer.get("ok").and_then(serde_json::Value::as_i64) == Some(1) {
                Ok(())
            } else {
                Err(self.mirror_fail(
                    "write",
                    format!("{location}: the record replace script did not report ok"),
                ))
            };
        }
        let command = RemoteCommand::posix(&["mv", "-f", "--", temp, final_path])
            .map_err(|why| self.mirror_fail("write", format!("{location}: {why}")))?;
        self.shell
            .exec_posix_status(op, &command)
            .await
            .map_err(|e| named(e, location))
    }
}

/// `err` with `location` in front of its detail, when it is a transport
/// failure; any other error is returned as it is.
fn named(err: CoreError, location: &str) -> CoreError {
    match err {
        CoreError::TransportFailed {
            operation,
            node,
            detail,
        } => CoreError::TransportFailed {
            operation,
            node,
            detail: format!("{location}: {detail}"),
        },
        other => other,
    }
}

#[async_trait::async_trait]
impl ReleaseRecordWriter for SftpTransport {
    async fn write_release_record(
        &self,
        gateway: &str,
        project: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        let location = release_record_location(LedgerRoot::AccountHome, gateway, project);
        let bad = |problem: String| CoreError::DeployRunInvalid {
            run_id: "unknown".to_string(),
            problem: format!("{location}: {problem}"),
        };
        // The names get the check the read applies, on this host's family.
        for (kind, value) in [("gateway", gateway), ("project", project)] {
            if let Some(why) = component_problem(value) {
                return Err(bad(format!("the {kind} name {why}")));
            }
            if self.windows
                && let Some(why) = windows_component_problem(value)
            {
                return Err(bad(format!("the {kind} name: {why}")));
            }
        }
        // The same parse the read applies: a record that would be refused on
        // the way back is never put there. The run id it carries is already
        // held to `[A-Za-z0-9._-]{1,128}`, so it is a safe temporary suffix.
        let record = ReleaseRecord::parse(bytes, &location, gateway, project)?;
        let temp_name = format!("{RELEASE_RECORD_FILE}.tmp-{}", record.run_id);
        self.write_home_record(
            &HomeRecord {
                rel: &release_record_rel(gateway, project),
                file: RELEASE_RECORD_FILE,
                temp_name: &temp_name,
                what: "the release record",
                replace_op: "release_record_replace",
                location: &location,
            },
            bytes,
        )
        .await
    }
}

#[async_trait::async_trait]
impl TagRecordWriter for SftpTransport {
    async fn write_tag_record(
        &self,
        gateway: &str,
        provider: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        let location = tag_record_location(LedgerRoot::AccountHome, gateway, provider);
        // The names the read accepts on this host's family, glob characters
        // refused, before any session.
        if let Some(why) = tag_record_names_problem_on(gateway, provider, Some(self.windows)) {
            return Err(CoreError::DeployRunInvalid {
                run_id: "unknown".to_string(),
                problem: format!("{location}: {why}"),
            });
        }
        // The read's parse: only this gateway's and provider's record is put
        // there, and its run id is a safe temporary suffix.
        let record = TagRecord::parse(bytes, &location, gateway, provider)?;
        let temp_name = format!("{TAG_RECORD_FILE}.tmp-{}", record.run_id);
        self.write_home_record(
            &HomeRecord {
                rel: &tag_record_rel(gateway, provider),
                file: TAG_RECORD_FILE,
                temp_name: &temp_name,
                what: "the tag record",
                replace_op: "tag_record_replace",
                location: &location,
            },
            bytes,
        )
        .await
    }
}

#[async_trait::async_trait]
impl ProjectRunWriter for SftpTransport {
    async fn write_journal(
        &self,
        gateway: &str,
        project: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        let location = project_journal_location(LedgerRoot::AccountHome, gateway, project);
        if let Some(why) = project_run_names_problem_on(gateway, project, None, Some(self.windows))
        {
            return Err(CoreError::DeployRunInvalid {
                run_id: "unknown".to_string(),
                problem: format!("{location}: {why}"),
            });
        }
        let journal = ProjectJournal::parse(bytes, &location, gateway, project)?;
        let temp_name = format!("{PROJECT_JOURNAL_FILE}.tmp-{}", journal.run_id);
        self.write_home_record(
            &HomeRecord {
                rel: &project_journal_rel(gateway, project),
                file: PROJECT_JOURNAL_FILE,
                temp_name: &temp_name,
                what: "the journal",
                replace_op: "project_journal_replace",
                location: &location,
            },
            bytes,
        )
        .await
    }

    async fn clear_journal(&self, gateway: &str, project: &str) -> Result<(), CoreError> {
        let location = project_journal_location(LedgerRoot::AccountHome, gateway, project);
        if let Some(why) = project_run_names_problem_on(gateway, project, None, Some(self.windows))
        {
            return Err(CoreError::DeployRunInvalid {
                run_id: "unknown".to_string(),
                problem: format!("{location}: {why}"),
            });
        }
        let home = self.account_home().await?;
        let rel = project_journal_rel(gateway, project);
        let parts: Vec<&str> = rel.split('/').collect();
        // Every level is listed first: a symlink or a non-directory on the
        // way, or a journal that is not a regular file, refuses and is left.
        if self
            .record_walk(&home, &parts, "", &location, "the journal")
            .await?
            .is_some()
        {
            return Ok(());
        }
        match self
            .run_batch(
                "write",
                vec![SftpOp::Rm {
                    path: format!("{home}/{rel}"),
                }],
            )
            .await
        {
            Ok(()) => Ok(()),
            Err(CoreError::TransportFailed { detail, .. }) if detail.contains(NOT_FOUND_CLASS) => {
                Ok(())
            }
            Err(err) => Err(named(err, &location)),
        }
    }

    async fn write_pre_image(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        let location = pre_image_location(LedgerRoot::AccountHome, gateway, project, name);
        check_pre_image_write(
            &location,
            gateway,
            project,
            run_id,
            name,
            bytes,
            Some(self.windows),
        )?;
        let temp_name = format!("{name}.tmp-{run_id}");
        self.write_home_record(
            &HomeRecord {
                rel: &pre_image_rel(gateway, project, name),
                file: name,
                temp_name: &temp_name,
                what: "the pre-image file",
                replace_op: "pre_image_replace",
                location: &location,
            },
            bytes,
        )
        .await
    }
}

impl SftpTransport {
    /// The shared record write (module doc, steps 2 to 5): walk the path from
    /// the account home, make the missing levels, `put` the temporary sibling
    /// and replace the record in one atomic step; a failure removes the
    /// temporary file and leaves the previous record. The caller has checked
    /// the names and parsed `bytes`.
    async fn write_home_record(
        &self,
        target: &HomeRecord<'_>,
        bytes: &[u8],
    ) -> Result<(), CoreError> {
        let (temp_name, location) = (target.temp_name, target.location);
        let home = self.account_home().await?;
        let parts: Vec<&str> = target.rel.split('/').collect();
        let (_, dirs) = parts.split_last().expect("the record path has components");
        let missing = self
            .record_walk(&home, &parts, temp_name, location, target.what)
            .await?;

        let dir = format!("{home}/{}", dirs.join("/"));
        let temp = format!("{dir}/{temp_name}");
        let final_path = format!("{dir}/{}", target.file);

        if let Some(first) = missing {
            let mut path = home.clone();
            let mut ops = Vec::new();
            for (index, name) in dirs.iter().enumerate() {
                path = format!("{path}/{name}");
                if index >= first {
                    ops.push(SftpOp::Mkdir { path: path.clone() });
                }
            }
            self.run_batch("write", ops)
                .await
                .map_err(|e| named(e, location))?;
        }

        // A failed `put` may have left a partial temporary file, and a failed
        // replace leaves the whole one: either way the previous record is
        // untouched and the temporary file is taken away. A removal that
        // fails is reported in the error, so a leftover is never silent. After
        // a failed `put` the file often was never there, so a removal that
        // answers "not found" is not a leftover.
        if let Err(err) = self.put_record_temp(bytes, &temp, location).await {
            return Err(self.remove_temp_after(err, &temp, temp_name, true).await);
        }
        if let Err(err) = self
            .replace_record(&temp, &final_path, location, target.replace_op)
            .await
        {
            return Err(self.remove_temp_after(err, &temp, temp_name, false).await);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The git host channel (Phase 27, 27-08): POSIX nodes
// ---------------------------------------------------------------------------
//
// One fixed `sh -c` script per operation over the hardened exec channel, so a
// save, a restore, a manifest or a size is a fixed number of ssh connections
// whatever the file count (D-06; a per-file sftp loop costs about 0.8 s per
// folder on a production Windows host and is never used). The scripts take
// every path and name as a positional argument, hold no control character
// (one line each) and print one `IGN-RESULT:` JSON line. They need only POSIX
// `sh` and the usual utilities, so the same text runs on the Linux hosts and
// under the local `sh` in the tests.
//
// | script | does |
// |---|---|
// | `GIT_STATE_SH` | reads the files `classify` needs: HEAD, config, packed-refs, logs/HEAD tail, loose refs, markers, index |
// | `GIT_MANIFEST_SH` | `GitManifest` of a directory: sizes below `objects/`, sha256 elsewhere |
// | `GIT_SIZE_SH` | file count and total size from `ls -ln`, nothing opened |
// | `GIT_DIGEST_SH` | sha256 of one file below `.git` |
// | `GIT_SAVE_SH` | `cp -a` of `.git` into the private save, then both manifests compared |
// | `GIT_RESTORE_SH` | `cp -a` of the save beside `.git`, the D-G1 ACL step, manifest compare, `mv` into place |
// | `GIT_PROBE_W_SH` | exclusive create then remove of a probe file in the project directory and its `.git` |
// | `GIT_LOCK_SH` | `<rel>.lock` (or `.ign-tmp`) created exclusively; the sftp CLI cannot open `O_EXCL` |
// | `GIT_REPLACE_SH` | size check, then `mv -f` of each lock over its file |
// | `GIT_SAVE_RM_SH` | removes `git-save/<run-id>/repo` and nothing else |
// | `GIT_FREE_SH` | free bytes of the file system under the save root |
//
// A script exits 3 for a failure of the host or a denial, 4 for a refusal
// (a symlink, an existing target, a foreign path), 5 for a copy that does not
// match, 6 for the gateway-account ACL step. Every non-zero exit is
// `transport_failed` naming the operation, with the script's stderr tail.
//
// The Windows arm of every method is a WinScript (27-09), one per operation,
// built from native paths and base64 JSON parameters only. The script answers
// `ok: 1` and the fields of its doc; anything else (a missing field, `ok: 0`)
// is `transport_failed` naming the operation, never an absent `.git` and never
// an empty answer.

/// The `$0` the git scripts run under.
const GIT_SCRIPT_NAME: &str = "ign-git";

/// Longest a heavy git script (a save, restore, manifest or removal of a
/// whole `.git`) may run.
const GIT_HEAVY_LIMIT: std::time::Duration = std::time::Duration::from_secs(900);

/// The state files' caps the scripts ask for (one byte past each cap, so the
/// caller's cap check names a file that is over).
const STATE_SMALL_READ: usize = crate::deploy::git_state::GIT_SMALL_FILE_MAX + 1;

impl SftpTransport {
    /// Run one git WinScript and require `ok == 1` in its answer.
    async fn git_win(
        &self,
        op: &str,
        script: WinScript,
        limit: std::time::Duration,
    ) -> Result<serde_json::Value, CoreError> {
        let answer = self.shell.exec_powershell_within(&script, limit).await?;
        if answer.get("ok").and_then(serde_json::Value::as_i64) != Some(1) {
            return Err(self.git_unusable(op, "ok is not 1"));
        }
        Ok(answer)
    }

    /// The native spelling of the data directory of a Windows node.
    fn win_data(&self) -> String {
        windows_native_path(&self.data_dir)
    }

    /// The native `.git` of `project` on a Windows node.
    fn win_git(&self, project: &str) -> String {
        format!("{}\\projects\\{project}\\.git", self.win_data())
    }

    /// The native account home of a Windows node.
    async fn win_home(&self) -> Result<String, CoreError> {
        Ok(windows_native_path(&self.account_home().await?))
    }

    /// The names gateway, project and run id as safe components on this host's
    /// family, before any path is built (T-27-22).
    fn git_names_problem(&self, gateway: &str, project: &str, run_id: &str) -> Option<String> {
        let location = git_save_location(LedgerRoot::AccountHome, gateway, project, "<run-id>");
        if let Some(why) = project_run_names_problem_on(gateway, project, None, Some(self.windows))
        {
            return Some(format!("{location}: {why}"));
        }
        if RunId::parse(run_id).is_err() {
            return Some(format!(
                "{location}: the run id is not 1 to 128 characters of [A-Za-z0-9._-]"
            ));
        }
        // the run id is a directory name too: `CON` or a trailing dot is not
        // a name Windows keeps
        if self.windows
            && let Some(why) = windows_component_problem(run_id)
        {
            return Some(format!("{location}: the run id: {why}"));
        }
        None
    }

    fn git_names_check(
        &self,
        op: &str,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<(), CoreError> {
        match self.git_names_problem(gateway, project, run_id) {
            Some(why) => Err(self.mirror_fail(op, why)),
            None => Ok(()),
        }
    }

    /// Run one git script with `args` (paths and names only as arguments).
    async fn git_run(
        &self,
        op: &str,
        script: &'static str,
        args: &[&str],
        limit: std::time::Duration,
    ) -> Result<serde_json::Value, CoreError> {
        let mut argv = vec!["sh", "-c", script, GIT_SCRIPT_NAME];
        argv.extend_from_slice(args);
        let command = RemoteCommand::posix(&argv).map_err(|why| {
            self.mirror_fail(op, format!("the {op} command cannot be built: {why}"))
        })?;
        self.shell.exec_posix_within(op, &command, limit).await
    }

    /// The `.git` of `project` as a path below the data directory.
    fn git_rel(project: &str) -> String {
        format!("projects/{project}/.git")
    }

    fn git_unusable(&self, op: &str, what: &str) -> CoreError {
        self.mirror_fail(op, format!("{op}: the host's answer is unusable ({what})"))
    }

    /// A [`GitManifest`] from a script answer.
    fn git_manifest_of(
        &self,
        op: &str,
        answer: &serde_json::Value,
    ) -> Result<GitManifest, CoreError> {
        let files = answer.get("files").and_then(serde_json::Value::as_u64);
        let bytes = answer.get("bytes").and_then(serde_json::Value::as_u64);
        let digest = answer.get("digest").and_then(serde_json::Value::as_str);
        match (files, bytes, digest) {
            (Some(files), Some(bytes), Some(digest))
                if digest.len() == 64
                    && digest
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) =>
            {
                Ok(GitManifest {
                    files: usize::try_from(files)
                        .map_err(|_| self.git_unusable(op, "the file count"))?,
                    bytes,
                    digest: digest.to_string(),
                })
            }
            _ => Err(self.git_unusable(op, "files, bytes or digest is missing or malformed")),
        }
    }

    /// The digest of a `git_file_digest` answer: `null` or 64 lowercase hex.
    fn git_digest_of(&self, answer: &serde_json::Value) -> Result<Option<String>, CoreError> {
        match answer.get("digest") {
            Some(serde_json::Value::Null) => Ok(None),
            Some(serde_json::Value::String(hex))
                if hex.len() == 64
                    && hex
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) =>
            {
                Ok(Some(hex.clone()))
            }
            _ => Err(self.git_unusable("git_file_digest", "the digest")),
        }
    }

    /// The bytes of one `{"n":size,"b":base64}` file answer (or `null`),
    /// checked against the size the host reported: `want` bytes were asked
    /// for, so a short read (a failed `head` the shell could not see) is an
    /// error here and never silently empty content.
    fn git_file_bytes(
        &self,
        op: &str,
        what: &str,
        value: Option<&serde_json::Value>,
        asked: usize,
    ) -> Result<Option<Vec<u8>>, CoreError> {
        let Some(value) = value else {
            return Err(self.git_unusable(op, &format!("{what} is missing")));
        };
        if value.is_null() {
            return Ok(None);
        }
        let size = value.get("n").and_then(serde_json::Value::as_u64);
        let text = value.get("b").and_then(serde_json::Value::as_str);
        let (Some(size), Some(text)) = (size, text) else {
            return Err(self.git_unusable(op, &format!("{what} has no size or no bytes")));
        };
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, text)
            .map_err(|_| self.git_unusable(op, &format!("{what} is not base64")))?;
        let want = size.min(asked as u64);
        if bytes.len() as u64 != want {
            return Err(self.git_unusable(
                op,
                &format!(
                    "{what} came back as {} bytes where {want} were expected; a read was cut short",
                    bytes.len()
                ),
            ));
        }
        Ok(Some(bytes))
    }

    /// The [`GitFiles`] of a `GIT_STATE_SH` answer.
    fn git_files_of(&self, op: &str, answer: &serde_json::Value) -> Result<GitFiles, CoreError> {
        let mut files = GitFiles::without_contents(GitEntry::Directory);
        files.head = self.git_file_bytes(op, "HEAD", answer.get("head"), STATE_SMALL_READ)?;
        files.config = self.git_file_bytes(op, "config", answer.get("config"), STATE_SMALL_READ)?;
        files.packed_refs = self.git_file_bytes(
            op,
            "packed-refs",
            answer.get("packed"),
            GIT_PACKED_REFS_MAX + 1,
        )?;
        files.head_log_tail =
            self.git_file_bytes(op, "logs/HEAD", answer.get("log"), GIT_LOG_TAIL_MAX)?;
        files.index_present = answer
            .get("index")
            .and_then(serde_json::Value::as_bool)
            .ok_or_else(|| self.git_unusable(op, "index is missing"))?;
        let markers = answer
            .get("markers")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| self.git_unusable(op, "markers is missing"))?;
        for marker in markers {
            let name = marker
                .as_str()
                .filter(|name| GIT_MARKERS.contains(name))
                .ok_or_else(|| self.git_unusable(op, "a marker the host may not report"))?;
            files.markers.push(name.to_string());
        }
        let refs = answer
            .get("refs")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| self.git_unusable(op, "refs is missing"))?;
        for entry in refs {
            let name = entry
                .get("p")
                .and_then(serde_json::Value::as_str)
                .and_then(|text| {
                    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, text).ok()
                })
                .and_then(|raw| String::from_utf8(raw).ok())
                .filter(|name| name.starts_with("refs/heads/") || name.starts_with("refs/remotes/"))
                .ok_or_else(|| self.git_unusable(op, "a loose ref name"))?;
            let bytes = self
                .git_file_bytes(op, "a loose ref", Some(entry), STATE_SMALL_READ)?
                .unwrap_or_default();
            files.loose_refs.push((name, bytes));
        }
        // Depth-first in name order, as the local walk lists them.
        files
            .loose_refs
            .sort_by(|a, b| a.0.split('/').cmp(b.0.split('/')));
        files.loose_refs.truncate(GIT_LOOSE_REFS_MAX + 1);
        Ok(files)
    }

    /// `git.yaml` in the data directory, one capped `get`.
    async fn read_git_yaml_file(&self) -> Result<Option<Vec<u8>>, CoreError> {
        let bad = |problem: String| CoreError::DeployRunInvalid {
            run_id: "unknown".to_string(),
            problem: format!("git.yaml: {problem}"),
        };
        self.read_file_below(
            Some(&self.data_dir),
            "git.yaml",
            YAML_MAX_BYTES,
            "git.yaml",
            &[],
            &bad,
        )
        .await
    }

    /// The `ign-deploy/.../git-save/<run>` directory below the account home.
    fn git_save_dir_rel(gateway: &str, project: &str, run_id: &str) -> String {
        let repo = git_save_rel(gateway, project, run_id);
        repo.strip_suffix("/repo").unwrap_or(&repo).to_string()
    }
}

#[async_trait::async_trait]
impl GitHostRead for SftpTransport {
    async fn read_git_state(&self, project: &str) -> Result<GitFiles, CoreError> {
        if let Some(why) = self.project_name_problem(project) {
            return Err(self.fail(why));
        }
        if self.windows {
            let kind = self.git_entry_windows(project).await?;
            if kind != GitEntry::Directory {
                return Ok(GitFiles::without_contents(kind));
            }
            let script = WinScript::git_state(&self.win_git(project), GIT_MARKERS);
            let answer = self
                .git_win("read_git_state", script, self.shell.op_timeout())
                .await?;
            return self.git_files_of("read_git_state", &answer);
        }
        let kind = self.git_entry_posix(project).await?;
        if kind != GitEntry::Directory {
            return Ok(GitFiles::without_contents(kind));
        }
        let rel = Self::git_rel(project);
        let mut args = vec![self.data_dir.as_str(), rel.as_str()];
        args.extend(GIT_MARKERS.iter().copied());
        let answer = self
            .git_run(
                "read_git_state",
                GIT_STATE_SH,
                &args,
                self.shell.op_timeout(),
            )
            .await?;
        self.git_files_of("read_git_state", &answer)
    }

    async fn git_manifest(&self, project: &str) -> Result<GitManifest, CoreError> {
        if let Some(why) = self.project_name_problem(project) {
            return Err(self.fail(why));
        }
        if self.windows {
            let script = WinScript::git_manifest(LONG_PATH_PREFIX, &self.win_git(project));
            let answer = self
                .git_win("git_manifest", script, GIT_HEAVY_LIMIT)
                .await?;
            return self.git_manifest_of("git_manifest", &answer);
        }
        let rel = Self::git_rel(project);
        let answer = self
            .git_run(
                "git_manifest",
                GIT_MANIFEST_SH,
                &[&self.data_dir, &rel],
                GIT_HEAVY_LIMIT,
            )
            .await?;
        self.git_manifest_of("git_manifest", &answer)
    }

    async fn save_manifest(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<GitManifest, CoreError> {
        self.git_names_check("save_manifest", gateway, project, run_id)?;
        if self.windows {
            let home = self.win_home().await?;
            let repo = git_save_rel(gateway, project, run_id).replace('/', "\\");
            let script = WinScript::git_manifest(LONG_PATH_PREFIX, &format!("{home}\\{repo}"));
            let answer = self
                .git_win("save_manifest", script, GIT_HEAVY_LIMIT)
                .await?;
            return self.git_manifest_of("save_manifest", &answer);
        }
        let home = self.account_home().await?;
        let rel = git_save_rel(gateway, project, run_id);
        let answer = self
            .git_run(
                "save_manifest",
                GIT_MANIFEST_SH,
                &[&home, &rel],
                GIT_HEAVY_LIMIT,
            )
            .await?;
        self.git_manifest_of("save_manifest", &answer)
    }

    async fn git_file_digest(
        &self,
        project: &str,
        name: &GitWriteName,
    ) -> Result<Option<String>, CoreError> {
        if let Some(why) = self.project_name_problem(project) {
            return Err(self.fail(why));
        }
        name.check_shape()
            .map_err(|why| self.fail(format!("a .git file name is refused: {why}")))?;
        if self.windows {
            let script = WinScript::git_digest(&self.win_git(project), &name.rel());
            let answer = self
                .git_win("git_file_digest", script, self.shell.op_timeout())
                .await?;
            return self.git_digest_of(&answer);
        }
        let (rel, file) = (Self::git_rel(project), name.rel());
        let answer = self
            .git_run(
                "git_file_digest",
                GIT_DIGEST_SH,
                &[&self.data_dir, &rel, &file],
                self.shell.op_timeout(),
            )
            .await?;
        self.git_digest_of(&answer)
    }

    async fn read_git_yaml(&self) -> Result<Option<Vec<u8>>, CoreError> {
        self.read_git_yaml_file().await
    }

    async fn project_dir_entry(
        &self,
        project: &str,
        dir: ImportDir,
    ) -> Result<GitEntry, CoreError> {
        if let Some(why) = self.project_name_problem(project) {
            return Err(self.fail(why));
        }
        if self.windows {
            return self.entry_windows(project, dir.name()).await;
        }
        self.entry_posix(project, dir.name()).await
    }

    async fn free_bytes(&self, gateway: &str, project: &str) -> Result<u64, CoreError> {
        self.git_names_check("free_bytes", gateway, project, "free")?;
        let repo = git_save_rel(gateway, project, "free");
        let dir = repo.strip_suffix("/free/repo").unwrap_or(&repo).to_string();
        if self.windows {
            let script = WinScript::git_free(&self.win_home().await?, &dir);
            let answer = self
                .git_win("free_bytes", script, self.shell.op_timeout())
                .await?;
            return answer
                .get("free")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| self.git_unusable("free_bytes", "the free byte count"));
        }
        let home = self.account_home().await?;
        let answer = self
            .git_run(
                "free_bytes",
                GIT_FREE_SH,
                &[&home, &dir],
                self.shell.op_timeout(),
            )
            .await?;
        answer
            .get("free")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| self.git_unusable("free_bytes", "the free byte count"))
    }

    async fn git_size(&self, project: &str) -> Result<GitSize, CoreError> {
        if let Some(why) = self.project_name_problem(project) {
            return Err(self.fail(why));
        }
        let answer = if self.windows {
            let script = WinScript::git_size(LONG_PATH_PREFIX, &self.win_git(project));
            self.git_win("git_size", script, self.shell.op_timeout())
                .await?
        } else {
            let rel = Self::git_rel(project);
            self.git_run(
                "git_size",
                GIT_SIZE_SH,
                &[&self.data_dir, &rel],
                self.shell.op_timeout(),
            )
            .await?
        };
        let files = answer.get("files").and_then(serde_json::Value::as_u64);
        let bytes = answer.get("bytes").and_then(serde_json::Value::as_u64);
        match (files.map(usize::try_from), bytes) {
            (Some(Ok(files)), Some(bytes)) => Ok(GitSize { files, bytes }),
            _ => Err(self.git_unusable("git_size", "the count or the size")),
        }
    }

    /// The object files of the project's `.git` (27-11, D-24): one script on a
    /// POSIX host, one WinScript on a Windows host, never a listing per
    /// directory. Both report only the fixed shapes below two levels of
    /// `objects/` and refuse a symlink, so the same repo lists the same here as
    /// on the local transport ([`GitObjectList::from_entries`] runs on the
    /// answer either way).
    async fn list_git_objects(&self, project: &str) -> Result<GitObjectList, CoreError> {
        const OP: &str = "list_git_objects";
        if let Some(why) = self.project_name_problem(project) {
            return Err(self.fail(why));
        }
        let answer = if self.windows {
            let script = WinScript::git_objects(
                LONG_PATH_PREFIX,
                &self.win_git(project),
                GIT_OBJECTS_LIST_MAX_ENTRIES,
                GIT_OBJECTS_REPORT_MAX,
            );
            self.git_win(OP, script, GIT_HEAVY_LIMIT).await?
        } else {
            let rel = format!("{}/objects", Self::git_rel(project));
            let (walk, report) = (
                GIT_OBJECTS_LIST_MAX_ENTRIES.to_string(),
                GIT_OBJECTS_REPORT_MAX.to_string(),
            );
            self.git_run(
                OP,
                GIT_OBJECTS_SH,
                &[&self.data_dir, &rel, &walk, &report],
                GIT_HEAVY_LIMIT,
            )
            .await?
        };
        self.git_objects_of(&answer)
    }

    /// Copy exactly the listed object `files` into `dest/<rel>` (27-11, D-24).
    ///
    /// Every name is re-checked with [`check_object_rel`] and the count, the
    /// total size and the destination are checked before the first byte moves.
    /// The files arrive in a private download directory through the shared
    /// machinery of the resource reads (one `get` per file in a fixed number
    /// of sessions on a POSIX host, one archive on a Windows host, every file
    /// exactly its listed size), and are then installed into `dest` with an
    /// exclusive create: a file already there is never overwritten.
    async fn fetch_git_objects(
        &self,
        project: &str,
        files: &[GitObjectFile],
        dest: &Path,
    ) -> Result<(), CoreError> {
        const OP: &str = "fetch_git_objects";
        if let Some(why) = self.project_name_problem(project) {
            return Err(self.fail(why));
        }
        if files.len() > GIT_OBJECTS_REPORT_MAX {
            return Err(self.mirror_fail(
                OP,
                format!(
                    "{} object files were asked for, over the {GIT_OBJECTS_REPORT_MAX} a read may fetch",
                    files.len()
                ),
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut total = 0u64;
        let mut found: Vec<FoundFile> = Vec::with_capacity(files.len());
        for file in files {
            check_object_rel(&file.rel).map_err(|why| {
                self.mirror_fail(OP, format!("{} is refused: {why}", file.rel.escape_debug()))
            })?;
            if !seen.insert(file.rel.as_str()) {
                return Err(self.mirror_fail(
                    OP,
                    format!("{} is asked for twice", file.rel.escape_debug()),
                ));
            }
            total = total.saturating_add(file.size);
            // `check_object_rel` proved the `objects/` prefix and the shape
            let below = file.rel.strip_prefix("objects/").unwrap_or_default();
            found.push(FoundFile {
                collection: "objects".to_string(),
                rel: below.to_string(),
                size: file.size,
            });
        }
        if total > GIT_OBJECTS_FETCH_MAX_BYTES {
            return Err(self.mirror_fail(
                OP,
                format!(
                    "the object files asked for add up to {total} bytes, over the {GIT_OBJECTS_FETCH_MAX_BYTES} byte cap"
                ),
            ));
        }
        if files.is_empty() {
            return Ok(());
        }
        self.git_dest_problem(dest, files)
            .map_err(|why| self.mirror_fail(OP, why))?;

        let base = ReadBase::ProjectGit(project.to_string());
        let tmp = self.download_dir(OP)?;
        let refs: Vec<&FoundFile> = found.iter().collect();
        if self.bundle_reads {
            self.fetch_bundled(&base, &refs, tmp.path()).await?;
        } else {
            self.fetch_files(&base, &refs, tmp.path()).await?;
        }
        let (from, to, listed) = (tmp.path().to_path_buf(), dest.to_path_buf(), files.to_vec());
        let installed =
            tokio::task::spawn_blocking(move || install_object_files(&from, &to, &listed))
                .await
                .map_err(|e| CoreError::Internal(format!("sftp read task failed: {e}")))?;
        drop(tmp);
        installed.map_err(|why| self.mirror_fail(OP, why))
    }
}

impl SftpTransport {
    /// The [`GitObjectList`] of a `GIT_OBJECTS_SH` or `GIT_OBJECTS` answer:
    /// `n` lines `size objects/<name>` in `t`, `more` 0. An answer that was cut off
    /// (`more`), that does not add up or that holds a line it cannot read is
    /// refused, and so is a listing [`GitObjectList::from_entries`] refuses.
    fn git_objects_of(&self, answer: &serde_json::Value) -> Result<GitObjectList, CoreError> {
        const OP: &str = "list_git_objects";
        let more = answer.get("more").and_then(serde_json::Value::as_i64);
        let count = answer.get("n").and_then(serde_json::Value::as_u64);
        let text = answer.get("t").and_then(serde_json::Value::as_str);
        let (Some(more), Some(count), Some(text)) = (more, count, text) else {
            return Err(self.git_unusable(OP, "n, more or t is missing or malformed"));
        };
        if more != 0 {
            return Err(self.mirror_fail(
                OP,
                format!(
                    "the gateway repo holds more object files than a read may list (over {GIT_OBJECTS_REPORT_MAX})"
                ),
            ));
        }
        let mut entries: Vec<GitObjectEntry> = Vec::new();
        for line in text.split('\n').filter(|line| !line.is_empty()) {
            let parsed = line
                .split_once(' ')
                .filter(|(size, _)| !size.is_empty() && size.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|(size, name)| Some((size.parse::<u64>().ok()?, name)));
            let Some((size, name)) = parsed else {
                return Err(self.git_unusable(OP, "a listing line is not `size name`"));
            };
            if entries.len() >= GIT_OBJECTS_REPORT_MAX {
                return Err(self.git_unusable(OP, "the listing is longer than it may be"));
            }
            entries.push(GitObjectEntry {
                rel: name.to_string(),
                size,
                kind: GitObjectEntryKind::File,
            });
        }
        if entries.len() as u64 != count {
            return Err(self.git_unusable(OP, "the line count does not match n"));
        }
        GitObjectList::from_entries(&entries)
            .map_err(|why| self.mirror_fail(OP, format!("the object listing is refused: {why}")))
    }

    /// Why `dest` cannot take `files`, or `None`: it must be a real directory
    /// (a symlink is not followed) and none of the targets may exist yet.
    fn git_dest_problem(&self, dest: &Path, files: &[GitObjectFile]) -> Result<(), String> {
        match std::fs::symlink_metadata(dest) {
            Ok(meta) if meta.file_type().is_dir() => {}
            Ok(_) => return Err("the destination is not a real directory".to_string()),
            Err(e) => {
                return Err(format!(
                    "the destination cannot be inspected ({:?})",
                    e.kind()
                ));
            }
        }
        for file in files {
            let target = file
                .rel
                .split('/')
                .fold(dest.to_path_buf(), |p, c| p.join(c));
            match std::fs::symlink_metadata(&target) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(format!(
                        "{} cannot be inspected at the destination ({:?})",
                        file.rel,
                        e.kind()
                    ));
                }
                Ok(_) => {
                    return Err(format!(
                        "{} already exists at the destination; a file there is never overwritten",
                        file.rel
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Make `path` a real directory: made when absent, accepted when it is one,
/// refused when it is anything else (a symlink is never followed).
fn ensure_real_dir(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_dir() => Ok(()),
        Ok(_) => Err(format!("{} is not a real directory", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::fs::create_dir(path)
            .map_err(|e| format!("a directory cannot be made ({:?})", e.kind())),
        Err(e) => Err(format!("a directory cannot be inspected ({:?})", e.kind())),
    }
}

/// Move the downloaded `files` from `from/<rel>` to `dest/<rel>`, creating each
/// target exclusively and each directory on the way as a real directory. A
/// hard link (fails when the target exists, copies nothing) is tried first; a
/// host that cannot link across file systems falls back to an exclusive copy.
/// Every target must end as a regular file of exactly its listed size.
fn install_object_files(from: &Path, dest: &Path, files: &[GitObjectFile]) -> Result<(), String> {
    for file in files {
        let parts: Vec<&str> = file.rel.split('/').collect();
        let (mut source, mut target) = (from.to_path_buf(), dest.to_path_buf());
        for (at, part) in parts.iter().enumerate() {
            source.push(part);
            target.push(part);
            if at + 1 < parts.len() {
                ensure_real_dir(&target)?;
            }
        }
        match std::fs::hard_link(&source, &target) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(format!(
                    "{} already exists at the destination; a file there is never overwritten",
                    file.rel
                ));
            }
            Err(_) => {
                let mut input = std::fs::File::open(&source)
                    .map_err(|e| format!("{} cannot be opened ({:?})", file.rel, e.kind()))?;
                let mut output = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)
                    .map_err(|e| format!("{} cannot be created ({:?})", file.rel, e.kind()))?;
                std::io::copy(&mut input, &mut output)
                    .map_err(|e| format!("{} cannot be copied ({:?})", file.rel, e.kind()))?;
            }
        }
        let meta = std::fs::symlink_metadata(&target)
            .map_err(|e| format!("{} cannot be inspected ({:?})", file.rel, e.kind()))?;
        if !meta.file_type().is_file() || meta.len() != file.size {
            return Err(format!(
                "{} is not a regular file of the {} bytes it was listed as",
                file.rel, file.size
            ));
        }
    }
    Ok(())
}

/// The suffix of the lock or temporary sibling a write is staged as: packs go
/// through `.ign-tmp`, every other file through `.lock`. [`winps::GIT_LOCK`]
/// derives the same name from the same rule (anything below `objects/pack/`),
/// and `git_lock_suffix_matches_the_script_rule` pins the two together.
///
/// [`winps::GIT_LOCK`]: crate::deploy::transport::winps::GIT_LOCK
pub(super) fn git_lock_suffix(name: &GitWriteName) -> &'static str {
    match name {
        GitWriteName::PackFile(_) | GitWriteName::PackIndex(_) => ".ign-tmp",
        _ => ".lock",
    }
}

/// One staged file of a `.git` write: the target, the lock or temporary
/// sibling it is written to, and the bytes.
struct StagedWrite {
    target: String,
    lock: String,
    local: std::path::PathBuf,
    size: usize,
}

#[async_trait::async_trait]
impl GitStateWriter for SftpTransport {
    async fn save_git(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<GitManifest, CoreError> {
        self.git_names_check("write", gateway, project, run_id)?;
        if self.windows {
            let script = WinScript::git_save(
                LONG_PATH_PREFIX,
                &self.win_data(),
                project,
                &self.win_home().await?,
                gateway,
                run_id,
            );
            let answer = self.git_win("save_git", script, GIT_HEAVY_LIMIT).await?;
            return self.git_manifest_of("save_git", &answer);
        }
        let home = self.account_home().await?;
        let (rel, dir) = (
            Self::git_rel(project),
            Self::git_save_dir_rel(gateway, project, run_id),
        );
        let answer = self
            .git_run(
                "save_git",
                GIT_SAVE_SH,
                &[&self.data_dir, &rel, &home, &dir],
                GIT_HEAVY_LIMIT,
            )
            .await?;
        self.git_manifest_of("save_git", &answer)
    }

    async fn restore_git(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<GitManifest, CoreError> {
        self.git_names_check("write", gateway, project, run_id)?;
        if self.windows {
            let script = WinScript::git_restore(
                LONG_PATH_PREFIX,
                &self.win_data(),
                project,
                &self.win_home().await?,
                gateway,
                run_id,
            );
            let answer = self.git_win("restore_git", script, GIT_HEAVY_LIMIT).await?;
            return self.git_manifest_of("restore_git", &answer);
        }
        let home = self.account_home().await?;
        let (project_rel, dir) = (
            format!("projects/{project}"),
            Self::git_save_dir_rel(gateway, project, run_id),
        );
        let answer = self
            .git_run(
                "restore_git",
                GIT_RESTORE_SH,
                &[&self.data_dir, &project_rel, &home, &dir, run_id],
                GIT_HEAVY_LIMIT,
            )
            .await?;
        self.git_manifest_of("restore_git", &answer)
    }

    async fn write_git_files(&self, project: &str, writes: &[GitWrite]) -> Result<(), CoreError> {
        if let Some(why) = self.project_name_problem(project) {
            return Err(self.mirror_fail("write", why));
        }
        // Every shape, and every path as an sftp batch line, is checked before
        // the first remote call.
        let git = format!("{}/{}", self.data_dir, Self::git_rel(project));
        let mut plan: Vec<(String, String, &GitWrite)> = Vec::new();
        for write in writes {
            write.name.check_shape().map_err(|why| {
                self.mirror_fail("write", format!("a .git write is refused: {why}"))
            })?;
            let rel = write.name.rel();
            if self.windows {
                for component in rel.split('/') {
                    if let Some(why) = windows_component_problem(component) {
                        return Err(self.mirror_fail(
                            "write",
                            format!("{rel} cannot be a Windows path: {why}"),
                        ));
                    }
                }
            }
            let lock = format!("{rel}{}", git_lock_suffix(&write.name));
            safe_remote_path(&format!("{git}/{lock}")).map_err(|why| {
                self.mirror_fail(
                    "write",
                    format!("{rel} cannot ride an sftp batch line ({why})"),
                )
            })?;
            plan.push((rel, lock, write));
        }
        if plan.is_empty() {
            return Ok(());
        }
        let rel = Self::git_rel(project);
        let limit = self.shell.op_timeout();
        // 1. Every lock exclusively (a lock that exists refuses, nothing is
        //    written), creating the missing parent directories.
        if self.windows {
            let targets: Vec<String> = plan.iter().map(|(target, _, _)| target.clone()).collect();
            let script = WinScript::git_lock(&self.win_git(project), false, &targets);
            self.git_win("write_git_files", script, limit).await?;
        } else {
            let mut lock_args = vec![self.data_dir.as_str(), rel.as_str(), "lock"];
            for (target, lock, _) in &plan {
                lock_args.push(target);
                lock_args.push(lock);
            }
            self.git_run("write_git_files", GIT_LOCK_SH, &lock_args, limit)
                .await?;
        }
        // 2. The bytes over sftp into the locks.
        let staged = match self.stage_git_writes(&plan) {
            Ok(staged) => staged,
            Err(err) => return Err(self.unlock_git_writes(err, project, &rel, &plan).await),
        };
        let ops: Vec<SftpOp> = staged
            .1
            .iter()
            .map(|s| SftpOp::Put {
                local: s.local.clone(),
                remote: format!("{git}/{}", s.lock),
            })
            .collect();
        if let Err(err) = self.run_batch("write", ops).await {
            return Err(self.unlock_git_writes(err, project, &rel, &plan).await);
        }
        // 3. Each lock over its file, after a size check.
        let replaced = if self.windows {
            let writes: Vec<(String, u64)> = staged
                .1
                .iter()
                .map(|s| (s.target.clone(), s.size as u64))
                .collect();
            let script = WinScript::git_replace(&self.win_git(project), &writes, RENAME_RETRIES);
            self.git_win("write_git_files", script, limit)
                .await
                .map(|_| ())
        } else {
            let sizes: Vec<String> = staged.1.iter().map(|s| s.size.to_string()).collect();
            let mut replace_args = vec![self.data_dir.as_str(), rel.as_str()];
            for (s, size) in staged.1.iter().zip(&sizes) {
                replace_args.push(&s.target);
                replace_args.push(&s.lock);
                replace_args.push(size);
            }
            self.git_run("write_git_files", GIT_REPLACE_SH, &replace_args, limit)
                .await
                .map(|_| ())
        };
        if let Err(err) = replaced {
            return Err(self.unlock_git_writes(err, project, &rel, &plan).await);
        }
        drop(staged);
        Ok(())
    }

    async fn remove_git_save(
        &self,
        gateway: &str,
        project: &str,
        run_id: &str,
    ) -> Result<(), CoreError> {
        self.git_names_check("write", gateway, project, run_id)?;
        let rel = git_save_rel(gateway, project, run_id);
        let answer = if self.windows {
            let script =
                WinScript::git_save_rm(LONG_PATH_PREFIX, &self.win_home().await?, &rel, run_id);
            self.git_win("remove_git_save", script, GIT_HEAVY_LIMIT)
                .await?
        } else {
            let home = self.account_home().await?;
            self.git_run(
                "remove_git_save",
                GIT_SAVE_RM_SH,
                &[&home, &rel, run_id],
                GIT_HEAVY_LIMIT,
            )
            .await?
        };
        match answer.get("removed").and_then(serde_json::Value::as_u64) {
            Some(0 | 1) => Ok(()),
            _ => Err(self.git_unusable("remove_git_save", "the removal count")),
        }
    }

    async fn probe_write(&self, project: &str, run_id: &str) -> Result<(), CoreError> {
        if let Some(why) = self.project_name_problem(project) {
            return Err(self.mirror_fail("write", why));
        }
        if RunId::parse(run_id).is_err() {
            return Err(self.mirror_fail(
                "write",
                "the run id is not 1 to 128 characters of [A-Za-z0-9._-]",
            ));
        }
        if self.windows
            && let Some(why) = windows_component_problem(run_id)
        {
            return Err(self.mirror_fail("write", format!("the run id: {why}")));
        }
        let (project_rel, probe) = (format!("projects/{project}"), git_probe_name(run_id));
        if self.windows {
            self.git_win(
                "probe_write",
                WinScript::git_probe(&self.win_data(), project, &probe),
                self.shell.op_timeout(),
            )
            .await?;
            return Ok(());
        }
        self.git_run(
            "probe_write",
            GIT_PROBE_W_SH,
            &[&self.data_dir, &project_rel, &probe],
            self.shell.op_timeout(),
        )
        .await?;
        Ok(())
    }
}

impl SftpTransport {
    /// Write each write's bytes to a private local file, ready for `put`.
    fn stage_git_writes(
        &self,
        plan: &[(String, String, &GitWrite)],
    ) -> Result<(tempfile::TempDir, Vec<StagedWrite>), CoreError> {
        let dir = self.download_dir("write")?;
        let mut staged = Vec::with_capacity(plan.len());
        for (index, (target, lock, write)) in plan.iter().enumerate() {
            let local = dir.path().join(format!("w{index}"));
            {
                use std::io::Write;
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                options
                    .open(&local)
                    .and_then(|mut file| file.write_all(&write.bytes).and_then(|()| file.flush()))
                    .map_err(|e| {
                        self.mirror_fail(
                            "write",
                            format!("{target} cannot be staged locally ({:?})", e.kind()),
                        )
                    })?;
            }
            staged.push(StagedWrite {
                target: target.clone(),
                lock: lock.clone(),
                local,
                size: write.bytes.len(),
            });
        }
        Ok((dir, staged))
    }
}

impl SftpTransport {
    /// Take the locks of a failed write away again (they are this call's: the
    /// lock script created each one exclusively) and return `err`.
    async fn unlock_git_writes(
        &self,
        err: CoreError,
        project: &str,
        rel: &str,
        plan: &[(String, String, &GitWrite)],
    ) -> CoreError {
        if self.windows {
            let targets: Vec<String> = plan.iter().map(|(target, _, _)| target.clone()).collect();
            let script = WinScript::git_lock(&self.win_git(project), true, &targets);
            let _ = self
                .git_win("write_git_files", script, self.shell.op_timeout())
                .await;
            return err;
        }
        let mut args = vec![self.data_dir.as_str(), rel, "unlock"];
        for (target, lock, _) in plan {
            args.push(target);
            args.push(lock);
        }
        let _ = self
            .git_run(
                "write_git_files",
                GIT_LOCK_SH,
                &args,
                self.shell.op_timeout(),
            )
            .await;
        err
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deploy::transport::process::{ProcessOutput, ProcessRunner};
    use crate::deploy::transport::remote::RemoteShell;
    use std::ffi::{OsStr, OsString};
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// Answers each `ssh` call with the next canned output and records argv.
    struct Scripted {
        outs: Mutex<Vec<ProcessOutput>>,
        argv: Mutex<Vec<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl ProcessRunner for Scripted {
        async fn run(
            &self,
            _program: &OsStr,
            args: &[OsString],
            _stdin: Option<Vec<u8>>,
            _timeout: Duration,
        ) -> ProcessOutput {
            self.argv.lock().expect("argv").push(
                args.iter()
                    .map(|a| a.to_string_lossy().into_owned())
                    .collect(),
            );
            self.outs.lock().expect("outs").remove(0)
        }
    }

    fn ok(stdout: &str) -> ProcessOutput {
        ProcessOutput {
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
            code: 0,
            timed_out: false,
        }
    }

    fn answer(projects: &str, project: &str, git: &str) -> ProcessOutput {
        ok(&format!(
            "IGN-RESULT:{{\"projects\":\"{projects}\",\"project\":\"{project}\",\"git\":\"{git}\"}}\n"
        ))
    }

    fn host(outs: Vec<ProcessOutput>, windows: bool) -> (Arc<Scripted>, SftpTransport) {
        let runner = Arc::new(Scripted {
            outs: Mutex::new(outs),
            argv: Mutex::new(Vec::new()),
        });
        let shell = RemoteShell::local_sftp_server(
            Path::new("/unused"),
            runner.clone(),
            Duration::from_secs(3),
        );
        let transport = if windows {
            SftpTransport::new_windows(shell, "/C:/gw/data".to_string(), "nodes.master")
        } else {
            SftpTransport::new(shell, "/gw/data".to_string(), "nodes.master")
        };
        (runner, transport)
    }

    fn detail(err: CoreError) -> String {
        match err {
            CoreError::TransportFailed { detail, .. } => detail,
            other => panic!("expected transport_failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn sftp_git_entry_posix() {
        let cases = [
            ("dir", GitEntry::Directory),
            ("file", GitEntry::File),
            ("symlink", GitEntry::Symlink),
            ("other", GitEntry::Other),
            ("none", GitEntry::Absent),
        ];
        for (word, want) in cases {
            let (runner, t) = host(vec![answer("dir", "dir", word)], false);
            assert_eq!(t.git_entry("CapApp").await.expect(word), want, "{word}");
            // one exec call: a fixed script, the paths as separate arguments
            let argv = runner.argv.lock().expect("argv");
            assert_eq!(argv.len(), 1);
            let command = argv[0].last().expect("remote command");
            assert!(command.starts_with("'sh' '-c' '"), "{command}");
            for path in [
                "'/gw/data/projects'",
                "'/gw/data/projects/CapApp'",
                "'/gw/data/projects/CapApp/.git'",
            ] {
                assert!(command.contains(path), "{path} in {command}");
            }
            assert!(!command.contains("sftp"), "{command}");
        }
        // a missing project directory is absent (the create case)
        let (_, t) = host(vec![answer("dir", "none", "skip")], false);
        assert_eq!(
            t.git_entry("CapApp").await.expect("absent"),
            GitEntry::Absent
        );
    }

    #[tokio::test]
    async fn sftp_git_entry_posix_failures_are_transport_failed_never_absent() {
        // permission denied: the script prints stat's message and exits 3
        let (_, t) = host(
            vec![ProcessOutput {
                stdout: Vec::new(),
                stderr: b"stat: cannot statx '/gw/data/projects/CapApp/.git': Permission denied\n"
                    .to_vec(),
                code: 3,
                timed_out: false,
            }],
            false,
        );
        let why = detail(t.git_entry("CapApp").await.expect_err("denied"));
        assert!(why.contains("Permission denied"), "{why}");
        assert!(why.contains("git_entry"), "{why}");

        // ssh itself failed (255), a timeout, no result line, a doubled line
        let failures = [
            ProcessOutput {
                stdout: Vec::new(),
                stderr: b"ssh: connect to host x port 22: Connection refused\n".to_vec(),
                code: 255,
                timed_out: false,
            },
            ProcessOutput {
                stdout: Vec::new(),
                stderr: Vec::new(),
                code: -1,
                timed_out: true,
            },
            ok("no result here\n"),
            ok("IGN-RESULT:not json\n"),
        ];
        for out in failures {
            let (_, t) = host(vec![out], false);
            let err = t.git_entry("CapApp").await.expect_err("failure");
            assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
        }

        // the projects directory is missing, a symlink or a file
        for (word, needle) in [
            ("none", "does not exist"),
            ("symlink", "symlink"),
            ("file", "not a directory"),
        ] {
            let (_, t) = host(vec![answer(word, "skip", "skip")], false);
            let why = detail(t.git_entry("CapApp").await.expect_err(word));
            assert!(why.contains(needle), "{word}: {why}");
        }
        // the project entry is a symlink or a file
        for (word, needle) in [("symlink", "symlink"), ("file", "not a directory")] {
            let (_, t) = host(vec![answer("dir", word, "skip")], false);
            let why = detail(t.git_entry("CapApp").await.expect_err(word));
            assert!(why.contains(needle), "{word}: {why}");
        }
        // a word the probe does not define is never a kind
        for out in [
            answer("dir", "dir", "weird"),
            answer("dir", "dir", "skip"),
            answer("dir", "maybe", "skip"),
            answer("", "dir", "dir"),
        ] {
            let (_, t) = host(vec![out], false);
            let err = t.git_entry("CapApp").await.expect_err("unknown word");
            assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
        }
    }

    #[tokio::test]
    async fn sftp_git_entry_refuses_unsafe_names_before_any_call() {
        for name in ["", ".", "..", "../x", "a/b", "a\nb", "a\u{7}b"] {
            let (runner, t) = host(Vec::new(), false);
            let err = t.git_entry(name).await.expect_err(name);
            assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
            assert!(runner.argv.lock().expect("argv").is_empty(), "{name:?}");
        }
        // a name with a quote cannot break out of its single-quoted argument
        let (runner, t) = host(vec![answer("dir", "none", "skip")], false);
        let name = "it's$(x)";
        t.git_entry(name).await.expect("absent");
        let argv = runner.argv.lock().expect("argv");
        let command = argv[0].last().expect("command");
        assert!(
            command.contains("'/gw/data/projects/it'\\''s$(x)'"),
            "{command}"
        );
    }

    #[tokio::test]
    async fn sftp_git_entry_windows_name_rules_run_before_the_probe() {
        for name in ["CON", "a:b", "trail.", "a<b"] {
            let (runner, t) = host(Vec::new(), true);
            let err = t.git_entry(name).await.expect_err(name);
            assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
            assert!(runner.argv.lock().expect("argv").is_empty(), "{name:?}");
        }
    }

    fn win_answer(projects: &str, project: &str, kind: &str, hidden: i64, reparse: i64) -> String {
        format!(
            "IGN-RESULT:{{\"ok\":1,\"projects\":\"{projects}\",\"project\":\"{project}\",\"kind\":\"{kind}\",\"hidden\":{hidden},\"reparse\":{reparse}}}\n"
        )
    }

    #[tokio::test]
    async fn win_git_entry_parse() {
        // (kind, hidden, reparse) of the `.git` entry
        let cases = [
            ("dir", 1, 0, GitEntry::Directory),
            ("dir", 0, 0, GitEntry::Directory),
            ("dir", 1, 1, GitEntry::Symlink),
            ("dir", 0, 1, GitEntry::Symlink),
            ("file", 0, 0, GitEntry::File),
            ("file", 1, 0, GitEntry::File),
            ("file", 0, 1, GitEntry::Symlink),
            ("none", 0, 0, GitEntry::Absent),
        ];
        for (kind, hidden, reparse, want) in cases {
            let (runner, t) = host(
                vec![ok(&win_answer("dir", "dir", kind, hidden, reparse))],
                true,
            );
            assert_eq!(
                t.git_entry("CapApp").await.expect(kind),
                want,
                "{kind} hidden={hidden} reparse={reparse}"
            );
            assert_eq!(runner.argv.lock().expect("argv").len(), 1, "one call");
        }
        // a missing project directory is absent (the create case)
        let (_, t) = host(vec![ok(&win_answer("dir", "none", "skip", 0, 0))], true);
        assert_eq!(
            t.git_entry("CapApp").await.expect("absent"),
            GitEntry::Absent
        );
    }

    #[tokio::test]
    async fn win_git_entry_failures_are_transport_failed_never_absent() {
        let failures = [
            // the script died (UnauthorizedAccessException under Stop): exit 1
            ProcessOutput {
                stdout: Vec::new(),
                stderr: b"Get-Item : Access to the path 'C:\\x\\.git' is denied.\n".to_vec(),
                code: 1,
                timed_out: false,
            },
            ProcessOutput {
                stdout: Vec::new(),
                stderr: b"ssh: connect to host x port 22: Connection refused\n".to_vec(),
                code: 255,
                timed_out: false,
            },
            ProcessOutput {
                stdout: Vec::new(),
                stderr: Vec::new(),
                code: -1,
                timed_out: true,
            },
            ok("no result here\n"),
            ok("IGN-RESULT:not json\n"),
            ok("IGN-RESULT:{\"ok\":0}\n"),
            ok("IGN-RESULT:{}\n"),
            // an exit 0 with an error record on stderr is a failure too
            ProcessOutput {
                stdout: win_answer("dir", "dir", "none", 0, 0).into_bytes(),
                stderr: b"#< CLIXML\n<Objs><S S=\"Error\">denied</S></Objs>".to_vec(),
                code: 0,
                timed_out: false,
            },
            // fields the script does not define
            ok(&win_answer("dir", "dir", "weird", 0, 0)),
            ok(&win_answer("dir", "dir", "skip", 0, 0)),
            ok(&win_answer("dir", "maybe", "skip", 0, 0)),
            ok(&win_answer("", "dir", "dir", 0, 0)),
            ok(&win_answer("dir", "dir", "dir", 2, 0)),
            ok(&win_answer("dir", "dir", "dir", 0, 7)),
            ok("IGN-RESULT:{\"ok\":1,\"projects\":\"dir\",\"project\":\"dir\",\"kind\":\"dir\"}\n"),
        ];
        for out in failures {
            let (_, t) = host(vec![out], true);
            let err = t.git_entry("CapApp").await.expect_err("failure");
            assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
        }
        // levels above `.git` that are not plain directories
        for (projects, project, needle) in [
            ("none", "skip", "does not exist"),
            ("symlink", "skip", "junction or symlink"),
            ("file", "skip", "not a directory"),
            ("dir", "symlink", "junction or symlink"),
            ("dir", "file", "not a directory"),
        ] {
            let (_, t) = host(vec![ok(&win_answer(projects, project, "skip", 0, 0))], true);
            let why = detail(t.git_entry("CapApp").await.expect_err(needle));
            assert!(why.contains(needle), "{projects}/{project}: {why}");
        }
    }

    #[tokio::test]
    async fn win_git_entry_script_is_fixed_text() {
        use crate::deploy::transport::winps::{COMMAND_BUDGET, GIT_ENTRY, encoded_command};

        // the body is a constant: no host or repo value is in it, and it never
        // uses the probes C-H ruled out
        assert!(!GIT_ENTRY.chars().any(|c| c == '\n'), "one line");
        assert!(GIT_ENTRY.contains("Get-Item -LiteralPath $x -Force"));
        for forbidden in [
            "Test-Path",
            "Exists(",
            "-Recurse",
            "Remove-",
            "Set-",
            "New-",
        ] {
            assert!(!GIT_ENTRY.contains(forbidden), "{forbidden} in GIT_ENTRY");
        }
        // an error other than ObjectNotFound is rethrown, never read as none
        assert!(GIT_ENTRY.contains("'ObjectNotFound'){return"));
        assert!(GIT_ENTRY.contains(";throw}"));

        // a hostile project name never reaches the script text: it is refused
        // by the component gate before anything is built, and a legal but odd
        // name travels in the base64 parameters
        let odd = "it's$(x)`;y";
        let script = WinScript::git_entry(
            "C:\\gw\\data\\projects",
            &format!("C:\\gw\\data\\projects\\{odd}"),
            &format!("C:\\gw\\data\\projects\\{odd}\\.git"),
        );
        assert_eq!(script.body, GIT_ENTRY);
        let command = encoded_command(&script, COMMAND_BUDGET).expect("fits the budget");
        assert!(command.as_str().chars().count() <= COMMAND_BUDGET);
        assert!(!command.as_str().contains("it's"));
        // a long realistic path set still fits the budget
        let long = format!(
            "C:\\Program Files\\Inductive Automation\\Ignition\\data\\projects\\{}",
            "P".repeat(100)
        );
        let script = WinScript::git_entry(
            "C:\\Program Files\\Inductive Automation\\Ignition\\data\\projects",
            &long,
            &format!("{long}\\.git"),
        );
        assert!(encoded_command(&script, COMMAND_BUDGET).is_ok());

        // through the transport: the paths are native, the sftp spelling is gone
        let (runner, t) = host(vec![ok(&win_answer("dir", "none", "skip", 0, 0))], true);
        t.git_entry("CapApp").await.expect("absent");
        let argv = runner.argv.lock().expect("argv");
        let text = argv[0].join(" ");
        assert!(text.contains("-EncodedCommand"), "{text}");
        assert!(
            !text.contains("CapApp"),
            "name is inside the encoding: {text}"
        );
    }

    #[test]
    fn git_probe_script_is_one_fixed_line() {
        assert!(!GIT_PROBE_SH.chars().any(char::is_control), "one line");
        assert!(GIT_PROBE_SH.contains("stat -c %F --"));
        assert!(
            !GIT_PROBE_SH.contains("stat -L"),
            "a symlink is reported, never followed"
        );
        assert!(
            !GIT_PROBE_SH.contains("test -e"),
            "C-H: test -e says absent"
        );
        assert!(GIT_PROBE_SH.contains("LC_ALL=C"));
    }

    /// Runs the real probe text under the local `sh` with a `stat` shim that
    /// answers like GNU `stat -c %F -- PATH` (the messages are the ones
    /// `## C-H before grant` captured), so the script's own branching is
    /// exercised on any unix host. A path with `/denied/` in it answers
    /// `Permission denied`; the `.git` of a project named `special*` is a
    /// socket.
    #[cfg(unix)]
    mod script {
        use super::*;
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        const SHIM: &str = "#!/bin/sh\n\
            p=$4\n\
            case \"$p\" in */denied/*) echo \"stat: cannot statx '$p': Permission denied\" >&2; exit 1;; esac\n\
            if [ -L \"$p\" ]; then echo 'symbolic link'\n\
            elif [ -d \"$p\" ]; then echo directory\n\
            elif [ -f \"$p\" ] && [ ! -s \"$p\" ]; then echo 'regular empty file'\n\
            elif [ -f \"$p\" ]; then echo 'regular file'\n\
            else case \"$p\" in */special*/.git) echo socket;; *) echo \"stat: cannot statx '$p': No such file or directory\" >&2; exit 1;; esac\n\
            fi\n";

        struct Run {
            code: i32,
            stdout: String,
            stderr: String,
        }

        /// The probe for `project` below `root/projects`.
        fn run(root: &Path, project: &str) -> Run {
            let bin = root.join("bin");
            std::fs::create_dir_all(&bin).expect("bin");
            let shim = bin.join("stat");
            std::fs::write(&shim, SHIM).expect("shim");
            std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).expect("chmod");
            let projects = root.join("projects");
            let dir = projects.join(project);
            let path = format!(
                "{}:{}",
                bin.display(),
                std::env::var("PATH").unwrap_or_default()
            );
            let out = Command::new("sh")
                .args(["-c", GIT_PROBE_SH, GIT_PROBE_NAME])
                .arg(&projects)
                .arg(&dir)
                .arg(dir.join(".git"))
                .env("PATH", path)
                .output()
                .expect("sh");
            Run {
                code: out.status.code().expect("exit code"),
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            }
        }

        fn words(run: &Run) -> serde_json::Value {
            assert_eq!(run.code, 0, "{}", run.stderr);
            let line = run.stdout.trim_end();
            serde_json::from_str(line.strip_prefix("IGN-RESULT:").expect("sentinel")).expect("json")
        }

        #[test]
        fn probe_script_classifies_every_shape() {
            let tmp = tempfile::tempdir().expect("tmp");
            let root = tmp.path();
            std::fs::create_dir_all(root.join("projects")).expect("projects");

            // no project directory: level 2 is none, `.git` is never asked
            assert_eq!(
                words(&run(root, "CapApp")),
                serde_json::json!({"projects":"dir","project":"none","git":"skip"})
            );

            // a project directory without `.git`
            std::fs::create_dir_all(root.join("projects/CapApp")).expect("app");
            assert_eq!(
                words(&run(root, "CapApp")),
                serde_json::json!({"projects":"dir","project":"dir","git":"none"})
            );

            let git = root.join("projects/CapApp/.git");
            std::fs::create_dir(&git).expect("git dir");
            assert_eq!(words(&run(root, "CapApp"))["git"], "dir");
            std::fs::remove_dir(&git).expect("rm");
            std::fs::write(&git, "gitdir: x").expect("git file");
            assert_eq!(words(&run(root, "CapApp"))["git"], "file");
            std::fs::write(&git, "").expect("empty");
            assert_eq!(words(&run(root, "CapApp"))["git"], "file");
            std::fs::remove_file(&git).expect("rm");
            // a dangling symlink is reported, not followed
            std::os::unix::fs::symlink("nowhere", &git).expect("link");
            assert_eq!(words(&run(root, "CapApp"))["git"], "symlink");

            // the project entry is a file or a symlink
            std::fs::write(root.join("projects/AFile"), "x").expect("file");
            assert_eq!(words(&run(root, "AFile"))["project"], "file");
            std::os::unix::fs::symlink("CapApp", root.join("projects/ALink")).expect("link");
            let w = words(&run(root, "ALink"));
            assert_eq!(w["project"], "symlink");
            assert_eq!(w["git"], "skip");

            // another kind of entry
            std::fs::create_dir_all(root.join("projects/special1")).expect("dir");
            assert_eq!(words(&run(root, "special1"))["git"], "other");
        }

        #[test]
        fn probe_script_denied_is_a_failure_never_none() {
            let tmp = tempfile::tempdir().expect("tmp");
            let root = tmp.path();
            std::fs::create_dir_all(root.join("projects/denied")).expect("dir");
            let r = run(root, "denied");
            assert_eq!(r.code, 3, "{}", r.stderr);
            assert!(r.stderr.contains("Permission denied"), "{}", r.stderr);
            assert!(!r.stdout.contains("IGN-RESULT"), "{}", r.stdout);
        }

        #[test]
        fn probe_script_missing_projects_directory_is_none_at_level_one() {
            let tmp = tempfile::tempdir().expect("tmp");
            assert_eq!(
                words(&run(tmp.path(), "CapApp")),
                serde_json::json!({"projects":"none","project":"skip","git":"skip"})
            );
        }
    }

    /// Every git script is one fixed line: a remote command argument may hold
    /// no control character, and a script that held one could never be sent.
    #[test]
    fn git_scripts_are_one_fixed_line() {
        for (name, script) in ALL_GIT_SCRIPTS {
            assert!(
                !script.chars().any(char::is_control),
                "{name} holds a control character"
            );
            assert!(script.starts_with("LC_ALL=C; export LC_ALL;"), "{name}");
            assert!(!script.contains("eval "), "{name} must not eval");
            assert!(
                !script.contains("$(cat") && !script.contains("`"),
                "{name} must not interpolate a file or use backticks"
            );
        }
    }

    const ALL_GIT_SCRIPTS: [(&str, &str); 12] = [
        ("GIT_OBJECTS_SH", GIT_OBJECTS_SH),
        ("GIT_STATE_SH", GIT_STATE_SH),
        ("GIT_MANIFEST_SH", GIT_MANIFEST_SH),
        ("GIT_SIZE_SH", GIT_SIZE_SH),
        ("GIT_DIGEST_SH", GIT_DIGEST_SH),
        ("GIT_SAVE_SH", GIT_SAVE_SH),
        ("GIT_RESTORE_SH", GIT_RESTORE_SH),
        ("GIT_PROBE_W_SH", GIT_PROBE_W_SH),
        ("GIT_LOCK_SH", GIT_LOCK_SH),
        ("GIT_REPLACE_SH", GIT_REPLACE_SH),
        ("GIT_SAVE_RM_SH", GIT_SAVE_RM_SH),
        ("GIT_FREE_SH", GIT_FREE_SH),
    ];

    /// The parameters a Windows git script was sent with: the base64 JSON blob
    /// inside the UTF-16LE `-EncodedCommand` of one recorded ssh argv.
    fn win_params(argv: &[String]) -> serde_json::Value {
        use base64::Engine as _;
        let command = argv.last().expect("a command");
        let encoded = command
            .strip_prefix("powershell -NoProfile -NonInteractive -EncodedCommand ")
            .unwrap_or_else(|| panic!("not an encoded command: {command}"));
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .expect("base64");
        let units: Vec<u16> = bytes
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let text = String::from_utf16(&units).expect("utf16");
        let from = text.find("FromBase64String('").expect("params") + "FromBase64String('".len();
        let blob = &text[from..from + text[from..].find('\'').expect("end")];
        let json = base64::engine::general_purpose::STANDARD
            .decode(blob)
            .expect("blob");
        serde_json::from_slice(&json).expect("json")
    }

    const WIN_MANIFEST: &str = "IGN-RESULT:{\"ok\":1,\"files\":3,\"bytes\":42,\"digest\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}\n";

    fn win_host(outs: Vec<ProcessOutput>) -> (Arc<Scripted>, SftpTransport) {
        let (runner, t) = host(outs, true);
        (runner, t.with_home("/C:/Users/svc"))
    }

    /// 27-09: every Windows arm is one script over the encoded PowerShell
    /// route, with native paths and the names as data, and nothing of the
    /// sftp spelling left.
    #[tokio::test]
    async fn sftp_git_windows_arms_run_one_script_each() {
        let name = GitWriteName::Head;
        // (what ran, its answer, the parameters it must carry)
        let (runner, t) = win_host(vec![ok(WIN_MANIFEST)]);
        let m = t.git_manifest("CapApp").await.expect("manifest");
        assert_eq!((m.files, m.bytes), (3, 42));
        let p = win_params(&runner.argv.lock().expect("argv")[0]);
        assert_eq!(p["a"], "C:\\gw\\data\\projects\\CapApp\\.git");
        assert_eq!(p["x"], "\\\\?\\");

        let (runner, t) = win_host(vec![ok(WIN_MANIFEST)]);
        t.save_manifest("gw", "CapApp", "r1")
            .await
            .expect("save manifest");
        let p = win_params(&runner.argv.lock().expect("argv")[0]);
        assert_eq!(
            p["a"],
            "C:\\Users\\svc\\ign-deploy\\gw\\projects\\CapApp\\git-save\\r1\\repo"
        );

        let (runner, t) = win_host(vec![ok(WIN_MANIFEST)]);
        t.save_git("gw", "CapApp", "r1").await.expect("save");
        let p = win_params(&runner.argv.lock().expect("argv")[0]);
        for (key, want) in [
            ("d", "C:\\gw\\data"),
            ("j", "CapApp"),
            ("h", "C:\\Users\\svc"),
            ("g", "gw"),
            ("i", "r1"),
        ] {
            assert_eq!(p[key], want, "{key}");
        }

        let (runner, t) = win_host(vec![ok(WIN_MANIFEST)]);
        t.restore_git("gw", "CapApp", "r1").await.expect("restore");
        assert_eq!(runner.argv.lock().expect("argv").len(), 1);

        let (runner, t) = win_host(vec![ok(
            "IGN-RESULT:{\"ok\":1,\"digest\":\"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"}\n",
        )]);
        let digest = t.git_file_digest("CapApp", &name).await.expect("digest");
        assert_eq!(digest.as_deref().map(str::len), Some(64));
        let p = win_params(&runner.argv.lock().expect("argv")[0]);
        assert_eq!(p["f"], "HEAD");
        let (_, t) = win_host(vec![ok("IGN-RESULT:{\"ok\":1,\"digest\":null}\n")]);
        assert_eq!(
            t.git_file_digest("CapApp", &name).await.expect("none"),
            None
        );

        let (_, t) = win_host(vec![ok("IGN-RESULT:{\"ok\":1,\"files\":7,\"bytes\":9}\n")]);
        let size = t.git_size("CapApp").await.expect("size");
        assert_eq!((size.files, size.bytes), (7, 9));

        let (runner, t) = win_host(vec![ok("IGN-RESULT:{\"ok\":1,\"free\":123}\n")]);
        assert_eq!(t.free_bytes("gw", "CapApp").await.expect("free"), 123);
        let p = win_params(&runner.argv.lock().expect("argv")[0]);
        assert_eq!(p["s"], "ign-deploy/gw/projects/CapApp/git-save");

        let (runner, t) = win_host(vec![ok("IGN-RESULT:{\"ok\":1,\"removed\":1}\n")]);
        t.remove_git_save("gw", "CapApp", "r1")
            .await
            .expect("remove");
        let p = win_params(&runner.argv.lock().expect("argv")[0]);
        assert_eq!(p["r"], "ign-deploy/gw/projects/CapApp/git-save/r1/repo");
        assert_eq!(p["i"], "r1");

        let (runner, t) = win_host(vec![ok("IGN-RESULT:{\"ok\":1}\n")]);
        t.probe_write("CapApp", "r1").await.expect("probe");
        let p = win_params(&runner.argv.lock().expect("argv")[0]);
        assert_eq!(p["d"], "C:\\gw\\data");
        assert_eq!(p["j"], "CapApp");
        assert_eq!(p["n"], ".ign-probe-r1");

        // state: the probe, then one script, with the marker names as data
        let state = "IGN-RESULT:{\"ok\":1,\"head\":{\"n\":3,\"b\":\"eHgK\"},\"config\":null,\"packed\":null,\"log\":null,\"index\":true,\"markers\":[\"shallow\"],\"refs\":[]}\n";
        let (runner, t) = win_host(vec![ok(&win_answer("dir", "dir", "dir", 1, 0)), ok(state)]);
        let files = t.read_git_state("CapApp").await.expect("state");
        assert!(files.index_present);
        assert_eq!(files.markers, ["shallow"]);
        assert_eq!(runner.argv.lock().expect("argv").len(), 2);
        let p = win_params(&runner.argv.lock().expect("argv")[1]);
        assert_eq!(p["m"].as_array().map(Vec::len), Some(GIT_MARKERS.len()));
        // a `.git` that is not a directory is answered by kind, never read
        let (runner, t) = win_host(vec![ok(&win_answer("dir", "dir", "file", 0, 0))]);
        let files = t.read_git_state("CapApp").await.expect("file");
        assert_eq!(files, GitFiles::without_contents(GitEntry::File));
        assert_eq!(runner.argv.lock().expect("argv").len(), 1);
    }

    /// Every Windows arm refuses an answer that does not say `ok: 1` with the
    /// fields it needs, and a failed script is `transport_failed`.
    #[tokio::test]
    async fn sftp_git_windows_answers_are_checked_strictly() {
        let write = [GitWrite {
            name: GitWriteName::Head,
            bytes: vec![1],
        }];
        let name = GitWriteName::Head;
        let bad_answers = [
            "IGN-RESULT:{\"ok\":0}\n",
            "IGN-RESULT:{}\n",
            "IGN-RESULT:{\"ok\":1}\n",
            "IGN-RESULT:{\"ok\":1,\"files\":3,\"bytes\":42}\n",
            "IGN-RESULT:{\"ok\":1,\"files\":3,\"bytes\":42,\"digest\":\"short\"}\n",
            "IGN-RESULT:{\"ok\":1,\"digest\":\"nothex\"}\n",
            "IGN-RESULT:{\"ok\":1,\"free\":\"a lot\"}\n",
            "no result\n",
        ];
        for answer in bad_answers {
            let (_, t) = win_host(vec![ok(answer)]);
            assert!(t.git_manifest("CapApp").await.is_err(), "manifest {answer}");
            if !answer.contains("\"files\":3") {
                // (an answer with a count and a size is a good size answer)
                let (_, t) = win_host(vec![ok(answer)]);
                assert!(t.git_size("CapApp").await.is_err(), "size {answer}");
            }
            let (_, t) = win_host(vec![ok(answer)]);
            assert!(
                t.save_git("gw", "CapApp", "r1").await.is_err(),
                "save {answer}"
            );
            let (_, t) = win_host(vec![ok(answer)]);
            assert!(
                t.restore_git("gw", "CapApp", "r1").await.is_err(),
                "restore {answer}"
            );
            let (_, t) = win_host(vec![ok(answer)]);
            assert!(
                t.git_file_digest("CapApp", &name).await.is_err(),
                "digest {answer}"
            );
            let (_, t) = win_host(vec![ok(answer)]);
            assert!(t.free_bytes("gw", "CapApp").await.is_err(), "free {answer}");
        }
        for answer in ["IGN-RESULT:{\"ok\":0}\n", "IGN-RESULT:{\"ok\":1}\n", "x\n"] {
            // `removed` must be 0 or 1; the probe and the lock need ok == 1
            let (_, t) = win_host(vec![ok(answer)]);
            let removed = t.remove_git_save("gw", "CapApp", "r1").await;
            assert!(removed.is_err(), "remove {answer}");
        }
        let (_, t) = win_host(vec![ok("IGN-RESULT:{\"ok\":0}\n")]);
        assert!(t.probe_write("CapApp", "r1").await.is_err());
        let (_, t) = win_host(vec![ok("IGN-RESULT:{\"ok\":0}\n")]);
        assert!(t.write_git_files("CapApp", &write).await.is_err());
        // a state answer with a marker the host may not report, or no index
        let bad_state = [
            "IGN-RESULT:{\"ok\":1,\"head\":null,\"config\":null,\"packed\":null,\"log\":null,\"index\":true,\"markers\":[\"evil\"],\"refs\":[]}\n",
            "IGN-RESULT:{\"ok\":1,\"head\":null,\"config\":null,\"packed\":null,\"log\":null,\"markers\":[],\"refs\":[]}\n",
            "IGN-RESULT:{\"ok\":1,\"head\":{\"n\":9,\"b\":\"eHgK\"},\"config\":null,\"packed\":null,\"log\":null,\"index\":true,\"markers\":[],\"refs\":[]}\n",
        ];
        for answer in bad_state {
            let (_, t) = win_host(vec![ok(&win_answer("dir", "dir", "dir", 1, 0)), ok(answer)]);
            assert!(t.read_git_state("CapApp").await.is_err(), "state {answer}");
        }
        // a script that died is transport_failed
        let died = ProcessOutput {
            stdout: Vec::new(),
            stderr: b"Exception: the project already has a .git\n".to_vec(),
            code: 1,
            timed_out: false,
        };
        let (_, t) = win_host(vec![died]);
        let err = t.restore_git("gw", "CapApp", "r1").await.expect_err("died");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
    }

    fn objects_answer(lines: &[&str], n: usize, more: i32) -> ProcessOutput {
        ok(&format!(
            "IGN-RESULT:{{\"ok\":1,\"n\":{n},\"more\":{more},\"t\":\"{}\"}}\n",
            lines.join("\\n")
        ))
    }

    const LOOSE_A: &str = "ab/cdef0123456789abcdef0123456789abcdef01";
    const PACK_A: &str = "pack/pack-0123456789abcdef0123456789abcdef01234567";

    /// 27-11: the listing is one script whose answer is parsed strictly; the
    /// arguments are the data directory, the path and the two caps, and
    /// nothing of the project is in the script text.
    #[tokio::test]
    async fn sftp_git_objects_posix_arm_runs_one_script_and_parses_strictly() {
        let pack = |ext: &str| format!("100 objects/{PACK_A}.{ext}");
        let (runner, t) = host(
            vec![objects_answer(
                &[
                    &format!("12 objects/{LOOSE_A}"),
                    &pack("pack"),
                    &format!("{} ", 7).replace(' ', ""),
                ],
                3,
                0,
            )],
            false,
        );
        // a line that is not `size name` is unusable, never skipped
        let err = t.list_git_objects("CapApp").await.expect_err("bad line");
        assert!(detail(err).contains("unusable"), "bad line");
        let argv = runner.argv.lock().expect("argv").clone();
        assert_eq!(argv.len(), 1, "one exec, no sftp session");
        let command = argv[0].last().expect("command").clone();
        assert!(!command.chars().any(char::is_control), "one line");
        assert!(command.contains("/gw/data"));
        assert!(command.contains("projects/CapApp/.git/objects"));
        assert!(command.contains(&GIT_OBJECTS_LIST_MAX_ENTRIES.to_string()));
        assert!(command.contains(&GIT_OBJECTS_REPORT_MAX.to_string()));

        let good = |lines: &[&str], n, more| host(vec![objects_answer(lines, n, more)], false).1;
        let loose = format!("12 objects/{LOOSE_A}");
        // the good answer builds the list
        let list = good(&[&loose, &pack("pack"), &pack("idx")], 3, 0)
            .list_git_objects("CapApp")
            .await
            .expect("list");
        assert_eq!(list.loose.len(), 1);
        assert_eq!(list.packs.len(), 1);
        assert_eq!(list.packs[0].pack_size, 100);
        // an answer cut off at the report cap is refused, never read as the repo
        let err = good(&[&loose], 1, 1).list_git_objects("CapApp").await;
        assert!(detail(err.expect_err("more")).contains("more object files"));
        // a count that does not add up, a hostile name, a listed path twice
        for (lines, n) in [
            (vec![loose.clone()], 2usize),
            (vec!["5 objects/../x".to_string()], 1),
            (vec!["5 ../x".to_string(), "5 x".to_string()], 2),
            (vec![loose.clone(), loose.clone()], 2),
            (vec!["-5 objects/x".to_string()], 1),
            (vec!["x objects/y".to_string()], 1),
        ] {
            let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
            let got = good(&refs, n, 0).list_git_objects("CapApp").await;
            assert!(got.is_err(), "{lines:?}");
        }
        // an answer without its fields is unusable
        let (_, t) = host(vec![ok("IGN-RESULT:{\"n\":0}\n")], false);
        assert!(t.list_git_objects("CapApp").await.is_err());
    }

    /// 27-11: a Windows node lists through one WinScript whose parameters are
    /// the native `.git`, the walk cap and the report cap, and nothing else.
    #[tokio::test]
    async fn sftp_git_objects_windows_arm_runs_one_script() {
        let (runner, t) = win_host(vec![objects_answer(
            &[&format!("12 objects/{LOOSE_A}")],
            1,
            0,
        )]);
        let list = t.list_git_objects("CapApp").await.expect("list");
        assert_eq!(list.loose.len(), 1);
        let argv = runner.argv.lock().expect("argv").clone();
        assert_eq!(argv.len(), 1);
        let p = win_params(&argv[0]);
        assert_eq!(p["a"], "C:\\gw\\data\\projects\\CapApp\\.git");
        assert_eq!(p["x"], "\\\\?\\");
        assert_eq!(p["l"], GIT_OBJECTS_LIST_MAX_ENTRIES);
        assert_eq!(p["m"], GIT_OBJECTS_REPORT_MAX);
        // `ok` must be 1 and the answer must be whole
        for answer in [
            "IGN-RESULT:{\"ok\":0}\n",
            "IGN-RESULT:{\"ok\":1,\"n\":0}\n",
            "IGN-RESULT:{\"ok\":1,\"n\":0,\"more\":0}\n",
        ] {
            let (_, t) = win_host(vec![ok(answer)]);
            assert!(t.list_git_objects("CapApp").await.is_err(), "{answer}");
        }
        // an empty repo is an empty list, not an error
        let (_, t) = win_host(vec![ok(
            "IGN-RESULT:{\"ok\":1,\"n\":0,\"more\":0,\"t\":\"\"}\n",
        )]);
        let empty = t.list_git_objects("CapApp").await.expect("empty");
        assert_eq!(empty, GitObjectList::default());
        // a script that died is transport_failed
        let died = ProcessOutput {
            stdout: Vec::new(),
            stderr: b"Exception: objects holds a link\n".to_vec(),
            code: 1,
            timed_out: false,
        };
        let (_, t) = win_host(vec![died]);
        let err = t.list_git_objects("CapApp").await.expect_err("died");
        assert!(matches!(err, CoreError::TransportFailed { .. }), "{err:?}");
    }

    /// 27-11: a project name the host cannot hold, and every name of a fetch,
    /// is refused before any process starts.
    #[tokio::test]
    async fn sftp_git_objects_names_are_checked_before_any_process() {
        for windows in [false, true] {
            let (runner, t) = host(Vec::new(), windows);
            let dest = tempfile::tempdir().expect("dest");
            let good = GitObjectFile {
                rel: format!("objects/{LOOSE_A}"),
                size: 1,
            };
            assert!(t.list_git_objects("..").await.is_err());
            assert!(t.list_git_objects("a/b").await.is_err());
            assert!(
                t.fetch_git_objects("CapApp/..", std::slice::from_ref(&good), dest.path())
                    .await
                    .is_err()
            );
            for rel in [
                "objects/../../config",
                "config",
                "hooks/pre-commit",
                "objects/info/alternates",
                "objects/pack/pack-xyz.pack",
                "/etc/passwd",
                "objects/ab/cd:ef",
                "objects\\ab\\cdef",
                "objects/ab/cdef\n",
                "",
            ] {
                let bad = GitObjectFile {
                    rel: rel.to_string(),
                    size: 1,
                };
                let got = t
                    .fetch_git_objects("CapApp", &[good.clone(), bad], dest.path())
                    .await;
                assert!(got.is_err(), "{rel:?}");
            }
            // the same rel twice, and a total over the cap
            let got = t
                .fetch_git_objects("CapApp", &[good.clone(), good.clone()], dest.path())
                .await;
            assert!(got.is_err());
            let huge = GitObjectFile {
                rel: format!("objects/{LOOSE_A}"),
                size: GIT_OBJECTS_FETCH_MAX_BYTES + 1,
            };
            assert!(
                t.fetch_git_objects("CapApp", &[huge], dest.path())
                    .await
                    .is_err()
            );
            assert!(
                runner.argv.lock().expect("argv").is_empty(),
                "a refusal started a process (windows {windows})"
            );
        }
    }

    /// The names are checked on the Windows rules before any script runs.
    #[tokio::test]
    async fn sftp_git_windows_names_are_checked_before_any_script() {
        for hostile in ["CON", "a:b", "trail.", "a<b", "a/b", ".."] {
            let (runner, t) = win_host(Vec::new());
            let write = [GitWrite {
                name: GitWriteName::Head,
                bytes: vec![1],
            }];
            assert!(t.git_manifest(hostile).await.is_err(), "{hostile}");
            assert!(t.git_size(hostile).await.is_err(), "{hostile}");
            assert!(t.read_git_state(hostile).await.is_err(), "{hostile}");
            assert!(t.save_git("gw", hostile, "r1").await.is_err(), "{hostile}");
            assert!(
                t.save_git(hostile, "CapApp", "r1").await.is_err(),
                "{hostile}"
            );
            assert!(
                t.save_git("gw", "CapApp", hostile).await.is_err(),
                "{hostile}"
            );
            assert!(
                t.restore_git("gw", hostile, "r1").await.is_err(),
                "{hostile}"
            );
            assert!(
                t.remove_git_save("gw", "CapApp", hostile).await.is_err(),
                "{hostile}"
            );
            assert!(t.probe_write(hostile, "r1").await.is_err(), "{hostile}");
            assert!(
                t.write_git_files(hostile, &write).await.is_err(),
                "{hostile}"
            );
            assert!(
                runner.argv.lock().expect("argv").is_empty(),
                "{hostile:?} started a process"
            );
        }
        // a ref name Windows cannot hold is refused before any lock is made
        let (runner, t) = win_host(Vec::new());
        let write = [GitWrite {
            name: GitWriteName::BranchRef("con".to_string()),
            bytes: vec![1],
        }];
        let err = t.write_git_files("CapApp", &write).await.expect_err("con");
        assert!(detail(err).contains("Windows"), "names the Windows rule");
        assert!(runner.argv.lock().expect("argv").is_empty());
    }

    /// The lock name Rust uploads to is the name the script locks: packs
    /// through `.ign-tmp`, everything else through `.lock`, decided by the
    /// `objects/pack/` prefix on both sides.
    #[test]
    fn git_lock_suffix_matches_the_script_rule() {
        use crate::deploy::transport::winps::{GIT_LOCK, GIT_REPLACE};
        let hash = "a".repeat(40);
        let packs = [
            GitWriteName::PackFile(hash.clone()),
            GitWriteName::PackIndex(hash),
        ];
        let others = [
            GitWriteName::Head,
            GitWriteName::Config,
            GitWriteName::Index,
            GitWriteName::BranchRef("feature/x".to_string()),
            GitWriteName::RemoteRef {
                remote: "origin".to_string(),
                branch: "main".to_string(),
            },
        ];
        for name in &packs {
            assert_eq!(git_lock_suffix(name), ".ign-tmp");
            assert!(name.rel().starts_with("objects/pack/"));
        }
        for name in &others {
            assert_eq!(git_lock_suffix(name), ".lock");
            assert!(!name.rel().starts_with("objects/pack/"));
        }
        for script in [GIT_LOCK, GIT_REPLACE] {
            assert!(
                script.contains("if($t.StartsWith('objects/pack/')){'.ign-tmp'}else{'.lock'}"),
                "the script derives the lock name by the same rule"
            );
        }
    }

    /// A Windows write is the lock script, one sftp batch and the replace
    /// script (the sftp client cannot create a file exclusively), and a failed
    /// replace takes the locks away again.
    #[tokio::test]
    async fn sftp_git_windows_write_is_lock_put_replace() {
        let write = [GitWrite {
            name: GitWriteName::BranchRef("production".to_string()),
            bytes: b"0123\n".to_vec(),
        }];
        let ok_answer = || ok("IGN-RESULT:{\"ok\":1}\n");
        let (runner, t) = win_host(vec![ok_answer(), ok(""), ok_answer()]);
        t.write_git_files("CapApp", &write).await.expect("write");
        let argv = runner.argv.lock().expect("argv").clone();
        assert_eq!(argv.len(), 3, "lock, put, replace");
        let lock = win_params(&argv[0]);
        assert_eq!(lock["m"], "lock");
        assert_eq!(lock["w"][0], "refs/heads/production");
        let replace = win_params(&argv[2]);
        assert_eq!(replace["w"][0], "refs/heads/production:5");
        assert_eq!(replace["q"], RENAME_RETRIES);

        // a replace that fails unlocks: lock, put, replace (fails), unlock
        let (runner, t) = win_host(vec![
            ok_answer(),
            ok(""),
            ok("IGN-RESULT:{\"ok\":0}\n"),
            ok_answer(),
        ]);
        assert!(t.write_git_files("CapApp", &write).await.is_err());
        let argv = runner.argv.lock().expect("argv").clone();
        assert_eq!(argv.len(), 4);
        assert_eq!(win_params(&argv[3])["m"], "unlock");
    }

    /// The git scripts run under the local `sh`, with shims standing in for
    /// the tools a Linux host has (`id`, `setfacl`, `getfacl`) so the
    /// gateway-account ACL step of D-G1 is exercised on any unix host.
    #[cfg(unix)]
    mod git_scripts {
        use super::*;
        use std::os::unix::fs::PermissionsExt;
        use std::path::PathBuf;
        use std::process::Command;

        struct Out {
            code: i32,
            stdout: String,
            stderr: String,
        }

        fn run(script: &str, args: &[&str], path: &str) -> Out {
            let out = Command::new("/bin/sh")
                .arg("-c")
                .arg(script)
                .arg("ign-git")
                .args(args)
                .env("PATH", path)
                .output()
                .expect("sh");
            Out {
                code: out.status.code().expect("exit code"),
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            }
        }

        fn system_path() -> String {
            std::env::var("PATH").unwrap_or_default()
        }

        fn shim(dir: &Path, name: &str, body: &str) {
            let path = dir.join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("shim");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }

        fn my_uid() -> String {
            let out = Command::new("id").arg("-u").output().expect("id");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }

        /// A data dir with a project holding a small `.git`, a save of it,
        /// and `.git` removed (the state after an import).
        struct Rig {
            root: tempfile::TempDir,
        }

        impl Rig {
            fn new() -> Rig {
                let root = tempfile::tempdir().expect("root");
                let git = root.path().join("data/projects/CapApp/.git");
                std::fs::create_dir_all(git.join("objects/ab")).expect("dirs");
                std::fs::create_dir_all(git.join("refs/heads")).expect("dirs");
                std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").expect("head");
                std::fs::write(git.join("config"), "[core]\n").expect("config");
                std::fs::write(git.join("objects/ab/cdef"), "object bytes").expect("object");
                std::fs::write(
                    git.join("refs/heads/main"),
                    "0123456789012345678901234567890123456789\n",
                )
                .expect("ref");
                std::fs::create_dir_all(root.path().join("home")).expect("home");
                Rig { root }
            }

            fn data(&self) -> String {
                self.root.path().join("data").display().to_string()
            }

            fn home(&self) -> String {
                self.root.path().join("home").display().to_string()
            }

            fn git(&self) -> PathBuf {
                self.root.path().join("data/projects/CapApp/.git")
            }

            fn save(&self) -> Out {
                run(
                    GIT_SAVE_SH,
                    &[
                        &self.data(),
                        "projects/CapApp/.git",
                        &self.home(),
                        "ign-deploy/gw/projects/CapApp/git-save/r1",
                    ],
                    &system_path(),
                )
            }

            fn restore(&self, path: &str) -> Out {
                run(
                    GIT_RESTORE_SH,
                    &[
                        &self.data(),
                        "projects/CapApp",
                        &self.home(),
                        "ign-deploy/gw/projects/CapApp/git-save/r1",
                        "r1",
                    ],
                    path,
                )
            }

            fn import_deletes_git(&self) {
                std::fs::remove_dir_all(self.git()).expect("rm .git");
            }
        }

        #[test]
        fn restore_without_a_foreign_gateway_account_makes_no_acl_call() {
            let rig = Rig::new();
            assert_eq!(rig.save().code, 0);
            rig.import_deletes_git();
            let bin = tempfile::tempdir().expect("bin");
            let calls = bin.path().join("calls");
            // The project directory belongs to this account, which is the
            // account that runs the script: no ACL step.
            shim(
                bin.path(),
                "setfacl",
                &format!("echo \"$*\" >> {}", calls.display()),
            );
            let path = format!("{}:{}", bin.path().display(), system_path());
            let out = rig.restore(&path);
            assert_eq!(out.code, 0, "{}", out.stderr);
            assert!(out.stdout.contains("IGN-RESULT:"), "{}", out.stdout);
            assert!(rig.git().join("config").is_file());
            assert!(
                !calls.exists(),
                "no setfacl call for the deploy account itself"
            );
        }

        #[test]
        fn restore_gives_a_non_root_gateway_account_an_explicit_acl() {
            if my_uid() == "0" {
                println!("running as root; the gateway-account branch cannot be set up");
                return;
            }
            let rig = Rig::new();
            assert_eq!(rig.save().code, 0);
            rig.import_deletes_git();
            let bin = tempfile::tempdir().expect("bin");
            let calls = bin.path().join("calls");
            let uid = my_uid();
            // The script runs as "uid 424242"; the project directory belongs
            // to the real account, which therefore plays the gateway account.
            shim(bin.path(), "id", "echo 424242");
            shim(
                bin.path(),
                "setfacl",
                &format!("echo \"$*\" >> {}", calls.display()),
            );
            let read_calls = bin.path().join("getfacl-calls");
            shim(
                bin.path(),
                "getfacl",
                &format!(
                    "echo \"$*\" >> {}; echo 'user::rw-'; echo 'user:{uid}:rw-'; echo 'mask::rw-'",
                    read_calls.display()
                ),
            );
            let path = format!("{}:{}", bin.path().display(), system_path());
            let out = rig.restore(&path);
            assert_eq!(out.code, 0, "{}", out.stderr);
            assert!(rig.git().join("config").is_file());
            // getfacl prints a named account by name unless asked for numbers,
            // and the entry is looked up by uid.
            let read = std::fs::read_to_string(&read_calls).expect("getfacl calls");
            assert!(read.starts_with("-n -p -- "), "{read}");
            let log = std::fs::read_to_string(&calls).expect("setfacl calls");
            let lines: Vec<&str> = log.lines().collect();
            assert!(
                lines
                    .iter()
                    .any(|l| l.starts_with(&format!("-R -P -m u:{uid}:rwX "))),
                "the recursive access entry: {lines:?}"
            );
            assert!(
                lines
                    .iter()
                    .any(|l| l.starts_with(&format!("-d -m u:{uid}:rwX "))),
                "the default entry on directories: {lines:?}"
            );
            // the ACL is applied to the copy before it is put in place
            assert!(
                lines.iter().all(|l| l.contains(".ign-restore-r1")),
                "{lines:?}"
            );
        }

        #[test]
        fn restore_refuses_when_the_gateway_account_would_only_read() {
            if my_uid() == "0" {
                return;
            }
            let rig = Rig::new();
            assert_eq!(rig.save().code, 0);
            rig.import_deletes_git();
            let bin = tempfile::tempdir().expect("bin");
            let uid = my_uid();
            shim(bin.path(), "id", "echo 424242");
            shim(bin.path(), "setfacl", "exit 0");
            shim(
                bin.path(),
                "getfacl",
                &format!("echo 'user:{uid}:rw- #effective:r--'"),
            );
            let path = format!("{}:{}", bin.path().display(), system_path());
            let out = rig.restore(&path);
            assert_eq!(out.code, 6, "{}", out.stderr);
            assert!(out.stderr.contains("effective"), "{}", out.stderr);
            assert!(!rig.git().exists(), "nothing is put in place");
            assert!(
                !rig.root
                    .path()
                    .join("data/projects/CapApp/.ign-restore-r1")
                    .exists(),
                "no staging directory is left"
            );
        }

        #[test]
        fn restore_refuses_without_setfacl_for_a_foreign_gateway_account() {
            if my_uid() == "0" {
                return;
            }
            let rig = Rig::new();
            assert_eq!(rig.save().code, 0);
            rig.import_deletes_git();
            // A tools directory of symlinks with everything the script needs
            // except `setfacl`, and an `id` that is not the project owner.
            let bin = tempfile::tempdir().expect("bin");
            for tool in [
                "find",
                "wc",
                "awk",
                "sort",
                "tr",
                "ls",
                "cp",
                "mv",
                "rm",
                "grep",
                "sha256sum",
                "shasum",
                "mkdir",
                "cat",
                "sed",
                "head",
                "tail",
                "base64",
                "dirname",
                "env",
            ] {
                let found = Command::new("sh")
                    .args(["-c", &format!("command -v {tool}")])
                    .output()
                    .expect("sh");
                let target = String::from_utf8_lossy(&found.stdout).trim().to_string();
                if found.status.success() && target.starts_with('/') {
                    std::os::unix::fs::symlink(&target, bin.path().join(tool)).expect("link");
                }
            }
            shim(bin.path(), "id", "echo 424242");
            let out = rig.restore(&bin.path().display().to_string());
            assert_eq!(out.code, 6, "{}", out.stderr);
            assert!(out.stderr.contains("setfacl"), "{}", out.stderr);
            assert!(!rig.git().exists());
        }

        fn probe(rig: &Rig, path: &str) -> Out {
            run(
                GIT_PROBE_W_SH,
                &[&rig.data(), "projects/CapApp", ".ign-probe-r1"],
                path,
            )
        }

        fn no_probe_left(rig: &Rig) {
            for dir in ["data/projects/CapApp", "data/projects/CapApp/.git"] {
                assert!(
                    !rig.root.path().join(dir).join(".ign-probe-r1").exists(),
                    "a probe file is left in {dir}"
                );
            }
        }

        /// A directory of symlinks to the tools a probe needs, minus the
        /// ones named in `without`.
        fn tool_dir(without: &[&str]) -> tempfile::TempDir {
            let bin = tempfile::tempdir().expect("bin");
            for tool in ["ls", "rm", "awk", "grep"] {
                if without.contains(&tool) {
                    continue;
                }
                let found = Command::new("sh")
                    .args(["-c", &format!("command -v {tool}")])
                    .output()
                    .expect("sh");
                let target = String::from_utf8_lossy(&found.stdout).trim().to_string();
                if found.status.success() && target.starts_with('/') {
                    std::os::unix::fs::symlink(&target, bin.path().join(tool)).expect("link");
                }
            }
            bin
        }

        #[test]
        fn probe_without_a_foreign_gateway_account_makes_no_acl_call() {
            let rig = Rig::new();
            let bin = tempfile::tempdir().expect("bin");
            let calls = bin.path().join("calls");
            // The project directory belongs to the account that runs the
            // probe: no ACL step.
            for tool in ["setfacl", "getfacl"] {
                shim(
                    bin.path(),
                    tool,
                    &format!("echo \"{tool} $*\" >> {}", calls.display()),
                );
            }
            let path = format!("{}:{}", bin.path().display(), system_path());
            let out = probe(&rig, &path);
            assert_eq!(out.code, 0, "{}", out.stderr);
            assert!(
                out.stdout.contains("IGN-RESULT:{\"ok\":1}"),
                "{}",
                out.stdout
            );
            assert!(!calls.exists(), "no ACL call for the deploy account itself");
            no_probe_left(&rig);
        }

        #[test]
        fn probe_proves_the_acl_step_for_a_non_root_gateway_account() {
            if my_uid() == "0" {
                println!("running as root; the gateway-account branch cannot be set up");
                return;
            }
            let rig = Rig::new();
            let bin = tempfile::tempdir().expect("bin");
            let calls = bin.path().join("calls");
            let uid = my_uid();
            // The probe runs as "uid 424242"; the project directory belongs
            // to the real account, which therefore plays the gateway account.
            shim(bin.path(), "id", "echo 424242");
            shim(
                bin.path(),
                "setfacl",
                &format!("echo \"setfacl $*\" >> {}", calls.display()),
            );
            shim(
                bin.path(),
                "getfacl",
                &format!(
                    "echo \"getfacl $*\" >> {}; echo 'user::rw-'; echo 'user:{uid}:rw-'; echo 'mask::rw-'",
                    calls.display()
                ),
            );
            let path = format!("{}:{}", bin.path().display(), system_path());
            let out = probe(&rig, &path);
            assert_eq!(out.code, 0, "{}", out.stderr);
            no_probe_left(&rig);
            let log = std::fs::read_to_string(&calls).expect("ACL calls");
            let probe_in_git = rig.git().join(".ign-probe-r1").display().to_string();
            assert!(
                log.lines()
                    .any(|l| l == format!("setfacl -m u:{uid}:rw {probe_in_git}")),
                "the named entry on the probe file in .git: {log}"
            );
            // numeric ids, so a named account is matched by its uid
            assert!(
                log.lines()
                    .any(|l| l == format!("getfacl -n -p {probe_in_git}")),
                "the entry is read back numerically: {log}"
            );
            assert_eq!(
                log.lines().filter(|l| l.starts_with("setfacl")).count(),
                1,
                "only the `.git` probe is checked: {log}"
            );
        }

        #[test]
        fn probe_refuses_when_the_gateway_account_would_only_read() {
            if my_uid() == "0" {
                return;
            }
            let rig = Rig::new();
            let bin = tempfile::tempdir().expect("bin");
            let uid = my_uid();
            shim(bin.path(), "id", "echo 424242");
            shim(bin.path(), "setfacl", "exit 0");
            shim(
                bin.path(),
                "getfacl",
                &format!("echo 'user:{uid}:rw- #effective:r--'"),
            );
            let path = format!("{}:{}", bin.path().display(), system_path());
            let out = probe(&rig, &path);
            assert_eq!(out.code, 6, "{}", out.stderr);
            assert!(out.stderr.contains("effective"), "{}", out.stderr);
            assert!(
                out.stderr.contains("projects/CapApp/.git"),
                "{}",
                out.stderr
            );
            no_probe_left(&rig);
        }

        #[test]
        fn probe_refuses_when_getfacl_shows_no_entry() {
            if my_uid() == "0" {
                return;
            }
            let rig = Rig::new();
            let bin = tempfile::tempdir().expect("bin");
            shim(bin.path(), "id", "echo 424242");
            shim(bin.path(), "setfacl", "exit 0");
            shim(
                bin.path(),
                "getfacl",
                "echo 'user::rw-'; echo 'group::r--'; echo 'mask::r--'",
            );
            let path = format!("{}:{}", bin.path().display(), system_path());
            let out = probe(&rig, &path);
            assert_eq!(out.code, 6, "{}", out.stderr);
            assert!(out.stderr.contains("no ACL entry"), "{}", out.stderr);
            no_probe_left(&rig);
        }

        #[test]
        fn probe_refuses_when_setfacl_fails() {
            if my_uid() == "0" {
                return;
            }
            let rig = Rig::new();
            let bin = tempfile::tempdir().expect("bin");
            shim(bin.path(), "id", "echo 424242");
            shim(bin.path(), "setfacl", "exit 1");
            shim(bin.path(), "getfacl", "exit 0");
            let path = format!("{}:{}", bin.path().display(), system_path());
            let out = probe(&rig, &path);
            assert_eq!(out.code, 6, "{}", out.stderr);
            assert!(out.stderr.contains("setfacl failed"), "{}", out.stderr);
            no_probe_left(&rig);
        }

        #[test]
        fn probe_refuses_without_setfacl_or_getfacl_for_a_foreign_gateway_account() {
            if my_uid() == "0" {
                return;
            }
            for missing in ["setfacl", "getfacl"] {
                let rig = Rig::new();
                let bin = tool_dir(&[]);
                shim(bin.path(), "id", "echo 424242");
                for tool in ["setfacl", "getfacl"] {
                    if tool != missing {
                        shim(bin.path(), tool, "exit 0");
                    }
                }
                let out = probe(&rig, &bin.path().display().to_string());
                assert_eq!(out.code, 6, "{missing}: {}", out.stderr);
                assert!(
                    out.stderr.contains("setfacl or getfacl"),
                    "{missing}: {}",
                    out.stderr
                );
                no_probe_left(&rig);
            }
        }

        #[test]
        fn restore_refuses_a_symlink_planted_at_the_stage_path() {
            let rig = Rig::new();
            assert_eq!(rig.save().code, 0);
            rig.import_deletes_git();
            // Someone with write access to the project directory plants the
            // stage name as a link to a directory elsewhere.
            let outside = rig.root.path().join("outside");
            std::fs::create_dir_all(&outside).expect("outside");
            let stage = rig.root.path().join("data/projects/CapApp/.ign-restore-r1");
            std::os::unix::fs::symlink(&outside, &stage).expect("plant");
            let out = rig.restore(&system_path());
            assert_eq!(out.code, 4, "{}", out.stderr);
            assert_eq!(
                std::fs::read_dir(&outside).expect("outside").count(),
                0,
                "nothing was copied through the link"
            );
            assert!(!rig.git().exists(), "no .git was put in place");
            assert!(
                std::fs::symlink_metadata(&stage)
                    .expect("link")
                    .file_type()
                    .is_symlink(),
                "the planted entry is left alone"
            );
        }

        #[test]
        fn save_refuses_a_symlink_planted_at_the_repo_path() {
            let rig = Rig::new();
            let outside = rig.root.path().join("outside");
            std::fs::create_dir_all(&outside).expect("outside");
            let run = rig
                .root
                .path()
                .join("home/ign-deploy/gw/projects/CapApp/git-save/r1");
            std::fs::create_dir_all(&run).expect("run dir");
            std::os::unix::fs::symlink(&outside, run.join("repo")).expect("plant");
            let out = rig.save();
            assert_eq!(out.code, 4, "{}", out.stderr);
            assert_eq!(std::fs::read_dir(&outside).expect("outside").count(), 0);
        }

        #[test]
        fn save_rm_script_refuses_foreign_paths() {
            let rig = Rig::new();
            assert_eq!(rig.save().code, 0);
            let home = rig.home();
            let repo = "ign-deploy/gw/projects/CapApp/git-save/r1/repo";
            for foreign in [
                "ign-deploy/gw/projects/CapApp/git-save/r1",
                "projects/CapApp/.git",
                "ign-deploy/gw/projects/CapApp/git-save/r2/repo",
                "../x/git-save/r1/repo",
                "ign-deploy/gw/projects/CapApp/git-save/r1/repo/objects",
            ] {
                let out = run(GIT_SAVE_RM_SH, &[&home, foreign, "r1"], &system_path());
                assert_eq!(out.code, 4, "{foreign}: {}", out.stderr);
            }
            assert!(rig.root.path().join("home").join(repo).is_dir());
            // a symlink on the way is refused and nothing is removed
            let real = rig.root.path().join("home/ign-deploy/gw/projects/CapApp");
            std::fs::rename(real.join("git-save"), real.join("kept")).expect("rename");
            std::os::unix::fs::symlink("kept", real.join("git-save")).expect("link");
            let out = run(GIT_SAVE_RM_SH, &[&home, repo, "r1"], &system_path());
            assert_eq!(out.code, 4, "{}", out.stderr);
            assert!(real.join("kept/r1/repo").is_dir());
        }

        #[test]
        fn replace_script_refuses_a_lock_of_the_wrong_size() {
            let rig = Rig::new();
            std::fs::write(rig.git().join("config.lock"), "12345").expect("lock");
            let out = run(
                GIT_REPLACE_SH,
                &[
                    &rig.data(),
                    "projects/CapApp/.git",
                    "config",
                    "config.lock",
                    "99",
                ],
                &system_path(),
            );
            assert_eq!(out.code, 5, "{}", out.stderr);
            assert_eq!(
                std::fs::read_to_string(rig.git().join("config")).expect("config"),
                "[core]\n",
                "the file is untouched"
            );
        }

        #[test]
        fn manifest_script_names_what_it_refuses() {
            let rig = Rig::new();
            let args = [rig.data(), "projects/CapApp/.git".to_string()];
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let ok = run(GIT_MANIFEST_SH, &args, &system_path());
            assert_eq!(ok.code, 0, "{}", ok.stderr);
            // a symlink, then a name with a control character, then a missing dir
            std::os::unix::fs::symlink("HEAD", rig.git().join("link")).expect("link");
            let out = run(GIT_MANIFEST_SH, &args, &system_path());
            assert_eq!(out.code, 4, "{}", out.stderr);
            assert!(out.stderr.contains("symlink"), "{}", out.stderr);
            std::fs::remove_file(rig.git().join("link")).expect("rm");
            std::fs::write(rig.git().join("bad\u{1}name"), "x").expect("control name");
            let out = run(GIT_MANIFEST_SH, &args, &system_path());
            assert_eq!(out.code, 4, "{}", out.stderr);
            std::fs::remove_file(rig.git().join("bad\u{1}name")).expect("rm");
            let out = run(
                GIT_MANIFEST_SH,
                &[&rig.data(), "projects/Missing/.git"],
                &system_path(),
            );
            assert_eq!(out.code, 4, "{}", out.stderr);
        }

        /// The listing script reports only the fixed shapes, with their sizes,
        /// as one JSON line, and refuses what a read must not trust.
        #[test]
        fn objects_script_lists_the_fixed_shapes_and_names_what_it_refuses() {
            let rig = Rig::new();
            let objects = rig.git().join("objects");
            let loose = format!("{}/{}", "0a", "b".repeat(38));
            let pack = |ext: &str| format!("pack/pack-{}.{ext}", "c".repeat(40));
            std::fs::create_dir_all(objects.join("0a")).expect("fan");
            std::fs::create_dir_all(objects.join("pack")).expect("pack dir");
            std::fs::create_dir_all(objects.join("info")).expect("info");
            std::fs::write(objects.join(&loose), vec![b'x'; 17]).expect("loose");
            std::fs::write(objects.join(pack("pack")), vec![b'p'; 300]).expect("pack");
            std::fs::write(objects.join(pack("idx")), vec![b'i'; 40]).expect("idx");
            // not of the fixed shapes: never reported
            std::fs::write(objects.join(pack("keep")), "").expect("keep");
            std::fs::write(objects.join("info/packs"), "P x\n").expect("info");
            std::fs::write(objects.join("ab/cdef"), "short name").expect("short");
            std::fs::write(objects.join("0a/B"), "upper").expect("upper");
            std::fs::write(objects.join("0a/with space"), "space").expect("space");
            let args = [
                rig.data(),
                "projects/CapApp/.git/objects".to_string(),
                "200000".to_string(),
                "51025".to_string(),
            ];
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let out = run(GIT_OBJECTS_SH, &args, &system_path());
            assert_eq!(out.code, 0, "{}", out.stderr);
            let line = out
                .stdout
                .trim_end()
                .strip_prefix("IGN-RESULT:")
                .expect("sentinel");
            let json: serde_json::Value = serde_json::from_str(line).expect("json");
            assert_eq!(json["more"], 0);
            assert_eq!(json["n"], 3);
            let mut lines: Vec<&str> = json["t"].as_str().expect("t").split('\n').collect();
            lines.retain(|l| !l.is_empty());
            lines.sort_unstable();
            let mut want = vec![
                format!("17 objects/{loose}"),
                format!("300 objects/{}", pack("pack")),
                format!("40 objects/{}", pack("idx")),
            ];
            want.sort_unstable();
            assert_eq!(lines, want);

            // the report cap says `more` and the walk cap refuses
            let capped = run(
                GIT_OBJECTS_SH,
                &[args[0], args[1], "200000", "2"],
                &system_path(),
            );
            assert_eq!(capped.code, 0, "{}", capped.stderr);
            let line = capped
                .stdout
                .trim_end()
                .strip_prefix("IGN-RESULT:")
                .expect("s");
            let json: serde_json::Value = serde_json::from_str(line).expect("json");
            assert_eq!(
                (json["more"].clone(), json["n"].clone()),
                (1.into(), 2.into())
            );
            let walked = run(
                GIT_OBJECTS_SH,
                &[args[0], args[1], "3", "51025"],
                &system_path(),
            );
            assert_eq!(walked.code, 4, "{}", walked.stderr);
            assert!(walked.stderr.contains("more entries"), "{}", walked.stderr);

            // a symlink, a name with a control character, a name with a colon
            // or a backslash, and a missing or linked objects directory refuse
            std::os::unix::fs::symlink("/", objects.join("0a/link")).expect("link");
            let out = run(GIT_OBJECTS_SH, &args, &system_path());
            assert_eq!(out.code, 4, "{}", out.stderr);
            assert!(out.stderr.contains("symlink"), "{}", out.stderr);
            std::fs::remove_file(objects.join("0a/link")).expect("rm");
            std::fs::write(objects.join("0a/bad\u{7}"), "x").expect("bell");
            assert_eq!(run(GIT_OBJECTS_SH, &args, &system_path()).code, 4);
            std::fs::remove_file(objects.join("0a/bad\u{7}")).expect("rm");
            for name in ["0a/a:b", "0a/a\\b"] {
                std::fs::write(objects.join(name), "x").expect("name");
                let out = run(GIT_OBJECTS_SH, &args, &system_path());
                assert_eq!(out.code, 4, "{name}: {}", out.stderr);
                std::fs::remove_file(objects.join(name)).expect("rm");
            }
            assert_eq!(run(GIT_OBJECTS_SH, &args, &system_path()).code, 0);
            let out = run(
                GIT_OBJECTS_SH,
                &[args[0], "projects/Missing/.git/objects", "9", "9"],
                &system_path(),
            );
            assert_eq!(out.code, 4, "{}", out.stderr);
            let moved = rig.git().join("objects-real");
            std::fs::rename(&objects, &moved).expect("move");
            std::os::unix::fs::symlink(&moved, &objects).expect("link objects");
            let out = run(GIT_OBJECTS_SH, &args, &system_path());
            assert_eq!(out.code, 4, "{}", out.stderr);
            assert!(
                out.stderr.contains("not a real directory"),
                "{}",
                out.stderr
            );
        }

        /// A hostile value is data: a data directory or a path with quote,
        /// `$`, backtick and space characters lists as it would otherwise.
        #[test]
        fn objects_script_keeps_hostile_path_text_inert() {
            let root = tempfile::tempdir().expect("root");
            let odd = "it's $(touch pwned) `x` \"q\"";
            let data = root.path().join(odd);
            std::fs::create_dir_all(data.join("projects/P/.git/objects/0a")).expect("dirs");
            let name = "b".repeat(38);
            std::fs::write(
                data.join(format!("projects/P/.git/objects/0a/{name}")),
                "xyz",
            )
            .expect("loose");
            let data_s = data.display().to_string();
            let out = run(
                GIT_OBJECTS_SH,
                &[&data_s, "projects/P/.git/objects", "99", "99"],
                &system_path(),
            );
            assert_eq!(out.code, 0, "{}", out.stderr);
            assert!(
                out.stdout.contains(&format!("3 objects/0a/{name}")),
                "{}",
                out.stdout
            );
            assert!(!root.path().join("pwned").exists());
            assert!(!std::path::Path::new("pwned").exists());
        }
    }
}
