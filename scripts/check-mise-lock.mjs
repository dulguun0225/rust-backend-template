// mise.lock against mise.toml: every tool mise installs is pinned by checksum. It reads each mise.toml the wall
// covers (this directory's; project-root/'s while this directory carries it; and the project root's when this
// directory is backend/ inside a project) with the mise.lock beside it. Rules:
//   locked      mise.toml lacks `[tool_config] locked = true`. With it, `mise install` refuses a tool the lock
//               lacks; without it, `mise install` over a stale lock installs the new version and rewrites the lock
//               (both run with mise 2026.9.16, 2026-09-29)
//   platforms   `[settings] lockfile_platforms` is not the five platforms below, so `mise lock` would not refresh
//               each of them
//   no-lock     mise.lock is missing
//   stale       a tool mise.toml lists has no lock entry at its exact version and options
//   extra       the lock has an entry for a tool mise.toml does not list, or a second entry for one it does
//   checksum    an entry lacks, for one of the five platforms, a `url` or a `checksum` of the form sha256:<64 hex>,
//               sha512:<128 hex> or blake3:<64 hex>, unless an exemption below covers that tool and platform
//   exemption   an exemption below that no longer applies: no mise.toml lists a tool it matches, or the lock
//               records a checksum the rule above accepts for a platform it exempts
// What it does not decide: that a recorded checksum is the artifact's. `mise install` downloads the artifact and
// refuses a mismatch (run, 2026-09-29: a changed digit in gitleaks' and squawk's linux-x64 checksums each failed
// the install, exit 1). An entry with a url and no checksum installs unverified, which is why the checksum rule
// exists. A tool already installed is not downloaded again, so not checked again.
// Usage: node scripts/check-mise-lock.mjs [--selftest]
//   --selftest runs scripts/fixtures/mise-lock/<rule>[--<variant>]/ (mise.toml, and mise.lock unless the case is
//   its absence) with the fixtures' own exemptions: each must produce its rule alone; `good` must produce none.
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { capture, main, readText, report, Fail } from './_lib.mjs';
import { parseToml } from './_toml.mjs';

// The platforms the scripts claim: they are Node so the wall runs on each.
export const PLATFORMS = ['linux-x64', 'linux-arm64', 'macos-x64', 'macos-arm64', 'windows-x64'];

// Where the lock cannot carry a checksum the rule accepts, by tool (a name or a pattern) and platform, each with
// its reason. `accepts` is what stands in: 'nothing' (no checksum, or no entry, for the platform) or 'sha1'.
export const EXEMPTIONS = [
  {
    tool: /^cargo:/,
    platforms: PLATFORMS,
    accepts: 'nothing',
    reason: 'built from source by `cargo install --locked`; mise records no artifact for the cargo backend, and cargo checks each crate it downloads against the checksum in the crates.io index',
  },
  {
    tool: 'github:sourcefrog/cargo-mutants',
    platforms: ['linux-arm64', 'macos-arm64'],
    accepts: 'nothing',
    reason: 'the release publishes no arm64 build (27.1.0: x86_64 Linux, macOS and Windows only), so there is no artifact to record',
  },
];

// The fixtures' exemptions, shaped like the ones above.
const FIXTURE_EXEMPTIONS = [
  { tool: /^cargo:/, platforms: PLATFORMS, accepts: 'nothing', reason: 'fixture' },
  { tool: 'github:example/tool', platforms: ['linux-arm64'], accepts: 'nothing', reason: 'fixture' },
  { tool: 'example-jdk', platforms: PLATFORMS, accepts: 'sha1', reason: 'fixture' },
];

const CHECKSUM = /^(sha256:[0-9a-f]{64}|sha512:[0-9a-f]{128}|blake3:[0-9a-f]{64})$/;
const SHA1 = /^sha1:[0-9a-f]{40}$/;
const matches = (ex, tool) => (typeof ex.tool === 'string' ? ex.tool === tool : ex.tool.test(tool));
const exemptionFor = (exemptions, tool, platform) => exemptions.find((ex) => matches(ex, tool) && ex.platforms.includes(platform));

/** The pinned version and the options of one [tools] value, or null for a form this check does not read. */
function request(value) {
  if (typeof value === 'string') return { version: value, options: {} };
  if (value && typeof value === 'object' && !Array.isArray(value) && typeof value.version === 'string') {
    const { version, ...options } = value;
    return { version, options };
  }
  return null;
}

const sameOptions = (want, have = {}) => {
  const keys = new Set([...Object.keys(want), ...Object.keys(have)]);
  return [...keys].every((k) => k in want && k in have && String(want[k]) === String(have[k]));
};

/** Why one platform entry fails the checksum rule, or null when it passes. */
function platformProblem(p, ex) {
  if (p && CHECKSUM.test(p.checksum ?? '') && typeof p.url === 'string' && p.url !== '') return null;
  if (ex?.accepts === 'nothing' && !CHECKSUM.test(p?.checksum ?? '')) return null;
  if (ex?.accepts === 'sha1' && p && SHA1.test(p.checksum ?? '') && typeof p.url === 'string' && p.url !== '') return null;
  if (!p) return 'has no entry';
  if (p.checksum === undefined) return 'has no checksum';
  if (!CHECKSUM.test(p.checksum) && !(ex?.accepts === 'sha1' && SHA1.test(p.checksum))) {
    return `has checksum ${JSON.stringify(p.checksum)}, not sha256:<64 hex>, sha512:<128 hex> or blake3:<64 hex>`;
  }
  return 'has no url';
}

/**
 * Findings for one directory holding mise.toml (and, it must, mise.lock), each `<rule> <label>: <text>`. `used`
 * collects the exemptions some listed tool matches, and the exempted platforms that carry an accepted checksum.
 */
export function lockFindings(dir, label, exemptions, used = { tools: new Set(), recorded: [] }) {
  const findings = [];
  const add = (rule, text) => findings.push(`${rule} ${label}: ${text}`);
  const config = parseToml(readText(path.join(dir, 'mise.toml')), `${label}/mise.toml`);
  if (config.tool_config?.locked !== true) add('locked', 'mise.toml does not set `[tool_config] locked = true`, so `mise install` would rewrite a stale lock instead of refusing it');
  const listed = config.settings?.lockfile_platforms;
  if (!Array.isArray(listed) || listed.length !== PLATFORMS.length || !PLATFORMS.every((p) => listed.includes(p))) {
    add('platforms', `\`[settings] lockfile_platforms\` is ${JSON.stringify(listed ?? null)}, not ${JSON.stringify(PLATFORMS)}`);
  }
  const tools = config.tools ?? {};
  for (const tool of Object.keys(tools)) for (const ex of exemptions) if (matches(ex, tool)) used.tools.add(ex);
  const lockFile = path.join(dir, 'mise.lock');
  if (!fs.existsSync(lockFile)) {
    add('no-lock', 'mise.lock is missing; run `mise lock` and commit it');
    return findings;
  }
  const lock = parseToml(readText(lockFile), `${label}/mise.lock`).tools ?? {};
  for (const [tool, value] of Object.entries(tools)) {
    const want = request(value);
    if (!want) {
      add('stale', `${tool} is not a version string or a table with one; this check reads no other form`);
      continue;
    }
    const entries = Array.isArray(lock[tool]) ? lock[tool] : [];
    const hit = entries.filter((e) => e.version === want.version && sameOptions(want.options, e.options));
    if (hit.length === 0) {
      const have = entries.map((e) => `${e.version}${e.options ? ` ${JSON.stringify(e.options)}` : ''}`).join(', ');
      const opts = Object.keys(want.options).length ? ` ${JSON.stringify(want.options)}` : '';
      add('stale', `${tool} ${want.version}${opts} has no lock entry${have ? ` (the lock has ${have})` : ''}; run \`mise lock\``);
      continue;
    }
    if (entries.length > 1) add('extra', `${tool} has ${entries.length} lock entries (${entries.map((e) => e.version).join(', ')}); mise.toml pins one`);
    for (const entry of hit) {
      for (const platform of PLATFORMS) {
        const p = entry[`platforms.${platform}`];
        const ex = exemptionFor(exemptions, tool, platform);
        if (ex && p && CHECKSUM.test(p.checksum ?? '')) used.recorded.push(`${tool} ${platform} (${label})`);
        const problem = platformProblem(p, ex);
        if (problem) add('checksum', `${tool} ${entry.version} ${platform} ${problem}`);
      }
    }
  }
  for (const tool of Object.keys(lock)) if (!(tool in tools)) add('extra', `${tool} is in mise.lock and not in mise.toml; run \`mise lock\``);
  return findings;
}

/** The exemption rule over a whole run: each exemption must match a listed tool and exempt nothing recorded. */
export function exemptionFindings(exemptions, used) {
  const findings = [];
  for (const ex of exemptions) if (!used.tools.has(ex)) findings.push(`exemption ${ex.tool}: no mise.toml lists a tool it matches; remove it`);
  for (const r of used.recorded) findings.push(`exemption ${r}: exempted, and the lock records a checksum the rule accepts; remove the exemption`);
  return findings;
}

function selftest(repo) {
  const dir = path.join(repo, 'scripts', 'fixtures', 'mise-lock');
  const cases = fs.readdirSync(dir).sort();
  const problems = [];
  for (const name of cases) {
    const used = { tools: new Set(), recorded: [] };
    const findings = [...lockFindings(path.join(dir, name), name, FIXTURE_EXEMPTIONS, used), ...exemptionFindings(FIXTURE_EXEMPTIONS, used)];
    const rules = new Set(findings.map((f) => f.split(' ')[0]));
    const want = name.split('--')[0];
    if (name === 'good' ? rules.size !== 0 : rules.size !== 1 || !rules.has(want)) problems.push(`fixture ${name}: got ${[...rules].join(', ') || 'nothing'}\n  ${findings.join('\n  ')}`);
  }
  for (const rule of ['good', 'locked', 'platforms', 'no-lock', 'stale', 'extra', 'checksum', 'exemption']) {
    if (!cases.some((c) => c.split('--')[0] === rule)) problems.push(`no fixture for ${rule}`);
  }
  report(problems, 'mise-lock fixture(s) not refused as expected');
  console.log(`mise-lock selftest: ${cases.length} fixtures, each refused by its rule alone (good: none)`);
}

/** This directory, project-root/ while it is here, and the project root when this directory is backend/. */
function lockDirs(repo) {
  const dirs = [[repo, '.']];
  if (fs.existsSync(path.join(repo, 'project-root', 'mise.toml'))) dirs.push([path.join(repo, 'project-root'), 'project-root']);
  const service = fs.realpathSync(repo);
  const top = fs.realpathSync(capture('git', ['rev-parse', '--show-toplevel'], { cwd: service }));
  if (path.relative(top, service) !== '' && fs.existsSync(path.join(service, '..', 'mise.toml'))) dirs.push([path.dirname(service), '..']);
  return dirs;
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
  const used = { tools: new Set(), recorded: [] };
  const dirs = lockDirs(repo);
  const findings = dirs.flatMap(([dir, label]) => lockFindings(dir, label, EXEMPTIONS, used));
  findings.push(...exemptionFindings(EXEMPTIONS, used));
  report(findings, 'mise.lock finding(s)');
  console.log(`mise.lock (${dirs.map(([, l]) => l).join(', ')}): every tool at its pinned version, with a url and a checksum for ${PLATFORMS.join(', ')}`);
  for (const ex of EXEMPTIONS) {
    const where = ex.platforms === PLATFORMS ? 'every platform' : ex.platforms.join(', ');
    console.log(`  exempt: ${ex.tool} on ${where}, ${ex.accepts === 'sha1' ? 'a sha1 accepted' : 'no checksum'}: ${ex.reason}`);
  }
  console.log("  not decided here: that a checksum is the artifact's; `mise install` refuses a mismatch when it downloads");
});
