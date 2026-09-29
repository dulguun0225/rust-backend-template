// squawk over the migrations a change adds or edits, gated on exit code (squawk 2.66.0 exits 1 on any
// finding and 0 on none, checked 2026-09-29). sqlx runs each migration in a transaction unless its first line
// is `-- no-transaction`, so the others are linted with --assume-in-transaction. Before the real run, squawk
// must refuse scripts/fixtures/squawk/bad.sql: a linter that passes its own negative fixture gates nothing.
// Usage: node scripts/squawk-changed-migrations.mjs [base-sha]   (no base, or an unknown one: every file in migrations/)
// The binary comes from mise.toml's exact pin (`mise install`).
import fs from 'node:fs';
import path from 'node:path';
import { capture, captureAll, lines, main, ok, run, Fail } from './_lib.mjs';

function squawk(files, inTransaction) {
  const args = ['-c', '.squawk.toml', ...(inTransaction ? ['--assume-in-transaction'] : ['--no-assume-in-transaction']), ...files];
  try {
    return run('squawk', args, { check: false });
  } catch (e) {
    if (e instanceof Fail && e.message.endsWith('not on PATH')) throw new Fail('squawk not on PATH; run `mise install` (mise.toml pins it)');
    throw e;
  }
}

main(() => {
  process.chdir(path.resolve(import.meta.dirname, '..'));
  const canary = captureAll('squawk', ['-c', '.squawk.toml', '--assume-in-transaction', 'scripts/fixtures/squawk/bad.sql']);
  if (canary.status === 0) throw new Fail('squawk passed scripts/fixtures/squawk/bad.sql: it would pass every migration');
  console.log(`squawk refused its negative fixture (exit ${canary.status})`);

  const base = process.argv[2] ?? '';
  const known = base && !/^0+$/.test(base) && ok('git', ['cat-file', '-e', `${base}^{commit}`]);
  const files = (
    known
      ? lines(capture('git', ['diff', '--relative', '--name-only', '--diff-filter=AM', `${base}...HEAD`, '--', 'migrations/']))
      : fs.readdirSync('migrations').map((f) => `migrations/${f}`)
  ).filter((f) => f.endsWith('.sql') && fs.existsSync(f));
  if (files.length === 0) {
    console.log('no migrations changed');
    return;
  }
  const outside = files.filter((f) => /^--\s*no-transaction/.test(fs.readFileSync(f, 'utf8')));
  const inside = files.filter((f) => !outside.includes(f));
  console.log(`linting ${files.length} migration(s)`);
  const status = Math.max(inside.length ? squawk(inside, true) : 0, outside.length ? squawk(outside, false) : 0);
  if (status !== 0) throw new Fail(`squawk exited ${status}`, status);
});
