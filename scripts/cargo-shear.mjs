// cargo-shear: a declared dependency no code uses fails the build (errors by default; --deny-warnings makes
// the rest fail too). First, a canary: a workspace generated under target/shear-canary declares thiserror and
// never uses it, and cargo-shear must refuse it.
// Usage: node scripts/cargo-shear.mjs
import fs from 'node:fs';
import path from 'node:path';
import { captureAll, main, readText, run, Fail } from './_lib.mjs';

const repo = path.resolve(import.meta.dirname, '..');

main(() => {
  const work = path.join(repo, 'target', 'shear-canary');
  fs.rmSync(work, { recursive: true, force: true });
  fs.mkdirSync(path.join(work, 'src'), { recursive: true });
  const thiserror = /^thiserror = (.+)$/m.exec(readText(path.join(repo, 'Cargo.toml')))?.[1];
  if (!thiserror) throw new Fail('Cargo.toml pins no thiserror for the canary to declare');
  fs.writeFileSync(path.join(work, 'Cargo.toml'), `[package]\nname = "shear-canary"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n\n[workspace]\n\n[dependencies]\nthiserror = ${thiserror}\n`);
  fs.writeFileSync(path.join(work, 'src', 'lib.rs'), '//! Declares thiserror and never uses it.\n');
  fs.copyFileSync(path.join(repo, 'Cargo.lock'), path.join(work, 'Cargo.lock'));
  const canary = captureAll('cargo', ['shear', '--deny-warnings', '--offline', '--color', 'never'], { cwd: work });
  if (canary.status === 0 || !/unused dependency `thiserror`/.test(canary.stdout + canary.stderr)) {
    console.error(canary.stdout + canary.stderr);
    throw new Fail('cargo-shear did not refuse its canary, an unused thiserror');
  }
  console.log(`cargo-shear refused its canary (exit ${canary.status})`);
  run('cargo', ['shear', '--deny-warnings', '--locked', '--color', 'never'], { cwd: repo });
});
