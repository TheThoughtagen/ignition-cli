#!/usr/bin/env bash
# README checks for the "Running deploy in CI" section. Run from the repo root
# or anywhere inside it. Fails with a message naming the first broken check.
set -euo pipefail

# shellcheck source=lib.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"
cd "$REPO_ROOT"

WORK="${HARNESS_WORK:-$(mktemp -d)}"
mkdir -p "$WORK"

# 1. The exit-code region is byte-identical to the pre-change README. The
# baseline is pinned to 44919d4 (main before this task), not HEAD, which
# already holds this task's README and would hide a changed exit code.
BASELINE=44919d409e3a4c007baeecf87bfbcfb7fb2cf507
region() { # region <file>: from `## Exit codes` up to `## Gateway authentication`
  awk '/^## Exit codes/{on=1} /^## Gateway authentication/{on=0} on' "$1"
}
git show "$BASELINE":README.md > "$WORK/readme-head.md"
region "$WORK/readme-head.md" > "$WORK/exit-head.txt"
region README.md > "$WORK/exit-work.txt"
[ -s "$WORK/exit-head.txt" ] || die "1 the baseline README has no Exit codes region"
cmp -s "$WORK/exit-head.txt" "$WORK/exit-work.txt" || die "1 the Exit codes region differs from the baseline ($BASELINE)"
pass "1 the Exit codes region is byte-identical to the baseline"

# 2. Single headings.
[ "$(grep -c '^## Exit codes' README.md)" = 1 ] || die "2 '## Exit codes' does not occur exactly once"
[ "$(grep -c '^### Running deploy in CI$' README.md)" = 1 ] || die "2 '### Running deploy in CI' does not occur exactly once"
[ "$(grep -c '^### `deploy rollback`$' README.md)" = 1 ] || die "2 '### \`deploy rollback\`' does not occur exactly once"
pass "2 the headings occur exactly once"

# 3. Placement.
ci_line="$(grep -n '^### Running deploy in CI$' README.md | cut -d: -f1)"
rb_line="$(grep -n '^### `deploy rollback`$' README.md | cut -d: -f1)"
lint_line="$(grep -n '^## Linting' README.md | head -n 1 | cut -d: -f1)"
{ [ "$ci_line" -gt "$rb_line" ] && [ "$ci_line" -lt "$lint_line" ]; } || die "3 the section is not between deploy rollback and ## Linting"
pass "3 the section sits between deploy rollback and ## Linting"

# 4. Required terms.
awk '/^### Running deploy in CI$/{on=1; next} on && /^## /{exit} on' README.md > "$WORK/ci-section.txt"
for term in deploy-plan.yml deploy-apply.yml IGN_MASTER_TOKEN IGN_BACKUP_TOKEN IGN_SSH_KEY IGNITION_CLI_CONFIG \
  deploy_rolled_back deploy_backup_unsynced deploy_preflight_failed plan_stale destructive_change_refused \
  '--designers wait' 'ign deploy rollback --env' OpenSSH 2.327.1; do
  grep -qF -e "$term" "$WORK/ci-section.txt" || die "4 the section lacks: $term"
done
pass "4 the section contains every required term"
