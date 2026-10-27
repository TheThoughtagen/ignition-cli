#!/usr/bin/env bash
# README checks for the "Running deploy in CI" section. Run from the repo root
# or anywhere inside it. Fails with a message naming the first broken check.
set -euo pipefail

# shellcheck source=lib.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"
cd "$REPO_ROOT"

WORK="${HARNESS_WORK:-$(mktemp -d)}"
mkdir -p "$WORK"

# 1. The exit-code region is byte-identical to the README at the commit that last
# changed it on purpose. The baseline is pinned to a commit, not HEAD, which
# already holds this task's README and would hide a changed exit code. It was
# 44919d4 (main before this task); Phase 25 added three exit-6 slugs
# (project_git_managed, project_parent_invalid, project_hash_mismatch), and
# 4814da9 (25-11) is the last commit that changed the region.
BASELINE=4814da9
region() { # region <file>: from `## Exit codes` up to `## Gateway authentication`
  awk '/^## Exit codes/{on=1} /^## Gateway authentication/{on=0} on' "$1"
}
git show "$(git rev-parse --verify "$BASELINE^{commit}")":README.md > "$WORK/readme-head.md"
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
awk '/^### Running deploy in CI$/{on=1; next} on && (/^## / || /^### Running `deploy projects` in CI$/){exit} on' README.md > "$WORK/ci-section.txt"
for term in deploy-plan.yml deploy-apply.yml IGN_MASTER_TOKEN IGN_BACKUP_TOKEN IGN_SSH_KEY IGNITION_CLI_CONFIG \
  deploy_rolled_back deploy_backup_unsynced deploy_preflight_failed plan_stale destructive_change_refused \
  '--designers wait' 'ign deploy rollback --env' OpenSSH 2.327.1; do
  grep -qF -e "$term" "$WORK/ci-section.txt" || die "4 the section lacks: $term"
done
pass "4 the section contains every required term"

# 5. The deploy projects CI section.
# shellcheck disable=SC2016
pj='^### Running `deploy projects` in CI$'
[ "$(grep -c "$pj" README.md)" = 1 ] || die "5 '### Running \`deploy projects\` in CI' does not occur exactly once"
pj_line="$(grep -n "$pj" README.md | cut -d: -f1)"
{ [ "$pj_line" -gt "$ci_line" ] && [ "$pj_line" -lt "$lint_line" ]; } || die "5 the deploy projects section is not between Running deploy in CI and ## Linting"
awk '/^### Running `deploy projects` in CI$/{on=1; next} on && /^## /{exit} on' README.md > "$WORK/pj-section.txt"
for term in deploy-projects.yml 'github.ref_name' 'needs: dev' 'environment: prod' 'IGN_CHECK_TOKEN' 'IGN_CHECK_SSH_KEY' \
  'IGN_MASTER_TOKEN' 'IGN_SSH_KEY' 'IGN_TARGETS_TOKEN' 'read-only' 'replaces' 'cancel-in-progress: false' \
  'artifact_sha256' 'project_hash_mismatch' 'deploy_project_unverified' \
  '(#cutting-a-project-over-from-the-git-module)' '(#what-a-project-repo-needs)'; do
  grep -qF -e "$term" "$WORK/pj-section.txt" || die "5 the deploy projects section lacks: $term"
done
# Every in-page link target of the section resolves to a heading of the README.
slug() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | sed -e 's/`//g' -e 's/[^a-z0-9 _-]//g' -e 's/ /-/g'; }
grep -o '](#[a-z0-9-]*)' "$WORK/pj-section.txt" | sed -e 's/^](#//' -e 's/)$//' | sort -u > "$WORK/pj-links.txt"
grep -E '^#{1,6} ' README.md | sed -E 's/^#+ //' | while IFS= read -r h; do slug "$h"; echo; done | sort -u > "$WORK/heading-slugs.txt"
while IFS= read -r l; do
  grep -qxF -e "$l" "$WORK/heading-slugs.txt" || die "5 the deploy projects section links to #$l, which is no heading"
done < "$WORK/pj-links.txt"
pass "5 the deploy projects section exists once, after Running deploy in CI, with the caller, the secret split, the concurrency rule and resolving links"
