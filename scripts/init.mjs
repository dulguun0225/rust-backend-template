// Turn the template into a service: the service's name, wherever it is a name. Run once, then commit.
//
//   node scripts/init.mjs --name some_service_1 [--standalone]
//
// --name is the only project input: the binary, the OpenAPI document's title, the image and the local
// database. It must be a crate-name-safe identifier: a lowercase letter, then lowercase letters, digits or
// underscores, 64 characters at most, and none of the names the workspace, Rust or Cargo already use.
//
// Two modes:
//   vendored (default)  this directory is <project>/backend (added with `git subtree add --prefix backend ...`):
//                       rename, then lift project-root/ one level up — root CI with backend and frontend jobs,
//                       rulesets, compose, the frontend/ stub, the project CLAUDE.md, the root scripts — never
//                       overwriting a file that exists, and remove the template's own .github/ and
//                       renovate.json, which only mean something at a root.
//   --standalone        this directory is the repository root: rename, and move the base branch from main to
//                       dev in the root CLAUDE.md and the CI trigger, since a service works on dev and this
//                       template on main. project-root/ stays: the CI checks its workflow's action pins.
// The rename changes no Rust line's width past rustfmt's limit, so `cargo fmt --all` leaves the tree as it is;
// the printed next step runs it anyway, then the wall. Everything else, the gates and the scripts, is
// deliberately identical across services.
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { capture, main, Fail } from './_lib.mjs';

const OLD = 'starter';

const RESERVED = new Set([
  // Rust keywords, strict and reserved
  'as', 'async', 'await', 'break', 'const', 'continue', 'crate', 'dyn', 'else', 'enum', 'extern', 'false', 'fn', 'for',
  'gen', 'if', 'impl', 'in', 'let', 'loop', 'match', 'mod', 'move', 'mut', 'pub', 'ref', 'return', 'self', 'static',
  'struct', 'super', 'trait', 'true', 'try', 'type', 'unsafe', 'use', 'where', 'while', 'abstract', 'become', 'box',
  'do', 'final', 'macro', 'override', 'priv', 'typeof', 'unsized', 'virtual', 'yield',
  // the standard crates
  'std', 'core', 'alloc', 'test', 'proc_macro',
  // Cargo's own directories under target/<profile>/, which a binary of that name would collide with
  'build', 'deps', 'examples', 'incremental',
]);

/** [file, pattern, replacement]: every place the service's name is a name. Each must match at least once. */
function renames(name) {
  const at = (re) => new RegExp(re, 'gm');
  return [
    ['crates/server/Cargo.toml', at(`^(\\[\\[bin\\]\\]\\nname = )"${OLD}"$`), `$1"${name}"`],
    ['crates/api/src/lib.rs', at(`^pub const SERVICE_NAME: &str = "${OLD}";$`), `pub const SERVICE_NAME: &str = "${name}";`],
    ['openapi/v1.json', at(`^(\\s*)"title": "${OLD}",$`), `$1"title": "${name}",`],
    ['Dockerfile', at(`--bin ${OLD}$`), `--bin ${name}`],
    ['Dockerfile', at(`target/release/${OLD} /app/${OLD}$`), `target/release/${name} /app/${name}`],
    ['Dockerfile', at(`^ENTRYPOINT \\["/app/${OLD}"\\]$`), `ENTRYPOINT ["/app/${name}"]`],
    ['project-root/compose.yaml', at(`^name: ${OLD}$`), `name: ${name}`],
    ['project-root/compose.yaml', at(`^(\\s+POSTGRES_(?:DB|USER|PASSWORD)): ${OLD}$`), `$1: ${name}`],
    ['project-root/compose.yaml', at(`pg_isready -U ${OLD} -d ${OLD}`), `pg_isready -U ${name} -d ${name}`],
    ['project-root/compose.yaml', at(`postgres://${OLD}:${OLD}@postgres:5432/${OLD}$`), `postgres://${name}:${name}@postgres:5432/${name}`],
    ['project-root/compose.yaml', at(`^(\\s+image): ${OLD}:dev$`), `$1: ${name}:dev`],
  ];
}

main(() => {
  const root = path.resolve(import.meta.dirname, '..');
  process.chdir(root);
  let opts;
  try {
    ({ values: opts } = parseArgs({ options: { name: { type: 'string' }, standalone: { type: 'boolean' } } }));
  } catch (e) {
    throw new Fail(e.message, 2);
  }
  const name = opts.name ?? '';
  if (!/^[a-z][a-z0-9_]{0,63}$/.test(name)) throw new Fail('--name must be a lowercase letter, then lowercase letters, digits or underscores, 64 characters at most', 2);
  const crates = fs.readdirSync('crates').map((c) => c.replaceAll('-', '_'));
  if (RESERVED.has(name) || crates.includes(name)) throw new Fail(`--name ${name} is a name Rust, Cargo or this workspace already uses`, 2);

  const mode = opts.standalone ? 'standalone' : 'vendored';
  const top = fs.realpathSync(capture('git', ['rev-parse', '--show-toplevel']));
  const here = fs.realpathSync(root);
  if (mode === 'standalone' && path.relative(top, here) !== '') throw new Fail(`--standalone expects this directory to be the repository root (${top})`, 2);
  if (mode === 'vendored' && path.relative(top, path.dirname(here)) !== '') {
    throw new Fail(`vendored mode expects this directory to sit directly under the repository root (${top}); pass --standalone for a repository that is the service`, 2);
  }
  if (!fs.existsSync('project-root')) throw new Fail('project-root/ is gone: this template has already been initialised in vendored mode', 2);

  // Read every file before writing any, so a missing target stops the run with nothing changed. A standalone
  // service also moves its base branch from main to dev in the root CLAUDE.md and the CI trigger.
  const branch = [
    ['CLAUDE.md', /^Base branch: `main`$/gm, 'Base branch: `dev`'],
    ['.github/workflows/ci.yml', /^ {4}branches: \[main\]$/gm, '    branches: [dev, main]'],
  ];
  const edits = new Map();
  for (const [file, re, to] of [...renames(name), ...(mode === 'standalone' ? branch : [])]) {
    if (!fs.existsSync(file)) throw new Fail(`${file} is missing: the template changed under this script`);
    const before = edits.get(file) ?? fs.readFileSync(file, 'utf8');
    if (!re.test(before)) throw new Fail(`${file} does not carry ${re}: the template changed under this script`);
    re.lastIndex = 0;
    edits.set(file, before.replace(re, to));
  }
  for (const [file, text] of edits) fs.writeFileSync(file, text);
  console.log(`renamed: ${name} (${mode}): ${[...edits.keys()].join(', ')}`);

  if (mode === 'vendored') {
    for (const f of fs.readdirSync('project-root', { recursive: true, withFileTypes: true })) {
      if (!f.isFile()) continue;
      const rel = path.relative('project-root', path.join(f.parentPath, f.name));
      const dest = path.join('..', rel);
      if (fs.existsSync(dest)) {
        console.log(`kept existing ../${rel.split(path.sep).join('/')} (template copy not applied)`);
        continue;
      }
      fs.mkdirSync(path.dirname(dest), { recursive: true });
      const source = path.join(f.parentPath, f.name);
      fs.copyFileSync(source, dest);
      fs.chmodSync(dest, fs.statSync(source).mode);
    }
    for (const p of ['project-root', '.github', 'renovate.json']) fs.rmSync(p, { recursive: true, force: true });
    console.log(`lifted project-root/ to ${path.dirname(here)}; removed the template's own .github/ and renovate.json from ${path.basename(here)}/`);
    console.log('next, here: cargo fmt --all && node scripts/wall.mjs; then at the project root: git add -A, commit, node scripts/apply-ruleset.mjs');
  } else {
    console.log('base branch: dev (CLAUDE.md, .github/workflows/ci.yml); create dev and make it the default branch on the forge');
    console.log('next: cargo fmt --all && node scripts/wall.mjs, then commit');
  }
});
