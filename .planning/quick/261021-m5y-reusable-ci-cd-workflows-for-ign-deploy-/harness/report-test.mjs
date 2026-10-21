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

// ---------------------------------------------------------------- main

const [mode, ...args] = process.argv.slice(2);
if (mode === 'plan') {
  if (args.length !== 1) throw new Error('usage: report-test.mjs plan <ign-out dir>');
  await planCases(args[0]);
} else if (mode === 'apply') {
  const { applyCases } = await import('./report-test-apply.mjs').catch(() => ({ applyCases: null }));
  if (!applyCases) throw new Error('apply cases are not available');
  await applyCases(args);
} else {
  throw new Error('usage: report-test.mjs plan|apply ...');
}
console.log('report-test ' + mode + ': ' + passed + ' case(s) passed');
