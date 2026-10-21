#!/usr/bin/env bash
# capture24.sh SUBCOMMAND [ARG ...]
#
# Phase 24 (24-01) pair rig capture driver: the redundancy facts the backup
# check depends on, taken from a live two-node 8.3.6 pair (compose project
# `ign-pair-rig`, see setup-pair.sh) before any backup-check code exists.
#
#   header24  the file header, `## Captures Owed` and `## Pair rig (24-01)`
#             (does nothing when the file already exists)
#   cp4 LABEL C-P4: both nodes' GET redundancy and GET redundancy/providers
#             verbatim, plus each node's gateway-info name and deploymentMode,
#             under `### C-P4 LABEL`; for the labels `settled` and
#             `end-of-24-01` also which token answers which node (status codes)
#   cp1       C-P1: gw-b lastSyncTimestamp / syncPending / peerConnected / find
#             every 2 s for 120 s after a file-path change on gw-a (scan-lock,
#             files into the bind-mounted production collection, config scan)
#             and after a REST-made change (POST create, then PUT with its
#             signature); a `### C-P4 during-sync` block at the first poll where
#             gw-b syncPending is true
#   cp2       C-P2: sha256 of every file of the changed units and of one
#             untouched unit on both bind directories, equal yes/no
#   cp3       C-P3: POST redundancy/gwaction/resync sent to gw-a (never gw-b),
#             then gw-b status every 2 s until settled or 120 s
#   cleanup24 remove the CAP24_* units from gw-a, scan, wait for gw-b to drop
#             them, then `cp4 after-cleanup`
#   cp5       C-P5: link cut by toggleApproval false on gw-a, gw-b peerConnected
#             and both REST statuses every 2 s for 60 s, then reconnect; the
#             docker-network fallback only when the API cut did not drop the link
#   cp6       C-P6: gw-a and gw-b activityLevel every 5 s for 120 s after a cut,
#             then reconnect and `cp4 end-of-24-01`
#   cp7       C-P7 (24-02): secret listings on both nodes (names only, never a
#             value): the provider inventory, the candidate built-in names, a
#             file provider `cap24-file` and an internal provider `cap24-int`
#             created on gw-a, an unknown provider, a call with no token, a
#             wrong key and a token without the permission; then `CAP24-Name`
#             created in `cap24-file` on gw-a and both nodes' listing every 5 s
#             for 60 s; ends with `cleanup7` (providers and the extra token
#             removed from gw-a, gw-b waited on)
#   cp8       C-P8 (24-02): the sync-exhausted state (TEC-5105 case): gw-b
#             recreated without a mode while gw-a runs `production`; gw-b
#             providers, events and filtered log lines every 15 s until the
#             `max attempt count` line or 20 minutes; then one resync sent to
#             gw-a and 120 s of gw-b state; then gw-b restored and `cp4
#             after-exhaust`. Only GETs go to gw-b.
#   c1poll c3poll c5poll c5link c6poll c7poll c8poll spwait  internal: run
#             through shcap so the poll output is recorded verbatim
#   down24    prints the teardown command; it does not run it
#
# Every byte inside a capture block is appended by `cap` (HTTP), `capjq` (HTTP
# through a jq filter) or `shcap` (any other command) and is preceded by a
# line carrying the exact command and a UTC time taken by this script just
# before the run.
#
# LOCALHOST ONLY. assert_local runs on both base URLs before anything else
# (before any file is read or written, before any request) and exits 2 unless
# the host is localhost, 127.0.0.1 or [::1]. The authority is cut at the first
# `/`, `?`, `#` or `\` BEFORE userinfo is stripped, so
# 'http://evil.example#@localhost' is refused (as in capture22.sh/capture23.sh).
# Tokens come from tokens.env (0600, outside the repo), are sent as a request
# header and are never printed: a command line names the token variable, never
# its value, and a block that holds a token value is refused and not appended.
# Never enable `set -x` here.
set -euo pipefail

BASE_A=${BASE_A:-http://localhost:18300}
BASE_B=${BASE_B:-http://localhost:18301}

assert_local() {
  local name=$1 url=$2 rest authority hostport host
  case "$url" in
    *[[:space:]]* | *[[:cntrl:]]*) echo "REFUSING: $name holds whitespace or a control character" >&2; exit 2 ;;
  esac
  rest=${url#*://}
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
    *) echo "REFUSING: $name host '$host' is not localhost, 127.0.0.1 or ::1" >&2; exit 2 ;;
  esac
}
assert_local BASE_A "$BASE_A"
assert_local BASE_B "$BASE_B"
export BASE_A BASE_B

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(git -C "$HERE" rev-parse --show-toplevel)
OUT=${CAPTURE_OUT:-$REPO/.planning/phases/24-redundant-pair/24-LIVE-CAPTURES.md}
STATE=${IGN_PAIR_RIG_STATE:-$HOME/.cache/ign-pair-rig}
PROJECT=ign-pair-rig
DATA_A=$STATE/data-a
DATA_B=$STATE/data-b
BIND_A=$DATA_A/config/resources
BIND_B=$DATA_B/config/resources
CRES=/usr/local/bin/ignition/data/config/resources
JSON=(-H 'Content-Type: application/json')
export IGN_PAIR_DATA_A=$DATA_A IGN_PAIR_DATA_B=$DATA_B

# shellcheck disable=SC1091
[ -f "$STATE/tokens.env" ] && . "$STATE/tokens.env"
if [ -f "$STATE/mode.env" ]; then
  # shellcheck disable=SC1091
  . "$STATE/mode.env"
fi
export IGN_PAIR_MODE_A=${IGN_PAIR_MODE_A:--Dign.pair.rig.noop=true}
export IGN_PAIR_MODE_B=${IGN_PAIR_MODE_B:--Dign.pair.rig.noop=true}

TMPD=$(mktemp -d)
trap '/bin/rm -rf "$TMPD"' EXIT

utc() { date -u +%FT%TZ; }
now_s() { date +%s; }
h() { printf '\n## %s\n' "$1" >> "$OUT"; }
sub() { printf '\n### %s\n' "$1" >> "$OUT"; }
# prose TEXT: a line of script-written commentary (never gateway bytes).
prose() { printf '\n%s\n' "$1" >> "$OUT"; }

dc() { ( cd "$HERE" && docker compose -p "$PROJECT" -f compose.yml "$@" ); }
cid() { docker ps -q --filter "label=com.docker.compose.project=$PROJECT" --filter "label=com.docker.compose.service=gw-$1"; }

# node_base NODE / node_var NODE: the base URL and the token variable of a node.
node_base() { case "$1" in a) printf '%s' "$BASE_A" ;; b) printf '%s' "$BASE_B" ;; *) echo "bad node '$1' (a or b)" >&2; exit 64 ;; esac; }
node_basevar() { case "$1" in a) printf 'BASE_A' ;; b) printf 'BASE_B' ;; *) echo "bad node '$1' (a or b)" >&2; exit 64 ;; esac; }
node_tokvar() { case "$1" in a) printf 'IGN_PAIR_TOKEN_A' ;; b) printf 'IGN_PAIR_TOKEN_B' ;; *) echo "bad node '$1' (a or b)" >&2; exit 64 ;; esac; }

# refuse_if_token FILE: exit 2, appending nothing, when FILE holds either token's key.
refuse_if_token() {
  local v val key
  for v in IGN_PAIR_TOKEN_A IGN_PAIR_TOKEN_B IGN_PAIR_TOKEN_NOPERM; do
    val=${!v:-}
    if [ -n "$val" ]; then
      key=${val#*:}
      if grep -qF -- "$key" "$1"; then
        echo "REFUSING: output contains the value of $v; nothing appended" >&2
        exit 2
      fi
    fi
  done
}

# cap NODE LABEL METHOD PATH [curl args]: one verbatim HTTP exchange against
# node a (gw-a, $BASE_A) or b (gw-b, $BASE_B). A line with the command and a
# script-taken UTC time precedes the fence. CAP_TOK_VAR overrides the token
# variable (NONE sends no token). The command line names the variable and the
# base URL variable, never a value.
LAST_BODY=""
LAST_STATUS=""
cap() {
  local node=$1 label=$2 method=$3 path=$4; shift 4
  local base tv tok="" started shown bv
  base=$(node_base "$node"); bv=$(node_basevar "$node")
  tv=${CAP_TOK_VAR:-$(node_tokvar "$node")}
  local raw=$TMPD/raw
  local auth=()
  if [ "$tv" = NONE ]; then
    shown='no token'
  else
    tok=${!tv:?$tv is not set; run setup-pair.sh mint and source tokens.env}
    auth=(-H "X-Ignition-API-Token: $tok")
    shown="token \$$tv"
  fi
  started=$(utc)
  curl -sS -g -i -m 90 -w '\n[curl http=%{http_code} total=%{time_total}s]\n' \
    -X "$method" ${auth[@]+"${auth[@]}"} "$@" "$base$path" > "$raw" || true
  refuse_if_token "$raw"
  {
    printf '\n#### %s\n\n`curl -sS -g -i -X %s [%s] %s%s` @ %s\n\n~~~\n' \
      "$label" "$method" "$shown" "${*:+$* }" "'\$$bv$path'" "$started"
    tr -d '\r' < "$raw"
    printf '~~~\n'
  } | sed -E 's/("tokenHash": ?")[^"]+"/\1<redacted>"/g' >> "$OUT"
  LAST_STATUS=$(sed -n 's/^\[curl http=\([0-9]*\) .*/\1/p' "$raw" | tail -1)
  LAST_BODY=$(tr -d '\r' < "$raw" | sed '1,/^$/d' | sed '/^\[curl http=/d')
}

# capjq NODE LABEL PATH JQFILTER: GET through a jq filter; the recorded command
# is the curl piped to the jq filter, the block is the filter's output.
capjq() {
  local node=$1 label=$2 path=$3 filter=$4 base tv tok started bv out=$TMPD/capjq.out
  base=$(node_base "$node"); bv=$(node_basevar "$node"); tv=$(node_tokvar "$node")
  tok=${!tv:?$tv is not set; run setup-pair.sh mint and source tokens.env}
  started=$(utc)
  { curl -sS -g -m 60 -H "X-Ignition-API-Token: $tok" "$base$path" | jq -c "$filter"; } > "$out" 2>&1 || true
  refuse_if_token "$out"
  {
    printf "\\n#### %s\\n\\n\`curl -sS -g [token \$%s] %s | jq -c '%s'\` @ %s\\n\\n~~~\\n" \
      "$label" "$tv" "'\$$bv$path'" "$filter" "$started"
    cat "$out"
    printf '~~~\n'
  } >> "$OUT"
}

# shcap LABEL CMD [ARG ...]: one verbatim local command block. Refuses (exit 2)
# when an argument or the output holds a token value; nothing is appended then.
LAST_OUT=""
shcap() {
  local label=${1:?shcap LABEL CMD [ARG ...]}; shift
  [ $# -gt 0 ] || { echo "shcap: no command given" >&2; exit 2; }
  local a v val key
  for a in "$@"; do
    for v in IGN_PAIR_TOKEN_A IGN_PAIR_TOKEN_B IGN_PAIR_TOKEN_NOPERM; do
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
  "$@" > "$out" 2>&1 || rc=$?
  tr -d '\r' < "$out" > "$out.nocr"
  refuse_if_token "$out.nocr"
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

# status_of NODE PATH [TOKEN_VAR]: the HTTP status only (token var default: the node's own).
status_of() {
  local node=$1 path=$2 tv=${3:-} base tok
  base=$(node_base "$node"); [ -n "$tv" ] || tv=$(node_tokvar "$node")
  tok=${!tv:-}
  curl -sS -g -m 15 -o /dev/null -w '%{http_code}' -H "X-Ignition-API-Token: $tok" "$base$path" 2>/dev/null || printf '000'
}

# rjq NODE FIELD: one field of GET redundancy on a node (empty when unreadable).
rjq() {
  local node=$1 base tv tok
  base=$(node_base "$node"); tv=$(node_tokvar "$node"); tok=${!tv:-}
  curl -sS -g -m 15 -H "X-Ignition-API-Token: $tok" "$base/data/api/v1/redundancy" 2>/dev/null | jq -r ".$2 | if . == null then empty else tostring end" 2>/dev/null || true
}

# --------------------------------------------------------------- header24

header24() {
  if [ -e "$OUT" ]; then
    echo "header24: $OUT already exists; nothing written" >&2
    return 0
  fi
  [ -f "$STATE/start-utc" ] || { echo "no start-utc: run setup-pair.sh up first" >&2; exit 1; }
  local image dockerv composev ca n
  ca=$(cid a)
  [ -n "$ca" ] || { echo "no running gw-a container in compose project $PROJECT" >&2; exit 1; }
  image=$(docker inspect -f '{{.Config.Image}}' "$ca")
  dockerv=$(docker version --format '{{.Server.Version}}' 2>/dev/null || echo '?')
  composev=$(docker compose version --short 2>/dev/null || echo '?')
  mkdir -p "$(dirname "$OUT")"
  {
    printf '# Phase 24 — Live Captures (8.3.6 redundant pair)\n\n| Field | Value |\n| --- | --- |\n'
    printf '| Date (UTC) | %s |\n' "$(date -u +%F)"
    printf '| Image | %s (platform linux/amd64, emulated on an arm64 host) |\n' "$image"
    printf '| Compose project | %s |\n' "$PROJECT"
    printf '| Rig started (UTC) | %s |\n' "$(cat "$STATE/start-utc")"
    printf '| Docker server | %s |\n' "$dockerv"
    printf '| Docker Compose | %s |\n' "$composev"
    # shellcheck disable=SC2016
    printf '| Bind paths (outside repo) | %s and %s |\n' "${BIND_A/#$HOME/\$HOME}" "${BIND_B/#$HOME/\$HOME}"
    printf '| Runner | setup-pair.sh and capture24.sh (crates/ignition-core/tests/fixtures/live-rig-pair) |\n'
    printf '\nRule: every captured block is preceded by the exact command and a UTC time taken by the driver just before it. A request line names the token variable it used and the base URL variable (`$BASE_A` is gw-a, the master; `$BASE_B` is gw-b, the backup), never a token value. Tokens live in a 0600 file outside the repo.\n'
    cat <<'MD'

## Captures Owed

Written before any capture block (decision D-18: capture first). Each row names the question, the CONTEXT decision it settles (and the research assumption or pitfall) and the plan that reads the record.

| ID | Question | Settles | Read by |
| --- | --- | --- | --- |
| C-P1 | Does the backup's `lastSyncTimestamp` advance after a master config change made the way apply makes it (files into the bind-mounted `production` collection, scan-lock, config scan), and after a REST-made change? How soon does the changed resource read back on gw-b? | D-06 timestamp-advance clause (Assumption A1, Pitfall 1) | 24-02, 24-05 |
| C-P2 | After a sync, are the backup's on-disk files for a changed unit and an untouched unit byte-equal to the master's? | D-03 byte equality (Assumption A2, Pitfall 4) | 24-05 |
| C-P3 | What does `POST /data/api/v1/redundancy/gwaction/resync` sent to the master answer (status, headers, body), and what does the backup do next? | D-07 resync shape (Pitfall 9) | 24-03, 24-06 |
| C-P4 | What do `GET /redundancy` and `GET /redundancy/providers` answer on each node when settled, during a sync, after a link cut and after the reconnect? Which token answers on which node? | status field values (Pitfall 5, Assumptions A8, A10) | 24-05 |
| C-P5 | Does `toggleApproval/<id>/false` on the master cut an established link while both REST APIs stay up? How fast does `peerConnected` flip and recover? Does the two-network fallback differ? | D-17 link cut (Pitfall 2, Assumption A5) | 24-10 |
| C-P6 | With the ping settings applied, does a cut backup stay `Cold` or promote itself (`active_node` reads `Both`)? | D-11 versus D-17 (Pitfall 3, Assumption A3) | 24-07, 24-10 |
| C-P7 | Secret listings on both nodes: the internal provider, a file provider, an unknown provider (404), a 501 candidate, a token without permission. | D-13/D-14 listings (Pitfall 6, Assumption A4) | 24-03, 24-07, 24-09 |
| C-P8 | Is the sync-exhausted state (TEC-5105) reachable on the rig, how is it detected (providers, events, log lines), and does a resync alone clear it? | D-10 detection (C12, Assumptions A6, A7) | 24-03, 24-06 |

C-P1 to C-P6 are recorded by 24-01 (this plan). C-P7 and C-P8 are owed to 24-02.
MD
    printf '\n## Pair rig (24-01)\n\n'
    printf '| Field | Value |\n| --- | --- |\n'
    printf '| Compose project | `ign-pair-rig` (compose.yml in this fixture directory) |\n'
    printf '| gw-a | gateway name `ign-pair-a`, hostname `gw-a`, redundancy role Master, REST `127.0.0.1:18300` |\n'
    printf '| gw-b | gateway name `ign-pair-b`, hostname `gw-b`, redundancy role Backup, REST `127.0.0.1:18301` |\n'
    printf '| Networks | `rig` (published REST ports) and `gan` (`internal: true`); gw-a alias `gw-a-gan`, GAN port 8060 with TLS, not published on the host |\n'
    printf '| GAN trust | rejected certificates moved to `trusted/certs` on both nodes (spike recipe), incoming connection approved on gw-a with `PUT /data/api/v1/gateway-network-incoming/toggleApproval/<id>/true` |\n'
    printf '| Deployment mode | production on both nodes (mode created on gw-a, seen on gw-b, then gw-b and gw-a recreated with `-Dignition.config.mode=production`, in that order) |\n'
    printf '| Bind mounts | each node'"'"'s `config/resources` from the host (`data-a`, `data-b`) so master and backup read with a `local:` transport (D-16) |\n'
    printf '| Ping settings applied (`gatewayNetworkSetup`) | pingRate 1000 ms, pingTimeout 1000 ms, pingMaxMissed 1200 (at least 1200 s of missed pings; the backup should not treat the master as faulted inside the capture windows) |\n'
    printf '\n### Redundancy config as applied (read back from each node)\n'
    printf '\nDefaults found before the settings were applied (script output, `pair-before.log`):\n\n~~~\n'
    cat "$STATE/pair-before.log" 2>/dev/null || true
    printf '~~~\n'
  } > "$OUT"
  local n
  for n in a b; do
    capjq "$n" "GET redundancy/config on gw-$n (after pairing)" /data/api/v1/redundancy/config \
      '{role, joinWaitTime, masterRecoveryMode, standbyActivityLevel, syncTimeoutSecs, gatewayNetworkSetup: (.gatewayNetworkSetup | {host, port, enableSsl, pingRate, pingTimeout, pingMaxMissed})}'
  done
  {
    printf '\n### Credential each setup write needed\n\nEvery redundancy-config write with the credential that answered, and every call that needed more than the node'"'"'s own token (`credentials.log`, no secret in it):\n\n~~~\n'
    cat "$STATE/credentials.log" 2>/dev/null || true
    printf '~~~\n'
    printf '\n### Seconds each setup step took\n\n~~~\n'
    cat "$STATE/step-times.log" 2>/dev/null || true
    printf '~~~\n'
  } >> "$OUT"
}

# --------------------------------------------------------------------- cp4

# a10table: which token answers 200 on which node (status codes only).
a10table() {
  local tv n code rows=""
  prose 'Which token answers which node: the HTTP status of `GET /data/api/v1/redundancy` per token and node (status codes only; A10: the spike reused one key on both nodes, the setup mints one per node).'
  {
    printf '\n| token | gw-a (`$BASE_A`) | gw-b (`$BASE_B`) |\n| --- | --- | --- |\n'
    for tv in IGN_PAIR_TOKEN_A IGN_PAIR_TOKEN_B; do
      printf '| `$%s` ' "$tv"
      for n in a b; do
        code=$(status_of "$n" /data/api/v1/redundancy "$tv")
        printf '| %s ' "$code"
      done
      printf '|\n'
    done
  } >> "$OUT"
}

cp4() {
  local label=${1:?cp4 LABEL} n
  if [ -n "${CP4_BARE:-}" ]; then
    : # snapshot written to a side file by a poll; the caller places it
  elif ! grep -q '^## C-P4' "$OUT" 2>/dev/null; then
    h C-P4
    prose 'Redundancy status of both nodes at labelled moments: GET `/data/api/v1/redundancy` and GET `/data/api/v1/redundancy/providers` verbatim, then each node'"'"'s gateway-info `name` and `deploymentMode`. gw-a is the master (`$BASE_A`), gw-b the backup (`$BASE_B`).'
  elif ! grep '^## ' "$OUT" | tail -1 | grep -q '^## C-P4'; then
    h "C-P4 (continued: $label)"
  fi
  sub "C-P4 $label"
  for n in a b; do
    cap "$n" "GET redundancy on gw-$n ($label)" GET /data/api/v1/redundancy
    cap "$n" "GET redundancy/providers on gw-$n ($label)" GET /data/api/v1/redundancy/providers
    capjq "$n" "gateway-info name and deploymentMode on gw-$n ($label)" /data/api/v1/gateway-info '{name, deploymentMode}'
  done
  case "$label" in
    settled | end-of-24-01) a10table ;;
  esac
}

# ------------------------------------------------------------ shared helpers

SELF=$HERE/capture24.sh
SETUP=$HERE/setup-pair.sh
SIDE=$STATE/cp4-during.md
NAMES=$STATE/cp1-names
DBCFG=$HERE/../live-rig-deploy/db-connection-config.json
RESAPI=/data/api/v1/resources
DBT=ignition/database-connection

# has_h ID: true when the section heading already exists in the file.
has_h() { grep -q "^## $1\$" "$OUT" 2>/dev/null; }

# rstate NODE: "lastSyncTimestamp syncPending peerConnected activityLevel projectState"
# ("?" for what cannot be read; spaces in projectState become "_", so Out of Date
# reads Out_of_Date. The first C-P6 run was taken before this and shows "Out".)
rstate() {
  local node=$1 base tv tok out
  base=$(node_base "$node"); tv=$(node_tokvar "$node"); tok=${!tv:-}
  out=$(curl -sS -g -m 8 -H "X-Ignition-API-Token: $tok" "$base/data/api/v1/redundancy" 2>/dev/null \
    | jq -r '[(.lastSyncTimestamp // "?"), (.syncPending | tostring), (.peerConnected | tostring), (.activityLevel // "?"), ((.projectState // "?") | gsub(" "; "_"))] | join(" ")' 2>/dev/null) || out=""
  [ -n "$out" ] || out='? ? ? ? ?'
  printf '%s' "$out"
}

# find_probe NODE NAME: "HTTP:signature12" of GET resources/find/<type>/<name>.
find_probe() {
  local node=$1 name=$2 base tv tok out=$TMPD/find.$1 code sig
  base=$(node_base "$node"); tv=$(node_tokvar "$node"); tok=${!tv:-}
  code=$(curl -sS -g -m 8 -o "$out" -w '%{http_code}' -H "X-Ignition-API-Token: $tok" "$base$RESAPI/find/$DBT/$name" 2>/dev/null) || code=000
  sig=$(jq -r '(.signature // "-") | .[0:12]' "$out" 2>/dev/null) || sig='-'
  printf '%s:%s' "$code" "${sig:--}"
}

ping_of() { # NODE: HTTP status of GET /StatusPing
  curl -sS -g -m 6 -o /dev/null -w '%{http_code}' "$(node_base "$1")/StatusPing" 2>/dev/null || printf '000'
}

settled_now() {
  local b a
  b=$(rstate b); a=$(rstate a)
  set -- $b; local bpend=$2 bpeer=$3 bproj=$5
  set -- $a; local aproj=$5
  [ "$bpeer" = true ] && [ "$bpend" = false ] && [ "$bproj" = Good ] && [ "$aproj" = Good ]
}

# wait_settled SECS: seconds to settled on stdout, return 1 on timeout.
wait_settled() {
  local secs=$1 t0
  t0=$(date +%s)
  until settled_now; do
    [ $(( $(date +%s) - t0 )) -lt "$secs" ] || return 1
    sleep 2
  done
  echo $(( $(date +%s) - t0 ))
}

scan_state() { curl -sS -g -m 10 -H "X-Ignition-API-Token: ${IGN_PAIR_TOKEN_A}" "$BASE_A/data/api/v1/scan/config" 2>/dev/null || true; }
scan_idle() {
  local t0 body
  t0=$(date +%s)
  while :; do
    body=$(scan_state)
    [ "$(jq -r '.scanActive' <<<"$body" 2>/dev/null || echo ?)" = false ] && return 0
    [ $(( $(date +%s) - t0 )) -lt 60 ] || { echo "scan stayed active for 60 s" >&2; return 1; }
    sleep 1
  done
}
scan_ts() { scan_state | jq -r '.lastScanTimestamp // 0' 2>/dev/null || echo 0; }
scan_done() { # TS0
  local ts0=$1 t0 body ts
  t0=$(date +%s)
  while :; do
    body=$(scan_state)
    ts=$(jq -r '.lastScanTimestamp // 0' <<<"$body" 2>/dev/null || echo 0)
    if [ "$(jq -r '.scanActive' <<<"$body" 2>/dev/null || echo ?)" = false ] && [ "$ts" -gt "$ts0" ] 2>/dev/null; then return 0; fi
    [ $(( $(date +%s) - t0 )) -lt 120 ] || { echo "scan did not complete within 120 s" >&2; return 1; }
    sleep 1
  done
}
chown_a() { docker exec -u root "$(cid a)" chown -R 2003:2003 "$CRES"; }
LOCKPATH=/data/api/v1/scan-lock/config
SCANPATH=/data/api/v1/scan/config

# ---------------------------------------------------------------- polls

# c1poll SECS NAME MODE PRE_TS TAKE_DURING: every 2 s, gw-b lastSyncTimestamp,
# syncPending, peerConnected and the find status:signature of NAME on both nodes.
# MODE exists = the change has read back when gw-b answers 200; sig:SIG12 = when
# gw-b answers 200 with that signature. At the first syncPending true and
# TAKE_DURING=yes, `cp4 during-sync` is written to a side file.
c1poll() {
  local secs=$1 name=$2 mode=$3 pre=$4 take=${5:-no}
  local t0 n i target now el ts pend peer act proj fa fb rb="" fp="" ft=""
  t0=$(date +%s); n=$(( secs / 2 ))
  printf '# columns: UTC, elapsed since the poll started, gw-b lastSyncTimestamp, gw-b syncPending, gw-b peerConnected, gw-a find (HTTP:signature12), gw-b find (HTTP:signature12)\n'
  printf '# pre-change gw-b lastSyncTimestamp=%s; read-back rule: %s\n' "$pre" "$mode"
  for (( i = 0; i <= n; i++ )); do
    target=$(( t0 + i * 2 )); now=$(date +%s)
    [ "$now" -ge "$target" ] || sleep $(( target - now ))
    el=$(( $(date +%s) - t0 ))
    read -r ts pend peer act proj <<<"$(rstate b)"
    fa=$(find_probe a "$name"); fb=$(find_probe b "$name")
    printf '%s +%3ss lastSyncTimestamp=%s syncPending=%s peerConnected=%s gwA_find=%s gwB_find=%s\n' "$(utc)" "$el" "$ts" "$pend" "$peer" "$fa" "$fb"
    if [ -z "$rb" ]; then
      case "$mode" in
        exists) case "$fb" in 200:*) rb=$el ;; esac ;;
        sig:*) [ "$fb" = "200:${mode#sig:}" ] && rb=$el ;;
      esac
    fi
    if [ "$pend" = true ] && [ -z "$fp" ]; then
      fp=$el
      if [ "$take" = yes ] && [ ! -e "$SIDE" ]; then
        CAPTURE_OUT=$SIDE CP4_BARE=1 bash "$SELF" cp4 during-sync > /dev/null 2>&1 || true
      fi
    fi
    if [ -z "$ft" ] && [ "$ts" != '?' ] && [ "$ts" != "$pre" ]; then ft=$el; fi
  done
  printf 'SUMMARY first_readback_on_gw-b=%s first_syncPending_true=%s first_lastSyncTimestamp_change=%s (elapsed seconds; empty = never within %s s)\n' "${rb:-never}" "${fp:-never}" "${ft:-never}" "$secs"
}

# c3poll SECS PRE_TS: after the resync, gw-b status every 2 s until settled with an
# advanced timestamp, or SECS.
c3poll() {
  local secs=$1 pre=$2 t0 el ts pend peer act proj bproj fp="" ft="" fs="" done_="" i=0 aproj
  t0=$(date +%s)
  printf '# columns: UTC, elapsed since the poll started, gw-b lastSyncTimestamp, syncPending, peerConnected, activityLevel, gw-b projectState, gw-a projectState, settled\n'
  printf '# pre-resync gw-b lastSyncTimestamp=%s\n' "$pre"
  while :; do
    el=$(( $(date +%s) - t0 ))
    read -r ts pend peer act bproj <<<"$(rstate b)"
    read -r _ _ _ _ aproj <<<"$(rstate a)"
    local st=no
    if [ "$peer" = true ] && [ "$pend" = false ] && [ "$bproj" = Good ] && [ "$aproj" = Good ]; then st=yes; fi
    printf '%s +%3ss lastSyncTimestamp=%s syncPending=%s peerConnected=%s activityLevel=%s gwB_projectState=%s gwA_projectState=%s settled=%s\n' "$(utc)" "$el" "$ts" "$pend" "$peer" "$act" "$bproj" "$aproj" "$st"
    [ "$pend" = true ] && [ -z "$fp" ] && fp=$el
    [ -z "$ft" ] && [ "$ts" != '?' ] && [ "$ts" != "$pre" ] && ft=$el
    [ "$st" = yes ] && [ -z "$fs" ] && fs=$el
    if [ "$st" = yes ] && [ -n "$ft" ]; then done_=yes; break; fi
    [ "$el" -lt "$secs" ] || break
    sleep 2
  done
  if [ -n "$done_" ]; then
    printf 'SUMMARY settled with an advanced lastSyncTimestamp at +%ss (first_syncPending_true=%s first_lastSyncTimestamp_change=%s first_settled=%s)\n' "$el" "${fp:-never}" "${ft:-never}" "${fs:-never}"
  else
    printf 'SUMMARY TIMEOUT after %s s (first_syncPending_true=%s first_lastSyncTimestamp_change=%s first_settled=%s)\n' "$secs" "${fp:-never}" "${ft:-never}" "${fs:-never}"
  fi
}

# c5poll SECS: every 2 s, gw-b peerConnected, gw-a peerConnected, the HTTP status of
# GET /StatusPing on both nodes and of GET /data/api/v1/redundancy on gw-b.
c5poll() {
  local secs=$1 t0 n i target now el b a pb pa ra rb rr first=""
  t0=$(date +%s); n=$(( secs / 2 ))
  printf '# columns: UTC, elapsed, gw-b peerConnected, gw-a peerConnected, gw-a StatusPing HTTP, gw-b StatusPing HTTP, gw-b GET redundancy HTTP, gw-b syncPending\n'
  for (( i = 0; i <= n; i++ )); do
    target=$(( t0 + i * 2 )); now=$(date +%s)
    [ "$now" -ge "$target" ] || sleep $(( target - now ))
    el=$(( $(date +%s) - t0 ))
    b=$(rstate b); a=$(rstate a)
    set -- $b; pb=$3; local bp=$2
    set -- $a; pa=$3
    ra=$(ping_of a); rb=$(ping_of b); rr=$(status_of b /data/api/v1/redundancy)
    printf '%s +%3ss gwB_peerConnected=%s gwA_peerConnected=%s gwA_StatusPing=%s gwB_StatusPing=%s gwB_redundancy_http=%s gwB_syncPending=%s\n' "$(utc)" "$el" "$pb" "$pa" "$ra" "$rb" "$rr" "$bp"
    [ "$pb" = false ] && [ -z "$first" ] && first=$el
  done
  printf 'SUMMARY first_gw-b_peerConnected_false=%s (elapsed seconds; never = it stayed true for %s s)\n' "${first:-never}" "$secs"
}

# c5link METHOD(api|net): restore the link, report the seconds until
# peerConnected true and until settled, both from before the restore command.
c5link() {
  local method=$1 t0 tl ts
  t0=$(date +%s)
  case "$method" in
    api) bash "$SETUP" link ;;
    net) bash "$SETUP" link-net ;;
    *) echo "c5link: api or net" >&2; exit 64 ;;
  esac
  tl=$(( $(date +%s) - t0 ))
  if ts=$(wait_settled 300); then
    printf 'SUMMARY reconnect: peerConnected true %s s and settled %s s after the restore command started\n' "$tl" "$(( $(date +%s) - t0 ))"
  else
    printf 'SUMMARY reconnect: peerConnected true %s s after the restore command started; settled TIMEOUT (300 s)\n' "$tl"
  fi
}

# c6poll SECS STEP: every STEP s, activityLevel / peerConnected of both nodes.
c6poll() {
  local secs=$1 step=$2 t0 n i target now el a b first_b="" first_a=""
  t0=$(date +%s); n=$(( secs / step ))
  printf '# columns: UTC, elapsed, gw-a role-activityLevel, gw-a peerConnected, gw-b activityLevel, gw-b peerConnected, gw-b projectState, gw-a StatusPing, gw-b StatusPing\n'
  for (( i = 0; i <= n; i++ )); do
    target=$(( t0 + i * step )); now=$(date +%s)
    [ "$now" -ge "$target" ] || sleep $(( target - now ))
    el=$(( $(date +%s) - t0 ))
    a=$(rstate a); b=$(rstate b)
    set -- $a; local aact=$4 apeer=$3
    set -- $b; local bact=$4 bpeer=$3 bproj=$5
    printf '%s +%3ss gwA_activityLevel=%s gwA_peerConnected=%s gwB_activityLevel=%s gwB_peerConnected=%s gwB_projectState=%s gwA_StatusPing=%s gwB_StatusPing=%s\n' "$(utc)" "$el" "$aact" "$apeer" "$bact" "$bpeer" "$bproj" "$(ping_of a)" "$(ping_of b)"
    [ "$bact" = Active ] && [ -z "$first_b" ] && first_b=$el
    [ "$aact" != Active ] && [ -z "$first_a" ] && first_a=$el
  done
  if [ -n "$first_b" ]; then
    printf 'SUMMARY gw-b reported Active after %s s of the cut window: the cut backup promoted itself\n' "$first_b"
  else
    printf 'SUMMARY gw-b never reported Active in %s s: the cut backup stayed %s\n' "$secs" "${bact:-?}"
  fi
  printf 'SUMMARY gw-a first non-Active reading: %s\n' "${first_a:-never}"
}

# dropwait NAME...: every 2 s until gw-b answers non-200 for every NAME, bounded 120 s.
dropwait() {
  local t0 el n all code
  t0=$(date +%s)
  while :; do
    el=$(( $(date +%s) - t0 )); all=yes
    printf '%s +%3ss' "$(utc)" "$el"
    for n in "$@"; do
      code=$(find_probe b "$n"); printf ' %s=gwB:%s' "$n" "$code"
      case "$code" in 200:*) all=no ;; esac
    done
    printf '\n'
    [ "$all" = yes ] && { printf 'SUMMARY gw-b dropped every named resource at +%ss\n' "$el"; return 0; }
    [ "$el" -lt 120 ] || { printf 'SUMMARY TIMEOUT: gw-b still lists a resource after 120 s\n'; return 0; }
    sleep 2
  done
}

# ---------------------------------------------------------------------- cp1

cp1() {
  local ts0 epoch sname rname d u body sig newsig
  epoch=$(date +%s)
  sname=CAP24_SYNC_$epoch; rname=CAP24_REST_$epoch
  printf 'sync %s\nrest %s\n' "$sname" "$rname" > "$NAMES"
  /bin/rm -f "$SIDE"
  has_h C-P1 || h C-P1
  prose 'Does gw-b `lastSyncTimestamp` advance after a change on the master, and how soon does the change read back on gw-b? Two write paths: (1) the way apply writes: a scan-lock on gw-a, files into gw-a'"'"'s bind-mounted `production` collection, a config scan; (2) the REST API: `POST` then `PUT` with its signature on `/data/api/v1/resources/ignition/database-connection`. Poll lines: UTC, seconds since the poll started, gw-b `lastSyncTimestamp`, `syncPending`, `peerConnected`, and the `find` result of the changed resource on both nodes (HTTP status and the first 12 characters of the signature).'

  # ---- path 1: files + lock + scan
  sub "C-P1 file path: $sname written into gw-a production/, scan-lock, config scan"
  cap b "GET redundancy on gw-b, before the change" GET /data/api/v1/redundancy
  local pre; pre=$(rjq b lastSyncTimestamp)
  cap a "GET redundancy on gw-a, before the change" GET /data/api/v1/redundancy
  scan_idle
  ts0=$(scan_ts)
  cap a "POST scan-lock/config (acquire 5, hold 60)" POST $LOCKPATH "${JSON[@]}" --data '{"acquireTimeout":5,"holdTimeout":60}'
  d=$BIND_A/production/ignition/database-connection/$sname
  u=$(uuidgen | tr 'A-Z' 'a-z')
  mkdir -p "$d"
  cp "$DBCFG" "$d/config.json"
  printf '{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["config.json"],"attributes":{"uuid":"%s","enabled":true}}\n' "$u" > "$d/resource.json"
  chown_a
  shcap "$sname on gw-a's bind mount (relative paths)" bash -c "cd '$BIND_A' && find production -path '*$sname*' -type f | LC_ALL=C sort"
  cap a "POST scan/config" POST $SCANPATH
  shcap "C-P1 file path poll: 120 s, every 2 s ($sname)" bash "$SELF" c1poll 120 "$sname" exists "$pre" yes
  scan_done "$ts0" || true
  cap a "GET scan-lock/config after the scan" GET $LOCKPATH
  wait_settled 120 > /dev/null || true

  # ---- path 2: REST
  sub "C-P1 REST path: $rname created by POST, changed by PUT with its signature"
  body=$TMPD/c1-post.json
  jq -c --arg n "$rname" '[{name: $n, config: .}]' "$DBCFG" > "$body"
  cap a "POST resources/$DBT (create $rname)" POST "$RESAPI/$DBT" "${JSON[@]}" --data "@$body"
  shcap "C-P1 REST create poll: 30 s, every 2 s ($rname)" bash "$SELF" c1poll 30 "$rname" exists "$(rjq b lastSyncTimestamp)" yes
  wait_settled 120 > /dev/null || true
  cap a "GET find $rname (read the signature)" GET "$RESAPI/find/$DBT/$rname"
  sig=$(jq -r '.signature' <<<"$LAST_BODY")
  body=$TMPD/c1-put.json
  jq -c --arg n "$rname" --arg s "$sig" '[{name: $n, signature: $s, description: "cap24 rest update", config: (. + {poolMaxActive: 9})}]' "$DBCFG" > "$body"
  cap b "GET redundancy on gw-b, before the PUT" GET /data/api/v1/redundancy
  pre=$(rjq b lastSyncTimestamp)
  cap a "PUT resources/$DBT (change $rname)" PUT "$RESAPI/$DBT" "${JSON[@]}" --data "@$body"
  newsig=$(jq -r '.changes[0].newSignature // empty' <<<"$LAST_BODY")
  if [ -n "$newsig" ]; then
    shcap "C-P1 REST change poll: 120 s, every 2 s ($rname, read-back = gw-b answers the new signature)" bash "$SELF" c1poll 120 "$rname" "sig:${newsig:0:12}" "$pre" yes
  else
    prose "The PUT did not answer a newSignature (HTTP ${LAST_STATUS:-?}); the REST change poll was not run."
  fi
  wait_settled 120 > /dev/null || true

  if [ -s "$SIDE" ]; then
    if ! grep '^## ' "$OUT" | tail -1 | grep -q '^## C-P4'; then h "C-P4 (continued: during-sync)"; fi
    cat "$SIDE" >> "$OUT"
  else
    prose 'No `### C-P4 during-sync` block: no 2-second poll of the C-P1 runs read gw-b `syncPending` as true, so the state was never caught mid-sync at this poll rate.'
  fi
  /bin/rm -f "$SIDE"
}

# ---------------------------------------------------------------------- cp2

# sha_of FILE: sha256 or "(absent)".
sha_of() { if [ -f "$1" ]; then shasum -a 256 "$1" | cut -d' ' -f1; else printf '(absent)'; fi; }

cp2() {
  [ -f "$NAMES" ] || { echo "run cp1 first" >&2; exit 1; }
  local started kind name unit f sa sb eq units="" rows="" differ="" nrows=0 neq=0
  has_h C-P2 || h C-P2
  prose 'After the C-P1 syncs settled: the sha256 of every file below each changed unit and below one untouched unit, on gw-a'"'"'s and gw-b'"'"'s bind directories (`$HOME/.cache/ign-pair-rig/data-a/config/resources` and `data-b`), and whether the pair is byte-equal. The units are found by name in either directory, so a REST-made resource shows which collection it landed in.'
  started=$(utc)
  while read -r kind name; do
    units="$units
$( { (cd "$BIND_A" && find . -type d -name "$name") ; (cd "$BIND_B" && find . -type d -name "$name"); } 2>/dev/null | sed 's|^\./||' | LC_ALL=C sort -u )"
  done < "$NAMES"
  for unit in core/ignition/general-alarm-settings; do
    if [ -d "$BIND_A/$unit" ] && [ -d "$BIND_B/$unit" ]; then units="$units
$unit"; fi
  done
  units=$(printf '%s\n' "$units" | sed '/^$/d')
  printf '\n`for each unit: (cd $BIND_A/config/resources; find <unit> -type f) + (cd $BIND_B/config/resources; ...) | shasum -a 256` @ %s\n\nUnits compared (relative to `config/resources`): %s\n\n' "$started" "$(printf '%s' "$units" | tr '\n' ' ')" >> "$OUT"
  {
    printf '| path (relative to config/resources) | sha256 gw-a | sha256 gw-b | equal |\n| --- | --- | --- | --- |\n'
    while IFS= read -r unit; do
      [ -n "$unit" ] || continue
      while IFS= read -r f; do
        [ -n "$f" ] || continue
        sa=$(sha_of "$BIND_A/$f"); sb=$(sha_of "$BIND_B/$f")
        if [ "$sa" = "$sb" ]; then eq=yes; else eq=no; differ="$differ
$f"; fi
        printf '| `%s` | `%s` | `%s` | %s |\n' "$f" "$sa" "$sb" "$eq"
      done < <( { (cd "$BIND_A" && find "$unit" -type f); (cd "$BIND_B" && find "$unit" -type f); } 2>/dev/null | LC_ALL=C sort -u )
    done <<<"$units"
  } >> "$OUT"
  differ=$(printf '%s\n' "$differ" | sed '/^$/d')
  if [ -z "$differ" ]; then
    prose 'Every compared file is byte-equal (no row says `no`), so no `diff -u` was taken and nothing was redacted.'
  else
    prose "Files that differ: $(printf '%s' "$differ" | tr '\n' ' ')"
    sub 'C-P2 diff of the differing files (values of secret-bearing members replaced by <redacted>)'
    while IFS= read -r f; do
      [ -n "$f" ] || continue
      {
        printf '\n`diff -u $BIND_A/config/resources/%s $BIND_B/config/resources/%s | perl (redact password/secret/token/apikey values)` @ %s\n\n~~~\n' "$f" "$f" "$(utc)"
        diff -u "$BIND_A/$f" "$BIND_B/$f" 2>&1 | perl -pe 's/("[^"]*(?:password|secret|token|apikey)[^"]*"\s*:\s*)"[^"]*"/$1"<redacted>"/gi' || true
        printf '~~~\n'
      } >> "$OUT"
    done <<<"$differ"
    prose 'Any value of a member whose name holds password, secret, token or apikey would be shown as `<redacted>` above; none of these units is expected to carry one.'
  fi
}

# ---------------------------------------------------------------------- cp3

cp3() {
  local pre
  has_h C-P3 || h C-P3
  prose 'POST `/data/api/v1/redundancy/gwaction/resync` sent to the master (gw-a, `$BASE_A`); it is never sent to gw-b. Then gw-b status every 2 s until the pair is settled with an advanced `lastSyncTimestamp`, or 120 s.'
  wait_settled 120 > /dev/null || true
  cap b "GET redundancy on gw-b, before the resync" GET /data/api/v1/redundancy
  pre=$(rjq b lastSyncTimestamp)
  cap a "POST resync (sent to gw-a, the master)" POST /data/api/v1/redundancy/gwaction/resync
  case "$LAST_STATUS" in 415 | 411 | 400)
    prose "The bodiless POST answered HTTP $LAST_STATUS; repeating with a JSON content type and an empty object body."
    cap a "POST redundancy/gwaction/resync (sent to gw-a, JSON content type)" POST /data/api/v1/redundancy/gwaction/resync "${JSON[@]}" --data '{}' ;;
  esac
  shcap "C-P3 poll: gw-b status every 2 s until settled with an advanced lastSyncTimestamp or 120 s" bash "$SELF" c3poll 120 "$pre"
  cap b "GET redundancy/providers on gw-b, after the resync" GET /data/api/v1/redundancy/providers
}

# ------------------------------------------------------------------ cleanup24

cleanup24() {
  local sname rname ts0 sig
  [ -f "$NAMES" ] || { echo "no cp1-names" >&2; exit 1; }
  sname=$(awk '$1=="sync"{print $2}' "$NAMES"); rname=$(awk '$1=="rest"{print $2}' "$NAMES")
  # WR-06: both names go into a path that is removed with rm -rf. Refuse an
  # empty name (the rm would then target the whole collection), a path
  # separator or a `..` before anything is touched.
  for n in "$sname" "$rname"; do
    case "$n" in
      '' | */* | *..*) echo "REFUSING: cp1-names holds an empty name or one with '/' or '..': '$n'" >&2; exit 1 ;;
    esac
  done
  has_h 'Cleanup after C-P1 to C-P3' || h 'Cleanup after C-P1 to C-P3'
  prose "Remove $sname (files, under a scan-lock, then a config scan) and $rname (REST DELETE with its signature) from gw-a, and wait for gw-b to drop both."
  scan_idle
  ts0=$(scan_ts)
  cap a "POST scan-lock/config (acquire 5, hold 60)" POST $LOCKPATH "${JSON[@]}" --data '{"acquireTimeout":5,"holdTimeout":60}'
  shcap "every path of $sname on gw-a's bind mount before removal" bash -c "cd '$BIND_A' && find . -path '*$sname*' | LC_ALL=C sort"
  /bin/rm -rf "${BIND_A:?}"/production/ignition/database-connection/"${sname:?}"
  cap a "POST scan/config" POST $SCANPATH
  scan_done "$ts0" || true
  cap a "GET find $rname (read the signature)" GET "$RESAPI/find/$DBT/$rname"
  sig=$(jq -r '.signature // empty' <<<"$LAST_BODY")
  if [ -n "$sig" ]; then
    cap a "DELETE resources/$DBT/$rname/<signature>" DELETE "$RESAPI/$DBT/$rname/$sig"
  fi
  shcap "wait for gw-b to drop $sname and $rname (every 2 s, bound 120 s)" bash "$SELF" dropwait "$sname" "$rname"
  wait_settled 120 > /dev/null || true
  cp4 after-cleanup
}

# ---------------------------------------------------------------------- cp5/6

# cut_with METHOD: the link cut, recorded.
cut_with() {
  case "$1" in
    api) shcap "cut the link: setup-pair.sh cut (PUT toggleApproval false on gw-a)" bash "$SETUP" cut ;;
    net) shcap "cut the link, fallback: setup-pair.sh cut-net (docker network disconnect of gw-b from gan)" bash "$SETUP" cut-net ;;
  esac
}

cp5() {
  local method=api dropped
  has_h C-P5 || h C-P5
  prose 'Does `PUT /data/api/v1/gateway-network-incoming/toggleApproval/<id>/false` on gw-a cut an established link while both REST APIs stay up? Poll lines every 2 s for 60 s: gw-b and gw-a `peerConnected`, the HTTP status of `GET /StatusPing` on both nodes and of `GET /data/api/v1/redundancy` on gw-b. The fallback (`docker network disconnect` of gw-b from `gan` only) is captured only when the API cut does not drop the link.'
  sub 'C-P5 cut by toggleApproval false on gw-a'
  wait_settled 120 > /dev/null || true
  cap a "GET resources/list/ignition/gateway-network-incoming (before the cut)" GET /data/api/v1/resources/list/ignition/gateway-network-incoming
  shcap "setup-pair.sh status (before the cut)" bash "$SETUP" status
  cut_with api
  shcap "C-P5 poll: 60 s, every 2 s, after the toggleApproval cut" bash "$SELF" c5poll 60
  dropped=no
  case "$LAST_OUT" in *"first_gw-b_peerConnected_false=never"*) ;; *) dropped=yes ;; esac
  cap a "GET resources/list/ignition/gateway-network-incoming (during the cut)" GET /data/api/v1/resources/list/ignition/gateway-network-incoming
  cp4 after-cut
  printf '%s\n' "$method" > "$STATE/cut-method"
  [ "$dropped" = yes ] || printf 'none\n' > "$STATE/cut-method"
  shcap "restore the link: toggleApproval true on gw-a and relink, then wait for settled" bash "$SELF" c5link api
  cp4 after-reconnect
  if [ "$dropped" = yes ]; then
    prose 'Result: the toggleApproval cut dropped the link (gw-b `peerConnected` read false in the poll above) while the REST APIs stayed up; the fallback was not needed.'
    return 0
  fi
  prose 'Result: the toggleApproval cut did NOT drop the link within 60 s (gw-b `peerConnected` stayed true). Repeating with the two-network fallback.'
  sub 'C-P5 fallback: gw-b disconnected from the gan network'
  cut_with net
  shcap "C-P5 fallback poll: 60 s, every 2 s, after docker network disconnect" bash "$SELF" c5poll 60
  case "$LAST_OUT" in *"first_gw-b_peerConnected_false=never"*) printf 'none\n' > "$STATE/cut-method" ;; *) printf 'net\n' > "$STATE/cut-method" ;; esac
  cp4 after-cut-net
  shcap "restore the link: docker network connect and relink, then wait for settled" bash "$SELF" c5link net
  cp4 after-reconnect-net
}

cp6() {
  local method
  method=$(cat "$STATE/cut-method" 2>/dev/null || echo api)
  has_h C-P6 || h C-P6
  if [ "$method" = none ]; then
    prose 'Neither cut method dropped the link in C-P5; C-P6 uses the API cut regardless and records what it shows.'
    method=api
  fi
  prose "With the ping settings setup-pair.sh applied (pingRate 1000 ms, pingTimeout 1000 ms, pingMaxMissed 1200), does a cut backup stay Cold or promote itself? Cut method: $method. Poll lines every 5 s for 120 s: both nodes' \`activityLevel\` and \`peerConnected\`, gw-b \`projectState\` and both \`/StatusPing\` statuses."
  wait_settled 120 > /dev/null || true
  cut_with "$method"
  shcap "C-P6 poll: 120 s, every 5 s, after the cut ($method)" bash "$SELF" c6poll 120 5
  case "$LAST_OUT" in
    *"gw-b never reported Active"*) prose 'Result: gw-b never reported `Active` in the 120 s after the cut; the cut backup stayed Cold under these ping settings.' ;;
    *"gw-b reported Active"*) prose 'Result: gw-b reported `Active` during the cut window; the cut backup promoted itself (see the SUMMARY line above for the second).' ;;
  esac
  cp4 during-cut-c-p6
  shcap "restore the link, then wait for settled" bash "$SELF" c5link "$method"
  cp4 end-of-24-01
}

# cp5net: an extra capture beyond the plan, run because the API cut promoted gw-b
# at once: the same two series (C-P5 2 s link poll, C-P6 5 s activity poll) with
# the two-network cut, to show whether pingMaxMissed 1200 holds a cut backup Cold
# when the cause is missed pings and not a revoked approval.
cp5net() {
  has_h C-P5 || h C-P5
  sub 'C-P5 fallback (extra): gw-b disconnected from the gan network, same series as the API cut'
  prose 'Run in addition to the plan'"'"'s conditional fallback because the toggleApproval cut dropped the link and promoted gw-b immediately: this shows whether a network-level cut, which only starves the pings, behaves differently under pingMaxMissed 1200.'
  wait_settled 120 > /dev/null || true
  cut_with net
  shcap "C-P5 fallback poll: 60 s, every 2 s, after docker network disconnect" bash "$SELF" c5poll 60
  shcap "C-P6 fallback poll: 120 s, every 5 s, activityLevel after docker network disconnect (continues the 60 s above)" bash "$SELF" c6poll 120 5
  cp4 after-cut-net
  shcap "restore the link: docker network connect and relink, then wait for settled" bash "$SELF" c5link net
  cp4 after-reconnect-net
}


# ------------------------------------------------------------------- cp7/cp8

SPT=ignition/secret-provider
SPAPI=/data/api/v1/secret-providers
NOPERM_NAME=cap24-noperm
SEEN8=$STATE/c8-seen
START8=$STATE/c8-start
LOGPAT='did not match remote system snapshot|failed .* time\(s\)|max attempt count|deployment mode differs'

# list_probe NODE PROVIDER: "HTTP:<secrets array>" (or "HTTP:-" when the body is not JSON).
list_probe() {
  local node=$1 prov=$2 base tv tok out=$TMPD/lp.$1 code names
  base=$(node_base "$node"); tv=$(node_tokvar "$node"); tok=${!tv:-}
  code=$(curl -sS -g -m 15 -o "$out" -w '%{http_code}' -H "X-Ignition-API-Token: $tok" "$base$SPAPI/$prov/secrets" 2>/dev/null) || code=000
  names=$(jq -c '.secrets' "$out" 2>/dev/null) || names='-'
  printf '%s:%s' "$code" "${names:--}"
}

# spwait NAME...: every 2 s for up to 90 s, until gw-b answers 200 to find for every NAME.
spwait() {
  local t0 n ok st
  t0=$(date +%s)
  printf '# columns: UTC, elapsed, find status on gw-b per provider name\n'
  while :; do
    ok=yes; st=""
    for n in "$@"; do
      c=$(curl -sS -g -m 8 -o /dev/null -w '%{http_code}' -H "X-Ignition-API-Token: ${IGN_PAIR_TOKEN_B}" "$BASE_B$RESAPI/find/$SPT/$n" 2>/dev/null) || c=000
      st="$st $n=$c"; [ "$c" = 200 ] || ok=no
    done
    printf '%s +%3ss%s\n' "$(utc)" "$(( $(date +%s) - t0 ))" "$st"
    [ "$ok" = yes ] && { printf 'SUMMARY reached gw-b after %s s\n' "$(( $(date +%s) - t0 ))"; return 0; }
    [ $(( $(date +%s) - t0 )) -lt 90 ] || { printf 'SUMMARY not on gw-b within 90 s\n'; return 0; }
    sleep 2
  done
}

# c7poll SECS STEP PROVIDER...: both nodes' listing every STEP seconds.
c7poll() {
  local secs=$1 step=$2; shift 2
  local t0 target now i n line p
  t0=$(date +%s)
  printf '# columns: UTC, elapsed, then per provider the HTTP status and the secrets array on gw-a and gw-b (names only)\n'
  for (( i = 0; i * step <= secs; i++ )); do
    target=$(( t0 + i * step )); now=$(date +%s)
    [ "$now" -ge "$target" ] || sleep $(( target - now ))
    line=""
    for p in "$@"; do line="$line $p: gwA=$(list_probe a "$p") gwB=$(list_probe b "$p")"; done
    printf '%s +%3ss%s\n' "$(utc)" "$(( $(date +%s) - t0 ))" "$line"
  done
}

# del_resource NODE TYPE NAME: DELETE with the signature find answered (no-op when absent).
del_resource() {
  local node=$1 type=$2 name=$3 sig
  cap "$node" "GET find $type/$name (read the signature)" GET "$RESAPI/find/$type/$name"
  sig=$(jq -r '.signature // empty' <<<"$LAST_BODY" 2>/dev/null || true)
  if [ -n "$sig" ]; then
    cap "$node" "DELETE resources/$type/$name/<signature>" DELETE "$RESAPI/$type/$name/$sig"
  fi
}

cleanup7() {
  local sig
  prose 'Cleanup after C-P7: the two capture providers and the extra token are deleted from gw-a (with the signature find answers), the throwaway files are removed from gw-a'"'"'s container, and gw-b is waited on.'
  del_resource a "$SPT" cap24-file
  del_resource a "$SPT" cap24-int
  del_resource a ignition/api-token "$NOPERM_NAME"
  shcap "remove the throwaway secret files from gw-a's container" docker exec -u root "$(cid a)" sh -c 'rm -f /tmp/CAP24-Name /tmp/cap24-name'
  shcap "wait for gw-b to drop both providers (every 2 s, up to 90 s)" bash -c '
    t0=$(date +%s)
    while :; do
      n=0
      for p in cap24-file cap24-int; do
        c=$(curl -sS -g -m 8 -o /dev/null -w "%{http_code}" -H "X-Ignition-API-Token: ${IGN_PAIR_TOKEN_B}" "$BASE_B'"$RESAPI/find/$SPT"'/$p" 2>/dev/null) || c=000
        [ "$c" = 404 ] || n=$((n+1))
      done
      echo "$(date -u +%FT%TZ) +$(( $(date +%s) - t0 ))s still_present=$n"
      [ "$n" = 0 ] && break
      [ $(( $(date +%s) - t0 )) -lt 90 ] || break
      sleep 2
    done'
  cap b "GET resources/list/ignition/secret-provider on gw-b (after cleanup)" GET "$RESAPI/list/$SPT"
}

cp7() {
  local n p key b64 pad hash now body
  has_h C-P7 || h C-P7
  prose 'Secret listings, names only: `GET /data/api/v1/secret-providers/<provider>/secrets` on both nodes (gw-a `$BASE_A`, gw-b `$BASE_B`). The throwaway secret value lives only inside `setup-pair.sh` and is written into a file inside the container through stdin; it is never printed, so no capture below can hold it. Provider config is recorded through a jq projection of name and type only.'
  wait_settled 120 > /dev/null || true

  sub 'C-P7 provider inventory and the candidate built-in names (before any capture provider)'
  for n in a b; do
    capjq "$n" "GET resources/list/ignition/secret-provider on gw-$n (name and type only)" "$RESAPI/list/$SPT" \
      '[.items[] | {name, type: (.config.profile.type // .config.type)}]'
  done
  for p in default internal; do
    for n in a b; do cap "$n" "GET secret-providers/$p/secrets on gw-$n (candidate built-in name)" GET "$SPAPI/$p/secrets"; done
  done

  sub 'C-P7 unknown provider nope-24'
  for n in a b; do cap "$n" "GET secret-providers/nope-24/secrets on gw-$n" GET "$SPAPI/nope-24/secrets"; done

  sub 'C-P7 providers created on gw-a for the capture'
  shcap "setup-pair.sh provider a cap24-file file (empty file provider)" bash "$SETUP" provider a cap24-file file
  shcap "setup-pair.sh provider a cap24-int internal (internal provider, no secrets)" bash "$SETUP" provider a cap24-int internal
  shcap "wait for gw-b to list both providers" bash "$SELF" spwait cap24-file cap24-int
  for n in a b; do
    capjq "$n" "GET resources/list/ignition/secret-provider on gw-$n (name and type only, after creating the capture providers)" "$RESAPI/list/$SPT" \
      '[.items[] | {name, type: (.config.profile.type // .config.type)}]'
  done
  for p in cap24-file cap24-int; do
    for n in a b; do cap "$n" "GET secret-providers/$p/secrets on gw-$n (no secret created yet)" GET "$SPAPI/$p/secrets"; done
  done

  sub 'C-P7 calls that cannot read the listing: no token, a wrong key, a token without the permission'
  CAP_TOK_VAR=NONE cap a "GET secret-providers/cap24-file/secrets on gw-a (no token header)" GET "$SPAPI/cap24-file/secrets"
  IGN_PAIR_TOKEN_WRONG="pair-a:not-a-real-key" CAP_TOK_VAR=IGN_PAIR_TOKEN_WRONG cap a "GET secret-providers/cap24-file/secrets on gw-a (right token name, wrong key)" GET "$SPAPI/cap24-file/secrets"
  key=$(openssl rand 32 | openssl base64 -A | tr '+/' '-_' | tr -d '=')
  b64=$(printf '%s' "$key" | tr -- '-_' '+/'); pad=$(( (4 - ${#b64} % 4) % 4 )); b64="$b64$(printf '%*s' "$pad" '' | tr ' ' '=')"
  hash=$(printf '%s' "$b64" | openssl base64 -d -A | openssl dgst -sha256 -binary | openssl base64 -A | tr -- '+/' '-_' | tr -d '=')
  now="$(date +%s)000"
  body=$(jq -cn --arg n "$NOPERM_NAME" --arg h "$hash" --argjson t "$now" '[{name: $n, config: {profile: {type: "basic-token", secureChannelRequired: false, securityLevels: [{name: "Authenticated", children: []}], timestamp: $t}, settings: {tokenHash: $h}}}]')
  export IGN_PAIR_TOKEN_NOPERM="$NOPERM_NAME:$key"
  cap a "POST resources/ignition/api-token (a token holding only Authenticated, no Deploy level; the key is generated here and never printed)" POST "$RESAPI/ignition/api-token" "${JSON[@]}" --data "$body"
  sleep 5
  CAP_TOK_VAR=IGN_PAIR_TOKEN_NOPERM cap a "GET secret-providers/cap24-file/secrets on gw-a (token without the Deploy level)" GET "$SPAPI/cap24-file/secrets"
  CAP_TOK_VAR=IGN_PAIR_TOKEN_NOPERM cap a "GET resources/list/ignition/secret-provider on gw-a (token without the Deploy level)" GET "$RESAPI/list/$SPT"

  sub 'C-P7 a secret created on gw-a: does the name reach gw-b, and is the case kept'
  shcap "setup-pair.sh secrets a cap24-file CAP24-Name" bash "$SETUP" secrets a cap24-file CAP24-Name
  shcap "C-P7 poll: 60 s, every 5 s, cap24-file listing on both nodes" bash "$SELF" c7poll 60 5 cap24-file
  shcap "setup-pair.sh secrets a cap24-file cap24-name (same name in lower case)" bash "$SETUP" secrets a cap24-file cap24-name
  shcap "C-P7 poll: 30 s, every 5 s, cap24-file listing on both nodes after the second name" bash "$SELF" c7poll 30 5 cap24-file
  for n in a b; do
    capjq "$n" "GET resources/list/ignition/secret-provider on gw-$n (provider name, type and secret NAMES only)" "$RESAPI/list/$SPT" \
      '[.items[] | {name, type: (.config.profile.type // .config.type), secret_names: ((.config.settings.files // {}) | keys)}]'
  done
  for n in a b; do cap "$n" "GET secret-providers/cap24-file/secrets on gw-$n (final)" GET "$SPAPI/cap24-file/secrets"; done
  cp7remote
  sub 'C-P7 cleanup'
  cleanup7
}

# cp7remote: a provider of type `remote` (the one candidate for a listing answer of
# 501), created on gw-a, listed on both nodes, deleted again.
cp7remote() {
  local n
  has_h C-P7 || h C-P7
  sub 'C-P7 remote provider (candidate for a 501 listing)'
  prose 'The provider types the gateway offers are `file`, `internal` and `remote` (`GET /data/api/v1/resources/type/ignition/secret-provider`). A `remote` provider links to a secret provider on another gateway through the gateway network, so it is the one candidate for a provider that cannot list its secrets. Created on gw-a pointing at the peer `ign-pair-a-Backup`, provider `cap24-file`.'
  shcap "setup-pair.sh provider a cap24-remote remote" bash "$SETUP" provider a cap24-remote remote
  shcap "wait for gw-b to list the provider" bash "$SELF" spwait cap24-remote
  for n in a b; do cap "$n" "GET secret-providers/cap24-remote/secrets on gw-$n" GET "$SPAPI/cap24-remote/secrets"; done
  del_resource a "$SPT" cap24-remote
  shcap "wait for gw-b to drop the provider" bash -c '
    t0=$(date +%s)
    while :; do
      c=$(curl -sS -g -m 8 -o /dev/null -w "%{http_code}" -H "X-Ignition-API-Token: ${IGN_PAIR_TOKEN_B}" "$BASE_B'"$RESAPI/find/$SPT"'/cap24-remote" 2>/dev/null) || c=000
      echo "$(date -u +%FT%TZ) +$(( $(date +%s) - t0 ))s find=$c"
      [ "$c" = 404 ] && break
      [ $(( $(date +%s) - t0 )) -lt 90 ] || break
      sleep 2
    done'
}

# c8poll SECS STEP START: gw-b state every STEP seconds for SECS seconds, or until
# the `max attempt count` line shows. Only GETs go to gw-b. Log lines already printed
# in an earlier poll are not printed again (SEEN8).
c8poll() {
  local secs=$1 step=$2 start=$3 stop=${4:-stop}
  local t0 target now i ts pend peer act proj prov evn newl found=never base tv tok
  t0=$(date +%s)
  [ "$stop" = stop ] || found=after-resync
  base=$BASE_B; tok=${IGN_PAIR_TOKEN_B}
  printf '# columns: UTC, elapsed, gw-b lastSyncTimestamp syncPending peerConnected activityLevel projectState, gw-b config-resource provider (status, pendingUpdates, lastSyncError cut to 120, hashcode), count of providers not GOOD, count of redundancy events at WARN or ERROR; then new filtered log lines (cut to 300)\n'
  for (( i = 0; i * step <= secs; i++ )); do
    target=$(( t0 + i * step )); now=$(date +%s)
    [ "$now" -ge "$target" ] || sleep $(( target - now ))
    read -r ts pend peer act proj <<<"$(rstate b)"
    prov=$(curl -sS -g -m 15 -H "X-Ignition-API-Token: $tok" "$base/data/api/v1/redundancy/providers" 2>/dev/null \
      | jq -c '[(.items[] | select(.name=="config-resource") | {status, pendingUpdates, lastSyncError: (.lastSyncError[0:120]), hashcode}), ([.items[] | select(.status != "GOOD")] | length)]' 2>/dev/null) || prov='?'
    evn=$(curl -sS -g -m 15 -H "X-Ignition-API-Token: $tok" "$base/data/api/v1/redundancy/events?limit=200" 2>/dev/null \
      | jq -c '[.items[] | select(.level == "WARN" or .level == "ERROR")] | length' 2>/dev/null) || evn='?'
    printf '%s +%4ss gwB: %s %s %s %s %s | provider/notGOOD=%s | warn_error_events=%s\n' "$(utc)" "$(( $(date +%s) - t0 ))" "$ts" "$pend" "$peer" "$act" "$proj" "${prov:-?}" "${evn:-?}"
    newl=$(dc logs gw-b --no-color --since "$start" 2>/dev/null | grep -E "$LOGPAT" | cut -c1-300 || true)
    if [ -n "$newl" ]; then
      touch "$SEEN8"
      while IFS= read -r l; do
        grep -qxF -- "$l" "$SEEN8" || { printf '%s\n' "$l" >> "$SEEN8"; printf '    LOG %s\n' "$l"; }
      done <<<"$newl"
    fi
    if grep -q 'max attempt count' "$SEEN8" 2>/dev/null && [ "$found" = never ]; then
      found=$(( $(date +%s) - t0 ))
      printf 'SUMMARY max_attempt_count_line_seen_at=+%ss\n' "$found"
      [ "$stop" = stop ] && return 0
    fi
  done
  printf 'SUMMARY max_attempt_count_line_seen_at=%s\n' "$found"
}

cp8() {
  local start1 end1 start_log line
  has_h C-P8 || h C-P8
  prose 'TEC-5105 case (backup project-sync failure): this state feeds its own finding, reason `sync_exhausted` (D-09, D-10), and never triggers a master rollback or restore (ROADMAP Phase 24 constraint). Every K6 option keeps that rule; the open question is only whether resync clears the state.'
  start1=$(utc); printf '%s\n' "$start1" > "$STATE/c8-box-start"
  prose "Time box 20 minutes, started $start1 (UTC). Path (the spike RIG.md path): gw-b runs without a deployment mode while gw-a runs \`production\`; expected log chain on gw-b: \`Active deployment mode differs from peer node!\`, then \`[config-resource] After applying diff, new local system snapshot ... did not match remote system snapshot\`, then \`Sync operation has failed 5 time(s), which exceeds the max attempt count\`. Only GETs go to gw-b; gw-b is changed only by recreating its container."
  wait_settled 120 > /dev/null || true
  cp4 before-exhaust
  : > "$SEEN8"
  start_log=$(utc); printf '%s\n' "$start_log" > "$START8"
  shcap "recreate gw-b with no deployment mode (IGN_PAIR_MODE_B=-Dign.pair.rig.noop=true)" env IGN_PAIR_MODE_B=-Dign.pair.rig.noop=true docker compose --project-directory "$HERE" -p "$PROJECT" -f "$HERE/compose.yml" up -d gw-b
  shcap "relink: setup-pair.sh link (toggleApproval true on gw-a, trust loop until gw-b peerConnected)" bash "$SETUP" link
  shcap "C-P8 poll: gw-b state every 15 s until the max-attempt line or 20 minutes since $start1" bash "$SELF" c8poll 1200 15 "$start_log" stop
  case "$LAST_OUT" in
    *"max_attempt_count_line_seen_at=+"*)
      prose 'The `max attempt count` line appeared on gw-b. Providers and events once more (GET only), then ONE resync sent to gw-a.'
      cap b "GET redundancy on gw-b (exhausted)" GET /data/api/v1/redundancy
      cap b "GET redundancy/providers on gw-b (exhausted, before the resync)" GET /data/api/v1/redundancy/providers
      cap b "GET redundancy/events?limit=25 on gw-b (exhausted, before the resync)" GET "/data/api/v1/redundancy/events?limit=25"
      cap a "POST redundancy/gwaction/resync (sent to gw-a)" POST /data/api/v1/redundancy/gwaction/resync
      shcap "C-P8 poll: gw-b state every 10 s for 120 s after the resync" bash "$SELF" c8poll 120 10 "$start_log" go
      cap b "GET redundancy on gw-b (after the resync)" GET /data/api/v1/redundancy
      cap b "GET redundancy/providers on gw-b (after the resync)" GET /data/api/v1/redundancy/providers
      cap b "GET redundancy/events?limit=25 on gw-b (after the resync)" GET "/data/api/v1/redundancy/events?limit=25"
      ;;
    *)
      end1=$(utc)
      cap b "GET redundancy/providers on gw-b (box ended, line never seen)" GET /data/api/v1/redundancy/providers
      cap b "GET redundancy/events?limit=25 on gw-b (box ended)" GET "/data/api/v1/redundancy/events?limit=25"
      prose "not reachable on the rig. Tried: gw-b recreated without a deployment mode against a \`production\` master, relinked, then polled every 15 s from $start1 to $end1 (UTC); the \`max attempt count\` line did not appear. The log lines that did appear are in the poll above."
      ;;
  esac
  prose "Restore: gw-b is recreated with the production mode argument, relinked, and waited on until settled; the end of the time box was $(utc) (UTC)."
  shcap "recreate gw-b with the production mode argument" env IGN_PAIR_MODE_B="$IGN_PAIR_MODE_B" docker compose --project-directory "$HERE" -p "$PROJECT" -f "$HERE/compose.yml" up -d gw-b
  shcap "relink: setup-pair.sh link" bash "$SETUP" link
  wait_settled 600 > /dev/null || true
  cp4 after-exhaust
}

case "${1:-}" in
  header24) header24 ;;
  cp5net) cp5net ;;
  cp1) cp1 ;;
  cp2) cp2 ;;
  cp3) cp3 ;;
  cp4) cp4 "${2:?LABEL}" ;;
  cp5) cp5 ;;
  cp6) cp6 ;;
  cleanup24) cleanup24 ;;
  cp7) cp7 ;;
  cp7remote) cp7remote ;;
  cp8) cp8 ;;
  cleanup7) cleanup7 ;;
  spwait) shift; spwait "$@" ;;
  c7poll) shift; c7poll "$@" ;;
  c8poll) c8poll "${2:?SECS}" "${3:?STEP}" "${4:?START}" "${5:-stop}" ;;
  c1poll) c1poll "${2:?SECS}" "${3:?NAME}" "${4:?MODE}" "${5:?PRE_TS}" "${6:-no}" ;;
  c3poll) c3poll "${2:?SECS}" "${3:?PRE_TS}" ;;
  c5poll) c5poll "${2:?SECS}" ;;
  c5link) c5link "${2:?METHOD}" ;;
  c6poll) c6poll "${2:?SECS}" "${3:?STEP}" ;;
  dropwait) shift; dropwait "$@" ;;
  down24) echo "docker compose -p $PROJECT down -v" ;;
  *) echo "usage: capture24.sh header24|cp1|cp2|cp3|cp4 LABEL|cp5|cp6|cp7|cp8|cleanup24|down24" >&2; exit 64 ;;
esac
