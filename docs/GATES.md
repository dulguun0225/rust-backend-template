# Gates

Every check `scripts/wall.mjs` runs (and the project-root CI around it), what it implements, where it lives,
and what shows it failing. Then the directives and record items that nothing here reaches, so a reader does
not mistake a green build for coverage of them.

"The record" is `docs/history/rust-backend-stack.md` in `dulguun0225/skills`, the decision this template
implements; its sections are cited in italics. Skill names are the directories under `skills/` there; a
directive is cited by published id (`money` `M-7`) or `###` heading. Where the governing directive lives in a
Java-named skill (`java-backend-api`, `java-backend-rules`, `java-backend-observability`), its content is
language-neutral and the row carries its Rust form.

Legend for *kind*: **compile** fails `cargo clippy --workspace --all-targets --all-features --locked`;
**test** fails the test run (`cargo llvm-cov`, on PostgreSQL); **script** is a Node script in the wall that
fails on exit code; **ci** is a CI step outside the wall. Nothing is advisory. A gate is counted as wired only
when something shows it failing: a committed fixture or canary the wall runs every time, a negative control
inside the test, or, marked *(run)*, a mutation applied by hand on 2026-09-29, seen failing, and reverted.

## Wired

| Gate | Kind | Where | Implements | Shown failing by |
|---|---|---|---|---|
| Workspace lints at the record's split: `forbid` for `unwrap_used`, `panic`, `todo`, `unimplemented`, `indexing_slicing`, `arithmetic_side_effects`, `integer_division`, `integer_division_remainder_used`, `as_conversions`, the four `cast_*`, `float_arithmetic`, `float_cmp`, `let_underscore_must_use`, `wildcard_enum_match_arm`, `match_wildcard_for_single_variants`, `allow_attributes`, `dbg_macro`, `print_stdout`, `print_stderr`, `mem_forget`, `unsafe_code`, `unused_must_use`; `deny` for `expect_used`, `unreachable`, `await_holding_lock`, `await_holding_invalid_type`, the three `disallowed_*`, and every warning | compile | `Cargo.toml` `[workspace.lints]` | record *Compiler and lints*, *Unsafe code*, *Panics in request paths*, *Integer overflow and lossy casts*, *Float money*, *Discarded results*, *Non-exhaustive matching*, *Locks across `.await`*; `money` `M-7` | `canaries/bans`: one marked violation per lint, `scripts/check-ban-canaries.mjs` |
| The ban list: blocking `std::fs` and `std::thread::sleep`; wall-clock reads; `Uuid::now_v7`/`new_v7`; the unchecked sqlx functions, `Executor` methods, `AssertSqlSafe`, `QueryBuilder`, `query_file!` and the `*_unchecked!` macros; `Pool::begin`/`acquire` and kin; the axum and utoipa-axum routing methods; integer division methods and `Iterator::sum`/`product`; anyhow's types, macros and `Context`; `bigdecimal::BigDecimal`; `serde_json::from_*`, `axum::body::to_bytes`, `Bytes`, `RawForm`; the `tracing` event, span and instrument macros; span guards across `.await` | compile | `clippy.toml` | record *Banned APIs and types*, *API contract* (the route-registration ban), *SQL*, *Money*, *Errors*; `primary-keys` (ids); `java-backend-rules` clock clause; `java-backend-observability` *One typed logging facade* | `canaries/bans`: one marked violation per entry; the script also fails when an entry has no canary, so a misspelled path, which clippy only warns about, fails |
| Crate-scoped exemptions: a crate-local `clippy.toml` is the root list minus the paths `clippy-scopes.toml` declares (platform: the clock read, `now_v7`, the tracing macros; money: `i128` division; money-sql: `BigDecimal`; db: `Pool::begin_with`; web: `from_slice`, `to_bytes`; server: anyhow) | script | `clippy-scopes.toml`, `crates/*/clippy.toml`, `scripts/clippy-scopes.mjs`, `check-lint-config.mjs` `clippy-scope` | record *Banned APIs and types* (scoping by crate-local `clippy.toml`, which replaces the root file) | `scripts/fixtures/lint-config/clippy-scope` |
| One pinned clippy command, `--workspace --all-targets --all-features --locked` with `SQLX_OFFLINE=true`, and the wall fails on clippy's "does not refer to" warning | script | `scripts/wall.mjs` `clippy()` | record *The CI command*, *Banned APIs and types* (a misspelled path is a warning, exit 0) | the ban canaries' coverage rule, above |
| Lint-configuration guards, one rule each: `--cap-lints`; `RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS`, `RUSTC_WRAPPER`, `RUSTC_WORKSPACE_WRAPPER`, `CLIPPY_CONF_DIR`, `RUSTC_BOOTSTRAP` in a CI, deploy, config or script file (a vendored project's root files included) or in the environment; `rustflags` or a wrapper in `.cargo/config.toml`; a member without `[lints] workspace = true` (the inline and dotted forms accepted); a manifest outside the workspace; `cfg(clippy)`, `cfg_attr(clippy, …)` and `cfg!(clippy)`; an `allow` or `expect` naming a denied or forbidden lint or a group holding one; `allow-invalid`; `overflow-checks = true` missing from the release profile; `panic = "abort"`; a channel that is not an exact version; `#![feature]`, `+toolchain`, `-Z` | script | `scripts/check-lint-config.mjs` | record *Guards on the lint configuration itself* (all five bullets), *Integer overflow*, *Web framework* (`CatchPanicLayer` does nothing under abort), *Toolchain pin*; `java-backend-rules` *No preview APIs* (the Rust form: no nightly features) | `scripts/fixtures/lint-config/<rule>/`: each refused by its rule alone, `good/` by none |
| Suppression inventory: every gate-configuration file hashed (the three `Cargo.toml` tables, `clippy.toml`, `clippy-scopes.toml`, `deny.toml`, `.squawk.toml`, `layering.toml`, `table-owners.toml`, `rust-toolchain.toml`, `rustfmt.toml`, `sgconfig.yml`, the ast-grep rules, `rules/openapi.yaml`, every `sqlx.toml`) and every `allow`, `expect` and `squawk-ignore` listed; CI diffs it | script | `suppressions.txt`, `scripts/check-suppressions.mjs` | record *Edits to the configuration*; `guardrails-toolchain` *Record the caveat that bites, per tool* | selftest: a new `expect`, an edited `clippy.toml` and an edited lint table, each reported |
| Formatter, fail on diff | script | `rustfmt.toml`, `scripts/source-rules.mjs` | `guardrails-toolchain` *A guardrail is a tool whose verdict fails a build by itself* | `scripts/fixtures/rustfmt/unformatted.rs` |
| serde source rules: every `Deserialize` type, struct or enum, carries `deny_unknown_fields`; every `Option` field of one carries `deserialize_with = "Deserialize::deserialize"`; `flatten`, `default` and `other` are banned. The derive is found through attributes and comments, so a doc comment between derive and struct, a swapped order and `cfg_attr(…, derive(Deserialize))` are all seen | script | `sgconfig.yml`, `rules/ast-grep/`, `scripts/source-rules.mjs` | record *JSON*, *Source rules* (closes its four named misses: `flatten`, `default`, `Deserialize` enums, the doc comment) | `rules/ast-grep-tests/` (every invalid case must match, every valid one must not); `scripts/fixtures/ast-grep/violations.rs` must make `ast-grep scan` exit 1 |
| SQL rules over every checked query literal: a write outside `crates/store`; a feature writing a table `table-owners.toml` gives another; a function that writes and names another feature's table; a table with no owner row, or a row with no table; an `UPDATE` of a version-columned table that is not the guarded update; an `ORDER BY` starting with `id`; `OFFSET`; `now()` and the other SQL clocks | script | `scripts/check-sql.mjs`, `table-owners.toml`, `scripts/_rust.mjs` | `ai-maintainer-principles` (each table written only by the module that owns it); `java-backend-api` *The guarded version-column update*, *Keyset pagination only*; `primary-keys` *A time-ordered key is not an ordering*; `java-backend-rules` clock clause | `scripts/fixtures/sql/<rule>/`: each refused by its rule alone, `good/` by none |
| The SQL rules' reach: the query literals the lexer finds equal the committed `.sqlx` files by hash | script | `scripts/check-sql.mjs --inventory` | record *SQL* (what the rules read is what sqlx checks) | `scripts/fixtures/sql/sqlx-inventory/` |
| Query/schema drift: `cargo sqlx prepare --workspace --check -- --all-targets --all-features` against a freshly migrated database; then the regenerated metadata compared with `.sqlx` file for file, since sqlx passes with a warning on an extra file and on an empty regeneration; every other build `SQLX_OFFLINE=true` | script | `scripts/sqlx-check.mjs`, `.sqlx/`, `scripts/_db.mjs` | record *Query/schema drift*, *SQL* gate wiring and its traps (#1470, #4117) | each run: the comparison refuses an altered copy and an empty regeneration. *(run)*: an edited `.sqlx` file and a column made nullable each failed `prepare --check` |
| NUMERIC is read and written by one mapper: every `sqlx.toml` maps NUMERIC to `money_sql::DbAmount`, which refuses a digit past the currency's minor unit and a value past `i64`, before rescaling | compile + test | `crates/money-sql`, `crates/*/sqlx.toml`, `crates/money-sql/tests/roundtrip.rs` | `money-storage` `M-10`, `M-37`; record *Money* | *(run)*: the excess-precision check removed, `roundtrip` failed; cargo-mutants on the guard's comparison times out on the billion-digit test instead of passing |
| Migration conventions: uuidv7 keys, no sequences or random generators, `timestamptz`, no clock defaults, money columns `numeric(19\|20,4) not null` with a NaN check and a currency sibling, no float or `money` types, no triggers or functions, no `octet_length`, file names `<NNNN>_<name>.sql`, and no `squawk-ignore` | script | `scripts/check-migrations.mjs` | `primary-keys` check lines; `money-java` `M-10`, `M-11`, `M-31`, `M-32`, `M-33` (language-neutral); `java-backend-rules` clock clause; `ai-maintainer-principles` *runtime-silent magic is banned*; record *Migration hazards* (the `squawk-ignore` ban) | `scripts/fixtures/migrations/bad_<rule>.sql`, ported from java-backend-template, plus `bad_squawk-ignore.sql`; `good_*.sql` clean |
| squawk over the migrations a change adds or edits; `--assume-in-transaction` except for a `-- no-transaction` file | script | `scripts/squawk-changed-migrations.mjs`, `.squawk.toml` | record *Migration hazards*; `java-backend-rules` *Every migration is linted for lock and rewrite hazards*; `money-java` `M-42` | `scripts/fixtures/squawk/bad.sql`, every run |
| An applied migration is never edited, deleted, or preceded by a new one | script | `scripts/check-applied-migrations.mjs` | record *Migrations*, *Edited applied migration* | selftest: a throwaway git repository, one case per rule |
| Crate-graph layering: first-party edges and the controlled external crates (sqlx, bigdecimal, axum, tower-http, utoipa-axum, anyhow, tracing-subscriber) as `layering.toml` lists them | script | `scripts/check-layering.mjs`, `layering.toml` | record *Layering*, *Dependency hosts* | `scripts/fixtures/layering/<rule>/` |
| cargo-deny: advisories (vulnerable, unsound, yanked and unmaintained deny), licences deny-by-default, bans (`multiple-versions = "deny"` with each duplicate on the pin date listed with its reason, `wildcards = "deny"`, the crates the stack decided against, `wrappers` for sqlx, axum, anyhow and bigdecimal, uuid's `v4` and axum's `json`/`form`/`multipart` features), sources | script | `deny.toml`, `scripts/cargo-deny.mjs` | record *Dependency hosts*; `guardrails-toolchain` *Licences gate deny-by-default over a committed dependency inventory* | a canary workspace generated each run: a local advisory against itoa, a GPL crate, anyhow outside its wrapper, a `*` requirement, uuid `v4`, a git source; all six must be named |
| cargo-shear: an unused dependency, `--deny-warnings` | script | `scripts/cargo-shear.mjs` | record *Dependency hosts* | a canary crate declaring thiserror and never using it |
| Tests on real PostgreSQL 18.6: `#[sqlx::test]` creates and migrates a database per test | test | `crates/*/tests`, `scripts/_db.mjs` | record *Real-database tests*; `java-backend-rules` *Integration tests run against real PostgreSQL* | *(run)*: the read transaction opened READ WRITE, `a_read_transaction_refuses_a_write` failed |
| Coverage floors: lines 85, regions 85, functions 85 | script | `scripts/wall.mjs` `COVERAGE` | record *Coverage* | every run: the same report held to a 101% line floor must fail |
| Property tests on `Money` and `mul_div_round`: exact and undone, cross-currency refused, the wire form round-trips, excess precision refused, every mode lands on a neighbour of the exact quotient in its direction, exact ties by mode | test | `crates/money/tests/properties.rs` | `money` `M-1`, `M-3`, `M-4`, `M-7`, `M-24` | *(run)*: HalfEven's tie made to round away. The first property set passed it, since random inputs rarely tie; `an_exact_tie_goes_where_its_mode_says` was added and fails on it |
| The OpenAPI document is written by a test, committed, and diffed; the test writes the new copy to `target/openapi/` | test | `crates/api/tests/api/openapi.rs`, `openapi/v1.json` | record *API contract* (the spec is written by a test); `java-backend-api` *One committed OpenAPI document* | *(run)*: an operation id changed, the test failed |
| The same test under `TZ=Pacific/Kiritimati`, `LC_ALL=tr_TR.UTF-8`, byte-identical | script | `scripts/openapi.mjs` | `java-backend-api` *Authoritative generation runs on one operating system* (the reproducibility half) | not shown failing: nothing in the document depends on time or locale today |
| vacuum over the committed document: no offset or page parameter, no `PATCH`, `limit` declares a maximum, every 4xx/5xx carries `Problem`, `*At` properties and only they carry `date-time`, every request-body schema closed | script | `rules/openapi.yaml` (from java-backend-template), `scripts/openapi.mjs` | `java-backend-api` vacuum lints | `scripts/fixtures/openapi/vacuum-violations.json`, which must be refused naming all six rules |
| oasdiff `breaking --fail-on ERR` against the base branch's document | script | `scripts/openapi.mjs` | record *API contract*; `java-backend-api` *Breaking-change diff* | `scripts/fixtures/openapi/breaking.json` must exit 1 and `compatible.json` exit 0, every run |
| Every route is registered through `OpenApiRouter::routes`, so the served routes are the documented ones | compile | `clippy.toml` routing entries | record *API contract* | the twelve routing canaries |
| serde and the schema agree, per type: every component has a sample, the sample validates against its schema (jsonschema, draft 2020-12), and a request type binds back to its value through the service's own reader | test | `crates/api/tests/api/schemas.rs` | record *API contract*, the second gap | the negative control: a `serialize_with` field the schema does not know must fail validation. *(run)*: `GreetingView.createdAt`'s `rfc3339` attribute removed, the test failed |
| The request-body contract: every body schema is a named component, closed, with scalar members only, none named after a path variable, and no component is the body of two operations | test | `crates/api/tests/api/openapi.rs` | `java-backend-api` *A request body refuses every member its type does not declare*, *An identifier travels in the path only*, *Each operation binds its own request type* | the negative control: a fixture document where each operation breaks exactly one check, and each check reports exactly those |
| Strict request bodies: `StrictJson<T>` reads the top-level members once and reports every undeclared member, member named after a path variable, wrong JSON type (`detail` `expected <type>`), duplicate member and missing required member (an `Option` member marked `deserialize_with` included) as entries of one `validation.failed`, binding failures first by pointer, then the feature's rule failures at pointers not yet taken; the value only through `Bound::validate`; malformed, missing, wrong-media-type and too-large bodies each coded; no detail built from the value sent | test | `crates/web/src/body.rs` and its unit tests, `crates/api/tests/api/strict_body.rs` | `java-backend-api` *A request body refuses every member its type does not declare*, *An identifier travels in the path only*; record *JSON* | the sweep over every documented body-taking operation: undeclared member, each path variable, each member as an array, broken JSON, missing body — a 400 with no transaction opened (`Tx::begun`) and no sentinel in any response or log line; its negative control `/test/lenient` must be reported. *(run)*: the unknown-member entry removed, the sweep failed |
| Every framework-edge error is a coded problem (404, 405 with `Allow`, a wrong path type, 413 by declared length and by the reader, 415); a panic and an internal error are a 500 with `platform.internal` and `incidentId` only, the incident id is the correlation id, and exactly one `request.unhandled-error` event carries it and the cause | test | `crates/web/src/edge.rs`, `crates/api/tests/api/edge.rs` | `java-backend-api` *Every error response is a problem document*, *One advice builds every error body*; `java-backend-observability` *The correlation id in an error response resolves to a log event* | *(run)*: `CatchPanicLayer` removed, `RequestBodyLimitLayer` removed, the request span removed — each failed its edge test |
| `CatchPanicLayer` and `RequestBodyLimitLayer` on the served router; an extractor with its own larger limit is held to `BODY_LIMIT` too | test | `web::edge::finish` | record *Web framework* | as above |
| The error catalog: declared by `wire_errors!`/`field_codes!` (enum, wire strings, statuses 400–599 checked at compile time, and snapshot rows from one declaration), snapshotted, one wire code to one status, and the listed catalogs equal the ones the source declares | compile + test | `crates/platform/src/catalog.rs`, `crates/api/tests/api/catalog.rs`, `snapshots/error-catalog.txt` | `java-backend-api` *Every error carries a code from one compile-checked catalog*, *The catalog is snapshotted and diffed* | *(run)*: a wire code changed, the snapshot test failed; `the_declaration_scan_finds_a_catalog` |
| One typed logging facade: the `tracing` macros are banned outside `platform`; events carry `module` and, in a request, `correlation_id`; fields are ids, counts, flags and catalog codes only; WARN and above name a `LogEvent` | compile + test | `crates/platform/src/log.rs`, `clippy.toml` | `java-backend-observability` *One typed logging facade*, *Every scoped log event carries the correlation fields*, *Event and metric names come from a compile-checked catalog* (events) | the tracing canaries; `platform::log` tests; `a_created_greeting_reads_back_at_its_location` asserts the name never reaches the log |
| UUIDv7 ids from one producer | compile + test + script | `platform::ids`, `clippy.toml`, `deny.toml` (uuid `v4`), `check-migrations.mjs` | `primary-keys` worked case | the uuid canaries; `bad_random-uuid.sql`; the cargo-deny feature canary |
| The one transaction seam: `Pool::begin` and kin banned outside `db`; a write needs `&mut WriteTx`, which only `Tx::write` makes; `Tx::read` is READ ONLY | compile + test | `crates/db/src/lib.rs` | `java-backend-rules` (the `Tx` seam) | the sqlx canaries; *(run)* above |
| The guarded version-column update and its classification (Applied, Stale, Absent) | script + test | `check-sql.mjs`, `db::versioned`, `crates/store/tests/greeting.rs` | `java-backend-api` *The guarded version-column update* | `scripts/fixtures/sql/versioned-update`; `db::versioned` tests |
| The keyset pager: opaque cursor, a malformed one refused, pages newest first with the id as the final tiebreak, no gaps or repeats | test + script | `platform::pager`, `store::greeting::page` | `java-backend-api` *Keyset pagination only*; `primary-keys` *A time-ordered key is not an ordering* | `scripts/fixtures/sql/order-by-id`, `offset` |
| Configuration defaults: a small fixed pool, the bind address | test | `crates/server/src/config.rs` | `java-backend-rules` *Bound concurrency at the limited resource* | the tests pin the committed values |
| Mutation testing: cargo-mutants `--in-diff` against the base, the whole workspace tested per mutant; exit 0 passes, exit 3 passes only when `mutants.out/missed.txt` exists and is empty, everything else fails | script | `scripts/mutants.mjs` | record *Mutation testing* | every run: a verdict selftest over each exit code, and a canary crate whose test misses mutants must be refused. Full runs on 2026-09-29 found 22 surviving mutants (two more were artifacts of testing one crate alone); each was killed by a new test, by removing a match arm no input reaches, or by rewriting a comparison that could not be told from its mutant. The last whole-workspace pass (257 mutants) ended at exit 3 with one timeout, the billion-digit guard in `DbAmount`, and one miss, `shutdown`, killed since by `shutdown_waits_for_a_signal` |
| Every action SHA-pinned | ci | `project-root/scripts/check-action-pins.mjs` | `llm-default-traps` *CI actions and scanners are SHA-pinned* | a tag reference fails the script |
| Required status checks on `main` equal the committed job names; `dev` requires none, since it takes direct pushes | ci (project root) | `project-root/scripts/check-required-checks.mjs`, `project-root/.github/rulesets/main.json` | `guardrails-toolchain` layer clause on *fails the build* | unrun: needs the forge |
| A service works on `dev`, committed to directly, with no deletion or force-push; `main` takes pull requests from `dev` only, by merge commit | ci (project root) | the source check in `project-root/.github/workflows/ci.yml` and `project-root/.gitlab-ci.yml`, `project-root/.github/rulesets/` | no published skill governs branches; a project convention, as in java-backend-template | unrun: needs the forge |
| Named path for moving a pin | process | `renovate.json`, `project-root/renovate.json` | `llm-default-traps` composite condition on SHA pins | — |
| Frontend: a required `frontend` job that states nothing gates the frontend while it is empty, and demands a lockfile-exact install and a `check` script once `frontend/package.json` exists | ci (project root) | `project-root/scripts/frontend-gate.mjs` (from java-backend-template) | `guardrails-toolchain` *Record what stayed advisory* | unrun here |

## Runs the record owed, taken 2026-09-29

On rustc and clippy 1.98.1, against PostgreSQL 18.6; each is re-run by the gate named beside it.

- **Which ban lints survive `forbid` beside the stack's macros.** Every lint in the `forbid` list builds beside
  `query!`, `query_as!`, `query_scalar!`, `#[tokio::main]`, `#[tokio::test]`, `#[sqlx::test]`, `proptest!`,
  utoipa's `ToSchema`, `OpenApi` and `#[utoipa::path]`, `routes!`, and the serde and thiserror derives. At
  `forbid`, `expect_used` is E0453 on `#[tokio::main]`; `unreachable` on `query!` with a bind parameter and on
  `#[tracing::instrument]`; and every default-group lint (`await_holding_lock`, `await_holding_invalid_type`,
  the `disallowed_*` three) on the `#[allow(clippy::all)]` that `query!` emits. `tokio::select!` trips
  `integer_division_remainder_used` in its own expansion, so it cannot be written here; `poll_fn` replaces it.
  (The workspace itself, and `canaries/bans`.)
- **The source rule on first-party `allow` and `expect`**: wired, `check-lint-config.mjs` `allow-ban-lint`.
- **The serde `flatten`, `default` and enum rules, and the doc comment between derive and struct**: closed,
  `rules/ast-grep-tests`.
- **The axum and utoipa-axum routing-method ban**: all twelve paths resolve and fire; `OpenApiRouter::routes`
  with `routes!` passes. (`canaries/bans`.)
- **`anyhow` confinement by `disallowed-types`**: `anyhow::Error` fires on the type named; `anyhow::Result` is a
  separate alias with its own entry; `anyhow!`, `bail!`, `ensure!` need `disallowed-macros`; `.context()` needs
  `anyhow::Context::context` in `disallowed-methods`. A path whose crate is not in a crate's graph is ignored
  with no warning; a misspelled item in a crate that is present only warns. (`canaries/bans`.)
- **The `NUMERIC` ↔ minor-units mapper**: values round-trip through `$1::numeric(19,4)` for USD, JPY, KWD and
  CLF; `12.345` into USD is refused as excess precision; `123456789012345678901234.5::numeric(38,4)` as
  overflow; `'NaN'::numeric` fails to decode. sqlx's `sqlx-toml` feature with a `sqlx.toml` type override makes
  every NUMERIC in a query macro decode as `DbAmount`, parameters included. (`crates/money-sql/tests`.)
- **squawk's exit code**: 1 on any finding, a warning-level `prefer-robust-stmts` included; 0 when clean; and
  0 on a violating file that starts `-- squawk-ignore-file`, hence the ban. (`squawk-changed-migrations.mjs`.)
- **`wildcard_enum_match_arm` over a foreign `#[non_exhaustive]` enum**: fires when `_` covers a known variant;
  passes when every known variant is listed beside `_`, so a variant added upstream fires on the upgrade; where
  `_` covers exactly one variant only `match_wildcard_for_single_variants` fires, so both lints are needed;
  `matches!` and `if let` are not reached; `std::num::IntErrorKind` fires with its five documented variants
  listed, since it has hidden ones. (`canaries/bans`, the two wildcard lines.)
- **tower-http 0.7.1 with axum 0.8.9**: builds with one tower-http in the graph; `CatchPanicLayer` turns a
  panic into a 500 and `RequestBodyLimitLayer` answers 413, both through the edge. (`crates/api/tests/api/edge.rs`.)
- Found beside them: `pool.execute("select 1")` compiles in sqlx 0.9 through the `Executor` trait (banned);
  `cargo sqlx prepare --check` passes, with a warning, over an extra `.sqlx` file and over an empty
  regeneration (read from sqlx-cli 0.9.0's `prepare.rs`; `sqlx-check.mjs` compares the two itself);
  cargo-deny 0.20.2 refuses a `file://` advisory database URL and, with `--offline`, reads the database already
  under `db-path`.

## Deviations from the record

- **The database server is a throwaway container the wall starts, not testcontainers.** `#[sqlx::test]` needs
  only `DATABASE_URL` pointing at a server; it creates and migrates a database per test itself. One server per
  wall run serves the query macros' prepare check, every `#[sqlx::test]`, the ban canaries' sqlx macros and
  mutation testing. testcontainers would need a harness wrapped around the macro and a container per test
  binary, and would add a dependency to every crate with database tests. The image is pinned by digest in
  `scripts/_db.mjs`.
- **More bans than the record's initial list, from runs:** `sqlx::Executor` methods (`pool.execute("…")` with
  a `&'static str` compiles in sqlx 0.9, run 2026-09-29), the transaction-opening methods outside `db`, the
  clock, the id producers, the `tracing` macros, `serde_json::from_*` and the raw body types, `query_file!` and
  the `*_unchecked!` macros. `axum::Json` and `axum::Form` are not banned by path: axum's `json` and `form`
  features are off and `deny.toml` refuses them, so the types do not exist (a ban on them was an unresolvable
  path, run).
- **More `forbid` lints than the record's:** `allow_attributes`, `dbg_macro`, `print_stdout`, `print_stderr`,
  `mem_forget`, `unused_must_use` build cleanly beside every macro the stack uses.
- **`platform` also exempts `tracing::event` and `tracing::span`:** clippy checks every macro in an expansion
  chain, and `tracing::info!` expands to `tracing::event!` (run).
- **The NUMERIC mapper is enforced at the macro level too:** the `sqlx-toml` feature and a `sqlx.toml` per crate
  with queries map NUMERIC to `DbAmount`, so a query cannot hand out a `BigDecimal` even by inference.
  `store` declares `money-sql` with its first money column: cargo-shear refuses an unused dependency, and
  without it the override fails to compile, loudly.
- **cargo-deny's `wrappers` is carried beside the `cargo metadata` allow-list**, both as the record allows.
- **squawk runs with `--assume-in-transaction` per file**, not from `.squawk.toml`, because sqlx runs a file
  starting `-- no-transaction` outside a transaction.

## Not reached

Each is a directive or record item that no gate here enforces. A row leaving this list moves to the table
above in the same commit.

| Item | Why not here | What would wire it |
|---|---|---|
| Module layering inside one crate: a feature module in `store` or `api` reaching into another | no maintained Rust tool reaches in-crate module edges (record *Layering*); crate boundaries carry the layering between layers, not between features | feature crates (`<feature>-store`, `<feature>-api`) with `layering.toml` rows, if the feature count makes the build cost worth it |
| Branch coverage, dylint custom lints, cargo-fuzz | nightly only (record *Coverage*, *Custom compiler lints*, *Property tests*); `check-lint-config.mjs` refuses nightly in the build | a separate nightly job with its own pinned nightly |
| A foreign `#[non_exhaustive]` enum's `_` arm, fully | `wildcard_enum_match_arm` fires when `_` covers a known variant and passes when every known variant is listed, so a new upstream variant fires on the upgrade (run); `matches!` and `if let` are not reached, and a std enum with hidden variants (`std::num::IntErrorKind`) fires even with every documented variant listed, so it can only be matched through them | none on stable (`non_exhaustive_omitted_patterns` is nightly) |
| Blocking calls inside an async function | the method ban is crate-wide, not per function (record *Named gaps*) | a lint that does not exist on stable |
| Cancellation safety and futurelock | no lint (record *Named gaps*) | review |
| sqlx nullability misinference through an outer join | issue #3202 open (record *Named gaps*); the `col AS "col?"` override is convention | none |
| A body read by hand | `String`, `Body` and `Request` extractors cannot be banned by path; the sweep catches such a handler only when the document declares a body for its operation | none static |
| serde: a derive produced by another macro, a type alias of `Option`, a hand-written `Deserialize` | the ast-grep rules match syntax, with no expansion or type resolution | none |
| An identifier in the path spelled differently from the body member | every check matches by name, as in java-backend-template | a naming convention a test can hold |
| An update type declaring only what it writes (the update half) | as in java-backend-template | review of the plan |
| The document under another timezone and locale, shown failing | nothing in the document depends on either today | a date-bearing default in a schema |
| OpenTelemetry export | the record's stack names tracing and OpenTelemetry; the template emits JSON lines with correlation ids and ships no exporter, as java-backend-template ships no agent: the collector endpoint is a deployment decision | `opentelemetry-otlp` and `tracing-opentelemetry` layered onto `platform::log::json_subscriber`, behind `OTEL_EXPORTER_OTLP_ENDPOINT` |
| SBOM and osv-scanner (java-backend-template) | cargo-deny's advisories check reads the RustSec database over `Cargo.lock`; no SBOM generator is pinned | `cargo-cyclonedx` plus osv-scanner, if an SBOM is a deliverable |
| Schemathesis; `limit` default and maximum, sealed cursors, strong ETags (java-backend-template's rows) | no list endpoint and no conditional request in the worked example; `platform::pager::MAX_LIMIT` exists for the first one | as in java-backend-template |
| `money-java` `M-6`, `M-8`, `M-12`…`M-18`, `M-23`, `M-35`…`M-41`, `M-43` | no money-bearing feature: `Money::parse` and `DbAmount` are shipped, the per-feature tests wait for it; a JSON number where the schema says string is `validation.wrong-type` (java-backend-template's `money.number-not-string` is not carried) | beside the first money feature |
| Metric label cardinality, alert rules, the database poller | no metrics or alert rules | as in java-backend-template |
| The async handoff (outbox, Kafka relay) and caching | out of scope for this template; java-backend-template has neither | the skills' wiring lists, with the first handoff or cache |
| Unsafe code in dependencies | `unsafe_code = "forbid"` reaches the workspace only (record *Unsafe code*) | none adopted; cargo-geiger undercounts edition 2024 |
| How LLM agents circumvent Rust lints | no claim survived the record's runs; the guards above cover the routes the runs showed to work | research |
| Authentication and authorization | not a subject of these skills; every endpoint here is open | the project's decision |
| Required checks and rulesets, shown failing | they read the forge | a first push |

## Per-project parameters

The template ships a value only so the wall is green; each is a one-commit change with a reason:

- the coverage floors (`COVERAGE` in `scripts/wall.mjs`)
- `BODY_LIMIT` (`crates/web/src/body.rs`)
- the pool size (`DATABASE_MAX_CONNECTIONS`, default 10)
- the migration `lock_timeout` and `statement_timeout`
- the currency table (`crates/money/src/currency.rs`)
- money precision, `numeric(19,4)` or `numeric(20,4)`, with the first money column
- the licence allowlist and the duplicate-version `skip` list (`deny.toml`)
