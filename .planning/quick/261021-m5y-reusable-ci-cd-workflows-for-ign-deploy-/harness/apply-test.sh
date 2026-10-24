#!/usr/bin/env bash
# Harness for .github/workflows/deploy-apply.yml. Run from anywhere.
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

# ---------------------------------------------------------------- 1. lint
actionlint "$APPLY_WF" || die "actionlint (with shellcheck) rejected deploy-apply.yml"
pass "1a actionlint + shellcheck clean"
body_checks "$APPLY_WF"
pass "1b run/script bodies are block scalars in body regions with no expression"

# ---------------------------------------------------------------- 2. shared regions
for region in install-ign write-config ign-env targets-args; do
  extract "$PLAN_WF" "shared:$region" > "$WORK/shared-plan-$region.txt"
  extract "$APPLY_WF" "shared:$region" > "$WORK/shared-apply-$region.txt"
  [ -s "$WORK/shared-plan-$region.txt" ] || die "2 shared:$region is empty in deploy-plan.yml"
  cmp -s "$WORK/shared-plan-$region.txt" "$WORK/shared-apply-$region.txt" \
    || die "2 shared:$region differs between deploy-plan.yml and deploy-apply.yml"
done
pass "2 shared install-ign, write-config, ign-env and targets-args regions are byte-identical"

# ---------------------------------------------------------------- 3. flags
for f in "$PLAN_WF" "$APPLY_WF"; do
  if grep -q -e '--skip-gwbk' -e '--allow-destructive' -e '--allow-drift' "$f"; then
    die "3 $f passes a flag the workflows must never pass (--skip-gwbk, --allow-destructive, --allow-drift)"
  fi
done
for needle in '--designers wait' '--designers-timeout' '--plan-sha256' 'cancel-in-progress: false' 'environment: ${{ inputs.environment }}' '--yes'; do
  grep -qF -e "$needle" "$APPLY_WF" || die "3 deploy-apply.yml lacks: $needle"
done
pass "3 apply flags: designers wait, plan hash, per-env concurrency, environment; no skip/allow flags"
targets_static_checks "$APPLY_WF"
targets_static_checks "$PLAN_WF"
pass "3b optional targets inputs: conditional read-only checkout into .ign-targets, no persisted credentials, token only in the checkout and its validation step, --targets only when set"

# ---------------------------------------------------------------- 4. end to end
IGN_REPO_BIN="$(build_repo_bin)"
E2E="$WORK/apply-e2e"
make_e2e_repo "$E2E"
fake_runner_env "$E2E/runner"
HOST_OS="$(host_runner_os)"

extract "$PLAN_WF" body:write-config > "$WORK/write-config.sh"
extract "$PLAN_WF" body:run-plan > "$WORK/run-plan.sh"
extract "$APPLY_WF" body:run-apply > "$WORK/run-apply.sh"
extract "$APPLY_WF" body:stage > "$WORK/stage.sh"

COMMON=("RUNNER_OS=$HOST_OS" "IGN_BIN=$IGN_REPO_BIN" IGN_ENV=dev "IGN_REPO_DIR=$E2E/repo"
  IGN_SSH_KEY_ENV= IGN_SSH_KEY= IGN_MASTER_PROFILE=dev IGN_BACKUP_PROFILE=)

# run_step <script> [VAR=value ...]: run an extracted step the way a later
# workflow step would see it (GITHUB_ENV loaded, GITHUB_OUTPUT truncated).
run_step() {
  local script="$1"
  shift
  : > "$GITHUB_OUTPUT"
  ( load_env_file "$GITHUB_ENV"; env "${COMMON[@]}" "$@" bash "$script" )
}

run_step "$WORK/write-config.sh" IGN_MASTER_URL=http://127.0.0.1:9 IGN_BACKUP_URL= IGN_SSL_VERIFY=true > "$E2E/write-config.log" 2>&1 \
  || { cat "$E2E/write-config.log" >&2; die "4 write-config failed"; }
run_step "$WORK/run-plan.sh" > "$E2E/plan.log" 2>&1 || { cat "$E2E/plan.log" >&2; die "4 run-plan failed"; }
OUT="$RUNNER_TEMP/ign-out"
REAL_HASH="$(jq -r .data.plan_sha256 "$OUT/plan.stdout.json")"
[[ "$REAL_HASH" =~ ^[0-9a-f]{64}$ ]] || die "4 no real plan_sha256 from the plan run"

# An OLD run directory that predates this job's apply.
OLD_DIR="$E2E/repo/.ign/deploy/tracer-gw/20200101T000000Z-old000"
mkdir -p "$OLD_DIR"
printf '{"old":true}\n' > "$OLD_DIR/result.json"
sleep 1

# (a) a stale hash
ZERO="$(printf '0%.0s' $(seq 1 64))"
run_step "$WORK/run-apply.sh" IGN_MASTER_TOKEN=dummy-token "IGN_PLAN_SHA256=$ZERO" IGN_DESIGNERS_TIMEOUT=600 > "$E2E/apply-a.log" 2>&1 \
  || { cat "$E2E/apply-a.log" >&2; die "4a run-apply step failed"; }
[ "$(cat "$GITHUB_OUTPUT")" = "exit_code=6" ] || die "4a expected exit_code=6, got: $(cat "$GITHUB_OUTPUT")"
[ "$(jq -r .error.code "$OUT/apply.stderr.json")" = "plan_stale" ] || die "4a expected plan_stale, got: $(cat "$OUT/apply.stderr.json")"
A_OUT="$WORK/apply-out-a"
/bin/rm -rf "$A_OUT"
cp -R "$OUT" "$A_OUT"
pass "4a a 64-zero plan-sha256 is refused as plan_stale (exit 6)"

# (c) an empty master token
if run_step "$WORK/run-apply.sh" IGN_MASTER_TOKEN= "IGN_PLAN_SHA256=$REAL_HASH" IGN_DESIGNERS_TIMEOUT=600 > "$E2E/apply-c.log" 2>&1; then
  die "4c an empty IGN_MASTER_TOKEN was accepted"
fi
grep -q 'IGN_MASTER_TOKEN' "$E2E/apply-c.log" || die "4c the refusal does not name IGN_MASTER_TOKEN"
[ ! -e "$OUT/apply.stdout.json" ] || die "4c apply.stdout.json was written although ign never ran"
pass "4c an empty IGN_MASTER_TOKEN is refused before ign runs"

# (b) the real hash against an unreachable gateway
PRE_DIRS="$(cd "$E2E/repo/.ign/deploy" && find . -mindepth 2 -maxdepth 2 -type d | sort)"
run_step "$WORK/run-apply.sh" IGN_MASTER_TOKEN=dummy-token "IGN_PLAN_SHA256=$REAL_HASH" IGN_DESIGNERS_TIMEOUT=600 > "$E2E/apply-b.log" 2>&1 \
  || { cat "$E2E/apply-b.log" >&2; die "4b run-apply step failed"; }
B_EXIT="$(sed -n 's/^exit_code=//p' "$GITHUB_OUTPUT")"
[ -n "$B_EXIT" ] && [ "$B_EXIT" != "0" ] || die "4b expected a non-zero ign exit, got: $B_EXIT"
[ "$(jq -r .error.code "$OUT/apply.stderr.json")" = "network_error" ] || die "4b expected network_error, got: $(cat "$OUT/apply.stderr.json")"
B_OUT="$WORK/apply-out-b"
/bin/rm -rf "$B_OUT"
cp -R "$OUT" "$B_OUT"
pass "4b the reviewed hash with an unreachable gateway reports network_error (exit $B_EXIT)"

# ---------------------------------------------------------------- 5. staging
NEW_DIR="$E2E/repo/.ign/deploy/tracer-gw/20261021T130000Z-new001"
mkdir -p "$NEW_DIR/snapshot/files/x"
for f in run.json result.json events.ndjson plan.json 20261021T130000Z-new001.gwbk; do
  printf 'secret-bearing-or-not\n' > "$NEW_DIR/$f"
done
printf '{"password":"hunter2"}\n' > "$NEW_DIR/snapshot/files/x/config.json"

run_step "$WORK/stage.sh" > "$E2E/stage.log" 2>&1 || { cat "$E2E/stage.log" >&2; die "5 stage step failed"; }
ART="$RUNNER_TEMP/ign-artifacts"
POST_DIRS="$(cd "$E2E/repo/.ign/deploy" && find . -mindepth 2 -maxdepth 2 -type d | sort)"
EXPECTED="$(
  {
    echo apply.stderr.json
    echo apply.stdout.json
    while IFS= read -r d; do
      [ -n "$d" ] || continue
      case "$PRE_DIRS" in *"$d"*) continue ;; esac
      for f in run.json result.json events.ndjson; do
        if [ -f "$E2E/repo/.ign/deploy/$d/$f" ]; then echo "${d#./}/$f"; fi
      done
    done <<< "$POST_DIRS"
  } | sort
)"
STAGED="$(cd "$ART" && find . -type f | sed 's|^\./||' | sort)"
[ "$STAGED" = "$EXPECTED" ] || die "5 staged tree differs
--- expected
$EXPECTED
--- staged
$STAGED"
printf '%s\n' "$STAGED" | grep -q 'tracer-gw/20261021T130000Z-new001/run.json' || die "5 the NEW run directory was not staged"
if printf '%s\n' "$STAGED" | grep -Eq 'gwbk|snapshot|plan\.json|old000'; then
  die "5 a forbidden file reached the artifact staging directory"
fi
pass "5 staging carries only the allowlisted records of this job's run directories"

# ---------------------------------------------------------------- 6. report cases
node "$HARNESS_DIR/report-test.mjs" apply "$A_OUT:6" "$B_OUT:$B_EXIT"
pass "6 report apply cases"
