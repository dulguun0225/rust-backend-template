// The committed OpenAPI document (openapi/v1.json), past the test that writes and diffs it:
//   1. the test reruns under another timezone and locale, and the document must come out byte-identical;
//   2. vacuum lints it against rules/openapi.yaml — after refusing scripts/fixtures/openapi/vacuum-violations.json,
//      which breaks every rule once, naming each;
//   3. oasdiff compares it with the base branch's copy, `breaking --fail-on ERR` — after refusing
//      scripts/fixtures/openapi/breaking.json against the committed document and passing compatible.json.
// Usage: node scripts/openapi.mjs [base-sha]   (no base, or a base without the document: step 3 compares nothing)
import fs from 'node:fs';
import path from 'node:path';
import { captureAll, main, ok, run, Fail } from './_lib.mjs';

const repo = path.resolve(import.meta.dirname, '..');
const DOCUMENT = 'openapi/v1.json';
const RULES = ['no-offset-or-page-parameter', 'no-patch-operation', 'limit-declares-maximum', 'error-responses-use-problem-schema', 'temporal-name-matches-format', 'request-body-schemas-are-closed'];

const vacuum = (file) => captureAll('vacuum', ['lint', '-r', 'rules/openapi.yaml', '-d', '-a', '-n', 'error', '--no-clip', '-b', file], { cwd: repo });
const oasdiff = (base, revision) => captureAll('oasdiff', ['breaking', base, revision, '--fail-on', 'ERR'], { cwd: repo });

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
  const base = process.argv[2] ?? '';
  if (!base || /^0+$/.test(base) || !ok('git', ['cat-file', '-e', `${base}:${DOCUMENT}`], { cwd: repo })) {
    console.log(`no base document to compare against (base ${base || 'not given'})`);
    return;
  }
  const baseFile = path.join(repo, 'target', 'openapi', 'base.json');
  fs.mkdirSync(path.dirname(baseFile), { recursive: true });
  fs.writeFileSync(baseFile, captureAll('git', ['show', `${base}:${DOCUMENT}`], { cwd: repo }).stdout);
  const diff = oasdiff(baseFile, DOCUMENT);
  process.stdout.write(diff.stdout);
  if (diff.status !== 0) throw new Fail(`oasdiff: ${DOCUMENT} breaks the document on ${base.slice(0, 12)} (exit ${diff.status})`, diff.status);
  console.log(`${DOCUMENT} makes no breaking change against ${base.slice(0, 12)}`);
});
