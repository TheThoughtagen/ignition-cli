#!/usr/bin/env bash
# Harness for .github/workflows/deploy-plan.yml. Run from anywhere.
# Prints `PASS <case>` per case and exits non-zero on the first failure.
#
# Environment: HARNESS_WORK (scratch dir, default mktemp -d; must be under a
# path Docker can bind-mount, for example /private/tmp or /Users), IMAGE
# (default buildpack-deps:noble-curl), IGN_TEST_VERSION (default v1.5.0).
set -euo pipefail

# shellcheck source=lib.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

WORK="${HARNESS_WORK:-$(mktemp -d)}"
IMAGE="${IMAGE:-buildpack-deps:noble-curl}"
VERSION="${IGN_TEST_VERSION:-v1.5.0}"
RELEASE_BASE="https://github.com/TheThoughtagen/ignition-cli/releases/download"
mkdir -p "$WORK"
export HARNESS_TMP="$WORK"
cd "$REPO_ROOT"

# ---------------------------------------------------------------- 1. lint
actionlint "$PLAN_WF" || die "actionlint (with shellcheck) rejected deploy-plan.yml"
pass "1a actionlint + shellcheck clean"
body_checks "$PLAN_WF"
pass "1b run/script bodies are block scalars in body regions with no expression"

# ---------------------------------------------------------------- 2. install
case "$(uname -m)" in
  arm64 | aarch64) CONTAINER_ARCH=ARM64 ;;
  *) CONTAINER_ARCH=X64 ;;
esac

extract "$PLAN_WF" body:install-ign > "$WORK/install.sh"

make_shim() { # make_shim <dir> ; behaviour is driven by SHIM_TAMPER and SHIM_LOG
  mkdir -p "$1"
  cat > "$1/curl" << 'EOF'
#!/bin/bash
echo "$*" >> "${SHIM_LOG:?}"
dir="$(cd "$(dirname "$0")" && pwd)"
real="$(PATH="${PATH//$dir:/}" command -v curl)"
"$real" "$@" || exit $?
prev=""
out=""
for a in "$@"; do
  if [ "$prev" = "-o" ]; then out="$a"; fi
  prev="$a"
done
if [ "${SHIM_TAMPER:-0}" = "1" ]; then
  case "$out" in
    *.tar.gz | *.zip) printf 'x' >> "$out" ;;
  esac
fi
EOF
  chmod +x "$1/curl"
}

run_install_container() { # run_install_container <case-dir> <tamper 0|1>
  local d="$1" tamper="$2"
  mkdir -p "$d/temp"
  : > "$d/github_env"
  cp "$WORK/install.sh" "$d/install.sh"
  make_shim "$d/shim"
  : > "$d/curl.log"
  docker run --rm -v "$d:/work" \
    -e RUNNER_OS=Linux -e "RUNNER_ARCH=$CONTAINER_ARCH" \
    -e RUNNER_TEMP=/work/temp -e GITHUB_ENV=/work/github_env \
    -e "IGN_VERSION=$VERSION" -e "IGN_RELEASE_BASE=$RELEASE_BASE" \
    -e SHIM_LOG=/work/curl.log -e "SHIM_TAMPER=$tamper" \
    "$IMAGE" bash -c 'PATH=/work/shim:$PATH exec bash /work/install.sh'
}

C2A="$WORK/c2a"
/bin/rm -rf "$C2A"
mkdir -p "$C2A"
run_install_container "$C2A" 0 > "$C2A/stdout.txt" 2> "$C2A/stderr.txt" \
  || { cat "$C2A/stderr.txt" >&2; die "2a install of $VERSION failed in $IMAGE (if this is a glibc error, use a newer buildpack-deps:*-curl tag)"; }
grep -q '^IGN_BIN=' "$C2A/github_env" || die "2a GITHUB_ENV has no IGN_BIN"
grep -q "${VERSION#v}" "$C2A/stdout.txt" || die "2a ign --version output does not contain ${VERSION#v}"
[ "$(grep -c . "$C2A/curl.log")" = 2 ] || die "2a expected exactly two curl calls"
pass "2a pinned install verifies the checksum, extracts and reports $VERSION ($IMAGE, $CONTAINER_ARCH)"

C2B="$WORK/c2b"
/bin/rm -rf "$C2B"
mkdir -p "$C2B"
if run_install_container "$C2B" 1 > "$C2B/stdout.txt" 2> "$C2B/stderr.txt"; then
  die "2b a tampered archive was accepted"
fi
grep -q 'checksum mismatch' "$C2B/stderr.txt" "$C2B/stdout.txt" || die "2b no checksum mismatch message"
if grep -q '^IGN_BIN=' "$C2B/github_env"; then die "2b IGN_BIN was exported after a mismatch"; fi
[ ! -e "$C2B/temp/ign-install/x" ] || die "2b an extraction directory exists after a mismatch"
if find "$C2B/temp" -type f -name ign -o -type f -name ign.exe | grep -q .; then
  die "2b a binary was extracted after a mismatch"
fi
pass "2b a tampered archive fails on the checksum before any extraction"

# 2c and 2d fail before any network call, so they run on the host.
HOSTDIR="$WORK/c2cd"
/bin/rm -rf "$HOSTDIR"
mkdir -p "$HOSTDIR"
make_shim "$HOSTDIR/shim"
(
  fake_runner_env "$HOSTDIR/c"
  export RUNNER_OS=Linux RUNNER_ARCH=X64 IGN_RELEASE_BASE="$RELEASE_BASE" SHIM_LOG="$HOSTDIR/curl.log"
  : > "$SHIM_LOG"
  PATH="$HOSTDIR/shim:$PATH" IGN_VERSION='v1.5.0/../x' bash "$WORK/install.sh" > /dev/null 2>&1 && exit 11
  [ ! -s "$SHIM_LOG" ] || exit 12
  exit 0
) || die "2c a path-bearing ign-version was not refused before the network ($?)"
pass "2c ign-version with a path is refused before any request"
(
  fake_runner_env "$HOSTDIR/d"
  export RUNNER_OS=macOS RUNNER_ARCH=ARM64 IGN_RELEASE_BASE="$RELEASE_BASE" IGN_VERSION="$VERSION"
  bash "$WORK/install.sh" > /dev/null 2>&1 && exit 1
  exit 0
) || die "2d macOS was accepted"
pass "2d an unsupported runner OS is refused"

# ---------------------------------------------------------------- 3. end to end
IGN_REPO_BIN="$(build_repo_bin)"

E2E="$WORK/e2e"
make_e2e_repo "$E2E"

extract "$PLAN_WF" body:write-config > "$WORK/write-config.sh"
extract "$PLAN_WF" body:run-plan > "$WORK/run-plan.sh"
(
  fake_runner_env "$E2E/runner"
  export RUNNER_OS
  RUNNER_OS="$(host_runner_os)"
  export IGN_MASTER_PROFILE=dev IGN_MASTER_URL=http://127.0.0.1:9 IGN_BACKUP_PROFILE='' IGN_BACKUP_URL='' IGN_SSL_VERIFY=true
  bash "$WORK/write-config.sh"
  load_env_file "$GITHUB_ENV"
  grep -q 'url = "http://127.0.0.1:9"' "$IGNITION_CLI_CONFIG" || exit 21
  if grep -qi 'token\|auth' "$IGNITION_CLI_CONFIG"; then exit 22; fi
  export IGN_BIN="$IGN_REPO_BIN" IGN_ENV=dev IGN_REPO_DIR="$E2E/repo" IGN_SSH_KEY_ENV='' IGN_SSH_KEY=''
  bash "$WORK/run-plan.sh"
) > "$E2E/steps.log" 2>&1 || { cat "$E2E/steps.log" >&2; die "3 the extracted write-config/run-plan steps failed"; }
[ "$(cat "$E2E/runner/github_output")" = "exit_code=0" ] || die "3 GITHUB_OUTPUT is not exit_code=0: $(cat "$E2E/runner/github_output")"
E2E_OUT="$E2E/runner/temp/ign-out"
jq -e '.data.plan_sha256 | test("^[0-9a-f]{64}$")' "$E2E_OUT/plan.stdout.json" > /dev/null || die "3 plan.stdout.json has no 64-hex plan_sha256"
pass "3 real ign deploy plan --json through the extracted steps (exit 0, plan_sha256)"

node "$HARNESS_DIR/report-test.mjs" plan "$E2E_OUT"
pass "3 report cases (i) to (vii)"
