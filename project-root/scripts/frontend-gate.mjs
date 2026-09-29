// The frontend job's one step. The micro-frontend is not started yet, and this script says so on every run
// rather than passing silently: a required check that gates nothing is recorded as gating nothing.
// The moment frontend/package.json exists, this script demands a `check` script there and runs it with a
// lockfile-exact install; a frontend with code and no gate fails the build.
import fs from 'node:fs';
import path from 'node:path';
// The shared helpers live beside the backend's scripts: at the template this file sits under project-root/ and the
// backend is the repository root; lifted into a project, the backend is backend/.
const lib = await import(fs.existsSync(new URL('../backend/scripts/_lib.mjs', import.meta.url)) ? '../backend/scripts/_lib.mjs' : '../../scripts/_lib.mjs');
const { main, run, Fail } = lib;

main(() => {
  const frontend = path.resolve(import.meta.dirname, '..', 'frontend');
  if (!fs.existsSync(path.join(frontend, 'package.json'))) {
    console.log('frontend: not started (no frontend/package.json). Nothing gates the frontend yet; frontend/README.md records the decisions taken and the gates to wire.');
    return;
  }
  process.chdir(frontend);
  const pkg = JSON.parse(fs.readFileSync('package.json', 'utf8'));
  if (!pkg.scripts || !pkg.scripts.check) throw new Fail('frontend/package.json has no `check` script; a frontend with code and no gate is not allowed');
  if (!fs.existsSync('package-lock.json')) throw new Fail('frontend has no package-lock.json; the install must be lockfile-exact');
  run('npm', ['ci']);
  run('npm', ['run', 'check']);
});
