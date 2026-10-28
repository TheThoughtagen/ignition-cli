#!/usr/bin/env bash
# README checks for the "Running deploy in CI" section. Run from the repo root
# or anywhere inside it. Fails with a message naming the first broken check.
set -euo pipefail
# shellcheck disable=SC2015,SC2016

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
  '--designers wait' 'ign deploy rollback --env' OpenSSH 2.327.1 ign-deploy; do
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
for term in deploy-projects.yml 'github.ref_name' 'needs: apply-dev' 'environment: prod' \
  deploy-projects-check deploy-projects-apply 'artifact-sha256' 'targets-sha' 'secrets: inherit' DEPLOY_SSH_KEY \
  '#a-read-only-check-token' 'check-write' 'Git\bin' 'GIT_CONFIG_COUNT' 'job.workflow_sha' 'IGN_CHECK_TOKEN' 'IGN_CHECK_SSH_KEY' \
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

# 6. The caller template and the same-organization form of the deploy projects section.
# extract_after <marker-comment> <file>: the fenced yaml block that follows the marker line.
extract_after() {
  awk -v m="$1" '
    $0 == m { armed = 1; next }
    armed && !on && $0 == "```yaml" { on = 1; next }
    on && $0 == "```" { exit }
    on { print }
  ' "$2"
}
# job_block <job-id> <file>: the lines of one two-space job under jobs:.
job_block() {
  awk -v j="$1" '
    /^jobs:/ { in_jobs = 1; next }
    in_jobs && /^  [A-Za-z0-9_-]+:/ { cur = $1; sub(/:$/, "", cur); on = (cur == j) }
    in_jobs && /^[^ ]/ { on = 0 }
    on { print }
  ' "$2"
}

TPL="$WORK/caller-template.yml"
extract_after '<!-- deploy-projects-caller-template -->' README.md > "$TPL"
[ -s "$TPL" ] || die "6a no caller template block follows the marker"

# 6a. actionlint of the template against the real actions: the two remote uses
# are rewritten to the local actions, so every with: key and required input is checked.
TPL_LOCAL="$WORK/caller-template-local.yml"
# actionlint knows only the GitHub-hosted labels and the self-hosted ones, never a
# runner's custom label, so the unique label is dropped from the lint copy; 6c
# checks it on the original block.
sed -E -e 's#uses: TheThoughtagen/ignition-cli/\.github/actions/([a-z-]+)@[^[:space:]]+#uses: ./.github/actions/\1#' \
  -e 's#^(    runs-on: \[self-hosted, windows), ign-deploy-[a-z]+\]#\1]#' "$TPL" > "$TPL_LOCAL"
[ "$(grep -c 'uses: \./\.github/actions/deploy-projects-' "$TPL_LOCAL")" = 4 ] || die "6a the template does not use the two actions in four steps"
lint_with_local_actions "$TPL_LOCAL" "$WORK/tpl-lint" || die "6a actionlint rejected the caller template against the real actions"
pass "6a the caller template is actionlint-valid against the real actions"

# 6b. Structure.
ids="$(awk '/^jobs:/{j=1; next} j && /^  [A-Za-z0-9_-]+:/{id=$1; sub(/:$/, "", id); printf "%s ", id}' "$TPL")"
[ "$ids" = "check-dev apply-dev check-prod apply-prod " ] || die "6b the template jobs are '$ids', not check-dev apply-dev check-prod apply-prod"
grep -qE '^permissions:$' "$TPL" && grep -qE '^  contents: read$' "$TPL" || die "6b the template lacks top-level permissions: contents: read"
for e in dev prod; do
  c="$(job_block "check-$e" "$TPL")"
  a="$(job_block "apply-$e" "$TPL")"
  ! grep -qE '^    environment:' <<<"$c" || die "6b check-$e has an environment:"
  ! grep -qE 'secrets\.IGN_MASTER_TOKEN|secrets\.IGN_SSH_KEY' <<<"$c" || die "6b check-$e references a write secret"
  grep -qF 'artifact_sha256: ${{ steps.check.outputs.artifact_sha256 }}' <<<"$c" || die "6b check-$e does not output artifact_sha256 from steps.check"
  grep -qF 'targets_sha: ${{ steps.check.outputs.targets_sha }}' <<<"$c" || die "6b check-$e does not output targets_sha from steps.check"
  grep -qxF "    needs: check-$e" <<<"$a" || die "6b apply-$e lacks needs: check-$e"
  grep -qxF "    environment: $e" <<<"$a" || die "6b apply-$e lacks environment: $e"
  grep -qxF "      group: ign-deploy-projects-$e-\${{ github.repository }}" <<<"$a" || die "6b apply-$e lacks its concurrency group"
  grep -qxF "      cancel-in-progress: false" <<<"$a" || die "6b apply-$e lacks cancel-in-progress: false"
  grep -qF "artifact-sha256: \${{ needs.check-$e.outputs.artifact_sha256 }}" <<<"$a" || die "6b apply-$e lacks the artifact-sha256 handoff"
  grep -qF "targets-sha: \${{ needs.check-$e.outputs.targets_sha }}" <<<"$a" || die "6b apply-$e lacks the targets-sha handoff"
  grep -qF 'master-token: ${{ secrets.IGN_MASTER_TOKEN }}' <<<"$a" || die "6b apply-$e lacks master-token from IGN_MASTER_TOKEN"
  grep -qF 'ssh-key: ${{ secrets.IGN_SSH_KEY }}' <<<"$a" || die "6b apply-$e lacks ssh-key from IGN_SSH_KEY"
done
grep -qxF "    needs: apply-dev" <<<"$(job_block check-prod "$TPL")" || die "6b check-prod lacks needs: apply-dev"
# The write secrets appear in apply jobs only.
[ "$(grep -c 'secrets\.IGN_MASTER_TOKEN' "$TPL")" = 2 ] && [ "$(grep -c 'secrets\.IGN_SSH_KEY' "$TPL")" = 2 ] || die "6b the write secrets are not referenced exactly once per apply job"
pass "6b the template has the secret split, the handoff, the environments, the concurrency groups and the job order"

# 6c. Runner labels, ssh-key-env and the single tag.
while IFS= read -r line; do
  labels="$(sed -E 's/^[[:space:]]*runs-on:[[:space:]]*\[(.*)\][[:space:]]*$/\1/' <<<"$line" | tr ',' '\n' | sed -E 's/^[[:space:]]+|[[:space:]]+$//g')"
  unique="$(awk 'NF && $0 !~ /^(self-hosted|windows|linux|macos|x64|arm64)$/' <<<"$labels")"
  [ -n "$unique" ] || die "6c a runs-on has no label unique to the deploy runner: $line"
done < <(grep -E '^[[:space:]]*runs-on:' "$TPL")
[ "$(grep -c '^[[:space:]]*runs-on:' "$TPL")" = 4 ] || die "6c the template does not have four runs-on lines"
while IFS= read -r v; do
  [[ "$v" =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || die "6c ssh-key-env '$v' is not a valid variable name"
  case "$v" in
    PATH|HOME|BASH_ENV|ENV|IFS|SHELLOPTS|PS4|CDPATH|GITHUB_*|RUNNER_*|ACTIONS_*|IGNITION_*|IGN_*|LD_*|DYLD_*) die "6c ssh-key-env '$v' is reserved" ;;
  esac
done < <(grep -E '^[[:space:]]*ssh-key-env:' "$TPL" | sed -E 's/^[[:space:]]*ssh-key-env:[[:space:]]*//')
tags="$( { grep -oE 'uses: [^ ]+/\.github/actions/[a-z-]+@[^[:space:]]+' "$TPL" | sed -E 's/.*@//'; grep -E '^[[:space:]]*ign-version:' "$TPL" | sed -E 's/^[[:space:]]*ign-version:[[:space:]]*//'; } | sort -u)"
[ "$(wc -l <<<"$tags" | tr -d ' ')" = 1 ] || die "6c the action refs and ign-version are not one tag: $(tr '\n' ' ' <<<"$tags")"
pass "6c every runs-on has a unique label, every ssh-key-env is valid and not reserved, and one tag is shared"

# 6d. The same-organization form.
SAME="$WORK/same-org.yml"
extract_after '<!-- deploy-projects-same-org -->' README.md > "$SAME"
[ -s "$SAME" ] || die "6d no same-org block follows the marker"
[ "$(grep -c 'uses: TheThoughtagen/ignition-cli/\.github/workflows/deploy-projects\.yml@' "$SAME")" = 2 ] || die "6d the same-org form does not call deploy-projects.yml twice"
[ "$(grep -c '^    secrets: inherit$' "$SAME")" = 2 ] || die "6d each same-org job must have secrets: inherit"
! grep -qE '^      IGN_[A-Z_]+: \$\{\{ secrets\.' "$SAME" || die "6d the same-org form maps a secret by name"
grep -qxF "    needs: dev" <<<"$(job_block prod "$SAME")" || die "6d the same-org prod job lacks needs: dev"
pass "6d the same-org form uses secrets: inherit, maps no secret by name and chains prod after dev"
