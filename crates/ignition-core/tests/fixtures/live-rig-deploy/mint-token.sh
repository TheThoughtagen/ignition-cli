#!/usr/bin/env bash
# mint-token.sh [NAME...]  (default: cap-a cap-b)
#
# Mints config-defined API tokens on the disposable `ign-deploy-rig` gateway,
# each with the custom `Authenticated > Deploy` security level, then restarts
# the gateway. Adapted from the gateway-modes spike (single service `gw`).
#
# Keys are written ONLY to ${IGN_DEPLOY_RIG_STATE:-$HOME/.cache/ign-deploy-rig}/tokens.env
# (directory 0700, file 0600) as `export IGN_TOKEN_CAP_A=cap-a:<key>` lines.
# The script refuses a state directory inside this git repository. Stdout
# carries the state-file path only, never a key. Do not enable `set -x`.
set -euo pipefail
cd "$(dirname "$0")"
HERE=$(pwd)

PROJECT=${IGN_RIG_PROJECT:-ign-deploy-rig}
RES=/usr/local/bin/ignition/data/config/resources
STATE_DIR=${IGN_DEPLOY_RIG_STATE:-$HOME/.cache/ign-deploy-rig}

NAMES=("$@")
[ ${#NAMES[@]} -gt 0 ] || NAMES=(cap-a cap-b)

umask 077
mkdir -p "$STATE_DIR"
chmod 700 "$STATE_DIR"
STATE_REAL=$(cd "$STATE_DIR" && pwd -P)
REPO_ROOT=$(git -C "$HERE" rev-parse --show-toplevel 2>/dev/null || true)
if [ -n "$REPO_ROOT" ]; then
  REPO_REAL=$(cd "$REPO_ROOT" && pwd -P)
  case "$STATE_REAL/" in
    "$REPO_REAL"/*) echo "REFUSING: state dir $STATE_REAL is inside the repository $REPO_REAL" >&2; exit 2 ;;
  esac
fi

CID=$(docker ps -q --filter "label=com.docker.compose.project=$PROJECT" --filter "label=com.docker.compose.service=gw")
[ -n "$CID" ] || { echo "no running gw container in compose project $PROJECT" >&2; exit 1; }

TOKENS_ENV="$STATE_REAL/tokens.env"
: > "$TOKENS_ENV"
chmod 600 "$TOKENS_ENV"
tmp=$(mktemp -d)
trap '/bin/rm -rf "$tmp"' EXIT

NOW_MS="$(date +%s)000"
for name in "${NAMES[@]}"; do
  key=$(openssl rand 32 | openssl base64 -A | tr '+/' '-_' | tr -d '=')
  b64=$(printf '%s' "$key" | tr -- '-_' '+/'); pad=$(( (4 - ${#b64} % 4) % 4 )); b64="$b64$(printf '%*s' "$pad" '' | tr ' ' '=')"
  hash=$(printf '%s' "$b64" | openssl base64 -d -A | openssl dgst -sha256 -binary | openssl base64 -A | tr -- '+/' '-_' | tr -d '=')
  d="$tmp/ignition/api-token/$name"; mkdir -p "$d"
  cat > "$d/config.json" <<JSON
{"profile":{"secureChannelRequired":false,"securityLevels":[{"children":[{"children":[],"name":"Deploy"}],"name":"Authenticated"}],"timestamp":$NOW_MS,"type":"basic-token"},"settings":{"tokenHash":"$hash"}}
JSON
  cat > "$d/resource.json" <<JSON
{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["config.json"],"attributes":{"enabled":true}}
JSON
  var=$(printf '%s' "$name" | tr 'a-z-' 'A-Z_')
  printf 'export IGN_TOKEN_%s=%s:%s\n' "$var" "$name" "$key" >> "$TOKENS_ENV"
done

dx() { docker exec -u root "$CID" "$@"; }
dx mkdir -p "$RES/external" "$RES/core/ignition/security-levels" "$RES/core/ignition/security-properties"
docker cp "$tmp/ignition" "$CID:$RES/external/"
# security-properties must go in core: commissioning creates core/ignition/security-properties, and core overrides external
docker cp "$HERE/security-levels/." "$CID:$RES/core/ignition/security-levels/"
docker cp "$HERE/security-properties/." "$CID:$RES/core/ignition/security-properties/"
dx chown -R 2003:2003 "$RES/external" "$RES/core/ignition/security-levels" "$RES/core/ignition/security-properties"
docker restart "$CID" >/dev/null

echo "$TOKENS_ENV"
