// A throwaway PostgreSQL server for local work, the same one the wall starts.
//   node scripts/db.mjs prepare   start a server, migrate it, rewrite .sqlx with `cargo sqlx prepare`, stop it:
//                                 run after changing a query or a migration, and commit .sqlx
//   node scripts/db.mjs start     start a migrated server and print its DATABASE_URL and container id
//   node scripts/db.mjs stop <id> stop it
import path from 'node:path';
import { main, run, Fail } from './_lib.mjs';
import { migrate, startPostgres, stopPostgres } from './_db.mjs';

const repo = path.resolve(import.meta.dirname, '..');

main(() => {
  const [command, arg] = process.argv.slice(2);
  if (command === 'prepare') {
    const db = startPostgres();
    try {
      migrate(db.url, repo);
      run('cargo', ['sqlx', 'prepare', '--workspace', '--', '--all-targets', '--all-features'], { cwd: repo, env: { DATABASE_URL: db.url, SQLX_OFFLINE: 'false' } });
    } finally {
      stopPostgres(db.id);
    }
  } else if (command === 'start') {
    const db = startPostgres();
    migrate(db.url, repo);
    console.log(`DATABASE_URL=${db.url}`);
    console.log(`stop with: node scripts/db.mjs stop ${db.id.slice(0, 12)}`);
  } else if (command === 'stop' && arg) {
    stopPostgres(arg);
  } else {
    throw new Fail('usage: node scripts/db.mjs prepare | start | stop <id>', 2);
  }
});
