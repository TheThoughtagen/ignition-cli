#!/usr/bin/env bash
# capture25.sh SUBCOMMAND [ARG ...]
#
# Phase 25 (25-01) rig capture driver: how the 8.3.6 gateway imports and
# re-exports a project zip built from a git checkout, recorded before any
# `deploy projects` code relies on it.
#
#   c8c-synth  C8c on a small synthetic checkout: zip it in the checkout
#              layout and in the member_path (export) layout, import each with
#              overwrite=false and overwrite=true, re-export, compare members
#   c8c-whk    C8c on a zip of ~/data/projects/WHK-Global built with the
#              research allowlist (root project.json, top-level `ignition` and
#              module-id directories); the source checkout is copied, never
#              modified
#   c8c-probe  C8c layout-recognition probe: does the gateway treat an
#              export-layout zip (<category>/resources/<rest>) as resources, or
#              store it as opaque paths? Cross-layout overwrite in both
#              directions, projects/copy, and the gateway's on-disk file names
#   c8d        C8d: one variant per zip for a file not listed in its folder's
#              resource.json, a dotfile, an empty directory, a folder without
#              resource.json, a *.py sidecar, a __pycache__ directory, and a
#              resource.json that lists a file the zip lacks
#   c8a        C8a: does import?overwrite=true delete what the zip omits
#   c8b        C8b: export, overwrite import, export again; per-member sha256
#              raw and after resource.json normalisation
#   c8e        C8e: parent edge cases and project-list membership
#   c9         C9 (25-02): what a parent overwrite does to a child project
#   c10up|c10|c10down
#              C10 (25-02): a disposable gateway with the signed git module
#              (compose project ign-c10-rig, port 18291): does an API import
#              interfere with the module, is a removed .git recreated
#   ch HOST    C-H (25-02), HOST lnx or win: what the deploy account sees of
#              data/projects/<name>/.git on a capture host (needs the
#              IGN_LNXVM_* or IGN_WINVM_* variables of 23-LIVE-CAPTURES.md)
#
# Subcommands: header25 | zipco DIR OUT [checkout|export] | members ZIP
#              | cmp ZIP EXPORT | cmpsum ZIP EXPORT | files ZIP MEMBER
#              | findsel NAME | gwdisk NAME | note TEXT | c8c-synth | c8c-probe | c8c-whk | c8c-whk-rj | rjdiff ZIP EXPORT | c8d | c8a | c8b | c8e | c9 | c10up | c10 | c10down | ch HOST
#              | c8del NAME | sect ID TEXT | shcap LABEL CMD [ARG ...]
#              | up25 (25-19: the rig with data/projects and config/resources bound
#                onto a host data root, token minted)
#              | down25 (prints the teardown command, both compose files; it does not run it)
#
# The rig is the Phase 22 compose project `ign-deploy-rig` (port 18290),
# brought up with `capture23.sh up23` then `capture23.sh setup23`. Every
# project this script creates is named CAP25-* and is deleted by the
# subcommand that created it; c8del refuses any other name.
#
# Every byte inside a capture section is appended by `cap` (HTTP), `capdl`
# (a download kept on disk, only its headers, size and sha256 recorded) or
# `shcap` (any other command) and is preceded by a line carrying the exact
# command and a UTC time taken by this script just before the run. The file
# holds member names, sizes, sha256 values, statuses and counts only: no token
# value and no project file content.
#
# LOCALHOST ONLY. assert_local runs before anything else (before any file is
# read or written, before any request) and exits 2 unless BASE's host is
# localhost, 127.0.0.1 or [::1]. The authority is cut at the first `/`, `?`,
# `#` or `\` BEFORE userinfo is stripped, so 'http://evil.example#@localhost'
# is refused (the R-01 / WR-08 fix of Phase 21, as in capture22.sh).
# Tokens come from tokens.env (0600, outside the repo), are sent as a request
# header and are never printed: the command line names the token variable,
# never its value. Never enable `set -x` here.
set -euo pipefail

BASE=${BASE:-http://localhost:18290}
# C10 runs on its own disposable gateway (compose project ign-c10-rig), never
# on the shared rig: its subcommands pin BASE to that gateway's loopback port
# before assert_local runs.
case "${1:-}" in
  c10 | c10up | c10down | c10cfg | c10log | c10state | c10poll) BASE=http://localhost:18291 ;;
esac

assert_local() {
  local rest authority hostport host
  case "$BASE" in
    *[[:space:]]* | *[[:cntrl:]]*) echo "REFUSING: BASE holds whitespace or a control character" >&2; exit 2 ;;
  esac
  rest=${BASE#*://}
  # Cut the authority first: userinfo may only be stripped from what precedes
  # the first '/', '?', '#' or '\'.
  authority=${rest%%[/?#\\]*}
  hostport=${authority##*@}
  case "$hostport" in
    \[*) host=${hostport%%]*}; host=${host#[} ;;
    *) host=${hostport%%:*} ;;
  esac
  case "$host" in
    localhost | 127.0.0.1 | ::1) ;;
    *) echo "REFUSING: BASE host '$host' is not localhost, 127.0.0.1 or ::1" >&2; exit 2 ;;
  esac
}
assert_local
export BASE

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(git -C "$HERE" rev-parse --show-toplevel)
OUT=${CAPTURE_OUT:-$REPO/.planning/phases/25-guarded-project-deploy/25-LIVE-CAPTURES.md}
STATE=${IGN_DEPLOY_RIG_STATE:-$HOME/.cache/ign-deploy-rig}
W=$STATE/c8
WHK_SRC=${WHK_GLOBAL:-$HOME/data/projects/WHK-Global}
IGN=${IGN_BIN:-ign}
export CAPTURE25=$HERE/capture25.sh
TMPD=$(mktemp -d)
trap '/bin/rm -rf "$TMPD"' EXIT

# shellcheck disable=SC1091
[ -f "$STATE/tokens.env" ] && . "$STATE/tokens.env"
# C10 subcommands use the tokens of their own gateway, never the shared rig's.
case "${1:-}" in
  c10 | c10up | c10down | c10cfg | c10log | c10state | c10poll)
    unset IGN_TOKEN_CAP_A IGN_TOKEN_CAP_B IGN_TOKEN_CAP_RO
    # shellcheck disable=SC1091
    [ -f "$STATE/c10/tokens.env" ] && . "$STATE/c10/tokens.env" || true ;;
esac

PRJ=/data/api/v1/projects
# The disposable rig's admin password is public in compose.yml; it is used
# only for the 401/403 retry of a write and is never printed.
ADMIN_PW=$(awk -F': *' '/GATEWAY_ADMIN_PASSWORD/ {print $2; exit}' "$HERE/compose.yml" | tr -d '"')
ZIPCT=(-H 'Content-Type: application/zip')

utc() { date -u +%FT%TZ; }
h() { printf '\n## %s\n' "$1" >> "$OUT"; }
sub() { printf '\n### %s\n' "$1" >> "$OUT"; }
# prose TEXT: a line of script-written commentary (never gateway bytes).
prose() { printf '\n%s\n' "$1" >> "$OUT"; }
die() { echo "$*" >&2; exit 64; }

# sect ID [TEXT]: the `## ID` heading (exactly the id, so the section is found
# by name) and, when TEXT is given, one line of commentary under it.
sect() {
  case "${1:-}" in
    C8a | C8b | C8c | C8d | C8e | C9 | C10 | C-H | 'C-H before grant') ;;
    *) echo "sect: unknown capture id '${1:-}'" >&2; exit 64 ;;
  esac
  h "$1"
  [ -z "${2:-}" ] || prose "$2"
}

# cap LABEL METHOD PATH [curl args]: one verbatim HTTP exchange. A line with
# the command and a script-taken UTC time precedes the fence. CAP_TOK_VAR names
# the token variable (default IGN_TOKEN_CAP_A; NONE sends no token; BASIC sends
# the rig admin over HTTP basic auth). The command line names the variable,
# never its value. CAP_MAX is the curl timeout in seconds (default 90);
# CAP_BODY_MAX, when set, shows only that many body bytes plus the full size
# and sha256 of the body.
LAST_BODY=""
LAST_STATUS=""
cap() {
  local label=$1 method=$2 path=$3; shift 3
  local tv=${CAP_TOK_VAR:-IGN_TOKEN_CAP_A} tok="" started shown args
  local hdr=$TMPD/hdr bod=$TMPD/bod
  local auth=()
  case "$tv" in
    NONE) shown='no token' ;;
    BASIC) shown='basic auth, rig admin'; auth=(-u "admin:$ADMIN_PW") ;;
    *)
      tok=${!tv:?$tv is not set; run capture23.sh setup23 and source tokens.env}
      auth=(-H "X-Ignition-API-Token: $tok")
      shown="token \$$tv"
      ;;
  esac
  args="${*//$STATE/\$STATE}"
  started=$(utc)
  local meta
  meta=$(curl -sS -g -D "$hdr" -o "$bod" -m "${CAP_MAX:-90}" -w '[curl http=%{http_code} total=%{time_total}s]' \
    -X "$method" ${auth[@]+"${auth[@]}"} "$@" "$BASE$path" || true)
  {
    printf '\n#### %s\n\n`curl -sS -g -i -X %s [%s] %s%s` @ %s\n\n~~~\n' \
      "$label" "$method" "$shown" "${args:+$args }" "'\$BASE$path'" "$started"
    tr -d '\r' < "$hdr"
    if [ -n "${CAP_BODY_MAX:-}" ] && [ "$(wc -c < "$bod" | tr -d ' ')" -gt "$CAP_BODY_MAX" ]; then
      head -c "$CAP_BODY_MAX" "$bod" | tr -d '\r'
      printf '\n[body truncated to %s bytes; full body %s bytes, sha256 %s]\n' \
        "$CAP_BODY_MAX" "$(wc -c < "$bod" | tr -d ' ')" "$(shasum -a 256 "$bod" | cut -d' ' -f1)"
    else
      tr -d '\r' < "$bod"
    fi
    printf '\n%s\n~~~\n' "$meta"
  } >> "$OUT"
  LAST_STATUS=$(sed -n 's/^\[curl http=\([0-9]*\) .*/\1/p' <<<"$meta" | tail -1)
  LAST_BODY=$(tr -d '\r' < "$bod")
}

# capw LABEL METHOD PATH [curl args]: a write. A 401 or 403 is retried once
# with the rig admin over HTTP basic auth, and both exchanges are recorded, so
# the record shows which credential each write needed.
capw() {
  cap "$@"
  if [ "$LAST_STATUS" = 401 ] || [ "$LAST_STATUS" = 403 ]; then
    prose "Script reading: the API token answered $LAST_STATUS; retrying the call once with the rig admin credentials over HTTP basic auth."
    CAP_TOK_VAR=BASIC cap "$1 (retry, basic auth)" "${@:2}"
  fi
}

# capdl LABEL PATH OUTFILE: a GET whose body is kept on disk. Only the status,
# the response headers, the size and the sha256 are recorded, never the bytes.
capdl() {
  local label=$1 path=$2 out=$3
  local tv=${CAP_TOK_VAR:-IGN_TOKEN_CAP_A} started meta
  local tok=${!tv:?$tv is not set}
  local hdr=$TMPD/dlhdr
  started=$(utc)
  meta=$(curl -sS -g -D "$hdr" -o "$out" -m "${CAP_MAX:-120}" -w '[curl http=%{http_code} total=%{time_total}s]' \
    -H "X-Ignition-API-Token: $tok" "$BASE$path" || true)
  {
    printf '\n#### %s\n\n`curl -sS -g -D - -o %s -X GET [token $%s] %s` @ %s\n\n~~~\n' \
      "$label" "'\$STATE${out#"$STATE"}'" "$tv" "'\$BASE$path'" "$started"
    tr -d '\r' < "$hdr"
    printf 'size_bytes=%s\nsha256=%s\n%s\n~~~\n' \
      "$(wc -c < "$out" | tr -d ' ')" "$(shasum -a 256 "$out" | cut -d' ' -f1)" "$meta"
  } >> "$OUT"
  LAST_STATUS=$(sed -n 's/^\[curl http=\([0-9]*\) .*/\1/p' <<<"$meta" | tail -1)
}

# shcap LABEL CMD [ARG ...]: one verbatim local command block. Refuses (exit 2)
# when an argument or the output holds a token value; nothing is appended in
# that case.
LAST_OUT=""
shcap() {
  local label=${1:?shcap LABEL CMD [ARG ...]}; shift
  [ $# -gt 0 ] || { echo "shcap: no command given" >&2; exit 2; }
  local a v val key
  for a in "$@"; do
    for v in IGN_TOKEN_CAP_A IGN_TOKEN_CAP_B IGN_TOKEN_CAP_RO; do
      val=${!v:-}
      if [ -n "$val" ]; then
        key=${val#*:}
        case "$a" in
          *"$val"* | *"$key"*) echo "REFUSING: a shcap argument contains the value of $v" >&2; exit 2 ;;
        esac
      fi
    done
  done
  local out=$TMPD/shcap.out rc=0 started shown
  started=$(utc)
  "$@" > "$out" 2>&1 || rc=$?
  tr -d '\r' < "$out" > "$out.nocr"
  for v in IGN_TOKEN_CAP_A IGN_TOKEN_CAP_B IGN_TOKEN_CAP_RO; do
    val=${!v:-}
    if [ -n "$val" ]; then
      key=${val#*:}
      if grep -qF -- "$key" "$out.nocr"; then
        echo "REFUSING: command output contains the value of $v; nothing appended" >&2
        exit 2
      fi
    fi
  done
  LAST_OUT=$(cat "$out.nocr")
  shown=$(printf '%q ' "$@")
  shown=${shown//$STATE/\$STATE}
  shown=${shown//$HERE\//}
  shown=${shown//$HOME/\~}
  {
    printf '\n#### %s\n\n`%s` @ %s\n\n~~~\n' "$label" "${shown% }" "$started"
    cat "$out.nocr"
    if [ -s "$out.nocr" ] && [ "$(tail -c1 "$out.nocr" | wc -l | tr -d ' ')" = 0 ]; then
      printf '\n[output did not end with a newline]\n'
    fi
    printf '[exit %s]\n~~~\n' "$rc"
  } >> "$OUT"
}

# ------------------------------------------------------------ pure helpers

# zipco DIR OUT [layout]: zip DIR into OUT with `zip -X`, entries sorted
# (directory entries included, as `zip -r` writes them). layout `checkout` is
# verbatim; `export` rewrites each <collection>/<rest> member to
# <collection>/resources/<rest> (the member_path rule of client/resources.rs);
# project.json stays at the root and a top-level file X rides X/resources/X.
zipco() {
  local dir=${1:?DIR} out=${2:?OUT} layout=${3:-checkout} src c odir
  [ -d "$dir" ] || die "zipco: no such directory: $dir"
  case "$layout" in checkout | export) ;; *) die "zipco: layout must be checkout or export" ;; esac
  odir=$(cd "$(dirname "$out")" && pwd)
  out=$odir/$(basename "$out")
  src=$dir
  if [ "$layout" = export ]; then
    src=$(mktemp -d "$W/zipco.XXXXXX")
    while IFS= read -r c; do
      if [ "$c" = project.json ]; then
        cp "$dir/$c" "$src/$c"
      elif [ -d "$dir/$c" ]; then
        mkdir -p "$src/$c/resources"
        cp -R "$dir/$c/." "$src/$c/resources/"
      else
        mkdir -p "$src/$c/resources"
        cp "$dir/$c" "$src/$c/resources/$c"
      fi
    done < <(cd "$dir" && ls -A | LC_ALL=C sort)
  fi
  /bin/rm -f "$out"
  ( cd "$src" && find . -mindepth 1 | LC_ALL=C sort | sed 's|^\./||' | zip -X -q "$out" -@ )
  [ "$src" = "$dir" ] || /bin/rm -rf "$src"
}

# members ZIP: every entry sorted by name with its uncompressed size and
# sha256 (a directory entry shows 0 and -). Names, sizes and hashes only.
members() {
  python3 -I - "${1:?ZIP}" <<'PY'
import hashlib
import sys
import zipfile

z = zipfile.ZipFile(sys.argv[1])
print("member\tbytes\tsha256")
for i in sorted(z.infolist(), key=lambda i: i.filename):
    if i.filename.endswith("/"):
        print("%s\t0\t-" % i.filename)
    else:
        print("%s\t%d\t%s" % (i.filename, i.file_size, hashlib.sha256(z.read(i)).hexdigest()))
PY
}

# The comparison engine behind cmp and cmpsum. Keys are user-path keys: a
# `resources` second segment is dropped (the member_path inverse), so an
# export-layout zip and a checkout-layout export compare member for member.
# Normalisation (resource.json only): drop attributes.lastModification and
# lastModificationSignature, sort keys, the jq -S 'del(...)' of the plan.
cmp_py() {
  python3 -I - "$@" <<'PY'
import hashlib
import json
import sys
import zipfile

mode, pa, pb = sys.argv[1:4]


def key(name):
    parts = name.split("/")
    if len(parts) >= 3 and parts[1] == "resources":
        del parts[1]
    return "/".join(parts)


def load(path):
    out = {}
    dirs = 0
    z = zipfile.ZipFile(path)
    for i in z.infolist():
        if i.filename.endswith("/"):
            dirs += 1
            continue
        raw = z.read(i)
        out[key(i.filename)] = (i.filename, raw)
    return out, dirs


def norm(k, raw):
    if not k.endswith("resource.json"):
        return hashlib.sha256(raw).hexdigest()
    try:
        d = json.loads(raw)
        if isinstance(d, dict) and isinstance(d.get("attributes"), dict):
            d["attributes"].pop("lastModification", None)
            d["attributes"].pop("lastModificationSignature", None)
        return hashlib.sha256(json.dumps(d, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    except ValueError:
        return hashlib.sha256(raw).hexdigest()


a, da = load(pa)
b, db = load(pb)
keys = sorted(set(a) | set(b))
both = [k for k in keys if k in a and k in b]
only_a = [k for k in keys if k in a and k not in b]
only_b = [k for k in keys if k in b and k not in a]
raw_diff = [k for k in both if hashlib.sha256(a[k][1]).hexdigest() != hashlib.sha256(b[k][1]).hexdigest()]
norm_diff = [k for k in both if norm(k, a[k][1]) != norm(k, b[k][1])]

if mode == "table":
    print("member (user-path key) | in zip | in export | sha256 zip | sha256 export | equal | equal after normalisation")
    for k in keys:
        sa = hashlib.sha256(a[k][1]).hexdigest() if k in a else "-"
        sb = hashlib.sha256(b[k][1]).hexdigest() if k in b else "-"
        eq = "yes" if (k in a and k in b and sa == sb) else "no"
        en = "yes" if (k in a and k in b and norm(k, a[k][1]) == norm(k, b[k][1])) else "no"
        print("%s | %s | %s | %s | %s | %s | %s" % (k, "yes" if k in a else "no", "yes" if k in b else "no", sa, sb, eq, en))
    print("directory entries: zip %d, export %d" % (da, db))
else:
    print("members (files): zip %d, export %d, in both %d, zip only %d, export only %d" % (len(a), len(b), len(both), len(only_a), len(only_b)))
    print("directory entries: zip %d, export %d" % (da, db))
    print("in both, sha256 differs raw: %d" % len(raw_diff))
    print("in both, sha256 differs after resource.json normalisation: %d" % len(norm_diff))
    print("first 40 zip-only keys:")
    for k in only_a[:40]:
        print("  " + k)
    print("first 40 export-only keys:")
    for k in only_b[:40]:
        print("  " + k)
    print("first 20 raw-differing keys:")
    for k in raw_diff[:20]:
        print("  " + k)
    print("first 20 differing keys after normalisation:")
    for k in norm_diff[:20]:
        print("  " + k)

if "project.json" in both and "project.json" in raw_diff:
    ja, jb = json.loads(a["project.json"][1]), json.loads(b["project.json"][1])
    ks = sorted(k for k in set(ja) | set(jb) if ja.get(k, "<absent>") != jb.get(k, "<absent>"))
    print("project.json differing key names: " + (", ".join(ks) if ks else "(none; bytes differ only in formatting)"))
elif "project.json" in both:
    print("project.json: byte-identical")
PY
}
cmp_table() { cmp_py table "$1" "$2"; }
cmpsum() { cmp_py summary "$1" "$2"; }

# files ZIP MEMBER-SUFFIX-KEY: the `files` array (names only) of a
# resource.json member, by user-path key; `<absent>` when the member is not
# there.
files() {
  python3 -I - "${1:?ZIP}" "${2:?KEY}" <<'PY'
import json
import sys
import zipfile

z = zipfile.ZipFile(sys.argv[1])
want = sys.argv[2]
for n in z.namelist():
    parts = n.split("/")
    if len(parts) >= 3 and parts[1] == "resources":
        del parts[1]
    if "/".join(parts) == want:
        d = json.loads(z.read(n))
        print(json.dumps(d.get("files", "<no files key>")))
        break
else:
    print("<absent>")
PY
}

# findsel NAME: status plus the parent, enabled and inheritable fields of
# GET projects/find/NAME (never the whole record).
findsel() {
  local n=${1:?NAME} out code
  out=$(mktemp "$TMPD/find.XXXXXX")
  code=$(curl -sS -g -m 30 -o "$out" -w '%{http_code}' -H "X-Ignition-API-Token: ${IGN_TOKEN_CAP_A:?}" "$BASE$PRJ/find/$n" || true)
  printf 'status=%s ' "$code"
  jq -c '{parent, enabled, inheritable}' "$out" 2>/dev/null || printf '(body is not JSON)\n'
}

# plist_after: the closing line of a task block: the project list and how many
# CAP25-* projects remain.
plist_after() {
  local n
  cap 'GET projects/list (limit=-1), end of block' GET "$PRJ/list?limit=-1"
  n=$(jq '[.items[]? | .name | select(startswith("CAP25-"))] | length' <<<"$LAST_BODY" 2>/dev/null || echo '?')
  prose "Script reading: $n CAP25-* project(s) listed after this block; the rig list holds $(jq '.items | length' <<<"$LAST_BODY" 2>/dev/null || echo '?') project(s) in all."
}

# c8del NAME: delete a CAP25-* project (confirm=true). Any other name is
# refused with exit 2 before a request is made.
c8del() {
  local n=${1:?NAME}
  [[ $n =~ ^CAP25-[A-Za-z0-9_-]+$ ]] || { echo "REFUSING: c8del deletes CAP25-* projects only, not '$n'" >&2; exit 2; }
  capw "DELETE project $n" DELETE "$PRJ/$n?confirm=true"
}

# imp NAME OVERWRITE ZIP [label]: import a zip as NAME.
imp() {
  local n=$1 ow=$2 z=$3
  capw "${4:-POST import $n overwrite=$ow}" POST "$PRJ/import/$n?overwrite=$ow" "${ZIPCT[@]}" --data-binary "@$z"
}

# synth_tree DIR: the minimal synthetic checkout (checkout layout): project.json,
# one script folder cap25 (code.py listed by its resource.json) and one
# Perspective view Cap25 (view.json listed). The gateway accepted this minimal
# shape at the first try, so no gateway export had to be copied.
synth_tree() {
  local d=${1:?DIR}
  /bin/rm -rf "$d"
  mkdir -p "$d/ignition/script-python/cap25" "$d/com.inductiveautomation.perspective/views/Cap25"
  printf '{"title":"CAP25 synth","enabled":true,"inheritable":false}\n' > "$d/project.json"
  printf 'def f():\n    return 1\n' > "$d/ignition/script-python/cap25/code.py"
  printf '{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["code.py"],"attributes":{}}\n' > "$d/ignition/script-python/cap25/resource.json"
  printf '{"custom":{},"params":{},"props":{},"root":{"type":"ia.container.flex","children":[]}}\n' > "$d/com.inductiveautomation.perspective/views/Cap25/view.json"
  printf '{"scope":"G","version":1,"restricted":false,"overridable":true,"files":["view.json"],"attributes":{}}\n' > "$d/com.inductiveautomation.perspective/views/Cap25/resource.json"
}

# round NAME ZIP: import ZIP as NAME with overwrite=false, find, export,
# member lists and the zip-to-export comparison, then the same after an
# overwrite=true import. The project is left in place for the caller to delete.
round() {
  local n=$1 z=$2 e=$W/$1
  sub "$n — overwrite=false"
  shcap "members of the uploaded zip ($(basename "$z"))" bash "$CAPTURE25" members "$z"
  imp "$n" false "$z"
  shcap "GET projects/find/$n (status, parent, enabled, inheritable)" bash "$CAPTURE25" findsel "$n"
  capdl "GET projects/export/$n" "$PRJ/export/$n" "$e.1.zip"
  shcap "members of the export after overwrite=false" bash "$CAPTURE25" members "$e.1.zip"
  shcap "table: uploaded zip against export after overwrite=false" bash "$CAPTURE25" cmp "$z" "$e.1.zip"
  sub "$n — overwrite=true"
  imp "$n" true "$z"
  capdl "GET projects/export/$n (after overwrite=true)" "$PRJ/export/$n" "$e.2.zip"
  shcap "table: uploaded zip against export after overwrite=true" bash "$CAPTURE25" cmp "$z" "$e.2.zip"
}

# ------------------------------------------------------------------ header

# header25: write the file header and the Captures Owed table (creates the
# file; does nothing when it exists).
header25() {
  [ ! -e "$OUT" ] || { echo "header25: $OUT exists; leaving it" >&2; return 0; }
  curl -s -m 5 "$BASE/StatusPing" | grep -q RUNNING || { echo "gateway is not RUNNING" >&2; exit 1; }
  local cid image mode
  cid=$(docker ps -q --filter "label=com.docker.compose.project=ign-deploy-rig" --filter "label=com.docker.compose.service=gw")
  image=$(docker inspect -f '{{.Config.Image}}' "$cid")
  mode=$(curl -sS -m 10 -H "X-Ignition-API-Token: ${IGN_TOKEN_CAP_A:?}" "$BASE/data/api/v1/gateway-info" | jq -r '.deploymentMode')
  mkdir -p "$(dirname "$OUT")"
  {
    printf '# Phase 25 — Live Captures (8.3.6 rig)\n\n| Field | Value |\n| --- | --- |\n'
    printf '| Date (UTC) | %s |\n' "$(date -u +%F)"
    printf '| Image | %s (platform linux/amd64, emulated on an arm64 host) |\n' "$image"
    printf '| Compose project | ign-deploy-rig (compose.yml, this fixture directory) |\n'
    printf '| deploymentMode | %s |\n' "$mode"
    printf '| Rig started (UTC) | %s |\n' "$(cat "$STATE/start-utc-22" 2>/dev/null || echo unknown)"
    printf '| Docker server | %s |\n' "$(docker version -f '{{.Server.Version}}' 2>/dev/null || echo unknown)"
    printf '| Docker Compose | %s |\n' "$(docker compose version --short 2>/dev/null || echo unknown)"
    printf '| Runner | capture25.sh (crates/ignition-core/tests/fixtures/live-rig-deploy) |\n'
    printf '\nRule: every captured block is preceded by the exact command and a UTC time taken by the driver just before it. A request line names the token variable it used, never the token. Tokens live in a 0600 file outside the repo. This file holds member names, sizes, sha256 values, statuses and counts only: no token value and no project file content. Every project the captures create is named `CAP25-*` and is deleted at the end of the block that made it.\n'
    printf '\n## Captures Owed\n\n'
    printf 'Written before any capture block (capture first, as in Phases 20 to 24). Each row names the question, the CONTEXT decision or research finding it settles, and the plan that reads the record.\n\n'
    printf '| ID | Question | Settles | Read by |\n| --- | --- | --- | --- |\n'
    printf '| C8a | Does `import?overwrite=true` delete resources the zip omits (a dropped member, a dropped folder, a dropped `resource.json`-listed file)? What does each answer list under `changes`? | Research C8a (silent emptying on overwrite); contract item K5 | 25-03, 25-11 |\n'
    printf '| C8b | Does a gateway-produced export survive export, overwrite import, export unchanged? Which members and fields still differ before and after `resource.json` normalisation, `project.json` included? | D-09 canonical hash; contract item K4 | 25-06, 25-11 |\n'
    printf '| C8c | Which layout does the importer adopt: the checkout layout (no `resources/` segment) or the `member_path`-adapted export layout? Which layout does the export come back in? Both for a synthetic checkout and for the WHK-Global allowlist zip. | Research F1, D-09 (zip the repo root); contract item K1 | 25-04, 25-06 |\n'
    printf '| C8d | What does the gateway do with a file present but not listed in its folder'"'"'s `resource.json`, a dotfile, an empty directory, a folder with no `resource.json`, a `*.py` sidecar, a `__pycache__` directory, and a listed file the zip lacks? | Research F2 (Assumptions A2, A3); contract items K2, K3 | 25-06 |\n'
    printf '| C8e | What does import answer for a `parent` naming a missing project, the project itself, and a non-inheritable project? Does `projects/list` show a disabled project and an inheritable parent? | D-05 (parent presence from the list or from `find`); Assumption A5; contract item K6 | 25-10 |\n'
    printf '| C9 | What does a child project do after its parent is overwritten? | D-06; contract item K7 | 25-15 |\n'
    printf '| C10 | Does the git module interfere with a project import? | D-11; contract item K8 | 25-17 |\n'
    printf '| C-H | What does a host `.git` probe and the deploy account'"'"'s rights on `data/projects` answer? | DPRJ-04, research F3; contract item K9 | 25-07, 25-08 |\n'
    printf '\nC8a to C8e are recorded by 25-01 (this plan). C9, C10 and C-H are owed to 25-02.\n'
  } > "$OUT"
}

# ------------------------------------------------------------------- C8c

c8c_synth() {
  local layout n z
  sect C8c 'C8c: which layout the 8.3.6 importer adopts. A synthetic checkout (project.json, one script folder, one Perspective view) is zipped in the checkout layout (no `resources/` segment) and in the `member_path` export layout (`<collection>/resources/<rest>`), each imported with overwrite=false into a fresh CAP25-SYNTH-* project and again with overwrite=true, and re-exported. Member keys in the tables are user-path keys (the `resources/` segment dropped), so a checkout-layout zip and its export compare member for member; the raw member names are in the member lists above each table.'
  mkdir -p "$W"
  cap 'GET projects/list (before)' GET "$PRJ/list?limit=-1"
  synth_tree "$W/synth"
  shcap 'the synthetic checkout (relative paths)' bash -c 'cd "$1" && find . -type f | LC_ALL=C sort' _ "$W/synth"
  for layout in checkout export; do
    n=CAP25-SYNTH-$layout
    z=$W/synth-$layout.zip
    zipco "$W/synth" "$z" "$layout"
    sub "layout $layout"
    round "$n" "$z"
    c8del "$n"
  done
  sub 'end of block'
  plist_after
}


# gwdisk NAME: the file names (relative, never content) the gateway keeps for a
# CAP25-* project under its data/projects directory, read with `docker exec` on
# the rig container.
gwdisk() {
  local n=${1:?NAME} cid root=/usr/local/bin/ignition/data/projects
  [[ $n =~ ^CAP25-[A-Za-z0-9_-]+$ ]] || die "gwdisk: CAP25-* names only, not '$n'"
  cid=$(docker ps -q --filter "label=com.docker.compose.project=ign-deploy-rig" --filter "label=com.docker.compose.service=gw")
  [ -n "$cid" ] || die "gwdisk: rig container not found"
  docker exec "$cid" find "$root/$n" \( -type f -o -type l \) | sed "s|^$root/||" | LC_ALL=C sort
}

# synth_v2 DIR: the synthetic checkout with a distinguishable code.py (returns
# 2), so a later overwrite shows which path the gateway wrote it to.
synth_v2() {
  synth_tree "$1"
  printf 'def f():\n    return 2\n' > "$1/ignition/script-python/cap25/code.py"
}

# cpy FROM TO: POST projects/copy.
cpy() {
  capw "POST projects/copy $1 -> $2" POST "$PRJ/copy" -H 'Content-Type: application/json' \
    --data "{\"fromName\":\"$1\",\"toName\":\"$2\"}"
}

# probe_state NAME TAG: the project's disk names and its export members.
probe_state() {
  local n=$1 tag=$2
  shcap "gateway disk file names of $n ($tag)" bash "$CAPTURE25" gwdisk "$n"
  capdl "GET projects/export/$n ($tag)" "$PRJ/export/$n" "$W/$n.$tag.zip"
  shcap "members of the export of $n ($tag)" bash "$CAPTURE25" members "$W/$n.$tag.zip"
}

# c8c_probe: does the 8.3.6 gateway recognise an export-layout zip
# (<category>/resources/<rest>, the old 8.3.3 member_path rule) as resources, or
# store it as opaque paths? The synthetic checkout round trip (c8c-synth) keeps
# zip paths as given, which cannot tell the two apart; these probes can:
#   A  checkout layout v1, then overwrite with export layout v2
#   B  export layout v1, then overwrite with checkout layout v2
#   C  copy of an export-layout project, and of a checkout-layout project
# If the gateway maps the export layout onto the same resources, the overwrite
# in A replaces ignition/script-python/cap25/code.py (v1 -> v2) with no
# ignition/resources/ names, and the copy exports in one layout whatever the
# source used. If it stores opaque paths, A leaves v1 in place beside a new
# ignition/resources/ v2 and the copy exports in the layout of its source.
c8c_probe() {
  mkdir -p "$W"
  sub 'C8c layout-recognition probe'
  prose 'Question: does the gateway recognise an export-layout zip (`<category>/resources/<rest>`) as resources, or only store it as opaque paths? The synthetic round trip above shows the gateway keeps zip paths as given, which cannot tell the two apart. code.py v1 returns 1 and v2 returns 2; the sha256 of code.py tells which version a path holds. Gateway disk file names are read with `docker exec` on the rig container (names only).'
  shcap 'sha256 of code.py v1 and v2 (the two versions told apart below)' bash -c 'printf "def f():\n    return 1\n" | shasum -a 256 | cut -d" " -f1 | sed "s/^/v1 /"; printf "def f():\n    return 2\n" | shasum -a 256 | cut -d" " -f1 | sed "s/^/v2 /"'
  synth_tree "$W/p1"; synth_v2 "$W/p2"
  zipco "$W/p1" "$W/p1-checkout.zip" checkout
  zipco "$W/p1" "$W/p1-export.zip" export
  zipco "$W/p2" "$W/p2-checkout.zip" checkout
  zipco "$W/p2" "$W/p2-export.zip" export

  sub 'Probe A: import checkout layout v1, overwrite with export layout v2'
  imp CAP25-PROBE-A false "$W/p1-checkout.zip" 'POST import CAP25-PROBE-A overwrite=false (checkout layout, code.py v1)'
  probe_state CAP25-PROBE-A after-checkout-v1
  imp CAP25-PROBE-A true "$W/p2-export.zip" 'POST import CAP25-PROBE-A overwrite=true (export layout, code.py v2)'
  probe_state CAP25-PROBE-A after-export-v2-overwrite

  sub 'Probe B: import export layout v1, overwrite with checkout layout v2'
  imp CAP25-PROBE-B false "$W/p1-export.zip" 'POST import CAP25-PROBE-B overwrite=false (export layout, code.py v1)'
  probe_state CAP25-PROBE-B after-export-v1
  imp CAP25-PROBE-B true "$W/p2-checkout.zip" 'POST import CAP25-PROBE-B overwrite=true (checkout layout, code.py v2)'
  probe_state CAP25-PROBE-B after-checkout-v2-overwrite

  sub 'Probe C: projects/copy of an export-layout project and of a checkout-layout project'
  imp CAP25-PROBE-C false "$W/p1-export.zip" 'POST import CAP25-PROBE-C overwrite=false (export layout, code.py v1)'
  cpy CAP25-PROBE-C CAP25-PROBE-CC
  probe_state CAP25-PROBE-CC copy-of-export-layout
  imp CAP25-PROBE-D false "$W/p1-checkout.zip" 'POST import CAP25-PROBE-D overwrite=false (checkout layout, code.py v1)'
  cpy CAP25-PROBE-D CAP25-PROBE-DD
  probe_state CAP25-PROBE-DD copy-of-checkout-layout

  sub 'Probe cleanup'
  local n
  for n in CAP25-PROBE-A CAP25-PROBE-B CAP25-PROBE-C CAP25-PROBE-CC CAP25-PROBE-D CAP25-PROBE-DD; do c8del "$n"; done
  plist_after
}


# zhas ZIP PREFIX: how many entries of ZIP have a name starting with PREFIX.
zhas() { unzip -Z1 "${1:?ZIP}" | awk -v p="${2:?PREFIX}" 'index($0, p) == 1 { n++ } END { print n + 0 }'; }

# c8c_whk: C8c on a zip of the WHK-Global checkout built with the research
# allowlist (root project.json, the top-level `ignition` directory, and the
# top-level directories named like module ids, research A3). The source
# checkout is read, never modified: its `git status --porcelain` is recorded
# before and after. Layout: checkout (the layout the synthetic round trip and
# the layout probe settled).
c8c_whk() {
  local n=CAP25-WHK z=$W/whk.zip src=$W/whk c before after
  local kept=() excl=()
  [ -d "$WHK_SRC" ] || die "c8c-whk: no checkout at $WHK_SRC"
  mkdir -p "$W"
  sub 'C8c WHK-Global allowlist zip'
  prose 'Source: `~/data/projects/WHK-Global`, copied into `$STATE/c8/whk` keeping the root `project.json` and the top-level directories named `ignition` or matching `^[a-z][a-z0-9_-]*(\.[a-z0-9_-]+)+$` (research A3). Layout: checkout (verbatim), as settled by the synthetic round trip and the layout probe. `resource.json` normalisation is the one in the script (drop `attributes.lastModification` and `lastModificationSignature`, sort keys), the same transform as `jq -S '"'"'del(.attributes.lastModification, .attributes.lastModificationSignature)'"'"'`.'
  shcap 'git status --porcelain of the WHK-Global checkout (before)' git -C "$WHK_SRC" status --porcelain
  before=$LAST_OUT
  /bin/rm -rf "$src"; mkdir -p "$src"
  cp "$WHK_SRC/project.json" "$src/project.json"
  while IFS= read -r c; do
    [ "$c" = project.json ] && continue
    if [ -d "$WHK_SRC/$c" ] && { [ "$c" = ignition ] || [[ $c =~ ^[a-z][a-z0-9_-]*(\.[a-z0-9_-]+)+$ ]]; }; then
      cp -RP "$WHK_SRC/$c" "$src/$c"; kept+=("$c")
    else
      excl+=("$c")
    fi
  done < <(cd "$WHK_SRC" && /bin/ls -A | LC_ALL=C sort)
  sub 'C8c WHK-Global excluded roots'
  shcap 'top-level names kept (root project.json plus these directories)' printf '%s\n' "${kept[@]}"
  shcap 'top-level names excluded' printf '%s\n' "${excl[@]}"
  shcap 'allowlist copy: file count, symlink count, size in KiB' bash -c 'cd "$1" && printf "files %s\nsymlinks %s\n" "$(find . -type f | wc -l | tr -d " ")" "$(find . -type l | wc -l | tr -d " ")" && du -sk . | cut -f1 | sed "s/^/KiB /"' _ "$src"
  zipco "$src" "$z" checkout
  shcap 'the zip: entries (files and directories), size in bytes, sha256' bash -c 'printf "entries %s\nbytes %s\nsha256 %s\n" "$(unzip -Z1 "$1" | wc -l | tr -d " ")" "$(wc -c < "$1" | tr -d " ")" "$(shasum -a 256 "$1" | cut -d" " -f1)"' _ "$z"
  sub 'C8c WHK-Global — overwrite=false'
  CAP_MAX=900 imp "$n" false "$z"
  shcap "GET projects/find/$n (status, parent, enabled, inheritable)" bash "$CAPTURE25" findsel "$n"
  CAP_MAX=900 capdl "GET projects/export/$n" "$PRJ/export/$n" "$W/$n.1.zip"
  shcap 'zip against export after overwrite=false (member counts, hashes differing raw and after normalisation, first 40 zip-only and export-only keys)' bash "$CAPTURE25" cmpsum "$z" "$W/$n.1.zip"
  sub 'C8c WHK-Global — overwrite=true'
  CAP_MAX=900 imp "$n" true "$z"
  CAP_MAX=900 capdl "GET projects/export/$n (after overwrite=true)" "$PRJ/export/$n" "$W/$n.2.zip"
  shcap 'zip against export after overwrite=true' bash "$CAPTURE25" cmpsum "$z" "$W/$n.2.zip"
  c8del "$n"
  shcap 'git status --porcelain of the WHK-Global checkout (after)' git -C "$WHK_SRC" status --porcelain
  after=$LAST_OUT
  if [ "$before" = "$after" ]; then
    prose 'Script reading: the WHK-Global `git status --porcelain` output is identical before and after, so the source checkout is untouched.'
  else
    prose 'Script reading: the WHK-Global `git status --porcelain` output DIFFERS before and after.'
  fi
  plist_after
}

# c8d: one zip per stray-member variant, each imported with overwrite=false
# into CAP25-C8D-<variant>, exported, and summarised as one table row.
c8d() {
  local v n z e t d rows=() ok nch bkeys has res row title='C8d summary table'
  local variants=("$@")
  mkdir -p "$W"
  if [ ${#variants[@]} -gt 0 ]; then
    sub 'C8d extra variants'
    title='C8d summary table (extra variants)'
  else
    variants=(unlisted dotfile dotlisted emptydir nodescriptor pysidecar pycache listedmissing)
  fi
  [ $# -gt 0 ] || sect C8d 'C8d: what the gateway does with a file present but not listed in its folder'"'"'s `resource.json`, a dotfile, an empty directory, a folder with no `resource.json`, a `*.py` sidecar, a `__pycache__` directory, and a listed file the zip lacks. Each variant starts from the synthetic checkout (checkout layout), is imported with overwrite=false as CAP25-C8D-<variant>, and is re-exported. The summary table at the end has one row per variant.'
  for v in "${variants[@]}"; do
    n=CAP25-C8D-$v; d=$W/c8d-$v; z=$W/c8d-$v.zip; e=$W/$n.zip
    synth_tree "$d"
    t=$d/ignition/script-python/cap25
    case "$v" in
      unlisted) printf 'notes\n' > "$t/notes.txt"; res=ignition/script-python/cap25/notes.txt ;;
      dotfile) printf '' > "$t/.gitkeep"; res=ignition/script-python/cap25/.gitkeep ;;
      dotlisted)
        printf '' > "$t/.gitkeep"
        printf '{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["code.py",".gitkeep"],"attributes":{}}\n' > "$t/resource.json"
        res=ignition/script-python/cap25/.gitkeep ;;
      emptydir) mkdir -p "$d/ignition/script-python/empty"; res=ignition/script-python/empty ;;
      nodescriptor) mkdir -p "$d/ignition/script-python/loose"; printf 'def g():\n    return 3\n' > "$d/ignition/script-python/loose/code.py"; res=ignition/script-python/loose/code.py ;;
      nesteddir) mkdir -p "$t/sub"; printf 'x\n' > "$t/sub/x.txt"; res=ignition/script-python/cap25/sub/x.txt ;;
      pysidecar) printf 'def h():\n    return 4\n' > "$t/helper.py"; res=ignition/script-python/cap25/helper.py ;;
      pycache) mkdir -p "$t/__pycache__"; printf '\000\001\002cap25' > "$t/__pycache__/code.cpython-311.pyc"; res=ignition/script-python/cap25/__pycache__ ;;
      listedmissing)
        printf '{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["code.py","extra.py"],"attributes":{}}\n' > "$t/resource.json"
        res=ignition/script-python/cap25/extra.py ;;
    esac
    zipco "$d" "$z" checkout
    sub "variant $v"
    shcap "variant $v: members of the uploaded zip" bash "$CAPTURE25" members "$z"
    imp "$n" false "$z" "POST import $n overwrite=false"
    ok=$(jq -r 'if .success == null then "-" else (.success|tostring) end' <<<"$LAST_BODY" 2>/dev/null || echo '-')
    nch=$(jq -r 'if .changes == null then "-" else (.changes|length|tostring) end' <<<"$LAST_BODY" 2>/dev/null || echo '-')
    bkeys=$(jq -r 'keys | join(",")' <<<"$LAST_BODY" 2>/dev/null || echo 'not-json')
    row="$v | $LAST_STATUS | success=$ok | changes=$nch | body keys: $bkeys"
    if [ "$LAST_STATUS" = 200 ] && [ "$ok" = true ]; then
      capdl "GET projects/export/$n" "$PRJ/export/$n" "$e"
      shcap "variant $v: table, uploaded zip against export" bash "$CAPTURE25" cmp "$z" "$e"
      shcap "variant $v: resource.json files array of cap25 in the export" bash "$CAPTURE25" files "$e" ignition/script-python/cap25/resource.json
      has=$(zhas "$e" "$res")
      row="$row | in zip: $(zhas "$z" "$res") | in export: $has | cap25/resource.json files after export: $(bash "$CAPTURE25" files "$e" ignition/script-python/cap25/resource.json)"
    else
      row="$row | in zip: $(zhas "$z" "$res") | in export: n/a (not imported) | -"
    fi
    rows+=("$row")
    c8del "$n"
  done
  sub "$title"
  prose "$(printf '| variant | import status | success | changes | body keys | entries named like the probed member in zip | entries in export | cap25/resource.json files after export |\n| --- | --- | --- | --- | --- | --- | --- | --- |\n'; for row in "${rows[@]}"; do printf '| %s |\n' "${row// | / | }"; done)"
  plist_after
}


# rjdiff ZIP EXPORT: for each resource.json present in both, after the
# normalisation (drop attributes.lastModification and lastModificationSignature),
# which top-level and attributes field NAMES still differ, how many members
# differ per field name, and whether a `files` difference is order only. Field
# names and counts only, never values.
rjdiff() {
  python3 -I - "${1:?ZIP}" "${2:?EXPORT}" <<'PY'
import collections
import json
import sys
import zipfile


def key(name):
    parts = name.split("/")
    if len(parts) >= 3 and parts[1] == "resources":
        del parts[1]
    return "/".join(parts)


def load(path):
    z = zipfile.ZipFile(path)
    return {key(i.filename): z.read(i) for i in z.infolist() if not i.filename.endswith("/")}


def norm(raw):
    d = json.loads(raw)
    if isinstance(d.get("attributes"), dict):
        d["attributes"].pop("lastModification", None)
        d["attributes"].pop("lastModificationSignature", None)
    return d


a, b = load(sys.argv[1]), load(sys.argv[2])
cnt = collections.Counter()
total = same = 0
for k in sorted(set(a) & set(b)):
    if not k.endswith("/resource.json"):
        continue
    total += 1
    x, y = norm(a[k]), norm(b[k])
    if x == y:
        same += 1
        continue
    for f in sorted(set(x) | set(y)):
        if x.get(f, "<absent>") == y.get(f, "<absent>"):
            continue
        if f == "attributes" and isinstance(x.get(f), dict) and isinstance(y.get(f), dict):
            for g in sorted(set(x[f]) | set(y[f])):
                if x[f].get(g, "<absent>") != y[f].get(g, "<absent>"):
                    cnt["attributes." + g] += 1
        elif f == "files" and isinstance(x.get(f), list) and isinstance(y.get(f), list):
            cnt["files (same set, order differs)" if sorted(x[f]) == sorted(y[f]) else "files (set differs)"] += 1
        else:
            cnt[f] += 1
print("resource.json members in both: %d, equal after normalisation: %d, differing: %d" % (total, same, total - same))
for f, n in sorted(cnt.items()):
    print("  differing field %s: %d member(s)" % (f, n))
PY
}

# c8c_whk_rj: resource.json difference analysis for the WHK-Global exports
# already on disk from c8c-whk (no gateway request).
c8c_whk_rj() {
  sub 'C8c WHK-Global — what still differs in resource.json after normalisation'
  shcap 'field names that differ, export after overwrite=false' bash "$CAPTURE25" rjdiff "$W/whk.zip" "$W/CAP25-WHK.1.zip"
  shcap 'field names that differ, export after overwrite=true' bash "$CAPTURE25" rjdiff "$W/whk.zip" "$W/CAP25-WHK.2.zip"
}


# zinfo ZIP: entry order and compression method of every entry (names and the
# method only, no sizes, no content).
zinfo() {
  python3 -I - "${1:?ZIP}" <<'PY'
import sys
import zipfile

names = {0: "stored", 8: "deflated"}
z = zipfile.ZipFile(sys.argv[1])
print("index\tmethod\tname")
for n, i in enumerate(z.infolist()):
    print("%d\t%s\t%s" % (n, names.get(i.compress_type, str(i.compress_type)), i.filename))
PY
}

# pj_zip OUT JSON: a zip holding only project.json with the given text (a
# project with no resources).
pj_zip() {
  local out=${1:?OUT} d
  d=$(mktemp -d "$W/pj.XXXXXX")
  printf '%s\n' "${2:?JSON}" > "$d/project.json"
  zipco "$d" "$out" checkout
  /bin/rm -rf "$d"
}

# ------------------------------------------------------------------- C8a

# a_counts ZIP: how many entries the export holds for folder B, the view folder
# and code.py of folder A, and in all.
a_counts() {
  printf 'folder B (script-python/cap25b/) entries %s, view folder entries %s, folder A code.py entries %s, folder A resource.json entries %s, project.json entries %s, entries in all %s' \
    "$(zhas "$1" ignition/script-python/cap25b/)" \
    "$(zhas "$1" com.inductiveautomation.perspective/views/Cap25/)" \
    "$(zhas "$1" ignition/script-python/cap25/code.py)" \
    "$(zhas "$1" ignition/script-python/cap25/resource.json)" \
    "$(zhas "$1" project.json)" \
    "$(unzip -Z1 "$1" | wc -l | tr -d ' ')"
}

c8a() {
  local n=CAP25-C8A full=$W/a-full c base=$W/c8a-base.zip pre post case_
  mkdir -p "$W"
  sect C8a 'C8a: does `import?overwrite=true` delete resources the zip omits? One project (`CAP25-C8A`) holds script folders A (`cap25`) and B (`cap25b`) and one Perspective view. Before each case the project is restored from the full zip with overwrite=true and exported; the case zip is then imported with overwrite=true and exported again. The reading line under each case counts the export entries for the omitted members before and after. Cases: (i) without folder B, (ii) without the view folder, (iii) folder A `resource.json` still lists `code.py` but the `code.py` member is absent, (iv) a zip holding only `project.json`.'
  synth_tree "$full"
  mkdir -p "$full/ignition/script-python/cap25b"
  printf 'def g():\n    return 2\n' > "$full/ignition/script-python/cap25b/code.py"
  printf '{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["code.py"],"attributes":{}}\n' > "$full/ignition/script-python/cap25b/resource.json"
  zipco "$full" "$W/a-full.zip" checkout
  /bin/rm -rf "$W/a-noB" "$W/a-noview" "$W/a-nocode"
  cp -R "$full" "$W/a-noB";    /bin/rm -rf "$W/a-noB/ignition/script-python/cap25b"
  cp -R "$full" "$W/a-noview"; /bin/rm -rf "$W/a-noview/com.inductiveautomation.perspective"
  cp -R "$full" "$W/a-nocode"; /bin/rm -f "$W/a-nocode/ignition/script-python/cap25/code.py"
  zipco "$W/a-noB" "$W/a-noB.zip" checkout
  zipco "$W/a-noview" "$W/a-noview.zip" checkout
  zipco "$W/a-nocode" "$W/a-nocode.zip" checkout
  pj_zip "$W/a-projonly.zip" '{"title":"CAP25 C8A","enabled":true,"inheritable":false}'
  sub 'baseline'
  imp "$n" false "$W/a-full.zip" "POST import $n overwrite=false (full zip)"
  for case_ in noB noview nocode projonly; do
    sub "case $case_"
    imp "$n" true "$W/a-full.zip" "POST import $n overwrite=true (restore the full zip)"
    capdl "GET projects/export/$n (before $case_)" "$PRJ/export/$n" "$W/c8a-$case_.pre.zip"
    shcap "members of the export before case $case_" bash "$CAPTURE25" members "$W/c8a-$case_.pre.zip"
    shcap "members of the case zip $case_" bash "$CAPTURE25" members "$W/a-$case_.zip"
    imp "$n" true "$W/a-$case_.zip" "POST import $n overwrite=true (case $case_)"
    capdl "GET projects/export/$n (after $case_)" "$PRJ/export/$n" "$W/c8a-$case_.post.zip"
    shcap "members of the export after case $case_" bash "$CAPTURE25" members "$W/c8a-$case_.post.zip"
    pre=$(a_counts "$W/c8a-$case_.pre.zip"); post=$(a_counts "$W/c8a-$case_.post.zip")
    prose "Script reading ($case_): before: $pre. After: $post."
  done
  c8del "$n"
  plist_after
}

# ------------------------------------------------------------------- C8b

c8b() {
  local n=CAP25-C8B e1=$W/c8b-E1.zip e2=$W/c8b-E2.zip
  mkdir -p "$W"
  sect C8b 'C8b: round trip of a gateway-produced export. Part 1, the synthetic project: import the synthetic checkout as `CAP25-C8B`, export it (E1), import E1 back with overwrite=true, export again (E2). Part 2, the WHK-Global allowlist export from C8c (the export after overwrite=false, `CAP25-WHK.1.zip`) is imported into `CAP25-C8B-WHK`, exported (E1b), imported again with overwrite=true and exported (E2b). Equality is by sha256 per member, raw and after `resource.json` normalisation (drop `attributes.lastModification` and `lastModificationSignature`, sort keys). Only member names, field names and counts are recorded.'
  sub 'Part 1: synthetic project'
  synth_tree "$W/b-synth"; zipco "$W/b-synth" "$W/b-synth.zip" checkout
  imp "$n" false "$W/b-synth.zip" "POST import $n overwrite=false (synthetic checkout)"
  capdl "GET projects/export/$n (E1)" "$PRJ/export/$n" "$e1"
  imp "$n" true "$e1" "POST import $n overwrite=true (E1 back, gateway-produced zip)"
  capdl "GET projects/export/$n (E2)" "$PRJ/export/$n" "$e2"
  shcap 'members of E1' bash "$CAPTURE25" members "$e1"
  shcap 'members of E2' bash "$CAPTURE25" members "$e2"
  shcap 'table: E1 against E2 (per member sha256, raw and after normalisation)' bash "$CAPTURE25" cmp "$e1" "$e2"
  shcap 'entry order and compression method of E1' bash "$CAPTURE25" zinfo "$e1"
  shcap 'entry order and compression method of E2' bash "$CAPTURE25" zinfo "$e2"
  c8del "$n"
  if [ -f "$W/CAP25-WHK.1.zip" ]; then
    sub 'Part 2: WHK-Global allowlist export'
    n=CAP25-C8B-WHK
    CAP_MAX=900 imp "$n" false "$W/CAP25-WHK.1.zip" "POST import $n overwrite=false (the C8c export, E1)"
    CAP_MAX=900 capdl "GET projects/export/$n (E1b)" "$PRJ/export/$n" "$W/c8b-E1b.zip"
    CAP_MAX=900 imp "$n" true "$W/CAP25-WHK.1.zip" "POST import $n overwrite=true (E1 again)"
    CAP_MAX=900 capdl "GET projects/export/$n (E2b)" "$PRJ/export/$n" "$W/c8b-E2b.zip"
    shcap 'E1 against E1b (import overwrite=false, export)' bash "$CAPTURE25" cmpsum "$W/CAP25-WHK.1.zip" "$W/c8b-E1b.zip"
    shcap 'E1 against E2b (import overwrite=true, export)' bash "$CAPTURE25" cmpsum "$W/CAP25-WHK.1.zip" "$W/c8b-E2b.zip"
    shcap 'E1b against E2b' bash "$CAPTURE25" cmpsum "$W/c8b-E1b.zip" "$W/c8b-E2b.zip"
    shcap 'resource.json fields still differing, E1 against E2b' bash "$CAPTURE25" rjdiff "$W/CAP25-WHK.1.zip" "$W/c8b-E2b.zip"
    shcap 'entry order and compression method: first 8 entries of E1' bash -c '"$0" zinfo "$1" | sed -n 1,9p' "$CAPTURE25" "$W/CAP25-WHK.1.zip"
    shcap 'entry order and compression method: first 8 entries of E2b' bash -c '"$0" zinfo "$1" | sed -n 1,9p' "$CAPTURE25" "$W/c8b-E2b.zip"
    c8del "$n"
  else
    prose 'Part 2 skipped: the C8c WHK-Global export is not on disk.'
  fi
  plist_after
}

# ------------------------------------------------------------------- C8e

c8e() {
  local c
  mkdir -p "$W"
  sect C8e 'C8e: parent edge cases and project-list membership. `CAP25-PARENT-NI` (`inheritable: false`) and `CAP25-PARENT-I` (`inheritable: true`) hold no resources. Child zips are imported with overwrite=false: `CAP25-CHILD-MISSING` (parent `CAP25-NOPE`, absent), `CAP25-CHILD-SELF` (parent is itself), `CAP25-CHILD-NI` (parent `CAP25-PARENT-NI`) and, as a control, `CAP25-CHILD-OK` (parent `CAP25-PARENT-I`). `CAP25-DISABLED` is `enabled: false`.'
  sub 'parents'
  pj_zip "$W/e-ni.zip" '{"title":"CAP25 parent not inheritable","enabled":true,"inheritable":false}'
  pj_zip "$W/e-i.zip" '{"title":"CAP25 parent inheritable","enabled":true,"inheritable":true}'
  pj_zip "$W/e-dis.zip" '{"title":"CAP25 disabled","enabled":false,"inheritable":false}'
  imp CAP25-PARENT-NI false "$W/e-ni.zip"
  imp CAP25-PARENT-I false "$W/e-i.zip"
  imp CAP25-DISABLED false "$W/e-dis.zip"
  sub 'children'
  pj_zip "$W/e-c-missing.zip" '{"title":"CAP25 child of a missing project","enabled":true,"inheritable":false,"parent":"CAP25-NOPE"}'
  pj_zip "$W/e-c-self.zip" '{"title":"CAP25 child of itself","enabled":true,"inheritable":false,"parent":"CAP25-CHILD-SELF"}'
  pj_zip "$W/e-c-ni.zip" '{"title":"CAP25 child of a non-inheritable project","enabled":true,"inheritable":false,"parent":"CAP25-PARENT-NI"}'
  pj_zip "$W/e-c-ok.zip" '{"title":"CAP25 child of an inheritable project","enabled":true,"inheritable":false,"parent":"CAP25-PARENT-I"}'
  imp CAP25-CHILD-MISSING false "$W/e-c-missing.zip"
  imp CAP25-CHILD-SELF false "$W/e-c-self.zip"
  imp CAP25-CHILD-NI false "$W/e-c-ni.zip"
  imp CAP25-CHILD-OK false "$W/e-c-ok.zip"
  sub 'find and list membership'
  for c in CAP25-PARENT-NI CAP25-PARENT-I CAP25-DISABLED CAP25-CHILD-MISSING CAP25-CHILD-SELF CAP25-CHILD-NI CAP25-CHILD-OK; do
    shcap "GET projects/find/$c (status, parent, enabled, inheritable)" bash "$CAPTURE25" findsel "$c"
  done
  cap 'GET projects/list (limit=-1)' GET "$PRJ/list?limit=-1"
  cap 'GET projects/names' GET "$PRJ/names"
  cap 'GET projects/parents' GET "$PRJ/parents"
  sub 'cleanup'
  for c in CAP25-CHILD-OK CAP25-CHILD-NI CAP25-CHILD-SELF CAP25-CHILD-MISSING CAP25-DISABLED CAP25-PARENT-I CAP25-PARENT-NI; do c8del "$c"; done
  plist_after
}

# ------------------------------------------------------------------- C9

# c9digest ZIP: one line, member count and the sha256 of the sorted
# "name sha256" lines of every file entry (directory entries ignored). Equal
# digests mean equal member lists and equal member bytes.
c9digest() {
  python3 -I - "${1:?ZIP}" <<'PY'
import hashlib
import sys
import zipfile

z = zipfile.ZipFile(sys.argv[1])
lines = sorted("%s %s" % (i.filename, hashlib.sha256(z.read(i)).hexdigest()) for i in z.infolist() if not i.filename.endswith("/"))
print("members=%d digest=%s" % (len(lines), hashlib.sha256("\n".join(lines).encode()).hexdigest()[:16]))
PY
}

# c9child IN OUT PARENT: take the zip of a project born from `ign webdev
# deploy` (route bundle, no parent), set `parent` in project.json and add one
# view of the child's own (checkout layout). Written to OUT; nothing printed.
c9child() {
  python3 -I - "${1:?IN}" "${2:?OUT}" "${3:?PARENT}" <<'PY'
import json
import sys
import zipfile

src, out, parent = sys.argv[1:4]
zin = zipfile.ZipFile(src)
view = b'{"custom":{},"params":{},"props":{},"root":{"type":"ia.container.flex","children":[]}}\n'
res = b'{"scope":"G","version":1,"restricted":false,"overridable":true,"files":["view.json"],"attributes":{}}\n'
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as zo:
    for i in sorted(zin.infolist(), key=lambda i: i.filename):
        data = zin.read(i)
        if i.filename == "project.json":
            d = json.loads(data)
            d["parent"] = parent
            d["enabled"] = True
            data = (json.dumps(d, indent=2) + "\n").encode()
        zo.writestr(i.filename, data)
    zo.writestr("com.inductiveautomation.perspective/views/CapChild/view.json", view)
    zo.writestr("com.inductiveautomation.perspective/views/CapChild/resource.json", res)
PY
}

# c9line NAME T0: one poll line: elapsed seconds since T0 (epoch), the child's
# find enabled and parent, its export member count and digest, and the result
# of cap25lib.answer() called in the child's scope (cut to 60 characters).
c9line() {
  local n=${1:?NAME} t0=${2:?T0} now end f call e=$TMPD/c9poll.zip code=0
  now=$(date +%s)
  /bin/rm -f "$e"
  f=$(findsel "$n" | tr ' ' ',' | cut -c1-120)
  curl -sS -g -m 60 -o "$e" -w '' -H "X-Ignition-API-Token: ${IGN_TOKEN_CAP_A:?}" "$BASE$PRJ/export/$n" || code=$?
  call=$("$IGN" script run --project "$n" --code 'import cap25lib; print cap25lib.answer()' --json 2>&1 | jq -r '.data.stdout // ("ERR " + (.error.code // "?") + " " + ((.error.message // "") | .[0:40]))' 2>/dev/null | tr '\n' ' ' | cut -c1-60 || true)
  end=$(date +%s)
  printf 't=%ss took=%ss find=%s export=%s call=%s\n' "$((now - t0))" "$((end - now))" "$f" "$( [ -s "$e" ] && c9digest "$e" 2>&1 || echo "export-failed curl=$code")" "${call:-<none>}"
}

# c9poll NAME T0: one line every 5 s for 120 s from T0 (25 lines, t=0..120).
c9poll() {
  local n=${1:?NAME} t0=${2:?T0} i now
  for i in $(seq 0 24); do
    while now=$(date +%s); [ "$now" -lt "$((t0 + 5 * i))" ]; do sleep 0.5; done
    c9line "$n" "$t0"
  done
}

c9() {
  local P=$W/c9-p C=$W/c9-c cfg=$TMPD/ign-config.toml start t0 poll first_new
  mkdir -p "$W"
  sect C9 'C9: what a parent overwrite does to a child. The S5 residual (session impact on a child after a parent overwrite) stays OPEN and no Phase 25 criterion claims it; this capture only sizes the D-06 settle wait and states whether a child'"'"'s export changes when its parent is overwritten. `CAP25-P` is `inheritable: true` and holds one `script-python` library `cap25lib` (`answer()` returns 1, then 2) and one view. `CAP25-C` (`parent: CAP25-P`) holds one view of its own and the CLI scriptExec route bundle, so a call can run in its project scope (`ign script run --project CAP25-C`). Each poll line is: seconds since the overwrite call, the child'"'"'s `find` (status, parent, enabled), its export member count and digest (sha256 of the sorted `name sha256` lines, first 16 hex), and the result of `import cap25lib; print cap25lib.answer()` in the child'"'"'s scope (the scriptExec namespace holds no project libraries until they are imported, so a bare `cap25lib.answer()` answers NameError). `took` is the wall time of the poll itself; a poll that runs long makes the next ones fire back to back.'
  export IGNITION_CLI_CONFIG=$cfg IGN_C9_TOKEN=${IGN_TOKEN_CAP_A:?}
  sub 'setup'
  shcap 'a throwaway ign config with a profile for the rig (token read from an env var)' \
    "$IGN" profile add ign-deploy-rig "$BASE" --token-env IGN_C9_TOKEN --active --json
  /bin/rm -rf "$P" "$C"
  mkdir -p "$P/ignition/script-python/cap25lib" "$P/com.inductiveautomation.perspective/views/CapParent"
  printf '{"title":"CAP25 C9 parent","enabled":true,"inheritable":true}\n' > "$P/project.json"
  printf 'def answer():\n    return 1\n' > "$P/ignition/script-python/cap25lib/code.py"
  printf '{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["code.py"],"attributes":{}}\n' > "$P/ignition/script-python/cap25lib/resource.json"
  printf '{"custom":{},"params":{},"props":{},"root":{"type":"ia.container.flex","children":[]}}\n' > "$P/com.inductiveautomation.perspective/views/CapParent/view.json"
  printf '{"scope":"G","version":1,"restricted":false,"overridable":true,"files":["view.json"],"attributes":{}}\n' > "$P/com.inductiveautomation.perspective/views/CapParent/resource.json"
  zipco "$P" "$W/c9-p1.zip" checkout
  sed -i.bak 's/return 1/return 2/' "$P/ignition/script-python/cap25lib/code.py" && /bin/rm -f "$P/ignition/script-python/cap25lib/code.py.bak"
  zipco "$P" "$W/c9-p2.zip" checkout
  shcap 'members of the parent zip, answer() returns 1' bash "$CAPTURE25" members "$W/c9-p1.zip"
  imp CAP25-P false "$W/c9-p1.zip"
  prose 'Script note: the child is first born from `ign webdev deploy --project CAP25-C --with-script-exec` (the only way the CLI installs scriptExec), exported, given `parent: CAP25-P` and a view of its own by `c9child`, deleted, and imported again with overwrite=false, so the imported child is the one under test.'
  shcap 'ign webdev deploy --project CAP25-C --with-script-exec (route bundle; child born without a parent)' \
    "$IGN" webdev deploy --project CAP25-C --with-script-exec --yes --json
  capdl 'GET projects/export/CAP25-C (route bundle only)' "$PRJ/export/CAP25-C" "$W/c9-born.zip"
  c8del CAP25-C
  c9child "$W/c9-born.zip" "$W/c9-c.zip" CAP25-P
  shcap 'members of the child zip (route bundle, project.json with parent, one view)' bash "$CAPTURE25" members "$W/c9-c.zip"
  imp CAP25-C false "$W/c9-c.zip"
  sub 'E0: the child before the overwrite'
  shcap 'GET projects/find/CAP25-C (status, parent, enabled, inheritable)' bash "$CAPTURE25" findsel CAP25-C
  capdl 'GET projects/export/CAP25-C (E0)' "$PRJ/export/CAP25-C" "$W/c9-e0.zip"
  shcap 'members of the child export E0 (names, sizes, sha256)' bash "$CAPTURE25" members "$W/c9-e0.zip"
  shcap 'ign webdev status --project CAP25-C (is scriptExec answering after the import?)' \
    bash -c 'for i in 1 2 3 4 5 6; do out=$("$0" webdev status --project CAP25-C --json 2>&1); if echo "$out" | jq -e ".data.ok // false" >/dev/null 2>&1; then echo "$out" | jq -c "[.data.routes[] | {route, status}]"; exit 0; fi; sleep 5; done; echo "$out" | jq -c "(.data.routes // .error) | tostring | .[0:300]"' "$IGN"
  shcap 'E0: import cap25lib; print cap25lib.answer() in the child scope, before the overwrite' bash -c 'c=$("$0" script run --project CAP25-C --code "import cap25lib; print cap25lib.answer()" --json 2>&1); echo "$c" | jq -r ".data.stdout // .error.code" | head -3' "$IGN"
  shcap 'E0 digest of the child export' bash "$CAPTURE25" c9digest "$W/c9-e0.zip"
  sub 'overwrite of the parent and 120 s of polling'
  start=$(utc)
  t0=$(date +%s)
  imp CAP25-P true "$W/c9-p2.zip" 'POST import CAP25-P overwrite=true (answer() returns 2) — T0'
  shcap 'poll CAP25-C every 5 s for 120 s from T0 (25 lines)' bash "$CAPTURE25" c9poll CAP25-C "$t0"
  poll=$LAST_OUT
  shcap "gateway log lines naming CAP25- since $start, cut to 300 characters" \
    bash -c 'docker compose -p ign-deploy-rig logs --no-log-prefix --since "$0" 2>&1 | grep "CAP25-" | cut -c1-300' "$start"
  sub 'readings'
  c9read "$poll"
  sub 'cleanup'
  c8del CAP25-C
  c8del CAP25-P
  plist_after
}

# c9read POLL: the script's own reading of the poll lines (counts and times,
# derived by awk from the recorded lines).
c9read() {
  local r
  r=$(awk -v e0="$(c9digest "$W/c9-e0.zip" | sed 's/.*digest=//')" '
    { t=$1; sub(/^t=/,"",t); sub(/s$/,"",t);
      d=$0; sub(/.*digest=/,"",d); sub(/ .*/,"",d);
      c=$0; sub(/.*call=/,"",c); gsub(/ +$/,"",c);
      k=$0; sub(/^t=[0-9]+s took=[0-9]+s /,"",k);
      n++; ts[n]=t; key[n]=k; dg[n]=d; ca[n]=c;
      if (d != e0 && !chg) chg=t;
      if (c == "2" && !seen2) seen2=t;
      if (n>1 && k != key[n-1]) last=t;
    }
    END {
      printf "polls=%d\n", n;
      printf "child_export_digest_changed=%s\n", (chg=="" ? "no" : "yes (first at t=" chg "s)");
      printf "child_reads_parent_new_value=%s\n", (seen2=="" ? "never within 120 s" : "yes (first at t=" seen2 "s)");
      if (n>1 && key[n] != key[n-1]) s="TIMEOUT (still changing at the last poll)"; else s=(last=="" ? "0 (no poll differs from the one before)" : last " s");
      printf "settle_seconds=%s\n", s;
    }' <<<"$(grep '^t=' <<<"$1")")
  shcap 'derived readings (awk over the poll lines above)' printf '%s\n' "$r"
  prose 'Script reading: `child_export_digest_changed` compares each poll digest with the E0 digest; `child_reads_parent_new_value` is the first poll whose call result is `2`; `settle_seconds` is the elapsed time of the last poll whose (find, export, call) fields differ from the poll before it.'
}

# ------------------------------------------------------------------ C10

# C10 runs on a disposable gateway of its own: compose project ign-c10-rig,
# loopback port 18291, the signed git module installed by `ign rig up
# --with-module`, and a git server container holding one bare repository.
# Nothing here points at the shared rig, a WHK gateway or a production host.
C10=$STATE/c10
C10P=ign-c10-rig
C10_VER=${C10_MODULE_VERSION:-2.3.4}
C10_CI=${WHK_GLOBAL_CI:-$WHK_SRC/.github/ci}
C10_MOD_SRC=${GIT_MODULE_SRC:-$HOME/whiskeyhouse/ignition-git-module}
C10_PROJ=/usr/local/bin/ignition/data/projects
C10_CFG=$C10/ign-config.toml

c10cid() { docker ps -q --filter "label=com.docker.compose.project=$C10P" --filter "label=com.docker.compose.service=gw"; }

c10dc() {
  local f=(-f "$C10/compose.yml")
  [ ! -f "$C10/compose.ign-modules.yml" ] || f+=(-f "$C10/compose.ign-modules.yml")
  docker compose -p "$C10P" "${f[@]}" "$@"
}

# c10yaml MODE: write $C10/git.yaml IN PLACE (a bind-mounted file keeps its
# inode) from the WHK-Global CI template through the CI's own renderer.
#   template    repo_branch main, production_mode false (the CI shape)
#   production  repo_branch production, production_mode true,
#               production_branch production, production_tagPattern "v*"
c10yaml() {
  local mode=${1:?MODE} tmp branch=main
  [ "$mode" != production ] || branch=production
  tmp=$(mktemp "$TMPD/gityaml.XXXXXX")
  GIT_USER_NAME=rig GIT_USER_PASSWORD=not-checked \
    bash "$C10_CI/render-git-yaml.sh" http://git-server/git/cap25-git.git "$branch" CAP25-GIT "" "$tmp" > /dev/null
  if [ "$mode" = production ]; then
    sed -i.bak 's/^  production_mode: false$/  production_mode: true/' "$tmp" && /bin/rm -f "$tmp.bak"
    printf '  production_branch: production\n  production_tagPattern: "v*"\n' >> "$tmp"
  fi
  cat "$tmp" > "$C10/git.yaml"
}

# c10cfg: the effective git.yaml keys and values, the credential key dropped.
c10cfg() {
  grep -v '^  user_[p]assword:' "$C10/git.yaml"
}

# c10seed: the synthetic checkout CAP25-GIT in a bare repository with
# branches main and production and a tag v1.0.0 on the same commit.
c10seed() {
  local src=$C10/seed-src work=$C10/seed-work bare=$C10/repo/cap25-git.git
  /bin/rm -rf "$src" "$work" "$bare"
  mkdir -p "$src/ignition/script-python/cap25git" "$src/com.inductiveautomation.perspective/views/CapGit" "$C10/repo"
  printf '{"title":"CAP25 C10 git project","enabled":true,"inheritable":false}\n' > "$src/project.json"
  printf 'def v():\n    return 1\n' > "$src/ignition/script-python/cap25git/code.py"
  printf '{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["code.py"],"attributes":{}}\n' > "$src/ignition/script-python/cap25git/resource.json"
  printf '{"custom":{},"params":{},"props":{},"root":{"type":"ia.container.flex","children":[]}}\n' > "$src/com.inductiveautomation.perspective/views/CapGit/view.json"
  printf '{"scope":"G","version":1,"restricted":false,"overridable":true,"files":["view.json"],"attributes":{}}\n' > "$src/com.inductiveautomation.perspective/views/CapGit/resource.json"
  cp -R "$src" "$work"
  git -C "$work" init -q -b main
  git -C "$work" -c user.name=rig -c user.email=rig@example.invalid -c commit.gpgsign=false add -A
  git -C "$work" -c user.name=rig -c user.email=rig@example.invalid -c commit.gpgsign=false commit -q -m 'seed CAP25-GIT'
  git -C "$work" branch production
  git -C "$work" -c user.name=rig -c user.email=rig@example.invalid -c tag.gpgsign=false tag -a v1.0.0 -m v1.0.0
  git init -q --bare -b main "$bare"
  git -C "$bare" config http.receivepack true
  git -C "$work" push -q "$bare" main production v1.0.0
  /bin/rm -rf "$work"
}

# c10up: write the rig files, seed the repository, start the rig with the
# signed module, mint a token. Prints nothing secret.
c10up() {
  mkdir -p "$C10/git-server" "$C10/w"
  cat > "$C10/compose.yml" <<'YML'
name: ign-c10-rig
services:
  git-server:
    build: ./git-server
    hostname: git-server
    volumes:
      - ./repo:/srv/git
  gw:
    image: inductiveautomation/ignition:8.3.6
    platform: linux/amd64
    hostname: gw
    depends_on:
      - git-server
    ports:
      - "127.0.0.1:18291:8088"
    environment:
      ACCEPT_IGNITION_EULA: "Y"
      GATEWAY_ADMIN_USERNAME: admin
      GATEWAY_ADMIN_PASSWORD: ign-c10-rig
      GATEWAY_GIT_USER_SECRET: not-checked
      IGNITION_EDITION: standard
      IGNITION_UID: "2003"
      IGNITION_GID: "2003"
    command: ["-n", "ign-c10-rig"]
    volumes:
      - gw-data:/usr/local/bin/ignition/data
      - ./git.yaml:/usr/local/bin/ignition/data/git.yaml:ro
volumes:
  gw-data: {}
YML
  cat > "$C10/git-server/Dockerfile" <<'DF'
FROM httpd:2.4
RUN apt-get update && apt-get install -y --no-install-recommends git && rm -rf /var/lib/apt/lists/* \
    && git config --system safe.directory '*'
RUN sed -i -E 's|^[[:space:]]*#(LoadModule cgid?_module)|\1|' /usr/local/apache2/conf/httpd.conf \
    && grep -qE '^LoadModule cgid_module' /usr/local/apache2/conf/httpd.conf \
    && echo "Include conf/httpd-git.conf" >> /usr/local/apache2/conf/httpd.conf
COPY httpd-git.conf /usr/local/apache2/conf/httpd-git.conf
EXPOSE 80
CMD ["httpd-foreground"]
DF
  cp "$C10_MOD_SRC/docker/test-rig/git-server/httpd-git.conf" "$C10/git-server/httpd-git.conf"
  printf '[rigs.c10]\ncompose_file = "%s"\n' "$C10/compose.yml" > "$C10_CFG"
  c10seed
  : > "$C10/git.yaml"
  c10yaml template
  IGNITION_CLI_CONFIG=$C10_CFG "$IGN" rig --rig c10 up --with-module "git@$C10_VER" --timeout 900 --json > "$C10/rigup.json" 2>&1 || { echo "c10up: rig up failed; see $C10/rigup.json" >&2; jq -c '.error // .' "$C10/rigup.json" 2>/dev/null | cut -c1-400 >&2; exit 1; }
  IGN_RIG_PROJECT=$C10P IGN_DEPLOY_RIG_STATE=$C10 bash "$HERE/mint-token.sh" cap-a > /dev/null
  echo "c10up: rig is up; tokens in $C10/tokens.env"
}

# c10wait: wait for RUNNING, then for CAP25-GIT to answer find (or $1 s).
c10wait() {
  local limit=${1:-420} end now
  end=$(( $(date +%s) + limit ))
  while now=$(date +%s); [ "$now" -lt "$end" ]; do
    curl -s -m 5 "$BASE/StatusPing" | grep -q RUNNING && break
    sleep 5
  done
  while now=$(date +%s); [ "$now" -lt "$end" ]; do
    [ "$(curl -sS -m 10 -o /dev/null -w '%{http_code}' -H "X-Ignition-API-Token: ${IGN_TOKEN_CAP_A:?}" "$BASE$PRJ/find/CAP25-GIT" || true)" = 200 ] && return 0
    sleep 5
  done
  return 1
}

c10down() {
  if [ -f "$C10/compose.yml" ]; then
    c10dc down -v --remove-orphans
  fi
  /bin/rm -rf "$C10"
  echo "c10down: removed compose project $C10P and $C10"
}

# c10state: key=value lines for CAP25-GIT and CAP25-OTHER: export digest, the
# on-disk sha256 (first 16 hex) of the script file, the `.git` entry kind and
# HEAD ref. No file content.
c10state() {
  local cid n e dig disk kind head f
  cid=$(c10cid)
  for n in CAP25-GIT CAP25-OTHER; do
    e=$TMPD/c10state.zip
    /bin/rm -f "$e"
    curl -sS -g -m 60 -o "$e" -H "X-Ignition-API-Token: ${IGN_TOKEN_CAP_A:?}" "$BASE$PRJ/export/$n" || true
    if [ -s "$e" ] && unzip -tq "$e" > /dev/null 2>&1; then dig=$(c9digest "$e"); else dig="export-unavailable"; fi
    if [ "$n" = CAP25-GIT ]; then f=ignition/script-python/cap25git/code.py; else f=ignition/script-python/cap25other/code.py; fi
    disk=$(docker exec "$cid" sh -c "if [ -f $C10_PROJ/$n/$f ]; then sha256sum $C10_PROJ/$n/$f | cut -c1-16; else echo absent; fi" 2>&1 | tr -d '\r')
    kind=$(docker exec "$cid" sh -c "if [ -L $C10_PROJ/$n/.git ]; then echo symlink; elif [ -d $C10_PROJ/$n/.git ]; then echo dir; elif [ -f $C10_PROJ/$n/.git ]; then echo file; else echo absent; fi" 2>&1 | tr -d '\r')
    head=$(docker exec "$cid" sh -c "if [ -f $C10_PROJ/$n/.git/HEAD ]; then cat $C10_PROJ/$n/.git/HEAD; else echo -; fi" 2>&1 | tr -d '\r')
    printf '%s %s disk=%s dotgit=%s head=%s\n' "$n" "$dig" "$disk" "$kind" "$head"
  done
}

# c10log SINCE: gateway log lines about the projects, git, sync and
# commissioning since SINCE (UTC). Dropped, with their count printed: lines
# holding the credential key or a secret, the module's one-line-per-key
# `Successful addition of field` echo of git.yaml, and stack-trace lines. At
# most 60 lines are shown, each cut to 300 characters.
c10log() {
  local since=${1:?SINCE} all kept dropped echoed total
  all=$(docker logs --since "$since" "$(c10cid)" 2>&1 | grep -E 'CAP25-|[Ss]ync|[Cc]ommission|[Pp]ull|[Ss]tash|[Cc]lone|\.git|GitProject|git-gateway|[Pp]roduction' | grep -vE '^jvm 1 +\| [0-9/ :]+\| (\sat |com\.inductive|java\.)' || true)
  dropped=$(grep -ciE 'password|secret' <<<"$all" || true)
  echoed=$(grep -c 'Successful addition of field' <<<"$all" || true)
  kept=$(grep -viE 'password|secret|Successful addition of field' <<<"$all" | cut -c1-300 || true)
  total=$(grep -c . <<<"$kept" || true)
  [ -z "$kept" ] || head -60 <<<"$kept"
  printf '[%s line(s) shown of %s; %s `Successful addition of field` echo line(s) and %s line(s) holding the credential key or a secret dropped]\n' "$(( total > 60 ? 60 : total ))" "$total" "${echoed:-0}" "${dropped:-0}"
}

# c10field LINE-NAME FIELD STATE: pick one value out of a c10state block.
c10field() {
  grep "^$1 " <<<"$3" | tr ' ' '\n' | sed -n "s/^$2=//p" | head -1
}

# c10snap LABEL: one recorded state snapshot; sets SNAP to the output.
SNAP=""
c10snap() {
  shcap "state: $1" bash "$CAPTURE25" c10state
  SNAP=$LAST_OUT
}

# c10imp NAME OVERWRITE ZIP LABEL: import, retried up to 4 times 20 s apart
# while the answer is not 2xx (a gateway that has just restarted answers 503).
c10imp() {
  local k
  for k in 1 2 3 4; do
    imp "$1" "$2" "$3" "$4 (attempt $k)"
    case "$LAST_STATUS" in 2??) return 0 ;; esac
    sleep 20
  done
  return 1
}

# c10reclone MODE: put CAP25-GIT back into a module-managed state: delete it
# through the API (rig setup), remove any leftover directory inside the
# container, restart the gateway and let the module clone it again from the
# repository. Used so that each experiment starts from a project that has a
# `.git` directory.
c10reclone() {
  local mode=${1:?MODE} cid log_since
  cid=$(c10cid)
  c8del CAP25-GIT
  shcap 'rig setup: leftover directory of CAP25-GIT after the API delete (then removed)' docker exec "$cid" sh -c "ls -d $C10_PROJ/CAP25-GIT 2>&1; rm -rf $C10_PROJ/CAP25-GIT; ls -d $C10_PROJ/CAP25-GIT 2>&1 | head -1"
  log_since=$(utc)
  shcap "restart the gateway so the module clones CAP25-GIT again ($mode) (docker restart)" docker restart "$cid"
  c10wait 900 || prose 'Script reading: the gateway did not report CAP25-GIT within 900 s of the restart.'
  sleep 60
  shcap "gateway log since the restart ($mode)" bash "$CAPTURE25" c10log "$log_since"
}

# c10variant MODE: the observations of one git.yaml variant on the running
# C10 gateway (see c10 for the setup).
c10variant() {
  local mode=${1:?MODE} t0 log_since cid d0g d0o d0b s_import s_restart s_scan s_nodot firstg lastg s o1 odot g1 g2 g3 firstdot
  local rev_poll=no rev_restart=no rev_scan=no recreated=no touched=no kept_poll=no removes_git=no digest_moved=no dpoll
  cid=$(c10cid)
  sub "variant $mode"
  prose "Variant \`$mode\`: effective \`git.yaml\` keys (the credential key omitted):"
  shcap "git.yaml keys, variant $mode" bash "$CAPTURE25" c10cfg
  if [ "$mode" = production ]; then
    prose 'Script note: the production `git.yaml` is read at gateway start, and a project the module has not cloned (or from which `.git` is gone) is skipped by the sync. So the production variant starts from a fresh clone: CAP25-GIT is deleted through the API, the leftover directory is removed, and the gateway is restarted with the production `git.yaml`; the module clones the `production` branch.'
    c10reclone "$mode"
  fi
  # CAP25-OTHER: a project the module does not manage (no .git), born by API.
  /bin/rm -rf "$C10/w/other"
  mkdir -p "$C10/w/other/ignition/script-python/cap25other"
  printf '{"title":"CAP25 C10 unmanaged project","enabled":true,"inheritable":false}\n' > "$C10/w/other/project.json"
  printf 'def o():\n    return 1\n' > "$C10/w/other/ignition/script-python/cap25other/code.py"
  printf '{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["code.py"],"attributes":{}}\n' > "$C10/w/other/ignition/script-python/cap25other/resource.json"
  zipco "$C10/w/other" "$C10/w/other.zip" checkout
  if [ "$(curl -sS -m 10 -o /dev/null -w '%{http_code}' -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$PRJ/find/CAP25-OTHER" || true)" != 200 ]; then
    c10imp CAP25-OTHER false "$C10/w/other.zip" 'POST import CAP25-OTHER overwrite=false (not managed by the module)' || die "c10: CAP25-OTHER could not be imported"
  else
    prose 'Script note: CAP25-OTHER already exists from the earlier variant; it is not imported again.'
  fi
  sub "variant $mode: D0, before any API import"
  c10snap "D0 ($mode)"
  [ "$(c10field CAP25-GIT dotgit "$SNAP")" = dir ] || { prose "Script reading: CAP25-GIT has no .git directory at D0 ($mode); the variant is not a module-managed start and is not run."; die "c10: CAP25-GIT is not module-managed at D0"; }
  d0g=$(c10field CAP25-GIT digest "$SNAP")
  d0o=$(c10field CAP25-OTHER digest "$SNAP")
  sub "variant $mode (i): API overwrite import of a changed CAP25-GIT zip, then 120 s of polling"
  /bin/rm -rf "$C10/w/changed"
  cp -R "$C10/seed-src" "$C10/w/changed"
  printf 'def v():\n    return 2\n' > "$C10/w/changed/ignition/script-python/cap25git/code.py"
  zipco "$C10/w/changed" "$C10/w/changed.zip" checkout
  shcap 'members of the changed zip' bash "$CAPTURE25" members "$C10/w/changed.zip"
  log_since=$(utc)
  t0=$(date +%s)
  c10imp CAP25-GIT true "$C10/w/changed.zip" "POST import CAP25-GIT overwrite=true (changed zip) — T0 ($mode)" || die "c10: the changed CAP25-GIT zip could not be imported"
  shcap 'poll CAP25-GIT and CAP25-OTHER every 10 s for 120 s from T0 (13 polls)' bash "$CAPTURE25" c10poll "$t0"
  dpoll=$LAST_OUT
  firstg=$(grep '^t=' <<<"$dpoll" | head -1 | sed -n 's/.*GIT members=[0-9]* digest=\([0-9a-f]*\).*/\1/p')
  lastg=$(grep '^t=' <<<"$dpoll" | tail -1 | sed -n 's/.*GIT members=[0-9]* digest=\([0-9a-f]*\).*/\1/p')
  firstdot=$(grep '^t=' <<<"$dpoll" | head -1 | sed -n 's/.*GIT members=[0-9]* digest=[0-9a-f]* disk=[0-9a-f]* dotgit=\([a-z]*\).*/\1/p')
  if [ "$firstg" = "$d0g" ]; then kept_poll=no; else kept_poll=yes; fi
  if [ "$kept_poll" = yes ] && [ "$lastg" = "$d0g" ]; then rev_poll=yes; fi
  [ "$firstdot" = dir ] || removes_git=yes
  shcap "gateway log lines since the import ($mode)" bash "$CAPTURE25" c10log "$log_since"
  c10snap "after the 120 s of polling ($mode)"
  s_import=$SNAP
  sub "variant $mode (i): gateway restart, then the commissioning cycle"
  log_since=$(utc)
  shcap 'restart the gateway (docker restart)' docker restart "$cid"
  c10wait 600 || prose 'Script reading: the gateway did not report CAP25-GIT within 600 s of the restart.'
  c10snap "right after RUNNING and CAP25-GIT found ($mode)"
  s_restart=$SNAP
  sleep 90
  c10snap "90 s later, module startup sync settled ($mode)"
  shcap "gateway log lines since the restart ($mode)" bash "$CAPTURE25" c10log "$log_since"
  log_since=$(utc)
  prose 'Script note: the module offers no inbound trigger (its pull is Designer/RPC driven; WHK-Global ships its own WebDev trigger in the project), so the commissioning cycle is the gateway start sync above. The one further trigger the WHK-Global CD runs after a pull is the filesystem scan, fired next.'
  cap 'POST scan/projects (the WHK-Global CD post-pull step)' POST /data/api/v1/scan/projects
  sleep 60
  c10snap "60 s after POST scan/projects ($mode)"
  s_scan=$SNAP
  shcap "gateway log lines since the scan ($mode)" bash "$CAPTURE25" c10log "$log_since"
  sub "variant $mode (ii): a fresh module-managed project, then remove its .git"
  prose 'Script note: step (i) ended without a `.git` directory (see the poll lines), so (ii) starts from a fresh clone.'
  c10reclone "$mode"
  c10snap "D0 of step (ii), a fresh clone ($mode)"
  d0b=$(c10field CAP25-GIT digest "$SNAP")
  [ "$(c10field CAP25-GIT dotgit "$SNAP")" = dir ] || { prose "Script reading: the fresh clone has no .git directory ($mode); step (ii) is not run."; die "c10: no fresh clone"; }
  shcap 'rig setup, not an ign write: remove the .git directory of CAP25-GIT inside the container' \
    docker exec -u root "$cid" rm -rf "$C10_PROJ/CAP25-GIT/.git"
  c10snap "after removing .git, before the restart ($mode)"
  log_since=$(utc)
  shcap 'restart the gateway (docker restart)' docker restart "$cid"
  c10wait 600 || prose 'Script reading: the gateway did not report CAP25-GIT within 600 s of the restart.'
  sleep 90
  c10snap "90 s after RUNNING, .git removed before the restart ($mode)"
  s_nodot=$SNAP
  shcap "gateway log lines since the restart ($mode)" bash "$CAPTURE25" c10log "$log_since"
  sub "variant $mode: verdicts"
  g1=$(c10field CAP25-GIT digest "$s_restart")
  g2=$(c10field CAP25-GIT digest "$s_scan")
  g3=$(c10field CAP25-GIT digest "$s_nodot")
  if [ "$kept_poll" = yes ] && [ "$g1" = "$d0g" ]; then rev_restart=yes; fi
  if [ "$kept_poll" = yes ] && [ "$g2" = "$d0g" ]; then rev_scan=yes; fi
  [ "$(c10field CAP25-GIT dotgit "$s_nodot")" = absent ] || recreated=yes
  [ "$g3" = "$d0b" ] || digest_moved=yes
  for s in "$s_import" "$s_restart" "$s_scan" "$s_nodot"; do
    o1=$(c10field CAP25-OTHER digest "$s")
    odot=$(c10field CAP25-OTHER dotgit "$s")
    [ "$o1" = "$d0o" ] || touched=yes
    [ "$odot" = absent ] || touched=yes
  done
  shcap "verdict lines, variant $mode" printf 'variant=%s\nd0_digest=%s\napi-import-reverted: %s (import took effect at the first poll: %s; last poll equals D0: %s; after restart equals D0: %s; after scan equals D0: %s)\nimport-removes-git: %s (.git kind at the first poll after the import: %s; D0 was dir)\ngit-recreated: %s (.git entry after removal and restart: %s; export digest changed against the fresh clone: %s)\nother-project-touched: %s (CAP25-OTHER digest or .git differed from D0 at some snapshot)\n' \
    "$mode" "$d0g" \
    "$( [ "$rev_poll" = yes ] || [ "$rev_restart" = yes ] || [ "$rev_scan" = yes ] && echo yes || echo no )" "$kept_poll" "$rev_poll" "$rev_restart" "$rev_scan" \
    "$removes_git" "${firstdot:-?}" \
    "$recreated" "$(c10field CAP25-GIT dotgit "$s_nodot")" "$digest_moved" "$touched"
}

# c10control: the same import on CAP25-OTHER, which the module does not
# manage, after a `.git` directory is placed in it from inside the container
# (rig setup). Shows whether the removal of `.git` by an overwrite import is
# the gateway's own replace or the module's.
c10control() {
  local cid st_before st_after
  cid=$(c10cid)
  sub 'control: an overwrite import of a project that has a .git the module does not manage'
  prose 'Script note: CAP25-OTHER is not in `git.yaml`. A `.git` directory (one HEAD file) is placed in it from inside the container (rig setup, not an ign write), then CAP25-OTHER is overwrite-imported with a changed zip (`def o()` returns 2 instead of 1), as step (i) did to CAP25-GIT. This is the only step that changes CAP25-OTHER on purpose, so it runs after the verdict lines above.'
  shcap 'rig setup: create .git (one HEAD file) in CAP25-OTHER' docker exec -u root "$cid" sh -c "mkdir $C10_PROJ/CAP25-OTHER/.git && printf 'ref: refs/heads/main\n' > $C10_PROJ/CAP25-OTHER/.git/HEAD && chown -R ignition:ignition $C10_PROJ/CAP25-OTHER/.git && ls -la $C10_PROJ/CAP25-OTHER"
  c10snap 'control, before the import'
  st_before=$SNAP
  /bin/rm -rf "$C10/w/other2"
  cp -R "$C10/w/other" "$C10/w/other2"
  printf 'def o():\n    return 2\n' > "$C10/w/other2/ignition/script-python/cap25other/code.py"
  zipco "$C10/w/other2" "$C10/w/other2.zip" checkout
  c10imp CAP25-OTHER true "$C10/w/other2.zip" 'POST import CAP25-OTHER overwrite=true (a changed zip, a .git present)' || prose 'Script reading: the control import did not answer 2xx.'
  sleep 20
  c10snap 'control, 20 s after the import'
  st_after=$SNAP
  shcap 'rig setup: remove the control .git again if it is still there' docker exec -u root "$cid" sh -c "rm -rf $C10_PROJ/CAP25-OTHER/.git; ls -a $C10_PROJ/CAP25-OTHER"
  shcap 'control verdict' printf 'import-removes-unmanaged-git: %s (.git kind of CAP25-OTHER before: %s; after: %s; export digest before: %s; after: %s)\n' \
    "$( [ "$(c10field CAP25-OTHER dotgit "$st_after")" = absent ] && echo yes || echo no )" "$(c10field CAP25-OTHER dotgit "$st_before")" "$(c10field CAP25-OTHER dotgit "$st_after")" "$(c10field CAP25-OTHER digest "$st_before")" "$(c10field CAP25-OTHER digest "$st_after")"
}

# c10poll T0: 13 lines, one every 10 s from T0: seconds since T0 and the
# state of both projects (digest, on-disk hash, .git kind).
c10poll() {
  local t0=${1:?T0} i now st g o
  for i in $(seq 0 12); do
    while now=$(date +%s); [ "$now" -lt "$((t0 + 10 * i))" ]; do sleep 0.5; done
    st=$(c10state)
    g=$(grep '^CAP25-GIT ' <<<"$st" | cut -d' ' -f2-)
    o=$(grep '^CAP25-OTHER ' <<<"$st" | cut -d' ' -f2-)
    printf 't=%ss GIT %s | OTHER %s\n' "$((now - t0))" "$g" "$o"
  done
}

c10() {
  local before_wh after_wh start cid
  mkdir -p "$C10/w"
  W=$C10/w
  [ -f "$C10/tokens.env" ] || die "c10: run 'capture25.sh c10up' first"
  . "$C10/tokens.env"
  cid=$(c10cid)
  [ -n "$cid" ] || die "c10: the ign-c10-rig gateway is not running"
  c10wait 600 || die "c10: the gateway does not report CAP25-GIT"
  sleep 30
  before_wh=$(git -C "$WHK_SRC" status --porcelain | shasum -a 256 | cut -c1-16)
  sect C10 'C10: does the git module interfere with an API project import, and does it recreate a removed `.git`? Recipe used: (b), `ign rig up --with-module git@'"$C10_VER"'` on a rig declared for this capture (compose project `ign-c10-rig`, 8.3.6 amd64 on loopback port 18291, a git server container serving one bare repository held under `$STATE/c10/repo`), with `git.yaml` rendered by the WHK-Global CI renderer `render-git-yaml.sh` from `git.yaml.tmpl` (read only; that repository is never written). The gateway is disposable and is torn down at the end of this block. `CAP25-GIT` is the module-managed project (synthetic checkout, repo branches `main` and `production`, tag `v1.0.0` on one commit); `CAP25-OTHER` is born by API import and is not in `git.yaml`. Every observation is a state snapshot: per project the export member count and digest (sha256 of the sorted `name sha256` lines, first 16 hex), the first 16 hex of the sha256 of its script file on the gateway disk (`absent` when none), the kind of `data/projects/<name>/.git` and its HEAD ref. No file content is recorded.'
  sub 'C10 setup'
  shcap 'rig up record (the module install, from the ign rig up JSON)' bash -c 'jq -c "{ok, state: .data.state, rig: .data.rig, modules: (.data.modules // .data.provisioned // null)}" "$0" | cut -c1-600' "$C10/rigup.json"
  cap 'GET gateway-info (version, deploymentMode)' GET /data/api/v1/gateway-info
  CAP_BODY_MAX=600 cap 'GET modules/healthy?search=git' GET '/data/api/v1/modules/healthy?search=git'
  shcap 'effective git.yaml keys, template variant (the credential key omitted)' bash "$CAPTURE25" c10cfg
  shcap 'the bare repository: branches, tags, commits (read from the host)' bash -c 'git -C "$0" branch -a; git -C "$0" tag -l; git -C "$0" log --oneline --all' "$C10/repo/cap25-git.git"
  shcap 'compose services of the C10 gateway' bash -c 'docker compose -p "$0" ps --format "table {{.Service}}\t{{.Image}}\t{{.State}}"' "$C10P"
  shcap 'git binary inside the gateway container' docker exec "$cid" sh -c 'command -v git || echo "no git binary"'
  start=$(utc)
  sub 'variant template: baseline'
  shcap "gateway log lines since the start of this block (module startup sync), CAP25-GIT import" bash "$CAPTURE25" c10log "$(date -u -v-30M +%FT%TZ 2>/dev/null || date -u -d '30 minutes ago' +%FT%TZ)"
  c10variant template
  c10control
  c10yaml production
  c10variant production
  sub 'cleanup'
  c8del CAP25-GIT
  c8del CAP25-OTHER
  plist_after
  shcap 'tear down the C10 gateway and remove its state' bash "$CAPTURE25" c10down
  after_wh=$(git -C "$WHK_SRC" status --porcelain | shasum -a 256 | cut -c1-16)
  prose "Script reading: \`git -C WHK-Global status --porcelain\` hashed before ($before_wh) and after ($after_wh) this block: $( [ "$before_wh" = "$after_wh" ] && echo unchanged || echo CHANGED )."
}

# -------------------------------------------------------------------- C-H

# C-H: what the deploy account sees of data/projects/<name>/.git on the Linux
# and the Windows capture host. Test setup runs through the recorded admin
# route (Linux: ssh as ignadmin with sudo; Windows: `az vm run-command` as
# SYSTEM), never as the deploy account; every probe runs as the deploy
# account over the ssh options the Phase 22/23 live tests use (pinned
# known_hosts, key file, no agent forwarding). Only CAP25-GITPROBE and its
# `.git` entries are created, and they are removed in the same run.
CH_NAME=CAP25-GITPROBE
CH_WIN_RG=${IGN_WINVM_RG:-RG-IGN-CLI-WIN-TEST}
CH_WIN_VM=${IGN_WINVM_VM:-ign-win-836}
CH_WIN_DATA=${IGN_WINVM_DATA_DIR:-C:\\Program Files\\Inductive Automation\\Ignition\\data}
CH_LNX_ADMIN_KEY=${IGN_LNXVM_ADMIN_KEY:-$HOME/.cache/ign-lnxvm01/ignadmin_ed25519}
CH_LNX_CFG=${IGN_LNXVM_CONFIG:-$HOME/.cache/ign-lnxvm01/ign-config.toml}
CH_LNX_TOKEN_FILE=${IGN_LNXVM_TOKEN_FILE:-$HOME/.cache/ign-lnxvm01/api-token}

# chtime CMD [ARG ...]: run CMD with a 180 s limit, print its stdout then its
# stderr, then a last line `@@exit=N ms=M`.
chtime() {
  python3 -I - "$@" <<'PY'
import subprocess
import sys
import time

t = time.monotonic()
try:
    r = subprocess.run(sys.argv[1:], capture_output=True, timeout=180, stdin=subprocess.DEVNULL)
    sys.stdout.write(r.stdout.decode("utf-8", "replace"))
    sys.stdout.write(r.stderr.decode("utf-8", "replace"))
    code = r.returncode
except subprocess.TimeoutExpired:
    sys.stdout.write("[timed out after 180 s]\n")
    code = 124
sys.stdout.write("\n@@exit=%d ms=%d\n" % (code, round((time.monotonic() - t) * 1000)))
PY
}

CH_EXIT=""
CH_MS=""
CH_OUT=""
# chcap LABEL SHOWN CMD [ARG ...]: one probe. SHOWN is the command line as it
# is written to the record (host, key and known_hosts spelled as variables).
# CH_ANS_PAT, when set, is the grep pattern whose first matching line is the
# probe's answer in the table row.
chcap() {
  local label=$1 shown=$2; shift 2
  local out=$TMPD/ch.raw started body ans
  started=$(utc)
  chtime "$@" > "$out" 2>&1 || true
  tr -d '\r' < "$out" | grep -v '^\*\* \|^Connected to \|^Warning: Permanently added\|^$' > "$out.f" || true
  CH_EXIT=$(sed -n 's/^@@exit=\([0-9]*\) ms=.*/\1/p' "$out.f" | tail -1)
  CH_MS=$(sed -n 's/^@@exit=[0-9]* ms=\([0-9]*\)/\1/p' "$out.f" | tail -1)
  body=$(grep -v '^@@exit=' "$out.f" | cut -c1-300 | head -60 || true)
  CH_OUT=$body
  {
    printf '\n#### %s\n\n`%s` @ %s\n\n~~~\n' "$label" "$shown" "$started"
    [ -z "$body" ] || printf '%s\n' "$body"
    printf '[exit %s, %s ms]\n~~~\n' "$CH_EXIT" "$CH_MS"
  } >> "$OUT"
  if [ -n "${CH_ANS_PAT:-}" ]; then
    ans=$(grep -E -- "$CH_ANS_PAT" <<<"$body" | head -1 | cut -c1-110 || true)
    [ -n "$ans" ] || ans="no line matches"
  else
    ans=$(grep -v '^sftp> ' <<<"$body" | head -1 | cut -c1-110 || true)
    [ -n "$ans" ] || ans="(no output)"
  fi
  CH_ANS=$ans
}
CH_ANS=""

# chrow SHAPE PROBE: append the last probe to the summary table of the host.
chrow() {
  printf '| %s | %s | %s | %s | `%s` |\n' "$1" "$2" "$CH_EXIT" "$CH_MS" "${CH_ANS//|/\\|}" >> "$TMPD/ch.table"
}

# psenc SCRIPT: the PowerShell prelude of winps.rs plus SCRIPT, as the
# `-EncodedCommand` payload (UTF-16LE, base64).
psenc() {
  python3 -I - "$1" <<'PY'
import base64
import sys

pre = "$ProgressPreference='SilentlyContinue';$ErrorActionPreference='Stop';"
print(base64.b64encode((pre + sys.argv[1]).encode("utf-16-le")).decode())
PY
}

# ---- per-host plumbing (set by ch_init)
CH_OS=""; CH_USER=""; CH_HOST=""; CH_KEY=""; CH_KH=""; CH_DATA=""; CH_PROFILE=""
CH_SSHO=()
CH_SSH_SHOWN='ssh -i $KEY -o UserKnownHostsFile=$KNOWN_HOSTS -o StrictHostKeyChecking=yes -o IdentitiesOnly=yes -o BatchMode=yes -o ForwardAgent=no'
CH_SFTP_SHOWN='sftp -b BATCH -i $KEY -o UserKnownHostsFile=$KNOWN_HOSTS -o StrictHostKeyChecking=yes -o IdentitiesOnly=yes -o BatchMode=yes -o ForwardAgent=no'

ch_init() {
  case "${1:-}" in
    lnx)
      : "${IGN_LNXVM_HOST:?IGN_LNXVM_HOST is not set (see 23-LIVE-CAPTURES.md)}" "${IGN_LNXVM_USER:?}" "${IGN_LNXVM_KEY_FILE:?}" "${IGN_LNXVM_KNOWN_HOSTS:?}" "${IGN_LNXVM_DATA_DIR:?}" "${IGN_LNXVM_PROFILE:?}"
      CH_OS=posix; CH_USER=$IGN_LNXVM_USER; CH_HOST=$IGN_LNXVM_HOST; CH_KEY=$IGN_LNXVM_KEY_FILE; CH_KH=$IGN_LNXVM_KNOWN_HOSTS; CH_DATA=$IGN_LNXVM_DATA_DIR; CH_PROFILE=$IGN_LNXVM_PROFILE
      export IGNITION_CLI_CONFIG=$CH_LNX_CFG
      IGNITION_TOKEN_AZURE_LNX=$(cat "$CH_LNX_TOKEN_FILE")
      export IGNITION_TOKEN_AZURE_LNX ;;
    win)
      : "${IGN_WINVM_HOST:?IGN_WINVM_HOST is not set (see 23-LIVE-CAPTURES.md)}" "${IGN_WINVM_USER:?}" "${IGN_WINVM_KEY:?}" "${IGN_WINVM_KNOWN_HOSTS:?}" "${IGN_WINVM_PROFILE:?}"
      CH_OS=windows; CH_USER=$IGN_WINVM_USER; CH_HOST=$IGN_WINVM_HOST; CH_KEY=$IGN_WINVM_KEY; CH_KH=$IGN_WINVM_KNOWN_HOSTS; CH_DATA=$CH_WIN_DATA; CH_PROFILE=$IGN_WINVM_PROFILE ;;
    *) die "ch: HOST must be lnx or win" ;;
  esac
  [ -f "$CH_KEY" ] || die "ch: the key file of the $1 host is not a file"
  [ -f "$CH_KH" ] || die "ch: the known_hosts file of the $1 host is missing"
  CH_SSHO=(-i "$CH_KEY" -o "UserKnownHostsFile=$CH_KH" -o StrictHostKeyChecking=yes -o IdentitiesOnly=yes -o BatchMode=yes -o ForwardAgent=no -o ConnectTimeout=30)
}

# chsftp PATH FLAGS SHOWN-PATH LABEL: an sftp batch `ls FLAGS PATH` as the deploy account.
chsftp() {
  local path=$1 flags=$2 b=$TMPD/ch.batch
  printf 'ls %s "%s"\nbye\n' "$flags" "$path" > "$b"
  chcap "sftp ls $flags: $4" "$CH_SFTP_SHOWN $CH_USER@\$HOST   # batch: ls $flags \"$3\"" \
    sftp -b "$b" "${CH_SSHO[@]}" "$CH_USER@$CH_HOST"
}

# ch_admin_lnx SCRIPT SHAPE: run a POSIX script as root through the ignadmin sudo route.
ch_admin_lnx() {
  chcap "admin route (ignadmin, sudo): shape $2" "ssh -i \$ADMIN_KEY ... ignadmin@\$HOST \"sudo -n sh -c '<setup script for the $2 shape>'\"" \
    ssh -i "$CH_LNX_ADMIN_KEY" -o "UserKnownHostsFile=$CH_KH" -o StrictHostKeyChecking=yes -o IdentitiesOnly=yes -o BatchMode=yes -o ForwardAgent=no -o ConnectTimeout=30 "ignadmin@$CH_HOST" "sudo -n sh -c '$1'"
}

# ch_admin_win SCRIPT SHAPE: run PowerShell as SYSTEM through az vm run-command.
ch_admin_win() {
  chcap "admin route (az vm run-command, SYSTEM): shape $2" "az vm run-command invoke -g \$RG -n \$VM --command-id RunPowerShellScript --scripts '<setup script for the $2 shape>' --query value[0].message -o tsv" \
    az vm run-command invoke -g "$CH_WIN_RG" -n "$CH_WIN_VM" --command-id RunPowerShellScript --scripts "$1" --query 'value[0].message' -o tsv
}

# ch_create / ch_delete: the test project through the gateway's REST API.
ch_create() {
  chcap "create the test project through the gateway REST API (ign project new)" "ign --profile \$PROFILE project new $CH_NAME --title 'CAP25 C-H probe' --json" \
    "$IGN" --profile "$CH_PROFILE" project new "$CH_NAME" --title 'CAP25 C-H probe' --json
}
ch_delete() {
  chcap "delete the test project through the gateway REST API (ign project delete)" "ign --profile \$PROFILE project delete $CH_NAME --yes --json" \
    "$IGN" --profile "$CH_PROFILE" project delete "$CH_NAME" --yes --json
}

# ch_psshape SHAPE: the PowerShell setup script of a Windows shape (admin route).
ch_psshape() {
  local s=$1 P="$CH_DATA\\projects\\$CH_NAME" pre
  pre=$(cat <<'PS'
$P='__P__'
foreach ($n in '.git','.git-real') {
  $q = Join-Path $P $n
  if (Get-Item -LiteralPath $q -Force -ErrorAction SilentlyContinue) {
    cmd /c rmdir "$q" 2>$null
    if (Get-Item -LiteralPath $q -Force -ErrorAction SilentlyContinue) { Remove-Item -LiteralPath $q -Recurse -Force }
  }
}
PS
)
  printf '%s\n' "${pre//__P__/$P}"
  case "$s" in
    none) : ;;
    directory-hidden) cat <<'PS'
New-Item -ItemType Directory -Path (Join-Path $P '.git') | Out-Null
Set-Content -LiteralPath (Join-Path $P '.git\HEAD') -Value 'ref: refs/heads/main'
attrib +h (Join-Path $P '.git')
PS
      ;;
    directory-visible) cat <<'PS'
New-Item -ItemType Directory -Path (Join-Path $P '.git') | Out-Null
Set-Content -LiteralPath (Join-Path $P '.git\HEAD') -Value 'ref: refs/heads/main'
PS
      ;;
    file-hidden) cat <<'PS'
Set-Content -LiteralPath (Join-Path $P '.git') -Value 'gitdir: .\.git-real'
attrib +h (Join-Path $P '.git')
PS
      ;;
    junction) cat <<'PS'
New-Item -ItemType Directory -Path (Join-Path $P '.git-real') | Out-Null
cmd /c mklink /J (Join-Path $P '.git') (Join-Path $P '.git-real') | Out-Null
PS
      ;;
  esac
  cat <<'PS'
Get-ChildItem -LiteralPath $P -Force | ForEach-Object { $_.Name + ' attrs=' + [int]$_.Attributes + ' (' + $_.Attributes + ')' } | Out-String
PS
}

# ch_psprobe N: the N-th PowerShell probe body ($g is the .git path, $d the project dir).
ch_psprobe() {
  case "$1" in
    0) cat <<'PS'
try { [Console]::Out.WriteLine('Test-Path=' + (Test-Path -LiteralPath $g)) } catch { [Console]::Out.WriteLine('ERR ' + $_.Exception.GetType().Name + ': ' + $_.Exception.Message.Split("`n")[0]) }
PS
      ;;
    1) cat <<'PS'
try { [Console]::Out.WriteLine('Test-Path-Force=' + (Test-Path -LiteralPath $g -Force)) } catch { [Console]::Out.WriteLine('ERR ' + $_.Exception.GetType().Name + ': ' + $_.Exception.Message.Split("`n")[0]) }
PS
      ;;
    2) cat <<'PS'
[Console]::Out.WriteLine('Directory.Exists=' + [IO.Directory]::Exists($g) + ' File.Exists=' + [IO.File]::Exists($g))
PS
      ;;
    3) cat <<'PS'
try { $i = Get-Item -LiteralPath $g -Force; [Console]::Out.WriteLine('Attributes=' + [int]$i.Attributes + ' (' + $i.Attributes + ') LinkType=' + $i.LinkType) } catch { [Console]::Out.WriteLine('Get-Item: ' + $_.Exception.GetType().Name) }
PS
      ;;
    4) cat <<'PS'
try { foreach ($f in ([IO.DirectoryInfo]$d).EnumerateFileSystemInfos()) { [Console]::Out.WriteLine($f.Name + ' attrs=' + [int]$f.Attributes + ' dir=' + (($f.Attributes -band 16) -ne 0) + ' reparse=' + (($f.Attributes -band 1024) -ne 0)) } } catch { [Console]::Out.WriteLine('ERR ' + $_.Exception.GetType().Name + ': ' + $_.Exception.Message.Split("`n")[0]) }
PS
      ;;
  esac
}

# ch_shape SHAPE: put data/projects/CAP25-GITPROBE/.git into SHAPE (admin route).
ch_shape() {
  local s=$1 P
  if [ "$CH_OS" = posix ]; then
    P=$CH_DATA/projects/$CH_NAME
    case "$s" in
      none) ch_admin_lnx "rm -rf $P/.git $P/.git-real; ls -la $P" "$s" ;;
      directory) ch_admin_lnx "rm -rf $P/.git $P/.git-real; mkdir $P/.git; printf \"ref: refs/heads/main\\n\" > $P/.git/HEAD; ls -la $P" "$s" ;;
      file) ch_admin_lnx "rm -rf $P/.git $P/.git-real; printf \"gitdir: ./.git-real\\n\" > $P/.git; ls -la $P" "$s" ;;
      symlink) ch_admin_lnx "rm -rf $P/.git $P/.git-real; mkdir $P/.git-real; ln -s .git-real $P/.git; ls -la $P" "$s" ;;
    esac
  else
    ch_admin_win "$(ch_psshape "$s")" "$s"
  fi
}

# ch_probes SHAPE: every candidate probe as the deploy account.
ch_probes() {
  local s=$1 P g d enc i body
  sub "$CH_OS host, shape: $s"
  if [ "$CH_OS" = posix ]; then
    P=$CH_DATA/projects/$CH_NAME
    CH_ANS_PAT="(^| |/)$CH_NAME\$" chsftp "$CH_DATA/projects" "-lan" "<data>/projects" "data/projects ($s)"; chrow "$s" "sftp ls -lan data/projects (line for the project)"
    CH_ANS_PAT="(^| |/)\\.git\$" chsftp "$P" "-lan" "<data>/projects/$CH_NAME" "data/projects/$CH_NAME ($s)"; chrow "$s" "sftp ls -lan data/projects/<name> (line for .git)"
    CH_ANS_PAT="" chsftp "$P/.git" "-lan" "<data>/projects/$CH_NAME/.git" "data/projects/$CH_NAME/.git ($s)"; chrow "$s" "sftp ls -lan data/projects/<name>/.git (first line)"
    CH_ANS_PAT="" chcap "ssh stat of the .git entry ($s)" "$CH_SSH_SHOWN $CH_USER@\$HOST stat -c '%F %A %U:%G' <data>/projects/$CH_NAME/.git" \
      ssh "${CH_SSHO[@]}" "$CH_USER@$CH_HOST" "stat -c '%F %A %U:%G' '$P/.git' 2>&1"; chrow "$s" "ssh stat -c '%F %A %U:%G' .git"
    CH_ANS_PAT="" chcap "ssh test -L / -d / -f / -e on the .git entry ($s)" "$CH_SSH_SHOWN $CH_USER@\$HOST sh -c 'test -L; test -d; test -f; test -e on <data>/projects/$CH_NAME/.git'" \
      ssh "${CH_SSHO[@]}" "$CH_USER@$CH_HOST" "p='$P/.git'; r=''; [ -L \"\$p\" ] && r=\"\$r symlink\"; [ -d \"\$p\" ] && r=\"\$r dir\"; [ -f \"\$p\" ] && r=\"\$r file\"; [ -e \"\$p\" ] && r=\"\$r exists\"; echo \"kinds:\${r:- none}\""; chrow "$s" "ssh test -L/-d/-f/-e .git"
  else
    d="$CH_DATA\\projects\\$CH_NAME"; g="$d\\.git"
    CH_ANS_PAT="(^| |/)$CH_NAME\$" chsftp "/${CH_DATA//\\//}/projects" "-la" "/<data>/projects" "data/projects ($s)"; chrow "$s" "sftp ls -la data/projects (line for the project)"
    CH_ANS_PAT="(^| |/)\\.git\$" chsftp "/${d//\\//}" "-la" "/<data>/projects/$CH_NAME" "data/projects/$CH_NAME ($s)"; chrow "$s" "sftp ls -la data/projects/<name> (line for .git)"
    CH_ANS_PAT="" chsftp "/${g//\\//}" "-la" "/<data>/projects/$CH_NAME/.git" "data/projects/$CH_NAME/.git ($s)"; chrow "$s" "sftp ls -la data/projects/<name>/.git (first line)"
    local names=(Test-Path Test-Path-Force Exists-methods Get-Item-Force Enumerate-project-dir)
    local pats=('^Test-Path=' '^Test-Path-Force=|^ERR' '^Directory.Exists' '^Attributes=|^Get-Item' '^\.git ')
    for i in 0 1 2 3 4; do
      body=$(ch_psprobe "$i")
      enc=$(psenc "\$g='$g';\$d='$d';$body")
      CH_ANS_PAT=${pats[$i]} chcap "PowerShell ${names[$i]} on the .git entry ($s)" "$CH_SSH_SHOWN $CH_USER@\$HOST powershell -NoProfile -NonInteractive -EncodedCommand <encoded: \$g='<data>\\projects\\$CH_NAME\\.git'; \$d='<data>\\projects\\$CH_NAME'; ${body}>" \
        ssh "${CH_SSHO[@]}" "$CH_USER@$CH_HOST" "powershell -NoProfile -NonInteractive -EncodedCommand $enc"
      chrow "$s" "PowerShell ${names[$i]}"
    done
  fi
}

# ch_restrict MODE: narrow the test project directory (apply), give the deploy
# account the narrowest list/traverse right on it (grant), put it back (restore).
# Only data/projects/CAP25-GITPROBE is touched, never data/projects itself.
ch_restrict() {
  local m=$1 P
  if [ "$CH_OS" = posix ]; then
    P=$CH_DATA/projects/$CH_NAME
    case "$m" in
      apply) ch_admin_lnx "chmod 750 $P; setfacl -b $P; ls -ld $P; getfacl -p $P" "restrict: chmod 750, no ACL" ;;
      grant) ch_admin_lnx "setfacl -m u:$CH_USER:r-x $P; ls -ld $P; getfacl -p $P" "grant: setfacl -m u:$CH_USER:r-x" ;;
      restore) ch_admin_lnx "setfacl -b $P; chmod 755 $P; ls -ld $P" "restore: setfacl -b, chmod 755" ;;
    esac
  else
    local pre body
    pre=$(cat <<'PS'
$P='__P__'
$a = $env:COMPUTERNAME + '\svc-ignition-deploy'
PS
)
    local win="$CH_DATA\\projects\\$CH_NAME"
    pre=${pre//__P__/$win}
    case "$m" in
      apply) body='icacls $P /inheritance:d | Out-Null
icacls $P /remove:g "BUILTIN\Users" | Out-Null
icacls $P' ;;
      grant) body='icacls $P /grant ($a + ":(OI)(CI)(RX)") | Out-Null
icacls $P' ;;
      restore) body='icacls $P /reset /T | Out-Null
icacls $P' ;;
    esac
    ch_admin_win "$pre
$body" "restricted: $m"
  fi
}

# ch_cleanup: remove the test entries and the test project. Never fails.
ch_cleanup() {
  set +e
  sub "$CH_OS host: cleanup"
  ch_restrict restore
  ch_shape none
  ch_delete
  local n=0
  if [ "$CH_OS" = posix ]; then
    CH_ANS_PAT="(^| |/)$CH_NAME\$" chsftp "$CH_DATA/projects" "-lan" "$CH_DATA/projects" "data/projects after the cleanup"
  else
    CH_ANS_PAT="(^| |/)$CH_NAME\$" chsftp "/${CH_DATA//\\//}/projects" "-la" "/<data>/projects" "data/projects after the cleanup"
  fi
  n=$(grep -c "$CH_NAME" <<<"$CH_OUT" || true)
  prose "Script reading: the sftp listing of data/projects after the cleanup names $CH_NAME $n time(s)."
  set -e
}

ch_body() {
  local s shapes
  ch_create
  # wait for the gateway to write the project directory (up to 60 s)
  local k
  for k in $(seq 1 12); do
    if [ "$CH_OS" = posix ]; then chsftp "$CH_DATA/projects/$CH_NAME" "-lan" "$CH_DATA/projects/$CH_NAME" "wait for the project directory ($k)"
    else chsftp "/${CH_DATA//\\//}/projects/$CH_NAME" "-la" "/<data>/projects/$CH_NAME" "wait for the project directory ($k)"; fi
    grep -q 'project.json' <<<"$CH_OUT" && break
    sleep 5
  done
  if [ "$CH_OS" = posix ]; then shapes="none directory file symlink"; else shapes="none directory-hidden directory-visible file-hidden junction"; fi
  : > "$TMPD/ch.table"
  for s in $shapes; do
    sub "$CH_OS host: set the shape to $s"
    ch_shape "$s"
    ch_probes "$s"
  done
  local dshape=directory
  [ "$CH_OS" = posix ] || dshape=directory-hidden
  sub "$CH_OS host: the project directory restricted, then the narrow grant"
  prose "Script note: with a $dshape \`.git\` in place, only the test project directory is narrowed (Linux: mode 750 and no ACL; Windows: inheritance copied and the Users group removed), probed, given the narrowest list/traverse right for the deploy account on that one directory (Linux: \`setfacl -m u:<account>:r-x\`; Windows: \`icacls ... /grant <computer>\\<account>:(OI)(CI)(RX)\`), probed again, and put back. \`data/projects\` itself is never changed."
  ch_shape "$dshape"
  ch_restrict apply
  ch_probes "$dshape, project directory restricted, no grant"
  ch_restrict grant
  ch_probes "$dshape, project directory restricted, grant applied"
  ch_restrict restore
  sub "$CH_OS host: probe table"
  prose 'One row per probe and shape: exit status and wall time at the runner, and the first line the probe answered (the matching line for a listing). Wall time includes the ssh connection setup (and for PowerShell the process start).'
  {
    printf '\n| shape | probe | exit | ms | answer |\n| --- | --- | --- | --- | --- |\n'
    cat "$TMPD/ch.table"
  } >> "$OUT"
}

ch() {
  local host=${1:?HOST (lnx or win)} rc=0
  ch_init "$host"
  grep -q '^## C-H before grant$' "$OUT" 2>/dev/null || sect 'C-H before grant' 'C-H before grant: what the deploy account (`svc-ignition-deploy`) sees of `data/projects/<name>/.git` on the Linux and on the Windows capture host, before any grant for DPRJ-04. Test setup runs through the admin route recorded in 23-LIVE-CAPTURES.md (Linux: ssh as `ignadmin` with sudo; Windows: `az vm run-command` as SYSTEM), never as the deploy account. Only the project `CAP25-GITPROBE` (created and deleted through the gateway REST API) and its `.git` entries are created, and they are removed at the end of each host block. Every probe runs as the deploy account over ssh with the pinned known_hosts, a key file and no agent forwarding. Host, key and known_hosts are written as variables; no key or token text is recorded.'
  sub "$([ "$host" = lnx ] && echo 'Linux capture host (ign-lnx-836)' || echo 'Windows capture host (ign-win-836)')"
  ch_body || rc=$?
  ch_cleanup
  return "$rc"
}

# ------------------------------------------------------------------- up25

# up25 (25-19): the rig for the live `deploy projects` gates. A fresh, empty
# host data root OUTSIDE the repo (default $STATE/data25, or
# IGN_DEPLOY_RIG_DATA), `data/projects` and `data/config/resources` bound onto
# it by compose.projects.yml, the gateway up and RUNNING, the cap-a token
# minted (mint-token.sh restarts the gateway) and answering. Loopback only:
# assert_local has run, and compose.yml binds 127.0.0.1.
#
# Afterwards:
#   . "$STATE/tokens.env"
#   IGNITION_LIVE_DEPLOY=1 IGNITION_LIVE_URL=http://localhost:18290 \
#     IGNITION_LIVE_TOKEN="$IGN_TOKEN_CAP_A" IGN_DEPLOY_RIG_DATA=<the root up25 printed> \
#     cargo test -p ignition-core --test live_deploy_projects -- --ignored --nocapture --test-threads=1
up25() {
  local root=${IGN_DEPLOY_RIG_DATA:-$STATE/data25} real repo_real t0 st
  mkdir -p "$(dirname "$root")"
  case "$root" in /*) ;; *) echo "REFUSING: the data root '$root' is not an absolute path" >&2; exit 2 ;; esac
  real=$(cd "$(dirname "$root")" && pwd -P)/$(basename "$root")
  repo_real=$(cd "$REPO" && pwd -P)
  case "$real/" in
    "$repo_real"/*) echo "REFUSING: the data root $real is inside the repository $repo_real" >&2; exit 2 ;;
  esac
  if [ -e "$real" ] && [ -n "$(ls -A "$real" 2>/dev/null)" ]; then
    echo "REFUSING: the data root $real is not empty; run down25, remove it, then up25" >&2
    exit 2
  fi
  # A gw-data volume left by an earlier rig holds a commissioned gateway whose
  # config/resources is not the empty bind: the new gateway would fault with
  # "Commissioning Failure". Start from nothing.
  if docker volume inspect ign-deploy-rig_gw-data > /dev/null 2>&1; then
    echo "REFUSING: volume ign-deploy-rig_gw-data exists; run the down25 command first" >&2
    exit 2
  fi
  mkdir -p "$real/projects" "$real/config/resources"
  export IGN_DEPLOY_RIG_DATA=$real
  date -u +%FT%TZ > "$STATE/start-utc-25"
  ( cd "$HERE" && docker compose -p ign-deploy-rig -f compose.yml -f compose.projects.yml up -d )
  t0=$(date +%s)
  until curl -s -m 5 "$BASE/StatusPing" | grep -q RUNNING; do
    [ $(( $(date +%s) - t0 )) -lt 900 ] || { echo "gateway not RUNNING within 900 s" >&2; exit 1; }
    sleep 3
  done
  bash "$HERE/mint-token.sh" cap-a > /dev/null
  # shellcheck disable=SC1091
  . "$STATE/tokens.env"
  t0=$(date +%s)
  while :; do
    st=$(curl -sS -m 10 -o /dev/null -w '%{http_code}' -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE/data/api/v1/gateway-info" 2>/dev/null || true)
    [ "$st" = 200 ] && break
    [ $(( $(date +%s) - t0 )) -lt 600 ] || { echo "gateway-info answers '$st' with the cap-a token after 600 s" >&2; exit 1; }
    sleep 3
  done
  echo "RUNNING; data root $real; token in $STATE/tokens.env"
}

# --------------------------------------------------------------- dispatch

case "${1:-}" in
  header25) header25 ;;
  zipco) zipco "${2:?DIR}" "${3:?OUT}" "${4:-checkout}" ;;
  members) members "${2:?ZIP}" ;;
  cmp) cmp_table "${2:?ZIP}" "${3:?EXPORT}" ;;
  cmpsum) cmpsum "${2:?ZIP}" "${3:?EXPORT}" ;;
  files) files "${2:?ZIP}" "${3:?KEY}" ;;
  findsel) findsel "${2:?NAME}" ;;
  sect) sect "${2:?ID}" "${3:-}" ;;
  shcap) shift; shcap "$@" ;;
  c8del) c8del "${2:?NAME}" ;;
  c8c-synth) c8c_synth ;;
  c8c-probe) c8c_probe ;;
  c8c-whk) c8c_whk ;;
  c8c-whk-rj) c8c_whk_rj ;;
  rjdiff) rjdiff "${2:?ZIP}" "${3:?EXPORT}" ;;
  c8d) shift; c8d "$@" ;;
  c8a) c8a ;;
  c8b) c8b ;;
  c8e) c8e ;;
  c9) c9 ;;
  ch) ch "${2:?HOST (lnx or win)}" ;;
  chtime) shift; chtime "$@" ;;
  c10) c10 ;;
  c10up) c10up ;;
  c10down) c10down ;;
  c10cfg) c10cfg ;;
  c10state) c10state ;;
  c10log) c10log "${2:?SINCE}" ;;
  c10poll) c10poll "${2:?T0}" ;;
  c9digest) c9digest "${2:?ZIP}" ;;
  c9poll) c9poll "${2:?NAME}" "${3:?T0}" ;;
  zinfo) zinfo "${2:?ZIP}" ;;
  gwdisk) gwdisk "${2:?NAME}" ;;
  note) prose "${2:?TEXT}" ;;
  up25) up25 ;;
  down25)
    echo "docker compose -p ign-deploy-rig -f compose.yml -f compose.projects.yml down -v"
    echo "(then remove the data root, e.g. /bin/rm -rf \"\${IGN_DEPLOY_RIG_DATA:-$STATE/data25}\")" ;;
  *) echo "usage: capture25.sh header25|zipco|members|cmp|cmpsum|files|findsel|gwdisk|note|rjdiff|zinfo|c8c-synth|c8c-probe|c8c-whk|c8c-whk-rj|c8d|c8a|c8b|c8e|c9|c10up|c10|c10down|ch|c8del|sect|shcap|up25|down25" >&2; exit 64 ;;
esac
