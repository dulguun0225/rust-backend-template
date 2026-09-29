# rust-backend-template

The `backend/` of a repo whose code is written by LLM agents and read line by line by nobody: a Rust service
on axum, sqlx and PostgreSQL 18, edition 2024, with every build gate of the Rust stack record in
[`dulguun0225/skills`](https://github.com/dulguun0225/skills) (`docs/history/rust-backend-stack.md`) wired,
shown failing, and green. A new service spends its first tokens on domain code, not on scaffolding.

The record and the skills carry the decisions and the reasoning. This repo carries the consequences: the
workspace, the lint table, the ban list, the source rules, the migration lint, the contract snapshots, and one
wall script. The scripts are Node, not bash, so the wall and the scaffold run the same on Linux, macOS and
Windows; no script has a dependency to install. `docs/GATES.md` maps each gate to what it implements and to
what shows it failing, and lists, by name, what no gate here reaches.

## Use it as a project's backend

The intended shape is one project repo holding one service and one micro-frontend. This template is the
service; it is vendored into `backend/` and lifts the project-level files (root CI with `backend` and
`frontend` jobs, branch rulesets, compose, `frontend/` stub, project `CLAUDE.md`) one level up:

```bash
mkdir some_service_1 && cd some_service_1 && git init -b dev && git commit --allow-empty -m "init: empty root"   # subtree add needs a HEAD; a service works on dev
git subtree add --prefix backend https://github.com/dulguun0225/rust-backend-template.git main --squash
cd backend
node scripts/init.mjs --name some_service_1        # rename + lift project-root/ to ..
cargo fmt --all && node scripts/wall.mjs            # the rename moves no line past rustfmt's width; the wall is the definition of done
cd .. && git add -A && git commit -m "init: some_service_1 from rust-backend-template"
git branch main                                     # main takes pull requests from dev only
gh repo create acme/some_service_1 --private --source=. --push && git push -u origin main
gh repo edit acme/some_service_1 --default-branch dev
node scripts/apply-ruleset.mjs                      # dev: direct pushes, no force-push; main: PR + backend + frontend checks
```

A project is created by the `new-rust-backend` skill in `dulguun0225/skills`, which lands this template at a
recorded commit; prefer it over retyping the lines above.

`git subtree` keeps the template's history, so `git subtree pull --prefix backend … main --squash` can bring
later gate changes in; expect to resolve the name when it does.

## Use it standalone

`gh repo create acme/some_service_1 --template dulguun0225/rust-backend-template --clone`, then
`node scripts/init.mjs --name some_service_1 --standalone`: it renames, moves the base branch to `dev`, and the
template's own `.github/workflows/ci.yml` is the service's CI.

Toolchain: rustup reads `rust-toolchain.toml`; `mise install` reads `mise.toml` (Node, squawk, ast-grep,
gitleaks, cargo-deny, cargo-shear, cargo-llvm-cov, cargo-mutants, oasdiff, vacuum, sqlx-cli). Docker is needed
for the wall.

## What is in the box

| Path | What |
|---|---|
| `Cargo.toml` | The workspace: exact pins, every feature, the lint table, the release profile |
| `crates/platform` | Clock, UUIDv7 ids, the typed logging facade, the wire-code catalogs, the keyset pager. No HTTP, no SQL |
| `crates/money`, `crates/money-sql` | `Money` in whole minor units and `mul_div_round`; `DbAmount`, the one NUMERIC mapper |
| `crates/db`, `crates/store` | The migrator and `Tx`, the one transaction seam; feature SQL, checked macros only |
| `crates/web`, `crates/api` | RFC 9457 problems, the strict body reader and the edge; one module per feature, `greeting` the worked example, and the document |
| `crates/server` | The binary: configuration, logging, pool, migrations, listener |
| `migrations/`, `.sqlx/` | sqlx migrations; the committed query metadata builds read offline |
| `openapi/v1.json`, `snapshots/error-catalog.txt` | The committed contract and error catalog the tests diff |
| `clippy.toml`, `clippy-scopes.toml`, `deny.toml`, `layering.toml`, `table-owners.toml`, `.squawk.toml`, `sgconfig.yml`, `rules/` | Gate configuration, each file hashed in `suppressions.txt` with the scripts, fixtures and canaries |
| `canaries/bans` | One marked violation per ban and per denied or forbidden lint |
| `scripts/wall.mjs` | The whole wall as one command; the template's CI and a project's `backend` job both run it |
| `scripts/` | The wall's parts, each with its selftest or canary; `fixtures/` holds their negative inputs; `init.mjs`; `db.mjs` for a local database and `.sqlx` |
| `project-root/` | What a project needs at its root; `init.mjs` lifts it when vendored |
| `docs/GATES.md` | Gate-to-directive map, deviations from the record, and the named gaps |
| `CLAUDE.md` | What the agent reads first in this directory |

## Adding a feature

Copy the `greeting` shape: a migration, a row in `table-owners.toml`, a `store` module owning the table, an
`api` module with handlers, request and response types and catalogs, the catalogs listed in
`crates/api/tests/api/catalog.rs`, a sample of each new schema in `crates/api/tests/api/schemas.rs`, and
tests. Run `node scripts/db.mjs prepare` for `.sqlx`. The tests write a changed document or catalog to
`target/` and say so. Then delete `greeting`.

## Provenance

Pins, each checked newest on 2026-09-29 unless noted: Rust 1.98.1 (2026-09-01, rustup stable manifest);
tokio 1.53.1, axum 0.8.9, tower 0.5.3, tower-http 0.7.1, http-body-util 0.1.5, utoipa 6.0.0, utoipa-axum
0.3.0, sqlx 0.9.0, bigdecimal 0.4.11, serde 1.0.229, serde_json 1.0.151, serde_path_to_error 0.1.20, uuid
1.26.1, time 0.3.55, thiserror 2.0.21, anyhow 1.0.104, tracing 0.1.44, tracing-subscriber 0.3.23, proptest
1.11.0, jsonschema 0.58.2 (crates.io); base64 0.22.1 (2024-04-30), not the newest 0.23.1, because sqlx-core
0.9.0 depends on 0.22 and a second version is a duplicate `deny.toml` refuses. Tools: Node 24.21.0 (the newest
LTS line; 26.10.0 is current, not LTS), squawk 2.66.0, ast-grep 0.45.3, gitleaks 8.30.1, cargo-deny 0.20.2, cargo-shear 1.14.0,
cargo-llvm-cov 0.9.1, cargo-mutants 27.1.0, oasdiff 1.32.1, vacuum 0.30.6, sqlx-cli 0.9.0, mise 2026.9.16
(GitHub releases). PostgreSQL 18.6 (`postgres:18.6-alpine`, 2026-09-21, pinned by digest), the major
java-backend-template uses. Images `rust:1.98.1-slim-trixie` and `gcr.io/distroless/cc-debian13:nonroot`,
by digest. Actions `actions/checkout` v7.0.1, `jdx/mise-action` v5.0.0, `actions/cache` v6.1.0,
`actions/upload-artifact` v7.0.1, by SHA. Where these differ from the record (cargo-deny's release, which the
record left unestablished; vacuum 0.30.6 against java-backend-template's 0.30.5), the newest was taken.

Every gate was run green with `node scripts/wall.mjs` on 2026-09-29, on Linux with Docker 29.8.1; the runs the
record owed are recorded in `docs/GATES.md` and in the skills repo. The template's own GitHub Actions workflow
ran green on `e230ed9` (run 36506117148, 2026-09-29). The project-root workflow and GitLab CI are unrun: correct
by reading, their actions SHA-pinned.
