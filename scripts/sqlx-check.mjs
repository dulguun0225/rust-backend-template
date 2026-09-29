// The committed .sqlx query metadata against a freshly migrated database. The wall starts the server, applies
// the migrations, and runs this with DATABASE_URL set; every other build reads .sqlx with SQLX_OFFLINE=true,
// because a present DATABASE_URL, one from a .env file included, takes precedence over .sqlx.
//
// `cargo sqlx prepare --check` regenerates the metadata under target/sqlx-prepare-check and fails when .sqlx
// lacks a query or a file differs. It passes, with a warning, when .sqlx holds a query the regeneration did not
// produce, and an empty regeneration is such a pass (sqlx issues #1470, #4117). So after it, this script
// compares the two directories itself — the same file names, JSON-equal contents — and fails on any difference
// or on either warning. Its own comparison is shown failing each run, against an altered copy of .sqlx and
// against an empty regeneration.
// Usage: DATABASE_URL=… node scripts/sqlx-check.mjs
import fs from 'node:fs';
import path from 'node:path';
import { captureAll, main, readText, Fail } from './_lib.mjs';

const repo = path.resolve(import.meta.dirname, '..');

function load(dir) {
  const out = new Map();
  if (!fs.existsSync(dir)) return out;
  for (const f of fs.readdirSync(dir).filter((n) => /^query-[0-9a-f]{64}\.json$/.test(n))) out.set(f, JSON.parse(readText(path.join(dir, f))));
  return out;
}

const canonical = (v) => JSON.stringify(v, (_, x) => (x && typeof x === 'object' && !Array.isArray(x) ? Object.fromEntries(Object.entries(x).sort()) : x));

export function compare(committed, regenerated) {
  const problems = [];
  if (regenerated.size === 0) problems.push('the regeneration produced no query metadata at all');
  for (const f of regenerated.keys()) if (!committed.has(f)) problems.push(`.sqlx lacks ${f}`);
  for (const f of committed.keys()) if (!regenerated.has(f)) problems.push(`.sqlx holds ${f}, which no query produced`);
  for (const [f, v] of regenerated) if (committed.has(f) && canonical(committed.get(f)) !== canonical(v)) problems.push(`.sqlx/${f} differs from the regenerated metadata`);
  return problems;
}

main(() => {
  if (!process.env.DATABASE_URL) throw new Fail('DATABASE_URL is not set: run from the wall, or against `node scripts/db.mjs start`');
  const r = captureAll('cargo', ['sqlx', 'prepare', '--workspace', '--check', '--', '--all-targets', '--all-features'], { cwd: repo, env: { SQLX_OFFLINE: 'false' } });
  process.stdout.write(r.stdout);
  process.stderr.write(r.stderr);
  if (r.status !== 0) throw new Fail(`cargo sqlx prepare --check exited ${r.status}: .sqlx is stale; run node scripts/db.mjs prepare`, r.status);
  if (/potentially unused queries|no queries found/.test(r.stdout + r.stderr)) throw new Fail('cargo sqlx prepare --check passed with a warning, which this wall treats as a failure');

  const committed = load(path.join(repo, '.sqlx'));
  const regenerated = load(path.join(repo, 'target', 'sqlx-prepare-check'));
  // The comparison's own canaries: an altered copy and an empty regeneration must each be refused.
  const altered = new Map(committed);
  const [first] = committed.keys();
  if (!first) throw new Fail('.sqlx is empty');
  altered.set(first, { ...committed.get(first), describe: { ...committed.get(first).describe, nullable: ['canary'] } });
  if (compare(altered, regenerated).length === 0) throw new Fail('the comparison passed an altered .sqlx file');
  if (compare(committed, new Map()).length === 0) throw new Fail('the comparison passed an empty regeneration');

  const problems = compare(committed, regenerated);
  if (problems.length > 0) {
    for (const p of problems) console.error(p);
    throw new Fail(`${problems.length} difference(s) between .sqlx and the regenerated metadata; run node scripts/db.mjs prepare`);
  }
  console.log(`sqlx: ${committed.size} committed queries equal the metadata regenerated against the migrated schema`);
});
