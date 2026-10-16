#!/bin/sh
# ign deploy: the on-box batch for a POSIX host (Phase 23, plan 23-06, D-01).
#
# Run by a fixed stub that has already compared the SHA-256 of this script and
# of work.tsv with the values the runner computed. Arguments:
#
#   commit.sh OP STAGING COLLECTION DEPLOY
#
# OP is one of commit, verify, unstage, restore, fingerprint, finalize,
# discard, probe and mirror. STAGING holds work.tsv and the files the work list
# names, COLLECTION is <data dir>/config/resources/<mode> and DEPLOY is
# <home>/ign-deploy/<gateway>/<mode>, all absolute.
#
# The script reads work.tsv and the pre-rendered files beside it and nothing
# else. It never prints a path or a file's content: its one IGN-RESULT: line is
# ASCII JSON whose per-entry results are indices into the work list (a removed
# directory is [delete index, levels]). POSIX sh only.
#
# Operation order and rules are the local host's (deploy/apply/host.rs).
# Residual, recorded in the plan as FA-23-05: a shell cannot open a path
# relative to a descriptor, so each check below walks the path with `[ -L ]`
# and acts on it a moment later; a writer with access to the collection can
# race that gap. File modes (0644, directories 0755) and `mv` across file
# systems are proven only against a local Linux-style shell.

# shellcheck disable=SC2015
set -eu
umask 022
LC_ALL=C
export LC_ALL

OP=${1:-}
ST=${2:-}
CO=${3:-}
DE=${4:-}
TAB=$(printf '\t')
NL='
'
WORK=$ST/work.tsv

# A control directory (.ign-ctl-*, made by the runner for an operation that has
# no staging directory of its own) holds this script and its work list; it is
# removed when the script exits, whatever the outcome, so a staged copy the
# script moved away never leaves the runner a file it cannot delete one by one.
cleanup_ctl() {
  case ${ST##*/} in
    .ign-ctl-*)
      if [ -d "$ST" ] && [ ! -L "$ST" ]; then
        rm -rf -- "$ST"
      fi
      ;;
  esac
}
trap cleanup_ctl EXIT

# ---- output ------------------------------------------------------------------

emit() {
  printf 'IGN-RESULT:%s\n' "$1"
}

# refuse CODE TEXT [EXTRA]: nothing was written; one result line, exit 0.
refuse() {
  emit "{\"ok\":0,\"err\":\"$1\",\"detail\":\"$2\"${3:+,$3}}"
  exit 0
}

# ---- primitives --------------------------------------------------------------

is_hex64() {
  case $1 in
    *[!0-9a-f]*) return 1 ;;
  esac
  [ "${#1}" -eq 64 ]
}

# A hash or "-".
is_hex_or_dash() {
  [ "$1" = "-" ] || is_hex64 "$1"
}

if command -v sha256sum >/dev/null 2>&1; then
  SHA=sha256sum
elif command -v shasum >/dev/null 2>&1; then
  SHA='shasum -a 256'
else
  refuse no_sha256 "the host has neither sha256sum nor shasum"
fi

# hash_file PATH: H becomes the file's SHA-256; fails when it cannot be read.
hash_file() {
  H=$($SHA 2>/dev/null <"$1") || return 1
  H=${H%% *}
  is_hex64 "$H"
}

real_dir() {
  [ -d "$1" ] && [ ! -L "$1" ]
}

# classify BASE REL: what BASE/REL is, without following a symlink. KIND
# becomes absent, file, dir, symlink or unsafe. A symlink on the way is
# symlink; a special file, a file where a directory is needed and a directory
# that cannot be searched are unsafe.
classify() {
  KIND=dir
  if [ ! -e "$1" ] && [ ! -L "$1" ]; then
    KIND=absent
    return 0
  fi
  _cur=$1
  _oldifs=$IFS
  IFS=/
  set -f
  # shellcheck disable=SC2086
  set -- $2
  set +f
  IFS=$_oldifs
  while [ $# -gt 0 ]; do
    _parent=$_cur
    _cur=$_cur/$1
    shift
    if [ -L "$_cur" ]; then
      KIND=symlink
      return 0
    fi
    if [ -d "$_cur" ]; then
      KIND=dir
    elif [ -f "$_cur" ]; then
      KIND='file'
      if [ $# -gt 0 ]; then
        KIND=unsafe
        return 0
      fi
    elif [ -e "$_cur" ]; then
      KIND=unsafe
      return 0
    else
      if [ -x "$_parent" ]; then
        KIND=absent
      else
        KIND=unsafe
      fi
      return 0
    fi
  done
  return 0
}

# live BASE REL: LIVE becomes the SHA-256 of BASE/REL, "-" when absent and
# "!" when it is not a plain regular file reached without a symlink.
live() {
  classify "$1" "$2"
  case $KIND in
    absent) LIVE=- ;;
    file)
      if hash_file "$1/$2"; then
        LIVE=$H
      else
        LIVE='!'
      fi
      ;;
    *) LIVE='!' ;;
  esac
}

# plain_abs PATH: absolute, no empty, . or .. component, no control character.
plain_abs() {
  case $1 in
    /*) ;;
    *) return 1 ;;
  esac
  case $1 in
    *[[:cntrl:]]*) return 1 ;;
  esac
  case "$1/" in
    */./* | */../* | *//*) return 1 ;;
  esac
  return 0
}

# plain_name NAME: one path component that may name a gateway, a mode or a run.
plain_name() {
  case $1 in
    '' | . | .. | .* | */* | *\\* | *[[:cntrl:]]*) return 1 ;;
  esac
  return 0
}

# collection_path REL: <module>/<type>/<name>..., no dot-named or gateway-owned
# component, no control character.
collection_path() {
  case $1 in
    '' | /* | */ | *//* | *[[:cntrl:]]*) return 1 ;;
  esac
  _oldifs=$IFS
  IFS=/
  set -f
  # shellcheck disable=SC2086
  set -- $1
  set +f
  IFS=$_oldifs
  [ $# -ge 3 ] || return 1
  for _c in "$@"; do
    case $_c in
      .* | config-mode.json) return 1 ;;
    esac
  done
  return 0
}

# depth_of REL: DEPTH becomes the number of components of REL.
depth_of() {
  DEPTH=1
  _t=$1
  while :; do
    case $_t in
      */*)
        _t=${_t#*/}
        DEPTH=$((DEPTH + 1))
        ;;
      *) break ;;
    esac
  done
}

# has_entries DIR: success when DIR holds anything.
has_entries() {
  for _e in "$1"/* "$1"/.[!.]* "$1"/..?*; do
    if [ -e "$_e" ] || [ -L "$_e" ]; then
      return 0
    fi
  done
  return 1
}

# ---- the work list --------------------------------------------------------------

# check_work: read work.tsv once, refuse anything that is not exactly what the
# runner writes, and set RUN GW MD WOP RETRIES SRC JSEEN LNEW HAVE_L and
# the counts NW ND NX NP NF, HAVE_J and RECDOC (the staged record's name).
check_work() {
  if [ ! -f "$WORK" ] || [ -L "$WORK" ]; then
    refuse bad_work "there is no work list"
  fi
  RUN='' GW='' MD='' WOP='' RETRIES=3 SRC='' JSEEN='-' LNEW=''
  HAVE_L=0 NW=0 ND=0 NX=0 NP=0 NF=0
  _have_t=0 _have_s=0 HAVE_J=0 RECDOC=''
  _first=1
  while IFS=$TAB read -r k a b c d; do
    if [ "$_first" -eq 1 ]; then
      _first=0
      [ "$k" = 'ign-work 1' ] || refuse bad_work "the work list header is wrong"
      continue
    fi
    case $k in
      R)
        [ -z "$RUN" ] && [ -n "$a" ] || refuse bad_work "a run record is repeated"
        RUN=$a
        ;;
      G)
        [ -z "$GW" ] && [ -n "$a" ] || refuse bad_work "a gateway record is repeated"
        GW=$a
        ;;
      M)
        [ -z "$MD" ] && [ -n "$a" ] || refuse bad_work "a mode record is repeated"
        MD=$a
        ;;
      O)
        [ -z "$WOP" ] && [ -n "$a" ] || refuse bad_work "an operation record is repeated"
        WOP=$a
        ;;
      T)
        [ "$_have_t" -eq 0 ] || refuse bad_work "a retry record is repeated"
        _have_t=1
        case $a in
          '' | *[!0-9]*) refuse bad_work "the retry count is not a number" ;;
        esac
        [ "${#a}" -le 2 ] && [ "$a" -le 10 ] || refuse bad_work "the retry count is over the limit"
        RETRIES=$a
        ;;
      J)
        [ "$HAVE_J" -eq 0 ] || refuse bad_work "a journal record is repeated"
        HAVE_J=1
        is_hex_or_dash "$a" || refuse bad_work "the journal hash is wrong"
        JSEEN=$a
        ;;
      S)
        [ "$_have_s" -eq 0 ] || refuse bad_work "a source record is repeated"
        _have_s=1
        plain_abs "$a" || refuse bad_work "the staged files directory is wrong"
        SRC=$a
        ;;
      F)
        case $a in
          journal.snapshotting.json | journal.ready.json | journal.writing.json | journal.written.json | journal.restoring.json | record.applying.json | record.applied.json | record.rolled_back.json) ;;
          *) refuse bad_work "a staged file name is wrong" ;;
        esac
        is_hex64 "$b" || refuse bad_work "a staged file hash is wrong"
        case $a in
          record.*) RECDOC=$a ;;
        esac
        NF=$((NF + 1))
        ;;
      W)
        [ "$a" = "$NW" ] || refuse bad_work "a write index is wrong"
        is_hex64 "$b" || refuse bad_work "a write hash is wrong"
        is_hex_or_dash "$c" || refuse bad_work "a write pre-image hash is wrong"
        collection_path "$d" || refuse bad_work "a write path is wrong"
        NW=$((NW + 1))
        ;;
      D)
        [ "$a" = "$ND" ] || refuse bad_work "a delete index is wrong"
        is_hex64 "$b" || refuse bad_work "a delete pre-image hash is wrong"
        collection_path "$c" || refuse bad_work "a delete path is wrong"
        ND=$((ND + 1))
        ;;
      X)
        [ "$a" = "$NX" ] || refuse bad_work "a mirror delete index is wrong"
        collection_path "$b" || refuse bad_work "a mirror delete path is wrong"
        NX=$((NX + 1))
        ;;
      L)
        [ "$HAVE_L" -eq 0 ] || refuse bad_work "a ledger record is repeated"
        is_hex64 "$a" || refuse bad_work "the ledger hash is wrong"
        is_hex_or_dash "$b" || refuse bad_work "the prior ledger hash is wrong"
        HAVE_L=1
        LNEW=$a
        ;;
      P)
        [ "$a" = "$NP" ] || refuse bad_work "a restore index is wrong"
        is_hex_or_dash "$b" || refuse bad_work "a restore hash is wrong"
        if [ "$c" != 'ledger.json' ]; then
          collection_path "$c" || refuse bad_work "a restore path is wrong"
        fi
        NP=$((NP + 1))
        ;;
      *) refuse bad_work "the work list holds a record this script does not know" ;;
    esac
  done <"$WORK"
  [ -n "$WOP" ] && [ -n "$MD" ] && [ -n "$RUN" ] && [ -n "$GW" ] ||
    refuse bad_work "the work list is missing a record"
  # verify reads the commit list a commit is about to run
  case $OP:$WOP in
    verify:commit) ;;
    *) [ "$WOP" = "$OP" ] || refuse bad_work "the work list is for another operation" ;;
  esac
  plain_name "$MD" || refuse bad_work "the mode is wrong"
  case $OP in
    probe | fingerprint)
      [ "$RUN" = '-' ] || refuse bad_work "this operation belongs to no run"
      plain_name "$GW" || refuse bad_work "the gateway is wrong"
      ;;
    mirror)
      [ "$RUN" = '-' ] && [ "$GW" = '-' ] || refuse bad_work "this operation belongs to no run"
      ;;
    *)
      plain_name "$RUN" && plain_name "$GW" || refuse bad_work "the run or gateway is wrong"
      case $RUN in
        *[!A-Za-z0-9._-]*) refuse bad_work "the run id is wrong" ;;
      esac
      [ "${#RUN}" -le 128 ] || refuse bad_work "the run id is too long"
      ;;
  esac
  case $OP in
    restore | finalize | discard)
      [ "$HAVE_J" -eq 1 ] || refuse bad_work "the work list does not say which journal was seen"
      ;;
  esac
}

# check_roots: the three roots are plain absolute paths of the shape the
# runner builds for this work list, and none of the directories this script
# creates inside is a symlink.
check_roots() {
  plain_abs "$ST" && plain_abs "$CO" && plain_abs "$DE" || refuse bad_roots "a root is not a plain absolute path"
  case $CO in
    */config/resources/"$MD") ;;
    *) refuse bad_roots "the collection is not the mode collection" ;;
  esac
  if [ -L "$CO" ]; then
    refuse unsafe "the mode collection is a symlink"
  fi
  case $OP in
    commit | verify | restore | finalize | discard | unstage | probe | fingerprint)
      case $DE in
        */ign-deploy/"$GW"/"$MD") ;;
        *) refuse bad_roots "the deploy directory is not this gateway and mode" ;;
      esac
      _d=$DE
      for _n in 1 2 3; do
        if [ -L "$_d" ]; then
          refuse unsafe "the deploy directory is or is below a symlink"
        fi
        _d=${_d%/*}
      done
      ;;
  esac
  case $OP in
    commit | verify)
      [ "$ST" = "$DE/staging/$RUN" ] || refuse bad_roots "the staging directory is not this run's"
      ;;
  esac
}

# srcdir: where the staged files are.
srcdir() {
  SRCDIR=${SRC:-$ST/files}
}

# ---- staged files ------------------------------------------------------------------

# verify_staged: every pre-rendered file, staged copy and the staged ledger is
# a regular file with the hash the work list gives. On a mismatch the indices
# are in BAD and the status is 1.
verify_staged() {
  BAD=''
  srcdir
  while IFS=$TAB read -r k a b c d; do
    case $k in
      F)
        if [ -L "$ST/$a" ] || ! hash_file "$ST/$a" || [ "$H" != "$b" ]; then
          BAD=${BAD:+$BAD,}\"F$a\"
        fi
        ;;
      W)
        classify "$SRCDIR" "$d"
        if [ "$KIND" != file ] || ! hash_file "$SRCDIR/$d" || [ "$H" != "$b" ]; then
          BAD=${BAD:+$BAD,}\"W$a\"
        fi
        ;;
      L)
        if [ -L "$ST/ledger.json" ] || ! hash_file "$ST/ledger.json" || [ "$H" != "$a" ]; then
          BAD=${BAD:+$BAD,}\"L\"
        fi
        ;;
    esac
  done <"$WORK"
  [ -z "$BAD" ]
}

# ---- journal and record installs ----------------------------------------------------------

# replace_with SRC DEST: copy SRC beside DEST and rename it over DEST.
replace_with() {
  _tmp=$2.ign-tmp
  rm -f -- "$_tmp"
  cp -- "$1" "$_tmp" || return 1
  chmod 600 "$_tmp" || {
    rm -f -- "$_tmp"
    return 1
  }
  mv -f -- "$_tmp" "$2" || {
    rm -f -- "$_tmp"
    return 1
  }
}

# create_journal SRC: put SRC at DEPLOY/journal.json, whole or not at all.
# 0 created, 1 one is already there, 2 it could not be created.
create_journal() {
  _j=$DE/journal.json
  if [ -e "$_j" ] || [ -L "$_j" ]; then
    return 1
  fi
  _tmp=$DE/journal.json.ign-new
  rm -f -- "$_tmp"
  cp -- "$1" "$_tmp" || return 2
  chmod 600 "$_tmp" || {
    rm -f -- "$_tmp"
    return 2
  }
  if ln -- "$_tmp" "$_j" 2>/dev/null; then
    rm -f -- "$_tmp"
    return 0
  fi
  rm -f -- "$_tmp"
  if [ -e "$_j" ] || [ -L "$_j" ]; then
    return 1
  fi
  # No hard links on this file system: fall back to an exclusive create.
  if (
    set -C
    cat -- "$1" >"$_j"
  ) 2>/dev/null; then
    chmod 600 "$_j" || return 2
    return 0
  fi
  if [ -e "$_j" ] || [ -L "$_j" ]; then
    return 1
  fi
  return 2
}

# abort_before_move: undo a commit that has not moved a file.
abort_before_move() {
  if real_dir "$DE/snapshots"; then
    rm -rf -- "$DE/snapshots/$RUN"
  fi
  rm -f -- "$DE/journal.json"
}

# ---- commit ------------------------------------------------------------------------------

MOVED='' DELETED='' DIRS='' STALE='' FAIL='' REMOVED=$NL RMWHAT='' CREATED='' RESTORED=''
SNAPBAD=''
BAD=''
H=''
KIND=''
LIVE=''
DEPTH=1
SRCDIR=''


fail_json() {
  if [ -n "$FAIL" ]; then
    printf '"%s"' "$FAIL"
  else
    printf 'null'
  fi
}

report() {
  emit "{\"ok\":1,\"phase\":\"$1\",\"stale\":[$STALE],\"moved\":[$MOVED],\"deleted\":[$DELETED],\"dirs\":[$DIRS],\"fail\":$(fail_json)}"
}

# stale_check: set STALE to the entries whose live state is not the pre-image.
stale_check() {
  STALE=''
  while IFS=$TAB read -r k a b c d; do
    case $k in
      W)
        live "$CO" "$d"
        _pre=$c
        if [ "$LIVE" != "$_pre" ]; then
          STALE=${STALE:+$STALE,}\"W$a\"
        fi
        ;;
      D)
        live "$CO" "$c"
        if [ "$LIVE" != "$b" ]; then
          STALE=${STALE:+$STALE,}\"D$a\"
        fi
        ;;
      L)
        live "$DE" ledger.json
        if [ "$LIVE" != "$b" ]; then
          STALE=${STALE:+$STALE,}\"L\"
        fi
        ;;
    esac
  done <"$WORK"
  [ -z "$STALE" ]
}

# snapshot_one BASE REL PRE DEST: copy BASE/REL to DEST (private) and verify
# it. 1 when the live file is not the pre-image, 2 on an error. SNAPBAD names
# the entry for a stale report.
snapshot_one() {
  classify "$1" "$2"
  [ "$KIND" = file ] || return 1
  if [ -e "$4" ] || [ -L "$4" ]; then
    return 2
  fi
  mkdir -p -- "${4%/*}" || return 2
  cp -- "$1/$2" "$4" || return 1
  chmod 600 "$4" || return 2
  hash_file "$4" || return 2
  [ "$H" = "$3" ] || return 1
  return 0
}

# snapshot_all: copy each existing pre-image and the prior ledger. 0 done, 1
# stale, 2 error.
snapshot_all() {
  SNAPBAD=''
  if [ -e "$DE/snapshots" ] || [ -L "$DE/snapshots" ]; then
    real_dir "$DE/snapshots" || return 2
  fi
  mkdir -p -- "$DE/snapshots/$RUN" || return 2
  while IFS=$TAB read -r k a b c d; do
    case $k in
      W)
        if [ "$c" != '-' ]; then
          SNAPBAD=\"W$a\"
          snapshot_one "$CO" "$d" "$c" "$DE/snapshots/$RUN/files/$d" || return $?
        fi
        ;;
      D)
        SNAPBAD=\"D$a\"
        snapshot_one "$CO" "$c" "$b" "$DE/snapshots/$RUN/files/$c" || return $?
        ;;
      L)
        if [ "$b" != '-' ]; then
          SNAPBAD=\"L\"
          snapshot_one "$DE" ledger.json "$b" "$DE/snapshots/$RUN/ledger.json" || return $?
        fi
        ;;
    esac
  done <"$WORK"
  return 0
}

# make_parents REL: create the directories above CO/REL that are missing.
make_parents() {
  _dir=${1%/*}
  _cur=$CO
  _oldifs=$IFS
  IFS=/
  set -f
  # shellcheck disable=SC2086
  set -- $_dir
  set +f
  IFS=$_oldifs
  for _c in "$@"; do
    _cur=$_cur/$_c
    if [ -L "$_cur" ]; then
      return 1
    fi
    if [ -d "$_cur" ]; then
      continue
    fi
    if [ -e "$_cur" ]; then
      return 1
    fi
    mkdir -- "$_cur" || return 1
  done
  return 0
}

# ensure_collection: the mode collection exists (created when missing).
ensure_collection() {
  if [ -L "$CO" ]; then
    return 1
  fi
  if [ -d "$CO" ]; then
    return 0
  fi
  _res=${CO%/*}
  if [ ! -d "$_res" ] || [ -L "$_res" ]; then
    return 1
  fi
  mkdir -- "$CO"
}

# move_one INDEX REL SHA: move the staged copy over the target. Sets FAIL and
# fails when it cannot.
move_one() {
  _i=$1
  _rel=$2
  _sha=$3
  classify "$CO" "$_rel"
  case $KIND in
    absent | file) ;;
    *)
      FAIL="write $_i: the target is no longer a regular file"
      return 1
      ;;
  esac
  classify "$SRCDIR" "$_rel"
  if [ "$KIND" != file ] || ! hash_file "$SRCDIR/$_rel" || [ "$H" != "$_sha" ]; then
    FAIL="write $_i: the staged copy changed"
    return 1
  fi
  # Never move a file onto itself (C7a-4: that deletes it).
  if [ -e "$CO/$_rel" ] && [ "$SRCDIR/$_rel" -ef "$CO/$_rel" ]; then
    FAIL="write $_i: the staged copy is the target itself"
    return 1
  fi
  if ! make_parents "$_rel"; then
    FAIL="write $_i: cannot create the directories"
    return 1
  fi
  if ! chmod 644 "$SRCDIR/$_rel"; then
    FAIL="write $_i: cannot set the file mode"
    return 1
  fi
  _attempt=0
  while :; do
    if mv -f -- "$SRCDIR/$_rel" "$CO/$_rel" 2>/dev/null; then
      break
    fi
    _attempt=$((_attempt + 1))
    if [ "$_attempt" -gt "$RETRIES" ]; then
      FAIL="write $_i: cannot move the file into place"
      return 1
    fi
    sleep 1
  done
  if ! hash_file "$CO/$_rel" || [ "$H" != "$_sha" ]; then
    FAIL="write $_i: the file in place does not match its hash"
    return 1
  fi
  return 0
}

# moves: step 6. Stops at the first failure.
moves() {
  while IFS=$TAB read -r k a b c d; do
    if [ "$k" = W ]; then
      move_one "$a" "$d" "$b" || return 1
      MOVED=${MOVED:+$MOVED,}$a
    fi
  done <"$WORK"
  return 0
}

# rmdir_up INDEX REL: remove the directories the removal of REL emptied,
# deepest first, never <module>/<type> or above.
rmdir_up() {
  _i=$1
  _dir=${2%/*}
  depth_of "$_dir"
  while [ "$DEPTH" -ge 3 ]; do
    case $REMOVED in
      *"$NL$_dir$NL"*) ;;
      *)
        classify "$CO" "$_dir"
        case $KIND in
          dir)
            if rmdir -- "$CO/$_dir" 2>/dev/null; then
              REMOVED=$REMOVED$_dir$NL
              DIRS=${DIRS:+$DIRS,}[$_i,$DEPTH]
            elif has_entries "$CO/$_dir"; then
              return 0
            else
              FAIL="$RMWHAT: cannot remove an emptied directory"
              return 1
            fi
            ;;
          absent) ;;
          *)
            FAIL="$RMWHAT: an emptied directory is not a real directory"
            return 1
            ;;
        esac
        ;;
    esac
    _dir=${_dir%/*}
    DEPTH=$((DEPTH - 1))
  done
  return 0
}

# deletes: step 7. The delete set, then the directories it emptied.
deletes() {
  while IFS=$TAB read -r k a b c d; do
    if [ "$k" = D ]; then
      classify "$CO" "$c"
      if [ "$KIND" != file ]; then
        FAIL="delete $a: the target is no longer a regular file"
        return 1
      fi
      if ! rm -- "$CO/$c"; then
        FAIL="delete $a: cannot delete the file"
        return 1
      fi
      DELETED=${DELETED:+$DELETED,}$a
    fi
  done <"$WORK"
  while IFS=$TAB read -r k a b c d; do
    if [ "$k" = D ]; then
      RMWHAT="delete $a"
      rmdir_up "$a" "$c" || return 1
    fi
  done <"$WORK"
  return 0
}

# swap_ledger: step 8, last.
swap_ledger() {
  [ "$HAVE_L" -eq 1 ] || return 0
  classify "$DE" ledger.json
  case $KIND in
    absent | file) ;;
    *)
      FAIL="cannot swap the ledger"
      return 1
      ;;
  esac
  if ! hash_file "$ST/ledger.json" || [ "$H" != "$LNEW" ]; then
    FAIL="the staged ledger changed"
    return 1
  fi
  if ! replace_with "$ST/ledger.json" "$DE/ledger.json"; then
    FAIL="cannot swap the ledger"
    return 1
  fi
  return 0
}

# journal_unchanged: the live journal is what the runner saw when it asked for
# this operation (its hash, or absent), so a journal that appeared or changed
# since then is never overwritten or removed on stale information.
journal_unchanged() {
  _j=$DE/journal.json
  if [ -e "$_j" ] || [ -L "$_j" ]; then
    [ ! -L "$_j" ] && hash_file "$_j" && [ "$H" = "$JSEEN" ]
  else
    [ "$JSEEN" = '-' ]
  fi
}

# restore_one INDEX PATH PRE: put one path back. Sets FAIL (a value-free
# reason) and fails when it cannot. RESTORED becomes put or removed.
restore_one() {
  _i=$1
  _p=$2
  _pre=$3
  RESTORED=put
  if [ "$_p" = ledger.json ]; then
    _base=$DE
    _snap=$DE/snapshots/$RUN/ledger.json
    _is_ledger=1
  else
    _base=$CO
    _snap=$DE/snapshots/$RUN/files/$_p
    _is_ledger=0
  fi
  classify "$_base" "$_p"
  _live=$KIND
  if [ "$_pre" = '-' ]; then
    case $_live in
      absent) RESTORED=removed ;;
      file)
        rm -- "$_base/$_p" || {
          FAIL="restore $_i: cannot remove the file"
          return 1
        }
        RESTORED=removed
        ;;
      *)
        FAIL="restore $_i: the live path is not a regular file"
        return 1
        ;;
    esac
    return 0
  fi
  case $_live in
    absent | file) ;;
    *)
      FAIL="restore $_i: the live path is not a regular file"
      return 1
      ;;
  esac
  classify "$DE/snapshots/$RUN" "${_snap#"$DE/snapshots/$RUN/"}"
  if [ "$KIND" != file ] || ! hash_file "$_snap"; then
    FAIL="restore $_i: the snapshot has no copy of the file"
    return 1
  fi
  if [ "$H" != "$_pre" ]; then
    FAIL="restore $_i: the snapshot copy does not match its recorded hash"
    return 1
  fi
  # already back (a second restore): nothing to write
  if [ "$_live" = file ] && hash_file "$_base/$_p" && [ "$H" = "$_pre" ]; then
    return 0
  fi
  if [ "$_is_ledger" -eq 1 ]; then
    if ! replace_with "$_snap" "$DE/ledger.json"; then
      FAIL="restore $_i: cannot put the ledger back"
      return 1
    fi
  else
    if ! make_parents "$_p"; then
      FAIL="restore $_i: cannot create the directories"
      return 1
    fi
    _tmp=$ST/restore.ign-tmp
    rm -f -- "$_tmp"
    if ! cp -- "$_snap" "$_tmp" || ! chmod 644 "$_tmp"; then
      rm -f -- "$_tmp"
      FAIL="restore $_i: cannot stage the restore copy"
      return 1
    fi
    _attempt=0
    while :; do
      if mv -f -- "$_tmp" "$CO/$_p" 2>/dev/null; then
        break
      fi
      _attempt=$((_attempt + 1))
      if [ "$_attempt" -gt "$RETRIES" ]; then
        rm -f -- "$_tmp"
        FAIL="restore $_i: cannot move the file into place"
        return 1
      fi
      sleep 1
    done
  fi
  if ! hash_file "$_base/$_p" || [ "$H" != "$_pre" ]; then
    FAIL="restore $_i: the restored file does not match its recorded hash"
    return 1
  fi
  return 0
}

# restore_pass WANT_LEDGER: visit the P records that are (1) or are not (0)
# the ledger.
restore_pass() {
  while IFS=$TAB read -r k a b c d; do
    if [ "$k" = P ]; then
      if { [ "$c" = ledger.json ] && [ "$1" -eq 1 ]; } || { [ "$c" != ledger.json ] && [ "$1" -eq 0 ]; }; then
        restore_one "$a" "$c" "$b" || return 1
        if [ "$RESTORED" = put ]; then
          MOVED=${MOVED:+$MOVED,}$a
        else
          DELETED=${DELETED:+$DELETED,}$a
          if [ "$c" != ledger.json ]; then
            CREATED=${CREATED:+$CREATED }$a
          fi
        fi
      fi
    fi
  done <"$WORK"
  return 0
}

op_restore() {
  check_work
  check_roots
  if ! verify_staged; then
    refuse staged "a staged file does not match the work list" "\"bad\":[$BAD]"
  fi
  if [ ! -d "$DE/snapshots/$RUN" ] || [ -L "$DE/snapshots" ] || [ -L "$DE/snapshots/$RUN" ]; then
    refuse no_snapshot "the run has no snapshot on this host"
  fi
  journal_unchanged || refuse journal_changed "the journal changed since it was read"
  if ! replace_with "$ST/journal.restoring.json" "$DE/journal.json"; then
    refuse journal "the journal could not be set to restoring"
  fi
  CREATED=''
  _needs_coll=0
  while IFS=$TAB read -r k a b c d; do
    if [ "$k" = P ] && [ "$b" != '-' ] && [ "$c" != ledger.json ]; then
      _needs_coll=1
    fi
  done <"$WORK"
  if [ "$_needs_coll" -eq 1 ] && ! ensure_collection; then
    FAIL="the mode collection could not be created"
    report restoring
    exit 0
  fi
  # collection files first, the ledger last
  if ! restore_pass 0 || ! restore_pass 1; then
    report restoring
    exit 0
  fi
  if [ -d "$CO" ] && [ ! -L "$CO" ]; then
    for _i in $CREATED; do
      while IFS=$TAB read -r k a b c d; do
        if [ "$k" = P ] && [ "$a" = "$_i" ]; then
          RMWHAT="removing an emptied directory failed"
          rmdir_up "$a" "$c" || {
            report restoring
            exit 0
          }
        fi
      done <"$WORK"
    done
  fi
  report restoring
}

op_fingerprint() {
  check_work
  check_roots
  _out=''
  while IFS=$TAB read -r k a b c d; do
    if [ "$k" = P ]; then
      if [ "$c" = ledger.json ]; then
        live "$DE" ledger.json
      else
        live "$CO" "$c"
      fi
      if [ "$LIVE" = '!' ]; then
        refuse not_regular "a path is not a regular file" "\"at\":$a"
      fi
      _out=${_out:+$_out,}\"$LIVE\"
    fi
  done <"$WORK"
  emit "{\"ok\":1,\"hashes\":[$_out]}"
}

op_probe() {
  check_work
  check_roots
  _problems=''
  _add() {
    _problems=${_problems:+$_problems,}\"$1\"
  }
  if [ -L "$CO" ]; then
    _add coll_symlink
  elif [ -d "$CO" ]; then
    [ -w "$CO" ] || _add coll_unwritable
  elif [ -e "$CO" ]; then
    _add coll_not_dir
  else
    _res=${CO%/*}
    if [ -d "$_res" ] && [ ! -L "$_res" ]; then
      [ -w "$_res" ] || _add resources_unwritable
    else
      _add resources_missing
    fi
  fi
  _ok=1
  for _d in "${DE%/*/*}" "${DE%/*}" "$DE"; do
    if [ -L "$_d" ]; then
      _add deploy_symlink
      _ok=0
      break
    elif [ -d "$_d" ]; then
      :
    elif [ -e "$_d" ]; then
      _add deploy_not_dir
      _ok=0
      break
    elif ! mkdir -- "$_d" 2>/dev/null; then
      _add deploy_uncreatable
      _ok=0
      break
    fi
  done
  if [ "$_ok" -eq 1 ]; then
    _pf=$DE/probe-$$.tmp
    rm -f -- "$_pf"
    if (
      set -C
      : >"$_pf"
    ) 2>/dev/null; then
      rm -f -- "$_pf"
    else
      _add deploy_unwritable
    fi
  fi
  emit "{\"ok\":1,\"problems\":[$_problems]}"
}

op_finalize() {
  check_work
  check_roots
  [ -n "$RECDOC" ] || refuse bad_work "the work list names no record"
  if ! verify_staged; then
    refuse staged "a staged file does not match the work list" "\"bad\":[$BAD]"
  fi
  if [ ! -d "$DE/snapshots/$RUN" ] || [ -L "$DE/snapshots" ] || [ -L "$DE/snapshots/$RUN" ]; then
    refuse no_snapshot "the run has no snapshot on this host"
  fi
  journal_unchanged || refuse journal_changed "the journal changed since it was read"
  if ! replace_with "$ST/$RECDOC" "$DE/snapshots/$RUN/record.json"; then
    refuse record "the run record could not be installed"
  fi
  if [ "$JSEEN" != '-' ]; then
    rm -f -- "$DE/journal.json" || refuse journal "the journal could not be removed"
  fi
  if [ -d "$DE/staging" ] && [ ! -L "$DE/staging" ]; then
    rm -rf -- "$DE/staging/$RUN" || refuse staging "the staging directory could not be removed"
  fi
  emit '{"ok":1}'
}

op_discard() {
  check_work
  check_roots
  journal_unchanged || refuse journal_changed "the journal changed since it was read"
  if [ -d "$DE/snapshots" ] && [ ! -L "$DE/snapshots" ]; then
    rm -rf -- "$DE/snapshots/$RUN" || refuse snapshot "the snapshot could not be removed"
  fi
  if [ -d "$DE/staging" ] && [ ! -L "$DE/staging" ]; then
    rm -rf -- "$DE/staging/$RUN" || refuse staging "the staging directory could not be removed"
  fi
  if [ "$JSEEN" != '-' ]; then
    rm -f -- "$DE/journal.json" || refuse journal "the journal could not be removed"
  fi
  emit '{"ok":1}'
}

# mirror_check_one: refuse code for BASE/REL kind KIND of a mirror entry AT.
# (name of the entry, as W<i> or X<i>, in the refusal's "at".)
mirror_kind_refusal() {
  case $KIND in
    symlink) refuse symlink "a path is or is below a symlink" "\"at\":\"$1\"" ;;
    unsafe) refuse not_regular "a path is not a regular file or has a file where a directory must be" "\"at\":\"$1\"" ;;
  esac
}

# mirror_precheck: everything that can be refused is refused before the first
# write, as the per-file path did: the resources directories, the collection,
# every delete target, every write target and every staged copy.
mirror_precheck() {
  _res=${CO%/*}
  _cfg=${_res%/*}
  for _d in "$_cfg" "$_res"; do
    if [ -L "$_d" ]; then
      refuse symlink "config/resources is or is below a symlink" "\"at\":\"R\""
    fi
    [ -d "$_d" ] || refuse resources "config/resources is not a directory"
  done
  if [ -L "$CO" ]; then
    refuse symlink "the mode collection is a symlink" "\"at\":\"C\""
  fi
  if [ -e "$CO" ] && [ ! -d "$CO" ]; then
    refuse coll_not_dir "the mode collection is not a directory"
  fi
  while IFS=$TAB read -r k a b c d; do
    case $k in
      X)
        classify "$CO" "$b"
        mirror_kind_refusal "X$a"
        [ "$KIND" = file ] || refuse not_file "a delete target is not a regular file" "\"at\":\"X$a\""
        ;;
      W)
        classify "$CO" "$d"
        mirror_kind_refusal "W$a"
        [ "$KIND" != dir ] || refuse is_dir "a write target is a directory" "\"at\":\"W$a\""
        ;;
    esac
  done <"$WORK"
  if ! verify_staged; then
    # the staged copies that failed are removed from the staging root, only
    # regular files with no symlink above them, as the per-file path did
    _removed=0
    while IFS=$TAB read -r k a b c d; do
      if [ "$k" = W ]; then
        classify "$SRCDIR" "$d"
        if [ "$KIND" = file ] && rm -f -- "$SRCDIR/$d"; then
          _removed=$((_removed + 1))
        fi
      fi
    done <"$WORK"
    refuse staged "a staged copy does not match the work list" "\"bad\":[$BAD],\"removed\":$_removed"
  fi
}

# mirror_deletes: the delete set, then the directories it emptied.
mirror_deletes() {
  while IFS=$TAB read -r k a b c d; do
    if [ "$k" = X ]; then
      classify "$CO" "$b"
      if [ "$KIND" != file ]; then
        FAIL="delete $a: the target is no longer a regular file"
        return 1
      fi
      if ! rm -- "$CO/$b"; then
        FAIL="delete $a: cannot delete the file"
        return 1
      fi
      DELETED=${DELETED:+$DELETED,}$a
    fi
  done <"$WORK"
  while IFS=$TAB read -r k a b c d; do
    if [ "$k" = X ]; then
      RMWHAT="delete $a"
      rmdir_up "$a" "$b" || return 1
    fi
  done <"$WORK"
  return 0
}

op_mirror() {
  check_work
  check_roots
  srcdir
  mirror_precheck
  if ! ensure_collection; then
    refuse resources "the mode collection could not be created"
  fi
  if moves && mirror_deletes; then
    :
  fi
  emit "{\"ok\":1,\"moved\":[$MOVED],\"deleted\":[$DELETED],\"dirs\":[$DIRS],\"fail\":$(fail_json)}"
}

op_verify() {
  check_work
  check_roots
  if ! verify_staged; then
    refuse staged "a staged file does not match the work list" "\"bad\":[$BAD]"
  fi
  emit '{"ok":1}'
}

op_unstage() {
  check_work
  check_roots
  if [ -e "$DE/staging" ] || [ -L "$DE/staging" ]; then
    real_dir "$DE/staging" || refuse unsafe "the staging area is not a real directory"
    rm -rf -- "$DE/staging/$RUN"
  fi
  emit '{"ok":1}'
}

op_commit() {
  check_work
  check_roots
  srcdir
  # read-only: the staged files are what the list says
  if ! verify_staged; then
    refuse staged "a staged file does not match the work list" "\"bad\":[$BAD]"
  fi
  # 1. the journal, exclusively; an existing one refuses
  create_journal "$ST/journal.snapshotting.json" && _created=0 || _created=$?
  case $_created in
    0) ;;
    1) refuse journal_present "a journal of another run is in place" ;;
    *) refuse journal "the journal could not be created" ;;
  esac
  # 2. every touched path against its pre-image
  if ! stale_check; then
    abort_before_move
    report snapshotting
    exit 0
  fi
  # 3. snapshot
  snapshot_all && _snap=0 || _snap=$?
  case $_snap in
    0) ;;
    1)
      STALE=$SNAPBAD
      abort_before_move
      report snapshotting
      exit 0
      ;;
    *)
      abort_before_move
      refuse snapshot "the snapshot could not be made"
      ;;
  esac
  # 4. the record, at state applying
  if [ -e "$DE/snapshots/$RUN/record.json" ] || ! cp -- "$ST/record.applying.json" "$DE/snapshots/$RUN/record.json" ||
    ! chmod 600 "$DE/snapshots/$RUN/record.json"; then
    abort_before_move
    refuse record "the run record could not be written"
  fi
  # 5. phase ready, then writing
  if ! replace_with "$ST/journal.ready.json" "$DE/journal.json"; then
    abort_before_move
    refuse journal "the journal could not be advanced"
  fi
  if ! replace_with "$ST/journal.writing.json" "$DE/journal.json"; then
    abort_before_move
    refuse journal "the journal could not be advanced"
  fi
  if ! ensure_collection; then
    FAIL="the mode collection could not be created"
    report writing
    exit 0
  fi
  # 6. move each staged file over its target
  if ! moves; then
    report writing
    exit 0
  fi
  # 7. the delete set, then the directories it emptied
  if ! deletes; then
    report writing
    exit 0
  fi
  # 8. the ledger, last
  if ! swap_ledger; then
    report writing
    exit 0
  fi
  # 9. phase written
  if ! replace_with "$ST/journal.written.json" "$DE/journal.json"; then
    FAIL="cannot write journal phase written"
    report writing
    exit 0
  fi
  report written
}

case $OP in
  commit) op_commit ;;
  restore) op_restore ;;
  mirror) op_mirror ;;
  fingerprint) op_fingerprint ;;
  probe) op_probe ;;
  finalize) op_finalize ;;
  discard) op_discard ;;
  verify) op_verify ;;
  unstage) op_unstage ;;
  *) refuse bad_op "this script does not know the operation" ;;
esac
