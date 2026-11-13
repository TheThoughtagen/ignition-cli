#!/usr/bin/env bash
# capture27.sh SUBCOMMAND [ARG ...]
#
# Phase 27 (27-03) rig driver: a disposable Ignition 8.3.6 gateway with the
# signed git module 2.3.4 and a git server whose remote has the branches `main`
# and `production` and no `master` (D-17), so the keep-and-advance handover of
# the 27-01/27-02 engine can be proven against JGit inside the module.
#
#   up27             write the rig files, build the git server, start the rig
#                    with the module (`ign rig up --with-module git@2.3.4`)
#   seed27           (re)build the synthetic repository `capapp.git`: project
#                    CapApp, branches main + production (at v1.0.0), tags
#                    v1.0.0 and v1.1.0 (v1.1.0 is a child commit no branch
#                    holds); the same history every time
#   reset27          the state right after the first bring-up: seed27, the
#                    template git.yaml, CapApp removed and re-cloned by the
#                    module at v1.0.0; every live gate is re-runnable after it
#   yaml27 MODE      git.yaml variant, MODE template | production
#   mint27           mint the rig API token into tokens.env (0600), never stdout
#   restart27        restart the gateway and wait until CapApp is cloned
#   script27 FILE    run a Jython file on the gateway (`ign script run`)
#   status27         JGit view of CapApp: uncommitted paths and branch counts
#   live27 LOG TEST... run the #[ignore] live gates with IGNITION_LIVE_GIT=1,
#                    tee everything to evidence/LOG.log, fail unless every named
#                    test printed its LIVE-RAN line
#   g2real TAG       static counts of a real repository at a release tag
#                    (G2_REAL_REPO, a local clone outside this repository)
#   down27           remove the rig, its volumes and its state directory
#
# This script calls the `ign` on PATH (`ign rig up`, `ign script run`,
# `ign webdev deploy`), which is the installed binary and NOT the repo build
# (CLAUDE.md): run `cargo install --path crates/ignition-cli --force` before any
# rig use.
#
# LOOPBACK ONLY. The gateway URL is checked first (before any file is written
# and before any request) and must be localhost, 127.0.0.1 or [::1]. The rig
# git.yaml carries a fixed dummy credential the module requires; it and the API
# token are never printed. Never enable `set -x` here. Synthetic names only.
set -euo pipefail

PORT=18292
PROJ=ign-git27-rig
GW_NAME=ign-git27-rig
MODULE_VER=${GIT27_MODULE_VERSION:-2.3.4}
BASE=${IGN_GIT27_RIG_URL:-http://127.0.0.1:$PORT}
# The module's server-side repository name and the project it clones into.
APP=CapApp
BARE=capapp.git
PRJ=/data/api/v1/projects
CONTAINER_DATA=/usr/local/bin/ignition/data

assert_local() {
  local rest authority hostport host
  case "$BASE" in
    *[[:space:]]* | *[[:cntrl:]]*) echo "REFUSING: the gateway URL holds whitespace or a control character" >&2; exit 2 ;;
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
    *) echo "REFUSING: the gateway host '$host' is not localhost, 127.0.0.1 or ::1" >&2; exit 2 ;;
  esac
}
assert_local

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(git -C "$HERE" rev-parse --show-toplevel)
STATE=${IGN_GIT27_RIG_STATE:-$HOME/.cache/ign-git27-rig}
DATA=$STATE/data
CFG=$STATE/ign-config.toml
IGN=${IGN_BIN:-ign}

die() { echo "$*" >&2; exit 64; }

# The state directory holds the gateway data, tokens and evidence: never inside
# the repository.
case "$STATE" in
  /*) ;;
  *) die "REFUSING: the state directory '$STATE' is not an absolute path" ;;
esac
mkdir -p "$STATE"
STATE=$(cd "$STATE" && pwd -P)
REPO_REAL=$(cd "$REPO" && pwd -P)
case "$STATE/" in
  "$REPO_REAL"/*) echo "REFUSING: the state directory $STATE is inside the repository $REPO_REAL" >&2; exit 2 ;;
esac
DATA=$STATE/data
CFG=$STATE/ign-config.toml
chmod 700 "$STATE"

TMPD=$(mktemp -d)
trap '/bin/rm -rf "$TMPD"' EXIT

dc() {
  local f=(-f "$STATE/compose.yml")
  [ ! -f "$STATE/compose.ign-modules.yml" ] || f+=(-f "$STATE/compose.ign-modules.yml")
  docker compose -p "$PROJ" "${f[@]}" "$@"
}

cid() {
  docker ps -q --filter "label=com.docker.compose.project=$PROJ" --filter "label=com.docker.compose.service=gw"
}

# token_env: the rig token as an environment of the calling process only.
token_env() {
  [ -f "$STATE/tokens.env" ] || die "no $STATE/tokens.env: run mint27 first"
  set -a
  # shellcheck disable=SC1091 # the file is written by mint27 at run time
  . "$STATE/tokens.env"
  set +a
  BASE=${IGN_GIT27_RIG_URL:?tokens.env holds no IGN_GIT27_RIG_URL}
  assert_local
}

# ------------------------------------------------------------------ rig files

write_rig_files() {
  # Only projects/ and config/resources are bound onto the host: an empty bind
  # over the whole data directory hides the image's factory data and the
  # gateway cannot start (no gateway.xml_clean). git.yaml sits beside them and
  # is bound as one file, so yaml27 can rewrite it in place.
  mkdir -p "$STATE/git-server" "$STATE/repo" "$DATA/projects" "$DATA/config/resources"
  [ -e "$DATA/git.yaml" ] || : > "$DATA/git.yaml"
  cat > "$STATE/compose.yml" <<YML
name: $PROJ
services:
  git-server:
    build: ./git-server
    hostname: git-server
    volumes:
      - ./repo:/srv/git
  gw:
    image: inductiveautomation/ignition:8.3.6
    platform: linux/amd64
    hostname: gw
    depends_on:
      - git-server
    ports:
      - "127.0.0.1:$PORT:8088"
    environment:
      ACCEPT_IGNITION_EULA: "Y"
      GATEWAY_ADMIN_USERNAME: admin
      GATEWAY_ADMIN_PASSWORD: $PROJ
      GATEWAY_GIT_USER_SECRET: not-checked
      IGNITION_EDITION: standard
      IGNITION_UID: "2003"
      IGNITION_GID: "2003"
    command: ["-n", "$GW_NAME"]
    volumes:
      - gw-data:$CONTAINER_DATA
      - ./data/projects:$CONTAINER_DATA/projects
      - ./data/config/resources:$CONTAINER_DATA/config/resources
      - ./data/git.yaml:$CONTAINER_DATA/git.yaml:ro
volumes:
  gw-data: {}
YML
  # httpd + git-http-backend. A plain http remote keeps the module on its
  # username/password transport, the one production gateways use.
  cat > "$STATE/git-server/Dockerfile" <<'DF'
FROM httpd:2.4
RUN apt-get update && apt-get install -y --no-install-recommends git && rm -rf /var/lib/apt/lists/* \
    && git config --system safe.directory '*'
RUN sed -i -E 's|^[[:space:]]*#(LoadModule cgid?_module)|\1|' /usr/local/apache2/conf/httpd.conf \
    && grep -qE '^LoadModule cgid_module' /usr/local/apache2/conf/httpd.conf \
    && echo "Include conf/httpd-git.conf" >> /usr/local/apache2/conf/httpd.conf
COPY httpd-git.conf /usr/local/apache2/conf/httpd-git.conf
EXPOSE 80
CMD ["httpd-foreground"]
DF
  cat > "$STATE/git-server/httpd-git.conf" <<'CONF'
SetEnv GIT_PROJECT_ROOT /srv/git
SetEnv GIT_HTTP_EXPORT_ALL

ScriptAlias /git/ /usr/lib/git-core/git-http-backend/

<Directory "/usr/lib/git-core">
    AllowOverride None
    Options +ExecCGI -MultiViews +SymLinksIfOwnerMatch
    Require all granted
</Directory>

<Directory "/srv/git">
    AllowOverride None
    Require all granted
</Directory>

<Location /git>
    Require all granted
</Location>
CONF
  printf '[rigs.git27]\ncompose_file = "%s"\n' "$STATE/compose.yml" > "$CFG"
}

# ---------------------------------------------------------------------- seed

# seed_commit DIR MESSAGE: one commit with a fixed identity and date, so the
# seed history is the same every run.
seed_commit() {
  local dir=$1 msg=$2
  GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
    GIT_AUTHOR_NAME=rig GIT_AUTHOR_EMAIL=rig@example.invalid GIT_AUTHOR_DATE=$3 \
    GIT_COMMITTER_NAME=rig GIT_COMMITTER_EMAIL=rig@example.invalid GIT_COMMITTER_DATE=$3 \
    git -C "$dir" -c commit.gpgsign=false -c tag.gpgsign=false commit -q -m "$msg"
}

seed_tag() {
  local dir=$1 tag=$2 when=$3
  GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
    GIT_COMMITTER_NAME=rig GIT_COMMITTER_EMAIL=rig@example.invalid GIT_COMMITTER_DATE=$when \
    git -C "$dir" -c tag.gpgsign=false tag -f -a "$tag" -m "$tag"
}

seedgit() {
  GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git "$@"
}

# seed27: project CapApp in the repository root, compact JSON resource files
# (the gateway pretty-prints them, which is the gateway-form drift), a folder
# whose resource.json lists code.py only, and out-of-scope paths (tags, images,
# README, deploy) the import removes from disk.
seed27() {
  local work=$STATE/seed-work bare=$STATE/repo/$BARE
  /bin/rm -rf "$work" "$bare"
  mkdir -p "$work" "$STATE/repo"
  seedgit -C "$work" init -q -b main
  printf 'synthetic CapApp repository\n' > "$work/README.md"
  seedgit -C "$work" add -A
  seed_commit "$work" 'init' '2026-01-01T00:00:00Z'

  seedgit -C "$work" checkout -q -b production
  mkdir -p "$work/ignition/script-python/cap/util" \
    "$work/com.inductiveautomation.perspective/views/V" \
    "$work/tags/default" "$work/images" "$work/deploy"
  printf '{"title":"CapApp live git rig","enabled":true,"inheritable":false}\n' > "$work/project.json"
  printf 'def f():\n    return 1\n' > "$work/ignition/script-python/cap/util/code.py"
  printf '{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["code.py"],"attributes":{}}\n' > "$work/ignition/script-python/cap/util/resource.json"
  printf '{"custom":{},"params":{},"props":{},"root":{"type":"ia.container.flex","children":[]}}\n' > "$work/com.inductiveautomation.perspective/views/V/view.json"
  printf '{"scope":"G","version":1,"restricted":false,"overridable":true,"files":["view.json"],"attributes":{}}\n' > "$work/com.inductiveautomation.perspective/views/V/resource.json"
  printf '{"tags":[]}\n' > "$work/tags/default/tags.json"
  printf 'logo\n' > "$work/images/logo.txt"
  printf 'schema_version: 1\nproject: %s\nenvironments: [rig]\n' "$APP" > "$work/deploy/project.yaml"
  printf '*.pyc\n' > "$work/.gitignore"
  seedgit -C "$work" add -A
  seed_commit "$work" 'release 1.0.0' '2026-01-02T00:00:00Z'
  seed_tag "$work" v1.0.0 '2026-01-02T00:00:00Z'

  # v1.1.0 is a tag on a child of v1.0.0 that no branch holds: the branch
  # `production` stays at v1.0.0, which is what the module clones, and the
  # release checkout is taken at the tag. It changes code.py, adds a second
  # script folder, and adds NOTES.txt inside a resource folder whose
  # resource.json does not list it (the gateway keeps only what a resource.json
  # lists).
  seedgit -C "$work" checkout -q -b release-line
  printf 'def f():\n    return 2\n' > "$work/ignition/script-python/cap/util/code.py"
  printf 'notes the gateway does not keep\n' > "$work/ignition/script-python/cap/util/NOTES.txt"
  mkdir -p "$work/ignition/script-python/cap/util2"
  printf 'def g():\n    return 3\n' > "$work/ignition/script-python/cap/util2/code.py"
  printf '{"scope":"A","version":1,"restricted":false,"overridable":true,"files":["code.py"],"attributes":{}}\n' > "$work/ignition/script-python/cap/util2/resource.json"
  seedgit -C "$work" add -A
  seed_commit "$work" 'release 1.1.0' '2026-01-03T00:00:00Z'
  seed_tag "$work" v1.1.0 '2026-01-03T00:00:00Z'

  seedgit init -q --bare -b main "$bare"
  seedgit -C "$bare" config http.receivepack true
  seedgit -C "$work" push -q -f "$bare" main production v1.0.0 v1.1.0
  [ "$(seedgit -C "$bare" rev-parse production)" = "$(seedgit -C "$bare" rev-parse 'v1.0.0^{commit}')" ] \
    || die "seed27: production is not at v1.0.0"
  [ -z "$(seedgit -C "$bare" branch --list master)" ] || die "seed27: the remote has a master branch"
  # The server runs as another user inside its container and reads this tree.
  chmod -R a+rX "$bare"
  /bin/rm -rf "$work"
  echo "seed27: $BARE holds main and production (at v1.0.0), tags v1.0.0 and v1.1.0"
}

# ---------------------------------------------------------------- git.yaml

# yaml27 MODE: write $DATA/git.yaml IN PLACE (the inode stays). The credential
# key holds a fixed dummy the module requires; the file is never printed.
#   template    repo_branch production, production_mode false
#   production  production_mode true, production_branch production,
#               production_tagPattern "v*"
# GIT27_IMPORTS=true turns the three commissioning imports on (default false).
yaml27() {
  local mode=${1:?MODE (template or production)} imports=${GIT27_IMPORTS:-false} out
  case "$mode" in template | production) ;; *) die "yaml27: MODE is template or production" ;; esac
  case "$imports" in true | false) ;; *) die "yaml27: GIT27_IMPORTS is true or false" ;; esac
  mkdir -p "$DATA"
  out=$TMPD/git.yaml
  {
    printf -- '- repo_uri: http://git-server/git/%s\n' "$BARE"
    printf '  repo_branch: production\n'
    printf '  ignition_projectName: %s\n' "$APP"
    printf '  ignition_userName: admin\n'
    printf '  ignition_inheritable: false\n'
    printf '  user_name: rig\n'
    printf '  user_email: rig@example.invalid\n'
    printf '  user_password: not-checked\n'
    printf '  commissioning_importThemes: %s\n' "$imports"
    printf '  commissioning_importTags: %s\n' "$imports"
    printf '  commissioning_importImages: %s\n' "$imports"
    printf '  commissioning_enforceBranch: false\n'
    if [ "$mode" = production ]; then
      printf '  production_mode: true\n'
      printf '  production_branch: production\n'
      printf '  production_tagPattern: "v*"\n'
    else
      printf '  production_mode: false\n'
    fi
  } > "$out"
  cat "$out" > "$DATA/git.yaml"
  chmod 644 "$DATA/git.yaml"
  echo "yaml27: git.yaml written ($mode; the credential key is not shown)"
}

# --------------------------------------------------------------- gateway waits

wait_running() {
  local limit=${1:-600} end
  end=$(( $(date +%s) + limit ))
  while [ "$(date +%s)" -lt "$end" ]; do
    if curl -s -m 5 "$BASE/StatusPing" 2> /dev/null | grep -q RUNNING; then return 0; fi
    sleep 5
  done
  return 1
}

# wait_app LIMIT: CapApp answers `find` with the rig token (the module has
# cloned it).
wait_app() {
  local limit=${1:-420} end code
  end=$(( $(date +%s) + limit ))
  while [ "$(date +%s)" -lt "$end" ]; do
    code=$(curl -sS -m 10 -o /dev/null -w '%{http_code}' -H "X-Ignition-API-Token: ${IGN_GIT27_RIG_TOKEN:?}" "$BASE$PRJ/find/$APP" 2> /dev/null || true)
    [ "$code" = 200 ] && return 0
    sleep 5
  done
  return 1
}

app_has_git() {
  docker exec "$(cid)" sh -c "test -d $CONTAINER_DATA/projects/$APP/.git"
}

# ----------------------------------------------------------------- up / mint

up27() {
  command -v docker > /dev/null || die "docker is not installed"
  command -v "$IGN" > /dev/null || die "$IGN is not on PATH: run cargo install --path crates/ignition-cli --force"
  if [ -n "$(ls -A "$DATA/projects" 2> /dev/null)" ] || [ -n "$(cid)" ]; then
    die "REFUSING: a rig is already up here (projects not empty or a gw container runs); run down27 first"
  fi
  write_rig_files
  seed27
  yaml27 template
  IGNITION_CLI_CONFIG=$CFG "$IGN" rig --rig git27 up --with-module "git@$MODULE_VER" --timeout 900 --json > "$STATE/rigup.json" 2>&1 \
    || { echo "up27: rig up failed; see $STATE/rigup.json" >&2; jq -c '.error // .' "$STATE/rigup.json" 2> /dev/null | cut -c1-400 >&2; exit 1; }
  echo "up27: the rig is up on $BASE (module git@$MODULE_VER)"
}

# mint27: the rig API token into $STATE/tokens.env (0600). The key never goes
# to stdout. mint-token.sh restarts the gateway.
mint27() {
  local line key
  [ -n "$(cid)" ] || die "no running gw container in compose project $PROJ: run up27 first"
  IGN_RIG_PROJECT=$PROJ IGN_DEPLOY_RIG_STATE=$STATE/mint bash "$HERE/mint-token.sh" cap-a > /dev/null
  line=$(grep '^export IGN_TOKEN_CAP_A=' "$STATE/mint/tokens.env") || die "mint27: no token line was written"
  key=${line#export IGN_TOKEN_CAP_A=}
  [ -n "$key" ] || die "mint27: the token is empty"
  (
    umask 077
    {
      printf 'IGN_GIT27_RIG_TOKEN=%s\n' "$key"
      printf 'IGN_GIT27_RIG_URL=http://127.0.0.1:%s\n' "$PORT"
    } > "$STATE/tokens.env.new"
  )
  chmod 600 "$STATE/tokens.env.new"
  mv "$STATE/tokens.env.new" "$STATE/tokens.env"
  echo "mint27: token written to $STATE/tokens.env"
}

restart27() {
  token_env
  docker restart "$(cid)" > /dev/null
  wait_running 600 || die "restart27: the gateway is not RUNNING within 600 s"
  wait_app 600 || die "restart27: $APP did not answer find within 600 s"
  # The module's start sync settles after the project answers.
  sleep 45
  app_has_git || die "restart27: $APP has no .git directory (the module did not clone it)"
  echo "restart27: RUNNING, $APP cloned by the module"
}

# cfg27: the ign profile for the rig (token read from the environment).
cfg27() {
  token_env
  if ! grep -q '^\[profiles.git27\]' "$CFG" 2> /dev/null; then
    IGNITION_CLI_CONFIG=$CFG "$IGN" profile add git27 "$BASE" --token-env IGN_GIT27_RIG_TOKEN --active > /dev/null
  fi
  # The scriptExec route (idempotent: the stored secret is reused).
  IGNITION_CLI_CONFIG=$CFG "$IGN" --profile git27 --yes webdev deploy --with-script-exec > /dev/null 2>&1
  # A route imported a moment ago answers 500 until the webdev project loads.
  local end=$(( $(date +%s) + 180 ))
  until IGNITION_CLI_CONFIG=$CFG "$IGN" --profile git27 script run --code 'print 1' > /dev/null 2>&1; do
    [ "$(date +%s)" -lt "$end" ] || die "cfg27: the scriptExec route does not answer within 180 s"
    sleep 5
  done
}

script27() {
  local file=${1:?FILE}
  [ -f "$file" ] || die "script27: no such file: $file"
  cfg27
  IGNITION_CLI_CONFIG=$CFG "$IGN" --profile git27 --yes script run --file "$file"
}

# status27: JGit's view of CapApp. Paths, types and counts only.
status27() {
  local py=$TMPD/status27.py
  cat > "$py" <<'PY'
import system
changes = system.git.getUncommittedChanges("CapApp", "admin")
rows = sorted(["%s %s" % (c.getType(), c.getResource()) for c in changes])
print "uncommitted=%d" % len(rows)
for r in rows:
    print "  " + r
print "branch=%s" % system.git.getCurrentBranch("CapApp")
bs = system.git.getBranchStatus("CapApp", "admin")
print "branch_status=%s" % bs
PY
  script27 "$py"
}

# ------------------------------------------------------------------- reset27

reset27() {
  token_env
  [ -n "$(cid)" ] || die "reset27: no running gw container: run up27 first"
  seed27
  yaml27 template
  curl -sS -m 60 -o /dev/null -X DELETE -H "X-Ignition-API-Token: $IGN_GIT27_RIG_TOKEN" "$BASE$PRJ/$APP?confirm=true" || true
  # The module cloned into the directory: remove what the API delete left.
  docker exec -u root "$(cid)" sh -c "rm -rf $CONTAINER_DATA/projects/$APP"
  docker restart "$(cid)" > /dev/null
  wait_running 600 || die "reset27: the gateway is not RUNNING within 600 s"
  wait_app 600 || die "reset27: $APP did not answer find within 600 s"
  sleep 45
  app_has_git || die "reset27: $APP has no .git directory (the module did not clone it)"
  echo "reset27: $APP re-cloned by the module"
}

# -------------------------------------------------------------------- live27

# live27 LOG TEST...: the live gates, evidence teed to evidence/LOG.log. Fails
# when cargo fails or when a named test's LIVE-RAN text is missing from the log
# (matched unanchored: libtest with --nocapture may print it after
# `test <name> ... ` on the same line).
live27() {
  local name=${1:?LOG-NAME} log rc=0 t missing=0
  shift
  [ $# -gt 0 ] || die "live27: name at least one test"
  [[ $name =~ ^[A-Za-z0-9._-]+$ ]] || die "live27: LOG-NAME is letters, digits, dot, dash, underscore"
  token_env
  mkdir -p "$STATE/evidence"
  log=$STATE/evidence/$name.log
  export IGNITION_LIVE_GIT=1
  export IGN_GIT27_RIG_DATA=$DATA
  export IGN_LIVE_EVIDENCE_DIR=$STATE/evidence
  export IGN_GIT27_RIG_CLI_CONFIG=$CFG
  [ -z "${IGN_PROD_SHAPE_FILE:-}" ] || export IGN_PROD_SHAPE_FILE
  [ -z "${PRODSHAPE_GIT_OUT:-}" ] || export PRODSHAPE_GIT_OUT
  (
    cd "$REPO"
    cargo test -p ignition-core --test live_deploy_project_git -- --ignored --nocapture --test-threads=1 "$@" 2>&1 | tee "$log"
  ) || rc=$?
  for t in "$@"; do
    if ! grep -qF "LIVE-RAN $t" "$log"; then
      echo "live27: the log has no 'LIVE-RAN $t'" >&2
      missing=1
    fi
  done
  [ "$rc" -eq 0 ] || { echo "live27: cargo test exited $rc" >&2; exit "$rc"; }
  [ "$missing" -eq 0 ] || exit 1
  echo "live27: every named gate printed its LIVE-RAN line; log $log"
}

# --------------------------------------------------------------------- g2real

# g2real TAG: counts only, never a name. G2_REAL_REPO is a local clone outside
# this repository; the release tag is checked out into a scratch directory.
g2real() {
  local tag=${1:?TAG} src real work total in_scope
  src=${G2_REAL_REPO:-}
  [ -n "$src" ] || die "g2real: set G2_REAL_REPO to a local clone"
  [ -d "$src" ] || die "g2real: G2_REAL_REPO is not a directory"
  real=$(cd "$src" && pwd -P)
  case "$real/" in
    "$REPO_REAL"/*) die "REFUSING: G2_REAL_REPO is inside this repository" ;;
  esac
  work=$TMPD/g2real
  mkdir -p "$work"
  seedgit -C "$real" archive --format=tar "$tag" | tar -x -C "$work"
  total=$(find "$work" -type f | wc -l | tr -d ' ')
  in_scope=$(find "$work" -type f \( -path "$work/project.json" -o -path "$work/ignition/*" -o -path "$work/com.inductiveautomation.*" \) | wc -l | tr -d ' ')
  printf 'g2real: tracked=%s in_scope=%s out_of_scope=%s\n' "$total" "$in_scope" "$(( total - in_scope ))"
}

down27() {
  if [ -f "$STATE/compose.yml" ]; then
    dc down -v --remove-orphans
  fi
  # The gateway wrote some of its data as another user; let a container remove it.
  if [ -d "$DATA" ]; then
    docker run --rm -v "$DATA:/d" alpine:3 sh -c 'rm -rf /d/* /d/.[!.]* 2>/dev/null; true' > /dev/null 2>&1 || true
  fi
  /bin/rm -rf "$STATE/data" "$STATE/repo" "$STATE/git-server" "$STATE/compose.yml" "$STATE/compose.ign-modules.yml" \
    "$STATE/mint" "$STATE/tokens.env" "$STATE/ign-config.toml" "$STATE/rigup.json" "$STATE/seed-work"
  echo "down27: removed compose project $PROJ and its state (the evidence directory is kept)"
}

# ------------------------------------------------------------------- dispatch

case "${1:-}" in
  up27) up27 ;;
  seed27) mkdir -p "$STATE/repo"; seed27 ;;
  reset27) reset27 ;;
  yaml27) yaml27 "${2:-}" ;;
  mint27) mint27 ;;
  restart27) restart27 ;;
  script27) script27 "${2:-}" ;;
  status27) status27 ;;
  live27) shift; live27 "$@" ;;
  g2real) g2real "${2:-}" ;;
  down27) down27 ;;
  *) echo "usage: capture27.sh up27|seed27|reset27|yaml27 MODE|mint27|restart27|script27 FILE|status27|live27 LOG TEST...|g2real TAG|down27" >&2; exit 64 ;;
esac
