#!/usr/bin/env bash
# Harness for .github/workflows/deploy-projects.yml. Run from anywhere.
# Prints `PASS <case>` per case and exits non-zero on the first failure.
#
# Environment: HARNESS_WORK (scratch dir, default mktemp -d).
set -euo pipefail

# shellcheck source=lib.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

WORK="${HARNESS_WORK:-$(mktemp -d)}"
mkdir -p "$WORK"
export HARNESS_TMP="$WORK"
cd "$REPO_ROOT"

MOCK_PID=""
cleanup() {
  if [ -n "$MOCK_PID" ]; then kill "$MOCK_PID" > /dev/null 2>&1 || true; fi
}
trap cleanup EXIT

# count_regions <file> <region>: how many times <region> is opened.
count_regions() { grep -cE ">>> $2\$" "$1" || true; }

# ---------------------------------------------------------------- 1. lint
lint_with_local_actions "$PROJECTS_WF" "$WORK/lint" || die "actionlint (with shellcheck) rejected deploy-projects.yml with its actions in place"
pass "1a actionlint + shellcheck clean, local action inputs validated"
body_checks "$PROJECTS_WF"
body_checks "$CHECK_ACTION"
body_checks "$APPLY_ACTION"
pass "1b run/script bodies are block scalars in body regions with no expression"
shellcheck_bodies "$CHECK_ACTION"
shellcheck_bodies "$APPLY_ACTION"
pass "1c shellcheck clean on every action bash body"

# ---------------------------------------------------------------- 2. shared regions
for region in install-ign write-config ign-env; do
  [ "$(count_regions "$CHECK_ACTION" "shared:$region")" = 1 ] || die "2 shared:$region does not occur once in the check action"
  [ "$(count_regions "$APPLY_ACTION" "shared:$region")" = 1 ] || die "2 shared:$region does not occur once in the apply action"
  [ "$(count_regions "$PROJECTS_WF" "shared:$region")" = 0 ] || die "2 shared:$region is copied into deploy-projects.yml"
  extract "$PLAN_WF" "shared:$region" > "$WORK/shared-plan-$region.txt"
  [ -s "$WORK/shared-plan-$region.txt" ] || die "2 shared:$region is empty in deploy-plan.yml"
  extract "$CHECK_ACTION" "shared:$region" > "$WORK/shared-check-$region.txt"
  extract "$APPLY_ACTION" "shared:$region" > "$WORK/shared-apply-$region.txt"
  cmp -s "$WORK/shared-plan-$region.txt" "$WORK/shared-check-$region.txt" \
    || die "2 shared:$region differs between deploy-plan.yml and the check action"
  cmp -s "$WORK/shared-plan-$region.txt" "$WORK/shared-apply-$region.txt" \
    || die "2 shared:$region differs between deploy-plan.yml and the apply action"
done
pass "2a shared install-ign, write-config and ign-env regions occur once in each action, byte-identical with deploy-plan.yml, and nowhere in deploy-projects.yml"

[ "$(count_regions "$CHECK_ACTION" shared:targets-args)" = 1 ] || die "2b shared:targets-args does not occur once in the check action"
[ "$(count_regions "$APPLY_ACTION" shared:targets-args)" = 1 ] || die "2b shared:targets-args does not occur once in the apply action"
[ "$(count_regions "$PROJECTS_WF" shared:targets-args)" = 0 ] || die "2b shared:targets-args is copied into deploy-projects.yml"
[ "$(count_regions "$PLAN_WF" shared:targets-args)" = 1 ] || die "2b shared:targets-args does not occur once in deploy-plan.yml"
[ "$(count_regions "$APPLY_WF" shared:targets-args)" = 1 ] || die "2b shared:targets-args does not occur once in deploy-apply.yml"
extract "$PLAN_WF" shared:targets-args > "$WORK/targets-args-plan.txt"
[ -s "$WORK/targets-args-plan.txt" ] || die "2b shared:targets-args is empty in deploy-plan.yml"
for f in "$APPLY_WF" "$CHECK_ACTION" "$APPLY_ACTION"; do
  extract "$f" shared:targets-args > "$WORK/targets-args-other.txt"
  cmp -s "$WORK/targets-args-plan.txt" "$WORK/targets-args-other.txt" || die "2b shared:targets-args differs between deploy-plan.yml and $(basename "$(dirname "$f")")/$(basename "$f")"
done
pass "2b shared targets-args region is byte-identical in deploy-plan.yml, deploy-apply.yml, the check action and the apply action"

# One source, not a copy: deploy-projects.yml carries no step body except locating the actions,
# verifying their commit and naming the cross-org cause of empty apply secrets.
[ "$(grep -oE '>>> body:[A-Za-z0-9-]+' "$PROJECTS_WF" | sed 's/^>>> //' | sort -u | tr '\n' ' ')" = 'body:actions-ref body:apply-secrets body:verify-actions-sha ' ] \
  || die "2c the body regions of deploy-projects.yml are not exactly actions-ref, verify-actions-sha and apply-secrets"
pass "2c deploy-projects.yml holds only the actions-ref, verify-actions-sha and apply-secrets bodies"

# ---------------------------------------------------------------- 3. structure
# shellcheck disable=SC2016
for needle in 'needs: check' 'environment: ${{ inputs.environment }}' 'cancel-in-progress: false' \
  'group: ign-deploy-projects-${{ inputs.env }}-${{ github.repository }}' \
  'needs.check.outputs.artifact_sha256' 'workflow_call'; do
  grep -qF -e "$needle" "$PROJECTS_WF" || die "3 deploy-projects.yml lacks: $needle"
done
for needle in '--check' '--targets'; do
  grep -qF -e "$needle" "$CHECK_ACTION" || die "3 the check action lacks: $needle"
done
for needle in '--targets' '--artifact-sha256' '--yes' '--designers wait' '--designers-timeout'; do
  grep -qF -e "$needle" "$APPLY_ACTION" || die "3 the apply action lacks: $needle"
done
[ "$(grep -cE '^  (check|apply):$' "$PROJECTS_WF")" = 2 ] || die "3 expected exactly the jobs check and apply"
needs_line="$(grep -n '^    needs: check$' "$PROJECTS_WF" | cut -d: -f1)"
apply_line="$(grep -n '^  apply:$' "$PROJECTS_WF" | cut -d: -f1)"
[ "$needs_line" = "$((apply_line + 1))" ] || die "3 the apply job does not need the check job"
grep -A1 -E '^permissions:$' "$PROJECTS_WF" | grep -q 'contents: read' || die "3 top-level permissions is not contents: read"
if grep -qE 'contents: write|pull-requests: write|pull_request_target' "$PROJECTS_WF"; then
  die "3 deploy-projects.yml grants write access or uses pull_request_target"
fi
for f in "$PROJECTS_WF" "$CHECK_ACTION" "$APPLY_ACTION"; do
  if grep -q -e '--allow-downgrade' -e '--allow-first-change' -e '--skip-gwbk' -e '--allow-destructive' "$f"; then
    die "3 $(basename "$f") passes an allow flag the workflow must never pass"
  fi
done
pass "3a jobs: apply needs check, environment, per-env concurrency without cancel, read-only permissions, no allow flags"

for f in "$PROJECTS_WF" "$CHECK_ACTION" "$APPLY_ACTION"; do
  checkouts="$(grep -cE 'uses: actions/checkout@v7$' "$f")"
  persist="$(grep -cE '^ +persist-credentials: false$' "$f")"
  [ "$checkouts" = "$persist" ] || die "3b $(basename "$(dirname "$f")")/$(basename "$f"): $checkouts checkouts but $persist persist-credentials: false"
done
[ "$(grep -cE 'uses: actions/checkout@v7$' "$PROJECTS_WF")" = 2 ] || die "3b expected 2 checkouts in deploy-projects.yml (the actions checkout of each job)"
for a in "$CHECK_ACTION" "$APPLY_ACTION"; do
  [ "$(grep -cE 'uses: actions/checkout@v7$' "$a")" = 2 ] || die "3b expected 2 checkouts in $(basename "$(dirname "$a")")"
  # shellcheck disable=SC2016
  [ "$(grep -cE '^ +token: \$\{\{ inputs\.targets-token \}\}$' "$a")" = 1 ] || die "3b $(basename "$(dirname "$a")")'s targets checkout does not use targets-token once"
  # shellcheck disable=SC2016
  [ "$(grep -cF 'inputs.targets-token' "$a")" = 2 ] || die "3b targets-token is used outside the targets step env and checkout in $(basename "$(dirname "$a")")"
  [ "$(grep -cF 'path: targets' "$a")" = 1 ] && [ "$(grep -cF 'path: project' "$a")" = 1 ] || die "3b the checkout paths of $(basename "$(dirname "$a")") are not targets and project"
done
if grep -qE '^ +token: ' "$PROJECTS_WF"; then die "3b deploy-projects.yml has a checkout that takes a token"; fi
[ "$(grep -cF 'secrets.IGN_TARGETS_TOKEN' "$PROJECTS_WF")" = 2 ] || die "3b the workflow does not use IGN_TARGETS_TOKEN exactly once per job"
pass "3b every checkout drops its credentials; the targets checkout is read with the token only, outside the project checkout"

# A secret name or value reaching a log line.
for f in "$PROJECTS_WF" "$CHECK_ACTION" "$APPLY_ACTION"; do
  if grep -nE "(echo|printf)[^|>]*(TOKEN|IGN_SSH_KEY|secrets\.)" "$f" | grep -v 'fail "' | grep -q .; then
    grep -nE "(echo|printf)[^|>]*(TOKEN|IGN_SSH_KEY|secrets\.)" "$f" >&2
    die "3c $(basename "$f"): a run step echoes a token, key or secret"
  fi
  if grep -qE '^[[:space:]]*(set -x|set -[a-z]*x)' "$f"; then die "3c $(basename "$f"): a run step enables xtrace"; fi
done
pass "3c no step echoes a token or key, no xtrace"

# Approval-gate secret split. The check job runs before and without the environment
# approval, so it must hold only the read-only IGN_CHECK_* secrets; the apply job
# holds the write-capable ones. Both are static, per-job line-range checks.
check_start="$(grep -n '^  check:$' "$PROJECTS_WF" | cut -d: -f1)"
apply_start="$(grep -n '^  apply:$' "$PROJECTS_WF" | cut -d: -f1)"
[ -n "$check_start" ] && [ -n "$apply_start" ] && [ "$check_start" -lt "$apply_start" ] || die "3d cannot locate the check and apply jobs"
sed -n "${check_start},$((apply_start - 1))p" "$PROJECTS_WF" > "$WORK/job-check.yml"
sed -n "${apply_start},\$p" "$PROJECTS_WF" > "$WORK/job-apply.yml"
[ "$(grep -oE 'secrets\.[A-Z_]+' "$WORK/job-check.yml" | sort -u | tr '\n' ' ')" = 'secrets.IGN_CHECK_SSH_KEY secrets.IGN_CHECK_TOKEN secrets.IGN_TARGETS_TOKEN ' ] \
  || die "3d the check job's secrets are not exactly IGN_CHECK_SSH_KEY, IGN_CHECK_TOKEN and IGN_TARGETS_TOKEN"
[ "$(grep -oE 'secrets\.[A-Z_]+' "$WORK/job-apply.yml" | sort -u | tr '\n' ' ')" = 'secrets.IGN_MASTER_TOKEN secrets.IGN_SSH_KEY secrets.IGN_TARGETS_TOKEN ' ] \
  || die "3d the apply job's secrets are not exactly IGN_MASTER_TOKEN, IGN_SSH_KEY and IGN_TARGETS_TOKEN"
if grep -nE 'secrets\.(IGN_MASTER_TOKEN|IGN_SSH_KEY)\b' "$WORK/job-check.yml" | grep -q .; then
  grep -nE 'secrets\.(IGN_MASTER_TOKEN|IGN_SSH_KEY)\b' "$WORK/job-check.yml" >&2
  die "3d the check job references a write-capable secret (IGN_MASTER_TOKEN or IGN_SSH_KEY)"
fi
if grep -nE 'secrets\.IGN_CHECK_(TOKEN|SSH_KEY)\b' "$WORK/job-apply.yml" | grep -q .; then
  grep -nE 'secrets\.IGN_CHECK_(TOKEN|SSH_KEY)\b' "$WORK/job-apply.yml" >&2
  die "3d the apply job references a check secret (IGN_CHECK_TOKEN or IGN_CHECK_SSH_KEY)"
fi
# shellcheck disable=SC2016
[ "$(grep -cF 'check-token: ${{ secrets.IGN_CHECK_TOKEN }}' "$WORK/job-check.yml")" = 1 ] || die "3d the check job does not pass check-token from IGN_CHECK_TOKEN"
# shellcheck disable=SC2016
[ "$(grep -cF 'check-ssh-key: ${{ secrets.IGN_CHECK_SSH_KEY }}' "$WORK/job-check.yml")" = 1 ] || die "3d the check job does not pass check-ssh-key from IGN_CHECK_SSH_KEY"
# shellcheck disable=SC2016
[ "$(grep -cF 'master-token: ${{ secrets.IGN_MASTER_TOKEN }}' "$WORK/job-apply.yml")" = 1 ] || die "3d the apply job does not pass master-token from IGN_MASTER_TOKEN"
# shellcheck disable=SC2016
[ "$(grep -cF 'ssh-key: ${{ secrets.IGN_SSH_KEY }}' "$WORK/job-apply.yml")" = 1 ] || die "3d the apply job does not pass ssh-key from IGN_SSH_KEY"
# shellcheck disable=SC2016
[ "$(grep -cF 'IGN_MASTER_TOKEN: ${{ secrets.IGN_MASTER_TOKEN }}' "$WORK/job-apply.yml")" = 1 ] || die "3d the apply job does not check the IGN_MASTER_TOKEN secret once"
# shellcheck disable=SC2016
[ "$(grep -cF 'IGN_SSH_KEY: ${{ secrets.IGN_SSH_KEY }}' "$WORK/job-apply.yml")" = 1 ] || die "3d the apply job does not check the IGN_SSH_KEY secret once"
# No fallback from a check secret to a write secret (or back), and no whole-context use
# that would hand every secret to a step. `secrets: inherit` is matched only as a YAML
# key line, so prose that mentions the phrase does not trip it.
if grep -nE 'secrets\.[A-Z_]+ *\|\||\|\| *secrets\.|secrets\[|toJSON\(secrets|\$\{\{ *secrets *\}\}|^[[:space:]]*secrets:[[:space:]]*inherit[[:space:]]*$' "$PROJECTS_WF" | grep -q .; then
  die "3d a secret expression falls back to another secret or uses the whole secrets context"
fi
pass "3d the check job references only IGN_CHECK_TOKEN, IGN_CHECK_SSH_KEY and the targets token, the apply job only IGN_MASTER_TOKEN and IGN_SSH_KEY, with no fallback between them"

# The check action's interface: no write-capable input, no secrets context.
inputs_of() { # inputs_of <action.yml>: the declared input names, sorted, space separated
  awk '/^inputs:/ { on = 1; next } /^outputs:/ { on = 0 } on && /^  [a-z][a-z0-9-]*:$/ { sub(/^  /, ""); sub(/:$/, ""); print }' "$1" | sort | tr '\n' ' '
}
want_check_inputs="$(printf '%s\n' env ign-version release master-profile master-url check-token targets-token repo-dir targets-repository targets-ref targets-dir ssl-verify verify-attestation ssh-key-env check-ssh-key | sort | tr '\n' ' ')"
[ "$(inputs_of "$CHECK_ACTION")" = "$want_check_inputs" ] || die "3d2 the check action's inputs are '$(inputs_of "$CHECK_ACTION")', not '$want_check_inputs'"
if grep -qF 'secrets.' "$CHECK_ACTION"; then die "3d2 the check action references the secrets context"; fi
# shellcheck disable=SC2016
[ "$(grep -cF 'IGN_MASTER_TOKEN: ${{ inputs.check-token }}' "$CHECK_ACTION")" = 1 ] || die "3d2 the check action does not take its gateway token from check-token"
# shellcheck disable=SC2016
[ "$(grep -cF 'IGN_SSH_KEY: ${{ inputs.check-ssh-key }}' "$CHECK_ACTION")" = 1 ] || die "3d2 the check action does not take its ssh key from check-ssh-key"
[ "$(grep -cF 'inputs.check-token' "$CHECK_ACTION")" = 1 ] || die "3d2 check-token is used more than once in the check action"
pass "3d2 the check action declares no write-capable input, holds no secrets reference and takes its token and key from check-token and check-ssh-key"

# The apply action's interface: the checked hash and commit are required, no check input, no secrets context.
want_apply_inputs="$(printf '%s\n' env ign-version release master-profile master-url artifact-sha256 targets-sha repo-dir targets-repository targets-dir designers-timeout ssl-verify ssh-key-env master-token ssh-key artifact-suffix verify-attestation targets-token | sort | tr '\n' ' ')"
[ "$(inputs_of "$APPLY_ACTION")" = "$want_apply_inputs" ] || die "3d3 the apply action's inputs are '$(inputs_of "$APPLY_ACTION")', not '$want_apply_inputs'"
for inp in artifact-sha256 targets-sha; do
  awk -v want="$inp" '/^inputs:/ { on = 1; next } /^outputs:/ { on = 0 } on && $0 == "  " want ":" { hit = 1; next } on && hit && /^  [a-z]/ { hit = 0 } on && hit && /required: true/ { ok = 1 } END { exit ok ? 0 : 1 }' "$APPLY_ACTION" \
    || die "3d3 the apply action's $inp input is not required: true"
done
if grep -qF 'secrets.' "$APPLY_ACTION"; then die "3d3 the apply action references the secrets context"; fi
if grep -qF 'inputs.targets-ref' "$APPLY_ACTION"; then die "3d3 the apply action reads a targets-ref input"; fi
if grep -qF 'check-token' "$APPLY_ACTION"; then die "3d3 the apply action mentions check-token"; fi
first_body="$(grep -m1 -oE '>>> body:[A-Za-z0-9-]+' "$APPLY_ACTION")"
[ "$first_body" = '>>> body:apply-handoff' ] || die "3d3 the apply action's first step is not the handoff check ($first_body)"
# shellcheck disable=SC2016
[ "$(grep -cE '^ +ref: \$\{\{ inputs\.targets-sha \}\}$' "$APPLY_ACTION")" = 1 ] || die "3d3 the apply action does not read the targets at targets-sha once"
pass "3d3 the apply action requires artifact-sha256 and targets-sha, holds no secrets reference, no targets-ref and no check input, and runs the handoff check first"

# Targets commit pin (WR-06). The approver reviews a check that read the targets at one
# moment; the apply job must read the very same commit, not whatever the ref is later.
# shellcheck disable=SC2016
[ "$(grep -cF 'value: ${{ steps.targets_sha.outputs.sha }}' "$CHECK_ACTION")" = 1 ] || die "3e the check action does not expose the targets commit as an output"
# shellcheck disable=SC2016
[ "$(grep -cF 'value: ${{ steps.report.outputs.artifact_sha256 }}' "$CHECK_ACTION")" = 1 ] || die "3e the check action does not expose artifact_sha256 as an output"
# shellcheck disable=SC2016
[ "$(grep -cE '^ +ref: \$\{\{ inputs\.targets-ref \}\}$' "$CHECK_ACTION")" = 1 ] || die "3e the check action does not read the targets at inputs.targets-ref"
grep -qE '^  (artifact_sha256|targets_sha):$' "$CHECK_ACTION" || die "3e the check action declares no artifact_sha256 or targets_sha output"
# shellcheck disable=SC2016
for needle in 'artifact_sha256: ${{ steps.check.outputs.artifact_sha256 }}' 'targets_sha: ${{ steps.check.outputs.targets_sha }}'; do
  [ "$(grep -cF "$needle" "$WORK/job-check.yml")" = 1 ] || die "3e the check job does not map: $needle"
done
if grep -qE '^ +ref: \$\{\{ inputs\.targets-ref \}\}$' "$PROJECTS_WF"; then
  die "3e deploy-projects.yml reads the targets at the moving inputs.targets-ref"
fi
# shellcheck disable=SC2016
for needle in 'artifact-sha256: ${{ needs.check.outputs.artifact_sha256 }}' 'targets-sha: ${{ needs.check.outputs.targets_sha }}'; do
  [ "$(grep -cF "$needle" "$WORK/job-apply.yml")" = 1 ] || die "3e the apply job does not pass: $needle"
done
grep -qF '>>> body:targets-sha' "$CHECK_ACTION" || die "3e the check action has no step that pins the targets commit"
grep -qF '>>> body:verify-targets-sha' "$APPLY_ACTION" || die "3e the apply action has no step that verifies the targets commit"
if grep -qF 'needs.check.outputs.targets_sha' "$WORK/job-check.yml"; then die "3e the check job reads its own output through needs"; fi
pass "3e the check action pins the targets commit, the apply job passes exactly that commit and the apply action verifies it"

autocrlf_checks "$PROJECTS_WF"
autocrlf_checks "$CHECK_ACTION"
autocrlf_checks "$APPLY_ACTION"
pass "3f every checkout and every git or ign step disables core.autocrlf through the git environment config"

# The workflow runs the actions of its own commit: located, checked out at
# job.workflow_sha, HEAD verified, then used.
ln_ref="$(grep -n '>>> body:actions-ref' "$WORK/job-check.yml" | head -1 | cut -d: -f1)"
ln_co="$(grep -n 'name: Checkout the ign deploy actions' "$WORK/job-check.yml" | head -1 | cut -d: -f1)"
ln_ver="$(grep -n '>>> body:verify-actions-sha' "$WORK/job-check.yml" | head -1 | cut -d: -f1)"
ln_use="$(grep -n 'uses: ./.ign-actions/.github/actions/deploy-projects-check' "$WORK/job-check.yml" | head -1 | cut -d: -f1)"
[ -n "$ln_ref" ] && [ -n "$ln_co" ] && [ -n "$ln_ver" ] && [ -n "$ln_use" ] || die "3g the check job lacks the locate, checkout, verify or use step for the actions"
[ "$ln_ref" -lt "$ln_co" ] && [ "$ln_co" -lt "$ln_ver" ] && [ "$ln_ver" -lt "$ln_use" ] || die "3g the check job's actions steps are out of order"
# shellcheck disable=SC2016
for needle in 'repository: ${{ job.workflow_repository }}' 'ref: ${{ job.workflow_sha }}' 'path: .ign-actions' 'sparse-checkout: .github/actions'; do
  [ "$(grep -cF -e "$needle" "$WORK/job-check.yml")" = 1 ] || die "3g the check job's actions checkout lacks: $needle"
done
pass "3g the check job locates, checks out at job.workflow_sha, verifies and then runs the check action"

ln_sec="$(grep -n '>>> body:apply-secrets' "$WORK/job-apply.yml" | head -1 | cut -d: -f1)"
ln_ref="$(grep -n '>>> body:actions-ref' "$WORK/job-apply.yml" | head -1 | cut -d: -f1)"
ln_co="$(grep -n 'name: Checkout the ign deploy actions' "$WORK/job-apply.yml" | head -1 | cut -d: -f1)"
ln_ver="$(grep -n '>>> body:verify-actions-sha' "$WORK/job-apply.yml" | head -1 | cut -d: -f1)"
ln_use="$(grep -n 'uses: ./.ign-actions/.github/actions/deploy-projects-apply' "$WORK/job-apply.yml" | head -1 | cut -d: -f1)"
[ -n "$ln_sec" ] && [ -n "$ln_ref" ] && [ -n "$ln_co" ] && [ -n "$ln_ver" ] && [ -n "$ln_use" ] || die "3g2 the apply job lacks the secrets, locate, checkout, verify or use step"
[ "$ln_sec" -lt "$ln_ref" ] && [ "$ln_ref" -lt "$ln_co" ] && [ "$ln_co" -lt "$ln_ver" ] && [ "$ln_ver" -lt "$ln_use" ] || die "3g2 the apply job's steps are out of order"
# shellcheck disable=SC2016
for needle in 'repository: ${{ job.workflow_repository }}' 'ref: ${{ job.workflow_sha }}' 'path: .ign-actions' 'sparse-checkout: .github/actions'; do
  [ "$(grep -cF -e "$needle" "$WORK/job-apply.yml")" = 1 ] || die "3g2 the apply job's actions checkout lacks: $needle"
done
pass "3g2 the apply job checks the secrets first, then locates, checks out at job.workflow_sha, verifies and runs the apply action"

# WR-04: opt-in release attestation. The input exists on both actions with a false default;
# the install step receives it (and a github.token only when it is exactly 'true') through env,
# never through the body; the gate is one region copied byte for byte into all four install steps.
for a in "$CHECK_ACTION" "$APPLY_ACTION"; do
  an="$(basename "$(dirname "$a")")"
  awk -v q="'" '/^inputs:/ { on = 1; next } /^outputs:/ { on = 0 } on && $0 == "  verify-attestation:" { hit = 1; next } on && hit && /^  [a-z]/ { hit = 0 } on && hit && /required: false$/ { r = 1 } on && hit && index($0, "default: " q "false" q) { d = 1 } END { exit (r && d) ? 0 : 1 }' "$a" \
    || die "3h $an: verify-attestation is not an input with required: false and default 'false'"
  awk '/^    - name: Install ign$/ { on = 1; print; next } on && /^    - name:/ { on = 0 } on' "$a" > "$WORK/install-step-$an.yml"
  [ -s "$WORK/install-step-$an.yml" ] || die "3h $an: no Install ign step"
  # shellcheck disable=SC2016
  [ "$(grep -cxF '        IGN_VERIFY_ATTESTATION: ${{ inputs.verify-attestation }}' "$WORK/install-step-$an.yml")" = 1 ] \
    && [ "$(grep -cE '^[[:space:]]*IGN_VERIFY_ATTESTATION:' "$a")" = 1 ] \
    || die "3h $an: IGN_VERIFY_ATTESTATION is not set exactly once, in the Install ign step"
  # shellcheck disable=SC2016
  [ "$(grep -cxF "        GH_TOKEN: \${{ inputs.verify-attestation == 'true' && github.token || '' }}" "$WORK/install-step-$an.yml")" = 1 ] \
    && [ "$(grep -cE '^[[:space:]]*GH_TOKEN:' "$a")" = 1 ] \
    && [ "$(grep -cF 'github.token' "$a")" = 1 ] \
    || die "3h $an: GH_TOKEN is not set exactly once, gated on verify-attestation == 'true', in the Install ign step"
done
for f in "$PLAN_WF" "$APPLY_WF"; do
  if grep -qE '^[[:space:]]*(IGN_VERIFY_ATTESTATION|GH_TOKEN):' "$f"; then die "3h $(basename "$f") sets IGN_VERIFY_ATTESTATION or GH_TOKEN in a step env"; fi
done
GH_VERIFY_LITERAL='gh attestation verify "$dir/$asset" --repo TheThoughtagen/ignition-cli --signer-workflow TheThoughtagen/ignition-cli/.github/workflows/release.yml'
for f in "$PLAN_WF" "$APPLY_WF" "$CHECK_ACTION" "$APPLY_ACTION"; do
  extract_first "$f" shared:install-ign > "$WORK/install-region.sh"
  [ "$(grep -cF -e "$GH_VERIFY_LITERAL" "$WORK/install-region.sh")" = 1 ] || die "3h $(basename "$f"): shared:install-ign lacks the gh attestation verify call exactly once"
  ln_sum="$(grep -n 'checksum mismatch' "$WORK/install-region.sh" | head -1 | cut -d: -f1)"
  ln_gh="$(grep -nF -e "$GH_VERIFY_LITERAL" "$WORK/install-region.sh" | head -1 | cut -d: -f1)"
  ln_unset="$(grep -n '^unset GH_TOKEN$' "$WORK/install-region.sh" | head -1 | cut -d: -f1)"
  ln_ext="$(grep -n '^mkdir -p "\$dir/x"$' "$WORK/install-region.sh" | head -1 | cut -d: -f1)"
  [ -n "$ln_sum" ] && [ -n "$ln_gh" ] && [ -n "$ln_unset" ] && [ -n "$ln_ext" ] \
    && [ "$ln_sum" -lt "$ln_gh" ] && [ "$ln_gh" -lt "$ln_unset" ] && [ "$ln_unset" -lt "$ln_ext" ] \
    || die "3h $(basename "$f"): the attestation gate is not after the sha256 check and before the token is dropped and the asset is extracted"
done
pass "3h verify-attestation: an input defaulting to false on both actions, env-only (IGN_VERIFY_ATTESTATION, gated GH_TOKEN) in the Install ign step, one gh call in each of the four shared install regions, ordered after sha256 and before extraction"

# ---------------------------------------------------------------- 4. targets-args step
extract_first "$CHECK_ACTION" body:targets-args > "$WORK/targets-args.sh"
T="$WORK/targets"
/bin/rm -rf "$T"
mkdir -p "$T"
targets_step() { # targets_step <name> VAR=value ... ; runs in $T/<name>
  local name="$1"
  shift
  mkdir -p "$T/$name"
  : > "$T/$name/github_env"
  (
    cd "$T/$name"
    env -i PATH="$PATH" GITHUB_ENV="$T/$name/github_env" \
      IGN_TARGETS_REPOSITORY=WhiskeyHouse/whk-onprem-infra-iac IGN_TARGETS_REF=main IGN_TARGETS_DIR_INPUT=ignition \
      IGN_TARGETS_CHECKOUT=targets IGN_TARGETS_REQUIRED=true IGN_TARGETS_TOKEN=tok-SECRETVALUE-123 \
      "$@" bash "$WORK/targets-args.sh"
  ) > "$T/$name/out.log" 2>&1
}
targets_step ok || { cat "$T/ok/out.log" >&2; die "4a the default inputs were refused"; }
[ "$(cat "$T/ok/github_env")" = "IGN_TARGETS_DIR=targets/ignition" ] || die "4a unexpected GITHUB_ENV: $(cat "$T/ok/github_env")"
if grep -rq 'SECRETVALUE' "$T/ok"; then die "4a the targets token reached a log or the environment file"; fi
pass "4a default inputs resolve to IGN_TARGETS_DIR=targets/ignition and never print the token"

refused() { # refused <name> <expected text> VAR=value ...
  local name="$1" want="$2"
  shift 2
  if targets_step "$name" "$@"; then die "4 case $name was accepted"; fi
  grep -qF -e "$want" "$T/$name/out.log" || { cat "$T/$name/out.log" >&2; die "4 case $name: no message containing: $want"; }
  [ ! -s "$T/$name/github_env" ] || die "4 case $name wrote GITHUB_ENV"
  if grep -rq 'SECRETVALUE' "$T/$name"; then die "4 case $name printed the token"; fi
}
refused dotdot 'targets-dir' IGN_TARGETS_DIR_INPUT=../x
refused dotdot2 'targets-dir' IGN_TARGETS_DIR_INPUT=a/../b
refused abs 'targets-dir' IGN_TARGETS_DIR_INPUT=/etc
refused drive 'targets-dir' IGN_TARGETS_DIR_INPUT=C:/x
refused backslash 'targets-dir' 'IGN_TARGETS_DIR_INPUT=a\b'
refused dash 'targets-dir' IGN_TARGETS_DIR_INPUT=-rf
refused dot 'targets-dir' IGN_TARGETS_DIR_INPUT=.
refused emptydir 'targets-dir' IGN_TARGETS_DIR_INPUT=
refused badrepo 'targets-repository' IGN_TARGETS_REPOSITORY=not-a-repo
refused badrepo2 'targets-repository' 'IGN_TARGETS_REPOSITORY=a/b c'
refused badref 'targets-ref' 'IGN_TARGETS_REF=-x'
refused notoken 'IGN_TARGETS_TOKEN' IGN_TARGETS_TOKEN=
refused emptyrepo 'targets-repository must not be empty' IGN_TARGETS_REPOSITORY=
pass "4b traversal, absolute, drive, backslash, dash and empty targets-dir; bad repository and ref; a missing token; an empty repository; all refused before any checkout"

targets_step optional IGN_TARGETS_REPOSITORY= IGN_TARGETS_REQUIRED=false IGN_TARGETS_TOKEN= \
  || { cat "$T/optional/out.log" >&2; die "4c an optional empty repository was refused"; }
[ ! -s "$T/optional/github_env" ] || die "4c an optional empty repository set IGN_TARGETS_DIR"
pass "4c an optional empty targets-repository sets nothing"

# The pin and verify bodies, run against a real git checkout (WR-06).
extract_first "$CHECK_ACTION" body:targets-sha > "$WORK/targets-sha.sh"
extract_first "$APPLY_ACTION" body:verify-targets-sha > "$WORK/verify-targets-sha.sh"
PIN="$WORK/pin"
/bin/rm -rf "$PIN"
mkdir -p "$PIN/work/targets"
git -C "$PIN/work/targets" init -q
git -C "$PIN/work/targets" -c user.name=harness -c user.email=harness@example.invalid commit -q --allow-empty -m first
FIRST_SHA="$(git -C "$PIN/work/targets" rev-parse HEAD)"
: > "$PIN/out"
( cd "$PIN/work" && env -i PATH="$PATH" GITHUB_OUTPUT="$PIN/out" bash "$WORK/targets-sha.sh" ) > "$PIN/pin.log" 2>&1 \
  || { cat "$PIN/pin.log" >&2; die "4d the pin step failed on a git checkout"; }
[ "$(cat "$PIN/out")" = "sha=$FIRST_SHA" ] || die "4d the pin step wrote '$(cat "$PIN/out")', not sha=$FIRST_SHA"
mkdir -p "$PIN/nogit"
: > "$PIN/out2"
if ( cd "$PIN/nogit" && env -i PATH="$PATH" GITHUB_OUTPUT="$PIN/out2" bash "$WORK/targets-sha.sh" ) > "$PIN/pin2.log" 2>&1; then
  die "4d the pin step passed without a targets checkout"
fi
[ ! -s "$PIN/out2" ] || die "4d the pin step wrote an output without a targets checkout"
verify_pin() { # verify_pin <sha>
  ( cd "$PIN/work" && env -i PATH="$PATH" IGN_CHECKED_TARGETS_SHA="$1" bash "$WORK/verify-targets-sha.sh" ) > "$PIN/verify.log" 2>&1
}
verify_pin "$FIRST_SHA" || { cat "$PIN/verify.log" >&2; die "4d the verify step refused the checked commit"; }
# The targets branch moves after the check: the checkout is at a newer commit.
git -C "$PIN/work/targets" -c user.name=harness -c user.email=harness@example.invalid commit -q --allow-empty -m second
if verify_pin "$FIRST_SHA"; then die "4d the verify step passed although the targets moved after the check"; fi
grep -q 'not the commit the check read' "$PIN/verify.log" || die "4d the verify refusal does not say the commit moved"
for bad in '' main "$(printf '%s' "$FIRST_SHA" | tr 'a-f' 'A-F')" "${FIRST_SHA:0:12}"; do
  if verify_pin "$bad"; then die "4d the verify step accepted '$bad' as the checked commit"; fi
done
pass "4d the pin step records the targets commit and the verify step refuses a moved, empty, abbreviated or non-commit value"

# G-25-9, behavioural. Git for Windows sets core.autocrlf=true in the system gitconfig, and a
# checkout honours it. The control proves the config rewrites a checkout; the fixed clone
# takes the three variables from the check action's own targets checkout step.
CR="$WORK/crlf"
/bin/rm -rf "$CR"
mkdir -p "$CR/src/targets"
printf 'schema_version: 1\ngateway: x\n' > "$CR/src/targets/dev.yaml"
git -C "$CR/src" init -q
git -C "$CR/src" add -A
git -C "$CR/src" -c user.name=harness -c user.email=harness@example.invalid commit -q -m lf
printf '[core]\n\tautocrlf = true\n' > "$CR/gitconfig"
GIT_CONFIG_GLOBAL="$CR/gitconfig" GIT_CONFIG_NOSYSTEM=1 git clone -q "$CR/src" "$CR/control" > /dev/null 2>&1 || die "4e the control clone failed"
grep -q "$(printf '\r')" "$CR/control/targets/dev.yaml" || die "4e the control clone has no CR, so the autocrlf=true config proves nothing"
step_value() { # step_value <step name> <KEY>: the value of a step env key in the check action
  awk -v name="$1" -v key="$2" '
    /^    - name:/ { on = (index($0, "- name: " name) > 0) }
    on && index($0, key ": ") { v = $0; sub(/^[^:]*: /, "", v); gsub(/\047/, "", v); print v; exit }
  ' "$CHECK_ACTION"
}
GC_COUNT="$(step_value 'Checkout targets' GIT_CONFIG_COUNT)"
GC_KEY="$(step_value 'Checkout targets' GIT_CONFIG_KEY_0)"
GC_VAL="$(step_value 'Checkout targets' GIT_CONFIG_VALUE_0)"
[ "$GC_COUNT" = 1 ] && [ "$GC_KEY" = core.autocrlf ] && [ "$GC_VAL" = false ] || die "4e could not read the git config variables of the check action's targets checkout ($GC_COUNT, $GC_KEY, $GC_VAL)"
GIT_CONFIG_GLOBAL="$CR/gitconfig" GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_COUNT="$GC_COUNT" GIT_CONFIG_KEY_0="$GC_KEY" GIT_CONFIG_VALUE_0="$GC_VAL" \
  git clone -q "$CR/src" "$CR/fixed" > /dev/null 2>&1 || die "4e the fixed clone failed"
if grep -q "$(printf '\r')" "$CR/fixed/targets/dev.yaml"; then die "4e the clone still has CR with the git config variables set"; fi
pass "4e the action's git config variables keep a checkout byte-identical under core.autocrlf=true (control clone has CR, fixed clone has none)"

# The actions-ref and verify-actions-sha bodies of deploy-projects.yml (T-25-86).
extract_first "$PROJECTS_WF" body:actions-ref > "$WORK/actions-ref.sh"
extract_first "$PROJECTS_WF" body:verify-actions-sha > "$WORK/verify-actions-sha.sh"
GOOD_SHA="$(printf 'a1b2c3d4e5%.0s' 1 2 3 4)"
actions_ref() { # actions_ref <repository> <sha>
  ( env -i PATH="$PATH" IGN_WORKFLOW_REPOSITORY="$1" IGN_WORKFLOW_SHA="$2" bash "$WORK/actions-ref.sh" ) > "$WORK/actions-ref.log" 2>&1
}
actions_ref TheThoughtagen/ignition-cli "$GOOD_SHA" || { cat "$WORK/actions-ref.log" >&2; die "4h actions-ref refused a valid repository and commit"; }
grep -qF "ign deploy actions: TheThoughtagen/ignition-cli@$GOOD_SHA" "$WORK/actions-ref.log" || die "4h actions-ref did not print the repository and commit"
for bad in '' "${GOOD_SHA:0:12}" "$(printf '%s' "$GOOD_SHA" | tr 'a-f' 'A-F')"; do
  if actions_ref TheThoughtagen/ignition-cli "$bad"; then die "4h actions-ref accepted the commit '$bad'"; fi
  grep -qF 'job.workflow_sha' "$WORK/actions-ref.log" || die "4h the refusal of '$bad' does not name job.workflow_sha"
done
for bad in '' not-a-repo 'a/b c' '/x'; do
  if actions_ref "$bad" "$GOOD_SHA"; then die "4h actions-ref accepted the repository '$bad'"; fi
  grep -qF 'job.workflow_repository' "$WORK/actions-ref.log" || die "4h the refusal of '$bad' does not name job.workflow_repository"
done
AV="$WORK/actions-verify"
/bin/rm -rf "$AV"
mkdir -p "$AV/.ign-actions"
git -C "$AV/.ign-actions" init -q
git -C "$AV/.ign-actions" -c user.name=harness -c user.email=harness@example.invalid commit -q --allow-empty -m first
ACT_SHA="$(git -C "$AV/.ign-actions" rev-parse HEAD)"
verify_actions() { # verify_actions <sha>
  ( cd "$AV" && env -i PATH="$PATH" IGN_WORKFLOW_SHA="$1" bash "$WORK/verify-actions-sha.sh" ) > "$AV/verify.log" 2>&1
}
verify_actions "$ACT_SHA" || { cat "$AV/verify.log" >&2; die "4h verify-actions-sha refused the matching HEAD"; }
if verify_actions "$GOOD_SHA"; then die "4h verify-actions-sha accepted another commit"; fi
grep -qF 'not the workflow' "$AV/verify.log" || die "4h the verify refusal does not name the workflow's own commit"
pass "4h actions-ref accepts owner/name with a 40 hex commit and refuses an empty, abbreviated or uppercase commit and a malformed repository; verify-actions-sha refuses another commit"

# The apply-handoff body: nothing runs before the checked hash and commit and the approving
# environment's secrets are present (T-25-85), and no value is ever printed.
extract_first "$APPLY_ACTION" body:apply-handoff > "$WORK/apply-handoff.sh"
H_HASH="$(printf 'ab%.0s' $(seq 1 32))"
H_TSHA="$(printf 'cd%.0s' $(seq 1 20))"
handoff() { # handoff <name> <expected text or ''> VAR=value ... ; the valid values are the defaults
  local name="$1" want="$2" rc=0
  shift 2
  ( env -i PATH="$PATH" IGN_ARTIFACT_SHA256="$H_HASH" IGN_CHECKED_TARGETS_SHA="$H_TSHA" IGN_MASTER_TOKEN=tok-SECRETVALUE \
      IGN_SSH_KEY= IGN_SSH_KEY_ENV= "$@" bash "$WORK/apply-handoff.sh" ) > "$WORK/handoff-$name.log" 2>&1 || rc=$?
  if grep -q 'SECRETVALUE' "$WORK/handoff-$name.log"; then die "4f case $name printed the token"; fi
  if [ -z "$want" ]; then
    [ "$rc" = 0 ] || { cat "$WORK/handoff-$name.log" >&2; die "4f case $name was refused"; }
  else
    [ "$rc" != 0 ] || die "4f case $name was accepted"
    grep -qF -e "$want" "$WORK/handoff-$name.log" || { cat "$WORK/handoff-$name.log" >&2; die "4f case $name: no message containing: $want"; }
  fi
}
handoff ok ''
grep -qF "checked artifact_sha256 $H_HASH at targets commit $H_TSHA" "$WORK/handoff-ok.log" || die "4f the handoff did not print the hash and commit"
handoff ssh '' IGN_SSH_KEY_ENV=DEPLOY_SSH_KEY IGN_SSH_KEY=key-value
handoff art-empty artifact-sha256 IGN_ARTIFACT_SHA256=
handoff art-upper artifact-sha256 "IGN_ARTIFACT_SHA256=$(printf '%s' "$H_HASH" | tr 'a-f' 'A-F')"
handoff art-short artifact-sha256 "IGN_ARTIFACT_SHA256=${H_HASH:0:63}"
handoff tsha-empty targets-sha IGN_CHECKED_TARGETS_SHA=
handoff tsha-short targets-sha "IGN_CHECKED_TARGETS_SHA=${H_TSHA:0:12}"
handoff tok-empty master-token IGN_MASTER_TOKEN=
grep -qF 'environment:' "$WORK/handoff-tok-empty.log" || die "4f the empty-token refusal does not name the approving environment:"
handoff key-empty ssh-key IGN_SSH_KEY_ENV=DEPLOY_SSH_KEY
pass "4f the apply handoff accepts the checked hash and commit and refuses a malformed hash or commit, an empty master token and a missing ssh key, naming the input and never printing a value"

# The artifact-name body (WR-03): the deploy-records artifact name carries an optional caller
# suffix, so two applies to one env in one workflow run do not collide on upload.
extract_first "$APPLY_ACTION" body:artifact-name > "$WORK/artifact-name.sh"
artifact_name() { # artifact_name <name> <expected output or ''> VAR=value ... ; prints nothing on a refusal
  local name="$1" want="$2" rc=0
  shift 2
  : > "$WORK/artifact-name-$name.out"
  ( env -i PATH="$PATH" IGN_ENV=dev IGN_ARTIFACT_SUFFIX= IGN_RUN_ID=42 IGN_RUN_ATTEMPT=3 \
      GITHUB_OUTPUT="$WORK/artifact-name-$name.out" "$@" bash "$WORK/artifact-name.sh" ) > "$WORK/artifact-name-$name.log" 2>&1 || rc=$?
  if [ -n "$want" ]; then
    [ "$rc" = 0 ] || { cat "$WORK/artifact-name-$name.log" >&2; die "4i case $name was refused"; }
    [ "$(cat "$WORK/artifact-name-$name.out")" = "name=$want" ] || die "4i case $name wrote '$(cat "$WORK/artifact-name-$name.out")', not name=$want"
  else
    [ "$rc" != 0 ] || die "4i case $name was accepted"
    [ ! -s "$WORK/artifact-name-$name.out" ] || die "4i case $name wrote an output although it was refused"
    grep -qF 'artifact-suffix' "$WORK/artifact-name-$name.log" || die "4i case $name: the refusal does not name artifact-suffix"
  fi
}
artifact_name plain ign-deploy-projects-dev-42-3
artifact_name suffixed ign-deploy-projects-dev-proj-a-42-3 IGN_ARTIFACT_SUFFIX=proj-a
artifact_name other ign-deploy-projects-dev-b.2_x-42-3 IGN_ARTIFACT_SUFFIX=b.2_x
artifact_name slash '' IGN_ARTIFACT_SUFFIX=a/b
artifact_name space '' 'IGN_ARTIFACT_SUFFIX=a b'
artifact_name lead '' IGN_ARTIFACT_SUFFIX=-a
artifact_name colon '' IGN_ARTIFACT_SUFFIX=a:b
artifact_name long '' "IGN_ARTIFACT_SUFFIX=$(printf 'a%.0s' $(seq 1 41))"
[ "$(grep -c 'steps.artifact.outputs.name' "$APPLY_ACTION")" = 2 ] || die "4i the upload and the report do not both take the artifact name from the name step"
pass "4i the artifact name carries the optional suffix and refuses a suffix that is not letters, digits, dot, underscore and hyphen"

# WR-04, behavioural: the shared install body against a hermetic release (a curl shim that never
# touches the network, a gh stub that records its argv), compared with the install body of commit
# 408f65b, the last commit before attestation existed.
BASELINE=408f65b # the apply action's install body before verify-attestation
git show "$BASELINE:.github/actions/deploy-projects-apply/action.yml" > "$WORK/baseline-apply-action.yml"
extract_first "$WORK/baseline-apply-action.yml" body:install-ign > "$WORK/install-baseline.sh"
extract_first "$APPLY_ACTION" body:install-ign > "$WORK/install-new.sh"
[ -s "$WORK/install-baseline.sh" ] && [ -s "$WORK/install-new.sh" ] || die "4j could not extract the install bodies"
AT="$WORK/attest"
AT_TAG=v9.9.9
AT_ASSET="ign-$AT_TAG-x86_64-unknown-linux-gnu.tar.gz"
/bin/rm -rf "$AT"
mkdir -p "$AT/rel" "$AT/curlbin" "$AT/ghbin" "$AT/nogh" "$AT/src/ign-$AT_TAG-x86_64-unknown-linux-gnu"
printf '%s\n' '#!/bin/sh' 'echo "ran GH_TOKEN=${GH_TOKEN-unset}" >> "$IGN_FAKE_LOG"' 'echo "ign 9.9.9"' > "$AT/src/ign-$AT_TAG-x86_64-unknown-linux-gnu/ign"
chmod +x "$AT/src/ign-$AT_TAG-x86_64-unknown-linux-gnu/ign"
COPYFILE_DISABLE=1 tar -czf "$AT/rel/$AT_ASSET" -C "$AT/src" "ign-$AT_TAG-x86_64-unknown-linux-gnu"
if command -v sha256sum > /dev/null 2>&1; then
  (cd "$AT/rel" && sha256sum "$AT_ASSET" > "$AT_ASSET.sha256")
else
  (cd "$AT/rel" && shasum -a 256 "$AT_ASSET" > "$AT_ASSET.sha256")
fi
cat > "$AT/curlbin/curl" << 'SH'
#!/bin/sh
echo "$*" >> "$CURL_LOG"
out=""
prev=""
url=""
for a in "$@"; do
  if [ "$prev" = "-o" ]; then out="$a"; fi
  prev="$a"
  url="$a"
done
f="$FIXTURE_DIR/$(basename "$url")"
[ -f "$f" ] || exit 22
cp "$f" "$out"
SH
cat > "$AT/ghbin/gh" << 'SH'
#!/bin/sh
echo call >> "$GH_CALLS"
for a in "$@"; do printf '%s\n' "$a"; done > "$GH_ARGS"
{
  if [ -n "${GH_TOKEN:-}" ]; then echo token=set; else echo token=empty; fi
  if [ -e "$RUNNER_TEMP/ign-install/x" ]; then echo extracted=yes; else echo extracted=no; fi
} > "$GH_STATE"
exit "${GH_STUB_RC:-0}"
SH
chmod +x "$AT/curlbin/curl" "$AT/ghbin/gh"
BASH_BIN="$(command -v bash)"
AT_ROOT="$AT/root" # fixed, so RUNNER_TEMP paths are the same in every run
run_install() { # run_install <name> <body-file> [VAR=value ...]; results in $AT/out/<name>/
  local name="$1" body="$2" rc=0 o="$AT/out/$1"
  shift 2
  /bin/rm -rf "$AT_ROOT" "$o"
  mkdir -p "$AT_ROOT/temp" "$o"
  : > "$AT_ROOT/github_env"
  : > "$AT_ROOT/curl.log"
  : > "$AT_ROOT/gh.calls"
  : > "$AT_ROOT/gh.args"
  : > "$AT_ROOT/gh.state"
  : > "$AT_ROOT/fake.log"
  ( cd "$AT_ROOT" && env -i PATH="$AT/curlbin:$AT/ghbin:$PATH" RUNNER_OS=Linux RUNNER_ARCH=X64 RUNNER_TEMP="$AT_ROOT/temp" \
      GITHUB_ENV="$AT_ROOT/github_env" IGN_VERSION="$AT_TAG" IGN_RELEASE_BASE=https://example.invalid/releases/download \
      CURL_LOG="$AT_ROOT/curl.log" FIXTURE_DIR="$AT/rel" GH_CALLS="$AT_ROOT/gh.calls" GH_ARGS="$AT_ROOT/gh.args" \
      GH_STATE="$AT_ROOT/gh.state" IGN_FAKE_LOG="$AT_ROOT/fake.log" "$@" "$BASH_BIN" "$body" ) > "$o/stdout" 2> "$o/stderr" || rc=$?
  echo "$rc" > "$o/rc"
  cp "$AT_ROOT/github_env" "$o/github_env"
  cp "$AT_ROOT/curl.log" "$o/curl.log"
  cp "$AT_ROOT/gh.calls" "$o/gh.calls"
  cp "$AT_ROOT/gh.args" "$o/gh.args"
  cp "$AT_ROOT/gh.state" "$o/gh.state"
  cp "$AT_ROOT/fake.log" "$o/fake.log"
  if [ -e "$AT_ROOT/temp/ign-install/x" ]; then echo yes > "$o/extracted"; else echo no > "$o/extracted"; fi
}
at_out() { echo "$AT/out/$1/$2"; } # at_out <name> <file>

# 4j-baseline: false, and unset, behave exactly as before attestation existed.
run_install base "$WORK/install-baseline.sh"
[ "$(cat "$(at_out base rc)")" = 0 ] || { cat "$(at_out base stdout)" "$(at_out base stderr)" >&2; die "4j the baseline install body failed on the fixture release"; }
grep -qF 'ign 9.9.9' "$(at_out base stdout)" || die "4j the baseline install did not run the fake ign"
for variant in false-empty-token unset; do
  case "$variant" in
    false-empty-token) run_install "new-$variant" "$WORK/install-new.sh" IGN_VERIFY_ATTESTATION=false GH_TOKEN= ;;
    unset) run_install "new-$variant" "$WORK/install-new.sh" ;;
  esac
  for f in rc stdout stderr github_env curl.log; do
    cmp -s "$(at_out base "$f")" "$(at_out "new-$variant" "$f")" || die "4j verify-attestation $variant: $f differs from the 408f65b install body"
  done
  [ ! -s "$(at_out "new-$variant" gh.calls)" ] || die "4j verify-attestation $variant: gh was invoked"
done

# 4j-true: one gh call, exact argv, before extraction, token seen by gh but never by the binary.
run_install true "$WORK/install-new.sh" IGN_VERIFY_ATTESTATION=true GH_TOKEN=tok-SECRETVALUE
[ "$(cat "$(at_out true rc)")" = 0 ] || { cat "$(at_out true stdout)" "$(at_out true stderr)" >&2; die "4j verify-attestation true was refused on a valid fixture"; }
[ "$(wc -l < "$(at_out true gh.calls)" | tr -d ' ')" = 1 ] || die "4j verify-attestation true: gh was not called exactly once"
printf '%s\n' attestation verify "$AT_ROOT/temp/ign-install/$AT_ASSET" --repo TheThoughtagen/ignition-cli \
  --signer-workflow TheThoughtagen/ignition-cli/.github/workflows/release.yml > "$AT/want-gh-args"
cmp -s "$AT/want-gh-args" "$(at_out true gh.args)" || { cat "$(at_out true gh.args)" >&2; die "4j verify-attestation true: unexpected gh argv"; }
[ "$(cat "$(at_out true gh.state)")" = "$(printf 'token=set\nextracted=no')" ] || die "4j verify-attestation true: gh did not see a token and no extraction directory"
[ "$(cat "$(at_out true fake.log)")" = 'ran GH_TOKEN=unset' ] || die "4j verify-attestation true: the extracted binary did not run once with GH_TOKEN unset ($(cat "$(at_out true fake.log)"))"
grep -qxF "IGN_BIN=$AT_ROOT/temp/ign-install/x/ign-$AT_TAG-x86_64-unknown-linux-gnu/ign" "$(at_out true github_env)" || die "4j verify-attestation true: IGN_BIN was not exported"
if grep -rq 'SECRETVALUE' "$AT/out/true"; then die "4j verify-attestation true: the token reached an output"; fi
grep -qF "verified the build-provenance attestation of $AT_ASSET" "$(at_out true stdout)" || die "4j verify-attestation true: no confirmation line"

# 4j-gh-fail: a failing verification fails closed.
run_install ghfail "$WORK/install-new.sh" IGN_VERIFY_ATTESTATION=true GH_TOKEN=tok-SECRETVALUE GH_STUB_RC=1
[ "$(cat "$(at_out ghfail rc)")" != 0 ] || die "4j a failing gh attestation verify did not fail the install"
grep -E '^::error::.*attestation' "$(at_out ghfail stdout)" > /dev/null || die "4j the gh failure has no ::error:: line naming the attestation"
[ ! -s "$(at_out ghfail github_env)" ] || die "4j the gh failure still wrote GITHUB_ENV"
[ "$(cat "$(at_out ghfail extracted)")" = no ] || die "4j the gh failure left an extraction directory"
[ ! -s "$(at_out ghfail fake.log)" ] || die "4j the downloaded binary ran after a failed attestation"
if grep -rq 'SECRETVALUE' "$AT/out/ghfail"; then die "4j the gh failure printed the token"; fi

# 4j-invalid and 4j-no-gh: refused before any download.
for bad in '' yes TRUE 1 'true '; do
  run_install invalid "$WORK/install-new.sh" "IGN_VERIFY_ATTESTATION=$bad"
  [ "$(cat "$(at_out invalid rc)")" != 0 ] || die "4j verify-attestation '$bad' was accepted"
  cat "$(at_out invalid stdout)" "$(at_out invalid stderr)" | grep -qF 'verify-attestation' || die "4j the refusal of '$bad' does not name verify-attestation"
  [ ! -s "$(at_out invalid curl.log)" ] && [ ! -s "$(at_out invalid gh.calls)" ] || die "4j verify-attestation '$bad' was refused only after a download or a gh call"
done
run_install nogh "$WORK/install-new.sh" IGN_VERIFY_ATTESTATION=true "PATH=$AT/curlbin:$AT/nogh"
[ "$(cat "$(at_out nogh rc)")" != 0 ] || die "4j verify-attestation true was accepted with no gh on PATH"
cat "$(at_out nogh stdout)" "$(at_out nogh stderr)" | grep -qF 'gh' || die "4j the missing-gh refusal does not name gh"
[ ! -s "$(at_out nogh curl.log)" ] || die "4j the missing-gh refusal came after a download"
pass "4j the install body is byte-identical to 408f65b for verify-attestation false and unset; true calls gh once with the exact argv before extraction and the binary never sees the token; a failing gh, a malformed value and a missing gh all fail closed before the binary runs"

# The apply-secrets body of deploy-projects.yml names the cross-organization cause (T-25-90).
extract_first "$PROJECTS_WF" body:apply-secrets > "$WORK/apply-secrets.sh"
secrets_step() { # secrets_step <name> <expected text or ''> VAR=value ...
  local name="$1" want="$2" rc=0
  shift 2
  ( env -i PATH="$PATH" IGN_MASTER_TOKEN=tok-SECRETVALUE IGN_SSH_KEY=key-SECRETVALUE IGN_SSH_KEY_ENV= "$@" bash "$WORK/apply-secrets.sh" ) > "$WORK/secrets-$name.log" 2>&1 || rc=$?
  if grep -q 'SECRETVALUE' "$WORK/secrets-$name.log"; then die "4g case $name printed a secret"; fi
  if [ -z "$want" ]; then
    [ "$rc" = 0 ] || { cat "$WORK/secrets-$name.log" >&2; die "4g case $name was refused"; }
  else
    [ "$rc" != 0 ] || die "4g case $name was accepted"
    for w in "$want" 'secrets: inherit' 'composite actions'; do
      grep -qF -e "$w" "$WORK/secrets-$name.log" || { cat "$WORK/secrets-$name.log" >&2; die "4g case $name: no message containing: $w"; }
    done
  fi
}
secrets_step both ''
secrets_step both-key '' IGN_SSH_KEY_ENV=DEPLOY_SSH_KEY
secrets_step no-token IGN_MASTER_TOKEN IGN_MASTER_TOKEN=
secrets_step no-key IGN_SSH_KEY IGN_SSH_KEY_ENV=DEPLOY_SSH_KEY IGN_SSH_KEY=
pass "4g apply-secrets refuses an empty master token or ssh key naming secrets: inherit and the composite actions, and prints no value"

# ---------------------------------------------------------------- 5. end to end
IGN_REPO_BIN="$(build_repo_bin)"
E2E="$WORK/projects-e2e"
/bin/rm -rf "$E2E"
mkdir -p "$E2E/work/project/deploy" "$E2E/work/project/ignition/script-python/cap" \
  "$E2E/work/targets/ignition/targets" "$E2E/data/projects" \
  "$E2E/data/config/resources/core/ignition/system-properties"
printf '{"systemName":"capgw"}' > "$E2E/data/config/resources/core/ignition/system-properties/config.json"
printf '{"title":"Cap App","enabled":true}' > "$E2E/work/project/project.json"
printf 'schema_version: 1\nproject: CapApp\nenvironments: [dev]\n' > "$E2E/work/project/deploy/project.yaml"
printf '{"scope":"A","files":["code.py"],"attributes":{"enabled":true}}' > "$E2E/work/project/ignition/script-python/cap/resource.json"
printf 'def f():\n    return 1\n' > "$E2E/work/project/ignition/script-python/cap/code.py"
printf '.ign/\n' > "$E2E/work/project/.gitignore"
cat > "$E2E/work/targets/ignition/targets/dev.yaml" << YAML
schema_version: 1
gateway: capgw
mode: dev
nodes:
  master:
    profile: dev
    local:
      data_dir: $E2E/data
YAML
git -C "$E2E/work/project" init -q
git -C "$E2E/work/project" add -A
git -C "$E2E/work/project" -c user.name=harness -c user.email=harness@example.invalid commit -q -m fixture

# A gateway that lists no projects: the project is absent, so the action is create.
cat > "$WORK/mock-gateway.py" << 'PY'
import http.server, json, sys

class H(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.split("?")[0] == "/data/api/v1/projects/list":
            body = json.dumps({"items": [], "metadata": {"total": 0, "matching": 0, "limit": -1, "offset": 0}}).encode()
            self.send_response(200)
        else:
            body = b"{}"
            self.send_response(404)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *a):
        pass

srv = http.server.HTTPServer(("127.0.0.1", 0), H)
open(sys.argv[1], "w").write(str(srv.server_address[1]))
srv.serve_forever()
PY
python3 -I "$WORK/mock-gateway.py" "$WORK/mock-port" &
MOCK_PID=$!
for _ in $(seq 1 50); do [ -s "$WORK/mock-port" ] && break; sleep 0.1; done
[ -s "$WORK/mock-port" ] || die "5 the mock gateway did not start"
GW_URL="http://127.0.0.1:$(cat "$WORK/mock-port")"

fake_runner_env "$E2E/runner"
HOST_OS="$(host_runner_os)"
extract_first "$CHECK_ACTION" body:write-config > "$WORK/write-config.sh"
extract_first "$CHECK_ACTION" body:run-check > "$WORK/run-check.sh"
extract_first "$APPLY_ACTION" body:run-apply > "$WORK/run-apply.sh"
extract_first "$APPLY_ACTION" body:stage > "$WORK/stage.sh"

COMMON=("RUNNER_OS=$HOST_OS" "IGN_BIN=$IGN_REPO_BIN" IGN_ENV=dev IGN_RELEASE=v1.0.0 IGN_REPO_DIR=.
  IGN_SSH_KEY_ENV= IGN_SSH_KEY= IGN_MASTER_PROFILE=dev)

# run_step <script> [VAR=value ...]: run an extracted step in the work directory
# the way a later workflow step would see it (GITHUB_ENV loaded, GITHUB_OUTPUT
# truncated).
run_step() {
  local script="$1"
  shift
  : > "$GITHUB_OUTPUT"
  ( cd "$E2E/work" && load_env_file "$GITHUB_ENV" && env "${COMMON[@]}" "$@" bash "$script" )
}

# The targets step runs first and exports IGN_TARGETS_DIR through GITHUB_ENV.
( cd "$E2E/work" && env -i PATH="$PATH" GITHUB_ENV="$GITHUB_ENV" IGN_TARGETS_REPOSITORY=WhiskeyHouse/whk-onprem-infra-iac \
  IGN_TARGETS_REF=main IGN_TARGETS_DIR_INPUT=ignition IGN_TARGETS_CHECKOUT=targets IGN_TARGETS_REQUIRED=true \
  IGN_TARGETS_TOKEN=tok bash "$WORK/targets-args.sh" ) > "$E2E/targets.log" 2>&1 || { cat "$E2E/targets.log" >&2; die "5 the targets step failed"; }
run_step "$WORK/write-config.sh" "IGN_MASTER_URL=$GW_URL" IGN_BACKUP_PROFILE= IGN_BACKUP_URL= IGN_SSL_VERIFY=true > "$E2E/write-config.log" 2>&1 \
  || { cat "$E2E/write-config.log" >&2; die "5 write-config failed"; }
OUT="$RUNNER_TEMP/ign-out"

# (a) the check against a gateway without the project
run_step "$WORK/run-check.sh" IGN_BACKUP_PROFILE= IGN_MASTER_TOKEN=dummy-token IGN_BACKUP_TOKEN= IGN_DESIGNERS_TIMEOUT=600 > "$E2E/check.log" 2>&1 \
  || { cat "$E2E/check.log" >&2; die "5a run-check step failed"; }
[ "$(cat "$GITHUB_OUTPUT")" = "exit_code=0" ] || { cat "$E2E/check.log" "$OUT/projects.stderr.json" >&2; die "5a expected exit_code=0, got: $(cat "$GITHUB_OUTPUT")"; }
REAL_HASH="$(jq -r .data.artifact_sha256 "$OUT/projects.stdout.json")"
[[ "$REAL_HASH" =~ ^[0-9a-f]{64}$ ]] || die "5a no artifact_sha256 from the check run"
[ "$(jq -r .data.action "$OUT/projects.stdout.json")" = create ] || die "5a expected action create"
[ "$(jq -r .data.check "$OUT/projects.stdout.json")" = true ] || die "5a expected check true"
[ ! -e "$E2E/work/project/.ign" ] || die "5a --check created .ign/"
A_OUT="$WORK/projects-out-a"
/bin/rm -rf "$A_OUT"
cp -R "$OUT" "$A_OUT"
pass "5a the extracted targets, config and check steps run the real ign --check against a mock gateway (action create, hash $REAL_HASH)"

# (a2) the check job's own token message names the check secret, never the write one
if run_step "$WORK/run-check.sh" IGN_BACKUP_PROFILE= IGN_MASTER_TOKEN= IGN_BACKUP_TOKEN= IGN_DESIGNERS_TIMEOUT=600 > "$E2E/check-notoken.log" 2>&1; then
  die "5a an empty check token was accepted"
fi
grep -q 'IGN_CHECK_TOKEN' "$E2E/check-notoken.log" || die "5a the refusal does not name IGN_CHECK_TOKEN"
if grep -q 'IGN_MASTER_TOKEN' "$E2E/check-notoken.log"; then die "5a the check refusal names the write token"; fi
pass "5a2 an empty check token is refused naming IGN_CHECK_TOKEN only"

# (b) a stale or wrong hash is refused before anything is uploaded
ZERO="$(printf '0%.0s' $(seq 1 64))"
run_step "$WORK/run-apply.sh" IGN_BACKUP_PROFILE= IGN_MASTER_TOKEN=dummy-token IGN_BACKUP_TOKEN= "IGN_ARTIFACT_SHA256=$ZERO" IGN_DESIGNERS_TIMEOUT=600 > "$E2E/apply-b.log" 2>&1 \
  || { cat "$E2E/apply-b.log" >&2; die "5b run-apply step failed"; }
[ "$(cat "$GITHUB_OUTPUT")" = "exit_code=6" ] || { cat "$E2E/apply-b.log" "$OUT/projects.stderr.json" >&2; die "5b expected exit_code=6, got: $(cat "$GITHUB_OUTPUT")"; }
[ "$(jq -r .error.code "$OUT/projects.stderr.json")" = project_hash_mismatch ] || die "5b expected project_hash_mismatch, got: $(cat "$OUT/projects.stderr.json")"
B_OUT="$WORK/projects-out-b"
/bin/rm -rf "$B_OUT"
cp -R "$OUT" "$B_OUT"
pass "5b a hash that is not the checkout's artifact is refused as project_hash_mismatch (exit 6)"

# (c) the apply step refuses its own bad inputs before ign runs
for bad in 'IGN_ARTIFACT_SHA256=' 'IGN_ARTIFACT_SHA256=ABC'; do
  if run_step "$WORK/run-apply.sh" IGN_BACKUP_PROFILE= IGN_MASTER_TOKEN=dummy-token IGN_BACKUP_TOKEN= "$bad" IGN_DESIGNERS_TIMEOUT=600 > "$E2E/apply-c.log" 2>&1; then
    die "5c an artifact hash of '$bad' was accepted"
  fi
  grep -q 'artifact_sha256' "$E2E/apply-c.log" || die "5c the refusal does not name artifact_sha256"
done
if run_step "$WORK/run-apply.sh" IGN_BACKUP_PROFILE= IGN_MASTER_TOKEN= IGN_BACKUP_TOKEN= "IGN_ARTIFACT_SHA256=$REAL_HASH" IGN_DESIGNERS_TIMEOUT=600 > "$E2E/apply-c.log" 2>&1; then
  die "5c an empty IGN_MASTER_TOKEN was accepted"
fi
grep -q 'IGN_MASTER_TOKEN' "$E2E/apply-c.log" || die "5c the refusal does not name IGN_MASTER_TOKEN"
if run_step "$WORK/run-apply.sh" IGN_BACKUP_PROFILE= IGN_MASTER_TOKEN=dummy-token IGN_BACKUP_TOKEN= "IGN_ARTIFACT_SHA256=$REAL_HASH" IGN_DESIGNERS_TIMEOUT=600 IGN_RELEASE='v1;rm' > "$E2E/apply-c.log" 2>&1; then
  die "5c a release with a shell metacharacter was accepted"
fi
if run_step "$WORK/run-apply.sh" IGN_BACKUP_PROFILE= IGN_MASTER_TOKEN=dummy-token IGN_BACKUP_TOKEN= "IGN_ARTIFACT_SHA256=$REAL_HASH" IGN_DESIGNERS_TIMEOUT=600 IGN_REPO_DIR=../x > "$E2E/apply-c.log" 2>&1; then
  die "5c a repo-dir that leaves the project checkout was accepted"
fi
pass "5c an empty or malformed artifact hash, an empty master token, a bad release and a bad repo-dir are refused before ign runs"

# ---------------------------------------------------------------- 6. staging
mkdir -p "$E2E/work/project/.ign/deploy/capgw"
OLD_DIR="$E2E/work/project/.ign/deploy/capgw/20200101T000000Z-old000"
mkdir -p "$OLD_DIR"
printf '{"old":true}\n' > "$OLD_DIR/result.json"
sleep 1
# The 5c refusals left an empty ign-out; put the envelopes of the real 5b run back.
cp "$B_OUT"/projects.std*.json "$OUT/"
touch "$OUT/.apply-start"
sleep 1
NEW_DIR="$E2E/work/project/.ign/deploy/capgw/20261023T130000Z-new001"
mkdir -p "$NEW_DIR/export"
for f in run.json result.json events.ndjson plan.json; do printf 'x\n' > "$NEW_DIR/$f"; done
printf 'PROJECT-CONTENT-CANARY\n' > "$NEW_DIR/export/CapApp.zip"
run_step "$WORK/stage.sh" > "$E2E/stage.log" 2>&1 || { cat "$E2E/stage.log" >&2; die "6 stage step failed"; }
ART="$RUNNER_TEMP/ign-artifacts"
STAGED="$(cd "$ART" && find . -type f | sed 's|^\./||' | sort)"
EXPECTED="$(printf '%s\n' capgw/20261023T130000Z-new001/events.ndjson capgw/20261023T130000Z-new001/result.json capgw/20261023T130000Z-new001/run.json projects.stderr.json projects.stdout.json | sort)"
[ "$STAGED" = "$EXPECTED" ] || die "6 staged tree differs
--- expected
$EXPECTED
--- staged
$STAGED"
if grep -rq 'PROJECT-CONTENT-CANARY' "$ART"; then die "6 the pre-image reached the artifact staging directory"; fi
pass "6 staging carries only the allowlisted records of this job's run directories, never the saved pre-image"

# ---------------------------------------------------------------- 7. report bodies
HARNESS_DIR="$HARNESS_DIR" CHECK_ACTION="$CHECK_ACTION" APPLY_REPORT_FILE="$APPLY_ACTION" A_OUT="$A_OUT" B_OUT="$B_OUT" REAL_HASH="$REAL_HASH" \
  node --input-type=module - << 'JS'
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const fileOf = (region) => (region === 'body:check-report' ? process.env.CHECK_ACTION : process.env.APPLY_REPORT_FILE);
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;

const extractFirst = (file, region) => {
  const lines = fs.readFileSync(file, 'utf8').split('\n');
  const out = [];
  let on = false;
  let indent = 0;
  for (const raw of lines) {
    const line = raw.replace(/[ \t\r]+$/, '');
    if (!on && line.endsWith('>>> ' + region)) {
      indent = raw.length - raw.trimStart().length;
      on = true;
      continue;
    }
    if (on && line.endsWith('<<< ' + region)) return out.join('\n');
    if (on) out.push(raw.slice(indent));
  }
  throw new Error('region not found: ' + region);
};

const makeCore = () => {
  const core = {
    outputs: {}, failed: [], warnings: [], summary_: '',
    summary: { addRaw(t) { core.summary_ += t; return this; }, async write() { return this; } },
    setOutput(k, v) { core.outputs[k] = v; },
    setFailed(m) { core.failed.push(String(m)); },
    warning(m) { core.warnings.push(String(m)); },
  };
  return core;
};
const context = { serverUrl: 'https://github.com', repo: { owner: 'acme', repo: 'app' }, runId: 7 };

async function run(region, env, files) {
  const tmp = fs.mkdtempSync(path.join(process.env.HARNESS_TMP, 'prep-'));
  fs.mkdirSync(path.join(tmp, 'ign-out'));
  for (const [n, t] of Object.entries(files)) fs.writeFileSync(path.join(tmp, 'ign-out', n), t);
  const saved = { ...process.env };
  process.env.RUNNER_TEMP = tmp;
  Object.assign(process.env, env);
  const core = makeCore();
  try {
    await new AsyncFunction('context', 'core', 'require', extractFirst(fileOf(region), region))(context, core, (m) => (m === 'fs' ? fs : m === 'path' ? path : null));
  } finally {
    for (const k of Object.keys(process.env)) if (!(k in saved)) delete process.env[k];
    Object.assign(process.env, saved);
    fs.rmSync(tmp, { recursive: true, force: true });
  }
  return core;
}
const readDir = (dir) => Object.fromEntries(fs.readdirSync(dir).filter((f) => f.endsWith('.json')).map((f) => [f, fs.readFileSync(path.join(dir, f), 'utf8')]));
let passed = 0;
async function test(name, fn) {
  try { await fn(); } catch (e) { console.error('FAIL ' + name + '\n' + (e.stack || e)); process.exit(1); }
  passed += 1;
  console.log('PASS ' + name);
}

const real = readDir(process.env.A_OUT);
const hash = process.env.REAL_HASH;
const base = { IGN_ENV: 'dev', IGN_RELEASE: 'v1.0.0' };

await test('report check (i) a real check envelope exposes the hash and fails nothing', async () => {
  const core = await run('body:check-report', { ...base, IGN_EXIT: '0' }, real);
  assert.equal(core.outputs.artifact_sha256, hash);
  assert.equal(core.outputs.action, 'create');
  assert.deepEqual(core.failed, []);
  assert.ok(core.summary_.includes(hash));
  assert.ok(core.summary_.includes('would create'));
});
await test('report check (i-b) the summary names the targets commit the check read', async () => {
  const sha = 'a'.repeat(40);
  const core = await run('body:check-report', { ...base, IGN_EXIT: '0', IGN_TARGETS_SHA: sha }, real);
  assert.deepEqual(core.failed, []);
  assert.ok(core.summary_.includes('targets commit'));
  assert.ok(core.summary_.includes(sha));
});
await test('report check (ii) a failed check exposes no hash and fails the job', async () => {
  const stderr = JSON.stringify({ ok: false, error: { code: 'deploy_tree_invalid', message: 'bad tree', hint: 'fix it' } });
  const core = await run('body:check-report', { ...base, IGN_EXIT: '2' }, { 'projects.stdout.json': '', 'projects.stderr.json': stderr });
  assert.equal(core.outputs.artifact_sha256, '');
  assert.equal(core.failed.length, 1);
  assert.ok(core.summary_.includes('deploy_tree_invalid') && core.summary_.includes('bad tree'));
});
await test('report check (iii) a run step that never returned fails with no hash', async () => {
  const core = await run('body:check-report', { ...base, IGN_EXIT: '' }, {});
  assert.equal(core.outputs.artifact_sha256, '');
  assert.equal(core.failed.length, 1);
});
await test('report check (iv) exit 0 with a malformed hash is not trusted', async () => {
  const bad = JSON.stringify({ ok: true, data: { check: true, artifact_sha256: 'nothex', action: 'import' } });
  const core = await run('body:check-report', { ...base, IGN_EXIT: '0' }, { 'projects.stdout.json': bad });
  assert.equal(core.outputs.artifact_sha256, '');
  assert.equal(core.failed.length, 1);
});

const okData = (over = {}) => ({
  ok: true, slug: null, env: 'dev', gateway: 'capgw', project: 'CapApp', release: 'v1.0.0', check: false,
  action: 'import', outcome: 'imported', artifact_sha256: hash, live_sha256: hash, run_id: 'R1', run_dir: '.ign/deploy/capgw/R1',
  record_written: true, members: { added: 0, changed: 1, removed: 0, same: 2 }, warnings: ['live project drifted'],
  children: [
    { name: 'ChildA', status: 'ok', release: 'v1.0.0', recorded_sha256: hash, live_sha256: hash },
    { name: 'ChildB', status: 'mismatch', release: 'v0.9.0', recorded_sha256: hash, live_sha256: hash },
    { name: 'ChildC', status: 'unverified', release: null, recorded_sha256: null, live_sha256: null },
  ],
  ...over,
});
const applyEnv = { ...base, IGN_DESIGNERS_TIMEOUT: '600', IGN_ARTIFACT: 'ign-deploy-projects-dev-1-1' };

await test('report apply (i) an import lists warnings and children, annotates only the mismatch', async () => {
  const core = await run('body:apply-report', { ...applyEnv, IGN_EXIT: '0' }, { 'projects.stdout.json': JSON.stringify({ ok: true, data: okData() }) });
  assert.deepEqual(core.failed, []);
  assert.equal(core.outputs.outcome, 'imported');
  assert.equal(core.outputs.run_id, 'R1');
  assert.ok(core.summary_.includes('live project drifted'));
  for (const n of ['ChildA', 'ChildB', 'ChildC']) assert.ok(core.summary_.includes(n), n);
  assert.equal(core.warnings.length, 1);
  assert.ok(core.warnings[0].includes('ChildB'));
});
await test('report apply (ii) created, recorded and skipped all pass', async () => {
  for (const outcome of ['created', 'recorded', 'skipped']) {
    const core = await run('body:apply-report', { ...applyEnv, IGN_EXIT: '0' }, { 'projects.stdout.json': JSON.stringify({ ok: true, data: okData({ outcome, children: [] }) }) });
    assert.deepEqual(core.failed, [], outcome);
    assert.equal(core.outputs.outcome, outcome);
  }
});
await test('report apply (iii) the unverified verdict fails with the slug and the restore command', async () => {
  const restore = 'ign deploy rollback --env dev --run R1';
  const data = okData({ ok: false, slug: 'deploy_project_unverified', outcome: 'imported_unverified', restore_command: restore, record_written: false });
  const core = await run('body:apply-report', { ...applyEnv, IGN_EXIT: '6' }, { 'projects.stdout.json': JSON.stringify({ ok: true, data }) });
  assert.equal(core.failed.length, 1);
  assert.ok(core.failed[0].includes('deploy_project_unverified'));
  assert.ok(core.failed[0].includes(restore));
  assert.equal(core.outputs.outcome, 'imported_unverified');
  assert.ok(core.summary_.includes(restore));
});
await test('report apply (iv) an unverified verdict without a pre-image says nothing can be restored', async () => {
  const data = okData({ ok: false, slug: 'deploy_project_unverified', outcome: 'imported_unverified', restore_command: null });
  const core = await run('body:apply-report', { ...applyEnv, IGN_EXIT: '6' }, { 'projects.stdout.json': JSON.stringify({ ok: true, data }) });
  assert.equal(core.failed.length, 1);
  assert.ok(core.failed[0].includes('deploy_project_unverified') && core.failed[0].includes('nothing to restore'));
});
await test('report apply (v) a real project_hash_mismatch refusal names the slug and the next step', async () => {
  const core = await run('body:apply-report', { ...applyEnv, IGN_EXIT: '6' }, readDir(process.env.B_OUT));
  assert.equal(core.failed.length, 1);
  assert.ok(core.failed[0].includes('project_hash_mismatch'));
  assert.equal(core.outputs.outcome, 'hash_mismatch');
});
await test('report apply (vi) designers, plan_stale, destructive refusal, usage error and a run that never returned', async () => {
  const err = (code, message) => ({ 'projects.stdout.json': '', 'projects.stderr.json': JSON.stringify({ ok: false, error: { code, message } }) });
  let core = await run('body:apply-report', { ...applyEnv, IGN_EXIT: '6' }, err('deploy_preflight_failed', 'x\n  designers: 2 open sessions'));
  assert.equal(core.outputs.outcome, 'designers_timeout');
  core = await run('body:apply-report', { ...applyEnv, IGN_EXIT: '6' }, err('plan_stale', 'live project changed since the plan'));
  assert.equal(core.outputs.outcome, 'plan_stale');
  core = await run('body:apply-report', { ...applyEnv, IGN_EXIT: '2' }, err('destructive_change_refused', 'downgrade'));
  assert.equal(core.outputs.outcome, 'destructive_refused');
  core = await run('body:apply-report', { ...applyEnv, IGN_EXIT: '2' }, err('invalid_input', 'bad'));
  assert.equal(core.outputs.outcome, 'usage');
  core = await run('body:apply-report', { ...applyEnv, IGN_EXIT: '' }, {});
  assert.equal(core.outputs.outcome, 'not_run');
  assert.ok(core.failed.length === 1);
});
await test('report apply (vii) exit 0 with an unexpected body is a failure, not a pass', async () => {
  const core = await run('body:apply-report', { ...applyEnv, IGN_EXIT: '0' }, { 'projects.stdout.json': JSON.stringify({ ok: true, data: okData({ outcome: 'checked' }) }) });
  assert.equal(core.failed.length, 1);
  assert.equal(core.outputs.outcome, 'unexpected');
});
console.log('ok ' + passed);
JS
pass "7 report check and apply cases"
