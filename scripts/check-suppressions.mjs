// The committed suppression inventory (suppressions.txt), which the wall regenerates and compares. Two kinds
// of line:
//   config <file> [<table>] sha256:<hash>   every file that decides what a gate refuses, hashed, so an edit to
//                                           one shows up in the inventory beside it in the same change: the
//                                           configuration files, the gates' own scripts, fixtures and canaries,
//                                           and the root Cargo.toml's [workspace], lint and profile tables
//   suppress <file> <text>                  every allow or expect attribute (outer, inner, cfg_attr), every
//                                           #[ignore] test and every #[rustfmt::skip] in first-party Rust,
//                                           every squawk-ignore comment in a migration, and every entry in
//                                           .trivyignore with the reason on the line above it
// A regenerated inventory that differs from the committed one fails, printing the difference; `--write`
// rewrites it. It makes an edit visible in the change that makes it; it refuses none, since the same change can
// rewrite it. Files Renovate moves (mise.toml, the Dockerfile, the workflows, [workspace.dependencies]) are not
// hashed, so a pin update does not fail here; a digest inside a script is masked for the same reason. mise.lock
// is the exception: it holds the checksum each tool is installed against, so an edit to it, a Renovate pin move
// included, is listed and regenerated in the same change. Suppressing a lint the workspace denies or forbids is refused outright by
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
  'mise.lock',
  '.squawk.toml',
  '.trivyignore',
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
const CARGO_TABLES = ['workspace', 'workspace.lints.rust', 'workspace.lints.clippy'];
const GATE_FILES = (f) => /^rules\//.test(f) || /^crates\/[^/]+\/sqlx\.toml$/.test(f) || (/^scripts\//.test(f) && !/^scripts\/fixtures\//.test(f));
// Fixture and canary trees: one line per tree, a hash over every file's path and text.
const TREES = (f) => /^scripts\/fixtures\/[^/]+\//.test(f) || /^canaries\/[^/]+\//.test(f);
const treeOf = (f) => f.split('/').slice(0, f.startsWith('scripts/') ? 3 : 2).join('/');
// An image or action digest Renovate moves: masked, so a pin update does not change a script's hash.
const unpinned = (text) => text.replace(/(['"])([^'"\s]*)@sha256:[0-9a-f]{64}\1/g, '$1$2@sha256:<digest>$1');
const HEADER = [
  '# The suppression inventory: every file that decides what a gate refuses, hashed, and every lint or',
  '# migration-lint suppression in the tree. scripts/check-suppressions.mjs regenerates it and fails on any',
  '# difference; `node scripts/check-suppressions.mjs --write` rewrites it. A change here is a change to what',
  '# the gates refuse: make it in its own commit, and say why.',
];

const sha = (text) => `sha256:${crypto.createHash('sha256').update(text).digest('hex')}`;

export function inventory(root) {
  const out = [];
  const configs = [...CONFIG_FILES, ...walk(root, GATE_FILES)];
  for (const f of [...new Set(configs)].sort()) {
    const full = path.join(root, f);
    if (fs.existsSync(full)) out.push(`config ${f} ${sha(unpinned(readText(full)))}`);
  }
  const trees = new Map();
  for (const f of walk(root, TREES)) {
    const tree = treeOf(f);
    trees.set(tree, `${trees.get(tree) ?? ''}${f}\0${sha(readText(path.join(root, f)))}\n`);
  }
  for (const [tree, listing] of [...trees].sort()) out.push(`config ${tree}/ ${sha(listing)}`);
  const cargo = readText(path.join(root, 'Cargo.toml'));
  const profiles = cargo.split('\n').map((l) => /^\[(profile\.[^\]]+)\]\s*$/.exec(l)?.[1]).filter(Boolean);
  for (const t of [...CARGO_TABLES, ...profiles]) out.push(`config Cargo.toml [${t}] ${sha(tableText(cargo, t) ?? '')}`);
  const suppressions = [];
  for (const f of walk(root, (p) => p.endsWith('.rs') && !p.startsWith('scripts/fixtures/') && !p.startsWith('canaries/'))) {
    for (const attr of attributes(tokens(readText(path.join(root, f))))) {
      const head = attr.toks[0]?.text;
      const ignored = head === 'ignore' || (head === 'cfg_attr' && attr.toks.some((t, k) => k > 1 && t.text === 'ignore' && attr.toks[k - 1]?.text !== '::'));
      const unformatted = attr.toks.some((t, k) => t.text === 'rustfmt' && attr.toks[k + 1]?.text === '::' && attr.toks[k + 2]?.text === 'skip');
      if (ignored || unformatted || attr.toks.some((t, k) => (t.text === 'allow' || t.text === 'expect') && attr.toks[k + 1]?.text === '(')) {
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
  const trivyignore = path.join(root, '.trivyignore');
  if (fs.existsSync(trivyignore)) {
    const lines = readText(trivyignore).split('\n').map((l) => l.trim());
    lines.forEach((line, i) => {
      if (line === '' || line.startsWith('#')) return;
      const reason = /^#\s*(\S.*)$/.exec(lines[i - 1] ?? '')?.[1];
      suppressions.push(`suppress .trivyignore ${line}${reason ? `  # ${reason}` : ''}`);
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
    fs.writeFileSync(path.join(dir, 'crates', 'a', 'src', 'lib.rs'), '//! a\n#[expect(dead_code, reason = "x")]\nfn f() {}\n#[test]\n#[ignore = "slow"]\nfn t() {}\n#[rustfmt::skip]\nfn g() {}\n');
    const suppressed = inventoryDiff(dir);
    for (const want of ['#[expect(dead_code', '#[ignore', '#[rustfmt::skip]']) {
      if (!suppressed.some((l) => l.startsWith(`+ suppress crates/a/src/lib.rs ${want}`))) problems.push(`a new ${want} attribute was not reported`);
    }
    write(dir);
    fs.writeFileSync(path.join(dir, '.trivyignore'), '# no fixed package yet\nCVE-2026-0001 exp:2026-12-31\n');
    if (!inventoryDiff(dir).some((l) => l === '+ suppress .trivyignore CVE-2026-0001 exp:2026-12-31  # no fixed package yet')) problems.push('a new .trivyignore entry was not reported with its reason');
    write(dir);
    fs.writeFileSync(path.join(dir, 'clippy.toml'), 'disallowed-methods = []\nallow-unwrap-in-tests = true\n');
    if (!inventoryDiff(dir).some((l) => l.startsWith('+ config clippy.toml'))) problems.push('an edited clippy.toml was not reported');
    write(dir);
    fs.writeFileSync(path.join(dir, 'mise.lock'), `[tools.node."platforms.linux-x64"]\nchecksum = "sha256:${'0'.repeat(64)}"\n`);
    write(dir);
    fs.writeFileSync(path.join(dir, 'mise.lock'), `[tools.node."platforms.linux-x64"]\nchecksum = "sha256:${'1'.repeat(64)}"\n`);
    if (!inventoryDiff(dir).some((l) => l.startsWith('+ config mise.lock'))) problems.push('an edited mise.lock checksum was not reported');
    write(dir);
    fs.writeFileSync(path.join(dir, 'Cargo.toml'), '[workspace.lints.clippy]\nunwrap_used = "allow"\n');
    if (!inventoryDiff(dir).some((l) => l.startsWith('+ config Cargo.toml [workspace.lints.clippy]'))) problems.push('an edited lint table was not reported');
    write(dir);
    fs.mkdirSync(path.join(dir, 'scripts', 'fixtures', 'sql'), { recursive: true });
    fs.writeFileSync(path.join(dir, 'scripts', 'wall.mjs'), "const COVERAGE = { lines: 85 };\nconst IMAGE = 'postgres:18@sha256:" + '0'.repeat(64) + "';\n");
    fs.writeFileSync(path.join(dir, 'scripts', 'fixtures', 'sql', 'case.rs'), 'x\n');
    fs.writeFileSync(path.join(dir, 'Cargo.toml'), '[workspace.lints.clippy]\nunwrap_used = "allow"\n\n[profile.release.package.a]\nopt-level = 1\n');
    const added = inventoryDiff(dir);
    for (const want of ['+ config scripts/wall.mjs', '+ config scripts/fixtures/sql/', '+ config Cargo.toml [profile.release.package.a]']) {
      if (!added.some((l) => l.startsWith(want))) problems.push(`${want.slice(9)} was not reported`);
    }
    write(dir);
    fs.writeFileSync(path.join(dir, 'scripts', 'wall.mjs'), "const COVERAGE = { lines: 85 };\nconst IMAGE = 'postgres:18@sha256:" + '1'.repeat(64) + "';\n");
    if (inventoryDiff(dir).length !== 0) problems.push('a moved image digest in a script was reported');
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
  report(problems, 'suppression selftest case(s) failed');
  console.log('suppression selftest: a new expect, ignore, rustfmt::skip and .trivyignore entry, an edited clippy.toml, mise.lock checksum, lint table, profile table, script and fixture tree are each reported; a moved digest is not');
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
