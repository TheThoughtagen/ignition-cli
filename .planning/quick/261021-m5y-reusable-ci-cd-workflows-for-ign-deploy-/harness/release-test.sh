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
