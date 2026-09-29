// An applied migration is never edited: sqlx refuses to start against a database whose applied migration's
// checksum differs from the file (VersionMismatch), but only when migrations run, so the build compares the
// migrations against the base branch instead. Rules:
//   edited        a migration the base has differs from it byte for byte
//   deleted       a migration the base has is gone
//   out-of-order  a new migration sorts before the base's latest one: a database already past it would apply
//                 it out of order
// Usage: node scripts/check-applied-migrations.mjs [base-sha] [--selftest]
//   With no base (or an unknown one) there is nothing to compare, and it says so.
//   --selftest builds a throwaway git repository and checks each rule fires, and a plain addition passes.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { capture, captureAll, lines, main, ok, report, run, Fail } from './_lib.mjs';

export function appliedFindings(root, base) {
  const listed = lines(capture('git', ['ls-tree', '-r', '--name-only', base, '--', 'migrations/'], { cwd: root })).filter((f) => f.endsWith('.sql'));
  const findings = [];
  for (const f of listed) {
    const full = path.join(root, f);
    if (!fs.existsSync(full)) {
      findings.push(`deleted ${f}: applied on the base branch and gone here`);
      continue;
    }
    // `./`: relative to this directory, which is backend/ when vendored, not to the repository root.
    const before = captureAll('git', ['show', `${base}:./${f}`], { cwd: root }).stdout;
    if (before !== fs.readFileSync(full, 'utf8')) findings.push(`edited ${f}: differs from the base branch; a change is a new migration`);
  }
  const latest = listed.map((f) => path.basename(f)).sort().at(-1);
  if (latest) {
    const now = fs.existsSync(path.join(root, 'migrations')) ? fs.readdirSync(path.join(root, 'migrations')).filter((f) => f.endsWith('.sql')) : [];
    const known = new Set(listed.map((f) => path.basename(f)));
    for (const f of now) if (!known.has(f) && f < latest) findings.push(`out-of-order migrations/${f}: sorts before migrations/${latest}, which the base already has`);
  }
  return findings;
}

function selftest() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'applied-migrations-'));
  const git = (...args) => run('git', ['-c', 'user.name=selftest', '-c', 'user.email=selftest@example.invalid', '-c', 'commit.gpgsign=false', ...args], { cwd: dir });
  const problems = [];
  try {
    git('init', '-q', '-b', 'main');
    fs.mkdirSync(path.join(dir, 'migrations'));
    fs.writeFileSync(path.join(dir, 'migrations', '0001_a.sql'), 'create table a (id uuid primary key);\n');
    fs.writeFileSync(path.join(dir, 'migrations', '0002_b.sql'), 'create table b (id uuid primary key);\n');
    git('add', '-A');
    git('commit', '-q', '-m', 'base');
    const base = capture('git', ['rev-parse', 'HEAD'], { cwd: dir });
    const expect = (label, rule) => {
      const rules = new Set(appliedFindings(dir, base).map((f) => f.split(' ')[0]));
      const want = rule ? [rule] : [];
      if ([...rules].join() !== want.join()) problems.push(`${label}: expected ${rule ?? 'nothing'}, got ${[...rules].join(', ') || 'nothing'}`);
    };
    fs.writeFileSync(path.join(dir, 'migrations', '0003_c.sql'), 'create table c (id uuid primary key);\n');
    expect('a new migration after the base', null);
    fs.appendFileSync(path.join(dir, 'migrations', '0001_a.sql'), 'alter table a add column n int;\n');
    expect('an edited applied migration', 'edited');
    run('git', ['checkout', '-q', '--', 'migrations/0001_a.sql'], { cwd: dir });
    fs.rmSync(path.join(dir, 'migrations', '0002_b.sql'));
    expect('a deleted applied migration', 'deleted');
    run('git', ['checkout', '-q', '--', 'migrations/0002_b.sql'], { cwd: dir });
    fs.writeFileSync(path.join(dir, 'migrations', '0000_early.sql'), 'create table e (id uuid primary key);\n');
    expect('a new migration before the latest applied one', 'out-of-order');
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
  report(problems, 'applied-migration selftest case(s) failed');
  console.log('applied-migration selftest: edited, deleted and out-of-order each refused; a plain addition passes');
}

main(() => {
  const args = process.argv.slice(2);
  if (args.includes('--selftest')) return selftest();
  const root = path.resolve(import.meta.dirname, '..');
  const base = args[0] ?? '';
  if (!base || /^0+$/.test(base) || !ok('git', ['cat-file', '-e', `${base}^{commit}`], { cwd: root })) {
    console.log('applied migrations: no base commit given or known, nothing to compare against');
    return;
  }
  report(appliedFindings(root, base), 'applied-migration finding(s)');
  console.log(`applied migrations: every migration on ${base.slice(0, 12)} is unchanged here, and every new one sorts after them`);
});
