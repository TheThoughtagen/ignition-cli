#!/usr/bin/env bash
# shellcheck disable=SC2016 # single-quoted inner scripts and literal $NAME labels are intended
# capture26-pair.sh SUBCOMMAND
#
# Phase 26 (26-02) pair capture driver, TAGDEP-00 on the disposable two-node
# 8.3.6 pair of setup-pair.sh (compose project `ign-pair-rig`, gw-a Master on
# 127.0.0.1:18300, gw-b Backup on 127.0.0.1:18301, both in deployment mode
# `production`). Every write goes to gw-a; the backup is only read, and the
# driver records how long gw-b takes to show what gw-a shows.
#
#   header26p           the `## TAGDEP-00 pair (8.3.6)` header (refuses when the
#                       section already exists)
#   p26-create          Abort import of a folder plus two memory tags (P26P1)
#   p26-change          seed, a stand-in runtime value change, the pre-image
#                       export, then a value-less MergeOverwrite change of two
#                       tags and two UDT instances (P26P2, kept for the rollback)
#   p26-delete          records why no import delete is captured (A2 fails, C4/C5)
#   p26-provider        tag-provider settings modify on gw-a (the C11 PUT, P26P4)
#   p26-groups          a tag-group unit written to gw-a's mode collection plus a
#                       config scan (the C14 form), then a tag that uses it (P26P5)
#   p26-rollback-shape  the P26P2 pre-image re-imported under Overwrite, value
#                       keys stripped for V and I1, live values carried for V2
#                       and I2; then P26P2 is deleted
#   down26p             prints the teardown command; it does not run it
#
# Bring the pair up first (fixtures/live-rig-pair):
#   ./setup-pair.sh up && ./setup-pair.sh mint && ./setup-pair.sh pair && ./setup-pair.sh mode
#
# Convergence: after a write on gw-a the driver reads both nodes'
# `tags/export?provider=P&type=json` every second for up to 120 s and compares
# them as canonical JSON (keys sorted with `jq -S`, children of every `tags`
# array sorted by name, since the nodes may list them in a different order).
# It records the first
# second the two are equal, and separately the first second they are equal
# with `value`, `timestamp` and `quality` removed from AtomicTag objects.
#
# NODE B IS READ ONLY. Every request goes through `http`, which refuses
# (exit 2, before curl runs) any method but GET for node b and any curl argument
# that would send a body or change the method. Files are listed from the
# bind-mounted resources directories on the host, never written on gw-b's side.
#
# LOCALHOST ONLY. assert_local runs on both base URLs before anything else
# (before any file is read or written, before any request) and exits 2 unless
# the host is localhost, 127.0.0.1 or [::1]. The authority is cut at the first
# `/`, `?`, `#` or `\` BEFORE userinfo is stripped, so
# 'http://evil.example#@localhost' is refused (the R-01 / WR-08 fix of Phase 21).
# Tokens come from tokens.env (0600, outside the repo), are sent as a request
# header and are never printed: a command line names the token variable, never
# its value, and a block holding a token value is refused and not appended.
# Never enable `set -x` here. Names and values are synthetic.
set -euo pipefail

BASE_A=${BASE_A:-http://localhost:18300}
BASE_B=${BASE_B:-http://localhost:18301}

assert_local() {
  local name=$1 url=$2 rest authority hostport host
  case "$url" in
    *[[:space:]]* | *[[:cntrl:]]*) echo "REFUSING: $name holds whitespace or a control character" >&2; exit 2 ;;
  esac
  rest=${url#*://}
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
OUT=${CAPTURE_OUT:-$REPO/.planning/phases/26-project-owned-tag-deploy/26-LIVE-CAPTURES.md}
STATE=${IGN_PAIR_RIG_STATE:-$HOME/.cache/ign-pair-rig}
PROJECT=ign-pair-rig
BIND_A=$STATE/data-a/config/resources
BIND_B=$STATE/data-b/config/resources
CRES=/usr/local/bin/ignition/data/config/resources
PREIMAGE=$STATE/p26-preimage-P26P2.json
CONVERGE_S=${CONVERGE_S:-120}

# shellcheck disable=SC1091
[ -f "$STATE/tokens.env" ] && . "$STATE/tokens.env"

TMPD=$(mktemp -d)
trap '/bin/rm -rf "$TMPD"' EXIT

RES=/data/api/v1/resources
IMP=/data/api/v1/tags/import
EXP=/data/api/v1/tags/export
SCAN=/data/api/v1/scan/config
RED=/data/api/v1/redundancy

utc() { date -u +%FT%TZ; }
now_ms() { perl -MTime::HiRes=time -e 'printf "%d", time*1000'; }
secs() { awk -v m="$1" 'BEGIN { printf "%.1f", m / 1000 }'; }
sub() { printf '\n### %s\n' "$1" >> "$OUT"; }
note() { printf '\n%s\n' "$1" >> "$OUT"; }
reading() { note "Script reading: $1"; }
die() { echo "$*" >&2; exit 64; }

node_base() { case "$1" in a) printf '%s' "$BASE_A" ;; b) printf '%s' "$BASE_B" ;; *) echo "bad node '$1' (a or b)" >&2; exit 64 ;; esac; }
node_basevar() { case "$1" in a) printf 'BASE_A' ;; b) printf 'BASE_B' ;; *) echo "bad node '$1' (a or b)" >&2; exit 64 ;; esac; }
node_tokvar() { case "$1" in a) printf 'IGN_PAIR_TOKEN_A' ;; b) printf 'IGN_PAIR_TOKEN_B' ;; *) echo "bad node '$1' (a or b)" >&2; exit 64 ;; esac; }

# guard_b NODE METHOD [curl args]: node b (the backup) takes GET only, with no
# argument that sends a body or overrides the method. Exits 2 before any curl.
guard_b() {
  local node=$1 method=$2 a; shift 2
  [ "$node" = b ] || return 0
  [ "$method" = GET ] || { echo "REFUSING: $method to node b; the backup is read only (GET)" >&2; exit 2; }
  for a in "$@"; do
    case "$a" in
      -d | --data* | -F | --form* | -T | --upload-file | -X | --request | --json | -G | --get)
        echo "REFUSING: curl argument '$a' to node b; the backup is read only (GET)" >&2; exit 2 ;;
    esac
  done
}

# refuse_if_token FILE: exit 2, appending nothing, when FILE holds a token key.
refuse_if_token() {
  local v val
  for v in IGN_PAIR_TOKEN_A IGN_PAIR_TOKEN_B; do
    val=${!v:-}
    [ -z "$val" ] || ! grep -qF -- "${val#*:}" "$1" || { echo "REFUSING: output contains the value of $v; nothing appended" >&2; exit 2; }
  done
}

# http NODE METHOD PATH [curl args]: the one place a request is sent. Body in
# $TMPD/bod, headers in $TMPD/hdr, status in LAST_STATUS, curl time in LAST_TIME.
LAST_STATUS=""
LAST_TIME=""
http() {
  local node=$1 method=$2 path=$3; shift 3
  guard_b "$node" "$method" "$@"
  local base tv tok meta
  base=$(node_base "$node"); tv=$(node_tokvar "$node")
  tok=${!tv:?$tv is not set; run setup-pair.sh mint}
  /bin/rm -f "$TMPD/bod" "$TMPD/hdr"
  meta=$(curl -sS -g -D "$TMPD/hdr" -o "$TMPD/bod" -m "${CAP_MAX:-90}" -w '%{http_code} %{time_total}' \
    -X "$method" -H "X-Ignition-API-Token: $tok" "$@" "$base$path" 2>/dev/null || true)
  touch "$TMPD/bod" "$TMPD/hdr"
  LAST_STATUS=${meta%% *}
  LAST_TIME=${meta##* }
  [ -n "$LAST_STATUS" ] || LAST_STATUS=000
}

# q NODE METHOD PATH [curl args]: http, body to stdout, unrecorded.
q() { http "$@"; cat "$TMPD/bod"; }

# cap NODE LABEL METHOD PATH [curl args]: one recorded exchange. The command
# line names the token and base variables, never their values. CAP_BODY_MAX,
# when set, shows only that many body bytes plus the size and sha256.
cap() {
  local node=$1 label=$2 method=$3 path=$4; shift 4
  local started args blk=$TMPD/blk
  args="${*//$TMPD/\$TMPD}"
  started=$(utc)
  http "$node" "$method" "$path" "$@"
  {
    printf '\n#### %s\n\n`curl -sS -g -i -X %s [token $%s] %s%s` @ %s\n\n~~~\n' \
      "$label" "$method" "$(node_tokvar "$node")" "${args:+$args }" "'\$$(node_basevar "$node")$path'" "$started"
    tr -d '\r' < "$TMPD/hdr"
    if [ -n "${CAP_BODY_MAX:-}" ] && [ "$(wc -c < "$TMPD/bod" | tr -d ' ')" -gt "$CAP_BODY_MAX" ]; then
      head -c "$CAP_BODY_MAX" "$TMPD/bod" | tr -d '\r'
      printf '\n[body truncated to %s bytes; full body %s bytes, sha256 %s]\n' \
        "$CAP_BODY_MAX" "$(wc -c < "$TMPD/bod" | tr -d ' ')" "$(shasum -a 256 "$TMPD/bod" | cut -d' ' -f1)"
    else
      tr -d '\r' < "$TMPD/bod"
    fi
    printf '\n[curl http=%s total=%ss]\n~~~\n' "$LAST_STATUS" "$LAST_TIME"
  } > "$blk"
  refuse_if_token "$blk"
  cat "$blk" >> "$OUT"
}

# shcap LABEL CMD [ARG ...]: a local command run in $STATE, output verbatim.
shcap() {
  local label=$1; shift
  local started rc=0 out=$TMPD/sh
  started=$(utc)
  ( cd "$STATE" && "$@" ) > "$out" 2>&1 || rc=$?
  refuse_if_token "$out"
  {
    printf '\n#### %s\n\n`' "$label"
    printf '%q ' "$@" | sed "s|$TMPD|\$TMPD|g"
    printf '` (in $STATE) @ %s\n\n~~~\n' "$started"
    cat "$out"
    printf '[exit %s]\n~~~\n' "$rc"
  } >> "$OUT"
}

# block LABEL FILE: a script-produced block (poll lines), with the time it was
# taken; never gateway bytes beyond what the lines name.
block() {
  refuse_if_token "$2"
  { printf '\n#### %s (driver poll log, taken %s)\n\n~~~\n' "$1" "$(utc)"; cat "$2"; printf '~~~\n'; } >> "$OUT"
}

body_block() {
  local label=$1 file=$2
  {
    printf '\n#### %s (request body)\n\n~~~json\n' "$label"
    jq . "$file" 2>/dev/null || cat "$file"
    printf '~~~\n'
  } >> "$OUT"
}

# assert_rig: gw-a is the rig's Master `ign-pair-a`; gw-b is its Backup (a
# backup answers gateway-info with the pair's name, so its name is ign-pair-a
# too).
assert_rig() {
  local na nb ra rb
  na=$(q a GET /data/api/v1/gateway-info | jq -r '.name // empty' 2>/dev/null || true)
  nb=$(q b GET /data/api/v1/gateway-info | jq -r '.name // empty' 2>/dev/null || true)
  ra=$(q a GET "$RED" | jq -r '.role // empty' 2>/dev/null || true)
  rb=$(q b GET "$RED" | jq -r '.role // empty' 2>/dev/null || true)
  [ "$na" = ign-pair-a ] && [ "$nb" = ign-pair-a ] && [ "$ra" = Master ] && [ "$rb" = Backup ] || {
    echo "REFUSING: gateways name '${na:-?}' / '${nb:-?}', roles '${ra:-?}' / '${rb:-?}'; not the pair rig" >&2; exit 2; }
}

own() { case "$1" in P26P[0-9]*) ;; *) die "'$1' is not a P26P* provider" ;; esac; }

# red_status LABEL: both nodes' redundancy status, one compact line each.
red_status() {
  local f=$TMPD/red n
  : > "$f"
  for n in a b; do
    printf 'gw-%s %s\n' "$n" "$(q "$n" GET "$RED" | jq -c '{role, activityLevel, peerConnected, syncPending, projectState, lastSyncTimestamp}' 2>/dev/null || echo '?')" >> "$f"
  done
  block "redundancy status of both nodes: $1" "$f"
  cp "$f" "$TMPD/red-last"
}
in_sync() {
  local n s
  for n in a b; do
    s=$(q "$n" GET "$RED" | jq -r '"\(.peerConnected) \(.syncPending) \(.projectState)"' 2>/dev/null || echo '?')
    [ "$s" = "true false Good" ] || return 1
  done
}

STRIP='def s: if type == "object" then (if .tagType == "AtomicTag" then del(.value, .timestamp, .quality) else . end) | with_entries(.value |= s) elif type == "array" then map(s) else . end; s'
# KEYED: children of every `tags` array keyed by name (sorted by name), so two
# nodes that return the same children in a different order compare equal.
KEYED='def k: if type == "object" then with_entries(.value |= k) | (if (.tags | type) == "array" then .tags |= sort_by(.name) else . end) elif type == "array" then map(k) else . end; k'
raw_sorted() { jq -S . "$1" 2>/dev/null | shasum -a 256 | cut -c1-12; }
canon() { jq -S "$KEYED" "$1" 2>/dev/null | shasum -a 256 | cut -c1-12; }
canon_nv() { jq -S "($STRIP) | ($KEYED)" "$1" 2>/dev/null | shasum -a 256 | cut -c1-12; }

# converge P LABEL T0_MS: poll both exports every second for CONVERGE_S
# seconds from T0_MS (the moment gw-a answered the write). Sets CONV_EXACT and
# CONV_NV to the seconds after T0_MS, or "none", and leaves the last exports in
# $TMPD/ea-P.json / $TMPD/eb-P.json.
CONV_EXACT=none
CONV_NV=none
CONV_ORDER=none
converge() {
  local p=$1 label=$2 t0=$3 log=$TMPD/conv t sa sb na nb sta stb oa ob
  CONV_EXACT=none; CONV_NV=none; CONV_ORDER=none
  : > "$log"
  while :; do
    http a GET "$EXP?provider=$p&type=json"; sta=$LAST_STATUS; cp "$TMPD/bod" "$TMPD/ea-$p.json"
    http b GET "$EXP?provider=$p&type=json"; stb=$LAST_STATUS; cp "$TMPD/bod" "$TMPD/eb-$p.json"
    t=$(( $(now_ms) - t0 ))
    sa=$(canon "$TMPD/ea-$p.json"); sb=$(canon "$TMPD/eb-$p.json")
    na=$(canon_nv "$TMPD/ea-$p.json"); nb=$(canon_nv "$TMPD/eb-$p.json")
    oa=$(raw_sorted "$TMPD/ea-$p.json"); ob=$(raw_sorted "$TMPD/eb-$p.json")
    printf '+%ss a=%s b=%s exact a=%s b=%s %s | no-value a=%s b=%s %s | array order %s\n' "$(secs "$t")" "$sta" "$stb" \
      "$sa" "$sb" "$([ "$sa" = "$sb" ] && echo EQUAL || echo differ)" "$na" "$nb" "$([ "$na" = "$nb" ] && echo EQUAL || echo differ)" \
      "$([ "$oa" = "$ob" ] && echo same || echo differs)" >> "$log"
    if [ "$sta" = 200 ] && [ "$stb" = 200 ]; then
      [ "$CONV_NV" != none ] || [ "$na" != "$nb" ] || CONV_NV=$(secs "$t")
      if [ "$sa" = "$sb" ]; then CONV_EXACT=$(secs "$t"); [ "$oa" = "$ob" ] && CONV_ORDER=same || CONV_ORDER=differs; break; fi
    fi
    [ "$t" -lt $(( CONVERGE_S * 1000 )) ] || break
    sleep 1
  done
  block "$label: gw-a and gw-b export of $p, canonical sha256 (first 12) per poll" "$log"
  local verdict_exact=${CONV_EXACT} verdict_nv=${CONV_NV}
  [ "$verdict_exact" != none ] && verdict_exact="$verdict_exact s" || verdict_exact="did not converge within $CONVERGE_S s"
  [ "$verdict_nv" != none ] && verdict_nv="$verdict_nv s" || verdict_nv="did not converge within $CONVERGE_S s"
  reading "$label: gw-b's export equal to gw-a's (canonical, every key) after $verdict_exact; equal with AtomicTag value keys removed after $verdict_nv; at the first equal poll the two nodes' child order was: $CONV_ORDER."
}

# pick FILE PATH KEY: KEY of the node at PATH in an export, compact JSON, or
# <absent node> / <absent key>.
pick() {
  python3 -I - "$1" "$2" "$3" <<'PY'
import json, sys
f, path, key = sys.argv[1:4]
try:
    node = json.load(open(f, encoding="utf-8"))
except Exception:
    print("<export not JSON>"); sys.exit()
for seg in [s for s in path.split("/") if s]:
    kids = [c for c in node.get("tags", []) if c.get("name") == seg]
    if not kids:
        print("<absent node>"); sys.exit()
    node = kids[0]
if key == ".":
    print(json.dumps(node, sort_keys=True, separators=(",", ":")))
elif key in node:
    print(json.dumps(node[key], sort_keys=True, separators=(",", ":")))
else:
    print("<absent key>")
PY
}
both() { # P PATH KEY: "gw-a X, gw-b Y" from the last converge exports
  printf 'gw-a %s, gw-b %s' "$(pick "$TMPD/ea-$1.json" "$2" "$3")" "$(pick "$TMPD/eb-$1.json" "$2" "$3")"
}

# imp LABEL P POLICY BODY: one tags/import on gw-a at the provider root, the
# body shown first; IMP_T0 is the moment the answer arrived.
IMP_T0=0
imp() {
  local label=$1 p=$2 policy=$3 f=$TMPD/req.json
  own "$p"
  printf '%s' "$4" > "$f"
  body_block "$label" "$f"
  cap a "$label" POST "$IMP?provider=$p&type=json&collisionPolicy=$policy" -H 'Content-Type: application/octet-stream' --data-binary "@$f"
  IMP_T0=$(now_ms)
}

# files P: every file of provider P in both collections on both nodes.
files() {
  shcap "files of $1 in core and production on both nodes (size, sha256)" bash -c '
    for n in a b; do
      ( cd "data-$n/config/resources" && find core production -path "*/'"$1"'/*" -type f 2>/dev/null | LC_ALL=C sort |
        while IFS= read -r f; do printf "gw-%s %s size=%s sha256=%s\n" "$n" "$f" "$(wc -c < "$f" | tr -d " ")" "$(shasum -a 256 "$f" | cut -c1-16)"; done )
    done'
}

# tagsjson P LABEL: the root tags.json of P on both nodes (the core file each
# node loads its tag configuration from), pretty-printed.
tagsjson() {
  shcap "core tag-definition/$1/tags.json on both nodes, $2" bash -c '
    for n in a b; do printf "== gw-%s\n" "$n"; jq -S . "data-$n/config/resources/core/ignition/tag-definition/$0/tags.json" 2>&1; done' "$1"
}

find_status() { http "$1" GET "$RES/find/ignition/tag-provider/$2"; printf '%s' "$LAST_STATUS"; }

# mkprov P: create P on gw-a (core, STANDARD), wait until gw-a exports it, then
# time how long until gw-b finds and exports it.
mkprov() {
  local p=$1 t0 tb=none
  own "$p"
  delprov_quiet "$p"
  cap a "POST resources/ignition/tag-provider (create $p on gw-a)" POST "$RES/ignition/tag-provider" -H 'Content-Type: application/json' \
    --data "[{\"name\":\"$p\",\"type\":\"ignition/tag-provider\",\"collection\":\"core\",\"enabled\":true,\"config\":{\"profile\":{\"type\":\"STANDARD\"},\"settings\":{}}}]"
  t0=$(now_ms)
  until [ "$(find_status a "$p")" = 200 ]; do [ $(( $(now_ms) - t0 )) -lt 180000 ] || die "$p never answered find on gw-a"; sleep 1; done
  until http a GET "$EXP?provider=$p&type=json"; [ "$LAST_STATUS" = 200 ]; do [ $(( $(now_ms) - t0 )) -lt 180000 ] || die "$p never exported on gw-a"; sleep 1; done
  while :; do
    if [ "$(find_status b "$p")" = 200 ] && http b GET "$EXP?provider=$p&type=json" && [ "$LAST_STATUS" = 200 ]; then tb=$(secs $(( $(now_ms) - t0 ))); break; fi
    [ $(( $(now_ms) - t0 )) -lt 180000 ] || break
    sleep 1
  done
  reading "provider $p: gw-b found and exported it $tb s after gw-a answered the create."
  [ "$tb" != none ] || die "provider $p never reached gw-b within 180 s"
}

delprov_quiet() {
  local p=$1 sig
  own "$p"
  sig=$(q a GET "$RES/find/ignition/tag-provider/$p" | jq -r '.signature // empty' 2>/dev/null || true)
  [ -z "$sig" ] || http a DELETE "$RES/ignition/tag-provider/$p/$sig"
}

# delprov P: delete P on gw-a (recorded) and time until gw-b's find answers 404.
delprov() {
  local p=$1 sig t0 tb=none
  own "$p"
  sig=$(q a GET "$RES/find/ignition/tag-provider/$p" | jq -r '.signature // empty' 2>/dev/null || true)
  [ -n "$sig" ] || { note "Provider $p was already gone on gw-a."; return 0; }
  cap a "DELETE tag-provider $p on gw-a (cleanup)" DELETE "$RES/ignition/tag-provider/$p/$sig"
  t0=$(now_ms)
  while :; do
    [ "$(find_status b "$p")" != 404 ] || { tb=$(secs $(( $(now_ms) - t0 ))); break; }
    [ $(( $(now_ms) - t0 )) -lt "$(( CONVERGE_S * 1000 ))" ] || break
    sleep 1
  done
  reading "cleanup: gw-a find for $p answers $(find_status a "$p"); gw-b find answered 404 $tb s after the delete (none = not within $CONVERGE_S s)."
}

# scan_a: POST scan/config on gw-a and wait for it to complete.
scan_a() {
  local ts0 ts t0
  ts0=$(q a GET "$SCAN" | jq -r '.lastScanTimestamp // 0' 2>/dev/null || echo 0)
  cap a 'POST scan/config on gw-a' POST "$SCAN"
  t0=$(date +%s)
  while :; do
    q a GET "$SCAN" > "$TMPD/scan"
    ts=$(jq -r '.lastScanTimestamp // 0' "$TMPD/scan" 2>/dev/null || echo 0)
    if [ "$(jq -r '.scanActive' "$TMPD/scan" 2>/dev/null || echo ?)" = false ] && [ "$ts" -gt "$ts0" ] 2>/dev/null; then break; fi
    [ $(( $(date +%s) - t0 )) -lt 300 ] || { note "Scan on gw-a did not complete within 300 s."; return 0; }
    sleep 1
  done
}

chown_a() { docker exec -u root "$(docker ps -q --filter "label=com.docker.compose.project=$PROJECT" --filter "label=com.docker.compose.service=gw-a")" chown -R 2003:2003 "$CRES/$1"; }

atomic() { # NAME VALUE [EXTRA-JSON-MEMBERS]
  printf '{"name":"%s","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":%s%s}' "$1" "$2" "${3:+,$3}"
}

# ------------------------------------------------------------------ header

header26p() {
  assert_rig
  grep -q '^## TAGDEP-00 pair (8.3.6)' "$OUT" && die "REFUSING: the pair section already exists in $OUT"
  local ma mb image
  ma=$(q a GET /data/api/v1/gateway-info | jq -r '.deploymentMode // "?"')
  mb=$(q b GET /data/api/v1/gateway-info | jq -r '.deploymentMode // "?"')
  image=$(docker inspect -f '{{.Config.Image}}' "$(docker ps -q --filter "label=com.docker.compose.project=$PROJECT" --filter "label=com.docker.compose.service=gw-a")")
  {
    printf '\n## TAGDEP-00 pair (8.3.6)\n\n| Field | Value |\n| --- | --- |\n'
    printf '| Docker image | %s |\n' "$image"
    printf '| Compose project | %s (live-rig-pair/compose.yml): gw-a Master 127.0.0.1:18300, gw-b Backup 127.0.0.1:18301 |\n' "$PROJECT"
    # shellcheck disable=SC2016
    printf '| Bind paths (outside repo) | %s, %s |\n' "${BIND_A/#$HOME/\$HOME}" "${BIND_B/#$HOME/\$HOME}"
    printf '| deploymentMode | gw-a %s, gw-b %s |\n' "$ma" "$mb"
    printf '| Rig started (UTC) | %s |\n' "$(cat "$STATE/start-utc" 2>/dev/null || echo unknown)"
    printf '| Driver | capture26-pair.sh (crates/ignition-core/tests/fixtures/live-rig-pair) |\n'
    printf '\nRule: every write goes to gw-a; every gw-b request is a GET (the driver refuses anything else before curl runs). Convergence is the first poll (every 1 s, up to %s s from gw-a'"'"'s answer) at which both nodes'"'"' `tags/export` are equal as canonical JSON (`jq -S`, children of every `tags` array keyed by name), recorded both with every key and with AtomicTag `value`/`timestamp`/`quality` removed. Each row uses its own disposable provider (P26P1 ..) and deletes it at its end. Names and values are synthetic; no production gateway is contacted.\n' "$CONVERGE_S"
  } >> "$OUT"
  red_status 'at the start'
}

# ------------------------------------------------------------------ rows

p26_create() {
  local p=P26P1
  assert_rig
  sub p26-create
  note 'Abort import on gw-a of folder F holding memory tags T1 = 7 and T2 = 8; gw-b convergence; both nodes'"'"' collections.'
  mkprov $p
  imp 'p26-create Abort on gw-a: F with T1 = 7, T2 = 8' $p Abort "{\"tags\":[{\"name\":\"F\",\"tagType\":\"Folder\",\"tags\":[$(atomic T1 7),$(atomic T2 8)]}]}"
  converge $p 'p26-create' "$IMP_T0"
  reading "after p26-create: F/T1 value $(both $p F/T1 value); F/T2 value $(both $p F/T2 value)."
  CAP_BODY_MAX=1500 cap b 'p26-create gw-b export (GET)' GET "$EXP?provider=$p&type=json"
  sleep 5
  files $p
  red_status 'after p26-create'
  delprov $p
}

p26_change() {
  local p=P26P2 i1 i2 strip_inst
  assert_rig
  sub p26-change
  note 'Seed (Abort) on gw-a: UDT type Ty (P1 = 1; M1 Int4 memory 0, M2 String memory "type doc"); tags V and V2 = 5 documentation d1; instances I1 and I2 with P1 = 42, M1 = 5 and M2 documentation "override doc" (the C7 shape). A MergeOverwrite carrying value 9 on V and V2 stands in for a runtime change. The pre-image is exported. Then the change under the policy the single-node verdict chose for existing nodes (MergeOverwrite, A1 holds): V and V2 without value keys and documentation d2; I1 and I2 from the export, AtomicTag value keys stripped, parameters and member configuration kept, P1 = 43 and M2 documentation "changed doc".'
  mkprov $p
  local inst='"tagType":"UdtInstance","typeId":"Ty","parameters":{"P1":{"dataType":"Integer","value":42}},"tags":[{"name":"M1","tagType":"AtomicTag","value":5},{"name":"M2","tagType":"AtomicTag","documentation":"override doc"}]'
  imp 'p26-change seed type (Abort)' $p Abort '{"tags":[{"name":"_types_","tagType":"Folder","tags":[{"name":"Ty","tagType":"UdtType","parameters":{"P1":{"dataType":"Integer","value":1}},"tags":[{"name":"M1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":0},{"name":"M2","tagType":"AtomicTag","dataType":"String","valueSource":"memory","value":"x","documentation":"type doc"}]}]}]}'
  imp 'p26-change seed tags and instances (Abort)' $p Abort "{\"tags\":[$(atomic V 5 '"documentation":"d1"'),$(atomic V2 5 '"documentation":"d1"'),{\"name\":\"I1\",$inst},{\"name\":\"I2\",$inst}]}"
  converge $p 'p26-change seed' "$IMP_T0"
  imp 'p26-change stand-in runtime change (MergeOverwrite, value 9 on V and V2)' $p MergeOverwrite "{\"tags\":[$(atomic V 9),$(atomic V2 9)]}"
  converge $p 'p26-change runtime change' "$IMP_T0"
  reading "before the change: V value $(both $p V value); V2 value $(both $p V2 value); I1/M1 value $(both $p I1/M1 value)."
  tagsjson $p 'after the stand-in runtime change'
  cp "$TMPD/ea-$p.json" "$PREIMAGE"
  shcap 'p26-change pre-image saved from gw-a export (sha256, size)' bash -c 'shasum -a 256 p26-preimage-P26P2.json | cut -c1-16; wc -c < p26-preimage-P26P2.json'
  strip_inst='.tags[] | select(.name == $n) | ('"$STRIP"') | .parameters.P1.value = 43 | .tags |= map(if .name == "M2" then .documentation = "changed doc" else . end)'
  i1=$(jq -c --arg n I1 "$strip_inst" "$PREIMAGE")
  i2=$(jq -c --arg n I2 "$strip_inst" "$PREIMAGE")
  local nv='"tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","documentation":"d2"'
  imp 'p26-change MergeOverwrite without value keys: V, V2 d2; I1, I2 stripped with P1 = 43, M2 "changed doc"' $p MergeOverwrite "{\"tags\":[{\"name\":\"V\",$nv},{\"name\":\"V2\",$nv},$i1,$i2]}"
  converge $p 'p26-change' "$IMP_T0"
  local n
  for n in V V2; do
    reading "after p26-change: $n value $(both $p $n value); $n documentation $(both $p $n documentation)."
  done
  for n in I1 I2; do
    reading "after p26-change: $n parameters $(both $p $n parameters); $n/M1 value $(both $p $n/M1 value); $n/M2 documentation $(both $p $n/M2 documentation)."
  done
  CAP_BODY_MAX=3000 cap b 'p26-change gw-b export (GET)' GET "$EXP?provider=$p&type=json"
  sleep 5
  files $p
  tagsjson $p 'after p26-change'
  red_status 'after p26-change'
  note "Provider $p is kept for p26-rollback-shape, which deletes it."
}

p26_delete() {
  sub p26-delete
  note 'skipped: no import delete (see C4/C5). The single-node capture found that neither a parent Overwrite nor a parent MergeOverwrite that omits a child deletes it (A2 fails, F4), so there is no import-based delete primitive to replay on the pair. Per D-13 no WebDev route is used here; the delete design goes to the user at the 26-08 checkpoint.'
}

p26_provider() {
  local p=P26P4 rec body t0 tb=none da db sa sb
  assert_rig
  sub p26-provider
  note 'Tag-provider settings modify on gw-a: the C11 PUT (resources/ignition/tag-provider with the signature from find, description "p26 modified"); when gw-b'"'"'s find shows the same description and signature; where the resource file lands on each node.'
  mkprov $p
  sleep 5
  cap a "GET find tag-provider $p on gw-a" GET "$RES/find/ignition/tag-provider/$p"
  rec=$(cat "$TMPD/bod")
  body=$(jq -c '[{name: .name, collection: .collection, enabled: .enabled, description: "p26 modified", config: .config, signature: .signature}]' <<<"$rec")
  printf '%s' "$body" > "$TMPD/put.json"
  body_block 'p26-provider PUT tag-provider modify on gw-a' "$TMPD/put.json"
  cap a 'p26-provider PUT tag-provider modify on gw-a' PUT "$RES/ignition/tag-provider" -H 'Content-Type: application/json' --data-binary "@$TMPD/put.json"
  t0=$(now_ms)
  : > "$TMPD/plog"
  while :; do
    q a GET "$RES/find/ignition/tag-provider/$p" > "$TMPD/fa"
    q b GET "$RES/find/ignition/tag-provider/$p" > "$TMPD/fb"
    da=$(jq -r '.description // "-"' "$TMPD/fa" 2>/dev/null); db=$(jq -r '.description // "-"' "$TMPD/fb" 2>/dev/null)
    sa=$(jq -r '.signature // "-"' "$TMPD/fa" 2>/dev/null | cut -c1-12); sb=$(jq -r '.signature // "-"' "$TMPD/fb" 2>/dev/null | cut -c1-12)
    printf '+%ss gw-a description=%s signature=%s | gw-b description=%s signature=%s\n' "$(secs $(( $(now_ms) - t0 )))" "$da" "$sa" "$db" "$sb" >> "$TMPD/plog"
    if [ "$db" = 'p26 modified' ] && [ "$sa" = "$sb" ]; then tb=$(secs $(( $(now_ms) - t0 ))); break; fi
    [ $(( $(now_ms) - t0 )) -lt $(( CONVERGE_S * 1000 )) ] || break
    sleep 1
  done
  block "p26-provider: gw-a and gw-b find of $p per poll" "$TMPD/plog"
  reading "p26-provider: gw-b's find showed description \"p26 modified\" and gw-a's signature after ${tb} s (none = did not converge within $CONVERGE_S s)."
  cap b "GET find tag-provider $p on gw-b after the modify" GET "$RES/find/ignition/tag-provider/$p"
  sleep 3
  shcap "tag-provider resource files of $p in both collections on both nodes" bash -c '
    for n in a b; do
      ( cd "data-$n/config/resources" && find core production -path "*/tag-provider/'"$p"'/*" -type f 2>/dev/null | LC_ALL=C sort |
        while IFS= read -r f; do printf "== gw-%s %s sha256=%s\n" "$n" "$f" "$(shasum -a 256 "$f" | cut -c1-16)"; cat "$f"; printf "\n[eof]\n"; done )
    done'
  red_status 'after p26-provider'
  delprov $p
}

p26_groups() {
  local p=P26P5 src dsta dstb t0 tb=none ha hb
  assert_rig
  sub p26-groups
  note 'A tag-group unit G26 written into gw-a'"'"'s mode collection (production/ignition/tag-group/P26P5/G26, copied from the provider'"'"'s Default group with its uuid attributes removed) plus a config scan on gw-a (the C14 form): whether gw-b receives it, whether both nodes stay in sync (no wedge), and whether a tag import that names the group still reaches gw-b afterwards. Then the unit is removed from gw-a'"'"'s mode collection, scanned, and gw-b is watched until it drops it.'
  mkprov $p
  sleep 5
  shcap "tag-group units of $p on both nodes before" bash -c 'for n in a b; do ( cd "data-$n/config/resources" && find core production -path "*/tag-group/'"$p"'/*" -type f 2>/dev/null | LC_ALL=C sort | sed "s/^/gw-$n /" ); done'
  src=$(find "$BIND_A/core/ignition/tag-group/$p" "$BIND_A/production/ignition/tag-group/$p" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | head -1 || true)
  if [ -z "$src" ]; then
    note "Not captured: gw-a wrote no tag-group unit for $p to copy from."
    delprov $p
    return 0
  fi
  red_status 'p26-groups before the mode-collection unit'
  dsta=$BIND_A/production/ignition/tag-group/$p/G26
  dstb=$BIND_B/production/ignition/tag-group/$p/G26
  mkdir -p "$(dirname "$dsta")"
  /bin/rm -rf "$dsta"
  cp -R "$src" "$dsta"
  if [ -f "$dsta/resource.json" ]; then
    jq -c '.attributes |= (del(.uuid, .lastModification, .lastModificationSignature) // {})' "$dsta/resource.json" > "$TMPD/rj"
    cp "$TMPD/rj" "$dsta/resource.json"
  fi
  chown_a production/ignition/tag-group
  shcap "the new mode-collection unit on gw-a" bash -c 'find data-a/config/resources/production/ignition/tag-group/'"$p"'/G26 -type f | LC_ALL=C sort | while IFS= read -r f; do printf "== %s\n" "$f"; cat "$f"; printf "\n[eof]\n"; done'
  scan_a
  t0=$(now_ms)
  : > "$TMPD/glog"
  while :; do
    ha=$(cat "$dsta/config.json" 2>/dev/null | shasum -a 256 | cut -c1-12)
    hb=$( [ -f "$dstb/config.json" ] && shasum -a 256 < "$dstb/config.json" | cut -c1-12 || echo absent)
    printf '+%ss gw-a G26/config.json=%s gw-b G26/config.json=%s | %s\n' "$(secs $(( $(now_ms) - t0 )))" "$ha" "$hb" \
      "$(for n in a b; do printf 'gw-%s %s ' "$n" "$(q "$n" GET "$RED" | jq -r '"peer=\(.peerConnected) syncPending=\(.syncPending) project=\(.projectState)"' 2>/dev/null || echo '?')"; done)" >> "$TMPD/glog"
    if [ "$ha" = "$hb" ]; then tb=$(secs $(( $(now_ms) - t0 ))); break; fi
    [ $(( $(now_ms) - t0 )) -lt $(( CONVERGE_S * 1000 )) ] || break
    sleep 1
  done
  block "p26-groups: G26 on both bind dirs and both nodes' redundancy per poll after the gw-a scan" "$TMPD/glog"
  reading "p26-groups: gw-b's production/ignition/tag-group/$p/G26/config.json matched gw-a's ${tb} s after the scan completed (none = not within $CONVERGE_S s)."
  sleep 20
  red_status 'p26-groups 20 s after the unit reached gw-b'
  local sync=no
  in_sync && sync=yes
  reading "p26-groups: both nodes peerConnected true, syncPending false and projectState Good 20 s later: $sync."
  imp 'p26-groups Abort on gw-a: tag K in tag group G26 (does tag sync still flow?)' $p Abort "{\"tags\":[$(atomic K 1 '"tagGroup":"G26"')]}"
  converge $p 'p26-groups tag after the group unit' "$IMP_T0"
  reading "p26-groups: K tagGroup $(both $p K tagGroup)."
  files $p
  /bin/rm -rf "$BIND_A/production/ignition/tag-group/$p"
  scan_a
  t0=$(now_ms); tb=none
  while :; do
    [ -e "$dstb" ] || { tb=$(secs $(( $(now_ms) - t0 ))); break; }
    [ $(( $(now_ms) - t0 )) -lt $(( CONVERGE_S * 1000 )) ] || break
    sleep 1
  done
  reading "p26-groups cleanup: gw-b dropped its G26 copy ${tb} s after the gw-a scan (none = not within $CONVERGE_S s)."
  sleep 10
  red_status 'after p26-groups cleanup'
  delprov $p
  sleep 10
  shcap 'mode-collection tag-group leftovers on both nodes after p26-groups' bash -c 'for n in a b; do printf "gw-%s: " "$n"; find "data-$n/config/resources/production/ignition/tag-group" -path "*P26P*" 2>/dev/null | wc -l | tr -d " "; done'
}

p26_rollback_shape() {
  local p=P26P2 body
  assert_rig
  sub p26-rollback-shape
  note 'Rollback of p26-change on gw-a: the pre-image (gw-a export taken before p26-change) re-imported under Overwrite at the root. V and I1 are sent with value, timestamp and quality removed from AtomicTag objects (the form 26-RESEARCH proposes). V2 and I2 are sent with each AtomicTag value replaced by the live value read from gw-a just before the rollback (value carried). gw-b convergence and what each node holds afterwards.'
  [ -f "$PREIMAGE" ] || die "no pre-image at $PREIMAGE; run p26-change first"
  [ "$(find_status a $p)" = 200 ] || die "provider $p is gone on gw-a; run p26-change first"
  http a GET "$EXP?provider=$p&type=json"; cp "$TMPD/bod" "$TMPD/live.json"
  body=$(python3 -I - "$PREIMAGE" "$TMPD/live.json" <<'PY'
import json, sys
pre = json.load(open(sys.argv[1], encoding="utf-8"))
live = json.load(open(sys.argv[2], encoding="utf-8"))
def by(node): return {c.get("name"): c for c in node.get("tags", [])}
def strip(n):
    n = dict(n)
    if n.get("tagType") == "AtomicTag":
        for k in ("value", "timestamp", "quality"):
            n.pop(k, None)
    if "tags" in n:
        n["tags"] = [strip(c) for c in n["tags"]]
    return n
def carry(n, l):
    n = dict(n)
    if n.get("tagType") == "AtomicTag":
        for k in ("timestamp", "quality"):
            n.pop(k, None)
        n.pop("value", None)
        if l is not None and "value" in l:
            n["value"] = l["value"]
    if "tags" in n:
        lk = by(l) if l else {}
        n["tags"] = [carry(c, lk.get(c.get("name"))) for c in n["tags"]]
    return n
p, lv = by(pre), by(live)
out = [strip(p["V"]), strip(p["I1"]), carry(p["V2"], lv.get("V2")), carry(p["I2"], lv.get("I2"))]
print(json.dumps({"tags": out}, separators=(",", ":")))
PY
)
  imp 'p26-rollback-shape Overwrite of the pre-image: V, I1 stripped; V2, I2 live values carried' $p Overwrite "$body"
  converge $p 'p26-rollback-shape' "$IMP_T0"
  local n
  for n in V V2; do
    reading "after the rollback: $n value $(both $p $n value); $n documentation $(both $p $n documentation)."
  done
  for n in I1 I2; do
    reading "after the rollback: $n parameters $(both $p $n parameters); $n/M1 value $(both $p $n/M1 value); $n/M2 documentation $(both $p $n/M2 documentation)."
  done
  shcap 'p26-rollback-shape: each node against the pre-image, canonical with AtomicTag value keys removed (sha256 first 12)' bash -c '
    s() { jq -S "$1" "$2" | shasum -a 256 | cut -c1-12; }
    printf "pre-image %s\ngw-a      %s\ngw-b      %s\n" "$(s "$0" p26-preimage-P26P2.json)" "$(s "$0" "$1")" "$(s "$0" "$2")"' "($STRIP) | ($KEYED)" "$TMPD/ea-$p.json" "$TMPD/eb-$p.json"
  CAP_BODY_MAX=3000 cap b 'p26-rollback-shape gw-b export (GET)' GET "$EXP?provider=$p&type=json"
  sleep 5
  tagsjson $p 'after p26-rollback-shape'
  red_status 'after p26-rollback-shape'
  delprov $p
  /bin/rm -f "$PREIMAGE"
}

case "${1:-}" in
  header26p) header26p ;;
  p26-create) p26_create ;;
  p26-change) p26_change ;;
  p26-delete) p26_delete ;;
  p26-provider) p26_provider ;;
  p26-groups) p26_groups ;;
  p26-rollback-shape) p26_rollback_shape ;;
  down26p) echo "docker compose -p $PROJECT down -v" ;;
  *) echo "usage: capture26-pair.sh header26p|p26-create|p26-change|p26-delete|p26-provider|p26-groups|p26-rollback-shape|down26p" >&2; exit 64 ;;
esac
