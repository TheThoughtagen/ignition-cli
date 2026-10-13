#!/usr/bin/env bash
# capture.sh SUBCOMMAND [ARG]
#
# Reproducible capture driver for the Phase 20 wire truth (20-01). Appends
# verbatim request/response blocks to 20-LIVE-CAPTURES.md.
#
# Subcommands: c1 | cnull [LABEL] | cmode | c3a | clist | wait
#   cnull LABEL is the heading suffix, e.g. "(no mode)" or "(mode production)".
#
# Phase 21 (21-02) subcommands, always run with
#   CAPTURE_OUT=$REPO/.planning/phases/21-deploy-render/21-LIVE-CAPTURES.md:
#   up21 | setup21 | header21 | hdrrow KEY VALUE | uuid8 | c3b | c4 | oq8 | oq1 | oq4 | c5
#   check21 [--decisions] [ID ...] | shcap LABEL CMD [ARG ...] | v8uuid GATEWAY MODULE TYPE NAME
#   Every byte inside a `## C21-<ID>` section is appended by `cap` (HTTP) or `shcap`
#   (any other command), stamped with the exact command and a script-taken UTC time.
#   `check21` reads the file and sends no request.
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

# ---------------------------------------------------------------------------
# Phase 21 (21-02): live captures for deploy render. Run every one of these
# with CAPTURE_OUT=$REPO/.planning/phases/21-deploy-render/21-LIVE-CAPTURES.md
# (check21 defaults to that file, never to the Phase 20 default OUT).
# ---------------------------------------------------------------------------
P21OUT=$REPO/.planning/phases/21-deploy-render/21-LIVE-CAPTURES.md
BIND21=${IGN_DEPLOY_RIG_RESOURCES:-$HOME/.cache/ign-deploy-rig/resources-21}
export IGN_DEPLOY_RIG_RESOURCES=$BIND21
# Exported so `bash -c '... "$IGN21_DIR/x" ...'` command lines stay free of local paths.
export IGN21_DIR=$STATE
export IGN21_BIND=$BIND21
export IGNITION_CLI_CONFIG=$STATE/ign21.toml
CRES=/usr/local/bin/ignition/data/config/resources
GWC=ign-deploy-rig-gw-1
GATEWAY=ign-deploy-rig

dc() { ( cd "$HERE" && docker compose -p ign-deploy-rig -f compose.yml -f compose.bind.yml "$@" ); }

# sect ID TEXT: the `## C21-<ID>` heading of one capture id.
sect() {
  case "${1:-}" in
    UUID8 | C3b | C4 | OQ8 | OQ1 | OQ4 | C5) ;;
    *) echo "sect: unknown capture id '${1:-}'" >&2; exit 64 ;;
  esac
  h "C21-$1 — ${2:-}"
}

# shcap LABEL CMD [ARG ...]: one verbatim command block. The fence opens with
# `$ <args rendered with printf %q> @ <UTC taken just before the run>`, then the
# combined stdout+stderr (CR removed), then `[exit N]`. The command runs with the
# bind directory as cwd so paths stay relative. Refuses (exit 2) when an argument
# or the output contains a token value; nothing is appended in that case.
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
  ( cd "$BIND21" 2>/dev/null || true; "$@" ) > "$out" 2>&1 || rc=$?
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
  {
    printf '\n### %s\n\n~~~\n$ ' "$label"
    printf '%q ' "$@"
    printf '@ %s\n' "$started"
    cat "$out.nocr"
    if [ -s "$out.nocr" ] && [ "$(tail -c1 "$out.nocr" | wc -l | tr -d ' ')" = 0 ]; then
      printf '\n[output did not end with a newline]\n'
    fi
    printf '[exit %s]\n~~~\n' "$rc"
  } >> "$OUT"
}

# v8uuid GATEWAY MODULE TYPE NAME: the D-03 derivation 21-07 implements in Rust.
v8uuid() {
  [ $# -eq 4 ] || { echo "usage: v8uuid GATEWAY MODULE TYPE NAME" >&2; exit 64; }
  python3 -I -c '
import hashlib, sys
g, m, t, n = sys.argv[1:5]
d = hashlib.sha256(b"ign-deploy:uuid:v1\0" + g.encode() + b"\0" + (m + "/" + t + "/" + n).encode()).digest()[:16]
b = bytearray(d)
b[6] = (b[6] & 0x0F) | 0x80
b[8] = (b[8] & 0x3F) | 0x80
x = b.hex()
print("-".join([x[:8], x[8:12], x[12:16], x[16:20], x[20:]]))
' "$@"
}

# bind_write REL SRC: copy a host file to $BIND21/REL (mkdir -p) and give the
# gateway user ownership of the whole collection directory (D11 pattern).
bind_write() {
  local rel=${1:?REL} src=${2:?SRC} coll
  mkdir -p "$BIND21/$(dirname "$rel")"
  cp "$src" "$BIND21/$rel"
  coll=${rel%%/*}
  dc exec -T -u root gw chown -R 2003:2003 "$CRES/$coll"
}

# scan21: wait out any active scan, POST scan/config, poll until scanActive is
# false and lastScanTimestamp advanced (Phase 20 D4), then log the final GET.
scan_state() { curl -sS -m 10 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$SCAN" || true; }
scan21() {
  local body ts0 ts t0
  t0=$(date +%s)
  while :; do
    body=$(scan_state)
    [ "$(jq -r '.scanActive' <<<"$body" 2>/dev/null || echo ?)" = false ] && break
    [ $(( $(date +%s) - t0 )) -lt 60 ] || { echo "scan stayed active for 60 s before the request" >&2; return 1; }
    sleep 0.5
  done
  ts0=$(jq -r '.lastScanTimestamp // 0' <<<"$body")
  cap 'POST scan/config' IGN_TOKEN_CAP_A POST $SCAN
  t0=$(date +%s)
  while :; do
    body=$(scan_state)
    ts=$(jq -r '.lastScanTimestamp // 0' <<<"$body" 2>/dev/null || echo 0)
    if [ "$(jq -r '.scanActive' <<<"$body" 2>/dev/null || echo ?)" = false ] && [ "$ts" -gt "$ts0" ] 2>/dev/null; then break; fi
    [ $(( $(date +%s) - t0 )) -lt 120 ] || { echo "scan did not complete within 120 s" >&2; return 1; }
    sleep 0.5
  done
  cap 'GET scan/config (after completion)' IGN_TOKEN_CAP_A GET $SCAN
}

# glog PATTERN: gateway log lines matching an extended regex.
glog() {
  shcap "gateway log lines matching: $1" bash -c 'docker logs "$1" 2>&1 | grep -E -- "$2" || true' _ "$GWC" "$1"
}

# up21: fresh bind dir (outside the repo), compose up with the bind override,
# wait RUNNING. The start time is taken when `up -d` is issued.
up21() {
  case "$BIND21" in
    "$HOME/.cache/ign-deploy-rig/"?*) ;;
    *) echo "REFUSING: bind dir '$BIND21' is not under \$HOME/.cache/ign-deploy-rig/" >&2; exit 2 ;;
  esac
  [ -e "$BIND21" ] && /bin/rm -rf "$BIND21"
  mkdir -p "$BIND21" "$STATE"
  utc > "$STATE/start-utc-21"
  dc up -d
  wait_running
}

# setup21: mint tokens, then put the gateway into production mode (Phase 20
# C-mode procedure) and confirm deploymentMode. No capture block is written.
setup21() {
  bash "$HERE/mint-token.sh" > /dev/null
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

# header21: write the header table (creates the file; refuses if it exists).
# Only after RUNNING and production are both confirmed live.
header21() {
  [ -f "$STATE/start-utc-21" ] || { echo "no start-utc-21: run up21 first" >&2; exit 1; }
  [ ! -e "$OUT" ] || { echo "REFUSING: $OUT already exists" >&2; exit 1; }
  curl -s -m 5 "$BASE/StatusPing" | grep -q RUNNING || { echo "gateway is not RUNNING" >&2; exit 1; }
  local mode image cid
  mode=$(curl -sS -m 10 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE/data/api/v1/gateway-info" | jq -r '.deploymentMode')
  [ "$mode" = production ] || { echo "deploymentMode is '$mode', not production" >&2; exit 1; }
  cid=$(docker ps -q --filter "label=com.docker.compose.project=ign-deploy-rig" --filter "label=com.docker.compose.service=gw")
  image=$(docker inspect -f '{{.Config.Image}}' "$cid")
  {
    printf '# Phase 21 — Live Captures (8.3.6)\n\n| Field | Value |\n| --- | --- |\n'
    printf '| Image | %s |\n' "$image"
    printf '| Compose variant | bind (compose.yml + compose.bind.yml) |\n'
    # shellcheck disable=SC2016
    printf '| Bind path (outside repo) | %s |\n' "${BIND21/#$HOME/\$HOME}"
    printf '| Gateway name | %s |\n' "$GATEWAY"
    printf '| deploymentMode | %s |\n' "$mode"
    printf '| Started (UTC) | %s |\n' "$(cat "$STATE/start-utc-21")"
    printf '| ign --version | %s |\n' "$(ign --version)"
  } > "$OUT"
}

# hdrrow KEY VALUE: add a row to the header table (script-written, not typed).
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

# check21 [--decisions] [ID ...]: structural evidence check; sends no request,
# prints PASS/FAIL lines and never block content. Reads CAPTURE_OUT, else the
# Phase 21 file.
check21() {
  local f=${CAPTURE_OUT:-$P21OUT}
  python3 -I - "$f" "$@" <<'PY'
import re, sys

path = sys.argv[1]
args = sys.argv[2:]
decisions = False
ids = []
for a in args:
    if a == "--decisions":
        decisions = True
    else:
        ids.append(a)
ALL = ["UUID8", "C3b", "C4", "OQ8", "OQ1", "OQ4", "C5"]
if not ids:
    ids = ALL

TS = r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ"
HTTP_FIRST = re.compile(r"(?:GET|POST|PUT|PATCH|DELETE) \S+ @ (" + TS + r")")
SH_FIRST = re.compile(r"\$ .+ @ (" + TS + r")")
STARTED = re.compile(r"\|\s*Started \(UTC\)\s*\|\s*(" + TS + r")\s*\|")

try:
    lines = open(path, encoding="utf-8", errors="replace").read().split("\n")
except OSError as e:
    print("FAIL file cannot-read %s" % e.__class__.__name__)
    sys.exit(1)

fails = 0


def out(ok, who, rule):
    global fails
    if not ok:
        fails += 1
    print("%s %s %s" % ("PASS" if ok else "FAIL", who, rule))


started = None
sections = []
cur = None
curblock = None
infence = False
decs_present = False
cds = []
curcd = None
for l in lines:
    if not infence:
        if started is None:
            m = STARTED.match(l)
            if m:
                started = m.group(1)
        if l.startswith("## "):
            fields = l.split()
            curblock = None
            curcd = None
            if len(fields) > 1 and fields[1].startswith("C21-"):
                cur = {"kind": "c21", "id": fields[1][4:], "blocks": []}
                sections.append(cur)
            elif len(fields) > 1 and fields[1] == "Decisions":
                cur = {"kind": "dec"}
                decs_present = True
            else:
                cur = {"kind": "other"}
        elif l.startswith("### ") and cur is not None and cur["kind"] == "dec":
            m = re.match(r"### CD(\d+)\b", l)
            if m:
                curcd = {"n": int(m.group(1)), "text": []}
                cds.append(curcd)
            else:
                curcd = None
        if l.startswith("~~~"):
            infence = True
            curblock = {"first": None, "seen": False}
            if cur is not None and cur["kind"] == "c21":
                cur["blocks"].append(curblock)
            if curcd is not None:
                curcd["text"].append(l)
            continue
        if curcd is not None:
            curcd["text"].append(l)
    else:
        if l.startswith("~~~"):
            infence = False
        elif curblock is not None and not curblock["seen"]:
            curblock["first"] = l
            curblock["seen"] = True
        if curcd is not None:
            curcd["text"].append(l)

out(started is not None, "header", "started-utc")

bad = {}
counts = {}
prev = started
for s in sections:
    i = s["id"]
    counts.setdefault(i, [0, 0])
    counts[i][0] += 1
    rules = bad.setdefault(i, [])
    if not s["blocks"]:
        rules.append("section-has-no-fenced-block")
    for b in s["blocks"]:
        counts[i][1] += 1
        f = b["first"] or ""
        m = HTTP_FIRST.fullmatch(f) or SH_FIRST.fullmatch(f)
        if not m:
            rules.append("block-first-line-not-command-at-utc")
            continue
        ts = m.group(1)
        if started is not None and ts < started:
            rules.append("timestamp-before-started")
        if prev is not None and ts < prev:
            rules.append("timestamp-decreases")
        prev = ts if prev is None or ts > prev else prev

passed = set()
reported = set()
for i in ids + [i for i in bad if i not in ids]:
    if i in reported:
        continue
    reported.add(i)
    if i not in counts:
        out(False, i, "section-present")
        continue
    rules = sorted(set(bad[i]))
    if rules:
        for r in rules:
            out(False, i, r)
    else:
        passed.add(i)
        out(True, i, "sections=%d blocks=%d" % tuple(counts[i]))

if decs_present:
    for cd in cds:
        cited = set(re.findall(r"C21-([A-Za-z0-9]+)", "\n".join(cd["text"])))
        out(bool(cited & passed), "CD%d" % cd["n"], "cites-passing-block")

if decisions:
    nums = sorted(cd["n"] for cd in cds)
    out(decs_present and nums == list(range(1, 8)), "Decisions", "CD1-CD7-exactly-once")

sys.exit(1 if fails else 0)
PY
}

CORPUS=$REPO/crates/ignition-core/tests/fixtures/deploy/corpus
DT=ignition/database-translator

# coll_of: the collection a create answered with (from $LAST_BODY).
coll_of() { jq -r '.changes[0].collection // "core"' <<<"$LAST_BODY" 2>/dev/null || echo core; }

# hand_resource_json OUT UUID [STALE]: the RESEARCH Pattern 4 synthesized
# resource.json, 2-space indent, no trailing newline. STALE adds a
# lastModification object and an all-zero lastModificationSignature (C3b).
hand_resource_json() {
  python3 -I - "$1" "$2" "${3:-}" <<'PY'
import json, sys
out, uuid, stale = sys.argv[1:4]
attrs = {}
if stale:
    attrs["lastModification"] = {"actor": "cap21", "timestamp": "2026-01-01T00:00:00Z"}
attrs["uuid"] = uuid
if stale:
    attrs["lastModificationSignature"] = "0" * 64
attrs["enabled"] = True
doc = {"scope": "A", "version": 1, "restricted": False, "overridable": True,
       "files": ["config.json"], "attributes": attrs}
open(out, "w", encoding="utf-8").write(json.dumps(doc, indent=2, ensure_ascii=False))
PY
}

# uuid8 (C21-UUID8, D-03): does 8.3.6 load a sha2-derived version-8 uuid?
uuid8() {
  local coll v8 body
  sect UUID8 'D-03 live check: a hand-written resource carrying a sha2-derived version-8 uuid'
  body=$(jq -c '[{name: "CAP21_API", config: .}]' "$CORPUS/$DT/POSTGRES/config.json")
  cap 'POST create database-translator CAP21_API through the config API (no collection)' IGN_TOKEN_CAP_A POST $RES/$DT "${JSON[@]}" --data "$body"
  coll=$(coll_of)
  shcap 'CAP21_API config.json as the gateway wrote it' cat "$coll/$DT/CAP21_API/config.json"
  shcap 'CAP21_API resource.json as the gateway wrote it' cat "$coll/$DT/CAP21_API/resource.json"
  shcap 'CAP21_API files with sha256' bash -c 'find . -path "*/database-translator/CAP21_API/*" -type f -exec shasum -a 256 {} + | sort -k2'
  v8=$(v8uuid "$GATEWAY" ignition database-translator CAP21_V8)
  shcap 'v8 uuid for CAP21_V8 (D-03 derivation)' v8uuid "$GATEWAY" ignition database-translator CAP21_V8
  cp "$BIND21/$coll/$DT/CAP21_API/config.json" "$TMPD/v8.config.json"
  hand_resource_json "$TMPD/v8.resource.json" "$v8"
  bind_write "production/$DT/CAP21_V8/config.json" "$TMPD/v8.config.json"
  bind_write "production/$DT/CAP21_V8/resource.json" "$TMPD/v8.resource.json"
  shcap 'CAP21_V8 resource.json (hand-written, before scan)' cat "production/$DT/CAP21_V8/resource.json"
  shcap 'CAP21_V8 sha256 (before scan)' shasum -a 256 "production/$DT/CAP21_V8/config.json" "production/$DT/CAP21_V8/resource.json"
  scan21
  cap 'GET find CAP21_V8 (collection=production)' IGN_TOKEN_CAP_A GET "$RES/find/$DT/CAP21_V8?collection=production"
  cap 'GET names (database-translator)' IGN_TOKEN_CAP_A GET "$RES/names/$DT"
  shcap 'CAP21_V8 sha256 (after scan)' shasum -a 256 "production/$DT/CAP21_V8/config.json" "production/$DT/CAP21_V8/resource.json"
  glog 'CAP21_V8|will not be loaded|CAP21_API'
}

# c3b (C21-C3b): omitted and stale signatures on hand-written resources.
c3b() {
  local v8 stale r1 r2
  sect C3b 'omitted vs stale lastModificationSignature on hand-written resource.json, and whether the gateway rewrites it'
  v8=$(v8uuid "$GATEWAY" ignition database-translator CAP21_STALE)
  shcap 'v8 uuid for CAP21_STALE' v8uuid "$GATEWAY" ignition database-translator CAP21_STALE
  cp "$BIND21/production/$DT/CAP21_V8/config.json" "$TMPD/stale.config.json"
  hand_resource_json "$TMPD/stale.resource.json" "$v8" stale
  bind_write "production/$DT/CAP21_STALE/config.json" "$TMPD/stale.config.json"
  bind_write "production/$DT/CAP21_STALE/resource.json" "$TMPD/stale.resource.json"
  r1=production/$DT/CAP21_V8/resource.json
  r2=production/$DT/CAP21_STALE/resource.json
  shcap 'CAP21_STALE resource.json (hand-written: stale lastModificationSignature)' cat "$r2"
  shcap 'sha256 of both hand-written resource.json (before scan)' shasum -a 256 "$r1" "$r2"
  scan21
  cap 'GET find CAP21_V8 (after scan 1)' IGN_TOKEN_CAP_A GET "$RES/find/$DT/CAP21_V8?collection=production"
  cap 'GET find CAP21_STALE (after scan 1)' IGN_TOKEN_CAP_A GET "$RES/find/$DT/CAP21_STALE?collection=production"
  shcap 'sha256 of both hand-written resource.json (after scan 1)' shasum -a 256 "$r1" "$r2"
  sleep 60
  scan21
  shcap 'sha256 of both hand-written resource.json (after scan 2, 60 s later)' shasum -a 256 "$r1" "$r2"
  shcap 'resource.json bytes after scan 2: CAP21_V8' cat "$r1"
  shcap 'resource.json bytes after scan 2: CAP21_STALE' cat "$r2"
  glog 'CAP21_V8|CAP21_STALE|will not be loaded|signature'
}

# c4_one MODULE TYPE NAME CONFIGJSON: create, then show what the gateway wrote.
c4_one() {
  local m=$1 t=$2 n=$3 cfg=$4 body coll
  body=$(jq -c --arg n "$n" --argjson c "$cfg" '[{name: $n, config: $c}]' <<<'null')
  cap "POST create $m/$t $n (minimal config, no secret values)" IGN_TOKEN_CAP_A POST "$RES/$m/$t" "${JSON[@]}" --data "$body"
  if [ "$(jq -r '.success // false' <<<"$LAST_BODY" 2>/dev/null)" = true ]; then
    coll=$(coll_of)
    shcap "$n files as the gateway wrote them" bash -c 'find . -path "*/$1/$2/*" -type f -exec shasum -a 256 {} + | sort -k2' _ "$t" "$n"
    shcap "$n resource.json as the gateway wrote it" cat "$coll/$m/$t/$n/resource.json"
  fi
}

# c4 (C21-C4): types the corpus and OpenAPI census do not cover.
c4() {
  sect C4 'uuid/scope census for types absent from the corpus: oauth2-client, alarm-notification-profile, roster-config, holiday, service-connector'
  c4_one ignition oauth2-client CAP21_OAUTH \
    '{"clientId":"cap21-client","clientSecret":null,"tokenUrl":"http://localhost/token","httpVersion":"HTTP_1_1","httpConnectTimeout":10000,"httpRequestTimeout":10000}'
  c4_one com.inductiveautomation.alarm-notification alarm-notification-profile CAP21_ANP \
    '{"profile":{"type":"OneWayNotificationProfileType"},"settings":{}}'
  c4_one ignition roster-config CAP21_ROSTER '{"users":[]}'
  c4_one ignition holiday CAP21_HOLIDAY '{"date":"2026-12-25","repeatAnnually":true}'
  c4_one ignition service-connector CAP21_SVC \
    '{"profile":{"type":"kafka"},"settings":{"bootstrapServers":["localhost:9092"],"saslMechanism":"PLAIN","securityProtocol":"PLAINTEXT"}}'
}

# c4b (C21-C4, second section): oauth2-client again, now with a Referenced
# clientSecret (run after oq8; the null form was refused with "Missing required
# property 'clientSecret'").
c4b() {
  sect C4 'oauth2-client retried with a Referenced clientSecret (no secret value)'
  c4_one ignition oauth2-client CAP21_OAUTH \
    '{"clientId":"cap21-client","clientSecret":{"type":"Referenced","data":{"providerName":"CAP21_SP_FILE","secretName":"cap21-secret"}},"tokenUrl":"http://localhost/token","httpVersion":"HTTP_1_1","httpConnectTimeout":10000,"httpRequestTimeout":10000}'
}

# oq8 (C21-OQ8): the gateway's own key order for a Referenced secret.
oq8() {
  local ref body coll
  sect OQ8 'Referenced secret: the gateway-written key order of {type, data{providerName, secretName}}'
  shcap 'throwaway secret value generated now, piped into a file inside the disposable rig container (value never printed or stored on the host)' \
    bash -c 'openssl rand -hex 16 | docker exec -i -u root "$1" sh -c "cat > /tmp/cap21-secret && chown 2003:2003 /tmp/cap21-secret && chmod 600 /tmp/cap21-secret"' _ "$GWC"
  cap 'POST create secret-provider CAP21_SP_INT (internal, no secrets)' IGN_TOKEN_CAP_A POST "$RES/ignition/secret-provider" "${JSON[@]}" \
    --data '[{"name":"CAP21_SP_INT","config":{"profile":{"type":"internal"},"settings":{"secrets":{}}}}]'
  cap 'POST create secret-provider CAP21_SP_FILE (file provider holding cap21-secret)' IGN_TOKEN_CAP_A POST "$RES/ignition/secret-provider" "${JSON[@]}" \
    --data '[{"name":"CAP21_SP_FILE","config":{"profile":{"type":"file"},"settings":{"files":{"cap21-secret":{"filePath":"/tmp/cap21-secret","fileType":"CLEARTEXT"}}}}}]'
  cap 'GET secret names of CAP21_SP_FILE' IGN_TOKEN_CAP_A GET /data/api/v1/secret-providers/CAP21_SP_FILE/secrets
  # Referenced, sent with type first and data{secretName, providerName} (reverse alphabetical).
  body=$(jq -c '[{name: "CAP21_REF", config: (. + {password: {type: "Referenced", data: {secretName: "cap21-secret", providerName: "CAP21_SP_FILE"}}})}]' "$DBCFG")
  cap 'POST create database-connection CAP21_REF (Referenced password; type first, secretName before providerName)' IGN_TOKEN_CAP_A POST "$RES/$DBT" "${JSON[@]}" --data "$body"
  coll=$(coll_of)
  shcap 'CAP21_REF config.json as the gateway wrote it' cat "$coll/$DBT/CAP21_REF/config.json"
  # Same reference sent with data first and providerName before secretName.
  body=$(jq -c '[{name: "CAP21_REF2", config: (. + {password: {data: {providerName: "CAP21_SP_FILE", secretName: "cap21-secret"}, type: "Referenced"}})}]' "$DBCFG")
  cap 'POST create database-connection CAP21_REF2 (Referenced password; data first, providerName before secretName)' IGN_TOKEN_CAP_A POST "$RES/$DBT" "${JSON[@]}" --data "$body"
  coll=$(coll_of)
  shcap 'CAP21_REF2 config.json as the gateway wrote it' cat "$coll/$DBT/CAP21_REF2/config.json"
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
  up21) up21 ;;
  setup21) setup21 ;;
  header21) header21 ;;
  hdrrow) hdrrow "${2:?KEY}" "${3:?VALUE}" ;;
  sect) sect "${2:?ID}" "${3:-}" ;;
  shcap) shift; shcap "$@" ;;
  v8uuid) shift; v8uuid "$@" ;;
  check21) shift; check21 "$@" ;;
  uuid8) uuid8 ;;
  c3b) c3b ;;
  c4) c4 ;;
  c4b) c4b ;;
  oq8) oq8 ;;
  *) echo "usage: capture.sh c1|cnull [LABEL]|cmode|c3a|clist|wait|up21|setup21|header21|hdrrow|shcap|v8uuid|check21|uuid8|c3b|c4|c4b|oq8" >&2; exit 64 ;;
esac
