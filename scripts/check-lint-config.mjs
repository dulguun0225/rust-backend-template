// Guards on the lint configuration itself: the routes by which a lint level is lowered, or code reaches the
// build without clippy or this script reading it, with no error.
//
//   cap-lints            `--cap-lints` in a CI, deploy, config or script file: it makes a forbid violation exit 0.
//   rustflags-env        RUSTFLAGS and every other *RUSTFLAGS variable, RUSTDOCFLAGS, CARGO_PROFILE_*,
//                        CARGO_BUILD_RUSTC*, RUSTC_WRAPPER, RUSTC_WORKSPACE_WRAPPER, CLIPPY_CONF_DIR or
//                        RUSTC_BOOTSTRAP named in such a file or set in this process's environment, and
//                        `cargo --config` with a key or a TOML file in such a file: each changes what the compiler
//                        or clippy is told, and one that reaches only the release build no canary sees.
//   nightly              a `+toolchain` override or a `-Z` flag in such a file, or `#![feature(` in source.
//   cargo-config         `.cargo/config.toml` or `.cargo/config`, here or, vendored, at the project root (cargo
//                        reads both), carrying rustflags, a rustc or wrapper, [patch], [source], `paths`, or one
//                        of the variables above under [env].
//   lints-workspace      a workspace member without `[lints] workspace = true`: it inherits no lint at all.
//   stray-manifest       a Cargo.toml that is neither the workspace root nor a member.
//   member-location      a member outside crates/<dir>/: the source rules and the SQL rules read crates/ only.
//   compile-time-code    a member with a build script or a procedural-macro target: code that runs in the build
//                        and can set a cfg or emit code no rule here reads.
//   path-dependency      a dependency by path on a crate that is not a workspace member, or [patch] or [replace]
//                        in the root manifest: clippy lints workspace members only.
//   cfg-clippy           `clippy` as a cfg predicate in first-party source: code `cargo build` compiles and
//                        clippy never reads.
//   cfg-unlinted         `debug_assertions`, `coverage` or `coverage_nightly` in a cfg or cfg_attr predicate, or
//                        `feature` under `not(…)`: clippy runs on the dev profile with every feature and without
//                        cfg(coverage), so code under such a predicate ships (release, default features) or is
//                        measured (coverage) without clippy reading it.
//   include              `#[path = …]` or `include!` in first-party source: the file it names may be one no rule
//                        here reads (another extension, a skipped directory, a fixture).
//   allow-ban-lint       an allow, expect or warn, outer, inner or under cfg_attr, naming a lint the workspace
//                        sets to deny or forbid, a group holding one, `renamed_and_removed_lints` or
//                        `unknown_lints` (which would let a renamed lint name through), or a name built from a
//                        macro variable, which no reader can check.
//   allow-invalid        `allow-invalid` in a clippy.toml: it silences the warning a misspelled ban path gives.
//   clippy-key           a root clippy.toml key outside the ones this template uses: keys such as
//                        `arithmetic-side-effects-allowed` narrow a forbidden lint for chosen types, where the
//                        canaries do not look.
//   clippy-scope         a crate-local clippy.toml that is not the root ban list minus its declared exemptions.
//   rustfmt-config       a rustfmt.toml other than the root one, or any .rustfmt.toml: rustfmt reads the nearest
//                        file, so one in a crate replaces the root's (`disable_all_formatting = true` included).
//   test-target          a member target with `test = false` or `harness = false`, or `autotests = false`: each
//                        drops tests from `cargo test` without a line in any test file changing.
//   overflow-checks      the release profile without `overflow-checks = true`, or `overflow-checks = false` in any
//                        profile or package override, in a manifest or a cargo config.
//   panic-abort          `panic = "abort"` in any profile, in a manifest or a cargo config: tower-http's
//                        CatchPanicLayer does nothing under it.
//   toolchain-exact      rust-toolchain.toml whose channel is not an exact version, or a legacy rust-toolchain.
//
// Usage: node scripts/check-lint-config.mjs [--root <dir>] [--selftest]
//   --selftest runs every fixture under scripts/fixtures/lint-config/: a directory named <rule> or
//   <rule>--<variant> must produce that rule and no other, and every source line marked `// refused` must be
//   named by a finding; `good` must produce none.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { captureAll, main, readText, report, run, walk, Fail } from './_lib.mjs';
import { attributes, macroCalls, tokens } from './_rust.mjs';
import { checkScopes } from './_scopes.mjs';
import { parseToml } from './_toml.mjs';

const ENV_VAR = /^(?:[A-Z0-9_]*RUSTFLAGS|RUSTDOCFLAGS|CARGO_PROFILE_[A-Z0-9_]+|CARGO_BUILD_RUSTC(?:_WRAPPER|_WORKSPACE_WRAPPER)?|RUSTC_WRAPPER|RUSTC_WORKSPACE_WRAPPER|CLIPPY_CONF_DIR|RUSTC_BOOTSTRAP)$/;
const ENV_VAR_IN_TEXT = new RegExp(`\\b(${ENV_VAR.source.slice(1, -1)})\\b`);
// `cargo --config KEY=VALUE` and `cargo --config <file>.toml`, in shell and in a JavaScript argument list;
// cargo-deny's own --config (its deny.toml) is not cargo's.
const CARGO_CONFIG_FLAG = /--config['"]?(?:\s*,\s*|\s+|=)['"]?(?:[A-Za-z_][\w.-]*\s*=|[^\s'",]+\.toml\b)/;
const CLIPPY_GROUPS = ['clippy::all', 'clippy::correctness', 'clippy::suspicious', 'clippy::style', 'clippy::complexity', 'clippy::perf', 'clippy::pedantic', 'clippy::restriction', 'clippy::nursery', 'clippy::cargo', 'clippy'];
const RUST_GROUPS = ['warnings', 'unused', 'nonstandard_style', 'rust_2018_idioms', 'future_incompatible', 'let_underscore'];
// Suppressing either lets a lint name that is not literally a ban name reach one: a renamed name
// (`clippy::disallowed_method`) is applied to the lint it was renamed to, with only this warning.
const LINT_NAME_LINTS = ['renamed_and_removed_lints', 'unknown_lints'];
const CLIPPY_KEYS = new Set([
  'allow-unwrap-in-tests',
  'allow-expect-in-tests',
  'allow-panic-in-tests',
  'allow-indexing-slicing-in-tests',
  'await-holding-invalid-types',
  'disallowed-methods',
  'disallowed-types',
  'disallowed-macros',
]);
const UNLINTED_CFGS = new Set(['debug_assertions', 'coverage', 'coverage_nightly']);

/** Files whose text reaches a build: CI, deploy, config and scripts. Docs and fixtures are not scanned. */
function buildFiles(root) {
  return walk(root, (f) => {
    if (f.startsWith('scripts/fixtures/') || f === 'scripts/check-lint-config.mjs' || f.endsWith('.md')) return false;
    return (
      /^(\.github|project-root\/\.github)\//.test(f) ||
      /(^|\/)\.gitlab-ci\.yml$/.test(f) ||
      /(^|\/)mise\.toml$/.test(f) ||
      /(^|\/)Dockerfile[^/]*$/.test(f) ||
      /(^|\/)compose[^/]*\.ya?ml$/.test(f) ||
      /^(project-root\/)?scripts\/.*\.mjs$/.test(f) ||
      /(^|\/)\.cargo\//.test(f) ||
      /(^|\/)Cargo\.toml$/.test(f) ||
      /(^|\/)rust-toolchain(\.toml)?$/.test(f) ||
      /(^|\/)\.env[^/]*$/.test(f)
    );
  });
}

/** The project root when this directory is vendored into one as backend/, else null. */
function projectRoot(root) {
  const top = captureAll('git', ['rev-parse', '--show-toplevel'], { cwd: root });
  if (top.status !== 0) return null;
  const here = fs.realpathSync(root);
  const toplevel = fs.realpathSync(top.stdout.trim());
  if (path.relative(toplevel, here) === '' || path.relative(toplevel, path.dirname(here)) !== '') return null;
  return path.dirname(here);
}

/** Vendored into a project as backend/, the project root's CI, deploy and script files reach the build too. */
function projectFiles(root) {
  const project = projectRoot(root);
  if (!project) return [];
  const here = path.basename(fs.realpathSync(root));
  return walk(project, (f) => !f.startsWith(`${here}/`) && (/^\.github\/workflows\//.test(f) || /^\.gitlab-ci\.yml$/.test(f) || /^compose[^/]*\.ya?ml$/.test(f) || /^Dockerfile[^/]*$/.test(f) || /^\.env[^/]*$/.test(f) || /^mise\.toml$/.test(f) || /^scripts\/[^/]+\.mjs$/.test(f))).map((f) => path.join('..', f));
}

function scanBuildFiles(root, findings) {
  for (const f of [...buildFiles(root), ...projectFiles(root)]) {
    readText(path.join(root, f))
      .split('\n')
      .forEach((line, i) => {
        const at = `${f}:${i + 1}`;
        if (/--cap-lints\b/.test(line)) findings.push(`cap-lints ${at}: ${line.trim()}`);
        if (ENV_VAR_IN_TEXT.test(line)) findings.push(`rustflags-env ${at}: ${line.trim()}`);
        if (/\bcargo\b/.test(line) && !/\bdeny\b/.test(line) && CARGO_CONFIG_FLAG.test(line)) findings.push(`rustflags-env ${at}: cargo --config: ${line.trim()}`);
        if (/(^|[\s'"`[,])\+(nightly|beta|stable|\d+\.\d+)/.test(line) || /\bcargo\b.*\s-Z\s*[a-z]/.test(line)) findings.push(`nightly ${at}: ${line.trim()}`);
      });
  }
}

function checkEnvironment(env, findings) {
  for (const [name, value] of Object.entries(env)) if (ENV_VAR.test(name) && value !== undefined && value !== '') findings.push(`rustflags-env environment: ${name} is set`);
}

/** Profile settings that switch off overflow checks or unwinding, wherever a profile table sits. */
function checkProfiles(profiles, file, findings) {
  const visit = (table, where) => {
    if (table === null || typeof table !== 'object') return;
    if (table['overflow-checks'] === false) findings.push(`overflow-checks ${file}: [${where}] overflow-checks = false`);
    if (table.panic === 'abort') findings.push(`panic-abort ${file}: [${where}] panic = "abort"`);
    for (const [k, v] of Object.entries(table)) if (v !== null && typeof v === 'object') visit(v, `${where}.${k}`);
  };
  for (const [name, profile] of Object.entries(profiles ?? {})) visit(profile, `profile.${name}`);
}

function checkCargoConfigFile(full, label, findings) {
  const config = parseToml(readText(full), label);
  const bad = [];
  for (const key of ['patch', 'paths', 'source']) if (key in config) bad.push(key);
  const visit = (value, where) => {
    if (value === null || typeof value !== 'object') return;
    for (const [k, v] of Object.entries(value)) {
      const key = where ? `${where}.${k}` : k;
      if (['rustflags', 'rustdocflags', 'rustc', 'rustc-wrapper', 'rustc-workspace-wrapper'].includes(k)) bad.push(key);
      if (where === 'env' && ENV_VAR.test(k)) bad.push(key);
      visit(v, key);
    }
  };
  visit(config, '');
  for (const key of bad) findings.push(`cargo-config ${label}: ${key}`);
  checkProfiles(config.profile, label, findings);
}

function checkCargoConfig(root, findings) {
  for (const f of walk(root, (p) => /(^|\/)\.cargo\/config(\.toml)?$/.test(p) && !p.startsWith('scripts/fixtures/'))) checkCargoConfigFile(path.join(root, f), f, findings);
  const project = projectRoot(root);
  if (!project) return;
  for (const name of ['config.toml', 'config']) {
    const full = path.join(project, '.cargo', name);
    if (fs.existsSync(full)) checkCargoConfigFile(full, `../.cargo/${name}`, findings);
  }
}

function metadata(root) {
  const env = { ...process.env };
  const channel = parseToml(readText(path.join(root, 'rust-toolchain.toml')), 'rust-toolchain.toml').toolchain?.channel;
  // The fixture trees carry deliberately wrong toolchain files; the metadata call never reads them.
  if (channel && /^\d+\.\d+\.\d+$/.test(channel)) env.RUSTUP_TOOLCHAIN = channel;
  const repoChannel = parseToml(readText(path.join(import.meta.dirname, '..', 'rust-toolchain.toml')), 'rust-toolchain.toml').toolchain.channel;
  env.RUSTUP_TOOLCHAIN = env.RUSTUP_TOOLCHAIN ?? repoChannel;
  const r = captureAll('cargo', ['metadata', '--no-deps', '--format-version', '1', '--offline'], { cwd: root, env });
  if (r.status !== 0) throw new Fail(`cargo metadata failed in ${root}:\n${r.stderr}`);
  return JSON.parse(r.stdout);
}

function checkManifests(root, findings) {
  const meta = metadata(root);
  const rel = (p) => path.relative(root, p).split(path.sep).join('/');
  const memberManifests = meta.packages.map((p) => rel(p.manifest_path));
  const memberDirs = new Set(meta.packages.map((p) => path.dirname(fs.realpathSync(p.manifest_path))));
  const pathTargets = new Set();
  for (const pkg of meta.packages) {
    const m = rel(pkg.manifest_path);
    const manifest = parseToml(readText(path.join(root, m)), m);
    if (manifest.lints?.workspace !== true) findings.push(`lints-workspace ${m}: no [lints] workspace = true`);
    if (manifest.package?.autotests === false) findings.push(`test-target ${m}: autotests = false`);
    for (const kind of ['lib', 'bin', 'test', 'example', 'bench']) {
      for (const target of [manifest[kind] ?? []].flat()) {
        for (const key of ['test', 'harness']) if (target[key] === false) findings.push(`test-target ${m}: [${kind}] ${key} = false`);
      }
    }
    if (!/^crates\/[^/]+\/Cargo\.toml$/.test(m)) findings.push(`member-location ${m}: a member lives at crates/<dir>/Cargo.toml`);
    for (const target of pkg.targets) {
      for (const kind of target.kind) {
        if (kind === 'custom-build' || kind === 'proc-macro') findings.push(`compile-time-code ${m}: ${kind === 'custom-build' ? 'a build script' : 'a procedural-macro target'}, ${rel(target.src_path)}`);
      }
    }
    for (const dep of pkg.dependencies) {
      if (dep.path === undefined) continue;
      const target = fs.existsSync(dep.path) ? fs.realpathSync(dep.path) : dep.path;
      if (memberDirs.has(target)) continue;
      findings.push(`path-dependency ${m}: ${dep.name} by path ${dep.path}, not a workspace member`);
      pathTargets.add(path.relative(fs.realpathSync(root), path.join(target, 'Cargo.toml')).split(path.sep).join('/'));
    }
  }
  // A crate reached by a path dependency is that rule's finding, not also a stray manifest.
  const known = new Set(['Cargo.toml', ...memberManifests, ...pathTargets]);
  for (const f of walk(root, (p) => /(^|\/)Cargo\.toml$/.test(p) && !p.startsWith('scripts/fixtures/'))) {
    if (!known.has(f)) findings.push(`stray-manifest ${f}: neither the workspace root nor a member`);
  }
  for (const f of pathTargets) known.delete(f);
  for (const f of known) checkProfiles(parseToml(readText(path.join(root, f)), f).profile, f, findings);
  const rootManifest = parseToml(readText(path.join(root, 'Cargo.toml')), 'Cargo.toml');
  if (rootManifest.profile?.release?.['overflow-checks'] !== true) findings.push('overflow-checks Cargo.toml: [profile.release] overflow-checks = true is missing');
  for (const key of ['patch', 'replace']) if (key in rootManifest) findings.push(`path-dependency Cargo.toml: [${key}] replaces a dependency's source`);
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
  const named = new Set([...bans, ...CLIPPY_GROUPS, ...RUST_GROUPS, ...LINT_NAME_LINTS]);
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
      const shown = `#${attr.inner ? '!' : ''}[${attr.text}]`;
      if (attr.inner && head === 'feature') findings.push(`nightly ${f}:${attr.line}: ${shown}`);
      if (head === 'cfg' || head === 'cfg_attr') {
        const predicate = head === 'cfg' ? attr.toks : predicateOf(attr.toks);
        if (predicate.some((t, k) => t.text === 'clippy' && predicate[k + 1]?.text !== '::')) findings.push(`cfg-clippy ${f}:${attr.line}: ${shown}`);
        const unlinted = unlintedCfg(predicate);
        if (unlinted) findings.push(`cfg-unlinted ${f}:${attr.line}: ${shown} (${unlinted})`);
      }
      if ((head === 'path' && attr.toks[1]?.text === '=') || (head === 'cfg_attr' && attr.toks.some((t, k) => t.kind === 'ident' && t.text === 'path' && attr.toks[k + 1]?.text === '='))) {
        findings.push(`include ${f}:${attr.line}: ${shown}`);
      }
      for (const lint of loweredLints(attr.toks)) {
        if (lint.includes('$')) findings.push(`allow-ban-lint ${f}:${attr.line}: ${shown} names a lint through a macro variable`);
        else if (named.has(lint.replace(/-/g, '_'))) findings.push(`allow-ban-lint ${f}:${attr.line}: ${shown} names ${lint.replace(/-/g, '_')}`);
      }
    }
    for (const call of macroCalls(toks)) {
      if (call.name === 'include') findings.push(`include ${f}:${call.line}: ${call.path}!`);
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

/** Why code under this cfg predicate is compiled without clippy reading it, or null. */
function unlintedCfg(predicate) {
  const negated = [false];
  for (let k = 0; k < predicate.length; k += 1) {
    const t = predicate[k];
    if (t.text === '(') negated.push(negated.at(-1) || predicate[k - 1]?.text === 'not');
    else if (t.text === ')') negated.pop();
    else if (t.kind === 'ident' && UNLINTED_CFGS.has(t.text) && predicate[k - 1]?.text !== '::') return t.text;
    else if (t.kind === 'ident' && t.text === 'feature' && negated.at(-1)) return 'a feature under not(…)';
  }
  return null;
}

/** Lint names inside every allow(…), expect(…) or warn(…) in an attribute, cfg_attr included. */
function loweredLints(toks) {
  const out = [];
  for (let k = 0; k < toks.length; k += 1) {
    if (!['allow', 'expect', 'warn'].includes(toks[k].text) || toks[k + 1]?.text !== '(') continue;
    let depth = 0;
    let current = '';
    let skipping = false;
    for (let j = k + 1; j < toks.length; j += 1) {
      const t = toks[j];
      if (t.text === '(') {
        depth += 1;
        if (depth > 1) current += t.text;
        continue;
      }
      if (t.text === ')') {
        depth -= 1;
        if (depth === 0) {
          if (current && !skipping) out.push(current);
          break;
        }
        current += t.text;
        continue;
      }
      if (depth === 1 && t.text === ',') {
        if (current && !skipping) out.push(current);
        current = '';
        skipping = false;
      } else if (depth === 1 && t.text === '=') {
        // `reason = "…"`: not a lint name
        skipping = true;
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
  const rootClippy = path.join(root, 'clippy.toml');
  if (fs.existsSync(rootClippy)) {
    for (const key of Object.keys(parseToml(readText(rootClippy), 'clippy.toml'))) {
      if (!CLIPPY_KEYS.has(key)) findings.push(`clippy-key clippy.toml: ${key} is not a key this template uses`);
    }
  }
  for (const problem of checkScopes(root)) findings.push(`clippy-scope ${problem}`);
}

function checkRustfmtFiles(root, findings) {
  for (const f of walk(root, (p) => /(^|\/)\.?rustfmt\.toml$/.test(p) && p !== 'rustfmt.toml' && !p.startsWith('scripts/fixtures/'))) {
    findings.push(`rustfmt-config ${f}: rustfmt reads the nearest configuration, so this one replaces the root rustfmt.toml`);
  }
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
  checkRustfmtFiles(root, findings);
  checkToolchain(root, findings);
  return findings;
}

/** `file:line` of every source line marked `// refused` in a fixture: each must be named by a finding. */
function refusedLines(dir) {
  const out = [];
  for (const f of walk(dir, (p) => p.endsWith('.rs'))) {
    readText(path.join(dir, f))
      .split('\n')
      .forEach((line, i) => {
        if (/\/\/ refused\b/.test(line)) out.push(`${f}:${i + 1}`);
      });
  }
  return out;
}

/** Vendored mode: a project root's .cargo/config.toml reaches backend/'s build, so it is read too. */
function projectConfigCase(repo, problems) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'lint-config-'));
  try {
    fs.cpSync(path.join(repo, 'scripts', 'fixtures', 'lint-config', 'good'), path.join(dir, 'backend'), { recursive: true });
    fs.mkdirSync(path.join(dir, '.cargo'));
    fs.writeFileSync(path.join(dir, '.cargo', 'config.toml'), '[profile.release]\noverflow-checks = false\n\n[build]\nrustflags = ["-Aclippy::unwrap_used"]\n');
    run('git', ['init', '-q'], { cwd: dir });
    const rules = new Set(lintConfigFindings(path.join(dir, 'backend'), {}).map((f) => f.split(' ')[0]));
    if (!rules.has('cargo-config') || !rules.has('overflow-checks')) problems.push(`a project root's .cargo/config.toml was not read (got ${[...rules].join(', ') || 'nothing'})`);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function selftest(repo) {
  const dir = path.join(repo, 'scripts', 'fixtures', 'lint-config');
  const cases = fs.readdirSync(dir).sort();
  const problems = [];
  for (const name of cases) {
    const rule = name.split('--')[0];
    const findings = lintConfigFindings(path.join(dir, name), {});
    const rules = new Set(findings.map((f) => f.split(' ')[0]));
    if (name === 'good' ? rules.size !== 0 : rules.size !== 1 || !rules.has(rule)) {
      problems.push(`fixture ${name}: expected ${name === 'good' ? 'no finding' : `only ${rule}`}, got:\n  ${findings.join('\n  ') || '(none)'}`);
    }
    for (const at of refusedLines(path.join(dir, name))) {
      if (!findings.some((f) => f.includes(` ${at}:`))) problems.push(`fixture ${name}: ${at} is marked refused and no finding names it`);
    }
  }
  const envFindings = [];
  checkEnvironment({ RUSTFLAGS: '--cap-lints=allow', CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS: 'false', CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS: '-Aclippy::all', CARGO_HOME: '/x' }, envFindings);
  if (envFindings.length !== 3) problems.push(`the environment check reported ${envFindings.length} of the 3 variables set:\n  ${envFindings.join('\n  ')}`);
  projectConfigCase(repo, problems);
  report(problems, 'lint-config fixture(s) not refused as expected');
  const expected = ['allow-ban-lint', 'allow-invalid', 'cap-lints', 'cargo-config', 'cfg-clippy', 'cfg-unlinted', 'clippy-key', 'clippy-scope', 'compile-time-code', 'good', 'include', 'lints-workspace', 'member-location', 'nightly', 'overflow-checks', 'panic-abort', 'path-dependency', 'rustflags-env', 'rustfmt-config', 'stray-manifest', 'test-target', 'toolchain-exact'];
  const missing = expected.filter((e) => !cases.some((c) => c.split('--')[0] === e));
  if (missing.length > 0) throw new Fail(`lint-config fixtures missing: ${missing.join(', ')}`);
  console.log(`lint-config selftest: ${cases.length} fixtures, each refused by its rule alone and on every marked line (good: none); a project root's cargo config is read`);
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
  console.log('lint configuration: no cap-lints, no flag or profile overrides, every member in crates/ inherits the workspace lints, no build scripts or path crates, no unlinted cfg or include, no suppressed ban lint, scopes generated');
});
