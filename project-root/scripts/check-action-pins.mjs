// Every `uses:` in every workflow references a 40-hex commit SHA, never a tag. A tag moves; a SHA does not.
// A reference is found wherever `uses:` is a key, block or flow style (`- { uses: … }`), quoted or not.
// Pinning the caller does not pin a reusable workflow's own callees: review those by hand when adding one.
// Usage: node scripts/check-action-pins.mjs [dir] [--selftest]   (dir holds .github/workflows; defaults to the root)
//   --selftest: a generated workflow with a tag reference in each style must be refused, and a SHA one passed.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
// The shared helpers live beside the backend's scripts: at the template this file sits under project-root/ and the
// backend is the repository root; lifted into a project, the backend is backend/.
const lib = await import(fs.existsSync(new URL('../backend/scripts/_lib.mjs', import.meta.url)) ? '../backend/scripts/_lib.mjs' : '../../scripts/_lib.mjs');
const { lines, main, Fail } = lib;

/** Every action reference in a workflow's text that is not pinned to a commit SHA. */
export function unpinned(text) {
  const out = [];
  for (const line of lines(text)) {
    const code = line.replace(/\s#.*$/, '');
    for (const m of code.matchAll(/(?:^|[\s{,-])uses:\s*["']?([^\s"',}]+)/g)) {
      const ref = m[1];
      if (ref.startsWith('./')) continue; // local composite action
      if (!/@[0-9a-f]{40}$/.test(ref)) out.push(line.trim());
    }
  }
  return out;
}

function scan(dir) {
  const workflows = path.join(dir, '.github', 'workflows');
  const found = [];
  for (const entry of fs.readdirSync(workflows, { recursive: true, withFileTypes: true })) {
    if (entry.isFile()) found.push(...unpinned(fs.readFileSync(path.join(entry.parentPath, entry.name), 'utf8')));
  }
  return found;
}

function selftest() {
  const sha = 'a'.repeat(40);
  const cases = [
    [`steps:\n  - uses: actions/checkout@v7\n`, 1],
    [`steps:\n  - { uses: actions/checkout@v7, with: { fetch-depth: 0 } }\n`, 1],
    [`steps:\n  - uses: "actions/checkout@main"\n`, 1],
    [`steps:\n  - uses: docker://alpine:3\n`, 1],
    [`steps:\n  - uses: actions/checkout@${sha} # v7\n  - uses: ./local\n  - run: echo "uses: x@v1"\n`, 0],
  ];
  const problems = [];
  for (const [text, want] of cases) {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'action-pins-'));
    try {
      fs.mkdirSync(path.join(dir, '.github', 'workflows'), { recursive: true });
      fs.writeFileSync(path.join(dir, '.github', 'workflows', 'ci.yml'), text);
      const got = scan(dir).length;
      if (got !== want) problems.push(`expected ${want} unpinned, got ${got}:\n${text}`);
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  }
  if (problems.length > 0) throw new Fail(`action-pin selftest failed:\n${problems.join('\n')}`);
  console.log('action-pin selftest: a tag reference in block, flow and quoted style and a docker tag refused; a SHA passed');
}

main(() => {
  const args = process.argv.slice(2);
  if (args.includes('--selftest')) return selftest();
  const found = scan(args[0] ?? path.resolve(import.meta.dirname, '..'));
  for (const line of found) console.error(`not SHA-pinned: ${line}`);
  if (found.length > 0) throw new Fail(`${found.length} action reference(s) not SHA-pinned`);
  console.log('all actions SHA-pinned');
});
