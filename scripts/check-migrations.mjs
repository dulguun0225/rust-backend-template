// The schema conventions over the committed migrations: the bans no off-the-shelf migration linter carries,
// because they need to know which column holds money or which is the key. Ported rule for rule from
// java-backend-template's MigrationConventionsTest, plus the squawk opt-out ban and the file name.
//   sequence              serial, bigserial, identity columns, create sequence
//   random-uuid           gen_random_uuid(), uuid_generate_v4()
//   uuidv7-primary-key    a table's key is `id uuid primary key default uuidv7()`, unless a
//                         `-- composite-key:` comment says why it is not
//   bare-timestamp        timestamp without time zone
//   clock-default         a clock function as a column default: a wall-clock read the Clock cannot see
//   float-or-money-type   real, double precision, float, the PostgreSQL money type
//   trigger, function     program text outside the program
//   octet-length          a length is characters, never bytes
//   money-column-type     `<x>_amount` is numeric(19,4) or numeric(20,4), not null
//   money-nan-check       `<x>_amount` carries check (<x>_amount <> 'NaN')
//   money-currency-sibling `<x>_amount` has a not-null `<x>_currency` beside it
//   squawk-ignore         any `squawk-ignore` comment: a rule is turned off in .squawk.toml, for every file
//   file-name             migrations/<NNNN>_<name>.sql, four digits, lowercase name
// Usage: node scripts/check-migrations.mjs [--selftest]
//   --selftest: every scripts/fixtures/migrations/bad_<rule>.sql must trip <rule>; good_*.sql must trip nothing.
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { main, readText, report, walk, Fail } from './_lib.mjs';

export function lint(sql) {
  const out = new Set();
  const lower = sql.replace(/--[^\n]*/g, '').toLowerCase();
  const has = (re) => re.test(lower);
  if (/squawk-ignore/i.test(sql)) out.add('squawk-ignore');
  if (has(/\b(big)?serial\b|generated\s+(always|by\s+default)\s+as\s+identity|create\s+sequence\b/)) out.add('sequence');
  if (has(/gen_random_uuid|uuid_generate_v4/)) out.add('random-uuid');
  if (has(/\btimestamp\b(?!tz)(?!\s+with\s+time\s+zone)/)) out.add('bare-timestamp');
  if (has(/default\s+(now\(\)|current_timestamp|clock_timestamp\(\)|localtimestamp|current_date|statement_timestamp\(\)|transaction_timestamp\(\))/)) out.add('clock-default');
  if (has(/\b(real|double\s+precision|float\d*|money)\b/)) out.add('float-or-money-type');
  if (has(/create\s+(or\s+replace\s+)?trigger\b/)) out.add('trigger');
  if (has(/create\s+(or\s+replace\s+)?function\b/)) out.add('function');
  if (has(/\boctet_length\s*\(/)) out.add('octet-length');
  const composite = /--\s*composite-key:/i.test(sql);
  for (const m of lower.matchAll(/create\s+table\s+(?:if\s+not\s+exists\s+)?[\w."]+\s*\(([\s\S]*?)\);/g)) {
    const body = m[1];
    if (!composite && !/\bid\s+uuid\s+primary\s+key\s+default\s+uuidv7\(\)/.test(body)) out.add('uuidv7-primary-key');
    for (const a of body.matchAll(/^\s*(\w+)_amount\s+(.+)$/gm)) {
      const [, prefix, definition] = a;
      if (!/^numeric\((19|20),\s*4\)\s+not\s+null/.test(definition)) out.add('money-column-type');
      if (!new RegExp(`check\\s*\\(\\s*${prefix}_amount\\s*<>\\s*'nan'`).test(body)) out.add('money-nan-check');
      if (!new RegExp(`\\b${prefix}_currency\\s+(char\\(3\\)|text|varchar\\(3\\))\\s+not\\s+null`).test(body)) out.add('money-currency-sibling');
    }
  }
  return out;
}

function selftest(repo) {
  const dir = path.join(repo, 'scripts', 'fixtures', 'migrations');
  const problems = [];
  const files = fs.readdirSync(dir).filter((f) => f.endsWith('.sql')).sort();
  for (const f of files) {
    const rules = lint(readText(path.join(dir, f)));
    if (f.startsWith('good_')) {
      if (rules.size > 0) problems.push(`${f}: expected nothing, got ${[...rules].join(', ')}`);
    } else {
      const rule = f.slice('bad_'.length, -'.sql'.length);
      if (!rules.has(rule)) problems.push(`${f}: ${rule} did not fire (got ${[...rules].join(', ') || 'nothing'})`);
    }
  }
  if (fileNameFindings(['0001_ok.sql', '1_short.sql', '0002_Upper.sql']).length !== 2) problems.push('file-name did not refuse 1_short.sql and 0002_Upper.sql');
  report(problems, 'migration fixture(s) not refused as expected');
  console.log(`migration selftest: ${files.length} fixtures, every rule fired on its own fixture, the good ones clean`);
}

function fileNameFindings(names) {
  return names.filter((n) => !/^\d{4}_[a-z0-9_]+\.sql$/.test(n)).map((n) => `file-name migrations/${n}: not <NNNN>_<name>.sql`);
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
  const files = walk(path.join(repo, 'migrations'));
  if (files.length === 0) throw new Fail('no migrations found under migrations/');
  const findings = fileNameFindings(files);
  for (const f of files.filter((n) => n.endsWith('.sql'))) for (const rule of lint(readText(path.join(repo, 'migrations', f)))) findings.push(`${rule} migrations/${f}`);
  report(findings, 'migration convention finding(s)');
  console.log(`migrations: ${files.length} file(s) follow the conventions`);
});
