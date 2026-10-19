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
  for v in IGN_PAIR_TOKEN_A IGN_PAIR_TOKEN_B; do
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
    -X "$method" "${auth[@]}" "$@" "$base$path" > "$raw" || true
  refuse_if_token "$raw"
  {
    printf '\n#### %s\n\n`curl -sS -g -i -X %s [%s] %s%s` @ %s\n\n~~~\n' \
      "$label" "$method" "$shown" "${*:+$* }" "'\$$bv$path'" "$started"
    tr -d '\r' < "$raw"
    printf '~~~\n'
  } >> "$OUT"
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
    for v in IGN_PAIR_TOKEN_A IGN_PAIR_TOKEN_B; do
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
  if ! grep -q '^## C-P4$' "$OUT" 2>/dev/null; then
    h C-P4
    prose 'Redundancy status of both nodes at labelled moments: GET `/data/api/v1/redundancy` and GET `/data/api/v1/redundancy/providers` verbatim, then each node'"'"'s gateway-info `name` and `deploymentMode`. gw-a is the master (`$BASE_A`), gw-b the backup (`$BASE_B`).'
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

case "${1:-}" in
  header24) header24 ;;
  cp4) cp4 "${2:?LABEL}" ;;
  down24) echo "docker compose -p $PROJECT down -v" ;;
  *) echo "usage: capture24.sh header24|cp4|down24" >&2; exit 64 ;;
esac
