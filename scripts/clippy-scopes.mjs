// Generates each crate-local clippy.toml from the root one minus the exemptions clippy-scopes.toml declares,
// or, without --write, reports every crate-local file that differs. check-lint-config.mjs runs the check.
//   node scripts/clippy-scopes.mjs [--write] [--root <dir>]
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { main, report, Fail } from './_lib.mjs';
import { checkScopes, expectedScopes } from './_scopes.mjs';

main(() => {
  let opts;
  try {
    ({ values: opts } = parseArgs({ options: { write: { type: 'boolean' }, root: { type: 'string' } } }));
  } catch (e) {
    throw new Fail(e.message, 2);
  }
  const root = path.resolve(opts.root ?? path.join(import.meta.dirname, '..'));
  if (opts.write) {
    const { files, problems } = expectedScopes(root);
    report(problems, 'problem(s) in clippy-scopes.toml');
    for (const [f, text] of Object.entries(files)) fs.writeFileSync(path.join(root, f), text);
    console.log(`wrote ${Object.keys(files).join(', ')}`);
    return;
  }
  report(checkScopes(root), 'crate-local clippy.toml problem(s)');
  console.log('every crate-local clippy.toml is the root ban list minus its declared exemptions');
});
