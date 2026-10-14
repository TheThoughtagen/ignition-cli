#!/usr/bin/env bash
# capture22.sh SUBCOMMAND [ARG ...]
#
# Phase 22 (22-09) rig capture driver: the three rig facts `deploy verify`
# (22-10) depends on, taken from a real 8.3.6 gateway before verify is written.
#
#   cv1  the REST log entries (message, loggerName, level, timestamp) a scan
#        leaves for a uuid-less named resource, with the scan's
#        lastScanTimestamp and lastScanDuration (research A6)
#   cv2  whether the REST effective view `config` equals the file config.json
#        as a JSON value, per non-tag type present on the rig (research A7)
#   cv3  whether a scan rewrites the bytes of resource files placed by a plain
#        file write into the mode collection (sha256 before/after, 30 s poll)
#
# Subcommands: up22 | setup22 | header22 | hdrrow KEY VALUE | cv1 | cv2 | cv3
#              | sect ID TEXT | shcap LABEL CMD [ARG ...] | down22 (prints the
#              teardown command; it does not run it)
#
# Every byte inside a `## CV<n>` section is appended by `cap` (HTTP) or `shcap`
# (any other command) and is preceded by a line carrying the exact command and
# a UTC time taken by this script just before the run.
#
# LOCALHOST ONLY. assert_local runs before anything else (before any file is
# read or written, before any request) and exits 2 unless BASE's host is
# localhost, 127.0.0.1 or [::1]. The authority is cut at the first `/`, `?`,
# `#` or `\` BEFORE userinfo is stripped, so 'http://evil.example#@localhost'
# is refused (the R-01 / WR-08 fix of Phase 21, applied to this driver).
# Tokens come from tokens.env (0600, outside the repo), are sent as a header
# and are never printed: curl -i prints RESPONSE headers only. Never enable
# `set -x` here.
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

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(git -C "$HERE" rev-parse --show-toplevel)
OUT=${CAPTURE_OUT:-$REPO/.planning/phases/22-transport-plan-pull-verify/22-LIVE-CAPTURES.md}
STATE=${IGN_DEPLOY_RIG_STATE:-$HOME/.cache/ign-deploy-rig}
DATA=$STATE/data
BIND=$DATA/config/resources
export IGN_DEPLOY_RIG_RESOURCES=$BIND
export IGN_DEPLOY_RIG_STATE=$STATE
IGN=${IGN_BIN:-ign}
TRACER=$REPO/crates/ignition-core/tests/fixtures/deploy/repos/tracer
RENDER22=$STATE/render22
CV2_DIR=$STATE/cv2-bodies
export CAPTURE22=$HERE/capture22.sh RENDER22 BIND
TMPD=$(mktemp -d)
trap '/bin/rm -rf "$TMPD"' EXIT

# shellcheck disable=SC1091
[ -f "$STATE/tokens.env" ] && . "$STATE/tokens.env"

SCAN=/data/api/v1/scan/config
RES=/data/api/v1/resources
CRES=/usr/local/bin/ignition/data/config/resources
GATEWAY=ign-deploy-rig
JSON=(-H 'Content-Type: application/json')

utc() { date -u +%FT%TZ; }
h() { printf '\n## %s\n' "$1" >> "$OUT"; }
sub() { printf '\n### %s\n' "$1" >> "$OUT"; }
# prose TEXT: a line of script-written commentary (never gateway bytes).
prose() { printf '\n%s\n' "$1" >> "$OUT"; }

sect() {
  case "${1:-}" in
    CV1 | CV2 | CV3) ;;
    *) echo "sect: unknown capture id '${1:-}'" >&2; exit 64 ;;
  esac
  h "$1 — ${2:-}"
}

dc() { ( cd "$HERE" && docker compose -p ign-deploy-rig -f compose.yml -f compose.bind.yml "$@" ); }

# cap LABEL METHOD PATH [curl args]: one verbatim HTTP exchange. A line with
# the command and a script-taken UTC time precedes the fence. The token is
# written as the literal text $IGN_TOKEN_CAP_A, never its value.
LAST_BODY=""
LAST_STATUS=""
cap() {
  local label=$1 method=$2 path=$3; shift 3
  local tok=${IGN_TOKEN_CAP_A:?IGN_TOKEN_CAP_A is not set; run setup22 and source tokens.env}
  local raw=$TMPD/raw started
  started=$(utc)
  curl -sS -g -i -m 90 -w '\n[curl http=%{http_code} total=%{time_total}s]\n' \
    -X "$method" -H "X-Ignition-API-Token: $tok" "$@" "$BASE$path" > "$raw" || true
  {
    printf '\n#### %s\n\n`curl -sS -g -i -X %s -H %s %s%s` @ %s\n\n~~~\n' \
      "$label" "$method" "'X-Ignition-API-Token: \$IGN_TOKEN_CAP_A'" "${*:+$* }" "'\$BASE$path'" "$started"
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
    for v in IGN_TOKEN_CAP_A IGN_TOKEN_CAP_B; do
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
  for v in IGN_TOKEN_CAP_A IGN_TOKEN_CAP_B; do
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

# scan22: wait out any active scan, POST scan/config, poll until scanActive is
# false and lastScanTimestamp advanced, then record the final GET. The body of
# that GET stays in $LAST_BODY (lastScanTimestamp, lastScanDuration).
scan22() {
  local body ts0 ts t0
  t0=$(date +%s)
  while :; do
    body=$(scan_state)
    [ "$(jq -r '.scanActive' <<<"$body" 2>/dev/null || echo ?)" = false ] && break
    [ $(( $(date +%s) - t0 )) -lt 60 ] || { echo "scan stayed active for 60 s before the request" >&2; return 1; }
    sleep 0.5
  done
  ts0=$(jq -r '.lastScanTimestamp // 0' <<<"$body")
  cap 'GET scan/config (before the scan)' GET $SCAN
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

# chown22: give the gateway user the whole bind directory (Phase 21 D11).
chown22() { dc exec -T -u root gw chown -R 2003:2003 "$CRES"; }

# up22: fresh bind directory under $STATE/data (outside the repo), compose up
# with the bind override, wait RUNNING.
up22() {
  case "$DATA" in
    "$HOME/.cache/ign-deploy-rig/"?*) ;;
    *) echo "REFUSING: data dir '$DATA' is not under \$HOME/.cache/ign-deploy-rig/" >&2; exit 2 ;;
  esac
  [ -e "$DATA" ] && /bin/rm -rf "$DATA"
  mkdir -p "$BIND"
  utc > "$STATE/start-utc-22"
  dc up -d
  wait_running
}

# setup22: mint tokens, then put the gateway into production mode (Phase 20
# C-mode procedure) and confirm deploymentMode. No capture block is written.
setup22() {
  bash "$HERE/mint-token.sh" cap-a > /dev/null
  # shellcheck disable=SC1091
  . "$STATE/tokens.env"
  sleep 20
  wait_running
  local st
  st=$(curl -sS -m 30 -o /dev/null -w '%{http_code}' -X POST -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "${JSON[@]}" \
    --data '{"name":"production","title":"Production","description":"ign-deploy-rig"}' "$BASE/data/api/v1/mode")
  [ "$st" = 200 ] || { echo "POST /mode production answered $st" >&2; exit 1; }
  IGN_DEPLOY_RIG_MODE_ARG=-Dignition.config.mode=production dc up -d
  sleep 10
  wait_running
  local t0 m; t0=$(date +%s)
  while :; do
    m=$(curl -sS -m 10 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE/data/api/v1/gateway-info" 2>/dev/null | jq -r '.deploymentMode' 2>/dev/null || true)
    [ "$m" = production ] && break
    [ $(( $(date +%s) - t0 )) -lt 120 ] || { echo "deploymentMode is '$m', not production" >&2; exit 1; }
    sleep 3
  done
  echo "RUNNING, deploymentMode production"
}

# header22: write the header table (creates the file; refuses if it exists).
header22() {
  [ -f "$STATE/start-utc-22" ] || { echo "no start-utc-22: run up22 first" >&2; exit 1; }
  [ ! -e "$OUT" ] || { echo "REFUSING: $OUT already exists" >&2; exit 1; }
  curl -s -m 5 "$BASE/StatusPing" | grep -q RUNNING || { echo "gateway is not RUNNING" >&2; exit 1; }
  local mode image cid
  mode=$(curl -sS -m 10 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE/data/api/v1/gateway-info" | jq -r '.deploymentMode')
  [ "$mode" = production ] || { echo "deploymentMode is '$mode', not production" >&2; exit 1; }
  cid=$(docker ps -q --filter "label=com.docker.compose.project=ign-deploy-rig" --filter "label=com.docker.compose.service=gw")
  image=$(docker inspect -f '{{.Config.Image}}' "$cid")
  {
    printf '# Phase 22 — Live Captures (8.3.6 rig)\n\n| Field | Value |\n| --- | --- |\n'
    printf '| Image | %s |\n' "$image"
    printf '| Compose variant | bind (compose.yml + compose.bind.yml) |\n'
    # shellcheck disable=SC2016
    printf '| Bind path (outside repo) | %s |\n' "${BIND/#$HOME/\$HOME}"
    printf '| Gateway name | %s |\n' "$GATEWAY"
    printf '| deploymentMode | %s |\n' "$mode"
    printf '| Started (UTC) | %s |\n' "$(cat "$STATE/start-utc-22")"
    printf '| ign --version | %s |\n' "$("$IGN" --version)"
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

# render22: render the tracer repo for mode `production` into $RENDER22 (the
# `ignition/...` tree plus render.json). Local files only; no request.
render22() {
  local t=$TMPD/tracer
  /bin/rm -rf "$t" "$RENDER22"
  cp -R "$TRACER" "$t"
  sed 's/^mode: dev$/mode: production/' "$t/targets/dev.yaml" > "$t/targets/prod.yaml"
  "$IGN" deploy render --env prod --repo "$t" --out "$RENDER22" --json > /dev/null
}

# LOG_SLACK_MS is the slack below the scan START in the CV1 window: the log
# entries of a scan are stamped AFTER its lastScanTimestamp (CV1d), so the
# start of the scan (lastScanTimestamp - lastScanDuration) is already early.
LOG_SLACK_MS=1000
# LOG_SETTLE_S is the pause between "scan complete" and the first log read:
# the entries became readable 183-470 ms after the scan reported complete in
# CV1d, so four times the worst case.
LOG_SETTLE_S=2

now_ms() { perl -MTime::HiRes=time -e 'printf "%d", time*1000'; }

# logwin TS DUR: the log query start (gateway clock) for a scan that ended at
# TS and lasted DUR ms.
logwin() { echo $(( $1 - $2 - LOG_SLACK_MS )); }

# window_note LABEL: after scan22, compute and announce the window.
WIN_START=0
CV1B_START=0
window_note() {
  local ts dur
  ts=$(jq -r '.lastScanTimestamp' <<<"$LAST_BODY"); dur=$(jq -r '.lastScanDuration' <<<"$LAST_BODY")
  WIN_START=$(logwin "$ts" "$dur")
  prose "$1 lastScanTimestamp $ts, lastScanDuration $dur, so the window start is $ts - $dur - $LOG_SLACK_MS = $WIN_START (all on the gateway clock). The script then waits $LOG_SETTLE_S s before the first log read."
  sleep "$LOG_SETTLE_S"
}

# write_nouuid_db NAME: a database-connection with no uuid, in production/.
write_nouuid_db() {
  mkdir -p "$BIND/production/ignition/database-connection/$1"
  cp "$HERE/db-connection-config.json" "$BIND/production/ignition/database-connection/$1/config.json"
  printf '%s\n' '{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["config.json"],"attributes":{"enabled":true}}' > "$BIND/production/ignition/database-connection/$1/resource.json"
}

# lagprobe [N]: N uuid-less database-connections, one scan each; per scan the
# time between "scan complete" and the first readable log entry, and how far
# the entry is stamped after lastScanTimestamp. Run through shcap by cv1.
lagprobe() {
  local n=${1:-5} k name tp t0 s0 s ts0 done seen body hits stamps level first_ts
  for k in $(seq 1 "$n"); do
    name=CV1_LAG_$k
    write_nouuid_db "$name"
    chown22 > /dev/null
    s0=$(scan_state); ts0=$(jq -r '.lastScanTimestamp' <<<"$s0")
    tp=$(now_ms)
    curl -sS -m 30 -o /dev/null -X POST -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$SCAN"
    done=""; seen=""; t0=$(date +%s)
    while [ $(( $(date +%s) - t0 )) -lt 20 ]; do
      s=$(scan_state)
      if [ -z "$done" ] && [ "$(jq -r '.scanActive' <<<"$s")" = false ] && [ "$(jq -r '.lastScanTimestamp' <<<"$s")" -gt "$ts0" ]; then done=$(now_ms); fi
      body=$(curl -sS -m 10 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE/data/api/v1/logs?startTime=$ts0&limit=200")
      hits=$(jq --arg n "$name" '[.items[] | select(.message | contains($n))] | length' <<<"$body")
      if [ -z "$seen" ] && [ "$hits" -gt 0 ]; then seen=$(now_ms); fi
      [ -n "$done" ] && [ -n "$seen" ] && break
      sleep 0.05
    done
    if [ -z "$done" ] || [ -z "$seen" ]; then echo "$name: done='$done' seen='$seen' (no readable entry within 20 s)"; continue; fi
    stamps=$(jq -c --arg n "$name" --argjson t "$(jq -r '.lastScanTimestamp' <<<"$s")" '[.items[] | select(.message | contains($n)) | .timestamp - $t]' <<<"$body")
    level=$(jq -r --arg n "$name" '[.items[] | select(.message | contains($n)) | .level] | unique | join(",")' <<<"$body")
    echo "$name: post_to_scan_complete_ms=$(( done - tp )) lastScanDuration=$(jq -r '.lastScanDuration' <<<"$s") readable_after_complete_ms=$(( seen - done )) entries=$hits level=$level entry_stamp_minus_lastScanTimestamp_ms=$stamps"
  done
}

# cv1 (research A6): the REST log shape and window for "will not be loaded".
cv1() {
  local names
  sect CV1 'REST log entries after a scan loads a uuid-less named resource (research A6)'
  prose 'Resources are written into `production/` by a plain file write (no REST). Scans use the Phase 20 completion rule (scanActive false and lastScanTimestamp advanced). The log window start is computed by the script from the same scan answer.'
  render22

  sub 'CV1a — a uuid-less database-translator (the tracer POSTGRES unit with attributes.uuid removed)'
  mkdir -p "$BIND/production/ignition/database-translator/CV1_NOUUID"
  cp "$RENDER22/ignition/database-translator/POSTGRES/config.json" "$BIND/production/ignition/database-translator/CV1_NOUUID/config.json"
  jq 'del(.attributes.uuid)' "$RENDER22/ignition/database-translator/POSTGRES/resource.json" > "$BIND/production/ignition/database-translator/CV1_NOUUID/resource.json"
  chown22
  shcap 'resource.json written (no uuid key)' bash -c 'cat production/ignition/database-translator/CV1_NOUUID/resource.json'
  scan22
  window_note 'Scan of CV1a:'
  cap 'GET logs, all levels, window of the CV1a scan' GET "/data/api/v1/logs?startTime=$WIN_START&limit=200"
  cap 'GET resources/list/ignition/database-translator (is CV1_NOUUID loaded?)' GET "$RES/list/ignition/database-translator?limit=-1"
  names=$(jq -r '[.items[]? | .name] | join(" ")' <<<"$LAST_BODY")
  prose "Script reading of the list above: CV1_NOUUID present in the REST view: $(case " $names " in *" CV1_NOUUID "*) echo yes;; *) echo no;; esac)."

  sub 'CV1b — a uuid-less database-connection (the type of the RIG.md line)'
  write_nouuid_db CV1_NOUUID_DB
  chown22
  shcap 'resource.json written (no uuid key)' bash -c 'cat production/ignition/database-connection/CV1_NOUUID_DB/resource.json'
  scan22
  window_note 'Scan of CV1b:'
  CV1B_START=$WIN_START
  cap 'GET logs minLevel=WARN (the query shape 22-10 sends)' GET "/data/api/v1/logs?minLevel=WARN&startTime=$WIN_START&limit=200"
  cap 'GET logs logger=NamedResourceHandler (the logger prefix filter), same window' GET "/data/api/v1/logs?logger=NamedResourceHandler&startTime=$WIN_START&limit=200"
  cap 'GET resources/list/ignition/database-connection (is CV1_NOUUID_DB loaded?)' GET "$RES/list/ignition/database-connection?limit=-1"
  names=$(jq -r '[.items[]? | .name] | join(" ")' <<<"$LAST_BODY")
  prose "Script reading of the list above: CV1_NOUUID_DB present in the REST view: $(case " $names " in *" CV1_NOUUID_DB "*) echo yes;; *) echo no;; esac)."

  sub 'CV1c — a second scan with the resource unchanged and still uuid-less'
  scan22
  window_note 'Scan of CV1c:'
  cap 'GET logs minLevel=WARN, window of the CV1c scan' GET "/data/api/v1/logs?minLevel=WARN&startTime=$WIN_START&limit=200"
  prose 'The same query with the window opened at the start of the CV1b scan, so it covers both scans: it shows whether the second scan logged the resource again.'
  cap 'GET logs minLevel=WARN, window opened at the CV1b start (both scans)' GET "/data/api/v1/logs?minLevel=WARN&startTime=$CV1B_START&limit=200"

  sub 'CV1d — how long after "scan complete" the entry is readable, and where it is stamped'
  shcap 'five uuid-less database-connections, one scan each, polling the logs every 50 ms' bash -c 'bash "$CAPTURE22" lagprobe 5'

  sub 'CV1e — cleanup: the CV1 resources removed again, then a scan'
  /bin/rm -rf "$BIND/production/ignition/database-translator/CV1_NOUUID" "$BIND"/production/ignition/database-connection/CV1_*
  find "$BIND/production/ignition" -mindepth 1 -type d -empty -delete 2>/dev/null || true
  shcap 'listing of production after the removal (empty directories pruned)' bash -c 'find production -mindepth 1 | LC_ALL=C sort'
  scan22
}

# cv2 (research A7): REST effective-view config vs the file config.json.
cv2() {
  local coll rel module type kind
  sect CV2 'REST effective-view config vs the file config.json, per non-tag type (research A7)'
  prose 'The tracer render is written into `production/` by a plain file write, one scan, then every non-tag `module/type` present in `core` or `production` on the bind directory is read back through the REST list (named types) or singleton route. Only the differing JSON paths are recorded, never values.'
  render22
  mkdir -p "$BIND/production"
  cp -R "$RENDER22/ignition" "$BIND/production/"
  chown22
  shcap 'tracer render files placed in production/' bash -c 'find production -type f | LC_ALL=C sort'
  scan22
  shcap 'non-tag module/type directories present in core and production' bash -c '
    for c in core production; do
      [ -d "$c" ] || continue
      find "$c" -mindepth 2 -maxdepth 2 -type d | while IFS= read -r d; do
        r=${d#*/}
        case "$r" in ignition/tag-*) ;; *) echo "$r" ;; esac
      done
    done | LC_ALL=C sort -u'
  local types="$LAST_OUT"
  /bin/rm -rf "$CV2_DIR"; mkdir -p "$CV2_DIR"; : > "$CV2_DIR/verdicts"
  while read -r rel; do
    [ -n "${rel:-}" ] || continue
    module=${rel%%/*}; type=${rel#*/}
    cap "GET resources/list/$rel" GET "$RES/list/$module/$type?limit=-1"
    kind=named
    if [ "$LAST_STATUS" != 200 ]; then
      cap "GET resources/singleton/$rel" GET "$RES/singleton/$module/$type"
      kind=singleton
    fi
    printf '%s\t%s\t%s\n' "$rel" "$kind" "$LAST_STATUS" >> "$CV2_DIR/verdicts"
    printf '%s' "$LAST_BODY" > "$CV2_DIR/$(echo "$rel" | tr '/' '_')"
  done <<<"$types"
  shcap 'per-type equality of REST config and file config.json (differing JSON paths only)' \
    bash -c 'bash "$CAPTURE22" cv2cmp'
}

# cv2cmp: the comparison cv2 records through shcap. Reads the REST bodies cv2
# saved; compares each item's `config` with the config.json of the collection
# the gateway reports as effective, as JSON values under `jq -S`. Prints
# `equal`, `equal after N1` or the differing JSON paths, never a value.
#   N1 = members whose file value is JSON null are dropped from the file side
#        (the REST view omits them).
cv2cmp() {
  cd "$BIND"
  local rel kind status body item name sub coll file a b res
  while IFS=$'\t' read -r rel kind status; do
    body="$CV2_DIR/$(echo "$rel" | tr '/' '_')"
    if [ "$status" != 200 ]; then echo "$rel: no REST read route (HTTP $status on the $kind route); not compared"; continue; fi
    jq -c 'if type == "object" and has("items") then .items[] else . end' "$body" | while IFS= read -r item; do
      name=$(jq -r '.name // empty' <<<"$item")
      coll=$(jq -r '.collection' <<<"$item")
      if [ "$kind" = singleton ] || [ -z "$name" ]; then sub=""; else sub="/$name"; fi
      file="$coll/$rel$sub/config.json"
      if [ ! -f "$file" ]; then
        if [ "$coll" = system ]; then echo "$rel$sub: collection 'system' (gateway-provided, not file-backed); not compared"
        elif [ -f "$coll/$rel$sub/resource.json" ] && [ "$(jq -c '.files' "$coll/$rel$sub/resource.json")" = '[]' ]; then
          echo "$rel$sub: resource.json files [] (no config.json) ($coll): $(jq -nc --argjson c "$(jq -c '.config' <<<"$item")" --argjson a "$(jq -c '.attributes' "$coll/$rel$sub/resource.json")" '
            ($c | keys) as $ck
            | {rest_config_keys: $ck,
               keys_also_in_resource_json_attributes_equal: ([$ck[] as $k | select($a | has($k)) | ($a[$k] == $c[$k])] | all),
               rest_only_keys: [$ck[] as $k | select($a | has($k) | not) | $k]}')"
        else echo "$rel$sub: REST item in collection '$coll' with no file $file"; fi
        continue
      fi
      a=$(jq -S . "$file"); b=$(jq -S '.config' <<<"$item")
      res=$(jq -nc --argjson f "$a" --argjson r "$b" '
        def dn: walk(if type == "object" then with_entries(select(.value != null)) else . end);
        def leaves: . as $x | reduce (paths(type != "object" and type != "array")) as $p ({}; .[$p | map(tostring) | join(".")] = ($x | getpath($p)));
        def subset($f; $r):
          if ($f | type) == "object" then ($r | type) == "object" and ($f | to_entries | all(.key as $k | .value as $v | ($r | has($k)) and subset($v; $r[$k])))
          elif ($f | type) == "array" then
            if ($r | type) != "array" then false
            elif ($f | all(type == "object" and has("name"))) then ($f | all(. as $fe | $r | any(type == "object" and .name == $fe.name and subset($fe; .))))
            else $f == $r end
          else $f == $r end;
        if $f == $r then "equal"
        elif ($f | dn) == $r then
          "equal after N1 (file members with JSON null, absent in REST: " + (([$f | leaves | to_entries[] | select(.value == null) | .key]) | join(", ")) + ")"
        else
          ($f | dn | leaves) as $fl | ($r | leaves) as $rl
          | "DIFFERS " + ({file_is_name_keyed_subset_of_rest: subset($f | dn; $r), value_differs: [$fl | keys[] as $k | select($rl | has($k)) | select($rl[$k] != $fl[$k]) | $k],
                          only_in_rest: [$rl | keys[] as $k | select($fl | has($k) | not) | $k],
                          only_in_file: [$fl | keys[] as $k | select($rl | has($k) | not) | $k]} | tojson)
        end' -r)
      echo "$rel$sub: $res ($coll)"
    done
  done < "$CV2_DIR/verdicts"
}

# cv3: does a scan rewrite the bytes of files placed by a plain file write?
cv3() {
  local i
  sect CV3 'post-scan byte stability of resource files placed by a plain file write into production/'
  prose 'The files of the tracer render (placed by `cv2` with `cp -R`) are hashed from the render directory (the bytes written), then from `production/` after a fresh scan, then every 5 s for 30 s, and finally compared byte for byte.'
  [ -d "$RENDER22/ignition" ] || { echo "run cv2 first (no $RENDER22)" >&2; exit 1; }
  shcap 'sha256 of the rendered files (the bytes that were written)' bash -c 'cd "$RENDER22" && find ignition -type f | LC_ALL=C sort | while IFS= read -r f; do shasum -a 256 "$f"; done'
  scan22
  shcap 'sha256 of the same files in production/ right after the scan' bash -c 'find production/ignition -type f | LC_ALL=C sort | while IFS= read -r f; do shasum -a 256 "$f"; done'
  for i in 1 2 3 4 5 6; do
    sleep 5
    shcap "sha256 in production/ at +$(( i * 5 )) s after the scan" bash -c 'find production/ignition -type f | LC_ALL=C sort | while IFS= read -r f; do shasum -a 256 "$f"; done'
  done
  shcap 'byte comparison: every rendered file against its copy in production/ (cmp)' \
    bash -c 'cd "$RENDER22" && find ignition -type f | LC_ALL=C sort | while IFS= read -r f; do if cmp -s "$f" "$BIND/production/$f"; then echo "identical $f"; else echo "DIFFERS $f"; fi; done'
}

case "${1:-}" in
  up22) up22 ;;
  setup22) setup22 ;;
  header22) header22 ;;
  hdrrow) hdrrow "${2:?KEY}" "${3:?VALUE}" ;;
  sect) sect "${2:?ID}" "${3:-}" ;;
  shcap) shift; shcap "$@" ;;
  cv1) cv1 ;;
  cv2) cv2 ;;
  cv2cmp) cv2cmp ;;
  lagprobe) lagprobe "${2:-5}" ;;
  cv3) cv3 ;;
  down22) echo "docker compose -p ign-deploy-rig down -v" ;;
  *) echo "usage: capture22.sh up22|setup22|header22|hdrrow|sect|shcap|cv1|cv2|cv3|down22" >&2; exit 64 ;;
esac
