// Guards on the lint configuration itself: the routes by which a lint level is lowered without an error.
//
//   cap-lints            `--cap-lints` in a CI, deploy, config or script file: it makes a forbid violation exit 0.
//   rustflags-env        RUSTFLAGS, CARGO_ENCODED_RUSTFLAGS, RUSTC_WRAPPER, RUSTC_WORKSPACE_WRAPPER,
//                        CLIPPY_CONF_DIR or RUSTC_BOOTSTRAP named in such a file, or set in this process's
//                        environment: each changes what the compiler or clippy is told.
//   nightly              a `+toolchain` override or a `-Z` flag in such a file, or `#![feature(` in source.
//   cargo-config         `.cargo/config.toml` or `.cargo/config` carrying rustflags, a rustc wrapper, or one
//                        of the variables above under [env].
//   lints-workspace      a workspace member without `[lints] workspace = true`: it inherits no lint at all.
//   stray-manifest       a Cargo.toml that is neither the workspace root nor a member.
//   cfg-clippy           `clippy` as a cfg predicate in first-party source: code `cargo build` compiles and
//                        clippy never reads.
//   allow-ban-lint       an allow or expect, outer, inner or under cfg_attr, naming a lint the workspace sets
//                        to deny or forbid, or a group holding one.
//   allow-invalid        `allow-invalid` in a clippy.toml: it silences the warning a misspelled ban path gives.
//   clippy-scope         a crate-local clippy.toml that is not the root ban list minus its declared exemptions.
//   overflow-checks      the release profile without `overflow-checks = true`.
//   panic-abort          `panic = "abort"` in any profile: tower-http's CatchPanicLayer does nothing under it.
//   toolchain-exact      rust-toolchain.toml whose channel is not an exact version, or a legacy rust-toolchain.
//
// Usage: node scripts/check-lint-config.mjs [--root <dir>] [--selftest]
//   --selftest runs every fixture under scripts/fixtures/lint-config/: a directory named after a rule must
//   produce that rule and no other; `good` must produce none.
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { captureAll, main, readText, report, walk, Fail } from './_lib.mjs';
import { attributes, tokens } from './_rust.mjs';
import { checkScopes } from './_scopes.mjs';
import { parseToml } from './_toml.mjs';

const ENV_VARS = ['RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'CLIPPY_CONF_DIR', 'RUSTC_BOOTSTRAP'];
const CLIPPY_GROUPS = ['clippy::all', 'clippy::correctness', 'clippy::suspicious', 'clippy::style', 'clippy::complexity', 'clippy::perf', 'clippy::pedantic', 'clippy::restriction', 'clippy::nursery', 'clippy::cargo', 'clippy'];
const RUST_GROUPS = ['warnings', 'unused', 'nonstandard_style', 'rust_2018_idioms', 'future_incompatible', 'let_underscore'];

/** Files whose text reaches a build: CI, deploy, config and scripts. Docs and fixtures are not scanned. */
function buildFiles(root) {
  return walk(root, (f) => {
    if (f.startsWith('scripts/fixtures/') || f === 'scripts/check-lint-config.mjs' || f.endsWith('.md')) return false;
    return (
      /^(\.github|project-root\/\.github)\//.test(f) ||
      /(^|\/)\.gitlab-ci\.yml$/.test(f) ||
      /(^|\/)mise\.toml$/.test(f) ||
      /(^|\/)Dockerfile$/.test(f) ||
      /(^|\/)compose[^/]*\.ya?ml$/.test(f) ||
      /^(project-root\/)?scripts\/.*\.mjs$/.test(f) ||
      /(^|\/)\.cargo\//.test(f) ||
      /(^|\/)Cargo\.toml$/.test(f) ||
      /(^|\/)rust-toolchain(\.toml)?$/.test(f) ||
      /(^|\/)\.env[^/]*$/.test(f)
    );
  });
}

/** Vendored into a project as backend/, the project root's CI, deploy and script files reach the build too. */
function projectFiles(root) {
  const top = captureAll('git', ['rev-parse', '--show-toplevel'], { cwd: root });
  if (top.status !== 0) return [];
  const here = fs.realpathSync(root);
  const toplevel = fs.realpathSync(top.stdout.trim());
  if (path.relative(toplevel, here) === '' || path.relative(toplevel, path.dirname(here)) !== '') return [];
  const project = path.dirname(here);
  return walk(project, (f) => !f.startsWith(`${path.basename(here)}/`) && (/^\.github\/workflows\//.test(f) || /^\.gitlab-ci\.yml$/.test(f) || /^compose[^/]*\.ya?ml$/.test(f) || /^mise\.toml$/.test(f) || /^scripts\/[^/]+\.mjs$/.test(f))).map((f) => path.join('..', f));
}

function scanBuildFiles(root, findings) {
  const envVar = new RegExp(`\\b(${ENV_VARS.join('|')})\\b`);
  for (const f of [...buildFiles(root), ...projectFiles(root)]) {
    readText(path.join(root, f))
      .split('\n')
      .forEach((line, i) => {
        const at = `${f}:${i + 1}`;
        if (/--cap-lints\b/.test(line)) findings.push(`cap-lints ${at}: ${line.trim()}`);
        if (envVar.test(line)) findings.push(`rustflags-env ${at}: ${line.trim()}`);
        if (/(^|[\s'"`[,])\+(nightly|beta|stable|\d+\.\d+)/.test(line) || /\bcargo\b.*\s-Z\s*[a-z]/.test(line)) findings.push(`nightly ${at}: ${line.trim()}`);
      });
  }
}

function checkEnvironment(env, findings) {
  for (const v of ENV_VARS) if (env[v] !== undefined && env[v] !== '') findings.push(`rustflags-env environment: ${v} is set`);
}

function checkCargoConfig(root, findings) {
  for (const f of walk(root, (p) => /(^|\/)\.cargo\/config(\.toml)?$/.test(p) && !p.startsWith('scripts/fixtures/'))) {
    const config = parseToml(readText(path.join(root, f)), f);
    const bad = [];
    const visit = (value, where) => {
      if (value === null || typeof value !== 'object') return;
      for (const [k, v] of Object.entries(value)) {
        const key = where ? `${where}.${k}` : k;
        if (k === 'rustflags' || k === 'rustdocflags' || k === 'rustc-wrapper' || k === 'rustc-workspace-wrapper') bad.push(key);
        if (where === 'env' && ENV_VARS.includes(k)) bad.push(key);
        visit(v, key);
      }
    };
    visit(config, '');
    for (const key of bad) findings.push(`cargo-config ${f}: ${key}`);
  }
}

function members(root) {
  const env = { ...process.env };
  const channel = parseToml(readText(path.join(root, 'rust-toolchain.toml')), 'rust-toolchain.toml').toolchain?.channel;
  // The fixture trees carry deliberately wrong toolchain files; the metadata call never reads them.
  if (channel && /^\d+\.\d+\.\d+$/.test(channel)) env.RUSTUP_TOOLCHAIN = channel;
  const repoChannel = parseToml(readText(path.join(import.meta.dirname, '..', 'rust-toolchain.toml')), 'rust-toolchain.toml').toolchain.channel;
  env.RUSTUP_TOOLCHAIN = env.RUSTUP_TOOLCHAIN ?? repoChannel;
  const r = captureAll('cargo', ['metadata', '--no-deps', '--format-version', '1', '--offline'], { cwd: root, env });
  if (r.status !== 0) throw new Fail(`cargo metadata failed in ${root}:\n${r.stderr}`);
  const meta = JSON.parse(r.stdout);
  return meta.packages.map((p) => path.relative(root, p.manifest_path).split(path.sep).join('/'));
}

function checkManifests(root, findings) {
  const memberManifests = members(root);
  for (const m of memberManifests) {
    const manifest = parseToml(readText(path.join(root, m)), m);
    if (manifest.lints?.workspace !== true) findings.push(`lints-workspace ${m}: no [lints] workspace = true`);
  }
  const known = new Set(['Cargo.toml', ...memberManifests]);
  for (const f of walk(root, (p) => /(^|\/)Cargo\.toml$/.test(p) && !p.startsWith('scripts/fixtures/'))) {
    if (!known.has(f)) findings.push(`stray-manifest ${f}: neither the workspace root nor a member`);
  }
  for (const f of known) {
    const manifest = parseToml(readText(path.join(root, f)), f);
    for (const [name, profile] of Object.entries(manifest.profile ?? {})) {
      if (profile.panic === 'abort') findings.push(`panic-abort ${f}: [profile.${name}] panic = "abort"`);
    }
  }
  const rootManifest = parseToml(readText(path.join(root, 'Cargo.toml')), 'Cargo.toml');
  if (rootManifest.profile?.release?.['overflow-checks'] !== true) findings.push('overflow-checks Cargo.toml: [profile.release] overflow-checks = true is missing');
  return rootManifest;
}

/** Lints set to deny or forbid in [workspace.lints], by the name an attribute uses. */
function banLints(rootManifest) {
  const out = new Set();
  for (const [tool, table] of Object.entries(rootManifest.workspace?.lints ?? {})) {
    for (const [lint, setting] of Object.entries(table)) {
      const level = typeof setting === 'string' ? setting : setting.level;
      if (level === 'deny' || level === 'forbid') out.add(tool === 'rust' ? lint : `${tool}::${lint}`);
    }
  }
  return out;
}

function checkSource(root, bans, findings) {
  const named = new Set([...bans, ...CLIPPY_GROUPS, ...RUST_GROUPS]);
  for (const f of walk(root, (p) => p.endsWith('.rs') && !p.startsWith('scripts/fixtures/'))) {
    let toks;
    try {
      toks = tokens(readText(path.join(root, f)));
    } catch (e) {
      findings.push(`cfg-clippy ${f}: cannot be read as Rust (${e.message}), so no rule here can clear it`);
      continue;
    }
    for (const attr of attributes(toks)) {
      const head = attr.toks[0]?.text;
      if (attr.inner && head === 'feature') findings.push(`nightly ${f}:${attr.line}: #![${attr.text}]`);
      if (head === 'cfg' || head === 'cfg_attr') {
        const predicate = head === 'cfg' ? attr.toks : predicateOf(attr.toks);
        if (predicate.some((t, k) => t.text === 'clippy' && predicate[k + 1]?.text !== '::')) {
          findings.push(`cfg-clippy ${f}:${attr.line}: #[${attr.text}]`);
        }
      }
      for (const lint of suppressedLints(attr.toks)) {
        const normal = lint.replace(/-/g, '_');
        if (named.has(normal)) findings.push(`allow-ban-lint ${f}:${attr.line}: #${attr.inner ? '!' : ''}[${attr.text}] names ${normal}`);
      }
    }
    toks.forEach((t, k) => {
      if (t.text === 'cfg' && toks[k + 1]?.text === '!' && toks[k + 2]?.text === '(') {
        let depth = 0;
        for (let j = k + 2; j < toks.length; j += 1) {
          if (toks[j].text === '(') depth += 1;
          if (toks[j].text === ')' && --depth === 0) break;
          if (toks[j].text === 'clippy' && toks[j + 1]?.text !== '::') findings.push(`cfg-clippy ${f}:${t.line}: cfg!(… clippy …)`);
        }
      }
    });
  }
}

/** The first argument of cfg_attr(predicate, …). */
function predicateOf(toks) {
  const out = [];
  let depth = 0;
  for (const t of toks.slice(2)) {
    if (t.text === '(') depth += 1;
    if (t.text === ')') depth -= 1;
    if (depth === 0 && t.text === ',') break;
    out.push(t);
  }
  return out;
}

/** Lint names inside every allow(…) or expect(…) in an attribute, cfg_attr included. */
function suppressedLints(toks) {
  const out = [];
  for (let k = 0; k < toks.length; k += 1) {
    if (!['allow', 'expect'].includes(toks[k].text) || toks[k + 1]?.text !== '(') continue;
    let depth = 0;
    let current = '';
    for (let j = k + 1; j < toks.length; j += 1) {
      const t = toks[j];
      if (t.text === '(') {
        depth += 1;
        if (depth > 1) break;
        continue;
      }
      if (t.text === ')' && --depth === 0) {
        if (current) out.push(current);
        break;
      }
      if (t.text === ',') {
        if (current) out.push(current);
        current = '';
      } else if (t.text === '=') {
        if (current) out.push(current);
        current = '';
        break;
      } else current += t.text;
    }
  }
  return out;
}

function checkClippyFiles(root, findings) {
  for (const f of walk(root, (p) => /(^|\/)\.?clippy\.toml$/.test(p) && !p.startsWith('scripts/fixtures/'))) {
    readText(path.join(root, f))
      .split('\n')
      .forEach((line, i) => {
        if (/allow-invalid/.test(line.replace(/#.*/, ''))) findings.push(`allow-invalid ${f}:${i + 1}: ${line.trim()}`);
      });
  }
  for (const problem of checkScopes(root)) findings.push(`clippy-scope ${problem}`);
}

function checkToolchain(root, findings) {
  const channel = parseToml(readText(path.join(root, 'rust-toolchain.toml')), 'rust-toolchain.toml').toolchain?.channel;
  if (!/^\d+\.\d+\.\d+$/.test(channel ?? '')) findings.push(`toolchain-exact rust-toolchain.toml: channel ${JSON.stringify(channel)} is not an exact version`);
  for (const f of walk(root, (p) => /(^|\/)rust-toolchain$/.test(p) && !p.startsWith('scripts/fixtures/'))) {
    findings.push(`toolchain-exact ${f}: a legacy toolchain file beside rust-toolchain.toml`);
  }
}

export function lintConfigFindings(root, env = process.env) {
  const findings = [];
  scanBuildFiles(root, findings);
  checkEnvironment(env, findings);
  checkCargoConfig(root, findings);
  const rootManifest = checkManifests(root, findings);
  checkSource(root, banLints(rootManifest), findings);
  checkClippyFiles(root, findings);
  checkToolchain(root, findings);
  return findings;
}

function selftest(repo) {
  const dir = path.join(repo, 'scripts', 'fixtures', 'lint-config');
  const cases = fs.readdirSync(dir).sort();
  const problems = [];
  for (const name of cases) {
    const findings = lintConfigFindings(path.join(dir, name), {});
    const rules = new Set(findings.map((f) => f.split(' ')[0]));
    if (name === 'good' ? rules.size !== 0 : rules.size !== 1 || !rules.has(name)) {
      problems.push(`fixture ${name}: expected ${name === 'good' ? 'no finding' : `only ${name}`}, got:\n  ${findings.join('\n  ') || '(none)'}`);
    }
  }
  const envFindings = [];
  checkEnvironment({ RUSTFLAGS: '--cap-lints=allow' }, envFindings);
  if (envFindings.length !== 1) problems.push('the environment check did not report RUSTFLAGS');
  report(problems, 'lint-config fixture(s) not refused as expected');
  const expected = ['allow-ban-lint', 'allow-invalid', 'cap-lints', 'cargo-config', 'cfg-clippy', 'clippy-scope', 'good', 'lints-workspace', 'nightly', 'overflow-checks', 'panic-abort', 'rustflags-env', 'stray-manifest', 'toolchain-exact'];
  const missing = expected.filter((e) => !cases.includes(e));
  if (missing.length > 0) throw new Fail(`lint-config fixtures missing: ${missing.join(', ')}`);
  console.log(`lint-config selftest: ${cases.length} fixtures, each refused by its rule alone (good: none)`);
}

main(() => {
  let opts;
  try {
    ({ values: opts } = parseArgs({ options: { root: { type: 'string' }, selftest: { type: 'boolean' } } }));
  } catch (e) {
    throw new Fail(e.message, 2);
  }
  const repo = path.resolve(import.meta.dirname, '..');
  if (opts.selftest) return selftest(repo);
  const root = path.resolve(opts.root ?? repo);
  report(lintConfigFindings(root), 'lint-configuration finding(s)');
  console.log('lint configuration: no cap-lints, no flag overrides, every member inherits the workspace lints, no cfg(clippy), no suppressed ban lint, scopes generated');
});
