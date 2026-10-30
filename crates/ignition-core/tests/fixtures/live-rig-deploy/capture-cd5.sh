#!/usr/bin/env bash
# capture-cd5.sh SUBCOMMAND
#
# Quick task 261030-8m8 rig capture driver: the two tag layouts that decision
# CD5 (21-LIVE-CAPTURES) left fail-closed, taken from a fresh 8.3.6 rig before
# any code relies on them.
#
#   header  write the capture doc header table (creates the file; refuses if
#           it exists) and the throwaway ign config this driver uses
#   run     section `## CD5-D`: S1 layout, S2 write, S3 change, S4 delete,
#           S5 unary-resource.json `files` probes, S6 a folder named tags.json
#   seed    S1's provider create and import only (no capture block; Task 3)
#   clean   delete providers CAP5, CAP5B and CAP5X through the API, remove
#           every file this driver wrote under production/, scan once, and
#           record that the providers are gone
#   flat | check   helpers run through shcap (export flattening and checks)
#
# Bring-up is capture22.sh's (`up22`, then `setup22`): the Phase 22 compose
# project `ign-deploy-rig`, bind-mounted resources, port 18290, token cap-a in
# $HOME/.cache/ign-deploy-rig/tokens.env (0600, outside the repo), production
# deployment mode. `ign rig` takes one compose file per rig and cannot express
# the compose.bind.yml overlay, so it is used here only for `status` and
# `logs`, through a throwaway config selected with IGNITION_CLI_CONFIG (never
# the user's config).
#
# Every byte inside a capture section is appended by `cap` (HTTP) or `shcap`
# (any other command) and is preceded by a line carrying the exact command and
# a UTC time taken by this script just before the run.
#
# LOCALHOST ONLY. assert_local runs before anything else (before any file is
# read or written, before any request) and exits 2 unless BASE's host is
# localhost, 127.0.0.1 or [::1]. The authority is cut at the first `/`, `?`,
# `#` or `\` BEFORE userinfo is stripped, so 'http://evil.example#@localhost'
# is refused (the WR-08 / R-01 fix). Tokens are sent as a request header or
# through the ign profile's --token-env and are never printed: `cap` prints
# the response only, `shcap` refuses any argument or output holding a token
# value. Never enable `set -x` here.
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
TASKDIR=$REPO/.planning/quick/261030-8m8-lift-cd5-root-level-tag-and-nested-udt-f
OUT=${CAPTURE_OUT:-$TASKDIR/261030-8m8-LIVE-CAPTURES.md}
STATE=${IGN_DEPLOY_RIG_STATE:-$HOME/.cache/ign-deploy-rig}
DATA=$STATE/data
BIND=$DATA/config/resources
export IGN_DEPLOY_RIG_RESOURCES=$BIND
export IGN_DEPLOY_RIG_STATE=$STATE
IGN=${IGN_BIN:-ign}
CAPTURED=$REPO/crates/ignition-core/tests/fixtures/deploy/captured
export CAPTURECD5=$HERE/capture-cd5.sh
TMPD=$(mktemp -d)
trap '/bin/rm -rf "$TMPD"' EXIT

# shellcheck disable=SC1091
[ -f "$STATE/tokens.env" ] && . "$STATE/tokens.env"

# The throwaway ign config: one profile for the rig (token from an env var)
# and one [rigs.ign-deploy-rig] entry. Never the user's config.
export IGNITION_CLI_CONFIG=$STATE/cd5-ign-config.toml
export IGN_CD5_TOKEN=${IGN_TOKEN_CAP_A:-}
PROF=cd5-rig

SCAN=/data/api/v1/scan/config
RES=/data/api/v1/resources
CRES=/usr/local/bin/ignition/data/config/resources
TT=(tag-provider tag-group tag-type-definition tag-definition)

utc() { date -u +%FT%TZ; }
now_ms() { perl -MTime::HiRes=time -e 'printf "%d", time*1000'; }
h() { printf '\n## %s\n' "$1" >> "$OUT"; }
sub() { printf '\n### %s\n' "$1" >> "$OUT"; }
# prose TEXT: a line of script-written commentary (never gateway bytes).
prose() { printf '\n%s\n' "$1" >> "$OUT"; }

dc() { ( cd "$HERE" && docker compose -p ign-deploy-rig -f compose.yml -f compose.bind.yml "$@" ); }

# cap LABEL METHOD PATH [curl args]: one verbatim HTTP exchange (token A). A
# line with the command and a script-taken UTC time precedes the fence. The
# command line names the token variable, never its value.
LAST_BODY=""
cap() {
  local label=$1 method=$2 path=$3; shift 3
  local tok started raw=$TMPD/raw
  tok=${IGN_TOKEN_CAP_A:?IGN_TOKEN_CAP_A is not set; run capture22.sh setup22}
  started=$(utc)
  curl -sS -g -i -m 90 -w '\n[curl http=%{http_code} total=%{time_total}s]\n' \
    -X "$method" -H "X-Ignition-API-Token: $tok" "$@" "$BASE$path" > "$raw" || true
  {
    printf '\n#### %s\n\n`curl -sS -g -i -X %s [token $IGN_TOKEN_CAP_A] %s%s` @ %s\n\n~~~\n' \
      "$label" "$method" "${*:+$* }" "'\$BASE$path'" "$started"
    tr -d '\r' < "$raw"
    printf '~~~\n'
  } >> "$OUT"
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

# scan1: ONE config scan, waited to completion (the scan23 shape): wait out
# any active scan, POST scan/config, poll until scanActive is false and
# lastScanTimestamp advanced, record the final GET. SCAN_MS is the epoch-ms
# just before the POST (the log window of the next scenario starts there).
SCAN_MS=0
PREV_SCAN_MS=0
scan1() {
  local body ts0 ts t0
  t0=$(date +%s)
  while :; do
    body=$(scan_state)
    [ "$(jq -r '.scanActive' <<<"$body" 2>/dev/null || echo ?)" = false ] && break
    [ $(( $(date +%s) - t0 )) -lt 60 ] || { echo "scan stayed active for 60 s before the request" >&2; return 1; }
    sleep 0.5
  done
  ts0=$(jq -r '.lastScanTimestamp // 0' <<<"$body")
  PREV_SCAN_MS=$SCAN_MS
  SCAN_MS=$(now_ms)
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

# chown_bind: give the gateway user the whole bind directory (Phase 21 D11).
chown_bind() { dc exec -T -u root gw chown -R 2003:2003 "$CRES"; }

newuuid() { python3 -I -c 'import uuid; print(uuid.uuid4())'; }

# ---------------------------------------------------------------- helpers

# The export flattener and row checker (stdin: a tags/export body, or an ign
# envelope around one). `flat` prints one compact line `path=value` for every
# AtomicTag (its config value: `defaultValue`, or `value` for a UDT type
# member; a differing runtime `value` is shown in parentheses) and
# `path<TagType>` for everything else; `check SPEC...` prints one `ok`/`FAIL`
# line per SPEC, where SPEC is `path=value` (that config value),
# `path` (present) or `!path` (absent).
PYFLAT='
import json, sys
raw = sys.stdin.read()
try:
    d = json.loads(raw)
except Exception:
    print("export_unreadable"); sys.exit(0)
if isinstance(d, dict) and isinstance(d.get("data"), dict) and "result" in d["data"]:
    r = d["data"]["result"]
    d = r.get("data") if isinstance(r, dict) else None
flat = {}
cfg = {}
def walk(n, pre):
    if not isinstance(n, dict):
        return
    for t in n.get("tags") or []:
        p = pre + str(t.get("name"))
        tt = t.get("tagType")
        if tt == "AtomicTag" and ("defaultValue" in t or "value" in t):
            cv = t["defaultValue"] if "defaultValue" in t else t["value"]
            cfg[p] = json.dumps(cv)
            flat[p] = "=" + cfg[p]
            if "defaultValue" in t and "value" in t and t["value"] != t["defaultValue"]:
                flat[p] += "(value=%s)" % json.dumps(t["value"])
        else:
            flat[p] = "<%s>" % tt
        walk(t, p + "/")
walk(d, "")
if sys.argv[1] == "flat":
    print(" ".join(k + v for k, v in sorted(flat.items())) or "<no tags>")
else:
    for s in sys.argv[2:]:
        if s.startswith("!"):
            ok = s[1:] not in flat; got = "absent" if ok else flat[s[1:]]
        elif "=" in s:
            k, v = s.split("=", 1); ok = cfg.get(k) == v; got = flat.get(k, "absent")
        else:
            ok = s in flat; got = flat.get(s, "absent")
        print("%s %s (observed %s)" % ("ok  " if ok else "FAIL", s, got))
'
export_body() { curl -sS -m 30 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE/data/api/v1/tags/export?provider=$1&type=json" || true; }
flat_of() { export_body "$1" | python3 -I -c "$PYFLAT" flat; }
# check_rows PROVIDER SPEC...: quiet; prints the check lines.
check_rows() { local p=$1; shift; export_body "$p" | python3 -I -c "$PYFLAT" check "$@"; }
provider_listed() {
  curl -sS -m 20 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$RES/list/ignition/tag-provider?limit=-1" \
    | jq -r --arg n "$1" '[.items[]? | select(.name == $n) | .collection] | if length == 0 then "no" else "yes(" + join(",") + ")" end' 2>/dev/null || echo unreadable
}

# probe P...: one compact line per provider, run through shcap.
probe() {
  local p
  for p in "$@"; do
    echo "provider=$p listed=$(provider_listed "$p") export: $(flat_of "$p")"
  done
}

# setval FILE TAGNAME OLD NEW: exact-lexeme edit (CD4: never re-serialize).
# Inside the innermost JSON object that holds `"name": "TAGNAME"`, replace
# `"defaultValue": OLD` (an atomic tag on disk) or `"value": OLD` (a UDT type
# member) with NEW; dies unless exactly one replacement.
setval() {
  perl -0pi -e '
    BEGIN { ($n, $o, $w) = splice(@ARGV, 1, 3); $c = 0 }
    s{(\{[^{}]*\})}{ my $b = $1; if ($b =~ /"name": "\Q$n\E"/) { $c += ($b =~ s/("(?:defaultValue|value)": )\Q$o\E(?=[,\s])/${1}$w/g) } $b }ge;
    END { die "setval: $c replacements of $n value $o (want 1)\n" unless $c == 1 }
  ' "$1" "$2" "$3" "$4"
}

# rig_logs / gw_warn: the gateway log at WARN and above since MS (REST, the
# counted source), and the container log through `ign rig logs` filtered to
# WARN/ERROR lines (the plan's form).
gw_warn() {
  shcap "gateway log, WARN and above since the previous scan (ign logs --since $1; timestamp, logger, level, message)" \
    bash -c '"$0" --profile "$1" --json logs --min-level warn --since "$2" --limit 200 | jq -c ".data.items[] | {timestamp, loggerName, level, message}"' "$IGN" "$PROF" "$1"
  shcap 'container log through ign rig logs --tail 400 gw, WARN/ERROR lines only (last 40)' \
    bash -c '"$0" rig --rig ign-deploy-rig logs --tail 400 gw 2>&1 | grep -wE "WARN|ERROR" | tail -40' "$IGN"
}

record_after() {
  local p
  cap 'GET resources/list/ignition/tag-provider' GET "$RES/list/ignition/tag-provider?limit=-1"
  for p in "$@"; do
    cap "GET tags/export?provider=$p&type=json" GET "/data/api/v1/tags/export?provider=$p&type=json"
  done
  shcap "probe: provider listing and every exported tag of $*" bash "$CAPTURECD5" probe "$@"
}

# rows LABEL PROVIDER SPEC...: one block of row checks (after a scan).
FAILS=0
rows() {
  local label=$1 p=$2; shift 2
  shcap "$label" bash "$CAPTURECD5" check "$p" "$@"
  local n
  n=$(grep -c '^FAIL' <<<"$LAST_OUT" || true)
  FAILS=$((FAILS + n))
}

# ladder SCENARIO CHECKFN: when a row failed after the one scan, the C11
# ladder: poll every 5 s for 30 s with no scan, then a second scan.
ladder() {
  local id=$1 fn=$2 i
  if [ "$FAILS" = 0 ]; then
    prose "Script reading ($id): every row matched its expectation after ONE scan; no ladder needed."
    return 0
  fi
  prose "Script reading ($id): $FAILS row(s) did not match after ONE scan; running the C11 ladder."
  sub "$id-ladder-a — no further scan: poll every 5 s for 30 s"
  for i in 1 2 3 4 5 6; do
    sleep 5
    FAILS=0
    prose "Poll at +$(( i * 5 )) s:"
    "$fn"
    if [ "$FAILS" = 0 ]; then prose "Script reading ($id): every row matched at +$(( i * 5 )) s with no second scan."; return 0; fi
  done
  sub "$id-ladder-b — a second scan"
  scan1
  FAILS=0
  "$fn"
  if [ "$FAILS" = 0 ]; then
    prose "Script reading ($id): every row matched after the SECOND scan."
  else
    prose "Script reading ($id): $FAILS row(s) still did not match after the second scan (never took effect within the ladder)."
  fi
}

# ------------------------------------------------------------- the config

ign_cfg() {
  [ -n "$IGN_CD5_TOKEN" ] || { echo "IGN_TOKEN_CAP_A is not set; run capture22.sh setup22" >&2; exit 1; }
  /bin/rm -f "$IGNITION_CLI_CONFIG"
  "$IGN" profile add "$PROF" "$BASE" --token-env IGN_CD5_TOKEN --active --json > /dev/null
  printf '\n[rigs.ign-deploy-rig]\ncompose_file = "%s"\nproject_name = "ign-deploy-rig"\n' "$HERE/compose.yml" >> "$IGNITION_CLI_CONFIG"
  chmod 600 "$IGNITION_CLI_CONFIG"
}

header() {
  [ -f "$STATE/start-utc-22" ] || { echo "no start-utc-22: run capture22.sh up22 first" >&2; exit 1; }
  [ ! -e "$OUT" ] || { echo "REFUSING: $OUT already exists" >&2; exit 1; }
  curl -s -m 5 "$BASE/StatusPing" | grep -q RUNNING || { echo "gateway is not RUNNING" >&2; exit 1; }
  local mode image cid
  mode=$(curl -sS -m 10 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE/data/api/v1/gateway-info" | jq -r '.deploymentMode')
  [ "$mode" = production ] || { echo "deploymentMode is '$mode', not production" >&2; exit 1; }
  cid=$(docker ps -q --filter "label=com.docker.compose.project=ign-deploy-rig" --filter "label=com.docker.compose.service=gw")
  image=$(docker inspect -f '{{.Config.Image}}' "$cid")
  ign_cfg
  mkdir -p "$(dirname "$OUT")"
  {
    printf '# Quick 261030-8m8 — Live Captures: root-level tags and nested UDT type folders (CD5a..CD5d)\n\n| Field | Value |\n| --- | --- |\n'
    printf '| Docker image | %s |\n' "$image"
    printf '| Compose variant | bind (compose.yml + compose.bind.yml), project ign-deploy-rig, 127.0.0.1:18290 |\n'
    # shellcheck disable=SC2016
    printf '| Bind path (outside repo) | %s |\n' "${BIND/#$HOME/\$HOME}"
    printf '| deploymentMode | %s |\n' "$mode"
    printf '| Rig started (UTC) | %s |\n' "$(cat "$STATE/start-utc-22")"
    printf '| Docker runner | capture-cd5.sh (crates/ignition-core/tests/fixtures/live-rig-deploy) |\n'
    printf '| Windows runner | capture22w.sh cd5w (crates/ignition-core/tests/fixtures/wflwvm01) |\n'
    printf '| ign --version | %s |\n' "$("$IGN" --version)"
    # shellcheck disable=SC2016
    printf '| ign config | throwaway `%s` selected with IGNITION_CLI_CONFIG (never the user config) |\n' "${IGNITION_CLI_CONFIG/#$HOME/\$HOME}"
    printf '\nRule: every captured block is preceded by the exact command and a UTC time taken by the driver just before it. A request line names the token variable it used, never the token. Tokens live in a 0600 file outside the repo; the VM key is a temp file shown as `<key>`.\n'
    printf '\nAccepted deviation (L1): the Docker rig is brought up with `capture22.sh up22` then `setup22` (the bind-mounted, production-mode rig), not with `ign rig up`. `ign rig` takes one compose file per rig and cannot express the `compose.bind.yml` overlay or the mode recreate; it is used here only for `status` and `logs`, through the throwaway config above.\n'
    printf '\nProviders and names are synthetic (CAP5, CAP5B, CAP5X; R1, RInst, F, F1, G, G1, T0, A, B, T, M0, M1, R9, X1). No production gateway is contacted and no production data is used.\n'
  } > "$OUT"
}

# ------------------------------------------------------------------- S1

S1_TYPES='{"tags":[{"name":"_types_","tagType":"Folder","tags":[{"name":"T0","tagType":"UdtType","tags":[{"name":"M0","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":0}]},{"name":"A","tagType":"Folder","tags":[{"name":"B","tagType":"Folder","tags":[{"name":"T","tagType":"UdtType","tags":[{"name":"M1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":1}]}]}]}]}]}'
S1_REST='{"tags":[{"name":"R1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":1},{"name":"RInst","tagType":"UdtInstance","typeId":"A/B/T"},{"name":"F","tagType":"Folder","tags":[{"name":"F1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":1}]},{"name":"G","tagType":"Folder","tags":[{"name":"G1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":1}]}]}'

import_block() {
  local label=$1 p=$2 body=$3
  shcap "$label" "$IGN" --profile "$PROF" --json api call --method POST --path /data/api/v1/tags/import \
    --query "provider=$p" --query type=json --query collisionPolicy=Abort \
    --header 'Content-Type: application/octet-stream' --data "$body"
}

seed_blocks() {
  shcap 'ign tags provider create CAP5' "$IGN" --profile "$PROF" --json tags provider create CAP5
  import_block 'native tags/import into CAP5: the _types_ folder (T0 flat; A/B/T nested)' CAP5 "$S1_TYPES"
  import_block 'native tags/import into CAP5: root R1 and RInst (of A/B/T), folders F (F1) and G (G1)' CAP5 "$S1_REST"
}

seed() {
  "$IGN" --profile "$PROF" --json tags provider create CAP5
  for b in "$S1_TYPES" "$S1_REST"; do
    "$IGN" --profile "$PROF" --json api call --method POST --path /data/api/v1/tags/import \
      --query provider=CAP5 --query type=json --query collisionPolicy=Abort \
      --header 'Content-Type: application/octet-stream' --data "$b"
  done
  sleep 20
  find "$BIND/core/ignition" -path '*/CAP5/*' -type f | sed "s|^$BIND/||" | LC_ALL=C sort
}

s1() {
  sub 'S1 — layout: what the gateway writes in core for root-level tags and nested UDT type folders'
  prose 'Provider CAP5 is created through the REST resources route (`ign tags provider create`), then two imports through the native route `POST /data/api/v1/tags/import?provider=CAP5&type=json&collisionPolicy=Abort` (the c5ntfs call form): the `_types_` folder first, then the root tags and folders, so the instance RInst is imported after its type exists. The asynchronous flush (CD4) is given 20 s.'
  local t_s1; t_s1=$(now_ms)
  seed_blocks
  prose 'Waiting 20 s for the gateway to flush the tag files (CD4).'
  sleep 20
  shcap 'sorted listing of every CAP5 file under the four tag types (relative to the bind dir)' \
    bash -c 'find core production -path "*/CAP5/*" -type f 2>/dev/null | LC_ALL=C sort'
  shcap 'every directory of CAP5 under the four tag types' \
    bash -c 'find core production -path "*/CAP5*" -type d 2>/dev/null | LC_ALL=C sort'
  shcap 'is there a _types_ directory anywhere on the bind mount?' \
    bash -c 'r=$(find . -name _types_ 2>/dev/null); echo "${r:-<none>}"'
  shcap 'every CAP5 file: size, sha256, then its bytes verbatim (a [eof] line follows each file; no newline is added inside)' \
    bash -c 'find core -path "*/CAP5/*" -type f | LC_ALL=C sort | while IFS= read -r f; do printf "== %s size=%s sha256=%s\n" "$f" "$(wc -c < "$f" | tr -d " ")" "$(shasum -a 256 "$f" | cut -d" " -f1)"; cat "$f"; printf "\n[eof]\n"; done'
  shcap 'tag-definition/CAP5/unary-resource.json (the provider-root marker)' cat core/ignition/tag-definition/CAP5/unary-resource.json
  shcap 'tag-type-definition/CAP5/unary-resource.json (the type-root marker)' cat core/ignition/tag-type-definition/CAP5/unary-resource.json
  shcap 'ls -la of tag-type-definition/CAP5/A (does the folder holding only a sub-folder carry its own marker?)' \
    ls -la core/ignition/tag-type-definition/CAP5/A
  if [ -f "$BIND/core/ignition/tag-type-definition/CAP5/A/unary-resource.json" ]; then
    prose 'Observation CD5b-A docker: marker'
  else
    prose 'Observation CD5b-A docker: none'
  fi
  cap 'GET resources/list/ignition/tag-provider' GET "$RES/list/ignition/tag-provider?limit=-1"
  cap 'GET tags/export?provider=CAP5&type=json' GET "/data/api/v1/tags/export?provider=CAP5&type=json"
  shcap 'probe: provider listing and every exported tag of CAP5' bash "$CAPTURECD5" probe CAP5
  gw_warn "$t_s1"
}

# ------------------------------------------------------------------- S2

s2_rows() {
  rows 'S2 rows CAP5B (production-only provider, C11 shape): CD5a R1 RInst; CD5b T (M1); control F1; standard G1 T0' \
    CAP5B R1=1 RInst F/F1=1 G/G1=1 _types_/T0 _types_/A/B/T/M1=1
  rows 'S2 rows CAP5 (production overrides): CD5a R1=2 and G1 not hidden; CD5b M1=2 and T0 not hidden; control F1=2' \
    CAP5 R1=2 G/G1=1 _types_/A/B/T/M1=2 _types_/T0 F/F1=2 RInst
  shcap 'S2 row: CAP5B is listed as a tag provider' bash -c 'echo "CAP5B listed: $1"' _ "$(provider_listed CAP5B)"
  case "$LAST_OUT" in *yes*) ;; *) FAILS=$((FAILS + 1)) ;; esac
}

s2() {
  sub 'S2 — write: plain file-level writes into production/, then ONE scan for all rows'
  prose '(a) Production-only provider CAP5B, the C11 shape: the gateway-written core files of CAP5 for all four tag types copied to `production/ignition/<type>/CAP5B/`, with a fresh uuid in the tag-provider `resource.json` (exact-lexeme edit). (b) Overrides of CAP5 in `production/`: the root unit of `tag-definition/CAP5` (marker, `tags.json`, `udts.json`) with R1 1 -> 2; the unit `tag-type-definition/CAP5/A/B/` with M1 1 -> 2; the control unit `tag-definition/CAP5/F/` with F1 1 -> 2. G is never written to `production/`. Every value edit is an exact-lexeme perl substitution on one file (CD4).'
  local u t
  u=$(newuuid)
  for t in "${TT[@]}"; do
    mkdir -p "$BIND/production/ignition/$t"
    cp -R "$BIND/core/ignition/$t/CAP5" "$BIND/production/ignition/$t/CAP5B"
  done
  perl -0pi -e 'BEGIN { $u = splice(@ARGV, 1, 1); $c = 0 } $c += s/("uuid"\s*:\s*")[0-9a-f-]{36}(")/$1$u$2/g; END { die "uuid replacements: $c\n" unless $c == 1 }' \
    "$BIND/production/ignition/tag-provider/CAP5B/resource.json" "$u"
  local P=$BIND/production/ignition C=$BIND/core/ignition
  mkdir -p "$P/tag-definition/CAP5/F" "$P/tag-type-definition/CAP5/A/B"
  cp "$C/tag-definition/CAP5/unary-resource.json" "$C/tag-definition/CAP5/tags.json" "$C/tag-definition/CAP5/udts.json" "$P/tag-definition/CAP5/"
  setval "$P/tag-definition/CAP5/tags.json" R1 1 2
  cp "$C/tag-type-definition/CAP5/A/B/udts.json" "$C/tag-type-definition/CAP5/A/B/unary-resource.json" "$P/tag-type-definition/CAP5/A/B/"
  setval "$P/tag-type-definition/CAP5/A/B/udts.json" M1 1 2
  cp "$C/tag-definition/CAP5/F/tags.json" "$C/tag-definition/CAP5/F/unary-resource.json" "$P/tag-definition/CAP5/F/"
  setval "$P/tag-definition/CAP5/F/tags.json" F1 1 2
  chown_bind
  shcap 'the files written into production/ (relative paths, sha256)' \
    bash -c 'find production -type f | LC_ALL=C sort | while IFS= read -r f; do printf "%s %s\n" "$(shasum -a 256 "$f" | cut -c1-16)" "$f"; done'
  shcap 'diff core vs production for the three CAP5 overrides and the CAP5B tag-provider resource.json (the edits only)' \
    bash -c 'for f in tag-definition/CAP5/tags.json tag-type-definition/CAP5/A/B/udts.json tag-definition/CAP5/F/tags.json; do diff core/ignition/$f production/ignition/$f; done; diff core/ignition/tag-provider/CAP5/resource.json production/ignition/tag-provider/CAP5B/resource.json | sed -E "s/[0-9a-f]{8}-[0-9a-f-]{27}/<uuid>/"; true'
  scan1
  record_after CAP5 CAP5B
  FAILS=0
  s2_rows
  gw_warn "$SCAN_MS"
  ladder S2 s2_rows
}

# ------------------------------------------------------------------- S3

s3_rows() {
  rows 'S3 rows CAP5B: CD5a R1=3; CD5b M1=3; control F1=3' CAP5B R1=3 _types_/A/B/T/M1=3 F/F1=3
  rows 'S3 rows CAP5: CD5a R1=3; CD5b M1=3; control F1=3' CAP5 R1=3 _types_/A/B/T/M1=3 F/F1=3
}

s3() {
  sub 'S3 — change: one member value edited in each production/ copy, then ONE scan'
  prose 'CAP5B R1 1 -> 3, CAP5B M1 1 -> 3, CAP5B F1 1 -> 3 (control); CAP5 overrides R1 2 -> 3, M1 2 -> 3, F1 2 -> 3 (control). Exact-lexeme edits.'
  local P=$BIND/production/ignition
  setval "$P/tag-definition/CAP5B/tags.json" R1 1 3
  setval "$P/tag-type-definition/CAP5B/A/B/udts.json" M1 1 3
  setval "$P/tag-definition/CAP5B/F/tags.json" F1 1 3
  setval "$P/tag-definition/CAP5/tags.json" R1 2 3
  setval "$P/tag-type-definition/CAP5/A/B/udts.json" M1 2 3
  setval "$P/tag-definition/CAP5/F/tags.json" F1 2 3
  chown_bind
  shcap 'the six edited files (value lines only)' \
    bash -c 'for f in tag-definition/CAP5B/tags.json tag-type-definition/CAP5B/A/B/udts.json tag-definition/CAP5B/F/tags.json tag-definition/CAP5/tags.json tag-type-definition/CAP5/A/B/udts.json tag-definition/CAP5/F/tags.json; do printf "%s: %s\n" "$f" "$(grep -o "\"value\": [0-9]*" production/ignition/$f | tr "\n" " ")"; done'
  scan1
  record_after CAP5 CAP5B
  FAILS=0
  s3_rows
  gw_warn "$SCAN_MS"
  ladder S3 s3_rows
}

# ------------------------------------------------------------------- S4

s4_rows() {
  rows 'S4 rows CAP5B: CD5a R1 RInst gone; CD5b T gone; control F1 gone' CAP5B '!R1' '!RInst' '!_types_/A/B/T' '!F/F1'
  shcap 'S4 row: CAP5B is still listed as a tag provider' bash -c 'echo "CAP5B listed: $1"' _ "$(provider_listed CAP5B)"
  case "$LAST_OUT" in *yes*) ;; *) FAILS=$((FAILS + 1)) ;; esac
  rows 'S4 rows CAP5: CD5a R1 RInst hidden by the production root unit, G1 still present; control F1 hidden' CAP5 '!R1' '!RInst' G/G1=1 '!F/F1'
}

s4() {
  sub 'S4 — delete: files removed from production/ (markers set to the gateway files: [] bytes), then ONE scan'
  prose 'CAP5B: root `tags.json` and `udts.json` removed, root marker replaced with the gateway `files: []` bytes (the CAP21 fixture shape, the consistent deploy case); `tag-type-definition/CAP5B/A/B/` removed whole; `tag-definition/CAP5B/F/` removed whole (control). CAP5: production root `tags.json` and `udts.json` removed and the production root marker set to `files: []`; the production `A/B/` override removed whole; production `F/tags.json` removed and the production F marker set to `files: []` (control).'
  local P=$BIND/production/ignition M=$CAPTURED/ignition/tag-definition/CAP21/unary-resource.json
  /bin/rm -f "$P/tag-definition/CAP5B/tags.json" "$P/tag-definition/CAP5B/udts.json"
  cp "$M" "$P/tag-definition/CAP5B/unary-resource.json"
  /bin/rm -rf "$P/tag-type-definition/CAP5B/A/B" "$P/tag-definition/CAP5B/F"
  /bin/rm -f "$P/tag-definition/CAP5/tags.json" "$P/tag-definition/CAP5/udts.json"
  cp "$M" "$P/tag-definition/CAP5/unary-resource.json"
  /bin/rm -rf "$P/tag-type-definition/CAP5/A/B"
  /bin/rm -f "$P/tag-definition/CAP5/F/tags.json"
  cp "$M" "$P/tag-definition/CAP5/F/unary-resource.json"
  chown_bind
  shcap 'production/ after the removals (relative paths)' bash -c 'find production -type f | LC_ALL=C sort'
  scan1
  record_after CAP5 CAP5B
  FAILS=0
  s4_rows
  gw_warn "$SCAN_MS"
  ladder S4 s4_rows
  rows 'S4 informational (not part of any verdict): with the production A/B unit removed, is T back to the core value M1=1?' CAP5 _types_/A/B/T/M1=1
}

# ------------------------------------------------------------------- S5

s5() {
  sub 'S5 — does the gateway honour unary-resource.json `files`? (OQ3, CD5c; on CAP5B)'
  local P=$BIND/production/ignition/tag-definition/CAP5B
  prose '(i) A root `tags.json` holding AtomicTag R9 (the gateway-written CAP5 root `tags.json` with the name lexeme R1 -> R9) written while the root marker still says `files: []`; ONE scan. Is R9 loaded?'
  perl -0pe 's/"name": "R1"/"name": "R9"/ or die "no R1 name\n"' "$BIND/core/ignition/tag-definition/CAP5/tags.json" > "$P/tags.json"
  chown_bind
  shcap 'S5(i) production CAP5B root: files and the marker' bash -c 'ls -la production/ignition/tag-definition/CAP5B; cat production/ignition/tag-definition/CAP5B/unary-resource.json; echo; cat production/ignition/tag-definition/CAP5B/tags.json; echo'
  scan1
  record_after CAP5B
  FAILS=0
  rows 'S5(i) row: is R9 loaded while the marker lists no files?' CAP5B R9=1
  gw_warn "$SCAN_MS"
  prose '(ii) The root marker set to list `tags.json` (exact-lexeme edit of `"files": []`) and `tags.json` removed; ONE scan. Is R9 gone or still loaded, and is a WARN logged naming the resource?'
  perl -0pi -e 's/"files": \[\]/"files": [\n    "tags.json"\n  ]/ or die "no files: []\n"' "$P/unary-resource.json"
  /bin/rm -f "$P/tags.json"
  chown_bind
  shcap 'S5(ii) production CAP5B root: files and the marker' bash -c 'ls -la production/ignition/tag-definition/CAP5B; cat production/ignition/tag-definition/CAP5B/unary-resource.json; echo'
  scan1
  record_after CAP5B
  FAILS=0
  rows 'S5(ii) row: is R9 gone after tags.json is removed while the marker lists it?' CAP5B '!R9'
  gw_warn "$SCAN_MS"
}

# ------------------------------------------------------------------- S6

s6() {
  sub 'S6 — a tag folder named tags.json (A5, CD5d; Docker only)'
  prose 'Provider CAP5X is created, root AtomicTag R1 is imported, then a Folder named `tags.json` holding X1. 20 s flush, then the listing and the log.'
  local t6; t6=$(now_ms)
  shcap 'ign tags provider create CAP5X' "$IGN" --profile "$PROF" --json tags provider create CAP5X
  import_block 'native tags/import into CAP5X: root R1' CAP5X '{"tags":[{"name":"R1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":1}]}'
  import_block 'native tags/import into CAP5X: a Folder named tags.json holding X1' CAP5X '{"tags":[{"name":"tags.json","tagType":"Folder","tags":[{"name":"X1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":1}]}]}'
  prose 'Waiting 20 s for the gateway to flush the tag files (CD4).'
  sleep 20
  shcap 'every entry under core/ignition/tag-definition/CAP5X (type d/f and path)' \
    bash -c 'find core/ignition/tag-definition/CAP5X | LC_ALL=C sort | while IFS= read -r p; do if [ -d "$p" ]; then echo "d $p"; else echo "f $p"; fi; done'
  shcap 'the CAP5X files, verbatim' \
    bash -c 'find core/ignition/tag-definition/CAP5X -type f | LC_ALL=C sort | while IFS= read -r f; do printf "== %s\n" "$f"; cat "$f"; printf "\n[eof]\n"; done'
  cap 'GET tags/export?provider=CAP5X&type=json' GET "/data/api/v1/tags/export?provider=CAP5X&type=json"
  shcap 'probe: CAP5X' bash "$CAPTURECD5" probe CAP5X
  gw_warn "$t6"
  if [ -d "$BIND/core/ignition/tag-definition/CAP5X/tags.json" ]; then
    prose 'Script reading (S6): a directory `ignition/tag-definition/CAP5X/tags.json/` exists.'
  else
    prose 'Script reading (S6): no directory `ignition/tag-definition/CAP5X/tags.json/` exists.'
  fi
}

# ---------------------------------------------------------------- run

run() {
  [ -f "$OUT" ] || { echo "no $OUT: run header first" >&2; exit 1; }
  if grep -q '^## CD5-D' "$OUT"; then echo "REFUSING: $OUT already holds ## CD5-D" >&2; exit 2; fi
  h 'CD5-D'
  prose 'Docker 8.3.6 rig, production deployment mode, bind-mounted resources. Expectations are the deploy model'"'"'s: a mode-collection unit hides the `core` files it does not hold (`plan.rs` `is_tag_folder` / `CoreRelation::Hidden`). Each scan is ONE `POST /data/api/v1/scan/config` waited to completion. A row that does not match after the one scan gets the C11 ladder (poll every 5 s for 30 s, then a second scan).'
  shcap 'ign --json rig --rig ign-deploy-rig status (through the throwaway config)' "$IGN" --json rig --rig ign-deploy-rig status
  cap 'GET gateway-info' GET /data/api/v1/gateway-info
  s1
  s2
  s3
  s4
  s5
  s6
}

# ---------------------------------------------------------------- clean

clean() {
  [ -f "$OUT" ] || { echo "no $OUT" >&2; exit 1; }
  sub 'CD5-D-clean — production/ files removed and scanned, providers CAP5, CAP5B and CAP5X deleted, leftovers swept, one scan'
  prose 'Order matters: a provider deleted through the API while a production/ override of one of its tag units exists loses the override but keeps the core copy of that unit on disk (seen in CD5-D-clean-1), and a later create of the same name loads it again. So the production/ files go first, with a scan, then the API delete, then any core file left for these providers is removed.'
  local p t
  for t in "${TT[@]}"; do
    /bin/rm -rf "$BIND/production/ignition/$t/CAP5" "$BIND/production/ignition/$t/CAP5B" "$BIND/production/ignition/$t/CAP5X"
  done
  find "$BIND/production/ignition" -mindepth 1 -type d -empty -delete 2>/dev/null || true
  shcap 'production/ after removing every CAP5* file this driver wrote' bash -c 'find production -type f | LC_ALL=C sort'
  scan1
  for p in CAP5X CAP5B CAP5; do
    if [ "$(provider_listed "$p")" = no ]; then
      shcap "is $p still a tag provider? (no API delete needed when it is not)" bash -c 'echo "$1 listed: $2"' _ "$p" "$(provider_listed "$p")"
    else
      shcap "ign tags provider delete $p --yes" "$IGN" --profile "$PROF" --json --yes tags provider delete "$p"
    fi
  done
  sleep 5
  shcap 'anything named CAP5* left on the bind mount after the API deletes' bash -c 'r=$(find . -name "CAP5*" 2>/dev/null | LC_ALL=C sort); echo "${r:-<none>}"'
  for t in "${TT[@]}"; do
    /bin/rm -rf "$BIND/core/ignition/$t/CAP5" "$BIND/core/ignition/$t/CAP5B" "$BIND/core/ignition/$t/CAP5X"
  done
  scan1
  cap 'GET resources/list/ignition/tag-provider (CAP5, CAP5B, CAP5X gone?)' GET "$RES/list/ignition/tag-provider?limit=-1"
  shcap 'anything named CAP5* left on the bind mount (after the sweep)' bash -c 'r=$(find . -name "CAP5*" 2>/dev/null | LC_ALL=C sort); echo "${r:-<none>}"'
  shcap 'production/ after the cleanup' bash -c 'find production -type f | LC_ALL=C sort'
  shcap 'probe: are CAP5, CAP5B and CAP5X gone?' bash "$CAPTURECD5" probe CAP5 CAP5B CAP5X
}

case "${1:-}" in
  header) header ;;
  run) run ;;
  seed) seed ;;
  clean) clean ;;
  probe) shift; probe "$@" ;;
  check) shift; check_rows "$@" ;;
  cfg) ign_cfg ;;
  *) echo "usage: capture-cd5.sh header|run|seed|clean" >&2; exit 64 ;;
esac
