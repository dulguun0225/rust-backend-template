// Shared by the scripts beside it: run a tool, capture its output, fail with a status. Node's standard library
// only, no package.json, nothing to install; Node 22 or newer, which mise.toml pins. Every script here runs
// the same on Linux, macOS and Windows, which is why they are Node and not bash.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const [major] = process.versions.node.split('.').map(Number);
if (major < 22) {
  console.error(`node ${process.versions.node} is too old; these scripts need 22 or newer (mise.toml pins it)`);
  process.exit(1);
}

const win = process.platform === 'win32';
// npm and npx are .cmd files on Windows, and Node refuses to spawn a .cmd without a shell (since 20.12).
const batchOnWindows = new Set(['npm', 'npx']);
const quote = (a) => (win && /[\s"&|<>^()]/.test(a) ? `"${a.replaceAll('"', '\\"')}"` : a);

export class Fail extends Error {
  constructor(message, status = 1) {
    super(message);
    this.status = status;
  }
}

function spawn(cmd, args, opts) {
  const shell = win && batchOnWindows.has(cmd);
  const r = spawnSync(cmd, shell ? args.map(quote) : args, { shell, ...opts });
  if (r.error) {
    if (r.error.code === 'ENOENT') throw new Fail(`${cmd} not on PATH`);
    throw r.error;
  }
  return r;
}

const withEnv = (env) => (env ? { ...process.env, ...env } : process.env);

/** Run with inherited stdio. Throws on a non-zero exit unless {check:false}; returns the exit status. */
export function run(cmd, args = [], { check = true, env, cwd } = {}) {
  const r = spawn(cmd, args, { stdio: 'inherit', env: withEnv(env), cwd });
  const status = r.status ?? 1;
  if (check && status !== 0) throw new Fail(`${cmd} ${args.join(' ')} exited ${status}`, status);
  return status;
}

/** Run and return trimmed stdout. Throws on a non-zero exit unless {check:false}, which then returns ''. */
export function capture(cmd, args = [], { check = true, env, cwd } = {}) {
  const r = spawn(cmd, args, { stdio: ['ignore', 'pipe', check ? 'inherit' : 'ignore'], encoding: 'utf8', env: withEnv(env), cwd });
  if ((r.status ?? 1) !== 0) {
    if (check) throw new Fail(`${cmd} ${args.join(' ')} exited ${r.status}`, r.status ?? 1);
    return '';
  }
  return r.stdout.trim();
}

/** True when the command exits zero; nothing is printed. */
export function ok(cmd, args = [], opts = {}) {
  return spawn(cmd, args, { stdio: 'ignore', ...opts }).status === 0;
}

export const lines = (s) => s.split(/\r?\n/).filter((l) => l.length > 0);

/** Run main; a thrown Fail prints its message and exits with its status, anything else keeps its stack. */
export function main(fn) {
  try {
    fn();
  } catch (e) {
    if (e instanceof Fail) {
      console.error(e.message);
      process.exit(e.status);
    }
    throw e;
  }
}

/** Run and return {status, stdout, stderr}, never throwing on a non-zero exit. */
export function captureAll(cmd, args = [], { env, cwd, input } = {}) {
  const r = spawn(cmd, args, { stdio: [input === undefined ? 'ignore' : 'pipe', 'pipe', 'pipe'], encoding: 'utf8', env: withEnv(env), cwd, input, maxBuffer: 1 << 30 });
  return { status: r.status ?? 1, stdout: r.stdout ?? '', stderr: r.stderr ?? '' };
}

/** Every file under dir (relative, forward slashes), skipping target/, .git/ and node_modules/. */
export function walk(dir, keep = () => true) {
  const out = [];
  const skip = new Set(['target', '.git', 'node_modules', 'mutants.out', 'mutants.out.old']);
  const visit = (d) => {
    if (!fs.existsSync(d)) return;
    for (const entry of fs.readdirSync(d, { withFileTypes: true })) {
      if (skip.has(entry.name)) continue;
      const full = path.join(d, entry.name);
      if (entry.isDirectory()) visit(full);
      else if (entry.isFile()) {
        const rel = path.relative(dir, full).split(path.sep).join('/');
        if (keep(rel)) out.push(rel);
      }
    }
  };
  visit(dir);
  return out.sort();
}

/** The file's text with CRLF normalised to LF. */
export const readText = (file) => fs.readFileSync(file, 'utf8').replace(/\r\n/g, '\n');

/** Print each finding and fail with a count when there is any. */
export function report(findings, what) {
  for (const f of findings) console.error(f);
  if (findings.length > 0) throw new Fail(`${findings.length} ${what}`);
}
