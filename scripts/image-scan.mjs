// Container image scan: the service image built from the Dockerfile under a fixed tag, `<binary>:wall`, then
// trivy (mise.toml pins it) over its operating-system and language packages, gated on exit code. It fails on a
// HIGH or CRITICAL vulnerability whether or not a fixed version exists: an unfixed one is exposed all the same,
// and `--ignore-unfixed` would hide it with no record. The way past one is an entry in .trivyignore with its
// reason and an expiry, as java-backend-template's osv-scanner.toml does. The vulnerability database changes
// daily, so an unchanged tree can fail on a new advisory; that is intended, as with cargo-deny.
//
// trivy reads only what this script passes: every TRIVY_* variable but TRIVY_CACHE_DIR is removed from its
// environment, it runs in target/trivy, where no trivy.yaml is, and the ignore file is named. A trivy.yaml,
// trivy.yml or .trivyignore.yaml beside the Dockerfile is refused, since it would not be read.
//
// .trivyignore: one entry per line, `<vulnerability id> exp:<yyyy-mm-dd>`, directly below a `#` line giving its
// reason; blank lines and other `#` lines are allowed, anything else is refused. suppressions.txt lists each
// entry. After the expiry trivy reports the vulnerability again.
//
// First, the canary: an image built from the Dockerfile's final base with one dpkg record added, openssl
// 3.5.1-1 (scripts/fixtures/trivy/canary-openssl), must be refused naming a HIGH or CRITICAL vulnerability in
// that package, which also shows the database covers the base's release; and the same image, with every
// finding listed in a generated ignore file, must pass, which shows the ignore file is read, and with each
// entry expired must be refused again. Then the service image must be read: trivy must detect its operating
// system, find its packages, and not report the release past its end of support.
// Usage: node scripts/image-scan.mjs [--selftest]
import fs from 'node:fs';
import path from 'node:path';
import { captureAll, main, readText, report, Fail } from './_lib.mjs';

const repo = path.resolve(import.meta.dirname, '..');
const work = path.join(repo, 'target', 'trivy');
const fixtures = path.join(repo, 'scripts', 'fixtures', 'trivy');
// trivy exits 1 on its own errors, so a finding gets an exit code of its own.
const FOUND = 5;
const SEVERITIES = ['HIGH', 'CRITICAL'];
const UNREAD = ['trivy.yaml', 'trivy.yml', '.trivyignore.yaml'];

const ENTRY = /^(\S+) exp:(\d{4})-(\d{2})-(\d{2})$/;
const ID = /^[A-Za-z][A-Za-z0-9]*-[A-Za-z0-9._:-]+$/;

/** The entries of a .trivyignore text and what is wrong with it: { entries: [{ id, exp, reason, line }], findings } */
export function parseIgnore(text, name = '.trivyignore') {
  const lines = text.split('\n');
  const entries = [];
  const findings = [];
  const seen = new Set();
  lines.forEach((raw, i) => {
    const line = raw.trim();
    if (line === '' || line.startsWith('#')) return;
    const at = `${name}:${i + 1}`;
    const m = ENTRY.exec(line);
    if (!m) return findings.push(`${at}: not \`<vulnerability id> exp:<yyyy-mm-dd>\`: ${line}`);
    const [, id, y, mo, d] = m;
    if (!ID.test(id)) findings.push(`${at}: ${id} is not a vulnerability id`);
    const date = new Date(Date.UTC(Number(y), Number(mo) - 1, Number(d)));
    if (date.toISOString().slice(0, 10) !== `${y}-${mo}-${d}`) findings.push(`${at}: ${y}-${mo}-${d} is not a date`);
    const above = (lines[i - 1] ?? '').trim();
    const reason = /^#\s*(\S.*)$/.exec(above)?.[1];
    if (!reason) findings.push(`${at}: ${id} has no reason on the line directly above it`);
    if (seen.has(id)) findings.push(`${at}: ${id} is listed twice`);
    seen.add(id);
    entries.push({ id, exp: `${y}-${mo}-${d}`, reason, line });
  });
  return { entries, findings };
}

/** trivy with no TRIVY_* setting from the environment but the cache directory, run where no trivy.yaml is. */
function trivy(args) {
  const env = Object.fromEntries(Object.keys(process.env).filter((k) => k.startsWith('TRIVY_')).map((k) => [k, undefined]));
  const cache = process.env.TRIVY_CACHE_DIR ? ['--cache-dir', process.env.TRIVY_CACHE_DIR] : [];
  return captureAll('trivy', [...args, ...cache], { env, cwd: work });
}

/** Scans a local image; returns { report, counted } and fails on a tool error or an exit code the report contradicts. */
function scan(image, ignorefile) {
  const out = path.join(work, `${image.replace(/[^A-Za-z0-9_.-]/g, '_')}.json`);
  fs.rmSync(out, { force: true });
  const r = trivy([
    'image',
    '--image-src', 'docker',
    '--scanners', 'vuln',
    '--pkg-types', 'os,library',
    '--severity', SEVERITIES.join(','),
    '--exit-code', `${FOUND}`,
    '--ignorefile', ignorefile,
    '--list-all-pkgs',
    '--format', 'json',
    '--output', out,
    '--disable-telemetry',
    '--skip-version-check',
    '--no-progress',
    '--timeout', '15m',
    image,
  ]);
  if ((r.status !== 0 && r.status !== FOUND) || !fs.existsSync(out)) throw new Fail(`trivy exited ${r.status} scanning ${image}:\n${r.stderr}`);
  const doc = JSON.parse(readText(out));
  const counted = (doc.Results ?? []).flatMap((res) => (res.Vulnerabilities ?? []).filter((v) => SEVERITIES.includes(v.Severity)).map((v) => ({ ...v, Target: res.Target })));
  if ((r.status === FOUND) !== (counted.length > 0)) throw new Fail(`trivy exited ${r.status} over ${image}, but its report lists ${counted.length} HIGH or CRITICAL vulnerabilities`);
  return { report: doc, counted };
}

const describe = (v) => `  ${v.Severity} ${v.VulnerabilityID} ${v.PkgName} ${v.InstalledVersion} (${v.Status}${v.FixedVersion ? `, fixed in ${v.FixedVersion}` : ''}) in ${v.Target}`;

/** docker build, its log printed only when it fails. */
function build(tag, dir) {
  const r = captureAll('docker', ['build', '--progress', 'plain', '--file', path.join(dir, 'Dockerfile'), '--tag', tag, dir]);
  if (r.status !== 0) {
    console.error(r.stdout + r.stderr);
    throw new Fail(`docker build of ${tag} from ${path.relative(repo, dir) || '.'} exited ${r.status}`, r.status);
  }
}

function dockerfile() {
  const text = readText(path.join(repo, 'Dockerfile'));
  const froms = text.split('\n').map((l) => /^\s*FROM\s+(?:--platform=\S+\s+)?(\S+)/i.exec(l)?.[1]).filter(Boolean);
  const bin = /--bin\s+([a-z0-9][a-z0-9_-]*)\s*$/m.exec(text)?.[1];
  if (froms.length === 0) throw new Fail('the Dockerfile has no FROM line');
  if (!bin) throw new Fail('the Dockerfile builds no `--bin <name>`; the image tag is taken from it');
  return { base: froms.at(-1), tag: `${bin}:wall`, canary: `${bin}-scan-canary:wall` };
}

function canary(base, tag) {
  const dir = path.join(work, 'canary');
  fs.mkdirSync(dir, { recursive: true });
  fs.copyFileSync(path.join(fixtures, 'canary-openssl'), path.join(dir, 'openssl'));
  fs.writeFileSync(path.join(dir, 'Dockerfile'), `FROM ${base}\nCOPY openssl /var/lib/dpkg/status.d/openssl\n`);
  build(tag, dir);
  const none = path.join(work, 'canary-none.trivyignore');
  fs.writeFileSync(none, '');
  const refused = scan(tag, none);
  const named = refused.counted.filter((v) => v.PkgName === 'openssl' && v.InstalledVersion === '3.5.1-1');
  if (named.length === 0) {
    throw new Fail(`trivy did not refuse the canary, openssl 3.5.1-1 over ${base}: no HIGH or CRITICAL vulnerability named in it (a base that is not Debian needs its own canary record)`);
  }
  const all = path.join(work, 'canary-all.trivyignore');
  const ids = [...new Set(refused.counted.map((v) => v.VulnerabilityID))];
  fs.writeFileSync(all, ids.map((id) => `# the canary's finding\n${id} exp:2999-12-31\n`).join(''));
  const parsed = parseIgnore(readText(all), 'the canary ignore file');
  report(parsed.findings, 'problem(s) in the generated canary ignore file');
  const ignored = scan(tag, all);
  if (ignored.counted.length > 0) throw new Fail(`trivy did not read the ignore file: the canary with its ${ids.length} findings listed still reports\n${ignored.counted.map(describe).join('\n')}`);
  const expired = path.join(work, 'canary-expired.trivyignore');
  fs.writeFileSync(expired, ids.map((id) => `# the canary's finding, expired\n${id} exp:2000-01-01\n`).join(''));
  if (scan(tag, expired).counted.length !== refused.counted.length) throw new Fail('trivy honoured an expired entry in the ignore file: the canary with its findings listed as expired passed');
  console.log(`trivy refused its canary, openssl 3.5.1-1 over the Dockerfile's base (${named.length} HIGH or CRITICAL, e.g. ${named[0].VulnerabilityID}); passed it with each finding listed in an ignore file, and refused it again with each entry expired`);
}

// What refuses each bad_<rule> fixture, and nothing else may.
const RULES = {
  date: /is not a date/,
  duplicate: /is listed twice/,
  'extra-field': /not `<vulnerability id>/,
  'no-expiry': /not `<vulnerability id>/,
  'no-reason': /has no reason/,
  'reason-apart': /has no reason/,
};

function selftest() {
  const dir = path.join(fixtures, 'ignore');
  const problems = [];
  const files = fs.readdirSync(dir).sort();
  for (const rule of Object.keys(RULES)) if (!files.includes(`bad_${rule}.trivyignore`)) problems.push(`bad_${rule}.trivyignore is missing`);
  for (const f of files) {
    const { entries, findings } = parseIgnore(readText(path.join(dir, f)), f);
    if (f.startsWith('good') && (findings.length > 0 || entries.length === 0)) problems.push(`${f}: refused or empty: ${findings.join('; ')}`);
    if (!f.startsWith('bad_')) continue;
    const want = RULES[f.slice(4, -'.trivyignore'.length)];
    if (!want) problems.push(`${f}: no rule of that name`);
    else if (findings.length === 0 || !findings.every((x) => want.test(x))) problems.push(`${f}: not refused by its rule alone: ${findings.join('; ') || 'nothing'}`);
  }
  report(problems, 'ignore-file selftest case(s) failed');
  console.log(`ignore-file selftest: every bad_ case under ${path.relative(repo, dir)} refused, every good one read`);
}

main(() => {
  if (process.argv.includes('--selftest')) return selftest();
  for (const f of UNREAD) if (fs.existsSync(path.join(repo, f))) throw new Fail(`${f} is not read: the image scan's options are in scripts/image-scan.mjs and its exceptions in .trivyignore`);
  const own = path.join(repo, '.trivyignore');
  if (fs.existsSync(own)) report(parseIgnore(readText(own)).findings, 'problem(s) in .trivyignore');
  fs.rmSync(work, { recursive: true, force: true });
  fs.mkdirSync(work, { recursive: true });
  const { base, tag, canary: canaryTag } = dockerfile();

  let started = Date.now();
  canary(base, canaryTag);
  const canarySeconds = Math.round((Date.now() - started) / 1000);

  started = Date.now();
  build(tag, repo);
  const buildSeconds = Math.round((Date.now() - started) / 1000);

  started = Date.now();
  const none = path.join(work, 'none.trivyignore');
  fs.writeFileSync(none, '');
  const { report: doc, counted } = scan(tag, fs.existsSync(own) ? own : none);
  const scanSeconds = Math.round((Date.now() - started) / 1000);
  const os = doc.Metadata?.OS;
  const packages = (doc.Results ?? []).filter((r) => r.Class === 'os-pkgs').reduce((n, r) => n + (r.Packages ?? []).length, 0);
  if (!os?.Family || packages === 0) throw new Fail(`trivy read no operating-system packages in ${tag} (os ${os?.Family ?? 'not detected'}, ${packages} packages): the scan reaches nothing`);
  if (os.EOSL) throw new Fail(`${tag} is ${os.Family} ${os.Name}, past its end of support: no new fix reaches it; move the base image`);
  if (counted.length > 0) {
    console.error(counted.map(describe).join('\n'));
    throw new Fail(`trivy found ${counted.length} HIGH or CRITICAL vulnerabilit${counted.length === 1 ? 'y' : 'ies'} in ${tag}: move the base image or the dependency, or list it in .trivyignore with its reason and an expiry`, FOUND);
  }
  const listed = fs.existsSync(own) ? parseIgnore(readText(own)).entries.length : 0;
  console.log(`trivy: no HIGH or CRITICAL vulnerability, fixed or not, in ${tag} (${os.Family} ${os.Name}, ${packages} packages; ${listed} entr${listed === 1 ? 'y' : 'ies'} in .trivyignore)`);
  console.log(`image scan: canary ${canarySeconds} s, build ${buildSeconds} s, scan ${scanSeconds} s`);
});
