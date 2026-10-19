#!/usr/bin/env bash
# setup-pair.sh SUBCOMMAND [ARG ...]
#
# Phase 24 (24-01) pair rig driver for the disposable two-node 8.3.6 gateway
# pair in compose.yml (compose project `ign-pair-rig`): gw-a (redundancy role
# Master, REST on 127.0.0.1:18300) and gw-b (role Backup, REST on
# 127.0.0.1:18301), linked over the internal `gan` network at gw-a's alias
# `gw-a-gan`, port 8060, TLS. Adapted from the gateway-modes spike (S3) and
# from live-rig-deploy/mint-token.sh. It is NOT an `ign rig` topology.
#
#   up        fresh bind directories, compose up, wait both RUNNING (600 s)
#   mint      one config-defined API token per node (pair-a on gw-a, pair-b on
#             gw-b, `Authenticated > Deploy`), then restart both
#   pair      redundancy config on both nodes (roles, GAN host/port/ssl, the
#             ping settings below), restart both, trust the GAN certificates and
#             approve the incoming connection on gw-a until gw-b peerConnected
#   mode      create the `production` mode on gw-a, wait for gw-b to list it,
#             then recreate gw-b and gw-a with -Dignition.config.mode=production
#             in that order (the spike's S3 order), relinking after each
#   status    one line per node: role activityLevel peerConnected syncPending
#             lastSyncTimestamp projectState deploymentMode
#   cut       drop the link with PUT .../toggleApproval/<id>/false on gw-a
#   link      restore it with .../true on gw-a and relink (trust + approve loop)
#   cut-net   fallback: docker network disconnect of gw-b from `gan` only
#   link-net  fallback: reconnect gw-b to `gan`, then relink
#   secrets NODE PROVIDER NAME
#             (24-02, test setup for C-P7 and the 24-10 secret gate; not an `ign`
#             write) create the secret NAME in the file secret provider PROVIDER
#             on node a|b: the fixed throwaway value is written to /tmp/NAME
#             inside that node's container (never printed, never an argument),
#             and PROVIDER gains a file entry for it (created when absent).
#             Prints only `created NAME in PROVIDER on gw-NODE`.
#   provider NODE PROVIDER TYPE
#             create an empty secret provider (TYPE `file`, `internal` or `remote`;
#             `remote` points at targetServer `ign-pair-a-Backup`, targetProvider
#             `cap24-file`, the one candidate for a listing answer of 501) on
#             node a|b; prints `created provider PROVIDER (TYPE) on gw-NODE`
#   down      PRINTS the teardown command; it does not run it
#
# Ping settings applied by `pair` (gatewayNetworkSetup, milliseconds and counts,
# see the OpenAPI /data/api/v1/redundancy/config schema): pingRate 1000,
# pingTimeout 1000, pingMaxMissed 1200, so the backup tolerates at least
# 1200 s of missed pings before it treats the master as faulted (the research
# Pitfall 3 / Assumption A3 setting; the values are recorded in the captures).
#
# Tokens go ONLY to ${IGN_PAIR_RIG_STATE:-$HOME/.cache/ign-pair-rig}/tokens.env
# (directory 0700, file 0600) as `export IGN_PAIR_TOKEN_A=pair-a:<key>` and
# `export IGN_PAIR_TOKEN_B=pair-b:<key>`. The script refuses a state directory
# inside this git repository, never prints a key and never enables `set -x`.
#
# LOCALHOST ONLY. assert_local runs on both base URLs before anything else
# (before any directory, file or request) and exits 2 unless the host is
# localhost, 127.0.0.1 or [::1]. The authority is cut at the first `/`, `?`,
# `#` or `\` BEFORE userinfo is stripped, so 'http://evil.example#@localhost'
# is refused (the R-01 / WR-08 fix of Phase 21).
#
# Credentials: a node's own API token is tried first. When a call answers
# 401 or 403 with the token (the token lacks the permission), the same call is
# repeated once with HTTP basic auth as the rig's known admin user (compose.yml),
# and the fact is appended to $STATE/credentials.log (call, node, which
# credential answered; no secret). Step durations go to $STATE/step-times.log.
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
PROJECT=ign-pair-rig
RES=/usr/local/bin/ignition/data/config/resources
STATE=${IGN_PAIR_RIG_STATE:-$HOME/.cache/ign-pair-rig}
ADMIN_USER=admin
ADMIN_PW=ign-pair-rig # the disposable rig's known password, compose.yml; loopback only
GAN_HOST=gw-a-gan
PING_RATE=1000
PING_TIMEOUT=1000
PING_MAX_MISSED=1200
WAIT_RUNNING_S=${IGN_PAIR_WAIT_S:-600}

umask 077
mkdir -p "$STATE"
chmod 700 "$STATE"
STATE_REAL=$(cd "$STATE" && pwd -P)
REPO_ROOT=$(git -C "$HERE" rev-parse --show-toplevel 2>/dev/null || true)
if [ -n "$REPO_ROOT" ]; then
  REPO_REAL=$(cd "$REPO_ROOT" && pwd -P)
  case "$STATE_REAL/" in
    "$REPO_REAL"/*) echo "REFUSING: state dir $STATE_REAL is inside the repository $REPO_REAL" >&2; exit 2 ;;
  esac
fi
STATE=$STATE_REAL
export IGN_PAIR_DATA_A=$STATE/data-a
export IGN_PAIR_DATA_B=$STATE/data-b
TOKENS_ENV=$STATE/tokens.env
MODE_ENV=$STATE/mode.env
# shellcheck disable=SC1090
[ -f "$TOKENS_ENV" ] && . "$TOKENS_ENV"
# shellcheck disable=SC1090
[ -f "$MODE_ENV" ] && . "$MODE_ENV"
export IGN_PAIR_MODE_A=${IGN_PAIR_MODE_A:--Dign.pair.rig.noop=true}
export IGN_PAIR_MODE_B=${IGN_PAIR_MODE_B:--Dign.pair.rig.noop=true}

utc() { date -u +%FT%TZ; }
dc() { ( cd "$HERE" && docker compose -p "$PROJECT" -f compose.yml "$@" ); }
cid() { docker ps -q --filter "label=com.docker.compose.project=$PROJECT" --filter "label=com.docker.compose.service=gw-$1"; }
base_of() { case "$1" in a) printf '%s' "$BASE_A" ;; b) printf '%s' "$BASE_B" ;; *) echo "bad node '$1'" >&2; return 2 ;; esac; }

# call NODE METHOD PATH [JSON-DATA]: body to stdout, HTTP status in CALL_STATUS,
# the credential that answered in CALL_CRED. The token first; on 401/403 the
# same call once more with basic auth (logged, no secret).
CALL_STATUS=""
CALL_CRED=""
_curl() { # NODE METHOD PATH DATA CRED
  local node=$1 method=$2 path=$3 data=$4 cred=$5 base tv tok out
  base=$(base_of "$node")
  local args=(-sS -g -m 60 -X "$method" -o "$STATE/.call.out" -w '%{http_code}')
  if [ "$cred" = token ]; then
    tv=IGN_PAIR_TOKEN_$(printf '%s' "$node" | tr 'a-z' 'A-Z')
    tok=${!tv:-}
    args+=(-H "X-Ignition-API-Token: $tok")
  else
    args+=(-u "$ADMIN_USER:$ADMIN_PW")
  fi
  if [ -n "$data" ] || [ "$method" != GET ]; then
    args+=(-H 'Content-Type: application/json' --data "$data")
  fi
  /bin/rm -f "$STATE/.call.out"
  out=$(curl "${args[@]}" "$base$path" 2>/dev/null) || out=000
  CALL_STATUS=$out
  CALL_CRED=$cred
  printf '%s %s\n' "$out" "$cred" > "$STATE/.call.status"
}
# sync_status: reload CALL_STATUS and CALL_CRED after a `$(call ...)`, whose
# subshell cannot set them.
sync_status() { read -r CALL_STATUS CALL_CRED < "$STATE/.call.status"; }
call() {
  local node=$1 method=$2 path=$3 data=${4-} tv tok
  tv=IGN_PAIR_TOKEN_$(printf '%s' "$node" | tr 'a-z' 'A-Z')
  tok=${!tv:-}
  if [ -n "$tok" ]; then
    _curl "$node" "$method" "$path" "$data" token
    case "$CALL_STATUS" in
      401 | 403)
        local first=$CALL_STATUS
        _curl "$node" "$method" "$path" "$data" basic
        printf '%s %s node=%s %s -> token answered %s, basic answered %s\n' "$(utc)" "$method" "$node" "$path" "$first" "$CALL_STATUS" >> "$STATE/credentials.log"
        ;;
    esac
  else
    _curl "$node" "$method" "$path" "$data" basic
  fi
  cat "$STATE/.call.out" 2>/dev/null || true
}
# callq: like call, but the body is dropped (status in CALL_STATUS).
callq() { call "$@" > /dev/null; }

wait_running() { # NODE [SECS]
  local node=$1 secs=${2:-$WAIT_RUNNING_S} base t0
  base=$(base_of "$node")
  t0=$(date +%s)
  until curl -s -m 5 "$base/StatusPing" 2>/dev/null | grep -q RUNNING; do
    [ $(( $(date +%s) - t0 )) -lt "$secs" ] || { echo "gw-$node not RUNNING within $secs s" >&2; return 1; }
    sleep 3
  done
}

# wait_for SECS CMD...: poll CMD every 3 s; prints elapsed seconds, returns 1 on timeout.
wait_for() {
  local secs=$1; shift
  local t0; t0=$(date +%s)
  until "$@"; do
    [ $(( $(date +%s) - t0 )) -lt "$secs" ] || return 1
    sleep 3
  done
  echo $(( $(date +%s) - t0 ))
}

step_begin() { STEP_NAME=$1; STEP_T0=$(date +%s); }
step_end() { printf '%s %s %s s\n' "$(utc)" "$STEP_NAME" "$(( $(date +%s) - STEP_T0 ))" | tee -a "$STATE/step-times.log"; }

redundancy() { call "$1" GET /data/api/v1/redundancy; }
rv() { # NODE FIELD
  redundancy "$1" | jq -r ".$2 | if . == null then empty else tostring end" 2>/dev/null || true
}
linked() { [ "$(rv b peerConnected)" = true ]; }
settled() {
  [ "$(rv b peerConnected)" = true ] && [ "$(rv b syncPending)" = false ] \
    && [ "$(rv b projectState)" = Good ] && [ "$(rv a projectState)" = Good ]
}

# The spike's trust_gan: 8.3 gateway-network trust is per-node local PKI files
# (no API). Move the rejected certificates to trusted, both directions, both nodes.
trust_gan() {
  local n c
  for n in a b; do
    c=$(cid "$n")
    [ -n "$c" ] || continue
    docker exec -u root "$c" sh -c '
      B=/usr/local/bin/ignition/data/config/local/ignition/gateway-network
      for side in server client; do
        R=$B/$side/security/pki/rejected; T=$B/$side/security/pki/trusted/certs
        mkdir -p "$T"
        for f in "$R"/*; do [ -e "$f" ] && mv "$f" "$T/"; done
        chown -R 2003:2003 "$T"
      done' > /dev/null 2>&1 || true
  done
}

enc() { jq -rn --arg s "$1" '$s|@uri'; }

# toggle APPROVE [pending]: PUT toggleApproval on gw-a (the master) for every
# incoming connection whose id names the backup (or, with `pending`, every
# connection waiting for approval). Approval toggling is forbidden on the
# backup node (403), so it is never sent to gw-b.
toggle() { # true|false [only-pending]
  local approve=$1 only_pending=${2:-no} body id ids
  body=$(call a GET /data/api/v1/resources/list/ignition/gateway-network-incoming)
  if [ "$only_pending" = pending ]; then
    ids=$(jq -r '.items[]? | select(.config.securityStatus=="PendingApproval") | .config.connectionId' <<<"$body" 2>/dev/null || true)
  else
    ids=$(jq -r '[.items[]? | .config.connectionId] as $all | ([$all[] | select(test("Backup"))]) as $b | (if ($b|length) > 0 then $b else $all end)[]' <<<"$body" 2>/dev/null || true)
  fi
  [ -n "$ids" ] || return 0
  while IFS= read -r id; do
    [ -n "$id" ] || continue
    callq a PUT "/data/api/v1/gateway-network-incoming/toggleApproval/$(enc "$id")/$approve"
    echo "toggleApproval $approve '$id' on gw-a -> HTTP $CALL_STATUS"
  done <<<"$ids"
}
approve_pending() { toggle true pending > /dev/null; }

relink() { # SECS: trust + approve loop until gw-b peerConnected
  local secs=${1:-300} t0
  t0=$(date +%s)
  while :; do
    trust_gan
    approve_pending
    linked && { echo $(( $(date +%s) - t0 )); return 0; }
    [ $(( $(date +%s) - t0 )) -lt "$secs" ] || return 1
    sleep 5
  done
}

# ---------------------------------------------------------------- subcommands

cmd_up() {
  local d
  for d in "$IGN_PAIR_DATA_A" "$IGN_PAIR_DATA_B"; do
    case "$d" in
      "$STATE"/data-?) ;;
      *) echo "REFUSING: data dir '$d' is not under $STATE" >&2; exit 2 ;;
    esac
  done
  if [ -n "$(docker ps -aq --filter "label=com.docker.compose.project=$PROJECT")" ]; then
    echo "REFUSING: compose project $PROJECT already has containers; tear it down first: docker compose -p $PROJECT down -v" >&2
    exit 2
  fi
  step_begin up
  /bin/rm -rf "$IGN_PAIR_DATA_A" "$IGN_PAIR_DATA_B"
  mkdir -p "$IGN_PAIR_DATA_A/config/resources" "$IGN_PAIR_DATA_B/config/resources"
  : > "$MODE_ENV"
  utc > "$STATE/start-utc"
  : > "$STATE/credentials.log"
  : > "$STATE/step-times.log"
  : > "$STATE/pair-before.log"
  IGN_PAIR_MODE_A=-Dign.pair.rig.noop=true IGN_PAIR_MODE_B=-Dign.pair.rig.noop=true dc up -d
  wait_running a
  wait_running b
  step_end
}

cmd_mint() {
  step_begin mint
  local name node key b64 pad hash now d c var
  tmp=$(mktemp -d)
  trap '/bin/rm -rf "$tmp"' EXIT
  : > "$TOKENS_ENV"
  chmod 600 "$TOKENS_ENV"
  now="$(date +%s)000"
  for node in a b; do
    name=pair-$node
    key=$(openssl rand 32 | openssl base64 -A | tr '+/' '-_' | tr -d '=')
    b64=$(printf '%s' "$key" | tr -- '-_' '+/'); pad=$(( (4 - ${#b64} % 4) % 4 )); b64="$b64$(printf '%*s' "$pad" '' | tr ' ' '=')"
    hash=$(printf '%s' "$b64" | openssl base64 -d -A | openssl dgst -sha256 -binary | openssl base64 -A | tr -- '+/' '-_' | tr -d '=')
    d="$tmp/$node/ignition/api-token/$name"; mkdir -p "$d"
    cat > "$d/config.json" <<JSON
{"profile":{"secureChannelRequired":false,"securityLevels":[{"children":[{"children":[],"name":"Deploy"}],"name":"Authenticated"}],"timestamp":$now,"type":"basic-token"},"settings":{"tokenHash":"$hash"}}
JSON
    cat > "$d/resource.json" <<JSON
{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["config.json"],"attributes":{"enabled":true}}
JSON
    var=IGN_PAIR_TOKEN_$(printf '%s' "$node" | tr 'a-z' 'A-Z')
    printf 'export %s=%s:%s\n' "$var" "$name" "$key" >> "$TOKENS_ENV"
    c=$(cid "$node")
    [ -n "$c" ] || { echo "no running gw-$node container in compose project $PROJECT" >&2; exit 1; }
    docker exec -u root "$c" mkdir -p "$RES/external" "$RES/core/ignition/security-levels" "$RES/core/ignition/security-properties"
    docker cp "$tmp/$node/ignition" "$c:$RES/external/"
    # security-properties must go in core: commissioning creates core/ignition/security-properties, and core overrides external
    docker cp "$HERE/../live-rig-deploy/security-levels/." "$c:$RES/core/ignition/security-levels/"
    docker cp "$HERE/../live-rig-deploy/security-properties/." "$c:$RES/core/ignition/security-properties/"
    docker exec -u root "$c" chown -R 2003:2003 "$RES/external" "$RES/core/ignition/security-levels" "$RES/core/ignition/security-properties"
  done
  docker restart "$(cid a)" "$(cid b)" > /dev/null
  sleep 10
  wait_running a
  wait_running b
  step_end
  echo "$TOKENS_ENV"
}

# cfg ROLE JSON: the redundancy config with the role, the GAN host/port/ssl
# and the ping settings applied.
cfg() {
  jq -c --arg role "$1" --arg host "$GAN_HOST" \
    --argjson rate "$PING_RATE" --argjson tmo "$PING_TIMEOUT" --argjson miss "$PING_MAX_MISSED" '
    .role=$role
    | .gatewayNetworkSetup.host=$host
    | .gatewayNetworkSetup.port=8060
    | .gatewayNetworkSetup.enableSsl=true
    | .gatewayNetworkSetup.pingRate=$rate
    | .gatewayNetworkSetup.pingTimeout=$tmo
    | .gatewayNetworkSetup.pingMaxMissed=$miss
    | .masterRecoveryMode="Automatic"' <<<"$2"
}

cmd_pair() {
  step_begin pair
  local node role body new
  for node in a b; do
    [ "$node" = a ] && role=Master || role=Backup
    body=$(call "$node" GET /data/api/v1/redundancy/config); sync_status
    [ "$CALL_STATUS" = 200 ] || { echo "GET redundancy/config on gw-$node answered $CALL_STATUS" >&2; exit 1; }
    echo "gw-$node redundancy/config before (defaults): $(jq -c '{role, joinWaitTime, masterRecoveryMode, gatewayNetworkSetup: (.gatewayNetworkSetup | {host, port, enableSsl, pingRate, pingTimeout, pingMaxMissed})}' <<<"$body")" | tee -a "$STATE/pair-before.log"
    new=$(cfg "$role" "$body")
    callq "$node" PUT /data/api/v1/redundancy/config "$new"
    echo "gw-$node PUT redundancy/config -> HTTP $CALL_STATUS, credential $CALL_CRED"
    [ "$CALL_STATUS" = 200 ] || [ "$CALL_STATUS" = 204 ] || { echo "PUT redundancy/config on gw-$node failed" >&2; exit 1; }
    printf '%s PUT redundancy/config node=%s credential=%s status=%s\n' "$(utc)" "$node" "$CALL_CRED" "$CALL_STATUS" >> "$STATE/credentials.log"
  done
  dc restart gw-a gw-b > /dev/null
  sleep 10
  wait_running a
  wait_running b
  local s
  s=$(relink 300) || { echo "pair not linked within 300 s" >&2; exit 1; }
  echo "linked after $s s"
  step_end
}

cmd_mode() {
  step_begin mode
  local r md s
  if call a GET /data/api/v1/mode | jq -e '.items[]? | select(.name=="production")' > /dev/null 2>&1; then
    echo "precondition: mode production already exists; run on a fresh rig" >&2; exit 1
  fi
  r=$(call a POST /data/api/v1/mode '{"name":"production","title":"Production","description":"ign-pair-rig"}'); sync_status
  [ "$CALL_STATUS" = 200 ] || { echo "POST /mode answered $CALL_STATUS" >&2; exit 1; }
  has_mode_b() { call b GET /data/api/v1/mode | jq -e '.items[]? | select(.name=="production")' > /dev/null 2>&1; }
  md=$(wait_for 600 has_mode_b) || { echo "gw-b never listed the mode" >&2; exit 1; }
  echo "mode_definition_sync_seconds=$md"
  # backup first, then master (the spike's S3 order)
  printf 'export IGN_PAIR_MODE_B=-Dignition.config.mode=production\n' >> "$MODE_ENV"
  export IGN_PAIR_MODE_B=-Dignition.config.mode=production
  dc up -d gw-b > /dev/null
  sleep 10
  wait_running b
  s=$(relink 300) || { echo "not relinked after gw-b recreate" >&2; exit 1; }
  echo "relinked after gw-b recreate: $s s"
  printf 'export IGN_PAIR_MODE_A=-Dignition.config.mode=production\n' >> "$MODE_ENV"
  export IGN_PAIR_MODE_A=-Dignition.config.mode=production
  dc up -d gw-a > /dev/null
  sleep 10
  wait_running a
  s=$(relink 300) || { echo "not relinked after gw-a recreate" >&2; exit 1; }
  echo "relinked after gw-a recreate: $s s"
  s=$(wait_for 600 settled) || { echo "pair not settled within 600 s" >&2; cmd_status; exit 1; }
  echo "settled after $s s"
  step_end
}

# jf FIELD JSON: one field, "?" when the member is absent (false and "" are kept).
jf() { jq -r ".$1 | if . == null then \"?\" elif . == \"\" then \"-\" else tostring end" <<<"$2" 2>/dev/null || echo '?'; }

cmd_status() {
  local node st info
  for node in a b; do
    st=$(redundancy "$node")
    info=$(call "$node" GET /data/api/v1/gateway-info)
    printf 'gw-%s %s %s %s %s %s %s %s\n' "$node" \
      "$(jf role "$st")" "$(jf activityLevel "$st")" "$(jf peerConnected "$st")" "$(jf syncPending "$st")" \
      "$(jf lastSyncTimestamp "$st")" "$(jf projectState "$st")" "$(jf deploymentMode "$info")"
  done
}

cmd_cut() { toggle false; }
cmd_link() {
  local s
  toggle true
  s=$(relink 300) || { echo "not relinked within 300 s" >&2; exit 1; }
  echo "peerConnected true after $s s"
}
cmd_cut_net() { docker network disconnect "${PROJECT}_gan" "$(cid b)"; echo "gw-b disconnected from ${PROJECT}_gan"; }
cmd_link_net() {
  local s
  docker network connect "${PROJECT}_gan" "$(cid b)"
  s=$(relink 300) || { echo "not relinked within 300 s" >&2; exit 1; }
  echo "gw-b reconnected to ${PROJECT}_gan; peerConnected true after $s s"
}
# The throwaway secret value of the C-P7 / 24-10 setup. It lives only here and is
# piped into a file inside the disposable container; it is never printed, never an
# argument and never recorded.
CANARY_VALUE='CAP24-CANARY-VALUE'
RESAPI=/data/api/v1/resources
SPT=ignition/secret-provider

# put_provider NODE PROVIDER CONFIG-JSON: create the provider, or replace its
# config with the signature that find answered. Fails on a non-200.
put_provider() {
  local node=$1 prov=$2 cfg=$3 found sig body
  found=$(call "$node" GET "$RESAPI/find/$SPT/$(enc "$prov")"); sync_status
  if [ "$CALL_STATUS" = 200 ]; then
    sig=$(jq -r '.signature // empty' <<<"$found")
    body=$(jq -c --arg n "$prov" --arg s "$sig" --argjson c "$cfg" '[{name: $n, signature: $s, config: $c}]' <<<'{}')
    callq "$node" PUT "$RESAPI/$SPT" "$body"
  else
    body=$(jq -c --arg n "$prov" --argjson c "$cfg" '[{name: $n, config: $c}]' <<<'{}')
    callq "$node" POST "$RESAPI/$SPT" "$body"
  fi
  [ "$CALL_STATUS" = 200 ] || { echo "secret-provider write for $prov on gw-$node answered $CALL_STATUS" >&2; return 1; }
}

cmd_provider() { # NODE PROVIDER TYPE
  local node=${1:?node a|b} prov=${2:?provider} type=${3:?file|internal} cfg
  base_of "$node" > /dev/null
  case "$type" in
    file) cfg='{"profile":{"type":"file"},"settings":{"files":{}}}' ;;
    internal) cfg='{"profile":{"type":"internal"},"settings":{"secrets":{}}}' ;;
    remote) cfg='{"profile":{"type":"remote"},"settings":{"targetServer":"ign-pair-a-Backup","targetProvider":"cap24-file"}}' ;;
    *) echo "provider type must be file, internal or remote" >&2; exit 64 ;;
  esac
  put_provider "$node" "$prov" "$cfg"
  echo "created provider $prov ($type) on gw-$node"
}

cmd_secrets() { # NODE PROVIDER NAME
  local node=${1:?node a|b} prov=${2:?provider} name=${3:?secret name} c found cfg
  base_of "$node" > /dev/null
  case "$name" in */* | *..* | '') echo "REFUSING: secret name '$name' is not a plain file name" >&2; exit 2 ;; esac
  c=$(cid "$node")
  [ -n "$c" ] || { echo "no running gw-$node container" >&2; exit 1; }
  # the value goes into the container through stdin only
  printf '%s' "$CANARY_VALUE" | docker exec -i -u root "$c" sh -c "cat > '/tmp/$name' && chown 2003:2003 '/tmp/$name' && chmod 600 '/tmp/$name'"
  found=$(call "$node" GET "$RESAPI/find/$SPT/$(enc "$prov")"); sync_status
  if [ "$CALL_STATUS" = 200 ]; then
    cfg=$(jq -c --arg n "$name" '.config | .settings.files = ((.settings.files // {}) + {($n): {filePath: ("/tmp/" + $n), fileType: "CLEARTEXT"}})' <<<"$found")
  else
    cfg=$(jq -cn --arg n "$name" '{profile: {type: "file"}, settings: {files: {($n): {filePath: ("/tmp/" + $n), fileType: "CLEARTEXT"}}}}')
  fi
  put_provider "$node" "$prov" "$cfg"
  echo "created $name in $prov on gw-$node"
}
cmd_down() { echo "docker compose -p $PROJECT down -v"; }

case "${1:-}" in
  up) cmd_up ;;
  mint) cmd_mint ;;
  pair) cmd_pair ;;
  mode) cmd_mode ;;
  status) cmd_status ;;
  cut) cmd_cut ;;
  link) cmd_link ;;
  cut-net) cmd_cut_net ;;
  link-net) cmd_link_net ;;
  secrets) shift; cmd_secrets "$@" ;;
  provider) shift; cmd_provider "$@" ;;
  down) cmd_down ;;
  *) echo "usage: setup-pair.sh up|mint|pair|mode|status|cut|link|cut-net|link-net|secrets|provider|down" >&2; exit 64 ;;
esac
