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
#                    module at v1.0.0, then publish27; every live gate is
#                    re-runnable after it
#   publish27        fast-forward the remote's production to v1.1.0 (the
#                    release reaches production before a deploy)
#   yaml27 MODE      git.yaml variant, MODE template | production
#   mint27           mint the rig API token into tokens.env (0600), never stdout
#   restart27        restart the gateway and wait until CapApp is cloned
#   script27 FILE    run a Jython file on the gateway (`ign script run`)
#   status27         JGit view of CapApp: uncommitted paths and branch counts
#   live27 LOG TEST... run the #[ignore] live gates with IGNITION_LIVE_GIT=1,
#                    tee everything to evidence/LOG.log, fail unless every named
#                    test ended with its own `LIVE-RAN <name>` line (a gate prints
#                    it as its last statement, so the line means the gate passed)
#   g2real TAG       static counts of a real repository at a release tag
#                    (G2_REAL_REPO, a local clone outside this repository)
#   g2norm OUT-DIR   (27-25, D-20) the gateway-form normalization of a real
#                    project repository, produced on this local rig: the
#                    release tag's artifact is imported as a throwaway project,
#                    read back, and the files the gateway rewrote are listed
#                    with a patch that replaces each with the gateway-form
#                    bytes. OUT-DIR must be outside this repository (the
#                    private repository); nothing is committed anywhere and
#                    only counts are printed. G2_REAL_REPO is a local clone
#                    (never fetched here); G2_NORM_TAG picks the release tag
#                    (default: the newest v* tag, else HEAD)
#   files27          write the rig compose file and the git server files only
#                    (the pair driver reuses the git server build context)
#   trigger27        remove CapApp's .git, then run the module's own
#                    setupLocalRepo on it (the Designer "set up repo" RPC)
#   images27 ensure|list   gateway images g3a.svg and g3b.svg; ensure creates
#                    the missing ones, list prints the user images by name
#   shape27 LABEL    the shape of CapApp's .git (names, counts, no URL/secret)
#   g3               (27-04) per git.yaml flag, what a gateway start does to a
#                    project whose .git ign kept; needs the advanced state
#                    (reset27, then live27 <log> git_advance_status)
#   g6               (27-04) the .git shapes both git.yaml variants leave, the
#                    setupLocalRepo partial state, enforceBranch
#   g9why SETTING   (27-04) why a second overwrite import did not delete the
#                    advanced .git: SETTING restart|scan|status|imports-on; needs
#                    the advanced state and ends with .git deleted or not
#   g7               (27-04) reset27, then the git_pull_push_roundtrip gate
#   seedprod27       (27-24) reset27 with the rig remote seeded from the 27-06
#                    production-shape generator: PRODSHAPE_GIT_OUT holds the
#                    emitted remote (emitted first through the deploy_prod_scale
#                    git_prodshape_emit test when absent) and IGN_PROD_SHAPE_FILE
#                    the private shape file; production sits at the gateway's
#                    head, the release tag v1.1.0 one advance ahead, and the
#                    emitted gateway project (its padded .git as generated) is
#                    put in place of a module clone (place_prodshape27). No
#                    shape value is printed.
#   seedtags27       (27-24) push release v1.3.0 to the remote's production: its
#                    deploy/project.yaml owns the tag provider CapTags and
#                    tags/CapTags/ holds two neutral tags (Phase 26 layout)
#   nogit27          (27-24) the production incident shape: the gateway stopped,
#                    CapApp's .git removed with it stopped, the gateway started
#   designer-evidence before|after   (27-24) the D-15 evidence: the .git
#                    manifest digest, the project export canonical digest and
#                    the CapTags export digest with its tag count; `before` is
#                    taken right after a green deploy, `after` after a human
#                    Designer open and prints designer-evidence: unchanged or
#                    the differing items
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
# token are never printed and never on a command line (curl reads the token
# header from a config on stdin). Never enable `set -x` here. Synthetic names only.
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

# publish27: the release lands on the remote's production branch (a fast
# forward from v1.0.0 to v1.1.0), as it does before a real deploy. The module's
# clone stays at v1.0.0 until a deploy advances it. Without this the start
# sync's fetch resets the remote-tracking ref to the remote tip and the
# advanced repo reads as one commit ahead (observed in the first 27-03 run).
publish27() {
  local bare=$STATE/repo/$BARE rel old
  rel=$(seedgit -C "$bare" rev-parse 'v1.1.0^{commit}')
  old=$(seedgit -C "$bare" rev-parse production)
  seedgit -C "$bare" merge-base --is-ancestor "$old" "$rel" || die "publish27: v1.1.0 does not descend from production"
  seedgit -C "$bare" update-ref refs/heads/production "$rel" "$old"
  chmod -R a+rX "$bare"
  echo "publish27: production on the remote is at v1.1.0; the module's clone is not touched"
}

# seed_prodshape27 (27-24): the rig remote built from the 27-06 production-shape
# generator's output instead of the synthetic history. PRODSHAPE_GIT_OUT (a
# directory outside this repository) holds the emitted `capapp.git` and the
# gateway data directory; when it holds no emitted remote yet, the generator's
# own emit test runs first with both variables passed through (the private shape
# file is read there; no value of it is printed or recorded). The remote's
# `production` is put back to the commit the generated gateway sits at, so the
# module clones that and the release tag v1.1.0 stays `advance_commits` (plus the
# one descriptor commit below) ahead on no branch, exactly as seed27 leaves
# v1.1.0 (publish27 then moves production).
seed_prodshape27() {
  local out=${PRODSHAPE_GIT_OUT:-} bare=$STATE/repo/$BARE gh rel
  [ -n "$out" ] || die "seedprod27: PRODSHAPE_GIT_OUT is not set"
  [ -n "${IGN_PROD_SHAPE_FILE:-}" ] || die "seedprod27: IGN_PROD_SHAPE_FILE is not set"
  case "$out" in
    /*) ;;
    *) die "seedprod27: PRODSHAPE_GIT_OUT is not an absolute path" ;;
  esac
  mkdir -p "$out"
  out=$(cd "$out" && pwd -P)
  case "$out/" in
    "$REPO_REAL"/*) echo "REFUSING: PRODSHAPE_GIT_OUT $out is inside the repository" >&2; exit 2 ;;
  esac
  if [ ! -d "$out/capapp.git" ] || [ ! -f "$out/data/projects/$APP/.git/refs/heads/production" ]; then
    mkdir -p "$STATE/evidence"
    (
      cd "$REPO"
      PRODSHAPE_GIT_OUT=$out IGN_PROD_SHAPE_FILE=$IGN_PROD_SHAPE_FILE \
        cargo test -p ignition-core --test deploy_prod_scale git_prodshape_emit -- --nocapture
    ) > "$STATE/evidence/seedprod27-emit.log" 2>&1 \
      || die "seedprod27: the generator's emit failed; see $STATE/evidence/seedprod27-emit.log"
  fi
  [ -d "$out/capapp.git" ] || die "seedprod27: no capapp.git below PRODSHAPE_GIT_OUT"
  # The generated gateway's head, read from its loose ref file (a fast-import
  # ref); git is not run on that tree.
  [ -f "$out/data/projects/$APP/.git/refs/heads/production" ] || die "seedprod27: the generated gateway has no loose production ref"
  gh=$(tr -d '[:space:]' < "$out/data/projects/$APP/.git/refs/heads/production")
  [[ $gh =~ ^[0-9a-f]{40}$ ]] || die "seedprod27: the generated gateway head is not a commit id"
  /bin/rm -rf "$STATE/seed-work" "$bare"
  mkdir -p "$STATE/repo"
  cp -R "$out/capapp.git" "$bare"
  rel=$(seedgit -C "$bare" rev-parse 'v1.1.0^{commit}') || die "seedprod27: the emitted remote has no tag v1.1.0"
  seedgit -C "$bare" cat-file -e "$gh^{commit}" || die "seedprod27: the gateway head is not in the emitted remote"
  seedgit -C "$bare" merge-base --is-ancestor "$gh" "$rel" || die "seedprod27: v1.1.0 does not descend from the gateway head"
  seedgit -C "$bare" update-ref refs/heads/production "$gh"
  seedgit -C "$bare" config http.receivepack true
  [ -z "$(seedgit -C "$bare" branch --list master)" ] || die "seedprod27: the remote has a master branch"
  # The generator emits project files only. A release names its gateway project
  # and environments in deploy/project.yaml, so one commit on top of the
  # generator's release adds that one neutral file and the tag v1.1.0 moves to it
  # (the only change to the generated history: one commit, one file).
  # live_deploy_project_git's git_prodscale_rig keys on this commit's subject.
  local blob tree commit idx=$TMPD/prodshape.index
  blob=$(printf 'schema_version: 1\nproject: %s\nenvironments: [rig]\n' "$APP" | seedgit -C "$bare" hash-object -w --stdin)
  GIT_INDEX_FILE=$idx seedgit -C "$bare" read-tree "$rel"
  GIT_INDEX_FILE=$idx seedgit -C "$bare" update-index --add --cacheinfo "100644,$blob,deploy/project.yaml"
  tree=$(GIT_INDEX_FILE=$idx seedgit -C "$bare" write-tree)
  commit=$(GIT_AUTHOR_NAME=rig GIT_AUTHOR_EMAIL=rig@example.invalid GIT_AUTHOR_DATE=2026-01-04T00:00:00Z \
    GIT_COMMITTER_NAME=rig GIT_COMMITTER_EMAIL=rig@example.invalid GIT_COMMITTER_DATE=2026-01-04T00:00:00Z \
    seedgit -C "$bare" -c commit.gpgsign=false commit-tree "$tree" -p "$rel" -m 'release deploy descriptor')
  seedgit -C "$bare" update-ref refs/tags/v1.1.0 "$commit"
  /bin/rm -f "$idx"
  chmod -R a+rX "$bare"
  PRODSHAPE_OUT_REAL=$out
  echo "seed_prodshape27: $BARE seeded from the generator output (production at the gateway head, tag v1.1.0 ahead); no shape value is shown"
}

# place_prodshape27 (CodeRabbit #58 follow-up X3): the generator pads the
# gateway's .git (one pack per history run, unreferenced loose objects, loose
# tag refs) after it made the remote, so the module's clone of the remote holds
# one pack and none of the padding: it never has the configured shape. Like the
# VM gates' `upload`, the rig therefore gets the emitted gateway project itself:
# the deploy_prod_scale git_prodshape_stage test copies it (in-scope files and
# .git as the generator left them) into a stage below the state directory with
# the remote URL set to the module's repo_uri; the gateway is stopped, the stage
# is put in place as the gateway's user (a throwaway root container of the
# gateway image) and the gateway is started. The module's start sync then finds
# an existing repository, as on a production gateway. git_prodscale_rig
# compares the live .git with the configured shape before it applies.
place_prodshape27() {
  local c img own stage=$STATE/prodshape-stage
  [ -n "${PRODSHAPE_OUT_REAL:-}" ] || die "place_prodshape27: run seed_prodshape27 first"
  c=$(cid)
  [ -n "$c" ] || die "place_prodshape27: no running gw container"
  img=$(docker inspect -f '{{.Config.Image}}' "$c")
  [ -n "$img" ] || die "place_prodshape27: the gw container has no image"
  own=$(docker exec "$c" sh -c 'echo "$(id -u):$(id -g)"')
  [[ $own =~ ^[0-9]+:[0-9]+$ ]] || die "place_prodshape27: the gateway user's ids are not numeric"
  /bin/rm -rf "$stage"
  mkdir -p "$stage" "$STATE/evidence"
  (
    cd "$REPO"
    PRODSHAPE_GIT_OUT=$PRODSHAPE_OUT_REAL PRODSHAPE_STAGE_TO=$stage/$APP \
      cargo test -p ignition-core --test deploy_prod_scale git_prodshape_stage -- --nocapture
  ) > "$STATE/evidence/seedprod27-stage.log" 2>&1 \
    || die "place_prodshape27: staging the emitted project failed; see $STATE/evidence/seedprod27-stage.log"
  [ -f "$stage/$APP/.git/refs/heads/production" ] && [ -f "$stage/$APP/project.json" ] \
    || die "place_prodshape27: the stage holds no project with a .git"
  docker stop "$c" > /dev/null
  # CodeRabbit #60 (Y2): from here the gateway is stopped. A failed copy (a
  # full disk, say) starts it again before the error is returned, with the
  # copy's own message and exit status, so seedprod27 can be rerun (reset27
  # needs a running gateway and replaces the partial copy).
  local rc=0
  docker run --rm --platform linux/amd64 --entrypoint sh -u root \
    -v "$DATA/projects:/p" -v "$stage/$APP:/src:ro" "$img" \
    -c "rm -rf /p/$APP && mkdir /p/$APP && cp -a /src/. /p/$APP/ && chown -R $own /p/$APP" > /dev/null || rc=$?
  if [ "$rc" -eq 0 ] && [ ! -f "$PDIR/.git/refs/heads/production" ]; then
    echo "place_prodshape27: $APP has no .git on the host after the copy" >&2
    rc=64
  fi
  if [ "$rc" -ne 0 ]; then
    docker start "$c" > /dev/null || echo "place_prodshape27: starting the gateway again failed as well" >&2
    echo "place_prodshape27: placing the emitted project failed (exit $rc); the gateway was started again, so seedprod27 can be rerun once the cause is fixed" >&2
    exit "$rc"
  fi
  /bin/rm -rf "$stage"
  docker start "$c" > /dev/null
  echo "place_prodshape27: the emitted gateway project is in place as $APP (its .git as generated, remote at the module's URL); no shape value is shown"
}

# seedtags27 (27-24, checker W12): release v1.3.0 on the remote's production, a
# child of the current tip. Its deploy/project.yaml lists the owned tag
# provider CapTags, deploy/tag-providers/CapTags.json holds its settings (the
# STANDARD provider an empty settings object creates on 8.3.6) and tags/CapTags/
# holds two neutral memory tags in the git module's layout (Phase 26).
seedtags27() {
  local bare=$STATE/repo/$BARE work=$STATE/seed-work-tags when=2026-01-05T00:00:00Z
  [ -d "$bare" ] || die "seedtags27: no $BARE: run reset27 first"
  [ -z "$(seedgit -C "$bare" tag --list v1.3.0)" ] || die "seedtags27: v1.3.0 exists already: run reset27 first"
  /bin/rm -rf "$work"
  seedgit clone -q --no-hardlinks -b production "$bare" "$work"
  mkdir -p "$work/deploy/tag-providers" "$work/tags/CapTags"
  printf 'schema_version: 1\nproject: %s\nenvironments: [rig]\ntag_providers: [CapTags]\n' "$APP" > "$work/deploy/project.yaml"
  printf '{"schema_version":1,"enabled":true,"config":{"profile":{"type":"STANDARD","allowBackfill":false,"enableTagReferenceStore":false},"settings":{"defaultDatasourceName":null,"readPermissions":null,"readOnly":false,"writePermissions":null,"editPermissions":null,"valuePersistence":null}}}\n' \
    > "$work/deploy/tag-providers/CapTags.json"
  printf '{"dataType":"Int4","tagType":"AtomicTag","value":5,"valueSource":"memory"}\n' > "$work/tags/CapTags/Setpoint.json"
  printf '{"dataType":"Float8","documentation":"level","tagType":"AtomicTag","value":1.5,"valueSource":"memory"}\n' > "$work/tags/CapTags/Level.json"
  seedgit -C "$work" add -A
  seed_commit "$work" 'release 1.3.0' "$when"
  seed_tag "$work" v1.3.0 "$when"
  seedgit -C "$work" push -q "$bare" HEAD:refs/heads/production refs/tags/v1.3.0
  chmod -R a+rX "$bare"
  /bin/rm -rf "$work"
  echo "seedtags27: production on the remote is at v1.3.0, which owns the tag provider CapTags (two tags)"
}

# ---------------------------------------------------------------- git.yaml

# yaml27 MODE: write $DATA/git.yaml IN PLACE (the inode stays). The credential
# key holds a fixed dummy the module requires; the file is never printed.
#   template    repo_branch production, production_mode false
#   production  production_mode true, production_branch production,
#               production_tagPattern "v*"
# GIT27_IMPORTS=true turns the three commissioning imports on (default false).
# GIT27_FLAG=importImages|importTags|importThemes|tags_importOnStartup turns on
# exactly that one flag (G3). GIT27_ENFORCE=true sets commissioning_enforceBranch.
yaml27() {
  local mode=${1:?MODE (template or production)} imports=${GIT27_IMPORTS:-false} out
  local flag=${GIT27_FLAG:-} enforce=${GIT27_ENFORCE:-false}
  local f_themes=$imports f_tags=$imports f_images=$imports f_startup=false
  case "$mode" in template | production) ;; *) die "yaml27: MODE is template or production" ;; esac
  case "$imports" in true | false) ;; *) die "yaml27: GIT27_IMPORTS is true or false" ;; esac
  case "$enforce" in true | false) ;; *) die "yaml27: GIT27_ENFORCE is true or false" ;; esac
  case "$flag" in
    "") ;;
    importImages) f_images=true ;;
    importTags) f_tags=true ;;
    importThemes) f_themes=true ;;
    tags_importOnStartup) f_startup=true ;;
    *) die "yaml27: GIT27_FLAG is importImages, importTags, importThemes or tags_importOnStartup" ;;
  esac
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
    printf '  commissioning_importThemes: %s\n' "$f_themes"
    printf '  commissioning_importTags: %s\n' "$f_tags"
    printf '  commissioning_importImages: %s\n' "$f_images"
    printf '  commissioning_enforceBranch: %s\n' "$enforce"
    printf '  tags_importOnStartup: %s\n' "$f_startup"
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
  echo "yaml27: git.yaml written ($mode, flag=${flag:-none}, imports=$imports, enforce=$enforce; the credential key is not shown)"
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
    code=$(curl_tok IGN_GIT27_RIG_TOKEN -sS -m 10 -o /dev/null -w '%{http_code}' "$BASE$PRJ/find/$APP" 2> /dev/null || true)
    [ "$code" = 200 ] && return 0
    sleep 5
  done
  return 1
}

app_has_git() {
  docker exec "$(cid)" sh -c "test -d $CONTAINER_DATA/projects/$APP/.git"
}

# git_server_ok: the git server answers the smart-HTTP advertisement (200). The
# Apache cgid socket inside the server container can disappear after hours up
# (observed in 27-23: every request answered 503 and the module's clone left an
# empty repository behind), so a reset restarts the server and checks this.
git_server_ok() {
  local code
  code=$(docker exec "$(cid)" sh -c "curl -s -o /dev/null -w '%{http_code}' 'http://git-server/git/$BARE/info/refs?service=git-upload-pack'" 2> /dev/null || true)
  [ "$code" = 200 ]
}

# wait_clone SECONDS: the module's clone of the remote finished: the tracked
# branch's ref and the index exist. `.git` exists as soon as the clone starts
# (an empty repository with an unborn HEAD), so its presence proves nothing.
wait_clone() {
  local limit=${1:-300} end
  end=$(( $(date +%s) + limit ))
  until docker exec "$(cid)" sh -c "test -f $CONTAINER_DATA/projects/$APP/.git/index && test -f $CONTAINER_DATA/projects/$APP/.git/refs/heads/production" 2> /dev/null; do
    [ "$(date +%s)" -lt "$end" ] || return 1
    sleep 3
  done
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
  case "${GIT27_SEED:-synthetic}" in
    synthetic) seed27 ;;
    prodshape) seed_prodshape27 ;;
    *) die "reset27: GIT27_SEED is synthetic or prodshape" ;;
  esac
  yaml27 "${GIT27_YAML_MODE:-template}"
  # A save a failed gate left behind would make the next save refuse.
  /bin/rm -rf "$DATA/ign-deploy"
  curl_tok IGN_GIT27_RIG_TOKEN -sS -m 60 -o /dev/null -X DELETE "$BASE$PRJ/$APP?confirm=true" || true
  # The module cloned into the directory: remove what the API delete left.
  docker exec -u root "$(cid)" sh -c "rm -rf $CONTAINER_DATA/projects/$APP"
  # The server container's CGI socket can vanish after hours up: restart it
  # and prove the advertisement answers before the module clones.
  dc restart git-server > /dev/null 2>&1 || true
  local tries=0
  until git_server_ok; do
    tries=$(( tries + 1 ))
    [ "$tries" -lt 20 ] || die "reset27: the git server does not answer the smart-HTTP advertisement"
    sleep 3
  done
  if [ "${GIT27_SEED:-synthetic}" = prodshape ]; then
    place_prodshape27
  else
    docker restart "$(cid)" > /dev/null
  fi
  wait_running 600 || die "reset27: the gateway is not RUNNING within 600 s"
  wait_app 600 || die "reset27: $APP did not answer find within 600 s"
  wait_clone 600 || die "reset27: the module's clone of $APP did not finish within 600 s (no refs/heads/production and index)"
  sleep 15
  app_has_git || die "reset27: $APP has no .git directory (the module did not clone it)"
  # The clone is at v1.0.0; now the release reaches the remote's production.
  publish27
  echo "reset27: $APP re-cloned by the module at v1.0.0, the remote's production at v1.1.0"
}

# -------------------------------------------------------------------- live27

# live27 LOG TEST...: the live gates, evidence teed to evidence/LOG.log. Fails
# when cargo fails or when a named test's `LIVE-RAN <name>` is not at the end of
# a log line (anchored: libtest with --nocapture may print `test <name> ... `
# before it on the same line, so it may follow whitespace). A gate prints that
# line as its last statement, after every assertion: it means the gate passed.
# A test name is `[a-z0-9_]+` and matches whole (WR-D2).
live27() {
  local name=${1:?LOG-NAME} log rc=0 t missing=0
  shift
  [ $# -gt 0 ] || die "live27: name at least one test"
  [[ $name =~ ^[A-Za-z0-9._-]+$ ]] || die "live27: LOG-NAME is letters, digits, dot, dash, underscore"
  for t in "$@"; do
    [[ $t =~ ^[a-z0-9_]+$ ]] || die "live27: a test name is lowercase letters, digits and underscore"
  done
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
    if ! grep -qE "(^|[[:space:]])LIVE-RAN ${t}\$" "$log"; then
      echo "live27: the log has no 'LIVE-RAN $t'" >&2
      missing=1
    fi
  done
  [ "$rc" -eq 0 ] || { echo "live27: cargo test exited $rc" >&2; exit "$rc"; }
  [ "$missing" -eq 0 ] || exit 1
  echo "live27: every named gate printed its LIVE-RAN line; log $log"
}

# designer-evidence before|after (27-24, D-15): the gate git_designer_evidence
# records, or compares with, the .git manifest digest (the definition of
# GitManifest), the project export canonical digest and the CapTags export
# digest with its tag count. `before` runs right after a green deploy and fails
# when CapTags is absent or holds fewer than two tags; `after` runs once a human
# has opened the project in the Designer and prints `designer-evidence:
# unchanged` or the differing items. Digests, counts and file names only.
designer_evidence() {
  local mode=${1:-} log rc=0
  case "$mode" in before | after) ;; *) die "designer-evidence: before or after" ;; esac
  export IGN_DESIGNER_EVIDENCE=$mode
  # live27 ends with `exit` on a failure: run it in a subshell to keep the status.
  ( live27 "27-24-designer-$mode" git_designer_evidence ) > /dev/null 2>&1 || rc=$?
  log=$STATE/evidence/27-24-designer-$mode.log
  grep -E '^designer-evidence' "$log" || true
  [ "$rc" -eq 0 ] || { echo "designer-evidence $mode: the gate failed (exit $rc); see $log" >&2; exit "$rc"; }
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

# g2norm OUT-DIR: see the header. The gate git_g2norm does the work; this
# function guards the paths, picks the tag and reports counts.
g2norm() {
  local out=${1:?OUT-DIR} src real out_real tag
  src=${G2_REAL_REPO:-}
  [ -n "$src" ] || die "g2norm: set G2_REAL_REPO to a local clone"
  [ -d "$src" ] || die "g2norm: G2_REAL_REPO is not a directory"
  real=$(cd "$src" && pwd -P)
  case "$real/" in
    "$REPO_REAL"/*) die "REFUSING: G2_REAL_REPO is inside this repository" ;;
  esac
  case "$out" in
    /*) ;;
    *) die "REFUSING: the output directory is not an absolute path" ;;
  esac
  mkdir -p "$out"
  out_real=$(cd "$out" && pwd -P)
  case "$out_real/" in
    "$REPO_REAL"/*) die "REFUSING: the output directory is inside this repository" ;;
  esac
  chmod 700 "$out_real"
  tag=${G2_NORM_TAG:-}
  if [ -z "$tag" ]; then
    tag=$(seedgit -C "$real" tag --list 'v*' --sort=-v:refname | head -n 1)
  fi
  [ -n "$tag" ] || tag=HEAD
  ( export G2_REAL_REPO=$real G2_NORM_OUT=$out_real G2_NORM_TAG=$tag; live27 27-25-g2norm git_g2norm ) > /dev/null 2>&1 || {
    echo "g2norm: the gate failed; see $STATE/evidence/27-25-g2norm.log (counts only)" >&2
    exit 1
  }
  grep -o 'LIVE-G2NORM.*' "$STATE/evidence/27-25-g2norm.log" || true
  printf 'g2norm: drift-files %s lines, patch %s bytes (%s)\n' \
    "$(wc -l < "$out_real/drift-files.txt" | tr -d ' ')" "$(wc -c < "$out_real/normalize.patch" | tr -d ' ')" "private output directory"
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

# ----------------------------------------------------------- 27-04 captures

PDIR=$DATA/projects/$APP
IMG_ROOT=ignition/images/

# ignp27 ARGS...: the installed ign with the rig profile (token from tokens.env).
ignp27() {
  cfg27
  IGNITION_CLI_CONFIG=$CFG "$IGN" --profile git27 --yes "$@"
}

# root_rm NAME: remove one fixed entry below the project directory, as root
# inside the container (the gateway wrote some of it as another user).
root_rm() {
  case "$1" in .git | tags | images) ;; *) die "root_rm: not an allowed name: $1" ;; esac
  docker exec -u root "$(cid)" sh -c "rm -rf $CONTAINER_DATA/projects/$APP/$1"
}

# trigger27: remove CapApp's .git, then run the module's own setupLocalRepo on
# it (the same implementation the Designer RPC calls). The result is the
# partial state of a project set up from the Designer against this remote.
trigger27() {
  local py=$TMPD/trigger27.py
  token_env
  if app_has_git; then root_rm .git; fi
  cat > "$py" <<'PY'
from com.inductiveautomation.ignition.gateway import IgnitionGateway
ctx = IgnitionGateway.get()
hook = ctx.getModule("com.axone_io.ignition.git")
f = hook.getClass().getDeclaredField("scriptModule")
f.setAccessible(True)
sm = f.get(hook)
sm.setupLocalRepo("CapApp", "admin")
print "TRIGGER setupLocalRepo returned"
PY
  script27 "$py" | grep -E '^TRIGGER |^error' || true
}

# nogit27 (27-24): the production incident shape. The gateway is stopped, the
# project's .git is removed while it is stopped (a throwaway container of the
# gateway image as root; the gateway wrote some of it as another user), and the
# gateway is started again. trigger27 then runs the module's own setupLocalRepo.
nogit27() {
  local c img
  token_env
  c=$(cid)
  [ -n "$c" ] || die "nogit27: no running gw container: run up27 first"
  img=$(docker inspect -f '{{.Config.Image}}' "$c")
  [ -n "$img" ] || die "nogit27: the gw container has no image"
  docker stop "$c" > /dev/null
  docker run --rm --platform linux/amd64 --entrypoint sh -u root -v "$DATA/projects:/p" "$img" \
    -c "rm -rf /p/$APP/.git" > /dev/null
  [ ! -e "$PDIR/.git" ] || die "nogit27: $APP still has a .git on the host"
  docker start "$c" > /dev/null
  wait_running 600 || die "nogit27: the gateway is not RUNNING within 600 s"
  wait_app 600 || die "nogit27: $APP did not answer find within 600 s"
  sleep 45
  echo "nogit27: the gateway ran again after $APP lost its .git while stopped"
}

# images27 ensure|list: the user images of the gateway, by name and count.
images27() {
  local mode=${1:?ensure or list} py=$TMPD/images27.py
  case "$mode" in ensure | list) ;; *) die "images27: ensure or list" ;; esac
  cat > "$py" <<PY
from com.inductiveautomation.ignition.gateway import IgnitionGateway
from com.inductiveautomation.ignition.common.images import ImageFormat
from jarray import array
im = IgnitionGateway.get().getImageManager()
root = "$IMG_ROOT"
def walk(path, out):
    for r in im.getImages(path):
        p = str(r.path())
        rel = p[len(root):] if p.startswith(root) else p
        if r.format() is None:
            if rel != "Builtin":
                walk(rel, out)
        else:
            out.append(rel)
have = []
walk("", have)
if "$mode" == "ensure":
    svg = '<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4"/></svg>'
    for name in ["g3a.svg", "g3b.svg"]:
        if name not in have:
            data = array([ord(c) for c in svg], 'b')
            im.insertImage(name, "", ImageFormat.SVG, "", data, 4, 4, len(data))
    have = []
    walk("", have)
print "IMAGES count=%d names=%s" % (len(have), ",".join(sorted(have)))
PY
  script27 "$py" | grep -E '^IMAGES |^error' || die "images27: no IMAGES line"
}

# tagprobe27 exists|clean: the synthetic tag G3Probe in the provider `default`.
tagprobe27() {
  local mode=${1:?exists or clean} py=$TMPD/tagprobe27.py
  case "$mode" in exists | clean) ;; *) die "tagprobe27: exists or clean" ;; esac
  cat > "$py" <<PY
import system
if "$mode" == "clean":
    system.tag.deleteTags(["[default]G3Probe"])
    print "TAGPROBE cleaned"
else:
    print "TAGPROBE exists=%s" % system.tag.exists("[default]G3Probe")
PY
  script27 "$py" | grep -E '^TAGPROBE |^error' || die "tagprobe27: no TAGPROBE line"
}

# exp27: the read-back export as counts and a digest of (member, content hash).
exp27() {
  local z=$TMPD/exp27.zip list=$TMPD/exp27.list n
  ignp27 project export "$APP" --output "$z" > /dev/null
  : > "$list"
  while IFS= read -r n; do
    printf '%s %s\n' "$n" "$(unzip -p "$z" "$n" | shasum -a 256 | cut -c1-16)" >> "$list"
  done < <(unzip -Z1 "$z" | grep -v '/$' | LC_ALL=C sort)
  printf 'members=%s git_members=%s digest=%s' "$(wc -l < "$list" | tr -d ' ')" \
    "$(grep -c '^\.git/' "$list" || true)" "$(shasum -a 256 < "$list" | cut -c1-12)"
}

# gitmanifest27: .git as a short digest of its small files plus an object count
# (FETCH_HEAD, ORIG_HEAD, locks and logs excluded: sync rewrites them).
gitmanifest27() {
  local g=$PDIR/.git
  if [ ! -d "$g" ]; then printf 'absent'; return 0; fi
  printf 'head=%s small_files=%s objects=%s digest=%s' \
    "$(seedgit -C "$PDIR" rev-parse --short HEAD 2> /dev/null || echo unborn)" \
    "$(cd "$g" && find . -type f ! -name FETCH_HEAD ! -name ORIG_HEAD ! -name '*.lock' ! -path './objects/*' ! -path './logs/*' | wc -l | tr -d ' ')" \
    "$(cd "$g" && find objects -type f | wc -l | tr -d ' ')" \
    "$(cd "$g" && find . -type f ! -name FETCH_HEAD ! -name ORIG_HEAD ! -name '*.lock' ! -path './objects/*' ! -path './logs/*' | LC_ALL=C sort | while IFS= read -r f; do shasum -a 256 "$f"; done | shasum -a 256 | cut -c1-12)"
}

# logs27 SINCE: the module's start-sync log lines since SINCE (UTC, RFC 3339).
logs27() {
  docker logs --since "$1" "$(cid)" 2>&1 \
    | grep -E "Project 'CapApp'|Syncing existing|Fetching from remote|Skipping branch|Switching project|tash|Sync complete|Importing (tags|images|themes|project)|Imported tags|Startup tag import|Pull result|Pulling latest|No tags directory|Tag provider|tag import|has no \.git" \
    | grep -v -E 'field:|password|secret' | cut -c1-230 || true
}

# restart_tol27: restart without requiring a .git afterwards (G3 may delete it).
restart_tol27() {
  token_env
  docker restart "$(cid)" > /dev/null
  wait_running 600 || die "restart_tol27: the gateway is not RUNNING within 600 s"
  wait_app 600 || die "restart_tol27: $APP did not answer find within 600 s"
  # the module's start sync, then the startup tag import, settle after this
  sleep 75
}

# shape27 LABEL: the shape of CapApp's .git. Names, counts and hex only: no URL,
# no value of any key.
shape27() {
  local g=$PDIR/.git label=$1 ix
  echo "SHAPE [$label]"
  if [ ! -d "$g" ]; then echo "  .git: absent"; return 0; fi
  echo "  HEAD: $(head -c 200 "$g/HEAD" | tr -d '\n')"
  echo "  config keys: $(seedgit config --file "$g/config" --list --name-only | LC_ALL=C sort -u | paste -sd' ' -)"
  echo "  remotes: $(seedgit config --file "$g/config" --get-regexp '^remote\..*\.fetch$' | sed -E 's/^remote\.//' | paste -sd'|' -)"
  echo "  branch sections: $(seedgit config --file "$g/config" --get-regexp '^branch\.' | paste -sd'|' -)"
  echo "  url_has_userinfo: $(seedgit config --file "$g/config" --get-regexp '^remote\..*\.url$' | grep -c '@' || true)"
  echo "  loose refs: $(cd "$g" && find refs -type f | LC_ALL=C sort | paste -sd' ' -)"
  if [ -f "$g/packed-refs" ]; then
    echo "  packed-refs: $(grep -vc '^#' "$g/packed-refs") lines: $(grep -v '^[#^]' "$g/packed-refs" | awk '{print $2}' | LC_ALL=C sort | paste -sd' ' -)"
  else
    echo "  packed-refs: none"
  fi
  if [ -f "$g/index" ]; then
    ix=$(od -An -tx1 -N12 "$g/index" | tr -d ' \n')
    echo "  index: signature+version+entries hex=$ix assume_unchanged=$(seedgit -C "$PDIR" ls-files -v 2> /dev/null | grep -c '^h' || true)"
  else
    echo "  index: absent"
  fi
  echo "  objects: loose=$(cd "$g" && find objects -type f ! -path 'objects/pack/*' ! -path 'objects/info/*' | wc -l | tr -d ' ') packs=$(cd "$g" && find objects/pack -name '*.pack' 2> /dev/null | wc -l | tr -d ' ')"
  echo "  stash: $(seedgit -C "$PDIR" stash list 2> /dev/null | wc -l | tr -d ' ') entries"
}

# classify27 LABEL: what GitHostRead::read_git_state and classify make of .git
# (the git_state_probe gate), the LIVE-CLASSIFY lines only. A failed gate fails
# the caller (WR-D3): the status is captured, not piped away.
classify27() {
  local out rc=0
  out=$(live27 "27-04-g6-$1" git_state_probe 2>&1) || rc=$?
  printf '%s\n' "$out" | grep -E '^LIVE-CLASSIFY' || true
  if [ "$rc" -ne 0 ]; then
    printf '%s\n' "$out" | tail -n 15 >&2
    die "classify27 $1: the git_state_probe gate failed (exit $rc)"
  fi
}

# require_advanced27: CapApp's HEAD is v1.1.0 and tags/ and images/ are absent.
require_advanced27() {
  local bare=$STATE/repo/$BARE head rel
  app_has_git || die "CapApp has no .git: run reset27, then live27 <log> git_advance_status"
  head=$(seedgit -C "$PDIR" rev-parse HEAD)
  rel=$(seedgit -C "$bare" rev-parse 'v1.1.0^{commit}')
  [ "$head" = "$rel" ] || die "CapApp's HEAD is not v1.1.0: run reset27, then live27 <log> git_advance_status"
}

g6() {
  local variant ts
  token_env
  for variant in template production; do
    echo "== G6 variant=$variant"
    GIT27_YAML_MODE=$variant reset27 > /dev/null
    shape27 "$variant: the module's clone after a fresh CapApp and a restart"
    classify27 "$variant-clone"
    trigger27
    shape27 "$variant: partial state after setupLocalRepo on a project with no .git"
    classify27 "$variant-partial"
  done
  echo "== G6 enforceBranch=true, the gateway repo on another local branch"
  GIT27_YAML_MODE=template reset27 > /dev/null
  seedgit -C "$PDIR" checkout -q -b g6-other
  echo "before: branch=$(seedgit -C "$PDIR" symbolic-ref --short HEAD) head=$(seedgit -C "$PDIR" rev-parse --short HEAD) uncommitted=$(seedgit -C "$PDIR" status --porcelain | wc -l | tr -d ' ')"
  GIT27_ENFORCE=true yaml27 template
  ts=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  restart_tol27
  echo "after:  branch=$(seedgit -C "$PDIR" symbolic-ref --short HEAD) head=$(seedgit -C "$PDIR" rev-parse --short HEAD) uncommitted=$(seedgit -C "$PDIR" status --porcelain | wc -l | tr -d ' ')"
  shape27 "enforceBranch after the start sync"
  echo "module log lines:"
  logs27 "$ts"
  yaml27 template > /dev/null
}

# g3_run FLAG PRESENCE: one gateway start with exactly one import flag on.
g3_run() {
  local flag=$1 presence=$2 ts ib ia tb ta xb xa gb ga verdict_img verdict_tag verdict_proj verdict_git
  tagprobe27 clean > /dev/null
  images27 ensure > /dev/null
  root_rm tags
  root_rm images
  if [ "$presence" = present ]; then
    mkdir -p "$PDIR/tags" "$PDIR/images"
    printf '{"tags":[{"name":"G3Probe","tagType":"AtomicTag","valueSource":"memory","dataType":"Int4","value":1}]}\n' > "$PDIR/tags/default.json"
    printf '<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4"/></svg>\n' > "$PDIR/images/p1.svg"
  fi
  ib=$(images27 list)
  tb=$(tagprobe27 exists)
  xb=$(exp27)
  gb=$(gitmanifest27)
  GIT27_FLAG=$flag yaml27 template > /dev/null
  ts=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  restart_tol27
  ia=$(images27 list)
  ta=$(tagprobe27 exists)
  xa=$(exp27)
  ga=$(gitmanifest27)
  echo "-- G3 run flag=$flag images_dir=$presence tags_dir=$presence"
  echo "   gateway images before: ${ib#IMAGES }"
  echo "   gateway images after:  ${ia#IMAGES }"
  echo "   tag G3Probe before: ${tb#TAGPROBE }  after: ${ta#TAGPROBE }"
  echo "   project export before: $xb"
  echo "   project export after:  $xa"
  echo "   .git before: $gb"
  echo "   .git after:  $ga"
  echo "   module log lines:"
  logs27 "$ts" | sed 's/^/     /'
  if [ "$ib" = "$ia" ]; then verdict_img="images unchanged (${ia#IMAGES })"; else verdict_img="images ${ib#IMAGES } -> ${ia#IMAGES }"; fi
  if [ "$tb" = "$ta" ]; then verdict_tag="tag ${ta#TAGPROBE }"; else verdict_tag="tag ${tb#TAGPROBE } -> ${ta#TAGPROBE }"; fi
  if [ "$xb" = "$xa" ]; then verdict_proj="project content unchanged"; else verdict_proj="project content changed"; fi
  if [ "$gb" = "$ga" ]; then verdict_git=".git unchanged"; elif [ "$ga" = absent ]; then verdict_git=".git deleted"; else verdict_git=".git changed"; fi
  echo "G3-EFFECT $flag images $([ "$presence" = present ] && echo present || echo absent) tags $([ "$presence" = present ] && echo present || echo absent) -> $verdict_img; $verdict_tag; $verdict_proj; $verdict_git"
}

g3() {
  local presence flag
  token_env
  require_advanced27
  echo "== G3 start state: CapApp at v1.1.0 (ign-advanced), project ign-kept .git, flags off"
  for presence in absent present; do
    for flag in importImages importTags importThemes tags_importOnStartup; do
      g3_run "$flag" "$presence"
      require_advanced27
    done
  done
  yaml27 template > /dev/null
  tagprobe27 clean > /dev/null
  root_rm tags
  root_rm images
}

# g9why SETTING: why the second overwrite import of 27-03 did not delete the
# advanced .git. Starting from the advanced state: restart (commissioning imports
# off, or on with SETTING imports-on), then an optional step between the restart
# and the import (SETTING scan: a project scan; SETTING status: a JGit status
# read through the module; SETTING restart: nothing), then an overwrite import of
# the project's own export, polling for .git to go away.
g9why() {
  local setting=${1:-restart} imports=false ts i gone x code z=$TMPD/g9why.zip
  token_env
  require_advanced27
  case "$setting" in
    restart | scan | status) ;;
    imports-on) imports=true ;;
    *) die "g9why: SETTING is restart, scan, status or imports-on" ;;
  esac
  echo "== g9why setting=$setting commissioning imports=$imports"
  GIT27_IMPORTS=$imports yaml27 template > /dev/null
  ts=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  restart_tol27
  x=$(exp27)
  echo "   after the restart: .git present=$(app_has_git && echo yes || echo no); export $x"
  echo "   module log lines:"
  logs27 "$ts" | grep -v 'Tag provider' | sed 's/^/     /'
  case "$setting" in
    scan)
      code=$(curl_tok IGN_GIT27_RIG_TOKEN -sS -m 60 -o /dev/null -w '%{http_code}' -X POST "$BASE/data/api/v1/scan/projects" || true)
      echo "   project scan triggered: HTTP $code"
      sleep 25
      echo "   after the scan: .git present=$(app_has_git && echo yes || echo no)"
      ;;
    status)
      status27 | grep -E '^uncommitted=|^branch=' | sed 's/^/   status read: /'
      sleep 5
      echo "   after the status read: .git present=$(app_has_git && echo yes || echo no)"
      ;;
  esac
  ignp27 project export "$APP" --output "$z" > /dev/null
  ignp27 project import "$APP" --file "$z" --collision-policy overwrite > /dev/null
  gone=no
  for i in $(seq 1 45); do
    if ! app_has_git; then gone="yes after $((i * 2)) s"; break; fi
    sleep 2
  done
  echo "   second overwrite import: .git deleted=$gone (polled 90 s)"
  echo "   after the import: export $(exp27)"
  yaml27 template > /dev/null
}

g7() {
  reset27
  live27 27-04 git_pull_push_roundtrip
}

# ------------------------------------------------------------------- dispatch

case "${1:-}" in
  up27) up27 ;;
  seed27) mkdir -p "$STATE/repo"; seed27 ;;
  reset27) reset27 ;;
  yaml27) yaml27 "${2:-}" ;;
  publish27) publish27 ;;
  mint27) mint27 ;;
  restart27) restart27 ;;
  script27) script27 "${2:-}" ;;
  status27) status27 ;;
  live27) shift; live27 "$@" ;;
  g2real) g2real "${2:-}" ;;
  g2norm) g2norm "${2:-}" ;;
  files27) write_rig_files ;;
  trigger27) trigger27 ;;
  nogit27) nogit27 ;;
  seedprod27) GIT27_SEED=prodshape reset27 ;;
  seedtags27) seedtags27 ;;
  designer-evidence) designer_evidence "${2:-}" ;;
  images27) images27 "${2:-}" ;;
  shape27) shape27 "${2:-shape}" ;;
  g3) g3 ;;
  g6) g6 ;;
  g7) g7 ;;
  g9why) g9why "${2:-restart}" ;;
  down27) down27 ;;
  *) echo "usage: capture27.sh up27|seed27|reset27|publish27|yaml27 MODE|mint27|restart27|script27 FILE|status27|live27 LOG TEST...|g2real TAG|g2norm OUT-DIR|trigger27|nogit27|seedprod27|seedtags27|designer-evidence before|after|images27 ensure|list|shape27 LABEL|g3|g6|g7|g9why SETTING|down27" >&2; exit 64 ;;
esac
