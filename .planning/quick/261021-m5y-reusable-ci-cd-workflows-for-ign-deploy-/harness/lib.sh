#!/usr/bin/env bash
# Shared helpers for the quick-m5y workflow harness. Source, do not execute.
#
# Region markers: a line containing `>>> <region>` opens a region and a line
# containing `<<< <region>` closes it. The marker may follow `#` or `//`.

HARNESS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$HARNESS_DIR" rev-parse --show-toplevel)"
PLAN_WF="$REPO_ROOT/.github/workflows/deploy-plan.yml"
APPLY_WF="$REPO_ROOT/.github/workflows/deploy-apply.yml"

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
