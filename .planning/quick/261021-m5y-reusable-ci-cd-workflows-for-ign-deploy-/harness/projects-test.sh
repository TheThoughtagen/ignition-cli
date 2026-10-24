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
actionlint "$PROJECTS_WF" || die "actionlint (with shellcheck) rejected deploy-projects.yml"
pass "1a actionlint + shellcheck clean"
body_checks "$PROJECTS_WF"
pass "1b run/script bodies are block scalars in body regions with no expression"

# ---------------------------------------------------------------- 2. shared regions
for region in install-ign write-config ign-env; do
  [ "$(count_regions "$PROJECTS_WF" "shared:$region")" = 2 ] || die "2 shared:$region does not occur once per job in deploy-projects.yml"
  extract "$PLAN_WF" "shared:$region" > "$WORK/shared-plan-$region.txt"
  [ -s "$WORK/shared-plan-$region.txt" ] || die "2 shared:$region is empty in deploy-plan.yml"
  cat "$WORK/shared-plan-$region.txt" "$WORK/shared-plan-$region.txt" > "$WORK/shared-plan-$region.twice"
  extract "$PROJECTS_WF" "shared:$region" > "$WORK/shared-projects-$region.txt"
  cmp -s "$WORK/shared-plan-$region.twice" "$WORK/shared-projects-$region.txt" \
    || die "2 shared:$region differs between deploy-plan.yml and a job of deploy-projects.yml"
done
pass "2a shared install-ign, write-config and ign-env regions are byte-identical with deploy-plan.yml in both jobs"

[ "$(count_regions "$PROJECTS_WF" shared:targets-args)" = 2 ] || die "2b shared:targets-args does not occur once per job"
[ "$(count_regions "$PLAN_WF" shared:targets-args)" = 1 ] || die "2b shared:targets-args does not occur once in deploy-plan.yml"
[ "$(count_regions "$APPLY_WF" shared:targets-args)" = 1 ] || die "2b shared:targets-args does not occur once in deploy-apply.yml"
extract "$PLAN_WF" shared:targets-args > "$WORK/targets-args-plan.txt"
extract "$APPLY_WF" shared:targets-args > "$WORK/targets-args-apply.txt"
extract "$PROJECTS_WF" shared:targets-args > "$WORK/targets-args-projects.txt"
extract_first "$PROJECTS_WF" shared:targets-args > "$WORK/targets-args-first.txt"
[ -s "$WORK/targets-args-plan.txt" ] || die "2b shared:targets-args is empty in deploy-plan.yml"
cat "$WORK/targets-args-plan.txt" "$WORK/targets-args-plan.txt" > "$WORK/targets-args-twice.txt"
cmp -s "$WORK/targets-args-twice.txt" "$WORK/targets-args-projects.txt" || die "2b the two shared:targets-args copies in deploy-projects.yml differ from deploy-plan.yml"
cmp -s "$WORK/targets-args-plan.txt" "$WORK/targets-args-apply.txt" || die "2b shared:targets-args differs between deploy-plan.yml and deploy-apply.yml"
cmp -s "$WORK/targets-args-plan.txt" "$WORK/targets-args-first.txt" || die "2b shared:targets-args differs between deploy-plan.yml and deploy-projects.yml"
pass "2b shared targets-args region is byte-identical in deploy-plan.yml, deploy-apply.yml and both jobs of deploy-projects.yml"

# ---------------------------------------------------------------- 3. structure
# shellcheck disable=SC2016
for needle in 'needs: check' 'environment: ${{ inputs.environment }}' 'cancel-in-progress: false' \
  'group: ign-deploy-projects-${{ inputs.env }}-${{ github.repository }}' \
  'needs.check.outputs.artifact_sha256' '--targets' '--check' '--artifact-sha256' '--yes' \
  '--designers wait' '--designers-timeout' 'workflow_call'; do
  grep -qF -e "$needle" "$PROJECTS_WF" || die "3 deploy-projects.yml lacks: $needle"
done
[ "$(grep -cE '^  (check|apply):$' "$PROJECTS_WF")" = 2 ] || die "3 expected exactly the jobs check and apply"
needs_line="$(grep -n '^    needs: check$' "$PROJECTS_WF" | cut -d: -f1)"
apply_line="$(grep -n '^  apply:$' "$PROJECTS_WF" | cut -d: -f1)"
[ "$needs_line" = "$((apply_line + 1))" ] || die "3 the apply job does not need the check job"
grep -A1 -E '^permissions:$' "$PROJECTS_WF" | grep -q 'contents: read' || die "3 top-level permissions is not contents: read"
if grep -qE 'contents: write|pull-requests: write|pull_request_target' "$PROJECTS_WF"; then
  die "3 deploy-projects.yml grants write access or uses pull_request_target"
fi
if grep -q -e '--allow-downgrade' -e '--allow-first-change' -e '--skip-gwbk' -e '--allow-destructive' "$PROJECTS_WF"; then
  die "3 deploy-projects.yml passes an allow flag the workflow must never pass"
fi
pass "3a jobs: apply needs check, environment, per-env concurrency without cancel, read-only permissions, no allow flags"

checkouts="$(grep -cE 'uses: actions/checkout@v7$' "$PROJECTS_WF")"
persist="$(grep -cE '^ +persist-credentials: false$' "$PROJECTS_WF")"
[ "$checkouts" = 4 ] && [ "$persist" = 4 ] || die "3b expected 4 checkouts all with persist-credentials: false, got $checkouts and $persist"
# shellcheck disable=SC2016
[ "$(grep -cF 'token: ${{ secrets.IGN_TARGETS_TOKEN }}' "$PROJECTS_WF")" = 2 ] || die "3b the targets checkout does not use IGN_TARGETS_TOKEN in both jobs"
[ "$(grep -cF 'path: targets' "$PROJECTS_WF")" = 2 ] || die "3b the targets checkout path is not targets in both jobs"
[ "$(grep -cF 'path: project' "$PROJECTS_WF")" = 2 ] || die "3b the project checkout path is not project in both jobs"
[ "$(grep -cF 'secrets.IGN_TARGETS_TOKEN' "$PROJECTS_WF")" = 4 ] || die "3b IGN_TARGETS_TOKEN is used outside the targets step env and checkout"
pass "3b every checkout drops its credentials; the targets checkout is read with the token only, outside the project checkout"

# A secret name or value reaching a log line.
if grep -nE "(echo|printf)[^|>]*(TOKEN|IGN_SSH_KEY|secrets\.)" "$PROJECTS_WF" | grep -v 'fail "' | grep -q .; then
  grep -nE "(echo|printf)[^|>]*(TOKEN|IGN_SSH_KEY|secrets\.)" "$PROJECTS_WF" >&2
  die "3c a run step echoes a token, key or secret"
fi
if grep -qE '^[[:space:]]*(set -x|set -[a-z]*x)' "$PROJECTS_WF"; then die "3c a run step enables xtrace"; fi
pass "3c no step echoes a token or key, no xtrace"

# Approval-gate secret split. The check job runs before and without the environment
# approval, so it must hold only the read-only IGN_CHECK_* secrets; the apply job
# holds the write-capable ones. Both are static, per-job line-range checks.
check_start="$(grep -n '^  check:$' "$PROJECTS_WF" | cut -d: -f1)"
apply_start="$(grep -n '^  apply:$' "$PROJECTS_WF" | cut -d: -f1)"
[ -n "$check_start" ] && [ -n "$apply_start" ] && [ "$check_start" -lt "$apply_start" ] || die "3d cannot locate the check and apply jobs"
sed -n "${check_start},$((apply_start - 1))p" "$PROJECTS_WF" > "$WORK/job-check.yml"
sed -n "${apply_start},\$p" "$PROJECTS_WF" > "$WORK/job-apply.yml"
if grep -nE 'secrets\.(IGN_MASTER_TOKEN|IGN_SSH_KEY)\b' "$WORK/job-check.yml" | grep -q .; then
  grep -nE 'secrets\.(IGN_MASTER_TOKEN|IGN_SSH_KEY)\b' "$WORK/job-check.yml" >&2
  die "3d the check job references a write-capable secret (IGN_MASTER_TOKEN or IGN_SSH_KEY)"
fi
if grep -nE 'secrets\.IGN_CHECK_(TOKEN|SSH_KEY)\b' "$WORK/job-apply.yml" | grep -q .; then
  grep -nE 'secrets\.IGN_CHECK_(TOKEN|SSH_KEY)\b' "$WORK/job-apply.yml" >&2
  die "3d the apply job references a check secret (IGN_CHECK_TOKEN or IGN_CHECK_SSH_KEY)"
fi
# shellcheck disable=SC2016
[ "$(grep -cF 'IGN_MASTER_TOKEN: ${{ secrets.IGN_CHECK_TOKEN }}' "$WORK/job-check.yml")" = 1 ] || die "3d the check job does not take its gateway token from IGN_CHECK_TOKEN"
# shellcheck disable=SC2016
[ "$(grep -cF 'IGN_SSH_KEY: ${{ secrets.IGN_CHECK_SSH_KEY }}' "$WORK/job-check.yml")" = 1 ] || die "3d the check job does not take its ssh key from IGN_CHECK_SSH_KEY"
# shellcheck disable=SC2016
[ "$(grep -cF 'IGN_MASTER_TOKEN: ${{ secrets.IGN_MASTER_TOKEN }}' "$WORK/job-apply.yml")" = 1 ] || die "3d the apply job does not take its gateway token from IGN_MASTER_TOKEN"
# shellcheck disable=SC2016
[ "$(grep -cF 'IGN_SSH_KEY: ${{ secrets.IGN_SSH_KEY }}' "$WORK/job-apply.yml")" = 1 ] || die "3d the apply job does not take its ssh key from IGN_SSH_KEY"
# No fallback from a check secret to a write secret (or back), and no whole-context use
# that would hand every secret to a step.
if grep -nE 'secrets\.[A-Z_]+ *\|\||\|\| *secrets\.|secrets\[|toJSON\(secrets|secrets: *inherit|\$\{\{ *secrets *\}\}' "$PROJECTS_WF" | grep -q .; then
  die "3d a secret expression falls back to another secret or uses the whole secrets context"
fi
pass "3d the check job references only IGN_CHECK_TOKEN and IGN_CHECK_SSH_KEY, the apply job only IGN_MASTER_TOKEN and IGN_SSH_KEY, with no fallback between them"

# ---------------------------------------------------------------- 4. targets-args step
extract_first "$PROJECTS_WF" body:targets-args > "$WORK/targets-args.sh"
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
extract_first "$PROJECTS_WF" body:write-config > "$WORK/write-config.sh"
extract_first "$PROJECTS_WF" body:run-check > "$WORK/run-check.sh"
extract_first "$PROJECTS_WF" body:run-apply > "$WORK/run-apply.sh"
extract_first "$PROJECTS_WF" body:stage > "$WORK/stage.sh"

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
HARNESS_DIR="$HARNESS_DIR" PROJECTS_WF="$PROJECTS_WF" A_OUT="$A_OUT" B_OUT="$B_OUT" REAL_HASH="$REAL_HASH" \
  node --input-type=module - << 'JS'
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const wf = process.env.PROJECTS_WF;
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;

const extractFirst = (region) => {
  const lines = fs.readFileSync(wf, 'utf8').split('\n');
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
    await new AsyncFunction('context', 'core', 'require', extractFirst(region))(context, core, (m) => (m === 'fs' ? fs : m === 'path' ? path : null));
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
