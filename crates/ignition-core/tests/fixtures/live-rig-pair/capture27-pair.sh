#!/usr/bin/env bash
# capture27-pair.sh SUBCOMMAND [ARG ...]
#
# Phase 27 (27-04) pair rig driver: the disposable two-node 8.3.6 pair of
# setup-pair.sh (compose project `ign-pair-rig`, gw-a Master on 127.0.0.1:18300,
# gw-b Backup on 127.0.0.1:18301) with the signed git module 2.3.4 on BOTH nodes,
# a git server both nodes reach, and the same `CapApp` seed and `git.yaml`
# (template variant) on both nodes. G5 records what redundancy sync does to the
# backup's `.git`.
#
#   up       idempotent. A rig that is already up with the module is left as is.
#            Otherwise: seed the synthetic repository, write a compose override
#            into this script's state directory ($STATE/capture27), bring the
#            pair up with the module and git.yaml on both nodes, record the
#            pre-pair shapes, publish the release to the remote's production,
#            then `setup-pair.sh mint` and `pair`.
#   reset    both nodes back to the state `up` leaves. A rig `up` built that no
#            `g5` or gate has used yet is already in that state (a marker says
#            so); a rig that is up is cloned again in place (`reclone`: the
#            remote's production back at v1.0.0, CapApp removed on both nodes,
#            both gateways restarted, the release published again); otherwise
#            the rig is built from scratch.
#   g5       the G5 steps (below); everything goes to
#            $STATE/evidence/27-04-g5.log and the last line is
#            `LIVE-RAN capture27-pair g5`, printed only when every step
#            succeeded (the steps run in a subshell with errexit, so the first
#            failing step ends the run before the line)
#   live LOG TEST...
#            (27-25, checker W13) the pair gates of live_deploy_project_git:
#            sources tokens.env, exports IGNITION_LIVE_PAIR=1, IGN_PAIR_DATA_A,
#            IGN_PAIR_DATA_B, IGN_PAIR_REMOTE (the rig's bare repository) and
#            IGN_LIVE_EVIDENCE_DIR, runs cargo test with
#            `--ignored --nocapture --test-threads=1`, tees to
#            $STATE/evidence/LOG.log and fails when cargo fails or a named
#            test's `LIVE-RAN <name>` is not at the end of a log line. A gate
#            prints that line as its last statement, so it means the gate
#            passed; test names are `[a-z0-9_]+` and match whole
#   detect LOG TEST...
#            (27-38) the detection checks: the `live` runner, except that each
#            named test must end its log with an anchored `LIVE-DETECT <name>`
#            line (the check proves the backup project.json guard fires on the
#            rig pair; it is never a passing gate) and the log must not hold a
#            `LIVE-RAN <name>` line for the same name. `git_pair_e2e` is not run
#            by it and stays RED until the git-module fix lands
#   down     remove the pair (compose down -v), the git server and the override
#
# G5 steps (each timed):
#   (a) after both nodes start: did each node's module clone CapApp, does the
#       backup's projects/CapApp/.git exist, what shape has it
#   (b) an overwrite import of the v1.1.0 artifact on the master only: poll the
#       backup's data dir every 2 s for 120 s; does and when does the backup's
#       .git disappear, does its project content converge to the artifact, does
#       the backup's module run its start sync
#   (c) the backup's .git restored from a copy taken before (b), then the same
#       artifact imported again on the master (nothing changes): does the
#       restored .git survive
#   (c2) a master import that changes the content (the v1.0.0 artifact) with the
#       restored .git in place: does the restored .git survive a real change
#   (d) the backup's REST project export before and after (digest only)
#
# This script calls the `ign` on PATH, which is the installed binary and NOT the
# repo build (CLAUDE.md): run `cargo install --path crates/ignition-cli --force`
# before any rig use.
#
# LOOPBACK ONLY. assert_local runs on both base URLs before anything else and
# exits 2 unless the host is localhost, 127.0.0.1 or [::1]. The backup (gw-b) is
# only ever read: every request to it is a GET and its project directory is only
# written by the restore of step (c), which is the point of that step. Tokens come
# from tokens.env (0600, outside the repository) and are never printed and never
# on a command line (curl reads the token header from a config on stdin). The
# rig git.yaml carries a fixed dummy credential the module requires; it is never
# printed. Never enable `set -x` here. Synthetic names only.
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

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(git -C "$HERE" rev-parse --show-toplevel)
DEPLOY_FIX=$HERE/../live-rig-deploy
PROJECT=ign-pair-rig
APP=CapApp
BARE=capapp.git
MODULE_VER=2.3.4
CDATA=/usr/local/bin/ignition/data
# IGN_PAIR_WAIT_S: how long up waits for each node (default 600 s; a loaded
# machine running the amd64 image under emulation needs more).
WAIT_S=${IGN_PAIR_WAIT_S:-600}
STATE=${IGN_PAIR_RIG_STATE:-$HOME/.cache/ign-pair-rig}
umask 077
mkdir -p "$STATE"
chmod 700 "$STATE"
STATE=$(cd "$STATE" && pwd -P)
REPO_REAL=$(cd "$REPO" && pwd -P)
case "$STATE/" in
  "$REPO_REAL"/*) echo "REFUSING: the state directory $STATE is inside the repository $REPO_REAL" >&2; exit 2 ;;
esac
S27=$STATE/capture27
SINGLE=$S27/single # the capture27.sh state directory used for the seed, git.yaml and git server files
export IGN_PAIR_DATA_A=$STATE/data-a
export IGN_PAIR_DATA_B=$STATE/data-b
IGN=${IGN_BIN:-ign}
CFG=$S27/ign-config.toml

die() { echo "$*" >&2; exit 64; }
utc() { date -u +%FT%TZ; }

TMPD=$(mktemp -d)
trap '/bin/rm -rf "$TMPD"' EXIT

dc() {
  (
    cd "$HERE"
    docker compose -p "$PROJECT" -f compose.yml -f "$S27/compose.git.yml" "$@"
  )
}
cid() { docker ps -q --filter "label=com.docker.compose.project=$PROJECT" --filter "label=com.docker.compose.service=gw-$1"; }
base_of() { case "$1" in a) printf '%s' "$BASE_A" ;; b) printf '%s' "$BASE_B" ;; *) die "bad node '$1'" ;; esac; }
pdir() { printf '%s/data-%s/projects/%s' "$STATE" "$1" "$APP"; }

seedgit() { GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git "$@"; }

# curl_tok VAR CURL-ARG...: curl with the gateway API token header taken from
# the variable named VAR and handed to curl as a config on stdin (`-K -`), so
# the token is never on a command line (`ps` shows argv). printf is a shell
# builtin: it starts no process. WR-D4.
curl_tok() {
  local var=$1 v
  shift
  v=${!var:?no token in $var}
  v=${v//\\/\\\\}
  v=${v//\"/\\\"}
  printf 'header = "X-Ignition-API-Token: %s"\n' "$v" | curl -K - "$@"
}

# token_env: the node tokens as the calling process's environment only.
token_env() {
  [ -f "$STATE/tokens.env" ] || die "no $STATE/tokens.env: run up first"
  set -a
  # shellcheck disable=SC1091 # written by setup-pair.sh mint at run time
  . "$STATE/tokens.env"
  set +a
}

# get NODE PATH: a GET with the node's token, body to stdout, never the token.
get() {
  local node=$1 path=$2 var tok
  var=IGN_PAIR_TOKEN_$(printf '%s' "$node" | tr '[:lower:]' '[:upper:]')
  tok=${!var:-}
  [ -n "$tok" ] || die "no token for node $node"
  curl_tok "$var" -sS -m 120 "$(base_of "$node")$path"
}

# wait_git_server: the git server answers the smart-HTTP advertisement from a
# container on the rig network (the nodes reach it the same way), within 60 s.
wait_git_server() {
  local tries=0
  until docker run --rm --network "${PROJECT}_rig" alpine:3 wget -q -O /dev/null "http://git-server/git/$BARE/info/refs?service=git-upload-pack" > /dev/null 2>&1; do
    tries=$((tries + 1))
    [ "$tries" -lt 20 ] || die "the git server does not answer the smart-HTTP advertisement"
    sleep 3
  done
}

wait_running() { # NODE SECS
  local node=$1 secs=${2:-600} t0
  t0=$(date +%s)
  until curl -s -m 5 "$(base_of "$node")/StatusPing" 2> /dev/null | grep -q RUNNING; do
    [ $(( $(date +%s) - t0 )) -lt "$secs" ] || die "gw-$node is not RUNNING within $secs s"
    sleep 3
  done
}

# ------------------------------------------------------------------ rig files

module_path() {
  local dir f n=0 found=''
  for dir in "$HOME/Library/Caches/ignition-cli/modules/git" "${XDG_CACHE_HOME:-$HOME/.cache}/ignition-cli/modules/git"; do
    for f in "$dir"/"$MODULE_VER"-*.modl; do
      [ -f "$f" ] || continue
      found=$f
      n=$((n + 1))
    done
  done
  [ "$n" -eq 1 ] || die "expected exactly one cached git module $MODULE_VER, found $n: run capture27.sh up27 once (ign rig up --with-module git@$MODULE_VER) to fill ign's verified cache"
  # The cache file name carries the sha256 of the signed module.
  local want got
  want=${found##*/}
  want=${want#"$MODULE_VER"-}
  want=${want%.modl}
  got=$(shasum -a 256 "$found" | cut -d' ' -f1)
  [ "$want" = "$got" ] || die "the cached module's digest does not match its file name"
  printf '%s' "$found"
}

write_override() {
  local mod
  mod=$(module_path)
  mkdir -p "$S27" "$IGN_PAIR_DATA_A/projects" "$IGN_PAIR_DATA_B/projects"
  cat > "$S27/compose.git.yml" <<YML
# Written by capture27-pair.sh; regenerated on every up. Disposable.
services:
  git-server:
    build: $SINGLE/git-server
    hostname: git-server
    volumes:
      - $SINGLE/repo:/srv/git
    networks:
      rig: {}
  gw-a:
    depends_on:
      - git-server
    environment:
      ACCEPT_MODULE_CERTS: "com.axone_io.ignition.git"
      ACCEPT_MODULE_LICENSES: "com.axone_io.ignition.git"
      GATEWAY_GIT_USER_SECRET: not-checked
    volumes:
      - type: bind
        source: '$mod'
        target: /usr/local/bin/ignition/user-lib/modules/git.modl
        read_only: true
      - $SINGLE/data/git.yaml:$CDATA/git.yaml:ro
      - $IGN_PAIR_DATA_A/projects:$CDATA/projects
  gw-b:
    depends_on:
      - git-server
    environment:
      ACCEPT_MODULE_CERTS: "com.axone_io.ignition.git"
      ACCEPT_MODULE_LICENSES: "com.axone_io.ignition.git"
      GATEWAY_GIT_USER_SECRET: not-checked
    volumes:
      - type: bind
        source: '$mod'
        target: /usr/local/bin/ignition/user-lib/modules/git.modl
        read_only: true
      - $SINGLE/data/git.yaml:$CDATA/git.yaml:ro
      - $IGN_PAIR_DATA_B/projects:$CDATA/projects
YML
}

# shape DIR LABEL: the shape of a copied or bind-mounted .git: names, counts and
# hex only, no URL and no value of any key.
shape() {
  local g=$1/.git label=$2
  echo "SHAPE [$label]"
  if [ ! -d "$g" ]; then echo "  .git: absent"; return 0; fi
  echo "  HEAD: $(head -c 200 "$g/HEAD" | tr -d '\n')"
  echo "  remotes: $(seedgit config --file "$g/config" --get-regexp '^remote\..*\.fetch$' | sed -E 's/^remote\.//' | paste -sd'|' -)"
  echo "  branch sections: $(seedgit config --file "$g/config" --get-regexp '^branch\.' | paste -sd'|' -)"
  echo "  loose refs: $(cd "$g" && find refs -type f | LC_ALL=C sort | paste -sd' ' -)"
  if [ -f "$g/index" ]; then
    echo "  index: present, signature+version+entries hex=$(od -An -tx1 -N12 "$g/index" | tr -d ' \n')"
  else
    echo "  index: absent"
  fi
  echo "  objects: $(cd "$g" && find objects -type f | wc -l | tr -d ' ') files"
}

# codepy NODE: the return value in the node's cap/util/code.py, or `-`.
codepy() {
  local f
  f=$(pdir "$1")/ignition/script-python/cap/util/code.py
  if [ -f "$f" ]; then grep -o 'return [0-9]*' "$f" | head -n1; else echo '-'; fi
}

util2() { if [ -d "$(pdir "$1")/ignition/script-python/cap/util2" ]; then echo yes; else echo no; fi; }

# ---------------------------------------------------------------------- up

rig_has_module() {
  [ -f "$S27/compose.git.yml" ] && [ -f "$S27/ready" ] && [ -n "$(cid a)" ] && [ -n "$(cid b)" ]
}

# wait_clones: each node's module has finished its clone of CapApp (a finished
# clone has an index; an empty partial `.git` is not a clone), within WAIT_S.
wait_clones() {
  local node end
  end=$(( $(date +%s) + WAIT_S ))
  for node in a b; do
    until [ -f "$(pdir "$node")/.git/index" ]; do
      [ "$(date +%s)" -lt "$end" ] || die "the module on node $node did not clone $APP within $WAIT_S s"
      sleep 5
    done
  done
}

# wait_pair_settled: gw-b reports Backup, linked and Good, within 600 s.
wait_pair_settled() {
  local t0 settled=no
  t0=$(date +%s)
  while [ $(( $(date +%s) - t0 )) -lt 600 ]; do
    if bash "$HERE/setup-pair.sh" status | awk '$1 == "gw-b" && $4 == "true" && $5 == "false" && $7 == "Good" { found = 1 } END { exit !found }'; then settled=yes; break; fi
    sleep 5
  done
  bash "$HERE/setup-pair.sh" status
  [ "$settled" = yes ] || echo "warning: the pair did not report settled within 600 s; g5 records the status as is"
}

# reclone: both nodes back to the state `up` leaves, in place: the remote's
# production back to v1.0.0, CapApp removed on both nodes, both gateways
# restarted (each module clones at its start), then the release published
# again. Much cheaper than a rebuild of the pair.
reclone() {
  token_env
  rig_has_module || die "reclone: the pair with the git module is not up: run up first"
  wait_git_server
  local bare=$SINGLE/repo/$BARE
  seedgit -C "$bare" update-ref refs/heads/production "$(seedgit -C "$bare" rev-parse 'v1.0.0^{commit}')"
  chmod -R a+rX "$bare"
  local node
  for node in a b; do
    docker exec -u root "$(cid "$node")" sh -c "rm -rf $CDATA/projects/$APP"
  done
  docker restart "$(cid a)" "$(cid b)" > /dev/null
  wait_running a "$WAIT_S"
  wait_running b "$WAIT_S"
  wait_clones
  sleep 45
  # The release reaches the remote's production now; the clones stay at v1.0.0.
  IGN_GIT27_RIG_STATE=$SINGLE bash "$DEPLOY_FIX/capture27.sh" publish27
  wait_pair_settled
  : > "$S27/fresh"
  echo "reclone: both nodes cloned at v1.0.0, the remote's production at v1.1.0"
}

up() {
  command -v docker > /dev/null || die "docker is not installed"
  command -v "$IGN" > /dev/null || die "$IGN is not on PATH: run cargo install --path crates/ignition-cli --force"
  if rig_has_module; then
    wait_running a 60
    wait_running b 60
    echo "up: the pair with the git module is already up; left as is"
    return 0
  fi
  local resume=no
  if [ -n "$(docker ps -aq --filter "label=com.docker.compose.project=$PROJECT")" ]; then
    # A start that ran out of patience (a loaded machine runs the amd64 image
    # under emulation) is resumed, never rebuilt.
    [ -f "$S27/starting" ] && [ -f "$S27/compose.git.yml" ] || die "REFUSING: compose project $PROJECT has containers but not this script's git rig; run: bash $0 down"
    resume=yes
    echo "up: resuming a start that had not finished"
  fi
  if [ "$resume" = no ]; then
    mkdir -p "$S27"
    # The seed, the template git.yaml and the git server files come from the
    # single-rig driver, into a state directory of their own.
    /bin/rm -rf "$SINGLE"
    export IGN_GIT27_RIG_STATE=$SINGLE
    mkdir -p "$SINGLE"
    bash "$DEPLOY_FIX/capture27.sh" files27 > /dev/null
    bash "$DEPLOY_FIX/capture27.sh" seed27
    bash "$DEPLOY_FIX/capture27.sh" yaml27 template
    unset IGN_GIT27_RIG_STATE
    /bin/rm -rf "$IGN_PAIR_DATA_A" "$IGN_PAIR_DATA_B"
    mkdir -p "$IGN_PAIR_DATA_A/config/resources" "$IGN_PAIR_DATA_B/config/resources"
    write_override
    : > "$STATE/mode.env"
    : > "$STATE/credentials.log"
    : > "$STATE/step-times.log"
    : > "$STATE/pair-before.log"
    utc > "$STATE/start-utc"
    # The git server is started and proven first: a gateway that starts while
    # the server is not there cannot clone, and its module leaves an empty
    # partial `.git` (compose created the container once without starting it).
    echo "up: starting the git server and proving its smart-HTTP advertisement"
    dc up -d --build git-server > /dev/null
    wait_git_server
    echo "up: starting both nodes with the git module (about two minutes each)"
    IGN_PAIR_MODE_A=-Dign.pair.rig.noop=true IGN_PAIR_MODE_B=-Dign.pair.rig.noop=true dc up -d --build > /dev/null
    wait_git_server
    : > "$S27/starting"
  fi
  wait_running a "$WAIT_S"
  wait_running b "$WAIT_S"
  # Both nodes run unpaired here, so each module's first start clones CapApp on
  # its own node. Wait for both, then record the shapes.
  wait_clones
  sleep 45
  {
    echo "PRE-PAIR (both nodes started unpaired, module cloned on each)"
    shape "$(pdir a)" "node a pre-pair"
    shape "$(pdir b)" "node b pre-pair"
  } > "$S27/pre-pair.txt"
  # The release reaches the remote's production now; the clones stay at v1.0.0.
  IGN_GIT27_RIG_STATE=$SINGLE bash "$DEPLOY_FIX/capture27.sh" publish27
  bash "$HERE/setup-pair.sh" mint > /dev/null
  bash "$HERE/setup-pair.sh" pair
  token_env
  wait_pair_settled
  : > "$S27/ready"
  : > "$S27/fresh"
  echo "up: the pair with the git module is up"
}

reset() {
  if rig_has_module && [ -f "$S27/fresh" ]; then
    echo "reset: the rig is in the state up leaves (no g5 has used it); nothing to do"
    return 0
  fi
  if rig_has_module; then
    echo "reset: cloning CapApp again on both nodes in place"
    reclone
    return 0
  fi
  echo "reset: tearing the pair down and building it again"
  down
  up
}

down() {
  if [ -f "$S27/compose.git.yml" ]; then
    dc down -v --remove-orphans > /dev/null 2>&1 || true
  else
    ( cd "$HERE" && IGN_PAIR_DATA_A=$IGN_PAIR_DATA_A IGN_PAIR_DATA_B=$IGN_PAIR_DATA_B docker compose -p "$PROJECT" -f compose.yml down -v --remove-orphans > /dev/null 2>&1 ) || true
  fi
  # the gateways wrote their project directories as another user
  docker run --rm -v "$STATE:/s" alpine:3 sh -c 'rm -rf /s/data-a /s/data-b /s/capture27/single' > /dev/null 2>&1 || true
  /bin/rm -rf "$S27/compose.git.yml" "$S27/ready" "$S27/fresh" "$S27/starting" "$S27/pre-pair.txt" "$S27/ign-config.toml"
  echo "down: removed compose project $PROJECT, the git server and the override (evidence kept)"
}

# --------------------------------------------------------------------- g5

# digest_b: the backup's REST project export (GET) as member count and a digest
# of (member, content hash). An answer other than HTTP 200, or a body that is
# not a ZIP, is reported as unavailable, never as a digest of no members.
digest_b() {
  local z=$TMPD/b-export.zip list=$TMPD/b-export.list names=$TMPD/b-export.names n code
  /bin/rm -f "$z"
  code=$(curl_tok IGN_PAIR_TOKEN_B -sS -m 120 -o "$z" -w '%{http_code}' "$BASE_B/data/api/v1/projects/export/$APP") \
    || { echo "export unavailable (no answer)"; return 0; }
  [ "$code" = 200 ] || { echo "export unavailable (HTTP $code)"; return 0; }
  unzip -Z1 "$z" > "$names" 2> /dev/null || { echo "export unavailable (the body is not a ZIP)"; return 0; }
  : > "$list"
  while IFS= read -r n; do
    printf '%s %s\n' "$n" "$(unzip -p "$z" "$n" | shasum -a 256 | cut -c1-16)" >> "$list"
  done < <(grep -v '/$' "$names" | LC_ALL=C sort)
  printf 'members=%s git_members=%s digest=%s' "$(wc -l < "$list" | tr -d ' ')" "$(grep -c '^\.git/' "$list" || true)" "$(shasum -a 256 < "$list" | cut -c1-12)"
}

cfg_ign() {
  if ! grep -q '^\[profiles.pair27a\]' "$CFG" 2> /dev/null; then
    IGNITION_CLI_CONFIG=$CFG "$IGN" profile add pair27a "$BASE_A" --token-env IGN_PAIR_TOKEN_A --active > /dev/null
  fi
}

# master_import ZIP: an overwrite import on the master only.
master_import() {
  cfg_ign
  IGNITION_CLI_CONFIG=$CFG "$IGN" --profile pair27a --yes project import "$APP" --file "$1" --collision-policy overwrite > /dev/null
}

# poll_backup LABEL SECS CODE UTIL2: every 2 s, print each change of the backup's
# .git presence, the backup's code.py value and util2 presence, and the master's
# .git presence. Converged means code.py reads CODE and util2 reads UTIL2. Sets
# POLL_GIT_GONE (seconds, or `none`) and POLL_CONVERGED.
POLL_GIT_GONE=none
POLL_CONVERGED=none
poll_backup() {
  local label=$1 secs=$2 want_code=$3 want_util2=$4 t0 now el bg ba cb ub ma last='' cur
  t0=$(date +%s)
  POLL_GIT_GONE=none
  POLL_CONVERGED=none
  bg=$([ -d "$(pdir b)/.git" ] && echo present || echo absent)
  while :; do
    now=$(date +%s)
    el=$((now - t0))
    ba=$([ -d "$(pdir b)/.git" ] && echo present || echo absent)
    cb=$(codepy b)
    ub=$(util2 b)
    ma=$([ -d "$(pdir a)/.git" ] && echo present || echo absent)
    cur="backup_git=$ba backup_code=$cb backup_util2=$ub master_git=$ma"
    if [ "$cur" != "$last" ]; then
      echo "   [$label] t=+${el}s $cur"
      last=$cur
    fi
    if [ "$bg" = present ] && [ "$ba" = absent ] && [ "$POLL_GIT_GONE" = none ]; then POLL_GIT_GONE=$el; fi
    if [ "$cb" = "$want_code" ] && [ "$ub" = "$want_util2" ] && [ "$POLL_CONVERGED" = none ]; then POLL_CONVERGED=$el; fi
    [ "$el" -lt "$secs" ] || break
    sleep 2
  done
}

logs_b() { # SINCE
  docker logs --since "$1" "$(cid b)" 2>&1 \
    | grep -E "Project 'CapApp'|Syncing existing|Fetching from remote|Skipping branch|Sync complete|Importing (tags|images|themes|project)|has no \.git" \
    | grep -v -E 'field:|password|secret' | cut -c1-200 || true
}

g5_steps() {
  local zip=$TMPD/v110.zip zip100=$TMPD/v100.zip ts t_import n
  token_env
  rig_has_module || die "g5: the pair with the git module is not up: run up first"
  rm -f "$S27/fresh"
  assert_local BASE_A "$BASE_A"
  assert_local BASE_B "$BASE_B"
  echo "== G5 start $(utc)"
  echo "== (a) after both nodes start"
  [ ! -f "$S27/pre-pair.txt" ] || cat "$S27/pre-pair.txt"
  for n in a b; do
    echo "node $n: project dir present=$([ -d "$(pdir "$n")" ] && echo yes || echo no) .git present=$([ -d "$(pdir "$n")/.git" ] && echo yes || echo no) code=$(codepy "$n") util2=$(util2 "$n")"
  done
  shape "$(pdir a)" "node a after pairing"
  shape "$(pdir b)" "node b after pairing (the backup)"
  bash "$HERE/setup-pair.sh" status
  echo "backup REST find $APP: HTTP $(curl_tok IGN_PAIR_TOKEN_B -s -o /dev/null -w '%{http_code}' "$BASE_B/data/api/v1/projects/find/$APP")"
  echo "== (d) backup export before: $(digest_b)"
  /bin/rm -rf "$TMPD/b-git-before"
  if [ -d "$(pdir b)/.git" ]; then
    cp -R "$(pdir b)/.git" "$TMPD/b-git-before"
    echo "copy of the backup's .git taken: $(find "$TMPD/b-git-before" -type f | wc -l | tr -d ' ') files"
  else
    echo "the backup has no .git before (b); there is nothing to copy for (c)"
  fi
  seedgit -C "$SINGLE/repo/$BARE" archive --format=zip -o "$zip" v1.1.0 project.json ignition com.inductiveautomation.perspective
  seedgit -C "$SINGLE/repo/$BARE" archive --format=zip -o "$zip100" v1.0.0 project.json ignition com.inductiveautomation.perspective
  echo "== (b) master-only overwrite import of the v1.1.0 artifact"
  ts=$(utc)
  t_import=$(date +%s)
  master_import "$zip"
  echo "   import answered after $(( $(date +%s) - t_import )) s; master .git present=$([ -d "$(pdir a)/.git" ] && echo yes || echo no)"
  poll_backup b 120 'return 2' yes
  echo "RESULT (b) backup .git disappeared after: $POLL_GIT_GONE s (none = still there at 120 s); backup content converged after: $POLL_CONVERGED s (none = not within 120 s)"
  echo "   backup .git at 120 s: present=$([ -d "$(pdir b)/.git" ] && echo yes || echo no)"
  echo "   backup module log lines since the import:"
  logs_b "$ts" | sed 's/^/     /'
  echo "   [(b) backup export after: $(digest_b)]"
  echo "== (c) restore the backup's .git, then import the same artifact again (nothing changes on the master)"
  if [ -d "$TMPD/b-git-before" ] && [ ! -d "$(pdir b)/.git" ]; then
    cp -R "$TMPD/b-git-before" "$(pdir b)/.git"
    echo "restored the backup's .git from the copy ($(find "$(pdir b)/.git" -type f | wc -l | tr -d ' ') files)"
  elif [ -d "$(pdir b)/.git" ]; then
    echo "the backup's .git was never removed; (c) re-imports with the original in place"
  else
    echo "no copy and no .git on the backup; (c) re-imports with the backup holding no .git"
  fi
  ts=$(utc)
  t_import=$(date +%s)
  master_import "$zip"
  echo "   import answered after $(( $(date +%s) - t_import )) s"
  poll_backup c 120 'return 2' yes
  echo "RESULT (c) restored backup .git disappeared after: $POLL_GIT_GONE s (none = survived 120 s)"
  echo "   backup .git at 120 s: present=$([ -d "$(pdir b)/.git" ] && echo yes || echo no)"
  shape "$(pdir b)" "backup after (c)"
  echo "   backup module log lines since the second import:"
  logs_b "$ts" | sed 's/^/     /'
  echo "== (c2) a master import that changes the content (the v1.0.0 artifact), the restored .git in place"
  if [ ! -d "$(pdir b)/.git" ] && [ -d "$TMPD/b-git-before" ]; then
    cp -R "$TMPD/b-git-before" "$(pdir b)/.git"
    echo "restored the backup's .git again before (c2)"
  fi
  ts=$(utc)
  t_import=$(date +%s)
  master_import "$zip100"
  echo "   import answered after $(( $(date +%s) - t_import )) s"
  poll_backup c2 120 'return 1' no
  echo "RESULT (c2) backup .git disappeared after: $POLL_GIT_GONE s (none = survived 120 s); backup content converged after: $POLL_CONVERGED s"
  echo "   backup .git at 120 s: present=$([ -d "$(pdir b)/.git" ] && echo yes || echo no)"
  echo "   backup module log lines since the third import:"
  logs_b "$ts" | sed 's/^/     /'
  echo "== (d) backup export after (c): $(digest_b)"
  echo "== G5 end $(utc)"
  echo "LIVE-RAN capture27-pair g5"
}

g5() {
  local log rc=0
  mkdir -p "$STATE/evidence"
  log=$STATE/evidence/27-04-g5.log
  # WR-D1: a function on the left of `||` runs with errexit off, so a failed
  # step would not stop it and the LIVE-RAN line would still print. A subshell
  # with its own errexit, outside any list, stops at the first failure.
  set +e
  ( set -e; g5_steps ) 2>&1 | tee "$log"
  rc=${PIPESTATUS[0]}
  set -e
  [ "$rc" -eq 0 ] || { echo "g5: failed ($rc); no LIVE-RAN line" >&2; exit "$rc"; }
  tail -n 1 "$log" | grep -qx 'LIVE-RAN capture27-pair g5' || { echo "g5: the log does not end with its LIVE-RAN line" >&2; exit 1; }
}

# pair_run KIND LOG TEST...: the shared runner of `live` and `detect`. KIND is
# the line kind a named test must print as its last statement: LIVE-RAN (a gate
# that passed) or LIVE-DETECT (27-38: a check that the guard fires, which is
# never a passing gate). A `detect` log that also holds a LIVE-RAN line for a
# name is refused, so a detection proof cannot be read as a green pair.
pair_run() {
  local kind=${1:?KIND} name=${2:?LOG-NAME} log rc=0 t missing=0
  shift 2
  [ $# -gt 0 ] || die "$kind: name at least one test"
  [[ $name =~ ^[A-Za-z0-9._-]+$ ]] || die "$kind: LOG-NAME is letters, digits, dot, dash, underscore"
  # WR-D2: a test name is a bare identifier; it is matched as a whole name below
  for t in "$@"; do
    [[ $t =~ ^[a-z0-9_]+$ ]] || die "$kind: a test name is lowercase letters, digits and underscore"
  done
  token_env
  mkdir -p "$STATE/evidence"
  log=$STATE/evidence/$name.log
  # a gate deploys to the pair: the rig is no longer in the state `up` leaves
  /bin/rm -f "$S27/fresh"
  export IGNITION_LIVE_PAIR=1
  export IGN_PAIR_DATA_A IGN_PAIR_DATA_B
  export IGN_PAIR_REMOTE=$SINGLE/repo/$BARE
  export IGN_LIVE_EVIDENCE_DIR=$STATE/evidence
  (
    cd "$REPO"
    cargo test -p ignition-core --test live_deploy_project_git -- --ignored --nocapture --test-threads=1 "$@" 2>&1 | tee "$log"
  ) || rc=$?
  for t in "$@"; do
    if ! grep -qE "(^|[[:space:]])${kind} ${t}\$" "$log"; then
      echo "$kind: the log has no '$kind $t'" >&2
      missing=1
    fi
    if [ "$kind" = LIVE-DETECT ] && grep -qE "(^|[[:space:]])LIVE-RAN ${t}\$" "$log"; then
      echo "$kind: the log also has 'LIVE-RAN $t'; a detection proof is never a passing gate" >&2
      missing=1
    fi
  done
  [ "$rc" -eq 0 ] || { echo "$kind: cargo test exited $rc" >&2; exit "$rc"; }
  [ "$missing" -eq 0 ] || exit 1
  echo "$kind: every named check printed its line; log $log"
}

# live LOG TEST...: the pair gates (27-25).
live() { pair_run LIVE-RAN "$@"; }

# detect LOG TEST...: the detection checks (27-38).
detect() { pair_run LIVE-DETECT "$@"; }

case "${1:-}" in
  up) up ;;
  reset) reset ;;
  g5) g5 ;;
  live) shift; live "$@" ;;
  detect) shift; detect "$@" ;;
  down) down ;;
  *) echo "usage: capture27-pair.sh up|reset|g5|live LOG TEST...|detect LOG TEST...|down" >&2; exit 64 ;;
esac
