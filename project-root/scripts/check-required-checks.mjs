// The forge's required-status-checks list is not a committed file, so assert it against the committed job names
// from the API. A gate that exits non-zero but is not required does not block a merge. Needs GH_TOKEN with read
// access to the repository (the default Actions token suffices on a public repo).
import fs from 'node:fs';
import path from 'node:path';
// The shared helpers live beside the backend's scripts: at the template this file sits under project-root/ and the
// backend is the repository root; lifted into a project, the backend is backend/.
const lib = await import(fs.existsSync(new URL('../backend/scripts/_lib.mjs', import.meta.url)) ? '../backend/scripts/_lib.mjs' : '../../scripts/_lib.mjs');
const { capture, lines, main, Fail } = lib;

main(() => {
  process.chdir(path.resolve(import.meta.dirname, '..'));
  const repo = process.env.GITHUB_REPOSITORY || capture('gh', ['repo', 'view', '--json', 'nameWithOwner', '--jq', '.nameWithOwner']);
  // Every branch named by a committed ruleset that requires status checks: main. dev.json requires none, since
  // dev takes direct pushes; CI still runs on them.
  const branches = [];
  for (const file of fs.readdirSync('.github/rulesets').filter((f) => f.endsWith('.json')).sort()) {
    const ruleset = JSON.parse(fs.readFileSync(path.join('.github/rulesets', file), 'utf8'));
    if (!ruleset.rules.some((r) => r.type === 'required_status_checks')) continue;
    for (const ref of ruleset.conditions.ref_name.include) {
      const m = /^refs\/heads\/(.+)$/.exec(ref);
      if (!m) throw new Fail(`.github/rulesets/${file} names ${ref}; name each branch as refs/heads/<branch>`);
      branches.push(m[1]);
    }
  }
  if (branches.length === 0) throw new Fail('no ruleset in .github/rulesets/ requires status checks');
  // Job names: the two-space-indented keys under `jobs:` in the committed workflow.
  const expected = [];
  let inJobs = false;
  for (const line of lines(fs.readFileSync('.github/workflows/ci.yml', 'utf8'))) {
    if (/^jobs:/.test(line)) { inJobs = true; continue; }
    const m = inJobs && /^  ([a-zA-Z0-9_-]+):$/.exec(line);
    if (m) expected.push(m[1]);
  }
  expected.sort();
  for (const branch of branches) {
    const actual = lines(capture('gh', ['api', `repos/${repo}/rules/branches/${branch}`, '--jq',
      '.[] | select(.type=="required_status_checks") | .parameters.required_status_checks[].context'])).sort();
    if (actual.length === 0) {
      throw new Fail(`no required status checks on ${repo}@${branch}; apply .github/rulesets/ (node scripts/apply-ruleset.mjs)`);
    }
    if (expected.join('\n') !== actual.join('\n')) {
      console.error(`required checks on ${branch} differ from committed job names`);
      console.error('expected (ci.yml jobs):'); console.error(expected.join('\n'));
      console.error('required (forge):'); console.error(actual.join('\n'));
      throw new Fail(`required checks on ${branch} differ from committed job names`);
    }
  }
  console.log(`required status checks on ${branches.join(' and ')} match ci.yml job names: ${expected.join(' ')}`);
});
