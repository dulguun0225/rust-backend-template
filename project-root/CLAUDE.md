# CLAUDE.md

One repo, one service, one micro-frontend.

Base branch: `dev`

The line above names the trunk, and tools read it as written, in that one form, unindented and once.
Everyone works on `dev` and commits to it directly; `dev` refuses deletion and force-push only
(`.github/rulesets/dev.json`), and CI runs on every push to it. When `dev` is stable, `main` takes a pull
request from `dev` and a release is made from `main`. Nothing is committed on `main`: it takes pull requests
from `dev` only, merged with a merge commit, and `.github/rulesets/main.json` and the source check in the
`backend` job hold it there. Rename a branch and change every place that names it in the same commit: this
line, the CI trigger, the source check and the rulesets.

A Claude Code session a person runs in this project keeps `dev` current itself. The person may not know git,
so without asking and without mentioning git to them, before changing a file the session updates `dev` from
origin: `git pull --ff-only`.

- `backend/` is the Rust service, API-only, created from `dulguun0225/rust-backend-template`. Its own
  `CLAUDE.md` and `docs/GATES.md` say what is decided there; `node backend/scripts/wall.mjs` is its definition of
  done. After squawk and before the crate-graph checks the wall runs each Node script listed one per line in
  `scripts/wall-checks.txt` here, a path relative to this directory, from this directory and in file order
  (blank and `#` lines skipped); a missing path or a failing script fails the wall, and no file means no project checks.
- `frontend/` is the micro-frontend, a separate static deploy. Not started; `frontend/README.md` records the
  decisions taken and the gates to wire, and the `frontend` CI job refuses frontend code that has no `check` script.
- The contract between them is `backend/openapi/v1.json`.
- `.github/workflows/ci.yml` has two jobs, `backend` and `frontend`, run on every push to `dev` and `main` and on
  every pull request, and required on `main` by `.github/rulesets/main.json` (`node scripts/apply-ruleset.mjs`
  applies `dev.json` and `main.json`). Nothing is advisory.
- `.gitlab-ci.yml` mirrors those two jobs for a GitLab remote (a docker-executor runner with `privileged = true`
  for docker:dind). Whichever forge this repo is not on, its file stays: both are CI files
  `backend/scripts/check-lint-config.mjs` scans, and the ruleset script only means something on GitHub. On GitLab,
  protect `dev` and `main` in the project's settings: `dev` allows push and no force-push; `main` allows no push
  and merges by merge request.
- `compose.yaml` runs PostgreSQL and the service locally: `docker compose up --build`.
- `mise.toml` here pins Node for these scripts and the frontend job; `backend/mise.toml` pins the backend's tools.
  Each has a `mise.lock` beside it holding every tool's sha256 per platform; after moving a pin, run `mise lock`
  in that directory. The backend wall refuses either lock when it is out of date with its `mise.toml`.

Install the skills once per machine: `npx skills add dulguun0225/skills -g -a claude-code -y`.
