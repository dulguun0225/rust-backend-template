// Every `uses:` in every workflow references a 40-hex commit SHA, never a tag. A tag moves; a SHA does not.
// Pinning the caller does not pin a reusable workflow's own callees: review those by hand when adding one.
// Usage: node scripts/check-action-pins.mjs [dir]   (the directory holding .github/workflows; defaults to the root)
import fs from 'node:fs';
import path from 'node:path';
// The shared helpers live beside the backend's scripts: at the template this file sits under project-root/ and the
// backend is the repository root; lifted into a project, the backend is backend/.
const lib = await import(fs.existsSync(new URL('../backend/scripts/_lib.mjs', import.meta.url)) ? '../backend/scripts/_lib.mjs' : '../../scripts/_lib.mjs');
const { lines, main, Fail } = lib;

main(() => {
  process.chdir(process.argv[2] ?? path.resolve(import.meta.dirname, '..'));
  const workflows = path.join('.github', 'workflows');
  let unpinned = 0;
  for (const entry of fs.readdirSync(workflows, { recursive: true, withFileTypes: true })) {
    if (!entry.isFile()) continue;
    for (const line of lines(fs.readFileSync(path.join(entry.parentPath, entry.name), 'utf8'))) {
      if (!/^\s*-?\s*uses:/.test(line)) continue;
      const ref = line.replace(/.*uses:\s*([^\s#]+).*/, '$1');
      if (ref.startsWith('./')) continue; // local composite action
      if (!/@[0-9a-f]{40}$/.test(ref)) {
        console.error(`not SHA-pinned: ${line}`);
        unpinned += 1;
      }
    }
  }
  if (unpinned > 0) throw new Fail(`${unpinned} action reference(s) not SHA-pinned`);
  console.log('all actions SHA-pinned');
});
