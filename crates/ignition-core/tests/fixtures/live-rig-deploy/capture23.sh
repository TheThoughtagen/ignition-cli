#!/usr/bin/env bash
# capture23.sh SUBCOMMAND [ARG ...]
#
# Phase 23 (23-01) rig capture driver: the rig facts the guarded apply engine
# depends on, taken from a fresh 8.3.6 gateway before any code relies on them.
#
#   csysname  C-sysname: the REST gateway-info `name` against the on-disk
#             `systemName` of ignition/system-properties (research Open
#             Question 1: what a wrong-target rule can compare)
#   crestart  C-restart: status and body of GET restart-tasks/pending with
#             nothing pending, after a restart-requiring change if the rig
#             can make one, and for a token without permission (DAPL-10,
#             DAPI-06: no DeployApi method before this record)
#   cgwbk     C-gwbk: is a downloaded .gwbk a ZIP whose central directory
#             reads, its size and time, Content-Length, and how a truncated
#             copy fails (research A2, D-13)
#   c11       C11: a tag provider and its tag definitions written together,
#             one config scan or two
#   c2        C2 (a), (c), (d), (e): what the config scan-lock blocks, and
#             whether files written into the mode collection load without
#             the engine's scan request; (b) is a manual stub section
#
# Subcommands: up23 | setup23 | tokens23 | header23 | hdrrow KEY VALUE
#              | csysname | crestart | cgwbk | c11 | c2
#              | sect ID TEXT | shcap LABEL CMD [ARG ...]
#              | down23 (prints the teardown command; it does not run it)
#
# up23 and setup23 delegate to capture22.sh (the Phase 22 compose project
# `ign-deploy-rig`, bind-mounted resources, port 18290); setup23 then mints
# the second token (cap-b) and a token with no security levels (cap-ro).
#
# Every byte inside a capture section is appended by `cap` (HTTP) or `shcap`
# (any other command) and is preceded by a line carrying the exact command and
# a UTC time taken by this script just before the run.
#
# LOCALHOST ONLY. assert_local runs before anything else (before any file is
# read or written, before any request) and exits 2 unless BASE's host is
# localhost, 127.0.0.1 or [::1]. The authority is cut at the first `/`, `?`,
# `#` or `\` BEFORE userinfo is stripped, so 'http://evil.example#@localhost'
# is refused (the R-01 / WR-08 fix of Phase 21, as in capture22.sh).
# Tokens come from tokens.env (0600, outside the repo), are sent as a request
# header and are never printed: `cap` prints the response headers and body
# only, and its command line names the token variable, never its value. Never
# enable `set -x` here.
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
OUT=${CAPTURE_OUT:-$REPO/.planning/phases/23-guarded-apply-rollback/23-LIVE-CAPTURES.md}
STATE=${IGN_DEPLOY_RIG_STATE:-$HOME/.cache/ign-deploy-rig}
DATA=$STATE/data
BIND=$DATA/config/resources
export IGN_DEPLOY_RIG_RESOURCES=$BIND
export IGN_DEPLOY_RIG_STATE=$STATE
IGN=${IGN_BIN:-ign}
CAPTURED=$REPO/crates/ignition-core/tests/fixtures/deploy/captured
export CAPTURE23=$HERE/capture23.sh BIND
TMPD=$(mktemp -d)
trap '/bin/rm -rf "$TMPD"' EXIT

# shellcheck disable=SC1091
[ -f "$STATE/tokens.env" ] && . "$STATE/tokens.env"

SCAN=/data/api/v1/scan/config
LOCK=/data/api/v1/scan-lock/config
RES=/data/api/v1/resources
PENDING=/data/api/v1/restart-tasks/pending
CRES=/usr/local/bin/ignition/data/config/resources
JSON=(-H 'Content-Type: application/json')

utc() { date -u +%FT%TZ; }
now_ms() { perl -MTime::HiRes=time -e 'printf "%d", time*1000'; }
h() { printf '\n## %s\n' "$1" >> "$OUT"; }
sub() { printf '\n### %s\n' "$1" >> "$OUT"; }
# prose TEXT: a line of script-written commentary (never gateway bytes).
prose() { printf '\n%s\n' "$1" >> "$OUT"; }

# sect ID [TEXT]: the `## ID` heading (exactly the id, so the section is found
# by name) and, when TEXT is given, one line of commentary under it.
sect() {
  local title
  case "${1:-}" in
    C-sysname | C-restart | C-gwbk | C11 | C2) title=$1 ;;
    C2b) title='C2b (manual)' ;;
    *) echo "sect: unknown capture id '${1:-}'" >&2; exit 64 ;;
  esac
  h "$title"
  [ -z "${2:-}" ] || prose "$2"
}

dc() { ( cd "$HERE" && docker compose -p ign-deploy-rig -f compose.yml -f compose.bind.yml "$@" ); }

# cap LABEL METHOD PATH [curl args]: one verbatim HTTP exchange. A line with
# the command and a script-taken UTC time precedes the fence. CAP_TOK_VAR names
# the token variable (default IGN_TOKEN_CAP_A; NONE sends no token). The
# command line names the variable, never its value.
LAST_BODY=""
LAST_STATUS=""
cap() {
  local label=$1 method=$2 path=$3; shift 3
  local tv=${CAP_TOK_VAR:-IGN_TOKEN_CAP_A} tok="" started shown
  local raw=$TMPD/raw
  local auth=()
  if [ "$tv" = NONE ]; then
    shown='no token'
  else
    tok=${!tv:?$tv is not set; run setup23 and source tokens.env}
    auth=(-H "X-Ignition-API-Token: $tok")
    shown="token \$$tv"
  fi
  started=$(utc)
  curl -sS -g -i -m 90 -w '\n[curl http=%{http_code} total=%{time_total}s]\n' \
    -X "$method" "${auth[@]}" "$@" "$BASE$path" > "$raw" || true
  {
    printf '\n#### %s\n\n`curl -sS -g -i -X %s [%s] %s%s` @ %s\n\n~~~\n' \
      "$label" "$method" "$shown" "${*:+$* }" "'\$BASE$path'" "$started"
    tr -d '\r' < "$raw"
    printf '~~~\n'
  } >> "$OUT"
  LAST_STATUS=$(sed -n 's/^\[curl http=\([0-9]*\) .*/\1/p' "$raw" | tail -1)
  LAST_BODY=$(tr -d '\r' < "$raw" | sed '1,/^$/d' | sed '/^\[curl http=/d')
}

# shcap LABEL CMD [ARG ...]: one verbatim local command block, run with the
# bind directory as cwd. Refuses (exit 2) when an argument or the output holds
# a token value; nothing is appended in that case.
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
  local out=$TMPD/shcap.out rc=0 started
  started=$(utc)
  ( cd "$BIND" 2>/dev/null || true; "$@" ) > "$out" 2>&1 || rc=$?
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
  {
    printf '\n#### %s\n\n`' "$label"
    printf '%q ' "$@"
    printf '` @ %s\n\n~~~\n' "$started"
    cat "$out.nocr"
    if [ -s "$out.nocr" ] && [ "$(tail -c1 "$out.nocr" | wc -l | tr -d ' ')" = 0 ]; then
      printf '\n[output did not end with a newline]\n'
    fi
    printf '[exit %s]\n~~~\n' "$rc"
  } >> "$OUT"
}

wait_running() {
  local t0; t0=$(date +%s)
  until curl -s -m 5 "$BASE/StatusPing" | grep -q RUNNING; do
    [ $(( $(date +%s) - t0 )) -lt 900 ] || { echo "gateway not RUNNING within 900 s" >&2; return 1; }
    sleep 3
  done
}

scan_state() { curl -sS -m 10 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$SCAN" || true; }

# scan23: wait out any active scan, POST scan/config (token A), poll until
# scanActive is false and lastScanTimestamp advanced (the Phase 20 rule), then
# record the final GET. The body of that GET stays in $LAST_BODY.
scan23() {
  local body ts0 ts t0
  t0=$(date +%s)
  while :; do
    body=$(scan_state)
    [ "$(jq -r '.scanActive' <<<"$body" 2>/dev/null || echo ?)" = false ] && break
    [ $(( $(date +%s) - t0 )) -lt 60 ] || { echo "scan stayed active for 60 s before the request" >&2; return 1; }
    sleep 0.5
  done
  ts0=$(jq -r '.lastScanTimestamp // 0' <<<"$body")
  cap 'POST scan/config' POST $SCAN
  t0=$(date +%s)
  while :; do
    body=$(scan_state)
    ts=$(jq -r '.lastScanTimestamp // 0' <<<"$body" 2>/dev/null || echo 0)
    if [ "$(jq -r '.scanActive' <<<"$body" 2>/dev/null || echo ?)" = false ] && [ "$ts" -gt "$ts0" ] 2>/dev/null; then break; fi
    [ $(( $(date +%s) - t0 )) -lt 120 ] || { echo "scan did not complete within 120 s" >&2; return 1; }
    sleep 0.5
  done
  cap 'GET scan/config (after completion)' GET $SCAN
}

# chown23: give the gateway user the whole bind directory (Phase 21 D11).
chown23() { dc exec -T -u root gw chown -R 2003:2003 "$CRES"; }

# newuuid: a lowercase v4 uuid.
newuuid() { python3 -I -c 'import uuid; print(uuid.uuid4())'; }

# write_dbconn NAME [UUID]: a database-connection with a uuid in production/
# (a uuid-less one is refused at load, Phase 22 CV1b).
write_dbconn() {
  local d=$BIND/production/ignition/database-connection/$1 u=${2:-$(newuuid)}
  mkdir -p "$d"
  cp "$HERE/db-connection-config.json" "$d/config.json"
  printf '{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["config.json"],"attributes":{"uuid":"%s","enabled":true}}\n' "$u" > "$d/resource.json"
}

# ---------------------------------------------------------------- bring-up

up23() { bash "$HERE/capture22.sh" up22; }

# tokens23: cap-a and cap-b (mint-token.sh, Authenticated > Deploy), then
# cap-ro, an API token with no security levels (for C-restart: what a token
# without the permission is answered). Keys go to tokens.env only.
tokens23() {
  bash "$HERE/mint-token.sh" cap-a cap-b > /dev/null
  # shellcheck disable=SC1091
  . "$STATE/tokens.env"
  local d key b64 pad hash now
  d=$BIND/external/ignition/api-token/cap-ro
  mkdir -p "$d"
  key=$(openssl rand 32 | openssl base64 -A | tr '+/' '-_' | tr -d '=')
  b64=$(printf '%s' "$key" | tr -- '-_' '+/'); pad=$(( (4 - ${#b64} % 4) % 4 )); b64="$b64$(printf '%*s' "$pad" '' | tr ' ' '=')"
  hash=$(printf '%s' "$b64" | openssl base64 -d -A | openssl dgst -sha256 -binary | openssl base64 -A | tr -- '+/' '-_' | tr -d '=')
  now="$(date +%s)000"
  printf '{"profile":{"secureChannelRequired":false,"securityLevels":[],"timestamp":%s,"type":"basic-token"},"settings":{"tokenHash":"%s"}}\n' "$now" "$hash" > "$d/config.json"
  printf '%s\n' '{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["config.json"],"attributes":{"enabled":true}}' > "$d/resource.json"
  umask 077
  printf 'export IGN_TOKEN_CAP_RO=%s:%s\n' cap-ro "$key" >> "$STATE/tokens.env"
  # shellcheck disable=SC1091
  . "$STATE/tokens.env"
  sleep 20
  wait_running
  chown23
  curl -sS -m 30 -o /dev/null -X POST -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$SCAN"
  sleep 8
  local v code
  for v in IGN_TOKEN_CAP_A IGN_TOKEN_CAP_B IGN_TOKEN_CAP_RO; do
    code=$(curl -sS -m 10 -o /dev/null -w '%{http_code}' -H "X-Ignition-API-Token: ${!v}" "$BASE/data/api/v1/gateway-info" || true)
    echo "$v: gateway-info answers $code"
  done
}

setup23() { bash "$HERE/capture22.sh" setup22; tokens23; }

# header23: write the header table (creates the file; refuses if it exists).
header23() {
  [ -f "$STATE/start-utc-22" ] || { echo "no start-utc-22: run up23 first" >&2; exit 1; }
  [ ! -e "$OUT" ] || { echo "REFUSING: $OUT already exists" >&2; exit 1; }
  curl -s -m 5 "$BASE/StatusPing" | grep -q RUNNING || { echo "gateway is not RUNNING" >&2; exit 1; }
  local mode image cid
  mode=$(curl -sS -m 10 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE/data/api/v1/gateway-info" | jq -r '.deploymentMode')
  [ "$mode" = production ] || { echo "deploymentMode is '$mode', not production" >&2; exit 1; }
  cid=$(docker ps -q --filter "label=com.docker.compose.project=ign-deploy-rig" --filter "label=com.docker.compose.service=gw")
  image=$(docker inspect -f '{{.Config.Image}}' "$cid")
  {
    printf '# Phase 23 — Live Captures (8.3.6 rig)\n\n| Field | Value |\n| --- | --- |\n'
    printf '| Image | %s |\n' "$image"
    printf '| Compose variant | bind (compose.yml + compose.bind.yml) |\n'
    # shellcheck disable=SC2016
    printf '| Bind path (outside repo) | %s |\n' "${BIND/#$HOME/\$HOME}"
    printf '| Gateway name (compose -n) | ign-deploy-rig |\n'
    printf '| deploymentMode | %s |\n' "$mode"
    printf '| Rig started (UTC) | %s |\n' "$(cat "$STATE/start-utc-22")"
    printf '| Runner | capture23.sh (this directory: crates/ignition-core/tests/fixtures/live-rig-deploy) |\n'
    printf '| ign --version | %s |\n' "$("$IGN" --version)"
    printf '\nRule: every captured block is preceded by the exact command and a UTC time taken by the driver just before it. A request line names the token variable it used, never the token. Tokens live in a 0600 file outside the repo.\n'
  } > "$OUT"
}

# hdrrow KEY VALUE: add a row to the header table (script-written).
hdrrow() {
  python3 -I - "$OUT" "$1" "$2" <<'PY'
import sys
p, k, v = sys.argv[1:4]
lines = open(p, encoding="utf-8").read().split("\n")
last = max(i for i, l in enumerate(lines[:20]) if l.startswith("|"))
lines.insert(last + 1, "| %s | %s |" % (k, v))
open(p, "w", encoding="utf-8").write("\n".join(lines))
PY
}

# ------------------------------------------------------------- C-sysname

csysname() {
  sect C-sysname 'The wrong-target rule can compare the REST gateway-info `name` with the host `systemName` (research Open Question 1). Both are read from one fresh rig: the REST answer, the value of `systemName` in the bind-mounted file of the core collection, and the same value through the REST singleton read.'
  cap 'GET gateway-info (token A)' GET /data/api/v1/gateway-info
  shcap 'systemName in core/ignition/system-properties/config.json (the one value, not the file)' \
    python3 -I -c 'import json; d = json.load(open("core/ignition/system-properties/config.json")); print(json.dumps({"systemName": d.get("systemName", "<absent>")}))'
  shcap 'which collections carry an ignition/system-properties directory on the bind mount' \
    bash -c 'ls -d */ignition/system-properties 2>/dev/null || echo "<none>"'
  cap 'GET resources/singleton/ignition/system-properties (the REST view, defaultIfUndefined)' GET "$RES/singleton/ignition/system-properties?defaultIfUndefined=true"
}

# ------------------------------------------------------------- C-restart

crestart() {
  local mod=com.inductiveautomation.sfc
  sect C-restart 'GET restart-tasks/pending (DAPL-10, DAPI-06). The OpenAPI declares `{"pending": [string]}`. Recorded: nothing pending; with no token and with a token that holds no security level; then after a restart-requiring change the rig can make (a module disabled through the REST toggle-state route); then after the change is reversed.'
  sub 'C-restart-1 — nothing pending'
  cap 'GET restart-tasks/pending, token A (Authenticated > Deploy)' GET $PENDING
  CAP_TOK_VAR=IGN_TOKEN_CAP_B cap 'GET restart-tasks/pending, token B' GET $PENDING
  sub 'C-restart-2 — a token without the permission, and no token'
  CAP_TOK_VAR=IGN_TOKEN_CAP_RO cap 'GET restart-tasks/pending, token cap-ro (an API token with no security levels)' GET $PENDING
  CAP_TOK_VAR=NONE cap 'GET restart-tasks/pending, no token' GET $PENDING
  sub 'C-restart-3 — a restart-requiring change'
  cap 'GET modules/healthy (is the candidate module present?)' GET /data/api/v1/modules/healthy
  cap "PUT modules/toggle-state: disable $mod" PUT /data/api/v1/modules/toggle-state "${JSON[@]}" --data "{\"disable\":[\"$mod\"]}"
  sleep 3
  cap 'GET restart-tasks/pending after the disable' GET $PENDING
  sub 'C-restart-4 — the change reversed'
  cap "PUT modules/toggle-state: enable $mod" PUT /data/api/v1/modules/toggle-state "${JSON[@]}" --data "{\"enable\":[\"$mod\"]}"
  sleep 3
  cap 'GET restart-tasks/pending after the enable' GET $PENDING
}

# ---------------------------------------------------------------- C-gwbk

cgwbk() {
  local gw=$TMPD/gwbk hdr=$TMPD/tokhdr cfg=$TMPD/ign-config.toml
  sect C-gwbk 'A gateway backup downloaded from the rig into a temp directory outside the repo. Only size, time, hash, headers and the entry listing are recorded; the file is deleted afterwards and never enters the repo. `type=all` is the type D-13 uses.'
  mkdir -p "$gw"
  umask 077
  printf 'X-Ignition-API-Token: %s\n' "$IGN_TOKEN_CAP_A" > "$hdr"
  umask 022
  export IGNITION_CLI_CONFIG=$cfg IGN_C23_TOKEN=$IGN_TOKEN_CAP_A
  sub 'C-gwbk-1 — the download route, response headers only (the body is discarded)'
  shcap 'curl of GET backup?type=all: response headers, status, bytes counted, time' \
    bash -c 'curl -sS -m 300 -D - -o /dev/null -w "[curl http=%{http_code} size_download=%{size_download} total=%{time_total}s]\n" -H @"$0" "$1/data/api/v1/backup?type=all"' "$hdr" "$BASE"
  sub 'C-gwbk-2 — ign backup download through a profile for the rig'
  shcap 'a throwaway ign config with a profile for the rig (token read from an env var)' \
    "$IGN" profile add ign-deploy-rig "$BASE" --token-env IGN_C23_TOKEN --active --json
  shcap 'ign backup download --type all, wall-clock time' \
    bash -c 's=$(perl -MTime::HiRes=time -e "printf q(%d), time*1000"); "$0" backup download --profile ign-deploy-rig --type all -o "$1" --json; rc=$?; e=$(perl -MTime::HiRes=time -e "printf q(%d), time*1000"); echo "elapsed_ms=$((e - s)) ign_exit=$rc"' "$IGN" "$gw/rig.gwbk"
  sub 'C-gwbk-3 — what the file is'
  shcap 'size, first four bytes, type, sha256 of the original' \
    bash -c 'f=$1; echo "size_bytes=$(wc -c < "$f" | tr -d " ")"; echo "first4=$(head -c 4 "$f" | xxd -p)"; file -b "$f"; shasum -a 256 "$f" | cut -d" " -f1' _ "$gw/rig.gwbk"
  shcap 'unzip -l: entry names and sizes (first 60 lines and the totals line)' \
    bash -c 'unzip -l "$1" | { head -60; } ; echo "..."; unzip -l "$1" | tail -2' _ "$gw/rig.gwbk"
  shcap 'unzip -tq of the whole file' unzip -tq "$gw/rig.gwbk"
  shcap 'python zipfile: is_zipfile, entry count, testzip (None means every CRC reads)' \
    python3 -I -c 'import sys, zipfile; p = sys.argv[1]; print("is_zipfile", zipfile.is_zipfile(p)); z = zipfile.ZipFile(p); print("entries", len(z.namelist())); print("testzip", z.testzip())' "$gw/rig.gwbk"
  sub 'C-gwbk-4 — a copy truncated to half its size'
  shcap 'head -c half of the original into trunc.gwbk, then its size' \
    bash -c 'f=$1; n=$(( $(wc -c < "$f") / 2 )); head -c "$n" "$f" > "$2"; echo "truncated_to_bytes=$(wc -c < "$2" | tr -d " ")"' _ "$gw/rig.gwbk" "$gw/trunc.gwbk"
  shcap 'unzip -tq of the truncated copy' unzip -tq "$gw/trunc.gwbk"
  shcap 'unzip -l of the truncated copy (first 5 lines)' bash -c 'unzip -l "$1" 2>&1 | head -5' _ "$gw/trunc.gwbk"
  shcap 'python zipfile on the truncated copy: is_zipfile and the error ZipFile raises' \
    python3 -I -c 'import sys, zipfile; p = sys.argv[1]; print("is_zipfile", zipfile.is_zipfile(p))
try:
    zipfile.ZipFile(p)
    print("opened")
except Exception as e:
    print(type(e).__name__, str(e))' "$gw/trunc.gwbk"
  sub 'C-gwbk-5 — cleanup'
  /bin/rm -rf "$gw"
  shcap 'the temp directory after deletion' bash -c 'ls "$1" 2>&1 || true' _ "$gw"
}

# ------------------------------------------------------------------- C11

# c11probe: one compact line: is provider CAP21 in the REST list, and which
# tag names does a REST export of it show. Run through shcap while waiting.
c11probe() {
  local list exp inlist names
  list=$(curl -sS -m 20 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$RES/list/ignition/tag-provider?limit=-1" || true)
  inlist=$(jq -r '[.items[]? | .name] | index("CAP21") | if . == null then "no" else "yes" end' <<<"$list" 2>/dev/null || echo "unreadable")
  exp=$(curl -sS -m 20 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE/data/api/v1/tags/export?provider=CAP21&type=json" || true)
  names=$(jq -c '[.. | objects | select(has("name")) | .name]' <<<"$exp" 2>/dev/null || echo "unreadable")
  echo "provider_CAP21_in_rest_list=$inlist export_tag_names=$names"
}

# c11_ok: 0 when CAP21 is in the provider list and the export shows BigInt.
c11_ok() {
  local list exp
  list=$(curl -sS -m 20 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$RES/list/ignition/tag-provider?limit=-1" || true)
  exp=$(curl -sS -m 20 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE/data/api/v1/tags/export?provider=CAP21&type=json" || true)
  jq -e '[.items[]? | .name] | index("CAP21") != null' <<<"$list" > /dev/null 2>&1 \
    && jq -e '[.. | objects | select(has("name")) | .name] | index("BigInt") != null' <<<"$exp" > /dev/null 2>&1
}

c11_record() {
  cap 'GET resources/list/ignition/tag-provider' GET "$RES/list/ignition/tag-provider?limit=-1"
  cap 'GET tags/export?provider=CAP21&type=json (the tags the gateway holds)' GET "/data/api/v1/tags/export?provider=CAP21&type=json"
}

c11() {
  local i ok=no scans=1
  sect C11 'A tag provider and its tag definitions written together into `production/` by a plain file write (the Phase 21 captured CAP21 unit: tag-provider, tag-group, tag-type-definition, tag-definition with FolderA), then ONE config scan. Recorded: whether the provider and its tags are present after that scan, and, if not, after a wait with no scan and after a second scan.'
  shcap 'the CAP21 files written into production/ (relative paths)' \
    bash -c 'mkdir -p production/ignition; for t in tag-provider tag-group tag-type-definition tag-definition; do cp -R "$1/ignition/$t" production/ignition/; done; find production -type f | LC_ALL=C sort' _ "$CAPTURED"
  chown23
  sub 'C11a — one scan'
  scan23
  c11_record
  if c11_ok; then ok=yes; fi
  prose "Script reading after scan 1: provider CAP21 listed and tag BigInt exported: $ok."
  if [ "$ok" = no ]; then
    sub 'C11b — no further scan: poll every 5 s for 30 s'
    for i in 1 2 3 4 5 6; do
      sleep 5
      shcap "probe at +$(( i * 5 )) s" bash "$CAPTURE23" c11probe
      if c11_ok; then ok=yes; break; fi
    done
    prose "Script reading after the wait: present without a second scan: $ok."
  fi
  if [ "$ok" = no ]; then
    sub 'C11c — a second scan'
    scan23
    c11_record
    scans=2
    if c11_ok; then ok=yes; fi
    prose "Script reading after scan 2: provider CAP21 listed and tag BigInt exported: $ok."
  fi
  prose "Script verdict: provider and tags present = $ok after $scans scan(s)."
  sub 'C11d — cleanup: the CAP21 files removed again, then a scan'
  /bin/rm -rf "$BIND"/production/ignition/tag-provider/CAP21 "$BIND"/production/ignition/tag-group/CAP21 \
    "$BIND"/production/ignition/tag-type-definition/CAP21 "$BIND"/production/ignition/tag-definition/CAP21
  find "$BIND/production/ignition" -mindepth 1 -type d -empty -delete 2>/dev/null || true
  scan23
  cap 'GET resources/list/ignition/tag-provider (CAP21 gone?)' GET "$RES/list/ignition/tag-provider?limit=-1"
}

# -------------------------------------------------------------------- C2

# c2poll SECS NAME: every 5 s for SECS seconds, one line: lastScanTimestamp,
# scanActive, whether NAME is in the REST effective view of database-
# connection, and the scan-lock state. No request here triggers a scan.
c2poll() {
  local secs=$1 name=$2 t=0 t0 s body list lk code
  t0=$(date +%s)
  while :; do
    t=$(( $(date +%s) - t0 ))
    s=$(curl -sS -m 10 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$SCAN" || echo '{}')
    list=$(curl -sS -m 20 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$RES/list/ignition/database-connection?limit=-1" || echo '{}')
    code=$(curl -sS -m 10 -o "$TMPD/lk" -w '%{http_code}' -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$LOCK" || echo 000)
    if [ "$code" = 204 ]; then lk=free; else lk="held remaining=$(jq -r '.remaining // "?"' "$TMPD/lk" 2>/dev/null || echo '?')"; fi
    echo "t+${t}s $(utc) lastScanTimestamp=$(jq -r '.lastScanTimestamp // "?"' <<<"$s" 2>/dev/null) scanActive=$(jq -r '.scanActive | tostring' <<<"$s" 2>/dev/null || echo '?') in_rest_view=$(jq -r --arg n "$name" '[.items[]? | .name] | index($n) | if . == null then "no" else "yes" end' <<<"$list" 2>/dev/null || echo '?') scan_lock=$lk"
    [ "$t" -lt "$secs" ] || break
    sleep 5
  done
}

# c2_clean NAME LABEL: scan (any scan ends a held lock), remove every copy of
# NAME on the bind mount, scan again, record the post-state.
c2_clean() {
  local name=$1
  sub "$2 — restore: scan, remove $name from the bind mount, scan, post-state"
  scan23
  shcap "every path of $name on the bind mount before removal" bash -c 'find . -path "*/database-connection/'"$name"'*" | LC_ALL=C sort'
  /bin/rm -rf "$BIND"/*/ignition/database-connection/"$name"
  scan23
  cap "GET scan-lock/config (post-state)" GET $LOCK
  cap "GET resources/list/ignition/database-connection (post-state)" GET "$RES/list/ignition/database-connection?limit=-1"
}

c2() {
  local body=$TMPD/c2a-body.json
  sect C2 'What the config scan-lock blocks (research Pitfall 1, Phase 20 D1/D2/D4/D8/D12). Token A holds the lock; token B and the host filesystem are the other actors. Sub-probes (a), (c), (d), (e); (b), a Designer or gateway-UI save under a held lock, is the manual section C2b. Each sub-probe restores the rig afterwards.'

  sub 'C2a — token A holds the lock (hold 60 s); token B creates a resource through the API'
  scan23
  jq -c --arg n C2_B_WRITE '[{name: $n, config: .}]' "$HERE/db-connection-config.json" > "$body"
  shcap 'the request body of the write by token B' cat "$body"
  cap 'POST scan-lock/config by token A (acquire 5, hold 60)' POST $LOCK "${JSON[@]}" --data '{"acquireTimeout":5,"holdTimeout":60}'
  CAP_TOK_VAR=IGN_TOKEN_CAP_B cap 'GET scan-lock/config by token B, before its write' GET $LOCK
  CAP_TOK_VAR=IGN_TOKEN_CAP_B cap 'POST resources/ignition/database-connection by token B while A holds the lock' POST "$RES/ignition/database-connection" "${JSON[@]}" --data "@$body"
  cap 'GET scan-lock/config by token A, after the write' GET $LOCK
  cap 'GET resources/list/ignition/database-connection (is C2_B_WRITE in the view?)' GET "$RES/list/ignition/database-connection?limit=-1"
  c2_clean C2_B_WRITE 'C2a'

  sub 'C2c — no lock: a resource written into production/ with no lock and no scan request (poll every 5 s for 120 s)'
  scan23
  write_dbconn C2_NOLOCK
  chown23
  shcap 'C2_NOLOCK written into production/ (relative paths)' bash -c 'find production -path "*C2_NOLOCK*" -type f | LC_ALL=C sort'
  shcap 'poll 120 s: lastScanTimestamp, scanActive, visibility, lock, no scan request sent' bash "$CAPTURE23" c2poll 120 C2_NOLOCK
  c2_clean C2_NOLOCK 'C2c'

  sub 'C2d — the same while token A holds the lock (hold 150 s, covers the whole window)'
  scan23
  cap 'POST scan-lock/config by token A (acquire 5, hold 150)' POST $LOCK "${JSON[@]}" --data '{"acquireTimeout":5,"holdTimeout":150}'
  write_dbconn C2_HELD
  chown23
  shcap 'C2_HELD written into production/ while the lock is held (relative paths)' bash -c 'find production -path "*C2_HELD*" -type f | LC_ALL=C sort'
  shcap 'poll 120 s: lastScanTimestamp, scanActive, visibility, lock, no scan request sent' bash "$CAPTURE23" c2poll 120 C2_HELD
  c2_clean C2_HELD 'C2d'

  sub 'C2e — a lock lapses with an unscanned file (hold 15 s, no scan, poll 120 s)'
  scan23
  cap 'POST scan-lock/config by token A (acquire 5, hold 15)' POST $LOCK "${JSON[@]}" --data '{"acquireTimeout":5,"holdTimeout":15}'
  write_dbconn C2_LAPSE
  chown23
  shcap 'C2_LAPSE written into production/ while the lock is held (relative paths)' bash -c 'find production -path "*C2_LAPSE*" -type f | LC_ALL=C sort'
  shcap 'poll 120 s across the lapse: lastScanTimestamp, scanActive, visibility, lock, no scan request sent' bash "$CAPTURE23" c2poll 120 C2_LAPSE
  c2_clean C2_LAPSE 'C2e'

  sect C2b ''
  prose 'NOT RUN BY THE DRIVER. A Designer or gateway-UI save under a held lock is an interactive session. Manual steps:'
  prose '1. With the rig up, hold the lock: `POST /data/api/v1/scan-lock/config` with `{"acquireTimeout":5,"holdTimeout":300}` as token A (source `~/.cache/ign-deploy-rig/tokens.env`; the header carries the token, never print it).'
  prose '2. Open the gateway web UI (http://localhost:18290, admin / the rig password in compose.yml) and save a change to a config resource in the production mode (for example edit a database connection description). Record whether the save succeeds, any refusal text, and the UTC time.'
  prose '3. If a Designer is available, repeat the save from the Designer and record the same.'
  prose '4. `GET /data/api/v1/scan-lock/config` after each save, then end the hold with one `POST /data/api/v1/scan/config` and remove what was saved.'
  prose 'Result: PENDING (human check, owed to the user).'
}

case "${1:-}" in
  up23) up23 ;;
  setup23) setup23 ;;
  tokens23) tokens23 ;;
  header23) header23 ;;
  hdrrow) hdrrow "${2:?KEY}" "${3:?VALUE}" ;;
  sect) sect "${2:?ID}" "${3:-}" ;;
  shcap) shift; shcap "$@" ;;
  csysname) csysname ;;
  crestart) crestart ;;
  cgwbk) cgwbk ;;
  c11) c11 ;;
  c11probe) c11probe ;;
  c2) c2 ;;
  c2poll) c2poll "${2:?SECS}" "${3:?NAME}" ;;
  down23) echo "docker compose -p ign-deploy-rig down -v" ;;
  *) echo "usage: capture23.sh up23|setup23|tokens23|header23|hdrrow|sect|shcap|csysname|crestart|cgwbk|c11|c2|down23" >&2; exit 64 ;;
esac
