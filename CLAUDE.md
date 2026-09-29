# CLAUDE.md

This directory is a Rust backend service created from `dulguun0225/rust-backend-template`. Read this file,
then `docs/GATES.md`. If this is `backend/` inside a project, the project's own `CLAUDE.md` one level up
covers the repo shape; this one covers the service.

## What is already decided

The stack and every build gate are fixed by the template and by the stack record in `dulguun0225/skills`
(`docs/history/rust-backend-stack.md`). Do not re-derive or re-design them; do not write scaffolding, lint
config, source rules or CI from scratch. They exist, they are green, and `node scripts/wall.mjs` here is the
definition of done.

- Rust stable at the exact version `rust-toolchain.toml` pins, edition 2024, tokio, axum 0.8 with tower-http,
  utoipa and utoipa-axum, sqlx 0.9 with checked macros only, sqlx migrations, PostgreSQL 18, tracing. Exact
  `=` pins, `Cargo.lock` committed, every build `--locked`; Renovate PRs move them.
- One crate per layer; `layering.toml` lists every allowed edge. `platform` (clock, ids, the logging facade, the
  catalogs, the pager) and `money` use no other first-party crate; `money-sql` is the one NUMERIC mapper; `db`
  holds `Tx`, the one transaction seam; `store` holds feature SQL; `web` holds the HTTP platform; `api` holds
  one module per feature; `server` is the binary. Only `db`, `store` and `money-sql` may declare sqlx (and `api` as a
  dev-dependency, for `#[sqlx::test]`); only `web` and `api` axum; only `server` anyhow. A crate lives at
  `crates/<dir>/` with a row in `layering.toml`, and has no build script and no procedural-macro target; a
  dependency by path is a workspace member.
- Every statement is `sqlx::query!`, `query_as!` or `query_scalar!`, checked against the migrated schema; the
  unchecked functions, `QueryBuilder` and `AssertSqlSafe` are banned. Every statement runs in `tx.read(…)` or
  `tx.write(…)`; a store function that writes takes `&mut db::WriteTx` and lives in a `store` feature
  module. Only `db` makes the pool.
- Errors are RFC 9457 problems whose `code` comes from a `wire_errors!` catalog; handlers return
  `web::problem::ApiError`, and the edge codes every other error response.
- Every request body binds as `web::body::StrictJson<T>` and reaches the handler only through
  `Bound::validate`, before any transaction: an undeclared member, a member named after a path variable, a
  wrong JSON type, a duplicate member and a missing required member are each an entry of one
  `validation.failed`. An identifier travels in the path only; each operation binds its own request type, with
  scalar members only, and an update type declares only the fields that operation writes.
- Money is `money::Money`, whole minor units; the only division is `money::mul_div_round`, which names its
  `RoundingMode`. A column is `NUMERIC(19,4)` or `NUMERIC(20,4)` with a currency sibling, read and written as
  `money_sql::DbAmount` (every `sqlx.toml` here maps NUMERIC to it; `store` declares `money-sql` with its first
  money column).
- Ids are UUIDv7 from `platform::ids::new_id`; time comes from the injected `platform::clock::Clock`; logging
  goes through `platform::log::Log`, with typed fields only.
- An `UPDATE` on a table with a `version` column is the guarded update,
  `set …, version = version + 1 where id = $1 and version = $2`, classified by `db::versioned::outcome`.
- API-only. A frontend, if the project has one, is a separate static deploy and consumes `openapi/v1.json`.

## Skills

Install the rule set these gates implement, once per machine:

```
npx skills add dulguun0225/skills -g -a claude-code -y
```

The skills carry the reasoning and the checks; this directory carries the wired checks. When a skill and a
gate here disagree, the gate is wrong or stale: fix the gate, do not bypass it.

## Working here

Base branch: `main`

The line above names the trunk: work is committed on it directly. Tools read it as written, in that one
form, unindented and once, and only from the `CLAUDE.md` at a repository's root: where this directory
is `backend/` inside a project, the project's own `CLAUDE.md` states the trunk and this line is not read. A
repository whose trunk has another name changes the name between the backticks.
This template works on `main`; the services made from it work on `dev`, and `scripts/init.mjs` sets that in
the project `CLAUDE.md` it lifts and, in a standalone service, in this line.

- `node scripts/wall.mjs` is exactly what CI runs. Docker required; `mise install` gives the pinned tools and
  rustup the pinned toolchain. Scripts are Node, standard library only.
  After squawk the wall runs the project checks: the Node scripts listed one per line, as paths relative to the
  project root, in `scripts/wall-checks.txt` there (the project root is the directory above this one when it is
  `backend/` inside a project, otherwise this directory); blank lines and `#` lines are skipped, a listed path
  that does not exist or a script that exits non-zero fails the wall, and with no such file the step runs
  nothing. The template ships none.
- The wall builds the service image from the `Dockerfile` and trivy scans it; a HIGH or CRITICAL vulnerability
  fails it, fixed or not. Move the base image digest or the dependency; failing that, a `.trivyignore` entry,
  `<id> exp:<yyyy-mm-dd>` below a `#` line giving the reason, then `node scripts/check-suppressions.mjs --write`.
- `cargo fmt --all` formats. `node scripts/db.mjs prepare` rewrites `.sqlx` after a query or a migration
  changes; commit it. Builds read `.sqlx` with `SQLX_OFFLINE=true`; `node scripts/db.mjs start` gives a
  migrated server for `cargo test` (`DATABASE_URL=… SQLX_OFFLINE=true cargo test --workspace`).
- A new migration is `migrations/<NNNN>_<name>.sql`, starting with `set lock_timeout` and
  `set statement_timeout`; a shipped one is never edited. A new table needs a row in `table-owners.toml`; any
  feature may read any table, only the owner writes it, and a function that writes names no other feature's
  table.
- A new wire error code goes in a `wire_errors!` or `field_codes!` catalog; a new catalog goes in
  `crates/api/tests/api/catalog.rs`. The tests write the new snapshot to `target/` and say so.
- A new endpoint changes `openapi/v1.json`; the test writes the new document to `target/openapi/` and says so.
  A new schema component needs a sample in `crates/api/tests/api/schemas.rs`.
- A new ban is one line in `clippy.toml` plus one marked violation in `canaries/bans/src/lib.rs`; the wall
  refuses either without the other. A crate that needs a banned path gets it through `clippy-scopes.toml`, then
  `node scripts/clippy-scopes.mjs --write`; never an `allow`, `expect` or `warn` naming a ban lint, which the
  wall refuses, as it refuses `#[path]`, `include!`, and a `cfg` on `debug_assertions`, `coverage` or a
  negated feature (code clippy never reads). Any other `allow` or `expect`, and any edit to a file that
  configures a gate, the scripts under `scripts/` included, changes `suppressions.txt`
  (`node scripts/check-suppressions.mjs --write`): commit it with a reason.
- Integration test files start with `#![cfg(test)]`: clippy exempts unwrap, expect, panic and indexing only
  inside `cfg(test)` code. `tokio::select!` trips `integer_division_remainder_used` in its own expansion; use
  `std::future::poll_fn`, as `crates/server/src/main.rs` does.
- The coverage floors (`scripts/wall.mjs`), `BODY_LIMIT`, the pool size, the migration timeouts and the
  currency table are this service's call; change one in a commit that says why.
