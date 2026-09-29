// The backend wall, as one command: the definition of done. The template's own CI and a project's `backend`
// job both run exactly this, so the two cannot drift on what "green" means. Needs Docker (a throwaway
// PostgreSQL server, and the service image the scan reads), the toolchain rust-toolchain.toml pins (rustup), the
// tools mise.toml pins at the checksums mise.lock records (`mise install`), and network for crates the first build downloads and for trivy's
// vulnerability database.
// Usage: node scripts/wall.mjs [base-sha]
//   The base scopes squawk, the applied-migration check, the OpenAPI breaking-change diff, mutation testing and
//   the commit-range secrets scan to the change; without one, squawk lints every migration and the other four
//   have nothing to compare.
import fs from 'node:fs';
import path from 'node:path';
import { capture, captureAll, main, readText, run, Fail } from './_lib.mjs';
import { migrate, startPostgres, stopPostgres } from './_db.mjs';
import { parseToml } from './_toml.mjs';

const here = import.meta.dirname;
const root = path.resolve(here, '..');
const script = (name, ...args) => run(process.execPath, [path.join(here, name), ...args]);
const step = (title) => console.log(`\n==> ${title}`);

// The coverage floors: this service's call, each moved in one commit that says why.
const COVERAGE = { lines: 85, regions: 85, functions: 85 };

main(() => {
  process.chdir(root);
  const base = process.argv[2] ?? '';
  const started = Date.now();

  step('mise.lock: every tool mise.toml pins, at that version, with a checksum for each platform');
  script('check-mise-lock.mjs', '--selftest');
  script('check-mise-lock.mjs');

  step('Toolchain: rust-toolchain.toml, and the tools mise.toml pins');
  toolchain();

  step('Cargo.lock: current for every manifest, before any step that resolves could rewrite it');
  lockfile();

  step('Lint configuration: no lint, flag or profile override, members in crates/ inheriting the lints, no code clippy cannot read, no suppressed ban lint, scopes generated');
  script('check-lint-config.mjs', '--selftest');
  script('check-lint-config.mjs');

  step('Suppression inventory (suppressions.txt)');
  script('check-suppressions.mjs', '--selftest');
  script('check-suppressions.mjs');

  step('Secrets: gitleaks over the files git would commit and the commits since the base');
  script('secrets.mjs', base);

  step('Source rules: rustfmt, ast-grep serde rules');
  script('source-rules.mjs');

  step('SQL rules: table ownership, the guarded update, keyset order, no OFFSET, no SQL clock');
  script('check-sql.mjs', '--selftest');
  script('check-sql.mjs');

  step('Migrations: conventions, applied migrations unchanged, squawk');
  script('check-migrations.mjs', '--selftest');
  script('check-migrations.mjs');
  script('check-applied-migrations.mjs', '--selftest');
  script('check-applied-migrations.mjs', base);
  script('squawk-changed-migrations.mjs', base);

  step('Project checks: the scripts listed in <project root>/scripts/wall-checks.txt');
  projectChecks();

  step('Crate graph: layering, cargo-deny, cargo-shear');
  script('check-layering.mjs', '--selftest');
  script('check-layering.mjs');
  script('cargo-deny.mjs');
  script('cargo-shear.mjs');

  step('PostgreSQL: a throwaway server, migrated');
  const db = startPostgres();
  try {
    migrate(db.url, root);
    const withDb = { DATABASE_URL: db.url };

    step('sqlx: .sqlx matches the queries prepared against the migrated schema');
    run(process.execPath, [path.join(here, 'sqlx-check.mjs')], { env: withDb });
    script('check-sql.mjs', '--inventory');

    step('clippy: cargo clippy --workspace --all-targets --all-features --locked');
    clippy();

    step('Ban canaries: every clippy.toml entry and every denied or forbidden lint fires');
    run(process.execPath, [path.join(here, 'check-ban-canaries.mjs')], { env: withDb });

    step(`Tests on PostgreSQL with coverage floors: lines ${COVERAGE.lines}%, regions ${COVERAGE.regions}%, functions ${COVERAGE.functions}%`);
    coverage(withDb);

    step('OpenAPI: rerun under another timezone and locale, vacuum ruleset, oasdiff against the base');
    script('openapi.mjs', base);

    step('Mutation testing over the change (cargo-mutants --in-diff)');
    run(process.execPath, [path.join(here, 'mutants.mjs'), base], { env: withDb });
  } finally {
    stopPostgres(db.id);
  }

  step('Image: the Dockerfile built as <binary>:wall, trivy over it; HIGH and CRITICAL fail, fixed or not');
  script('image-scan.mjs', '--selftest');
  script('image-scan.mjs');

  step(`backend wall green in ${Math.round((Date.now() - started) / 1000)} s`);
});

function toolchain() {
  const channel = parseToml(readText(path.join(root, 'rust-toolchain.toml')), 'rust-toolchain.toml').toolchain.channel;
  const rustc = capture('rustc', ['--version']);
  if (!rustc.startsWith(`rustc ${channel} `)) throw new Fail(`${rustc} is not the pinned ${channel}; install rustup, which reads rust-toolchain.toml`);
  const missing = [];
  for (const [cmd, args] of [
    ['squawk', ['--version']],
    ['ast-grep', ['--version']],
    ['vacuum', ['version']],
    ['gitleaks', ['version']],
    ['trivy', ['--version']],
    ['oasdiff', ['--version']],
    ['sqlx', ['--version']],
    ['cargo', ['deny', '--version']],
    ['cargo', ['shear', '--version']],
    ['cargo', ['llvm-cov', '--version']],
    ['cargo', ['mutants', '--version']],
    ['docker', ['--version']],
  ]) {
    try {
      if (captureAll(cmd, args).status !== 0) missing.push(`${cmd} ${args[0]}`);
    } catch {
      missing.push(cmd);
    }
  }
  if (missing.length > 0) throw new Fail(`not on PATH: ${missing.join(', ')}; run \`mise install\` (mise.toml pins them)`);
  console.log(`${rustc}; every pinned tool present`);
}

/**
 * `cargo metadata --locked` over the workspace, first: a step that resolves without --locked (cargo-deny's did)
 * rewrites a stale Cargo.lock, and every --locked step after it then passes over a lock the commit lacks.
 * Its canary: a workspace whose manifest declares a crate its Cargo.lock lacks must be refused.
 */
function lockfile() {
  const work = path.join(root, 'target', 'lock-canary');
  fs.rmSync(work, { recursive: true, force: true });
  fs.mkdirSync(path.join(work, 'src'), { recursive: true });
  fs.writeFileSync(path.join(work, 'Cargo.toml'), '[package]\nname = "lock-canary"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n\n[workspace]\n\n[dependencies]\nitoa = "1"\n');
  fs.writeFileSync(path.join(work, 'src', 'lib.rs'), '');
  fs.writeFileSync(path.join(work, 'Cargo.lock'), 'version = 4\n');
  const canary = captureAll('cargo', ['metadata', '--locked', '--offline', '--format-version', '1'], { cwd: work });
  if (canary.status === 0 || !/--locked was passed/.test(canary.stderr)) {
    throw new Fail(`cargo metadata --locked did not refuse a Cargo.lock that lacks a declared crate (exit ${canary.status}):\n${canary.stderr}`);
  }
  const r = captureAll('cargo', ['metadata', '--locked', '--format-version', '1']);
  if (r.status !== 0) throw new Fail(`Cargo.lock is not current for the manifests; run cargo to update it and commit it:\n${r.stderr}`, r.status);
  console.log('Cargo.lock is current; the canary, a lock that lacks a declared crate, was refused');
}

function clippy() {
  const r = captureAll('cargo', ['clippy', '--workspace', '--all-targets', '--all-features', '--locked'], { env: { SQLX_OFFLINE: 'true' } });
  process.stderr.write(r.stderr);
  if (r.status !== 0) throw new Fail(`cargo clippy exited ${r.status}`, r.status);
  // A misspelled ban path is only this warning, exit 0 even under -D warnings.
  if (/does not refer to/.test(r.stderr)) throw new Fail('clippy.toml names a path that does not refer to anything (see the warning above)');
  // Every warning is denied, so a warning that still prints is a lint someone lowered to warn, or cargo
  // setting aside part of the configuration; either way clippy exited 0 over it.
  if (/^warning: /m.test(r.stderr)) throw new Fail('cargo clippy printed a warning (see above): with every warning denied, one that prints was lowered, or cargo ignored configuration');
}

function coverage(env) {
  const floors = ['--fail-under-lines', `${COVERAGE.lines}`, '--fail-under-regions', `${COVERAGE.regions}`, '--fail-under-functions', `${COVERAGE.functions}`];
  run('cargo', ['llvm-cov', '--workspace', '--all-targets', '--all-features', '--locked', ...floors], { env: { ...env, SQLX_OFFLINE: 'true' } });
  // The floors' canary: the same report, held to a floor no run can meet, must fail.
  const canary = captureAll('cargo', ['llvm-cov', 'report', '--fail-under-lines', '101'], { env: { SQLX_OFFLINE: 'true' } });
  if (canary.status === 0) throw new Fail('cargo llvm-cov report passed a 101% line floor: the coverage floors fail nothing');
  console.log(`cargo llvm-cov refused a 101% line floor (exit ${canary.status})`);
}

/**
 * Runs the Node scripts a project lists in scripts/wall-checks.txt under its root, in file order, with the
 * project root as the working directory. The project root is the parent of this service directory when the
 * service is vendored into a project (the directory is not the git toplevel), otherwise this directory. One
 * path per line, relative to the project root; blank lines and lines starting with # are skipped. A listed
 * path that does not exist fails the wall, as does a script that exits non-zero. No list, no checks.
 */
function projectChecks() {
  const service = fs.realpathSync(root);
  const top = fs.realpathSync(capture('git', ['rev-parse', '--show-toplevel'], { cwd: service }));
  const projectRoot = path.relative(top, service) === '' ? service : path.dirname(service);
  const list = path.join(projectRoot, 'scripts', 'wall-checks.txt');
  if (!fs.existsSync(list)) {
    console.log(`no project checks listed (${list} does not exist)`);
    return;
  }
  const entries = readText(list)
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l !== '' && !l.startsWith('#'));
  for (const entry of entries) {
    const file = path.resolve(projectRoot, entry);
    if (!fs.existsSync(file)) throw new Fail(`${list}: ${entry} does not exist under ${projectRoot}`);
    console.log(`-- ${entry}`);
    run(process.execPath, [file], { cwd: projectRoot });
  }
  if (entries.length === 0) console.log(`no project checks listed (${list} lists none)`);
}
