# Fork workflow for agents

This repository is a fork (`Osanchez/xcl`) of `ivall/xcl`. The upstream rules in `AGENTS.md` and
`CONTRIBUTING.md` still apply in full; this page only adds what the fork needs.

## Branch model

- `main` is a mirror of `ivall/xcl` `main`. Never commit fork tooling to it.
- `dev` holds the fork tooling (`scripts/dev/`, `reverse-toolkit/`, `docs/guide/`, `.claude/`).
- Upstream PR branches are cut from `main`, never from `dev`, so fork tooling never reaches ivall/xcl.

## Before any task

The SessionStart hook runs `scripts/dev/preflight.sh` (read-only, never fails the session).

1. Read the preflight output: branch, dirty flag, commits behind upstream, newest upstream commits,
   last `fork-sync` run, open `sync-conflict` issue.
2. If `main` or `dev` is behind, run `scripts/dev/sync-upstream.sh` (the tree must be clean;
   `--dry-run` shows what it would do). It adds the `upstream` remote on first use.
3. Check whether the work is already covered:
   - `scripts/dev/pr-scan.py covers <keywords...>`
   - `scripts/dev/pr-scan.py covers --files-from-branch [BASE]` (files the current branch touches)
   - `gh pr list -R ivall/xcl --search "<keywords>" --state all`
4. If an open or merged PR covers the work, stop and tell the user.
5. Cut the PR branch from `main`: `git switch -c <name> main`.

## Automation

- An hourly GitHub Action (`fork-sync.yml`) mirrors upstream into `main` and merges it into `dev`.
  If origin/dev is ahead of local `dev`, pull it with `git pull --ff-only`.
- On a merge conflict the Action opens an issue labelled `sync-conflict`. Tell the user; do not
  resolve it silently.

## Docs and tools

- Architecture guides: `docs/guide/README.md` (start there before reading source).
- Reverse-engineering toolkit: `reverse-toolkit/` (`README.md`, `tools.tsv`); refresh with
  `reverse-toolkit/update.sh`.

## Hard limits

- Never push to, open PRs on, or comment on `ivall/xcl` unless the user asks in this session.
- Commit only when asked. Do not run `cargo fmt` on files. The crate builds on Windows only; say
  "not built / not tested" when you did not run it.
- Name your exact model in the AI use section of any PR (see `AGENTS.md`).
