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
