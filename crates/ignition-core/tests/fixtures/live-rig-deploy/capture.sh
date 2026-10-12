#!/usr/bin/env bash
# capture.sh SUBCOMMAND [ARG]
#
# Reproducible capture driver for the Phase 20 wire truth (20-01). Appends
# verbatim request/response blocks to 20-LIVE-CAPTURES.md.
#
# Subcommands: c1 | cnull [LABEL] | cmode | c3a | clist | wait
#   cnull LABEL is the heading suffix, e.g. "(no mode)" or "(mode production)".
#
# LOCALHOST ONLY. assert_local runs before anything else (before the output
# file is touched and before any request) and exits 2 unless BASE's host is
# localhost or 127.0.0.1. Tokens come from tokens.env (0600, outside the repo),
# are sent as a header, and are never printed: curl -i prints RESPONSE headers
# only. Never enable `set -x` here.
set -euo pipefail

BASE=${BASE:-http://localhost:18290}

assert_local() {
  local rest authority hostport host
  rest=${BASE#*://}
  authority=${rest%%/*}
  hostport=${authority##*@}
  case "$hostport" in
    \[*) host=${hostport%%]*}; host=${host#[} ;;
    *) host=${hostport%%:*} ;;
  esac
  case "$host" in
    localhost | 127.0.0.1) ;;
    *) echo "REFUSING: BASE '$BASE' host '$host' is not localhost or 127.0.0.1" >&2; exit 2 ;;
  esac
}
assert_local

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(git -C "$HERE" rev-parse --show-toplevel)
OUT=${CAPTURE_OUT:-$REPO/.planning/phases/20-config-api-client/20-LIVE-CAPTURES.md}
STATE=${IGN_DEPLOY_RIG_STATE:-$HOME/.cache/ign-deploy-rig}
DBCFG=$HERE/db-connection-config.json
TMPD=$(mktemp -d)
trap '/bin/rm -rf "$TMPD"' EXIT

# shellcheck disable=SC1091
[ -f "$STATE/tokens.env" ] && . "$STATE/tokens.env"

utc() { date -u +%FT%TZ; }

# h LABEL: a `## ` heading. One per capture id; sub-requests use `cap`.
h() { printf '\n## %s\n' "$1" >> "$OUT"; }

# note TEXT: a verbatim ~~~ block of local text (jq summaries, markers).
note() { printf '\n~~~\n%s\n~~~\n' "$1" >> "$OUT"; }

# cap LABEL TOKENVAR METHOD PATH [curl args]: one verbatim exchange.
# The body of the response is left in $LAST_BODY and the status in $LAST_STATUS.
LAST_BODY=""
LAST_STATUS=""
cap() {
  local label=$1 tokvar=$2 method=$3 path=$4; shift 4
  local tok=${!tokvar:?token variable $tokvar is not set; run mint-token.sh and source tokens.env}
  local raw=$TMPD/raw started; started=$(utc)
  curl -sS -g -i -m 60 -w '\n[curl http=%{http_code} total=%{time_total}s]\n' \
    -X "$method" -H "X-Ignition-API-Token: $tok" "$@" "$BASE$path" > "$raw" || true
  {
    printf '\n### %s\n\n~~~\n%s %s @ %s\n' "$label" "$method" "$path" "$started"
    tr -d '\r' < "$raw"
    printf '~~~\n'
  } >> "$OUT"
  LAST_STATUS=$(sed -n 's/^\[curl http=\([0-9]*\) .*/\1/p' "$raw" | tail -1)
  LAST_BODY=$(sed '1,/^\r$/d' "$raw" | sed '/^\[curl http=/d')
}

JSON=(-H 'Content-Type: application/json')

wait_running() {
  local t0; t0=$(date +%s)
  until curl -s -m 5 "$BASE/StatusPing" | grep -q RUNNING; do
    [ $(( $(date +%s) - t0 )) -lt 900 ] || { echo "gateway not RUNNING within 900 s" >&2; return 1; }
    sleep 3
  done
}

# Poll the config lock until it answers 204 (free); never logged.
wait_free() {
  local t0 st; t0=$(date +%s)
  while :; do
    st=$(curl -s -o /dev/null -m 10 -w '%{http_code}' -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE/data/api/v1/scan-lock/config" || true)
    [ "$st" = 204 ] && return 0
    [ $(( $(date +%s) - t0 )) -lt 120 ] || { echo "lock not free within 120 s" >&2; return 1; }
    sleep 1
  done
}

LOCK=/data/api/v1/scan-lock/config
SCAN=/data/api/v1/scan/config
RES=/data/api/v1/resources
DBT=ignition/database-connection

c1() {
  h 'C1a — idle GET scan-lock/config (no lock held)'
  cap 'GET idle' IGN_TOKEN_CAP_A GET $LOCK

  h 'C1b — POST scan-lock/config with no body, then with {"acquireTimeout":10,"holdTimeout":60}'
  cap 'POST no body' IGN_TOKEN_CAP_A POST $LOCK
  cap 'POST acquire 10/60' IGN_TOKEN_CAP_A POST $LOCK "${JSON[@]}" --data '{"acquireTimeout":10,"holdTimeout":60}'

  h 'C1c — held GET twice, 2 s apart (cap-a holds)'
  cap 'GET held #1' IGN_TOKEN_CAP_A GET $LOCK
  sleep 2
  cap 'GET held #2 (2 s later)' IGN_TOKEN_CAP_A GET $LOCK

  h 'C1d — cap-a acquires twice (same-token re-acquire while held)'
  cap 'POST re-acquire by the holder 3/20' IGN_TOKEN_CAP_A POST $LOCK "${JSON[@]}" --data '{"acquireTimeout":3,"holdTimeout":20}'
  cap 'GET after re-acquire' IGN_TOKEN_CAP_A GET $LOCK
  wait_free

  h 'C1e — cap-a holds (holdTimeout 20) while cap-b POSTs with acquireTimeout 2'
  cap 'cap-a acquires 10/20' IGN_TOKEN_CAP_A POST $LOCK "${JSON[@]}" --data '{"acquireTimeout":10,"holdTimeout":20}'
  cap 'cap-b competing acquire 2/5' IGN_TOKEN_CAP_B POST $LOCK "${JSON[@]}" --data '{"acquireTimeout":2,"holdTimeout":5}'
  cap 'GET as cap-b (holder visible?)' IGN_TOKEN_CAP_B GET $LOCK
  wait_free

  h 'C1f — holdTimeout 3, sleep 5, GET (expect 204), then scan POST and GET'
  cap 'acquire 10/3' IGN_TOKEN_CAP_A POST $LOCK "${JSON[@]}" --data '{"acquireTimeout":10,"holdTimeout":3}'
  sleep 5
  cap 'GET after expiry' IGN_TOKEN_CAP_A GET $LOCK
  cap 'POST scan/config' IGN_TOKEN_CAP_A POST $SCAN
  cap 'GET scan/config' IGN_TOKEN_CAP_A GET $SCAN

  h 'C1g — release probes: DELETE scan-lock/config and POST scan-lock/config/release (lock held by cap-a)'
  cap 'acquire 10/20' IGN_TOKEN_CAP_A POST $LOCK "${JSON[@]}" --data '{"acquireTimeout":10,"holdTimeout":20}'
  cap 'DELETE scan-lock/config' IGN_TOKEN_CAP_A DELETE $LOCK
  cap 'POST scan-lock/config/release' IGN_TOKEN_CAP_A POST $LOCK/release
  cap 'GET after the probes (still held?)' IGN_TOKEN_CAP_A GET $LOCK
  wait_free

  h 'C1h — GET /openapi.json (live 8.3.6): path count and scan/lock routes'
  mkdir -p "$STATE"
  local oa=$STATE/openapi-8.3.6.json
  curl -sS -D "$TMPD/oa.hdr" -m 120 -o "$oa" -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE/openapi.json" || true
  note "GET /openapi.json @ $(utc)
$(head -1 "$TMPD/oa.hdr" | tr -d '\r')
saved to \$IGN_DEPLOY_RIG_STATE/openapi-8.3.6.json ($(wc -c < "$oa" | tr -d ' ') bytes; not in the repo)
path count: $(jq '.paths | length' "$oa" 2>&1)
openapi: $(jq -r '.openapi' "$oa" 2>&1)   info: $(jq -c '.info' "$oa" 2>&1)
paths matching scan or lock, with methods:
$(jq -c '[.paths | to_entries[] | select(.key | test("scan|lock")) | {p: .key, m: (.value | keys)}]' "$oa" 2>&1)
paths matching release|unlock:
$(jq -c '[.paths | keys[] | select(test("release|unlock"))]' "$oa" 2>&1)"

  h 'C1i — scan/config: idle GET, POST, poll until scanActive is false and lastScanTimestamp advanced; second POST; cap-b scan while cap-a holds'
  cap 'GET idle' IGN_TOKEN_CAP_A GET $SCAN
  local before; before=$(jq -r '.lastScanTimestamp // 0' <<<"$LAST_BODY" 2>/dev/null || echo 0)
  cap 'POST scan #1' IGN_TOKEN_CAP_A POST $SCAN
  cap 'POST scan #2 immediately after #1' IGN_TOKEN_CAP_A POST $SCAN
  poll_scan "$before"
  cap 'acquire 10/20 (cap-a)' IGN_TOKEN_CAP_A POST $LOCK "${JSON[@]}" --data '{"acquireTimeout":10,"holdTimeout":20}'
  cap 'cap-b POST scan while cap-a holds the lock' IGN_TOKEN_CAP_B POST $SCAN
  cap 'GET scan after the cap-b scan' IGN_TOKEN_CAP_B GET $SCAN
  cap 'GET lock after the cap-b scan' IGN_TOKEN_CAP_A GET $LOCK
  wait_free

  h 'C1j — does a holder'"'"'s scan release the lock? (cap-a acquires 20, cap-a POSTs scan/config, GET lock)'
  cap 'acquire 10/20' IGN_TOKEN_CAP_A POST $LOCK "${JSON[@]}" --data '{"acquireTimeout":10,"holdTimeout":20}'
  cap 'holder POST scan/config' IGN_TOKEN_CAP_A POST $SCAN
  cap 'GET lock after the holder scan' IGN_TOKEN_CAP_A GET $LOCK
  poll_scan 0
  wait_free
}

# poll_scan BEFORE: poll GET scan/config every 500 ms until scanActive is false
# and lastScanTimestamp is greater than BEFORE (max 120 s). One line per poll.
poll_scan() {
  local before=$1 t0 now body active ts n=0
  t0=$(date +%s)
  printf '\n### poll GET scan/config every 500 ms (before lastScanTimestamp=%s) @ %s\n\n~~~\n' "$before" "$(utc)" >> "$OUT"
  while :; do
    body=$(curl -sS -m 10 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$SCAN" || true)
    now=$(( $(date +%s) - t0 )); n=$((n + 1))
    printf 't+%ss %s\n' "$now" "$body" >> "$OUT"
    active=$(jq -r '.scanActive' <<<"$body" 2>/dev/null || echo '?')
    ts=$(jq -r '.lastScanTimestamp // 0' <<<"$body" 2>/dev/null || echo 0)
    if [ "$active" = false ] && [ "$ts" -gt "$before" ] 2>/dev/null; then
      printf '[poll done: %s polls, completion predicate met]\n~~~\n' "$n" >> "$OUT"; return 0
    fi
    if [ "$now" -ge 120 ]; then
      printf '[poll gave up after %s polls, completion predicate NOT met]\n~~~\n' "$n" >> "$OUT"; return 0
    fi
    sleep 0.5
  done
}

cnull() {
  h "C-null ${1:-(no mode)}"
  cap 'GET gateway-info (raw)' IGN_TOKEN_CAP_A GET /data/api/v1/gateway-info
  note "$(jq -c '{has: has("deploymentMode"), v: .deploymentMode, t: (.deploymentMode | type)}' <<<"$LAST_BODY" 2>&1)"
}

cmode() {
  h 'C-mode'
  cap 'GET /mode (before)' IGN_TOKEN_CAP_A GET /data/api/v1/mode
  cap 'POST /mode production' IGN_TOKEN_CAP_A POST /data/api/v1/mode "${JSON[@]}" \
    --data '{"name":"production","title":"Production","description":"ign-deploy-rig"}'
  cap 'POST /mode production again (duplicate)' IGN_TOKEN_CAP_A POST /data/api/v1/mode "${JSON[@]}" \
    --data '{"name":"production","title":"Production","description":"ign-deploy-rig"}'
  cap 'POST /mode with the illegal name "a b/"' IGN_TOKEN_CAP_A POST /data/api/v1/mode "${JSON[@]}" \
    --data '{"name":"a b/","title":"Illegal","description":"ign-deploy-rig"}'
  cap 'GET /mode (after)' IGN_TOKEN_CAP_A GET /data/api/v1/mode
}

c3a() {
  h 'C3a'
  mkdir -p "$STATE"
  local name="IGN_DEPLOY_RIG_DB_$(date +%s)"
  printf '%s' "$name" > "$STATE/c3a-name"
  local find=$RES/find/$DBT/$name
  local body

  body=$(jq -c --arg n "$name" '[{name: $n, config: .}]' "$DBCFG")
  cap 'POST create (one-element array, name + config, no collection, no attributes)' IGN_TOKEN_CAP_A POST $RES/$DBT "${JSON[@]}" --data "$body"
  cap 'POST the same name again (duplicate)' IGN_TOKEN_CAP_A POST $RES/$DBT "${JSON[@]}" --data "$body"
  cap 'GET find (read the signature)' IGN_TOKEN_CAP_A GET "$find"
  local sig; sig=$(jq -r '.signature' <<<"$LAST_BODY")

  body=$(jq -c --arg n "$name" --arg s "$sig" '[{name: $n, signature: $s, description: "c3a update", config: (. + {poolMaxActive: 9})}]' "$DBCFG")
  cap 'PUT with the right signature' IGN_TOKEN_CAP_A PUT $RES/$DBT "${JSON[@]}" --data "$body"
  cap 'GET find after the good PUT' IGN_TOKEN_CAP_A GET "$find"
  local fresh; fresh=$(jq -r '.signature' <<<"$LAST_BODY")

  local stale=deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef
  body=$(jq -c --arg n "$name" --arg s "$stale" '[{name: $n, signature: $s, description: "c3a stale", config: (. + {poolMaxActive: 11})}]' "$DBCFG")
  cap 'PUT with a stale (64-hex wrong) signature' IGN_TOKEN_CAP_A PUT $RES/$DBT "${JSON[@]}" --data "$body"
  readback "$find" "$fresh"

  body=$(jq -c --arg n "$name" '[{name: $n, description: "c3a omitted", config: (. + {poolMaxActive: 12})}]' "$DBCFG")
  cap 'PUT with the signature key omitted' IGN_TOKEN_CAP_A PUT $RES/$DBT "${JSON[@]}" --data "$body"
  readback "$find" "$fresh"

  body=$(jq -c --arg n "$name" '[{name: $n, signature: "", description: "c3a empty", config: (. + {poolMaxActive: 13})}]' "$DBCFG")
  cap 'PUT with an empty-string signature' IGN_TOKEN_CAP_A PUT $RES/$DBT "${JSON[@]}" --data "$body"
  readback "$find" "$fresh"

  body=$(jq -c --arg n "${name}_NOPE" --arg s "$fresh" '[{name: $n, signature: $s, description: "c3a unknown", config: (. + {poolMaxActive: 14})}]' "$DBCFG")
  cap 'PUT of an unknown name (with the current signature)' IGN_TOKEN_CAP_A PUT $RES/$DBT "${JSON[@]}" --data "$body"
  readback "$find" "$fresh"
  cap 'GET find of the unknown name (never created?)' IGN_TOKEN_CAP_A GET "$RES/find/$DBT/${name}_NOPE"
}

# readback FINDPATH EXPECTED_SIGNATURE: find after a refusal, proving no change.
readback() {
  cap 'GET find read-back after the refusal' IGN_TOKEN_CAP_A GET "$1"
  note "unchanged check: signature=$(jq -r '.signature' <<<"$LAST_BODY" 2>/dev/null) expected=$2 poolMaxActive=$(jq -r '.config.poolMaxActive' <<<"$LAST_BODY" 2>/dev/null) description=$(jq -r '.description' <<<"$LAST_BODY" 2>/dev/null)"
}

clist() {
  h 'C-list'
  local name; name=$(cat "$STATE/c3a-name")
  local body; body=$(jq -c --arg n "$name" '[{name: $n, collection: "production", description: "c-list production override", config: (. + {poolMaxActive: 20})}]' "$DBCFG")
  cap 'POST the C3a name into collection production' IGN_TOKEN_CAP_A POST $RES/$DBT "${JSON[@]}" --data "$body"
  cap 'GET find, no collection (effective view)' IGN_TOKEN_CAP_A GET "$RES/find/$DBT/$name"
  cap 'GET find?collection=core' IGN_TOKEN_CAP_A GET "$RES/find/$DBT/$name?collection=core"
  cap 'GET find?collection=production' IGN_TOKEN_CAP_A GET "$RES/find/$DBT/$name?collection=production"
  cap 'GET list?limit=-1' IGN_TOKEN_CAP_A GET "$RES/list/$DBT?limit=-1"
  cap 'GET list?limit=-1&filter=collection[eq]=production' IGN_TOKEN_CAP_A GET "$RES/list/$DBT?limit=-1&filter=collection[eq]=production"
  cap 'GET names' IGN_TOKEN_CAP_A GET "$RES/names/$DBT"
  cap 'GET singleton security-properties (effective)' IGN_TOKEN_CAP_A GET "$RES/singleton/ignition/security-properties"
  cap 'GET singleton security-properties?collection=core' IGN_TOKEN_CAP_A GET "$RES/singleton/ignition/security-properties?collection=core"
}

# clist_extra: extra D7 probes appended under the existing C-list heading (no new heading).
clist_extra() {
  local name; name=$(cat "$STATE/c3a-name")
  cap 'GET list?limit=-1&collection=core (undeclared param)' IGN_TOKEN_CAP_A GET "$RES/list/$DBT?limit=-1&collection=core"
  cap 'GET list?limit=-1&filter=collection[eq]=core' IGN_TOKEN_CAP_A GET "$RES/list/$DBT?limit=-1&filter=collection[eq]=core"
  cap 'GET list?limit=-1&filter=name[eq]=<c3a name> (control: a filter on another field)' IGN_TOKEN_CAP_A GET "$RES/list/$DBT?limit=-1&filter=name[eq]=$name"
}

# clist_extra2: the filter parameter is a form-style object (`field[op]=value` is its OWN query
# parameter, per the live OpenAPI), not `filter=field[op]=value`. Run after clist_extra.
clist_extra2() {
  local name; name=$(cat "$STATE/c3a-name")
  cap 'GET list?limit=-1&collection[eq]=production (object-style filter)' IGN_TOKEN_CAP_A GET "$RES/list/$DBT?limit=-1&collection[eq]=production"
  cap 'GET list?limit=-1&collection[eq]=core (the core copy exists but is shadowed)' IGN_TOKEN_CAP_A GET "$RES/list/$DBT?limit=-1&collection[eq]=core"
  cap 'GET list?limit=-1&name[eq]=<c3a name> (control)' IGN_TOKEN_CAP_A GET "$RES/list/$DBT?limit=-1&name[eq]=$name"
}

case "${1:-}" in
  c1) c1 ;;
  clist_extra2) clist_extra2 ;;
  clist_extra) clist_extra ;;
  cnull) cnull "${2:-(no mode)}" ;;
  cmode) cmode ;;
  c3a) c3a ;;
  clist) clist ;;
  wait) wait_running ;;
  *) echo "usage: capture.sh c1|cnull [LABEL]|cmode|c3a|clist|wait" >&2; exit 64 ;;
esac
