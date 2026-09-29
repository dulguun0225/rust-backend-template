// The crate graph against layering.toml. Rules:
//   undeclared-edge    a workspace crate declares a first-party crate its entry does not list
//   controlled-crate   a crate declares a controlled external crate (sqlx, axum, anyhow, …) it is not listed for
//   unlisted-crate     a workspace member with no [crates.<name>] entry
//   stale-entry        an entry naming no workspace member
// Usage: node scripts/check-layering.mjs [--selftest]
//   --selftest runs scripts/fixtures/layering/<rule>/ (metadata.json + layering.toml): each must produce its
//   rule alone; `good` must produce none.
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { captureAll, main, readText, report, Fail } from './_lib.mjs';
import { parseToml } from './_toml.mjs';

export function layeringFindings(metadata, layering) {
  const findings = [];
  const members = new Set(metadata.packages.map((p) => p.name));
  const crates = layering.crates ?? {};
  const controlled = layering.controlled ?? {};
  for (const name of Object.keys(crates)) if (!members.has(name)) findings.push(`stale-entry [crates.${name}] names no workspace member`);
  for (const pkg of metadata.packages) {
    const entry = crates[pkg.name];
    if (!entry) {
      findings.push(`unlisted-crate ${pkg.name} has no [crates.${pkg.name}] entry in layering.toml`);
      continue;
    }
    for (const dep of pkg.dependencies) {
      const kind = dep.kind === 'dev' ? 'dev' : 'normal';
      if (members.has(dep.name)) {
        const allowed = kind === 'dev' ? [...(entry.depends ?? []), ...(entry.dev ?? [])] : entry.depends ?? [];
        if (!allowed.includes(dep.name)) findings.push(`undeclared-edge ${pkg.name} -> ${dep.name} (${kind})`);
      } else if (dep.name in controlled) {
        const rule = controlled[dep.name];
        const allowed = kind === 'dev' ? [...(rule.normal ?? []), ...(rule.dev ?? [])] : rule.normal ?? [];
        if (!allowed.includes(pkg.name)) findings.push(`controlled-crate ${pkg.name} declares ${dep.name} (${kind}); only ${allowed.join(', ') || 'no crate'} may`);
      }
    }
  }
  return findings;
}

function selftest(repo) {
  const dir = path.join(repo, 'scripts', 'fixtures', 'layering');
  const cases = fs.readdirSync(dir).sort();
  const problems = [];
  for (const name of cases) {
    const metadata = JSON.parse(readText(path.join(dir, name, 'metadata.json')));
    const layering = parseToml(readText(path.join(dir, name, 'layering.toml')), `${name}/layering.toml`);
    const rules = new Set(layeringFindings(metadata, layering).map((f) => f.split(' ')[0]));
    if (name === 'good' ? rules.size !== 0 : rules.size !== 1 || !rules.has(name)) problems.push(`fixture ${name}: got ${[...rules].join(', ') || 'nothing'}`);
  }
  const expected = ['controlled-crate', 'good', 'stale-entry', 'undeclared-edge', 'unlisted-crate'];
  for (const e of expected) if (!cases.includes(e)) problems.push(`fixture ${e} is missing`);
  report(problems, 'layering fixture(s) not refused as expected');
  console.log(`layering selftest: ${cases.length} fixtures, each refused by its rule alone (good: none)`);
}

main(() => {
  let opts;
  try {
    ({ values: opts } = parseArgs({ options: { selftest: { type: 'boolean' } } }));
  } catch (e) {
    throw new Fail(e.message, 2);
  }
  const repo = path.resolve(import.meta.dirname, '..');
  if (opts.selftest) return selftest(repo);
  const r = captureAll('cargo', ['metadata', '--no-deps', '--format-version', '1', '--offline'], { cwd: repo });
  if (r.status !== 0) throw new Fail(`cargo metadata failed:\n${r.stderr}`);
  const findings = layeringFindings(JSON.parse(r.stdout), parseToml(readText(path.join(repo, 'layering.toml')), 'layering.toml'));
  report(findings, 'layering finding(s)');
  console.log('crate graph: every first-party edge and every controlled external crate is declared in layering.toml');
});
