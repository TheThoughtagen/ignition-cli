// Harness for the github-script bodies of deploy-plan.yml and deploy-apply.yml.
//
//   node report-test.mjs plan  <ign-out dir from a real plan run>
//   node report-test.mjs apply <ign-out dir with a real plan_stale> <ign-out dir with a real network_error>
//
// Each body is extracted from the workflow file by its region markers and run
// as an AsyncFunction with the same parameters github-script v9 provides.

import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, '../../../..');
const planWf = path.join(repoRoot, '.github/workflows/deploy-plan.yml');
const applyWf = path.join(repoRoot, '.github/workflows/deploy-apply.yml');
const nodeRequire = createRequire(import.meta.url);
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;

export function extractBody(file, region) {
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
    if (on && line.endsWith('<<< ' + region)) {
      return out.join('\n');
    }
    if (on) out.push(raw.startsWith(' '.repeat(indent)) ? raw.slice(indent) : raw);
  }
  throw new Error('region not found or not closed: ' + region);
}

export function makeCore() {
  const core = {
    outputs: {},
    failed: [],
    warnings: [],
    infos: [],
    _summary: '',
    summary: {
      addRaw(t) {
        core._summary += t;
        return this;
      },
      async write() {
        return this;
      },
    },
    setOutput(k, v) {
      core.outputs[k] = v;
    },
    setFailed(m) {
      core.failed.push(String(m));
    },
    warning(m) {
      core.warnings.push(String(m));
    },
    info(m) {
      core.infos.push(String(m));
    },
    summaryText() {
      return core._summary;
    },
  };
  return core;
}

// opts: comments (existing PR comments), prs (listPullRequestsAssociatedWithCommit
// result), reject: { createComment, updateComment, listComments, listPrs, createCommitComment }
// set to a status code to make that call reject with an error carrying it.
export function makeGithub(opts = {}) {
  const calls = [];
  const reject = opts.reject || {};
  const maybeReject = (name) => {
    if (reject[name]) {
      const e = new Error('HttpError ' + reject[name] + ' from ' + name);
      e.status = reject[name];
      throw e;
    }
  };
  const issues = {
    listComments: async (args) => {
      calls.push({ fn: 'listComments', args });
      maybeReject('listComments');
      return { data: opts.comments || [] };
    },
    createComment: async (args) => {
      calls.push({ fn: 'createComment', args });
      maybeReject('createComment');
      return { data: { id: 9001 } };
    },
    updateComment: async (args) => {
      calls.push({ fn: 'updateComment', args });
      maybeReject('updateComment');
      return { data: {} };
    },
  };
  const repos = {
    listPullRequestsAssociatedWithCommit: async (args) => {
      calls.push({ fn: 'listPullRequestsAssociatedWithCommit', args });
      maybeReject('listPrs');
      return { data: opts.prs || [] };
    },
    createCommitComment: async (args) => {
      calls.push({ fn: 'createCommitComment', args });
      maybeReject('createCommitComment');
      return { data: { id: 9002 } };
    },
  };
  return {
    calls,
    paginate: async (fn, args) => (await fn(args)).data,
    rest: { issues, repos },
    callsOf: (name) => calls.filter((c) => c.fn === name),
  };
}

export function makeContext(over = {}) {
  return {
    repo: { owner: 'acme', repo: 'gateway-config' },
    sha: '0123456789abcdef0123456789abcdef01234567',
    actor: 'alice',
    runId: 4242,
    serverUrl: 'https://github.com',
    eventName: 'push',
    payload: {},
    ...over,
  };
}

// Run a body in a scratch RUNNER_TEMP seeded with `files` (ign-out/<name> -> text).
export async function runBody(file, region, { env = {}, files = {}, context, github, core }) {
  const tmp = fs.mkdtempSync(path.join(process.env.HARNESS_TMP || os.tmpdir(), 'report-'));
  const outDir = path.join(tmp, 'ign-out');
  fs.mkdirSync(outDir, { recursive: true });
  for (const [name, text] of Object.entries(files)) {
    fs.writeFileSync(path.join(outDir, name), text);
  }
  const saved = { ...process.env };
  process.env.RUNNER_TEMP = tmp;
  for (const [k, v] of Object.entries(env)) process.env[k] = v;
  try {
    const body = extractBody(file, region);
    const fn = new AsyncFunction(
      'github', 'octokit', 'context', 'core', 'glob', 'io', 'exec', 'require', 'getOctokit',
      body,
    );
    await fn(github, github, context, core, {}, {}, {}, nodeRequire, () => github);
  } finally {
    for (const k of Object.keys(process.env)) if (!(k in saved)) delete process.env[k];
    Object.assign(process.env, saved);
    fs.rmSync(tmp, { recursive: true, force: true });
  }
}

let passed = 0;
export async function testCase(name, fn) {
  try {
    await fn();
  } catch (e) {
    console.error('FAIL ' + name + '\n' + (e && e.stack ? e.stack : e));
    process.exit(1);
  }
  passed += 1;
  console.log('PASS ' + name);
}

function readDir(dir) {
  const files = {};
  for (const f of fs.readdirSync(dir)) files[f] = fs.readFileSync(path.join(dir, f), 'utf8');
  return files;
}

// ---------------------------------------------------------------- plan cases

async function planCases(outDir) {
  const real = readDir(outDir);
  const envelope = JSON.parse(real['plan.stdout.json']);
  const hash = envelope.data.plan_sha256;
  assert.match(hash, /^[0-9a-f]{64}$/);
  assert.equal(envelope.data.summary.added, 2, 'the tracer e2e plan should have 2 added entries');
  const marker = '<!-- ign-deploy-plan env=dev -->';
  const prContext = (extra = {}) => makeContext({
    eventName: 'pull_request',
    payload: { pull_request: { number: 7 } },
    ...extra,
  });
  const baseEnv = { IGN_ENV: 'dev', IGN_EXIT: '0', IGN_VERSION: 'v1.5.0' };

  await testCase('report plan (i) real envelope on a PR creates one sticky comment', async () => {
    const core = makeCore();
    const github = makeGithub();
    await runBody(planWf, 'body:plan-report', {
      env: baseEnv, files: real, context: prContext(), github, core,
    });
    assert.equal(github.callsOf('createComment').length, 1);
    assert.equal(github.callsOf('updateComment').length, 0);
    const body = github.callsOf('createComment')[0].args.body;
    assert.ok(body.includes(marker), 'marker');
    assert.ok(body.includes(hash), 'plan_sha256');
    assert.match(body, /\| 2 \|/, 'added count 2');
    assert.equal(github.callsOf('createComment')[0].args.issue_number, 7);
    assert.equal(core.outputs.plan_sha256, hash);
    assert.deepEqual(core.failed, []);
    assert.ok(core.summaryText().includes(hash));
  });

  await testCase('report plan (ii) an existing bot comment is updated in place', async () => {
    const core = makeCore();
    const github = makeGithub({
      comments: [{ id: 55, user: { login: 'github-actions[bot]' }, body: marker + '\nold' }],
    });
    await runBody(planWf, 'body:plan-report', {
      env: baseEnv, files: real, context: prContext(), github, core,
    });
    assert.equal(github.callsOf('createComment').length, 0);
    const upd = github.callsOf('updateComment');
    assert.equal(upd.length, 1);
    assert.equal(upd[0].args.comment_id, 55);
    assert.ok(upd[0].args.body.includes(hash));
  });

  await testCase('report plan (iii) a human comment holding the marker is never updated', async () => {
    const core = makeCore();
    const github = makeGithub({
      comments: [{ id: 66, user: { login: 'mallory' }, body: marker + '\nforged' }],
    });
    await runBody(planWf, 'body:plan-report', {
      env: baseEnv, files: real, context: prContext(), github, core,
    });
    assert.equal(github.callsOf('updateComment').length, 0);
    assert.equal(github.callsOf('createComment').length, 1);
  });

  await testCase('report plan (iv) a journal-present failure shows code and message, no output', async () => {
    const core = makeCore();
    const github = makeGithub();
    const stderr = JSON.stringify({
      ok: false,
      profile: 'dev',
      error: {
        code: 'deploy_journal_present',
        message: 'a previous apply left a journal; run: ign deploy rollback --run 20261015T142233Z-a1b2c3 --env dev',
        endpoint: null,
        hint: 'roll back first',
      },
    }, null, 2);
    await runBody(planWf, 'body:plan-report', {
      env: { ...baseEnv, IGN_EXIT: '6' },
      files: { 'plan.stdout.json': '', 'plan.stderr.json': stderr },
      context: prContext(), github, core,
    });
    assert.equal(core.failed.length, 1);
    assert.match(core.failed[0], /FAILED \(exit 6\)/);
    const body = github.callsOf('createComment')[0].args.body;
    assert.ok(body.includes('deploy_journal_present'));
    assert.ok(body.includes('ign deploy rollback --run 20261015T142233Z-a1b2c3'));
    assert.equal(core.outputs.plan_sha256, undefined);
  });

  await testCase('report plan (v) a push context writes the summary and makes no comment call', async () => {
    const core = makeCore();
    const github = makeGithub();
    await runBody(planWf, 'body:plan-report', {
      env: baseEnv, files: real, context: makeContext(), github, core,
    });
    assert.equal(github.calls.length, 0);
    assert.ok(core.summaryText().includes(hash));
    assert.equal(core.outputs.plan_sha256, hash);
  });

  await testCase('report plan (vi) an empty exit code fails saying ign did not run', async () => {
    const core = makeCore();
    const github = makeGithub();
    await runBody(planWf, 'body:plan-report', {
      env: { ...baseEnv, IGN_EXIT: '' }, files: {}, context: makeContext(), github, core,
    });
    assert.equal(core.failed.length, 1);
    assert.match(core.failed[0], /ign did not run/);
    assert.equal(core.outputs.plan_sha256, undefined);
  });

  await testCase('report plan (vii) a comment API error is a warning, not a failure of the plan', async () => {
    const core = makeCore();
    const github = makeGithub({ reject: { createComment: 403 } });
    await runBody(planWf, 'body:plan-report', {
      env: baseEnv, files: real, context: prContext(), github, core,
    });
    assert.equal(core.warnings.length, 1);
    assert.deepEqual(core.failed, []);
    assert.equal(core.outputs.plan_sha256, hash);
  });
}

// ---------------------------------------------------------------- apply cases

const RUN_ID = '20261021T120000Z-abc123';
const HASH = 'a'.repeat(64);
const applyData = (o = {}) => ({
  env: 'prod', gateway: 'gw1', mode: 'prod', node: 'master', transport: 'ssh',
  run_id: RUN_ID, run_dir: '/work/.ign/deploy/gw1/' + RUN_ID,
  outcome: 'applied', ok: true, slug: null, cause: null,
  plan_sha256: HASH, render_sha256: 'b'.repeat(64),
  summary: { added: 1, changed: 0, removed: 0, same: 3, unmanaged: 0, has_changes: true },
  written: ['x/config.json'], deleted: [], warnings: [], gwbk: null, skip_gwbk: false,
  restart_pending: false, restart_pending_new: false, backup: null,
  ...o,
});
const okEnvelope = (data) => JSON.stringify({ ok: true, profile: 'prod', data }, null, 2);
const errEnvelope = (errCode, message, hint) => JSON.stringify({
  ok: false, profile: 'prod', error: { code: errCode, message, endpoint: null, hint },
}, null, 2);

async function applyOnce({ exit, stdout = '', stderr = '', context, githubOpts, env = {} }) {
  const core = makeCore();
  const github = makeGithub(githubOpts);
  await runBody(applyWf, 'body:apply-report', {
    env: {
      IGN_ENV: 'prod', IGN_EXIT: exit, IGN_REPO_DIR: '.', IGN_DESIGNERS_TIMEOUT: '600',
      IGN_ARTIFACT: 'ign-deploy-apply-prod-1-1', ...env,
    },
    files: { 'apply.stdout.json': stdout, 'apply.stderr.json': stderr },
    context: context || makeContext(),
    github, core,
  });
  return { core, github };
}

async function applyCases(args) {
  const real = (spec) => {
    const i = spec.lastIndexOf(':');
    const dir = spec.slice(0, i);
    return {
      exit: spec.slice(i + 1),
      stdout: fs.readFileSync(path.join(dir, 'apply.stdout.json'), 'utf8'),
      stderr: fs.readFileSync(path.join(dir, 'apply.stderr.json'), 'utf8'),
    };
  };
  const rollbackCmd = 'ign deploy rollback --env prod --run ' + RUN_ID + ' --yes';
  const prCtx = (extra = {}) => makeContext({
    eventName: 'pull_request', payload: { pull_request: { number: 11 } }, ...extra,
  });
  const preflight = (lines) => errEnvelope(
    'deploy_preflight_failed',
    'deploy preflight failed (' + lines.length + ' check(s)):' + lines.map((l) => '\n  ' + l).join(''),
    'fix each failed check named above and rerun; no gateway file was written',
  );

  await testCase('report apply: applied passes, carries run_id and the rollback command, no comment', async () => {
    const { core, github } = await applyOnce({ exit: '0', stdout: okEnvelope(applyData()) });
    assert.deepEqual(core.failed, []);
    assert.ok(core.summaryText().includes(RUN_ID));
    assert.ok(core.summaryText().includes(rollbackCmd));
    assert.equal(github.calls.length, 0);
    assert.equal(core.outputs.run_id, RUN_ID);
    assert.equal(core.outputs.outcome, 'applied');
  });

  await testCase('report apply: noop passes with no rollback command', async () => {
    const data = applyData({ outcome: 'noop', run_id: null, run_dir: null, written: [] });
    const { core, github } = await applyOnce({ exit: '0', stdout: okEnvelope(data) });
    assert.deepEqual(core.failed, []);
    assert.ok(!core.summaryText().includes('ign deploy rollback'));
    assert.equal(core.outputs.run_id, '');
    assert.equal(core.outputs.outcome, 'noop');
    assert.equal(github.calls.length, 0);
  });

  await testCase('report apply: repo-dir gw appends --repo gw to the rollback command', async () => {
    const { core } = await applyOnce({
      exit: '0', stdout: okEnvelope(applyData()), env: { IGN_REPO_DIR: 'gw' },
    });
    assert.ok(core.summaryText().includes(rollbackCmd + ' --repo gw'));
  });

  await testCase('report apply: a repo-dir with a space or quote is shell-quoted in the rollback command', async () => {
    const { core } = await applyOnce({
      exit: '0', stdout: okEnvelope(applyData()), env: { IGN_REPO_DIR: "gateway config's" },
    });
    assert.ok(core.summaryText().includes(rollbackCmd + " --repo 'gateway config'\\''s'"), core.summaryText());
  });

  await testCase('report apply: backup_unsynced verify command carries a quoted --repo', async () => {
    const data = applyData({
      outcome: 'applied_backup_unsynced', ok: false, slug: 'deploy_backup_unsynced',
      backup: { reason: 'sync_timeout' },
    });
    const { core } = await applyOnce({
      exit: '6', stdout: okEnvelope(data), env: { IGN_REPO_DIR: 'gateway config' },
    });
    assert.ok(core.summaryText().includes("ign deploy verify --env prod --node backup --repo 'gateway config'"), core.summaryText());
  });

  await testCase('report apply: rolled_back (verify_failed) on a PR fails, mentions the actor, names the cause', async () => {
    const data = applyData({
      outcome: 'rolled_back', ok: false, slug: 'deploy_rolled_back', cause: 'verify_failed',
    });
    const { core, github } = await applyOnce({
      exit: '6', stdout: okEnvelope(data), context: prCtx(),
    });
    assert.equal(core.failed.length, 1);
    assert.match(core.failed[0], /rolled back automatically/);
    assert.ok(core.summaryText().includes('verify_failed'));
    const c = github.callsOf('createComment');
    assert.equal(c.length, 1);
    assert.equal(c[0].args.issue_number, 11);
    assert.ok(c[0].args.body.startsWith('@alice'));
    assert.equal(core.outputs.outcome, 'rolled_back');
  });

  await testCase('report apply: backup_unsynced says do not rerun and keeps the rollback command', async () => {
    const data = applyData({
      outcome: 'applied_backup_unsynced', ok: false, slug: 'deploy_backup_unsynced',
      backup: { reason: 'not_syncing' },
    });
    const { core } = await applyOnce({ exit: '6', stdout: okEnvelope(data), context: prCtx() });
    assert.match(core.failed[0], /master is live, backup lagging, do not rerun/);
    assert.ok(core.summaryText().includes('not_syncing'));
    assert.ok(core.summaryText().includes(rollbackCmd));
    assert.ok(core.summaryText().includes('ign deploy verify --env prod --node backup'));
  });

  const designers = preflight(['designers: 2 Designer session(s) open: bob on proj from host1, carol on proj2 from host2']);
  await testCase('report apply: designers timeout on push comments on the merged PR with actor and timeout', async () => {
    const sha = makeContext().sha;
    const { core, github } = await applyOnce({
      exit: '6', stderr: designers,
      githubOpts: { prs: [
        { number: 3, state: 'open', merge_commit_sha: 'other' },
        { number: 5, state: 'closed', merge_commit_sha: sha },
      ] },
    });
    assert.match(core.failed[0], /Designer sessions were still open after 600 s/);
    assert.match(core.failed[0], /nothing was written/);
    const c = github.callsOf('createComment');
    assert.equal(c.length, 1);
    assert.equal(c[0].args.issue_number, 5);
    assert.ok(c[0].args.body.includes('@alice'));
    assert.ok(c[0].args.body.includes('600'));
    assert.equal(github.callsOf('createCommitComment').length, 0);
    assert.equal(core.outputs.outcome, 'designers_timeout');
  });

  await testCase('report apply: designers timeout with no PR posts a commit comment on the sha', async () => {
    const { core, github } = await applyOnce({ exit: '6', stderr: designers, githubOpts: { prs: [] } });
    const c = github.callsOf('createCommitComment');
    assert.equal(c.length, 1);
    assert.equal(c[0].args.commit_sha, makeContext().sha);
    assert.ok(c[0].args.body.includes('@alice'));
    assert.equal(github.callsOf('createComment').length, 0);
    assert.equal(core.failed.length, 1);
  });

  await testCase('report apply: an open PR is used when none merged the commit', async () => {
    const { github } = await applyOnce({
      exit: '6', stderr: designers,
      githubOpts: { prs: [{ number: 8, state: 'open', merge_commit_sha: 'other' }] },
    });
    assert.equal(github.callsOf('createComment')[0].args.issue_number, 8);
  });

  await testCase('report apply: preflight names every failed check', async () => {
    const { core } = await applyOnce({
      exit: '6',
      stderr: preflight(['wrong_target: the master is gw2, not gw1', 'run_dir_not_ignored: .ign/ is not git-ignored']),
    });
    assert.match(core.failed[0], /wrong_target/);
    assert.match(core.failed[0], /run_dir_not_ignored/);
    assert.match(core.failed[0], /nothing was written/);
    assert.equal(core.outputs.outcome, 'preflight_failed');
  });

  await testCase('report apply: real plan_stale says nothing was written', async () => {
    if (!args[0]) throw new Error('missing the real plan_stale dir:exit argument');
    const r = real(args[0]);
    assert.equal(JSON.parse(r.stderr).error.code, 'plan_stale');
    const { core } = await applyOnce({ ...r });
    assert.match(core.failed[0], /nothing was written/);
    assert.equal(core.outputs.outcome, 'plan_stale');
  });

  await testCase('report apply: exit 2 invalid_input is a pipeline bug', async () => {
    const { core } = await applyOnce({
      exit: '2', stderr: errEnvelope('invalid_input', 'plan-sha256 is required', 'pass --plan-sha256'),
    });
    assert.match(core.failed[0], /pipeline bug/);
    assert.equal(core.outputs.outcome, 'usage');
  });

  await testCase('report apply: exit 2 destructive_change_refused is a policy refusal, not a pipeline bug', async () => {
    const { core } = await applyOnce({
      exit: '2',
      stderr: errEnvelope('destructive_change_refused', 'the plan removes 3 units', 'rerun with --allow-destructive'),
    });
    assert.doesNotMatch(core.failed[0], /pipeline bug/);
    assert.match(core.failed[0], /destructive/);
    assert.equal(core.outputs.outcome, 'destructive_refused');
  });

  await testCase('report apply: real network_error shows code and hint', async () => {
    if (!args[1]) throw new Error('missing the real network_error dir:exit argument');
    const r = real(args[1]);
    const err = JSON.parse(r.stderr).error;
    assert.equal(err.code, 'network_error');
    const { core } = await applyOnce({ ...r });
    assert.ok(core.failed[0].includes('network_error'));
    assert.ok(core.summaryText().includes(err.code));
    if (err.hint) assert.ok(core.summaryText().includes(err.hint.split('\n')[0]));
  });

  await testCase('report apply: exit 1 with non-JSON stderr shows the stderr tail', async () => {
    const stderr = Array.from({ length: 60 }, (_, i) => 'line ' + i).join('\n');
    const { core } = await applyOnce({ exit: '1', stderr });
    assert.equal(core.outputs.outcome, 'unparsed');
    assert.ok(core.summaryText().includes('line 59'));
    assert.ok(!core.summaryText().includes('line 5\n'));
  });

  await testCase('report apply: an empty exit code is not_run', async () => {
    const { core } = await applyOnce({ exit: '' });
    assert.equal(core.outputs.outcome, 'not_run');
    assert.equal(core.failed.length, 1);
  });

  await testCase('report apply: a 403 on the notification is a warning and the outcome still fails the job', async () => {
    const data = applyData({ outcome: 'rolled_back', ok: false, slug: 'deploy_rolled_back', cause: 'lock_expired' });
    const { core } = await applyOnce({
      exit: '6', stdout: okEnvelope(data), context: prCtx(), githubOpts: { reject: { createComment: 403 } },
    });
    assert.equal(core.warnings.length, 1);
    assert.match(core.warnings[0], /403/);
    assert.match(core.failed[0], /rolled back automatically/);
  });

  await testCase('report apply: a differing triggering actor is mentioned too', async () => {
    const data = applyData({ outcome: 'rolled_back', ok: false, slug: 'deploy_rolled_back', cause: 'write_failed' });
    const { github } = await applyOnce({
      exit: '6', stdout: okEnvelope(data), context: prCtx(), env: { GITHUB_TRIGGERING_ACTOR: 'bob' },
    });
    const body = github.callsOf('createComment')[0].args.body;
    assert.ok(body.startsWith('@alice @bob'));
  });

  await testCase('report apply: an unexpected exit-6 error (deploy_journal_present) shows its message', async () => {
    const { core } = await applyOnce({
      exit: '6',
      stderr: errEnvelope('deploy_journal_present', 'recover with: ign deploy rollback --run X --env prod', 'roll back first'),
    });
    assert.match(core.failed[0], /deploy_journal_present/);
    assert.ok(core.summaryText().includes('ign deploy rollback --run X --env prod'));
  });
}

// ---------------------------------------------------------------- main

const [mode, ...args] = process.argv.slice(2);
if (mode === 'plan') {
  if (args.length !== 1) throw new Error('usage: report-test.mjs plan <ign-out dir>');
  await planCases(args[0]);
} else if (mode === 'apply') {
  await applyCases(args);
} else {
  throw new Error('usage: report-test.mjs plan <ign-out dir> | apply <dir:exit of a real plan_stale> <dir:exit of a real network_error>');
}
console.log('report-test ' + mode + ': ' + passed + ' case(s) passed');
