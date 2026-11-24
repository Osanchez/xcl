# Developer workflow

For the human developer working in the fork `Osanchez/xcl` of `ivall/xcl`. The rules for pull requests
are in `CONTRIBUTING.md` and `AGENTS.md`; this page adds the fork's branches, automation and setup. The
short version for coding agents is [fork-workflow-for-agents.md](fork-workflow-for-agents.md).

## Branch model

| Branch | What it is | Rule |
|---|---|---|
| `main` | Mirror of `ivall/xcl` `main` | Fast-forward only, never committed to by hand (`scripts/dev/sync-upstream.sh:9`) |
| `dev` | Our work branch: `main` plus `docs/guide/`, `reverse-toolkit/`, `scripts/dev/`, `.claude/` | The sync script merges `main` into it (`scripts/dev/sync-upstream.sh:183`) |
| PR branches | One logical change each | Cut from `main`, never from `dev`, so fork tooling never reaches `ivall/xcl` |

## First-time setup

```bash
git clone https://github.com/Osanchez/xcl.git && cd xcl
git switch dev
scripts/dev/sync-upstream.sh          # adds the 'upstream' remote on first use, then syncs
reverse-toolkit/update.sh             # clone the reverse-engineering tools into reverse-toolkit/tools/
reverse-toolkit/update.sh doctor      # check the system tools for this platform; installs nothing
```

`sync-upstream.sh` adds `upstream` = `https://github.com/ivall/xcl.git` if it is missing
(`scripts/dev/sync-upstream.sh:66`-`scripts/dev/sync-upstream.sh:82`). To build and test the crate you
need Windows (see [README.md](README.md), "Where things run").

## Daily loop

1. **Preflight.** The Claude Code SessionStart hook runs `scripts/dev/preflight.sh` (`.claude/settings.json:1`).
   It is read-only and never fails the session. It prints the branch and dirty flag, how far `main` and
   `dev` are behind upstream, the newest upstream commits, the last `fork-sync` run and any open
   `sync-conflict` issue (`scripts/dev/preflight.sh:62`-`scripts/dev/preflight.sh:67`). Run it by hand any
   time: `scripts/dev/preflight.sh`.
2. **Sync.** If behind, run `scripts/dev/sync-upstream.sh` with a clean tree. If `origin/dev` is ahead
   (the hourly Action merged), `git pull --ff-only` on `dev` (`scripts/dev/preflight.sh:57`).
3. **Check that the work is not covered.** Before starting:
   ```bash
   scripts/dev/pr-scan.py covers <keywords...>
   scripts/dev/pr-scan.py covers --files-from-branch [BASE]    # files the current branch touches; BASE defaults to main
   ```
   `covers` searches open PRs and PRs merged in the last 30 days (title, body or files) and prints a ranked
   list with url, state and why it matched. Flags `--repo` (default `ivall/xcl`) and `--json` apply to
   every command (`scripts/dev/pr-scan.py:590`-`scripts/dev/pr-scan.py:591`). The browsable version is the
   [PR scan site](https://osanchez.github.io/xcl/) (see the hourly automation below). If a PR already
   covers the work, review or test it instead (`CONTRIBUTING.md:42`).
4. **Branch off `main` for upstream PRs.** `git switch -c <name> main`. Make one logical change under
   about 400 lines; commit subject `<Area>: <what the change does>` (`AGENTS.md`, "Commits").
5. **Test.** `cargo test --workspace -- --test-threads=4` on Windows; report the real counts.

### What a PR needs (digest)

The full list is `CONTRIBUTING.md:10`. In short:

- Both required checks green: `CI` and `PR checks`.
- The whole suite ran on a GPU, with counts and the `SKIPPED` tests under **Tested**.
- A runtime change (`crates/xcl/src/` outside tests) has two `xcl-smoke` blocks from
  `scripts/smoke.ps1`, one from `main` and one from the PR's head commit, pasted under **Regression run**.
- No game data; under about 400 changed lines; one logical change; rebased on `main`.
- **Not tested** and **AI use** sections filled in, naming the exact model.
- Never push, open PRs or comment on `ivall/xcl` unless you decide to; commit only when you mean to.

## Hourly automation

| Workflow | When | What it does |
|---|---|---|
| `.github/workflows/fork-sync.yml` | `17 * * * *` and manual (`.github/workflows/fork-sync.yml:5`-`.github/workflows/fork-sync.yml:6`) | Fast-forwards `main` to upstream, merges it into `dev`, pushes both with `SYNC_TOKEN` (runs `scripts/dev/sync-upstream.sh --branch dev --push-work`, `.github/workflows/fork-sync.yml:41`-`.github/workflows/fork-sync.yml:46`) |
| `.github/workflows/pr-scan-pages.yml` | `47 * * * *` and manual (`.github/workflows/pr-scan-pages.yml:5`-`.github/workflows/pr-scan-pages.yml:6`) | Runs `pr-scan.py report --out _site` and deploys it to GitHub Pages: <https://osanchez.github.io/xcl/> |

Both jobs only run when `github.repository == 'Osanchez/xcl'`, so they do nothing in `ivall/xcl`.

`pr-scan.py report [--out DIR]` writes `index.html`, `report.md` and `prs.json` (default
`reports/latest`, ignored by git): summary counts, a per-area table, overlap clusters, PRs that look done,
and stale PRs. It is read-only toward `ivall/xcl`: it never comments, labels or reviews. The site shows
public PR metadata only.

### When the sync hits a conflict

On a merge conflict the sync script exits 2 (`scripts/dev/sync-upstream.sh:198`). The workflow aborts the
merge, opens (or comments on) an issue labelled `sync-conflict` listing the files, and fails
(`.github/workflows/fork-sync.yml:57`-`.github/workflows/fork-sync.yml:81`). The next clean run closes
the issue (`.github/workflows/fork-sync.yml:83`-`.github/workflows/fork-sync.yml:93`). Resolve it locally:

```bash
git switch dev && scripts/dev/sync-upstream.sh --branch dev   # stops in the conflicted merge, lists the files
# fix the files, then:
git add <files>
git merge --continue
git push origin dev
```

`git merge --abort` gives up. The script never forces a push.

## One-time GitHub setup

1. **Default branch.** Make `dev` the fork's default branch (Settings, Branches), so the scheduled
   workflows run from it.
2. **Actions.** Enable GitHub Actions on the fork.
3. **Pages.** Settings, Pages, Source: GitHub Actions.
4. **`SYNC_TOKEN`.** Create a fine-grained personal access token limited to `Osanchez/xcl` with
   Repository permissions Contents: read and write, and Workflows: read and write (the merge brings in
   upstream workflow files, which the default token may not push). Store it:
   ```bash
   gh secret set SYNC_TOKEN -R Osanchez/xcl
   ```
   Without it, `fork-sync` fails with an error that says so (`.github/workflows/fork-sync.yml:21`-`.github/workflows/fork-sync.yml:28`).
5. **Upstream workflows in the fork.** `ci.yml` runs on pushes to `main` and on pull requests
   (`.github/workflows/ci.yml:13`-`.github/workflows/ci.yml:16`), so each mirror push to the fork's
   `main` starts a CI run there. `pr-check.yml` is `pull_request_target`
   (`.github/workflows/pr-check.yml:12`), so it runs on pull requests, not on pushes. Expect these runs in the Actions tab.

## `sync-upstream.sh` flags

Usage: `scripts/dev/sync-upstream.sh [--branch <name>] [--no-push] [--push-work] [--dry-run] [-h|--help]`
(`scripts/dev/sync-upstream.sh:38`-`scripts/dev/sync-upstream.sh:42`).

| Flag | Effect |
|---|---|
| `--branch <name>` | Work branch to merge `main` into; default is the current branch, or `dev` when on `main` |
| `--no-push` | Do not push `main` to origin |
| `--push-work` | After a successful merge, push the work branch to origin (no force) |
| `--dry-run` | Fetch, then print what would happen; change no refs, merge or push |
| `-h`, `--help` | Usage |

Env: `XCL_UPSTREAM_URL` (default `https://github.com/ivall/xcl.git`) and `XCL_UPSTREAM_BRANCH`
(default `main`). It refuses to run with an uncommitted or untracked change or with a merge, rebase or
cherry-pick in progress (`scripts/dev/sync-upstream.sh:51`-`scripts/dev/sync-upstream.sh:59`), and
stops if local `main` has commits that are not in upstream (`scripts/dev/sync-upstream.sh:106`-`scripts/dev/sync-upstream.sh:109`).

## What runs where

| Thing | macOS | Windows |
|---|---|---|
| `cargo build`, `cargo test`, `scripts/test.ps1`, `scripts/smoke.ps1`, `xcl run` | no | yes |
| `scripts/dev/*.sh`, `pr-scan.py` (Python 3.9+, `gh` required) | yes | yes (Git Bash) |
| `reverse-toolkit/update.sh` (bash 3.2 compatible) | yes | yes (Git Bash) |
| Most toolkit tools | yes | yes; a few are one platform only, see `reverse-toolkit/README.md` |
| Windows-only system tools (dxc, pix, renderdoc, windbg, rga, llvm-mc) | no | yes |

See [testing-and-scripts.md](testing-and-scripts.md) for the Python scripts that run on macOS.

## Updating the toolkit

From the repo root (`reverse-toolkit/README.md` has the manifest and the per-tool notes):

```bash
reverse-toolkit/update.sh                    # clone missing tools, fast-forward the rest
reverse-toolkit/update.sh status [--offline] # branch, HEAD, commits behind, dirty, pinned
reverse-toolkit/update.sh pin <name> <sha>   # freeze a tool at a commit
reverse-toolkit/update.sh unpin <name>       # back to the tracked branch
reverse-toolkit/update.sh doctor             # system tools check
```

`update` and `status` accept `--only a,b`. A clone with local changes is skipped with a warning and a
pinned one is left alone. `reverse-toolkit/tools.lock` records the commit of every clone after
`update`, `pin` and `unpin`. `tools/` is git-ignored: nothing from it is committed to XCL. The tools
index is [toolkit/README.md](toolkit/README.md).
