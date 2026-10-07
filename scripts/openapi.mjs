// The committed OpenAPI document (openapi/v1.json), past the test that writes and diffs it:
//   1. the test reruns under another timezone and locale, and the document must come out byte-identical;
//   2. vacuum lints it against rules/openapi.yaml — after refusing scripts/fixtures/openapi/vacuum-violations.json,
//      which breaks every rule once, naming each;
//   3. oasdiff compares it with the base branch's copy, `breaking --fail-on ERR` — after refusing
//      scripts/fixtures/openapi/breaking.json against the committed document and passing compatible.json.
//      A breaking change made on purpose passes only when a commit between the base and HEAD declares each
//      error-level finding in a trailer, `OpenAPI-break: <oasdiff id> <METHOD> <path>`, beside the reason in its
//      message: the declaration travels with the change and covers no later one, since the next base is past it.
//      For a document no consumer binds yet (this template's, a service's before its first release); a released
//      document that crosses the build boundary takes a new major version instead (java-backend-api
//      *Breaking-change diff where a contract crosses the build boundary*), which nothing here can tell apart.
// Usage: node scripts/openapi.mjs [base-sha]   (no base, or a base without the document: step 3 compares nothing)
import fs from 'node:fs';
import path from 'node:path';
import { captureAll, main, ok, run, Fail } from './_lib.mjs';

const repo = path.resolve(import.meta.dirname, '..');
const DOCUMENT = 'openapi/v1.json';
const RULES = ['no-offset-or-page-parameter', 'no-patch-operation', 'limit-declares-maximum', 'error-responses-use-problem-schema', 'temporal-name-matches-format', 'request-body-schemas-are-closed'];

const vacuum = (file) => captureAll('vacuum', ['lint', '-r', 'rules/openapi.yaml', '-d', '-a', '-n', 'error', '--no-clip', '-b', file], { cwd: repo });
const oasdiff = (base, revision, ...more) => captureAll('oasdiff', ['breaking', base, revision, '--fail-on', 'ERR', ...more], { cwd: repo });
const TRAILER = 'OpenAPI-break';
const ERR = 3; // oasdiff's level for an error-level finding, the level --fail-on ERR fails on
const key = (id, method, route) => `${id} ${method.toUpperCase()} ${route}`;

/** The findings oasdiff refused that no declaration accepts, and the ones a declaration accepts. */
export function judge(findings, declared) {
  const accepted = [];
  const refused = [];
  for (const f of findings.filter((x) => x.level === ERR)) {
    const k = key(f.id, f.operation ?? '', f.path ?? '');
    const by = declared.find((d) => d.key === k);
    (by ? accepted : refused).push({ key: k, text: f.text, by });
  }
  return { accepted, refused };
}

/** Every `OpenAPI-break:` trailer on a commit in base..HEAD, as {key, commit}; a malformed one fails. */
function declarations(base) {
  const log = captureAll('git', ['log', `--format=%H %s%x1f%(trailers:key=${TRAILER},valueonly,separator=%x1d)%x1e`, `${base}..HEAD`], { cwd: repo });
  if (log.status !== 0) throw new Fail(`git log ${base}..HEAD failed: ${log.stderr.trim()}`);
  const out = [];
  for (const record of log.stdout.split('\x1e').map((r) => r.trim()).filter(Boolean)) {
    const [commit, values = ''] = record.split('\x1f');
    for (const value of values.split('\x1d').map((v) => v.trim()).filter(Boolean)) {
      const m = /^([a-z0-9-]+) (GET|PUT|POST|DELETE|OPTIONS|HEAD|PATCH|TRACE) (\/\S*)$/.exec(value);
      if (!m) throw new Fail(`${TRAILER}: "${value}" on ${commit.slice(0, 12)} is not "<oasdiff id> <METHOD> <path>"`);
      out.push({ key: key(m[1], m[2], m[3]), commit });
    }
  }
  return out;
}

/** The judge's own canaries, run every time: each must come out as stated, or the acceptance is not trusted. */
function judgeSelftest() {
  const f = (id, operation, route, level = ERR) => ({ id, operation, path: route, level, text: id });
  const d = (k) => ({ key: k, commit: 'c' });
  const cases = [
    ['an undeclared finding is refused', judge([f('a', 'GET', '/x')], []).refused.length === 1],
    ['a declaration for another path accepts nothing', judge([f('a', 'GET', '/x')], [d('a GET /y')]).refused.length === 1],
    ['a declaration for another id accepts nothing', judge([f('a', 'GET', '/x')], [d('b GET /x')]).refused.length === 1],
    ['a declaration for another method accepts nothing', judge([f('a', 'GET', '/x')], [d('a POST /x')]).refused.length === 1],
    ['the exact declaration accepts its finding', judge([f('a', 'GET', '/x')], [d('a GET /x')]).accepted.length === 1],
    ['one declaration accepts only its own finding', judge([f('a', 'GET', '/x'), f('a', 'GET', '/z')], [d('a GET /x')]).refused.length === 1],
    ['a warning is neither accepted nor refused', judge([f('a', 'GET', '/x', 2)], []).refused.length === 0],
  ];
  const wrong = cases.filter(([, held]) => !held).map(([name]) => name);
  if (wrong.length > 0) throw new Fail(`the ${TRAILER} judge failed its canaries: ${wrong.join('; ')}`);
  console.log(`the ${TRAILER} judge passed its ${cases.length} canaries`);
}

main(() => {
  console.log('-- the document under TZ=Pacific/Kiritimati, LC_ALL=tr_TR.UTF-8');
  run('cargo', ['test', '--locked', '-p', 'api', '--test', 'api', '--', '--exact', 'openapi::the_document_matches_its_committed_copy'], {
    cwd: repo,
    env: { SQLX_OFFLINE: 'true', TZ: 'Pacific/Kiritimati', LANG: 'tr_TR.UTF-8', LC_ALL: 'tr_TR.UTF-8' },
  });

  const canary = vacuum('scripts/fixtures/openapi/vacuum-violations.json');
  const unnamed = RULES.filter((rule) => !(canary.stdout + canary.stderr).includes(rule));
  if (canary.status === 0 || unnamed.length > 0) throw new Fail(`vacuum did not refuse its fixture as expected (exit ${canary.status}; rules not named: ${unnamed.join(', ') || 'none'})`);
  console.log(`vacuum refused its fixture, naming all ${RULES.length} rules`);
  const lint = vacuum(DOCUMENT);
  if (lint.status !== 0) {
    console.error(lint.stdout + lint.stderr);
    throw new Fail(`vacuum refused ${DOCUMENT}`, lint.status);
  }
  console.log(`${DOCUMENT} passes rules/openapi.yaml`);

  if (oasdiff(DOCUMENT, 'scripts/fixtures/openapi/breaking.json').status !== 1) throw new Fail('oasdiff did not refuse scripts/fixtures/openapi/breaking.json');
  if (oasdiff(DOCUMENT, 'scripts/fixtures/openapi/compatible.json').status !== 0) throw new Fail('oasdiff refused scripts/fixtures/openapi/compatible.json');
  console.log('oasdiff refused its breaking fixture and passed its compatible one');
  judgeSelftest();
  const base = process.argv[2] ?? '';
  if (!base || /^0+$/.test(base) || !ok('git', ['cat-file', '-e', `${base}:./${DOCUMENT}`], { cwd: repo })) {
    console.log(`no base document to compare against (base ${base || 'not given'})`);
    return;
  }
  const baseFile = path.join(repo, 'target', 'openapi', 'base.json');
  fs.mkdirSync(path.dirname(baseFile), { recursive: true });
  fs.writeFileSync(baseFile, captureAll('git', ['show', `${base}:./${DOCUMENT}`], { cwd: repo }).stdout);
  const diff = oasdiff(baseFile, DOCUMENT, '--format', 'json');
  if (diff.status === 0) {
    console.log(`${DOCUMENT} makes no breaking change against ${base.slice(0, 12)}`);
    return;
  }
  let findings;
  try {
    findings = JSON.parse(diff.stdout || '[]') ?? [];
  } catch {
    process.stderr.write(diff.stdout + diff.stderr);
    throw new Fail(`oasdiff exited ${diff.status} with output that is not its JSON report`, diff.status);
  }
  const { accepted, refused } = judge(findings, declarations(base));
  if (accepted.length + refused.length === 0) {
    process.stderr.write(diff.stderr);
    throw new Fail(`oasdiff exited ${diff.status} reporting no error-level finding`, diff.status);
  }
  for (const a of accepted) console.log(`accepted, declared by ${a.by.commit.slice(0, 12)}: ${a.key}: ${a.text}`);
  for (const r of refused) console.error(`refused: ${r.key}: ${r.text}`);
  if (refused.length > 0) {
    throw new Fail(`oasdiff: ${DOCUMENT} breaks the document on ${base.slice(0, 12)}; a change made on purpose declares each finding in a commit after it ("${TRAILER}: <id> <METHOD> <path>")`, diff.status);
  }
  console.log(`${DOCUMENT} breaks the document on ${base.slice(0, 12)} only where a commit since declares it`);
});
