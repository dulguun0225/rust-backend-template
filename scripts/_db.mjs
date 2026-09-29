// A throwaway PostgreSQL server in Docker, for the wall and for `node scripts/db.mjs`. `#[sqlx::test]` creates
// a fresh database per test on whatever server DATABASE_URL names, and the query macros prepare against it,
// so one server per wall run serves both. The image is pinned by digest; Renovate moves it.
// WALL_DB_HOST overrides the host the server is reached on (docker:dind on GitLab publishes on `docker`).
import { capture, captureAll, run, Fail } from './_lib.mjs';

export const POSTGRES_IMAGE = 'postgres:18.6-alpine@sha256:77f585114c32fbca283dc835b0596f4e52b51b4c6662d7810b2f4084f60a1873';

const pause = (ms) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);

function reachable(host, port) {
  const r = captureAll(process.execPath, [
    '-e',
    `const s=require('node:net').connect(${port},${JSON.stringify(host)});s.on('connect',()=>process.exit(0));s.on('error',()=>process.exit(1));setTimeout(()=>process.exit(1),1000);`,
  ]);
  return r.status === 0;
}

/** Starts a server and returns { id, url }. The container is removed when it stops. */
export function startPostgres() {
  if (captureAll('docker', ['info']).status !== 0) throw new Fail('docker is not running: the wall needs a PostgreSQL server');
  // Published on loopback, unless WALL_DB_HOST says the server is reached through another host (docker:dind).
  const bind = process.env.WALL_DB_HOST ? '0.0.0.0' : '127.0.0.1';
  const id = capture('docker', ['run', '-d', '--rm', '-e', 'POSTGRES_USER=wall', '-e', 'POSTGRES_PASSWORD=wall', '-e', 'POSTGRES_DB=wall', '-p', `${bind}::5432`, POSTGRES_IMAGE]);
  try {
    const mapped = capture('docker', ['port', id, '5432/tcp']).split('\n')[0];
    const port = Number(mapped.slice(mapped.lastIndexOf(':') + 1));
    const host = process.env.WALL_DB_HOST || '127.0.0.1';
    const deadline = Date.now() + 60_000;
    // pg_isready inside the container answers before the server accepts TCP on the published port, and the
    // entrypoint restarts the server once after initdb; wait for both, twice in a row.
    let ready = 0;
    while (ready < 2) {
      if (Date.now() > deadline) throw new Fail('PostgreSQL did not become ready within 60 s');
      const up = captureAll('docker', ['exec', id, 'pg_isready', '-h', '127.0.0.1', '-U', 'wall', '-d', 'wall']).status === 0 && reachable(host, port);
      ready = up ? ready + 1 : 0;
      pause(500);
    }
    return { id, url: `postgres://wall:wall@${host}:${port}/wall` };
  } catch (e) {
    stopPostgres(id);
    throw e;
  }
}

export function stopPostgres(id) {
  if (id) captureAll('docker', ['stop', '-t', '2', id]);
}

/** Applies the committed migrations to the server at url with sqlx-cli (mise.toml pins it). */
export function migrate(url, cwd) {
  run('sqlx', ['migrate', 'run', '--source', 'migrations', '--database-url', url], { cwd });
}
