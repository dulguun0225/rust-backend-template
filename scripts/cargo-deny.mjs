// cargo-deny over the workspace: advisories, licences, bans and sources (deny.toml), gated on exit code.
// First, a canary: a workspace generated under target/deny-canary with the root deny.toml, in which each check
// has something to refuse — a local advisory database whose one advisory covers itoa, a crate licensed
// GPL-3.0-only, anyhow outside its wrapper, a `*` requirement, uuid's v4 feature, and a git source — and the
// run must name every one. A check that is never seen failing is not counted as wired.
// Usage: node scripts/cargo-deny.mjs
import fs from 'node:fs';
import path from 'node:path';
import { capture, captureAll, main, readText, run, Fail } from './_lib.mjs';

const repo = path.resolve(import.meta.dirname, '..');
const work = path.join(repo, 'target', 'deny-canary');
const url = (p) => `file://${p.split(path.sep).join('/').replace(/^([A-Za-z]):/, '/$1:')}`;

function gitRepo(dir, files) {
  fs.mkdirSync(dir, { recursive: true });
  for (const [rel, text] of Object.entries(files)) {
    fs.mkdirSync(path.dirname(path.join(dir, rel)), { recursive: true });
    fs.writeFileSync(path.join(dir, rel), text);
  }
  const git = ['-c', 'user.name=canary', '-c', 'user.email=canary@example.invalid', '-c', 'commit.gpgsign=false'];
  run('git', [...git, 'init', '-q', '-b', 'main'], { cwd: dir });
  run('git', [...git, 'add', '-A'], { cwd: dir });
  run('git', [...git, 'commit', '-q', '-m', 'canary'], { cwd: dir });
}

function canary() {
  fs.rmSync(work, { recursive: true, force: true });
  gitRepo(path.join(work, 'tiny'), {
    'Cargo.toml': '[package]\nname = "tiny"\nversion = "0.1.0"\nedition = "2024"\nlicense = "MIT"\n',
    'src/lib.rs': '//! A crate from a git source.\n',
  });
  const lock = readText(path.join(repo, 'Cargo.lock'));
  const itoa = /name = "itoa"\nversion = "([^"]+)"/.exec(lock)?.[1];
  if (!itoa) throw new Fail('Cargo.lock has no itoa; the advisory canary needs a crate already in the graph');
  fs.mkdirSync(path.join(work, 'rogue', 'src'), { recursive: true });
  fs.writeFileSync(
    path.join(work, 'rogue', 'Cargo.toml'),
    [
      '[package]',
      'name = "rogue"',
      'version = "0.1.0"',
      'edition = "2024"',
      'license = "GPL-3.0-only"',
      '',
      '[workspace]',
      '',
      '[dependencies]',
      'anyhow = "=1.0.104"',
      'serde = "*"',
      `itoa = "=${itoa}"`,
      'uuid = { version = "=1.26.1", features = ["v4"] }',
      `tiny = { git = "${url(path.join(work, 'tiny'))}" }`,
      '',
    ].join('\n'),
  );
  fs.writeFileSync(path.join(work, 'rogue', 'src', 'lib.rs'), '//! The cargo-deny canary.\n');
  fs.copyFileSync(path.join(repo, 'Cargo.lock'), path.join(work, 'rogue', 'Cargo.lock'));
  // The canary's advisory database: cargo-deny accepts only a URL with a domain, and with --offline reads the
  // database already cloned under db-path at a directory named after the URL; the first offline run names
  // that directory, and the fixture advisory is committed into a git repository there.
  const config = readText(path.join(repo, 'deny.toml')).replace(
    /^\[advisories\]$/m,
    `[advisories]\ndb-path = "${path.join(work, 'dbs').split(path.sep).join('/')}"\ndb-urls = ["https://example.invalid/canary/advisory-db"]`,
  );
  fs.writeFileSync(path.join(work, 'deny.toml'), config);
  const deny = (...checks) =>
    captureAll('cargo', ['deny', '--offline', '--manifest-path', path.join(work, 'rogue', 'Cargo.toml'), '--config', path.join(work, 'deny.toml'), 'check', ...checks], { cwd: work });
  // Resolve the new dependencies against the copied lock, so every registry crate stays at the workspace's version.
  const resolved = captureAll('cargo', ['metadata', '--format-version', '1'], { cwd: path.join(work, 'rogue') });
  if (resolved.status !== 0) throw new Fail(`the cargo-deny canary workspace does not resolve:\n${resolved.stderr}`);
  const probe = deny('advisories');
  const dbDir = /([^\s"',[\]]*dbs[\\/]advisory-db-[0-9a-f]+)/.exec(probe.stderr)?.[1];
  if (!dbDir) throw new Fail(`cargo-deny did not name the advisory database directory it expects:\n${probe.stderr}`);
  if (!path.isAbsolute(dbDir) || !dbDir.startsWith(work)) throw new Fail(`cargo-deny named an advisory database outside the canary directory: ${dbDir}`);
  gitRepo(dbDir, { 'crates/itoa/RUSTSEC-2026-9999.md': readText(path.join(repo, 'scripts', 'fixtures', 'cargo-deny', 'advisory.md')) });
  const r = deny();
  const out = r.stderr + r.stdout;
  const expected = [
    ['advisories', /error\[vulnerability\]: Canary advisory/],
    ['licenses', /error\[rejected\]: failed to satisfy license requirements/],
    ['bans: wrappers', /error\[banned\]: crate 'anyhow = /],
    ['bans: wildcards', /error\[wildcard\]: found \d+ wildcard dependenc/],
    ['bans: features', /error\[feature-banned\]: feature 'v4' for crate 'uuid = /],
    ['sources', /error\[source-not-allowed\]: detected 'git' source/],
  ];
  const missing = expected.filter(([, re]) => !re.test(out)).map(([name]) => name);
  if (r.status === 0 || missing.length > 0) {
    console.error(out);
    throw new Fail(`the cargo-deny canary was not refused as expected (exit ${r.status}; not named: ${missing.join(', ') || 'none'})`);
  }
  console.log('cargo-deny refused its canary: an advisory, a licence, a wrapper, a wildcard, a feature and a git source');
}

main(() => {
  canary();
  run('cargo', ['deny', '--locked', '--all-features', 'check'], { cwd: repo });
  console.log(`cargo-deny: advisories, bans, licenses and sources ok (${capture('cargo', ['deny', '--version'], { cwd: repo })})`);
});
