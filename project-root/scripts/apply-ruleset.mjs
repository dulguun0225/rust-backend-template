// Apply the committed branch rulesets — dev.json and main.json — once, at repo setup. Idempotent: replaces an existing
// ruleset of the same name.
// Usage: node scripts/apply-ruleset.mjs [owner/repo]   (defaults to the repository this checkout tracks)
import fs from 'node:fs';
import path from 'node:path';
// The shared helpers live beside the backend's scripts: at the template this file sits under project-root/ and the
// backend is the repository root; lifted into a project, the backend is backend/.
const lib = await import(fs.existsSync(new URL('../backend/scripts/_lib.mjs', import.meta.url)) ? '../backend/scripts/_lib.mjs' : '../../scripts/_lib.mjs');
const { capture, main, run } = lib;

main(() => {
  process.chdir(path.resolve(import.meta.dirname, '..'));
  const dir = '.github/rulesets';
  const repo = process.argv[2] ?? capture('gh', ['repo', 'view', '--json', 'nameWithOwner', '--jq', '.nameWithOwner']);
  for (const file of fs.readdirSync(dir).filter((f) => f.endsWith('.json')).sort()) {
    const ruleset = path.join(dir, file);
    const name = JSON.parse(fs.readFileSync(ruleset, 'utf8')).name;
    const existing = capture('gh', ['api', `repos/${repo}/rulesets`, '--jq', `.[] | select(.name=="${name}") | .id`], { check: false });
    if (existing) {
      run('gh', ['api', '--method', 'PUT', `repos/${repo}/rulesets/${existing}`, '--input', ruleset, '--silent']);
      console.log(`ruleset '${name}' updated (${existing})`);
    } else {
      run('gh', ['api', '--method', 'POST', `repos/${repo}/rulesets`, '--input', ruleset, '--silent']);
      console.log(`ruleset '${name}' created`);
    }
  }
});
