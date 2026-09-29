# frontend

The repo's micro-frontend. **Not started.** This directory exists so the repo shape is decided once, and
so the `frontend` CI job (required on `main`) has something honest to say: today it reports that nothing
gates the frontend, and it starts demanding a `check` script the moment `package.json` appears
(`scripts/frontend-gate.mjs`).

## Decided, 2026-09-16 (carried from java-backend-template, 2026-09-29)

- **Separate static deploy.** The frontend builds to static assets served by the shell or a CDN. The Rust
  service under `backend/` is API-only and never serves it; the two release independently.
- **Micro-frontend mechanism: undecided, no shell exists yet.** Build a standalone application first and
  keep one exposure point (the module or component the shell will load) named in this file. When the shell
  arrives, the mechanism (Angular native federation, module federation, or whatever the shell mandates) is
  added at that point and nowhere else.
- **The contract is `backend/openapi/v1.json`.** Generated client types come from that committed document
  and are diffed against it in CI; the frontend never hand-writes a request shape.

## What to add, in this order

1. The application, with strict TypeScript and a `check` script in `package.json` that runs every gate
   below and fails on exit code. A `package-lock.json`, installed with `npm ci`.
2. Gates, each on its own exit code: lint with an architecture-boundary rule (dependency-cruiser or the
   framework's equivalent), unused-code detection (Knip), unit tests with coverage thresholds, generated
   API types regenerated from `../backend/openapi/v1.json` and diffed, end-to-end tests with an
   accessibility check (Playwright + axe) once there is a page to test.
3. The exposure point for the shell, and the mechanism, when the shell exists.

No published skill in `dulguun0225/skills` governs a frontend yet; the Angular row is a backlog harvest
there. Until it lands, the decisions above are this repo's, recorded here.
