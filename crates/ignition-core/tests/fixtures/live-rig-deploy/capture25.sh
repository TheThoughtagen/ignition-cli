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
#
# Subcommands: header25 | zipco DIR OUT [checkout|export] | members ZIP
#              | cmp ZIP EXPORT | cmpsum ZIP EXPORT | files ZIP MEMBER
#              | findsel NAME | gwdisk NAME | note TEXT | c8c-synth | c8c-probe | c8c-whk | c8c-whk-rj | rjdiff ZIP EXPORT | c8d | c8a | c8b | c8e
#              | c8del NAME | sect ID TEXT | shcap LABEL CMD [ARG ...]
#              | down25 (prints the teardown command; it does not run it)
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
    C8a | C8b | C8c | C8d | C8e) ;;
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
  zinfo) zinfo "${2:?ZIP}" ;;
  gwdisk) gwdisk "${2:?NAME}" ;;
  note) prose "${2:?TEXT}" ;;
  down25) echo "docker compose -p ign-deploy-rig down -v" ;;
  *) echo "usage: capture25.sh header25|zipco|members|cmp|cmpsum|files|findsel|gwdisk|note|rjdiff|zinfo|c8c-synth|c8c-probe|c8c-whk|c8c-whk-rj|c8d|c8a|c8b|c8e|c8del|sect|shcap|down25" >&2; exit 64 ;;
esac
