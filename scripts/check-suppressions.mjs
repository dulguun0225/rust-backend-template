// The committed suppression inventory (suppressions.txt), which CI diffs. Two kinds of line:
//   config <file> [<table>] sha256:<hash>   every file that decides what a gate refuses, hashed, so an edit to
//                                           one shows up in the inventory beside it in the same change
//   suppress <file> <text>                  every allow or expect attribute (outer, inner, cfg_attr) in
//                                           first-party Rust and every squawk-ignore comment in a migration
// A regenerated inventory that differs from the committed one fails, printing the difference; `--write`
// rewrites it. Suppressing a lint the workspace denies or forbids is refused outright by
// check-lint-config.mjs; this inventory lists the rest, so each one is a visible, reviewable line.
// Usage: node scripts/check-suppressions.mjs [--write] [--selftest]
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { main, readText, report, walk, Fail } from './_lib.mjs';
import { attributes, tokens } from './_rust.mjs';
import { tableText } from './_toml.mjs';

const CONFIG_FILES = [
  '.cargo/config.toml',
  '.squawk.toml',
  'clippy.toml',
  'clippy-scopes.toml',
  'deny.toml',
  'layering.toml',
  'rust-toolchain.toml',
  'rustfmt.toml',
  'sgconfig.yml',
  'table-owners.toml',
  'rules/openapi.yaml',
];
const CARGO_TABLES = ['workspace.lints.rust', 'workspace.lints.clippy', 'profile.release'];
const HEADER = [
  '# The suppression inventory: every file that decides what a gate refuses, hashed, and every lint or',
  '# migration-lint suppression in the tree. scripts/check-suppressions.mjs regenerates it and fails on any',
  '# difference; `node scripts/check-suppressions.mjs --write` rewrites it. A change here is a change to what',
  '# the gates refuse: make it in its own commit, and say why.',
];

const sha = (text) => `sha256:${crypto.createHash('sha256').update(text).digest('hex')}`;

export function inventory(root) {
  const out = [];
  const configs = [
    ...CONFIG_FILES,
    ...walk(root, (f) => /^rules\/ast-grep\//.test(f) || /^crates\/[^/]+\/sqlx\.toml$/.test(f)),
  ];
  for (const f of [...new Set(configs)].sort()) {
    const full = path.join(root, f);
    if (fs.existsSync(full)) out.push(`config ${f} ${sha(readText(full))}`);
  }
  const cargo = readText(path.join(root, 'Cargo.toml'));
  for (const t of CARGO_TABLES) out.push(`config Cargo.toml [${t}] ${sha(tableText(cargo, t) ?? '')}`);
  const suppressions = [];
  for (const f of walk(root, (p) => p.endsWith('.rs') && !p.startsWith('scripts/fixtures/') && !p.startsWith('canaries/'))) {
    for (const attr of attributes(tokens(readText(path.join(root, f))))) {
      if (attr.toks.some((t, k) => (t.text === 'allow' || t.text === 'expect') && attr.toks[k + 1]?.text === '(')) {
        suppressions.push(`suppress ${f} #${attr.inner ? '!' : ''}[${attr.text}]`);
      }
    }
  }
  for (const f of walk(root, (p) => p.endsWith('.sql') && !p.startsWith('scripts/fixtures/') && !p.startsWith('canaries/'))) {
    readText(path.join(root, f))
      .split('\n')
      .forEach((line) => {
        if (/squawk-ignore/.test(line)) suppressions.push(`suppress ${f} ${line.trim()}`);
      });
  }
  return [...out, ...suppressions.sort()];
}

export function inventoryDiff(root) {
  const expected = inventory(root);
  const file = path.join(root, 'suppressions.txt');
  const committed = fs.existsSync(file) ? readText(file).split('\n').filter((l) => l && !l.startsWith('#')) : [];
  const findings = [];
  const count = (list) => list.reduce((m, l) => m.set(l, (m.get(l) ?? 0) + 1), new Map());
  const want = count(expected);
  const have = count(committed);
  for (const [l, n] of want) if ((have.get(l) ?? 0) < n) findings.push(`+ ${l}`);
  for (const [l, n] of have) if ((want.get(l) ?? 0) < n) findings.push(`- ${l}`);
  return findings;
}

function write(root) {
  fs.writeFileSync(path.join(root, 'suppressions.txt'), `${[...HEADER, ...inventory(root)].join('\n')}\n`);
}

function selftest() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'suppressions-'));
  const problems = [];
  try {
    fs.mkdirSync(path.join(dir, 'crates', 'a', 'src'), { recursive: true });
    fs.writeFileSync(path.join(dir, 'Cargo.toml'), '[workspace.lints.clippy]\nunwrap_used = "forbid"\n');
    fs.writeFileSync(path.join(dir, 'clippy.toml'), 'disallowed-methods = []\n');
    fs.writeFileSync(path.join(dir, 'crates', 'a', 'src', 'lib.rs'), '//! a\n');
    write(dir);
    if (inventoryDiff(dir).length !== 0) problems.push('a freshly written inventory differs from itself');
    fs.writeFileSync(path.join(dir, 'crates', 'a', 'src', 'lib.rs'), '//! a\n#[expect(dead_code, reason = "x")]\nfn f() {}\n');
    if (!inventoryDiff(dir).some((l) => l.startsWith('+ suppress crates/a/src/lib.rs #[expect(dead_code'))) problems.push('a new expect attribute was not reported');
    write(dir);
    fs.writeFileSync(path.join(dir, 'clippy.toml'), 'disallowed-methods = []\nallow-unwrap-in-tests = true\n');
    if (!inventoryDiff(dir).some((l) => l.startsWith('+ config clippy.toml'))) problems.push('an edited clippy.toml was not reported');
    write(dir);
    fs.writeFileSync(path.join(dir, 'Cargo.toml'), '[workspace.lints.clippy]\nunwrap_used = "allow"\n');
    if (!inventoryDiff(dir).some((l) => l.startsWith('+ config Cargo.toml [workspace.lints.clippy]'))) problems.push('an edited lint table was not reported');
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
  report(problems, 'suppression selftest case(s) failed');
  console.log('suppression selftest: a new suppression, an edited clippy.toml and an edited lint table are each reported');
}

main(() => {
  let opts;
  try {
    ({ values: opts } = parseArgs({ options: { write: { type: 'boolean' }, selftest: { type: 'boolean' } } }));
  } catch (e) {
    throw new Fail(e.message, 2);
  }
  const root = path.resolve(import.meta.dirname, '..');
  if (opts.selftest) return selftest();
  if (opts.write) {
    write(root);
    console.log('wrote suppressions.txt');
    return;
  }
  const diff = inventoryDiff(root);
  if (diff.length > 0) {
    console.error('suppressions.txt differs from the tree (+ in the tree, not listed; - listed, not in the tree):');
    report(diff, 'suppression inventory difference(s); review them, then run node scripts/check-suppressions.mjs --write');
  }
  console.log(`suppression inventory: ${inventory(root).length} line(s), unchanged`);
});
