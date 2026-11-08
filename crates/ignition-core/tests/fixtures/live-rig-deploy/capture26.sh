#!/usr/bin/env bash
# capture26.sh SUBCOMMAND [ARG ...]
#
# Phase 26 (26-01) rig capture driver, TAGDEP-00 on the single-node 8.3.6 rig:
# what the gateway's REST tag import does with values, omitted children,
# omitted properties, UDT instances, a mode-collection copy, a large body,
# provider settings, the native git-module form, failing bodies and tag
# groups, recorded before any project-owned tag write relies on it.
#
#   c26-create    C1: Abort import of a new folder and memory tag; a sibling
#                 at path=<folder>; a colliding name (the failure answer)
#   c26-value     C2/C3: a live value under a value-less MergeOverwrite and
#                 Overwrite (row a, A1); C3 Database persistence
#   c26-children  C4/C5: a parent Overwrite / MergeOverwrite that omits a
#                 child, in a folder and at the provider root (row b, A2)
#   c26-props     C6: a re-import that omits a property (row c, A3)
#   c26-instance  C7: an existing UDT instance re-imported without overrides
#                 and stripped of AtomicTag value keys (A12)
#   c26-mode      C8: an import edit of a tag whose unit sits in the mode
#                 collection (row d, A4, Pitfall 10)
#   c26-timing    C9: 3,000 memory tags in 30 folders: import and export wall
#                 time, file flush into core (A8)
#   c26-provider  C11: tag-provider settings modify (PUT with signature)
#   c26-canon     C12: a git-module-shaped tree, imported then exported,
#                 differences per node (A10)
#   c26-quality   C13: imports that must fail: a missing type, a missing tag
#                 group, a malformed body (A5)
#   c26-groups    C14: tag groups in the export; a mode-collection tag-group
#                 unit plus a scan, then a tag that references it
#   c26-verdict   the verdict table (A1..A12) from the rows' readings
#
# Subcommands: header26 | sect ID TEXT | note TEXT | shcap LABEL CMD [ARG ...]
#              | the c26-* rows above | down26 (prints the teardown command)
#
# The rig is the Phase 22 compose project `ign-deploy-rig` (port 18290, the
# resources directory bind-mounted from $HOME/.cache/ign-deploy-rig/data, the
# gateway in deployment mode `production`), brought up with
# `capture23.sh up23` then `capture23.sh setup23`. Every row makes its own
# disposable provider (P26C1 .. P26C14) and deletes it at its end; delprov
# refuses any other name. Names and values are synthetic.
#
# Imports and exports go through curl, not through ign, so the record is
# independent of the client under test. Every byte inside a capture section is
# appended by `cap` (HTTP) or `shcap` (any other command) and is preceded by a
# line carrying the exact command and a UTC time taken by this script just
# before the run. A request body is shown in its own block before the call.
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
OUT=${CAPTURE_OUT:-$REPO/.planning/phases/26-project-owned-tag-deploy/26-LIVE-CAPTURES.md}
STATE=${IGN_DEPLOY_RIG_STATE:-$HOME/.cache/ign-deploy-rig}
BIND=$STATE/data/config/resources
ANSWERS=$REPO/crates/ignition-core/tests/fixtures/tag-import
VERDICTS=$STATE/c26-verdicts
TMPD=$(mktemp -d)
trap '/bin/rm -rf "$TMPD"' EXIT

# shellcheck disable=SC1091
[ -f "$STATE/tokens.env" ] && . "$STATE/tokens.env"

RES=/data/api/v1/resources
IMP=/data/api/v1/tags/import
EXP=/data/api/v1/tags/export
SCAN=/data/api/v1/scan/config
CRES=/usr/local/bin/ignition/data/config/resources
RIG_NAME=ign-deploy-rig

utc() { date -u +%FT%TZ; }
now_ms() { perl -MTime::HiRes=time -e 'printf "%d", time*1000'; }
h() { printf '\n## %s\n' "$1" >> "$OUT"; }
sub() { printf '\n### %s\n' "$1" >> "$OUT"; }
# note TEXT: a line of script-written commentary (never gateway bytes).
note() { printf '\n%s\n' "$1" >> "$OUT"; }
die() { echo "$*" >&2; exit 64; }

# sect ID [TEXT]: the `### ID` heading under the single-node section (exactly
# the id, so the row is found by name) and, when TEXT is given, one line of
# commentary under it.
sect() {
  case "${1:-}" in
    C1 | C2 | C3 | C4 | C5 | C6 | C7 | C8 | C9 | C11 | C12 | C13 | C14) ;;
    *) echo "sect: unknown capture id '${1:-}'" >&2; exit 64 ;;
  esac
  sub "$1"
  [ -z "${2:-}" ] || note "$2"
}

# cap LABEL METHOD PATH [curl args]: one verbatim HTTP exchange. CAP_MAX is the
# curl timeout in seconds (default 90); CAP_BODY_MAX, when set, shows only that
# many body bytes plus the full size and sha256 of the body. The answer body
# stays in $TMPD/bod, its status in $LAST_STATUS, the curl time in $LAST_TIME.
LAST_STATUS=""
LAST_TIME=""
cap() {
  local label=$1 method=$2 path=$3; shift 3
  local tok started args meta
  local hdr=$TMPD/hdr bod=$TMPD/bod
  tok=${IGN_TOKEN_CAP_A:?IGN_TOKEN_CAP_A is not set; run capture23.sh setup23 and source tokens.env}
  args="${*//$TMPD/\$TMPD}"
  started=$(utc)
  meta=$(curl -sS -g -D "$hdr" -o "$bod" -m "${CAP_MAX:-90}" -w '[curl http=%{http_code} total=%{time_total}s]' \
    -X "$method" -H "X-Ignition-API-Token: $tok" "$@" "$BASE$path" || true)
  {
    printf '\n#### %s\n\n`curl -sS -g -i -X %s [token $IGN_TOKEN_CAP_A] %s%s` @ %s\n\n~~~\n' \
      "$label" "$method" "${args:+$args }" "'\$BASE$path'" "$started"
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
  LAST_TIME=$(sed -n 's/.* total=\([0-9.]*\)s\]$/\1/p' <<<"$meta" | tail -1)
}

# shcap LABEL CMD [ARG ...]: any other command, its stdout and stderr verbatim.
shcap() {
  local label=$1; shift
  local started rc=0 out=$TMPD/sh
  started=$(utc)
  ( cd "$BIND" && "$@" ) > "$out" 2>&1 || rc=$?
  {
    printf '\n#### %s\n\n`' "$label"
    printf '%q ' "$@" | sed "s|$TMPD|\$TMPD|g"
    printf '` (in $BIND) @ %s\n\n~~~\n' "$started"
    cat "$out"
    printf '[exit %s]\n~~~\n' "$rc"
  } >> "$OUT"
}

# assert_rig: before any write. Refuses (exit 2) unless gateway-info names the
# rig.
assert_rig() {
  local name
  name=$(curl -sS -m 30 -H "X-Ignition-API-Token: ${IGN_TOKEN_CAP_A:?IGN_TOKEN_CAP_A is not set}" "$BASE/data/api/v1/gateway-info" | jq -r '.name // empty' 2>/dev/null || true)
  [ "$name" = "$RIG_NAME" ] || { echo "REFUSING: gateway-info names '${name:-<nothing>}', not the rig '$RIG_NAME'" >&2; exit 2; }
}

# body_block TEXT FILE: show a request body (pretty when it is JSON) and keep
# it in FILE for the call.
body_block() {
  local label=$1 file=$2
  {
    printf '\n#### %s (request body)\n\n~~~json\n' "$label"
    if [ "$(wc -c < "$file" | tr -d ' ')" -gt 4000 ]; then
      head -c 1500 "$file"
      printf '\n[body truncated to 1500 bytes; full body %s bytes, sha256 %s]\n' \
        "$(wc -c < "$file" | tr -d ' ')" "$(shasum -a 256 "$file" | cut -d' ' -f1)"
    else
      jq . "$file" 2>/dev/null || cat "$file"
    fi
    printf '~~~\n'
  } >> "$OUT"
}

# imp LABEL PROVIDER POLICY PATH BODY: one tags/import call, the body shown
# first. PATH empty is the provider root (no `path` key on the query).
imp() {
  local label=$1 p=$2 policy=$3 at=$4 body=$5 q f=$TMPD/req.json
  printf '%s' "$body" > "$f"
  q="provider=$p&type=json&collisionPolicy=$policy"
  [ -z "$at" ] || q="$q&path=$at"
  body_block "$label" "$f"
  cap "$label" POST "$IMP?$q" -H 'Content-Type: application/octet-stream' --data-binary "@$f"
  printf '%s %s %s\n' "$policy" "$LAST_STATUS" "$(tr -d '\n' < "$TMPD/bod" | head -c 300)" >> "$STATE/c26-answers"
}

# exp LABEL PROVIDER: the provider's REST export, kept in $TMPD/export-<P>.json.
exp() {
  local label=$1 p=$2
  cap "$label" GET "$EXP?provider=$p&type=json"
  cp "$TMPD/bod" "$TMPD/export-$p.json"
}

# save_answer NAME: keep the last answer body as a wiremock fixture.
save_answer() {
  mkdir -p "$ANSWERS"
  if [ -s "$TMPD/bod" ]; then
    jq -S . "$TMPD/bod" > "$ANSWERS/answer-$1.json" 2>/dev/null || cp "$TMPD/bod" "$ANSWERS/answer-$1.json"
  else
    printf '' > "$ANSWERS/answer-$1.json"
  fi
  printf '%s\n' "$LAST_STATUS" > "$ANSWERS/answer-$1.status"
}

# pick P PATH KEY: KEY of the node at PATH (slash-separated) in the last export
# of P, as compact JSON, or <absent node> / <absent key>.
pick() {
  python3 -I - "$TMPD/export-$1.json" "$2" "$3" <<'PY'
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

# reading TEXT: a script-written reading of the record above.
reading() { note "Script reading: $1"; }

# verdict A STATUS ROW: one verdict line for c26-verdict (latest wins).
verdict() {
  touch "$VERDICTS"
  grep -v "^$1|" "$VERDICTS" > "$TMPD/v" || true
  printf '%s|%s|%s\n' "$1" "$2" "$3" >> "$TMPD/v"
  cp "$TMPD/v" "$VERDICTS"
}

# mkprov P: create disposable provider P (STANDARD) and wait until find answers.
mkprov() {
  local p=$1 t0
  case "$p" in P26C[0-9]*) ;; *) die "mkprov: '$p' is not a P26C* provider" ;; esac
  delprov_quiet "$p"
  cap "POST resources/ignition/tag-provider (create $p)" POST "$RES/ignition/tag-provider" \
    -H 'Content-Type: application/json' \
    --data "[{\"name\":\"$p\",\"type\":\"ignition/tag-provider\",\"collection\":\"core\",\"enabled\":true,\"config\":{\"profile\":{\"type\":\"STANDARD\"},\"settings\":{}}}]"
  t0=$(date +%s)
  until [ "$(curl -sS -m 30 -o /dev/null -w '%{http_code}' -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$RES/find/ignition/tag-provider/$p" || true)" = 200 ]; do
    [ $(( $(date +%s) - t0 )) -lt 180 ] || die "provider $p never answered find"
    sleep 2
  done
  # The provider answers find before it accepts an import; wait for its export.
  t0=$(date +%s)
  until [ "$(curl -sS -m 60 -o /dev/null -w '%{http_code}' -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$EXP?provider=$p&type=json" || true)" = 200 ]; do
    [ $(( $(date +%s) - t0 )) -lt 180 ] || die "provider $p never exported"
    sleep 2
  done
}

# delprov_quiet P: delete P if present, unrecorded (the entry sweep).
delprov_quiet() {
  local p=$1 sig
  case "$p" in P26C[0-9]*) ;; *) die "delprov: '$p' is not a P26C* provider" ;; esac
  sig=$(curl -sS -m 30 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$RES/find/ignition/tag-provider/$p" 2>/dev/null | jq -r '.signature // empty' 2>/dev/null || true)
  [ -z "$sig" ] || curl -sS -m 60 -o /dev/null -X DELETE -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$RES/ignition/tag-provider/$p/$sig" || true
}

# delprov P: delete P (recorded) and confirm find answers 404.
delprov() {
  local p=$1 sig
  case "$p" in P26C[0-9]*) ;; *) die "delprov: '$p' is not a P26C* provider" ;; esac
  sig=$(curl -sS -m 30 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$RES/find/ignition/tag-provider/$p" | jq -r '.signature // empty')
  [ -n "$sig" ] || { note "Provider $p was already gone."; return 0; }
  cap "DELETE tag-provider $p (cleanup)" DELETE "$RES/ignition/tag-provider/$p/$sig"
  sleep 3
  cap "GET find tag-provider $p (expect 404 after cleanup)" GET "$RES/find/ignition/tag-provider/$p"
}

# scan: POST scan/config and wait until it completed (lastScanTimestamp moved).
scan() {
  local body ts0 ts t0
  body=$(curl -sS -m 30 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$SCAN" || true)
  ts0=$(jq -r '.lastScanTimestamp // 0' <<<"$body" 2>/dev/null || echo 0)
  cap 'POST scan/config' POST "$SCAN"
  t0=$(date +%s)
  while :; do
    body=$(curl -sS -m 30 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE$SCAN" || true)
    ts=$(jq -r '.lastScanTimestamp // 0' <<<"$body" 2>/dev/null || echo 0)
    if [ "$(jq -r '.scanActive' <<<"$body" 2>/dev/null || echo ?)" = false ] && [ "$ts" -gt "$ts0" ] 2>/dev/null; then break; fi
    [ $(( $(date +%s) - t0 )) -lt 300 ] || { note "Scan did not complete within 300 s."; return 0; }
    sleep 1
  done
  cap 'GET scan/config (after completion)' GET "$SCAN"
}

# files P: every file of provider P under the bind dir, both collections, with
# size and sha256.
files() {
  shcap "files of $1 under core and production (size, sha256)" bash -c \
    'find core production -path "*/'"$1"'/*" -type f 2>/dev/null | LC_ALL=C sort | while IFS= read -r f; do printf "%s size=%s sha256=%s\n" "$f" "$(wc -c < "$f" | tr -d " ")" "$(shasum -a 256 "$f" | cut -d" " -f1)"; done'
}

chown_bind() { ( cd "$HERE" && docker compose -p ign-deploy-rig -f compose.yml -f compose.bind.yml exec -T -u root gw chown -R 2003:2003 "$CRES" ); }

atomic() { # atomic NAME VALUE [EXTRA-JSON-MEMBERS]
  printf '{"name":"%s","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":%s%s}' "$1" "$2" "${3:+,$3}"
}

# ------------------------------------------------------------------ header

header26() {
  curl -s -m 10 "$BASE/StatusPing" | grep -q RUNNING || die "gateway is not RUNNING"
  assert_rig
  local mode image cid
  mode=$(curl -sS -m 30 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE/data/api/v1/gateway-info" | jq -r '.deploymentMode')
  cid=$(docker ps -q --filter "label=com.docker.compose.project=ign-deploy-rig" --filter "label=com.docker.compose.service=gw")
  image=$(docker inspect -f '{{.Config.Image}}' "$cid")
  mkdir -p "$(dirname "$OUT")"
  [ -e "$OUT" ] || printf '# Phase 26 — Live Captures\n' > "$OUT"
  grep -q '^## TAGDEP-00 single node (8.3.6)' "$OUT" && die "REFUSING: the single-node section already exists in $OUT"
  : > "$VERDICTS"
  : > "$STATE/c26-answers"
  {
    printf '\n## TAGDEP-00 single node (8.3.6)\n\n| Field | Value |\n| --- | --- |\n'
    printf '| Docker image | %s |\n' "$image"
    printf '| Compose variant | bind (compose.yml + compose.bind.yml), project ign-deploy-rig, 127.0.0.1:18290 |\n'
    # shellcheck disable=SC2016
    printf '| Bind path (outside repo) | %s |\n' "${BIND/#$HOME/\$HOME}"
    printf '| deploymentMode | %s |\n' "$mode"
    printf '| Rig started (UTC) | %s |\n' "$(cat "$STATE/start-utc-22" 2>/dev/null || echo unknown)"
    printf '| Driver | capture26.sh (crates/ignition-core/tests/fixtures/live-rig-deploy) |\n'
    printf '\nRule: every captured block is preceded by the exact command and a UTC time taken by the driver just before it. A request line names the token variable it used, never the token. Every row uses its own disposable provider (P26C1 .. P26C14) and deletes it at its end. Names and values are synthetic; no production gateway is contacted.\n'
    printf '\nAccepted deviation: imports and exports go through curl rather than `ign api call`; the record stays independent of the client under test either way, and curl shows the raw status and headers.\n'
  } >> "$OUT"
}

# ------------------------------------------------------------------ C1

c26_create() {
  local p=P26C1
  assert_rig
  sect C1 'Abort import of a new folder plus memory tag at the provider root, an Abort import of a sibling at path=F, and Abort imports that collide with an existing name.'
  mkprov $p
  imp 'C1a Abort at the root: folder F holding memory tag T1 = 7' $p Abort '' "{\"tags\":[{\"name\":\"F\",\"tagType\":\"Folder\",\"tags\":[$(atomic T1 7)]}]}"
  save_answer c1-create
  local a1=$LAST_STATUS
  exp 'C1a export straight after the answer' $p
  reading "C1a answered HTTP $a1; F/T1 value in the export straight after: $(pick $p F/T1 value)."
  imp 'C1b Abort at path=F: sibling T2 = 8' $p Abort F "{\"tags\":[$(atomic T2 8)]}"
  save_answer c1-sibling
  exp 'C1b export' $p
  reading "C1b answered HTTP $LAST_STATUS; F/T2 value: $(pick $p F/T2 value); F/T1 value: $(pick $p F/T1 value)."
  imp 'C1c Abort at path=F colliding with T1 (value 99)' $p Abort F "{\"tags\":[$(atomic T1 99)]}"
  save_answer c1-collide-tag
  local c=$LAST_STATUS
  imp 'C1d Abort at the root colliding with folder F' $p Abort '' "{\"tags\":[{\"name\":\"F\",\"tagType\":\"Folder\",\"tags\":[$(atomic T3 3)]}]}"
  save_answer c1-collide-folder
  sleep 20
  exp 'C1 export 20 s later' $p
  reading "C1c (tag collision) answered HTTP $c; C1d (folder collision) answered HTTP $LAST_STATUS. After both: F/T1 value $(pick $p F/T1 value), F/T3 $(pick $p F/T3 value)."
  files $p
  delprov $p
}

# ------------------------------------------------------------------ C2/C3

c26_value() {
  local p=P26C2
  assert_rig
  sect C2 'Row (a), A1: memory tag V created with value 5 and documentation d1; its value changed to 9 by a MergeOverwrite import carrying value 9; then V imported WITHOUT value/timestamp/quality and with a changed documentation, under MergeOverwrite (d2), then under Overwrite (d3). Tag W gets the same sequence in the other order (Overwrite first).'
  mkprov $p
  imp 'C2a Abort: V = 5 and W = 5, documentation d1' $p Abort '' "{\"tags\":[$(atomic V 5 '"documentation":"d1"'),$(atomic W 5 '"documentation":"d1"')]}"
  imp 'C2b MergeOverwrite: V and W carrying value 9 (the stand-in runtime change)' $p MergeOverwrite '' "{\"tags\":[$(atomic V 9),$(atomic W 9)]}"
  exp 'C2b export' $p
  reading "after C2b: V value $(pick $p V value), W value $(pick $p W value), V documentation $(pick $p V documentation)."
  note 'A runtime write through `ign tags write` needs the ign-cli WebDev routes, which this fresh rig does not have; the MergeOverwrite import carrying value 9 is the runtime change here. Not captured: a value written by a running tag rather than by an import.'
  local noval='"tagType":"AtomicTag","dataType":"Int4","valueSource":"memory"'
  imp 'C2c MergeOverwrite without value keys: V documentation d2' $p MergeOverwrite '' "{\"tags\":[{\"name\":\"V\",$noval,\"documentation\":\"d2\"}]}"
  save_answer c2-merge-novalue
  imp 'C2c Overwrite without value keys: W documentation d2' $p Overwrite '' "{\"tags\":[{\"name\":\"W\",$noval,\"documentation\":\"d2\"}]}"
  save_answer c2-overwrite-novalue
  exp 'C2c export' $p
  local vm wm
  vm=$(pick $p V value); wm=$(pick $p W value)
  reading "after C2c: V (MergeOverwrite) value $vm documentation $(pick $p V documentation); W (Overwrite) value $wm documentation $(pick $p W documentation)."
  imp 'C2d Overwrite without value keys: V documentation d3' $p Overwrite '' "{\"tags\":[{\"name\":\"V\",$noval,\"documentation\":\"d3\"}]}"
  imp 'C2d MergeOverwrite without value keys: W documentation d3' $p MergeOverwrite '' "{\"tags\":[{\"name\":\"W\",$noval,\"documentation\":\"d3\"}]}"
  exp 'C2d export straight after' $p
  reading "after C2d: V (now Overwrite) value $(pick $p V value) documentation $(pick $p V documentation); W (now MergeOverwrite) value $(pick $p W value) documentation $(pick $p W documentation)."
  sleep 20
  exp 'C2d export 20 s later' $p
  reading "20 s later: V value $(pick $p V value), W value $(pick $p W value)."
  if [ "$vm" = 9 ]; then
    verdict A1 holds C2
    reading "A1: a value-less MergeOverwrite kept the live value 9 (V). A value-less Overwrite left W at $wm."
  else
    verdict A1 fails C2
    reading "A1: a value-less MergeOverwrite left V at $vm, not the live value 9."
  fi
  files $p
  delprov $p
  sect C3 'Row (a) on a provider whose settings set Database value persistence.'
  note 'Not captured: the rig has no reachable database connection (db-connection-config.json names a host that is not part of this compose project), so a provider with Database value persistence cannot be created here. C2 covers the memory-only provider.'
}

# ------------------------------------------------------------------ C4/C5

children_row() {
  local id=$1 p=$2 policy=$3
  sect "$id" "Row (b), A2, $policy: folder F with A = 1 and B = 2 and root tags R1 = 1 and R2 = 2; F re-imported holding only A (no value keys) at the root under $policy; then R1 alone (no value keys) at the root under $policy."
  mkprov "$p"
  imp "$id seed (Abort)" "$p" Abort '' "{\"tags\":[{\"name\":\"F\",\"tagType\":\"Folder\",\"tags\":[$(atomic A 1),$(atomic B 2)]},$(atomic R1 1),$(atomic R2 2)]}"
  imp "$id $policy of F holding only A" "$p" "$policy" '' '{"tags":[{"name":"F","tagType":"Folder","tags":[{"name":"A","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory"}]}]}'
  save_answer "$(tr "[:upper:]" "[:lower:]" <<<"$id")-folder"
  exp "$id export after the folder re-import" "$p"
  local b a
  b=$(pick "$p" F/B .); a=$(pick "$p" F/A value)
  reading "$policy of F without B: F/B is $b; F/A value $a."
  imp "$id $policy of R1 alone at the root" "$p" "$policy" '' '{"tags":[{"name":"R1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory"}]}'
  exp "$id export after the root re-import" "$p"
  reading "$policy of R1 alone at the root: R2 is $(pick "$p" R2 .); R1 value $(pick "$p" R1 value)."
  sleep 20
  exp "$id export 20 s later" "$p"
  reading "20 s later: F/B is $(pick "$p" F/B .); R2 is $(pick "$p" R2 .)."
  files "$p"
  delprov "$p"
  CHILD_B=$b
}

c26_children() {
  assert_rig
  children_row C4 P26C4 Overwrite
  local ob=$CHILD_B
  children_row C5 P26C5 MergeOverwrite
  if [ "$ob" = '<absent node>' ]; then
    verdict A2 holds C4
  else
    verdict A2 fails C4
  fi
}

# ------------------------------------------------------------------ C6

c26_props() {
  local p=P26C6
  assert_rig
  sect C6 'Row (c), A3: tags P and Q with documentation and tooltip; P re-imported without tooltip under Overwrite, Q under MergeOverwrite (no value keys either time).'
  mkprov $p
  imp 'C6 seed (Abort): P and Q with documentation doc and tooltip tip' $p Abort '' "{\"tags\":[$(atomic P 1 '"documentation":"doc","tooltip":"tip"'),$(atomic Q 1 '"documentation":"doc","tooltip":"tip"')]}"
  exp 'C6 export after the seed' $p
  local nt='"tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","documentation":"doc"'
  imp 'C6 Overwrite of P without tooltip' $p Overwrite '' "{\"tags\":[{\"name\":\"P\",$nt}]}"
  imp 'C6 MergeOverwrite of Q without tooltip' $p MergeOverwrite '' "{\"tags\":[{\"name\":\"Q\",$nt}]}"
  exp 'C6 export after both' $p
  local pt qt
  pt=$(pick $p P tooltip); qt=$(pick $p Q tooltip)
  reading "Overwrite without tooltip: P tooltip $pt (value $(pick $p P value)). MergeOverwrite without tooltip: Q tooltip $qt (value $(pick $p Q value))."
  if [ "$pt" = '<absent key>' ]; then verdict A3 holds C6; else verdict A3 fails C6; fi
  files $p
  delprov $p
}

# ------------------------------------------------------------------ C7

c26_instance() {
  local p=P26C7
  assert_rig
  sect C7 'A12: UDT type Ty (parameter P1 = 1; members M1 Int4 memory = 0 and M2 String memory with documentation "type doc"); four instances I1..I4, each with P1 = 42, a value override M1 = 5 and a configuration override M2 documentation "override doc". I1 (MergeOverwrite) and I2 (Overwrite) are re-imported bare, without overrides. I3 (MergeOverwrite) and I4 (Overwrite) are re-imported from their own export STRIPPED the way ign strips existing nodes: value, timestamp and quality removed from AtomicTag objects only, the parameters and the member configuration kept.'
  mkprov $p
  imp 'C7 type (Abort): _types_/Ty' $p Abort '' '{"tags":[{"name":"_types_","tagType":"Folder","tags":[{"name":"Ty","tagType":"UdtType","parameters":{"P1":{"dataType":"Integer","value":1}},"tags":[{"name":"M1","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","value":0},{"name":"M2","tagType":"AtomicTag","dataType":"String","valueSource":"memory","value":"x","documentation":"type doc"}]}]}]}'
  local inst='"tagType":"UdtInstance","typeId":"Ty","parameters":{"P1":{"dataType":"Integer","value":42}},"tags":[{"name":"M1","tagType":"AtomicTag","value":5},{"name":"M2","tagType":"AtomicTag","documentation":"override doc"}]'
  imp 'C7 instances I1..I4 (Abort)' $p Abort '' "{\"tags\":[{\"name\":\"I1\",$inst},{\"name\":\"I2\",$inst},{\"name\":\"I3\",$inst},{\"name\":\"I4\",$inst}]}"
  exp 'C7 export after the seed' $p
  local i
  for i in I1 I2 I3 I4; do
    reading "seed $i: P1 $(pick $p $i parameters); M1 value $(pick $p $i/M1 value); M2 documentation $(pick $p $i/M2 documentation); members as exported $(pick $p $i tags)."
  done
  imp 'C7 (i) MergeOverwrite of I1 bare (no parameters, no members)' $p MergeOverwrite '' '{"tags":[{"name":"I1","tagType":"UdtInstance","typeId":"Ty"}]}'
  imp 'C7 (i) Overwrite of I2 bare (no parameters, no members)' $p Overwrite '' '{"tags":[{"name":"I2","tagType":"UdtInstance","typeId":"Ty"}]}'
  # (ii): the instance as exported, AtomicTag value keys removed.
  local strip
  strip='def s: if type == "object" then (if .tagType == "AtomicTag" then del(.value, .timestamp, .quality) else . end) | with_entries(.value |= s) elif type == "array" then map(s) else . end; s'
  local i3 i4
  i3=$(python3 -I -c 'import json,sys; t=json.load(open(sys.argv[1])); print(json.dumps([c for c in t["tags"] if c.get("name")=="I3"][0]))' "$TMPD/export-$p.json" | jq -c "$strip")
  i4=$(python3 -I -c 'import json,sys; t=json.load(open(sys.argv[1])); print(json.dumps([c for c in t["tags"] if c.get("name")=="I4"][0]))' "$TMPD/export-$p.json" | jq -c "$strip")
  imp 'C7 (ii) MergeOverwrite of I3 as exported, AtomicTag value keys stripped' $p MergeOverwrite '' "{\"tags\":[$i3]}"
  imp 'C7 (ii) Overwrite of I4 as exported, AtomicTag value keys stripped' $p Overwrite '' "{\"tags\":[$i4]}"
  exp 'C7 export after the re-imports' $p
  local p3 d3 v3 p4 d4 v4
  for i in I1 I2 I3 I4; do
    reading "after $i: P1 $(pick $p $i parameters); M1 value $(pick $p $i/M1 value); M2 documentation $(pick $p $i/M2 documentation); members as exported $(pick $p $i tags)."
  done
  p3=$(pick $p I3 parameters); d3=$(pick $p I3/M2 documentation); v3=$(pick $p I3/M1 value)
  p4=$(pick $p I4 parameters); d4=$(pick $p I4/M2 documentation); v4=$(pick $p I4/M1 value)
  sleep 20
  exp 'C7 export 20 s later' $p
  reading "20 s later: I3 M1 value $(pick $p I3/M1 value); I4 M1 value $(pick $p I4/M1 value)."
  case "$p3" in *42*) p3=kept ;; *) p3=lost ;; esac
  case "$p4" in *42*) p4=kept ;; *) p4=lost ;; esac
  reading "A12 (stripped re-import): MergeOverwrite parameter $p3, M2 documentation $d3, M1 live value $v3; Overwrite parameter $p4, M2 documentation $d4, M1 live value $v4."
  if [ "$p3" = kept ] && [ "$d3" = '"override doc"' ] && [ "$v3" = 5 ]; then verdict A12 holds C7; else verdict A12 fails C7; fi
  files $p
  delprov $p
}

# ------------------------------------------------------------------ C8

c26_mode() {
  local p=P26C8 mode
  assert_rig
  mode=$(curl -sS -m 30 -H "X-Ignition-API-Token: $IGN_TOKEN_CAP_A" "$BASE/data/api/v1/gateway-info" | jq -r '.deploymentMode')
  sect C8 "Row (d), A4, Pitfall 10, on the rig in deployment mode '$mode': tag T imported into core (documentation core-doc), its tag-definition unit copied into the mode collection with documentation mode-doc, a config scan, then an import edit (MergeOverwrite, documentation edit-doc); which collection the gateway writes and what the export shows."
  [ "$mode" = production ] || { note "Not captured: the rig is not in deployment mode production."; verdict A4 'not captured' C8; return 0; }
  mkprov $p
  imp 'C8 Abort: T = 1, documentation core-doc' $p Abort '' "{\"tags\":[$(atomic T 1 '"documentation":"core-doc"')]}"
  note 'Waiting 20 s for the gateway to flush the tag files (CD4).'
  sleep 20
  files $p
  local src=$BIND/core/ignition/tag-definition/$p dst=$BIND/production/ignition/tag-definition/$p
  if [ ! -d "$src" ]; then
    note "Not captured: no core tag-definition directory for $p after 20 s."
    verdict A4 'not captured' C8
    delprov $p
    return 0
  fi
  mkdir -p "$(dirname "$dst")"
  /bin/rm -rf "$dst"
  cp -R "$src" "$dst"
  grep -rl 'core-doc' "$dst" | while IFS= read -r f; do sed -i '' 's/core-doc/mode-doc/g' "$f"; done
  chown_bind
  shcap "the mode copy: every file under production/ignition/tag-definition/$p and its bytes" bash -c \
    'find production/ignition/tag-definition/'"$p"' -type f | LC_ALL=C sort | while IFS= read -r f; do printf "== %s\n" "$f"; cat "$f"; printf "\n[eof]\n"; done'
  scan
  exp 'C8 export after the mode copy and the scan' $p
  local before
  before=$(pick $p T documentation)
  reading "after the mode copy and the scan: T documentation $before (core-doc means core is effective, mode-doc means the mode copy is)."
  imp 'C8 MergeOverwrite edit: T documentation edit-doc' $p MergeOverwrite '' '{"tags":[{"name":"T","tagType":"AtomicTag","dataType":"Int4","valueSource":"memory","documentation":"edit-doc"}]}'
  save_answer c8-mode-edit
  exp 'C8 export straight after the edit' $p
  local after
  after=$(pick $p T documentation)
  sleep 20
  exp 'C8 export 20 s after the edit' $p
  files $p
  shcap "which collection holds which documentation for $p" bash -c \
    'for c in core production; do printf "%s: " "$c"; grep -rhoE "(core|mode|edit)-doc" "$c/ignition/tag-definition/'"$p"'" 2>/dev/null | sort -u | tr "\n" " "; echo; done'
  reading "straight after the edit the export shows T documentation $after; 20 s later $(pick $p T documentation)."
  local wrote=none c
  for c in core production; do
    grep -rqs 'edit-doc' "$BIND/$c/ignition/tag-definition/$p" && wrote=$c
  done
  reading "the gateway wrote the edit into the $wrote collection (the collection whose tag-definition files now hold edit-doc)."
  if [ "$after" = '"edit-doc"' ]; then verdict A4 holds C8; else verdict A4 fails C8; fi
  /bin/rm -rf "$dst"
  scan
  delprov $p
}

# ------------------------------------------------------------------ C9

c26_timing() {
  local p=P26C9 f=$TMPD/big.json t0 t1 stable
  assert_rig
  sect C9 'A8: one Abort import of 3,000 memory tags in 30 folders (D00..D29, tags N000..N099), the export of the provider, and the delay until the tag files in core stop changing.'
  mkprov $p
  python3 -I -c '
import json
print(json.dumps({"tags": [{"name": "D%02d" % d, "tagType": "Folder", "tags": [
    {"name": "N%03d" % n, "tagType": "AtomicTag", "dataType": "Int4", "valueSource": "memory", "value": n}
    for n in range(100)]} for d in range(30)]}, separators=(",", ":")))' > "$f"
  body_block 'C9 Abort import of 3,000 tags' "$f"
  t0=$(now_ms)
  CAP_MAX=600 cap 'C9 Abort import of 3,000 tags' POST "$IMP?provider=$p&type=json&collisionPolicy=Abort" \
    -H 'Content-Type: application/octet-stream' --data-binary "@$f"
  t1=$(now_ms)
  save_answer c9-3000
  local imp_ms=$((t1 - t0)) imp_status=$LAST_STATUS
  # The flush: poll the provider's tag-definition files in core every second
  # from the answer on, until their count and total size hold for 10 polls.
  local sig prevsig='' first=-1 settled=-1 n=0 dir=$BIND/core/ignition/tag-definition/$p
  t0=$t1; stable=0
  while :; do
    n=$(find "$dir" -type f 2>/dev/null | wc -l | tr -d ' ')
    sig="$n:$(find "$dir" -type f -exec cat {} + 2>/dev/null | wc -c | tr -d ' ')"
    [ "$first" -ge 0 ] || [ "$n" -eq 0 ] || first=$(( $(now_ms) - t0 ))
    if [ "$sig" = "$prevsig" ] && [ "$n" -gt 0 ]; then stable=$((stable + 1)); else stable=0; settled=$(( $(now_ms) - t0 )); fi
    prevsig=$sig
    [ "$stable" -lt 10 ] || break
    [ $(( $(now_ms) - t0 )) -lt 300000 ] || break
    sleep 1
  done
  reading "file flush: the first tag-definition file of $p appeared in core $first ms after the import answered; the files (count:bytes $sig) last changed $settled ms after it and then held for 10 polls."
  t0=$(now_ms)
  CAP_MAX=600 CAP_BODY_MAX=600 exp 'C9 export of the provider' $p
  t1=$(now_ms)
  local exp_ms=$((t1 - t0))
  n=$(python3 -I -c 'import json,sys
t=json.load(open(sys.argv[1]))
def c(x): return sum(1 + c(k) for k in x.get("tags", []))
print(c(t))' "$TMPD/export-$p.json" 2>/dev/null || echo '?')
  reading "the import answered HTTP $imp_status in $imp_ms ms wall time; the export took $exp_ms ms wall time (curl $LAST_TIME s) and holds $n nodes below the provider."
  if [ "$imp_status" = 200 ] && [ "$imp_ms" -lt 300000 ]; then verdict A8 holds C9; else verdict A8 fails C9; fi
  delprov $p
}

# ------------------------------------------------------------------ C11

c26_provider() {
  local p=P26C11 rec sig body
  assert_rig
  sect C11 'Tag-provider settings modify: PUT /data/api/v1/resources/ignition/tag-provider with the signature from find; the request shape, the answer, and which collection the changed resource file lands in.'
  mkprov $p
  sleep 10
  files $p
  cap "GET find tag-provider $p" GET "$RES/find/ignition/tag-provider/$p"
  rec=$(cat "$TMPD/bod")
  sig=$(jq -r '.signature' <<<"$rec")
  body=$(jq -c '[{name: .name, collection: .collection, enabled: .enabled, description: "p26 modified", config: (.config | .settings = ((.settings // {}) + {"allowBackfill": true})), signature: .signature}]' <<<"$rec")
  printf '%s' "$body" > "$TMPD/put.json"
  body_block 'C11 PUT tag-provider modify' "$TMPD/put.json"
  cap 'C11 PUT tag-provider modify' PUT "$RES/ignition/tag-provider" -H 'Content-Type: application/json' --data-binary "@$TMPD/put.json"
  save_answer c11-provider-put
  local st=$LAST_STATUS
  sleep 10
  cap "GET find tag-provider $p after the modify" GET "$RES/find/ignition/tag-provider/$p"
  reading "the modify answered HTTP $st; signature before $sig, after $(jq -r '.signature // empty' "$TMPD/bod"); description after: $(jq -c '.description // null' "$TMPD/bod"); settings after: $(jq -c '.config.settings // null' "$TMPD/bod")."
  shcap "tag-provider resource files for $p in both collections" bash -c \
    'find core production -path "*/tag-provider/'"$p"'*" -type f 2>/dev/null | LC_ALL=C sort | while IFS= read -r f; do printf "== %s\n" "$f"; cat "$f"; printf "\n[eof]\n"; done'
  delprov $p
}

# ------------------------------------------------------------------ C12

c26_canon() {
  local p=P26C12
  assert_rig
  sect C12 'A10: a tree in the git-module export shape (sorted keys, a type parameter without value, instance member stubs {"name","tagType":"AtomicTag"}), imported then exported; each difference per node between the imported JSON and the exported JSON.'
  mkprov $p
  local types='{"tags":[{"name":"_types_","tagType":"Folder","tags":[{"name":"Ty","parameters":{"P1":{"dataType":"Integer"}},"tagType":"UdtType","tags":[{"dataType":"Int4","name":"M1","tagType":"AtomicTag","value":0,"valueSource":"memory"},{"dataType":"String","documentation":"m2","name":"M2","tagType":"AtomicTag","valueSource":"memory"}]}]}]}'
  local rest='{"tags":[{"name":"F","tagType":"Folder","tags":[{"dataType":"Int4","documentation":"t","name":"T","tagType":"AtomicTag","value":3,"valueSource":"memory"},{"name":"I","parameters":{"P1":{"dataType":"Integer","value":7}},"tagType":"UdtInstance","tags":[{"name":"M1","tagType":"AtomicTag"},{"name":"M2","tagType":"AtomicTag"}],"typeId":"Ty"}]}]}'
  imp 'C12 types (Abort)' $p Abort '' "$types"
  imp 'C12 folder F with T and instance I (Abort)' $p Abort '' "$rest"
  exp 'C12 export' $p
  c12_py
  printf '%s' "$types" > "$TMPD/c12-types.json"
  printf '%s' "$rest" > "$TMPD/c12-rest.json"
  shcap 'C12 per-node differences, imported against exported (keys compared after sorting; children keyed by name)' \
    bash -c 'python3 -I - "$@" < "$0" | tee "$1.diff"' "$TMPD/c12.py" "$TMPD/c12-types.json" "$TMPD/c12-rest.json" "$TMPD/export-$p.json"
  local other
  other=$(grep -cvE '^(\[end of differences\]|.*: exported key order .*|.*\.value: sent "<absent>" exported null)$' "$TMPD/c12-types.json.diff" || true)
  reading "A10: $other difference line(s) fall outside the planned canonicalisation rules (key order, an absent parameter value read as null, instance member stubs). Any such line means the REST export does not equal the imported native form after that canonicalisation. The git module's own read (getTagConfigsAsync) is not reachable from outside the gateway and is not captured here."
  if [ "$other" = 0 ]; then verdict A10 holds C12; else verdict A10 fails C12; fi
  delprov $p
}

# c12_py: the C12 comparison program (written to $TMPD/c12.py by c26_canon).
c12_py() {
  cat > "$TMPD/c12.py" <<'PY'
import json, sys
types, rest, exported = (json.load(open(f, encoding="utf-8")) for f in sys.argv[1:4])
sent = {"tags": types["tags"] + rest["tags"]}
def walk(node, path, out):
    for c in node.get("tags", []):
        p = path + "/" + c.get("name", "?") if path else c.get("name", "?")
        out[p] = {k: v for k, v in c.items() if k != "tags"}
        walk(c, p, out)
    return out
a, b = walk(sent, "", {}), walk(exported, "", {})
for p in sorted(set(a) | set(b)):
    if p not in b:
        print(f"{p}: sent, absent from the export"); continue
    if p not in a:
        print(f"{p}: in the export only: {json.dumps(b[p], sort_keys=True)}"); continue
    for k in sorted(set(a[p]) | set(b[p])):
        if a[p].get(k, "<absent>") != b[p].get(k, "<absent>"):
            print(f"{p}.{k}: sent {json.dumps(a[p].get(k, '<absent>'), sort_keys=True)} exported {json.dumps(b[p].get(k, '<absent>'), sort_keys=True)}")
def order(node, path):
    for c in node.get("tags", []):
        p = path + "/" + c.get("name", "?") if path else c.get("name", "?")
        keys = list(c.keys())
        if keys != sorted(keys):
            print(f"{p}: exported key order {keys}")
        order(c, p)
order(exported, "")
print("[end of differences]")
PY
}

# ------------------------------------------------------------------ C13

c26_quality() {
  local p=P26C13 all=counted
  assert_rig
  sect C13 'A5: imports that must fail, each answer recorded: an instance of a missing type, a tag naming a tag group the provider lacks, a malformed body.'
  mkprov $p
  imp 'C13a Abort: an instance of the missing type Nope' $p Abort '' '{"tags":[{"name":"X","tagType":"UdtInstance","typeId":"Nope"}]}'
  save_answer c13-missing-type
  jq -e 'has("successCount")' "$TMPD/bod" > /dev/null 2>&1 || all=other
  imp 'C13b Abort: a tag in the missing tag group NoGroup' $p Abort '' "{\"tags\":[$(atomic G 1 '"tagGroup":"NoGroup"')]}"
  save_answer c13-missing-group
  jq -e 'has("successCount")' "$TMPD/bod" > /dev/null 2>&1 || all=other
  imp 'C13c Abort: a malformed body' $p Abort '' '{"tags":[{"name":"Y",'
  save_answer c13-malformed
  local mal=$LAST_STATUS
  exp 'C13 export after the three' $p
  reading "X is $(pick $p X .); G is $(pick $p G .) (tagGroup $(pick $p G tagGroup)); the malformed body answered HTTP $mal."
  shcap 'every import answer this run recorded: policy, status, body (first 300 bytes)' cat "$STATE/c26-answers"
  if [ "$all" = counted ] && ! awk '$2 == 200 && $3 !~ /successCount/ {bad=1} END {exit !bad}' "$STATE/c26-answers"; then
    verdict A5 holds C13
  else
    verdict A5 fails C13
  fi
  delprov $p
}

# ------------------------------------------------------------------ C14

c26_groups() {
  local p=P26C14
  assert_rig
  sect C14 'Whether tags/export carries tag groups, and whether a tag-group unit written to the mode collection plus a config scan is accepted (a tag referencing the new group imports clean afterwards).'
  mkprov $p
  sleep 10
  exp 'C14 export of an empty provider' $p
  reading "top-level keys of the export: $(jq -c 'keys' "$TMPD/export-$p.json" 2>/dev/null || echo '?')."
  shcap "tag-group units of $p in both collections" bash -c \
    'find core production -path "*/tag-group/'"$p"'/*" -type f 2>/dev/null | LC_ALL=C sort | while IFS= read -r f; do printf "== %s\n" "$f"; cat "$f"; printf "\n[eof]\n"; done'
  local src dst=$BIND/production/ignition/tag-group/$p/G26
  src=$(find "$BIND/core/ignition/tag-group/$p" "$BIND/production/ignition/tag-group/$p" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | head -1 || true)
  if [ -z "$src" ]; then
    note "Not captured: the gateway wrote no tag-group unit for $p to copy from."
    delprov $p
    return 0
  fi
  mkdir -p "$(dirname "$dst")"
  /bin/rm -rf "$dst"
  cp -R "$src" "$dst"
  # A fresh unit: drop the copied uuid/signature attributes the gateway keys on.
  if [ -f "$dst/resource.json" ]; then
    jq -c '.attributes |= (del(.uuid, .lastModification, .lastModificationSignature) // {})' "$dst/resource.json" > "$TMPD/rj"
    cp "$TMPD/rj" "$dst/resource.json"
  fi
  chown_bind
  shcap "the new mode-collection unit production/ignition/tag-group/$p/G26" bash -c \
    'find production/ignition/tag-group/'"$p"'/G26 -type f | LC_ALL=C sort | while IFS= read -r f; do printf "== %s\n" "$f"; cat "$f"; printf "\n[eof]\n"; done'
  scan
  imp 'C14 Abort: tag K in tag group G26' $p Abort '' "{\"tags\":[$(atomic K 1 '"tagGroup":"G26"')]}"
  save_answer c14-group-tag
  local kst=$LAST_STATUS
  exp 'C14 export after the group and the tag' $p
  cap 'C14 GET resources/list/ignition/tag-group (is there a REST read of tag groups?)' GET "$RES/list/ignition/tag-group?limit=-1"
  reading "K tagGroup in the export: $(pick $p K tagGroup); the import answered HTTP $kst. The tag-group resource list answered HTTP $LAST_STATUS. C13 shows that a tag naming a group the provider lacks also imports clean, so a clean import here does not prove the mode-collection unit loaded: whether the gateway accepted G26 is not captured."
  /bin/rm -rf "$BIND/production/ignition/tag-group/$p"
  scan
  delprov $p
}

# ------------------------------------------------------------------ verdict

c26_verdict() {
  local a s r
  {
    printf '\n### Verdicts (single node)\n\n| Assumption | Verdict | Row |\n| --- | --- | --- |\n'
    for a in A1 A2 A3 A4 A5 A8 A10 A12; do
      s=$(awk -F'|' -v a="$a" '$1 == a {s=$2} END {print s}' "$VERDICTS" 2>/dev/null)
      r=$(awk -F'|' -v a="$a" '$1 == a {r=$3} END {print r}' "$VERDICTS" 2>/dev/null)
      printf '| %s | %s | %s |\n' "$a" "${s:-not captured}" "${r:--}"
    done
  } >> "$OUT"
}

case "${1:-}" in
  header26) header26 ;;
  sect) sect "${2:?ID}" "${3:-}" ;;
  note) note "${2:?TEXT}" ;;
  shcap) shift; shcap "$@" ;;
  verdict) verdict "${2:?A}" "${3:?STATUS}" "${4:?ROW}" ;;
  c26-create) c26_create ;;
  c26-value) c26_value ;;
  c26-children) c26_children ;;
  c26-props) c26_props ;;
  c26-instance) c26_instance ;;
  c26-mode) c26_mode ;;
  c26-timing) c26_timing ;;
  c26-provider) c26_provider ;;
  c26-canon) c26_canon ;;
  c26-quality) c26_quality ;;
  c26-groups) c26_groups ;;
  c26-verdict) c26_verdict ;;
  down26) echo "docker compose -p ign-deploy-rig down -v" ;;
  *) echo "usage: capture26.sh header26|sect|note|shcap|verdict|c26-create|c26-value|c26-children|c26-props|c26-instance|c26-mode|c26-timing|c26-provider|c26-canon|c26-quality|c26-groups|c26-verdict|down26" >&2; exit 64 ;;
esac
