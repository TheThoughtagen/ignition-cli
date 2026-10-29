#!/usr/bin/env bash
# Offline structural checks of .github/workflows/release.yml (G-25-7): the
# static CRT flag sits on the Windows matrix entry only, reaches cargo through
# setup-rust-toolchain's rustflags input, and the Windows import check runs
# before anything is packaged or uploaded. Run from anywhere inside the repo.
set -euo pipefail

# shellcheck source=lib.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"
cd "$REPO_ROOT"

RELEASE_WF="$REPO_ROOT/.github/workflows/release.yml"
CHECK_NAME='Check the Windows binary links the C runtime statically'

# 1. actionlint accepts the workflow.
actionlint "$RELEASE_WF" || die "1 actionlint rejects release.yml"
pass "1 actionlint accepts release.yml"

# 2. +crt-static is on exactly one non-comment line, the Windows matrix entry.
n_static="$(grep -v '^[[:space:]]*#' "$RELEASE_WF" | grep -c 'target-feature=+crt-static' || true)"
[ "$n_static" = 1 ] || die "2 +crt-static appears on $n_static non-comment lines, expected 1"
win_entry="$(awk '
  /^          - os:/ { if (on) exit; entry = "" ; cur = 1 }
  /^    runs-on:/ { if (on) exit; cur = 0 }
  cur { entry = entry $0 "\n" }
  cur && /target: x86_64-pc-windows-msvc/ { on = 1 }
  END { if (on) printf "%s", entry }
' "$RELEASE_WF")"
[ -n "$win_entry" ] || die "2 no matrix entry holds target: x86_64-pc-windows-msvc"
printf '%s' "$win_entry" | grep -qE "^[[:space:]]+rustflags: '.*\+crt-static.*'$" \
  || die "2 the Windows matrix entry has no rustflags line with +crt-static"
n_plain="$(grep -cE "^[[:space:]]+rustflags: '-D warnings'$" "$RELEASE_WF" || true)"
[ "$n_plain" = 4 ] || die "2 expected 4 \"rustflags: '-D warnings'\" lines, found $n_plain"
pass "2 +crt-static is on the Windows matrix entry only; four entries keep -D warnings"

# 3. The setup step passes matrix.rustflags, once, inside setup-rust-toolchain.
# shellcheck disable=SC2016
n_input="$(grep -cF 'rustflags: ${{ matrix.rustflags }}' "$RELEASE_WF" || true)"
[ "$n_input" = 1 ] || die "3 'rustflags: \${{ matrix.rustflags }}' occurs $n_input times, expected 1"
# shellcheck disable=SC2016
setup_block="$(awk '
  /^      - / { if (on) exit; on = 0 }
  !done && /uses: actions-rust-lang\/setup-rust-toolchain@v1/ { on = 1; done = 1 }
  on { print }
' "$RELEASE_WF")"
printf '%s' "$setup_block" | grep -qF 'rustflags: ${{ matrix.rustflags }}' \
  || die "3 the rustflags input is not inside the setup-rust-toolchain step"
pass "3 the setup-rust-toolchain step passes matrix.rustflags"

# 4. The import check step exists once and has the required contents.
[ "$(grep -cF -e "- name: $CHECK_NAME" "$RELEASE_WF")" = 1 ] || die "4 the step '$CHECK_NAME' does not occur exactly once"
check_block="$(awk -v name="- name: $CHECK_NAME" '
  index($0, name) { on = 1; print; next }
  on && /^      - name:/ { exit }
  on { print }
' "$RELEASE_WF")"
for needle in "if: runner.os == 'Windows'" 'shell: pwsh' '/dependents' 'vswhere' 'KERNEL32' 'vcruntime' 'msvcp'; do
  printf '%s' "$check_block" | grep -qF -e "$needle" || die "4 the check step lacks: $needle"
done
pass "4 the Windows import check step has the required contents"

# 5. Step order: build < check < zip packaging < upload.
line_of() { grep -nF -e "- name: $1" "$RELEASE_WF" | head -n 1 | cut -d: -f1; }
l_build="$(line_of 'Build release binary')"
l_check="$(line_of "$CHECK_NAME")"
l_zip="$(line_of 'Package zip (Windows)')"
l_up="$(line_of 'Upload to release')"
for v in "$l_build" "$l_check" "$l_zip" "$l_up"; do [ -n "$v" ] || die "5 a step name was not found"; done
{ [ "$l_build" -lt "$l_check" ] && [ "$l_check" -lt "$l_zip" ] && [ "$l_zip" -lt "$l_up" ]; } \
  || die "5 step order is wrong: build=$l_build check=$l_check zip=$l_zip upload=$l_up"
pass "5 the import check runs after the build and before packaging and upload"

# WR-04. The attest job is the only holder of id-token: write and attestations: write. The build
# job runs third-party tag-pinned actions (rust-cache, setup-rust-toolchain, action-gh-release); a
# token minted there would sign as release.yml. The attest job runs only the first-party attest
# action and the gh CLI.
job_block() { # job_block <job>: the job's lines, from '  <job>:' to the next job or EOF
  awk -v job="$1" '$0 == "  " job ":" { on = 1; print; next } on && /^  [a-z]/ { exit } on { print }' "$RELEASE_WF"
}
noncomment() { grep -v '^[[:space:]]*#' || true; }
ATTEST="$(job_block attest)"
[ -n "$ATTEST" ] || die "6 release.yml has no attest job"

# 6. Permissions: top level stays contents: write; the two new ones appear once, in the attest job.
top_perms="$(awk '/^permissions:/ { on = 1; next } on && /^[^[:space:]]/ { exit } on && NF { print }' "$RELEASE_WF" | noncomment)"
[ "$top_perms" = '  contents: write' ] || die "6 the top-level permissions are not exactly 'contents: write': $top_perms"
[ "$(noncomment < "$RELEASE_WF" | grep -c 'id-token: write')" = 1 ] || die "6 id-token: write does not occur exactly once"
[ "$(noncomment < "$RELEASE_WF" | grep -c 'attestations: write')" = 1 ] || die "6 attestations: write does not occur exactly once"
printf '%s\n' "$ATTEST" | grep -q 'id-token: write' || die "6 id-token: write is not in the attest job"
printf '%s\n' "$ATTEST" | grep -q 'attestations: write' || die "6 attestations: write is not in the attest job"
for j in build publish-crates; do
  if job_block "$j" | grep -qE 'id-token|attestations'; then die "6 the $j job mentions id-token or attestations"; fi
done
pass "6 top-level permissions stay contents: write; id-token and attestations write appear once, in the attest job only"

# 7. The attest job's contents.
printf '%s\n' "$ATTEST" | grep -qxF '    needs: build' || die "7 the attest job does not need build"
printf '%s\n' "$ATTEST" | grep -qxF '    runs-on: ubuntu-latest' || die "7 the attest job does not run on ubuntu-latest"
attest_perms="$(printf '%s\n' "$ATTEST" | awk '/^    permissions:/ { on = 1; next } on && /^      [a-z-]+:/ { sub(/^ +/, ""); print; next } on { on = 0 }' | sort | tr '\n' '|')"
[ "$attest_perms" = 'attestations: write|contents: read|id-token: write|' ] || die "7 the attest job's permissions are '$attest_perms'"
[ "$(printf '%s\n' "$ATTEST" | grep -cE '^[[:space:]]*(- )?uses:')" = 1 ] || die "7 the attest job runs an action other than the attest action"
printf '%s\n' "$ATTEST" | grep -qE '^[[:space:]]+uses: actions/attest-build-provenance@v4$' || die "7 the attest job does not use actions/attest-build-provenance@v4"
[ "$(noncomment < "$RELEASE_WF" | grep -c 'actions/attest-build-provenance@v4')" = 1 ] || die "7 actions/attest-build-provenance@v4 does not occur exactly once"
printf '%s\n' "$ATTEST" | grep -qxF '          subject-path: assets/ign-*' || die "7 the attest step lacks subject-path: assets/ign-*"
for needle in 'gh release download "$TAG_NAME"' "--pattern 'ign-*'" '--dir assets' 'sha256sum -c' 'shell: bash'; do
  printf '%s\n' "$ATTEST" | grep -qF -e "$needle" || die "7 the attest job lacks: $needle"
done
pass "7 the attest job needs build, holds exactly the three permissions, runs only the attest action and checks every archive against its sha256"

# 8. Order: download before attest.
l_dl="$(printf '%s\n' "$ATTEST" | grep -nF -e '- name: Download the release assets' | head -n 1 | cut -d: -f1)"
l_at="$(printf '%s\n' "$ATTEST" | grep -nF -e '- name: Attest build provenance' | head -n 1 | cut -d: -f1)"
{ [ -n "$l_dl" ] && [ -n "$l_at" ] && [ "$l_dl" -lt "$l_at" ]; } || die "8 the download step does not precede the attest step ($l_dl, $l_at)"
pass "8 the assets are downloaded and checked before they are attested"
