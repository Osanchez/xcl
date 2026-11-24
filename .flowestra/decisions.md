# Decisions

## 2026-11-23: Dev workspace for the XCL fork (approved by the user)

Date: 2026-11-23. Repo: Osanchez/xcl (origin), upstream ivall/xcl. Host: macOS (cargo/rustup/brew/pwsh absent).

### Goal and scope
In:
1. `scripts/dev/sync-upstream.sh`: keep `main` identical to ivall/xcl main, push it to the fork, merge it into the work branch.
2. `docs/guide/`: a user/dev guide for every component of XCL (runtime, HLE, D3D12.X, GPU recompiler, CLI and env vars, tests and scripts), plus one page per toolkit tool.
3. `reverse-toolkit/`: a manifest of every external tool, one folder per git tool (plain clones, gitignored), `update.sh` to clone or update to the latest version, and `doctor` to check system tools.
Out: any change to `crates/`, `HANDOFF.md`, `docs/specs/`, upstream `scripts/*.ps1|py`; building tools; committing tool sources or game data; PRs to ivall/xcl.

### Branch model (keeps personal tooling out of upstream PRs)
- `main` = mirror of upstream/main, fast-forward only, never committed to by hand.
- `dev` = our work branch (main + docs/guide + reverse-toolkit + scripts/dev). The sync script merges main into it.
- Upstream PR branches are cut from `main`, never from `dev` (documented in docs/guide/dev-workflow.md).

### 1. sync-upstream.sh
`scripts/dev/sync-upstream.sh [--branch <name>] [--no-push] [--dry-run]`
1. Refuse when the tree is dirty or a merge or rebase is in progress.
2. Add the `upstream` remote (https://github.com/ivall/xcl.git) if missing, and fix its URL if it is wrong.
3. `git fetch upstream --prune` and `git fetch origin --prune`.
4. Fast-forward local `main` to upstream/main: `git fetch . upstream/main:main` when another branch is checked out, `merge --ff-only` when main is. If main has diverged, stop and explain; never force.
5. Push `main` to origin, fast-forward only (skipped with `--no-push`).
6. Merge `main` into the work branch (default: the current branch, or `dev` when on main) with `--no-edit`. On conflict, stop with the files listed and the commands to finish (`git merge --continue`) or abort.
7. Print a summary: commits pulled from upstream and the new HEADs.
The script must run in bash 3.2 (macOS) and Git Bash on Windows. Check: `bash -n`, shellcheck if present, and a dry run against a scratch clone with a fake upstream.

### 2. docs/guide/
| file | covers (source) |
|---|---|
| README.md | index, reading order, links to README/HANDOFF/specs |
| architecture.md | end-to-end flow: launcher → loader/PE → resolver/modules → HLE → GPU recomp; threads, memory, VFS |
| runtime-core.md | pe, loader, resolver, modules, stubs, hooks, tls, eh, launcher, config, manifest, preflight, vfs(_hooks), state, report, log, insn/disasm, inspect, rpf/rpf_probe, util, host |
| hle.md | kernelx(+lazy, placeholders), xgameruntime/*, xaudio2, xma2, opus, mfplat, dstorage, gameinput, kbm, user, acphal, netoff, xg, dxcompiler, d3d12_layout |
| d3d12x.md | hle/d3d12x/* (device_x, list_x, real, console_list, present, frame, pso_async, vram, sparse, title_*, …) |
| gpu-recompiler.md | gpu/* (rdna2 decode/spec, xbox, tiling, texture, uma, bvh, agility, pack, desc, dxbc, shader, fence, adapter) + gpu/recomp/* + shaders.rs |
| cli-and-config.md | `xcl` subcommands and flags (main.rs), every `XCL_*` env var (136) with file:line and meaning |
| testing-and-scripts.md | test layout, testtitle/testgamedll, GPU skip, XCL_SAMPLE_*, test.ps1, smoke.ps1, pr_check.py, gen_*.py, scripts/re/* |
| dev-workflow.md | branch model, sync script, macOS vs Windows (what runs where), PR rules digest |
| toolkit/README.md + toolkit/<tool>.md | per tool: what it is, licence and constraints, prerequisites, install/build, usage with real commands (taken from the cloned source, not only the README), inputs/outputs, how it relates to XCL, caveats |
Each component page: purpose, key types and functions (path:line), data flow, env vars, tests that cover it, known limits (citing HANDOFF section names). No claims without a citation.

### 3. reverse-toolkit/
```
reverse-toolkit/
  README.md          quick start + table of tools
  tools.tsv          name | kind(git|system) | url-or-check | branch | category | platform | notes
  update.sh          clone/update git tools; doctor for system tools
  tools.lock         generated: name → commit after each update (reproducibility, rollback)
  .gitignore         ignores tools/
  tools/<name>/      plain git clones (not committed)
```
`update.sh [update|status|doctor|pin <name> <sha>|list] [--only name,...]`
- update: clone missing (`--recurse-submodules`), else fetch and `pull --ff-only` on the tracked branch; a clone with local changes is skipped with a warning; prints old..new shortlog; rewrites tools.lock.
- status: branch, HEAD, commits behind, dirty flag.
- doctor: for `kind=system` rows, runs the check command (e.g. `rustc --version`) and prints the install hint (brew/winget/URL) when it is missing. It never installs anything.
- pin: check out a commit (detached) and record it; `update` leaves pinned tools alone until `--unpin`.
- bash 3.2 compatible; check: `bash -n`, shellcheck, a real `update` + `status` run.

### Toolkit decision
Git tools (cloned into tools/):
| tool | why we need it | note |
|---|---|---|
| awesome-gta6-RE | index of the ecosystem | reference |
| GTA6-Asset-Info | format notes: RPF9, textures, BCPack, models | GPL-3.0, reference only |
| 0x52504639 | RPF9 container, TTPF, BCPack entropy notes | docs only; publishes no key |
| rpf9-research | header/TOC/name-table notes (Spanish) | GPL-3.0, publishes no key |
| gta6-pc-port-hub | shader census, root-signature fallback, blockers | archived, read-only |
| rage-tools-vi | RPF9 extraction, audio decode, script schema | MIT, Python+Rust, cross-platform |
| rage9-re | PSO data and animations → glTF | MIT/Apache |
| bcpack-rs | BCPack texture decoder: directly relevant to XCL texture work | MIT/Apache |
| SixNG | models/textures/map → glTF, runs on macOS | GPL-3.0: use, do not copy code into XCL |
| gta6-explorer | read-only browse and extract, Python | MIT |
| ttpf-studio | .ttpf texture view/export | MIT; build from source, not the unsigned exe |
| PercyStar | RPF9 explorer, Bink2, ADAT Opus | all rights reserved: use only, no modifications or code reuse |
| Vice Lens | map/models/anim/movies explorer | MIT; source build (release bundles AGPL ffmpeg) |
| 6stars | macOS Metal world viewer | no licence: use only |
| AWC from RPF9 | AWC music stream extraction | MIT; first run downloads Leonida decoder; Android station packs not used |

Excluded:
| tool | reason |
|---|---|
| rpf9-explorer | indexed mode needs an external decoder that reads the game executable to decode the encrypted index; no licence |
| RageForge | author closed it, citing leaked builds; the only full copy is a third-party re-upload |
| xbox-hle-lab | built around running the leaked game executable; auto-merging agent runner |
| xseriesrecomp | repository not found |
| (also not added) AnyBox, GTA6-BootTest, G6Recomp | built around the leaked executable |
Rule written into the toolkit README: the tools work on files from the user's own install only; no leaked builds, keys or decryption.

System tools (`kind=system`, checked by doctor; install hints only):
- Both hosts: git, gh, Python 3.11+ with uv, rustup/cargo (stable).
- macOS: Xcode CLT (6stars, Swift), ImHex (hex + pattern language for RPF9), Ghidra (PE disassembly).
- Windows (XCL itself runs only here): stable-x86_64-pc-windows-msvc, VS Build Tools + Windows SDK (DXC), PowerShell 7, a D3D12 SM 6.6 GPU, PIX and RenderDoc (frame capture), WinDbg and x64dbg, Radeon GPU Analyzer (RDNA2 ISA), LLVM with AMDGPU (llvm-mc/objdump, matching tests/data/llvm-amdgpu), Ghidra, ImHex.

### Concurrency plan / work breakdown
Wave 1 (parallel, disjoint files, Sonnet writers):
- U1 sync-script: scripts/dev/sync-upstream.sh. Acceptance: steps 1-7 above; dry run on a scratch clone shows ff, push skipped with --no-push, conflict path stops cleanly.
- U2 toolkit-scripts: reverse-toolkit/{README.md,tools.tsv,update.sh,.gitignore}. Acceptance: commands above; `update` clones all 15; `status`/`doctor` run on macOS.
- U3 docs-core: runtime-core.md, cli-and-config.md.
- U4 docs-hle: hle.md.
- U5 docs-d3d12x: d3d12x.md.
- U6 docs-gpu: gpu-recompiler.md.
- U7 docs-tests: testing-and-scripts.md.
Wave 2 (after U2 clones the tools; orchestrator runs `update.sh update` between waves):
- U8/U9 toolkit-docs: docs/guide/toolkit/*.md, split 8/7 tools, read from the clones.
- U10 docs-index: README.md, architecture.md, dev-workflow.md (needs U1, U3-U7).
Then: orchestrator review of every file, fixes back to the writers, commit on `dev`, push `main` and `dev` to origin.
Every check: `bash -n`/shellcheck and real runs for the scripts; orchestrator spot-checks of path:line citations in the docs.

### Risks
- The tools are days old and change hourly; a README can contradict its code. The toolkit pages say which commit they describe (tools.lock).
- Some tools have no licence or forbid redistribution; they are cloned for local use only and nothing from them enters the XCL tree.
- XCL cannot be built or tested on this Mac; the docs say so, and no test results will be claimed.

## 2026-11-23: Staying current with upstream, PR awareness (approved by the user)

User choices: hourly sync as a GitHub Action on the fork (not launchd); SessionStart preflight hook + one CLAUDE.md import line; PR scanner run hourly and published as a GitHub Pages site.

### A. Hourly sync (GitHub Action)
- `.github/workflows/fork-sync.yml` on `dev`. `dev` becomes the fork's default branch, because scheduled workflows only run on the default branch.
- Triggers: `schedule: cron '17 * * * *'` and `workflow_dispatch`.
- Steps:
  1. Check out `dev` with `fetch-depth: 0` and token `secrets.SYNC_TOKEN`. The token is a fine-grained PAT with contents and workflows write on Osanchez/xcl; `GITHUB_TOKEN` cannot push upstream commits that touch `.github/workflows`.
  2. Set the git identity to github-actions[bot].
  3. Run `scripts/dev/sync-upstream.sh --branch dev --push-work`.
- `sync-upstream.sh` gets `--push-work`: after a clean merge it pushes the work branch to origin, with no force.
- On conflict (exit 2), the workflow:
  - opens or updates one issue on the fork titled "Upstream merge conflict" with the file list and the run URL (label `sync-conflict`);
  - then fails.
  - On the next success it closes that issue with a comment.
- `concurrency: fork-sync` with no cancelling; `permissions: issues: write, contents: read` (pushes use the PAT).
- When `SYNC_TOKEN` is missing, the workflow fails early with a clear message.

### B. Preflight (SessionStart hook + CLAUDE.md)
- `scripts/dev/preflight.sh`:
  - Read-only, bash 3.2, prints at most ~25 lines, never fails the session (always exit 0).
  - Network calls run under a 15 s guard; offline, it says so.
  - Prints:
    - the current branch and a dirty flag;
    - commits that `main` and `dev` are behind `upstream/main`;
    - the 5 newest upstream commits;
    - whether `origin/dev` is ahead of local `dev` (the Action merged);
    - the result of the last fork-sync run (`gh run list --workflow fork-sync.yml -L 1`);
    - whether a sync-conflict issue is open;
    - a reminder to run `scripts/dev/sync-upstream.sh` and `scripts/dev/pr-scan.py covers`.
- `.claude/settings.json`: a SessionStart hook running `bash "$CLAUDE_PROJECT_DIR/scripts/dev/preflight.sh"`.
- CLAUDE.md: append exactly one line, `@docs/guide/fork-workflow-for-agents.md`.
- The fork-workflow doc tells agents, before any task:
  1. read the preflight output;
  2. if behind, run `scripts/dev/sync-upstream.sh` (the tree must be clean);
  3. run `scripts/dev/pr-scan.py covers "<keywords>"` and `gh pr list -R ivall/xcl --search "<keywords>" --state all`;
  4. if an open or merged PR covers the work, stop and tell the user;
  5. cut upstream PR branches from `main`, never from `dev`, so fork tooling never reaches ivall/xcl.

### C. PR scanner + Pages
- `scripts/dev/pr-scan.py`:
  - Python 3.9+, stdlib only; shells out to `gh api graphql` and pages through every open PR on ivall/xcl.
  - Fields per PR: number, title, url, author, isDraft, createdAt, updatedAt, reviewDecision, review count, labels, additions/deletions, changed files (paths, up to 100), head commit status rollup state.
- Area per PR:
  - from the changed paths, using an ordered path-prefix table: `crates/xcl/src/hle/d3d12x` → D3D12.X, `gpu/recomp` → Recompiler, `hle/xgameruntime` → XGameRuntime, `hle/kernelx` → kernelx, `hle/xaudio2|xma2|opus` → XAudio2, `hle/dstorage` → DirectStorage, `vfs` → VFS, `gpu/uma` → Memory, `tls` → TLS, `pe|loader|resolver` → PE loader, `shaders` → Shaders, `frame` → Frame clock, `crates/xcl/tests|tests/` → Tests/e2e, `scripts/` → Scripts, `README.md` → README, `docs/|HANDOFF.md` → Docs, `.github/` → CI. Anything else under `crates/xcl/src/gpu` → GPU; otherwise Other.
  - The title prefix before ":" also counts.
  - A PR can be in several areas; its primary area is the one with the most changed lines.
- Unreviewed: reviewDecision is not APPROVED and the review count is 0.
- Overlap clusters (computed within each primary area, and also across all PRs):
  - two PRs are linked when the Jaccard similarity of their changed-file sets is ≥ 0.5, or the token Jaccard of their titles (lowercased, area prefix and stopwords removed) is ≥ 0.6;
  - clusters are the connected components of size ≥ 2.
- "Looks done": not draft, rollup SUCCESS, unreviewed or approved-but-unmerged.
- "Stale": no update in 7 days or more.
- Commands:
  - `report [--out DIR]` writes `DIR/index.html` (one self-contained page: summary counts, per-area table, overlap clusters, looks-done list, stale list, generation time; light and dark; no external JS), plus `DIR/report.md` and `DIR/prs.json`. The default DIR is `reports/latest`, and `reports/` is ignored by `reports/.gitignore`.
  - `covers [KEYWORDS...] [--files-from-branch [BASE]]` searches open PRs and PRs merged in the last 30 days, matching title, body or files against the keywords, or the files of `git diff --name-only BASE...HEAD` (BASE defaults to main). It prints a ranked list with url, state and why it matched.
  - `--repo` (default ivall/xcl) and `--json` flags.
- `.github/workflows/pr-scan-pages.yml`:
  - triggered hourly (cron `47 * * * *`) and by workflow_dispatch;
  - runs `pr-scan.py report --out _site` with `GH_TOKEN: ${{ github.token }}`;
  - deploys with actions/upload-pages-artifact and actions/deploy-pages;
  - permissions: `pages: write`, `id-token: write`, `contents: read`.
  - Pages source: GitHub Actions. The site is public, like the fork, and shows public PR metadata only.
- Read-only toward ivall/xcl: it never comments, labels or reviews.

### Work units
- U11 sync-auto: sync-upstream.sh `--push-work` and fork-sync.yml.
- U12 preflight: preflight.sh, .claude/settings.json, docs/guide/fork-workflow-for-agents.md, the CLAUDE.md line.
- U13 pr-scan: pr-scan.py, reports/.gitignore, pr-scan-pages.yml.
- Wave next: U10 docs-index (README, architecture, dev-workflow), which also fixes the duplicate pix/renderdoc row in reverse-toolkit/README.md.
- After the push, the orchestrator:
  - sets `dev` as the default branch;
  - enables Actions and Pages (workflow source) on the fork.
- The user creates the `SYNC_TOKEN` secret.

### C.1 Refinement (orchestrator-approved, 2026-11-23): hot files and area coverage
- The first real run made one 20-PR cluster: PRs chained through files most PRs touch (recomp/tests.rs in 24 PRs, emit.rs 20, d3d12x/real.rs 14, eh.rs 12).
- **Hot files:** a file touched by more than max(4, 5 % of open PRs) PRs. Hot files are left out of the file-set Jaccard, and a file link also needs at least one shared non-hot file. The title link is unchanged.
- The report gets a "Hot files" section (file, number of open PRs, PR links), sorted by count. These are conflict hotspots.
- Area table additions:
  - `crates/xcl/src/(eh|hooks|host|launcher|main|preflight|report|config|manifest)` → xcl run
  - `hle/(gameinput|kbm)` → Input
  - any other `crates/xcl/src/hle/` → HLE
  - any other `crates/xcl/src/` → Runtime
  - `THIRD_PARTY.md` → Docs
  - `tests/data/` → Tests/e2e
  - "Other" stays only for paths outside these.
