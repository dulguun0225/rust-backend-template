// Mutation testing over the change: cargo-mutants --in-diff against the base branch, gated on its exit code.
// cargo-mutants reports baseline failure (4) before any timeout (3) before missed mutants (2), so one timeout
// hides every miss: the gate passes 0, passes 3 only when mutants.out/missed.txt exists and is empty, and fails
// everything else, reading the file right after the run. Before the real run, a canary crate generated under
// target/mutants-canary, whose one test cannot tell a mutant from the original, must be refused.
// Usage: node scripts/mutants.mjs [base-sha]
//   With no base (or an unknown one) the diff is the uncommitted change against HEAD, often empty.
import fs from 'node:fs';
import path from 'node:path';
import { capture, captureAll, main, ok, readText, Fail } from './_lib.mjs';

const repo = path.resolve(import.meta.dirname, '..');

/** The gate's verdict on one cargo-mutants run: null when it passes, else why it fails. */
export function verdict(status, outDir) {
  const missedFile = path.join(outDir, 'mutants.out', 'missed.txt');
  const missed = fs.existsSync(missedFile) ? readText(missedFile).trim() : null;
  if (status === 0) return null;
  if (status === 3 && missed === '') return null;
  if (status === 3) return missed === null ? 'timeouts, and no missed.txt to show no mutant was missed' : `timeouts hid missed mutants:\n${missed}`;
  if (status === 2) return `missed mutants:\n${missed ?? '(no missed.txt)'}`;
  if (status === 4) return 'the baseline build or tests failed before any mutant ran';
  return `cargo-mutants exited ${status}`;
}

function mutants(cwd, outDir, extra, env) {
  fs.rmSync(path.join(outDir, 'mutants.out'), { recursive: true, force: true });
  fs.mkdirSync(outDir, { recursive: true });
  const r = captureAll('cargo', ['mutants', '--output', outDir, '--no-shuffle', '--colors', 'never', ...extra], { cwd, env });
  process.stdout.write(r.stdout.split('\n').filter((l) => /MISSED|TIMEOUT|mutants tested|Found \d+ mutant|No mutants/.test(l)).join('\n') + '\n');
  return { status: r.status, why: verdict(r.status, outDir), output: r.stdout + r.stderr };
}

function selftest() {
  const dir = path.join(repo, 'target', 'mutants-selftest');
  const cases = [
    [0, null, true],
    [3, '', true],
    [3, 'src/lib.rs:1:1: replace f with ()\n', false],
    [3, null, false],
    [2, 'src/lib.rs:1:1: replace f with ()\n', false],
    [4, null, false],
    [1, null, false],
  ];
  for (const [status, missed, passes] of cases) {
    fs.rmSync(dir, { recursive: true, force: true });
    fs.mkdirSync(path.join(dir, 'mutants.out'), { recursive: true });
    if (missed !== null) fs.writeFileSync(path.join(dir, 'mutants.out', 'missed.txt'), missed);
    if ((verdict(status, dir) === null) !== passes) throw new Fail(`verdict(${status}, missed.txt ${JSON.stringify(missed)}) should ${passes ? 'pass' : 'fail'}`);
  }
}

function canary() {
  const dir = path.join(repo, 'target', 'mutants-canary');
  fs.rmSync(dir, { recursive: true, force: true });
  fs.mkdirSync(path.join(dir, 'src'), { recursive: true });
  fs.writeFileSync(path.join(dir, 'Cargo.toml'), '[package]\nname = "mutants-canary"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n\n[workspace]\n');
  fs.writeFileSync(
    path.join(dir, 'src', 'lib.rs'),
    '//! A function whose one test cannot tell a mutant from it.\n\npub fn add(a: u8, b: u8) -> Option<u8> {\n    a.checked_add(b)\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn adds() {\n        assert!(super::add(1, 2).is_some());\n    }\n}\n',
  );
  const run = mutants(dir, path.join(dir, 'out'), [], {});
  if (run.why === null) throw new Fail(`cargo-mutants passed its canary, whose test misses mutants (exit ${run.status})`);
  console.log(`cargo-mutants refused its canary (exit ${run.status}): ${run.why.split('\n')[0]}`);
}

main(() => {
  selftest();
  console.log('mutants verdict selftest: 0 and 3-with-empty-missed.txt pass; 2, 4, 3 with misses or no file, and 1 fail');
  canary();
  const base = process.argv[2] ?? '';
  const known = base && !/^0+$/.test(base) && ok('git', ['cat-file', '-e', `${base}^{commit}`], { cwd: repo });
  // --relative: vendored as backend/, paths are relative to this directory, as cargo-mutants reads them.
  const diff = known ? capture('git', ['diff', '--relative', `${base}...HEAD`], { cwd: repo }) : capture('git', ['diff', '--relative', 'HEAD'], { cwd: repo });
  const diffFile = path.join(repo, 'target', 'mutants.diff');
  fs.mkdirSync(path.dirname(diffFile), { recursive: true });
  fs.writeFileSync(diffFile, diff ? `${diff}\n` : '');
  if (!/^\+\+\+ b\/crates\/.*\.rs$/m.test(diff)) {
    console.log(`mutants: the diff ${known ? `against ${base.slice(0, 12)}` : 'against HEAD'} changes no Rust under crates/; nothing to mutate`);
    return;
  }
  const run = mutants(repo, path.join(repo, 'target', 'mutants'), ['--in-diff', diffFile, '--test-workspace=true'], { SQLX_OFFLINE: 'true' });
  if (run.why !== null) {
    console.error(run.output.split('\n').slice(-30).join('\n'));
    throw new Fail(`mutation testing failed: ${run.why}`);
  }
  console.log(`mutants: every mutant in the diff was caught (exit ${run.status})`);
});
