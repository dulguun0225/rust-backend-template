// The source rules that need no build: formatting and the ast-grep serde rules.
//   rustfmt   `cargo fmt --all --check`, after `rustfmt --check` refuses scripts/fixtures/rustfmt/unformatted.rs
//   ast-grep  `ast-grep test` runs every rule over its valid and invalid cases (rules/ast-grep-tests); then
//             `ast-grep scan` must exit 1 on scripts/fixtures/ast-grep/violations.rs, since a rule below
//             severity error would exit 0; then `ast-grep scan crates`.
// Usage: node scripts/source-rules.mjs
import path from 'node:path';
import { captureAll, main, run, Fail } from './_lib.mjs';

const repo = path.resolve(import.meta.dirname, '..');

main(() => {
  const fmtCanary = captureAll('rustfmt', ['--check', '--edition', '2024', 'scripts/fixtures/rustfmt/unformatted.rs'], { cwd: repo });
  if (fmtCanary.status === 0) throw new Fail('rustfmt --check passed scripts/fixtures/rustfmt/unformatted.rs');
  console.log(`rustfmt refused its unformatted fixture (exit ${fmtCanary.status})`);
  run('cargo', ['fmt', '--all', '--check'], { cwd: repo });
  console.log('cargo fmt --check: the tree is formatted');

  run('ast-grep', ['test', '--skip-snapshot-tests'], { cwd: repo });
  const scanCanary = captureAll('ast-grep', ['scan', 'scripts/fixtures/ast-grep/violations.rs'], { cwd: repo });
  if (scanCanary.status !== 1) throw new Fail(`ast-grep scan exited ${scanCanary.status} on its violations fixture; it must exit 1`);
  console.log('ast-grep scan exits 1 on its violations fixture');
  run('ast-grep', ['scan', 'crates'], { cwd: repo });
  console.log('ast-grep: no serde rule matched under crates/');
});
