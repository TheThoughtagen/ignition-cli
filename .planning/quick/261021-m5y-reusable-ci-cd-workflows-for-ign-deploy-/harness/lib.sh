#!/usr/bin/env bash
# Shared helpers for the quick-m5y workflow harness. Source, do not execute.
#
# Region markers: a line containing `>>> <region>` opens a region and a line
# containing `<<< <region>` closes it. The marker may follow `#` or `//`.

HARNESS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$HARNESS_DIR" rev-parse --show-toplevel)"
export PLAN_WF="$REPO_ROOT/.github/workflows/deploy-plan.yml"
export APPLY_WF="$REPO_ROOT/.github/workflows/deploy-apply.yml"
export PROJECTS_WF="$REPO_ROOT/.github/workflows/deploy-projects.yml"
export CHECK_ACTION="$REPO_ROOT/.github/actions/deploy-projects-check/action.yml"
export APPLY_ACTION="$REPO_ROOT/.github/actions/deploy-projects-apply/action.yml"

if [ "${BASH_VERSINFO[0]}" -lt 4 ]; then
  echo "FAIL harness needs bash >= 4 (the workflows target Git Bash and Linux bash)" >&2
  exit 1
fi

pass() { echo "PASS $*"; }
die() {
  echo "FAIL $*" >&2
  exit 1
}

# extract <file> <region>: lines strictly between the markers, de-indented by
# the opening marker's indentation.
extract() {
  awk -v region="$2" '
    function endsWith(s, suffix) {
      return length(s) >= length(suffix) && substr(s, length(s) - length(suffix) + 1) == suffix
    }
    {
      line = $0
      sub(/[ \t\r]+$/, "", line)
    }
    !on && index(line, ">>> ") && endsWith(line, ">>> " region) {
      match($0, /^[ ]*/)
      ind = RLENGTH
      on = 1
      found = 1
      next
    }
    on && index(line, "<<< ") && endsWith(line, "<<< " region) {
      on = 0
      next
    }
    on {
      out = $0
      if (substr(out, 1, ind) == sprintf("%*s", ind, "")) out = substr(out, ind + 1)
      print out
    }
    END { if (!found) exit 3 }
  ' "$1"
}

# extract_first <file> <region>: like extract, but only the first occurrence.
# deploy-projects.yml carries one copy of a shared region per job, so a plain
# extract returns the copies back to back.
extract_first() {
  extract "$1" "$2" | awk -v n="$(extract "$1" "$2" | wc -l | tr -d ' ')" -v c="$(grep -cE ">>> $2\$" "$1")" 'NR <= n / c'
}

# body_checks <file>: every run:/script: key is a `|` block scalar wrapped in a
# `body:` region, and no `${{` appears inside any body region.
body_checks() {
  local f="$1" keys pipes opens closes region
  keys=$(grep -cE '^[[:space:]]*(- )?(run|script):' "$f" || true)
  pipes=$(grep -cE '^[[:space:]]*(- )?(run|script):[[:space:]]*\|[[:space:]]*$' "$f" || true)
  opens=$(grep -cE '(#|//) >>> body:' "$f" || true)
  closes=$(grep -cE '(#|//) <<< body:' "$f" || true)
  [ "$keys" = "$pipes" ] || die "$f: $keys run:/script: keys but only $pipes are '|' block scalars"
  [ "$keys" = "$opens" ] || die "$f: $keys run:/script: keys but $opens '>>> body:' markers"
  [ "$opens" = "$closes" ] || die "$f: $opens '>>> body:' markers but $closes '<<< body:' markers"
  while IFS= read -r region; do
    # shellcheck disable=SC2016
    if extract "$f" "$region" | grep -qF '${{'; then
      die "$f: region $region contains a \${{ }} expression"
    fi
  done < <(grep -oE '>>> body:[A-Za-z0-9-]+' "$f" | sed 's/^>>> //')
}

# fake_runner_env <dir>: create and export RUNNER_TEMP, GITHUB_ENV, GITHUB_OUTPUT.
fake_runner_env() {
  local d="$1"
  mkdir -p "$d/temp"
  : > "$d/github_env"
  : > "$d/github_output"
  export RUNNER_TEMP="$d/temp" GITHUB_ENV="$d/github_env" GITHUB_OUTPUT="$d/github_output"
}

# host_runner_os: the RUNNER_OS value for this host.
host_runner_os() {
  case "$(uname -s)" in
    Darwin) echo macOS ;;
    Linux) echo Linux ;;
    *) echo Windows ;;
  esac
}

# load_env_file <file>: export KEY=VALUE lines the way a later step would see them.
load_env_file() {
  local k v
  while IFS='=' read -r k v; do
    [ -n "$k" ] || continue
    export "$k=$v"
  done < "$1"
}

# build_repo_bin: build the repo's ign (never the installed one on PATH) and
# print its path.
build_repo_bin() {
  local td
  (cd "$REPO_ROOT" && cargo build -p ignition-cli > "${WORK:-/tmp}/cargo-build.log" 2>&1) \
    || { tail -20 "${WORK:-/tmp}/cargo-build.log" >&2; die "cargo build failed"; }
  td="$(cd "$REPO_ROOT" && cargo metadata --format-version 1 --no-deps | jq -r .target_directory)"
  [ -x "$td/debug/ign" ] || die "repo build not found at $td/debug/ign"
  echo "$td/debug/ign"
}

# make_e2e_repo <dir>: a git repo copied from the tracer fixture whose dev
# target is a `local` master over an empty data dir (config/resources/core and
# config/resources/dev). Creates <dir>/repo and <dir>/data.
make_e2e_repo() {
  local d="$1"
  /bin/rm -rf "$d"
  mkdir -p "$d/data/config/resources/core" "$d/data/config/resources/dev"
  cp -R "$REPO_ROOT/crates/ignition-core/tests/fixtures/deploy/repos/tracer" "$d/repo"
  cat > "$d/repo/targets/dev.yaml" << YAML
schema_version: 1
gateway: tracer-gw
mode: dev
nodes:
  master:
    profile: dev
    local:
      data_dir: $d/data
YAML
  printf '.ign/\n' > "$d/repo/.gitignore"
  git -C "$d/repo" init -q
  git -C "$d/repo" add -A
  git -C "$d/repo" -c user.name=harness -c user.email=harness@example.invalid commit -q -m fixture
}

# targets_static_checks <file>: the optional-targets wiring of deploy-plan.yml
# and deploy-apply.yml (25-18). The inputs and the secret exist, the targets
# checkout is conditional, read-only and outside the gateway tree, every
# checkout drops its credentials and the token is used only by the checkout and
# the validation step.
targets_static_checks() {
  local f="$1" name n_checkout n_persist
  name="$(basename "$f")"
  # shellcheck disable=SC2016
  for needle in 'targets-repository:' 'targets-ref:' 'targets-dir:' 'IGN_TARGETS_TOKEN:' \
    'if: inputs.targets-repository != '"''" 'path: .ign-targets' 'IGN_TARGETS_CHECKOUT: .ign-targets' \
    "IGN_TARGETS_REQUIRED: 'false'" '${targets_args[@]+"${targets_args[@]}"}' '--targets'; do
    grep -qF -e "$needle" "$f" || die "$name lacks: $needle"
  done
  [ "$(grep -cE '^      targets-repository:$' "$f")" = 1 ] && [ "$(grep -A3 -E '^      targets-repository:$' "$f" | grep -c "default: ''")" = 1 ] \
    || die "$name: targets-repository is not an input defaulting to empty"
  n_checkout="$(grep -cE 'uses: actions/checkout@v7$' "$f")"
  n_persist="$(grep -cE '^ +persist-credentials: false$' "$f")"
  [ "$n_checkout" = 2 ] && [ "$n_persist" = 2 ] || die "$name: expected 2 checkouts with persist-credentials: false, got $n_checkout and $n_persist"
  # shellcheck disable=SC2016
  [ "$(grep -cF 'token: ${{ secrets.IGN_TARGETS_TOKEN }}' "$f")" = 1 ] || die "$name: the targets checkout does not use IGN_TARGETS_TOKEN once"
  [ "$(grep -cF 'secrets.IGN_TARGETS_TOKEN' "$f")" = 2 ] || die "$name: IGN_TARGETS_TOKEN is used outside the targets step env and checkout"
  if grep -nE "(echo|printf)[^|>]*(IGN_TARGETS_TOKEN|secrets\.)" "$f" | grep -v 'fail "' | grep -q .; then
    die "$name: a step echoes the targets token"
  fi
}

# lint_with_local_actions <workflow-file> <work-dir>: actionlint of a workflow
# that runs local composite actions. actionlint validates the inputs of a local
# action only when its action.yml exists, and it silently skips a `uses: ./path`
# whose directory is missing, so the repo's actions are copied next to the
# workflow, both at .github/actions and at .ign-actions/.github/actions (where
# deploy-projects.yml checks them out). The job context documents
# job.workflow_repository and job.workflow_sha (docs.github.com, Contexts
# reference, "job context"), but the installed actionlint predates those two
# properties, so exactly those two "not defined" messages are ignored.
lint_with_local_actions() {
  local wf="$1" work="$2"
  /bin/rm -rf "$work"
  mkdir -p "$work/.github/workflows" "$work/.github/actions" "$work/.ign-actions/.github/actions"
  cp "$wf" "$work/.github/workflows/"
  cp -R "$REPO_ROOT"/.github/actions/. "$work/.github/actions/"
  cp -R "$REPO_ROOT"/.github/actions/. "$work/.ign-actions/.github/actions/"
  git -C "$work" init -q
  (cd "$work" && actionlint -ignore 'property "workflow_(repository|sha)" is not defined' ".github/workflows/$(basename "$wf")")
}

# shellcheck_bodies <file>: shellcheck of every bash body region of a file.
# actionlint checks the run: bodies of a workflow but never those of an
# action.yml. The `//` regions are JavaScript and are excluded.
shellcheck_bodies() {
  local f="$1" region
  while IFS= read -r region; do
    extract_first "$f" "$region" | shellcheck -s bash - || die "$f: shellcheck rejected region $region"
  done < <(grep -oE '# >>> body:[A-Za-z0-9-]+' "$f" | sed 's/^# >>> //' | awk '!seen[$0]++')
}

# autocrlf_checks <file>: git config from the environment (GIT_CONFIG_COUNT and
# friends) outranks the system gitconfig, where Git for Windows sets
# core.autocrlf=true. Every step is named; every actions/checkout step, and
# every step whose bash body runs git or "$IGN_BIN", carries the three lines.
autocrlf_checks() {
  local f="$1" d n body bad="" total=0
  d="$(mktemp -d)"
  awk -v d="$d" '
    /^[[:space:]]*- name:/ { n++; out = sprintf("%s/step-%03d", d, n) }
    n { print > out }
  ' "$f"
  for s in "$d"/step-*; do
    [ -e "$s" ] || continue
    total=$((total + 1))
    n="$(head -n 1 "$s" | sed -E 's/^[[:space:]]*- name:[[:space:]]*//')"
    need=0
    if grep -qE 'uses: actions/checkout@' "$s"; then need=1; fi
    body="$(awk '/# >>> body:/ { on = 1; next } /# <<< body:/ { on = 0; next } on' "$s")"
    case "$body" in
      *'git '* | *'"$IGN_BIN"'*) need=1 ;;
    esac
    if [ "$need" = 1 ]; then
      grep -qxE "[[:space:]]*GIT_CONFIG_COUNT: '1'" "$s" \
        && grep -qxE '[[:space:]]*GIT_CONFIG_KEY_0: core\.autocrlf' "$s" \
        && grep -qxE "[[:space:]]*GIT_CONFIG_VALUE_0: 'false'" "$s" \
        || bad="$bad
  $n"
    fi
  done
  /bin/rm -rf "$d"
  [ "$total" -gt 0 ] || die "$f: autocrlf_checks found no steps"
  [ "$(grep -cE '^[[:space:]]*- (uses|run|id|shell|if|env|with):' "$f")" = 0 ] || die "$f: a step is not introduced by '- name:'"
  [ -z "$bad" ] || die "$f: steps without GIT_CONFIG_COUNT/KEY_0/VALUE_0 (core.autocrlf=false):$bad"
}
