// Source rules over the SQL in checked query macros (`query!`, `query_as!`, `query_scalar!`) in first-party
// Rust, read with the lexer in _rust.mjs. Rules:
//   write-outside-store          an INSERT, UPDATE, DELETE, MERGE or TRUNCATE outside crates/store
//   foreign-write                a feature module writes a table table-owners.toml gives another feature
//   foreign-table-in-writing-fn  a function that writes names another feature's table in any of its queries
//   unowned-table                a table the migrations create has no owner row; an owner row names no table
//   versioned-update             an UPDATE of a table with a `version` column that is not the guarded update:
//                                `set …, version = version + 1 where id = $n and version = $m`
//   order-by-id                  an ORDER BY whose first key is the `id` column: a time-ordered key is not an
//                                ordering; it is only the final tiebreak
//   offset                       OFFSET: pages are keyset only
//   clock-in-sql                 now(), current_timestamp and their kin: time comes from the injected clock
//   sqlx-inventory               (--inventory) the query literals found here differ from the committed .sqlx
//                                metadata, so this script's reach and sqlx's disagree
// A feature is the module a file belongs to: crates/store/src/<feature>.rs or crates/store/src/<feature>/…,
// and crates/store/tests/<feature>.rs for its tests. Any feature may read any table.
// What it does not see: SQL outside those macros (the unchecked functions and query_file! are banned by
// clippy.toml), a table reached through a view or a function (migrations may create neither), and a table
// named in a string built at run time (runtime SQL is banned).
// Usage: node scripts/check-sql.mjs [--inventory] [--selftest]
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { main, readText, report, walk, Fail } from './_lib.mjs';
import { functions, macroCalls, tokens } from './_rust.mjs';
import { parseToml } from './_toml.mjs';

const QUERY_MACROS = new Set(['query', 'query_as', 'query_scalar']);

/** Every checked query literal in first-party Rust: { file, line, sql, fn }. */
export function queries(root) {
  const out = [];
  for (const file of walk(path.join(root, 'crates'), (f) => f.endsWith('.rs'))) {
    const rel = `crates/${file}`;
    const toks = tokens(readText(path.join(root, rel)));
    const fns = functions(toks);
    for (const call of macroCalls(toks)) {
      if (!QUERY_MACROS.has(call.name)) continue;
      const literal = call.name === 'query_as' ? call.args.find((t, k) => t.kind === 'string' && call.args[k - 1]?.text === ',') : call.args[0];
      if (literal?.kind !== 'string') throw new Fail(`${rel}:${call.line}: ${call.name}! without a string literal`);
      const enclosing = fns.filter((f) => f.start < call.start && call.end < f.end).sort((a, b) => a.start - b.start)[0];
      out.push({ file: rel, line: call.line, sql: literal.value, fn: enclosing ? `${enclosing.name}@${enclosing.line}` : null });
    }
  }
  return out;
}

function normalize(sql) {
  return sql
    .replace(/--[^\n]*/g, ' ')
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .replace(/'(?:[^']|'')*'/g, "''")
    .toLowerCase()
    .replace(/\s+/g, ' ')
    .trim();
}

/** Tables the migrations create, and which of them carry a `version` column. */
function schema(root) {
  const tables = new Set();
  const versioned = new Set();
  for (const file of walk(path.join(root, 'migrations'), (f) => f.endsWith('.sql'))) {
    const sql = normalize(readText(path.join(root, 'migrations', file)));
    for (const m of sql.matchAll(/create table (?:if not exists )?([a-z_][a-z0-9_]*) \((.*?)\);/g)) {
      tables.add(m[1]);
      if (/(^|, ?|\( ?)version /.test(m[2])) versioned.add(m[1]);
    }
    for (const m of sql.matchAll(/alter table (?:only )?([a-z_][a-z0-9_]*) add (?:column )?(?:if not exists )?version /g)) versioned.add(m[1]);
  }
  return { tables, versioned };
}

function featureOf(file) {
  const m = /^crates\/store\/(?:src|tests)\/([a-z_][a-z0-9_]*)(?:\.rs$|\/)/.exec(file);
  return m && m[1] !== 'lib' && m[1] !== 'main' ? m[1] : null;
}

function writeTargets(sql) {
  const targets = [];
  const patterns = [
    /(?<!for (?:no key )?)\bupdate (?:only )?([a-z_][a-z0-9_]*)\b(?! (?:of|nowait|skip)\b)/g,
    /\binsert into ([a-z_][a-z0-9_]*)/g,
    /\bdelete from (?:only )?([a-z_][a-z0-9_]*)/g,
    /\bmerge into ([a-z_][a-z0-9_]*)/g,
    /\btruncate (?:table )?(?:only )?([a-z_][a-z0-9_]*)/g,
  ];
  for (const re of patterns) for (const m of sql.matchAll(re)) targets.push(m[1]);
  return targets;
}

export function sqlFindings(root, { inventory = false } = {}) {
  const findings = [];
  const owners = parseToml(readText(path.join(root, 'table-owners.toml')), 'table-owners.toml').owners ?? {};
  const { tables, versioned } = schema(root);
  for (const t of tables) if (!(t in owners)) findings.push(`unowned-table ${t}: no row in table-owners.toml`);
  for (const t of Object.keys(owners)) if (!tables.has(t)) findings.push(`unowned-table table-owners.toml ${t}: no migration creates it`);

  const all = queries(root);
  const writingFns = new Map();
  for (const q of all) {
    const sql = normalize(q.sql);
    const at = `${q.file}:${q.line}`;
    const feature = featureOf(q.file);
    const targets = writeTargets(sql);
    if (targets.length > 0 && !q.file.startsWith('crates/store/')) findings.push(`write-outside-store ${at}: ${targets.join(', ')}`);
    for (const t of targets) {
      if (feature && owners[t] && owners[t] !== feature) findings.push(`foreign-write ${at}: ${feature} writes ${t}, owned by ${owners[t]}`);
      if (versioned.has(t) && /\bupdate (?:only )?/.test(sql) && new RegExp(`\\bupdate (?:only )?${t}\\b`).test(sql)) {
        const guarded = new RegExp(`^update (?:only )?${t} set (.+) where (id = \\$\\d+ and version = \\$\\d+|version = \\$\\d+ and id = \\$\\d+)( returning .+)?$`).exec(sql);
        if (!guarded || !/(^|, )version = version \+ 1(,|$)/.test(guarded[1])) findings.push(`versioned-update ${at}: ${sql}`);
      }
    }
    if (targets.length > 0 && q.fn) writingFns.set(`${q.file}#${q.fn}`, feature);
    if (/\border by (?:[a-z_][a-z0-9_]*\.)?id\b/.test(sql)) findings.push(`order-by-id ${at}: ${sql}`);
    if (/\boffset\b/.test(sql)) findings.push(`offset ${at}: ${sql}`);
    if (/\b(now ?\(|current_timestamp\b|current_date\b|current_time\b|localtimestamp\b|localtime\b|clock_timestamp ?\(|statement_timestamp ?\(|transaction_timestamp ?\(|timeofday ?\()/.test(sql)) {
      findings.push(`clock-in-sql ${at}: ${sql}`);
    }
  }
  for (const q of all) {
    const key = `${q.file}#${q.fn}`;
    if (!writingFns.has(key)) continue;
    const feature = writingFns.get(key);
    const sql = normalize(q.sql);
    const written = new Set(writeTargets(sql));
    for (const t of new Set(sql.match(/[a-z_][a-z0-9_]*/g) ?? [])) {
      // a table this very statement writes is foreign-write's finding, not this rule's
      if (tables.has(t) && !written.has(t) && feature && owners[t] && owners[t] !== feature) {
        findings.push(`foreign-table-in-writing-fn ${q.file}:${q.line}: ${q.fn.split('@')[0]} writes and names ${t}, owned by ${owners[t]}`);
      }
    }
  }
  if (inventory) {
    const sqlxDir = path.join(root, '.sqlx');
    const committed = new Set(fs.existsSync(sqlxDir) ? fs.readdirSync(sqlxDir).filter((f) => /^query-[0-9a-f]{64}\.json$/.test(f)).map((f) => f.slice(6, -5)) : []);
    const found = new Map(all.map((q) => [crypto.createHash('sha256').update(q.sql).digest('hex'), q]));
    for (const [hash, q] of found) if (!committed.has(hash)) findings.push(`sqlx-inventory ${q.file}:${q.line}: no .sqlx/query-${hash.slice(0, 12)}…; run node scripts/db.mjs prepare`);
    for (const hash of committed) if (!found.has(hash)) findings.push(`sqlx-inventory .sqlx/query-${hash}.json: no query literal in first-party Rust has this text`);
    if (found.size === 0) findings.push('sqlx-inventory no query literal found at all: the scan reached nothing');
  }
  return findings;
}

function selftest(repo) {
  const dir = path.join(repo, 'scripts', 'fixtures', 'sql');
  const cases = fs.readdirSync(dir).sort();
  const problems = [];
  for (const name of cases) {
    const findings = sqlFindings(path.join(dir, name), { inventory: name === 'sqlx-inventory' });
    const rules = new Set(findings.map((f) => f.split(' ')[0]));
    if (name === 'good' ? rules.size !== 0 : rules.size !== 1 || !rules.has(name)) {
      problems.push(`fixture ${name}: expected ${name === 'good' ? 'no finding' : `only ${name}`}, got:\n  ${findings.join('\n  ') || '(none)'}`);
    }
  }
  const expected = ['clock-in-sql', 'foreign-table-in-writing-fn', 'foreign-write', 'good', 'offset', 'order-by-id', 'sqlx-inventory', 'unowned-table', 'versioned-update', 'write-outside-store'];
  for (const e of expected) if (!cases.includes(e)) problems.push(`fixture ${e} is missing`);
  report(problems, 'SQL fixture(s) not refused as expected');
  console.log(`sql selftest: ${cases.length} fixtures, each refused by its rule alone (good: none)`);
}

main(() => {
  let opts;
  try {
    ({ values: opts } = parseArgs({ options: { inventory: { type: 'boolean' }, selftest: { type: 'boolean' } } }));
  } catch (e) {
    throw new Fail(e.message, 2);
  }
  const repo = path.resolve(import.meta.dirname, '..');
  if (opts.selftest) return selftest(repo);
  const findings = sqlFindings(repo, { inventory: opts.inventory });
  report(findings, 'SQL finding(s)');
  console.log(`sql: ${queries(repo).length} checked queries; ownership, the guarded update, keyset order and the clock hold${opts.inventory ? '; the .sqlx inventory matches' : ''}`);
});
