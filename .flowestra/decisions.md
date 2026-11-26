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

## 2026-11-24: Takedown-resistant archive (approved by the user)

### Goal
Keep every version of ivall/xcl even if upstream or the whole fork network is taken down.

### Design
- New **private, non-fork** repo `Osanchez/xcl-archive`, outside the fork network. Its default branch `meta` holds `archive.sh`, `.github/workflows/archive.yml` and a README.
- The workflow lives in the archive repo itself, so it keeps running if the fork disappears. Triggers: cron `5 * * * *` (hourly) and workflow_dispatch.
- Fetched from https://github.com/ivall/xcl (public, no token):
  - `refs/heads/*` → archive `refs/heads/upstream/<name>`
  - `refs/tags/*` → `refs/tags/<name>`
  - `refs/pull/*/head` → `refs/heads/pr/<N>`, every PR, open or closed
- Also fetched: the fork's `main` and `dev` from https://github.com/Osanchez/xcl → `refs/heads/fork/<name>`. A missing remote does not fail the run.
- Never delete:
  - no prune; refs that vanish upstream stay;
  - if a ref's new tip does not descend from the archived tip, the old tip is first pushed to `refs/heads/rewritten/<UTC yyyymmdd-HHMM>/<name>`, and then the ref is updated;
  - tags that are moved get the same treatment (`refs/tags/rewritten/<date>/<tag>`).
- Weekly (Mondays, or `workflow_dispatch` input `bundle=true`): `git bundle create xcl-<yyyymmdd>.bundle --all` of the fetched upstream refs, uploaded as release `bundle-<yyyymmdd>` in the archive repo. Old releases are kept.
- Push auth: secret `ARCHIVE_TOKEN`, the user's fine-grained PAT, extended to xcl-archive with Contents and Workflows write. `GITHUB_TOKEN` cannot push history that touches `.github/workflows`. Releases use `GITHUB_TOKEN` (`contents: write`).
- If upstream is unreachable, the run logs a warning and exits 0, so the schedule is not disabled for failing. The archive is untouched.
- Cost: a private repo uses Actions minutes, about 1 min per hourly run, roughly 720–1500 of the free 2000 min/month.
- Source of truth: the archive repo's `meta` branch. A local clone sits at ../xcl-archive.

### Work unit
- U14 archive: `archive.sh`, `archive.yml` and `README.md` in ../xcl-archive.
- Check: bash -n; scenario tests against local bare repos (first run; an incremental run; a rewritten branch preserved under rewritten/; a deleted upstream branch kept; a moved tag preserved; bundle created and `git bundle verify` passes; upstream unreachable → exit 0); YAML parse.

## 2026-11-24: PR scan page redesign with trends (approved by the user)

### Page (rendered by scripts/dev/pr-scan.py `report`)
- One self-contained `index.html`: inline CSS and a small inline JS. The only external resources are the avatar images from `https://avatars.githubusercontent.com/<login>?s=40`, lazy-loaded with an alt fallback.
- Light and dark via CSS variables, with `prefers-color-scheme` honoured. Phone width (≥360px) has a 16px gutter and no horizontal page scroll; wide tables get their own horizontal scroll.
- Sections, in order:
  1. **Header**: repo, "updated N min ago", plus an absolute UTC time.
  2. **Summary tiles**: open, unreviewed, looks done, possible duplicates (clusters), drafts. Each tile shows its 24 h change once history exists.
  3. **Trends** (when ≥2 snapshots): inline SVG line chart of open and unreviewed over the last 14 days; a "new vs closed per day" bar chart from consecutive snapshots' PR-number sets.
  4. **By area**: horizontal stacked bars per area (draft / waiting for review / reviewed / looks done), with counts and a 7-day sparkline per area from history.
  5. **Possible duplicate work**: one card per cluster, largest first. A card shows the member PRs (number, title, author, state pills), shared non-hot files as chips, and the match reason with its score.
  6. **Ready to merge** ("looks done"): table of PR, area, age, CI and size, oldest first.
  7. **Hot files**: path, count, a heat bar and PR links.
  8. **All PRs by area**: collapsible `<details>` per area. Rows show avatar, #number, title, pills (CI ok/fail/pending, review state, draft, looks done, stale), age, +/− lines and file count.
- **Filters** (JS, progressive enhancement; with JS off everything is shown): a text search over title, number, author and paths; area chips; state toggles (draft, unreviewed, looks done). Filter state lives in the URL hash.
- All PR-provided text is HTML-escaped. No external JS or CSS.

### History
- The workflow keeps an orphan branch `pr-history` in the fork with `history.jsonl`. Each line has `ts` (UTC ISO), `open`, `unreviewed`, `draft`, `looksDone`, `clusters`, `areas` (area → open count) and `prs` (sorted open PR numbers).
- `pr-scan.py report --history FILE`:
  - reads the existing lines (tolerating a missing file or bad lines);
  - appends the current snapshot, but only if the last line is ≥30 minutes older;
  - keeps at most 90 days (drops older lines);
  - renders the trends from the file.
- pr-scan-pages.yml:
  - `permissions: contents: write` is added;
  - before the report, it fetches `origin pr-history` into a temp worktree, or starts an empty orphan branch;
  - after the report, it commits `history.jsonl` as github-actions[bot] and pushes with plain `git push origin HEAD:pr-history`, no force;
  - a push conflict is retried once after a re-fetch. History failures never block the Pages deploy.
- Safe with `GITHUB_TOKEN`: the branch has no workflow files.

### Checks
- unittest for:
  - history append, the 30-minute rule, 90-day pruning and bad-line tolerance;
  - new and closed counts from PR-number sets;
  - SVG chart generation with 0, 1 and many points;
  - escaping.
- A real report run with a synthetic 14-day history file.
- Screenshots via `qlmanage -t -s 1400` of the generated HTML (light). Also a phone-width render if possible, viewed by the orchestrator.
- YAML parse.

### Visual direction (user request, 2026-11-24): inspired by gta6pc.meth.dev
- Inspiration only; none of their code or assets are copied. The look:
  - flat editorial dashboard: square corners, no shadows or gradients;
  - sections separated by 1px rules, and a 2px ink rule under the header bar;
  - Roboto for body text (15px/1.45) and bold Roboto Condensed for headings (h1 20px, h2 16px), big numbers (hero 72px) and stat values (20px); `font-variant-numeric: tabular-nums` throughout.
- Tokens:
  - light: page #fff, ink #10141c, soft #566070, rule #d9dee6, wash #eef1f5, accent #1546e0, partial #e8770c;
  - dark: page #0e1219, ink #eef2f8, soft #a2adbc, rule #2a3240, wash #1a212c, accent #8ca8ff, partial #ffab5c.
  - Category colours per area, light/dark pairs: #d81f82/#ff7fc0, #c96a12/#ffb057, #14839f/#5fd0ea, #4f7f16/#b4e06a, #7b6a78/#cdb9c8. Further areas get extra colours tuned to the same lightness.
- Layout:
  - header row: title on the left; on the right a "live" pulsing square dot with "updated N min ago" and a light/dark/auto theme button that remembers the choice in localStorage, wrapped in try/catch;
  - a hero with one large condensed number (open PRs) and a short note line (unreviewed, looks done, possible duplicates);
  - a stat strip of label-over-value pairs;
  - on desktop (≥1100px), a two-column grid: duplicates and trends on the left; areas and ready-to-merge on the right. A single column on phones.
- Section heads: h2 on the left, muted meta text on the right (counts, time ranges).
- Lists are ruled rows, not cards. Duplicate clusters are ruled blocks with a left colour bar in the area colour.
- Fonts: Google Fonts stylesheet `family=Roboto+Condensed:wght@700&family=Roboto:wght@400;500;700&display=swap`, with system fallbacks. This is the only external CSS allowed; it overrides "no external CSS" above.

### Closer match to gta6pc.meth.dev (user request, 2026-11-24): our own code
- We write all of the CSS, JS and markup ourselves, closely following their layout and components.
- Reference copies, for reading only, are in the session scratch `ref/`: index.html, city.css, goal.css.
- **App shell on desktop (≥1100px):**
  - The page fills the viewport (`height:100dvh`), and only the panels scroll.
  - Grid areas: top / filters / main panels / meta strip.
  - Panels use `min-height:0; overflow:auto` with thin rules between them.
  - Phones keep ordinary page scrolling in a single column.
- **Top bar:** title on the left; on the right the live dot (a square stepped blink, `steps(2)`), the updated time and the theme button. Height 50px, with a 2px ink rule below.
- **Filters row**, like their `.filters`: square chips with a 1px rule border, the active chip filled with ink.
- **Hero / score box:** one huge condensed number (open PRs) with a small suffix, a note with a bold first line, and muted sub-lines.
- **Standings-style ranked list for areas**, like their `.a-standings`/`.st` rows:
  - each row has a rank number, an area name with a colour tag (like `.ptype`), a thin track bar (`.track`, a wash background with coloured segments) and right-aligned values;
  - group headers like `.st-group`.
- **"Road" step list for the PR lifecycle**, like `.a-road`/`.step` with `.pips`: draft → waiting for review → reviewed → looks done → merged in the last 24 h, with the count at each step and filled pips for the share.
- **Tabbed feed panel**, like `.feed-tabs` with `aria-pressed` buttons. Tabs: Duplicates, Ready to merge, Hot files, All PRs. Rows are ruled list items with small tags like `.news-tag`. Without JS, every tab's content is shown in sequence.
- **Bottom meta strip**, like `.a-meta`: label-over-value stats in one row on desktop and two columns on phones. The key stat (open PRs) uses the accent colour, and a "warn" stat shows the number failing CI.
- **Chart tooltip** like their `.city-tip`: inverted colours, square, small text.
- Trends chart and empty states stay as built, restyled to match.
- The data rules are unchanged: no fake data, and explicit "no data yet" states.

### Effort map and top areas (user request, 2026-11-24)
- **Top areas:** the page features the 6 areas with the most open PRs. The rest are rolled into one "Everything else" row (with a list of what it contains) in the standings and the effort map. The All PRs tab keeps a filter for every area.
- **Effort map**: the new hero visual, a 3D voxel scene in the visual language of gta6pc.meth.dev's rotatable voxel "VI". The renderer is our own: their vi3d.js has no licence and is not copied.
  - **Scene:** one voxel tower per featured area plus "Everything else", in a row on a ruled ground plane.
  - **Tower height:** open PRs, one voxel layer per PR. When the tallest tower is over 40, layers are scaled and the scale is stated in the caption.
  - **Layers, bottom to top:** looks done (green), reviewed (accent), waiting (partial/orange), draft (soft grey). Faces are shaded light, mid and dark from the theme tokens, with square pixels and no gradients.
  - **Callouts:** label, open count, unreviewed and +/− lines, with thin leader lines from each tower top to its label, as in their callout style. A score box sits over the top-left corner of the scene: open count, with the note line under it.
- **Interaction (JS):**
  - our own 2D-canvas painter's-algorithm renderer at a fixed pitch;
  - yaw rotation by drag or pointer, and by the Left/Right arrow keys when the canvas has focus; a slow idle turn, off under `prefers-reduced-motion`;
  - HiDPI aware;
  - hovering a voxel shows an inverted square tooltip ("D3D12.X · waiting for review · 11 PRs");
  - the canvas has `role="img"` and an aria-label that summarises the data.
- **Static fallback**, also used for the screenshot checks: Python generates an isometric SVG of the same scene at a fixed angle with the same callouts. JS replaces it with the canvas. It works without JS.
- Data rules are unchanged: real data only, and "No data yet" where needed.

### Ghost voxels and sunset beach scene (user request, 2026-11-24)
- Their voxel scene, studied for style only, consists of:
  - solid voxels for what has been reached and **ghost voxels** for what remains: a faint translucent fill with a thin outline;
  - a coastal ground with sand and dunes, a water band with animated pixel wave bands and foam, and palm trees;
  - a warm palette.
- We write our own renderer with our own palette in that spirit.
- **Mapping:** a tower's full height is its open PRs.
  - Solid layers are work that has moved forward: looks done (green), then reviewed (accent).
  - Ghost layers are work not yet reviewed: waiting and draft, outlined and tinted faintly with the state colour.
  - The caption explains this. Callouts keep their counts.
- **Sunset scene:**
  - a sky band behind the towers; light mode is a peach → coral → lavender dusk, dark mode a violet → deep navy night;
  - a low, square-pixel sun disc on the horizon;
  - a sandy ground plane with dune patches, a water strip along the front edge with stepped wave bands, and foam that moves slowly when motion is allowed (static under reduced motion and in the SVG);
  - a few procedural palm trees made of voxel trunks and pixel fronds, placed by a fixed seed so every render is the same.
  - The scene's colours are theme tokens (`--sky-1..3`, `--sun`, `--sand`, `--dune`, `--water`, `--deep`, `--foam`, `--trunk`, `--frond`), with light and dark sets.
- The SVG fallback shows the same scene statically. The page chrome keeps the existing tokens.

### Fit-to-viewport, consolidated filters, renderer performance (user request, 2026-11-24)
- **Filters:** the chip row becomes one compact toolbar in the top bar, or directly under it.
  - Controls: a search input; an Area `<select>` (All areas, the top 6, Everything else, then every other area under an optgroup); and a 4-way segmented state control (All · Unreviewed · Ready · Draft).
  - State stays in the URL hash. With JS off the toolbar is hidden and everything is shown.
- **Fit the viewport (desktop and tablet, viewport ≥ 900px wide):**
  - The page is exactly `100dvh`, and `body` never scrolls.
  - The grid rows and columns are fractions of the viewport. The top bar and the meta strip take their content height, and the rest is split between panels.
  - Type and spacing use `clamp()` with vw/vh terms, so a smaller window scales them down instead of scrolling.
  - **The 3D scene panel never scrolls.** Its canvas sizes to the panel's box, using a ResizeObserver with debounced rAF, and the scene re-fits to it.
  - The trends charts go in their own panel, or a tab, that is sized to fit. SVG charts get viewBox plus `preserveAspectRatio` to fill their box.
  - Only lists that are genuinely long scroll, and only inside their own panel: the duplicates feed, All PRs and the standings when there are more rows than fit. Those panels get thin, styled scrollbars.
  - Below 900px the page scrolls normally in one column, and the scene height is `min(60vh, 420px)`.
- **Renderer performance:**
  - Static layers go on an offscreen canvas, rebuilt only on resize, theme change or yaw change: sky, sun, ground, dunes and palms.
  - Each tower's state layer is drawn as 3 face polygons plus one batched path of grid lines, instead of a path per voxel.
  - Fills are batched by colour.
  - Painter's-order sorting happens once per yaw change.
  - Animation runs only while needed, capped at 30 fps: wave and foam bands are drawn as cheap rects on a separate light pass.
  - Drawing pauses when the canvas is off-screen (IntersectionObserver) or the tab is hidden.
  - The idle spin is slow, and under reduced motion there is none.
  - `devicePixelRatio` is capped at 2.
  - Hover hit-testing uses the cached projected polygons per tower and layer, and doesn't redraw unless the hovered layer changes.
  - No shadowBlur and no filters.
- **Target:** per-frame draw calls cut by at least 5× compared with the current renderer, measured with the jsc mock-canvas op counter before and after.

### Page-wide sunset, transparent scene, 180° pan (user request, 2026-11-24)
- **The whole page sits on a sunset gradient:**
  - light: peach `#ffd8a8` → coral `#ff9f80` → rose `#e9849c` → lavender `#b39ddb`, top to bottom;
  - dark: deep violet `#3b2a5a` → plum `#5a2f55` → dusk `#2a1f3d` → navy `#0e1219`;
  - the gradient is fixed to the viewport, so it does not repaint on panel scroll.
  - Panels and the top bar get a translucent surface for readable text (light `rgba(255,255,255,.78)`, dark `rgba(14,18,25,.74)`). Text contrast stays at WCAG AA or better. No backdrop-filter, for performance.
- **The 3D canvas is transparent:**
  - no sky, sun, water, sand, dunes or palms in the scene;
  - only the towers, a faint translucent ground plate under them for anchoring, and the callouts;
  - the SVG fallback is transparent in the same way.
  - The scene tokens `--sky-*`, `--sun`, `--sand` etc. are removed, or reused for the page gradient.
- **Pan, not spin:** yaw is limited to a 180° arc, ±90° around the default view.
  - Drag and arrow keys clamp to that range.
  - The idle motion is a slow ease back and forth inside the arc. There is no idle motion under reduced motion, and it stops after user input.
- **Zoom:** the scene is drawn about 15% smaller than it fits now (scale ×0.85), centred.
- Everything else stays: the fit-to-viewport layout, the toolbar, the performance work and the data rules.

### Area groups and vibrant sunset (user choices, 2026-11-24)
- **Groups**, used by the effort map, the standings and the Area select's top section:
  - Core runtime = kernelx, xcl run, Runtime, VFS, TLS, PE loader, Memory;
  - Tooling = Scripts, CI, README, Docs.
  - A PR's group comes from its primary area.
  - The featured towers are the groups and ungrouped areas ranked by open PRs: Recompiler, D3D12.X, Core runtime, Tooling, Tests/e2e, XGameRuntime, HLE (top 7). The rest go into "Everything else".
  - The Area select keeps every individual area, in an optgroup under each group, so a member area can still be filtered alone.
  - "Other" is labelled "Other (unmapped paths)" in lists.
- **Sunset stops, top to bottom:**
  - light: gold `#ffb000` → hot orange `#ff5e3a` → magenta `#e0307a` → violet `#6a35c9`;
  - dark: `#ff7a1a` → `#c2185b` → `#4a148c` → `#120b2e`.
  - The panel surface alpha is tuned so all text keeps WCAG AA on every stop; the contrast test covers the new stops.

### Light theme without gradient (user request, 2026-11-24)
- Light theme: a plain white page (`#fff`), with no gradient and no `body::before` sunset. Panels are flat white, separated by the `--rule` lines, as in the original flat look.
- The sunset gradient is for the dark theme only, which keeps its current stops and surface.
- This applies both to the explicit light choice on the theme button and to "auto" when the OS is in light mode.
- The contrast tests are updated: light text is checked against white.

### Closed PRs and "Other" naming (user request, 2026-11-24)
- **Closed PRs:**
  - The report also fetches every PR closed in the last 7 days, merged or not, through the search API (`repo:ivall/xcl is:pr is:closed closed:>=DATE`), paged, with the same fields plus `state`, `mergedAt` and `closedAt`.
  - Areas and groups are computed the same way as for open PRs.
  - Constant `CLOSED_DAYS = 7`.
- **Where closed PRs show:**
  1. **Effort map:** each tower gets a solid base layer, "merged in 7 d", in a new token colour `--merged` (violet, light and dark pair, AA-checked). Closed-unmerged PRs are not stacked. The callout adds "N merged · M closed (7 d)", and the caption explains the layer.
  2. **Standings:** the state-mix track gets a merged segment, and the row text adds the merged count.
  3. A **new "Closed" tab** in the feed, listing the PRs closed in the last 7 days, newest first. Each row has a `merged` or `closed` tag, the closed age, the area and the author.
  4. The **state switch** gains "Closed": All · Unreviewed · Ready · Draft · Closed. Closed filters the All PRs list to closed PRs, and All shows open PRs only.
  5. **Duplicates:** clustering runs over open and closed PRs together. A cluster that contains a merged PR is marked "possibly superseded: #N merged", and its merged members are listed with the merged tag. Clusters made only of closed PRs are dropped.
  6. **Road to merged** keeps "Merged 24 h".
- **Naming:**
  - The rolled-up tower and row "Everything else" is renamed **"Other"**.
  - The unmapped-path area is also labelled **"Other"**, with no "(unmapped paths)" suffix, everywhere: towers, the select, lists and markdown.
  - In the Area select, the rolled-up optgroup is "Other" with "All Other (n)", then its member areas. The unmapped area appears in it as "Other".
- Snapshots and history are unchanged: they count open PRs only.

### Time-range filter and page tabs (user request, 2026-11-24)
- **Time range control**, a segmented control in the toolbar: `1 h · 12 h · 24 h · 7 d · 30 d`. The default is 7 d, and the choice lives in the URL hash as `range=`.
  - Closed and merged PRs: shown if closed or merged inside the range. The fetch window grows to 30 days (`CLOSED_DAYS = 30`), paged.
  - Open PRs: in lists and the effort map, shown if created or updated inside the range. The "open" headline and the road keep the total open count, labelled "all open".
  - The effort map, standings, superseded marks and tab counts all follow the range.
  - Python precomputes one dataset per range: scene towers, standings rows and counts. The JS swaps between them without recomputing clustering in the browser.
  - List rows carry `data-created`, `data-updated` and `data-closed` (epoch seconds), and the JS filters them by range.
  - An empty range shows "No PRs in the last <range>." There is no fake data.
- **Page tabs**, in the top bar next to the title, with state in the hash as `page=`:
  - **Effort:** the hero (open count and note), the effort map as large as the panel allows, and Road to merged.
  - **Areas:** standings in full, with per-area merged and closed for the range, the 7-day sparkline, member areas for groups, and the top 3 changed files per area.
  - **Trends:** the open/unreviewed chart, opened vs closed per day, and merged per day (from the closed data), sized large.
  - **Stats:** the label-over-value stat strip, expanded into a grid. It includes CI failing, stale, drafts and possible duplicates, with the 24 h change or "no 24 h data yet".
- **The live feed is on every page:** a persistent right-hand column (about 32% wide at ≥1100px; below the page content at narrower widths) holding the tabbed list (Duplicates, Ready to merge, Hot files, All PRs, Closed), with its own scroll.
- The page content area fits the viewport with no body scroll at ≥900px. Each page tab fills the space left of the feed.
- With JS off, all page tabs are shown stacked, and every row is shown.
- The rule for scroll stays the same: only long lists scroll, inside their own panel.

### Roomier navbar, GTA-style display font, no scene caption (user request, 2026-11-24)
- **Navbar in two rows, at ≥900px:**
  - Row 1: the title on the left, the page tabs (Effort · Areas · Trends · Stats) next to it, and on the right the live dot, updated time and theme button.
  - Row 2, the toolbar: the search box (wider, at least 240px), the Area select, the range switch and the state switch, in that order. Gaps of at least 16px between groups and comfortable 8–12px padding on the controls.
  - Both rows fit the viewport; the content grid gives up the height. Below 900px the toolbar wraps onto several lines.
- **Display font:**
  - "Anton" (Google Fonts, SIL OFL), a heavy condensed face in the spirit of GTA's Pricedown titles, for the title, page tabs, section heads, hero numbers, tower callout names and stat values.
  - Pricedown itself is not used: its web-embedding licence is unclear.
  - Body and UI text stay Roboto, and Roboto Condensed is dropped where Anton replaces it.
  - The single Google Fonts stylesheet loads Anton plus Roboto 400/500/700, with `display=swap`.
- **No scene caption:** the "Effort map. One tower per area…" sentence is removed, and so is the scale note. The colour legend stays, and the tooltips still give exact counts.

### Inner spacing pass (user request, 2026-11-24)
- **Spacing tokens** on an 8 px rhythm, scaled with `clamp()` for small windows: `--sp-1:4px`, `--sp-2:8px`, `--sp-3:12px`, `--sp-4:16px`, `--sp-5:24px`. All paddings and gaps use the tokens; no ad-hoc pixel values remain in the CSS.
- **Minimum inner padding** (vertical / horizontal):
  - panels: 16 / 20;
  - the hero box, stat cells and road tiles: 12 / 16;
  - list rows in the feed, standings, closed and ready lists: 10 / 12;
  - page tabs and feed tabs: 8 / 12;
  - buttons, segmented controls, the select and the search input: 8 / 14 (the input also gets 12 px of left padding before the placeholder);
  - pills and tags: 2 / 8, plus 6 px between pills;
  - file chips: 3 / 8;
  - legend items: 6 px between swatch and label and 16 px between items;
  - SVG and canvas callout boxes: at least 8 / 10 between text and box edge, and the text stays clear of the leader line.
- **Scroll panels:** `scrollbar-gutter: stable` plus at least 8 px of right padding, so text never touches the scrollbar.
- **Text never touches** a border, rule or colour bar. The duplicate group's left colour bar gets 14 px of space before its text.
- Line-height is at least 1.35 for body text in lists.
- The viewport fit stays: when space is tight, fonts and gaps shrink through `clamp()` before padding falls below the minimums above.

### Non-crossing callouts, manual rotation only, click-to-focus tower (user request, 2026-11-24)
- **Callout cards in one row, ordered by on-screen tower position:**
  - At every yaw, the towers' projected top-centre x is computed, and the cards are laid out left to right in that order across the full panel width, with equal slots.
  - Each leader line runs from the card's bottom centre to its tower top.
  - Because the order matches, no two leader lines cross.
  - Cards are re-ordered with a short ease when a pan changes the order.
  - When a slot is under about 110 px wide, the cards switch to compact mode (name and open count only), still in one row.
  - The SVG fallback uses the same algorithm at the default yaw.
  - A test checks that the leader segments never intersect, over a sweep of yaws within the ±90° arc.
- **No auto-rotation:** the idle ease and spin are removed. Rotation is manual only, by drag and arrow keys.
- **Click to focus a tower:**
  - Triggers: clicking a tower or its callout card. Cards become `<button>`s, so Tab plus Enter also works. Clicking an area row's bar on the Areas page switches to Effort and focuses that tower.
  - Animation, about 600 ms ease-out, done in our own canvas code:
    1. the camera pans and zooms so the tower is centred and fills about 70% of the stage height;
    2. the other towers fade to faint ghosts;
    3. the focused tower's state layers separate vertically ("exploded" view) with small gaps, each with a side label "Merged in 7 d · 40", "Looks done · 32", and so on.
  - A detail panel slides in over the right part of the stage, on the surface background. It shows:
    - the area name and member areas;
    - per-state counts for the current range;
    - the top 3 files;
    - the PRs grouped by state (number, title and a link), with their own scroll.
  - Leaving focus: Esc, a "Back" button, or a click on empty stage plays the reverse animation.
  - Under reduced motion there is no animation: the view switches straight to the end state.
  - Focus state goes in the hash as `focus=<slug>`, so a focused view can be linked.
  - It follows the time range: changing the range while focused updates the counts and the list.
- **Data:** the per-range scene data carries, for each tower, the PR numbers per state. Titles come from a single shared `{number: [title, url, author]}` map in the JSON, escaped, so they are not duplicated per range.
- Performance stays the same: no animation loop at rest, and drawing only during the focus transition or a pan.

## 2026-11-24: Progress tab (approved by the user)

The user asked for a Progress tab like portwatch-vi.xyz: milestone steps with percentages, what the README says, and what is next or being worked on. The user chose an assessed progress file with a README-change alert, plus a scheduled Claude re-assessment, and no PortWatch rating on the page. The portwatch site has no licence, so we use the idea only and write our own ladder and code.

### Ladder (ours)
- Steps and percentages: Groundwork 15 → Loads 30 → Boots 45 → Menus 60 → In-game 80 → Playable 100.
  - Each step can be partly reached (0–1). A game's % is the position of its furthest reached step, plus the partial fraction toward the next step.
  - "Playable" means fully playable, or working as far as the game allows (for example missing online servers); the limit is noted.
- **Overall:** the % of the furthest game still in progress (not yet Playable), with "N of M games playable" next to it.

### Assessment file: `scripts/dev/progress.json`
- Fields:
  - `assessedAt`, `assessedBy` (`manual` or `claude-routine`);
  - `readmeBlob` (`git rev-parse upstream/main:README.md`) and `handoffBlob`;
  - a `ladder` array (key, label, pct, description);
  - `games`: each with name, package, levels {loads, boots, menus, ingame, playable} (0–1), limits, an evidence quote copied verbatim from the README, and a source anchor.
- Initial values, assessed from README `4d1932d2` on 2026-11-24 (manual):
  - Minecraft Preview, Unicorn Overlord: all steps 1 ("fully playable").
  - Fortnite, XDefiant: all steps 1, each with its limit (no online services / servers shut down).
  - "A newer retail game": loads 1, boots 1, menus 1, ingame 0, playable 0. Evidence: "starts in ~4 s, runs at a steady 30 fps without a crash, shows its menus and goes on past the first one to its settings"; limit "gameplay not reached yet; its minimap mesh pipelines are refused". That puts it at 60%.
- **Change alert:** at build time, `pr-scan.py` fetches the current upstream README and HANDOFF blob shas (read-only, through `gh api repos/ivall/xcl/contents/...`). If they differ from the file's, the page shows a "README changed since this assessment (date)" banner with a compare link.

### Parsed from upstream at build time (deterministic, no judgement)
- **README feature tables** under "What works": every table row with a State column becomes a checklist row (feature, state ✅/🚧/other, what it does). It is grouped by `###` section, with a % done per section (done / rows) and overall.
- **README Games table** is rendered as rows next to the ladder.
- **README Highlights** bullets are shown as is.
- **HANDOFF "Next steps (priority)":** the numbered items, each shortened to its first sentence with the full text on expand.
- **HANDOFF "In progress (unfinished)":** its sub-headings and first lines.
- **Related open PRs per next-step item:** title-token Jaccard ≥ 0.2, or a shared path mentioned in the item (crate module names, e.g. `gpu::texture` → `crates/xcl/src/gpu/texture.rs`). Up to 5 per item, ranked.
- **Being worked on now:**
  - upstream commits on main in the selected range (`gh api repos/ivall/xcl/commits?since=`), up to 30, with subject and author;
  - the 5 areas with the most PR activity in the range.
- If a fetch fails, its block says "No data yet". Nothing is invented.

### Page
- A new page tab, **Progress**, first in the tab order (Progress · Effort · Areas · Trends · Stats). The feed column stays.
- Layout, top to bottom:
  - an overall hero: big %, the current step name, "N of M games playable", and the assessed date with the change banner when needed;
  - a mega ladder bar with step ticks and labels, and a marker per game placed at its %;
  - per-game rows, each with its own small ladder of filled, partial or empty step cells plus evidence and limits;
  - README feature checklist sections with % bars;
  - What's next (HANDOFF) with related PRs;
  - Being worked on now.
- The page fits the viewport, and its long blocks scroll inside their panels. It uses the existing tokens, spacing and fonts, in light and dark.

### Scheduled re-assessment
- A Claude routine is set up with the schedule skill after the page ships. It runs every 6 hours and checks whether the upstream README or HANDOFF blob differs from `progress.json`. If it does, it re-rates the game levels using only README/HANDOFF text and the ladder definitions, updates `progress.json` (`assessedBy: claude-routine`, new blobs, verbatim evidence quotes), commits it to `dev` and pushes. If nothing changed, it does nothing.

### Progress tab shows only games in progress (user request, 2026-11-24)
- The Progress tab lists only the games that are not yet Playable, which today is the retail game (README row "A newer retail game", 🚧). Finished titles (Minecraft Preview, Unicorn Overlord, Fortnite, XDefiant) are not shown anywhere on the page: no rows, no ladder markers and no "N of M games playable" line.
- The rule is "not Playable", not a hard-coded name, so a renamed or added in-progress row still shows. If every game is Playable, the page says "Every listed game is playable."
- The hero is that game's own %, step and evidence. With several in-progress games, it shows the furthest one and lists the others below.
- progress.json still holds all games (the routine rates every row); the filter applies at render time only.

## 2026-11-25 Merge protection and telemetry spec correction (user approved both)

### Merge protection
Goal: outside code (pull requests, upstream syncs) enters only after a full-diff AI security review bound to the exact commit, plus the user's approval. Building locally afterwards is fine. Not a sandbox; a guard cannot catch every trick. Out of scope: crates/, upstream files, GitHub workflows, pushing.
Files: scripts/dev/pr-gate.py, merge-pr.py, merge-guard.py, .githooks/pre-merge-commit, new scripts/dev/test_merge_protection.py. Python standard library only, Python 3.12+, LF. State in <git-common-dir>/pr-gate/.

#### Gate (pr-gate.py)
- `scan N [--repo OWNER/NAME]` (default repo ivall/xcl) and `scan --sync` (upstream main): read the head id (gh for a PR), `git fetch --no-tags <https url> <ref>:refs/pr-gate/<id>`, require the fetched id equals the head. Never check out, build or run it.
- Full diff locally: `git diff --no-ext-diff --no-textconv --no-renames --binary <merge-base(HEAD,id)> <id>`, saved as <id>.diff. No GitHub patch text is used, so nothing is truncated. Any git error, empty diff or missing object: exit 3.
- Keep path buckets and added-line patterns as review hints. Generated files: union of the marker scan (marker alone, no "do not edit" needed) and the built-in list. Scan writes <id>.scan.json: id, base, diff_sha256, line count, repo, number, hints. Scan always ends exit 2 (review needed); --ack is removed.
- `record-review <id> --report FILE`: JSON {id, base, diff_sha256, reviewer, verdict: clean|findings|malicious, ranges: [[a,b],...], findings: [...]}. Refuse (exit 2) unless id/base/hash match the scan and the saved diff, ranges cover 1..line count with no gap, and the verdict is not malicious. Writes <id>.review.json.
- `status <id>`: exit 0 only when scan and review match and the saved diff re-hashes.

#### Merge (merge-pr.py)
- `merge-pr.py N | --sync` with `--approve <full id>` required; `--dry-run` stops before merging. Requires a clean tree, status 0, current PR/upstream head == id == approve, and merge-base(HEAD,id) == reviewed base (otherwise scan again). Merges `--no-ff` with a message naming the id, review hash and reviewer. No cherry-pick or no-commit modes. Writes <id>.approved.json just before merging; removes it if the merge fails without conflicts.

#### Guard (merge-guard.py, PreToolUse)
- Outcomes: allow (exit 0), ask (exit 0 with hookSpecificOutput permissionDecision "ask" and a reason), deny (exit 2). Internal error: deny.
- `merge-pr.py` without --dry-run: ask, so the user approves in Claude Code's own prompt. Recognised only as the program actually run, never as text or a comment.
- Deny: merge/pull/rebase/cherry-pick/reset/checkout/switch/restore/worktree add/am/apply of FETCH_HEAD, MERGE_HEAD, a URL or refs/pr-gate/*; `gh pr merge`, `gh pr checkout`, `gh repo sync`; sync-upstream.sh without --dry-run; git with -c core.hooksPath, --no-verify on merge, or `git config core.hooksPath`; enabling the fork-sync workflow.
- Ask: any other merge, pull (also --ff-only), rebase, cherry-pick, am, apply, or reset/checkout/switch whose target is not an ancestor of HEAD and not a local branch. `git checkout -- path` and creating a branch from HEAD are allowed.
- Allow: --abort/--continue/--quit/--skip, read-only git, everything else. The origin/dev exemption is removed.
- Parsing: newlines, ; && || | & ( ) { } $( ) and backticks separate commands with or without spaces; env prefixes; git global options; .exe; recurse into bash/sh -c, eval, cmd /c, powershell -Command. Heredoc bodies are data unless fed to a shell. Text it cannot parse that contains a blocked form: ask. Quoted text in echo, grep and commit messages stays allowed.

#### Hook (pre-merge-commit)
Allow when MERGE_HEAD is an ancestor of HEAD, or <id>.approved.json exists for it and HEAD matches; otherwise refuse. No remote-branch exemption. Must work in worktrees (git rev-parse --git-path).

#### Acceptance
test_merge_protection.py (unittest, no network, throwaway repositories, a local path standing in for the remote): every previous bypass (`git merge FETCH_HEAD; echo merge-pr.py`, `# merge-pr.py`, `(git merge FETCH_HEAD)`, rebase, reset --hard, checkout && build, gh pr checkout, bash -c, `;ls` without spaces) is ask or deny; data-only text is allowed; malformed input does not allow; gap in ranges, stale base, changed diff, moved head, malicious verdict, missing review and missing approval each refuse; a full happy path merges in a throwaway repository; hook cases. Run: python -I -m unittest style invocation documented in the file. Existing scripts/dev/test_pr_scan.py still passes.

### Telemetry spec correction
Files: docs/guide/telemetry.md, and in docs/guide/fork-workflow-for-agents.md only the statements that sync is hourly/automatic or done with sync-upstream.sh or git pull (now: manual, through scripts/dev/merge-pr.py --sync after review; fork-sync is disabled). Documentation; no code. Rewrite as one consistent document, under 400 lines, with the user's decisions D1, D2, D3, D4, D5, D6, D8 kept and D7 replaced.

#### Corrections that must appear
- No baseline has been measured on this machine. The 1 fps figure is VIctory's on an RX 6700 XT. XCL's README (line 47 at a67c592) reports about 5 fps in the 3D world on an RTX 5090 with #2057; its 14 fps is a whole-run average including menus and loading. Gap in the world is about 6x on that card, unknown here. Target 30. All causes H1-H6 are open, plus split wave64 (H7) and refused pipelines (H8). No ranking before measurement.
- Loop cap: `xcl_budget = 1u << 16` (65,536) at crates/xcl/src/gpu/recomp/emit.rs:6220; the direct DXIL path has its own guard. HANDOFF's 4 M is stale.
- XCL_WAVE64_SPIN=0 changes only groups of one console wave (emit.rs x64_spin); the multi-wave case always spins.
- None of the nine wave64 PRs has merged (#1193 #2343 #1596 #1597 #1111 #2377 #2097 #2210 #917); #2203 and #2305 are other PRs.
- GPU timing uses its own query heaps, never list_x.rs TimestampPool (shared 4096 slots, round robin). Pass intervals can overlap: report start and end ticks, do not sum them.
- Part order: 1 event log, manifest, refused shaders and pipelines (with HRESULT, e.g. 0x8007000E on Blackwell groupshared subtract); 2 frame CPU/GPU/wait timing; 3 pass timing; 4 shader shape report; 5 no-window mode, error events, loop probe and counters; 6 smoke.ps1 reader last. Part 5 headless must be re-estimated against the compositor.
- D7: measure overhead off versus on from part 2, on the test title; report it; sample if it exceeds 5 %.
- D6: code appended in log.rs and report.rs, one-line call sites; remove every telemetry/ module reference. D8: remove smoke-block requirements. Switch name XCL_WINDOW_SIZE. Manifest example must match D5 (no XCL_FRAME_INTERVAL_US).
- phase events carry confidence; pass events carry queue, start_tick, end_tick; nested manifest objects are allowed, events stay flat.
- Split-wave report stays clean for possible sharing on a branch from main.
- Testing uses tests/testtitle and synthetic shaders. No game run is part of this plan; choosing a title is an open question for the user.
- Known test state at a67c592 (RTX 5080, driver 621.01): 1285 passed, 5 failed, 46 ignored, 26 skip messages; one stale barrier expectation (#2074 with #2203), four driver refusals of fuzz shaders. Security review of 052fddf^1..052fddf^2 found no malicious code.
- Sync is manual now; remove hourly-sync claims.
- Status line: awaiting the user's final review; nothing built.

#### Review refinements (orchestrator, inside the approved design)
1. "Do these before building anything" becomes "Once a title is chosen": these runs need a title, so they follow part 2 and none is part of this plan.
2. Part 2 owns the query heaps and takes list-level start/end timestamps, so frame.gpu_ms is real: the union of calibrated list intervals; null when unavailable. Part 3 adds pass boundaries. Test 2 and 3 and the part table follow.
3. Ticks are per queue. Each queue's GetClockCalibration is recorded (event `clock`: queue, gpu_tick, cpu_qpc, frequency) at start and every 60 frames; intervals on different queues are compared only after calibration. Direct and compute first; copy queues only when the device supports copy-queue timestamps, else null.
4. Query slots belong to a submission, not a recording. A list submitted again before its results were read is emitted with null ticks and `resubmitted: true`. Bundles get no timestamps.
5. XCL_LOOP_GUARD and XCL_LOOP_COUNTERS change generated code, so they join the list that bypasses the shader caches (gpu/recomp/pso.rs:805) on both paths; test 5 checks a changed value is never served from cache.
6. Run directories hold title paths and names: they stay on the machine and are never committed.
7. fork-workflow guide: `merge-pr.py --sync` merges into the checked-out work branch (dev); `main` is no longer updated automatically.
8. "The exchange baseline moved with them" refers to #2203 only.

#### Protection review refinements (orchestrator, security tightening inside the approved design)
1. Hooks from incoming code must not run during the merge: core.hooksPath is the in-tree .githooks. merge-pr.py runs its merge, commit and abort with `-c core.hooksPath=<empty directory made in the state directory, verified empty>`. Test: an incoming commit that adds .githooks/pre-commit, prepare-commit-msg, commit-msg, post-commit, post-merge and post-checkout, each writing a marker, leaves no marker.
2. Gate buckets add `agent-config` (.githooks/*, .claude/*, .mcp.json, CLAUDE.md, AGENTS.md, .gitattributes, .gitmodules, reverse-toolkit/*) and printed hints for them. merge-pr.py refuses (exit 2) when the diff touches .githooks/, .claude/, scripts/dev/, CLAUDE.md or .gitattributes, unless `--allow-protected` is also given; the guard's ask reason then says so.
3. pre-merge-commit: take the incoming ids from the GITHEAD_<40 hex> environment variables git sets for this hook (measured with git 2.53: present, MERGE_HEAD absent), plus MERGE_HEAD when present; same ancestor / approval rule; none found: refuse. Test through a real `git merge` in a throwaway repository: unapproved refused, approved for this HEAD allowed.
4. Guard asks for: anything whose words or redirect targets name the pr-gate state directory (pr-gate/....json, .diff) unless the program run is pr-gate.py or a read-only viewer (cat, ls, head, tail, wc, grep, sed -n, sha256sum) with no redirect into it; writes to the protection files (redirect target, or rm/mv/cp/sed -i/tee/truncate/chmod/dd, and PowerShell Set-Content/Out-File/Remove-Item/Copy-Item/Move-Item) naming merge-guard.py, merge-pr.py, pr-gate.py, pre-merge-commit or .claude/settings*.json; `patch`; git read-tree, checkout-index, archive, and `git show|cat-file <non-ancestor>:path` with an output redirect; gh workflow run of fork-sync (deny). `gh pr diff` piped to patch is covered by patch.
5. Wrappers add timeout (with its duration word), watch, busybox, winpty, script -c, chroot, unshare, parallel. PowerShell iex / Invoke-Expression recurse into their text; Start-Process / saps / Invoke-Command / & with a git or gh program and danger words ask. A command anywhere whose any later word is git/gh followed by a FAMILY word, with an unknown program in front, asks (echo, printf, grep, rg, git, gh, cat excluded).
6. Scan diff adds `--no-color --src-prefix=a/ --dst-prefix=b/ --no-relative --submodule=short -U3`; a diff containing a binary file needs each such path named in the review's findings (record-review refuses otherwise).
7. Every row of the orchestrator's corpus is a test; remaining allowed by design: `git reset --hard origin/main` when it is an ancestor, `git clone` elsewhere, editing files through non-shell tools. The docstring says so.

## 2026-11-25 Telemetry part 1: event log, manifest, refusals, and gap fill B1-B3 (user approved the scope; orchestrator approved the design)

The user approved implementing the whole of docs/guide/telemetry.md and, for part 1, adding stall, stub and io_failed events on top of the adopted upstream PRs #1656 (XCL_STALL_DUMP), #2244 (sync state in thread dumps) and #1811 (PDB names in dumps), merged through the gate at 20304356. Designed by an Opus worker; citations spot-checked by the orchestrator against 20304356 (host.rs:38, :111-134; real.rs:553, :3750, :3852-3868; stubs.rs:44-55; dstorage.rs:1079-1088; report.rs:183-190; log.rs 136 lines, report.rs 215 lines; Cargo.toml:19-20). Paths below are relative to crates/xcl/.

### Orchestrator decisions on the open points (section 10)

- D-1 commit: (a) read `.git` at run time; `dirty` is null; `commit_source: "git-head"`. No build.rs, no process spawned.
- D-2 `rust`: null.
- D-3 stub tier: `all`, as the plan's 5.10 has it.
- D-4 `xcl.log`: stays at `logs/xcl.log`; the manifest gets one more field, `log` (its path, lossy string).
- D-5 volume limits: first 100, then every 1000th, each event carrying `n`.
- D-6 invalid `XCL_TELEMETRY`: off, with one warning.
- D-7 manifest `config`: not in part 1.
- Unit split: C9 (the `telemetry_final` call in `write_final`, report.rs:188) is written by the report.rs unit (U2/U3), not by U4, so no two units touch one file.
- Check for every unit: the orchestrator builds on the Windows PC (`cargo build --release --offline --locked -p xcl`) and runs the whole suite in the desktop session; baseline at 20304356 is 1438 passed, 4 failed (gpu::dxil::fuzz_tests::fuzz_validate_and_gpu_0..3, NVIDIA 0x8007000E), 47 ignored, 44 SKIPPED lines.

Status: draft for review. Nothing is built. Design only; the crate was not built (it does not
build on macOS). All `path:line` citations are against `20304356` (`dev` and
`adopt/stall-diagnostics` point at the same commit; `git diff` of `crates/` between them is empty).
Plan: `docs/guide/telemetry.md` (cited as "plan"). Paths below are relative to `crates/xcl/`.

---

### 1. Goal and scope

**Goal.** With `XCL_TELEMETRY=frame|pass|all`, a run leaves `<root>/run-<utc>-<commit12>/` with a
`manifest.json` written at startup and an `events.jsonl` stream, flushed line by line, that holds every
shader and pipeline refusal with its HRESULT, plus the gap-fill events: `stall`, `stub` and
`error/io_failed`. With `off`, the default, nothing is created, and each event site costs one relaxed
atomic load.

**In scope**
- A. Plan part 1 (plan:323): the run directory, `manifest.json` (plan 5.2), a JSONL writer that flushes
  each line, tier parsing (plan 5.10), `XCL_TELEMETRY` and `XCL_TELEMETRY_DIR` (plan:289–290),
  `shader_refused` events, and `pipeline` refusal events with their HRESULT (plan 5.3).
- B1. When `XCL_STALL_DUMP` fires: `list_x::log_sync_state` runs before the dumps (this happens with
  telemetry off too), and a `stall` event is written.
- B2. A `stub` event on a stub's first call, then at count thresholds (plan:132 "first call, then at
  count thresholds").
- B3. An `error` event with `kind: "io_failed"` for each failed DirectStorage request, volume-limited.
- A copy of `report.json` into the run directory on each `write_final` (plan 5.1 lists it; see
  ASSUMPTION A6).

**Explicitly out of scope**
- `frame`, `list`, `clock`, `memory`, `pass`, `resource`, `io`, `loop_guard` and `phase` events
  (parts 2, 3 and 5). The `pipeline` *creation* events (`source: cache/dxc/direct`) are part 3. Only
  refusals are in part 1.
- `XCL_TELEMETRY_SAMPLE` (part 3, D7). It means nothing without `pass` events.
- The other `error` kinds of plan 5.8 (`device_removed`, `title_exception`, `unresolved_import`, …):
  part 5.
- B4 (`activity`/`memory` periodic event). It is out of scope, for three reasons. The counters exist
  only inside the `XCL_ACTIVITY` thread (`src/host.rs:44–89`). The memory half is part 2's `memory`
  event (plan:324). `activity` is not an event type in plan 5.3. If wanted later, it is one call line
  after `src/host.rs:61` passing `&d`, plus about 15 lines in report.rs.
- `shaders.json`, the summary block in `report.json`, and moving `xcl.log` into the run directory (open
  decision D-4).

---

### 2. Facts from the tree that shape the design

| Fact | Where |
|---|---|
| `serde` + `serde_json` are already dependencies, so no new dependency is needed | `Cargo.toml:19-20` |
| The text log is a `static FILE: Mutex<Option<File>>`, unbuffered `File`, flushed at Warn and above; `START` is a `OnceLock<Instant>` set in `init` | `src/log.rs:42`, `:45`, `:52-66`, `:79-86` |
| `log::uptime()` measures from the same start, so `t` lines up with `xcl.log`'s `[ t ]` column | `src/log.rs:48-50`, `:76-78` |
| The logs dir is `data_dir/logs` | `src/config.rs:291-293` |
| The host sets up the log, then sets `XCL_UMA_MB` from config, then `set_boot`, then spawns threads | `src/host.rs:19-23`, `:35-37`, `:38`, `:39` |
| **No commit at run time today.** `build.rs` emits only translator hashes and a link arg; nothing in `src` reads a git commit | `build.rs:53-95`; `grep -rn "rev-parse\|GIT_HASH\|option_env!" src` finds nothing |
| `smoke.ps1` gets commit/dirty by running git outside the process | `scripts/smoke.ps1:34-35` |
| The OS version is already available | `src/hle/kernelx.rs:1707` (`host_os_version`) |
| Pattern for total RAM | `src/gpu/uma.rs:816-823` (`GlobalMemoryStatusEx`) |
| Device creation: `raw`, `DEVICE.store`, `ADAPTER` set | `src/hle/d3d12x/real.rs:550-554` |
| The LUID → `IDXGIAdapter3` pattern | `src/hle/d3d12x/real.rs:427-440` (`video_memory`) |
| The wave lane query exists but is private | `src/gpu/recomp/pso.rs:1197` (`wave_range`) |
| DXC versions | `src/gpu/recomp/dxc.rs:148-154` (`Toolchain`), `:200` (`toolchain()`) |
| **Shader refusal chokepoint:** `note_translation_error(e: String)`, after the heap-window retry filter and the cached-only deferral, before the log dedup | `src/hle/d3d12x/real.rs:3852-3868` |
| Refusals that carry an HRESULT give it only inside the string as `({hr:#x})` (`HRESULT = i32`, so a negative value prints as `0x8007000e`) | `src/util.rs:5`; `src/hle/d3d12x/real.rs:4386`, `:6002`, `:6051`, `:6332` |
| Cached-only attempts replay their notes through the same chokepoint once they count | `src/hle/d3d12x/real.rs:3843-3849` |
| **Pipeline refusal chokepoint:** `note_fallback(what, hr, ok)`, called for "graphics PSO", "compute PSO" and "PSO stream" | `src/hle/d3d12x/real.rs:3750-3759`; callers `:5709`, `:5754`, `:6827`, `:6832` |
| A console compute refusal reaches `note_fallback` with XCL's own `E_INVALIDARG`, not the driver's code | `src/hle/d3d12x/real.rs:5746` |
| Stall watcher (#1656): warn, dump, sleep 1 s, dump, `write_final`, flush, `end_title` | `src/host.rs:111-134` (warn at `:125`) |
| `log_sync_state(at)` spawns a thread and returns at once | `src/hle/d3d12x/list_x.rs:1252-1258` |
| `presents()` is the frame count the stall watcher compares | `src/hle/d3d12x/present.rs:1256-1258` |
| `end_title` suspends every other thread, then calls `TerminateProcess` | `src/hle/d3d12x/real.rs:3587-3612` (`:3602`, `:3611`) |
| Stub first call: `[STUB] … first called from` is logged inside `if n == 0` | `src/stubs.rs:44-60` (log at `:48-54`) |
| DirectStorage failure: `FAILED` counts every failure, but the log line appears only while the queue has no pending error record (reset when the title calls `RetrieveErrorRecord`) | `src/hle/dstorage.rs:1079-1088`, `:1521-1545`; counter `:850`, `:853-855` |
| The crash handler writes the report and flushes the log | `src/eh.rs:111-113` |
| Precedent for a second test module in a file | `src/gpu/recomp/dxc.rs:205` (`mod version_tests`) |
| `XCL_*` env reads: a scan found 171 distinct `XCL_` names in `var(`/`var_os(` calls (`grep -rho 'var\(_os\)\?("XCL_[A-Z0-9_]*' src \| sort -u \| wc -l`) | crate-wide |

Plan citations that moved since `a67c592`: plan:57 `report.rs:170` is now `write_final` at
`src/report.rs:171`. Plan:61/:262 `host.rs:106` (`XCL_RUN_SECONDS`) is now `src/host.rs:137`. Plan:56
`log.rs:52`/`:72` and plan:58 `host.rs:75` still match.

---

### 3. Core writer: options

#### O1: global `OnceLock<Events>` (Mutex<File>, unbuffered), one `write_all` per line (recommended)
- `static TIER: AtomicU8` gates everything. `static EVENTS: OnceLock<Events>` holds
  `Mutex<File>` plus counters.
- The caller checks `telemetry_on(tier)` first: one relaxed load. When telemetry is off, no fields are
  built and nothing is allocated.
- The body (`"ev":…,fields`) is formatted outside the lock. `t` is taken under the lock, so the order
  of lines in the file matches `t`. The full line goes out in one `write_all` on an unbuffered `File`,
  which is one `WriteFile` per line. Data is in the OS file cache as soon as the call returns, so it
  survives `TerminateProcess` (`real.rs:3611`), a crash and a GPU-reset kill. That meets D1 (plan:85)
  with no flush hook.
- Lock waits are bounded (`try_lock` for at most 100 ms, then the event is dropped and `dropped` is
  counted). `end_title` suspends other threads (`real.rs:3602`), and a suspended lock holder must
  never hang the ending thread. The same reasoning is behind `report.rs:157-168` (`lock_within`).
- It matches the text log's own pattern (`log.rs:42`, `:79-86`).
- Cost: one syscall per event, a few µs. That is fine for refusals, stalls and stubs (all
  volume-limited) and for part 2's `frame` events (≤ fps). Part 3 (`pass`) may want buffering;
  measure it under D7.

#### O2: channel to a writer thread with a `BufWriter`
- Cheapest on the hot path (a send).
- **Rejected.** Queued events are lost on `TerminateProcess` (`real.rs:3611`). The writer thread is
  itself suspended by `end_title` (`real.rs:3602`) and frozen by `dump_threads`'s suspend/resume
  (`eh.rs:852-855`). The events most likely to be lost are those of the end of a stall or a crash,
  which are the ones that matter, so it breaks D1. It also adds a long-lived thread inside the title's
  process.

#### O3: reuse the text log's writer and lock (`log.rs:42`, `:72-90`)
- Either events go into `xcl.log` with a marker, or one mutex guards both files.
- **Rejected.** The text log is level-gated and not per run (`File::create` at `log.rs:60` truncates
  it). Mixing formats breaks "a flat JSONL file" (plan 5.3). One mutex would make every `xlog_*`
  contend with events.
- What O1 does reuse from `log.rs`: `START` for `t`, the module's lock style, and the `xlog_warn!`
  macro for its own failures.

(Variant O1b, a `BufWriter` with `flush()` after each line: the same syscalls as O1, more code. A
`BufWriter` flushed once per frame breaks D1. Neither is used.)

**Recommendation: O1.**

---

### 4. Architecture and data flow

```
host_main ─ log::init (host.rs:19) ─ XCL_UMA_MB (host.rs:36) ─ state::set_boot (host.rs:38)
   └─ report::telemetry_start(&boot)                         [new call line, after host.rs:38]
        ├─ log::parse_tier(env XCL_TELEMETRY)  → Off? return (nothing created)
        ├─ root = XCL_TELEMETRY_DIR or boot.logs_dir()
        ├─ commit = git_commit(..)            (runtime read of .git, no build.rs; D-1)
        ├─ dir = log::create_run_dir(root, utc_compact, commit)  → run-20261125T180411Z-918d42c1f0aa
        ├─ manifest.json  (MANIFEST: Mutex<Option<Manifest>>, written tmp+rename)
        └─ log::telemetry_open(dir, tier)     → events.jsonl, TIER stored last
real::init (device)       → report::telemetry_device(&name)   adapter/driver/vram/waves → manifest rewrite
note_translation_error    → report::telemetry_shader_refused(&e)
note_fallback             → report::telemetry_pipeline_refused(what, hr, ok)
stall watcher             → list_x::log_sync_state("the stall") + report::telemetry_stall(..)
stubs::record_call        → report::telemetry_stub(info, n + 1, caller)
dstorage QueueInner::fail → report::telemetry_io_failed(hr, bytes, path)
report::write_final       → telemetry_final(&s)   report.json copy + dxc into manifest
                    every telemetry_* : if !log::telemetry_on(T) { return } … log::event(T, ev, &fields)
```

Split of the code (D6). **log.rs** gets the generic machinery: tier, run directory, writer, time stamps
and the volume limiter. **report.rs** gets what knows XCL's domain: the manifest and the event
constructors called from call sites. Both are appended as one block at the very end of each file,
after the existing `mod tests`, with their tests in a second module `mod telemetry_tests`
(precedent: `dxc.rs:205`). The fork's diff to these two files is then a pure append, which keeps
upstream merge conflicts to nothing (plan:312-315).

#### 4.1 Appended to `src/log.rs` (after line 136)

```rust
// ---- fork telemetry (docs/guide/telemetry.md, part 1) ----
use std::sync::atomic::AtomicU64;            // (AtomicU8, Mutex, OnceLock, File, Instant already imported, log.rs:4-9)
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum Tier { Off = 0, Frame = 1, Pass = 2, All = 3 }
impl Tier { pub fn name(self) -> &'static str }                 // "off" | "frame" | "pass" | "all"

/// `XCL_TELEMETRY`: None / "" / "off" → Off; "frame" | "pass" | "all" (trimmed, any case);
/// anything else → Err(message) – the caller warns and stays off.
pub fn parse_tier(value: Option<&str>) -> Result<Tier, String>

static TIER: AtomicU8 = AtomicU8::new(0);
static EVENTS: OnceLock<Events> = OnceLock::new();
static RUN_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Whether `current` writes events of `wanted` (pure; `telemetry_on` with TIER).
fn tier_allows(current: u8, wanted: Tier) -> bool                // current != 0 && wanted as u8 <= current
/// One relaxed load while off.
#[inline] pub fn telemetry_on(wanted: Tier) -> bool
pub fn run_dir() -> Option<&'static std::path::Path>

/// `<root>/run-<utc>-<commit12|unknown>`, made with create_dir (not _all for the leaf); an existing
/// one gets `-2`, `-3` … (two runs in one second). Creates `root` if missing.
pub fn create_run_dir(root: &std::path::Path, utc: &str, commit: Option<&str>) -> std::io::Result<PathBuf>

/// Opens `<dir>/events.jsonl` and turns telemetry on at `tier` (TIER stored after EVENTS is set).
/// Off: opens nothing, returns false. A second call: false, nothing changed.
pub fn telemetry_open(dir: &std::path::Path, tier: Tier) -> bool

pub struct Events { file: Mutex<File>, written: AtomicU64, dropped: AtomicU64 }
impl Events {
    pub fn create(path: &std::path::Path) -> std::io::Result<Events>
    /// `{"t":<ms>,` + body + `}\n` in one write_all; `t` (ms since `START`, 3 decimals) taken under the
    /// lock; lock waited ≤ 100 ms, else the event is dropped and counted.
    pub fn write(&self, ev: &str, fields: &[(&str, serde_json::Value)])
}
/// `"ev":"<ev>","k":v,…` – keys are static identifiers, values serialised by serde_json (escapes
/// quotes, backslashes, control characters: one physical line guaranteed).
fn event_body(ev: &str, fields: &[(&str, serde_json::Value)]) -> String

/// Writes the event if `tier` is on (no-op otherwise).
pub fn event(tier: Tier, ev: &str, fields: &[(&str, serde_json::Value)])

/// Volume limit per event kind: increments `counter`; Some(n) (1-based) for n ≤ 100 and every
/// 1000th after, None otherwise. Every emitted event carries `n`, so totals stay readable.
pub fn sampled(counter: &AtomicU64) -> Option<u64>

/// (`20261125T180411Z`, `2026-11-25T18:04:11Z`) for `now` in UTC – no chrono (days-to-civil).
pub fn utc_stamps(now: std::time::SystemTime) -> (String, String)
fn civil_from_days(days: i64) -> (i64, u32, u32)
```

#### 4.2 Appended to `src/report.rs` (after line 215)

```rust
// ---- fork telemetry (docs/guide/telemetry.md, part 1) ----
use crate::log::{self as xlog, Tier};

#[derive(Serialize, Default, Clone)]
struct Manifest { /* fields of §5.1, in that order */ }
#[derive(Serialize, Default, Clone)]
struct ManifestDxc { dxcompiler: Option<String>, dxil: Option<String> }
static MANIFEST: Mutex<Option<Manifest>> = Mutex::new(None);

/// `XCL_TELEMETRY` on: run directory, manifest.json, events.jsonl. Off: one env read, nothing made.
/// Failures: one `xlog_warn!`, telemetry stays off.
pub fn telemetry_start(boot: &crate::config::Boot)
/// The D3D12 device was created: adapter name, driver (UMD) version, VRAM, wave lane range.
pub fn telemetry_device(adapter: &str)
/// From write_final: `report.json` into the run directory, DXC versions into the manifest.
fn telemetry_final(report_json: &str)

pub fn telemetry_shader_refused(reason: &str)                       // tier Frame
pub fn telemetry_pipeline_refused(what: &str, hr: i32, standin_built: bool)   // tier Frame
pub fn telemetry_stall(last_frame: u64, idle_s: f64, limit_s: f64)   // tier Frame
pub fn telemetry_stub(info: &crate::stubs::StubInfo, calls: u64, caller: usize) // tier All
pub fn telemetry_io_failed(hr: i32, bytes: u64, path: Option<&[u16]>) // tier Frame

// pure, unit-tested
fn hresult_text(hr: i32) -> String                                  // "0x8007000E"
fn refusal_parts(reason: &str) -> (Option<String>, Option<String>)  // (stage, hresult)
fn shader_refused_fields(reason: &str, n: u64) -> Vec<(&'static str, serde_json::Value)>
fn pipeline_fields(what: &str, hr: i32, standin_built: bool, n: u64) -> Vec<(&'static str, serde_json::Value)>
fn stall_fields(last_frame: u64, idle_s: f64, limit_s: f64) -> Vec<(&'static str, serde_json::Value)>
fn stub_threshold(calls: u64) -> bool                               // 1, 10, 100, 1000 …
fn io_failed_fields(hr: i32, bytes: u64, path: Option<&[u16]>, n: u64) -> Vec<(&'static str, serde_json::Value)>
fn xcl_switches(vars: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>) -> BTreeMap<String, String>
fn git_commit(starts: &[&std::path::Path]) -> Option<String>       // 40-hex HEAD of the first checkout found
fn cpu_brand() -> Option<String>                                    // CPUID 0x80000002..4, std::arch
fn write_manifest(dir: &std::path::Path, m: &Manifest)             // pretty JSON → manifest.json.tmp → rename
```

Each public `telemetry_*` has the same shape:
`if !xlog::telemetry_on(T) { return; }` → (`sampled(&COUNTER)` where it applies) →
`xlog::event(T, EV, &x_fields(..))`. All formatting, `describe_address`, UTF-16 decoding and parsing
happen after the gate.

#### 4.3 Call sites (one line each, D6)

| # | Where (insert after) | Line to add |
|---|---|---|
| C1 | `src/host.rs:38` (`crate::state::set_boot(boot.clone());`) | `crate::report::telemetry_start(&boot);` |
| C2 | `src/host.rs:125` (the `XCL_STALL_DUMP` warn) | `crate::hle::d3d12x::list_x::log_sync_state("the stall");` |
| C3 | `src/host.rs:125` (after C2) | `crate::report::telemetry_stall(last, since.elapsed().as_secs_f64(), secs);` |
| C4 | `src/hle/d3d12x/real.rs:553` (`*ADAPTER.lock().unwrap() = name.clone();`) | `crate::report::telemetry_device(&name);` |
| C5 | `src/hle/d3d12x/real.rs:3751` (`let n = if ok { FALLBACK_PSO… }`) | `crate::report::telemetry_pipeline_refused(what, hr, ok);` |
| C6 | `src/hle/d3d12x/real.rs:3860` (closing `}` of the `in_cached_only` block) | `crate::report::telemetry_shader_refused(&e);` |
| C7 | `src/stubs.rs:55` (closing `}` of `if n == 0`) | `crate::report::telemetry_stub(info, n + 1, caller);` |
| C8 | `src/hle/dstorage.rs:1080` (`FAILED.fetch_add(1, …);`) | `crate::report::telemetry_io_failed(hr, request.destination_size as u64, file.map(\|f\| f.path.as_slice()));` |
| C9 | `src/report.rs:188` (`let _ = std::fs::write(boot.report_path(), s);`) | `telemetry_final(&s);` |

Notes on the call sites:
- C1 runs before the first spawned thread (`host.rs:39`) and after `XCL_UMA_MB` is set (`host.rs:36`),
  so the switch snapshot includes it.
- C2 and C3 come before `dump_threads` (`host.rs:126`). The sync-state output is written by its own
  thread (`list_x.rs:1254`), so its lines may interleave with the first dump. The watcher sleeps 1 s
  and writes the report before `end_title` (`host.rs:127-131`), which gives that thread time. C3 is
  written before anything that could hang.
- C6 goes where `e` is still owned, before the `TRANSLATE_ERRORS` dedup (`real.rs:3863-3867`), so
  every refusal counts, not only the first 40 distinct ones. Refusals that the heap-window retry
  rebuilds (`:3854-3856`) and cached-only notes (`:3857-3860`) are already filtered out. The notes are
  replayed through `:3847` when they count.
- C7 runs on every stub call. While off it costs one relaxed load. The thresholds keep the events to
  about 7 per stub.
- C8 borrows `request` before it is moved into `FirstError` (`dstorage.rs:1085`).
- C9 is inside `report.rs`'s own `write_final`. It covers every end path that writes the report:
  the loader error, the normal return, `XCL_RUN_SECONDS`, the stall, and crash
  (`host.rs:129`, `:142`, `:178`, `:183`, `:186`, `eh.rs:112`, `real.rs:7450`, `loader.rs:415`,
  `present.rs:368`).

Upstream files touched: `host.rs` (3 lines), `real.rs` (3), `stubs.rs` (1) and `dstorage.rs` (1). Each
is a single added line. No upstream line is changed or removed.

---

### 5. Schemas

General rules (plan 5.3): flat objects, `t` first (ms since host start, a number with 3 decimals),
then `ev`. HRESULTs are strings `"0x%08X"` (plan:142 example `0x8007000E`). A field that is not
known at a site is **omitted**; readers treat a missing field as null (ASSUMPTION A3). Fields may be
added later, and none changes meaning.

#### 5.1 `manifest.json` (written at start; rewritten at device creation and at each `write_final`)

| field | type | source in part 1 |
|---|---|---|
| `schema` | int = 1 | constant |
| `run_id` | string `20261125T180411Z-918d42c1f0aa` | the run directory's name minus `run-` (no `:`, which is illegal in Windows file names; A2) |
| `xcl_version` | string | `env!("CARGO_PKG_VERSION")`, as `report.rs:132` |
| `commit` | string (40 hex) or null | `git_commit` (D-1) |
| `commit_source` | `"git-head"` or null | how `commit` was found: the checkout's HEAD at run time, not proof of what the binary was built from |
| `dirty` | null in part 1 | not knowable without running git (D-1) |
| `rust` | null | no runtime source without build.rs (D-2) |
| `profile` | `"debug"` or `"release"` | `cfg!(debug_assertions)` |
| `os` | string `"Windows 10.0.26100"` | `kernelx::host_os_version()` (`kernelx.rs:1707`) |
| `cpu` | string or null | CPUID brand string (`std::arch::x86_64::__cpuid`) |
| `cores` | null in part 1 | physical cores need `GetLogicalProcessorInformationEx`; deferred |
| `threads` | int | `std::thread::available_parallelism` (pattern `shaders.rs:748`) |
| `ram_mb` | int | `GlobalMemoryStatusEx.ullTotalPhys >> 20` (pattern `uma.rs:816`) |
| `adapter` | string or null | `telemetry_device(&name)`, the same string as `report.json` `d3d12.adapter` (`real.rs:553`) |
| `driver` | string `"32.0.15.6614"` or null | `IDXGIAdapter::CheckInterfaceSupport(IDXGIDevice)` UMD version via the LUID path (`real.rs:427-440` pattern) |
| `vram_mb` | int or null | `IDXGIAdapter1::GetDesc1().DedicatedVideoMemory >> 20` |
| `wave_lane_count_min` / `_max` | int or null | `CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS1)`, a copy of the private `pso.rs:1197-1208` |
| `dxc` | `{"dxcompiler": str\|null, "dxil": str\|null}` | `dxc::toolchain()` file versions (`dxc.rs:200`), at `telemetry_final` |
| `title` | string | `boot.title` |
| `title_version` | null in part 1 | not parsed today (`manifest.rs:8-19` has no Identity Version) |
| `exe` | string | `boot.exe_path`, lossy |
| `tier` | `"frame"`/`"pass"`/`"all"` | parsed tier |
| `switches` | object {name: value} | snapshot of every `XCL_*` variable set at C1 (see below) |
| `started` | string `2026-11-25T18:04:11Z` | `utc_stamps` |
| `events_dropped` | int | `Events.dropped` at the last rewrite (only grows; normally 0) |

**Switches: "actually read" vs. set.** The crate reads `XCL_*` directly through `std::env::var` /
`var_os` at about 171 distinct names spread across modules (§2). Recording only the names that were
actually read would need a wrapper at every read site, which goes against D6. Part 1 therefore
records **every `XCL_*` variable set in the process environment at C1**: `std::env::vars_os()`,
names matched case-insensitively (Windows), values decoded lossily. That is a superset of what was
read; a typo in a name shows up too, which helps. It misses variables XCL sets on itself later
(`present.rs:1572` sets `XCL_WINDOW_POS`). Config-file settings are not switches; see D-7.

#### 5.2 `shader_refused` (tier `frame`)
`{"t":812.402,"ev":"shader_refused","stage":"cs","reason":"CS: <recompiler message>","hresult":"0x8007000E","n":3}`

| field | type | notes |
|---|---|---|
| `stage` | string, optional | lowercased prefix before `": "` when it is one of `VS`, `PS`, `CS`, `MS`, `VS+GS` (`real.rs:4332`, `:5213`, `:5226`, `:6116`, `:6213`); otherwise omitted |
| `reason` | string | the full message `note_translation_error` received |
| `hresult` | string, optional | from the last `(0x…)` group in `reason` (`real.rs:4386`, `:6002`, `:6051`, `:6332`), normalised to `0x%08X`; omitted when absent (recompiler and DXC refusals carry none) |
| `n` | int | 1-based count of refusals this run (`sampled`) |
| `record`, `pc` | omitted in part 1 | the chokepoint gets only a string. Threading the record hash through roughly 20 call sites goes against D6 (§8) |

#### 5.3 `pipeline` refusal (tier `frame`)
`{"t":905.110,"ev":"pipeline","refused":true,"stage":"compute","hresult":"0x80070057","source":"standin","n":1}`

| field | type | notes |
|---|---|---|
| `refused` | bool = true | separates these from part 3's creation events |
| `stage` | `"graphics"` / `"compute"` / `"stream"` | from `what` (`real.rs:5709`, `:5754`, `:6827`, `:6832`) |
| `hresult` | string | the first creation's HRESULT |
| `source` | `"standin"` (stand-in shaders built) or `"none"` (the stand-in failed too) | `ok` of `note_fallback` |
| `n` | int | count (`sampled`) |

#### 5.4 `stall` (tier `frame`)
`{"t":61250.003,"ev":"stall","last_frame":940,"idle_s":6.012,"limit_s":6.0,"ends_run":true}`
`last_frame` is `present::presents()` at the stall (`host.rs:113`, `present.rs:1256`). `idle_s` is the
measured time since the last change, `limit_s` the `XCL_STALL_DUMP` value. Written once; the run
ends right after (`host.rs:131`).

#### 5.5 `stub` (tier `all`, per plan 5.10)
`{"t":1302.551,"ev":"stub","dll":"api-ms-win-…","function":"Foo","calls":1,"caller":"title.exe+0x1a2b3c"}`
Written when `calls` is 1, 10, 100, …. `caller` appears only on the first event, from
`modules::describe_address` (as `stubs.rs:51`).

#### 5.6 `error` / `io_failed` (tier `frame`)
`{"t":4410.020,"ev":"error","kind":"io_failed","source":"dstorage","detail":"request failed","hresult":"0x80070026","fatal":false,"path":"…","bytes":65536,"n":12}`
`path` is `String::from_utf16_lossy` of the request's file. It is omitted for memory-source requests
(`file` is None). Volume: `sampled` (first 100, then every 1000th, each with `n`), independent of the
log line's per-queue gating (`dstorage.rs:1083`). The log tells only "the first error until the title
reads the record", while `n` and the activity counter (`dstorage.rs:850`) give totals.

---

### 6. Constraints

- **No new dependency.** It uses `serde`/`serde_json` (`Cargo.toml:19-20`), std, and `windows` /
  `windows-sys` features already enabled (`Win32_Graphics_Dxgi`, `Win32_Graphics_Direct3D12`,
  `Win32_System_SystemInformation`: `Cargo.toml:26-31`, `:61`).
- **No build-time code.** `build.rs` is not touched. The commit is found at run time (D-1).
  `env!("CARGO_MANIFEST_DIR")` is a constant cargo supplies to every crate, not build-script output.
  It is used only as a starting point when looking for `.git`.
- **No new module or file** (D6). Code is appended to `log.rs` and `report.rs`, and every other file
  gets one-line calls (§4.3).
- **Off cost.** `telemetry_start` does one `env::var` at startup. Each site does one relaxed
  `AtomicU8` load and a compare, with no allocation. No directory or file is created.
- **Thread safety.** `TIER` is stored only after `EVENTS` is set (`OnceLock` publishes it), so a
  thread that sees the tier also sees the writer. Writers serialise on one mutex, held only for a
  `write_all`. A poisoned mutex is recovered with `into_inner`. `MANIFEST` rewrites are serialised by
  their own mutex.
- **Exit and flush.** Each line is in the OS cache when `write_all` returns, so no flush is needed on
  `end_title`, crash or `XCL_RUN_SECONDS`. `manifest.json` and the report copy go through
  write-to-tmp then `std::fs::rename`, which replaces the file on Windows. A reader never sees half a
  manifest.
- **Title crash path.** Part 1 writes nothing from the crash handler (`eh.rs:111`). The bounded lock
  wait (§3 O1) is designed now so that part 5's `title_exception` cannot hang on a lock held by a
  suspended or crashed thread.
- **Fork only (D8).** All of this stays on `dev`.

### 7. Considerations: edge cases and failure modes

| Case | Behaviour |
|---|---|
| Root or run dir not writable, disk full at start | one `xlog_warn!("telemetry: … – telemetry off")`, `TIER` stays 0, the run continues |
| Disk full mid-run | `write_all` errors are ignored and counted (`dropped`). One warning on the first failure |
| Two runs started in the same second (two titles, a retry) | `create_dir` on the leaf; `AlreadyExists` → `-2`, `-3` … (up to 100, then a warning and off) |
| Invalid `XCL_TELEMETRY` (`on`, `1`, `verbose`) | warning naming the value and the four valid ones; off (A4, D-6) |
| `XCL_TELEMETRY_DIR` relative | resolved against the process's current directory (the host's, launcher.rs spawns it); the manifest does not record it, the switch snapshot does |
| `XCL_TELEMETRY` inherited by child processes | only `host_main` (`main.rs:60`) calls C1, so test workers re-spawned with `current_exe` (e.g. `pso.rs:1641`) and CLI subcommands create nothing |
| Concurrent writers | one mutex; `t` taken inside it, so the file is in time order |
| Huge volume | refusals and io failures go through `sampled` (≤100 + 1/1000); stubs log10 thresholds; stall once; total part-1 volume is bounded by distinct stubs × ~7 plus small constants |
| Same console shader refused at each PSO creation | each creation is one event (up to `sampled`'s limits); the log keeps its dedup (`real.rs:3863`) |
| Console compute refusal | gives a `shader_refused` (real reason) **and** a `pipeline` refusal whose `hresult` is XCL's own `E_INVALIDARG` (`real.rs:5746`), not the driver's. Documented in the schema; a reader joins on `t` |
| Non-UTF-8 / non-UTF-16 paths | `from_utf16_lossy` / `to_string_lossy` (U+FFFD); serde_json escapes `\`, `"` and control characters, so a line stays one line |
| Title paths and names | in `exe`, `title`, `switches`, `path`, `reason`. Run directories stay on the machine (plan:88). Default root is `data_dir/logs`, outside the repo. A user-set `XCL_TELEMETRY_DIR` inside the repo is the user's choice. The repo `.gitignore` has no `run-*` entry (adding one is fork tooling, not part 1) |
| Device recreated or `init` called twice | `real::init` returns early once the device exists (`real.rs:472-474`), so C4 runs once |
| Stale binary vs. checkout | `commit` is the checkout's HEAD at run time; if the exe is older it is wrong. Marked by `commit_source`; D-1 |
| `.git` is a file (worktree) | follow `gitdir:`; refs from `commondir`; `packed-refs` fallback; detached HEAD read directly |
| Stall sync-state thread blocks on a lock a stuck thread holds | it is its own thread (`list_x.rs:1254`); the watcher does not wait for it; `end_title` ends the process regardless (`real.rs:3594-3595`) |
| Unit tests share globals | the global wiring (`telemetry_open` with a tier other than Off, `MANIFEST`) is not exercised by unit tests. Every piece is a pure function or an `Events` instance on a temp path |

---

### 8. Alternatives rejected (besides O2 and O3)

- **Per-site HRESULT for shader refusals** (an extra line at `real.rs:4386`, `:6002`, `:6051`,
  `:6332` passing `hr`). It would emit twice with C6, or C6 would have to go. Parsing `(0x…)` at the
  single chokepoint keeps one line, and a test pins the format. Cost: an upstream rewording of those
  messages silently drops `hresult`; the parser test then still passes. Flagged in UNCERTAIN.
- **`record` (shader hash) on `shader_refused`.** Every caller of `note_translation_error` would have
  to pass it (about 20 sites), which is not one line each. Deferred to part 3, where pipeline
  creation is instrumented anyway.
- **Writing the full manifest only at the end.** That breaks plan 5.2 ("exists even if the title dies
  at once"). The design rewrites at three points instead.
- **Probing the adapter at startup** (creating a throwaway device, as `preflight.rs:147` does). It
  costs a device creation before the title runs and may pick a different adapter. The real device's
  values are filled at C4 instead.
- **Spawning `git` from the host**, and **build.rs**: see D-1.

---

### 9. Work breakdown

| Unit | Files | Inputs | Outputs | Acceptance | Code / tests (est. lines) |
|---|---|---|---|---|---|
| **U1** writer core | `src/log.rs` (append only) | §3 O1, §4.1 | tier, run dir, `Events`, `event`, `sampled`, `utc_stamps` | the diff to `log.rs` is a pure append after line 136; tests below pass; nothing above line 136 changed | 130 / 85 |
| **U2** manifest | `src/report.rs` (append only) | U1's §4.1 signatures, §5.1 | `Manifest`, `telemetry_start`, `telemetry_device`, `telemetry_final`, `git_commit`, `xcl_switches`, `cpu_brand`, `write_manifest` | a pure append after line 215; manifest has every §5.1 key; tests pass | 150 / 55 |
| **U3** event constructors | `src/report.rs` (append, after U2's block) | U1, §5.2–5.6 | the five `telemetry_*` event fns and their `*_fields` helpers | the gate comes first in each fn; tests pass | 70 / 60 |
| **U4** call sites | `src/host.rs`, `src/hle/d3d12x/real.rs`, `src/stubs.rs`, `src/hle/dstorage.rs`, `src/report.rs:188` | §4.3 | C1–C9 | `git diff --stat` shows +3 / +3 / +1 / +1 and +1 in `write_final`, 0 deletions; builds; whole suite green on Windows | 9 / 0 |
| **U5** verification (no code) | none | U1–U4 built on the Windows PC | real test counts; one `tests/testtitle` run with `XCL_TELEMETRY=frame` (run dir, manifest and valid JSONL) and one with it unset (no run dir) | real output reported per AGENTS.md; skips listed | 0 |

**Tests** (each in `mod telemetry_tests` of the file it covers; plan test row 1 plus B1–B3):

U1 (`log.rs`)
1. `tier_parsing_takes_four_names_and_refuses_the_rest`: None, `""`, `"off"`, `" OFF "` → Off;
   `"frame"`, `"Pass"`, `"all"` → tiers; `"on"`, `"1"`, `"verbose"`, `"frames"` → Err.
2. `tiers_are_cumulative`: `tier_allows(0, Frame)` false; `(1, Frame)` true; `(1, Pass)` false;
   `(3, Pass)` true.
3. `an_event_is_one_flat_json_line`: `event_body` plus a `t` prefix parses with serde_json; the line
   starts with `{"t":`; `ev` is second; values with `"`, `\`, `\n` and U+FFFD do not break the line;
   no nested objects.
4. `the_writer_flushes_each_line` (plan row 1 "read back mid-write"): `Events::create(temp)`, write 3,
   read the file **while the writer is alive**: 3 lines, each valid JSON.
5. `concurrent_writers_keep_lines_whole_and_in_time_order`: 8 threads × 200 events → 1600 parseable
   lines; `t` never decreases.
6. `the_run_directory_is_made_once_per_run` (plan row 1): two `create_run_dir` calls with the same stamp
   give two directories, the second ending `-2`; names hold no `:`; a None commit gives `-unknown`; a
   commit is cut to 12 characters.
7. `off_opens_no_file` (plan row 1): `telemetry_open(empty_temp, Tier::Off)` returns false, and the
   directory is still empty.
8. `utc_stamps_match_known_dates`: epoch 0 → `19700101T000000Z`; 2028-02-29T12:00:00Z (leap day);
   2026-11-25T18:04:11Z.
9. `sampling_keeps_the_first_hundred_then_every_thousandth`.

U2 (`report.rs`)
10. `manifest_has_every_field_and_is_valid_json` (plan row 1): a `Manifest` built from a test `Boot` →
    serde round trip; every §5.1 key present, nulls included.
11. `switches_keep_only_xcl_variables`: `[("XCL_A","1"),("xcl_b","2"),("PATH","x"),("XCLX","3")]`
    → `{XCL_A, xcl_b}` (case-insensitive prefix, `XCL_` with the underscore).
12. `git_commit_reads_a_checkout`: a temp tree with `.git/HEAD` → `ref: refs/heads/m` plus a loose
    ref; then `packed-refs` only; then detached HEAD; then a worktree `.git` file with `gitdir:` and
    `commondir`; and a missing `.git` → None.
13. `the_manifest_is_replaced_whole`: `write_manifest` twice; the file parses and holds the second
    content; no `.tmp` is left.

U3 (`report.rs`)
14. `a_refusal_carries_its_hresult` (plan row 1): `hresult_text(0x8007000Eu32 as i32)` ==
    `"0x8007000E"`; `refusal_parts("the driver rejected the compute PSO with the translated CS (0x8007000e)")`
    → (None, Some("0x8007000E")); `refusal_parts("CS: unsupported …")` → (Some("cs"), None);
    `"VS+GS: …"` → `vs+gs`.
15. `a_pipeline_refusal_names_stage_hresult_and_source`: `pipeline_fields("compute PSO", E_INVALIDARG, true, 1)`.
16. `a_stall_event_has_frame_and_times` (B1): `stall_fields(940, 6.012, 6.0)`.
17. `stub_events_come_at_powers_of_ten` (B2): 1, 10, 100 and 1000 are true; 2, 11 and 999 are false.
18. `an_io_failure_is_an_error_with_path_bytes_hresult` (B3): `io_failed_fields` with a path holding
    `\`, `"` and a lone surrogate (0xD800) → valid JSON, `kind == "io_failed"`, `fatal == false`, U+FFFD
    in the path; with None the `path` field is omitted.

B1's wiring (C2/C3) and C4–C9 get no unit test: they are single calls in threads or driver paths. The
existing `host.rs` tests (`host.rs:207-222`) must still pass. An end-to-end stall test needs a title
that stalls; it is not part of this plan (plan:334).

**Concurrency plan**
- Wave 1, in parallel: **U1** (`log.rs`) ∥ **U2→U3** (`report.rs`, one worker, U3 after U2 because they
  share the file). They depend only on the frozen §4.1 signatures, not on each other's code, but they
  compile only together.
- Wave 2: **U4** (four upstream files plus one line in `report.rs`; it can be written during wave 1
  against §4.3, and is applied after U1–U3 land).
- Wave 3: **U5** on the Windows PC (build plus `cargo test --workspace -- --test-threads=4`, and
  `cargo test -p xcl --lib telemetry` first).

**Estimate.** Code ≈ 130 + 150 + 70 + 9 = **359**; tests ≈ 85 + 55 + 60 = **200**; total ≈ **560
changed lines**. That is above the plan's ~400 because of gap fill B and the tests. D8 places no size
limit on this work.

---

### 10. Open decisions for the user

- **D-1 Commit identity at run time** (affects `commit`, `dirty`, the run directory name).
  (a) *Recommended:* read `.git` at run time, starting from `env!("CARGO_MANIFEST_DIR")/../..`, then
  the ancestors of `current_exe()`. This gives HEAD (worktrees and `packed-refs` handled) with
  `dirty: null`. No process is spawned and nothing runs at build time. It can be wrong for a stale
  binary.
  (b) Spawn `git rev-parse HEAD` and `git status --porcelain --untracked-files=no -- crates Cargo.toml Cargo.lock`
  (as `smoke.ps1:34-35`) from the host when telemetry is on. This gives `dirty`, but it runs an
  external program found on PATH from inside the title's process at startup.
  (c) A caller-supplied `XCL_TELEMETRY_COMMIT` / `XCL_TELEMETRY_DIRTY`, which adds switches the plan
  does not have.
  (d) build.rs `rustc-env`: excluded by the brief (build-time code).
- **D-2 `rust` field:** null (no build-time code), or drop the field.
- **D-3 Stub tier:** keep `all` (plan 5.10, the default here), or move it to `frame`; volume is bounded
  by the thresholds either way.
- **D-4 `xcl.log` in the run directory** (plan 5.1 lists it). It stays at `logs/xcl.log` in part 1:
  moving it changes `launcher.rs:511`, `:535` and `smoke.ps1`. Copying it at the end costs its size
  and covers only clean ends. Options: leave it, copy it at `telemetry_final`, or record its path in
  the manifest.
- **D-5 Volume limits:** 100 + every 1000th (proposed) for refusals and I/O failures, or another
  figure.
- **D-6 Invalid `XCL_TELEMETRY`:** off plus a warning (proposed), or treat it as `frame`.
- **D-7 Manifest `config`:** add `boot.config` (serialisable, `config.rs:280-289`) as a nested
  object, so config-file settings are reproducible too. This is not in plan 5.2. Proposed: not in
  part 1.

### 11. Assumptions made here

- A1. `t` is a float in ms with 3 decimals, from the text log's `START` (`log.rs:45`).
- A2. `run_id` and the directory use compact ISO-8601 (`20261125T180411Z`), because the plan's
  example `2026-11-25T18:04:11Z` contains `:`, which Windows rejects in file names. `started` keeps
  the extended form.
- A3. Fields unknown at a site are omitted, not written as null.
- A4. An invalid tier means off.
- A5. HRESULT is the string `0x%08X`.
- A6. `report.json` is copied into the run directory at each `write_final`. The "summary block" (plan
  5.1) is not part of part 1.
- A7. Test modules are named `telemetry_tests`, appended after the existing `mod tests`.

## 2026-11-26 Telemetry part 4: shaders.json, the static translation shape (orchestrator approved the design under the user's instruction to implement the whole plan)

Designed by an Opus worker against worktree xcl-adopt at 21890e09 (dev + 15 adopted upstream PRs) with part 1 applied; citations spot-checked by the orchestrator (pso.rs:155-172 console_to_dxil_in, :1015 console_translation, :1167 console_mesh, :1249 console_geometry, :1231 wave_mode_line; emit.rs:781 vreg, :966 write_mask, :1125 try_emit_x64_reduction_tail, :5949 xcl_scratch; dxil/cfg.rs:34/:42/:48; dxil/stats.rs:15/:87). Paths below are relative to crates/xcl/src/ unless they say otherwise.

### Orchestrator decisions on the open points of part 4 (section 11)

- O1 placement: (a) Blocks S and F appended to report.rs with inline test modules.
- O2 direct-backend exchanges: null in this version.
- O3 rebuild cost: (a) one background worker at tier pass.
- O4 comparability: the split form is always emitted for counting, with `paid` saying whether the device split it.
- O5 plan deviations accepted: `form` is straight | structured | switch_pc | direct; `register_arrays` lists the declared private arrays (empty in the HLSL switch_pc form unless scratch_* is used). To be reported to the user; docs/guide/telemetry.md 5.6 is updated in the closing docs unit, docs/specs/2026-11-24-direct-dxil.md:49 is reported, not edited.
- O6 call sites: the 7 pso.rs line edits of section 6 as written.
- O7 files: shaders.jsonl (incremental) and shaders.json (final).
- O8: counts are static sites. O9: `split_reason` copies the text; no upstream edit.
- Units: U1 then U2 by one writer (both in report.rs); U3 (pso.rs) in parallel; U4 (plan text) is deferred to one closing docs unit for all parts.
- Check: the orchestrator builds on the Windows PC and runs the whole suite in the desktop session; baseline with 15 adopted PRs + part 1: 1494 passed, 4 failed (gpu::dxil::fuzz_tests::fuzz_validate_and_gpu_0..3, NVIDIA 0x8007000E), 47 ignored, 44 SKIPPED lines.

Status: draft for the orchestrator, written 2026-11-26. Nothing is built.
Tree: worktree `xcl-adopt`, branch `adopt/diagnostics-2`, HEAD `21890e09`, with part 1 in the index.
Every `path:line` below is relative to `crates/xcl/src/` unless it says otherwise, and was read in that tree.
The crate builds on Windows only. Nothing here was compiled or run.

### 1. Goal and scope

Plan reference: `docs/guide/telemetry.md` section 5.6, tier table 5.10, Work order row 4, Test plan row 4, D1–D8.

**In scope**
- One record per distinct translated console program in the run, whether it was translated in this run or
  came from the memory or disk cache. Covers VS, PS, CS (FSSM and FSPX containers), the mesh compute pass
  and the geometry (ES+GS) compute pass, on the HLSL backend and the direct DXIL backend.
- Fields from 5.6. Where 5.6 does not match the tree, the field is redefined precisely and the change is
  listed in section 11.
- Written at tier `pass` and above. Below `pass`, each call site costs one relaxed atomic load.
- Files: `shaders.jsonl`, written line by line and flushed like `events.jsonl`, and `shaders.json`, the
  merged document written by `write_final`.

**Out of scope**
- Dynamic counts: how many times an exchange or loop ran. That needs GPU counters, which belong to part 5.
- Driver scratch or register spills. Each driver's compiler decides these and no API reports them.
  `scratch_hint` covers only what the form declares.
- Exchange counts for programs the direct DXIL backend produced. They are `null` in v1 (open decision O2).
- The offline `xcl shaders` CLI. It never opens telemetry, so it gets nothing.

### 2. What the tree offers (verified)

| Need | Where | Note |
|---|---|---|
| One entry point for VS/PS/CS, both backends, hits and misses | `gpu/recomp/pso.rs:155` `console_to_dxil_in` | returns at `:158` (cache disallowed) and `:171` (`cached_shader`) |
| Cache hits return no `Translation` | `pso.rs:871` `cached_shader` (memory) and `pso.rs:170` `translated::cached` (disk) | only `Compiled { dispatch_chain, dxil, samplers }` (`pso.rs:125`) comes back |
| Mesh entry | `pso.rs:1167` `console_mesh` → `console_mesh_with` `pso.rs:1127` (cached, `:1150-1157`) | one caller: `hle/d3d12x/real.rs:6218` |
| Geometry entry | `pso.rs:1249` `console_geometry` (never cached) | returns at `:1261` (direct) and `:1267` (HLSL) |
| HLSL emission without DXC, for VS/PS/CS | `pso.rs:1015` `console_translation` (pub(crate)) returns `Translation` with `hlsl`, `instructions`, `blocks` (`gpu/recomp/mod.rs:540`) | `pso.rs:939` `console_hlsl` wraps it |
| Emission for mesh / geometry | `mod.rs:1387` `translate_mesh`, `mod.rs:1380` `translate_geometry`, `mod.rs:285` `geometry_program` | pub |
| Container → program | `gpu/xbox.rs:193` `parse`, `mod.rs:464` `interface`, `mod.rs:1395` `fssm_wave64`, `mod.rs:1400` `detect_wave64`, `pso.rs:50` `fspx` | pub |
| Mask liveness (exchange or not) | `gpu/recomp/x64_masks.rs:742` `by_pc` | used by both backends: HLSL `mod.rs:1568`, direct `gpu/dxil/kernel.rs:599` |
| Console CFG with loop headers | `gpu/dxil/cfg.rs:42` `Cfg::build`, `:48` `build_geometry`; `headers` = back-edge targets (`cfg.rs:33`) | pub; the direct path caps a loop at each header (`kernel.rs:360-370`) |
| DXIL structure stats (#1954) | `gpu/dxil/stats.rs:87` `container_stats` → `DxilStats` (`stats.rs:15`) | `allocas`, `blocks`, `backward_branches`, `bitcode_bytes`; not on upstream `main` |
| Device wave range, split decision (#1410) | `pso.rs:1177` `wave32_only`, `:1192` `graphics_wave32`, `:1183` `split_wave64_forced`; text in `:1231` `wave_mode_line` (private) | the manifest already stores the range: `report.rs` `telemetry_device` (part 1, ~`:416`) |
| Part 1 writer | `log.rs:364` `pub struct Events` (`create`, `write`: one `write_all` per line, unbuffered file); `log.rs:316` `telemetry_on`; `log.rs:321` `run_dir` | reused as is; `log.rs` gets no part 4 code |
| End of run | `report.rs:171` `write_final` → `report.rs:188` `telemetry_final(&s)` → `report.rs:431` | part 1 code: one added line there |

**Exchange sites in the HLSL emitter.** Every one is a call to a prelude function, and each is preceded by a
`// <pc>:` comment (`gpu/recomp/emit.rs:1252` `inst`; `:1134-1228` for folded pairs):
- `xcl_x_ballot(`: a mask whose whole value is read (`emit.rs:966` `write_mask`; it falls back to
  `xcl_x_ballot_local` when `mask_whole_live` says lane-by-lane only, `:975-985`), plus one entry ballot
  per compute/mesh/geometry program (`emit.rs:5045`, `:5090`, `:5194`).
- `xcl_x_rla_uniform(`: `v_readlane`/`v_readfirstlane` (`emit.rs:743` `rla_uniform_fn`, used at `:4089`, `:4103`).
- `xcl_x_rla_any(`: `ds_bpermute_b32` (`emit.rs:4555`).
- a bare `xcl_x_meet()` in two forms: `if (xcl_x_meet() && xcl_half != 0u)` for a coherent (GLC/DLC) scalar
  load (`emit.rs:1985-1993`), and the folded `v_readlane` reduction tails
  (`emit.rs:1125` `try_emit_x64_reduction_tail`, meets at `:1141`, `:1165`, `:1216`), which merge 2 to 5
  console instructions into one exchange.
- Definitions live in `PRELUDE_X64` (`emit.rs:6108-6200`), before `void main(` (`emit.rs:4910`, `:5010`, `:5070`, `:5111`, `:5173`).
- `#define XCL_X_THREADS`, `XCL_X_WAVES`, `XCL_X_SPIN` are written by `emit.rs:4778` `x64_prelude` (`:4784`).
- The upstream code already reads generated HLSL as text: `emit.rs:5863` `linked_form`.

**Exchange sites in the direct backend.** These are `gpu/dxil/lanes.rs:221` `exchange_n` (2 group barriers, or the
spin form `:263`), called from `lanes.rs:438` `ballot`, `:539` `read_lane_uniform`, `:388` `read_lane_any`, and
`gpu/dxil/lower_mem.rs:1201` (coherent load). Mask locality is decided in `gpu/dxil/lower.rs:909` `write_mask`.
The direct backend differs from the HLSL one in three ways:
- it does not fold readlane pairs;
- it applies `by_pc` to geometry too (`kernel.rs:598`, against `mod.rs:1567`, which covers Kernel/Compute/Mesh only);
- it treats a mask as local when the condition folds to a DXIL constant 0 (`lower.rs:916`).

Counts from one backend are therefore not valid for the other.

**Registers in the HLSL backend.** Registers are named scalars (`emit.rs:781` `vreg` → `v{n}`, `:771` `sreg`).
The `pc` form is a chain of `if (pc == …)` blocks (`mod.rs:1599-1630`), not a `switch` over register arrays.
The only thread-private array the emitter declares is `static uint xcl_scratch[1024]` (`PRELUDE_SCRATCH`,
`emit.rs:5948-5949`), added when `scratch_*` is used (`emit.rs:4435`, `:4878`). `v_movrel*` and
`v_swaprel` become `switch` statements over named registers (`emit.rs:3334` `movrel`, `:3384` `swaprel`).
So the premise of `docs/specs/2026-11-24-direct-dxil.md:49` ("a uniform `switch (pc)` dispatcher over register
arrays") is stale. See section 11.

**Registers in the direct backend.**
- The per-instruction form keeps registers in SSA.
- The compact form (`gpu/dxil/compact.rs:1`) gives each register a scalar `alloca` (`lower.rs:93-111` `Init::slot`).
- Scratch is a `[1024 x i32]` alloca (`lower_mem.rs:2349-2361`).
- Geometry uses a 4-element alloca (`kernel.rs:932`).
- `DxilStats.allocas` counts all of these together, without sizes.

### 3. Options

#### Option A: count while emitting
Add counters to `Em` (`emit.rs`) and `Lw` (`dxil/lower.rs`), increment them at each exchange site, and return
them through `Translation`, `Kernel` and `Compiled`.
- Pro: exact on both backends.
- Con: about 15 edits interleaved in `emit.rs`, `lanes.rs` and `lower_mem.rs`, and in three structs. `Compiled`
  is serialised into the disk cache (`pso.rs` `translated::load_in`, `:294`).
- Con: it breaks D6 ("one-line call sites"), and cache hits still return no counts, so a rebuild would be needed anyway.

#### Option B: a separate static predictor over the decoded program
Decode the program, run `by_pc`, and classify each instruction with the emitter's rules.
- Pro: cheap, and the same code serves misses and hits.
- Con: it re-implements decisions spread over 13 `write_mask`/`ballot_fn` sites (`emit.rs:3663-4262`), the folding
  matcher (`emit.rs:1036` `x64_const_readlane`, `:1125-1240`, with its operator tables) and the direct
  backend's constant folding (`lower.rs:916`). It would drift silently, and its output would be a prediction,
  not a count.

#### Option C (recommended): count the translator's own output, rebuilt deterministically
On a background worker, rebuild the program's HLSL with the translator's own pub/pub(crate) functions
(`console_translation`, `translate_mesh`, `translate_geometry`; no DXC) and count the exchange call sites in the
text. Each site is attributed to the console pc of the preceding `// <pc>:` comment.
- The HLSL is a deterministic function of (program, options, environment), so rebuilding gives exactly what
  the miss produced. Misses and hits take the same path. Folding is included because the emitter did it.
- The other fields come from `Cfg` (console side) and `DxilStats` (on the DXIL actually returned, which is in hand
  on hits and misses).
- Con: CPU cost of one HLSL emission per distinct program. The doc comment at `pso.rs:174-180` puts it at
  0.1–0.3 s for large retail programs. It runs on one background thread, at tier `pass` only, and is bounded (section 9).
- Con: counting text depends on the emitter's tokens. Mitigations: tests pin it (section 10), and any `xcl_x_meet()` matching
  no known form is reported as `exchanges.other`, so a new form shows up instead of vanishing.
- Direct backend: the same rebuild would describe a program that did not run. Exchanges are `null` there (O2).

**Recommendation: C.** It is the only option that is exact on cache hits without touching the emitters. It adds
2 to 3 lines per entry function in `pso.rs` and nothing in `emit.rs` or `gpu/dxil/`.

### 4. Architecture and data flow

```
title thread (PSO creation)                          worker "xcl-shader-shape" (1 thread)
───────────────────────────                          ───────────────────────────────────
pso::console_to_dxil_in / console_mesh /
console_geometry
  start = telemetry_on(Pass).then(Instant::now)    ← off: 1 relaxed load, nothing else
  result = <existing body>
  report::telemetry_shader*(.., start, result)
    start == None → return result
    result Err → return result (refusals are part 1's)
    key = (fnv64, stage, split arg, windows)
    first time? (Mutex<HashSet>) else return
    write "seen" line to shaders.jsonl  ─────────┐
    try enqueue Job{blob(s), stage, split,       │
       windows, dxil copy, chain dwords,         │
       compile_ms}  (bounded: count + bytes)     │
       full → "seen" line carries                │
              shape:"dropped"                    │
  return result                                  │    loop: recv Job
                                                 │      backend = hlsl | direct | null (5.3)
                                                 │      core::shape_of(...)  [Block S]
                                                 │        container → program, Cfg shape,
                                                 │        HLSL rebuild (what ran),
                                                 │        + split-form rebuild for exchanges
                                                 │          when it did not run split
                                                 │      DxilStats on the dxil copy [Block F]
                                                 └──→   write "shape" line to shaders.jsonl
                                                        push record into RECORDS (bounded)
write_final → telemetry_final → shaders_final():
   stop intake, wait ≤ 2 s for the queue, merge seen+shape by key, write shaders.json (tmp + rename)
```

#### 4.1 Where the code lives: reconciling D6 with D8

Two blocks are appended to `crates/xcl/src/report.rs`, after part 1's block. Nothing goes in `log.rs`.

- **Block S, "split-wave shader shape" (shareable).** Pure functions and plain `Serialize` structs. They depend only on
  APIs present on upstream `main`: `gpu::recomp::{pso, translate_mesh, translate_geometry, geometry_program,
  interface, Mode, fssm_wave64, detect_wave64, x64_masks}`, `gpu::dxil::cfg`, `gpu::rdna2`, `gpu::xbox`, `serde`.
  They do not use `crate::log`, `Tier`, `run_dir`, `Events`, `DxilStats` (#1954 is not on `main`), env switches or
  any fork tooling. The block's tests follow it in `#[cfg(test)] mod shape_tests`. To share it upstream, cut
  the block and its tests verbatim into `gpu/recomp/shape.rs` on a branch from `main` and add one `mod` line.
  Nothing else changes.
- **Block F, "fork glue".** Covers the tier gate, dedupe, worker, JSONL lines, `DxilStats`, backend detection, the wave range
  from the manifest and `shaders.json` assembly. Tests: `#[cfg(test)] mod shape_glue_tests`.

This keeps D6 to the letter (appended code, no new file in the fork) and D8 in substance (a self-contained block
that can be moved). Merge cost is low: `report.rs` is already the fork's appended file, and `pso.rs` gets 7 line edits.
D6 says "no new module". If an inline `mod shape_tests` counts as one, that is O1.

### 5. Signatures (all in `report.rs`)

#### 5.1 Block S (shareable)

```rust
/// What the split-wave report counts for one program.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Exchanges {
    pub ballot: u32,          // xcl_x_ballot( sites attributed to a pc
    pub entry: u32,           // entry ballots (no pc: the prologue)
    pub readlane: u32,        // xcl_x_rla_uniform( + folded meets
    pub readlane_folded: u32, // of readlane: folded reduction tails (each replaced >= 2 v_readlane)
    pub bpermute: u32,        // xcl_x_rla_any(
    pub coherent_load: u32,   // `xcl_x_meet() && xcl_half` meets
    pub other: u32,           // xcl_x_meet() in no known form (must be 0; a test pins it)
    pub total: u32,           // all of the above except readlane_folded
    pub in_loops: u32,        // sites whose pc lies in a loop range
    pub threads_per_group: u32, // #define XCL_X_THREADS
    pub waves_per_group: u32,   // #define XCL_X_WAVES
    pub spin: bool,             // #define XCL_X_SPIN 1: halves meet through counters, not group barriers
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct RegisterArray { pub name: String, pub dwords: u32 }

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Form { Straight, Structured, SwitchPc, Direct }

/// Console-side shape (backend independent), from `gpu::dxil::cfg::Cfg`.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct ProgramShape {
    pub instructions: u32, pub blocks: u32, pub loops: u32, pub max_loop_depth: u32,
    pub divergent_branches: u32, pub movrel_sites: u32,
    #[serde(skip)] pub loop_ranges: Vec<(u64, u64)>,
}

/// The program a container holds for `stage` (mirrors `pso::console_translation` `pso.rs:1015-1058` and
/// `console_mesh_with` `pso.rs:1136-1143`): code, mode, wave64, group threads (None for VS / PS).
pub struct ConsoleProgram { pub code: Vec<u8>, pub mode: crate::gpu::recomp::Mode, pub wave64: bool, pub group_threads: Option<u32>, pub gs_start: Option<Option<u64>> }
pub fn shape_console_program(blob: &[u8], stage: crate::gpu::recomp::pso::Stage) -> Result<ConsoleProgram, String>;
pub fn shape_geometry_program(vs: &[u8], gs: &[u8]) -> Result<ConsoleProgram, String>;   // geometry_program + Cfg gs_start

pub fn shape_program(code: &[u8], gs_start: Option<Option<u64>>) -> Result<ProgramShape, String>;
pub fn shape_hlsl_exchanges(hlsl: &str, loop_ranges: &[(u64, u64)]) -> Exchanges;
pub fn shape_hlsl_register_arrays(hlsl: &str) -> Vec<RegisterArray>;   // `static uint NAME[N];`, non-const
pub fn shape_hlsl_form(hlsl: &str, branches: bool) -> Form;            // "    uint pc = " in main → SwitchPc
pub fn shape_hlsl_upper_lanes(hlsl: &str) -> bool;                     // "static bool xcl_hw32" (emit.rs:4870)

/// The HLSL backend's shape of one program: what ran (`split` as passed) and, for a wave64
/// compute / mesh / geometry program, exchanges of the split form, emitted for counting when it did not run split.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct HlslShape {
    pub program: ProgramShape, pub form: Form, pub unstructured: bool,
    pub register_arrays: Vec<RegisterArray>, pub private_bytes_per_thread: u64,
    pub upper_lanes: Option<bool>, pub exchanges: Option<Exchanges>, pub hlsl_bytes: u64,
}
pub fn shape_hlsl(p: &ConsoleProgram, stage_is_graphics: bool, windows: u32, split: bool) -> Result<HlslShape, String>;
```

Internally, `shape_hlsl` emits through `pso::console_translation` for VS/PS/CS (with `sampler_base` 0, which does
not affect shape), `translate_mesh` for mesh, and `translate_geometry` for geometry.
- `split` is true, or the program is not a wave64 compute/mesh/geometry program: one emission, and both the shape and
  the exchanges come from it.
- `split` is false for a wave64 compute/mesh/geometry program: a second emission with `split_wave64 = true` is
  used only for `exchanges`.
- Wave32 programs and VS/PS: `exchanges` is None.

#### 5.2 Block F (fork glue)

```rust
/// Entry points the call sites use; `start` is None unless tier >= pass (the only cost when off).
pub fn telemetry_shader(blob: &[u8], stage: crate::gpu::recomp::pso::Stage, split_wave64: bool, windows: u32,
    start: Option<std::time::Instant>, r: Result<crate::gpu::recomp::pso::Compiled, String>) -> Result<crate::gpu::recomp::pso::Compiled, String>;
pub fn telemetry_mesh(blob: &[u8], split_wave64: bool, windows: u32, start: Option<std::time::Instant>,
    r: Result<crate::gpu::recomp::pso::TwoPass, String>) -> Result<crate::gpu::recomp::pso::TwoPass, String>;
pub fn telemetry_geometry(vs: &[u8], gs: &[u8], split_wave64: bool, start: Option<std::time::Instant>,
    r: Result<crate::gpu::recomp::pso::TwoPass, String>) -> Result<crate::gpu::recomp::pso::TwoPass, String>;

/// One sink (the global one, or a test's own: part 1's globals are OnceLocks, tests must not open them).
struct ShapeLog { lines: crate::log::Events, seen: Mutex<HashSet<ShapeKey>>, queue: SyncSender<Job>,
    queued_bytes: AtomicU64, records: Mutex<Vec<serde_json::Value>>, counts: ShapeCounts }
fn shape_log() -> Option<&'static ShapeLog>;           // OnceLock; created on first use when run_dir() is Some
fn shape_job(job: Job) -> serde_json::Value;           // worker body: backend, Block S, DxilStats → record
fn dxil_backend(container: &[u8]) -> Option<Backend>;  // reads `!llvm.ident` through gpu::dxil::bitreader: "xcl" → Direct
fn split_reason() -> String;                            // mirrors pso.rs:1231-1236 with the manifest's range
fn shaders_final();                                     // called from telemetry_final (part 1, report.rs:431)
```

### 6. Call sites (pso.rs). Every edit is one line.

`S` below stands for `crate::log::telemetry_on(crate::log::Tier::Pass).then(std::time::Instant::now)`.
Rust evaluates call arguments left to right, so the start time is taken before the wrapped expression runs.

| # | Where | Today | After |
|---|---|---|---|
| 1 | `gpu/recomp/pso.rs:158`, `console_to_dxil_in` (cache disallowed: debug switches, `XCL_LOOP_BUDGET`) | `return compile_one().map(\|(c, _)\| c);` | `return crate::report::telemetry_shader(blob, stage, split_wave64, windows, S, compile_one().map(\|(c, _)\| c));` |
| 2 | `pso.rs:171`, `console_to_dxil_in` | `cached_shader(shader_cache(), key, blob, from_disk).map(\|shader\| shader.as_ref().clone())` | `crate::report::telemetry_shader(blob, stage, split_wave64, windows, S, cached_shader(shader_cache(), key, blob, from_disk).map(\|shader\| shader.as_ref().clone()))` |
| 3 | `pso.rs:1170`, `console_mesh` | `console_mesh_with(blob, ps, split_wave64, windows, &dir, shader_cache(), \|code, ms, wave64\| {` | `crate::report::telemetry_mesh(blob, split_wave64, windows, S, console_mesh_with(blob, ps, split_wave64, windows, &dir, shader_cache(), \|code, ms, wave64\| {` |
| 4 | `pso.rs:1172`, `console_mesh` | `})` | `}))` |
| 5 | after `pso.rs:1249`, `console_geometry` (inserted line) | none | `let shape_start = S;` |
| 6 | `pso.rs:1261`, `console_geometry` (direct) | `return Ok(TwoPass { cs: k.dxil, … });` | `return crate::report::telemetry_geometry(vs, gs, split_wave64, shape_start, Ok(TwoPass { cs: k.dxil, … }));` |
| 7 | `pso.rs:1267`, `console_geometry` (HLSL) | `finish_two_pass(translation, record, ps)` | `crate::report::telemetry_geometry(vs, gs, split_wave64, shape_start, finish_two_pass(translation, record, ps))` |
| 8 | `report.rs:444`, `telemetry_final` (part 1, fork code) | end of the function | `shaders_final();` |

There are 7 edited lines in `pso.rs`, in 3 functions, and none in `real.rs`, `emit.rs` or `gpu/dxil/`.
Two of the edits wrap an existing return line instead of adding a line. That departs from the letter of D6
(open decision O6). Off cost: each entry evaluates `S` once, which is one relaxed load, and `telemetry_*`
returns at `start == None` without another load.

### 7. `shaders.json` schema (schema 1)

Top level:

| field | type | source / rule |
|---|---|---|
| `schema` | 1 | — |
| `run_id` | string | manifest (part 1) |
| `translator_source_id` | string | `env!("XCL_TRANSLATOR_SOURCE_ID")` (`build.rs:73`). Two files are comparable only when this matches |
| `wave_lane_count_min` / `_max` | u32\|null | manifest (`telemetry_device`) |
| `split_wave64_forced` | bool | `pso::split_wave64_forced()` (`pso.rs:1183`) |
| `compute_split` | bool\|null | `wave32_only(min,max)` or forced; null when the range is unknown and not forced (the runtime then splits: `pso.rs:1213`) |
| `graphics_upper_lanes_eligible` | bool\|null | `graphics_wave32(min,max)` or forced |
| `split_reason` | string | `"XCL_SPLIT_WAVE64=1"` \| `"device waves {min}-{max}"` \| `"wave range unknown"` (same text as `pso.rs:1232-1236`) |
| `direct_dxil` | string\|null | value of `XCL_DIRECT_DXIL` |
| `counts` | object | `records`, `analysed`, `dropped`, `failed`, `pending_at_exit`, `kept_in_memory_cap_hit` |
| `records` | array | below; one per key `(record, stage, split, windows)` |

Per record:

| field | type | source | null rule |
|---|---|---|---|
| `record` | string (16 hex) | `gpu::shader::fnv64(blob)` (`gpu/shader.rs:141`), the same as `[SHADER]` log hashes; geometry: the GS blob | never |
| `vs` | string\|absent | geometry only: fnv64 of the VS blob | — |
| `stage` | `"vs"\|"ps"\|"cs"\|"ms"\|"gs"` | call site | never |
| `windows` | u32 | call-site argument | never |
| `wave64` | bool\|null | `fssm_wave64` / `detect_wave64` (FSPX) / true (geometry, `mod.rs:1381`) | container not parsed |
| `split` | bool | cs/ms/gs: `wave64 && split_wave64` argument (ran as two 32-lane halves); vs/ps: false | null only if `wave64` is null |
| `upper_lanes` | bool\|null | vs/ps: rebuilt HLSL declares `xcl_hw32` (the form is applied only when a read needs it, `mod.rs:1576-1581`) | cs/ms/gs; direct backend |
| `split_reason` | string | as at the top level (repeated so a record stands alone) | never |
| `backend` | `"hlsl"\|"direct"\|null` | `"hlsl"` when the stage is not routed to direct (`kernel::enabled`/`enabled_graphics`, `gpu/dxil/kernel.rs:45`, `:52`) and the rebuilt HLSL is not over `dxc::UNOPTIMISED_ABOVE` (`gpu/recomp/dxc.rs:501`; see `pso.rs:615`); otherwise `dxil_backend()` on the returned DXIL | ambiguous and the ident could not be read |
| `form` | `"straight"\|"structured"\|"switch_pc"\|"direct"` | HLSL: `shape_hlsl_form` (`uint pc =` from `mod.rs:1627`/`:1629` → switch_pc; no branches → straight); direct: `"direct"` | backend null |
| `unstructured` | bool\|null | `form == "switch_pc"` | backend null or direct |
| `register_arrays` | `[{name, dwords}]`\|null | HLSL: every non-const `static uint NAME[N];` in the rebuilt HLSL (today only `xcl_scratch`, 1024). Direct: `[]` when `dxil.allocas == 0` | direct with allocas > 0 (size and kind not readable from `DxilStats`), backend null |
| `scratch_hint` | object | `private_bytes_per_thread` = Σ dwords×4 of `register_arrays` (null when those are null); `chain_bytes_per_group` = `Compiled.dispatch_chain.dwords_per_group×4` (`gpu/dxil/module/split.rs:23`), 0 without a chain; `group_threads` (null for vs/ps); `bytes_per_group` = private×threads + chain (null if a part is null). The chain bytes are the ones to compare with `XCL_SPLIT_SCRATCH_MB` (`hle/d3d12x/real/dispatch_chain.rs:24-30`) | per subfield, as stated |
| `instructions`, `blocks` | u32\|null | `Cfg::build`: `insts.len()`, `rpo.len()` (reachable console blocks) | Cfg failed |
| `loops` | u32\|null | `Cfg.headers.len()`, one per guarded loop head on the direct path (`kernel.rs:360`) | Cfg failed |
| `max_loop_depth` | u32\|null | per header, range [header pc, end of its farthest back-edge source); depth = largest count of ranges containing a range's start (a partial overlap counts as nesting) | Cfg failed |
| `divergent_branches` | u32\|null | count of `S_CBRANCH_EXECZ/EXECNZ/VCCZ/VCCNZ`: branches whose condition is a lane mask. This says the branch tests a mask, not that it diverges at run time | Cfg failed |
| `movrel_sites` | u32\|null | `V_MOVREL*` + `V_SWAPREL_B32` (become register `switch`es, `emit.rs:3334`) | Cfg failed |
| `exchanges` | object\|null | §5.1 `Exchanges`, always counted on the split form, plus `paid` (= `split`) | not wave64; vs/ps; backend not `hlsl`; rebuild failed |
| `dxil_bytes` | u64 | `Compiled.dxil.len()` / `TwoPass.cs.len()` (compute pass) | never |
| `dxil` | object\|null | `container_stats`: `bitcode_bytes`, `blocks`, `instructions`, `phis`, `backward_branches`, `allocas`, `switches` (DXIL side: not comparable with the console `blocks`) | stats parse error |
| `dispatch_chain_pieces` | u32\|null | `dispatch_chain.pieces.len()` | no chain |
| `compile_ms` | f64 | wall time of the first successful call for this key. It covers translation and DXC on a miss and only the lookup on a hit; the field does not say which | never |
| `shape_status` | string | `"ok"`, `"partial: <reason>"`, `"failed: <first line>"`, `"dropped"` (queue full), `"pending_at_exit"` | never |

**Record-level vendor meaning.** On RDNA4 (`32-64`), a wave64 CS/MS/GS record has `split: false` and
`exchanges.paid: false`, with the same `exchanges` numbers as on the RTX 5080 (`32-32`), which has `split: true`
and `paid: true`, provided `translator_source_id` is the same. No vendor names are checked anywhere.

### 8. Writing

- `shaders.jsonl` is opened through `crate::log::Events::create(run_dir/"shaders.jsonl")` when the sink is first used.
  Each line is one `write_all` and flushed (`log.rs:364-420`), so a run that ends by TerminateProcess keeps
  every line written before the kill, as `events.jsonl` does.
- `"ev":"seen"` is written on the caller's thread at the first sighting: key, `stage`, `split`, `wave64` (null until
  analysed), `dxil_bytes`, `compile_ms`, `split_reason`, and `shape:"dropped"` when the job was not queued.
- `"ev":"shape"` is written on the worker and holds the full record.
- `shaders.json` is written by `shaders_final()` from `write_final` → `telemetry_final` (`report.rs:188`, `:431`).
  1. stop intake;
  2. wait up to 2 s for the queue to empty;
  3. merge `seen` and `shape` by key (records still queued get `pending_at_exit`);
  4. write the file compact (`serde_json::to_string`, one record per line inside the array) through
     `tmp` + rename, as `write_manifest` does (`report.rs:698`);
  5. rewrite it on later `write_final` calls (idempotent).

  A killed run has no `shaders.json`, but its `shaders.jsonl` holds the same data line by line. The tier `pass` gate is
  part 1's `telemetry_on(Tier::Pass)`.
- One `"ev":"shape_queue"` event goes to `events.jsonl` when the queue drains, sampled by `xlog::sampled`. A reader
  can then skip `pass` timings taken while the worker was busy.

### 9. Constraints and considerations

- **Cache hits.** Every record is rebuilt from (blob, stage, windows, split) and the `Compiled` returned. Nothing depends on
  whether this process translated the program, so hits and misses produce identical records (test G3).
- **Worker threads and duplicates.** `console_to_dxil_in` runs on title threads and on PSO workers. `shader_flight`
  (`pso.rs:859`) coalesces concurrent misses, and both callers then reach the hook. The `seen` set
  (`Mutex<HashSet<(u64, u8, bool, u32)>>`) admits the first.
  - Errors (including `"DXIL not compiled yet"` from `dxc::defer`, `pso.rs:590`) are not records and do not enter the set, so a later success is still recorded.
  - Refusals stay part 1's `shader_refused` events.
- **Thousands of shaders: memory and size bounds.**
  - The queue holds at most 1024 jobs and 64 MiB of blob+DXIL copies. When full, the job is dropped and counted; the worker never blocks a title thread.
  - The `seen` set costs about 32 B per key.
  - `records` in memory is capped at 50 000 values; beyond that only JSONL holds them, and the cap is counted.
  - `shaders.json` is about 0.7 KB per record, so about 2 MB for 2 800 programs.
- **Worker cost.**
  - One HLSL emission per distinct program; two for wave64 compute/mesh/geometry programs that did not run split.
  - On a retail title this is minutes of one core on a cold start (`pso.rs:176-178` measured 0.1–0.3 s for large programs).
  - The worker runs at below-normal priority with a 64 MiB stack: `translate` recurses, and `MAX_NEST` is 24 (`mod.rs:726`).
  - Panics are caught with `catch_unwind` and become `failed: panic`.
  - The DXIL copy is parsed by `container_stats` on the worker, not on the caller.
- **Mesh and geometry.**
  - Mesh records describe the compute pass (`TwoPass.cs`). The pass-through MS is XCL's own and gets no record.
  - Geometry is never cached and has no `Stage`. Its record has `stage:"gs"` and `vs`.
  - HLSL geometry does not apply `by_pc` (`mod.rs:1567`), so its masks are all exchanged. The text count shows that without special-casing.
- **Off.** One relaxed load per entry call (`S`). No thread, file or allocation. Below `pass`: the same, because `S` is `None`.
- **Shareable upstream (D8).** Block S has no fork imports and no title data. The numbers it produces for a title are
  title-derived, and publishing them is a separate decision (5.6).
- **Environment consistency.** The rebuild runs in the same process and environment as the real translation, so switches
  such as `XCL_RECOMP_NO_STRUCTURED`, `XCL_X64_ALL_MASKS` and `XCL_LOOP_BUDGET` apply to both.

### 10. Work breakdown

| Unit | Files owned | Content | Acceptance | Lines (est.) |
|---|---|---|---|---|
| U1 shape core | `crates/xcl/src/report.rs` (Block S + `mod shape_tests` only) | §5.1 functions and structs | tests T1–T6 pass; Block S imports nothing from `crate::log` or `DxilStats` (grep check); `cargo build` on Windows | ~120 code + ~110 tests |
| U2 fork glue | `crates/xcl/src/report.rs` (Block F + `mod shape_glue_tests`; 1 line in `telemetry_final`) | §5.2, §8 | tests G1–G6 pass | ~140 code + ~90 tests |
| U3 call sites | `crates/xcl/src/gpu/recomp/pso.rs` | §6 rows 1–7 exactly | builds; `git diff --stat` on pso.rs shows 7 lines changed and 1 added; existing pso tests pass | 7 |
| U4 plan text | `docs/guide/telemetry.md` (fork doc, `dev`) | 5.6 field definitions per §7; section 11 deviations; Test plan row 4 wording | the orchestrator reviews it | ~30 |
| U5 run | none | whole suite on Windows; `tests/testtitle` at `XCL_TELEMETRY=pass`: `shaders.json` parses, records ≥ the `[SHADER]` count; the same on the RTX 5080 and on an RDNA4 if available; diff `exchanges` between the two | real counts reported | — |

**Concurrency.** U1 comes first. U2 shares `report.rs` with U1, so it runs after U1. U3 and U4 own other files and
can run in parallel with U2 against the signatures in §5. U3 builds only once U2 has landed. U5 runs last.
Total: about 500 lines with tests, about 270 without. The plan's ~300 did not count tests.

#### Unit tests (Test plan row 4, plus cache hit)

All tests are CPU only (HLSL emission needs no DXC). Programs are assembled with `rdna2::encode`
(`gpu/rdna2.rs:459`), in the style of `x64_masks.rs` tests and `recomp/tests.rs:108` `prog`. Compute mode is
`CsInterface` 64×1×1 with `wave64: true`.

- **T1 bpermute.** Program: `ds_bpermute_b32` ×2, `v_readlane_b32 s4, v1, 5`, `s_endpgm`, with split.
  Expect `bpermute 2, readlane 1, readlane_folded 0, ballot 0, entry 1, coherent_load 0, other 0, total 4,
  waves_per_group 1, threads_per_group 64`. The same program emitted with `split_wave64: false` contains no `xcl_x_`
  call. This pins that "native" means 0 exchanges.
- **T2 lane-local masks only.** Program: `v_cmp_gt_u32` (vcc), `v_cndmask_b32` (reads vcc per lane), `s_endpgm`
  (the program of `x64_masks.rs` `a_selector_only_mask_needs_no_exchange`). Expect `ballot 0, entry 1, total 1`.
  Control: add `s_mov_b64 s[2:3], vcc` and expect `ballot 1, total 2`.
- **T3 folded pair and coherent load.** `v_readlane s4, v1, 3; v_readlane s5, v1, 35` gives `readlane 1, readlane_folded 1`.
  `s_load_dword` with GLC gives `coherent_load 1`.
- **T4 register arrays, `switch_pc` form.** A loop program (backward `s_cbranch_scc1`) translated with
  `Options { structured: false, .. }` gives `form switch_pc, unstructured true, register_arrays []`,
  `private_bytes_per_thread 0`. The same program plus `scratch_store_dword`/`scratch_load_dword` gives
  `register_arrays [{xcl_scratch, 1024}]`, `private_bytes_per_thread 4096`. With `structured: true` the form is `structured`.
- **T5 program shape.** A program with `s_cbranch_execz`, `s_cbranch_vccnz`, `s_cbranch_scc1` and two nested loops gives
  `divergent_branches 2, loops 2, max_loop_depth 2`, and `instructions` equals `translate(..).instructions`.
- **T6 in_loops and drift guard.** A bpermute inside the loop gives `in_loops 1`. Every `xcl_x_meet()` occurrence in the
  T1–T5 emissions classifies, so `other == 0`.
- **G1 cache-hit rebuild.** Build an FSPX container (pattern `recomp/tests.rs:8899-8921`
  `fspx_compute_user_sgprs_from_rsrc2`, `gpu::xbox::tests::container` `xbox.rs:771`) holding the T1 program. Run
  `shape_job` with a `Compiled` whose DXIL comes from `gpu::dxil::kernel::translate` (CPU, no DXC), in a fresh sink
  where nothing was translated before. The record equals the one from a job made right after `console_translation`
  of the same blob. The only exception is `compile_ms`.
- **G2 dedupe.** The same key twice gives one `seen` and one `shape` line. A different `windows` gives two records.
- **G3 off.** With `start = None`, `telemetry_shader` returns its input unchanged, and the sink `OnceLock` stays uninitialised.
- **G4 bounds.** A sink with a 1-byte budget: the job is dropped, `seen` carries `shape:"dropped"`, and `counts.dropped == 1`.
- **G5 final merge.** JSONL with one record that has only `seen` and one with `seen`+`shape` gives a `shaders.json` with
  `pending_at_exit` and `ok`, and it is valid JSON.
- **G6 backend.** `dxil_backend` returns `Direct` on `kernel::translate` output and `Hlsl` on a DXC container
  (`skip_test("no DXC")` when DXC is missing).

### 11. Open decisions for the user

- **O1 Placement.**
  - (a, recommended) Blocks S and F appended to `report.rs`, with inline test modules.
  - (b) Block S appended to `gpu/recomp/x64_masks.rs`, its natural upstream home. Append-only, but outside D6's two files.
  - (c) A new `gpu/recomp/shape.rs` plus one `mod` line. Cleanest upstream, but breaks "no new module".
- **O2 Direct-backend exchanges.**
  - (a, v1) `null`.
  - (b) A counter on `Lw`: about 6 one-line upstream edits in `gpu/dxil/lanes.rs`, `lower_mem.rs` and `lower.rs`, plus a way out of the translator thread (`kernel.rs:163`).
  - (c) Option B's predictor for the direct rules only. That needs `lower.rs:916`'s constant folding, so it is not exact.
- **O3 Rebuild cost.**
  - (a, recommended) Background worker at tier `pass`.
  - (b) A fork "shape sidecar" next to `translated/v2` entries, so later runs read instead of re-emitting. It adds invalidation logic and files in upstream's cache dir.
  - (c) Shape for misses only. Hits get `shape_status:"cache_hit"`, which would fail "covers cached shaders".
- **O4 Comparability emission.** On devices that did not split, a second split-form emission per wave64 compute/mesh/geometry
  program, so `exchanges` are equal across vendors. The other choice is `exchanges: null` when not split, which halves the RDNA4 cost and loses the comparison.
- **O5 Plan deviations to accept (U4 edits the plan):**
  - `form` gets `straight`, `structured`, `switch_pc` (the `pc` chain, not a `switch` over arrays) and `direct`. `per_lane` is not derivable from cached DXIL (`XCL_PER_LANE` against the per-instruction form and the compact form).
  - `register_arrays` is a list of declared private arrays, and in the HLSL `switch_pc` form it is empty unless `scratch_*` is used. Test row 4's "set exactly for the `switch_pc` form" becomes T4.
  - The spec line `docs/specs/2026-11-24-direct-dxil.md:49` is stale. Under the repo rules it is reported, not edited.
- **O6 Call-site shape.** 7 lines in `pso.rs`. Two edits wrap existing return lines, and one is an added `let`. The other
  choice is moving the hook to the callers in `real.rs` (`:4426`, `:5307`, `:5317`, `:6084`, `:6218`, `:6282`, plus the cache-hit paths): more sites, and in a 7000-line file.
- **O7 Files.** `shaders.jsonl` (incremental) plus `shaders.json` (final), or only `shaders.json` as the plan says. That
  would lose everything on TerminateProcess.
- **O8 Static vs dynamic.** Counts are static sites, with `in_loops` as the only hint of repetition. Dynamic counts belong to
  part 5's counters.
- **O9 #1410 text.** `split_reason` copies 5 lines of `pso.rs:1231-1236` `wave_mode_line`. The other choice is making it
  `pub(crate)`, a one-word upstream edit.

### 12. Alternatives rejected

- Option A: exact, but about 15 edits interleaved in emitters and structs, a change to the cache format, and hits still not covered.
- Option B: a predictor duplicating the emitter's mask, folding and constant rules. It drifts silently and gives a guess, not a count.
- Counting in DXIL: barriers carry no kind, the compact form emits each block class once (`compact.rs:1`), and the spin
  form has no group barrier (`lanes.rs:263`).
- Synchronous analysis on title threads: it would add about one translation time to every cache hit during loading.

## 2026-11-26 Telemetry parts 2 and 3: frame timing and GPU list/pass timing (user decided the file placement, the scope "everything before the deep dive" and commits per part; orchestrator approved the design)

Designed by an Opus worker against worktree xcl-adopt at 21890e09 with part 1 applied (part 1 is now commit a16cc528 on that branch); citations spot-checked by the orchestrator (real.rs:7385-7393 q_execute_command_lists closure, :638 orig_fn pub, :427 device_raw; list_x.rs:3384-3385 and :3404 kickoff_x; present.rs:1017 present_now, :1237 PRESENTS.fetch_add; frame.rs:362 wait_frame_event_x, :174 qpc pub, :180 qpf private; layout.rs:8/:12; d3d12_layout.rs:15). Paths below are relative to crates/xcl/ unless they start with tests/ or docs/.

### User decisions (2026-11-26)

- Commits: one commit per part, after the whole suite passes on the Windows PC; nothing is pushed until the user says.
- File placement (replaces D6 for the GPU timing code): a fork-only file. Everything section 5.2 and sections 4 and 7 put in report.rs goes in `crates/xcl/src/telemetry_gpu.rs` instead, pulled in from the fork's appended block in report.rs by `#[path = "telemetry_gpu.rs"] mod gpu_tel;` and `pub use gpu_tel::*;`, so the call sites keep the paths `crate::report::telemetry_*`. Upstream sees neither the file nor the line.
- Scope: all parts are built before the game deep dive.

### Orchestrator decisions on the open points of parts 2 and 3 (section 12)

- O-1: (c), as the user decided. The pure math and field builders of section 5.1 stay appended to log.rs.
- O-2: a resubmitted list keeps its valid list ticks and gets `resubmitted: true`; pass ticks of a resubmitted list are null with `why: "resubmitted"`. docs/guide/telemetry.md 5.4 and Test row 2 are updated in the closing docs unit.
- O-3: `io` is aggregated per frame. O-4: not done; `list.id` is the PC list. O-5: no `first_read`. O-6: no copy/clear pass kinds in part 3. O-7: `translate_us` and `console_bytes` are null.
- O-8: new fork-only test file crates/xcl/tests/telemetry_e2e.rs and the test title changes are accepted. O-10: the telemetry thread exists only at tier frame and above.
- Unit changes that follow from the file placement: P2-A owns `src/telemetry_gpu.rs` (new) only and does not touch report.rs; P2-C adds the frame glue to `src/telemetry_gpu.rs` (after P2-A), the two include lines plus the `telemetry_gpu_start()` / `telemetry_gpu_final()` calls in report.rs's fork block, and C1-C6; P3-A, P3-D and P3-E likewise write to `src/telemetry_gpu.rs`.
- Check for every unit: the orchestrator builds on the Windows PC and runs the whole suite in the desktop session; baseline with 15 adopted PRs + part 1 (a16cc528): 1494 passed, 4 failed (gpu::dxil::fuzz_tests::fuzz_validate_and_gpu_0..3, NVIDIA 0x8007000E), 47 ignored, 44 SKIPPED lines.

Status: draft for the orchestrator. Design only. Nothing was built or run: the crate builds on
Windows only and this was written on macOS. All `path:line` citations are against the worktree
`xcl-adopt` (branch `adopt/diagnostics-2`, HEAD `21890e09`, telemetry part 1 staged, not
committed). Paths are relative to `crates/xcl/` unless they start with `tests/` or `docs/`.
Plan = `docs/guide/telemetry.md`. Part 1 = the blocks appended to `src/log.rs` (from `:264`,
"fork telemetry (docs/guide/telemetry.md, part 1)") and `src/report.rs` (from `:290`,
`telemetry_start`).

---

### 1. Goal and scope

**Goal.** With `XCL_TELEMETRY=frame`, every shown frame gets one `frame` event that says, from the
event alone, whether the frame was CPU-bound or GPU-bound (H6). `gpu_ms` is the union of the
measured GPU intervals of the command lists submitted in that frame, after calibration. Every
submitted PC command list gets a `list` event with its start and end ticks on its queue. Each queue
gets `clock` events, and the run gets `memory` events. With `XCL_TELEMETRY=pass`, the GPU span of
each list is also split at barrier batches into `pass` events. `pipeline` creation, `resource` and
`io` events are added at that tier too. With telemetry off, no heap, buffer or thread is made, and
each site costs one relaxed atomic load.

#### Part 2 (tier `frame`): in

- P2.1 GPU stamp engine: own timestamp query heaps per queue, one set per frame in flight;
  start/end stamps for every submitted PC list; resolve once per queue per frame; readback at
  least 2 frames later, and only after our own fence has passed; per-queue frequency and
  calibration; copy queues timed only when the device reports the capability; resubmissions
  detected and flagged; device removal gives null ticks; everything bounded.
- P2.2 frame delimiting and CPU-side timing at `present_now` / `wait_frame_event_x`, plus
  counters the adopted PRs added (#899 pipeline waits, #2144 look time, bind draw/dispatch counts).
- P2.3 events: `frame`, `list`, `clock`, `memory` (with #1515's budget).
- P2.4 `XCL_TELEMETRY_SAMPLE=<n>`: GPU stamps only on every nth frame. `frame` events are still
  written for every frame.
- P2.5 the e2e test (Test plan row 2) and the first D7 overhead run.

#### Part 3 (tier `pass`): in

- P3.1 pass stamps: one timestamp per barrier batch, recorded into the PC list at the title's
  barrier sites (`resource_barrier_x`, `console_list::close`), on recording-owned slot blocks
  bound to the submission at submit time.
- P3.2 per-pass counters (draws, dispatches, barriers, pipeline ids) from one-line call sites in
  `bind.rs`.
- P3.3 events: `pass`; `pipeline` creation (`source`, `compile_ms`); `resource`; `io`.
- P3.4 the GPU test of Test plan row 3.

#### Explicitly out (both parts)

- Per-draw timing (tier `all`, plan 5.10) and upstream PR #1766. Its per-op `ResolveQueryData`
  serialises each queue, and that is why it was not adopted. Nothing here resolves per op.
- `shaders.json` (part 4), `error`/`phase`/`loop_guard` (part 5), `smoke.ps1` (part 6).
- `list.translate_us` and `list.console_bytes`: written as `null` in part 2. Filling them needs a
  call site in `console_list::close` (`src/hle/d3d12x/console_list.rs:734`). Whether to add it in
  part 3 is listed in section 12, O-7.
- A grouping of PC segments back into the title list they came from (section 12, O-4).
- No change to how the title or XCL submits, records, paces or presents.

---

### 2. Facts from the tree that shape the design

| Fact | Where |
|---|---|
| Part 1 API: `Tier` (Copy, `repr(u8)`), `telemetry_on(Tier)` is one relaxed load, `event(tier, ev, fields)`, `sampled(&AtomicU64)`, `run_dir()`, `Events::write` (lock waited ≤100 ms, else dropped and counted) | `src/log.rs:269-316` (`Tier`, `tier_allows`, `telemetry_on`), `:385-421` (`Events::write`), `:433` (`event`), `:449` (`sampled`) |
| Part 1 report side: `telemetry_start`, `update_manifest`, `telemetry_final` (called from `write_final`) | `src/report.rs:293` (`telemetry_start`), `:375` (`update_manifest`), `:431` (`telemetry_final`), `:188` (call in `write_final`) |
| **Submit choke point.** `q_execute_command_lists` maps title lists to PC lists, then calls `list_x::execute_with_waits_on` with a closure whose driver call is `f(this, chunk.len() as u32, chunk.as_ptr())`. `f` is the PC's own `ExecuteCommandLists` (`orig_fn`) | `src/hle/d3d12x/real.rs:7357` (fn), `:7372-7383` (mapping), `:7385-7393` (closure), `:7387` (the `f` line) |
| `execute_with_waits_on` splits the batch before a list whose `Wait32/64BitValueX` is not met, and inserts a GPU-side `queue.Wait` between chunks (`wait_all_on`) | `src/hle/d3d12x/list_x.rs:2147-2177` (`execute_with_waits_on`) |
| **KickoffX submits on its own**, through `exec` = PC `ExecuteCommandLists`: the earlier split segments in an `execute_with_waits_on` closure, then the list itself | `src/hle/d3d12x/list_x.rs:3309` (`kickoff_x`), `:3377` (`exec`), `:3385`, `:3404` |
| **Title list → PC lists.** A console-shaped list (`console_list::pc_list`) is one PC list. A `title_list` wrapper expands through `title_list::push_segments` into `first` (or the primary) followed by `extra_active` segments, in recording order. Anything else is passed through unchanged | `src/hle/d3d12x/real.rs:7375-7382`; `src/hle/d3d12x/title_list.rs:249-262` (`push_segments`), `:44-53` (`SegmentBook`: `extra_active`, `first` for #1452); `src/hle/d3d12x/console_list.rs:616` (`pc_list`) |
| `title_list::wrapper_of` (PC list → wrapper) is `pub(super)`, so report.rs cannot call it | `src/hle/d3d12x/title_list.rs:265` |
| XCL's own submissions (present copy, texture uploads) go through the hooked `ExecuteCommandLists` too, and arrive in `q_execute_command_lists` as raw PC lists | `src/hle/d3d12x/present.rs:1201` (`q.ExecuteCommandLists`); `src/hle/d3d12x/title_textures.rs:703` ("the uploads' own ExecuteCommandLists come back here"), `:1325` |
| Resubmission check exists, but only while `alloc_pool::enabled()` (`XCL_ALLOC_SWAP=0` turns it off) | `src/hle/d3d12x/alloc_pool.rs:71` (`enabled`), `:152-182` (`note_resubmits`), called at `real.rs:7386` |
| `orig_fn(obj, slot)` gives the PC function of an adopted object. `slot_of` is private. `layout::find` / `layout::slot` are public | `src/hle/d3d12x/real.rs:638` (`orig_fn`), `:2930` (`slot_of`); `src/hle/d3d12x/layout.rs:8` (`find`), `:12` (`slot`) |
| The device's creation methods are hooked, and objects made through them are **adopted** (`creator!` → `adopt`) | `src/hle/d3d12x/real.rs:2867-2880` (hook table), `:3134-3166` (`creator!`), `:3170` |
| **The title-facing queue vtable is the console layout.** Calling slot 16/17 through it can land on Signal/Wait or a stub, depending on `QUEUE_SHIFTED`. PC queue slots: ECL 10, Signal 14, GetTimestampFrequency 16, GetClockCalibration 17, GetDesc 18 | `src/hle/d3d12x/real.rs:7224-7291` (`queue_dispatch`); `src/hle/d3d12_layout.rs:15` (ID3D12CommandQueue method order) |
| No code calls `GetClockCalibration` for XCL's own use. The only `GetTimestampFrequency` read is in the GPU test runner | `real.rs:7265-7271` (title passthrough only); `src/gpu/recomp/runner.rs:1851` |
| The title's timestamp pool is shared and round-robin, so it must not be used | `src/hle/d3d12x/list_x.rs:2529` (`TIMESTAMP_SLOTS`), `:2599` (`NEXT_TIMESTAMP … % TIMESTAMP_SLOTS`) |
| **Frame point.** `present_now` increments `PRESENTS` once per shown frame. That count is `presents()` and `report.json d3d12.presents` | `src/hle/d3d12x/present.rs:1017` (`present_now`), `:1237` (`PRESENTS.fetch_add`), `:1259` (`presents`); `src/hle/d3d12x/real.rs:104`, `:393-402` |
| `present_now` has three callers: PresentX, the auto-present on submissions (from `note_execute`, called after the closure in `q_execute_command_lists`), and console queue methods. Re-entry is cut by `IN_PRESENT`, and `PRESENTER` serialises it | `present.rs:178` (`present_x`), `:342` (`note_execute`), `:353` (`on_queue_console_method`), `:1018-1027` |
| Frame-clock wait: `wait_frame_event_x` blocks in `WaitForMultipleObjects` and has several early returns | `src/hle/d3d12x/frame.rs:362-453` (`wait_frame_event_x`) |
| QPC is public, QPF is private | `src/hle/d3d12x/frame.rs:174` (`qpc`), `:180` (`qpf`) |
| Counters from adopted PRs | `pso_async.rs:1216` (`blocked`, #899: waits/total µs/longest/built-here, graphics and compute); `sparse.rs:1572` (`look_counters`, #2144: [0] whole-look µs); `list_x.rs:3797` (`sync_activity`, #2416, counts only after `enable_sync_activity` `:3790`); `bind.rs:45` (`activity`: console draws/dispatches) |
| Memory sources, all `pub` | `real.rs:433` (`video_memory` → local, non-local, budget; #1515); `vram.rs:123` (`arena_bytes`), `:1272` (`page_bytes`); `sparse.rs:419` (`pool_bytes`); the process commit pattern is at `src/host.rs:91-98` (inside the `XCL_ACTIVITY` thread only) |
| `video_memory` creates a DXGI factory on every call | `real.rs:436-439` |
| Device removed: `stop_if_device_removed` → `write_final` → `TerminateProcess`. The device is stored once and never re-created | `real.rs:7548-7561`; `real.rs:558` (only `DEVICE.store`) |
| Barrier sites. The title's `ResourceBarrier` is handled on the PC list (`this`) and emitted at `:4325`. Console-recorded lists emit their barriers at close. XCL-internal UAV barriers between split pieces sit inside `dispatch_chain::dispatch` | `list_x.rs:4245` (`resource_barrier_x`), `:4324-4326`; `console_list.rs:734` (`close`), `:810`, `:869`; `real/dispatch_chain.rs:205` (`dispatch`) |
| Draw/dispatch/PSO sites | `bind.rs:496` (`draw_instanced`), `:513` (`draw_indexed_instanced`), `:869` (`set_pipeline_state`), `:875` (`dispatch`), `:1013-1024` (`translated_dispatch`, with #2205 `culprit` markers around `dispatch_chain::dispatch`), `:269` (`execute_indirect`); `console_list.rs:915-917` (culprit + chain dispatch) |
| PC list reset (all PC resets go through it) | `real.rs:2988` (`cl_reset`), `:2996` (`list_x::on_pc_reset`) |
| Pipeline build choke point (driver compile, #2096's hook) | `real.rs:4142-4163` (`driver_compile_identified`), `:4158` (`let took`), `:4159` (`pso_manifest::note(Build…)`) |
| Shader translation entry point (memory cache → disk cache → `compile_one`) | `src/gpu/recomp/pso.rs:155-171` (`console_to_dxil_in`), `:156` (`compile_one`), `:585` (`compile_console_shader`), `:877-879` (`cached_shader` memory hit) |
| Every DirectStorage request runs through `execute`, on both the sequential and the parallel paths | `src/hle/dstorage.rs:880` (`execute`), `:882` (`note_request`) |
| Title-only resource creation slots (console layout) | `real.rs:2772` (`dev_create_committed_resource_x`), `:3213` (`dev_create_placed_resource_x`), `:3377` (`dev_create_reserved_resource`) |
| Test runner: `skip_test`, `dispatch_timed` (stamps around each rep, frequency read per queue) | `src/gpu/recomp/runner.rs:621`, `:1660`, `:1851` |
| `report` and `log` are public modules | `src/lib.rs:9`, `:26` |

---

### 3. Core mechanism: options

The design has to settle where the timestamps are written and where the state lives.

#### Option A: bracketing stamp lists at submit (recommended)

At the submit choke point, the chunk `[L0 … Ln-1]` that goes to the driver becomes
`[R?, S0, L0, S1, L1, …, Ln-1, Sn]`. Each `Sk` is a tiny command list of telemetry's own, holding
one `EndQuery(TIMESTAMP)` into this queue's heap for the current frame. `Lk` spans from stamp `k`
to stamp `k+1`. `R` is the resolve list for the previous frame's heap on this queue (section 4.4),
and is present only on the queue's first submission of a frame. It is one driver call, as before.

- Slots are taken at submission, so they belong to a submission by construction. A list submitted
  twice gets two pairs of valid ticks.
- No hook in any recording path. Three call-site lines: `real.rs:7387` and `list_x.rs:3385, :3404`.
- It covers every list origin: title lists, segments, console lists, XCL uploads and the present copy.
- Stamp lists are **recorded once and reused**: slot `k` of set `h` always holds the same
  `EndQuery`. They are resubmitted only after the set's fence has passed, which avoids debug layer
  id 553. The per-submit CPU cost is building a pointer vector.
- Cost: n+1 extra list boundaries per chunk. Whether a boundary costs GPU time depends on the
  driver; D7 (section 9) measures it. A timestamp is written when the preceding work is done, so
  the start stamp of `Lk` also includes any idle gap after `Lk-1`. That is the list's span on the
  queue, and is stated in the schema.

#### Option B: stamps recorded into the translated lists

`EndQuery` at the start of recording (`cl_reset`) and at close, with `ResolveQueryData` recorded at
close. The plan's wording ("ResolveQueryData at list end") leans this way.

- Slots are taken at recording. Frame and queue are unknown until submit. A resubmitted recording
  overwrites its own slots, which is the reason for the plan's "null ticks + resubmitted" rule.
  Recordings that are never submitted hold slots until their next reset.
- More hooks: every recording start (`cl_reset`, `title_list` segment starts,
  `create_raw_pc_command_list` `real.rs:3175`) and every close path (the title's Close, `kickoff_x`
  `list_x.rs:3365`, `console_list::close`). That means 5–7 call sites in hot upstream files.
- A resolve per list is a milder form of #1766's serialisation (a drain at every list end).
- XCL-internal lists are not covered unless each creation site is hooked too.

#### Option C: one stamp pair per submitted chunk

Like A, but `[S_begin, L0 … Ln-1, S_end]`: two stamps per driver call.

- Cheapest: two extra boundaries per chunk. The granularity is the chunk, not the list, which is
  coarser than the plan's per-list `list` event (D2 names per-list as the fallback, not per-chunk).
- Kept as the degradation path: if D7 shows that the n+1 boundaries of A cost more than 5 % even
  with sampling, the engine switches the interleave to C (a constant, about 10 lines). `list`
  events then carry `chunk_only: true`.

#### Where the state lives (D6)

| Choice | Upstream conflict cost | Notes |
|---|---|---|
| **(a) all appended to `src/report.rs`; pure math and field builders appended to `src/log.rs`** (recommended default: D6 as written) | none beyond part 1's appended blocks; one-line call sites | report.rs grows by about 1,100 lines of runtime code over parts 2+3. Ugly but conflict-free: upstream rarely touches the file's tail, and a conflict at the end of the file is an append/append conflict, resolved by keeping both |
| (b) a new fork-only file `src/telemetry_gpu.rs` plus `mod` in `lib.rs` | one line in `lib.rs`; can conflict when upstream adds a module next to it | violates D6 ("no new module or file") |
| (c) a new file pulled in from report.rs's appended block: `#[path = "telemetry_gpu.rs"] mod gpu_tel;` | **zero** upstream lines (the `mod` line lives in fork-appended code) | violates the letter of D6 ("no new file"), keeps its intent (no upstream conflict). See section 12, O-1 |

**Recommendation: Option A, with state in report.rs (a).** (c) is offered as O-1 because about
700 lines of D3D12 query code sits poorly next to manifest code, at no conflict cost.

---

### 4. Architecture and data flow (part 2)

#### 4.1 Objects and where they come from

All telemetry D3D12 objects are created through the **PC device's original functions**
(`real::orig_fn(real::device_raw(), layout::slot(layout::find("ID3D12Device"), "<Method>"))`), so
they are **never adopted**. They do not show up in `list_x`/`alloc_pool`/`present` bookkeeping, and
plain `windows` crate calls on them use the PC vtable. Methods used: `CheckFeatureSupport`,
`CreateQueryHeap`, `CreateCommittedResource` (READBACK buffer), `CreateCommandAllocator`,
`CreateCommandList`, `CreateFence`.

Queue methods go through `real::orig_fn(queue, n)` with the PC slots from `d3d12_layout.rs:15`:
ECL 10, Signal 14, GetTimestampFrequency 16, GetClockCalibration 17, GetDesc 18. Signatures follow
`queue_dispatch` (`real.rs:7265-7291`). GetDesc returns its struct through a hidden pointer, as at
`real.rs:7287-7290`.

To keep this testable without an adopted device, the engine takes a small trait-free table:

```rust
struct DevFns { raw: *mut c_void, create_query_heap: usize, create_committed_resource: usize,
                create_command_allocator: usize, create_command_list: usize, create_fence: usize,
                check_feature_support: usize }
struct QueueFns { raw: *mut c_void, ecl: usize, signal: usize, freq: usize, calib: usize, desc: usize }
```

`DevFns::orig()` / `QueueFns::orig(q)` fill these from `orig_fn` in production. `DevFns::plain(&ID3D12Device)`
/ `QueueFns::plain(&ID3D12CommandQueue)` read the object's own vtable (GPU tests, which use a fresh
non-adopted device).

#### 4.2 Per-queue state

```rust
const SETS: usize = 4;                 // frames in flight + 1 (title 2–3 frames in flight; XCL present ring ≤2)
const LIST_SLOTS: u32 = 1024;          // stamps per set at tier frame (≈1023 lists per queue per frame)
const PENDING_FRAMES: usize = 64;      // frame records waiting for GPU data
const LATE_FRAMES: u64 = 16;           // a set not harvested this many frames after its frame → null "late"

struct QueueClock {
    q: QueueFns, kind: QueueKind /*Direct|Compute|Copy|Other*/, id: u32 /*small, stable per run*/,
    freq: Option<u64>,                 // None: <1000 Hz, call failed, or copy queue without the cap
    cal: Option<(u64 /*gpu tick*/, u64 /*cpu qpc*/)>, cal_frame: u64,
    fence: ID3D12Fence, fence_value: u64,
    sets: [StampSet; SETS], cur: usize, cur_frame: u64,
}
struct StampSet {
    frame: u64, state: SetState /*Free|Open|Resolving{value}|Ready*/,
    heap: ID3D12QueryHeap,             // TIMESTAMP, or COPY_QUEUE_TIMESTAMP on a copy queue
    readback: ID3D12Resource, mapped: *const u64,   // persistently mapped, LIST_SLOTS*8 bytes
    stamps: Vec<ID3D12GraphicsCommandList>,          // stamps[k] = EndQuery(heap, k); grown lazily, kept
    resolve_alloc: ID3D12CommandAllocator, resolve_list: ID3D12GraphicsCommandList,
    used: u32, entries: Vec<ListEntry>, pass_blocks: Vec<BlockRef>, // part 3
}
struct ListEntry { seq: u64, list: usize, chunk: u64, slot: u32 /*start; end = slot+1*/,
                   cpu_us: u32, resubmitted: bool, why: Option<&'static str> }
```

- Heaps, readbacks and the fence are created on the queue's first stamped submission (lazily, on the
  submitting thread). Tier off never gets there.
- `freq` is read once per queue. The device is never re-created (`real.rs:558`), so there is no
  re-read path. If that ever changes, a new device means new queue pointers and so new
  `QueueClock`s.
- A copy queue gets a `COPY_QUEUE_TIMESTAMP` heap only if
  `D3D12_FEATURE_D3D12_OPTIONS3.CopyQueueTimestampQueriesSupported` is true (read once via
  `CheckFeatureSupport`). Otherwise `freq = None` and its lists get `why: "unsupported"`. There are
  no vendor-name checks.
- Video and other queue types get `why: "unsupported"`.

Global state (report.rs appended block):

```rust
static GPU: OnceLock<GpuTel> = OnceLock::new();
struct GpuTel {
    queues: RwLock<Vec<(usize, Arc<Mutex<QueueClock>>)>>,  // ≤ 8 queues, linear scan under read lock
    frames: Mutex<VecDeque<FrameRec>>,                      // bounded PENDING_FRAMES
    frame: AtomicU64,                                       // current frame index (presents so far)
    seq: AtomicU64, chunk: AtomicU64,                       // list / chunk sequence numbers
    in_flight: Mutex<HashMap<usize /*PC list*/, (u32 /*queue id*/, u64 /*frame*/)>>, // resubmission check
    drops: [AtomicU64; 6],                                  // full, late, unsupported, resubmitted, removed, sampled_out
    wake: (Mutex<bool>, Condvar), qpc0: i64, qpf: i64,
}
```

#### 4.3 Submit path (`telemetry_execute`)

```
telemetry_execute(queue, lists, submit):
  if !telemetry_on(Frame) { submit(lists); return }          // one relaxed load
  frame = GPU.frame; sampled = sample_due(frame)
  clock = queue_clock(queue)                                   // create on first use (and emit the start `clock` event)
  lock clock (held across the driver call; only submitters to this queue contend; telemetry thread uses try_lock)
    rotate if clock.cur_frame != frame:
        old = sets[cur]; if old.used > 0 { record resolve_list (ResolveQueryData 0..used → readback); prepend R }
        next = sets[(cur+1)%SETS]
        if next.state is Resolving and fence.GetCompletedValue() >= value → treat as Ready
        if next.state != Free (telemetry thread has not harvested yet) → this frame on this queue is unstamped (why "full")
    if !sampled or freq None or set unavailable or used+n+1 > LIST_SLOTS:
        entries get why=… (null ticks); build vec = [R?] + lists
    else: build vec = [R?, S(used), L0, S(used+1), L1, …, S(used+n)]; used += n+1 (stamp k shared as end of L(k-1) and start of Lk)
    resubmission: for each Lk, in_flight[Lk] present and its frame's set not Ready → resubmitted=true (ticks still valid, see O-2)
    t0 = qpc(); submit(&vec); cpu_us = qpc()-t0
    if R prepended: Signal(fence, ++fence_value) via orig; old.state = Resolving{fence_value}
    push ListEntry per Lk (seq, list, chunk, slot, cpu_us, resubmitted, why)
  unlock; notify telemetry thread
```

- **Exact mapping, stated once.** One `list` event = one PC command list in one driver call, as
  it appears in the slice `execute_with_waits_on` hands to the closure. A title list recorded
  through `title_list` produces one `list` event per segment from `push_segments` (`first` or the
  primary, then each `extra_active`), in recording order. A console-shaped list produces one event.
  Lists split into several chunks by unmet waits produce events with different `chunk` numbers.
  The GPU wait between those chunks happens outside any list span. XCL's own submissions (uploads,
  the present copy) produce `list` events too. Those submitted inside `present_now` carry
  `origin: "present"`, through a TLS flag set by the present span (4.6). Everything else is
  `origin: "submit"`.
- **Resolve placement.** `R` (one `ResolveQueryData` of `[0, used)`) is recorded fresh each frame
  into the set's own allocator/list. That is safe because the set is Free, so the GPU is done with
  it. `R` is submitted as the head of the queue's first driver call in the next frame, where a list
  boundary exists anyway. That is one resolve per queue per frame, never per list or per op.
- **Never in the same frame.** A set holds frame `f`'s stamps. Its resolve is submitted no earlier
  than frame `f+1`. The telemetry thread reads it only when `fence ≥ value` **and**
  `GPU.frame ≥ f + 2`. So nothing is read in the frame that wrote it, and nothing waits.
- **A queue idle in later frames** keeps its set Open until its next submission, or until
  `LATE_FRAMES`. After that the thread gives up on the set: its entries are emitted with null ticks
  (`why: "late"`), and the set is reclaimed on the queue's next submission. The resolve is
  discarded and the stamp lists are kept, because their fence has passed by then or they are never
  resubmitted.
- **Not serialising the queue.** The engine adds no queue `Wait`, no CPU wait, and no resolve
  between the title's lists. It adds one `Signal` per queue per frame. Whether the extra list
  boundaries cost GPU time is measured in D7 (section 9, step 4), not assumed.

#### 4.4 Telemetry thread (`xcl-telemetry`, only when the tier is ≥ frame)

It is spawned by `telemetry_start` after `telemetry_open` succeeds. Off spawns nothing. It wakes on
the condvar (every frame) or every 50 ms, and then:

1. For each queue (`try_lock`; skip if busy), for each set in `Resolving{v}`: read
   `fence.GetCompletedValue()`.
   - `u64::MAX` means device removed: mark every pending entry `why: "removed"` and stop GPU harvesting.
   - `≥ v` and `GPU.frame ≥ set.frame + 2`: copy `used` u64 from `mapped` into a local Vec and set
     the state to Free. The copy happens under the lock and takes microseconds. JSON comes after the
     lock is released.
2. Converts ticks: tick 0 or `freq None` gives null. A pair with end < start is null too, with
   `why: "order"`. For valid pairs, `qpc = cal_qpc + (tick − cal_tick) × qpf / freq`, using the
   queue's latest calibration, in i128 arithmetic. Then `ms_since_start = (qpc − qpc0) × 1000 / qpf`.
3. Writes the `list` events, then attaches their calibrated intervals to the matching `FrameRec`
   (by frame index).
4. A `FrameRec` is complete when every queue that had entries in that frame has been harvested, or
   when `LATE_FRAMES` have passed. Then it computes `gpu_ms` (section 6) and writes the `frame`
   event. Frames are written in order of `i`.
5. Every 60 frames, it calls `GetClockCalibration` per queue through `orig_fn` (this is
   thread-safe, a CPU-side call), writes a `clock` event and updates `cal`. It also writes a
   `memory` event, built on this thread, so `video_memory`'s DXGI factory cost stays off the
   present thread.
6. Flushes `io` aggregates (part 3).

`telemetry_final` (part 1, called from every `write_final`, `report.rs:188`) does a last
non-blocking harvest, then writes every pending `list`/`frame` with null GPU fields
(`why: "exit"`, or `"removed"` when the device is gone). It writes the drop counters to the
manifest and takes locks with `lock_within` (200 ms, as in part 1). It never waits on the GPU, so
device removal cannot hang it.

#### 4.5 Frame accounting

`FrameRec { i, qpc_end, interval_ms, present_ms, wait_ms, wait_calls, present_thread_wait_ms,
draws, dispatches, barriers, pso_wait_ms, look_ms, lists, queues_pending: u8, intervals: Vec<(f64,f64)>, gpu_lists_null: u32, sampled: bool }`.

The frame index is `presents()` after the increment, so `frame.i` is 1-based and equals the
running frame count. `report.json d3d12.presents` equals the number of `frame` events, by
construction.

#### 4.6 Spans (RAII, one line at the site)

```rust
pub struct PresentSpan(Option<i64>);   // None when off
pub struct WaitSpan(Option<i64>);
```

`telemetry_present_begin()` stores the start QPC and sets the TLS `IN_PRESENT_SPAN`.
`telemetry_frame_shown(n)` (called right after `PRESENTS.fetch_add`) sets the TLS `SHOWN = n`.
`Drop for PresentSpan`: if `SHOWN` was set during the span, it closes frame `n`, with
`present_ms = now − start` and `qpc_end = now`. It snapshots the counters, pushes a `FrameRec`,
bumps `GPU.frame = n`, and wakes the thread. A `present_now` that returns early (re-entry, lock
busy, nothing to show) never sets `SHOWN` and is not a frame.

`WaitSpan` drop adds its duration to the global `WAIT_NS` and `WAIT_CALLS`, and to the TLS
`THREAD_WAIT_NS`.

---

### 5. Signatures and call sites

#### 5.1 Appended to `src/log.rs` (pure, unit-tested, no D3D12)

```rust
/// `XCL_TELEMETRY_SAMPLE`: None/"" → 1; "1".."10000" → n; else Err (caller warns, uses 1).
pub fn parse_sample(value: Option<&str>) -> Result<u32, String>;
/// Whether frame `i` (1-based) gets GPU stamps at sampling `n`.
pub fn sample_due(i: u64, n: u32) -> bool;                        // i % n == 0 || n == 1
/// Length of the union of [start, end) intervals in ms; 0 intervals → None. Sorts in place.
pub fn union_ms(iv: &mut [(f64, f64)]) -> Option<f64>;
/// GPU tick → QPC through a calibration; None for tick 0, freq < 1000, overflow.
pub fn tick_to_qpc(tick: u64, cal_tick: u64, cal_qpc: i64, freq: u64, qpf: i64) -> Option<i64>;
/// (cpu_ms, wait-adjusted) from interval/present/thread-wait; clamped at 0.
pub fn frame_cpu_ms(interval_ms: f64, present_ms: f64, thread_wait_ms: f64) -> f64;
pub fn frame_fields(f: &FrameFacts) -> Vec<(&'static str, serde_json::Value)>;
pub fn list_fields(l: &ListFacts) -> Vec<(&'static str, serde_json::Value)>;
pub fn clock_fields(queue: u32, kind: &str, gpu_tick: u64, cpu_qpc: i64, freq: Option<u64>, qpf: i64) -> Vec<(&'static str, serde_json::Value)>;
pub fn memory_fields(m: &MemoryFacts) -> Vec<(&'static str, serde_json::Value)>;
// part 3
pub fn pass_fields(p: &PassFacts) -> Vec<(&'static str, serde_json::Value)>;
pub fn pipeline_fields_built(p: &PipelineFacts) -> Vec<(&'static str, serde_json::Value)>;
pub fn resource_fields(r: &ResourceFacts) -> Vec<(&'static str, serde_json::Value)>;
pub fn io_fields(i: &IoFacts) -> Vec<(&'static str, serde_json::Value)>;
pub struct FrameFacts { .. } pub struct ListFacts { .. } pub struct MemoryFacts { .. }   // plain data, Option<f64> for nullable
pub struct PassFacts { .. } pub struct PipelineFacts { .. } pub struct ResourceFacts { .. } pub struct IoFacts { .. }
```

#### 5.2 Appended to `src/report.rs`

```rust
// part 2
pub fn telemetry_execute(queue: *mut c_void, lists: &[*mut c_void], submit: impl FnOnce(&[*mut c_void]));
pub fn telemetry_present_begin() -> PresentSpan;
pub fn telemetry_frame_shown(n: u64);
pub fn telemetry_wait_begin() -> WaitSpan;
fn telemetry_gpu_start();                      // called inside telemetry_start: GpuTel + thread (tier ≥ frame)
fn telemetry_gpu_final();                      // called inside telemetry_final: last harvest + null flush
// internals: QueueClock, StampSet, DevFns, QueueFns, queue_clock(), harvest(), memory_snapshot(), process_commit()
// part 3
pub fn telemetry_pass_boundary(list: *mut c_void, barriers: u32);
pub fn telemetry_list_reset(list: *mut c_void);
pub fn telemetry_draw(list: *mut c_void);
pub fn telemetry_dispatch(list: *mut c_void, pso: usize);
pub fn telemetry_bind_pso(list: *mut c_void, pso: usize);
pub fn telemetry_shader_source(source: &'static str);   // "cache" | "dxc" | "direct": TLS for the current build
pub fn telemetry_pipeline_built(kind: &str, dxil_bytes: usize, record: Option<(u64, bool)>, driver: std::time::Duration, waited: std::time::Duration);
pub fn telemetry_resource_begin(kind: &'static str, desc: usize) -> ResourceSpan;
pub fn telemetry_io_begin(bytes: u64, zlib: bool) -> IoSpan;
```

Each `pub fn` starts with `if !xlog::telemetry_on(<tier>) { …passthrough…; return }`. Each span
constructor returns `Span(None)` when off, and its `Drop` does nothing then. Arguments at the
sites are locals or field reads only. No site calls a function to build its arguments.

#### 5.3 Call sites, part 2 (5 lines; 3 of them replace an existing line)

| # | Site | Line |
|---|---|---|
| C1 | `src/hle/d3d12x/real.rs:7387`, `q_execute_command_lists` closure — **replace** `f(this, chunk.len() as u32, chunk.as_ptr());` with | `crate::report::telemetry_execute(this, chunk, \|c\| f(this, c.len() as u32, c.as_ptr()));` |
| C2 | `src/hle/d3d12x/list_x.rs:3385`, `kickoff_x` prior-segments closure — **replace** `exec(q_raw, chunk.len() as u32, chunk.as_ptr());` with | `crate::report::telemetry_execute(q_raw, chunk, \|c\| exec(q_raw, c.len() as u32, c.as_ptr()));` |
| C3 | `src/hle/d3d12x/list_x.rs:3404`, `kickoff_x` — **replace** `exec(q_raw, 1, one.as_ptr());` with | `crate::report::telemetry_execute(q_raw, &one, \|c\| exec(q_raw, c.len() as u32, c.as_ptr()));` |
| C4 | `src/hle/d3d12x/present.rs:1018`, first statement of `present_now` — insert before `if IN_PRESENT…` | `let _telemetry = crate::report::telemetry_present_begin();` |
| C5 | `src/hle/d3d12x/present.rs:1237`, `present_now` — insert after `let n = PRESENTS.fetch_add(1, Ordering::Relaxed);` | `crate::report::telemetry_frame_shown(n + 1);` |
| C6 | `src/hle/d3d12x/frame.rs:363`, first statement of `wait_frame_event_x` — insert before `trace_calls(…)` | `let _telemetry = crate::report::telemetry_wait_begin();` |

That is six lines in four upstream files. Replacing the line, rather than adding a begin/end pair,
keeps C1–C3 to one line each and keeps one driver call per chunk. The cost is that an upstream
edit of that exact line conflicts, and the conflict is trivial. Nothing is needed in `host.rs`: the
memory event is built on the telemetry thread from the public functions above. The `XCL_ACTIVITY`
prose lines stay as they are.

#### 5.4 Call sites, part 3

| # | Site | Line |
|---|---|---|
| C7 | `list_x.rs:4324`, `resource_barrier_x`, before `if !out.is_empty() {` | `crate::report::telemetry_pass_boundary(this, out.len() as u32);` |
| C8 | `console_list.rs:810`, `close`, before `list.ResourceBarrier(std::slice::from_raw_parts(…))` | `crate::report::telemetry_pass_boundary(list.as_raw(), n as u32);` |
| C9 | `console_list.rs:869`, `close`, before the `BARRIER_MARKER` `list.ResourceBarrier(&[…])` | `crate::report::telemetry_pass_boundary(list.as_raw(), 1);` |
| C10 | `console_list.rs:916`, `close`, before `dispatch_chain::dispatch(&list, real, …)` | `crate::report::telemetry_dispatch(list.as_raw(), real as usize);` |
| C11 | `bind.rs:497`, first statement of `draw_instanced` | `crate::report::telemetry_draw(this);` |
| C12 | `bind.rs:514`, first statement of `draw_indexed_instanced` | `crate::report::telemetry_draw(this);` |
| C13 | `bind.rs:870`, first statement of `set_pipeline_state` | `crate::report::telemetry_bind_pso(this, pso as usize);` |
| C14 | `bind.rs:1022`, `translated_dispatch`, before `dispatch_chain::dispatch(list, pso, …)` | `crate::report::telemetry_dispatch(this, pso);` |
| C15 | `bind.rs:270`, first statement of `execute_indirect` | `crate::report::telemetry_draw(this);` (counted as `indirect`; see schema) |
| C16 | `real.rs:2996`, `cl_reset`, inside `if hr >= 0 {` after `on_pc_reset` | `crate::report::telemetry_list_reset(this);` |
| C17 | `real.rs:4158`, `driver_compile_identified`, after `let took = started.elapsed();` | `crate::report::telemetry_pipeline_built(kind, bytes, record, took, waited);` |
| C18 | `gpu/recomp/pso.rs:878`, `cached_shader` memory hit, after `dxc::note_memory_hit();` | `crate::report::telemetry_shader_source("cache");` |
| C19–C20 | `gpu/recomp/pso.rs:594/609` (direct DXIL `Ok`) and `:663` (HLSL→DXC result) in `compile_console_shader` | `crate::report::telemetry_shader_source("direct")` / `("dxc")` — **implementer verifies** the disk-cache hit path in `translated::cached` (`pso.rs:170`) and adds `("cache")` there. If the source cannot be told apart at a single line, `source: null` |
| C21 | `real.rs:2773`, `dev_create_committed_resource_x`; `:3214`, `dev_create_placed_resource_x`; `:3378`, `dev_create_reserved_resource` — first statement | `let _telemetry = crate::report::telemetry_resource_begin("committed", desc);` (resp. `"placed"`, `"reserved"`) |
| C22 | `dstorage.rs:881`, first statement of `execute` | `let _telemetry = crate::report::telemetry_io_begin(r.destination_size as u64, r.zlib());` |

In part 3, `telemetry_pass_boundary` also counts barriers at tier `frame` (one atomic add), so
`frame.barriers` is filled from then on. In part 2 it is null.

---

### 6. Event schemas

All events also carry `t` (ms since start, from part 1). `null` means unknown, never zero. A tick
or ms field that is null comes with a `why` string. Integers are JSON numbers. Pointers fit in 53
bits on x64 user space, so they are numbers too.

#### `frame` (tier frame; one per shown frame, `i` = 1, 2, …)

| field | type / unit | rule |
|---|---|---|
| `i` | u64 | `presents()` after this frame |
| `interval_ms` | f64 ms | end of the previous showing `present_now` → end of this one; first frame: from telemetry start |
| `cpu_ms` | f64 ms | `max(0, interval_ms − present_ms − present_thread_wait_ms)`: the presenting thread's time neither presenting nor waiting on the frame clock |
| `present_ms` | f64 ms | duration of the showing `present_now` (includes swapchain throttling and the present copy's CPU part) |
| `wait_ms` | f64 ms | `WaitFrameEventX` time that ended in this interval, summed over **all** threads (can exceed `interval_ms` when several threads wait) |
| `wait_calls` | u64 | the number of those calls |
| `gpu_ms` | f64 ms or null | union of the calibrated `[start, end)` of every `list` of this frame with valid ticks, across all queues. Null when no list has valid ticks (none submitted, not sampled, unsupported, late, removed) |
| `gpu_first_ms`, `gpu_last_ms` | f64 ms since start, or null | earliest start and latest end of those intervals (gives the GPU tail past the present) |
| `gpu_lists_null` | u32 | lists of this frame with null ticks. If >0 and `gpu_ms` is set, `gpu_ms` is a lower bound |
| `lists` | u32 | PC lists submitted in this frame (= the number of `list` events, when sampled) |
| `draws`, `dispatches` | u64 | delta of `bind::activity()` (console-code draws/dispatches, all threads) |
| `barriers` | u64 or null | title barrier batches (part 3 onward), else null |
| `pso_wait_ms` | f64 ms | delta of `pso_async::blocked()` total µs, graphics + compute (#899) |
| `look_ms` | f64 ms | delta of `sparse::look_counters()[0]` (#2144: XCL's CPU time before submits) |
| `sampled` | bool | whether GPU stamps were taken this frame |
| `why` | string or absent | the reason `gpu_ms` is null |

**Frames are delimited** by the showing `present_now`: a frame is everything submitted between the
end of one showing present and the end of the next. A list belongs to the frame in which its driver
call was made. This holds also for work the title submits for frame k+1 before presenting k.
CPU-bound vs GPU-bound: compare `gpu_ms` with `interval_ms − wait_ms` and `cpu_ms`. A reader takes
the union of `list` intervals and never sums them.

#### `list` (tier frame; one per PC list per driver call, in sampled frames)

| field | type | rule |
|---|---|---|
| `frame` | u64 | frame index of the submit |
| `seq` | u64 | run-unique submission sequence (`pass.list` refers to it) |
| `id` | u64 | PC command list address (stable while the list lives; joins segments of one PC list across frames) |
| `chunk` | u64 | run-unique driver-call number (lists with the same `chunk` went in one call) |
| `queue` | u32 | telemetry queue id (see `clock`) |
| `queue_type` | string | `direct`/`compute`/`copy`/`other` |
| `start_tick`, `end_tick` | u64 or null | raw ticks on `queue`. Null when `why` is set |
| `start_ms`, `end_ms` | f64 or null | calibrated, ms since start |
| `cpu_us` | u32 | duration of the driver `ExecuteCommandLists` for the whole chunk (the same value on every list of the chunk) |
| `translate_us`, `console_bytes` | null | part 2 does not measure them (O-7) |
| `resubmitted` | bool | this PC list had an earlier submission whose stamps were not yet read |
| `origin` | string | `present` (submitted inside `present_now`) or `submit` |
| `why` | string or absent | `unsupported`, `no_frequency`, `full`, `late`, `removed`, `exit`, `order`, `zero` |

Bundles never reach `ExecuteCommandLists`, so they have no `list` event and no stamps.

#### `clock` (tier frame; at a queue's first stamped submit, then every 60 frames)

`queue` u32, `queue_type` string, `gpu_tick` u64, `cpu_qpc` i64, `frequency` u64 or null (ticks/s
from `GetTimestampFrequency`, null when below 1000 or failed), `qpc_frequency` i64, `copy_timestamps`
bool (for copy queues: the cap bit). If the call fails: `gpu_tick`/`cpu_qpc` are null and `why` is
set.

#### `memory` (tier frame; every 60 frames, from the telemetry thread)

`committed_mb` (process private usage), `vram_local_mb`, `vram_nonlocal_mb`, `vram_budget_mb`
(#1515), `arenas_mb` (in use), `arenas_made_mb`, `pages_mb`, `page_pools_mb`, `sparse_mb`,
`sparse_mapped_mb`: all u64 MiB, the same sources and order as the `memory:` line (`host.rs:104-118`).
`frame` u64. A source that fails gives null.

#### `pass` (tier pass; one per barrier-delimited span of a stamped list)

| field | type | rule |
|---|---|---|
| `frame`, `list` (= `list.seq`), `queue` | u64/u64/u32 | from the submission |
| `i` | u32 | pass index in the list, from 0 |
| `start_tick`, `end_tick` | u64 or null | pass 0 starts at the list's start stamp, the last pass ends at the list's end stamp, inner bounds come from the in-list stamps. All are on the same queue clock |
| `start_ms`, `end_ms` | f64 or null | calibrated |
| `kind` | string | `dispatch` if it had dispatches and no draws, `draw` if draws, `copy`/`clear` not detected in part 3 (O-6), `empty` if neither |
| `draws`, `dispatches`, `indirect` | u32 | from C11–C15 |
| `pipelines` | array of u64 (≤ 8, plain numbers) | telemetry ids of the distinct pipelines bound or dispatched in the pass (first-seen order). `pipeline_ids_truncated: true` when more |
| `barriers_before` | u32 | barriers in the batch that opens this pass (0 for pass 0) |
| `resubmitted` | bool | |
| `why` | string or absent | `resubmitted` (both submissions' in-list stamps are unusable, so pass ticks are null; the plan's rule), `full`, `bundle`, `copy_list`, plus the list's own reasons |

Passes of one list never overlap. Passes on different queues can. A reader takes unions.

#### `pipeline` (tier pass; creation)

`id` u64 (telemetry's sequence), `stage` (`graphics`/`compute`, from `kind`), `records` (array with
the console record hash as a hex string, when `record` is known), `dxil_bytes` u64, `compile_ms`
f64 (driver time), `waited_ms` f64 (compile-slot wait), `source` (`cache`/`dxc`/`direct`, or null),
`blob` bool (a driver blob was supplied). Stand-ins: part 1's refusal `pipeline` event already
carries `standin` (`report.rs:539` `pipeline_fields`). `hresult` is not available at C17, because
`build` returns an opaque `R`. Refusals stay part 1's.

#### `resource` (tier pass; title creations only while a frame is open, i.e. after the first present)

`kind` (`committed`/`placed`/`reserved` + `dim` buffer/texture), `bytes` u64 (from the desc when
readable, else null), `where` = `title` (the console-slot hooks are title-only), `reason` =
`explicit`, `ms` f64 (time in the hook), `frame` u64. `first_read` is **not** emitted in part 3:
no lazy XCL creation site was confirmed (O-5).

#### `io` (tier pass; aggregated)

One event per frame boundary in which requests completed, and at least every 250 ms during long
loads (flushed by the telemetry thread): `requests` u32, `bytes` u64, `ms` f64 (sum of `execute`
durations), `max_ms` f64, `zlib` u32, `frame` u64. That is a per-frame batch, not per DirectStorage
`Submit()` batch (O-3). `queue` is not known at `execute` (it is a free fn), so it is omitted.

---

### 7. Part 3 mechanism: pass stamps

- **Pass heap:** a global `TIMESTAMP` heap of `PASS_SLOTS = 65536` (512 KiB) and a readback buffer
  of the same size, split into blocks of 256 slots. Both are created on the first pass boundary at
  tier pass. One heap type serves direct and compute lists. Copy lists and bundles are skipped (the
  list type is read once per PC list through `orig_fn(list, GetType)` and cached).
- **Recording state** `PassRec { list, blocks: Vec<u16>, used: u16, boundaries: Vec<(slot, barriers)>, counters per pass, pipelines }`
  lives in `HashMap<usize, PassRec>` under a mutex. Draw/dispatch/PSO notes go to a TLS cache
  `(list, counters)` that is flushed into the map at the next boundary or at submit, so there is no
  global lock per draw.
- **Boundary:** take the next slot (allocate a block when needed; at most 16 blocks per recording,
  then `full` with no more stamps), then call `EndQuery(pass_heap, TIMESTAMP, slot)` on the PC list
  through `orig_fn(list, layout::slot(find("ID3D12GraphicsCommandList"), "EndQuery"))`. The list is
  adopted, so its own vtable must not be used. The stamp is written **before** the barrier batch:
  the batch drains anyway, so the stamp adds almost no serialisation.
- **Reset (C16):** drops any `PassRec` of that list whose blocks were never submitted, and returns
  the blocks to the pool.
- **Submit:** `telemetry_execute` removes each list's `PassRec` from the map and moves it into the
  `ListEntry`. Its blocks are added to the current set's `pass_blocks`. The set's resolve list `R`
  then also resolves each used block range into the pass readback. A list submitted again while it
  has no `PassRec` but an earlier entry with unread blocks is a resubmission: both entries' passes
  get null ticks and `why: "resubmitted"`. The list stamps stay valid (A's property).
- **Harvest:** when the set is Ready, the pass ticks are read with the list ticks and written as
  `pass` events. Blocks return to the pool.
- **Pool exhausted:** no stamp, the list's passes get `why: "full"`, and the drop counter is
  incremented.

---

### 8. Constraints and considerations

- **Off:** every site is `telemetry_on(..)` = one relaxed load, or a span `None`. No heap, buffer,
  list, fence, thread or map is created. `telemetry_execute` off is `submit(lists)`.
- **Vendors:** frequency and calibration are read per queue. There are no constants and no vendor
  names. Copy-queue timestamps depend on the cap bit only. `<1000 Hz` or a resolved `0` gives null
  (the #996 rule). NVIDIA (5080, wave32, ~1 GHz reported by others) and AMD RDNA4 run the same code.
  No shaders and no PSOs are created, so there is no risk of driver refusals.
- **No stall:** no `Wait`, no `GetCompletedValue` loop, no `Map` per frame (mapped once), no
  readback in the writing frame. The queue mutex is held only by submitters to that queue, and the
  telemetry thread uses `try_lock`.
- **Bounded memory:** per queue, 4 × (8 KiB query heap + 8 KiB readback + ≤1024 stamp lists).
  `PENDING_FRAMES` 64. `in_flight` map pruned at harvest. Pass pool 512 KiB + 512 KiB. When any
  bound is reached, stamps are not taken, ticks are null with a `why`, and the drop counter grows.
- **Thread safety:** submissions to one queue from several threads are serialised by the queue
  clock mutex across the driver call. The driver serialises queue submission anyway. No re-entry is
  possible: the held region only calls PC original functions. Our own `Signal` goes through
  `orig_fn` too, so `fence_x` and `list_x` never see it.
- **Present ring / compositor:** the present copy (`present.rs:1201`) is a submission like any
  other and shows up as a `list` with `origin: "present"`. It counts in the frame's `gpu_ms`
  (it is real GPU work of the frame). The auto-present path calls `present_now` from `note_execute`
  after the closure (`real.rs:7397`), so no queue mutex is held then.
- **alloc_pool disabled:** the engine does its own resubmission check (`in_flight`), independent
  of `alloc_pool::enabled()`.
- **Multi-queue:** each queue has its own clock, heaps, fence and calibration. `gpu_ms` takes the
  union across queues after calibration. Queues created later get their `clock` event at their
  first submit.
- **`XCL_TELEMETRY_SAMPLE=n`:** unsampled frames get no stamps and no `list`/`pass` events. Their
  `frame` events still come, with `sampled: false` and `gpu_ms: null`. The value goes into the
  manifest (`sample`).
- **Device removed:** a fence value of `u64::MAX` gives `removed`. `write_final` runs
  `telemetry_final` before `TerminateProcess`. The flush only reads CPU state.
- **Title exit:** `telemetry_final` flushes. Frames not harvested by then get `why: "exit"`.
- **Manifest additions** (part 1's struct, fork code): `sample`, `gpu_sets` (4), `list_slots`,
  `pass_slots`, `copy_queue_timestamps`, `gpu_drops` {full, late, unsupported, resubmitted,
  removed, sampled_out}.

---

### 9. D7 overhead measurement

1. **Workload:** the test title's frame scenario (section 10, P2-D):
   `xcl_testtitle --frames 600 --uncapped`. It runs 600 PresentX frames, each with a direct list
   (draws), a compute list (dispatches), a bundle and one copy. The frame clock is uncapped
   (`XCL_FRAME_INTERVAL`-style override 0, `frame.rs:62`), so frames run as fast as XCL allows. The
   title prints its own `frames: N, elapsed_ms: X, p50_ms, p95_ms`, measured with QPC around its
   loop. That makes the measurement independent of telemetry.
2. **Runs:** `off`, `frame`, `pass`, then `frame` with `XCL_TELEMETRY_SAMPLE=10`. 5 runs each,
   interleaved (off, frame, pass, off, …), release build, same session, the window visible.
3. **Reported in the run's notes:** commit, GPU + driver, CPU, the median of `elapsed_ms` per
   mode, p50/p95 frame ms, Δ% against off, the manifest `gpu_drops`, and `events_dropped`. No
   figure is claimed without the run.
4. **Queue serialisation check:** the GPU test P3-E also runs a fixed GPU workload once with and
   once without the engine's interleaved stamps. It measures both with one outer stamp pair
   recorded by the test, and reports the difference. That is the direct answer to "does it
   serialise".
5. **Above 5 %** at tier frame: set the default `XCL_TELEMETRY_SAMPLE` to the smallest n that
   brings it under 5 % on the measured hardware, and record it in the manifest. If even n=10 is
   above 5 %, switch the interleave to Option C (chunk stamps) and measure again. The result goes to
   the orchestrator before part 3 is merged.

---

### 10. Work breakdown

Every runtime line lives in `log.rs` or `report.rs`, so units that write runtime code are split
**by file**. Pure code goes in log.rs, D3D12 and glue in report.rs. The upstream one-line sites are
grouped by file into their own units. No two units in one wave touch the same file.

#### Part 2

| Unit | Files owned | Inputs | Output / acceptance | Tests | Lines |
|---|---|---|---|---|---|
| **P2-A** GPU stamp engine | `src/report.rs` (append) | this spec §4.1–4.4, part 1 API | `DevFns`/`QueueFns`, `QueueClock`, `StampSet`, `telemetry_execute`, harvest, thread, `telemetry_gpu_start/final` (wired into `telemetry_start`/`telemetry_final`, both fork code); off path = passthrough | GPU (skip "no GPU"): 2 lists on a plain device → both have ticks, start<end, end(L0)==start(L1); a copy queue without the cap → `unsupported`; resubmit before read → `resubmitted`, ticks valid; `RemoveDevice` on a private device (ID3D12Device5) → harvest gives `removed` within 1 s, no hang; set full → `full`; read never in the writing frame (assert `frame ≥ set.frame+2` at every read via a debug counter) | ~520 + ~220 tests |
| **P2-B** pure math + fields | `src/log.rs` (append) | §5.1, §6 | `parse_sample`, `sample_due`, `union_ms`, `tick_to_qpc`, `frame_cpu_ms`, `*_fields`, facts structs | unit: union of overlapping/nested/disjoint/empty intervals (never a sum); tick conversion with 1 GHz and 25 MHz and 100 MHz (no fixed frequency), freq <1000 → None, tick 0 → None, i128 no overflow; sample parsing incl. invalid; each `*_fields` → flat JSON, nulls with `why` | ~160 + ~140 tests |
| **P2-C** frame glue + call sites | `src/report.rs` (append, after P2-A), `src/hle/d3d12x/real.rs`, `list_x.rs`, `present.rs`, `frame.rs` (C1–C6 only) | P2-A, P2-B | spans, `FrameRec`, frame/memory/clock emission on the thread; C1–C6 | unit: span with/without `SHOWN`; cpu_ms clamp; frame record completes on harvest or LATE | ~200 + ~80 tests, 6 call lines |
| **P2-D** test title scenario + e2e | `tests/testtitle/src/gpu.rs` (append fn), `tests/testtitle/src/main.rs` (one dispatch line for `--frames`), `crates/xcl/tests/telemetry_e2e.rs` (new, fork-only; O-8) | schema §6 only (parallel with P2-C) | scenario: N frames, direct+compute+copy queue, a bundle executed from the direct list, one list submitted twice before readback | e2e (Test plan row 2): `frame` count == `report.json d3d12.presents`; `memory` `t` increasing; for each frame `gpu_ms` == `union_ms` of its `list` intervals (±1e-6) and null when none; `clock` per queue at start and at frames 60, 120, …; the resubmitted list has `resubmitted: true` (ticks per O-2); no `list` event for the bundle; device-removal case is the P2-A GPU test (an e2e removal needs a title-callable RemoveDevice, see UNCERTAIN); skips cleanly without a GPU | ~180 title + ~200 test |
| **P2-E** D7 run | none (Windows PC; report in run notes) | P2-C, P2-D built | §9 table | — | 0 |

#### Part 3

| Unit | Files owned | Inputs | Output / acceptance | Tests | Lines |
|---|---|---|---|---|---|
| **P3-A** pass engine | `src/report.rs` (append) | §7, P2-A | pass pool, `PassRec`, TLS counters, boundary/reset/draw/dispatch/bind fns, block resolve in `R`, harvest → `pass` | unit: TLS flush across two lists on one thread; reset drops unsubmitted blocks; resubmission poisons both | ~300 + ~120 tests |
| **P3-B** pass call sites | `src/hle/d3d12x/list_x.rs` (C7), `console_list.rs` (C8–C10), `bind.rs` (C11–C15) | P3-A signatures (§5.2) | 9 lines | builds; existing suite unchanged | 9 lines |
| **P3-C** fields | `src/log.rs` (append) | §6 | `pass_fields`, `pipeline_fields_built`, `resource_fields`, `io_fields` | unit per builder | ~90 + ~60 tests |
| **P3-D** pipeline/resource/io glue + sites | `src/report.rs` (append, after P3-A), `src/hle/d3d12x/real.rs` (C16, C17, C21), `src/gpu/recomp/pso.rs` (C18–C20), `src/hle/dstorage.rs` (C22) | P3-A, P3-C | TLS source, pipeline ids, resource/io spans, io aggregation; implementer confirms the disk-cache hit site or writes `source: null` | unit: io aggregation and the 250 ms flush; source precedence (dxc > direct > cache across a pipeline's stages) | ~160 + ~60 tests, 8 lines |
| **P3-E** GPU test (row 3) + serialisation check | `src/report.rs` `#[cfg(test)]` (after P3-D) | P3-A | (1) one dispatch timed through the pass path matches `runner::dispatch_timed` (`runner.rs:1660`) on the same DXIL within max(15 %, 0.05 ms) on medians of 20; (2) a direct and a compute list running concurrently (long dispatches): each pass keeps its own start<end, the intervals overlap, frame `gpu_ms` == union < sum; (3) §9 step 4 numbers printed | skip "no GPU"/"no DXC" via `runner::skip_test` | ~200 |

#### Concurrency plan

| Wave | Units (parallel) | Why it is safe |
|---|---|---|
| 1 | P2-A (report.rs) ‖ P2-B (log.rs) ‖ P2-D (tests/testtitle, tests/telemetry_e2e.rs) | disjoint files. P2-D codes against the schema only |
| 2 | P2-C (report.rs + 4 upstream files) | needs A+B. Sole owner of report.rs in this wave |
| 3 | P2-E (run) ; then orchestrator gate on D7 | |
| 4 | P3-A (report.rs) ‖ P3-B (list_x.rs, console_list.rs, bind.rs) ‖ P3-C (log.rs) | disjoint files. P3-B uses the §5.2 signatures, and the integration build waits for P3-A |
| 5 | P3-D (report.rs, real.rs, pso.rs, dstorage.rs) | P3-B does not touch real.rs. Sole report.rs owner |
| 6 | P3-E (report.rs tests) ; full suite + D7 re-run at tier pass | |

Totals (runtime / tests): **part 2 ≈ 880 runtime + 6 call lines, ≈ 640 tests + 180 test title**
(the plan said ~250). **Part 3 ≈ 550 runtime + 17 call lines, ≈ 440 tests.** Every unit's check:
orchestrator build on the Windows PC and the whole suite (`cargo test --workspace -- --test-threads=4`),
compared with the part-1 baseline. Report SKIPPED lines.

---

### 11. Alternatives rejected

- **Option B** (stamps in the recording): more upstream hooks (5–7 sites across recording and close
  paths), slots that are lost on resubmission, a resolve per list (#1766-like drain), and no
  coverage of XCL-internal lists.
- **Option C as the default:** coarser than the plan's per-list event. Kept as the D7 fallback.
- **Adopting upstream #1766:** a per-op resolve serialises the queue, one global frequency, slots
  taken at recording. That fails 5.4 and D7.
- **A resolve inside each trailing stamp list:** a drain after every list. Rejected for one
  resolve per queue per frame.
- **Recording stamp lists per submission:** costs CPU for Reset+EndQuery+Close per stamp. Rejected
  for pre-recorded stamp lists reused after the fence.
- **Harvesting on the present thread:** JSON and DXGI calls would land on the frame. Rejected for
  one telemetry thread, created only when the tier is ≥ frame.
- **Creating telemetry objects through the hooked device:** they would be adopted and enter XCL's
  list/queue bookkeeping, and the console-layout queue vtable cannot be called for
  frequency/calibration. Rejected for `orig_fn` creation.

---

### 12. Open decisions for the user

- **O-1 (D6):** about 1,100 lines of runtime code, including about 700 of D3D12 query machinery,
  in `report.rs`. The default keeps D6 literally. The alternative (c), a fork-only file included by
  a `#[path]` `mod` from report.rs's appended block, costs **zero** upstream lines but adds a file.
  (b), a `lib.rs` module, costs one upstream line. Recommendation: (a) unless the user accepts (c).
- **O-2 (changes the plan's rule):** under Option A, a resubmitted list's **list** ticks are valid,
  because each submission has its own stamps. The plan and Test row 2 say "null ticks +
  `resubmitted: true`". Proposal: keep the list ticks and set the flag; apply null ticks to
  **pass** ticks, where the plan's reason still holds. If the user wants the plan literally, null
  the list ticks too. That is one line, and loses real data.
- **O-3:** `io` per frame (aggregated) instead of per DirectStorage batch: there is no single
  batch-completion site that sees bytes and time on both worker paths.
- **O-4:** grouping segments into title lists would need `title_list::wrapper_of`
  (`title_list.rs:265`) changed from `pub(super)` to `pub(crate)`. That is one token in an upstream
  line. Default: not done; `list.id` is the PC list.
- **O-5:** `resource.reason: first_read` is not emitted. No lazy XCL creation site was confirmed
  in this survey. It needs a separate look.
- **O-6:** pass `kind` `copy`/`clear` needs call sites in copy/clear handlers (more lines in
  `list_x.rs`). Default: not in part 3.
- **O-7:** `list.translate_us`/`console_bytes`: one more site in `console_list::close`. Default: null.
- **O-8:** new fork-only test file `crates/xcl/tests/telemetry_e2e.rs`, plus test title changes in
  `tests/testtitle` (upstream files: one appended fn plus one dispatch line). D6 names runtime code,
  so this is listed for clarity.
- **O-9:** the part 2 estimate is about 3.5× the plan's 250 lines. Parts 2+3 together are about
  1,430 runtime lines.
- **O-10:** a telemetry thread when on. The plan says only that off creates no thread.

No new dependency. No build-time code. Fork-only (D8).

## 2026-11-26 Telemetry parts 5 and 6: no-window switches, error and phase events, loop budget and counters, events reader (user decided "everything before the deep dive"; orchestrator approved the design)

Designed by an Opus worker against worktree xcl-adopt at 21890e09 with part 1 applied; citations spot-checked by the orchestrator (present.rs:1066-1084 present_now rate limit / ensure_window / prepare_pipelined_slot, :1237-1243 accounting tail, :480 and :503 window size; config.rs:253 raytracing_tier_override; dxil/lower.rs:36 LOOP_BUDGET, :114-116 first; recomp/mod.rs:659 loop_budget). Paths below are relative to crates/xcl/src/ unless they say otherwise.

### Orchestrator decisions on parts 5 and 6

- File placement (extends the user's 2026-11-26 decision for the GPU timing code to part 5): every "appended to report.rs, section 5a/5b/5c/5e" block goes in a fork-only file `crates/xcl/src/telemetry_run.rs` instead, pulled in from report.rs's fork block by `#[path = "telemetry_run.rs"] mod run_tel;` and `pub use run_tel::*;`, so call sites keep the paths `crate::report::...`. The edits section 5b makes to part 1's own functions and to `Manifest` stay in report.rs.
- OD1 headless pacing: (a) plain option A, with the measured check; (b) only if the check shows a difference.
- OD2: the fork-only test file crates/xcl/tests/telemetry_e2e.rs is accepted (part 2's unit P2-D creates it; part 5 tests are added to it afterwards by one tests unit).
- OD3: add the kind `load_failed` (fatal); `stub_called_fatal` stays reserved with no producer.
- OD4: as proposed. XCL_RUN_SECONDS, title exit and window close are not errors (`end` only); XCL_STALL_DUMP firing is `timeout`, fatal.
- OD5: the user asked for everything, so 5e is built; the spike 5e-0 still comes first because two premises are unverified (window 0 CPU visibility, the pipeline pointer `dred::label` sees under pso_async). Counters need tier `all`.
- OD6 (io slot 120, 1 MiB region in window 0) and OD7 (`XCL_PHASE_FILE`, default `<logs_dir>/phase`): accepted.
- Units: U5a owns present.rs, config.rs and creates telemetry_run.rs with section 5a; its e2e tests wait for the tests unit. U5d and U6 as written. U5b and U5c append to telemetry_run.rs after U5a; the include lines and the report.rs edits are made by the wiring unit that owns report.rs after part 4's writer and part 2's glue.
- The "smoke pair" of CLAUDE.md is an upstream-PR requirement; for this fork-only work the regression check per unit is the whole suite on the Windows PC, plus one smoke comparison of the finished branch against dev on the title at the end.
- Check for every unit: the orchestrator builds on the Windows PC and runs the whole suite in the desktop session; baseline with 15 adopted PRs + part 1 (a16cc528): 1494 passed, 4 failed (gpu::dxil::fuzz_tests::fuzz_validate_and_gpu_0..3, NVIDIA 0x8007000E), 47 ignored, 44 SKIPPED lines.

Tree: worktree `xcl-adopt`, branch `adopt/diagnostics-2`, HEAD `21890e09`, with part 1 in the index
(not committed). Every `path:line` below is in that tree, `crates/xcl/src/` omitted for Rust files.
Nothing was built or run: this is macOS and the crate builds on Windows only. Every behaviour claim
about existing code was read from source, not observed.

Binding inputs: plan `docs/guide/telemetry.md` (5.7, 5.8, 5.9, 5.11, D1-D8), part 1 as built
(`log.rs:264-480`, `report.rs:245-712`), the survey (`survey-shader.md`), and the orchestrator's four
decisions (headless = option A, frame dumps dropped in headless; `XCL_LOOP_BUDGET` kept, direct DXIL
side added under it; `smoke.ps1` not edited, fork reader under `scripts/dev/`; `XCL_PHASE_MARK`
redesigned so a script can drive it while the title runs).

### 0. Survey claims re-checked against this tree

| Survey says | This tree |
|---|---|
| `ensure_window()` only from `present_now`, `present.rs:1078` | yes: `present.rs:1078` (`present_now`); `ensure_window` defined `present.rs:553` |
| window size read only in `window_thread` | yes: `present.rs:480` (RECT) and `:503` (log line), `window_thread` |
| swap chain sized to the title's plane, not the window | yes: `(w, h)` from the resolved descs `present.rs:1070-1076`, passed to `ensure_swapchain` (`present.rs:962`) |
| compositor built lazily after the swap chain | yes: `present.rs:1105-1107` (`present_now`), only when `resolved.len() > 1` |
| `present = "off"` returns before frames are counted | yes: `enabled()` `present.rs:194`; `present_x_frame` returns at `present.rs:141` before `present_now`; `PRESENTS` only incremented at `present.rs:1237` |
| `XCL_STALL_DUMP` and `XCL_RUN_SECONDS` read `presents()` / `counters()` | yes: `host.rs:154,158` (stall thread), `host.rs:183-184` (`XCL_RUN_SECONDS`) |
| kbm needs the window | yes: `kbm::attach` `present.rs:504`; with no window `kbm.rs:397` treats focus as lost (neutral input) |
| `XCL_PRESENT_SYNC` | `present_slot_count` `present.rs:623-629`, used by `new_presenter` `present.rs:942` and the ring |
| #1284 HLSL only | yes: `recomp/mod.rs:659` `loop_budget()`, prelude swap `recomp/emit.rs:4853-4858`; `cache_allowed` lists `XCL_LOOP_BUDGET` `recomp/pso.rs:907-908`; direct path still `dxil/lower.rs:36` `LOOP_BUDGET` used at `dxil/lower.rs:116` (`first`) |
| direct guard sites | `dxil/kernel.rs:360-370` (plain form) and `dxil/compact.rs:302-316` (compact form) both read `VAR_BUDGET`, whose start value is `first()` `dxil/lower.rs:115-116`; the per-lane bridge `recomp/lower_dxil.rs` builds on the same `Lw` |
| `smoke.ps1` regexes | `scripts/smoke.ps1:76-82`, report `:85-90`; unchanged |
| #282 assertion stale | confirmed: `scripts/smoke.tests.ps1` calls `smoke.ps1` without `-Title` yet asserts `title: SomeTitle`; `smoke.ps1:21-30` hashes the folder name |
| E1 sources | `log_device_removed` `hle/d3d12x/real.rs:7500`; `stop_if_device_removed` `:7548`; `end_title` `:3594`; `unhandled_filter` `eh.rs:64`; `hk_nt_terminate_process` `eh.rs:127` |
| #967 (`XCL_CRASH_EXIT_SECONDS`) | **not in this tree** (grep finds nothing). A crash whose title filter hangs does not end on its own; see 5b failure modes |

New finding (not in the survey): `new_presenter` (`present.rs:938`) runs before the window and
creates only an allocator, a list, a fence and an event. Its `last` timestamp (`present.rs:958`) is
what rate-limits the non-PresentX fallback (`present.rs:1067`, `min_interval` 15 ms / 4 ms from
`note_execute` `present.rs:342` and `on_queue_console_method` `present.rs:353`). Headless must keep
that rate limit, so its early return goes after `present.rs:1067`, not before `new_presenter`.

### 1. Cross-cutting rules for all of part 5

- **D6 placement.** New logic is appended to `report.rs` (after its last test module, which ends at
  `report.rs:889`). `log.rs` gets nothing new in part 5: part 1's `event`, `telemetry_on`, `sampled`,
  `run_dir` (`log.rs:316-457`) are enough. Each part-5 unit appends one section
  `// ---- fork telemetry (docs/guide/telemetry.md, part 5x) ----` with its own
  `#[cfg(test)] mod telemetry_p5x_tests`.
- **Off cost.** Every runtime call site starts with one relaxed atomic load: `xlog::telemetry_on(..)`
  (`log.rs:316`) for events, or a new `static AtomicU8` mode word for the non-telemetry switches
  (headless, skip, counters). No env read, lock or allocation on the off path.
- **Error events never block a dying thread.** All writes go through part 1's `Events::write`, whose
  lock wait is bounded at 100 ms and drops the event when exceeded (`log.rs:383-399`). Part 5 adds no
  other lock on the crash path; it never calls `update_manifest` (`report.rs:375`, 200 ms bounded)
  from `unhandled_filter` or `hk_nt_terminate_process`.
- **Diagnostic runs are labelled, never compared with normal runs.** 5b adds manifest fields
  `diagnostic` (list) and `compare_key` (string, `""` for a normal run). Every switch that changes
  generated code or presentation lands there: `XCL_LOOP_BUDGET`, `XCL_LOOP_COUNTERS`,
  `XCL_HEADLESS`, `XCL_PRESENT_SKIP`, `XCL_WINDOW_SIZE`, `XCL_FRAME_INTERVAL_US`, `XCL_PRESENT_SYNC`,
  `XCL_WAVE64_SPIN`, `XCL_X64_ALL_MASKS`, `XCL_SPLIT_SCRATCH_MB`, `XCL_RECOMP_NO_STRUCTURED`,
  `XCL_DXIL_NO_OPT`. The Part 6 reader prints `compare_key` on stderr and refuses (exit 2) to put two
  runs side by side with different keys. That does not apply to `smoke.ps1`, which knows nothing
  of it.
- **Both vendors, no vendor names.** Behaviour forks only on measured capabilities: the manifest's
  `wave_lane_count_min/max` (`report.rs:386` `device_facts`) and the existing split-wave decision.
  The new manifest field `split_wave64 = (wave_lane_count_max == 32)` is set in `telemetry_device`
  (`report.rs:416`).

---

### 5a. No-window switches: `XCL_HEADLESS`, `XCL_PRESENT_SKIP`, `XCL_WINDOW_SIZE`

#### Goal and scope
In: run a title with no window and no swap chain while frames are still counted (headless). Keep the
window but skip the copy and the `Present` (skip). Override the window size from the environment.
Out: frame dumps in headless or skip (decision 1); option B (keeping `Presenter` and readback copies);
##1761 splash and #2555 preparation (not adopted); any change to the frame clock (D5).

#### Architecture
Both bypasses share one exit: "count this frame as shown, then return". It sits at two points in
`present_now` (`present.rs:1017`):
1. **headless:** after `swap_format` (`present.rs:1077`) and before `ensure_window()` (`present.rs:1078`).
   No window thread, no swap chain, no compositor, no command list recorded or executed.
2. **skip:** after `ensure_window()` and the queue check (`present.rs:1079-1081`) and before
   `prepare_pipelined_slot` (`present.rs:1083`). The window exists and kbm works. Nothing is copied
   or presented, so the window shows its class background.

The accounting tail of `present_now` (`present.rs:1237-1243`: `PRESENTS += 1`, the `pso_manifest`
milestone, the first-frame log line) moves into a private `fn frame_shown(w: u32, h: u32)` in
`present.rs`, called from the old tail and from both bypasses. One copy of the accounting means
headless frames feed `pso_manifest::present_milestone` (`present.rs:1238`), `XCL_STALL_DUMP`, the
`XCL_RUN_SECONDS` summary and smoke's "frames shown" exactly as windowed frames do. The log text
`present: first game frame shown ({w}x{h})` stays byte-identical (scripts grep it).

`(w, h)` in headless is the plane size (`present.rs:1070-1076`), the same figure the windowed run
logs.

Why not reuse `present = "off"`: it returns at `present.rs:141` before `PRESENTED` is updated and
before `present_now`, so `PRESENTS` stays 0. `XCL_STALL_DUMP` would then never arm (`host.rs:241`
`stalled` needs frames) and smoke's frame line would read 0.

#### Signatures (appended to `report.rs`, section 5a)
```rust
/// 0 window (default), 1 headless, 2 skip; 0xFF = not read yet. Read once from the environment.
static PRESENT_MODE: AtomicU8 = AtomicU8::new(0xFF);
/// `XCL_HEADLESS=1` / `XCL_PRESENT_SKIP=1` (both set: headless wins, one warning).
/// `after_window == false` asks "headless?", `true` asks "skip?". One relaxed load once read.
pub fn present_bypass(after_window: bool) -> bool;
/// pure: (headless value, skip value) -> mode; "1" means on, anything else off (tested)
fn parse_present_mode(headless: Option<&str>, skip: Option<&str>) -> (u8, Option<String>);
/// "window" | "headless" | "skip", for the manifest (5b)
pub fn present_mode_name() -> &'static str;
```
`present_bypass` logs once, on its first `true`: `present: XCL_HEADLESS – no window, frames are
counted, not drawn` (or the skip text). When `XCL_FRAME_DUMP` is set under either mode it also logs
`XCL_FRAME_DUMP is ignored`.

`config.rs`, next to `raytracing_tier_override` (`config.rs:253`):
```rust
/// `XCL_WINDOW_SIZE=<w>x<h>` (1..=16384 each, `x` any case) overrides window_width/height;
/// an invalid value warns once and keeps xcl.toml's.
pub fn window_size(&self) -> (u32, u32);
pub fn parse_window_size(v: &str) -> Option<(u32, u32)>;
```

#### Call sites and minimal edits (each justified)
| Where | Edit | Lines |
|---|---|---|
| `present.rs:1077/1078` `present_now` | insert `if crate::report::present_bypass(false) { p.last = Instant::now(); return frame_shown(w, h); }` | +1. Not telemetry-only by nature: it *is* the switch. `p.last` keeps the fallback's rate limit (`present.rs:1067`) |
| `present.rs:1081/1082` `present_now` | the same line with `present_bypass(true)` | +1 |
| `present.rs:1237-1243` `present_now` | the tail becomes `frame_shown(w, h);` with the 7 lines moved into `fn frame_shown` | about +9 / −6. A move with no logic change: the alternative is three copies of the accounting |
| `present.rs:480` `window_thread` | `let (ww, wh) = cfg.window_size();` then `right: ww as i32, bottom: wh as i32` | +1 / ~1 |
| `present.rs:503` `window_thread` | log `ww, wh` instead of `cfg.window_width, cfg.window_height` | ~1 |
| `config.rs` after `:257` | `window_size` + `parse_window_size` (style of `raytracing_tier_override`) | about +18, plus a test in `config.rs`'s test module about +12 |

`present.rs` in all: about 14 changed lines, all inside `present_now` / `window_thread` plus one
new 8-line function.

#### What happens to …
- **Frame clock (D5):** untouched. `present_x` still calls `frame::mark_flipped(token)`
  (`present.rs:63`) after `present_x_frame`, so the title's flip statistics and 30 fps deadlines
  behave as windowed.
- **Run end without a window:** `XCL_RUN_SECONDS` (`host.rs:179-189`) and `XCL_STALL_DUMP`
  (`host.rs:148-176`) both work, because `presents()` advances. `WM_CLOSE` (`present.rs:366`) cannot
  happen. With neither variable set, a headless run ends only when the title exits or crashes (or is
  killed). The 5a e2e test therefore runs the test title, which ends on its own.
- **Keyboard/mouse as gamepad:** headless has no window, so `kbm::attach` never runs and
  `kbm.rs:397` sees no focused window: kbm input stays neutral. A real controller (GameInput) still
  works. Under skip the window exists: kbm works while it has the focus, as today.
- **`XCL_PRESENT_SYNC`:** under both modes the return comes before `prepare_pipelined_slot`
  (`present.rs:1083`), so the present ring and its N-2 fence wait never run. The variable has no
  effect, and the manifest records it in `diagnostic`. See open decision OD1 (pacing).
- **Frame dumps / `last_frame_dump`:** never taken; `report.json`'s `d3d12.last_frame_dump` stays
  null (`real.rs:404`).
- **Window title / `show_compiling`:** headless has no window, so no compile-progress title. The
  progress counters themselves are unaffected.

#### Failure modes and considerations
- **Lost back-pressure (main risk).** Windowed, `prepare_pipelined_slot` waits up to 2 s for frame
  N-2's copy on the title's queue (`present.rs:640-645`), which bounds how far the CPU runs ahead of
  the GPU. Headless removes that wait. A title that relies on its own fences is unaffected; one that
  does not could queue more GPU work, raise commit, and show different frame times. The plan's
  "headless pass times vs windowed" check (test 5) has to look at commit and `frame.wait_ms` too, not
  only pass times. OD1 offers a pacing-only variant.
- **Clocks when nothing is presented:** unmeasured on either vendor (survey §5). Headless numbers
  are comparable only with headless runs: `compare_key` contains `XCL_HEADLESS=1`.
- **`XCL_WINDOW_SIZE=1x1`:** `CreateWindowExW` with `WS_OVERLAPPEDWINDOW` (`present.rs:486`) is
  likely clamped to the system minimum tracking size, so the client area will not be 1x1
  (unverified). The test asserts what XCL asked for (the log line), not the client rectangle. Since
  the swap chain follows the plane (`present.rs:1070-1076`), the title cannot see the window size
  through this code. Test 5's 1x1-vs-default comparison is a confirmation, not a risk gate.
- PresentX on a second queue (`present.rs:1053-1061`) still rebuilds the presenter in headless. That
  is harmless: no swap chain is created.

#### Tests (Test plan row 5, no-window part)
- unit (`report.rs` 5a tests): `parse_present_mode` truth table, including both set and garbage values.
- unit (`config.rs` tests): `parse_window_size` for `1280x720`, `1X1`, `0x5`, `x720`, `16385x1`,
  ` 640 x 480 ` (trim policy: trim the whole value only; inner spaces rejected).
- e2e (`crates/xcl/tests/telemetry_e2e.rs`, see OD2): the test title with `XCL_HEADLESS=1` completes
  (`xcl_testtitle: done, failures: 0`, exit 0). Its log has `present: first game frame shown` and has
  no `XCL window created`. With `XCL_TELEMETRY=frame` the manifest says `present_mode: "headless"`.
- e2e: `XCL_WINDOW_SIZE=320x200`: the log contains `XCL window created (320x200)`.
- e2e: `XCL_PRESENT_SKIP=1`: window created, first frame logged, exit 0.
- Not automated: pass times at `1x1` against the default (needs part 3 events and a title; a manual
  run, recorded in the run notes).

#### Estimate
Runtime about 45 lines (present.rs 14, config.rs 18, report.rs 13); tests about 85; total about 130.

---

### 5b. Error events: one `error` channel, one `end`, one `exit`

#### Goal and scope
In: every failure that already prints a prose line also writes an `error` event with a `kind`.
Exactly one `fatal: true` per run that a failure ended. A single `end` event names why the run ended.
An `exit` event carries the process status. Manifest additions for diagnostic labelling. An
`activity` event that mirrors the `XCL_ACTIVITY` lines; Part 6 needs it, and it lives here because 5b
already owns `host.rs`.
Out: reimplementing DRED (we hook the existing readout in `log_device_removed`); #967; debug-layer
messages (#2039, not adopted); new crash detection.

#### Kinds: producers, fatality, fields
Common fields of every `error` event (part 1's `io_failed_fields` `report.rs:567` already uses this
shape): `kind`, `source`, `detail`, `hresult` (`"0x8007000E"` or absent), `fatal` (bool), plus
`after_fatal: true` when it follows the fatal one, plus `n` when the kind is sampled (`log.rs:449`
`sampled`).

| kind | producer (existing prose line) | fatal | extra fields | sampled |
|---|---|---|---|---|
| `device_removed` | `log_device_removed` `real.rs:7508` (once, `DONE` latch `:7505`) | **yes** | `message` | no (once) |
| `gpu_fault` | DRED page fault `real.rs:7524`; DRED per-list line `real.rs:7539` (same fn) | no | `what` (`page_fault`/`breadcrumb`), `va`, `window`, `allocations`, `freed` / `list`, `done`, `count`, `bound` | no (bounded by DRED: ≤ 64 lists) |
| `pipeline_rejected` | part 1's `telemetry_pipeline_refused` `report.rs:458` (from `note_fallback` `real.rs:3759`) | no | `stage`, `standin` | yes (shares part 1's counter and `n`) |
| `shader_refused` | part 1's `telemetry_shader_refused` `report.rs:448` (from `note_translation_error` `real.rs:3869`) | no | `stage`, `reason` | yes (same) |
| `unresolved_import` | `resolve_import` `resolver.rs:321` when `source == Source::Stub` | no | `dll`, `function`, `importer`, `delay` | yes |
| `vfs_unmapped` | `[VFS] unmapped console path` `vfs.rs:284` (already capped at 1000 by the set) | no | `path` | no |
| `io_failed` | exists: part 1 `telemetry_io_failed` `report.rs:492` | no | `path`, `bytes` | yes |
| `title_exception` | `unhandled_filter` `eh.rs:107-111` | **yes** | `code`, `at` (`describe_address`, module+RVA), `message` (the prose `msg`) | no |
| `timeout` | `XCL_STALL_DUMP` fired, part 1's `telemetry_stall` `report.rs:468` (call `host.rs:168`) | **yes** | `last_frame`, `idle_s`, `limit_s` | no |
| `loop_guard_tripped` | 5e only: first trip of a (pipeline, stage) | no | `record`, `stage` | no (once per slot) |
| `stub_called_fatal` | **no producer exists**: a stub always returns its configured value (`stubs.rs:62-66` `stub_report`); nothing marks one as fatal | — | — | reserved; OD3 |

**`pipeline_rejected` / `shader_refused`:** part 1 already writes `ev: "pipeline"` and
`ev: "shader_refused"`. 5b makes the same two functions also write the `error` event, with the same
`n`. Volume stays bounded by `sampled`, and no call site is added. A reader of `ev == "error"` then
sees every failure.

#### `end` and `exit`
- `end` `{reason, code?, frames_shown, presentx, fatal_kind?}`, written **once** (latch
  `static END: AtomicBool`). `frames_shown, presentx` come from `present::counters()`
  (`present.rs:190`). Reasons and where they are set:
  `run_seconds` (`host.rs:184`), `stall` (inside `telemetry_stall`), `device_removed` (inside the
  device-removed function), `title_exception` (inside the exception function), `window_closed`
  (`present.rs:367`), `entry_returned` (`host.rs:228`), `load_failed` (`host.rs:220`).
- `exit` `{status}`, written from `hk_nt_terminate_process` (`eh.rs:127-139`) inside its `ENDING`
  latch, so it is written once per process, always. When no `end` exists yet it first writes
  `end{reason:"process_exit", code:status}` (a title calling `ExitProcess` itself).
- A run killed from outside (`taskkill /F`) has neither event. The reader prints that run as
  "killed".

**Exactly one `fatal: true`:** `static FATAL: AtomicBool`. `telemetry_error(.., fatal: true, ..)`
does `FATAL.swap(true)`: the first caller writes `fatal: true` and also sets `end`; a later fatal
candidate is written with `fatal: false, after_fatal: true`. Example: a device removal followed by
the crash it caused gives `device_removed` fatal, and `title_exception` with `after_fatal`. Runs
ended by `run_seconds`, `entry_returned` or `window_closed` have **no** fatal error. The invariant a
reader can check: `end.reason ∈ {device_removed, title_exception, stall, load_failed}` ⇔ exactly one
`error` with `fatal: true`, and its kind is `end.fatal_kind`.

#### Signatures (appended to `report.rs`, section 5b)
```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ErrKind { DeviceRemoved, GpuFault, PipelineRejected, ShaderRefused, UnresolvedImport,
                   StubCalledFatal, VfsUnmapped, IoFailed, TitleException, LoopGuardTripped, Timeout, LoadFailed /* OD3 */ }
impl ErrKind { pub fn name(self) -> &'static str; }
/// pure (tested): the field list of one error event
fn error_fields(kind: ErrKind, source: &str, detail: &str, hresult: Option<i32>, fatal: bool,
                after_fatal: bool, extra: &[(&'static str, serde_json::Value)]) -> Vec<(&'static str, serde_json::Value)>;
/// writes `error` (tier frame); latches FATAL; a fatal one also writes `end`
pub fn telemetry_error(kind: ErrKind, source: &str, detail: &str, hresult: Option<i32>, fatal: bool,
                       extra: &[(&'static str, serde_json::Value)]);
pub fn telemetry_device_removed(hr: i32, message: &str);
pub fn telemetry_gpu_fault(va: u64, window: &str, allocations: &str, freed: &str);
pub fn telemetry_gpu_fault_list(list: usize, done: usize, count: usize, bound: &str);
pub fn telemetry_title_exception(code: u32, at: usize, message: &str);
pub fn telemetry_unresolved_import(source: &crate::resolver::Source, dll: &str, function: &str, importer: &str, delay: bool);
pub fn telemetry_vfs_unmapped(path: &str);
pub fn telemetry_load_failed(message: &str);
pub fn telemetry_end(reason: &str, code: Option<i64>);
pub fn telemetry_exit(status: i32);
/// Part 6 support: one per XCL_ACTIVITY iteration
pub fn telemetry_activity(presents_2s: u64, committed_mb: u64);
/// pure (tested): ("XCL_HEADLESS=1", …) sorted, and the compare key
fn diagnostic_switches(vars: &BTreeMap<String, String>) -> (Vec<String>, String);
```
Manifest (`report.rs:253` `struct Manifest`), new fields, all `#[serde(default)]`-safe:
`present_mode` (`window|headless|skip`), `window_size` (`"1280x720"`), `loop_budget` (u32,
`recomp::loop_budget()`), `loop_counters` (bool), `split_wave64` (`Option<bool>`, set in
`telemetry_device`), `diagnostic` (`Vec<String>`), `compare_key` (`String`). They are set in
`new_manifest` (`report.rs:336`) from `switches` (part 1's `xcl_switches`, `report.rs:584`). The
schema stays 1: fields are only added (plan 5.3 rule).

#### Call sites (one line each)
| # | Where (fn) | Line inserted |
|---|---|---|
| 1 | `real.rs:7508`/`7509` `log_device_removed` | `crate::report::telemetry_device_removed(e.code().0, &e.message().to_string());` |
| 2 | `real.rs:7524`/`7525` `log_device_removed` | `crate::report::telemetry_gpu_fault(va, &window, &names(pf.pHeadExistingAllocationNode), &names(pf.pHeadRecentFreedAllocationNode));` (the `names` closure captures nothing, so calling it twice is fine; it walks at most 8 nodes) |
| 3 | `real.rs:7539`/`7540` (closure in `log_device_removed`) | `crate::report::telemetry_gpu_fault_list(list as usize, done, count, &bound);` |
| 4 | `eh.rs:110`/`111` `unhandled_filter` (inside the `if` that builds `msg`) | `crate::report::telemetry_title_exception(rec.ExceptionCode as u32, addr, &msg);` |
| 5 | `eh.rs:137`/`138` `hk_nt_terminate_process` (inside the `ENDING` block, before `log::flush`) | `crate::report::telemetry_exit(status);` |
| 6 | `resolver.rs:321`/`322` `resolve_import` | `crate::report::telemetry_unresolved_import(&source, &key, &fname(what), importer, delay);` (before `key` is moved into `RECORDS`) |
| 7 | `vfs.rs:284`/`285` (`Vfs` path mapping, inside the insert branch) | `crate::report::telemetry_vfs_unmapped(&p);` |
| 8 | `host.rs:184`/`185` (`XCL_RUN_SECONDS` thread) | `crate::report::telemetry_end("run_seconds", Some(0));` |
| 9 | `host.rs:220`/`221` `host_main` | `crate::report::telemetry_load_failed(&e);` |
| 10 | `host.rs:228`/`229` `host_main` | `crate::report::telemetry_end("entry_returned", Some(code as i64));` |
| 11 | `host.rs:115`/`116` (`XCL_ACTIVITY` thread, after the `memory:` line ends) | `crate::report::telemetry_activity(d[0], mb(private as u64));` |
| 12 | `present.rs:367`/`368` `wndproc` `WM_CLOSE` | `crate::report::telemetry_end("window_closed", Some(0));` |

No new site for `timeout`, `pipeline_rejected`, `shader_refused` or `io_failed`: they change inside
part 1's functions in `report.rs`. Each line costs one relaxed load when off. Every function starts
with `if !xlog::telemetry_on(Tier::Frame) { return; }`.

`telemetry_unresolved_import` is called for every import (thousands at load) but returns after one
compare when `source != Stub` or telemetry is off. Telemetry starts before imports resolve:
`telemetry_start` runs at `host.rs:39`, `load_main` at `host.rs:217`.

#### Crash-path constraints
- `unhandled_filter` (`eh.rs:64`) already allocates and takes the log lock (`xlog_error!`, then
  `write_final(Some)`). The event adds one `format!` and one bounded `Events::write`: the same risk
  class, no new unbounded wait.
- `hk_nt_terminate_process` runs inside `end_title` **after** `suspend_other_threads`
  (`real.rs:3609`). A suspended thread may hold the `Events` file mutex: the 100 ms bound then
  drops `exit`, and `events_dropped` counts it. The run-seconds and stall paths write their `end`
  **before** `end_title`, so `end` survives. The device-removed path writes `end` from
  `log_device_removed` before `TerminateProcess` (`real.rs:7552-7557`).
- No `update_manifest` on any of these paths (it does disk I/O). `ended`/`fatal_kind` live in
  events only.
- The crash text can contain any bytes: `serde_json` escaping (`log.rs:424` `event_body`) keeps one
  physical line.

#### Failure modes
- A title filter that returns `EXCEPTION_CONTINUE_EXECUTION` (`eh.rs:115-118`) survives an
  "unhandled" exception that we have marked fatal. Then `end` is already set while the run goes on.
  It is rare, and the event's `detail` records it. Without #967 a hung title filter keeps the
  process alive; under headless with no `XCL_RUN_SECONDS` nothing ends it. Recommendation: always
  set `XCL_RUN_SECONDS` for unattended runs (documented in the README section the part adds).
- `log_device_removed` is called from `unhandled_filter` on every crash. It only reaches our line on
  a real removal (`real.rs:7504` returns otherwise).
- Hard kill: no `end`. The reader prints "killed".

#### Tests (Test plan row 5, errors part)
- unit: `error_fields` for every `ErrKind`: keys present, `hresult` formatted `0x%08X`,
  `after_fatal` only when given; `ErrKind::name` is unique and snake_case.
- unit: the fatal latch. A pure helper `fatal_decision(&AtomicBool, wanted) -> (fatal, after)` is
  driven twice and gives `(true,false)` then `(false,true)`.
- unit: `diagnostic_switches` over a synthetic map: order-independent, and normal → `""`.
- e2e (`telemetry_e2e.rs`): the test title with `XCL_TELEMETRY=frame`. Exactly one `error`
  `unresolved_import` with `function == "XclDefinitelyMissingFunction"` (the import the existing e2e
  relies on, `crates/xcl/tests/e2e.rs:187-191`) and `fatal: false`. Exactly one `end` with
  `reason: "entry_returned"`. No event with `fatal: true`. One `exit` with `status: 0`.
- e2e: the same run with `XCL_ACTIVITY=1`: the number of `activity` events equals the number of
  `activity (2 s):` log lines (±1 for the last iteration, see Part 6). Each event's `presents_2s`
  equals its line's figure.
- Not automated: a forced device removal or title crash (there is no safe way in CI). A manual check
  is listed under "Not tested".

#### Estimate
`report.rs` about 190 runtime + 120 test lines; 12 call-site lines; e2e about 60. Total about 380.

---

### 5c. Phase detection: automatic guesses plus marks a script can drive

#### Goal and scope
In: `phase` events `{name, from, confidence, source, frames}` from (a) a heuristic over counters XCL
already keeps, (b) a schedule given at launch, (c) a control file a script writes while the title
runs.
Out: making phase load-bearing for anything else; `cutscene` detection without a mark (no reliable
signal exists; see below); #2637 input-rule events.

#### Options for driving marks while the title runs
| Option | How | Pros | Cons |
|---|---|---|---|
| A. control file | XCL polls a file every 250 ms; a new content is a mark | works over SSH (`echo world > …\phase`); any timing; no IPC | 250 ms latency; needs a path known before launch |
| B. schedule `XCL_PHASE_MARK=t:name,…` | seconds since start, like `XCL_THREAD_DUMP_AT` (`host.rs:132-143`) | zero interaction; reproducible | the script must know the timing in advance; drifts with load times |
| C. named pipe / socket | a listener thread | instant | new IPC surface, security review, more code; overkill |
| D. keyboard hotkey in the XCL window | `wndproc` | interactive | no window in headless; an upstream file edit; not scriptable |

**Recommendation: A + B, both in one poll thread.** A is what the SSH-driven workflow needs. B keeps
the variable name from D4 and covers fully scripted runs. A bare `XCL_PHASE_MARK=<name>` (no `t:`)
is accepted as `0:<name>`, the original D4 form: it labels the whole run.

The control file path is `XCL_PHASE_FILE` if set, else `<logs_dir>/phase` (`Boot::logs_dir`, the
folder of `xcl.log`). That folder is known **before** launch, while the run folder's name is not:
it contains the start time (`log.rs:327`). At start the poll thread records the file's current
mtime and content and ignores them, so a stale file from an earlier run does not mark. An empty
file or the word `auto` ends the mark. Scripts should write `phase.tmp` and rename it; a torn read
is ignored until the next poll and warned once.

#### Heuristic (never load-bearing; every event carries `confidence`)
Inputs are polled from existing public counters every 250 ms, in 1 s windows. **There is no call site
in any upstream file:**
`present::presents()` (`present.rs:1259`), `bind::activity()` draws/dispatches (`bind.rs:45`),
`dstorage::counters()` bytes (`dstorage.rs:853`), `xaudio2::stats().source_voices` (`xaudio2.rs:227`).

| phase | rule (pure fn `classify_phase(sample, prev)`) | confidence |
|---|---|---|
| `boot` | `presents() == 0` | 1.0 |
| `loading` | DirectStorage ≥ 32 MiB/s for 2 consecutive windows | 0.5 + min(0.4, MiB/s / 512) |
| `world` | frames advancing and console draws per frame ≥ 400 | 0.35 |
| `menu` | frames advancing and draws per frame < 400 | 0.35 |
| `cutscene` | never automatic | — |

Hysteresis: a candidate must win 2 consecutive windows before an auto event is written. Auto
detection keeps running while a mark is active (`source: "auto"`), so a marked run also measures the
heuristic against the truth. Thresholds are constants, not switches; marks are the authority. The
draws threshold is a guess: no measurement backs it.

#### `phase` schema
`{"t":…, "ev":"phase", "name":"world", "from":"loading", "confidence":1.0, "source":"mark|schedule|auto", "frames":1234}`
`from` is the previous phase **of the same source** (null for the first event). Names are the five
known ones or a custom `[a-z0-9_-]{1,32}` (e.g. `bench1`). Anything else is refused with a warning.

#### Signatures (appended to `report.rs`, section 5c)
```rust
/// spawned by telemetry_start's tail (a report.rs-internal call: no upstream site)
fn telemetry_poll_thread();
struct PhaseSample { frames: u64, draws: u64, io_bytes: u64, voices: u64, dt_s: f64 }
fn classify_phase(prev: Option<&str>, s: &PhaseSample, last_io_rate: f64) -> (&'static str, f64);
/// "12.5:menu,40:world" | "world" -> [(0.0,"world")]; invalid entries skipped and reported
fn parse_phase_schedule(v: &str) -> (Vec<(f64, String)>, Vec<String>);
fn valid_phase_name(s: &str) -> bool;
/// content of the control file -> Some(name) | Some("auto") | None (ignored)
fn parse_phase_file(text: &str) -> Option<String>;
```
The poll thread exists only when telemetry is on (`telemetry_start`, `report.rs:294`). 5e reuses it
for the loop-counter readback. The thread writes the initial `phase boot` event at start.

#### Failure modes
- If the title never presents, everything is `boot`: correct by definition.
- Streaming worlds read continuously: the `loading` confidence stays below 0.9, and marks win.
- The xaudio2 stats call clones two small `Vec`s once a second: negligible.
- `XCL_PHASE_MARK` with telemetry off does nothing; one warning.

#### Tests
- unit: `parse_phase_schedule` (order, decimals, bare name, garbage, negative), `valid_phase_name`,
  `parse_phase_file` (trailing newline, `auto`, empty, binary).
- unit: `classify_phase` table, with the hysteresis driven over a synthetic sample sequence
  (boot → loading → world).
- e2e: the test title with `XCL_TELEMETRY=frame XCL_PHASE_MARK=0:menu,0.5:bench1`. The first
  `phase` event is `boot` (auto), and `menu` and `bench1` events appear with `source: "schedule"` if
  the title lives past 0.5 s. The check is ≥ 1 schedule event: the test title's length is not
  controlled. The control-file path is covered by a unit test that drives the poll step function
  directly on a temp dir; no timing-based e2e.

#### Estimate
`report.rs` about 150 runtime + 100 tests; e2e about 25. Total about 275. No upstream lines.

---

### 5d. `XCL_LOOP_BUDGET` on the direct DXIL path (gap L1)

#### Goal and scope
In: the direct translator's loop budget follows `XCL_LOOP_BUDGET` exactly as #1284's HLSL path does,
with byte-identical DXIL when unset. Out: a fork-only `XCL_LOOP_GUARD` alias (decision 2); keying
instead of bypassing (upstream's choice kept).

#### Edit
`dxil/lower.rs:116` (`first`, `impl` of the variable initialiser):
`return s.m.i32c(LOOP_BUDGET);` → `return s.m.i32c(crate::gpu::recomp::loop_budget());`
One line, justified: it is the switch itself. `loop_budget()` (`recomp/mod.rs:659`, `pub(crate)`)
returns `1 << 16 == LOOP_BUDGET` when unset, so the emitted constant, and therefore the DXIL, is
identical. This one start value feeds every guard on the path: the plain form `dxil/kernel.rs:362`,
the compact form `dxil/compact.rs:240,303` (it reads the same `VAR_BUDGET` slot), and the per-lane
bridge `recomp/lower_dxil.rs` (it uses `Lw`). `LOOP_BUDGET` stays as the documented default
(tests use it: `dxil/compact_tests.rs:103,113,430,436`).

**Caches:** none to add. `cache_allowed` already bypasses the in-memory `ShaderCache` and the
translated disk cache for **both** paths when `XCL_LOOP_BUDGET` is present (`recomp/pso.rs:157`
`console_to_dxil_in`, `:1148` mesh, list at `:908`). The DXC cache is keyed by HLSL. Driver PSO
blobs (`driver_cache::name_for` `real.rs:4603`, in-memory `compute_blob_key` `real.rs:4784`) and
##2371's offline whole-pipeline lookup (`compute_container` `ComputeBuild::CachedWhole`
`real.rs:4515-4535`) are keyed by the DXIL bytes, which change with the budget. So a changed value
can never be served a stale pipeline.

**Test seam (test-only, 4 lines in `recomp/mod.rs`):** a `#[cfg(test)] thread_local!
LOOP_BUDGET_TEST: Cell<Option<u32>>` consulted first by `loop_budget()`. Setting the environment
inside a multi-threaded test run would leak into concurrent GPU tests. The direct translation in
`translate_full_as` runs on the calling thread (`dxil/compact_tests.rs:108-111`), so a thread-local
reaches it. Release code is unchanged.

#### Vendor hazard (record, do not hide)
On a GPU whose `WaveLaneCountMax == 32` (the RTX 5080), wave64 compute/mesh/GS programs take the
split form: two hardware waves per console wave, which exchange through groupshared memory. Each
lane has its own budget. With a low budget, lanes of one half can leave a loop while the other half
waits at an exchange inside it. The partner never arrives, the split form's dead-partner path runs
(`VAR_XDEAD`, `dxil/lower.rs:30-31`; HLSL `xcl_x_dead`), and results are wrong, not only faster. The
prelude names this desync (`recomp/emit.rs:6207`). On RDNA4 (`32-64`) wave64 compute runs natively,
so this does not occur there. VS/PS use the upper-lanes form on both vendors, which has no
cross-half exchange (`recomp/upper_lanes.rs:1-30`), so no desync. Consequences:
- The manifest's `split_wave64` and `loop_budget` together say whether a run was exposed.
- A low-budget run is a probe of *time*. Its images and outputs are not evidence of correctness on a
  32-only GPU.
- 5e's optional `desync` counter (below) would make it measurable.

#### Tests (Test plan row 5, loop part)
GPU test in `dxil/compact_tests.rs` (`gpu_direct_loop_budget_follows_the_variable`), skipping via
`runner::skip_test("no GPU" | "no DXC")` as `compact_tests.rs:97-100` does. `KernelOptions { wave64:
false, split: false, .. }` so it runs on both vendors. A loop whose exit never comes (as
`compact_tests.rs:102-106`) counts iterations in s0: with the seam at 4096, s0 is 4096; unset, it
is 65 536, for both `compact = true/false`. A loop with a real exit at 100 iterations runs exactly 100
under budget 4096 (a loop that exits on its own is not cut short).
The cache claim is already pinned by `recomp/pso.rs:2045` (the `cache_allowed` test lists
`XCL_LOOP_BUDGET`).

#### Estimate
1 runtime line, 4 test-seam lines, about 45 test lines. Total about 50.

---

### 5e. `XCL_LOOP_COUNTERS=1`: per-pipeline loop-guard trip counters

**Honest status:** this is the biggest unknown in part 5. It touches both code generators, the
binding path and the caches. Two of its premises are unverified (that the counter region's CPU
visibility holds on window 0, and that `dred::label` sees the final pipeline pointer under
`pso_async`). **Recommendation: split it into its own later units: 5e-0 (spike, half a day, no
merge), 5e-1 (HLSL path + readback + events), 5e-2 (direct DXIL path).** Do not start it until 5d
has shown, on a title, that budget sensitivity exists at all (D3: the probe comes first).

#### Goal and scope
In: when a lane's loop exits through the guard instead of its own condition, count it per
(pipeline, stage). Read the counts back without stalling and emit `loop_guard` events at tier `all`,
plus one `error` `loop_guard_tripped` per slot on its first trip. Off: generated code byte-identical,
caches valid.
Out: counting iterations of every loop (an atomic per lane per invocation, far too heavy; see
options); counting #2291's per-dispatch guards (not adopted; if it merges upstream, its exit must be
counted too, survey §2).

#### Where the counter lives: options
| Option | Shader side | Binding | Readback | Verdict |
|---|---|---|---|---|
| **A. an io slot + UMA window 0** | at entry `xcl_lc = io.Load(120*4)` (a window-0 byte offset); on trip `mem.InterlockedAdd(xcl_lc + 4*stage, 1)` | `bind_abi` writes `io[120]` (`bind.rs:1053-1054`) and the console-list replay does the same (`console_list.rs:896-897`): 1 line each | CPU reads the UMA region (host memory, `VirtualAlloc` + `OpenExistingHeapFromAddress`, `gpu/uma.rs:1-14`) with `read_volatile`, once a second from the poll thread: **no copy, no fence, no stall** | **recommended** |
| B. an extra root UAV (u6) | `RWByteAddressBuffer xcl_lc : register(u6)` | changes the ABI root signature (`pso::abi_root_signature`), every bind (`bind.rs:1063-1071`, `console_list.rs:907-911`), the dispatch-chain signature, the test runner | needs a default-heap buffer + copy to readback + fence | rejected: many files, and it changes the ABI for every pipeline unless versioned |
| C. the per-draw `res` buffer (256 B, `bind.rs:1056`) | atomic into res | none | needs to scan each draw's arena block before reuse | rejected: per-draw attribution is nice, but readback means hooking arena recycling |
| D. a slot baked into the shader as a constant | constant offset | none | as A | rejected: a run-dependent constant makes the DXIL differ every run, so caches could never be keyed (only bypassed), and it pollutes the driver blob cache |

Why A keeps shaders run-independent: the shader holds no address, only `io[120]` and a per-stage
constant (VS 0, PS 1, CS/MS 0, GS 2). The DXIL of a record is therefore the same in every counters
run. Caches can be **keyed** instead of bypassed, and a second counters run of a title does not
retranslate thousands of programs.

io slot 120: slots 110-112 are XCL's (`recomp/mod.rs:201-206`, the trap-temp SGPR range "unused by
titles"); 120 stays clear of upstream's growth from 110 (#2291 uses 113). Risk: a title program that
reads ttmp12 would see the offset instead of 0, the same accepted risk as 110-112.

#### Slot assignment (attribution)
At pipeline creation, `dred::label(pso, shaders)` (`hle/d3d12x/dred.rs:41`) is already called with
the console shader bytes at all five translated-pipeline sites (`real.rs:4451, 4488, 5397, 6273,
6348`). 5e adds **one line as the first statement of `dred::label`** (before its `on()` check):
`crate::report::loop_counters_label(pso, shaders);`. Behind one relaxed load, it gives the pipeline
the next 16-byte slot (4 stages × u32) in a 1 MiB region (65 536 pipelines) and records
`slot → (pso, [(stage, fnv64)])`. At bind, `crate::report::loop_counter_offset(real_pso) -> u32`
looks the pipeline up (a `Mutex<HashMap>` read, only when on) and returns its byte offset within
window 0 (0 when off or unknown). Slot 0 is a discard slot for unknown pipelines.

The region is allocated once, lazily, with `uma::get()?.alloc_in(0, 1 << 20, 1 << 16)`
(`gpu/uma.rs:625`), only if `is_committed_window(0)` (`gpu/uma.rs:350`). Otherwise counters stay
off with one warning. *Unverified: whether window 0 is always the committed (host) window in
runtime layouts with sparse VRAM windows (`gpu/uma.rs:34-38`, `committed_window` `:843`). The spike
must settle this.*

#### Generated code
- **HLSL path.** `LOOP_GUARD` (`recomp/mod.rs:648`) is pasted at `recomp/mod.rs:1112` and `:1233`.
  In counters mode only, the translated HLSL is post-processed once (in the style of #1284's prelude
  swap at `recomp/emit.rs:4855-4858`): `LOOP_GUARD` → `if (xcl_budget == 0u) { xcl_lc_trip(); break; } xcl_budget--;`
  and a prelude addition:
  `static uint xcl_lc = 0u; static bool xcl_lc_done = false; void xcl_lc_trip() { if (!xcl_lc_done) { xcl_lc_done = true; uint o; mem.InterlockedAdd(xcl_lc, 1u, o); } }`
  plus `xcl_lc = io.Load(480u) + <stage*4>u;` at entry. The flag matters: after a lane's budget hits 0
  every enclosing loop breaks at its head, so without it one lane would count many times. Off: no
  replacement, so the HLSL is byte-identical. Entry points that load io differently (GS `emit.rs:5113`,
  mesh `emit.rs:5175`, kernel `emit.rs:4894`) each need the `xcl_lc` load. The spike lists them all.
- **Direct path.** In the guard (`dxil/kernel.rs:362-366`, `dxil/compact.rs:303-310`), the `none`
  edge goes through a new block that does `raw_atomic(mem, Add, xcl_lc, 1)` (`dxil/ops.rs:726`) and
  then the original target. There, a guard exit leaves the program (`end`/latch with `END`), so a
  lane counts at most once and needs no flag. The `xcl_lc` value is one `io` raw load at entry
  (`Init.io`, `dxil/kernel.rs:511`; `mesh_init` `:759`; `geom_init` `:842`).
- **Optional (5e-3) `desync` counter:** in the split form, increment `xcl_lc + 12` where the
  dead-partner flag is set (`VAR_XDEAD`). This turns the 5d hazard into a number. It belongs to the
  NVIDIA question; leave it out of the first cut.

#### Caches (every one checked)
| Cache | Keyed by | Under counters |
|---|---|---|
| in-memory `ShaderCache` (`recomp/pso.rs:148` `shader_cache`) | console bytes + params, per process | fine: the flag is fixed for the process (read once into an `AtomicU8`) |
| translated disk cache (`recomp/pso.rs:178-290`, `identity()`) | console bytes + identity | **must tag**: add `identity_for(&id, "xcl-loop-counters-1")` when on, in `identity()` (`:250-280`) and in `direct_output_identity` (`:503-522`); otherwise an off-run entry would be served |
| DXC cache | HLSL text | automatically distinct |
| driver PSO blobs, memory (`compute_blob_key` `real.rs:4784`) and disk (`driver_cache::name_for` `real.rs:4603`, `graphics_name`) | DXIL bytes (+ root signature pointer) | automatically distinct |
| #2371 offline whole pipelines (`ComputeBuild::CachedWhole`, `real.rs:4508-4535`) | `driver_cache::path_for(dxil, ..)` | automatically distinct |
Alternative: add `XCL_LOOP_COUNTERS` to `cache_allowed` (`recomp/pso.rs:908`), one line, the safest.
Its cost is that every counters run retranslates and recompiles everything (hours of DXC on a large
title). **Recommendation: tag (keyed). Fall back to bypass if review finds a path that is not
identity-tagged.**

#### Readback and events
The poll thread from 5c, once a second plus once from `telemetry_final` (`report.rs:431`, called in
`write_final`): read each used slot's 4 u32 with `read_volatile`. GPU atomics to host memory are
visible once the work completes, so a mid-frame read can lag. The counters only grow, so that is
harmless. For each (slot, stage) whose value changed, write:
`{"ev":"loop_guard","record":"<fnv64 hex>","stage":"cs","pipeline":<slot>,"trips":<delta>,"total":<value>}`.
On a slot's first non-zero value, also write `error` `loop_guard_tripped` (non-fatal). The plan's
`iterations` field is **not** provided: it would need an atomic per lane per invocation at program
end, a different cost class. It could come later as `XCL_LOOP_COUNTERS=2`. `trips` counts lanes,
not waves.

Prerequisite: tier `all` (the plan puts `loop_guard` there). With `XCL_LOOP_COUNTERS=1` and a lower
tier, counters stay **off** (no shader change) with one warning, so caches and output stay clean.

#### Cost
Off: one relaxed load at `dred::label` (pipeline creation, cold) and one per bind at the two io
sites (`bind_abi`, console-list replay). On: one io dword load per invocation (io is already being
read for SGPRs) and an atomic over PCIe per tripping lane, rare unless the budget is low. Combine
counters with a low `XCL_LOOP_BUDGET` with care: every lane may trip. That is the diagnostic case
and it is expected to be slow. It is a diagnostic mode, so `compare_key` gets
`XCL_LOOP_COUNTERS=1`.

#### Signatures (report.rs, section 5e)
```rust
static LOOP_COUNTERS: AtomicU8;               // 0 off, 1 on (decided at telemetry_start)
pub fn loop_counters_on() -> bool;            // recompiler side: one relaxed load
pub fn loop_counters_label(pso: usize, shaders: &[(&str, Option<&[u8]>)]);
pub fn loop_counter_offset(pso: usize) -> u32; // bind side; 0 when off/unknown
fn loop_counters_tick(final_read: bool);       // poll thread
fn loop_guard_fields(record: u64, stage: &str, slot: u32, delta: u32, total: u32) -> Vec<(&'static str, serde_json::Value)>;
```
Plus `recomp/mod.rs`: `pub const IO_LOOP_COUNTER: usize = 120;` and the counted guard string;
`recomp/emit.rs`: the prelude and entry additions behind `loop_counters_on()`; `recomp/pso.rs`: the
identity tag; `dxil/kernel.rs`, `dxil/compact.rs`: the counted edge.

#### Call sites (one line each) and edits
| Where | Line |
|---|---|
| `hle/d3d12x/dred.rs:42` `label` (first statement) | `crate::report::loop_counters_label(pso, shaders);` |
| `hle/d3d12x/bind.rs:1054`/`1055` `bind_abi` | `io[crate::gpu::recomp::IO_LOOP_COUNTER] = crate::report::loop_counter_offset(real as usize);` |
| `hle/d3d12x/console_list.rs:897`/`898` (dispatch replay) | the same with `real` |
| `recomp/pso.rs` `identity()` / `direct_output_identity` | +2 tag lines each |
| `recomp/emit.rs` prelude push `:4855` and the entry loaders | about 15 lines behind `loop_counters_on()` |
| `dxil/kernel.rs:362`, `dxil/compact.rs:303` | about 10 lines each |

These are more than one line in the generators. That is unavoidable: the counter is generated code.

#### Tests
- GPU (HLSL, `recomp/tests.rs`, like `gpu_loop_budget_caps_a_loop` `recomp/tests.rs:8251`; skip via
  `gpu_ready` → `runner::skip_test`): a never-exiting loop with the test-seam budget 64, counters on,
  `wave64: false`. The runner's sgpr vector sets `s120` to a byte offset inside the input memory, and
  afterwards the word there equals the number of launched lanes. The same with a loop that exits at
  10 iterations gives 0.
- GPU (direct, `dxil/compact_tests.rs`): the same for `compact = true/false`.
- unit: with counters off, the translated HLSL of a looping program equals the HLSL without the
  feature (byte-identical); with counters on, `identity()` differs from off (pure `identity_for`
  calls, as `recomp/pso.rs:2045` tests `cache_allowed`).
- unit: `loop_guard_fields`, slot allocator (discard slot 0, capacity exhausted → 0 + one warning).

#### Estimate
5e-1 about 230 lines (runtime 120, tests 110); 5e-2 about 110; 5e-3 (desync) about 40. Total
350-400. Spike: no merged lines.

---

### 6. Events reader: `scripts/dev/smoke-events.py`

#### Goal and scope
In: a fork-only Python 3 (standard library only) reader. It prints a block byte-identical to
`smoke.ps1`'s `xcl-smoke v1` for the same run, building each line from `events.jsonl` where an event
carries the same figure, and from the log otherwise. `smoke.ps1` stays untouched (decision 3). A
`main` build writes no `events.jsonl`, so there everything comes from the log.
Out: changing `pr_check.py`; `xcl-smoke v2` (#2010); running the title (that stays `smoke.ps1`'s
job; the reader works on a finished run).

#### Interface
```
python scripts/dev/smoke-events.py --log <xcl.log> [--run <run-dir>] (--game <folder> | --title <label>)
       [--seconds N] [--repo <xcl checkout>] [--explain]
```
- `--run` is optional. By default the reader takes the newest `run-*` folder next to the log (part 1
  puts run folders in `logs_dir`, `report.rs:301`) whose `manifest.json` `log` field names this log.
- `--game`/`--title` follow `smoke.ps1:21-30`: SHA-256 of the trimmed folder leaf, first 4 bytes as
  hex, or the label with the same one-line check.
- `--seconds` defaults to the manifest's `switches.XCL_RUN_SECONDS`.
- `--explain` writes the source of each line (`events` / `log` / `report` / `git`) and the manifest's
  `compare_key` to stderr. stdout carries the block only.

#### Line by line (source and exact equivalence)
| Block line | smoke.ps1 | reader: events first | fallback |
|---|---|---|---|
| `commit:` | `git rev-parse HEAD`, dirty via `git status --porcelain --untracked-files=no -- crates Cargo.toml Cargo.lock` (`smoke.ps1:34-35`) | same two git commands (`subprocess`) | — |
| `gpu:` | report `d3d12.adapter` minus `\s*\([^()]*VRAM\)$`, else the first non-Basic `Win32_VideoController` (`:85-90`) | same from `report.json` in the run folder, then manifest `adapter` | Windows: the same CIM query via `powershell -NoProfile -Command`; elsewhere `unknown` |
| `title:`, `seconds:` | parameters | parameters / manifest | — |
| `exit:` | the launcher's exit code `0x{0:x}` (`:91`) | `exit.status & 0xFFFFFFFF`, as hex | log `process ends with status 0x…` / `process ends (thread` → `0x0`; neither → `killed (no exit)` |
| `frames shown:` | first `\((\d+) frames shown, (\d+) PresentX calls\)`, **case-insensitive** (`-match`, `:76`) | `end` with `reason == "run_seconds"`: `frames_shown`, `presentx` | same regex with `re.I` |
| `presents per 2 s (last 3):` | all `activity \(2 s\): (\d+) presents`, case-sensitive (`[regex]::Matches`, `:77-78`), last 3 joined by a space (empty string if none; the line then ends with a space) | last 3 `activity.presents_2s` | regex |
| `device removals:` | count of `device removed`, case-sensitive (`:81`) | **always the log**: the text also comes from `FenceError::Removed`'s Display (`gpu/fence.rs:23`), logged from many sites, so no event set reproduces the count | — |
| `crashes:` | count of `unhandled exception\|process ends with status` (`:82`) | count of `error` `title_exception` + (1 if `exit.status != 0`) | regex |
| `peak commit:` | max of `memory: (\d+) MB committed`, else `n/a` (`:79-80`) | max `activity.committed_mb` | regex |

Text encoding: read the log with `utf-8-sig`, as `Get-Content -Raw -Encoding utf8` does. Output
uses `\n` line ends and the same 12 lines including the two fence lines.

Known divergences, documented in the script's docstring:
(1) A run smoke killed on its own timeout: smoke prints `killed (no exit)`, and the reader prints the
same only when neither `exit` nor a log end line exists.
(2) The final `XCL_ACTIVITY` iteration cut between its two log lines (`host.rs:65` and `:104`) gives
smoke one more `presents` sample than there are `activity` events (the event is written after the
memory line). Both are rare, and `--explain` names the source.

#### Tests
- `scripts/dev/test_smoke_events.py` (unittest, as `scripts/dev/test_pr_scan.py`; runs on macOS and
  Windows): each regex fallback against the strings `smoke.ps1` matches, including case-folding
  (`Frames Shown` matches the frames regex, but `Activity (2 s)` does not match the presents one);
  events-first selection; trailing space when there are no presents; the title hash of `SomeTitle`;
  `exit` status formatting of `-1073741819` → `0xc0000005`.
- `scripts/dev/smoke-events.tests.ps1` (fork-only; Windows PowerShell or pwsh): **the equality
  test.** A fixture in the style of `scripts/smoke.tests.ps1`: a stand-in `xcl.cmd` printing `Log:`,
  a synthetic `xcl.log`, a `report.json` and a run folder with consistent `events.jsonl` and
  `manifest.json`. It runs `smoke.ps1 -Title SomeTitle -Seconds 1` and `smoke-events.py --title
  SomeTitle --seconds 1` and asserts the two blocks are equal line for line. Cases: (a) with a run
  folder, (b) without one (main build), (c) with a device-removal line and a crash line. It passes
  `-Title`, avoiding #282's stale assertion.
- Real-run acceptance (manual, on the PC): `XCL_TELEMETRY=frame .\scripts\smoke.ps1 -Game … -Title X`,
  then the reader on that log. The blocks must be equal; record the diff (none expected) in the run
  notes. This is the only check that the runtime events and the log agree on a real title.

#### Estimate
Script about 220 lines, Python tests about 150, PowerShell equality test about 90. Total about 460.
That is above the plan's ~120 because this is a separate reader with a fallback and its own tests,
not an edit to `smoke.ps1`.

---

### 7. Work breakdown, file ownership, waves

`report.rs` is one serial resource: every runtime unit appends there, so runtime units run one after
another. Parts 2-4 also append to `report.rs`/`log.rs`; part 5 units start after part 4 is merged on
`dev`, or are rebased onto it. Units in one wave touch disjoint files.

| Unit | Files owned | Depends on | Acceptance |
|---|---|---|---|
| **U5a** no-window | `hle/d3d12x/present.rs`, `config.rs`, `report.rs` (5a section), `crates/xcl/tests/telemetry_e2e.rs` (new, OD2), README section | part 1 | headless/skip/size e2e green; unit tests; `cargo test --workspace` counts reported; smoke pair (runtime change) |
| **U5d** direct loop budget | `gpu/dxil/lower.rs`, `gpu/recomp/mod.rs` (test seam only), `gpu/dxil/compact_tests.rs` | — | GPU test passes or `SKIPPED` on both vendors; unset → identical DXIL (the existing `compact_tests.rs:96` still passes) |
| **U6** reader | `scripts/dev/smoke-events.py`, `scripts/dev/test_smoke_events.py`, `scripts/dev/smoke-events.tests.ps1` | event schemas of 5b (from this spec, not its code) | Python tests green on macOS + Windows; equality test green on Windows; real-run equality after U5b |
| **U5b** errors | `report.rs` (5b section + edits to part 1's `telemetry_pipeline_refused`, `telemetry_shader_refused`, `telemetry_stall`, `Manifest`), `hle/d3d12x/real.rs`, `eh.rs`, `resolver.rs`, `vfs.rs`, `host.rs`, `hle/d3d12x/present.rs` (1 line), `telemetry_e2e.rs` | U5a (present.rs, report.rs, test file) | the unresolved-import/end/exit e2e; `activity` mirror e2e; unit tests |
| **U5c** phase | `report.rs` (5c section; adds the poll-thread spawn at the end of `telemetry_start`), `telemetry_e2e.rs` | U5b | unit tests; schedule e2e |
| **U5e-0** spike | none merged; notes | U5d + a title run showing budget sensitivity | answers: window-0 visibility, `pso_async` pointer at `label`, the complete list of io entry loaders |
| **U5e-1** counters HLSL | `report.rs` (5e), `recomp/mod.rs`, `recomp/emit.rs`, `recomp/pso.rs`, `recomp/tests.rs`, `hle/d3d12x/dred.rs`, `hle/d3d12x/bind.rs`, `hle/d3d12x/console_list.rs` | U5c, U5e-0 | GPU test; byte-identical-when-off unit test; identity test |
| **U5e-2** counters direct | `gpu/dxil/kernel.rs`, `gpu/dxil/compact.rs`, `gpu/dxil/compact_tests.rs` | U5e-1 | GPU test both forms |

Waves:
- **W1 (parallel):** U5a ‖ U5d ‖ U6. Files are disjoint.
- **W2:** U5b. Then U6's real-run equality check on the PC.
- **W3:** U5c.
- **W4 (gated, OD5):** U5e-0 → U5e-1 → U5e-2.

Every runtime unit: whole suite on the RTX 5080 (`cargo test --workspace -- --test-threads=4`,
counts and every `SKIPPED` line), and a smoke pair per CLAUDE.md when it touches `crates/xcl/src`
outside tests. Fork-only, so no upstream PR (D8), but the smoke pair is still the regression check.

Line estimates (runtime + tests): 5a ≈ 130, 5b ≈ 380, 5c ≈ 275, 5d ≈ 50, 5e ≈ 350-400 (+40 desync),
6 ≈ 460. Lines in upstream files outside the generators: 5a ≈ 14 (present.rs) + 18 (config.rs),
5b = 12, 5c = 0, 5d = 1, 5e ≈ 3 call sites + about 50 in the generators.

### 8. Alternatives rejected (summary)
- Headless option B (keep `Presenter` for readbacks): decided against by the orchestrator. It is
  only needed for frame dumps and #2637.
- Reusing `present = "off"` for headless: it does not count frames (`present.rs:141`).
- Phase via a named pipe or a hotkey: IPC surface / no window in headless.
- Loop counters through a new root UAV, the per-draw `res` buffer, or a baked constant (5e table).
- Bypassing caches for `XCL_LOOP_COUNTERS`: kept only as the fallback.
- Editing `smoke.ps1` or `scripts/test-smoke-artifacts.tests.ps1`: decision 3. Every edit would
  conflict on sync, since about ten open upstream PRs touch the script.
- A fork alias `XCL_LOOP_GUARD`: decision 2.

### 9. Open decisions for the user
- **OD1 Headless pacing.** Option A drops the present ring's N-2 fence wait, which today bounds CPU
  run-ahead (`present.rs:640-645`). Choice: (a) plain A, as decided, plus a measured check (commit,
  `wait_ms`, frame rate headless vs windowed on the test title and one title); (b) A plus pacing:
  about 8 more lines in `present_now`. It signals the presenter fence on the title queue and waits
  N-2 without recording a copy. Recommendation (a) first, (b) if the check shows a difference.
- **OD2 A new fork-only test file** `crates/xcl/tests/telemetry_e2e.rs`. D6 forbids new modules in
  the crate. A test file is not a module of the crate, it avoids edits to upstream's `e2e.rs`, and
  part 2 may create it first. Needs a yes.
- **OD3 Error kinds beyond the plan's list.** Add `load_failed` (the loader refusing the title,
  `host.rs:220`; fatal). Keep `stub_called_fatal` reserved with no producer: nothing in the tree
  makes a stub fatal, and inferring "the stub that killed it" is guesswork.
- **OD4 What counts as fatal.** Proposed: `XCL_RUN_SECONDS`, title exit and window close are **not**
  errors (`end` only); `XCL_STALL_DUMP` firing **is** (`timeout`, fatal). An alternative is to make
  every run end with one fatal event. That would turn every scripted run into an "error" run.
- **OD5 Gate 5e on the 5d probe.** Build 5e only after a title run shows the frame time moves with
  `XCL_LOOP_BUDGET` (D3 says build both, probe first; this makes "first" a gate rather than an
  order). Also: tier `all` is required for counters, and with a lower tier they stay off.
- **OD6 io slot 120** for the counter offset (ttmp range, as 110-112), and the 1 MiB UMA region in
  window 0. Both change what titles' shaders see only in counters mode.
- **OD7 Phase control-file default** `<logs_dir>/phase` (known before launch) rather than
  `<run_dir>/phase` (named at start). `XCL_PHASE_FILE` overrides it. This is a new switch, not in the
  plan's summary table.
- No new dependencies anywhere. No build-time code (`build.rs`) changes. The Python reader is stdlib
  only.

### 10. Switch summary after parts 5-6 (delta to the plan's table)
| switch | effect |
|---|---|
| `XCL_HEADLESS=1` | no window, no swap chain, no compositor; frames counted; frame dumps off; `XCL_PRESENT_SYNC` inert |
| `XCL_PRESENT_SKIP=1` | window, no copy, no `Present`; frames counted; `XCL_PRESENT_SYNC` inert |
| `XCL_WINDOW_SIZE=<w>x<h>` | window size override (read in `config.rs`) |
| `XCL_LOOP_BUDGET=<n>` | upstream's name (#1284), now on both paths (replaces the plan's `XCL_LOOP_GUARD`) |
| `XCL_LOOP_COUNTERS=1` | loop-guard trip counters, needs `XCL_TELEMETRY=all`; caches keyed |
| `XCL_PHASE_MARK=<t:name,…>` or `<name>` | phase schedule (D4 kept, made time-based) |
| `XCL_PHASE_FILE=<path>` | control file polled for marks (default `<logs_dir>/phase`) |

### Refinement of 5d, 2026-11-26 (orchestrator; inside the approved design)

The spec's claim that `translate_full_as` translates on the calling thread is wrong: `translate_entry` (gpu/dxil/kernel.rs:157-171) spawns the thread "xcl-dxil-translate". The test seam therefore also gets two `#[cfg(test)]` statements in `translate_entry` that carry `LOOP_BUDGET_TEST` into the spawned thread. Release code is unchanged; U5d's files are lower.rs, recomp/mod.rs, kernel.rs (those two test-only lines) and compact_tests.rs.

## 2026-11-26 Telemetry part 5e: loop counters, spike result and refined spec (orchestrator approved; replaces section 5e of the parts 5 and 6 entry where they differ)

Read-only spike by an Opus worker against worktree xcl-adopt (part 3 call sites uncommitted at the time, so every edit is anchored by quoted code, not line numbers). Verdict: go, with changes. Two premises of the earlier section 5e were wrong: UMA window 0 is sparse, GPU-only memory in every runtime layout (the counter region goes in the first committed window and the io word carries the window index), and the counters flag must be read once from the environment because the cache identity is computed once. The pipeline pointer premise holds.

### Orchestrator decisions on 5e

- The refined spec below is binding for units 5e-1 (HLSL path, slot table, readback, events) and 5e-2 (direct DXIL path); 5e-2 follows 5e-1. The optional desync counter stays out.
- Counters need XCL_TELEMETRY=all; with XCL_LOOP_COUNTERS=1 and a lower tier they stay off with one warning, as section 5e already said. Because the flag is read once from the environment (see the spec), "off at a lower tier" is decided in that same one read: the flag is on only when both XCL_LOOP_COUNTERS=1 and the parsed XCL_TELEMETRY tier is all.
- Correction to section 5d: the per-lane DXIL form (recomp/lower_dxil.rs) has no loop guard, so XCL_LOOP_BUDGET does not apply to it. To be stated in docs/guide/telemetry.md by the closing docs unit.
- The units start after part 3 is committed (they edit recomp/pso.rs, hle/d3d12x/bind.rs and console_list.rs, which carry part 3's uncommitted call sites).
- Check: PC build, the GPU tests of each unit, the whole suite in the desktop session against the baseline of the commit before, and one title run with XCL_LOOP_COUNTERS=1 at tier all.

Tree: worktree `xcl-adopt` (`adopt/diagnostics-2`, HEAD af6f718a + other workers' uncommitted edits),
read on 2026-11-26, nothing built or run (macOS). Paths are under `crates/xcl/src/`. Line numbers
were moving while I read (bind.rs, console_list.rs, real.rs, pso.rs were being edited by other
workers): **anchor every edit by the quoted code and the function name, not by the line number.**

### Verdict: GO-WITH-CHANGES

The design holds in shape (an io slot, a CPU-readable region, per-stage words, readback from the
poll thread, keyed caches), but premise P1 fails as written: **window 0 is not CPU-visible in any
runtime layout**. The region must live in the first committed (coherent) window, and the io word
must carry that window's index. P2 holds. The other changes are smaller (see "Changes to the
design" at the end).

---

### P1. Is UMA window 0 CPU-visible committed memory? **No (runtime layouts).**

- `uma::get()` (`gpu/uma.rs`, `pub fn get`, ~line 850) builds the runtime aperture with
  `Uma::for_runtime_with` / `for_runtime_windows` → `reserve_runtime` (`gpu/uma.rs` ~line 260).
  There `kinds[..sparse_wins] = Sparse` and the committed coherent windows come **after** them
  (`let k = sparse_wins + c; kinds[k] = WindowKind::Committed(..)`). `sparse_wins =
  min(windows - coh_wins, MAX_RUNTIME_SPARSE_WINDOWS = 3)`, and `max_coherent_windows` always
  leaves 3 sparse windows, so in every runtime layout (4, 8, 16 windows) **windows 0..2 are sparse
  and the first committed window is 3**.
- Sparse windows are GPU reserved resources (`hle/d3d12x/sparse.rs` `init`,
  `dev.CreateReservedResource(..)`), backed by VRAM tiles; their CPU side is only a write-watched
  `MEM_RESERVE` range. A GPU atomic there is not visible to a CPU `read_volatile`.
- Worse, `alloc_in(0, ..)` (`gpu/uma.rs` `alloc_in` → `alloc_where`) searches `inner.free`, which
  only holds committed windows: in a runtime layout `alloc_in(0, 1 << 20, 1 << 16)` returns `None`.
  (The doc comment of `Uma::for_runtime` says it outright: "window 0 holds 32-bit-addressed
  descriptor heaps + buffers + XMem", window 3 "a smaller committed coherent window ... for per-draw
  arguments and CPU readback buffers".)
- The only layout with a committed window 0 is the fallback `Uma::new` (taken when
  `reserve_runtime` finds no aligned slot): `with_windows(n, ..)` commits windows 0..n-1, sparse
  after.
- Committed windows are host memory opened as heaps: `hle/d3d12x/real.rs` `open_uma` →
  `OpenExistingHeapFromAddress` + `CreatePlacedResource(.. ALLOW_UNORDERED_ACCESS |
  ALLOW_CROSS_ADAPTER ..)`, the buffer VA goes into `UMA_BUFFER[k]` → `uma_window_vas()`.
  GPU→CPU visibility of stores there is already pinned by a test
  (`gpu/recomp/tests.rs`, the `Uma::new(1 << 20)` + `run_in_arena` test: "buffer_store visible to
  the CPU"), and UAV atomics on such buffers by `gpu/recomp/heap_window_tests.rs`
  `gpu_heap_windows_divergent_lanes_match_switch_form` (`GLOBAL_ATOMIC_ADD` into arena memory).

**How shaders address window k.**
- HLSL (`gpu/recomp/emit.rs` `Em::finish`): `mem` = u2 (window 0), `mem1..mem3` = u3..u5 when
  `windows > 1`; every `mem.Method(` of the whole program text, preludes included, is rewritten by
  `windowed()` into an accessor `xcl_mMethod(` that switches on the static `xcl_w` (root-UAV form,
  `mem_accessors` → `method_switch`) or indexes `ResourceDescriptorHeap[MEM_HEAP_BASE + xcl_w]`
  (heap form, `XCL_HEAP_WINDOWS=1`). `xcl_w` outside `0..windows` hits `default` (dropped).
  With one window there is only `mem` and no `xcl_w`.
- Direct DXIL (`gpu/dxil/lower_mem.rs` `Lw::with_mem_many_tys`): the window is SSA variable
  `VAR_WIN`; heap form: `heap_raw_buffer_uav(MEM_HEAP_BASE + w)` guarded by `w < n`; root form:
  `mems[k]` fixed when `VAR_WIN` is constant, else a `switch` over `mems`. One window: `mems[0]`
  regardless of `VAR_WIN`.
- Binding: `bind_abi` (`hle/d3d12x/bind.rs`) and the console-list dispatch replay
  (`hle/d3d12x/console_list.rs`) bind `uma_window_vas()[0..4]` as root UAVs u2..u5, so window 3 is
  `mem3` on the root form and descriptor `MEM_HEAP_BASE + 3` on the heap form. Shaders always have
  `windows = uma.shader_windows() = kinds.len() >= 4` when sparse windows exist, so k = 3 is
  addressable.

**Decision.** The region goes into `k = uma::get()?.committed_window()?` (3 in runtime layouts, 0 in
the `Uma::new` fallback): `u.alloc_in(k, 1 << 20, 1 << 16)`. A byte offset alone cannot reach it, so
the io word encodes **window and offset**:

```
word = (k << 28) | (byte_offset_in_window_of_slot >> 4)    // slots are 16-byte aligned
word == 0  ⇒  do not count (counters off, unknown pipeline, no region)
```
k ≤ 15 fits (MAX_HEAP_WINDOWS = 16); a window offset < 4 GiB shifted by 4 fits 28 bits. Slot 0 of
the region is never handed out, so a real word is never 0 even when k = 0 and the region starts at
offset 0. **Fallback:** no committed window, or `alloc_in` returns None → one `xlog_warn!`, no
region, every bind word is 0; the generated code stays the counted form (the flag is per process, so
caches stay consistent), it just never writes.

### P2. Which pipeline pointer does `dred::label` get under `pso_async`? **The real one; it matches the bind sites.**

- The five label sites are inside `translated_compute_pso_once` (twice: dispatch-chain branch and
  the plain branch), `translated_graphics_pso` (vs/ps), `translated_mesh_stream` (ms/ps) and the
  vs/gs/ps path in `hle/d3d12x/real.rs`; each passes `*ppv as usize` right after the driver created
  the PC pipeline into `ppv`.
- Under async/lazy creation the title gets a stand-in from `go_background` (`real.rs`), and the
  **job** calls `build_compute` / `build_graphics` / `build_stream` →
  `translated_*_pso(dev, .., &mut out)` with a local `out`: the label then receives the **real**
  pipeline. `complete(p, real)` stores the same pointer in `Pending::real` (`pso_async.rs`).
  Synchronous creation: `*ppv` is both the title's object and the real pipeline.
- Bind sites resolve to the same pointer: `bind_abi` (`let real = super::pso_async::resolve_wait(pso);`)
  and the console-list replay (`let real = super::pso_async::resolve_wait((*p).pc);`), whose
  `real` is also the key of the existing per-pipeline maps (`pso_user_sgprs(real)`,
  `pso_root_signature(real)`), which are filled with the same `*ppv` next to the label calls.
  A lazy pipeline is built inside `resolve_wait` (`job()` then `complete`), so its label runs before
  the bind reads the table.
- When `resolve_wait` returns the stand-in (late, spared, failed), the stand-in is unlabelled → word
  0; stand-ins run PC shaders with no guard anyway.
- **Address reuse:** `pso_release` → `forget_pso` drops XCL's maps by address; the label table does
  not need a forget call because every translated pipeline is labelled at creation, before its first
  bind, and the insert overwrites. A stale entry can only be read for a stand-in that got a freed
  address, and a stand-in never executes a guard. **Change:** slots are allocated per distinct
  shader set (the label text), not per pointer, so a title that creates and destroys pipelines
  cannot exhaust the 65 535 slots; the pointer map only points at the slot.
- `dispatch_chain::dispatch` binds further pieces with the io already bound: their trips land in the
  labelled pipeline's slot (correct attribution by record).

### P3. Entry points that load io, and every loop guard

**HLSL (`gpu/recomp/emit.rs` `Em::finish`).** Every mode loads its SGPRs through one closure,
`let load_sgprs = |h: &mut String| { if windows > 1 { .. xcl_uma_base = io.Load(..) } for &c in &sg { .. io.Load(c * 4) } };`,
called from: `Mode::Kernel`, `Mode::Vertex`, `Mode::Compute`, `Mode::Mesh`, `Mode::Geometry` native
form, `Mode::Geometry` split form (inside `phase(es)` **and** `phase(gs)`), `Mode::Pixel`. One edit
in the closure covers all 8 calls. (The GS/mesh `xcl_total`/`xcl_vin` loads of `IO_GS_*` are
separate and unaffected.)

**HLSL guards.** `LOOP_GUARD = "if (xcl_budget == 0u) break; xcl_budget--;"` (`gpu/recomp/mod.rs`) is
pasted in exactly two places, both in `mod.rs`: `emit_structured` (`[loop] for (;;) {` + guard) and
`emit_range` (`[loop] while (pc >= a && pc < b) {` + guard). Straight-line programs have none. The
other `[loop]`s in emit.rs (`xcl_x_meet` spin, safe-barrier spin, GS output copies) are not guards.
After a trip `xcl_budget` stays 0, so every enclosing loop breaks again at its head, and in the split
geometry form the GS phase runs on with budget 0 → **a per-lane "done" flag is required** in HLSL.

**Direct DXIL.** All entries (Kernel, Compute, Mesh, Geometry, Vertex, Pixel) go through
`gpu/dxil/kernel.rs` `translate_on_this_thread`; each has `lw.init.io` (u0). Guards:
1. plain form, same function: `if cfg.headers.contains(&b) { let left = lw.rd(VAR_BUDGET); let none = lw.cmpc(Pred::Eq, left, 0); let cont = lw.block(Some(1)); lw.cond_br(none, end, cont); ... }`
   — `end` is `exit` (program end), or the phase `switch` in the two-phase split geometry form
   (ES trip continues into the GS → needs a once-flag there only).
2. compact form, `gpu/dxil/compact.rs` `emit_with_nest` (shared by `emit`): `if cfg.headers.contains(&p.order[r]) { let left = lw.s.m.load(budget, 4); ... lw.cond_br(none, out, body); lw.s.m.set_block(out); store(to, END); br(target_latch) ... }`
   — a trip always ends the program (no flag). Compact plans exist only for Kernel/Compute.
3. **The per-lane bridge `recomp/lower_dxil.rs` (`XCL_PER_LANE=1`) has no loop guard at all**
   (`recomp/lower.rs` has no budget): nothing to count; 5d's note that it inherits the budget is
   wrong (see OUT_OF_SCOPE).

### P4. io slot 120

- io = the 128-dword SGPR image (`KERNEL_SGPRS = 128`, `recomp/mod.rs`); at runtime exactly two
  builders: `bind_abi` (`io.resize(crate::gpu::recomp::KERNEL_SGPRS, 0); io[crate::gpu::recomp::IO_UMA_BASE_HI] = ..`)
  and the console-list replay (same two lines). The test runner builds `io = state.sgpr` (+ VGPRs in
  Kernel mode) in `runner.rs` `run_with_statics` / `run_in_region`.
- Slot 120 = SGPR code 120 = `ttmp12` (`emit.rs` `sname`: 108..=123 → ttmp0..15). XCL writes only
  110..112 (`IO_GS_PRIMS`, `IO_GS_VERTICES`, `IO_UMA_BASE_HI`); grep finds no other use of 120.
  Titles' user SGPRs come from `user_data_for` / `user_data_gfx` (low SGPRs). Upstream #2291 uses 113.
- Reads: HLSL `io.Load(480u)` into a new static; direct `s.raw_load(io, c(480), 1)[0]` hoisted to the
  entry block (the `uma_base()` pattern in `gpu/dxil/lower_window.rs`). With counters off the bind
  writes 0 there, i.e. io is byte-identical to today.

### P5. Caches

| Cache | Keyed by | Under counters |
|---|---|---|
| in-memory `ShaderCache` (`pso.rs` `shader_cache`, `ShaderKey`) | console hash + stage, sampler base, windows, split | fine: the flag is fixed per process (OnceLock). Tests must not go through `console_to_dxil_*` with the seam set. |
| translated disk cache (`pso.rs` `mod translated`, `identity()`; mesh path `translated::cached(.., Stage::Mesh, ..)` uses the same) | `identity()` (Backend-tagged: one identity for HLSL and direct routing) + file name | **tag** at the end of `identity()` |
| rerouted-enormous entries (`rerouted_identity(ident, direct_output())`) | `identity()` + `direct_output_identity` | **tag** at the end of `direct_output_identity` (both inputs then differ) |
| DXC cache | HLSL text | distinct automatically (prelude/guard text differ) |
| driver blobs: compute (`driver_cache::name_for(dxil, ..)`, memory `compute_blob_key(dxil, rs, flags)`), graphics (`graphics_blob_key` hashes `fnv64(vs_dxil)`, `fnv64(ps)`, GS) | DXIL bytes | distinct automatically |
| #2371 offline whole pipelines (`ComputeBuild::CachedWhole` → `driver_cache::path_for(dxil, ..)`) | DXIL bytes | distinct automatically |
| `used-pipelines.txt` (`pso_async::used`) | console shader-set key | unaffected; it only orders preloads |
| `pso-manifest.txt` (`pso_manifest.rs`, `XCL_PSO_MANIFEST=1`) | records events, not outputs | unaffected |
| `xcl shaders --precompile` | fills the translated + DXC caches under the identity of its own process | without the variables it fills the off identity: a first counters run retranslates. Running precompile with `XCL_LOOP_COUNTERS=1 XCL_TELEMETRY=all` fills the counters identity (the flag reads env only, see below). |

Both identities are `OnceLock`s computed at first use, so the flag must be decided before the first
translation and never change: hence the flag is a `OnceLock<bool>` read from the environment, not a
mode word set by `telemetry_start` (the design's version could compute the identity before the flag
is set).

### P6. Split wave64 on a 32-lane GPU (RTX 5080)

- The exchange protocol uses only groupshared words: `XCL_XS` slots, `XCL_XW`, the arrival counters
  `XCL_XC` (`emit.rs` `PRELUDE_X64`: `xcl_x_thread`, `xcl_x_meet`), and in the direct path
  `lw.xs/xw/xc` (`gpu/dxil/lanes.rs` exchange). The counter atomic targets `mem` (a UAV in host
  memory); it touches none of them.
- A trip happens at a loop head, where no exchange is in progress. Both halves run the same scalar
  control flow with equal budgets, so they trip at the same head; if they do not, the halves had
  already desynced. The atomic only delays the tripping half by one PCIe round trip before it leaves;
  the partner's spin (≤ 2^20 polls) and dead-partner path are unchanged.
- **Must not** count through `ballot()`/`xcl_x_ballot` (exchanges in the split form): use the raw
  `WaveActiveCountBits(true)` / `WaveIsFirstLane()` (HLSL) and `s.wave_all_bit_count` /
  `s.wave_is_first_lane` (direct), which stay inside one hardware wave.
- Counting once per thread is right: each half counts its 32 threads, total 64 per console wave, the
  same as a native wave64.
- Dead-partner flag set (for a later `desync` word 3): HLSL `emit.rs` `PRELUDE_X64`, in
  `xcl_x_meet`: `if (WaveReadLaneFirst(ok) == 0u) { xcl_x_dead = true; return false; }`; direct
  `gpu/dxil/lanes.rs` exchange: `let d1 = lw.selc(fail, 1, d0); lw.wr(VAR_XDEAD, d1);`.

### P7. Cost and safety

- `mem` on the counter window is a placed buffer on a cross-adapter heap opened from host memory
  (`open_uma`), UAV-enabled. Atomics there already run in the suite (`heap_window_tests.rs`
  `GLOBAL_ATOMIC_ADD` into a `Uma::with_windows(2, ..)` arena, all four forms).
- Stages: VS and PS are graphics stages with root UAVs (`bind_abi` gfx → `SetGraphicsRootUnorderedAccessView`;
  translated PS/VS already emit `mem.Interlocked*` for console atomics). Compute, mesh pass
  (`Mode::Mesh`, compute) and GS pass (`Mode::Geometry`, compute) are compute. **No translated
  stage lacks UAV access; counters are on for all of them.**
- PS: helper lanes' UAV writes are discarded and helper lanes do not take part in wave ops, so
  helpers are never counted. A PS that had no UAV write before gains a possible one: the driver may
  run depth tests late for it. Images are identical; frame time may move — another reason
  `compare_key` carries `XCL_LOOP_COUNTERS=1` (already in `diagnostic_switches` FLAGS).
- Cost on: one io dword load per invocation; per trip **one atomic per hardware wave** (aggregated
  with `WaveActiveCountBits`) instead of one per lane: 32-64× fewer contended PCIe atomics when a low
  `XCL_LOOP_BUDGET` makes every lane trip (a full-screen PS would otherwise issue ~2 M same-address
  host atomics per frame).
- Off: one `OnceLock` load per translation (cold), one relaxed load at label (cold), one per bind
  (`loop_counter_word` returns 0 at once).

---

### Refined spec

Flag and names (both units):
- `XCL_LOOP_COUNTERS`: on when set, non-empty and not `"0"` **and** `XCL_TELEMETRY` parses to
  `Tier::All` (`crate::log::parse_tier`). Otherwise off; if the variable asks for it but the tier is
  lower, one `xlog_warn!("telemetry: XCL_LOOP_COUNTERS needs XCL_TELEMETRY=all - loop counters off")`.
- Stage word per mode: Kernel / Compute / Mesh / Vertex → 0, Pixel → 1, Geometry → 2; word 3
  reserved (desync, 5e-3). Label kinds map: word 0 ↔ the label entry `cs`/`ms`/`vs`, 1 ↔ `ps`, 2 ↔ `gs`.
- io word: `(k << 28) | (offset >> 4)`, 0 = do not count.

#### Unit 5e-1 (HLSL path, slot table, readback, events)

Files owned, in order: `telemetry_run.rs`, `gpu/recomp/mod.rs`, `gpu/recomp/emit.rs`,
`gpu/recomp/pso.rs`, `hle/d3d12x/dred.rs`, `hle/d3d12x/bind.rs`, `hle/d3d12x/console_list.rs`,
`report.rs` (two one-line calls), `gpu/recomp/tests.rs`, `gpu/recomp/heap_window_tests.rs`.

**1. `telemetry_run.rs`, new section `// ---- part 5e: loop guard counters ----`**

```rust
/// Whether translated code counts loop-guard exits (`XCL_LOOP_COUNTERS` with tier `all`): decided
/// once per process from the environment, before the first translation.
pub fn loop_counters_on() -> bool;   // OnceLock<bool> over loop_counters_wanted(env…) + the warning

/// pure: the decision from the two variables
fn loop_counters_wanted(counters: Option<&str>, tier: Option<&str>) -> (bool, Option<&'static str>); // (on, warning)

/// pure: the io word of byte offset `off` in window `k` (0 is reserved for "do not count")
pub(crate) fn loop_counter_word_of(k: usize, off: usize) -> u32;   // debug_assert k < 16, off % 16 == 0, off >> 4 < 1 << 28

/// Slot table: one 16-byte slot per distinct shader set, slot 0 never used.
struct LcSlots { by_label: HashMap<String, u32>, by_pso: HashMap<usize, u32>, labels: Vec<(String, Vec<(&'static str, u64)>)>, full_warned: bool }
impl LcSlots { fn assign(&mut self, pso: usize, text: String, shaders: Vec<(&'static str, u64)>, capacity: u32) -> u32; /* 0 when full, warn once */ }
static LC_SLOTS: Mutex<Option<LcSlots>>;
/// (window k, address of the region, its offset in window k); None: no region
static LC_REGION: OnceLock<Option<(usize, usize, usize)>>;
const LC_REGION_BYTES: usize = 1 << 20;   // 65 536 slots, slot 0 unused

/// region in the first committed window (`Uma::committed_window`), allocated once
fn lc_region() -> Option<(usize, usize, usize)>;   // u.committed_window()? ; u.alloc_in(k, LC_REGION_BYTES, 1 << 16)? ; warn once on None

/// From `dred::label`: give the pipeline its slot (a new one for a new shader set).
pub fn loop_counters_label(pso: usize, shaders: &[(&str, Option<&[u8]>)]);
//   if !loop_counters_on() || pso == 0 { return }  — label text built exactly as dred's ("cs {fnv:016x} ps …"),
//   stage kinds normalised to &'static str ("cs"|"ms"|"vs"|"ps"|"gs")

/// From the two bind sites: the io word of `pso` (0 when off, unknown or no region).
pub fn loop_counter_word(pso: usize) -> u32;
//   if !loop_counters_on() { return 0 } ; slot from LC_SLOTS.by_pso ; region → loop_counter_word_of(k, off + 16 * slot)

/// pure: per slot, the change since the last read: (slot, word, delta, total, first_trip)
fn lc_deltas(prev: &mut Vec<[u32; 4]>, now: &[[u32; 4]]) -> Vec<(u32, usize, u32, u32, bool)>;

/// pure: the fields of one `loop_guard` event
fn loop_guard_fields(record: u64, stage: &str, slot: u32, delta: u32, total: u32) -> Vec<(&'static str, serde_json::Value)>;
//   [("record", format!("{record:016x}")), ("stage", stage), ("pipeline", slot), ("trips", delta), ("total", total)]

/// Poll thread (every call, reads at most once a second) and `telemetry_final` (`final_read`):
/// read every assigned slot's 4 words with `read_volatile`, write `loop_guard` events (tier All)
/// for each changed word, and an `error` `loop_guard_tripped` (non-fatal) on a slot's first trip.
pub(super) fn loop_counters_tick(final_read: bool);
```

- Region read: `std::ptr::read_volatile((addr + 16 * slot + 4 * w) as *const u32)` for slots
  `1..labels.len()`; copy the label list under the lock, read memory outside it.
- `loop_guard_tripped`: `telemetry_error(ErrKind::LoopGuardTripped, "loop_counters", &format!("pipeline {slot} ({text}) left a loop through its guard"), None, false, &loop_guard_fields(..))`.
  `ErrKind::LoopGuardTripped` already exists (5b). Record of a word: the fnv64 of the label entry
  of that word's kind; unknown kind → 0.
- Poll thread edit: replace the marker line `// (5e: loop counter readback goes here)` in
  `telemetry_poll_thread` with `loop_counters_tick(false);`.
- Tests `#[cfg(test)] mod telemetry_p5e_tests`: `loop_counters_wanted` (on only with tier `all`;
  `"0"`/empty off; warning when asked with `frame`), `loop_counter_word_of` (k=3, off=0x1230 →
  `3 << 28 | 0x123`; k=0, off=16 → 1; never 0 for slot ≥ 1), `LcSlots::assign` (first slot 1; same
  text from another pso → same slot; capacity 3 → third distinct text gets 0 and `full_warned`),
  `lc_deltas` (first non-zero sets `first_trip`; unchanged words emit nothing; a word that grows
  again gives the delta), `loop_guard_fields`.

**2. `report.rs` (two one-line calls)**
- `telemetry_final`: after `gpu_tel::telemetry_gpu_final();` add `run_tel::loop_counters_tick(true);`.
- Nothing in `telemetry_start` (the flag is env-decided; the region is allocated lazily by the first
  `loop_counters_label`). If the orchestrator prefers the region taken before the title can fill
  the coherent window, add `let _ = run_tel::lc_region();` after `run_tel::telemetry_poll_start(..)`
  (make `lc_region` `pub(super)`); recommended, one line.

**3. `gpu/recomp/mod.rs`**
```rust
/// io slot (ttmp12, unused by titles as 110..112) holding the loop-counter word (`XCL_LOOP_COUNTERS`)
pub const IO_LOOP_COUNTER: usize = 120;
/// `LOOP_GUARD` in counters mode: the lane is counted (once) before it leaves
const LOOP_GUARD_COUNTED: &str = "if (xcl_budget == 0u) { xcl_lc_trip(); break; } xcl_budget--;";
#[cfg(test)]
thread_local! { pub(crate) static LOOP_COUNTERS_TEST: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) }; }
/// Whether translations count loop-guard exits (`report::loop_counters_on`; tests: the seam).
pub(crate) fn loop_counters() -> bool { #[cfg(test)] if let Some(b) = LOOP_COUNTERS_TEST.with(|c| c.get()) { return b; } crate::report::loop_counters_on() }
```
Put the const next to `IO_UMA_BASE_HI`, the rest next to `LOOP_GUARD` / `loop_budget`.

**4. `gpu/recomp/emit.rs`, `Em::finish` (all behind one `let lc = super::loop_counters();`)**
- At the top of `finish`, before anything uses `body`:
  `let counted; let body = if lc { counted = body.replace(super::LOOP_GUARD, super::LOOP_GUARD_COUNTED); counted.as_str() } else { body };`
- Right after the `XCL_LOOP_BUDGET` prelude `match` (`n if n == 1 << 16 => h.push_str(PRELUDE), ...`),
  `if lc { h.push_str(&loop_counter_prelude(windows > 1, stage_word)); }` where
  `stage_word = match self.opt.mode { Mode::Pixel(_) => 1, Mode::Geometry(_) => 2, _ => 0 }` and the
  new free function returns (windows > 1 form; the single-window form drops the `xcl_w` lines and
  adds `(xcl_lc >> 28) == 0u` to the early return, since only window 0 is bound):
```hlsl
// XCL_LOOP_COUNTERS: lanes that left a loop through its guard, once each (`LOOP_GUARD_COUNTED`)
static uint xcl_lc = 0u;
static bool xcl_lc_done = false;
void xcl_lc_trip() {
    if (xcl_lc_done) return;
    xcl_lc_done = true;
    if (xcl_lc == 0u) return;
    uint n = WaveActiveCountBits(true);
    if (WaveIsFirstLane()) {
        uint w = xcl_w; xcl_w = xcl_lc >> 28;
        uint o; mem.InterlockedAdd(((xcl_lc & 0x0FFFFFFFu) << 4) + {4*stage}u, n, o);
        xcl_w = w;
    }
}
```
  It must follow `MEM_ACCESSORS_HERE` (it does: pushed after PRELUDE); `windowed()` then rewrites
  its `mem.InterlockedAdd(` into `xcl_mInterlockedAdd(` (root and heap forms), and `linked_form`
  keeps it after the accessor block.
- In the `load_sgprs` closure, after the `xcl_uma_base` line:
  `if lc { let _ = writeln!(h, "    xcl_lc = io.Load({}u);", super::IO_LOOP_COUNTER * 4); }`
  (covers all 8 calls; the split GS loads it twice, harmlessly).
- Off: no replacement, no prelude, no load → HLSL byte-identical.

**5. `gpu/recomp/pso.rs`, `mod translated`**
- New pure fn: `pub(super) fn with_loop_counters(id: u64, on: bool) -> u64 { if on { identity_for(&id.to_string(), "xcl-loop-counters-1") } else { id } }`
- `identity()`: wrap the final value: the closure's last expression (the `XCL_PER_LANE` `if`)
  becomes `let id = if .. { .. } else { id }; with_loop_counters(id, crate::gpu::recomp::loop_counters())`.
  Keep the `return 0` for `XCL_TRANSLATED_CACHE=0` untouched.
- `direct_output_identity`: same wrap of its final `if`.
- Do **not** add `XCL_LOOP_COUNTERS` to `cache_allowed` (keyed, not bypassed). Test: in the
  existing test module, `with_loop_counters(42, false) == 42`, `with_loop_counters(42, true) != 42`,
  and `!= with_loop_counters(43, true)`.

**6. Call sites (one line each)**
- `hle/d3d12x/dred.rs` `label`, first statement, before `if !on() || pso == 0`:
  `crate::report::loop_counters_label(pso, shaders);`
- `hle/d3d12x/bind.rs` `bind_abi`, after `io[crate::gpu::recomp::IO_UMA_BASE_HI] = (u.base() >> 32) as u32;`:
  `io[crate::gpu::recomp::IO_LOOP_COUNTER] = crate::report::loop_counter_word(real);`
  (`real` is a `usize` there.)
- `hle/d3d12x/console_list.rs` dispatch replay, after the same `IO_UMA_BASE_HI` line:
  the same line with `real`.

**7. Tests**
- `gpu/recomp/tests.rs` (no GPU): `loop_counters_leave_hlsl_unchanged_when_off` — program of
  `gpu_loop_budget_caps_a_loop`; HLSL with `LOOP_COUNTERS_TEST = Some(false)` equals HLSL with the
  seam `None`, and contains no `xcl_lc`; with `Some(true)`: as many `xcl_lc_trip(); break;` as the
  off text has `LOOP_GUARD`, contains `xcl_lc = io.Load(480u);` and `xcl_lc_done`. Use a guard
  struct like `compact_tests.rs` `BudgetSeam` that resets the seam on drop.
- `gpu/recomp/heap_window_tests.rs` (GPU): `gpu_loop_counters_count_guard_exits` modelled on
  `gpu_heap_windows_divergent_lanes_match_switch_form`; skip via `gpu_kernel_ready()` (→
  `runner::skip_test("no GPU" | "no wave64" | "no DXC")`). Program: the never-exiting loop of
  `gpu_loop_budget_caps_a_loop` (s10 += 1; s_cmp_lg s10, 1; s_cbranch_scc1 back), with
  `LOOP_BUDGET_TEST = Some(64)` (fast) and the counters seam on. Arena `Uma::with_windows(2, 1 << 20)`,
  counter in **window 1**: `st.sgpr[120] = (1 << 28) | (0x4000 >> 4)`. Forms
  `(direct, heap) in [(false, false), (false, true)]` via `compile_variant(&code, direct, heap, 2)`
  (Kernel mode, `runner::options`: wave64, split on a 32-lane GPU). Expect, on **both** a 32-lane
  GPU (split: 2 × 32) and a 64-lane GPU (native): word at `w1 + 0x4000` == **64**, words
  `w1 + 0x4004..0x400C` == 0, word at `w0 + 0x4000` == 0 (no aliasing). Second program, same loop
  with a real exit after 10 iterations (`s_cmp_lg s10, 10`): **0**. Third run: `st.sgpr[120] = 0`
  → nothing written anywhere (all four words 0 in both windows). Reset the words before each run.
  The heap form needs no extra skip beyond what the existing heap tests use (check: if that file
  skips the heap form under a feature check, reuse it).

Estimate 5e-1: runtime ≈ 130 (telemetry_run ≈ 95, recomp 15, emit 15, pso 6, 3 call sites + 1),
tests ≈ 120. Total ≈ 250.

#### Unit 5e-2 (direct DXIL path)

Files owned, in order: `gpu/dxil/lower.rs`, `gpu/dxil/lower_window.rs`, `gpu/dxil/kernel.rs`,
`gpu/dxil/compact.rs`, `gpu/dxil/compact_tests.rs`, `gpu/recomp/heap_window_tests.rs` (extend
5e-1's test). Depends on 5e-1 (`IO_LOOP_COUNTER`, `loop_counters()`, the seam).

**1. `gpu/dxil/lower.rs`**
- `pub const VAR_LC_DONE: u32 = VAR_BUDGET + 6;` (free: +5 is `VAR_F32_MODE`, +8.. `VAR_IMG_WRITE`)
  with doc "the lane was counted (`XCL_LOOP_COUNTERS`, the two-phase geometry form only)".
- `Init::first`: add `|| var == VAR_LC_DONE` to the list that starts at `s.m.i32c(0)`.
- `Lw` fields: `pub lc: Option<u32>` ("counters mode: this program's stage word") and
  `pub lc_word: Option<Value>` ("the io word, loaded once in the entry block").

**2. `gpu/dxil/kernel.rs`**
- `translate_entry`: carry the seam exactly as the budget seam:
  `#[cfg(test)] let loop_counters_seam = crate::gpu::recomp::LOOP_COUNTERS_TEST.with(|c| c.get());`
  and inside the spawned closure `crate::gpu::recomp::LOOP_COUNTERS_TEST.with(|c| c.set(loop_counters_seam));`.
- `Lw::new`: in the struct literal (next to `uma_base_val: None,`) add
  `lc: crate::gpu::recomp::loop_counters().then_some(match entry { Entry::Pixel(_) => 1, Entry::Geometry(_) => 2, _ => 0 }),`
  and `lc_word: None,`.
- `translate_on_this_thread`, plain guard. Before:
  ```rust
  let cont = lw.block(Some(1));
  lw.cond_br(none, end, cont);
  ```
  After:
  ```rust
  let cont = lw.block(Some(1));
  if let Some(stage) = lw.lc {
      let trip = lw.block(Some(1));
      lw.cond_br(none, trip, cont);
      lw.s.m.set_block(trip);
      // the two-phase form continues into the GS after an ES trip: count a lane once
      let once = if two_phase { let d = lw.rd(super::lower::VAR_LC_DONE); Some(lw.cmpc(Pred::Eq, d, 0)) } else { None };
      lw.count_loop_trip(stage, once)?;
      if two_phase { let one = lw.c(1); lw.wr(super::lower::VAR_LC_DONE, one); }
      lw.br(end);
  } else {
      lw.cond_br(none, end, cont);
  }
  ```
  (`lw.br` — as the `Term::End => lw.br(end)` arm — so the SSA sees the edge.)

**3. `gpu/dxil/lower_window.rs`** (next to `uma_base`, same hoisting code):
```rust
/// The loop-counter word (`IO_LOOP_COUNTER` of the io buffer), loaded once in the entry block.
fn loop_counter_word(&mut self) -> Value;     // copy of `uma_base` with IO_LOOP_COUNTER and `lc_word`

/// Counters mode, in the block a loop guard leaves through: the lanes here (and `extra`) are
/// counted in their pipeline's word for `stage` – one atomic per hardware wave
/// (`WaveActiveCountBits`, never the split form's exchanging ballot). Writes no SSA variable.
pub fn count_loop_trip(&mut self, stage: u32, extra: Option<Value>) -> LResult<()>;
```
Body:
```rust
let word = self.loop_counter_word();
let nz = self.cmpc(Pred::Ne, word, 0);
let go = match extra { Some(e) => self.land(nz, e), None => nz };
self.guarded(go, &mut |lw| {
    let t = lw.s.m.i1c(true);
    let n = lw.s.wave_all_bit_count(t);
    let first = lw.s.wave_is_first_lane();
    let k = lw.lshrc(word, 28);
    let lo = lw.andc(word, 0x0FFF_FFFF);
    let off = lw.shlc(lo, 4);
    let off = lw.addc(off, 4 * stage);
    lw.guarded(first, &mut |lw| {
        if let Some(coherent) = lw.heap_mem {
            let inside = lw.cmpc(Pred::ULt, k, lw.mems.len() as u32);
            return lw.guarded(inside, &mut |lw| {
                let i = lw.addc(k, crate::gpu::recomp::MEM_HEAP_BASE);
                let m = lw.s.heap_raw_buffer_uav(i, coherent);
                lw.s.raw_atomic(m, AtomicOp::Add, off, n);
                Ok(())
            });
        }
        for (j, m) in lw.mems.clone().into_iter().enumerate() {
            let at = lw.cmpc(Pred::Eq, k, j as u32);
            lw.guarded(at, &mut |lw| { lw.s.raw_atomic(m, AtomicOp::Add, off, n); Ok(()) })?;
        }
        Ok(())
    })
})
```
(Explicit window selection instead of writing `VAR_WIN`: the compact form logs SSA writes per case
and stores them back; writing nothing keeps both forms safe. With one window only `k == 0` counts.
Import `AtomicOp` as `lower_mem.rs` does.)

**4. `gpu/dxil/compact.rs`, `emit_with_nest`, the guard's `out` block.** Before:
```rust
lw.s.m.set_block(out);
let end = lw.c(END);
```
After: insert between the two lines
`if let Some(stage) = lw.lc { lw.count_loop_trip(stage, None)?; }`
(a trip ends the program here: no once-flag). Note `count_loop_trip` leaves the builder in its
join block; the following `store(to, END)` / `br(target_latch)` then come from there, which is
what is wanted.

**5. Tests**
- `gpu/dxil/compact_tests.rs` (GPU): `gpu_direct_loop_counters_count_guard_exits`, skip as
  `gpu_direct_loop_budget_follows_the_variable` (`runner::skip_test("no GPU" | "no DXC")`). Program:
  its `looping(u32::MAX)` / `looping(100)`; `KernelOptions { wave64: false, split: false, mem_windows: 1, sampler_base: 0 }`
  (32 threads, runs on both vendors); `BudgetSeam::set(Some(64))` plus a counters seam guard;
  arena `crate::gpu::uma::Uma::new(1 << 20)`, `st.sgpr[120] = 0x4000 >> 4` (window 0);
  `translate_full_as(code, o, compact)` then `runner::run_in_arena(&k.dxil, &st, &arena)`; for
  `compact in [true, false]`: word at `base + 0x4000` == **32** on a 32-lane and on a 64-lane GPU
  (WaveSize 32 on both), the next three words 0; `looping(100)` → **0**; `st.sgpr[120] = 0` → 0.
- `gpu/recomp/heap_window_tests.rs`: extend 5e-1's forms list to all four
  `[(false,false),(false,true),(true,false),(true,true)]` (direct uses `kernel::translate`, which
  carries the seam): same expectation **64**, window 1 only.
- No unit test needed for the off case beyond the existing direct suite (`lc` is None ⇒ no code
  path changes); optionally assert `translate_full_as` output is byte-identical with the seam
  `Some(false)` vs `None`.

Estimate 5e-2: runtime ≈ 70 (lower 6, lower_window 45, kernel 18, compact 2), tests ≈ 45.
Total ≈ 115.

#### Not in 5e-1/5e-2
- 5e-3 desync word 3: HLSL at `xcl_x_dead = true;` in `xcl_x_meet`, direct at
  `lw.wr(VAR_XDEAD, d1)` in `lanes.rs`; same `xcl_lc_trip`-style once-per-wave atomic to word 3.
- `XCL_PER_LANE=1` programs have no guard: nothing to count.
- An `iterations` field (`XCL_LOOP_COUNTERS=2`).

---

### Changes to the design (summary)

1. **Region in the first committed window, not window 0** (P1). The io word carries the window
   index (`k << 28 | off >> 4`); generated code switches `xcl_w` / the window for the one atomic.
2. **Word 0 = do not count**, checked in the shader; no discard slot is written.
3. **Flag decided by a `OnceLock` from the environment** (`XCL_LOOP_COUNTERS` + `XCL_TELEMETRY=all`),
   not at `telemetry_start`, because `identity()` / `direct_output()` are `OnceLock`s that may be
   computed first.
4. **Slots per distinct shader set**, pointer → slot map overwritten at label (bounded, reuse-safe).
5. **One atomic per hardware wave** (`WaveActiveCountBits` by the first lane), same lane count.
6. **Direct path selects the window explicitly** (no `VAR_WIN` write; safe in the compact form), and
   needs a once-flag only in the two-phase split geometry form; HLSL keeps the static flag.
7. All HLSL entries are covered by one edit in `load_sgprs`; the design's three loaders were not
   the full list (8 calls).
8. Tests run in the real memory path (`run_in_arena` on committed windows), with the counter in
   window 1 for the multi-window forms.

### Alternatives rejected
- Window 0 with a byte offset (design): sparse VRAM, not CPU-readable, `alloc_in(0, ..)` fails.
- Two io slots (offset + window): one packed word suffices and keeps one bind line per site.
- A readback copy from a sparse/default-heap region with a fence: needs queue work per poll; the
  committed window gives a no-copy read.
- Reusing `VAR_WIN` in the direct path: SSA writes inside compact cases are logged and stored back.
- One atomic per lane: up to 64× more same-address host atomics in the low-budget case.
- Slots per pipeline pointer: unbounded with pipeline churn.
- `XCL_LOOP_COUNTERS` in `cache_allowed`: correct but retranslates everything each counters run.

### Uncertain
- Same-address atomics from many waves to host memory on NVIDIA in the heavy case: correct per the
  suite's arena atomics, cost not measured.
- PS early-Z loss when a PS gains a possible UAV write: not measured.
- The heap-form arena test path: confirm `heap_window_tests.rs` runs the heap form unconditionally
  on the dev GPU (it does today for its own test; reuse its gating).

### 5e refinements after the first PC runs (2026-11-26, orchestrator approved: internal, no change to what the user approved)

- Test correction, unit 5e-2: in `gpu_direct_loop_counters_count_guard_exits` the "loop that exits" run uses `looping(10)`, not `looping(100)`: under the test's budget of 64 a loop of 100 iterations does leave through its guard and is correctly counted (measured: 32). The exit must come before the budget is spent.
- Single-window HLSL prelude: the early return is `if (xcl_lc == 0u || (xcl_lc >> 28) != 0u) return;` (the spec text had the comparison inverted).
- Evidence that counters are on (a run with no trips must differ from counters that never worked): the manifest field `loop_counters` is `loop_counters_on()` instead of the constant false left by part 5b; one info log line when the region is allocated; at the final read one event `loop_counters` with `window` (the region's UMA window or null), `slots` (shader sets labelled) and `tripped` (slots with a non-zero word), plus one info log line. Written once per process, also with 0 slots or no region.
- Measured on an RTX 5080 (driver 32.0.16.2140), units 5e-1 and 5e-2 applied on 8b3397c2: `cargo test -p xcl --lib -- telemetry_p5e loop_counters loop_budget`: 13 passed; the heap-window GPU test reads 64 in window 1 on all four forms (HLSL and direct, root and heap), 0 for a loop that exits, nothing with word 0; whole suite 1571 passed, 4 failed (the known fuzz_validate_and_gpu_0..3), 47 ignored, 44 SKIPPED lines. Not tested on AMD.

## 2026-11-26 Telemetry part 3: fixes after the first PC run (orchestrator approved refinement of the parts 2 and 3 entry)

Part 3 as first written (pass engine in telemetry_gpu.rs + call sites C7-C22, uncommitted on 8b3397c2) builds; 38 of 39 gpu_tel tests pass on an RTX 5080. A 45 s title run at tier pass showed: exactly one pass per list and `frame.barriers` 0 in every frame; most passes `empty` while frames count 13 draws; 1,267 of 8,219 lists flagged `resubmitted`, 1,209 of them one list; pass and pipeline ids that do not join. An Opus debug worker traced each by reading (read-only); its report follows and is the binding fix spec for unit p3-fix. In it, TG = crates/xcl/src/telemetry_gpu.rs and D/ = crates/xcl/src/hle/d3d12x/.

### Orchestrator decisions on the open points

- All fixes A, B1 (F-B1a, F-B1b), B2, C (F-C1, F-C2, F-C3), D, E1, E2, E3 are approved as specified. They stay inside the design the user approved (fork-only engine file, one-line call sites in upstream files); the user-visible schema changes are the three below.
- B2 replaces section 7's thread-local counters with per-list shared counters: the thread-local design loses the counters of a list recorded on one thread and submitted on another, which is how the title works.
- `pass.pipelines` now means "pipelines drawn or dispatched with in the pass" (resolved pointers), not "bound or dispatched": the `set_pipeline_state` note is removed. `telemetry_draw` becomes `telemetry_draw(list: *mut c_void, pso: usize)`.
- `pass.kind` gets a fourth value: `indirect`, for a pass whose only work is ExecuteIndirect (draws == 0, dispatches == 0, indirect > 0). Order of precedence: `draw` if draws > 0, else `dispatch` if dispatches > 0, else `indirect` if indirect > 0, else `empty`. A pass with GPU work must not read as empty.
- New event `pipeline_bind` (tier pass): `id`, `stages`, `records` as in section D. `pipeline` (build) events keep their own ids; the join from a pass to shader records goes through `pipeline_bind`.
- `barriers` / `barriers_before` count the barriers the title passed in a batch (before XCL drops or rewrites any), per F-B1a.
- Two runtime checks are run by the orchestrator on the PC, not by the writer: A (the timing test alone with --test-threads=1) and B1 (a short run with XCL_D3D12_TRACE=1 counting title calls of ResourceBarrier #26 and #146). If B1 turns out to be case (b), a further call site is specified in a follow-up.
- docs/guide/telemetry.md gets these schema changes in the closing docs unit.

Tree: `/Users/osanchez/Documents/Development/reverse_engineer/xcl-adopt`, HEAD 8b3397c2 plus the uncommitted part 3.
I worked out everything below by reading the code. Nothing was built or run. Line numbers are for the current working tree.
"TG" means `crates/xcl/src/telemetry_gpu.rs`. "D" means `crates/xcl/src/hle/d3d12x/`.

---------------------------------------------------------------------------------------------------

### A. `one_dispatch_timed_through_the_pass_path_matches_dispatch_timed` (2.766 ms vs 0.938 ms)

#### What reading rules out (high certainty)
Both sides measure the same work and convert time the same way:
- **Same work.** Both use the same `dxil`, the same `groups` (256, 1, 1) and the same root signature. `compute_work` calls `pso::abi_root_signature(dev, false)` (TG:3627). `dispatch_timed` uses `gpu.rs`, which is `root_signature(&dev,false)`, which is that same `abi_root_signature` (runner.rs:126,131). u2 is a root UAV on a buffer in both. There is one `Dispatch` per rep in both (TG:3685 / runner.rs:1829) with no extra flush. `spin_hlsl` stores once per thread, so the store is negligible.
- **Same barrier position.** Both put a UAV barrier before every dispatch (TG:3684 `uav_all()`; runner.rs:1820, which skips k=0).
- **Slots are right.** Pass k runs from `ticks[k-1]` to `ticks[k]`; pass 0 starts at the list start and the last pass ends at the list end (TG:1871-1872). `PassRec::global` maps local slot j to `blocks[j/256]*256 + j%256` (TG:1393). `boundary` takes `global(total)` *after* pushing the block (TG:1501). `read_pass_ticks` reads `global(j)` for j < total (TG:1831). `record_resolve` resolves each block range into the pass readback at offset `start*8` (TG:503-505). There is no off-by-one, no base added twice, and the pass readback and the list readback are not mixed up. With 21 boundaries there are 22 passes, and pass k≥1 holds exactly barrier(k-1) plus D(k-1). The test's `ps[2..]` covers D1..D20, which matches `base[1..]`.
- **Same conversion.** Pass ticks go through `tick_ms` (TG:1164), which calls `xlog::tick_to_qpc` (log.rs:680, which is correct). List ticks use the same function. The frequency is the queue's own. Both are direct queues on the same adapter (the selector in TG:2814 is the same one `runner::init` uses).
- **Same thing measured.** `dispatch_timed`'s pair is [after barrier(k), after D(k)]. A pass is [before barrier(k-1), before barrier(k)]. With bottom-of-pipe timestamps (what NVIDIA and AMD do), the start stamp of either form lands when the previous dispatch has drained. So in steady state both measure exactly one dispatch plus a barrier. A pass is never 3 dispatches. If timestamps were top-of-pipe, `dispatch_timed` would read near 0, not 0.94 ms.
- **Physical plausibility.** 512 waves of a latency-bound chain of 100 k steps (IMAD + SHF + LOP3, about 12–15 cycles per step) is about 1.2–1.5 M cycles, roughly 0.5–1 ms at 1.5–2.6 GHz. 0.94 ms fits. 2.77 ms needs about 70 cycles per step, which only happens when the SMs are shared or time-sliced.

#### Root cause (ranked)
1. **(likely) Other GPU work ran on the device at the same time as the pass-path submission.** `dispatch_timed` holds `runner::GPU` (runner.rs:1661) while it runs. The pass path holds no lock (TG:3786-3790). The `gpu_tel` tests run in parallel under `cargo test`. The neighbouring test in alphabetical order, `passes_of_lists_on_two_queues_overlap...` (TG:3808), runs `spin_hlsl(1_500_000)` × 256 groups × 3 reps on a direct and a compute queue at once. That is about 40 ms of work per queue, about 15 times this kernel, and it starts at about the same time. Three contexts time-sliced on the GPU fit the observed factor of 2.95 well. The other GPU tests in the crate do not serialise with the rig either.
2. (unlikely) A GPU clock change between the two halves.

**Cheapest check:** `cargo test -p xcl --lib one_dispatch_timed_through_the_pass_path -- --test-threads=1 --nocapture`. If the ratio is about 1.0 when the test runs alone, cause 1 is confirmed. If it is still about 3, send it back with the printed per-pass spans.

#### Correct expected relation (must hold on any GPU)
With nothing else on the GPU, for each pass k≥2: `dispatch_timed(k) - ε ≤ pass(k) ≤ dispatch_timed(k) + max(15 %, 0.05 ms)`. The pass also contains its barrier, which has already drained, and one stamp.

Two relations hold even when other work shares the GPU. First, `min(pass[2..]) ≥ 0.85·min(timed)`, because contention only makes spans longer, so a pass shorter than the kernel means a slot bug. Second, Σ pass spans = list span exactly in ticks, because they tile the list by construction.

#### Fix (test only, TG tests module + one test-only line in runner.rs)
1. `crates/xcl/src/gpu/recomp/runner.rs`: add one line directly after `pub fn dispatch_timed(...) { ... }` (after runner.rs:1672):
   `#[cfg(test)] pub(crate) fn hold_gpu() -> impl Sized { GPU.lock().unwrap_or_else(|e| e.into_inner()) }`
2. TG tests module: add `static GPU_TIMING: Mutex<()> = Mutex::new(());`. Make the first statement of each of `one_dispatch_timed_through_the_pass_path_matches_dispatch_timed`, `passes_of_lists_on_two_queues_overlap_and_the_frame_takes_the_union` and `engine_stamps_around_a_fixed_workload_cost_what` this:
   `let _t = GPU_TIMING.lock().unwrap_or_else(|e| e.into_inner());`
3. In `one_dispatch...`, call `dispatch_timed` first, as now. Then wrap the pass path, from `submit_to(...)` through `settle(&r)`, in `{ let _g = crate::gpu::recomp::runner::hold_gpu(); ... }`. Do not hold the guard around `dispatch_timed`, because that call takes the same lock and would deadlock.
4. Replace the single assertion with these four:
   - `assert!(through.iter().cloned().fold(f64::MAX, f64::min) >= 0.85 * timed.iter().cloned().fold(f64::MAX, f64::min), ...)` (lower bound; catches slot bugs)
   - tiling: sum `(end_tick - start_tick)` over all 22 passes and assert it equals the list event's `end_tick - start_tick` (exact u64)
   - keep `assert!((a - b).abs() <= (0.15 * b).max(0.05), ...)` on the medians (now under both locks)
   - on failure, print every pass span so a rerun shows the shape.

---------------------------------------------------------------------------------------------------

### B1. No pass boundary is ever taken; `frame.barriers` is 0

#### Every way a ResourceBarrier reaches a PC list (from the code)
| Path | Route | Counted now |
|---|---|---|
| Title `ResourceBarrier`, console slot #26 (as on PC; `console_remap` changes only 28–55, 104–109 and 124–125, real.rs:861; `NEWER_LIST_SLOTS` does not contain 26, real.rs:7209) | `title_list` wrapper slot 26 → `forward` thunk (title_list.rs:84, a tail jmp, so the return address is still the title's) → the backing PC list's ext vtable[26] = `make_split(resource_barrier_x, pc)` (real.rs:1016, :805) → `list_x::resource_barrier_x` (list_x.rs:4245) | yes, but only `if !out.is_empty()` (list_x.rs:4324-4325) |
| Title console #146 `uav_barrier_x` (BVH builder) | list_x.rs:3124, which calls the PC method directly (:3132) | **no** |
| `console_list` (opt-in `XCL_CONSOLE_LISTS`, compute only) | replay at close | yes (console_list.rs:810, :870) |
| Runtime-internal calls on slot 26 (not from a title image) | `make_split` → PC | no (correct) |
| XCL's own barriers: list_x.rs `transition` helper :124, present.rs :1122-1175/:1792-1818, title_textures.rs :1319/:1648, vram.rs :2032, bind.rs ExecuteIndirect test :1271, split/kickoff predication transitions | direct PC calls | no (correct by design) |

`frame.barriers` is the delta of `BARRIER_BATCHES` (TG:2532, :2595). That counter is incremented only inside `telemetry_pass_boundary` (TG:2072), which is called only at the three sites above. **Zero in every frame means that not once in 45 s did a title batch reach `resource_barrier_x` with a non-empty `out`.** Reading cannot tell which of these is true:

1. (a) **The title calls #26, but every batch empties `out`.** Transitions whose before and after states become equal once `PC_STATES` and `BUFFER_ONLY_STATES` are masked are dropped (list_x.rs:4295-4310). Type > 2 or flags > 3 are dropped (:4266). The call is gated, so nothing is counted. This is possible in menus if the title's transitions use console-only state bits.
2. (b) **The title's barriers never come through #26** (another slot or layout; console extension slots are logged by the guard as "(not on PC)" or "(console extension)").

**Cheapest check:** run 10 s with `XCL_D3D12_TRACE=1` (real.rs:836-840 traces every public method a title image calls). Count `ResourceBarrier (#26) ←` lines, and look for `ID3D12GraphicsCommandList::#… (not on PC)` / `(console extension)` stub lines in the same seconds. If #26 calls are present, it is case (a) and the fix below fixes it. If there are none, it is case (b): send back the list of slots the title calls around its draws.

#### Fix (each one line)
- **F-B1a** `D/list_x.rs` `resource_barrier_x`. Count and stamp every title batch, as the design asks ("the title's batches"), whatever the PC records:
  - delete list_x.rs:4325 `crate::report::telemetry_pass_boundary(this, out.len() as u32);`
  - insert directly after the early return (anchor list_x.rs:4247-4249 `if barriers.is_null() || n == 0 { return; }`):
    `crate::report::telemetry_pass_boundary(this, n);`
  (`barriers_before` then means "barriers the title passed". A stamp before a batch that records nothing just splits the pass at the title's barrier point.)
- **F-B1b** `D/list_x.rs` `uav_barrier_x`. Insert before list_x.rs:3132 (`pc::<…>(this, PC_RESOURCE_BARRIER)(this, 1, &b);`):
  `crate::report::telemetry_pass_boundary(this, 1);`
- The console_list sites stay as they are.

#### Tests
None without a GPU is possible: these are wiring lines. Run the e2e `tests/testtitle` scenario if it records title barriers and check `frame.barriers > 0`. Otherwise check on a real run: `frame.barriers > 0`, and `pass` count > `list` count.

---------------------------------------------------------------------------------------------------

### B2. Draw attribution (13 draws per frame, about 3 in passes, most passes `empty`)

#### Root cause (certain, from the code)
1. **Counters stay in the recording thread's TLS.** `pass_note` → `tls_note` (TG:1611, :1720-1729) keeps one list's counters in a thread-local. They are flushed only in these cases:
   - when **that same thread** notes a *different* list (TG:1614),
   - at a pass boundary on that thread (TG:1743),
   - at `pass_take` on the **submitting** thread (TG:1815: `tls_flush` flushes only the caller's TLS).
   
   A list recorded on thread A and submitted by thread B therefore loses its counters at submit. If A records the same list again next frame (`c.list == list`, TG:1614), the counters are never flushed, because B1 means there are no boundaries. If A moves on to another list, the old counts are merged by `merge_counts` → `ensure` into a **new** recording of the old list (TG:1458-1463). They then end up on that list's next submission, a different recording. The list that does carry draws is the one recorded on the submitting thread.
2. **A draw site is missing.** console #81 `DrawIndexedX` (`list_x::draw_indexed_x`, list_x.rs:4129; newer layout #83) goes through `bind::prepare_indexed_draw` → `graphics_bound`, which counts `bind::COUNTS[DRAWS]`, the counter `frame.draws` uses (bind.rs:604). It never calls `telemetry_draw`. So `frame.draws` counts DrawIndexedX draws and passes never do.
3. **The pointers are right.** `this` at `bind.rs` draw/dispatch is the PC list: the wrapper thunk loads `[wrapper+F_BACKING]` (title_list.rs:84-86), which is the active PC segment. Submissions expand the wrapper with `title_list::push_segments` (real.rs:7381-7385, title_list.rs:249), which gives the same PC pointers (`first`/primary + `extra_active`). Deferred dispatches are recorded on `seg` at submit (list_x.rs:2761) and noted on `seg`. They are consistent.
4. **The PSO pointer at bind is the title's, not the real one.** `set_pipeline_state` notes the title's pointer (bind.rs:873). Under pso_async that is the stand-in key, while `translated_dispatch` notes the resolved pointer (bind.rs:1018). For graphics this gives two ids for one pipeline, and the id will not join D's label.

#### Fix: per-list shared counters (TG only, plus one upstream line move)
**Data (TG, replacing `TlsCache`, `TLS_COUNTS`, `tls_note`, `tls_flush` and `tls_discard`, TG:1599-1646):**
```rust
/// Counters of the current pass of one PC list: written by the thread recording it (no lock),
/// drained under the pass state lock at a boundary, at submit (any thread) and at Reset.
struct ListCounts {
    draws: AtomicU32,
    dispatches: AtomicU32,
    indirect: AtomicU32,
    pipes: [AtomicU64; MAX_PIPES],
    npipes: AtomicU32,
    truncated: AtomicBool,
}
impl ListCounts {
    fn new() -> ListCounts;                       // all zero
    /// single writer (the recording thread): appended when not among pipes[..npipes]
    fn note_pipe(&self, id: u64);                 // n = npipes.load(Acquire); if any pipes[i].load(Relaxed)==id return;
                                                  // if n < MAX_PIPES { pipes[n].store(id, Relaxed); npipes.store(n+1, Release) } else { truncated.store(true, Relaxed) }
    /// takes the counters (zeroed) as a `Counts`
    fn drain(&self) -> Counts;                    // swap(0, AcqRel) each counter; n = npipes.swap(0, AcqRel); pipes[..n] loads; truncated.swap(false, AcqRel)
}
const COUNT_SHARDS: usize = 16;
const LIST_COUNTS_MAX: usize = 65_536;
/// PC list pointer → its counters. An entry is never removed (a pointer reused by a new list
/// gets its counters zeroed at creation/Reset), so a cached Arc stays valid.
struct CountMap { shards: [RwLock<HashMap<usize, Arc<ListCounts>>>; COUNT_SHARDS], len: AtomicUsize }
impl CountMap {
    fn new() -> CountMap;
    fn shard(list: usize) -> usize { (list >> 6) % COUNT_SHARDS }
    fn get(&self, list: usize) -> Option<Arc<ListCounts>>;          // read lock
    fn get_or_insert(&self, list: usize) -> Option<Arc<ListCounts>>; // read; else write lock + insert while len < LIST_COUNTS_MAX; None when full
}
thread_local! {
    /// (engine address, list, its counters): the last list this thread noted
    static COUNT_CACHE: RefCell<(usize, usize, Option<Arc<ListCounts>>)> = const { RefCell::new((0, 0, None)) };
}
```
- `PassTel` gets the field `counts: CountMap` (initialised in `PassTel::new`).
- `GpuTel::pass_note(&self, list: *mut c_void, apply: impl FnOnce(&ListCounts))` (replaces TG:1720-1729):
  - gate `if !self.pass.on.load(Relaxed) || list.is_null() { return }`. Counting **no longer depends on sampling** (see E3). Only stamps do.
  - key = `(self as *const GpuTel as usize, list as usize)`. On a `COUNT_CACHE` hit, `apply(&arc)` with no lock. On a miss, `self.pass.counts.get_or_insert(l)`, store it in the cache, then apply. If the map is full, warn once (`"telemetry: more than {LIST_COUNTS_MAX} command lists - later ones get no pass counters"`) and return.
- Entry points (TG:2093-2141):
  - `telemetry_draw`: `t.pass_note(list, |c| { c.draws.fetch_add(1, Ordering::Relaxed); })`
  - `telemetry_indirect`: `c.indirect.fetch_add(1, …)`
  - `telemetry_dispatch`: `c.dispatches.fetch_add(1, …); if let Some(id) = pipe_id(pso) { c.note_pipe(id) }`
  - `telemetry_bind_pso`: `if let Some(id) = pipe_id(pso) { c.note_pipe(id) }`.
  - New signature: `pub fn telemetry_draw(list: *mut c_void, pso: usize)`. It counts a draw and notes `pipe_id(pso)` (see call site below).
- `pass_boundary` (TG:1734): in the first locked block, replace `tls_flush(&mut |li, c| st.merge_counts(li, c, frame));` with
  `if let Some(c) = self.pass.counts.get(l) { let d = c.drain(); if !d.is_empty() { st.merge_counts(l, d, frame); } }`
  This runs before `st.boundary` pushes the next pass, so the counts land in the pass that ends here.
- `list_reset` (TG:1776): replace `tls_discard(l);` with `if let Some(c) = self.pass.counts.get(l) { c.drain(); }`. Gate the whole `if pass` block on `self.pass.on.load(Relaxed)` as well.
- `pass_take` (TG:1812): drop `tls_flush`. For each list: `let d = self.pass.counts.get(l).map(|c| c.drain()).unwrap_or_default();` then `let mut rec = st.take(l, frame); if let Some(p) = rec.passes.last_mut() { p.merge(&d) }`. Drain on the `lock_busy` path too, merging into the `without_stamps` rec, so counts never spill into the next recording. On `Plan::Skip` (unsampled frame) the drained counts are dropped with the rec. That is correct.
- `PassState::merge_counts` stays as it is.

**Upstream call sites (one line each):**
- `D/bind.rs` `graphics_bound`: insert after bind.rs:554 `let real = super::pso_async::resolve_wait(pso);`:
  `crate::report::telemetry_draw(this, real);`
  This covers DrawInstanced, DrawIndexedInstanced and DrawIndexedX, which all pass through `graphics_bound` (bind.rs:500, :532 via :517 and list_x.rs:4139). The pointer is the resolved one, as at dispatch.
- delete bind.rs:498 `crate::report::telemetry_draw(this);` (draw_instanced) and bind.rs:516 (draw_indexed_instanced).
- delete bind.rs:873 `crate::report::telemetry_bind_pso(this, pso as usize);`. That is the title's stand-in pointer, and resolving it there has side effects (`pso_async::resolve` enqueues lazy builds). Keep `telemetry_bind_pso` in the API for future use.
- execute_indirect: see E1.

#### Tests (no GPU, TG tests module; replace the three TLS tests at TG:3259, :3293, :3491-3510)
- `counters_of_a_list_recorded_on_one_thread_reach_its_submit_on_another`: `tel = GpuTel::new(Box::new(|| None), Box::new(|_| None), 1, 8, None).with_pass(false)`. On a spawned thread (`std::thread::scope`), call `tel.pass_note(L, |c| { c.draws.fetch_add(3, Relaxed); })` and note one dispatch with `c.note_pipe(9)`. Join, then on the main thread `tel.pass_take(&[L], 1)`. Expect passes[0] to have draws 3, dispatches 1 and pipes [9]. A second `pass_take` gives an empty pass.
- `the_same_list_on_the_same_thread_every_frame_loses_nothing`: on thread A, note 2 draws on L, then on the main thread take, three times. Each take has draws 2. (Without the fix this was 0.)
- `a_boundary_splits_the_counters_of_its_list`: `pass_boundary` itself needs a device. Factor its counter drain out into `fn drain_into(&self, st: &mut PassState, l: usize, frame: u64)` and call that from `pass_boundary`. In the test: note 2 draws; with `let mut st = plock(&tel.pass.state)` call `tel.drain_into(&mut st, l, 1)` and then `st.boundary(l, 1, 1)`; drop the lock; note 3 draws; take. Expect passes[0].draws 2 and passes[1].draws 3.
- `a_reset_zeroes_the_counters`: note 4 draws, `tel.list_reset(L, true)`, take. Expect 0.
- `the_count_map_is_bounded`: `CountMap` at a small cap (make the cap a constructor argument `CountMap::new(cap)`). Inserting past the cap gives None, and get on an existing key still works.
- `note_pipe_keeps_first_seen_order_and_truncates` (as `the_pass_pipeline_list_is_bounded`, TG:3477, applied to `ListCounts`).

---------------------------------------------------------------------------------------------------

### C. `resubmitted` on one list (67600352) in every frame

#### Root cause (high certainty)
`resubmitted` means "the list is in `in_flight`" (TG:1029-1031). An entry is added at a stamped submit (TG:1046-1048) and removed only when its stamps are read, at frame ≥ set.frame+2 (TG:1236-1240), or by `list_reset` (TG:1776-1786). `telemetry_list_reset` is called only from `real.rs::cl_reset` (real.rs:2998). That is the PC-name `Reset` override used by **non-title** callers (`overrides` at real.rs:2898/:2911). **No title-side Reset reaches it.** The title paths are:
- console #10 on the wrapper → `title_list::reset` (title_list.rs:154) → `list_x::reset_pc` (list_x.rs:4186) → `alloc_pool::reset_list(… PC_RESET)` (list_x.rs:4193-4195), straight to the PC function;
- while primary is still on the GPU: `list_x::reset_stand_in` (list_x.rs:2645) → `reset_pc` on a spare;
- console #10 on an adopted PC list, or newer #139 → `list_x::reset_x` (list_x.rs:4174) → `title_list::reset` or `reset_pc`;
- `KickoffX`: `kickoff_x` submits `[this]` alone (list_x.rs:3403-3404) and then re-opens the list with `alloc_pool::reset_list(target, … PC_RESET)` (list_x.rs:3455-3458), with no hook;
- the mid-list split: a spare is re-opened through `alloc_pool::reset_list(spare, … PC_RESET)` (list_x.rs:2858-2861), or a new list is made with `create_raw_pc_command_list` (:2869);
- deferred segments: `record_deferred` (list_x.rs:2743) and `deferred_segment` (:3014) both go through `alloc_pool::reset_list(… PC_RESET)`;
- console_list: `cl_reset` (console_list.rs:423) → `pc_vtable_call(pc,10)`. From XCL, `make_split` picks the PC entry, which is real.rs `cl_reset`, so this path is covered.

So every title list that is Reset and submitted again before its stamps are read (within about 2 frames) is flagged. The flag on 67600352 is therefore **a false positive**. The pattern fits: a tiny list (about 1 µs of GPU work) has always finished by its next Reset, so `title_list::reset` reuses the same primary pointer every frame (no stand-in, title_list.rs:157). Its previous frame's entry is still in `in_flight`, so the next submission is flagged. Heavy lists are still on the GPU at their Reset, so they record on rotating spares (`first`), and different pointers escape the flag, except for the 58 other hits. Lists in the KickoffX shape (submitted alone in their own call) are flagged the same way.

**Cheapest check:** none is needed beyond the fix itself. After the hook, a list that really is resubmitted without a Reset stays flagged (existing GPU test `the_reset_makes_a_resubmission_a_new_recording...`, TG:3713). The real run's count for 67600352 must drop to 0.

#### Fix (one line each)
- **F-C1** `D/alloc_pool.rs` `reset_list`, directly after alloc_pool.rs:105 `let hr = reset(backing);`:
  `if hr >= 0 { crate::report::telemetry_list_reset(list as *mut c_void); }`
  This single choke point covers `reset_pc` (#10, #139, stand-in), `kickoff_x`, split spares, `record_deferred` and `deferred_segment`. It sits before the `!enabled()` early return, so it also runs with the pool off.
- **F-C2** `D/list_x.rs` `on_create`: insert after list_x.rs:809 `let key = list as usize;`:
  `crate::report::telemetry_list_reset(list);`
  A new list at the address of a released one is a new recording.
- **F-C3** `D/real.rs` `create_raw_pc_command_list`: insert before real.rs:3200 `(hr >= 0 && !out.is_null()).then_some(out)`:
  `if hr >= 0 && !out.is_null() { crate::report::telemetry_list_reset(out); }`
  This covers split and deferred lists made without `on_create`.
- Keep real.rs:2998 for XCL's and the runtime's own Resets through the vtable.

#### Tests
`a_reset_makes_the_next_submission_a_new_recording` (TG:3491) already covers the engine. Change its TLS assertion (TG:3507-3510) to assert `tel.pass.counts.get(l)` drains empty. The wiring lines are checked by a real run (resubmitted count for 67600352 = 0) and by the e2e `telemetry_e2e` "submitted twice before readback, no Reset" list, which must stay `true`.

---------------------------------------------------------------------------------------------------

### D. Pipeline ids that join to shader records

#### Verified
- `dred::label(pso: usize, shaders: &[(&str, Option<&[u8]>)])` (dred.rs:41; its own work is gated on `on()` = DRED, so the hook goes **before** the gate). It is called with `*ppv`, the **real driver pipeline**, after a successful build at every translated creation site:
  - compute dispatch-chain build (real.rs:4456),
  - compute whole/cached (real.rs:4493, reached by both the `warm` CachedWhole path and the `Whole` path through :4465-4471),
  - graphics VS/PS (real.rs:5402),
  - two-pass MS (real.rs:6278),
  - VS+GS (real.rs:6353).
- Bind sites see the same pointer: `translated_dispatch` gets `resolve_wait(pso)` (bind.rs:893-896). console_list uses `real` (console_list.rs:917). After B2, draws note `real` from `graphics_bound` (bind.rs:554). Stand-ins (`pso_async::resolve` returns the stand-in's driver pipeline while a build runs) get lazy ids with no label, which is correct: the stand-in is what ran.
- `pipeline` events cannot carry the pointer: `driver_compile_identified` (real.rs:4146) runs inside the build closure, and graphics builds report `ps_hash`, retries emit two builds, and so on.

#### Decision: a new event `pipeline_bind` (tier pass)
I chose this over adding `label: true` to `pipeline` for three reasons:
- `pipeline` already has two shapes: a build (TG:2251) and the tier-frame refusal in report.rs:517. A third shape with null build fields would make readers count it as a build.
- A label happens for cache hits too, where nothing is compiled.
- A pointer can be relabelled after reuse, and a separate event says so plainly.

Fields:
- `id` u64: the pointer's telemetry id, the same space as `pass.pipelines`.
- `stages`: array of stage names (`"cs"`, `"vs"`, `"ps"`, `"ms"`, `"gs"`).
- `records`: array of 16-hex `fnv64` strings, parallel to `stages`. Stages without bytes (`None`) are omitted from both.

A reader joins `pass.pipelines[i]` → `pipeline_bind.id` → `records` → `shaders.json`, which is keyed by the same fnv64. It reaches compile cost through `pipeline.records` (compute: `fnv64(cs)`, real.rs:4447; graphics: `ps_hash`).

#### Code (TG)
- Move the map static out of `pipe_id` (TG:2213) to the module: `fn pipe_map() -> &'static Mutex<PipeMap>`.
- Add `static PIPE_GEN: AtomicU64 = AtomicU64::new(0);`. Change `LAST_PSO` to `Cell<(usize, u64, u64)>` (pso, id, gen). In `pipe_id`, the cache hit becomes `last == pso && gen == PIPE_GEN.load(Ordering::Acquire)`. Store the current gen on set.
- `impl PipeMap { fn relabel(&mut self, ptr: usize, seq: &AtomicU64) -> Option<u64> }`: if `self.ids.contains_key(&ptr) || self.ids.len() < self.cap`, set `id = seq.fetch_add(1, Relaxed)`, `self.ids.insert(ptr, id)` and return `Some(id)`. Otherwise None.
- 
```rust
/// A translated pipeline was made at `pso` (the real pipeline) from these console shaders: the
/// pointer gets a NEW telemetry id (a reused pointer is a new pipeline) and a `pipeline_bind` event.
pub fn telemetry_pipeline_label(pso: usize, shaders: &[(&str, Option<&[u8]>)]) {
    if !xlog::telemetry_on(Tier::Pass) || pso == 0 { return; }
    let id = match lock_wait(pipe_map(), Duration::from_millis(50)) {
        Some(mut m) => m.relabel(pso, &PIPELINE_SEQ),
        None => None,
    };
    PIPE_GEN.fetch_add(1, Ordering::Release);       // other threads' LAST_PSO caches go stale
    let Some(id) = id else { return };              // (warn once: map full or busy)
    let (stages, records): (Vec<Value>, Vec<Value>) = shaders.iter()
        .filter_map(|(k, b)| b.map(|b| (Value::from(*k), Value::from(format!("{:016x}", crate::gpu::shader::fnv64(b))))))
        .unzip();
    pass_sink("pipeline_bind", vec![("id", Value::from(id)), ("stages", Value::Array(stages)), ("records", Value::Array(records))]);
}
```
- **Upstream call site:** `D/dred.rs` `label`, as the first statement, before dred.rs:42 `if !on() || pso == 0 {`:
  `crate::report::telemetry_pipeline_label(pso, shaders);`
- Pointer reuse: a pipeline released and a new one made at the same address is labelled again at creation, gets a new id, and `PIPE_GEN` invalidates every thread's cache. A pass recorded before the relabel keeps the old id. A bind that comes after creation but before the label is impossible, because label runs inside creation.

#### Tests (no GPU)
- `a_relabel_gives_a_reused_pointer_a_new_id` (`PipeMap` unit):
  - `id_of(p)` = a, then `relabel(p)` = b ≠ a, then `id_of(p)` = b.
  - At the cap, `relabel(unknown)` is None and `relabel(known)` is Some(new).
- `the_thread_cache_follows_a_relabel`: use a unique pointer such as 0x7E57_0000. `pipe_id(p)` = a. Call relabel through the internal helper (factor `fn pipe_relabel(pso) -> Option<u64>` out of `telemetry_pipeline_label`); it returns b. `pipe_id(p)` is then b on this thread, and on a spawned thread too.
- `pipeline_bind_fields`: factor the field building out into `fn pipeline_bind_fields(id, shaders) -> Vec<(&'static str, Value)>`. With `[("vs", Some(&[1,2,3])), ("ps", None)]`, expect stages ["vs"] and records [16-hex of fnv64([1,2,3])].

---------------------------------------------------------------------------------------------------

### E. Small items

**E1. ExecuteIndirect.** `D/bind.rs:270`, the first statement of `execute_indirect`. Replace `crate::report::telemetry_draw(this);` with:
`crate::report::telemetry_indirect(this);`
This matches design C15 ("counted as `indirect`"). Note that `Counts::kind()` (TG:1338) has no `indirect` case, so a pass with only indirect work is `empty`. See CLAIMS.

**E2. Two-queue overlap test** (TG:3858-3863). Replace
`assert!(a.0.max(b.0) < a.1.min(b.1), "the two queues did not overlap: …");` and `assert!(union < sum, …)` with:
```rust
let overlap = (a.1.min(b.1) - a.0.max(b.0)).max(0.0);
println!("overlap {overlap:.4} ms ({})", if overlap > 0.0 { "the queues ran together" } else { "this GPU serialised them" });
assert!((union - (sum - overlap)).abs() <= 1e-6, "union {union} ms != sum {sum} ms - overlap {overlap} ms");
assert!(union >= (a.1 - a.0).max(b.1 - b.0) - 1e-6 && union <= sum + 1e-6);
```
This holds on hardware that overlaps the two queues and on hardware that serialises them. Also take `GPU_TIMING` (A.2).

**E3. Sampling.**
- Now (TG:1663, :1722, :1736): counters and stamps are both gated by `pass_sampled()`. That breaks counters at sample > 1. A recording that starts in an unsampled frame misses its earlier draws. Counts cached in a thread's TLS during a sampled frame are not flushed in unsampled frames (`pass_note` and `pass_boundary` return before touching the TLS), and they spill into a later recording.
- After B2: counting is gated by `pass.on` only, and stamps by `pass_sampled()`. A recording that crosses into a sampled frame gets its earlier counts in its pass 0, because the counts stay in the open pass until the first stamped boundary. A submission in an unsampled frame drains and drops its counts in `pass_take`, so nothing leaks into the next recording.
- Test (no GPU): `counting_does_not_depend_on_sampling`. Use `GpuTel::new(…, 2 /*sample*/, 8, None).with_pass(false)` and `set_frame(1)` (unsampled). Note 2 draws on L, `set_frame(2)`, note 1 draw, then `pass_take(&[L], 2)`: passes[0].draws == 3. Then `set_frame(3)`, note 1 draw, take: 1 (no spill-over).

---------------------------------------------------------------------------------------------------

### Upstream one-line edits (summary)
| # | File | Function | Anchor | Edit |
|---|---|---|---|---|
| 1 | D/list_x.rs | resource_barrier_x | after `if barriers.is_null() \|\| n == 0 { return; }` (:4247-4249) | add `crate::report::telemetry_pass_boundary(this, n);` and delete :4325 |
| 2 | D/list_x.rs | uav_barrier_x | before :3132 | add `crate::report::telemetry_pass_boundary(this, 1);` |
| 3 | D/bind.rs | graphics_bound | after :554 `let real = super::pso_async::resolve_wait(pso);` | add `crate::report::telemetry_draw(this, real);` |
| 4 | D/bind.rs | draw_instanced / draw_indexed_instanced / set_pipeline_state | :498, :516, :873 | delete the three telemetry lines |
| 5 | D/bind.rs | execute_indirect | :270 | `telemetry_draw(this)` → `telemetry_indirect(this)` |
| 6 | D/alloc_pool.rs | reset_list | after :105 `let hr = reset(backing);` | `if hr >= 0 { crate::report::telemetry_list_reset(list as *mut c_void); }` |
| 7 | D/list_x.rs | on_create | after :809 `let key = list as usize;` | `crate::report::telemetry_list_reset(list);` |
| 8 | D/real.rs | create_raw_pc_command_list | before :3200 | `if hr >= 0 && !out.is_null() { crate::report::telemetry_list_reset(out); }` |
| 9 | D/dred.rs | label | before :42 | `crate::report::telemetry_pipeline_label(pso, shaders);` |
| 10 | gpu/recomp/runner.rs | (test-only helper) | after `dispatch_timed` (:1672) | `#[cfg(test)] pub(crate) fn hold_gpu() -> impl Sized { GPU.lock().unwrap_or_else(\|e\| e.into_inner()) }` |

### Open points for the orchestrator
- B1: whether the title calls #26 at all is undecided until the `XCL_D3D12_TRACE` check is run. F-B1a and F-B1b are right either way, but if the cause is (b), they will still leave the count at 0.
- Removing the `set_pipeline_state` note changes `pass.pipelines` from "bound or dispatched" (design §6, decisions.md:2226) to "drawn or dispatched with".
- Whether `Counts::kind()` gets an `indirect` value (a schema change) is the orchestrator's decision. This spec leaves `kind` as it is.
- `pipeline_bind` is a new event name for schema §6 and docs/guide/telemetry.md.

### Part 3 fixes: measured results and the last open point (2026-11-26, orchestrator)

Measured on an RTX 5080 (driver 32.0.16.2140), part 3 with the fixes on 96388e40:
- A confirmed as test contention: under the locks the pass path median is 0.9380 ms against dispatch_timed 0.9380 ms; the engine's interleaved stamps change a fixed 24-list workload by +0.0008 ms.
- B1 was case (a): the title's barrier batches all emptied `out` after XCL's state masking. With F-B1a a 110 s title run (3,157 frames) has `frame.barriers > 0` in 3,154 frames (6 in a typical menu frame, at most 12) and 40,685 passes over 21,851 lists.
- B2: sum of pass draws 39,301 equals the sum of frame draws; dispatches 3,047 equal 3,047. D: all 6 distinct pipeline ids in passes have a `pipeline_bind` event (1,322 written).
- C: the rotating title lists are no longer flagged. One list stays flagged in every frame after its first submission; section F below shows the flag is right (the title re-executes one closed, empty list it recorded at start, real.rs comment on queue #35). Decision: no change. Rule O-2 stays as written; the optional `rec.total > 0` relaxation is not taken. docs/guide/telemetry.md will say that a title may legitimately re-execute a closed list and that `gpu_drops.resubmitted` then counts those submissions.
- Tests: cargo test -p xcl --lib gpu_tel: 49 passed; cargo test -p xcl --test telemetry_e2e: 8 passed; whole suite 1603 passed, 4 failed (the known fuzz_validate_and_gpu_0..3), 47 ignored, 44 SKIPPED lines. Not tested on AMD.
- D7 overhead (test title, --frames 600 --uncapped, 5 runs per mode, release, medians): off 107.9 ms; frame 169.9 ms (+0.10 ms per frame); frame with XCL_TELEMETRY_SAMPLE=10 139.6 ms (+0.05 ms per frame); pass 179.7 ms (+0.12 ms per frame). The title's frames take 0.13 ms, so the percentages (57 %, 29 %, 66 %) are a worst case; on the retail title's 33 ms menu frames the same cost is about 0.3 to 0.4 %. Recommendation for docs: tier pass can stay on for investigation runs; smoke comparisons run with telemetry off.

### F. The one list still flagged after C

Tree: xcl-adopt at HEAD 96388e40 plus part 3. I worked this out by reading the code only.

#### Verdict: (1). The title really submits one recording every frame without a Reset, so the flag is correct (fairly high certainty)

**Evidence that it is (1):**
- **The retail pattern is already described in the code.** The comment on the newer layout's queue #35 says "a retail game submits all its command lists this way (flags 3; **0 for the one it recorded at start**)" (real.rs:7293-7294). That is a list recorded once at start-up and submitted again on later frames. The run's shape matches it:
  - 3,158 submissions, and only the first is unflagged;
  - the same pointer every frame, 0 draws, 0 dispatches, 0 barriers, about 1 µs on the GPU;
  - always the first list of the frame.
- **Every Reset a title can make of a wrapper list is now hooked.**
  - #10: `title_list::reset` (title_list.rs:154) → `reset_pc` (:165) → `alloc_pool::reset_list` → the hook at alloc_pool.rs:106.
  - Stand-in: `list_x::reset_stand_in` → `reset_pc`, which reaches the same hook.
  - #139 and #10 on the PC list: `list_x::reset_x` → `title_list::reset` / `reset_pc`.
  - The only unhooked branch is a non-adopted primary (title_list.rs:167, raw `pc_slot(primary, 10)`). It cannot happen for a title list, because `device_x.rs:117-119` wraps only lists returned by XCL's creator, which adopts them.
  - The same hook demonstrably clears the four rotating lists. So if the title Reset 0x414E0C0, the hook would have removed it from `in_flight`.
- **Why it is alone in its own `chunk`.** Two explanations fit, and neither involves XCL submitting the list itself:
  - The title submits it in a separate `ExecuteCommandListsX` call (the flags-0 call).
  - Or it comes first in the same call, and `execute_with_waits_on` (list_x.rs:2166-2174) starts a new chunk at the next list, because that list has an unsatisfied `Wait64BitValueX` (the one #74 per frame in the trace). That submits `lists[0..1]` on its own.

**Why it is not (2):**
- The array passed to `telemetry_execute` with origin `submit` contains only title lists. It is built in `q_execute_command_lists` (real.rs:7378-7392) from `console_list::pc_list`, `title_list::push_segments` or the pointer itself. No XCL-owned list is ever added to it.
- The other two `telemetry_execute` sites are both in `kickoff_x` (list_x.rs:3385, :3404). KickoffX is not in the trace: there is no newer #95 (older #93) call. Also, `kickoff_x` re-opens the list through `alloc_pool::reset_list` (:3455), which is hooked.
- Split segments: there are no split log lines in this run. Spare and new segments go through `reset_list` (:2858) or `create_raw_pc_command_list` (real.rs:3200), and both are hooked.
- `title_textures::refresh` and the present lists do not go through this path.

**Why it is not (3):**
- `list_reset` only *removes* the list from `in_flight` (TG:1776-1786). `execute` checks and inserts under the same `in_flight` lock (TG:1025-1048).
- A Reset followed by a submit therefore always leaves the submit unflagged.
- The only way to get a wrong flag would be a Reset that never happens. A race could make a flag *disappear*, but it could never make one appear.

#### What to do
- No hook is missing. The flag is the design's O-2 rule ("the same recording again") working as intended.
- **Optional engine refinement (an O-2 change, the orchestrator's decision).** A resubmitted recording with no in-list stamps does not need null pass ticks. Its one pass is exactly the list span, and the list stamps stay valid. Edit in TG `pass_facts`: change `} else if c.resubmitted {` to `} else if c.resubmitted && rec.total > 0 {`.
  - The first submission's split passes stay poisoned through `poisoned`, because their stamps can be overwritten.
  - Test (no GPU): `pass_facts(&PassRec::new(1, 1), &PassCtx { resubmitted: true, start: Some(10), end: Some(20), ..ctx() }, &|t| Some(t as f64))` returns one pass with ticks 10..20 and `why` None. A rec with `total = 1` still gives `why: "resubmitted"`.
- **Cheapest runtime check: none exists that decides this directly.** Close and Reset on wrapper lists are not traced, `XCL_LIST_TRACE` logs only when a Close fails, and the #35 log is written once. The check needs one temporary line, which the orchestrator must allow first: `crate::xlog_info!("title_list reset {primary:p}")` at title_list.rs:170 (`if hr >= 0 {`). If `0x414E0C0` never appears after start-up while the title keeps submitting it, (1) is confirmed.

## 2026-11-26 Stall fix: frames through loading and into the world by default (user approved all units, and "wait" as the default for compute)

Approved by the user on 2026-11-26 after runs 1-6 on the RTX 5080 (driver 32.0.16.2140, build
99a76e3e). Decisions the user made: approve all units; by default a compute pass whose pipeline
is still building is waited for (option C1), skipping stays the opt-in `XCL_PSO_WAIT_MS`.
Adoptions through the gate: #2579, #2098, #2698 and #751. Units below keep their ids (U0-U9).
The draft was written by an Opus design worker and reviewed by the orchestrator (citations
spot-checked: `bind.rs:270-278`, `bind.rs:586-592`, `pso_async.rs:214-223`, `run3/log.txt:1186`,
`1228`, `1397-1411`).

Status: draft for review. Nothing here is built or run. Paths are `crates/xcl/src/...` on `dev`
(094cbbbb, same code as the runs' 99a76e3e). Run data: `/Users/osanchez/xcl-pc/out/runs/runN/`.
Upstream PRs were read as text with `gh pr view/diff -R xcl-dev/xcl` on 2026-11-26.

### Summary (recommendation first)

**Recommendation.** Write our own changes for draws, the gate and the worker pool. Adopt only
three small upstream PRs through the gate: #2698, #2579 and #2098. Do not adopt #1798, #2615,
#1931 or #2114.

1. **Draws (option A): no title thread ever builds a pipeline.** A draw or graphics
   ExecuteIndirect that binds an unbuilt pipeline waits only inside a **shared per-frame budget
   of 100 ms** (all threads together, reset at each present). Then it gets the stand-in, which
   draws nothing. The build goes to the front of the workers' queue, marked wanted.
2. **Compute (options B, C): keep waiting at the submit, but only for a worker.** The submit
   thread never builds. Put-off builds are wanted from the moment they are put off (#2579).
   Preloads may hold at most one big-compile slot, and none while a wanted build waits. Slot
   defaults scale with cores (#2098) and with RAM (ours). Big compute programs that write only
   images are skipped by default (`XCL_PSO_IMAGE_SKIP` on). A 120 s safety valve skips a dispatch
   whose build never finishes.
3. **The 90-110 s gap in run 3 has two causes.** The first is the per-bind 50 ms budget, paid
   once per new pipeline, one after another: 335 graphics waits took 14.7 s and then 256 or more
   compute waits took 12.9 s. Item 1 removes this. The second is **worker starvation**: at the
   end all 15 `xcl-pso` workers sat on huge compute builds (6 in the NVIDIA compiler, 9 parked
   in `big_compile_turn`), so no graphics pipeline was built after 100 s. Items 1 and 2 do not
   fix that. New unit U7 does: a worker that parks at the gate is replaced by an extra worker.
4. **`XCL_RUN_SECONDS` / `XCL_STALL_DUMP` always end the run.** Each arms a terminate watchdog
   first, before the log line, the report and `end_title`.
5. **Telemetry proves it.** New `frame` and `activity` fields: draws and dispatches left out
   for unbuilt pipelines, wait time split into draw and dispatch, builds on title threads
   (target 0), and workers parked at the gate. `pipeline` build events get `by` and `wanted`.

Nothing is keyed on a vendor. On a GPU whose driver compiles fast, the budget is rarely used up,
draws are rarely left out, compute waits are short and no extra workers start.

### 1. Goal and scope

**Goal.** With default switches, the title keeps presenting through its loading screen and
reaches in-world frames. No title thread enters DXC or the driver compiler for a pipeline build.

**In scope.**
- What a draw, a graphics ExecuteIndirect, a dispatch and a dispatching ExecuteIndirect do
  when their pipeline is not built.
- Who builds pipelines, in what order, and how many huge compiles run at once.
- Making `XCL_RUN_SECONDS` and `XCL_STALL_DUMP` always end the process.
- Telemetry that shows all of the above per frame and per 2 s.

**Out of scope (stated explicitly).**
- The texture-upload crash at 131.97 s in run 3 (`run3/log.txt:1288`). Another worker owns it.
- DirectStorage submit timing.
- The unmodelled queue method `ID3D12GraphicsCommandList10::#101` (`hle/d3d12x/real.rs:775-780`).
- Heap windows (option D, #2427/#2428, `XCL_HEAP_WINDOWS`).
- Pipeline persistence (option E, #758/#759/#1006/#2001).
- Shader size and the cost of each compile.
- `XCL_PSO_WAIT_MS` stays as an opt-in switch with today's meaning. Its put-off problem (#2581)
  is not fixed here.

### 2. Architecture and data flow

#### Today

- **Create.** With lazy pipelines (default, `hle/d3d12x/pso_async.rs:383-386`) the title gets a
  stand-in. The build is stored, not queued (`submit_lazy`, `hle/d3d12x/real.rs:5748`).
  Pipelines an earlier run used go to 2-4 `xcl-pso-pre-*` threads at the lowest priority
  (`pso_async.rs:509-563`).
- **SetPipelineState** calls `resolve` (`real.rs:7062`). This sends the build to the back of the
  workers' FIFO (`pso_async.rs:1057-1069`, `enqueue` at `322-324`).
- **Draw.** `graphics_bound` calls `resolve_wait` (`hle/d3d12x/bind.rs:552`). With
  `XCL_PSO_WAIT_MS` unset there is no budget (`pso_async.rs:1125-1128`). If no worker has
  started the job, the draw takes it and builds it on the title thread (`pso_async.rs:1100-1104`),
  then waits without a limit (`1106-1110`).
- **Graphics ExecuteIndirect** does the same through `resolve_wait` (`bind.rs:278`).
- **Dispatch.** A pending compute pipeline's dispatch is put off to the submit
  (`bind.rs:879-881`, `list_x.rs:2704-2714`) and `need_by_submit` puts the build in `NEEDED`
  (`pso_async.rs:1377-1384`). Workers drain `NEEDED` before each FIFO item (`pso_async.rs:312-314`).
  At the submit, `record_deferred` marks the next 4 put-off builds wanted (`list_x.rs:2717-2729`)
  and records each call through `bind::dispatch` → `resolve_wait` (`list_x.rs:2762`). With no
  budget the submit thread may again build the job itself.
- **Gate.** A driver compile of 256 KB or more on an `xcl-pso*` thread with no driver blob waits
  in `big_compile_turn` (`real.rs:4148-4150`, `4194-4206`). Unwanted: 1 slot
  (`real.rs:4072-4075`) and 3 GB free commit (`real.rs:4213-4229`). Wanted: 2 slots
  (`real.rs:4081-4087`), no commit check. 4 MiB and more: a lane of 1 (`real.rs:4124-4129`).
  Graphics and compute share the 256 KB lane (`real.rs:5378`). Preloads compete for it as
  unwanted builds.
- **A worker waiting at the gate keeps its thread.** The pool has `compile_threads()` workers
  (cores - 1, `gpu/recomp/dxc.rs:409-415`; `pso_async.rs:300-320`). Nothing replaces a parked one.

#### What the runs show

| Run | Switches | Result | Cause in the data |
|---|---|---|---|
| 1 | default | 0 presents from 201.7 s to the copy at 293 s | title thread 18196 builds graphics in its draws, 24 holds of 1.4-6.5 s back to back (`freeze-analysis.md` Q1) |
| 2 | default, soak | last frame 42 s after "press A", then 1,322 s with none, closed by hand | 316 graphics waits, 1,281.8 s in all, 294 built in the call, longest 19.5 s (`run2/log.txt:4631`) |
| 3 | E4: `XCL_PSO_WAIT_MS=50`, `XCL_HUGE_PSO_CONCURRENCY=4`, `..._WANTED_CONCURRENCY=6`, `XCL_PSO_PRELOAD=0` | frames to ~86.7 s, one frame of 32.2 s (`frame` i=2121, `pso_wait_ms` 33,304), then 13 in-world frames to 132 s; crash at 131.97 s | see section 2.1 |

#### 2.1 The run 3 gap (item 3)

Times are from `run3/log.txt` and `run3/events.jsonl`.

1. **86.8-100 s: graphics waits, one at a time.** The render thread (31276) waits 50 ms per new
   graphics pipeline (first at `log.txt:810`). By 100.26 s: 335 graphics waits, 14,708 ms, longest
   51 ms, 0 built in the call (`log.txt:1186`). Activity shows 0 presents and 0 submits from
   90 s (`log.txt:1144`). The title records about one draw per wait.
2. **100-113 s: compute waits at the submit, one at a time.** The submit thread (36692) records
   the put-off dispatches. Each waits 50 ms. 256 compute waits, 12,854 ms, by 112.94 s
   (`log.txt:1228`).
3. **Meanwhile the workers build compute, not graphics.** From 86.7 to 100 s the workers made
   336 compute pipelines (184 from blobs) and only 28 graphics. From 100 to 140 s they made
   **0 graphics** pipelines (`pipeline` events). Put-off compute goes through `NEEDED`, ahead of
   the FIFO that holds the graphics builds.
4. **At the crash (132 s) every worker holds a huge compute build.** `xcl-pso-0, -2, -3, -5, -6,
   -11` are in the NVIDIA compiler. The other nine wait in `big_compile_turn`
   (`log.txt:1397-1411`). No worker is free for a graphics build. That is why 110-132 s show
   280-357 draws per frame but thousands left out.
5. Commit grows from 30.4 to 34.7 GB with 0 submits (90-118 s). That is the driver compiles.

**Do items 1-2 remove it?** Item 1 (shared 100 ms budget) bounds cause 1 to 100 ms per frame.
Cause 2 changes: compute waits until built (item 2), so its length becomes the build time of the
compute the submit needs. Cause 3-4 is **not** removed by items 1-2. U7 (extra workers when
workers park at the gate) is needed. U6 (graphics get their own big-compile lane) keeps a
graphics build from waiting behind compute slots.

#### After

```
SetPipelineState ── resolve ──> FIFO (as today)
Draw / graphics EI ── resolve_draw ──> built?  yes: real pipeline
                                       no:  want + NEEDED front; wait <= frame allowance (100 ms
                                            shared per present); then stand-in, draw left out
Dispatch / compute EI ── pending? ──> put off (as today) + wanted now (#2579)
    submit: record_deferred ── resolve_wait ──> want + NEEDED front; wait for the worker
                                               (never build here); after 120 s: skip, warn
    image-only big program: never waits, never put off, skipped until built
Workers: NEEDED first, then FIFO. A worker that must wait at the gate parks; if fewer than
    n workers are unparked, one extra worker starts (at most 2n threads).
Gate: compute lane (base / base+1), graphics lane (base / base+1), giant lane (1).
    Preloads: at most 1 slot of a lane, and none while a wanted build waits.
    base = min(clamp(cpus/4, 1, 4), max(1, RAM GiB / 12)).
```

### 3. Dependencies

#### 3.1 Upstream PRs, one by one

| PR | State | Size | Tests | Applies to dev? | Review comments | Defects I see | Decision |
|---|---|---|---|---|---|---|---|
| #1798 draws use `resolve_for_bind` | open, CONFLICTING | +38 -4, `bind.rs` | 1 unit test, timing based (1.5 s sleep) | No. Its `bind.rs:253-256` hunk predates our `telemetry_indirect` call site and #2118's put-off (`bind.rs:270-277`); its `graphics_bound` hunk predates `telemetry_draw` (`bind.rs:553`) and `two_pass_of` | Linux RADV: graphics wait gone, title then stuck on compute builds on the title thread. RTX 3060 Windows: past main's stop, then `DXGI_ERROR_DEVICE_HUNG` at 144.9 s (not tied to it). | No wait at all, no budget. It does not mark the build wanted, so a huge graphics build stays unwanted at the gate. Compute still builds on the title thread (`resolve_for_bind` → `resolve_wait`). | **Do not adopt.** Take the idea. |
| #2615 a bind never builds on the title thread | open, MERGEABLE vs main | +179 -86, 7 files | 1 new test, list_x and e2e tests changed | No. Its `resolve_wait_for` hunk removes the inline-build block that our #899 adoption changed (`already`, `OpenWait`, `note_blocked`, `pso_async.rs:1086-1122`; merge `db65000d`). | AMD RDNA2: presents to 361 s, 12-14 % of dispatches skipped. Linux RTX 4060 Ti with #2404: 18 of 18 runs lost the device (one control without it also did). Linux RTX 4070: 1 of 3 lost the device. | It makes **compute** skip by default and turns off the put-off for everyone (`waits_until_built()` false). That ignores the hang warning at `pso_async.rs:1168-1169`. Good parts: it wants the build before sending it, and no title thread builds. | **Do not adopt.** Take `resolve_unwaited`'s order (want, then send). |
| #1931 opt-in `XCL_PSO_FRAME_WAIT_MS` | open, CONFLICTING | +124 -7 | 1 unit test | No (old `pso_async.rs` base, before shared stand-ins) | RTX 4090: past the stop at 500 ms, then a crawl; 163 `queue.Wait` watchdog lines; peak commit 46.9 GB; one crash with a warm cache. | The budget is taken after the wait ends, so several threads can each wait the whole remainder at once. Opt-in only. | **Do not adopt.** Own budget in U4. |
| #2114 per-frame cap | open, CONFLICTING | +474 -39 | 6 unit tests | No | Reviewer: the cap is per thread (`thread_local!`), not per frame; it changes the default to skip compute (250 ms / 2,000 ms). RTX 4080: no device removal, presents to 134 s. | Over the 400-line limit. Skips compute by default. | **Do not adopt.** |
| #2579 put-off builds are wanted at once | open, MERGEABLE | +26 -20, 3 files | `a_build_a_submit_needs_is_wanted` (red without the change, confirmed by a reviewer on RX 7800 XT) | Yes. Its `record_deferred` context matches `list_x.rs:2716-2729`. `want_bound` / `pso_async::want` have no other callers (`bind.rs:125-127`, `list_x.rs:2728`). | Collaborator: "good within the reviewed scope". Gate waits after `#101` fell from 1,064-1,079 s to 161-342 s (cap 6, Linux). A reviewer notes it competes with every put-off build at a cap of 2. | More builds skip the commit check at once (least free RAM 7-8 GB against 13 GB). Bounded by the wanted cap. | **Adopt** (gate). |
| #2098 slots scale with CPUs | open, MERGEABLE | +170 -39, `real.rs`, README | 4 new unit tests, reservation tests | Probably. Its `real.rs` hunks sit at `real.rs:4066-4214` on our tree; the gate's merge check decides. | RTX 5070 Ti: 7 programs 87.5 s → 38.8 s at 4. RDNA4: 2.6x at 4, but **2.3 GB per extra build**. A 16-thread RTX 5080 peaks at 4. | Wanted builds still skip the commit check (they reserve, though). No RAM term: a 16 GB, 16-thread AMD machine gets 4 slots. | **Adopt** (gate), then U6 adds the RAM term. |
| #1996 cap of 2 for wanted | **merged** 2026-11-25 | +127 -68 | in `big_compile_tests` | Already in dev (`real.rs:4077-4087`, `4232` on) | - | - | Kept; #2098 changes its default to base+1. |
| #2698 watchdog ends through the real `NtTerminateProcess` | open, MERGEABLE | +53 -1, `eh.rs`, `real.rs` | `the_process_ends_while_another_thread_holds_the_heap` (red with the old ending) | Probably; small. | Linux: 3 of 4 runs without it never exited. | None seen. Conflicts with #967 by design (same function name). | **Adopt** (gate) before U1. |
| #2581 no put-off with `XCL_PSO_WAIT_MS` | open, MERGEABLE | +13 | 1 test changed | Yes | RTX 3050: `Close` → `E_OUTOFMEMORY` and device removal with `XCL_PSO_WAIT_MS=1` without it. | - | Not needed for the default. Optional later. |

Ideas only, no code: a sibling GPL-3.0 project gives slow pipelines a quarter of the workers and
shares a 500 ms per-frame wait across threads. U4's shared budget and U7's extra workers are
written from this spec, not from that project.

#### 3.2 Order

1. Gate: adopt #2698, #2579 and #2098 (the user's `scripts/dev/merge-pr.py`). They touch
   different files except that #2579 and #2098 do not overlap at all.
2. U1 after #2698. U2 (telemetry fields that need no new counters) at any time.
3. U4 after #2579 (both edit `pso_async.rs` near `need_by_submit`).
4. U6 after #2098 (both edit the gate in `real.rs`).
5. U5 after U4 (same functions in `pso_async.rs` and `bind.rs`).
6. U7 after U5 and U6 (`pso_async.rs` worker loop and one line in `real.rs::big_compile_turn`).
7. U8 (counters for unbuilt draws/dispatches and parked workers) after U7.
8. Docs unit U9 (README, `docs/guide/telemetry.md`) last.

### 4. Constraints

- **Performance.** The title's frame cost from pipeline waits is at most the draw budget
  (100 ms) plus compute build time on frames that need a new compute pipeline. No title thread
  runs DXC or the driver compiler.
- **Compatibility.** `XCL_ASYNC_PSO=0` and `XCL_LAZY_PSO=0` keep today's behaviour.
  `XCL_PSO_WAIT_MS=<ms>` keeps its meaning for both kinds (per-bind bound, late mark). When it is
  set, a draw waits `min(XCL_PSO_WAIT_MS, frame allowance)`.
- **Memory.** Run 3 reached 42 GB commit with 4/6 slots (`activity` at 132 s) and 15,168 of
  15,200 MB video memory. #2098 measured 0.34 GiB per extra huge compile on NVIDIA and 2.3 GB on
  RDNA4. The RAM term (one slot per 12 GiB) caps a 16 GB machine at 1 slot, 2 when wanted.
  Extra workers reserve 64 MB of stack each (`pso_async.rs:309`), virtual, not commit: at most
  15 extra here.
- **The title's 30 s watchdog.** It counts the time of the title's `Sleep`. While a bind waits,
  `title_held()` is true and `kernelx::Sleep` does not count that time (`pso_async.rs:216-222`,
  `hle/kernelx.rs:1529-1541`). Long compute waits at the submit stay inside `Held`. The 120 s
  safety valve is far above 30 s on purpose: the watchdog is already paused.
- **Platform.** The crate builds and runs on Windows only. This Mac cannot build or test it.
  Every acceptance check runs on the Windows PC.
- **Repo rules.** One logical change per PR, under about 400 changed lines, a test with every
  logic change, no `cargo fmt`. Telemetry code goes in `telemetry_gpu.rs` / `telemetry_run.rs`
  (and the appended parts of `log.rs`); upstream files get one-line call sites.

### 5. Considerations

- **What could hang the GPU.**
  - Skipping a compute pass whose counts or arguments a later ExecuteIndirect reads
    (`pso_async.rs:1162-1169`). This spec keeps waiting for compute that writes memory. Only
    image-only big programs (`gpu/effects.rs:99-105`) are skipped by default. They write no
    buffer, so no ExecuteIndirect can read stale counts from them.
  - The 120 s safety valve can skip such a pass. It fires only for a build that has not
    finished after 120 s. Without it the run stalls forever (#2615 saw an AMD compile that never
    finished in 466 s). It logs a warning and an `error` of kind `pipeline_wait_expired`.
  - A draw left out is safe for the GPU. Its occlusion query is forced visible
    (`list_x.rs:225-237`, called at `bind.rs:589-591`).
  - Upstream reports device losses after the title passes its old stop with #1798 (RTX 3060) and
    #2615 (Linux NVIDIA). None is tied to the change. Our acceptance run checks for removals.
- **The first in-world frame can still take long.** It waits for every compute pipeline it needs
  that writes memory. On a cold cache with N huge programs of ~10 s each and 4-5 slots, that is
  about N × 10 / 4.5 s. The second run has driver blobs and is fast (`freeze-analysis.md` Q5).
  This is the price of not risking a GPU hang. The alternative (C2 below) trades it for that risk.
- **Draws left out for long.** If a graphics build is slow, its geometry is missing for that long.
  On the loading screen this can mean a missing logo for a few frames.
- **Budget accounting across threads.** Waits on different threads at the same time overlap in
  wall time. The shared counter adds their elapsed times, so the sum can exceed 100 ms while the
  frame is delayed about 100 ms. Sequential waits are capped at 100 ms in total.
- **Race in ExecuteIndirect.** It reads `is_pending` after resolving (`bind.rs:278`, `286`). A
  build that ends between the two sends the stand-in to the PC call. U4 uses the resolve result
  instead (as #2615's description says #2598 does).
- **Preloads.** With the restriction, an earlier run's 1 MB preload can still hold one slot for up
  to 34 s (run 1). It cannot start while a wanted build waits.
- **Deadlock.** A wanted build never waits for commit (`real.rs:4198-4201`), and slots always free.
  Preload restriction only makes preloads wait. Extra workers are bounded at 2n.
- **Open risk.** The 9-10 DXIL instructions per console instruction make cold compute builds
  slow (`freeze-analysis.md` Q4). That stays.

#### Alternatives considered (with tradeoffs)

| Choice | Option | For | Against | Decision |
|---|---|---|---|---|
| Draw wait | A1 none (#1798/#2615) | simplest, frames never wait | on a fast driver many first draws are left out that 10-50 ms would have saved | rejected |
| | A2 fixed per bind (`XCL_PSO_WAIT_MS=50`) | exists | multiplies: run 3 paid 335 × 44 ms = 14.7 s with no frame | rejected |
| | **A3 shared per frame, 100 ms** | bounded frame cost; a fast driver finishes small builds inside it | a few more ms per frame on a slow driver | **chosen** |
| Compute | C1 wait for a worker at the submit | no GPU-hang risk from skipped passes | first in-world frame waits for the compute it needs | **chosen**, with image-only skip and 120 s valve |
| | C2 bound compute like draws | frames flow sooner | the hang risk the source warns about; #2581's put-off memory failure with a bound | rejected; stays opt-in as `XCL_PSO_WAIT_MS` |
| Starvation | dedicated graphics workers (n/4) | simple | small compute the submit needs can still starve behind parked huge compute | rejected |
| | **extra worker when one parks** | fixes both kinds | up to 2n threads | **chosen** |

### 6. Work breakdown

Each unit is one PR. Line numbers are on `dev` 094cbbbb; after earlier units land, find the
code by the function names given.

#### U0. Gate adoptions (no code written by us)

- **Inputs:** #2698 (`eh.rs`, `real.rs`), #2579 (`pso_async.rs`, `list_x.rs`, `bind.rs`),
  #2098 (`real.rs`, README).
- **Outputs:** three merges into `dev` through `python scripts/dev/merge-pr.py <N>`, run by the
  user. Never `git merge` / `cherry-pick` them by hand.
- **Acceptance:** each merge builds on the PC; `cargo test -p xcl --lib -- pso_async:: list_x::
  bind:: big_compile_tests eh::` passes; their own tests are among them
  (`a_build_a_submit_needs_is_wanted`, `default_huge_pso_slots_scale_with_available_cpus`,
  `a_wanted_huge_compile_has_a_slot_above_the_background_ones`,
  `the_process_ends_while_another_thread_holds_the_heap`).

#### U1. `XCL_RUN_SECONDS` and `XCL_STALL_DUMP` always end the run (option F)

- **Inputs:** `host.rs:149-191`; `eh::end_process_now` from #2698.
- **Files:** `host.rs` only.
- **Change.**
  1. Add to `host.rs`:
     ```rust
     /// The process ends `after` from now whatever the rest of the ending does.
     fn arm_end_watchdog(after: std::time::Duration, fire: impl FnOnce() + Send + 'static) {
         let _ = std::thread::Builder::new().name("xcl-run-end-watchdog".into()).spawn(move || {
             std::thread::sleep(after);
             fire();
         });
     }
     const END_GRACE: std::time::Duration = std::time::Duration::from_secs(30);
     ```
  2. In the `XCL_RUN_SECONDS` thread, right after the `sleep` and before `counters()` and the
     log line: `arm_end_watchdog(END_GRACE, || crate::eh::end_process_now(0));`.
  3. In the `XCL_STALL_DUMP` thread, right after `stalled(...)` is true and before the
     `xlog_warn!`: `arm_end_watchdog(END_GRACE, || crate::eh::end_process_now(0));`.
  4. Leave the order of the other steps as it is. `telemetry_end` is already before
     `write_final` in the run-seconds thread (`host.rs:186-187`).
- **Why 30 s.** `end_title`'s own watchdog is 15 s after `end_title` starts (`real.rs:3599-3609`).
  30 s leaves `write_final` 15 s before the backstop cuts the drain short.
- **Tests (`host.rs` `mod tests`):**
  - `an_armed_end_watchdog_fires_while_the_ending_blocks`: arm with 50 ms and a closure that
    sends on a channel; then block the test thread on `recv_timeout(2 s)`; assert it got the
    message within 2 s.
  - `an_armed_end_watchdog_waits_its_time`: arm with 300 ms; assert nothing arrives in 100 ms.
- **Acceptance:** the two tests pass; `telemetry_e2e::a_normal_run_ends_with_end_and_exit`
  still passes; on the PC, a run with `XCL_RUN_SECONDS=600` that stalls (default switches, as
  run 2) exits by 630 s with `end.reason` `run_seconds`, or with an `exit` event if `write_final`
  blocked.
- **Size:** about 30 lines.

#### U2. Telemetry fields that need no new counters

- **Files:** `telemetry_gpu.rs`, `telemetry_run.rs`, the appended telemetry part of `log.rs`
  (`FrameFacts` at `log.rs:721`, `PipelineFacts` at `log.rs:797`, `frame_fields` at `827`,
  `pipeline_fields_built` at `940`).
- **Change.**
  1. `frame`: add `pso_wait_draw_ms` and `pso_wait_dispatch_ms`, from
     `pso_async::blocked()[0][1]` and `[1][1]` (µs, deltas per frame, as `pso_us` is taken in
     `Counters::now`, `telemetry_gpu.rs:2694-2700`). Keep `pso_wait_ms` as their sum.
  2. `pipeline` (build shape): add `by`: from the current thread's name in
     `telemetry_pipeline_built` (`telemetry_gpu.rs:2406`): `preload` for `xcl-pso-pre-*`,
     `extra` for `xcl-pso-x*` (U7), `worker` for other `xcl-pso*`, `xcl` for other `xcl-*`,
     else `title`. Add `wanted`: `crate::hle::d3d12x::pso_async::wanted_here()()` taken at
     the start of the function.
  3. `activity` (written by `telemetry_run.rs` every 2 s): add `pso_wait_ms` (delta of the sum),
     and `builds_title` (count of `pipeline` builds with `by` = `title` since the last
     `activity`). Keep a static counter in `telemetry_gpu.rs` bumped in `telemetry_pipeline_built`.
- **Tests:** unit tests next to the existing `frame_fields_serialise_flat_with_nulls`
  (`log.rs:1087`): the new fields serialise flat and null-safe; a test of the thread-name
  mapping (a pure `fn built_by(name: Option<&str>) -> &'static str`) for the five cases.
- **Acceptance:** `cargo test -p xcl --lib -- log:: telemetry` passes;
  `telemetry_e2e::telemetry_frame_events_match_the_run` passes and its frames carry the two new
  fields.
- **Size:** about 120 lines.

(There is no U3: the adoptions it would have held are in U0.)

#### U4. Draws never build and wait only inside a shared frame budget (option A)

- **Inputs:** `pso_async.rs` after #2579; `bind.rs:269-330`, `548-592`.
- **Files:** `hle/d3d12x/pso_async.rs`, `hle/d3d12x/bind.rs`.
- **Change in `pso_async.rs`.**
  1. A process-wide frame budget:
     ```rust
     /// Draw waits per shown frame, all threads together (`XCL_PSO_FRAME_WAIT_MS`, default 100;
     /// 0: a draw never waits).
     fn frame_wait_budget() -> std::time::Duration  // OnceLock, parse u64 ms, default 100
     /// (frames shown when the spending began, µs spent)
     static FRAME_SPENT: Mutex<(u64, u64)> = Mutex::new((0, 0));
     /// What is left for frame `frame` (resets when `frame` differs from the stored one).
     fn frame_allowance(frame: u64) -> std::time::Duration
     /// Adds `spent` to frame `frame` (ignored if a newer frame has started).
     fn frame_spend(frame: u64, spent: std::time::Duration)
     ```
     The frame number is `super::present::presents()` (`present.rs:1269`). No change to
     `present.rs`.
  2. A helper that moves a build to the front, used by U4 and U5:
     ```rust
     /// A title thread needs `p` now: wanted, and ahead of the workers' FIFO.
     fn need_now(p: &Arc<Pending>) {
         p.want();
         if !std::mem::replace(&mut p.demand.lock().unwrap().by_submit, true) {
             NEEDED.lock().unwrap().push_back(p.clone());
             let _ = queue().lock().unwrap().send(Box::new(|| ()));
         }
         p.sent.store(true, Ordering::Release);
     }
     ```
     `need_by_submit` (after #2579 it calls `want()`) becomes a call to `need_now`.
     `sent` is set so a later `resolve` does not enqueue it again; the worker that takes it
     from `NEEDED` builds it (`build_queued` takes `job`; a second taker finds `None`).
  3. The draw resolve:
     ```rust
     /// The pipeline a draw or graphics ExecuteIndirect uses now, and whether it is not built
     /// (the stand-in). A compute pipeline goes to `resolve_wait`.
     pub fn resolve_draw(pso: usize) -> (usize, bool)
     ```
     - `entry(pso)` is None: `(pso, false)`.
     - `p.compute`: `let r = resolve_wait(pso);` then return `(r, is_pending(pso))`.
     - Built (`real != 0`): `(real, false)`. Build failed (`done` and `real == 0`):
       `(p.stand(pso), false)`.
     - Not built: call `need_now(&p)`. Then the allowance is
       `frame_allowance(frame)`, capped by `wait_budget()` when that is set. If the
       pipeline is `late`, or the allowance is zero: return `(p.stand(pso), true)` at once and
       do **not** set `late`. Otherwise wait on `p.cv` up to the allowance, inside `Held::new()`
       and an `OpenWait` (kind graphics, `built_here` false), then `frame_spend(frame,
       elapsed)` and `note_blocked(false, false, elapsed)`. If it is built now, return
       `(real, false)`. If not, set `late` and return `(p.stand(pso), true)`.
     - Never take `p.job`.
  4. In `resolve_wait_for` (the `budget == None` path, `pso_async.rs:1100-1104` today), a
     **graphics** entry no longer takes the job: replace with `need_now(&p)` and wait. (U5 does
     the same for compute; doing graphics here keeps any other graphics caller of
     `resolve_wait` off the compiler.)
  5. Fix the module doc line "A graphics pipeline draws nothing until then"
     (`pso_async.rs:5-7`): say a draw waits at most the frame budget, then draws nothing.
- **Change in `bind.rs`.**
  1. `graphics_bound` (`bind.rs:552`): `let (real, unbuilt) = super::pso_async::resolve_draw(pso);`.
     Use `unbuilt` instead of `super::pso_async::is_pending(pso)` at `bind.rs:589`.
     `telemetry_draw` stays right after (`bind.rs:553`).
  2. The trace-only `resolve_wait(pso)` at `bind.rs:577` becomes `resolve(pso)`.
  3. `execute_indirect` (`bind.rs:278`): `let (pso, unbuilt) = super::pso_async::resolve_draw(raw_pso);`
     and use `unbuilt` instead of `is_pending(raw_pso)` at `bind.rs:286`. The compute put-off
     above it (`bind.rs:274-277`) is unchanged.
- **Tests.**
  - `pso_async` `mod used_tests`:
    - `a_draw_never_builds_the_pipeline_on_its_thread`: `submit_lazy` a graphics stand-in whose
      job records `std::thread::current().name()` and blocks on a channel; call `resolve_draw`
      from a test thread with `XCL_PSO_FRAME_WAIT_MS` effectively 100 ms (call an inner
      `resolve_draw_with(pso, frame, allowance)` taking the allowance as a parameter, so no env
      var is needed); assert it returns `(standin, true)` within 400 ms, the job ran on an
      `xcl-pso*` thread, and `build_is_wanted` was true inside it.
    - `the_frame_budget_is_shared_and_resets_on_the_next_frame`: two slow graphics stand-ins,
      allowance 60 ms for frame 7: the first waits about 60 ms, the second returns in under
      15 ms; for frame 8 the second waits again.
    - `a_spent_budget_does_not_mark_the_pipeline_late`: with allowance 0 the stand-in is
      returned and `late` stays false; with a timed-out wait `late` is true.
    - `a_built_graphics_pipeline_is_returned_at_once`: built job, `(real, false)`.
    - Keep `a_wait_that_builds_the_pipeline_is_counted_with_its_time` (`pso_async.rs:1397`)
      working for compute only; if it uses a graphics entry, switch it to compute and say so in
      the PR.
  - `bind.rs` `mod tests`: `a_draw_does_not_wait_past_the_frame_budget` (as #1798's test, but
    through `graphics_bound`'s resolve: a 1.5 s graphics job returns the stand-in in under
    400 ms; once built, the real one).
- **Acceptance:** the tests above pass; `cargo test -p xcl --lib -- pso_async:: bind:: list_x::`
  and `--test gpu_pso_e2e` pass (report SKIPPED); in the acceptance run, `pipeline` events with
  `by` = `title` and stage `graphics` are 0, and no log line "held up its caller ... (built in
  the call)" appears for graphics.
- **Size:** about 180 lines with tests.

#### U5. Compute waits for a worker, never builds on the title thread (options B)

- **Inputs:** U4 merged.
- **Files:** `hle/d3d12x/pso_async.rs`, `hle/d3d12x/bind.rs`.
- **Change in `pso_async.rs`.**
  1. `resolve_wait_for` with `budget == None` for a **compute** entry: no `p.job.take()`.
     Call `need_now(&p)`, then wait on `p.cv` in a loop of `wait_timeout` steps until `done`
     or until `compute_wait_limit()` has passed since `started`. Keep `Held`, `OpenWait`
     (`built_here` false) and `note_blocked(true, false, elapsed)` as today.
  2. `compute_wait_limit()`: `XCL_PSO_COMPUTE_WAIT_MS`, default 120,000; `0` means no limit.
     When it passes: set `late`, log once per pipeline
     `"D3D12.X: a compute pipeline was not built after {ms} ms: its dispatches are skipped until it is"`,
     call `crate::report::telemetry_pipeline_wait_expired(true, ms)` (one line; U8 defines it,
     until then add the function as an empty stub in `telemetry_gpu.rs` in this PR), and return
     `p.stand(pso)`.
  3. `image_skip()` (`pso_async.rs:1170-1173`) defaults to **on**: off only with
     `XCL_PSO_IMAGE_SKIP=0`. Update its doc.
  4. `pub fn is_spared(pso: usize) -> bool`: the entry's `spared` flag.
- **Change in `bind.rs`.**
  1. `dispatch` (`bind.rs:879`): put off only when `is_pending(pso) && !is_spared(pso)`.
  2. `execute_indirect` (`bind.rs:274`): `unbuilt_compute` also requires `!is_spared(raw_pso)`.
     A spared pipeline then goes to `resolve_draw` → `resolve_wait` → `resolve_spared`
     (`pso_async.rs:1185-1203`) and is skipped at once.
- **Tests (`pso_async` `mod used_tests`).**
  - `a_compute_wait_never_builds_on_the_waiting_thread`: lazy compute job records its thread
    name; `resolve_wait_for(standin, None)` from a test thread returns the real pipeline; the
    job ran on `xcl-pso*`.
  - `a_compute_wait_goes_ahead_of_the_fifo`: fill the FIFO with 64 lazy graphics jobs sent by
    `resolve` that each sleep 20 ms; then a compute wait; assert it returns in under 1 s.
  - `a_compute_wait_ends_at_its_limit`: an inner `resolve_wait_limited(pso, limit)` with 50 ms
    and a job that blocks: returns the stand-in after about 50 ms, `late` is true.
  - `image_skip_is_on_unless_turned_off`: a pure `fn image_skip_from(v: Option<&str>) -> bool`
    for `None`, `"1"`, `"0"`, `" 0 "`.
  - `bind.rs`: `a_spared_dispatch_is_not_put_off` (a spared pending compute pipeline:
    `dispatch` does not call `defer`; the dispatch is skipped and `DISPATCHES_SKIPPED` grows).
  - Update `list_x` test `a_dispatch_with_a_pipeline_still_building_waits_at_the_submit`
    (`list_x.rs:7198-7249`) only if it relied on the submit thread building; it must still
    show the submit waits.
- **Acceptance:** tests pass; `--test gpu_compute_e2e` passes (report SKIPPED); in the
  acceptance run, compute `pipeline` events with `by` = `title` are 0 and compute waits at a
  bind are each no longer than that build's `waited_ms + compile_ms` plus 1 s.
- **Size:** about 150 lines with tests.

#### U6. The gate: a graphics lane, preloads give way, RAM-scaled slots (options B, C)

- **Inputs:** `real.rs` after #2098 (`big_compile_turn` takes `need`, `commit_free(u64)`,
  `wanted`; `Slots` has `reserved`; defaults `default_huge_pso_concurrency(cpus)` and
  `default_huge_pso_wanted_concurrency(base)`).
- **Files:** `hle/d3d12x/real.rs` only.
- **Change.**
  1. **RAM term.** `default_huge_pso_concurrency(cpus, ram_gib)` =
     `min((cpus / 4).clamp(1, 4), (ram_gib / 12).max(1))`. `ram_gib` is
     `GlobalMemoryStatusEx().ullTotalPhys >> 30` (the same call as
     `big_compile_commit_free`, `real.rs:4218-4223`); 0 when the call fails, which gives 1.
     `XCL_HUGE_PSO_CONCURRENCY` still overrides. This machine: 16 threads, 63 GiB → 4; wanted 5.
  2. **Graphics lane.** `static GRAPHICS_COMPILES: Slots` like `BIG_COMPILES` (most 0,
     wanted_most 0, same `reserved` pool). `compile_slots(bytes, graphics: bool)`: giant lane
     for `bytes >= 4 MiB` whatever the kind, else `GRAPHICS_COMPILES` when `graphics`, else
     `BIG_COMPILES`. In `driver_compile_identified`, `graphics = !kind.contains("compute")`
     (the rule `telemetry_pipeline_built` already uses, `telemetry_gpu.rs:2414`).
  3. **Preloads give way.** `Slots` gets `preloads: Mutex<usize>` (or count under `n`'s lock)
     and `wanted_waiting: AtomicUsize`. `big_compile_turn` gets `preload: bool` (true when the
     thread name starts with `xcl-pso-pre`). While `wanted()` is true and the build cannot take
     a slot, it counts itself in `wanted_waiting` (increment once, decrement on leaving). An
     unwanted preload takes a slot only if `preloads < 1` and `wanted_waiting == 0`, on top of
     the existing cap and commit rules. Its `SlotGuard` decrements `preloads`.
- **Tests (`real.rs` `mod big_compile_tests`).**
  - `slots_scale_with_ram_too`: table over (cpus, ram_gib): (16, 63)→4, (16, 32)→2,
    (16, 16)→1, (32, 8)→1, (4, 64)→1, (64, 0)→1.
  - `a_preload_holds_at_most_one_slot`: `Slots { most: 3 }`: one preload takes a slot; a
    second preload waits 120 ms while 2 slots are free; an ordinary unwanted build takes one.
  - `a_preload_waits_while_a_wanted_build_waits`: `most 1, wanted_most 1`, slot held; a wanted
    build waits; a preload started after it does not get the slot when it frees; the wanted
    one does.
  - `graphics_and_compute_take_separate_lanes`: `compile_slots(300 << 10, true)` and
    `(300 << 10, false)` are different lanes; `(5 << 20, true)` is the giant lane.
  - Existing `big_compile_tests` keep passing.
- **Acceptance:** tests pass; in the acceptance run, no `pipeline` event with `by` =
  `preload` starts a driver compile (log `driver compile start`) while another build with
  `wanted` true is in the gate (read from `waited_ms` intervals); graphics `waited_ms` stays
  under 5 s.
- **Size:** about 140 lines with tests.

#### U7. A worker parked at the gate is replaced (the starvation fix)

- **Inputs:** U5 and U6 merged.
- **Files:** `hle/d3d12x/pso_async.rs`; one line in `hle/d3d12x/real.rs::big_compile_turn`.
- **Change in `pso_async.rs`.**
  1. Keep the receiver: `queue()` stores `rx` (`Arc<Mutex<Receiver<Work>>>`) in a static
     `OnceLock` so more workers can be started later. Move the worker loop body
     (`pso_async.rs:309-316`) into `fn worker_loop(rx)`. It sets a thread-local `IS_WORKER = true`.
  2. Counters: `static WORKERS: AtomicUsize` (threads started, base and extra) and
     `static PARKED: AtomicUsize`.
  3. ```rust
     /// A guard while a worker waits at the gate; starts an extra worker when fewer than
     /// `compile_threads()` workers are free, up to twice that many threads.
     pub(super) struct Parked(bool);
     pub(super) fn park() -> Parked
     ```
     `park()`: if `!IS_WORKER` return `Parked(false)` (preload threads and title threads do not
     count). Else `PARKED += 1`; if `WORKERS - PARKED < n` and `WORKERS < 2 * n`, start one
     thread named `xcl-pso-x{k}` running `worker_loop` and `WORKERS += 1`. `Drop`:
     `PARKED -= 1` when it counted. Extra workers do not exit.
     A pure policy `fn extra_needed(workers: usize, parked: usize, n: usize) -> bool` holds the
     rule for testing.
  4. `pub fn workers() -> (usize, usize)`: (threads, parked), for U8.
- **Change in `real.rs`.** In `big_compile_turn`, the first time the loop is about to wait
  (`slots.cv.wait_timeout`), create `let _parked = super::pso_async::park();` and keep it until
  the function returns. A build that gets a slot at once never parks.
- **Tests (`pso_async` `mod used_tests`).**
  - `extra_workers_follow_the_parked_count`: table: (15, 0, 15)→false, (15, 1, 15)→true,
    (16, 1, 15)→false, (30, 16, 15)→false (cap 2n).
  - `queued_work_runs_while_every_worker_is_parked`: submit `compile_threads()` jobs that each
    call `park()` and block on a shared channel (with a 5 s timeout so a failure cannot hang
    the suite); then submit one more job; assert it completes within 2 s; release the blocked
    jobs.
- **Acceptance:** tests pass; in the acceptance run, `activity.workers` rises above 15 when
  `activity.gate_parked` > 0 (U8), and graphics `pipeline` events keep arriving while huge
  compute builds wait at the gate (no 10 s window with gate_parked > 0 and 0 graphics builds
  while draws are left out).
- **Size:** about 120 lines with tests.

#### U8. Telemetry for left-out work, parked workers, expired waits

- **Inputs:** U4, U5, U7.
- **Files:** `telemetry_gpu.rs`, `telemetry_run.rs`, `log.rs` (appended part); one-line call
  sites in `bind.rs`, `console_list.rs`, `list_x.rs`.
- **Change.**
  1. `pub fn telemetry_unbuilt(compute: bool)` in `telemetry_gpu.rs` (tier `frame`; one relaxed
     load when off): bumps `UNBUILT_DRAWS` or `UNBUILT_DISPATCHES`.
  2. Call sites (one line each):
     - `bind.rs` `graphics_bound`, in the `unbuilt` branch: `crate::report::telemetry_unbuilt(false);`
     - `bind.rs` `execute_indirect`, the "still being built skipped" branch (`bind.rs:286-293`):
       `telemetry_unbuilt(!gfx_signature)` where a dispatching signature counts as compute
       (`signature_dispatches(sig)`).
     - `bind.rs` `dispatch`, when the resolved pipeline is a stand-in of a pending pipeline
       (the `!is_translated_compute` branch with `is_pending(raw)` true):
       `telemetry_unbuilt(true);`
     - `console_list.rs:887-890` skip branch when `is_pending((*p).pc)`: `telemetry_unbuilt(true);`
  3. `pub fn telemetry_pipeline_wait_expired(compute: bool, ms: u64)` (replaces U5's stub):
     writes an `error` of kind `pipeline_wait_expired`, not fatal, fields `stage`, `ms`, `n`
     (volume rule as `io_failed`).
  4. `frame`: add `draws_unbuilt`, `dispatches_unbuilt` (deltas), `pso_budget_out` (binds that
     found the budget spent: a counter bumped in `resolve_draw` through a one-line
     `crate::report::telemetry_budget_out()`).
  5. `activity`: add `draws_unbuilt`, `dispatches_unbuilt` (deltas over the 2 s), `workers` and
     `gate_parked` (from `pso_async::workers()` at the sample). These matter most: during a
     stall there are no `frame` events.
- **Tests:** serialisation tests in `log.rs` for the new fields; a `telemetry_gpu` unit test
  that `telemetry_unbuilt` counts per kind and the frame delta takes them; extend
  `telemetry_e2e::activity_events_mirror_the_log` to assert the new `activity` fields exist.
- **Acceptance:** tests pass; the fields appear in the acceptance run.
- **Size:** about 150 lines.

#### U9. Docs

- **Files:** `README.md` (the huge-pipeline paragraph at `README.md:271-282`; the switches
  `XCL_PSO_FRAME_WAIT_MS`, `XCL_PSO_COMPUTE_WAIT_MS`, `XCL_PSO_IMAGE_SKIP` default),
  `docs/guide/telemetry.md` (5.3 `frame` table, 5.5 `pipeline`, 5.8 `activity` and `error`
  kinds).
- **PR note:** `HANDOFF.md` statements that draws wait for pipelines become wrong (#2615's
  description names `HANDOFF.md:1051` and `1420-1421` upstream). Say so; do not edit it.
- **Acceptance:** review only.

### 7. Concurrency plan

| Wave | Units in parallel | Files | Why they can run together |
|---|---|---|---|
| 1 | U0 (#2698, #2579, #2098 through the gate) | `eh.rs`+`real.rs` (end_title), `pso_async.rs`+`list_x.rs`+`bind.rs`, `real.rs` (gate)+README | #2698 and #2098 both touch `real.rs` but far apart (3604 vs 4066-4214); the gate merges them one at a time anyway |
| 1 | U2 | `telemetry_gpu.rs`, `telemetry_run.rs`, `log.rs` | no file shared with U0 |
| 2 | U1 | `host.rs` | needs #2698 |
| 2 | U4 | `pso_async.rs`, `bind.rs` | needs #2579 |
| 2 | U6 | `real.rs` | needs #2098; no file shared with U4 or U1 |
| 3 | U5 | `pso_async.rs`, `bind.rs` | same files as U4 |
| 4 | U7 | `pso_async.rs`, `real.rs` | same files as U5 and U6 |
| 5 | U8 | telemetry files, `bind.rs`, `console_list.rs`, `list_x.rs` | needs U4, U5, U7's counters; shares telemetry files with U2 |
| 6 | U9 | README, telemetry guide | describes what was built |

Each unit is built and tested on the Windows PC (`cargo build --workspace`,
`cargo test --workspace -- --test-threads=4`, counts and SKIPPED reported). Runtime units (U1,
U4-U8) also need a `scripts/smoke.ps1` pair before any upstream PR.

### 8. Acceptance run

**Build:** `dev` with U0-U8, clean tree, release, on the RTX 5080 PC (driver 32.0.16.2140).
Run it after the texture-upload crash fix lands. If that crash comes back, the run counts up
to the crash and is marked partial.

**Two runs, same input as runs 1-3 (press A at the same point):**

- **R-A (default switches):** `XCL_TELEMETRY=all XCL_ACTIVITY=1 XCL_RUN_SECONDS=900
  XCL_STALL_DUMP=120 XCL_UMA_MB=4095 XCL_THREAD_DUMP_AT=240,480`. No other `XCL_PSO_*` or
  `XCL_HUGE_*` switch. Caches as runs 1-3 left them.
- **R-B:** the same again right after, for the warm-cache numbers.

**Success (each from events unless named):**

| Measure | Run 1 | Run 2 | Run 3 (E4) | Target R-A |
|---|---|---|---|---|
| Driver compiles with `by` = `title` | many (24 holds ≥1.4 s) | 294 graphics + 16 compute built in the call (`run2/log.txt:4631`) | 0 (bounded waits) | **0** |
| Longest 2 s `activity` run with 0 presents after "press A" | ≥90 s | 1,322 s | ~32 s (one 32.2 s frame) | each gap explained by `pso_wait_dispatch_ms` (compute) only; draw waits ≤ 100 ms per frame |
| `frame.pso_wait_draw_ms` | - | - | - (33,304 ms summed in one frame) | ≤ 110 per frame, every frame |
| First frame with ≥ 280 draws | never | never | 110-120 s | reached; time reported against run 3 |
| `draws_unbuilt` / (draws + draws_unbuilt) per frame, 60 s after the first in-world frame | - | - | 9,461 draws skipped by 130 s | < 5 % |
| `activity.gate_parked` > 0 with 0 graphics builds for 10 s while `draws_unbuilt` > 0 | - | - | yes (100-132 s) | **never** |
| Preload compile start while a wanted build waits | 62.5 s of 90 held by preloads | - | preload off | **never** |
| `error` `pipeline_wait_expired` | - | - | - | 0 expected; any is listed |
| Device removals / GPU hang | 0 | 0 | 0 in 22 s | **0** |
| Peak `committed_mb` | 31.3 GB | - | 42.1 GB | ≤ 45 GB |
| Run end | not ended by `XCL_RUN_SECONDS` | closed by hand | crash | `end.reason` `run_seconds` by 930 s, or `stall` with dumps |

R-B: the same rows, plus the time from "press A" to the first frame with ≥ 280 draws must be
shorter than R-A's.

### 9. What I could not check

- No build or run was made. Effects are predictions from runs 1-3 and the code.
- How many compute pipelines the first in-world frame needs, so how long C1's wait is on a cold
  cache, is unknown.
- Upstream measurements are theirs, not ours.
- Whether #2098 and #2698 merge cleanly into `dev` (18 adopted PRs) was not tried; the gate
  decides.

### 10. Texture-read crash of run 3: adopt upstream #751 (added to U0)

- Crash: run 3 at 131.966 s, access violation reading `0x821a30000` in `memcpy` <-
  `gpu::tiling::Chain::detile_level` <- `hle::d3d12x::title_textures::subresources`, on a worker
  thread spawned in `subresources_on`.
- Cause (Opus debug worker, checked by the orchestrator at `title_textures.rs:1520-1526`):
  `title_bytes` borrows title memory once `gpu::shader::readable` passes; nothing keeps the pages
  alive during the hash and the detile, so a decommit in between faults. It is a race: it did not
  repeat in runs 4-6.
- Fix: adopt upstream PR #751 ("title texture reads outside the coherent window use the safe
  copy", +196 -36) through `scripts/dev/merge-pr.py`. It is unit U0's fourth adoption.
- Not in this entry: a `texture_read_hole` telemetry counter, logging which path decommits, and
  modelling queue method #31. They get their own entries if needed.

### 11. Measurements this entry rests on (RTX 5080 16 GB, 63 GB RAM, Windows 11; not tested on AMD)

| Run | Switches | Result |
|---|---|---|
| 1 | defaults, telemetry all | last frame 43 s after "press A"; a title thread builds graphics pipelines in its draws, 24 holds of 1.4-6.5 s in 90 s |
| 2 | defaults, soak | 1,322 s with no frame; about 200 graphics pipelines cold-built at about 9 per minute |
| 3 | `XCL_PSO_WAIT_MS=50`, `XCL_HUGE_PSO_CONCURRENCY=4`, `XCL_HUGE_PSO_WANTED_CONCURRENCY=6`, `XCL_PSO_PRELOAD=0` | gap 90-110 s, then 13 frames with 280-357 draws; texture-read crash at 132 s |
| 4 | as 3 | intro cutscene from about 120 s at 1-2 fps; commit 37 -> 80 GB in 60 s; device removed 0x887A0001 at 190 s |
| 5 | as 3 + `XCL_DEVICE_TYPE=lockhart` | same growth: 77.7 GB commit, 45.9 GB non-local video memory at 186 s |
| 6 | as 3, telemetry off | same growth: 82 GB commit; title asserts on E_OUTOFMEMORY at 194.5 s, device removed |

The memory growth of runs 4-6 is not covered by this entry.

## 2026-11-26 Memory growth in the intro: adopt the upstream allocator-pool fixes (user chose "all four now")

Measured on the RTX 5080 PC (16 GB, 63 GB RAM, driver 32.0.16.2140), switches of run 3
(`XCL_PSO_WAIT_MS=50`, `XCL_HUGE_PSO_CONCURRENCY=4`, `XCL_HUGE_PSO_WANTED_CONCURRENCY=6`,
`XCL_PSO_PRELOAD=0`). Not tested on AMD.

### Problem

From the first intro frames (about 120 s) process commit grows by 0.6-0.8 GB/s and non-local video
memory from 6 GB to 45 GB in about 60 s, while every pool on the `memory:` log line stays flat.

| Run | Difference | Non-local at about 190 s | End |
|---|---|---|---|
| 4 | telemetry all | 44.6 GB | device removed 0x887A0001 at 190 s |
| 5 | + `XCL_DEVICE_TYPE=lockhart` | 45.9 GB | closed by hand |
| 6 | telemetry off | 45.4 GB | XCL's split `Close` returns E_OUTOFMEMORY, the title asserts, device removed at 194.5 s |
| 7 | build d045a094, + `XCL_ALLOC_SWAP=0` | 16.3 GB (17.3 GB at 238 s) | still presenting at 250 s |

So it is not telemetry and not the console profile. Run 7 confirms the cause named by the Opus
debug worker (analysis in the session scratch folder, citations against 99a76e3e):

- `alloc_pool` swaps the PC command allocator behind a title allocator when the title resets it
  while lists still hold recordings on it. A put-aside allocator is reused only when no list is
  still a user of it, and only by the same title allocator (`alloc_pool.rs:34-35`, `:289`). It is
  never Reset or Released while the title allocator lives (`:317`); busy ones are leaked on
  `forget` (`:250`).
- XCL's own extra lists (split segments, stand-ins, put-off segments in `title_list`'s spare
  pools) stay users of the allocator they last recorded on. So almost every swap makes a new
  allocator: about 25 a second in runs 4-6, 5-8 MB of non-local each.
- Dispatches put off to the submit add a PC list and an own allocator each (the amplifier).

Run 7 still rises by about 10 GB of non-local when the intro starts; that part is not explained yet.

### Decision

Adopt through `scripts/dev/merge-pr.py`, after the stall-fix units U4 and U6 are committed:

| PR | Change | Size |
|---|---|---|
| #2725 | a title allocator takes a free spare another one put aside | +44 -3 |
| #2549 | a reused spare keeps the allocator of deferred work still on the GPU | +128 -1 |
| #2581 | with bounded pipeline waits, dispatches are not put off to the submit | +13 |
| #2707 | detach retired segments and reclaim idle put-aside allocators | +942 -61 (over the usual size; the user chose to take it now) |

On a merge conflict the user is told before it is resolved.

### Acceptance

Run 6's settings without `XCL_ALLOC_SWAP=0` on the merged build: non-local video memory stays
under 20 GB for 300 s after the first intro frame, no `Close` failure, no device removal.

### Later

- An `allocators` telemetry event (made, live, put aside free/busy, orphaned) is specified after
  the adoptions, against the merged `alloc_pool`.
- A commit and video-memory budget with back-pressure (option C of the analysis) once usage has
  settled.

## 2026-11-26 Stall fix, refinement U5b: compute waits share a frame budget (user chose "shared frame budget"; replaces the "wait" default of U5)

### Why

Run 9 (RTX 5080, build 8b774152 + U5, default switches): with compute waited for (U5's 120 s
limit), the title advanced one burst of work per 120 s. Two waits expired at 216.6 s and 336.8 s.
The programs waited for are 1.2-1.4 MB of DXIL and take 92-133 s each in the driver compiler
(`driver compile finish #2046 ... 92325 ms`, `#1975 ... 133522 ms`). Run 10 with
`XCL_PSO_COMPUTE_WAIT_MS=250`: frames from about 150 s at 1-3 fps, 1,000-2,300 draws per 2 s, no
device removal in 260 s; but a 40 s gap (110-150 s) because 183 compute programs each used
their 250 ms one after another, and `256 graphics (23154 ms, longest 102 ms)` shows the same
one-after-another cost for draws under U4. The user chose a shared per-frame budget for compute.

### Decisions

1. **One chance per pipeline (draws and compute).** A pipeline may be waited for only during the
   first `budget` after it was first needed. `need_now` records the time of its first call on
   the `Pending` (a new `needed_at: Option<Instant>` in its `demand`). A bind that finds
   `needed_at.elapsed() >= budget` marks the pipeline `late` and returns the stand-in without
   waiting. So a burst of N new pipelines costs at most one budget, not N budgets.
2. **Draws** (`resolve_draw_with`): after `need_now`, in this order: `late` -> stand-in, unbuilt;
   chance over -> set `late`, stand-in, unbuilt, no wait and no `note_blocked`; frame allowance
   zero -> stand-in, unbuilt, not late; else wait up to `min(frame allowance, chance left,
   wait_budget() if set)`, then as U4 (a wait that ran out sets `late`).
3. **Compute, default = frame mode.** `XCL_PSO_COMPUTE_FRAME_WAIT_MS` (default 100; 0: never
   wait) is a budget per shown frame shared by all threads, with its own counter
   (`COMPUTE_FRAME_SPENT`), separate from the draw budget. The no-budget compute path of
   `resolve_wait_inner` follows the same order as draws, with `note_blocked(true, false, took)`.
   It never takes the job (U5).
4. **`XCL_PSO_COMPUTE_WAIT_MS` becomes the opt-in for waiting.** Unset: frame mode. `0`: wait
   until built, no limit. `n`: wait at most n ms per bind (U5's behaviour, with its warning and
   `telemetry_pipeline_wait_expired`). A pure `compute_wait_from(wait_ms: Option<&str>,
   frame_ms: Option<&str>) -> ComputeWait` (`Frame(Duration)` or `Limit(Option<Duration>)`)
   decides: (None, None) -> Frame(100 ms); (Some("0"), _) -> Limit(None); (Some("5000"), _) ->
   Limit(Some(5 s)); (None, Some("250")) -> Frame(250 ms); (None, Some("0")) -> Frame(0);
   an unparsable value counts as unset.
5. **No put-off in frame mode.** `waits_bounded()` (from #2581) is also true in frame mode, so
   `list_x::defer` puts nothing off by default: the dispatch is recorded at its bind, which waits
   inside the budget or leaves it out. Put-off to the submit remains for
   `XCL_PSO_COMPUTE_WAIT_MS` set and `XCL_PSO_WAIT_MS` unset. This also removes the per-put-off
   list segment and allocator that the memory analysis named as the amplifier.
6. **Unit tests keep "wait" as their ambient mode.** Under `cfg!(test)` the mode read from the
   environment is `Limit(None)`, so existing unit tests that expect a compute bind to wait until
   built, or a dispatch to be put off, keep their meaning. New tests call inner functions that
   take the mode, frame and budget as parameters. Integration tests run the real binary and so
   run in frame mode; if one of them depends on a first-frame dispatch, it sets
   `XCL_PSO_COMPUTE_WAIT_MS=0` in its own environment.
7. Image-only skip (U5) is unchanged and applies in every mode.

### Unit U5b

- **Files:** `crates/xcl/src/hle/d3d12x/pso_async.rs` only (plus an integration test's
  environment, only if the PC run shows it is needed).
- **Tests (`pso_async` test module):**
  - `a_pipeline_gets_one_chance_to_be_waited_for`: draw path, budget 60 ms: first call waits
    about 60 ms and returns unbuilt; a second pipeline first needed in the same frame returns in
    under 15 ms and is not late; called again in the next frame more than 60 ms after its first
    need it returns in under 15 ms and is now late.
  - `the_frame_budget_is_shared_and_resets_on_the_next_frame` (U4) is rewritten for the new
    rule: in the next frame a pipeline first needed there still gets the full budget.
  - `a_compute_bind_waits_inside_the_frame_budget`: frame mode, budget 60 ms: returns the
    stand-in after about 60 ms, the job ran on a worker, `late` is set; once built the real
    pipeline is returned.
  - `compute_binds_share_the_frame_budget`: two pending compute pipelines in one frame: the
    second returns in under 15 ms.
  - `compute_wait_mode_follows_the_switches`: the table of decision 4.
  - `waits_are_bounded_in_frame_mode`: a pure `waits_bounded_for(mode, wait_budget_set)`.
- **Acceptance:** `cargo test -p xcl --lib -- pso_async:: bind:: list_x::` and the whole suite
  pass. Retail run with default switches: after the loading screen no stretch of more than 5 s
  with 0 presents; per frame `pso_wait_draw_ms + pso_wait_dispatch_ms` at most about 220 ms; no
  device removal; `builds_title` 0.

## 2026-11-26 Stall fix, refinement: pipeline worker pool with lanes and a graphics reserve (orchestrator approved; internal scheduling inside the approved stall-fix design)

Approved by the orchestrator on 2026-11-26 as a refinement the user would not see from outside
(order of work and which threads take it; no switch, file format or wait rule changes). Drafted by
an Opus design worker from run 12 (RTX 5080, driver 32.0.16.2140, build fa14d6f6, default
switches). Checked by the orchestrator in run 12's data: no graphics `pipeline` event between
128.2 s and 365.2 s (237.1 s), then 165 graphics builds between 365 and 370 s with a median
`compile_ms` of 12.7 (`run12/log.txt:2465`, `2466`, `2535`). The units below are called U1-U4
inside this entry; they are not the U1-U9 of the first stall-fix entry.

Status: draft for the orchestrator's review. Nothing here is built or run. Code paths are
`crates/xcl/src/...` on `dev` at fa14d6f6. Run data: `/Users/osanchez/xcl-pc/out/runs/run12/`.
Helper scripts: `scratchpad/worker-pool-tools/` (`per_minute.py`, `dump_classify.py`,
`skip_share.py`; run with `python3 -I <script> <file>`). Upstream PRs were read as text
(`gh pr view/diff -R xcl-dev/xcl`) on 2026-11-26.

### Summary (recommendation first)

**Recommendation: option B, "lanes and a graphics reserve", plus option 2 of the analysis for
image-only programs. Keep U7. Write our own code; do not adopt #628 or #2628.**

1. **Lanes instead of one FIFO.** The pool's single channel plus the `NEEDED` side list become one
   locked set of lanes. A general worker takes, in this order: wanted graphics, piece helpers,
   wanted compute, unwanted graphics, unwanted compute, image-only. Within a lane: first in, first
   out.
2. **A graphics reserve.** `k = clamp(n / 4, 2, 8)` extra threads (`xcl-pso-g*`, 3 on the
   16-thread PC) take only graphics lanes. They never run a compute build, so they never wait at a
   compute gate. This gives the invariant a test can check: *while any graphics build is queued,
   at least k workers are not inside a compute build or its gate.*
3. **Image-only programs come last.** A worker takes one only when every other lane is empty and
   no wanted build waits at any gate, and at most one is in flight at a time.
4. **Kept as is:** U7's parking and extra workers (general workers only, cap 2n), the gate (lanes,
   slots, commit headroom, U6's preload rule), and every wait and budget rule of U4/U5/U5b.
5. **Telemetry:** queue lengths, worker states and gate load on the 2 s `activity` event, and a
   new `pipeline_ready` event with the time from a pipeline's first need to built.

**Why, in one paragraph (run 12).** From 128.2 s to 365.2 s no graphics pipeline was built (237 s;
`pipeline` events). At 300 s all 30 general workers (15 + 15 extra, U7's cap) were busy: 26 waited
in `big_compile_turn` for compute programs and 4 were in the NVIDIA compiler (`log.txt:2331-2385`).
U7 started its extra workers, but they took the next queued item, which was compute again. The
graphics wave at 365 s came when one freed worker got past the compute entries queued before the
graphics ones: `xcl-pso-5` finished a 234.8 s compile at 334.7 s, then spent 30 s on smaller
uncached compute programs that are not gated, and then built about 185 graphics pipelines alone in
4 s (`log.txt:2465-2535`). Graphics builds take 3-57 ms each with a driver blob. The cause is the
order the work is picked in, not the number of threads.

**Size:** 4 units, each under 400 changed lines: U1 lanes and reserve (`pso_async.rs`), U2
image-only last (`pso_async.rs`, `real.rs`), U3 telemetry (fork-only files plus one line), U4 docs.

### 1. Goal and scope

**Goal.** With default switches, a graphics pipeline the scene needs is built within seconds of
its first need, however many huge compute builds are queued or waiting at the gate.

**In scope.**
- The order in which pool workers take builds.
- Which threads may take which builds (the graphics reserve).
- When image-only ("spared") compute programs are built.
- Telemetry that shows the queues, the workers and the time from need to built.

**Out of scope (explicitly).**
- Gate slot counts, commit headroom, lane sizes (`real.rs:4073-4109`, `4166-4179`, `4299-4304`).
- Wait and budget rules of U4/U5/U5b (`pso_async.rs:1557-1690`, `1739-1806`).
- Giving a worker back at the gate (option A). Rejected now, see 5.1. Possible follow-up.
- Gating 128-256 KB compute compiles (upstream #2726). These took 2-22 s each in run 12, ungated
  (`log.txt:2464`, `2478-2534`). The lanes stop them from delaying graphics. Their own cost is a
  separate change.
- Frame rate (2 fps from about 187 s, `log.txt:2002`, `2473`). It has a separate cause, the looks
  before submits.
- Per-cause skip counters (analysis 8.1) and stall-fix U8 counters. They help, but this change
  does not need them.
- The about 33 % of draws still skipped after the graphics wave (394-459 s, see 2.2). This change
  does not explain it.

### 2. Architecture and data flow

#### 2.1 Today (fa14d6f6)

- **Queue.** One `mpsc` channel of `Work` closures (`pso_async.rs:330-347`) and a side list
  `NEEDED` (`pso_async.rs:283-288`). A worker blocks in `recv`. After each item it receives, it
  first drains `NEEDED` (`pso_async.rs:318-328`).
- **Into `NEEDED`.** `need_now` pushes once per pipeline, graphics and compute alike, and sends a
  no-op closure to wake a worker (`pso_async.rs:1710-1722`). It is called by every waiting bind:
  draws through `wait_in_frame` (`pso_async.rs:1739-1740`, from `resolve_draw_with` at 1782-1804),
  compute binds in frame mode (`pso_async.rs:1564-1576`, through `wait_in_frame`) and the limited or unlimited
  compute wait (`pso_async.rs:1599-1602`), plus `need_by_submit` (`pso_async.rs:2077-2080`).
- **Into the channel.** `enqueue` (`pso_async.rs:400-402`): from `resolve` on a lazy pipeline's
  first use (SetPipelineState and others, `pso_async.rs:1521-1535`), from `resolve_within`
  (`1835`), from `resolve_spared` for image-only programs (`1886-1904`), and from `submit` when
  lazy creation is off (`494`). `build_pieces` sends piece helpers (`pso_async.rs:419-443`).
- **Workers.** `compile_threads()` workers (`gpu/recomp/dxc.rs:409-415`; 15 on the PC), named
  `xcl-pso-{k}`, 64 MB stack (`pso_async.rs:336-344`). Preloads run on 2-4 separate
  `xcl-pso-pre-*` threads at the lowest priority (`pso_async.rs:592-640`).
- **The size is known only inside the build.** A lazy compute job translates first. Only then does
  it know its DXIL size (`real.rs:4699-4702`). The gate is reached inside `compute_container`
  (`real.rs:4802-4824`) → `driver_compile_identified` (`real.rs:4202-4205`).
- **Gate.** On an `xcl-pso*` thread, a compile of 256 KB or more with no driver blob waits in
  `big_compile_turn` (`real.rs:4202-4205`, `4238-4240`, `4253-4294`). There are three lanes:
  compute (`BIG_COMPILES`), graphics, and giant (4 MiB and more) (`real.rs:4166-4179`). Unwanted
  builds: `most` = 4 on the PC (`real.rs:4073-4075`) and commit headroom
  (`real.rs:4270-4272`, `4301-4311`). Wanted builds: `wanted_most` = 5 (`real.rs:4107-4109`), no
  commit check. A preload gives way (`real.rs:4269`).
- **U7.** The first time a build waits in the gate it calls `park()` (`real.rs:4289-4291`). On a
  pool worker this counts it and, while `WORKERS - PARKED < n` and `WORKERS < 2n`, starts
  `xcl-pso-x{k}` (`pso_async.rs:351-393`). Extra workers never exit.
- **Image-only.** At creation, `big_and_image_only` marks a compute pipeline spared
  (`real.rs:5997`, `6014-6016`). Binds never wait for it, and its dispatches are left out until it
  is built (`pso_async.rs:1560-1562`, `1886-1904`). Its build is not wanted.

#### 2.2 What run 12 shows (switches in `manifest.json`: `XCL_TELEMETRY=pass`, `XCL_ACTIVITY=1`, `XCL_RUN_SECONDS=720`, `XCL_THREAD_DUMP_AT=300,600`, `XCL_UMA_MB=4095`)

| Time (s) | Graphics built | Compute built | Source |
|---|---|---|---|
| 90-120 | 124 (122 worker, 2 extra) | 229 | `pipeline` events, `per_minute.py` |
| 120-150 | 8 (extra) | 61 | same |
| 128.2 → 365.2 | **0** (a 237 s gap) | 12 | same |
| 360-390 | 188 (185 worker) | 19 | same |
| 390-720 | 2-22 per 30 s | 0-13 per 30 s | same |

- **Skipped draws.** Of the draws with translated pipelines, 76.7 %, 75.8 %, 75.0 %, 75.2 % and
  73.4 % were skipped in the windows 102-157, 157-200, 200-250, 250-297 and 297-351 s. After the
  wave it fell to 44.0 % (351-394 s) and 33.2 % (394-459 s). Source: the "console code so far"
  lines, with `skip_share.py`. The last such line is at 459 s (`log.txt` "262144 draws ...").
- **The 300 s dump** (`log.txt:2228`, 158 threads; `dump_classify.py`). Of 30 general workers, 26
  were in `big_compile_turn` called from `compute_container` and 4 were in `nvgpucomp64`
  (`xcl-pso-0, -1, -5, -x2`). There were 15 extra workers (`xcl-pso-x0` … `x14`,
  `log.txt:2371-2385`), so U7's cap of 2n was reached. Preloads: 2 in the gate, 1 in the driver.
  No graphics build was in the gate.
- **The 600 s dump** (`log.txt:3611`): 25 in the gate, 6 in the driver, 2 idle. **704 s**
  (`log.txt:4227`): 19 in the gate, 9 in the driver, 4 idle.
- **Compute compiles are slow.** Huge compute builds took 183-374 s each in the driver (`pipeline`
  events: `compile_ms` 207,422 / 223,971 / 234,802 / 374,461 …). Waits at the gate reached
  175-440 s (`waited_ms`). Only 20 uncached huge compute compiles started after 90 s
  (`driver compile start … compute … absent`).
- **Graphics compiles are cheap.** The graphics builds at 365-369 s took 9-57 ms each, with a
  driver blob (`pipeline` events, `log.txt:2535ff`).
- **Why the wave came at 365 s.** In order:
  1. `xcl-pso-0` (thread 31368) finished a huge compute at 307.3 s (`log.txt:2405`). It then built
     a 234,552 B compute in 21.9 s, ungated (`log.txt:2464`), and logged nothing more: it parked
     on the next huge one.
  2. `xcl-pso-1` (21372) finished at 323.9 s (`log.txt:2439`) and logged nothing more.
  3. `xcl-pso-5` (2204) finished at 334.7 s (`log.txt:2465`). It built five compute programs of
     73-178 KB in 1.9-13.2 s each (`log.txt:2478-2534`), then started graphics at 365.26 s
     (`log.txt:2535`). It built about 185 of them by 369 s.

  So graphics entries sat in `NEEDED` behind compute entries. The skip rate stayed flat at about
  75 % from 102 to 351 s, so these pipelines were needed the whole time. That is an inference: run
  12 records no time of first need.
- **The image-only programs are queued.** 129 to 255 were queued unwanted at 99.77 s
  (`log.txt:1328`, "(129 so far)"; the counter logs at powers of two). After 90 s, no unwanted
  non-preload huge compute passed the gate (every `pipeline` compute event after the gate has
  `wanted: true`). With 5 wanted compiles running, an unwanted one cannot take a slot (`*n < cap` with
  `cap = most` = 4 for an unwanted build, `real.rs:4268-4270`). So any image-only build a worker picked up is still parked. Run 12
  cannot say how many of the 26 these were. U3's gauges will.
- **Caveat.** `manifest.json` says `commit` 63a216d4. The `by: "extra"` events and the `xcl-pso-x*`
  threads show that U7 (fa14d6f6) code ran. The manifest's commit is stale.

#### 2.3 After

```
need_now (wanted) ─┬─ graphics ─> [WG]        enqueue (unwanted) ─┬─ graphics ─> [UG]
                   └─ compute ──> [WC]                            ├─ compute ──> [UC]
build_pieces helpers ───────────> [H]                             └─ spared ───> [S]   (U2)

general worker (xcl-pso-N, xcl-pso-xN):  WG > H > WC > UG > UC > S*
reserved worker (xcl-pso-gN, k of them):  WG > UG            (never compute, never H, never S)
S* = only if WG, H, WC, UG, UC are empty, no wanted build waits at any gate, and no other
     S build is in flight (at most 1)
Gate: unchanged. U7: unchanged for general workers; reserved workers never count as parked.
```

#### 2.4 The scheduling rule

| Class | Lane | Taken by | Order (general) | At most in flight |
|---|---|---|---|---|
| wanted graphics (a draw or graphics EI waited, `need_now`) | WG | general + reserved | 1 | unbounded (all workers) |
| piece helpers of a split program (`build_pieces`) | H | general | 2 | unbounded |
| wanted compute: small, huge, giant (a compute bind or submit waited) | WC | general | 3 | general workers (n plus U7 extras, at most 2n); slots as today |
| unwanted graphics (sent by `resolve`, nobody waited yet) | UG | general + reserved | 4 | unbounded |
| unwanted compute (sent by `resolve`, `submit` with lazy off) | UC | general | 5 | general workers |
| image-only (spared) compute | S | general | 6, gated | **1** |
| preloads (an earlier run's pipelines) | own list | `xcl-pso-pre-*` | separate threads | as today (U6: at most 1 slot, none while a wanted build waits) |

A pipeline can be in two lanes: UG, then WG when a draw waits. Whichever worker takes `job` first
builds it; the other entry finds `None` and does nothing (`pso_async.rs:404-416`). A wanted
pipeline that turns late stays wanted. Its entry already went to a wanted lane once (the
`by_submit` guard, `pso_async.rs:1714-1716`).

**Invariants (tested in U1 and U2):**
- **I1.** A reserved worker never takes an item from H, WC, UC or S.
- **I2.** While any graphics item (WG or UG) is queued and a reserved worker is idle, a reserved
  worker takes it within one wake-up. So with every general worker parked, a queued graphics build
  still completes.
- **I3.** A general worker takes the first non-empty lane in the order WG, H, WC, UG, UC, and S
  only under the S rule.
- **I4.** At most one S build is in flight. None starts while another lane has items or a wanted
  build waits at a gate.

### 3. Dependencies

#### 3.1 Upstream PRs

| PR | State (2026-11-26) | Size | Tests | Applies to dev? | Defects for us | Decision |
|---|---|---|---|---|---|---|
| #628 "prioritize graphics pipelines in the async PSO worker pool" | open, CONFLICTING | +143 -16, `pso_async.rs` only | 2 unit tests (`graphics_jobs_dequeue_ahead_of_queued_compute_jobs`, `worker_pool_prioritizes_graphics_pipeline_over_queued_compute`); 688 passed before its rebase; RTX 4070 smoke pair in the description | **No.** Its base `queue()` sends `Arc<Pending>` over the old channel (diff hunk `@@ -72,29`). Our tree has `Work` closures, `NEEDED`, `build_pieces` helpers, `RECEIVER` and `park()` (`pso_async.rs:283-443`). The gate's merge check would conflict on every hunk. | (1) The queue holds only `Arc<Pending>`, so the `build_pieces` helpers (`pso_async.rs:432`) have no place in it. (2) It does not know wanted from unwanted, so a SetPipelineState-sent graphics build goes ahead of a compute build the submit waits for. That breaks `a_compute_wait_goes_ahead_of_the_fifo`'s intent the other way. (3) No reserve: once every worker is in a compute gate, graphics wait anyway. That is run 12's exact state. (4) It removes nothing of U7, but its pool cannot start extras. | **Do not adopt.** Take its idea (graphics first) and credit it in U1's commit body. |
| #2628 "a worker waiting for a big compile turn leaves the queue to a helper" | open, MERGEABLE | +186 -18, `pso_async.rs`, `real.rs` | 2 unit tests | Overlaps U7 entirely | Its helpers take the next queued item too, the same failure as U7 in run 12 | Do not adopt |
| #2726 "gate 128-256 KiB compute driver compiles in MEDIUM_COMPILES" | open, CONFLICTING; targets an `amd-dev` branch | +169 -6, `real.rs` | 2 unit tests | No (`MEDIUM_COMPILES` is not in our tree) | n/a | Out of scope; see section 1 |
| #1686 "big compiles of bound programs take turns under a bind budget" | open, CONFLICTING | +142 -1 | 3 tests | No | Gate change for `XCL_PSO_WAIT_MS`; our U5b budgets replaced that need | Not adopted |
| #2053 "catch panics in async PSO builds" | open | +192 -18 | | Probably conflicts with U1 (both edit the worker loop) | A panicking build kills a worker today; unchanged by this spec | Out of scope. Note: if adopted later, rebase it onto U1's `worker_loop`. |

Searches made: "pso worker priority", "pipeline worker pool", "huge compile", "big_compile_turn",
"graphics first pipeline", "compile slot", "pso queue", "park worker", "image-only". No PR found
for "image-only builds last" or for a graphics reserve.

**Sibling project (idea only, GPL-3.0, no code read or copied).** It gives slow pipelines at most a
quarter of its workers. We cannot tell a slow compute build before it runs (2.1, "the size is
known only inside the build"), so we turn the idea around: a quarter of the pool, reserved for
graphics.

#### 3.2 Order between units

U1 → U2 (same file `pso_async.rs`) → U3 (reads U1/U2's `queue_gauges`; one line in `pso_async.rs`)
→ U4 (docs).

### 4. Constraints

- **Memory per compile.** A huge driver compile takes about 1-1.5 GB (the comments at `real.rs:4063-4065`
  and `real.rs:4296-4300`, which cite #1269). Unwanted compiles keep their 3 GB headroom check
  (`real.rs:4296-4311`). Nothing here starts more compiles: slot counts are unchanged. U2 lowers
  the unwanted load: at most one image-only build in flight.
- **Threads.** Today on the PC: 15 general, up to 15 extra, 3 preload, plus `xcl-pso-waits` (300 s
  dump). After: plus k = 3 reserved threads (`clamp(n/4, 2, 8)`). Each reserved thread has a 64 MB
  stack, as the other workers have (`pso_async.rs:341`). Translation recurses deeply, and the stack
  is committed only as it is used. The worst case grows from 34 to 37 pipeline threads.
- **Stack size.** Reserved threads must use 64 MB (`stack_size(64 << 20)`), like
  `pso_async.rs:341`: graphics translation recurses too.
- **The title's watchdog** ends the title when its main thread has been held about 30 s
  (`pso_async.rs:1806-1809` comment). No title-thread wait changes: draws and compute keep the
  100 ms frame budgets (U4/U5b).
- **Platform.** Windows only; the crate does not build on macOS. Units are built and tested on
  the PC (RTX 5080, 16 threads, 63 GB). Never `cargo fmt`. Each runtime unit needs a
  `scripts/smoke.ps1` pair before any upstream PR (CONTRIBUTING.md item 3).
- **Thread names matter.** The gate applies to threads named `xcl-pso*` (`real.rs:4203`). Preload
  detection is the prefix `xcl-pso-pre` (`real.rs:4182`). Telemetry's `built_by` maps
  prefixes (`telemetry_gpu.rs:2414-2421`). The reserved prefix `xcl-pso-g` matches `xcl-pso`, so
  its huge graphics compiles are gated in the graphics lane. It does not match `xcl-pso-pre`.

### 5. Considerations

#### 5.1 Mechanisms compared

| | (A) Give the worker back at the gate | **(B) Lanes + reserved workers (chosen)** | (C) U7 + cap on workers in huge compute |
|---|---|---|---|
| Idea | No slot: the build leaves the worker. It is re-queued on a "huge" list and handed out when a slot frees. | Order by class, and a reserve that never runs compute | Keep U7. Let at most H workers be in a huge compute build or its gate. |
| Fits the code? | Poorly. The job is `FnOnce` (`pso_async.rs:280`), and the gate sits three calls deep in `compute_container`. That code makes two gated attempts (`real.rs:4802-4824`) with blob, retry and logging side effects. Yielding needs a re-runnable job, a new "yield" result through `translated_compute_pso_once`, `compute_container` and `dispatch_chain`, and a translation repeated per attempt. | Well. Only the queue (`pso_async.rs:283-443`, `1710-1722`) changes. Builds and the gate do not. | Poorly. Whether a build is huge is known only after its translation (`real.rs:4702`). A cap on compute in flight also holds back small compute (the 229 cached compute builds of 90-120 s). A cap on parked workers cannot be enforced, because a worker at the gate holds its build on its stack. |
| Guarantee for graphics | Strong (no worker ever waits at the gate) | Strong by construction (I1, I2) | None at the 2n cap |
| Size and risk | Over 400 lines in `real.rs` plus `pso_async.rs`; side effects in a path with known driver retry logic | About 230 + 150 lines; queue logic only | About 150 lines; does not solve run 12 |
| U7 | Removed (nothing parks) | **Kept** (general workers) | Kept |

#### 5.2 Edge cases

- **Duplicate entries.** The same `Pending` can be in UG and WG, or in UC and WC. Taking the job
  is the only claim (`p.job.lock().take()`), so a pipeline is never built twice. An orphan
  (released stand-in) is dropped by whichever entry comes first (`pso_async.rs:405-409`).
- **Spared and `resolve`.** SetPipelineState calls `resolve` (`bind.rs`/`list_x.rs`, many sites),
  which sends a spared compute pipeline through `enqueue`. U2 routes it by its `spared` flag, which
  is set before the title gets the stand-in (`real.rs:6014-6016`; doc at `pso_async.rs:1875-1876`, "Told before the title has the stand-in").
  So a spared build never lands in UC.
- **A spared pipeline that is somehow waited for** (`need_now`) goes to WC like any wanted compute.
  Wanted wins.
- **`XCL_LAZY_PSO=0`.** Every pipeline is enqueued at creation, into UG or UC. Reserved workers
  drain UG. Compute that a bind waits for goes through WC ahead of UG, as `NEEDED` did.
- **`XCL_ASYNC_PSO=0`.** Pipelines are built in the call. The pool is never started. Unchanged.
- **`XCL_PSO_IMAGE_SKIP=0`.** No pipeline is spared, so the S lane stays empty. Those programs are
  waited for as compute, through WC.
- **`XCL_DXC_THREADS=1`.** n = 1 and k = 2. A general worker still exists for compute.
- **Graphics two-pass pipelines** compile a compute pass with `gate = false` (`real.rs:6394`,
  `6404`). So a reserved worker never enters a compute gate.
- **A huge cold graphics build** on a reserved worker waits in the graphics lane (5 slots, wanted).
  It does not park through U7 (reserved threads are not `IS_WORKER`). That waits only on other
  graphics compiles, which are short.

#### 5.3 Deadlock and lost wake-up

- **Lock order.** The lanes mutex is a leaf: nothing else is locked while it is held. The S rule
  reads atomics only (`SPARED_IN_FLIGHT`, `Slots::wanted_waiting`). `need_now` computes `first`
  under `p.demand` and drops that lock before it pushes (as today, `pso_async.rs:1712-1720`).
  `queue_gauges` reads the gate (`Slots::n` locks) after it releases the lanes lock.
- **No lost wake-up for pushes.** A worker checks `pick` and waits under the same mutex that a push
  takes. Every push notifies one general worker. A push to WG or UG also notifies one reserved
  worker. A woken worker that finds no item for its class waits again (a spurious wake-up is
  harmless). Items never outnumber the general notifications. A general worker that finishes a
  build loops back to `take` without waiting, so it sees what is queued.
- **The S rule changes without a push** (a gate's wanted waiters leave, or the spared build in
  flight ends). Two wake-ups cover it: (a) the S guard's drop notifies one general worker; (b) a
  general worker that waits while S is non-empty uses `wait_timeout(250 ms)` and re-checks.
- **`build_pieces`** cannot deadlock: the caller drains its own batch (`pso_async.rs:423-438`), and
  helpers only speed it up. A reserved worker never takes H. With every general worker parked, the
  caller still finishes alone.
- **U7 spawning.** `park()` starts a general worker through the pool, after it is initialised
  (`POOL.get()`, never `get_or_init`, inside `park`). Thread spawning happens outside every lock
  except a `Once`.

#### 5.4 What could starve, and how each makes progress

- **Graphics:** cannot starve behind compute (I1, I2).
- **Wanted compute, small and huge (question 5).** It is third in the general order. It is behind
  only WG and H, whose items take milliseconds (graphics 3-57 ms with a blob, 2.2). The gate is
  unchanged: wanted builds get up to 5 compute slots and skip the commit check
  (`real.rs:4268-4277`). U2 takes image-only builds out of the way: at most one holds a compute
  slot or parks, and none starts while a wanted build waits. U7 still adds general workers while
  fewer than n are free, up to 2n. So the lighting's compute keeps its slots, and its workers are
  freed from image-only parkers.
- **Open risk.** If the title waits for more than about 2n − 6 = 24 distinct uncached huge compute
  programs at once, every general worker parks again. Then small wanted compute waits for a slot
  to free, as today. Graphics are not affected. U3's gauges (`pso_workers.parked`, `pso_q`) show
  it. The follow-up is option A or a higher cap, decided on data.
- **Unwanted builds (UG, UC)** wait behind wanted ones. Nobody waits for them, so that is
  intended. Reserved workers keep UG moving.
- **Image-only:** starve while anything else is queued. That is intended: their dispatches are left
  out anyway (`pso_async.rs:1855-1863`). In a static scene, once the wanted backlog is done, they
  are built one at a time.

#### 5.5 Open risks

- A test that saturates "all workers" now has to saturate the general workers with compute jobs.
  U1 lists the three tests to change.
- Wake-up cost: about 350 pushes a second in the world-start burst (2.2), each waking at most 2
  threads. That is negligible.
- `wanted_waiting_at_gates` counts graphics-lane waiters too, so a waiting wanted graphics compile
  delays image-only builds. That is conservative and intended.

### 6. Work breakdown

#### U1. Lanes and a graphics reserve (`pso_async.rs` only)

- **Inputs:** dev at fa14d6f6.
- **Outputs:** the pool takes builds by lane; k reserved graphics threads; `queue_gauges()`.
- **Files:** `crates/xcl/src/hle/d3d12x/pso_async.rs`.
- **Changes (function by function):**
  1. Delete `NEEDED` and `next_needed` (`pso_async.rs:282-288`).
  2. Replace `RECEIVER` (`305`) with:
     ```rust
     /// What a pool worker runs: a pipeline's build, or a helper of one (`build_pieces`).
     enum Item { Build(Arc<Pending>), Help(Work) }
     /// The pool's lanes, in the order a general worker takes them (`pick`).
     const WANTED_GRAPHICS: usize = 0;
     const HELP: usize = 1;
     const WANTED_COMPUTE: usize = 2;
     const GRAPHICS: usize = 3;
     const COMPUTE: usize = 4;
     const LANES: usize = 5;
     struct Pool { lanes: Mutex<[std::collections::VecDeque<Item>; LANES]>, general: Condvar, reserved: Condvar }
     static POOL: OnceLock<Pool> = OnceLock::new();
     /// Reserved graphics workers started, and those running a build now.
     static RESERVED: AtomicUsize = AtomicUsize::new(0);
     static RESERVED_BUSY: AtomicUsize = AtomicUsize::new(0);
     ```
  3. Add `fn reserved_workers(n: usize) -> usize { (n / 4).clamp(2, 8) }`.
  4. Add the pure picker:
     `fn pick(lens: &[usize; LANES], reserved: bool) -> Option<usize>`. Reserved: `WANTED_GRAPHICS`
     if non-empty, else `GRAPHICS` if non-empty, else `None`. General: the first non-empty lane in
     index order 0..LANES.
  5. Add `fn lane_for(compute: bool, wanted: bool) -> usize`: (false, true) → `WANTED_GRAPHICS`,
     (true, true) → `WANTED_COMPUTE`, (false, false) → `GRAPHICS`, (true, false) → `COMPUTE`.
  6. Replace `queue()` (`330-347`) with `fn pool() -> &'static Pool`. It calls
     `POOL.get_or_init(|| Pool { .. })`, then a `static START: std::sync::Once` spawns:
     `n = compile_threads()` general threads `xcl-pso-{k}` running `worker_loop(false)`, with
     `WORKERS += n` and a decrement on a failed spawn, as today; and `reserved_workers(n)` threads
     `xcl-pso-g{k}` running `worker_loop(true)`, with `RESERVED += 1` per successful spawn. All
     use `.stack_size(64 << 20)`. Keep the comment about deep recursion.
  7. Add `fn push(lane: usize, item: Item)`: `pool()`; lock `lanes`; `push_back`; unlock; if
     `lane == WANTED_GRAPHICS || lane == GRAPHICS` call `reserved.notify_one()`; always call
     `general.notify_one()`.
  8. Add `fn take(reserved: bool) -> Item`: lock `lanes`; loop { compute `lens`; if
     `pick(&lens, reserved)` is `Some(l)`, return `lanes[l].pop_front().unwrap()`; else wait on
     `reserved`'s or `general`'s condvar }.
  9. Replace `worker_loop(rx)` (`318-328`) with `fn worker_loop(reserved: bool)`. If
     `!reserved`, set `IS_WORKER` to true (reserved threads stay false, so `park()` never counts
     them). Then loop: `let item = take(reserved)`; if reserved, `RESERVED_BUSY += 1`; run it
     (`Item::Build(p)` → `build_queued(&p)`, `Item::Help(w)` → `w()`); if reserved,
     `RESERVED_BUSY -= 1`.
  10. `park()` (`368-393`): replace `let Some(rx) = RECEIVER.get() else { return parked };` with
      `if POOL.get().is_none() { return parked; }`. The spawned closure becomes
      `move || worker_loop(false)`. Nothing else in `park`, `Parked`, `extra_needed` or `workers()`
      changes.
  11. `enqueue` (`400-402`): `push(lane_for(p.compute, false), Item::Build(p))`.
  12. `build_pieces` (`432`): `push(HELP, Item::Help(Box::new(move || { ..same body.. })))`.
  13. `need_now` (`1717-1720`): replace the two lines (the `NEEDED` push and the no-op send) with
      `push(lane_for(p.compute, true), Item::Build(p.clone()));`.
  14. Add
      `pub struct QueueGauges { pub lanes: [usize; LANES], pub general: usize, pub parked: usize, pub reserved: usize, pub reserved_busy: usize }`
      and `pub fn queue_gauges() -> QueueGauges`. It reads the lane lengths under the lock (all
      zero when `POOL` is not initialised; do not start it), then `WORKERS`, `PARKED`, `RESERVED`
      and `RESERVED_BUSY`, after the lock is released.
  15. Module doc (`pso_async.rs:1-10`): add one sentence: "Workers take wanted builds before
      unwanted ones and graphics before compute; a few workers build only graphics, so a queue of
      slow compute programs never holds up the scene's draws."
- **Tests to write (`pso_async` `mod tests`):**
  - `general_workers_take_lanes_in_order`: `pick` with general on `[1,1,1,1,1]` → 0,
    `[0,1,1,1,1]` → 1, `[0,0,1,1,1]` → 2, `[0,0,0,1,1]` → 3, `[0,0,0,0,1]` → 4, all zero → None.
  - `reserved_workers_take_only_graphics`: `pick` with reserved on `[1,1,1,1,1]` → 0,
    `[0,1,1,1,1]` → 3, `[0,1,1,0,1]` → None, `[0,0,0,0,0]` → None.
  - `lanes_follow_kind_and_demand`: the four rows of `lane_for`.
  - `reserved_workers_scale_with_the_pool`: n = 1 → 2, 8 → 2, 15 → 3, 32 → 8, 64 → 8.
  - `a_graphics_build_runs_while_every_general_worker_is_parked` (takes `WORKER_SATURATION_LOCK`):
    1. Submit `2 * n` compute blockers (`submit(s, true, ..)`). Each sends on a `started` channel,
       calls `park()` and waits on a shared gate (5 s timeout).
    2. Receive `2 * n` starts (10 s timeout). U7 starts extras up to 2n.
    3. Submit a compute job C (records that it ran) and a graphics job G
       (`submit(s, false, ..)`).
    4. Assert that G ran within 2 s, and that C had not run by then (no general worker free, and
       reserved workers do not take compute).
    5. Open the gate. Wait until all are built (10 s), then `released` every stand-in. Assert
       that `workers().1` returns to its value before the test.
- **Tests to change (blockers must saturate general workers only):** in
  `queued_work_runs_while_every_worker_is_parked` (`pso_async.rs:1083`; `submit(standin, false`
  at 1094 and the target at 1118), `a_released_queued_pipeline_is_not_built_by_normal_workers`
  (1140; submits at 1160 and 1179), and
  `released_queued_pipeline_drops_job_immediately_before_workers_drain` (1240; submits at 1268 and
  1284), change `false` to `true` in those `submit` calls. No assertion text changes. Reason:
  reserved workers would otherwise take graphics blockers or the target.
- **Acceptance:**
  - `cargo test -p xcl --lib -- pso_async::` passes, including the unchanged
    `a_compute_wait_goes_ahead_of_the_fifo` (1365) and
    `a_pipeline_a_submit_needs_is_built_before_the_queue` (925).
  - `cargo test --workspace -- --test-threads=4` passes, with counts and SKIPPED reported.
  - A retail run's 300 s thread dump shows 3 `xcl-pso-g*` threads.
- **Size:** about 130 production lines and 110 test lines.

#### U2. Image-only builds last, one at a time (`pso_async.rs`, `real.rs`)

- **Inputs:** U1 committed.
- **Outputs:** an S lane under the S rule; `real::wanted_waiting_at_gates`, `real::gate_gauges`;
  gate fields in `QueueGauges`.
- **Files:** `pso_async.rs`, `real.rs`.
- **Changes in `real.rs`** (next to `compile_slots`, `real.rs:4171`):
  1. `fn wanted_waiting_of(lanes: &[&Slots]) -> usize`: the sum of
     `wanted_waiting.load(SeqCst)`.
  2. `pub(super) fn wanted_waiting_at_gates() -> usize`:
     `wanted_waiting_of(&[&BIG_COMPILES, &GRAPHICS_COMPILES, &GIANT_COMPILES])`. Atomics only, no
     lock.
  3. `pub(super) fn gate_gauges() -> ([usize; 3], [usize; 3])`: per lane in the order compute,
     graphics, giant, `*s.n.lock().unwrap()` (slots held) and `wanted_waiting`.
- **Changes in `pso_async.rs`:**
  1. Add `const SPARED: usize = 5;` and set `LANES = 6`.
     `static SPARED_IN_FLIGHT: AtomicUsize`, `const SPARED_AT_ONCE: usize = 1`,
     `const SPARED_LOOK: Duration = 250 ms`.
  2. `pick(lens, reserved, spared_ok: bool)`. General: the first non-empty lane of 0..=4; else
     `SPARED` if `spared_ok && lens[SPARED] > 0`; else `None`. Reserved: unchanged.
  3. `lane_for(compute, wanted, spared)`: `wanted` → as U1 (a wanted spared one goes to
     `WANTED_COMPUTE`); else `spared` → `SPARED`; else as U1. `enqueue` passes
     `p.spared.load(Acquire)`; `need_now` passes `false`.
  4. `take(reserved) -> (Item, usize)` (it returns the lane). For a general worker,
     `spared_ok = SPARED_IN_FLIGHT.load() < SPARED_AT_ONCE && super::real::wanted_waiting_at_gates() == 0`,
     computed each time round the loop under the lanes lock. When it picks `SPARED`, it does
     `SPARED_IN_FLIGHT += 1` before it unlocks. When it waits with `lens[SPARED] > 0`, it uses
     `wait_timeout(SPARED_LOOK)` instead of `wait`.
  5. `worker_loop`: for lane `SPARED`, run the build inside a guard whose `Drop` does
     `SPARED_IN_FLIGHT -= 1` and then `pool().general.notify_one()`.
  6. Extend `QueueGauges` with `pub spared_in_flight: usize`, `pub gate_held: [usize; 3]` and
     `pub gate_wanted_waiting: [usize; 3]`. Fill them in `queue_gauges()` after the lanes lock is
     released (`real::gate_gauges()`).
  7. `resolve_spared` (`1886-1904`) is unchanged: it calls `enqueue`, which now routes to S. Fix
     its doc line "takes its turn among the driver's compiles" to "is built when no other build
     waits (`SPARED_AT_ONCE` at a time)".
- **Tests:**
  - `pso_async`: `image_only_builds_come_last_and_only_when_allowed`: `pick` (general) on
    `[0,0,0,0,0,1]` with true → Some(5), with false → None; `[0,0,0,0,1,1]` with true → Some(4);
    reserved on `[0,0,0,0,0,1]` → None.
  - `pso_async`: extend `lanes_follow_kind_and_demand` with (true, false, true) → `SPARED` and
    (true, true, true) → `WANTED_COMPUTE`.
  - `pso_async`: `at_most_one_image_only_build_runs_at_once` (takes `WORKER_SATURATION_LOCK`):
    1. `submit_lazy` three compute pipelines and mark each with `image_only`. Each job increments
       a shared `AtomicUsize`, records the maximum, sleeps 200 ms and decrements.
    2. Call `resolve` on each.
    3. Wait until all are built (5 s).
    4. Assert that the recorded maximum is 1 and that all three were built.
  - `real.rs` `mod big_compile_tests`: `wanted_waiting_is_summed_over_the_lanes`: two local
    `Slots` (as the U6 tests build them) with `wanted_waiting` 1 and 2 → `wanted_waiting_of` = 3.
- **Acceptance:**
  - `cargo test -p xcl --lib -- pso_async:: real::big_compile_tests` passes, and the whole suite.
  - The existing `bind.rs` test at `bind.rs:1690-1702` (a spared pipeline is not put off) still
    passes.
  - In the acceptance run, no more than one `pipeline` event with `wanted: false` and
    `by` ≠ `preload` has an overlapping `[t - compile_ms - waited_ms, t]` interval in which
    `waited_ms > 0`.
- **Size:** about 70 production lines and 70 test lines.

#### U3. Telemetry: queue gauges and time from need to built (fork-only files plus one line)

- **Inputs:** U2 committed (`queue_gauges` with gate fields).
- **Files:** `crates/xcl/src/telemetry_gpu.rs`, `crates/xcl/src/telemetry_run.rs`; one line in
  `pso_async.rs::complete`.
- **Changes:**
  1. `telemetry_gpu.rs::built_by` (`2414-2421`): add `Some(n) if n.starts_with("xcl-pso-g") => "reserved"`
     before the generic `xcl-pso` arm. Update its doc comment.
  2. `telemetry_gpu.rs`: add
     `pub fn telemetry_pipeline_ready(compute: bool, need: Option<Duration>, late: bool, ok: bool)`.
     It returns at once when `need` is `None` or `!xlog::telemetry_on(Tier::Frame)`. Otherwise it
     writes the event `pipeline_ready` with fields from the pure
     `fn pipeline_ready_fields(compute, need, late, ok) -> Vec<(&'static str, serde_json::Value)>`:
     `stage` (`"compute"` / `"graphics"`), `need_ms` (f64, 3 decimals), `late`, `ok`.
  3. `pso_async.rs::complete` (`133-151`): after the `done` block and before the orphan release,
     add one line:
     `crate::report::telemetry_pipeline_ready(p.compute, p.demand.lock().unwrap().needed_at.map(|at| at.elapsed()), p.late.load(Ordering::Acquire), real != 0);`
     (`demand` is locked alone here, never inside `done`.)
  4. `telemetry_run.rs::telemetry_activity` (`318-329`): append the fields of the pure
     `fn activity_queue_fields(g: &crate::hle::d3d12x::pso_async::QueueGauges) -> [(&'static str, serde_json::Value); 3]`:
     - `pso_q`: the array of the 6 lane lengths, in lane order;
     - `pso_workers`: `{general, parked, reserved, reserved_busy, spared_in_flight}`;
     - `pso_gate`: `{held: [c, g, giant], wanted_waiting: [c, g, giant]}`.
- **Tests:**
  - `telemetry_gpu`: extend `built_by_names_the_builder_from_the_thread_name` (`3299`) with
    `built_by(Some("xcl-pso-g1")) == "reserved"`.
  - `telemetry_gpu`: `pipeline_ready_fields_carry_stage_and_need`: (false, 1500.25 ms, true, true)
    → `stage` "graphics", `need_ms` 1500.25, `late` true, `ok` true; (true, …) → `stage` "compute".
  - `telemetry_run`: `activity_queue_fields_follow_the_gauges`: a hand-made `QueueGauges` →
    exact JSON for the three keys.
  - `crates/xcl/tests/telemetry_e2e.rs::activity_events_mirror_the_log` (`425`): assert that
    `pso_q`, `pso_workers` and `pso_gate` exist on `activity` events.
- **Acceptance:** tests pass. In the acceptance run, `activity` lines carry the three keys, and
  `pipeline_ready` events exist for graphics and compute.
- **Size:** about 90 production lines and 60 test lines.

#### U4. Docs

- **Files:** `README.md` (the huge-pipeline paragraph, `README.md:271-290`); add two sentences
  about reserved graphics workers and image-only builds coming last. `docs/guide/telemetry.md`:
  5.5 gets `pipeline_ready` and `by: "reserved"`; the `activity` table gets `pso_q`,
  `pso_workers`, `pso_gate`.
- **PR note:** if `HANDOFF.md` or `docs/specs/` say that workers drain needed builds first in one
  FIFO, say so in the PR. Do not edit them.
- **Acceptance:** review only.

### 7. Concurrency plan

| Wave | Unit | Files | Parallel with |
|---|---|---|---|
| 1 | U1 | `pso_async.rs` | U4's draft (docs only) |
| 2 | U2 | `pso_async.rs`, `real.rs` | none (same file as U1) |
| 3 | U3 | `telemetry_gpu.rs`, `telemetry_run.rs`, `tests/telemetry_e2e.rs`, one line in `pso_async.rs` | U3's telemetry-file part can be written during wave 2 against the `QueueGauges` fields fixed in this spec; it is committed after U2 |
| 4 | U4 | `README.md`, `docs/guide/telemetry.md` | final wording after U3 |

No two units in the same wave write the same file.

### 8. Acceptance run on the retail title

**Build:** dev with U1-U3, clean tree, release, on the RTX 5080 PC (driver 32.0.16.2140). Use the
same DXIL and driver-blob caches as run 12: do not clear them. Make the same input as run 12, then
leave the scene static.

**Switches (as run 12):** `XCL_TELEMETRY=pass XCL_TELEMETRY_SAMPLE=30 XCL_ACTIVITY=1
XCL_RUN_SECONDS=720 XCL_THREAD_DUMP_AT=300,600 XCL_UMA_MB=4095`. No other `XCL_PSO_*`.

**Optional control:** right after it, one run of fa14d6f6 with the same switches. The candidate
runs first, so the warmer blob cache helps the control, not the candidate.

The scene start is the first graphics `pipeline` event with `wanted: true` after 90 s (run 12:
99.8 s).

| Measure | How | Run 12 | Success |
|---|---|---|---|
| Longest gap between graphics `pipeline` events, scene start to end | `per_minute.py`; gap finder | 237 s (128.2 → 365.2) | ≤ 15 s while any `activity.pso_q[0]` or `pso_q[3]` > 0 in that gap |
| Graphics builds per minute, minutes 1-5 after scene start | `per_minute.py` | 124+2, 8, 0, 0, 0 | > 0 in every minute where `activity.pso_q[0] + pso_q[3]` > 0 at its start |
| Time from first need to built, graphics | `pipeline_ready` (`stage` graphics), after scene start | not recorded; at least 237 s for those needed in 128-365 s | p95 ≤ 5 s, max ≤ 30 s |
| Share of draws skipped from scene start + 180 s on | "console code so far" lines, `skip_share.py` | 75.0 %, 75.2 %, 73.4 % (200-351 s) | ≤ 35 % in every window (run 12's post-wave level, 33.2 %, reached without the 4-minute wait) |
| Reserved workers in the 300 s dump | `dump_classify.py` | none | 3 `xcl-pso-g*`, none in `big_compile_turn` under `compute_container` |
| Image-only builds in flight | U2's `pipeline` interval check; `activity.pso_workers.spared_in_flight` | not known | ≤ 1 always |
| Wanted compute still progresses | huge compute `pipeline` events with `wanted: true` after scene start | 15 in 600 s | ≥ 15 in 600 s (no fewer) |
| Safety | log, `report.json`, `activity` | no device removal | no device removal or crash; `builds_title` 0; peak `committed_mb` ≤ 125 % of run 12's |

The run counts as partial if the title crashes for an unrelated reason. The numbers then count up
to the crash.

## 2026-11-26 Shader delivery spec v1, design 1: skip counters, must-draw waits, size classes and build timeouts (the user asked for their spec to be incorporated; orchestrator approved the design; deviations reported to the user)

The requirements are the user's document "Spec - Shader Delivery & Pipeline Compilation
Optimization (Xbox-to-PC Compat Layer) v1" (2026-11-26; kept outside the repository). The user
said "lets incorporate this spec" on 2026-11-26. This entry is the design for its sections 7.2,
7.4, 7.6, 7.7, 8, 9.3 (items 1, 2, 6), 11, 12.2-12.5 and 13 (AC1-AC8, AC10), drafted by an Opus
design worker on dev at 20941768 and reviewed by the orchestrator. Units are Q1-Q10. It replaces
unit U8 of the first stall-fix entry, which was never built. Section 11 lists every departure
from the user's spec.

Status: draft for the orchestrator's review. Nothing here is built or run. Code paths are
`crates/xcl/src/...` on `dev` at 20941768. While this was written, pool U1 landed on `dev` as
6c0f9b15 (`pso_async.rs` only). `pso_async.rs` line numbers below are at 20941768; the U1 names
this design builds on (`push(lane, item)`, `take`, `worker_loop(reserved)`, `park`,
`QueueGauges`, `queue_gauges`) were checked in 6c0f9b15 (`pso_async.rs:313-520`). Other files are
the same in both commits. The user's spec is
`Spec - Shader Delivery & Pipeline Compilation Optimization (Xbox-to-PC Compat Layer) v1.md`
("the spec"); `§n` means its section n. Run data: `/Users/osanchez/xcl-pc/out/runs/runN/`.
Helper script: `scratchpad/spec-design-tools/classes.py` (compile time per stage and size class
from a run's `pipeline` events; `python3 -I classes.py <events.jsonl>`). Upstream PRs were read
as text with `gh pr view -R xcl-dev/xcl` on 2026-11-26.

This design sits on top of the approved pool refinement ("pipeline worker pool with lanes and a
graphics reserve", units U1-U4 there, called **pool U1-U4** here). It assumes pool U1 (lanes,
reserve, `queue_gauges`) and pool U2 (image-only last, `gate_gauges`) are merged, and pool U3
(queue gauges, `pipeline_ready`) is merged before unit Q9. Units here are **Q1-Q10**.

### Summary (recommendation first)

**Recommendation.** Build ten small units, all our own code. Adopt no upstream PR: #2625 (skip
causes) and #1795 (ExecuteIndirect clamp) are read and not taken (section 5).

1. **Counters (decision A).** Every left-out or empty draw and dispatch is counted once, by call
   kind and reason, as a per-frame delta on `frame` and a 2 s delta on `activity` (tier `frame`).
   The reason comes from one read-only function, `pso_async::skipped`, called from one-line call
   sites at the 15 skip sites of `bind.rs` and `console_list.rs`. Totals (`draw_total`,
   `ei_draw_total`, `dispatch_total`, `ei_dispatch_total`) make the spec's ratios exact. Each
   build gets one per-key record (extends pool U3's `pipeline_ready`), and pipelines never built
   get a `pipeline_unbuilt` line at the run's end. A fork script, `scripts/dev/pso-summary.py`,
   writes the per-run summary (`summary.json` plus a table) from the run folder and checks
   AC1-AC8 and AC10. The runtime does not write a summary itself.
2. **Must-draw (decision B).** The runtime gets a phase from the user's marks only: the phase
   control file and `XCL_PHASE_MARK` (already parsed by telemetry), read also with telemetry off.
   No automatic guess: in runs 10-12 the existing guess called the pre-gameplay stretch "menu" or
   "loading" correctly, but also called gameplay "menu" whenever draws were skipped (section 3.4).
   In a marked non-interactive phase (`boot`, `intro`, `loading`, `menu`, `cutscene`), every
   pipeline first needed is must-draw: it waits up to 10 s from its first need (one chance), with
   at most 10 s of such waits per frame, outside the 100 ms frame budgets. Its key is written to
   a per-title list (`must-draw-pipelines.txt`). In later runs a listed key is must-draw without a
   mark. Unknown phase: today's rules, except listed keys. Gameplay: listed keys wait every frame
   inside the existing 100 ms budget. Must-draw builds go to the front of their lane. A compute
   program known to be L or XL is never waited for. All waits stay inside `Held`, so the title's
   own 30 s watchdog does not count them (`kernelx.rs:1529-1541`).
3. **Size classes, timeouts, quarantine (decision C).** S < 256 KiB (ungated, as today), M up to
   768 KiB, L up to 2 MiB, XL from 2 MiB. M and L share the existing compute lane, but L may hold
   at most `cap - 1` of its slots, so an M build always has one. XL compute goes to the existing
   one-slot giant lane, is never "wanted" (so it always waits for commit headroom) and is never
   waited for. Every driver compile is watched: soft timeout logs, hard timeout marks the build
   **abandoned** (binds stop waiting, the slot's count is freed, its commit stays reserved, a
   replacement worker starts within U7's cap, the result is kept if the call ever returns). An XL
   build past its hard timeout also **quarantines** its shader key for the session. The size-gate
   skip `XCL_PSO_COMPUTE_SKIP_BYTES` exists, off by default, and is a `diagnostic` switch. The
   out-of-process compile helper and a per-compile memory cap are deferred (section 4.4).
4. **What is changed or deferred (decision D).** Timeouts are raised where our runs measured
   longer good builds (graphics M cold 14-21 s, compute S 22.6 s, compute L 374 s); XL starts at
   2 MiB, not 1.6 MB; the L cap is "lane cap minus one", not `workers/4`; priority aging, GPU
   markers, the helper process, PCS-based checks and the runtime-written summary are deferred.
   Full table in section 11.

**Size:** ten units in eleven PRs (Q3 is two), each under 400 changed lines; about 2,400 production, test and script lines in all.

### 1. Goal and scope

**Goal.** Every draw or dispatch that draws or computes nothing is counted by cause in every run
(§8.2.1, G5). Content the user marks as non-interactive is drawn even when its pipelines are late
(§8.4, §8.5, AC8). No class of compute build can hold the scene's work indefinitely, and a build
that runs past its limit stops blocking anyone (§7.4, §7.6, G1).

**In scope (spec sections).**

| Spec | What this design does with it |
|---|---|
| §7.2 | Priorities mapped onto pool U1's lanes; P0 = front of a wanted lane (Q7). Aging deferred. |
| §7.4 | Size classes S/M/L/XL, L share cap, XL lane (Q4). |
| §7.6 | Soft and hard timeouts per class, abandon, quarantine (Q5). |
| §7.7 | Logical interface mapped onto existing functions plus `why_unbuilt` (Q1, section 4.5). |
| §8.1-8.3 | Every skip counted by kind and reason (Q1, Q2); skip cascade counter (Q2). |
| §8.4-8.5 | Phase from marks (Q6), must-draw list and waits (Q7). |
| §9.3 items 1, 2, 6 | Size gate classification (Q4), opt-in size skip (Q8), memory guard by admission (Q4, section 4.4). |
| §11.1-11.2 | Counter set (Q2, Q9), per-key log (Q9), summary (Q3). |
| §12.2-12.5 | Decision rule and checks in the checker (Q3); unit tests per unit; acceptance runs (section 9). |
| §13 AC1-AC8, AC10 | Evaluated by `pso-summary.py --check` (Q3). |

**Out of scope (explicitly).**
- §7.3 (U7 itself) and §7.5 (the stronger form, slot-denied requeue): not my sections. U7 stays as
  built; `slot_denied_requeue` is not counted because nothing requeues.
- §3-§6 (capture set, PSDB, driver prewarm, load-time precache): so `psdb_hit/miss`,
  `driver_cache_hit_estimate`, `precache_*` and `direct_draw_generic_variant` are not built.
- §9.3 items 3-5 (optimisation levels, EXEC bypass, direct DXIL backend).
- §10 (wave size).
- Containment of stale indirect counts (#1795 clamp, the GPU-stall options C1/R1). This design
  only counts the hazard (`ei_after_skipped_dispatch`).
- The pool's own order and reserve (pool U1-U2): used, not changed, except that a must-draw build
  goes to the front of its lane (Q7).

### 2. What exists today ("check the fork first" results)

#### 2.1 §7.2, §7.4, §7.6, §7.7: queue, lanes, timeouts

| Spec item | Found in the fork | Where |
|---|---|---|
| Priorities | Two levels only: "wanted" (a title thread waits; `need_now` puts it on `NEEDED`, ahead of the FIFO) and unwanted. Preloads run on separate lowest-priority threads. Pool U1 turns this into lanes WG > H > WC > UG > UC > S. | `pso_async.rs:283-288`, `1710-1724`, `592-640`; pool U1 |
| Size threshold before the driver | One threshold, "huge" = 256 KiB of DXIL (`XCL_HUGE_PSO_BYTES`). A huge compute program is built in the background; a huge compile on an `xcl-pso*` thread waits at the gate. The size is known only after translation, on the worker. | `real.rs:4061-4069`, `4697-4705`, `4202-4205` |
| Lanes | Three gate lanes: compute (`BIG_COMPILES`), graphics (`GRAPHICS_COMPILES`), giant (≥ 4 MiB, 1 slot). Slots: `min(clamp(cpus/4,1,4), RAM GiB/12)`, wanted +1 (4 and 5 on the PC). Unwanted builds also wait for commit headroom (3 GB, or 512 × DXIL above 6 MiB); wanted builds bypass it. Preloads give way. | `real.rs:4073-4109`, `4130-4177`, `4253-4294`, `4301-4311` |
| L/XL distinction, worker-share cap | None. All compute from 256 KiB to 4 MiB shares one lane. | `real.rs:4171-4178` |
| Parking (U7) | A worker that must wait at the gate calls `park()`; an extra worker starts while fewer than n are unparked, up to 2n threads. | `real.rs:4289-4291`; `pso_async.rs:351-393` |
| Build timeouts, abandon, quarantine | None for builds. Bind waits are logged after 1 s (`xcl-pso-waits`), and the opt-in `XCL_PSO_COMPUTE_WAIT_MS=n` limits a bind's wait. | `pso_async.rs:1953-2037`, `1610-1627` |
| Out-of-process compile | None. Driver blobs are kept on disk and handed back with `CachedPSO`, across runs. | `real.rs:4780-4800` |
| Interfaces (§7.7) | `submit`, `submit_lazy`, `resolve`, `resolve_wait`, `resolve_draw`, `is_pending`, `is_spared`, `counts`, `workers`. No query of why a pipeline is not there. | `pso_async.rs:452-472`, `1521-1546`, `1776`, `2066-2088`, `396` |

#### 2.2 §8: fallback

| Spec item | Found in the fork | Where |
|---|---|---|
| Indirect skip | An ExecuteIndirect whose resolved pipeline is untranslated and `unbuilt` is skipped and counted `DRAWS_SKIPPED`, draws and dispatches together. | `bind.rs:292-299` |
| Direct-draw stand-in | A direct draw with an untranslated pipeline goes through with the stand-in (draws nothing) or a native PC pipeline, both counted `DRAWS_PC`. | `bind.rs:571-603` |
| 100 ms budget (run 7 vs runs 10-11) | Run 7 predates it: draws built or waited on the title thread. Now: a shared per-frame draw budget (`XCL_PSO_FRAME_WAIT_MS`, 100), a compute one (`XCL_PSO_COMPUTE_FRAME_WAIT_MS`, 100), and one chance per pipeline (the first budget after its first need), then `late`. | `pso_async.rs:1673-1704`, `1726-1737`, `1739-1771`, `1782-1804` |
| Dispatch skip path | Exists and is counted `DISPATCHES_SKIPPED`: no translated program (late, image-only or failed), ABI error, dropped put-off. Console-list replay has its own uncounted-per-frame skip. Only the total is logged, at powers of two. | `bind.rs:897-915`, `132-134`; `console_list.rs:886-889`, `924` |
| Must-draw or phase | None in the runtime. Phases exist only as telemetry events (schedule, control file, weak guess), written by a thread that runs only with telemetry on. | `telemetry_run.rs:474-760`; telemetry guide §5.9 |
| Title watchdog during waits | While a bind waits, `HELD` is non-zero, so `title_held()` is true; `kernelx::Sleep` does not count held time, and the GPU-wait watchdog takes `held` into account. | `pso_async.rs:224-226`, `259-273`; `kernelx.rs:1529-1566`; `list_x.rs:1612` |

#### 2.3 §9.3: size controls (items 1, 2, 6; the spec's (a)-(d) check)

- (a) The #1269 768 KB compute skip patch: **absent** (no 768 KiB threshold in `real.rs`, `dxc.rs`).
- (b) EXEC-emulation bypass: not my item.
- (c) "> 8 MB → -O0": **present**, on HLSL source length, not DXIL: `UNOPTIMISED_ABOVE` = 8 MiB
  (`gpu/recomp/dxc.rs:499-501`, applied at `dxc.rs:636-641`; `-O1` above 1 MiB).
- (d) Direct DXIL backend: exists (`XCL_DIRECT_DXIL`, telemetry guide switch table); not my item.
- Image-only skip (`XCL_PSO_IMAGE_SKIP`, on): decided at creation from console code
  (`real.rs:5997`, `gpu/effects.rs:99-105`), built later, never waited for
  (`pso_async.rs:1865-1906`).
- Memory guard: admission only. Unwanted huge compiles wait for commit headroom; wanted ones
  reserve but bypass (`real.rs:4253-4277`, `4301-4311`). No per-compile cap.

#### 2.4 §11: counters

- `bind.rs:41-67`: five counters (DRAWS, DRAWS_SKIPPED, DRAWS_PC, DISPATCHES,
  DISPATCHES_SKIPPED), printed at powers of two. These produced O1 (1.29 M) and O2 (277 k).
- `frame` (`log.rs:721-742`, `telemetry_gpu.rs:2710-2737`): `draws`, `dispatches` (only the
  translated ones that went through), `pso_wait_ms`, `pso_wait_draw_ms`, `pso_wait_dispatch_ms`.
- `pipeline` build event (tier `pass`, `telemetry_gpu.rs:2430-2453`): `stage`, `records`,
  `dxil_bytes`, `compile_ms`, `waited_ms`, `source`, `blob`, `by`, `wanted`.
- `activity` (`telemetry_run.rs:318-341`): `presents_2s`, `committed_mb`, `pso_wait_ms`,
  `builds_title`. Pool U3 adds `pso_q`, `pso_workers`, `pso_gate` and the `pipeline_ready` event.
- `report.json` `d3d12.background_pipelines` [built, failed, pending], `pipeline_waits`,
  `dxil_cache` hits and misses (run 12: [2481, 0, 56287]; 4,354 hits, 154 misses).
- Empty stub `telemetry_pipeline_wait_expired` (`telemetry_gpu.rs:2425`).
- **Missing:** per-frame dispatch skips (O6 is unmeasured), any reason split, stand-in vs native
  PC, totals of all issued calls, per-build hold time, cache hit at bind, must-draw outcomes.
- **Planned and not built:** stall-fix U8 (`draws_unbuilt`, `dispatches_unbuilt`, `workers`,
  `gate_parked`, `pso_budget_out`, the `pipeline_wait_expired` error). Q2 and Q9 replace it; see
  the mapping in 4.1.

#### 2.5 What the runs say about compile times by class

From `classes.py` on run 12 (`pipeline` events; classes with this design's thresholds):

| Stage | Class | Blob | Builds | p50 | Max | Largest DXIL |
|---|---|---|---|---|---|---|
| compute | S | no | 320 | 0.6 ms | 22.6 s | 249,088 B |
| compute | M | no | 4 | 37.0 s | 48.0 s | 366,168 B |
| compute | L | no | 13 | 224.0 s | **374.5 s** | 1,603,828 B |
| compute | L | yes | 104 | 10.6 ms | 33 ms | **2,077,556 B** |
| graphics | M | no | 29 | **14.0 s** | **21.3 s** | 422,324 B |
| graphics | S | no | 1,438 | 0.0 ms | 8.9 s | 254,672 B |

Gate waits reached 391 s (L) and 440 s (M). With a driver blob every class builds in under
60 ms. These numbers set the timeouts in Q5.

### 3. Architecture and data flow after the change

#### 3.1 One build, end to end

```
title thread                         worker (pool U1 lanes)                      watchers
------------                         ----------------------                      --------
SetPipelineState -> resolve -> UG/UC lane (queued_at)
draw / dispatch -> resolve_draw / resolve_wait
   need_now: wanted, must_draw?  -> WG/WC lane (front if must_draw)
   wait_in_frame(policy)            take -> build(): picked_at; quarantined key? -> outcome Quarantined, done
     Normal:   100 ms budget, 1 chance       translate -> note_size(bytes): class; XL -> background, notify
     Still:    10 s chance, 10 s/frame        size gate? (XCL_PSO_COMPUTE_SKIP_BYTES) -> outcome SizeGate
     PlayListed: 100 ms budget each frame     driver_compile_identified:
     stop when: built | background |            class -> lane (M/L share BIG, XL -> GIANT, never wanted)
                abandoned | L/XL known           gate wait -> note_driver_compile(bytes, waited)
   built? real pipeline                          compile_watch::enter ---------------> xcl-pso-watch (250 ms)
   not? stand-in; skip site:                     build()  (uncancellable)               soft: log, event
     telemetry_skip(kind, raw pso)               <- returns                             hard: abandon: late,
       -> pso_async::skipped(pso)            complete(): outcome, record ->                 notify, free slot
          (reason, must_draw, bump skips)        telemetry_pipeline_ready(&record)          count, replacement
       -> frame/activity counters                                                          worker; XL: quarantine
```

#### 3.2 Telemetry flow

- Hot-path call sites call one `crate::report::telemetry_*` function each. With telemetry off
  each is one relaxed load (`xlog::telemetry_on`), as today.
- `telemetry_gpu.rs` keeps one `[AtomicU64; COUNTS]` array. `Counters::now` snapshots it;
  `FrameBook::close` takes per-frame deltas; `telemetry_activity` takes 2 s deltas.
- Builds: `complete()` builds a `BuildRecord` (Q1) and passes it to `telemetry_pipeline_ready`.
- End of run: `write_end` writes `pipeline_unbuilt` lines from `pso_async::unbuilt_records`.
- Offline: `pso-summary.py` reads `events.jsonl`, `manifest.json`, `report.json`; writes
  `summary.json` and the table; `--check` evaluates the acceptance criteria.

#### 3.3 Must-draw policy table

| Phase (from marks) | Pipeline is must-draw when | Wait |
|---|---|---|
| `Still` (boot, intro, loading, menu, cutscene, splash, credits) | always, unless it is compute of known class L or XL | `Still`: up to 10 s from first need (one chance), at most 10 s of must-draw waits per frame (own counter), stop early if it turns out L/XL, abandoned or background |
| `Unknown` (no mark, or `auto`, or another name) | its key is in the must-draw list | listed: `Still`; others: today's `Normal` |
| `Play` (`world`) | its key is in the must-draw list | listed: `PlayListed` (the normal 100 ms frame budget, every frame, no one-chance rule, never marked late by a wait); others: `Normal` |

`XCL_MUSTDRAW=0` turns all of this off (every pipeline `Normal`, nothing listed or written).

#### 3.4 Why phases come from marks only (evidence)

`phase` events with `source: "auto"` in runs 10, 11 and 12 (`grep '"ev":"phase"'`):
- Before gameplay the guess alternates `menu` and `loading` every 2-40 s. That would be right for
  the must-draw rule.
- After gameplay starts it keeps alternating `menu` and `loading` (run 10 after 230 s; run 12
  after 209 s), and says `world` only briefly (run 11 at 165 s and 233 s). The rule uses
  translated draws per frame (`telemetry_run.rs:518-534`), and skipped draws are not translated,
  so a bad gameplay frame looks like a menu. Must-draw waits of 10 s in gameplay would follow.
- A latch "non-interactive until the first heavy frame" fails on run 10's case: the world's draws
  started at about 97-137 s (skipped at 744-1,091 a second) and the cards were shown at about
  150-230 s (grey-analysis Q6). The latch would flip before the cards.
So the guess stays telemetry-only and is not used for policy.

### 4. Decisions

#### 4.1 Decision A: the counter set

Every counter: spec name, where it is carried, call site, tier, and kind of value.
"Frame" = per-frame delta on `frame`; "2 s" = delta on `activity`; "gauge" = value at the sample.
All tier `frame` unless noted. Skip fields on `frame` are written only when non-zero (absent = 0);
totals are always written. Field names are flat (telemetry guide §5.3: "Events stay flat").

| Spec counter (§11.1) | Field(s) | Event | Call site (file: function) | Value |
|---|---|---|---|---|
| `indirect_draw_skipped` (building / quarantined / size_gate) | `ei_draw_skip_building`, `_late`, `_abandoned`, `_quarantined`, `_failed`, `_other` | `frame` | `bind.rs: execute_indirect` at the sites now at 295, 307, 334, 343, 366 | frame |
| (total for AC1) | `ei_draw_total` | `frame` | `bind.rs: execute_indirect`, after the put-off return (now 282) | frame |
| `direct_draw_standin` | `draw_standin_building`, `_late`, `_abandoned`, `_quarantined`, `_failed` | `frame` | `bind.rs: graphics_bound` at 578 (`telemetry_pc_call`) | frame |
| (direct draws that left out otherwise) | `draw_skip_other` (two-pass, ABI error) | `frame` | `bind.rs: graphics_bound` at 568, 616 | frame |
| (native PC pipelines; they draw) | `draw_pc` | `frame` | same as `direct_draw_standin` (no reason found) and `execute_indirect` at 334 | frame |
| `direct_draw_generic_variant` | not built (no generic variant exists, §6.3 out of scope) | - | - | - |
| `draw_total` | `draw_total` (direct + indirect draw calls) | `frame`, `activity` | `telemetry_draw` (already called at `bind.rs:559` for every direct draw) + `ei_draw_total` | frame, 2 s |
| `draw_no_output_ratio` | derived: (all `ei_draw_skip_*` + `draw_standin_*` + `draw_skip_other`) / `draw_total`; `activity.draw_no_output` carries the numerator | summary, `activity` | - | derived; 2 s |
| `dispatch_total` | `dispatch_total`, `ei_dispatch_total` | `frame`, `activity` | `bind.rs: dispatch` after the put-off return (now 896); `execute_indirect` (now 282); `console_list.rs` dispatch branch (now 879); `bind.rs: dispatch_dropped` | frame, 2 s |
| `dispatch_skipped` (by reason), per-frame histogram | `dispatch_skip_building`, `_late`, `_image_only`, `_background`, `_size_gate`, `_abandoned`, `_quarantined`, `_failed`, `_other` (direct and indirect together); `activity.dispatch_skipped` (sum) | `frame`, `activity` | `bind.rs: dispatch` at 905, 911; `execute_indirect` at 295 (dispatch signature), 324, 343, 354, 366; `dispatch_dropped` at 133; `console_list.rs` at 888, 924 | frame, 2 s; histogram in the summary |
| skip cascade hint (§8.3) | `ei_after_skipped_dispatch`: ExecuteIndirect calls recorded in a list after a dispatch that may write memory was skipped in it since its last Reset | `frame` | inside `telemetry_indirect_issued` (no extra line) | frame |
| `pipelines_compiled` by kind and class, per minute | `pipeline` build events (tier `pass`) gain `class` | `pipeline` | `telemetry_pipeline_built` (no new line) | per event; per minute in the summary |
| `compile_time_ms` per kind × class | `pipeline.compile_ms` with `class` | `pipeline` | as above | summary p50/p95/max |
| `dxil_size_bytes` per kind, per key | `pipeline.dxil_bytes`; `pipeline_ready.dxil_bytes` | both | as above | per event |
| `queue_depth` per lane | pool U3 `activity.pso_q` (lane order WG, H, WC, UG, UC, S) | `activity` | pool U3 | gauge, 2 s (spec: 1 Hz) |
| `queue_wait_ms` per priority | `pipeline_ready.queued_ms` (first send or need → worker took it) + `gate_ms` (gate waits); `pipeline.queued_ms` | both | `pso_async::complete` (pool U3 line, changed in Q1) | per build |
| `worker_held_ms` (max, p95) | `pipeline_ready.held_ms` (worker took it → done) | `pipeline_ready` | as above | per build; summary max/p95 |
| `workers_active / parked / spawned` | pool U3 `pso_workers` + flat `pso_workers_busy`, `pso_workers_abandoned`, `pso_large_held`, `pso_in_driver` | `activity` | `telemetry_activity` | gauge |
| `slot_denied_requeue` | not built (§7.5 out of scope) | - | - | - |
| `compile_timeout_soft / hard` | `compile_timeout` event, `kind` = `soft` / `hard` | `compile_timeout` | `real/compile_watch.rs: monitor` | per event |
| `quarantined_keys` | `compile_timeout.quarantined: true`; `pipeline_ready.outcome = "quarantined"` | both | as above | per event |
| `cache_hit_layer` / `cache_miss_layer` | first bind found the pipeline built / not built | `activity` | `pso_async.rs: Pending::note_use` (one line) | 2 s |
| `psdb_hit/miss`, `driver_cache_hit_estimate`, `precache_*` | not built (§3, §5, §6 out of scope) | - | - | - |
| `mustdraw_wait_ms` | `mustdraw_wait_ms` | `frame`, `activity` | `pso_async.rs: wait_in_frame_with` (Q7) | frame, 2 s |
| `mustdraw_wait_timeout` | `mustdraw_wait_timeout` (waits that used up a must-draw chance) | `frame` | as above | frame |
| `mustdraw_skipped` | `mustdraw_skipped` (left-out calls whose pipeline is must-draw) | `frame` | inside `telemetry_skip` | frame |
| `failed_builds` | `report.json` `background_pipelines[1]`; `pipeline_ready.outcome = "failed"` | both | existing | end; per build |
| (stall-fix U8) `pipeline_wait_expired` | `error` kind `pipeline_wait_expired`, not fatal | `error` | existing stub call `pso_async.rs:1627` | per event |

**Mapping to existing and proposed names.**

| This design | Existing counter | grey-options §8.1 / §8.2 | stall-fix U8 | #2625 cause |
|---|---|---|---|---|
| `ei_draw_skip_building` / `_late` | part of `DRAWS_SKIPPED` (`bind.rs:295`) | `ei_draw_building` / `ei_draw_late` | part of `draws_unbuilt` | "building" |
| `dispatch_skip_*` from an EI | `DRAWS_SKIPPED` (site 1) or `DISPATCHES_SKIPPED` | `ei_dispatch_pending`, `ei_dispatch_no_shader` | `dispatches_unbuilt` | "no translation", "build failed" |
| `ei_draw_skip_other` | `DRAWS_SKIPPED` (307, 366); uncounted (343) | `ei_two_pass`, `ei_root_args`, `ei_abi` | - | "two-pass", "arguments" |
| `ei_draw_skip_failed` | uncounted (`bind.rs:331-336`) | `ei_pc` | - | "build failed" |
| `draw_standin_*` | part of `DRAWS_PC` (`bind.rs:578`) | `draw_standin_building/late` | `draws_unbuilt` | - |
| `draw_pc` | part of `DRAWS_PC` | `draw_pc` | - | - |
| `dispatch_skip_late` / `_image_only` / `_failed` | `DISPATCHES_SKIPPED` (`bind.rs:905`) | `disp_late`, `disp_image_only`, `disp_no_shader` | `dispatches_unbuilt` | "building", "no translation" |
| `dispatch_skip_other` | `DISPATCHES_SKIPPED` (911, 133) | `disp_abi`, `disp_dropped_submit` | - | "dropped at submit" |
| `pso_workers_*`, `pso_q` | `workers()` | §8.2 `workers_*`, `needed`, `queued` | `workers`, `gate_parked` | - |

The existing five counters and their log line stay unchanged (the source of O1 and O2).

**What stays out, and why.**
- GPU markers around skipped draws (§11.2, §12.4): deferred. A skipped draw records nothing, so
  a marker only marks a position; the per-frame counters and pass events already place skips;
  frame captures of this title at 2 fps are impractical now; and writing a marker must use the
  original list function, not a hooked one, which needs its own check. Outline for later: an
  opt-in `XCL_SKIP_MARKERS=1`, one `SetMarker` through `real::orig_fn` inside `telemetry_skip`.
- `queue_depth` at 1 Hz: kept at the 2 s `activity` cadence (one sampler, no new thread).
- Per-transition events (§7.7 "every state transition emits an event"): 56,000 pipelines a run
  (report.json pending) would flood the file. Only terminal and exceptional transitions are
  written: built or not (`pipeline_ready`), timeout (`compile_timeout`), still unbuilt at the end
  (`pipeline_unbuilt`).

**Per-run summary (§11.2).** `scripts/dev/pso-summary.py <run-dir>` (Q3), fork-only, standard
library only, like `smoke-events.py`.
- It writes `<run-dir>/summary.json` and prints a table. It can run during a run: `events.jsonl`
  is written unbuffered, line by line (telemetry guide §5.1), so a partial file is valid.
- It reads `report.json` (copy in the run folder) for `background_pipelines` and `dxil_cache`.
- It does not touch the smoke block. `smoke-events.py` and `smoke.ps1` stay as they are.
- `--check` evaluates AC1-AC8, AC10 and the §12.2 decision rule (section 9).
- The runtime does not write a summary every 60 s (deviation, section 11).

#### 4.2 Decision B: must-draw and phases

**How the runtime knows the phase.** Options:

| Option | For | Against | Decision |
|---|---|---|---|
| B1 Marks: phase control file and `XCL_PHASE_MARK`, read by the runtime too | Exact; a person or the run script says what the screen shows; already parsed (`telemetry_run.rs:577-685`) | Needs someone to mark; a schedule's seconds drift between runs | **chosen** (with B2) |
| B2 Per-title list of pipeline keys captured from a marked run | Works in later unmarked runs; cheap (a hash set read once per pipeline) | Coarse: every pipeline first needed during the cards is listed, including the world's own | **chosen** |
| B3 Existing automatic guess (`classify_phase`) | No setup | Calls gameplay "menu" when draws are skipped (3.4) | rejected for policy |
| B4 Latch "non-interactive until the first heavy frame" | No setup | Flips before the cards in run 10 (3.4) | rejected |
| B5 Title signals: no input read; present cadence | - | GameInput is polled always (the reading is the state at call time, `gameinput.rs:1-13`); no measured cadence difference (run 10 menus ran ~29 fps; the cards' window was a stall) | rejected |
| B6 Content rule: draws whose render target is a presented image | Picks UI and cards | `rt0_resource` is tracked only with `XCL_TRACE_FRAME` (`present.rs:256-266`); per-draw lookups under two mutexes; the cards' content may be drawn off-screen first | rejected now |

**How a pipeline becomes must-draw.** At its first need (`need_now`): listed, or the phase is
`Still`. At creation (`submit_as`): listed. A must-draw flag never clears. In `Still`, the key of
each newly must-draw pipeline is appended to `must-draw-pipelines.txt` (beside
`used-pipelines.txt`, the same folder; at most 4,096 keys). Keys are the shader-set key
`shaders_key` (`real.rs:5742-5748`), the same key `used-pipelines.txt` uses. Compute that feeds a
must-draw draw (§8.4 bullet 3) is not detected; in `Still` all compute is must-draw anyway
(except known L/XL).

**Wait policy and how it combines with what is built.** See 3.3. In detail:
- `Still` waits do not spend the 100 ms draw or compute frame budgets. They have their own
  counter `MUSTDRAW_FRAME_SPENT`, budget 10 s (`XCL_MUSTDRAW_FRAME_MS`). Worst case per frame:
  10 s + 100 ms (draws) + 100 ms (compute).
- The one-chance rule stays, with the must-draw chance (10 s, `XCL_MUSTDRAW_WAIT_MS`) instead of
  100 ms. A pipeline whose chance is over becomes `late` and is logged as
  `mustdraw_wait_timeout`.
- A frame whose must-draw budget is spent leaves the pipeline out for now, not late (as today).
- `XCL_PSO_WAIT_MS`, when set, still caps every wait (as `cap` today).
- A wait stops early when the pipeline is built, abandoned (Q5), background XL (Q4) or known to be
  compute of class L/XL (spec §6.2 "never wait on L/XL compute"; size is known once the worker
  has translated it).
- Must-draw builds go to the **front** of their lane (P0 before P1), and listed keys go to the
  front of the preload list.
- **Title watchdog.** Every wait runs inside `Held` (`pso_async.rs:1758-1762`), so `title_held()`
  is true; `kernelx::Sleep` sleeps through held time without counting it
  (`kernelx.rs:1553-1566`), and the GPU-wait watchdog passes `held` (`list_x.rs:1612`). A 10 s
  hold therefore does not move the title's 30 s count. The per-frame cap bounds any single hold
  to 10 s, far below `XCL_STALL_DUMP` values in use (120 s).
- **Default when the phase is unknown:** today's rules for every pipeline not on the list. AC8
  then relies on the starvation fix (pool U1: graphics reserve) and on a list from an earlier
  marked run. This is deliberate (spec §8.2.2: "starvation fixes come first").

#### 4.3 Decision C: size classes, lanes, timeouts, quarantine

**Classes** (by DXIL bytes, after translation; tunables):

| Class | Bytes | Switch | Compute lane | Graphics lane |
|---|---|---|---|---|
| S | < 256 KiB | `XCL_HUGE_PSO_BYTES` (existing) | ungated (as today) | ungated |
| M | 256 KiB - < 768 KiB | `XCL_PSO_L_BYTES` (new, default 786,432) | `BIG_COMPILES` | `GRAPHICS_COMPILES` |
| L | 768 KiB - < 2 MiB | `XCL_PSO_XL_BYTES` (new, default 2,097,152) | `BIG_COMPILES`, at most `large_cap` slots | `GRAPHICS_COMPILES`, logged as anomaly |
| XL | ≥ 2 MiB | - | `GIANT_COMPILES` (1 slot), never wanted, never waited for | `GRAPHICS_COMPILES` below 4 MiB, giant from 4 MiB (as today); anomaly |

**L cap options.**

| Option | L concurrency on the PC | For | Against | Decision |
|---|---|---|---|---|
| C-a Spec: `max(1, floor(n/4))`, ≤ n - 2, own lane | 3 (n = 15) | spec number | below the measured best of 4 on a 16-thread RTX 5080 (#2098, stall-fix entry); an own lane with M's 4-5 slots doubles concurrent big compiles and their 1-1.5 GB each | rejected |
| **C-b L shares the compute lane, at most `cap - 1` of its slots (≤ n - 2)** | 3 unwanted / 4 wanted | memory envelope unchanged; M always has one slot; L throughput at the measured peak | not the spec's formula | **chosen** |
| C-c No L cap | 4 / 5 | none | M builds wait behind L for minutes (gate waits 440 s for M in run 12, 2.5) | rejected |

The spec's worker reason for the cap (no class may hold every worker, §7.4 invariant) is met by
pool U1's graphics reserve (k = 3 on the PC) and by U7's extra workers, not by slot counts.

**Timeouts** (driver time only: from just before the driver call to its return; gate waits are
bounded by the gate's own rules):

| Class / kind | Soft (log + event) | Hard (abandon) | After hard |
|---|---|---|---|
| S/M graphics | 2 s | 60 s | abandon |
| S/M compute | 5 s | 120 s | abandon |
| L (either kind) | 60 s | 450 s | abandon |
| XL (either kind) | 60 s | 300 s | abandon and quarantine the key |

`XCL_PSO_HARD_TIMEOUTS=0`: soft events and logs only.

**What "abandoned" means** (the call cannot be cancelled in-process):
- **Worker count.** The worker is counted in `ABANDONED`. U7's rule becomes
  `extra_needed(workers, parked + abandoned, n)`, so a replacement starts within the 2n cap. Only
  general pool workers count (as `park`). A reserved graphics worker is not replaced (the other
  k - 1 and the general workers still take graphics).
- **Slot.** The slot's count is given back at once, so the lane goes on. Its commit reservation
  stays until the call returns, so admission of unwanted builds still sees the memory.
- **Pipeline.** `late` and `abandoned` are set and waiters are woken: binds stop waiting and its
  calls are left out with reason `abandoned`. If the call returns a valid pipeline it is kept and
  used; if not, the build counts as failed.
- **Telemetry.** A `compile_timeout` event (`kind: "hard"`), a warning, `pipeline_ready.abandoned`.

**Quarantine.** A session set of shader keys (`QUARANTINE`), filled by an XL hard timeout. A
pipeline whose key is quarantined is never built: at pickup the worker drops its job and completes
it with outcome `quarantined`. Its calls are left out with reason `quarantined`. Key 0 (unknown)
is never quarantined. Not persisted (spec: session only).

**Size-gate skip (§9.3.2).** `XCL_PSO_COMPUTE_SKIP_BYTES=<bytes>` (unset or 0: off). On a pool or
preload worker, a compute program whose translated DXIL is at least that size is not compiled:
outcome `size_gate`, its dispatches are left out with reason `size_gate`, one log line per key
(first 16, then powers of two). Listed in `manifest.diagnostic` and so in `compare_key`.
How it differs from the image-only skip:

| | Image-only skip (`XCL_PSO_IMAGE_SKIP`, on) | Size gate (`XCL_PSO_COMPUTE_SKIP_BYTES`, off) |
|---|---|---|
| Decided | at creation, from console code: ≥ 16 KiB and writes no buffer | after translation, from DXIL size |
| What it may leave out | images only, never counts an ExecuteIndirect reads | anything, including culling counts: a GPU hang risk (`pso_async.rs:1857-1864`) |
| Built? | yes, last and one at a time (pool U2) | never in the session |
| Purpose | normal operation | diagnostic and stability lever (#1269) |

**Memory guard (§9.3.6).** In-process we cannot cap one compile's memory. What this design does:
XL is never wanted, so it never bypasses the commit-headroom check (3 GB, or 512 × DXIL above
6 MiB: a 14 MB program needs about 7 GB free), and only one XL compile runs at once. A per-compile
4 GB cap needs the helper process (4.4).

#### 4.4 The out-of-process compile helper: deferred

**What it would take.**
- A helper mode of the `xcl` binary (`xcl pso-helper`) that creates its own D3D12 device on the
  same adapter (by LUID), takes requests over a pipe (DXIL, serialized root signature, the
  pipeline description), compiles, and returns `GetCachedBlob()`.
- The main process creates the pipeline from that blob (`CachedPSO`), which takes milliseconds.
  The fork already does this with blobs kept on disk across runs (`real.rs:4785-4800`), so blobs
  from another process of the same driver are known to work.
- A job object per helper with a memory limit (4 GB), killed on hard timeout.
- Process lifetime, crash handling, a second device's video memory, the pipe protocol, and tests
  that start the helper.

**Why not now.** Over 800 lines across a new process mode and IPC; it changes the runtime's
process model; and in-process abandon already removes the blocking effect of a stuck build. The
remaining cost of a stuck build is one thread and its memory. Revisit if a run shows an abandoned
compile still running at run end, or commit growth above the guard (AC11, not in this design).

#### 4.5 §7.7 interface mapping

| Spec | Here |
|---|---|
| `Submit(request) → ticket` | `submit` / `submit_lazy`; the ticket is the stand-in pointer |
| `Query(ticket)` | new `why_unbuilt(pso) -> Option<Unbuilt>` (None = built or not ours); `is_pending` |
| `WaitFor(ticket, max_ms)` | `wait_in_frame_with` under the must-draw policy (Q7) |
| `SetLaneLimits(lane, slots)` | environment switches only (`XCL_HUGE_PSO_*`, `XCL_PSO_L_BYTES`, `XCL_PSO_XL_BYTES`) |
| `Drain(deadline)` for tests | tests use `wait_built` (`pso_async.rs:779`) |
| events per transition | terminal and exceptional only (4.1) |

### 5. Dependencies

#### 5.1 Upstream PRs

| PR | State (2026-11-26) | Size | What it does | Decision |
|---|---|---|---|---|
| #2625 "count skipped draws and dispatches by cause" | open, MERGEABLE against upstream main | +403 -28, `bind.rs`, `list_x.rs`, `pso_async.rs` | A `Cause` enum, `skip(total, cause)` replacing every `count(..._SKIPPED)`, causes appended to the power-of-two log line; mostly about why a call could not be put off (bundle, predicated, no wrapper); `build_state(pso)`. One behaviour change: a replayed put-off ExecuteIndirect whose stand-in was released is skipped. | **Do not adopt.** No per-frame values, no building/late split, no stand-in vs native split; it rewrites every skip site our one-line call sites need, and its put-off causes rarely occur in frame mode (U5b decision 5: nothing is put off by default). Reuse its cause words where they match ("building", "build failed", "no translation", "dropped at submit", "two-pass"). Its replay fix matters only with `XCL_PSO_COMPUTE_WAIT_MS` set; note it for the GPU-stall workstream. |
| #1795 "clamp ExecuteIndirect argument records and total work on the GPU" | open, CONFLICTING | +573 -11, 6 files | GPU clamp of indirect counts | Not part of this design (containment belongs to the GPU-stall options C1). Q2 only counts `ei_after_skipped_dispatch`. |
| #2726, #2628, #628 | open | - | gate 128-256 KiB, give the worker back, graphics first | already decided against in the pool refinement entry |
| Searches "pipeline timeout", "quarantine pipeline", "compile timeout", "must-draw", "intro cards pipeline", "size class compile", "abandon compile" | - | - | nothing that does a part of this design | - |

#### 5.2 Files

| Unit | Files |
|---|---|
| Q1 | `hle/d3d12x/pso_async.rs`; one line each in `hle/d3d12x/real.rs` (`translated_compute_pso_once`, `driver_compile_identified`); `telemetry_gpu.rs` (two stubs and pool U3's function signature) |
| Q2 | `telemetry_gpu.rs`, `telemetry_run.rs`, `log.rs` (appended part); one-line call sites in `hle/d3d12x/bind.rs`, `hle/d3d12x/console_list.rs` |
| Q3 | `scripts/dev/pso-summary.py`, `scripts/dev/test_pso_summary.py` |
| Q4 | `hle/d3d12x/real.rs`, `hle/d3d12x/pso_async.rs` |
| Q5 | `hle/d3d12x/real/compile_watch.rs` (new), `hle/d3d12x/real.rs`, `hle/d3d12x/pso_async.rs`, `telemetry_gpu.rs` (one stub) |
| Q6 | `telemetry_run.rs` |
| Q7 | `hle/d3d12x/pso_async.rs` |
| Q8 | `hle/d3d12x/pso_async.rs`, `hle/d3d12x/real.rs` |
| Q9 | `telemetry_gpu.rs`, `telemetry_run.rs`, `log.rs` (appended part), `crates/xcl/tests/telemetry_e2e.rs`; one line in `hle/d3d12x/real/compile_watch.rs` |
| Q10 | `README.md`, `docs/guide/telemetry.md` |

#### 5.3 Order

- All units come after pool U1 and pool U2 are merged into `dev` (they replace `NEEDED`, the
  channel and `worker_loop`, which Q1, Q5 and Q7 edit).
- Q1 after pool U3 too (it changes U3's `telemetry_pipeline_ready` signature).
- Q2 and Q4 after Q1. Q5 after Q4 (classes). Q6 after Q2 (same file). Q7 after Q2, Q4, Q5, Q6.
  Q8 after Q7 (same file). Q9 after Q5 and Q8's predecessors (reads Q4/Q5 gauges; fills Q5's
  stub). Q3 any time (it reads field names fixed here). Q10 last.

### 6. Constraints and considerations

- **Hot draw path.** About 400 draws and 4,000 dispatches a frame at about 2 fps in the scene.
  With telemetry off each call site costs one relaxed load. With it on: one `fetch_add` per issued
  call; a skip also takes `pso_async::skipped`, which locks `PENDING` once (the same lock
  `entry()` takes in every bind today). Must-draw adds work only when a pipeline is not built: a
  relaxed load of the phase class and two atomic flags. The list is read once per pipeline at its
  first need and creation (an `RwLock` read).
- **Event volume.** Skip fields are written only when non-zero; a healthy frame line grows by the
  four totals and two must-draw fields. `pipeline_ready` is written for needed or exceptional
  builds only (run 12: about 2,500 builds in all). `pipeline_unbuilt`: at most 2,000 lines once.
- **The title's watchdog.** Covered in 4.2. Abandon never holds a title thread: it only wakes
  waiters. Quarantine and the size gate leave dispatches out, as late ones are today.
- **Memory.** No more concurrent big compiles than today for M+L (shared lane). XL: one slot,
  always behind the headroom check (it was in the giant lane already from 4 MiB). An abandoned
  compile keeps its commit reservation. Replacement workers: 64 MB stack each, virtual, within 2n.
- **Edge cases.**
  - A pipeline needed during `Still` and bound again in `Play`: stays must-draw; in `Play` only
    listed keys use `PlayListed`. A `Still`-marked pipeline is listed when first needed, so in the
    same run it is also listed.
  - An abandoned build that later returns: `complete()` stores the pipeline; skips stop; the record
    says `abandoned: true, outcome: "built"`.
  - An abandoned build on a preload thread: counted for telemetry, no replacement (preloads are not
    pool workers).
  - A quarantined key that is already built for another stand-in: that pipeline stays.
  - `XCL_ASYNC_PSO=0`: no `Pending`, no worker: classes still pick lanes, timeouts still log (no
    pipeline to abandon), the size gate does nothing (it applies only on pool or preload workers).
  - `XCL_LAZY_PSO=0`: unchanged; must-draw marks apply at first need.
  - `XCL_PSO_COMPUTE_WAIT_MS` set (limit or wait mode): must-draw applies only in frame mode
    (`wait_in_frame`); the opt-in modes keep their meaning.
  - Two waiters on one must-draw pipeline: each waits within its own chance; the frame cap is
    shared.
- **Failure modes.**
  - Phase file never marked `world` after gameplay starts: every new pipeline waits up to 10 s
    (chance) and at most 10 s per frame. Slow, bounded, watchdog-safe. The checker flags it
    (`mustdraw_wait_ms` > 0 after the scene start).
  - A stale must-draw list (a title update): unknown keys never match; harmless.
  - A hard timeout on a build that would have finished: abandoned, result kept; cost one extra
    thread and one extra slot of concurrency for the overlap.
- **Open risks.**
  - Cold graphics M builds take 14-21 s (2.5): a card whose pipelines are cold misses its 10 s
    chance and draws late. Driver blobs make it about 13 ms. AC8 in a blob-cold run may fail for
    this reason alone; the checker reports per-pipeline must-draw waits so the cause is visible.
  - The graphics reserve covers P0/P1 graphics; P1 compute has no hard reservation (pool spec
    5.4 open risk). The checker measures it (AC7 second half).
  - XL never waited for: if a needed memory-writing program is XL, its dispatches are left out
    until it is built, as today after 100 ms; the stale-count hazard is unchanged.

### 7. Work breakdown

Line numbers are on `dev` 20941768; pool U1/U2 change `pso_async.rs`, so find code by function
name. Every unit: `cargo build --workspace` and `cargo test --workspace -- --test-threads=4` on
the PC with counts and SKIPPED lines; never `cargo fmt`; telemetry code in the fork-only files.

#### Q1. Build facts and skip reasons (`pso_async.rs`, two lines in `real.rs`, stubs)

- **Inputs:** pool U1-U3 merged.
- **Outputs:** `Facts` on every `Pending`; `Unbuilt`, `Skipped`, `why_unbuilt`, `skipped`;
  `BuildRecord` and `record`; `note_size`, `note_driver_compile`; `queued_here`,
  `must_draw_here`; `unbuilt_records`; `GENERAL_BUSY`.
- **Changes in `pso_async.rs`:**
  1. Add to `Pending` a field `facts: Facts`:
     ```rust
     #[derive(Default)]
     struct Facts {
         /// binds left out for it (`skipped`)
         skips: std::sync::atomic::AtomicU32,
         /// µs title threads waited for it, all binds together
         waited_us: AtomicU64,
         /// the largest DXIL its build translated or compiled (0: not known yet)
         dxil_bytes: AtomicUsize,
         /// µs its driver compiles waited at the gate
         gate_us: AtomicU64,
         /// first sent to the workers or needed (`enqueue`, `need_now`)
         queued_at: Mutex<Option<std::time::Instant>>,
         /// a worker took it (`build`)
         picked_at: Mutex<Option<std::time::Instant>>,
         /// how it ended (OUTCOME_*), set before `complete`
         outcome: std::sync::atomic::AtomicU8,
         /// must-draw (set by Q7; read by `skipped` and the record)
         must_draw: std::sync::atomic::AtomicBool,
         /// its driver compile ran past its hard limit (set by Q5)
         abandoned: std::sync::atomic::AtomicBool,
     }
     const OUTCOME_BUILT: u8 = 0; const OUTCOME_FAILED: u8 = 1;
     ```
     `submit_as` initialises it with `Default::default()`.
  2. `pub enum Unbuilt { Building, Late, ImageOnly, Failed }` with `pub fn name(self) -> &'static str`
     (`building`, `late`, `image_only`, `failed`), and `pub struct Skipped { pub why: Option<Unbuilt>, pub must_draw: bool }`.
  3. `fn why_of(p: &Pending) -> Option<Unbuilt>` (pure on the flags): `real != 0` → None; `done` →
     `Failed`; `spared` → `ImageOnly`; `late` → `Late`; else `Building`.
  4. `pub fn why_unbuilt(pso: usize) -> Option<Unbuilt>`: `entry(pso)` then `why_of`; None when no
     entry.
  5. `pub fn skipped(pso: usize) -> Skipped`: with an entry, bumps `facts.skips`, returns
     `why_of` and `must_draw`; without one, `Skipped { why: None, must_draw: false }` (the caller
     counts `None` as `failed`: a console pipeline with no translation).
  6. Set `queued_at` once (`get_or_insert_with(Instant::now)`) in `enqueue` and in `need_now`; set
     `picked_at` at the start of `build` (`pso_async.rs:76`).
  7. Add `facts.waited_us` the elapsed µs at each place `note_blocked` is called for a pipeline
     (`wait_in_frame`, `resolve_wait_inner` twice).
  8. `pub(super) fn note_size(bytes: usize)`: for the `BUILDING` pending of this thread (None: do
     nothing), `facts.dxil_bytes.fetch_max(bytes)`. Q4 extends it.
  9. `pub(super) fn note_driver_compile(bytes: usize, gate_wait: std::time::Duration)`: same
     thread rule; `dxil_bytes.fetch_max`, `gate_us.fetch_add`.
  10. `pub fn queued_here() -> Option<std::time::Duration>`: for the `BUILDING` pending,
      `picked_at - queued_at` when both are set. `pub fn must_draw_here() -> bool`.
  11. ```rust
      pub struct BuildRecord {
          pub compute: bool, pub key: u64,
          pub need: Option<std::time::Duration>,   // first need → now
          pub queued: Option<std::time::Duration>, // first send or need → picked
          pub held: Option<std::time::Duration>,   // picked → now
          pub waited: std::time::Duration, pub gate: std::time::Duration,
          pub dxil_bytes: usize, pub skips: u32,
          pub late: bool, pub must_draw: bool, pub abandoned: bool,
          pub outcome: &'static str,               // "built", "failed", later "size_gate", "quarantined", "pending"
      }
      fn record(p: &Pending, real: usize) -> BuildRecord
      ```
      `outcome`: `real != 0` → `built`; else the name of `facts.outcome`. `need` from
      `demand.needed_at` (locked alone).
  12. In `complete` (`pso_async.rs:133`), the pool U3 line becomes
      `crate::report::telemetry_pipeline_ready(&record(p, real));` (same place: after the `done`
      block, before the orphan release).
  13. `pub fn unbuilt_records(limit: usize) -> Vec<BuildRecord>`: entries of `PENDING` not done,
      with `skips > 0` or `needed_at` set, sorted by `skips` descending, first `limit`
      (`outcome: "pending"`). It clones the `Arc`s under the `PENDING` lock and reads facts after.
  14. `Pending::note_use` (`pso_async.rs:116`): after computing `e`, add
      `crate::report::telemetry_first_use(self.real.load(Ordering::Acquire) != 0);` before the
      `note_first` call. It runs once per pipeline (inside the existing `used` once-guard: place it
      inside the `if super::pso_manifest::note_first(...)` branch).
  15. `static GENERAL_BUSY: AtomicUsize`, `+1`/`-1` around running an item in `worker_loop` when
      `!reserved` (pool U1's loop). Add `pub busy: usize` to pool U1's `QueueGauges`, filled in
      `queue_gauges()`.
- **Changes in `real.rs`:**
  - `translated_compute_pso_once`, right after `let dxil = compiled.dxil;` (now 4697):
    `super::pso_async::note_size(dxil.len());`
  - `driver_compile_identified`, right after `let waited = waited.elapsed();` (now 4206):
    `super::pso_async::note_driver_compile(bytes, waited);`
- **Changes in `telemetry_gpu.rs`:**
  - `pub fn telemetry_first_use(_ready: bool) {}` (stub; Q2 fills it).
  - Pool U3's `telemetry_pipeline_ready(compute, need, late, ok)` becomes
    `telemetry_pipeline_ready(r: &crate::hle::d3d12x::pso_async::BuildRecord)`, writing exactly
    U3's four fields from `r` (`stage`, `need_ms`, `late`, `ok` = `outcome == "built"`), same
    early return when `r.need` is None. U3's test `pipeline_ready_fields_carry_stage_and_need`
    is updated to build a `BuildRecord`.
- **Tests (`pso_async` test modules):**
  - `a_skip_names_why_the_pipeline_is_not_there`: lazy compute stand-in with a blocking job,
    `resolve`d: `why_unbuilt` = Building; set `late` → Late; a second one marked `image_only` →
    ImageOnly; a job returning 0 after `wait_built` → Failed; a built one → None; an unknown
    pointer → None. Four calls of `skipped` on the first → `facts.skips` = 4 and `why` = Late.
  - `a_build_record_carries_its_times`: `submit_lazy` a graphics job that sleeps 50 ms; `resolve`;
    `wait_built`; capture the record by calling `record(&p, real)` on the entry: `queued` Some,
    `held` ≥ 50 ms, `outcome` "built", `skips` 0.
  - `driver_compile_facts_go_to_the_build_on_this_worker`: a job that calls
    `note_driver_compile(300 << 10, 5 ms)` and `note_driver_compile(200 << 10, 5 ms)` and
    `note_size(250 << 10)`: the record has `dxil_bytes` 307,200 and `gate` 10 ms.
  - `unbuilt_records_list_needed_pipelines_by_skips`: two blocking stand-ins, one needed
    (`need_by_submit`) and skipped 3 times, one only `resolve`d: the list has the first only.
- **Acceptance:** `cargo test -p xcl --lib -- pso_async:: telemetry` passes; whole suite passes.
  No behaviour change (no switch, no wait or order change).
- **Size:** about 150 production and 120 test lines.

#### Q2. Skip and total counters per frame and per 2 s (telemetry files, one-line call sites)

- **Inputs:** Q1 committed.
- **Outputs:** frame and activity fields of 4.1; `error` kind `pipeline_wait_expired`.
- **Changes in `telemetry_gpu.rs`:**
  1. Counter indices and names, in one table:
     ```rust
     /// Counted calls, in this order; frame and activity fields take these names.
     pub const COUNT_NAMES: [&str; 33] = [
         "draw_total", "ei_draw_total", "dispatch_total", "ei_dispatch_total",            // 0-3 totals
         "ei_draw_skip_building", "ei_draw_skip_late", "ei_draw_skip_abandoned",          // 4-6
         "ei_draw_skip_quarantined", "ei_draw_skip_failed", "ei_draw_skip_other",         // 7-9
         "draw_standin_building", "draw_standin_late", "draw_standin_abandoned",          // 10-12
         "draw_standin_quarantined", "draw_standin_failed", "draw_skip_other", "draw_pc", // 13-16
         "dispatch_skip_building", "dispatch_skip_late", "dispatch_skip_image_only",      // 17-19
         "dispatch_skip_background", "dispatch_skip_size_gate", "dispatch_skip_abandoned",// 20-22
         "dispatch_skip_quarantined", "dispatch_skip_failed", "dispatch_skip_other",      // 23-25
         "ei_after_skipped_dispatch", "mustdraw_skipped", "mustdraw_wait_timeout",        // 26-28
         "mustdraw_wait_us", "cache_hit_layer", "cache_miss_layer", "reserved_0",         // 29-32
     ];
     static COUNTS: [AtomicU64; 33] = [const { AtomicU64::new(0) }; 33];
     ```
     `reserved_0` is never written (keeps the array size stable for Q4/Q5/Q8 reasons that map to
     existing names). Q1's `Unbuilt` gains variants in later units; their names
     (`abandoned`, `quarantined`, `background`, `size_gate`) already have slots here.
  2. `pub enum Call { Draw, Indirect(bool /* dispatches */), Dispatch }`.
  3. Pure `fn skip_index(call: Call, why: Option<&str>) -> usize`: draw kinds map `building`,
     `late`, `abandoned`, `quarantined`, `failed` (and `None` → failed) to the `draw_standin_*` or
     `ei_draw_skip_*` slot; dispatch kinds (direct or `Indirect(true)`) map every reason name to
     `dispatch_skip_<name>` and `None` to `dispatch_skip_failed`. `image_only`/`background`/
     `size_gate` on a draw kind map to the `_failed` slot (cannot happen; safe).
  4. `pub fn telemetry_skip(list: *mut c_void, call: Call, pso: usize)`: returns unless tier
     `frame`; `let s = crate::hle::d3d12x::pso_async::skipped(pso);` bump
     `skip_index(call, s.why.map(|w| w.name()))`; if `s.must_draw` bump `mustdraw_skipped`; if the
     call is a dispatch kind and the reason is not `image_only`, insert `list as usize` into
     `static SKIPPED_WRITER: Mutex<HashSet<usize>>` (skip when `list` is null).
  5. `pub fn telemetry_skip_other(list: *mut c_void, call: Call)`: bump `ei_draw_skip_other`,
     `draw_skip_other` or `dispatch_skip_other`; dispatch kinds insert into `SKIPPED_WRITER` too.
  6. `pub fn telemetry_pc_call(list: *mut c_void, call: Call, pso: usize)`: `skipped(pso)`; if
     `why` is Some → as `telemetry_skip`; else, for a draw kind, bump `draw_pc` (for
     `Indirect(true)` nothing).
  7. `pub fn telemetry_indirect_issued(list: *mut c_void, dispatches: bool)`: bump `ei_dispatch_total`
     + `dispatch_total`, or `ei_draw_total` + `draw_total`; if `SKIPPED_WRITER` contains `list`,
     bump `ei_after_skipped_dispatch`.
  8. `pub fn telemetry_dispatch_issued()`: bump `dispatch_total`.
  9. `pub fn telemetry_dispatch_dropped()`: bump `dispatch_total` and `dispatch_skip_other`.
  10. `telemetry_draw` (`telemetry_gpu.rs:2205`): before its tier-`pass` return, at tier `frame`
      bump `draw_total` (it runs once per direct draw, `bind.rs:559`).
  11. `telemetry_list_reset` (`telemetry_gpu.rs:2195`): remove `list` from `SKIPPED_WRITER`.
  12. Fill Q1's stub: `telemetry_first_use(ready)` bumps `cache_hit_layer` or `cache_miss_layer`.
  13. `pub fn telemetry_mustdraw_wait(took: std::time::Duration, timed_out: bool)`: add µs to
      `mustdraw_wait_us`; bump `mustdraw_wait_timeout` when `timed_out`. (No caller until Q7.)
  14. `pub fn counts_now() -> [u64; 33]` (relaxed loads).
  15. `Counters` gets `counts: [u64; 33]` from `counts_now()`; `FrameRec` gets `counts: [u64; 33]`
      (deltas, in `FrameBook::close`); `frame_facts` fills `FrameFacts.counts` with
      `frame_count_fields(&deltas)`.
  16. Pure `fn frame_count_fields(d: &[u64; 33]) -> Vec<(&'static str, u64)>`: indices 0-3
      always; 4-31 only when non-zero; `mustdraw_wait_us` is written as `mustdraw_wait_ms`
      (`d / 1000`, as f64 3 decimals, via a separate entry: see log.rs change); index 32 never.
  17. Fill `telemetry_pipeline_wait_expired(compute, ms)`: `telemetry_error(ErrKind::PipelineWaitExpired,
      "pso_async", "a compute pipeline was not built in time", None, false, &[("stage", ..), ("ms", ms.into()), ("n", n.into())])`
      with the volume rule of `io_failed` (first 100, then every 1,000th).
- **Changes in `log.rs` (appended part):** `FrameFacts` gets
  `pub counts: Vec<(&'static str, u64)>` and `pub mustdraw_wait_ms: f64`. `frame_fields` appends
  each `counts` pair as a number and `mustdraw_wait_ms` always (`ms_val`), before `sampled`. The
  test helper `frame()` gets `counts: vec![], mustdraw_wait_ms: 0.0`.
- **Changes in `telemetry_run.rs`:** `ErrKind::PipelineWaitExpired` (`pipeline_wait_expired`);
  `telemetry_activity` appends pure `activity_count_fields(now: &[u64; 33], prev: &[u64; 33])`:
  `draw_total`, `draw_no_output` (sum of indices 4-15), `dispatch_total`, `dispatch_skipped`
  (sum of 17-25), `cache_hit_layer`, `cache_miss_layer`, `mustdraw_wait_ms`; `prev` kept in a
  `static ACTIVITY_COUNTS: Mutex<[u64; 33]>`.
- **Call sites (one line each; add or change only these lines):**

  | File: function (line now) | Line |
  |---|---|
  | `bind.rs: execute_indirect`, after the put-off `if … { return … }` (after 282) | `crate::report::telemetry_indirect_issued(this, signature_dispatches(sig));` |
  | same, after `count(DRAWS_SKIPPED)` (295) | `crate::report::telemetry_skip(this, crate::report::Call::Indirect(signature_dispatches(sig)), raw_pso);` |
  | same, after `count(DRAWS_SKIPPED)` (307) | `crate::report::telemetry_skip_other(this, crate::report::Call::Indirect(false));` |
  | same, after `count(DISPATCHES_SKIPPED)` (324) | `crate::report::telemetry_skip(this, crate::report::Call::Indirect(true), raw_pso);` |
  | same, before `let rs = with(this, \|s\| s.rs_gfx)` (334) | `crate::report::telemetry_pc_call(this, crate::report::Call::Indirect(signature_dispatches(sig)), raw_pso);` |
  | same, before `return;` of the root-argument branch (343) | `crate::report::telemetry_skip_other(this, crate::report::Call::Indirect(signature_dispatches(sig)));` |
  | same, after `count(DISPATCHES_SKIPPED)` (354) | `crate::report::telemetry_skip_other(this, crate::report::Call::Indirect(true));` |
  | same, after `count(if gfx …)` in `Err` (366) | `crate::report::telemetry_skip_other(this, crate::report::Call::Indirect(!gfx));` |
  | `bind.rs: graphics_bound`, after `count(DRAWS_SKIPPED)` (568) | `crate::report::telemetry_skip_other(this, crate::report::Call::Draw);` |
  | same, after `count(DRAWS_PC)` (578) | `crate::report::telemetry_pc_call(this, crate::report::Call::Draw, pso);` |
  | same, after `count(DRAWS_SKIPPED)` (616) | `crate::report::telemetry_skip_other(this, crate::report::Call::Draw);` |
  | `bind.rs: dispatch`, after the put-off `if … { return … }` (after 896) | `crate::report::telemetry_dispatch_issued();` |
  | same, change `let pso = super::pso_async::resolve_wait(pso);` (897) | `let (raw, pso) = (pso, super::pso_async::resolve_wait(pso));` |
  | same, after `count(DISPATCHES_SKIPPED)` (905) | `crate::report::telemetry_skip(this, crate::report::Call::Dispatch, raw);` |
  | same, after `count(DISPATCHES_SKIPPED)` (911) | `crate::report::telemetry_skip_other(this, crate::report::Call::Dispatch);` |
  | `bind.rs: dispatch_dropped` (133) | `crate::report::telemetry_dispatch_dropped();` |
  | `console_list.rs`, after `at += 4;` in the dispatch branch (880) | `crate::report::telemetry_dispatch_issued();` |
  | same, before `continue;` after `out.skipped_dispatches += 1` (888) | `crate::report::telemetry_skip(list.as_raw(), crate::report::Call::Dispatch, (*p).pc);` |
  | same, `_ => out.skipped_dispatches += 1,` (924) | `_ => { out.skipped_dispatches += 1; crate::report::telemetry_skip(list.as_raw(), crate::report::Call::Dispatch, 0) }` |

  Put-off calls are counted once, at their replay (the replay goes through the same functions).
- **Tests:**
  - `telemetry_gpu`: `skip_index_follows_call_and_reason` (table over every `Call` × reason,
    including `None`).
  - `telemetry_gpu`: `frame_count_fields_keep_totals_and_drop_zero_skips` (totals 0 still
    written, a zero skip absent, a non-zero one present, `reserved_0` never).
  - `telemetry_gpu`: `frame_book_takes_count_deltas` (two `close` calls with `Counters` whose
    `counts` differ: the second frame's `counts` hold the differences).
  - `telemetry_gpu`: `a_skipped_dispatch_marks_its_list_until_reset` (with tier forced in the
    test helper as other tests do: `telemetry_skip(list A, Dispatch, 0)`, then
    `telemetry_indirect_issued(A, false)` bumps `ei_after_skipped_dispatch`;
    `telemetry_list_reset(A)`; the next does not). If the tier cannot be forced in unit tests,
    test the pure `fn writer_hit(set, list)` and `fn writer_mark(set, list, why)` instead.
  - `log`: `frame_fields_append_counts_flat` (a `FrameFacts` with two counts: both keys present,
    flat numbers; `mustdraw_wait_ms` present).
  - `telemetry_run`: `activity_count_fields_are_deltas_and_sums` (`draw_no_output` = sum of
    4-15, `dispatch_skipped` = sum of 17-25).
  - `telemetry_e2e`: extend `telemetry_frame_events_match_the_run` to assert every `frame` has
    `draw_total ≥ draws` and `dispatch_total ≥ dispatches`; extend
    `activity_events_mirror_the_log` to assert `draw_total`, `dispatch_total`, `dispatch_skipped`
    exist.
- **Acceptance:** tests pass. On the PC, a 120 s retail run at tier `frame`: frame lines carry the
  totals; in the world `draw_total` exceeds `draws`; the "console code so far" log line is
  unchanged.
- **Size:** about 230 production and 110 test lines.

#### Q3. Run summary and acceptance checker (`scripts/dev/`)

- **Inputs:** the field names of Q2 and Q9 (fixed in this document). Can be written before them;
  tests use synthetic files.
- **Outputs:** `scripts/dev/pso-summary.py`, `scripts/dev/test_pso_summary.py`.
- **Command:** `python scripts/dev/pso-summary.py <run-dir> [--scene-start S] [--config cold|warm|prewarmed]
  [--still-span A-B ...] [--check] [--out FILE] [--quiet]`. `--still-span` (seconds, repeatable)
  replaces the marked non-interactive windows, for a run without marks. Python 3.9+, standard library only, read with
  `encoding="utf-8", errors="replace"`; malformed lines are counted and skipped.
- **Definitions it uses:**
  - Times in seconds = event `t` / 1000.
  - **Scene start:** `--scene-start`; else the first `phase` event with `name` `world` and
    `source` `mark` or `schedule`; else the first `frame` with `draw_total` ≥ 400; else none
    (post-warmup checks are N/A). The summary records which rule was used.
  - **Post-warmup:** `t` ≥ scene start + 60 s (§13).
  - **Non-interactive windows:** spans between `phase` marks (`mark`/`schedule`) whose name maps to
    `Still` (Q6's table).
  - **No-output draws:** sum of `ei_draw_skip_*`, `draw_standin_*`, `draw_skip_other`.
  - **Direct draws:** `draw_total - ei_draw_total`.
  - **Dispatch skips:** sum of `dispatch_skip_*`.
- **`summary.json`** (schema 1): `run_id`, `adapter`, `config`, `scene_start_s` and its rule;
  `totals` and `post_warmup` objects with every count field summed; ratios
  (`ei_draw_skip_ratio`, `draw_standin_ratio`, `draw_no_output_ratio`, `dispatch_skip_ratio`);
  `dispatch_skips_per_frame` {p50, p95, max, buckets `0`, `1-9`, `10-99`, `100-999`, `1000+`}, total
  and post-warmup; `pipelines_per_minute` after the scene start by stage × class (from `pipeline`
  events); `compile_ms` p50/p95/max by stage × class × blob; `dxil_bytes` p50/p95/max by stage;
  `queue_wait_ms` (`queued_ms + gate_ms` of `pipeline_ready` with `need_ms` set) p50/p95/max by
  stage and must-draw; `worker_held_ms` max/p95 by class; `compile_timeouts` by kind × class;
  `quarantined_keys`; `abandoned` count; `cache_hit_layer_rate`; must-draw totals and the largest
  per-pipeline must-draw wait; `failed_builds` (report.json `d3d12.background_pipelines[1]`, and
  `pipeline_ready` with `outcome` `failed`); `unbuilt` count and the top 20 keys by skips;
  `malformed_lines`.
- **Table:** one line per summary group, human-readable.
- **`--check`:** prints one line per criterion: `AC<n> PASS|FAIL|N/A measured=… target=… (reason)`
  and the §12.2 rule as `RUN12 U7-SUFFICIENT|STRONGER-FORM|N/A`. Exit 0 when no line is FAIL, 1
  when one is, 2 on usage or unreadable input. The rules are in section 9's table.
- **Tests (`test_pso_summary.py`, `unittest`, synthetic run folders in a temp dir):**
  - `test_scene_start_rules` (override, mark, heavy frame, none).
  - `test_ratios_and_post_warmup_sums` (three frames before and two after the warm-up).
  - `test_dispatch_histogram_buckets`.
  - `test_check_ac1_cold_and_warm` (0.05 % passes cold; 1 skip post-warmup fails warm).
  - `test_check_ac7_available_workers` (an `activity` sample with 1 available fails).
  - `test_check_ac8_uses_marks_and_pipeline_ready` (a must-draw `pipeline_ready` with
    `waited_ms` 10,500 fails; `mustdraw_skipped` in a `Still` window fails).
  - `test_check_ac10_reads_report_json`.
  - `test_malformed_lines_are_counted`.
  - `test_exit_codes`.
- **Acceptance:** `python -m unittest scripts/dev/test_pso_summary.py` passes on the Mac and the
  PC; on the run 12 folder it runs without error and reports N/A for fields run 12 lacks.
- **Size:** about 330 lines of script and 180 of tests: over the limit, so two commits/PRs:
  **Q3a** summary and table (`summary.json`, about 200 + 90 lines; tests `test_scene_start_rules`,
  `test_ratios_and_post_warmup_sums`, `test_dispatch_histogram_buckets`,
  `test_malformed_lines_are_counted`) and **Q3b** `--check`, `--still-span` and exit codes (about
  130 + 90 lines; the other five tests). Q3b after Q3a; both in wave 1-2.

#### Q4. Size classes on the gate (`real.rs`, `pso_async.rs`)

- **Inputs:** Q1 committed.
- **Outputs:** `SizeClass`, `size_class`, the L share cap, XL to the giant lane and background,
  `large_held()`, `in_driver()`, graphics anomaly log.
- **Changes in `real.rs`:**
  1. `fn large_pso_bytes() -> usize` (`XCL_PSO_L_BYTES`, default `768 << 10`, via `env_count`,
     min `huge_pso_bytes()`); `fn xl_pso_bytes() -> usize` (`XCL_PSO_XL_BYTES`, default `2 << 20`,
     min `large_pso_bytes()`).
  2. ```rust
     #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
     pub(crate) enum SizeClass { S, M, L, Xl }
     impl SizeClass { pub(crate) fn name(self) -> &'static str { /* "S","M","L","XL" */ } }
     fn size_class_for(bytes: usize, huge: usize, large: usize, xl: usize) -> SizeClass
     pub(crate) fn size_class(bytes: usize) -> SizeClass  // with the three switches
     ```
  3. `compile_slots(bytes, graphics)` (`real.rs:4171`): giant when `bytes >= GIANT_DRIVER_COMPILE`,
     or when `!graphics && bytes >= xl_pso_bytes()`; else as today.
  4. `Slots` gets `large: AtomicUsize` (changed under `n`'s lock); every `Slots` literal (three
     statics and the test ones) gets `large: AtomicUsize::new(0)`.
  5. `fn large_cap(cap: usize, workers: usize) -> usize { cap.saturating_sub(1).max(1).min(workers.saturating_sub(2).max(1)) }`.
  6. `big_compile_turn(slots, need, commit_free, wanted, preload, large: bool)`: the admission
     test becomes `*n < cap && !gives_way && (!large || slots.large.load(SeqCst) < large_cap(cap, crate::gpu::recomp::dxc::compile_threads()))`;
     on admission, `if large { slots.large.fetch_add(1) }`. `SlotGuard` gets a fourth field
     `large: bool`; `Drop` does `large.fetch_sub(1)` when set. All existing callers pass `false`.
  7. `driver_compile_identified`: `let class = size_class(bytes); let graphics = !kind.contains("compute");`
     - `let large = !graphics && class == SizeClass::L;`
     - wanted: `if !graphics && class == SizeClass::Xl { never wanted } else { build_is_wanted }`
       (pass `|| false` or `super::pso_async::build_is_wanted`; the `impl Fn` parameter takes a
       boxed or two-arm closure: `let xl = …; move || !xl && super::pso_async::build_is_wanted()`).
     - Gauges: `static LARGE_HELD: AtomicUsize` (+1 from just before the gate to just after the
       driver call, when `class >= L` and the thread is `xcl-pso*`), `static IN_DRIVER: AtomicUsize`
       (+1 around `build()`), with guards so a panic decrements. `pub(crate) fn large_held() -> usize`,
       `pub(crate) fn in_driver() -> usize`.
     - Graphics anomaly: when `graphics && class >= SizeClass::L`, warn
       `"D3D12.X: a graphics pipeline of {bytes} B DXIL (class {}): expected S or M"` for the first
       8 and then powers of two.
- **Changes in `pso_async.rs`:**
  1. `Facts` gets `background: AtomicBool`. `note_size(bytes)` (Q1) also: if the pending is
     compute and `real::size_class(bytes) == Xl`, set `background` and `p.cv.notify_all()` under
     `p.done`'s lock (so a waiter re-checks).
  2. `Unbuilt::Background` (`background`), checked in `why_of` after `Failed` and before
     `ImageOnly`.
  3. `fn stop_waiting(p: &Pending) -> bool { p.facts.background.load(Acquire) }` (Q5 adds
     `abandoned`). Every `wait_timeout_while` / `wait` loop on `p.done` for a bind uses
     `!*d && !stop_waiting(p)` as the condition (`wait_in_frame`, both loops of
     `resolve_wait_inner`, `resolve_within`).
  4. `resolve_wait_inner` (`pso_async.rs:1560`): `if p.spared.load(Acquire) || p.facts.background.load(Acquire) { return resolve_spared(&p, pso); }`.
     In `resolve_spared`, the "writes only images" info line is logged only when `p.spared` is set.
  5. `pub(crate) fn large_known(p: &Pending) -> bool`: compute and
     `real::size_class(p.facts.dxil_bytes.load()) >= SizeClass::L` (bytes 0 → false). Used by Q7.
- **Tests:**
  - `real::big_compile_tests::size_classes_follow_the_thresholds`: `size_class_for` with 256 KiB /
    768 KiB / 2 MiB: 100,000 → S; 262,144 → M; 786,431 → M; 786,432 → L; 1,603,828 → L;
    2,077,556 → L; 2,097,152 → XL.
  - `large_builds_leave_a_slot_for_medium_ones`: a test `Slots { most: 3, wanted_most: 3 }`: two
    `large` turns admitted; a third `large` turn on another thread is not admitted within 150 ms;
    an M turn is admitted at once; dropping one L guard admits the waiting L.
  - `large_cap_keeps_a_slot_and_two_workers`: `large_cap(4, 15)` = 3; `(5, 15)` = 4; `(1, 15)` = 1;
    `(5, 3)` = 1; `(64, 6)` = 4.
  - `xl_compute_takes_the_giant_lane`: `compile_slots(2 << 20, false)` is `GIANT_COMPILES`;
    `compile_slots(2 << 20, true)` is `GRAPHICS_COMPILES`; `compile_slots((2 << 20) - 1, false)` is
    `BIG_COMPILES`; `(4 << 20, true)` is giant.
  - `pso_async`: `an_xl_program_is_not_waited_for_once_its_size_is_known`: lazy compute job that
    calls `note_size(3 << 20)`, then blocks 2 s; a test thread calls `resolve_wait` (test mode
    waits until built): it returns the stand-in within 300 ms of the job's `note_size`;
    `why_unbuilt` = Background.
- **Acceptance:** tests pass; existing `big_compile_tests` pass unchanged except the new field in
  literals. Retail run: no `pipeline` event with `class` L shows overlapping driver intervals
  above `large_cap` (Q3 reports the maximum concurrent L from `t - compile_ms` intervals).
- **Size:** about 150 production and 110 test lines.

#### Q5. Per-build timeouts, abandon, quarantine (`real/compile_watch.rs`, `real.rs`, `pso_async.rs`)

- **Inputs:** Q4 committed.
- **Outputs:** the `xcl-pso-watch` monitor; abandon and quarantine; replacement workers;
  `Unbuilt::Abandoned`, `Unbuilt::Quarantined`; outcome `quarantined`; `telemetry_compile_timeout`
  stub.
- **New file `hle/d3d12x/real/compile_watch.rs`** (declared in `real.rs` next to
  `pub(super) mod dispatch_chain;` at 4767 as `pub(super) mod compile_watch;`):
  ```rust
  //! Driver compiles past their time: a soft limit logs, a hard one abandons the build (its waiters
  //! go on, its slot's count is freed, its worker is replaced); an XL one also quarantines its key.
  pub(crate) struct Limits { pub soft: Duration, pub hard: Duration, pub quarantine: bool }
  pub(crate) fn limits(class: SizeClass, compute: bool) -> Limits   // table of 4.3
  pub(crate) fn hard_timeouts_on() -> bool                           // XCL_PSO_HARD_TIMEOUTS, default on, "0" off
  pub(super) struct SlotRelease { pub slots: &'static Slots, pub preload: bool, pub large: bool, pub released: Arc<AtomicBool> }
  pub(super) struct Watched(u64);                                    // Drop: remove; if abandoned, pso_async::worker_returned(is_worker)
  pub(super) fn enter(class, compute: bool, bytes: usize, record: Option<u64>, slot: Option<SlotRelease>) -> Watched
  #[derive(Debug, PartialEq)] enum Due { Soft(u64), Hard(u64) }
  fn due(entries: &mut BTreeMap<u64, Entry>, now: Instant, hard_on: bool) -> Vec<Due>   // pure on the map; marks them
  fn monitor()                                                       // thread "xcl-pso-watch", every 250 ms while entries exist (Condvar like OPEN_CV)
  ```
  `Entry { started, class, compute, bytes, record, build: Option<pso_async::BuildHandle>, is_worker: bool, soft_done, hard_done, slot: Option<SlotRelease> }`.
  - Soft: `xlog_info!` (first 32, then powers of two)
    `"D3D12.X: a {class} {kind} driver compile ({bytes} B, record …) has run {ms} ms"`;
    `crate::report::telemetry_compile_timeout(false, compute, class.name(), bytes, record, ms, false)`.
  - Hard (when `hard_timeouts_on()`): `xlog_warn!` (first 32) "… past its hard limit of {s} s: its
    calls go on without it and its worker is replaced{, its key is quarantined}"; release the slot
    count (`release_count`, below); `build.abandon(quarantine)`; `pso_async::worker_abandoned(is_worker)`;
    telemetry with `hard = true`, `quarantined`.
- **Changes in `real.rs`:**
  1. `SlotGuard` gets `released: Arc<AtomicBool>`. `fn release_count(slots: &Slots, preload: bool, large: bool, released: &AtomicBool)`:
     if `!released.swap(true)`: lock `n`, `preloads -= 1` if preload, `large -= 1` if large,
     `*n -= 1`, `cv.notify_all()`. `SlotGuard::drop` calls it, then subtracts its commit
     reservation (always, as today).
  2. `driver_compile_identified`: after the gate, build `SlotRelease` from the guard (when
     gated); right before `let started = …`:
     `let _watch = compile_watch::enter(class, !graphics, bytes, record.map(|r| r.0), slot_release);`
     dropped right after `build()` returns.
- **Changes in `pso_async.rs`:**
  1. `pub(crate) struct BuildHandle(Arc<Pending>)`; `pub(crate) fn current_build() -> Option<BuildHandle>`
     (clone of `BUILDING`); `pub(crate) fn is_worker() -> bool` (`IS_WORKER`).
  2. `impl BuildHandle { pub(crate) fn abandon(&self, quarantine: bool) }`: set `facts.abandoned`,
     `late`; under `p.done`'s lock `p.cv.notify_all()`; if `quarantine && key != 0`, insert into
     `static QUARANTINE: Mutex<Option<HashSet<u64>>>`.
  3. `static ABANDONED: AtomicUsize`; `pub(crate) fn worker_abandoned(is_worker: bool)`: if
     `is_worker`: `ABANDONED += 1`, then the spawn loop of `park()` with
     `extra_needed(w, PARKED + ABANDONED, n)`. Refactor: move `park`'s spawn loop into
     `fn top_up()`, called by `park` and `worker_abandoned`. `pub(crate) fn worker_returned(is_worker)`:
     `ABANDONED -= 1` when `is_worker`.
  4. `stop_waiting` also returns true when `facts.abandoned`.
  5. `Unbuilt::Abandoned` (`abandoned`) and `Unbuilt::Quarantined` (`quarantined`); `why_of`: not
     done and `abandoned` → Abandoned (before Late); done and outcome quarantined → Quarantined.
  6. `const OUTCOME_QUARANTINED: u8 = 3;` `static GATED: AtomicUsize`. In `build()` before running
     the job: if `p.key != 0` and the key is quarantined: set outcome, drop the job, mark done
     (as `drop_unstarted_job` does, plus `GATED += 1`), return. `complete()`: when `real == 0` and
     the outcome is not `FAILED`, count `GATED`, not `FAILED`.
  7. `pub fn gated() -> usize`. `QueueGauges` gets `pub abandoned: usize`.
  8. `BuildRecord.abandoned` from `facts.abandoned`.
- **Changes in `telemetry_gpu.rs`:** stub
  `pub fn telemetry_compile_timeout(_hard: bool, _compute: bool, _class: &str, _bytes: usize, _record: Option<u64>, _ms: u64, _quarantined: bool) {}`.
- **Tests:**
  - `compile_watch`: `limits_follow_class_and_kind` (the 4.3 table, eight rows).
  - `compile_watch`: `due_gives_soft_then_hard_once` (an L entry: at 59 s nothing; 61 s Soft;
    61.5 s nothing; 451 s Hard; 460 s nothing; with `hard_on` false only Soft).
  - `real::big_compile_tests`: `a_released_slot_count_is_given_back_once` (`most: 1`: take a
    guard; `release_count` through its flag; a second turn is admitted; dropping the first guard
    leaves `n` = 1, not 0, and no underflow).
  - `pso_async`: `an_abandoned_worker_is_replaced` (takes `WORKER_SATURATION_LOCK`): fill every
    general worker with compute jobs that call `worker_abandoned(true)` and block (5 s timeout);
    submit one more compute job; it runs within 2 s; release; `ABANDONED` returns to its value
    before.
  - `pso_async`: `an_abandoned_pipeline_is_not_waited_for`: a blocking lazy compute job; a test
    thread in `resolve_wait`; from another thread `BuildHandle(p).abandon(false)`; the wait returns
    the stand-in within 300 ms; `why_unbuilt` = Abandoned.
  - `pso_async`: `a_quarantined_key_is_not_built_again`: quarantine key K; `submit_lazy` with key K
    and a job that sets a flag; `resolve`; wait until done (2 s): the flag is not set;
    `why_unbuilt` = Quarantined; `gated()` grew by 1; `counts().1` (failed) did not.
- **Acceptance:** tests pass. Retail run: no `pipeline` compute L event with `compile_ms` above
  450,000 without a matching `compile_timeout` hard event; `activity.pso_workers_abandoned`
  returns to 0 after each abandoned compile returns.
- **Size:** about 230 production and 150 test lines.

#### Q6. Phase marks for the runtime (`telemetry_run.rs`)

- **Inputs:** Q2 committed (same file).
- **Outputs:** `phase_mark_class()`, `phase_class_of`, a mark-only poller.
- **Changes:**
  1. `pub const PHASE_UNKNOWN: u8 = 0; pub const PHASE_STILL: u8 = 1; pub const PHASE_PLAY: u8 = 2;`
  2. Pure `pub fn phase_class_of(name: &str) -> u8`: `world` → PLAY; `boot`, `intro`, `loading`,
     `menu`, `cutscene`, `splash`, `credits` → STILL; `auto` and anything else → UNKNOWN.
  3. `static MARK_CLASS: AtomicU8`; `fn set_mark(name: &str)` stores `phase_class_of(name)`.
  4. In `telemetry_poll_thread`: where `file.poll()` returns `Some(name)`, call `set_mark(&name)`
     (for `auto` too, which clears it); where a schedule entry is due, `set_mark(name)`.
  5. `pub fn phase_mark_class() -> u8`: a `static START: Once` that, when
     `!telemetry_on(Tier::Frame)`, spawns `xcl-phase-mark`: the same `PhaseFile` and schedule
     parsing as `telemetry_poll_thread`, every 250 ms, calling `set_mark` only (no events, no
     warnings beyond the existing ones). The logs folder comes from
     `crate::state::boot().map(|b| b.logs_dir())`; with none, only `XCL_PHASE_FILE` and the
     schedule are used. Then returns `MARK_CLASS.load(Relaxed)`.
- **Tests (`telemetry_p5c_tests`):** `phase_names_map_to_classes` (every name above, `auto`,
  `bench1`); `a_mark_sets_the_class_and_auto_clears_it` (`set_mark("intro")` → STILL,
  `set_mark("world")` → PLAY, `set_mark("auto")` → UNKNOWN).
- **Acceptance:** tests pass; on the PC with telemetry off, writing `intro` to the phase file is
  seen by `phase_mark_class()` (Q7's log line "must-draw: phase still" at the first must-draw
  wait proves it in a run).
- **Size:** about 70 production and 30 test lines.

#### Q7. Must-draw waits (`pso_async.rs`)

- **Inputs:** Q2, Q4, Q5, Q6 committed.
- **Outputs:** the policy of 3.3; the must-draw list; must-draw builds first in their lane.
- **Changes:**
  1. Switches, each read once (`OnceLock`), pure parsers for tests:
     `fn mustdraw_on() -> bool` (`XCL_MUSTDRAW`, on unless `0`);
     `fn mustdraw_chance() -> Duration` (`XCL_MUSTDRAW_WAIT_MS`, default 10,000);
     `fn mustdraw_frame_cap() -> Duration` (`XCL_MUSTDRAW_FRAME_MS`, default 10,000).
     `fn list_on() -> bool` (`XCL_MUSTDRAW_LIST`, on unless `0`).
  2. `mod must_list` (like `mod used`): file `must-draw-pipelines.txt` in
     `crate::gpu::recomp::dxc::cache_dir()`; `parse` (shared with `used::parse`), `known(key)`,
     `note(key)` (append when new, at most 4,096 keys, then silently stop); in-memory under
     `cfg!(test)`; does nothing when `!list_on()`.
  3. ```rust
     #[derive(Clone, Copy, PartialEq, Debug)] enum Phase { Unknown, Still, Play }
     fn phase() -> Phase  // from crate::report::phase_mark_class()
     #[derive(Clone, Copy, PartialEq, Debug)] enum Policy { Normal, Still, PlayListed }
     fn policy_for(on: bool, phase: Phase, must_draw: bool, listed: bool, large_known: bool) -> Policy
     ```
     `policy_for`: `!on || large_known` → Normal; `Still` and `must_draw` → Still; `Unknown` and
     `listed` → Still; `Play` and `listed` → PlayListed; else Normal. (`listed` = the key is in the
     list; `must_draw` = the flag.)
  4. `Facts` gets `listed: AtomicBool`. `submit_as`: if `mustdraw_on() && must_list::known(key)`:
     set `listed` and `must_draw`; when it goes to the preload list, `push_front` instead of
     `push_back`.
  5. `need_now` (first need, inside the `first` branch): if `mustdraw_on() && phase() == Phase::Still`
     and the pipeline is not `large_known`: set `must_draw`; if `key != 0` and not listed,
     `must_list::note(key)` and set `listed`. Push into the lane with `front = must_draw` (pool U1's
     `push(lane, item)` gets a `front: bool`; `true` uses `push_front`; every existing caller passes
     `false`).
  6. `static MUSTDRAW_FRAME_SPENT: Mutex<(u64, u64)>`.
  7. `wait_in_frame` becomes a wrapper over
     `fn wait_in_frame_with(p, pso, frame, budget, counter, cap, policy: Policy, chance: Duration, frame_cap: Duration) -> Option<bool>`,
     called with `policy_for(mustdraw_on(), phase(), must_draw, listed, large_known(p))`,
     `mustdraw_chance()`, `mustdraw_frame_cap()`. Behaviour:
     - `Normal`: exactly today's body.
     - `Still`: `need_now`; `late` → None; `chance_left(p, chance)` zero → set `late`, log (first
       32) `"D3D12.X: a must-draw {kind} pipeline was not built {ms} ms after it was first needed: drawn without it"`,
       `telemetry_mustdraw_wait(0, true)`, None; allowance =
       `frame_allowance(&MUSTDRAW_FRAME_SPENT, frame, frame_cap)` zero → None (not late); then
       `allowance.min(chance_left)`, `.min(cap)` when set; wait inside `Held` and `OpenWait` with
       condition `!*d && !stop_waiting(p) && !large_known(p)`; `frame_spend(&MUSTDRAW_FRAME_SPENT, …)`;
       `note_blocked`; `telemetry_mustdraw_wait(took, timed_out)` where `timed_out` = not done
       and the chance is now zero; set `late` only when `timed_out`. Return `Some(done)`.
     - `PlayListed`: today's body with the chance check removed and `late` never set by a timed-out
       wait; it spends the normal `counter`; `telemetry_mustdraw_wait(took, false)`.
     - The first `Still` wait of a run logs once: `"D3D12.X: must-draw: phase still: pipelines wait up to {chance} ms"`.
  8. `skipped` (Q1) already returns `must_draw`: no change.
- **Tests (`pso_async` test modules; use `wait_in_frame_with` / a `resolve_draw_with_policy`
  inner fn, no environment):**
  - `must_draw_policy_follows_phase_and_list`: every row of 3.3 through `policy_for`, plus
    `on = false` and `large_known = true`.
  - `a_must_draw_pipeline_waits_past_the_frame_budget`: graphics job sleeping 300 ms; policy
    Still, budget 50 ms, chance 2 s, frame cap 2 s: returns the real pipeline after about 300 ms;
    `late` false; the normal draw counter for that frame unchanged.
  - `a_must_draw_wait_ends_at_its_chance`: blocking job, chance 200 ms: returns the stand-in after
    about 200 ms, `late` true; the next call returns in under 15 ms.
  - `must_draw_waits_share_their_frame_cap`: two blocking jobs, frame cap 200 ms, chance 2 s:
    first waits about 200 ms; second returns in under 15 ms, not late; in the next frame the
    second waits again.
  - `a_listed_pipeline_waits_every_frame_in_play`: PlayListed, budget 50 ms, blocking job: frame
    1 waits about 50 ms, not late; frame 2 waits about 50 ms again.
  - `must_draw_builds_go_to_the_front_of_their_lane`: with the pool's lanes locked, push a wanted
    graphics item A, then a must-draw wanted graphics item B (`front = true`): the lane's front is
    B. (Test the lane deque directly under the lock; no worker runs.)
  - `a_large_compute_program_is_not_waited_for_as_must_draw`: compute job that calls
    `note_size(1 << 20)` after 100 ms and then blocks; Still: the wait returns within 300 ms.
  - `the_must_draw_list_is_noted_once`: `must_list::note(K)` twice → `known(K)`, one line in the
    in-memory list.
- **Acceptance:** tests pass, and `cargo test -p xcl --lib -- pso_async:: bind:: list_x::` and the
  whole suite. Retail run R1 (section 9): AC8's telemetry checks.
- **Size:** about 200 production and 170 test lines.

#### Q8. Size-gate skip switch (`pso_async.rs`, `real.rs`)

- **Inputs:** Q7 committed (same file).
- **Outputs:** `XCL_PSO_COMPUTE_SKIP_BYTES`; outcome `size_gate`; `Unbuilt::SizeGate`.
- **Changes:**
  1. `pso_async.rs`: `fn skip_bytes() -> Option<usize>` (`XCL_PSO_COMPUTE_SKIP_BYTES`, a whole
     number of bytes; unset, 0 or unparsable → None; read once). `fn size_gated_for(bytes, threshold: Option<usize>, building: bool) -> bool`
     (pure: `building && threshold.is_some_and(|t| bytes >= t)`).
     `pub(super) fn size_gated(bytes: usize) -> bool`: `size_gated_for(bytes, skip_bytes(), building_here())`;
     when true, sets the `BUILDING` pending's outcome `OUTCOME_SIZE_GATE = 2` and logs (first 16,
     then powers of two) `"D3D12.X: a compute program of {bytes} B DXIL is not built (XCL_PSO_COMPUTE_SKIP_BYTES={t}): its dispatches are left out ({n} so far)"`.
  2. `Unbuilt::SizeGate` (`size_gate`); `why_of`: done and outcome size gate → SizeGate.
     `complete()` counts it in `GATED` (Q5's rule already does: outcome not FAILED).
  3. `real.rs: translated_compute_pso_once`, right after Q1's `note_size` line:
     `if super::pso_async::size_gated(dxil.len()) { return Some(Err(())); }`
     (before the warm, chain and whole paths; `build_compute` then returns false, the job returns 0,
     `complete` sees the outcome).
- **Tests (`pso_async`):** `the_size_gate_is_off_unless_set_and_only_on_workers`
  (`size_gated_for` table: None; Some(786,432) with 786,431 and 786,432; `building` false);
  `a_size_gated_program_is_left_out_not_failed` (lazy compute job that calls `size_gated` via a
  test seam `size_gated_with(bytes, Some(1))` and returns 0: `why_unbuilt` = SizeGate, `gated()`
  +1, failed unchanged).
- **Acceptance:** tests pass. Diagnostic run (not an acceptance run):
  `XCL_PSO_COMPUTE_SKIP_BYTES=786432` gives `dispatch_skip_size_gate` > 0, `pipeline_ready` with
  `outcome` `size_gate` for every such key, and `manifest.compare_key` naming the switch (Q9).
- **Size:** about 50 production and 50 test lines.

#### Q9. Per-build record, gauges, timeout events, unbuilt at the end (telemetry files)

- **Inputs:** Q5 committed; Q4's `large_held`, `in_driver`, `size_class`; Q1's getters.
- **Outputs:** the `pipeline_ready` extension, `pipeline` fields, `compile_timeout`,
  `pipeline_unbuilt`, flat worker gauges, the diagnostic switch.
- **Changes in `telemetry_gpu.rs`:**
  1. `telemetry_pipeline_ready(r)`: write when `r.need.is_some() || r.outcome != "built" || r.skips > 0 || r.abandoned`.
     Fields from pure `fn pipeline_ready_fields(r: &BuildRecord, class: &str) -> Vec<(&'static str, Value)>`:
     U3's `stage`, `need_ms` (null when None), `late`, `ok`, plus `key` (16 hex), `class`,
     `dxil_bytes`, `queued_ms`, `held_ms` (null when None), `gate_ms`, `waited_ms`, `skips`,
     `must_draw`, `abandoned`, `outcome`. All ms with 3 decimals.
  2. `telemetry_pipeline_built`: add `class` (`size_class(dxil_bytes).name()`), `queued_ms`
     (`pso_async::queued_here()`, null when None) and `must_draw` (`must_draw_here()`) to the
     `pipeline` event. `log.rs`'s `PipelineFacts` is in the appended part: add the three fields
     there and in `pipeline_fields_built`. (This makes Q9 touch `log.rs` too; Q8 does not touch it.)
  3. Fill Q5's stub, with one more parameter `worker: bool` (the monitor thread writes the event,
     so the builder cannot be read from the thread name; Q5's entry keeps `is_worker`):
     `telemetry_compile_timeout(hard, compute, class, bytes, record, ms, quarantined, worker)`.
     Event `compile_timeout` (tier `frame`): `kind` (`soft`/`hard`), `stage`, `class`,
     `dxil_bytes`, `record` (16 hex or null), `driver_ms`, `quarantined`, `worker`. Q9 changes
     Q5's one call in `real/compile_watch.rs` to pass `entry.is_worker` (one line).
- **Changes in `telemetry_run.rs`:**
  1. `telemetry_activity` appends flat gauges: `pso_workers_busy` (`queue_gauges().busy`),
     `pso_workers_abandoned` (`.abandoned`), `pso_large_held` (`real::large_held()`),
     `pso_in_driver` (`real::in_driver()`), `pso_gated` (`pso_async::gated()`), from pure
     `fn activity_worker_fields(busy, abandoned, large_held, in_driver, gated)`.
  2. `write_end`: after `loop_counters_end()`, `pipelines_unbuilt_end()`: once (an `AtomicBool`),
     at tier `frame`, `for r in pso_async::unbuilt_records(2000)` write `pipeline_unbuilt` with
     `pipeline_ready_fields(&r, class)`.
  3. `diagnostic_switches`: add `XCL_PSO_COMPUTE_SKIP_BYTES` to the valued switches (listed when
     non-empty and not `0`).
- **Tests:**
  - `telemetry_gpu`: `pipeline_ready_fields_carry_the_build_record` (a hand-made record: every key,
    `need_ms` null when None, `key` 16 hex).
  - `telemetry_gpu`: `compile_timeout_fields` (pure fields fn).
  - `telemetry_run`: `activity_worker_fields_are_flat`; `unbuilt_lines_are_written_once`
    (call the once-latch twice with a counter sink, pure seam).
  - `telemetry_run`: extend the diagnostic test with `XCL_PSO_COMPUTE_SKIP_BYTES=786432` listed
    and `=0` not listed.
  - `log`: `pipeline_fields_built_carry_class_and_queue_time`.
  - `telemetry_e2e::activity_events_mirror_the_log`: the five new `activity` keys exist.
- **Acceptance:** tests pass; in the acceptance run `pipeline_ready`, `pipeline.class` and
  `pipeline_unbuilt` exist; `pso-summary.py` has no N/A for fields this unit provides.
- **Size:** about 150 production and 100 test lines.

#### Q10. Docs (`README.md`, `docs/guide/telemetry.md`)

- `README.md`, the huge-pipeline paragraph: two sentences on classes and timeouts; the switches
  `XCL_MUSTDRAW`, `XCL_MUSTDRAW_WAIT_MS`, `XCL_MUSTDRAW_FRAME_MS`, `XCL_MUSTDRAW_LIST`,
  `XCL_PSO_L_BYTES`, `XCL_PSO_XL_BYTES`, `XCL_PSO_HARD_TIMEOUTS`, `XCL_PSO_COMPUTE_SKIP_BYTES`;
  the phase file now also steers must-draw waits.
- `docs/guide/telemetry.md`: §5.3 `frame` new fields (rule "absent = 0" for skip fields); §5.5
  `pipeline` new fields, `pipeline_ready` (with pool U4's text), `pipeline_unbuilt`,
  `compile_timeout`; §5.8 `activity` fields, `error` kind `pipeline_wait_expired`; §5.9 marks
  steer the runtime; §6 `pso-summary.py`.
- PR note: `HANDOFF.md` or `docs/specs/` statements that every compute build in the background
  shares one lane, or that a build cannot be given up on, become wrong; say so, do not edit them.
- **Acceptance:** review only.

### 8. Concurrency plan

| Wave | Units | Files written | Why they can run together |
|---|---|---|---|
| 1 | Q1, Q3 | Q1: `pso_async.rs`, `real.rs` (2 lines), `telemetry_gpu.rs` (stub, U3 signature). Q3: `scripts/dev/*` | no shared file |
| 2 | Q2, Q4 | Q2: `telemetry_gpu.rs`, `telemetry_run.rs`, `log.rs`, `bind.rs`, `console_list.rs`. Q4: `real.rs`, `pso_async.rs` | no shared file |
| 3 | Q5, Q6 | Q5: `real/compile_watch.rs`, `real.rs`, `pso_async.rs`, `telemetry_gpu.rs` (stub). Q6: `telemetry_run.rs` | no shared file |
| 4 | Q7 | `pso_async.rs` | needs Q5, Q6 |
| 5 | Q8, Q9 | Q8: `pso_async.rs`, `real.rs`. Q9: `telemetry_gpu.rs`, `telemetry_run.rs`, `log.rs`, `tests/telemetry_e2e.rs`, one line in `real/compile_watch.rs` | no shared file (Q8 does not touch `compile_watch.rs`) |
| 6 | Q10 | `README.md`, `docs/guide/telemetry.md` | last |

Each runtime unit (Q1, Q2, Q4, Q5, Q7, Q8; Q6 and Q9 only touch fork files) needs a
`scripts/smoke.ps1` pair before any upstream PR (CONTRIBUTING.md item 3). These are fork units on
`dev`; the pair is still the regression check.

### 9. Acceptance runs on the retail title

**Build.** `dev` with pool U1-U4 and Q1-Q9, clean tree, release, on the PC (RTX 5080, driver
32.0.16.2140, 16 logical CPUs, 63 GB). Same input as run 12, then a static scene.

**R1 "blob-cold", marked.** Before the run, move aside XCL's driver-blob folder and the driver's
own shader cache; keep the DXIL cache (otherwise DXC alone takes hours; see deviations). No
`must-draw-pipelines.txt`.
- Switches: `XCL_TELEMETRY=pass XCL_TELEMETRY_SAMPLE=30 XCL_ACTIVITY=1 XCL_RUN_SECONDS=1200
  XCL_THREAD_DUMP_AT=300,600 XCL_UMA_MB=4095 XCL_STALL_DUMP=120`. No other `XCL_PSO_*`.
- Marks: the run script writes `intro` to `<logs>\phase` 2 s after launch (the content at start is
  ignored), and `world` when the HUD is on screen. Who decides "HUD on screen": a screenshot sent
  to the user (memory rule: label from a screenshot, do not guess). Screenshots every 10 s from
  launch to 300 s for AC8, then every 60 s.
- Duration 1,200 s: 13+ minutes after the scene start (O5 window).
**R2 "warm", unmarked.** Right after R1, same switches, no marks. It uses R1's
`must-draw-pipelines.txt` and driver blobs. Screenshots as R1.
**Evaluate:** `python scripts/dev/pso-summary.py <run> --check --config cold` (R1) and
`--config warm` (R2).

| AC (§13) | How the checker evaluates it | R1 (cold) target | R2 (warm) target |
|---|---|---|---|
| AC1 | sum `ei_draw_skip_*` / `ei_draw_total` | ≤ 0.1 % whole run | 0 post-warmup |
| AC2 | sum `draw_standin_*` / (`draw_total - ei_draw_total`) | ≤ 0.1 % | 0 post-warmup |
| AC3 | no-output draws / `draw_total` | ≤ 1 % whole run; ≤ 0.1 % post-warmup | same |
| AC4 | dispatch skips per frame measured (fields present → "measured" with p50/p95/max) | measured | 0 `dispatch_skip_*` except `image_only` post-warmup ("no culling pass skipped" proxy: none outside image-only) |
| AC5 | no PCS exists: proxy = every graphics `pipeline_ready` with `t - need_ms` in [scene, scene+120 s] has `t ≤ scene + 120 s`, and no graphics `pipeline_unbuilt` first needed in that window; P0/P1 = `pipeline_ready` with `need_ms` set: p95 of `queued_ms + gate_ms` ≤ 100 ms | as stated | as stated |
| AC6 | `activity.pso_q` lanes UG + UC + S (P2+P3) = 0 at some sample ≤ scene + 600 s; report the number of increases after that (allowed: new pipelines) | as stated | as stated |
| AC7 | (a) every `pipeline` compile_ms ≤ the hard limit of its class, or a `compile_timeout` hard event for it exists (abandoned workers are not counted as held); (b) every `activity` sample: `pso_workers.reserved + pso_workers.general - pso_workers.parked - pso_workers_abandoned - pso_large_held` ≥ 2 | both | both |
| AC8 | in `Still` windows: `mustdraw_skipped` = 0; every must-draw `pipeline_ready` with `waited_ms` ≤ 10,000; report the share ≤ 2,000; plus all intro cards seen in the screenshots (person checks) | marked windows (R1) | the same time span as R1's marks (R2 has none: pass `--still-span a-b` copied from R1) |
| AC10 | `report.json` `d3d12.background_pipelines[1]` = 0 and no `pipeline_ready` `outcome: "failed"` | 0 | 0 |
| §12.2 rule | U7-sufficient if: P0/P1 queue-wait p95 ≤ 100 ms, AC1 and AC2 post-warmup pass, and no 3 consecutive `activity` samples (≥ 5 s) with `pso_large_held ≥ pso_workers.general` | reported | reported |
| Safety | no device removal, no crash; `builds_title` 0; peak `committed_mb` ≤ 125 % of run 12's | as stated | as stated |

A run that crashes for an unrelated reason counts up to the crash and is marked partial. `--check`
lines that need a field the build lacks print N/A, never PASS. For R2, pass R1's marked spans
with Q3's `--still-span`.

### 10. Alternatives considered and dropped

- Adopting #2625 for cause counting (5.1).
- An automatic phase guess or latch for policy (3.4, 4.2 B3/B4); input or present cadence (B5);
  presented-target draws as must-draw (B6).
- A separate must-draw frame budget inside 100 ms in `Still` (would not have drawn the cards that
  went missing at 100 ms in runs 10-11).
- A dedicated L lane with `n/4` slots (4.3 C-a), and no L cap (C-c).
- Releasing an abandoned build's commit reservation with its slot (would admit compiles into memory
  the stuck one still uses).
- Discarding an abandoned build's late result (the pipeline is valid; throwing it away costs a
  rebuild).
- Size hints from earlier runs so reserved workers could take small compute (would cover the
  compute half of the P0/P1 reservation; deferred until the checker shows AC7 (b) or AC5 failing
  for compute).
- A runtime-written summary every 60 s (duplicates the script in Rust; `events.jsonl` already
  survives a killed run).

### 11. Deviations from the user's spec

| Spec | The spec says | This design does | Why |
|---|---|---|---|
| §7.2 | Four priorities P0-P3 with deadlines | P0 = must-draw at the front of the wanted lanes; P1 = wanted lanes (WG, WC); P2 = `resolve`-sent lanes (UG, UC) and preloads; P3 = UC, image-only (S), preloads | pool U1's lanes are the approved queue; no new queue |
| §7.2 | Priority aging: P3 promoted after 60 s | not built | it would break pool U2's invariant I4 (no image-only build while a wanted build waits); image-only dispatches are left out by design |
| §7.4 | XL > 1.6 MB | XL ≥ 2 MiB (`XCL_PSO_XL_BYTES`) | run 12 built a 2,077,556 B program normally (blob, 33 ms) and the observed slow programs reach 1,603,828 B; at 1.6 MB the boundary would make known-good programs XL |
| §7.4 | L lane: at most `max(1, floor(workers/4))`, ≤ workers - 2 | L shares the compute lane, at most `cap - 1` slots (3 unwanted / 4 wanted on the PC), ≤ workers - 2 | measured peak at 4 slots on a 16-thread RTX 5080 (#2098); memory envelope unchanged; the worker reason is met by the graphics reserve |
| §7.4 | XL: quarantine lane, at most 1, background only | the existing giant lane (1 slot), never wanted (always behind commit headroom), never waited for | reuses the lane; "background only" made concrete |
| §7.4 | Graphics L/XL logged | logged; lanes unchanged (graphics lane below 4 MiB) | run 12: three graphics L builds, 6-8 ms each |
| §7.4 | Hard invariant: ≥ 2 workers for P0/P1 | hard for graphics (pool U1 reserve, k = 3); for compute only U7's extra workers; measured by AC7 (b) | size is known only after pickup, so a reserved worker cannot safely take compute |
| §7.6 | S/M graphics 1 s / 10 s | 2 s / 60 s | run 12 cold graphics M builds took 14.0-21.3 s and succeeded |
| §7.6 | S/M compute 2 s / 20 s | 5 s / 120 s | run 12 cold compute S up to 22.6 s, M up to 48.0 s |
| §7.6 | L 60 s / 300 s | 60 s / 450 s | run 12 cold compute L up to 374.5 s (spec's 245 s is from run 10) |
| §7.6 | Timeouts per build | per driver compile, driver time only | the driver call is the uncancellable part; gate waits have their own rules |
| §7.6 | Abandon: replacement within cap; keep or discard | keep a valid late result always; free the slot's count, keep its commit reservation | see section 10 |
| §7.5 / §7.6 | Out-of-process helper preferred for L/XL | deferred | section 4.4 |
| §7.7 | `Submit/Query/WaitFor/SetLaneLimits/Drain`; every transition emits an event | mapped onto existing functions plus `why_unbuilt`; lane limits by switch; events only for terminal and exceptional transitions | event volume (56 k pipelines a run) |
| §8.3 | Direct draw: stand-in or fast generic variant | stand-in only; `direct_draw_generic_variant` not built | §6.3 is out of scope |
| §8.3 | Dependent compute passes must-draw | not detected; all compute is must-draw in `Still` (except known L/XL); the cascade is counted (`ei_after_skipped_dispatch`) | no cheap producer-consumer tracking; containment is the GPU-stall work (#1795, option R1) |
| §8.4 | Phase "detected" (by phase or capture tag) | phase from marks only (file, schedule); per-title list keyed by shader-set key, not `PipelineKey` | the guess misclassifies gameplay (3.4); `PipelineKey` (§3.4) is not built; the shader-set key is the fork's existing per-title key |
| §8.5 | Non-interactive: no 100 ms cap, 10 s per pipeline | as stated, plus at most 10 s of must-draw waits per frame | bounds a frame's hold when many must-draw pipelines arrive in one frame |
| §8.5 | Gameplay: 100 ms per frame for must-draw | listed pipelines wait every frame inside the existing 100 ms draw budget (no one-chance rule); no separate budget | keeps the frame cost of gameplay as today |
| §8.5 | Default when unknown | not specified; here: today's rules except listed keys | no reliable automatic phase |
| §9.3.1 | XL to the quarantine lane, never on the gameplay path | XL never waited for; nothing compiles on title threads already (stall-fix U4/U5) | - |
| §9.3.2 | `compute_skip_threshold_bytes`, default off, 768 KB diagnostic | `XCL_PSO_COMPUTE_SKIP_BYTES` in bytes, off, `diagnostic`; only on pool and preload workers | fork's switch naming; tools and `XCL_ASYNC_PSO=0` unaffected |
| §9.3.6 | Memory guard 4 GB per compile, kill and quarantine | admission only (XL never bypasses headroom; one XL at a time); no per-compile cap | needs the helper process (4.4) |
| §11.1 | Counter names | spec names kept where they are single values; reason-split counters become `<name>_<reason>` fields; `draw_no_output_ratio` derived in the summary | flat events (telemetry guide §5.3) |
| §11.1 | `queue_depth` sampled at 1 Hz | 2 s (`activity`) | one sampler |
| §11.1 | `psdb_*`, `driver_cache_hit_estimate`, `precache_*`, `direct_draw_generic_variant`, `slot_denied_requeue` | not built | their sections (§3, §5, §6, §7.5) are out of scope |
| §11.1 | `cache_hit_layer` = layer pipeline cache hit at bind | = the pipeline was built at its first bind (from any source: preload, blob, DXIL cache) | the fork's bind-time notion of "ready" (`pso_manifest` `ready`) |
| §11.1 | Per-scene breakdown | scenes = mark spans; per minute after the scene start, in the summary | no scene id in the runtime |
| §11.2 | Summary written by the runtime at exit and every 60 s | offline script, any time during or after a run | `events.jsonl` is durable line by line; no second implementation in Rust |
| §11.2 | GPU markers around skipped draws | deferred | 4.1 "What stays out" |
| §12.1 / §13 | Cold = layer and driver caches cleared | "blob-cold": driver blobs and the driver's cache cleared, DXIL cache kept | a full DXC rebuild takes hours; the driver compile is the measured cost (O4) |
| §12.2 | "every worker held by L/XL" | every **general** worker (`pso_large_held ≥ general`) | the reserve never takes compute |
| §12.3 | Nsight / RenderDoc confirmation of O6 | layer-side counts and per-key attribution only (`dispatch_skip_*`, `pipeline_ready.skips`, `pipeline_unbuilt`) | captures at 2 fps are not practical now; do them once frames are healthy |
| §12.5 | Tests: aging, slot-denied requeue | not written (features not built); synthetic stress uses ms-scale stand-ins for 60-300 s compiles | - |
| §13 AC5 | "full predicted (PCS) graphics set" | proxy: graphics pipelines first needed in the first 120 s are built by scene + 120 s | no PCS (§3 out of scope) |
| §13 AC7 | "No worker held longer than the hard timeout" | no worker **counted as available** is held past it; abandoned workers are replaced | an in-process driver call cannot be stopped |
| §13 AC8 | "All intro cards drawn" | telemetry proxy plus a person's screenshot check | telemetry cannot see a card |

### 12. What I could not check

- Nothing was built or run; the Mac cannot build the crate.
- Pool U1 is on `dev` (6c0f9b15) and matches the names used here. Pool U2 and U3 are not; U3's
  `telemetry_pipeline_ready` signature and U2's `gate_gauges` are taken from the approved entry.
- Pool U3, as approved, writes `pso_workers` and `pso_gate` on `activity` as nested JSON objects.
  The telemetry guide says events stay flat, with arrays of plain numbers only (§5.3,
  `docs/guide/telemetry.md:142-145`). This design adds only flat fields. The orchestrator should
  decide whether U3 flattens them (`pso_workers_general`, …); Q3's checker reads either form.
- Whether the cards' pipelines are cold graphics M builds (14-21 s) is unknown; AC8 on a
  blob-cold run depends on it.
- Whether #2625 merges into `dev` was not tried (not adopted anyway).

## 2026-11-26 Shader delivery spec v1, design 2: capture set, cache store, prewarm, precache, measurements, run configurations (the user asked for their spec to be incorporated; orchestrator approved the design; deviations reported to the user)

Same requirements document as design 1. This entry is the design for its sections 3, 4, 5, 6,
9.3 (items 3-5, as measurement), 10 (as measurement), 12.1 and AC5, AC6, AC9, AC11, drafted by an
Opus design worker on dev at 20941768 and reviewed by the orchestrator. Its units are named
U1-U13 inside this entry (they are not the U-numbers of the stall-fix entries); refer to them as
"capture U<n>". Section 10 lists every departure from the user's spec. The ASD export and PSDB
consumption of the spec's section 3 are not designed: the build and test PC has no network.

Status: draft for the orchestrator's review. Nothing here is built or run. Code citations are
`crates/xcl/src/...` on `dev` at 20941768. Upstream PRs were read as text only
(`gh pr view/diff -R xcl-dev/xcl`, 2026-11-26). The user's spec is "Spec - Shader Delivery &
Pipeline Compilation Optimization v1"; section numbers below are the spec's ("spec 3.3").
The one measurement in this file that is new was made with a helper script on existing run
folders (run 12 and run 13, RTX 5080): `scratchpad/spec-capture-tools/dxil_breakdown.py`
(`python3 -I dxil_breakdown.py <run folder>`). It prints aggregates only.

---

### Summary (recommendation first)

**Recommendation.** Build "ready before use" on what XCL already has. Do not add a second
store or a second key. Add four things, in this order:

1. **Measure first (small).** A `xcl cache --clear-driver` command and a fork-only run script
   give repeatable Cold, Warm and Prewarmed runs. Cold uses a fresh XCL cache folder (the user's
   caches are never deleted) and clears the driver's cache through the D3D12 API for `xcl.exe`
   only. A telemetry check proves each Cold run was cold. (U1, U12)
2. **Fix the store contract (small).** Driver blobs move under a folder named by the adapter and
   driver identity, and each blob file gets a checksum header. A driver change then leaves the
   DXIL and the capture untouched and starts a new blob folder. (U3)
3. **Capture what a title uses (medium).** Each pipeline a run builds is recorded once, with
   its full pipeline description and its console shaders, in the title's data folder
   (`%LOCALAPPDATA%\XCL\<title>\pipelines\capture.bin`). Each first use adds a small record:
   scene, time since scene start, frames shown. A new capture key (shaders plus state) and a
   pipeline key (capture key plus translator identity) appear on telemetry events. On by default;
   `XCL_CAPTURE=0` turns it off. (U4, U5)
4. **Use the capture twice (medium).**
   - **Prewarm runner:** `xcl prewarm <game>` replays the capture through the runtime's own
     pipeline builders, on threads the runtime's compile gate treats like its own workers. It
     fills the translated-shader cache, the DXC cache, XCL's driver blobs and the driver's own
     cache. It builds graphics too, because the capture holds the full state. It writes a
     prewarm manifest. (U6, U7, U8)
   - **Load-time precache:** the existing preload (`xcl-pso-pre-*`) takes its order from the
     capture (earliest needed first, per scene) instead of creation order, and counts per scene
     how each first use went: precached and ready, precached but late, missed, or served from a
     prior run's cache. (U9, U9b)

Measurements of DXIL size and compile cost come from existing telemetry plus two missing fields,
an offline report and one adopted upstream tool (#1830). No translator change. (U10, U11)

**Staged order (most readiness for the least code first).**

| Stage | Units | Changed lines (est.) | What it gives |
|---|---|---|---|
| 0 | adopt #2613 through the gate (orchestrator's decision) | 290 (upstream) | every VS/PS of the pack warmed into the translated cache offline: the DXC share (64 % of a cold graphics build on our runs) leaves the title's path for the common case |
| 1 | U1, U12a | 150 + 200 (script) | repeatable Cold / Warm / Prewarmed runs; nothing below can be judged without them |
| 2 | U3 | 220 | the spec 4.3 contract; safe reuse across driver updates |
| 3 | U4, U5 | 330 + 390 | the capture set (spec 3.3) and keys on telemetry |
| 4 | U9, U9b | 300 + 150 | precache ordered by the capture, the spec 6.2 validation counts, eviction detection |
| 5 | U6, U7, U8 | 220 + 330 + 330 | `xcl prewarm`, graphics included (spec 5.2) |
| 6 | U10, U11, U12b, U13 | 120 + scripts + docs | spec 9.3 / 10 measurements, AC evaluation, docs |

Why this order: on our PC a Warm run already makes most graphics pipelines from caches (run 13:
1,565 small graphics builds without an XCL blob took 0 ms at the median, which means the
driver's own cache served them). What Warm runs lack is order and the slow compute tail. The
capture plus precache fixes the order at runtime; prewarm moves the tail out of play. The first
launch on a machine (true Cold) needs prewarm, which comes last because it is the largest.

**Deferred (section "Deviations"):** ASD SODB export and PSDB consumption, crowdsourced caches,
idle-time compile, automatic prewarm at first launch and after a driver change, "fast variant
first", stage linking, NVIDIA Auto Shader Compilation detection, mesh/stream pipelines in the
capture, a per-compile memory guard.

---

### 1. Goal and scope

**Goal.** Most pipelines a title binds are ready, or buildable from caches in milliseconds, when
first bound (spec G2). Each run shows how far from that it is (spec G5, AC5, AC6, AC9, AC11).

**In scope (spec sections).**

| Spec | What this design does |
|---|---|
| 3.3 capture | records every built pipeline: keys, full description, shader references, translator identity and flags, first-use context, size class, measured build time |
| 3.4 pipeline key | defines capture key and pipeline key on top of the existing shader-set key |
| 4.2 local precompile | `xcl prewarm` with a blocking set (must-draw and scene-start) and a background set (L/XL compute only here) |
| 4.3 store key, invalidation | adapter and driver identity in the native blob folder; checksums; DXIL and capture kept across driver changes |
| 5.2 prewarm runner, 5.4 eviction | `xcl prewarm`, the prewarm manifest, an eviction counter at runtime |
| 6.2 load-time precache, validation | preload ordered by the capture; per-scene classification |
| 6.3 stage-level caching | exists (per-stage translated cache); recorded, no change |
| 9.3 items 3-5, 10, Q5, Q6 | measurement only: report and form timing |
| 12.1 run configurations | `xcl cache --clear-driver`, `scripts/dev/run-config.ps1` |
| AC5, AC6, AC9, AC11 | definitions computed from telemetry (`scripts/dev/ac-eval.py`) |

**Out of scope (explicitly).**
- Worker pool lanes, priorities, slots, per-build timeouts at runtime, must-draw waits (spec 7,
  8). Another design owns them (`scratchpad/worker-pool-spec.md` and the sibling design for spec
  7-8). This design only reads their results where an AC needs them.
- Translator code generation (spec 9.3 items 1, 2, 4 code changes; 10.2 control).
- ASD export, PSDB consumption, crowdsourced caches, idle compile (section "Deviations").
- Mesh and stream pipelines (`OwnedDesc::Stream`, `real.rs:5735`) in the capture (counted, not
  recorded).
- Ray-tracing state objects.

---

### 2. What exists today ("check the fork first" results)

#### 2.1 Keys

| Key | Where | Inputs | Notes |
|---|---|---|---|
| **K0 shader-set key** | `shaders_key`, `hle/d3d12x/real.rs:5740-5748`; computed at creation `real.rs:5988` | FNV-1a over `fnv64` of each console shader blob, in stage order | no pipeline state, no translator. One K0 covers every state variant of the same shaders |
| **K1 translated store** | `gpu/recomp/pso.rs:174-298` | folder: `identity()` `pso.rs:247-283` = translator source hash (`XCL_TRANSLATOR_SOURCE_ID`, build.rs:73) + DXC toolchain identity + backend switches; file: `fnv64(blob)_stage_samplerbase_windows_split.bin` `pso.rs:293-298` | header checks input length and a second hash `pso.rs:300-306`; output checked with `valid_dxil_container` `pso.rs:331`; written whole then renamed `pso.rs:372-379` |
| **K2 DXC cache** | `gpu/recomp/dxc.rs:682-705`, file `<key>.dxil` `dxc.rs:965` | XCL version, toolchain identity, profile, entry, HLSL text | structural check only `dxc.rs:848-856` |
| toolchain identity | `dxc.rs:647-680` | DLL names, sizes **and modification times** (`dxc.rs:668-676`) | makes K1 and K2 machine-dependent |
| **K3 driver blobs** | `driver_cache`, `real.rs:4841-4960` | compute: `fnv64(dxil)-len-flags.pso` (`real.rs:4867-4870`); graphics: `gfx-<graphics_blob_key>-<bytes>.pso` (`real.rs:4873-4876`) where `graphics_blob_key` (`real.rs:5213-5331`) hashes the DXIL of each stage, static samplers and every state field one by one | no adapter or driver identity, no checksum; kept only if the build took 250 ms or more (`KEEP`, `real.rs:4856`, `4953-4960`) and the DXIL is 16 KiB or more (`COMPUTE_BLOB_MIN_DXIL`, `real.rs:5048`, used `5169`, `5199`); a refused blob is deleted and the pipeline rebuilt (`real.rs:4810-4816`, `4900-4916`) |

**Does it satisfy spec 3.4 / 4.3?**

| Contract | K0 | K1 | K2 | K3 |
|---|---|---|---|---|
| stable across runs | yes | yes | yes | yes |
| stable across machines | yes | **no** (DLL mtime) | **no** (DLL mtime) | yes, given the same DXIL |
| changes with translator output | n/a (not a pipeline key) | yes (source hash) | yes (HLSL) | yes (content-addressed DXIL digests) |
| device identity only in the store key | yes | yes (the split flag is a translator flag) | yes | **no device identity at all** |
| a driver change invalidates native entries, keeps DXIL | n/a | kept | kept | works only through driver refusal: one failed create per entry, then delete; no record of which driver a blob belongs to |

There is no `ID3D12PipelineLibrary` store. The title's `CreatePipelineLibrary` is forwarded to
the PC (`real.rs:2883`, `3663`). `shader_cache_session.rs` is a fallback for the title's own
`ID3D12ShaderCacheSession` objects, not a pipeline cache.

**Important:** `dxc::cache_dir()` (`dxc.rs:698-705`, `%LOCALAPPDATA%\xcl\dxil`) is one folder for
all titles. `used-pipelines.txt` lives there too, so it mixes titles. The title's own data folder
is `%LOCALAPPDATA%\XCL\<title>\` (`launcher.rs:9-16`, `296`); its `cache\` is the loader's
(`config.rs:313-315`, `host.rs:197`).

#### 2.2 Recording modes

- `used-pipelines.txt` (`pso_async.rs:503-581`): K0 of each pipeline at its first use, appended,
  always on. Read at start to drive the preload.
- `XCL_PSO_MANIFEST=1` (`pso_manifest.rs:1-17`): per run, first uses (`use`, K0, ready or cold),
  driver builds and present milestones, in order. No descriptions; nothing is replayable from it.
- Telemetry (`docs/guide/telemetry.md` 5.5, 5.6, 5.9): `pipeline` build events
  (`telemetry_gpu.rs:2430-2453`, fields `log.rs:948-962`: stage, one record hash, `dxil_bytes`,
  `compile_ms`, `waited_ms`, `source`, `blob`, `by`, `wanted`); `pipeline_bind`
  (`telemetry_gpu.rs:2384-2401`: id, stages, record per stage); `shaders.jsonl` per program shape;
  `phase` events (`telemetry_run.rs:687-698`). The graphics `pipeline` event names only the PS
  record (`real.rs:5612`, `5637`). No event carries a pipeline key.
- No capture of full pipeline descriptions exists on `dev`. Upstream #758 proposes one (below).

#### 2.3 Warmup, precompile, cache controls

- `xcl shaders <pack> --precompile` (`shaders.rs:979-985`, `1593-1745`): translates and compiles
  VS, PS, MS and CS into the DXIL cache. VS/PS go through `recomp::translate` + `compile_cached`
  (`shaders.rs:1673-1705`), not through `console_to_dxil_in`, so the translated cache (K1) is not
  filled for them. Upstream #2613 fixes that (sampler base 0 only).
- `--pipelines` (`shaders.rs:1198-1224`, `make_one` from `1478`): compute only, on its own device,
  its own concurrency (`XCL_PIPELINE_THREADS`, 4) and its own cap (`XCL_PIPELINE_MAX_KB`, 8 MiB,
  `shaders.rs:1226-1231`). It does not use the runtime's gate. Writes K3 blobs.
- `--used-only` (`shaders.rs:1587-1591`): compute programs whose K0 is in `used-pipelines.txt`.
- `--measure-pso HEX` (`shaders.rs:1126-1185`): one compute record, cold, with
  `ShaderCacheControl(DISABLE)` on two implicit kinds (`shaders.rs:1139-1142`). Needs Developer
  Mode.
- Switches: `XCL_TRANSLATED_CACHE=0`, `XCL_DXIL_CACHE`, `XCL_PSO_DISK_CACHE=0`, `XCL_PSO_PRELOAD=0`,
  `XCL_LAZY_PSO=0`, `XCL_PSO_TRACE=1`. Several diagnostic switches bypass the translated cache
  entirely (`cache_allowed`, `pso.rs:922-935`, includes `XCL_DXIL_NO_OPT`).
- No CLI command clears any cache.

#### 2.4 Prediction and variants

- **Preload exists** (`pso_async.rs:583-641`). With lazy pipelines (`pso_async.rs:461-466`;
  `real.rs:5560-5562` returns before translating, so every translated pipeline goes through
  `go_background`, `real.rs:5955-6014`), a pipeline whose K0 an earlier run used is queued at
  creation (`pso_async.rs:495-499`) for 2-4 threads `xcl-pso-pre-*` (`pso_async.rs:605`) at the
  lowest OS priority (`608-611`), first in first out. The gate makes a preload give way to wanted
  builds (`real.rs:4242-4252` doc, `big_compile_turn` `4253`). This is the spec 6.2 "source 1"
  analogue, without scene order.
- Whoever takes a pipeline's job first builds it; the others find nothing (`pso_async.rs:404-416`,
  `624-627`). This is the spec's de-duplication.
- The title creates its pipelines with their full description at load (about 51,000 in 30 s,
  `pso_async.rs:456-460` doc). That is where XCL learns each pipeline's state.
- **Stage-level caching exists** (spec 6.3): per-stage translated entries (K1) and DXC entries
  (K2). A new state combination of cached shaders reruns only the driver.
- **No two-tier variant scheme.** A pipeline not ready gets a stand-in that draws nothing
  (`standin.rs`). The direct DXIL backend (`XCL_DIRECT_DXIL`) is an alternative output, not a fast
  first tier.

#### 2.5 Compile gate the prewarm runner must share

`driver_compile_identified` (`real.rs:4202-4230`) gates a build of `huge_pso_bytes()` (256 KiB,
`real.rs:4061-4069`) or more only on threads whose name starts with `xcl-pso`. Slots:
`BIG_COMPILES`, `GRAPHICS_COMPILES`, `GIANT_COMPILES` (4 MiB, one at a time)
(`real.rs:4166-4178`). Commit headroom per compile (`big_compile_headroom`, `real.rs:4301`).
`xcl shaders --precompile` lowers the memory reserve when no title runs
(`dxc::set_memory_reserve(2)`, `shaders.rs:1617`).

#### 2.6 Device facts already available

UMD driver version, VRAM and wave lane range: `device_facts`, `report.rs:427-455`. OS version:
`kernelx::host_os_version`, `hle/kernelx.rs:1708-1715`.

#### 2.7 Upstream PRs

| PR | State | Size | What | Decision |
|---|---|---|---|---|
| #757 | open, CONFLICTING | +42 -25 | PC desc of a translated VS/PS pipeline in its own function | not needed: U6 calls the runtime builders unchanged |
| #758 | open, CONFLICTING, stacked on #757 | +647 -25, `pso_journal.rs`, `real.rs` | journal of every pipeline queued to `pso_async` at creation, desc field by field, shaders written once, `U` mark at first build; 123 MB for 57,807 pipelines in a 60 s run | **not adopted** (conflicts, over 400 lines, no first-use context, records every created pipeline). Its format ideas are reused in U5 and credited |
| #759 | open, CONFLICTING, stacked on #758 | +1341 -31 | `xcl pipelines`: builds a journal's pipelines outside the title; own concurrency; VS/PS and compute only | **not adopted** (size, conflicts, not the runtime gate). Its lessons are used in U6/U8: memory windows come from the record; `.trying` marks |
| #898 | open, CONFLICTING, stacked on #758 | +638 -35 | journal mark at first bind instead of first build | idea used (U5 marks first use, not build) |
| #1006 | open, MERGEABLE | +1064 -12 | `ID3D12PipelineLibrary1` store plus unrelated descriptor staging | **not adopted**: a second store (spec 4.3 says extend the existing one), bundles two changes |
| #2001 | open, CONFLICTING | +2152 -132 | pipeline library plus many `pso_async` changes | **not adopted**: same reasons, overlaps the worker pool design |
| #2613 | open, MERGEABLE | +290 -19, `pso.rs`, `shaders.rs`, `shaders_e2e.rs` | `--precompile` warms the runtime's translated VS/PS variants (sampler base 0) | **adopt through the gate (stage 0)**: offline DXC removal for the common case, independent of everything here |
| #2577 | open, CONFLICTING | +75 -5 | no driver blobs on disk on AMD (blobs are 954-byte references; a stale one removed the device) | **not adopted**: uses a vendor ID check, which our rule forbids. Recorded as a cross-vendor risk (section 6) |
| #1830 | open, MERGEABLE | +273 -4, `shaders.rs`, `pso.rs` | `xcl shaders --time-forms`: four compute forms timed cold, driver cache off with all three implicit kinds; two processes in a row stayed cold on RTX 4090 | **adopt through the gate** for spec 9.3/10 measurement (U10 depends on it only for the form matrix) |
| #2438 | open, CONFLICTING | +424 -37 | `--measure-pso` for pixel shaders and direct DXIL | later; graphics form timing waits for it |
| #1849 | open, CONFLICTING | +277 -27 | NVIDIA application profile to turn the driver cache off | **not adopted**: vendor-specific |
| #1113, #2635 | open | small | `used-pipelines.txt` hygiene | unaffected; compatible |

---

### 3. Architecture and data flow after the change

```
                         title run (xcl run)                                         offline
 CreateGraphics/ComputePipelineState
      │ go_background (real.rs)
      │   K0 = shaders_key, CK = capture_key(desc, windows), PK = pipeline_key(CK, identity, flags)
      ├─► Pending{K0, CK, PK, predicted}
      │      predicted? ── capture index (U9) ─► preload heap (earliest first) ─► xcl-pso-pre-*
      │                    else K0 in used list ─► preload heap (last)
      │
      ├─ first use (note_use) ─► used-pipelines.txt (K0)           [unchanged]
      │                         ├─► capture U frame (CK, scene, ms in scene, frames)   (U5)
      │                         └─► telemetry pipeline_use (CK, PK, ready, predicted)  (U4)
      │
      └─ build (job on a worker) ─► translate (K1) ─► DXC (K2) ─► driver (+K3 blob from
                                      driver\<store_id>\, checksummed)           (U3)
                                 ├─► capture G/C frame once + S frames once + B frame  (U5)
                                 └─► telemetry pipeline {…, key: PK, capture: CK}       (U4)

 %LOCALAPPDATA%\XCL\<title>\pipelines\
     capture.bin  ◄──────────────────────── xcl prewarm <game> ──► reads capture, plans sets (U7)
     must-draw.txt (optional, hand-kept)          replays each record through the runtime
     prewarm-manifest.json ◄──────────────────── builders on xcl-pso-pw-* (U6, U8):
                                                  fills K1, K2, K3 and the driver's own cache
 runtime start: reads prewarm-manifest.json (U9b) → telemetry only (prewarmed flag, eviction)
```

**Runtime lookup order for one pipeline (unchanged in kind):** in-memory translated cache →
K1 file → translate (+K2 DXC cache → DXC) → in-memory blob → K3 file in the current store folder
→ driver. The capture and the prewarm manifest are never consulted to pick a pipeline. They only
order the preload and label telemetry. So the runtime is identical with or without a prewarm
(spec 5.2), apart from cache hit rates.

---

### 4. Decisions A-G

#### A. Pipeline key and cache store key

**Recommendation: keep K0-K3. Add two derived keys and one store folder. No new store.**

- **Capture key (CK)** = FNV-1a 64 of: the tag `"xcl-ck1"`, the kind (graphics or compute), K0,
  the pipeline state hashed field by field (the same field list and order as
  `graphics_blob_key` uses for state, `real.rs:5244-5328`, applied to the title's description;
  compute: `Flags`, `NodeMask`), and the memory windows (`pso::mem_windows()`, `pso.rs:82-89`).
  Never 0 (`| 1`). It is translator-independent and machine-independent. It is what the capture
  set merges and de-duplicates on.
- **Pipeline key (PK)** = FNV-1a 64 of: `"xcl-pk1"`, CK, the translated-cache identity
  (`pso.rs:247-283`), the compute split flag and the graphics split flag of the device. It
  changes whenever the translator output can change (spec 3.4). It names a pipeline in telemetry,
  in the capture's build records and in the prewarm manifest. It is not a storage key.
- **Native store key (spec 4.3 CacheStoreKey).** Blob files keep their content-addressed names
  (K3 already changes with the DXIL and the full state, which is stronger than a version bump).
  They move under `<DXIL cache>\driver\<store_id>\` where `store_id` = 16 hex of FNV-1a 64 over:
  adapter vendor id, device id, subsystem id, revision, UMD driver version, Windows major build
  number, and `BLOB_FORMAT = 1`. A driver update or another adapter gives another folder. DXIL
  (K1, K2) and the capture are untouched (spec 4.3 first bullet). Old flat files in `driver\` are
  ignored (a one-time loss of blobs; the driver's own cache still holds most). `xcl cache
  --prune-stores` deletes folders other than the current one.
- **Corruption.** New blob files start with a 24-byte header: magic `XCLBLOB1`, FNV-1a 64 of the
  payload, payload length. A load that fails the header, the length or the checksum deletes the
  file and returns none, so the pipeline is compiled (never handed to the driver). The capture
  file has a checksum on every frame (B). K1 and K2 keep their structural checks: the D3D12
  runtime refuses DXIL whose container hash is wrong, and the existing refusal path rebuilds. A
  payload checksum for K1/K2 would invalidate both caches once; it is a possible follow-up, not
  part of this design.
- **Not changed:** the DLL modification time in the toolchain identity (`dxc.rs:668-676`). The
  capture is the cross-machine artefact, and it does not depend on K1/K2. Fixing K1/K2 portability
  would invalidate every user's caches once for no v1 benefit (deviation D5).

Alternatives:
- (A2) Make PK the storage key of a new index file over K1/K2/K3. Rejected: a second key path
  that must agree with the content keys; drift serves a wrong pipeline.
- (A3) Leave the store as is and rely on driver refusal. Rejected: no record of the driver a blob
  belongs to (needed by the manifest and eviction detection), a failed create per entry after
  each driver update, and two GPUs on one machine delete each other's blobs.

#### B. The capture set

**Recommendation: an append-only binary file per title, on by default for pipelines a run
builds, with full descriptions and shader bytes stored once.**

- **Where:** `<data dir>\pipelines\capture.bin`, data dir as `xcl run` resolves it
  (`launcher.rs:296`), reached in the runtime through `crate::state::boot()` (`state.rs:14`).
  `XCL_CAPTURE_DIR` overrides the folder (the run script and tests use it). No boot (CLI, unit
  tests): no capture. Title-derived; never in the repository.
- **Switch:** `XCL_CAPTURE` unset or `used`: record pipelines when built; `0`: off; `all`: also
  record every pipeline at creation (spec's QA mode; about 123 MB per first 60 s, per #758's
  measurement).
- **Format** (little-endian). File header: 8 bytes `XCLPCS01`. Then frames:
  `tag u8, 0 u8 ×3, len u32, key u64, payload [len], check u64`, where `check` = FNV-1a 64 of the
  16 header bytes and the payload.

| Tag | key | payload | written |
|---|---|---|---|
| `S` shader | `fnv64(bytes)` | console shader bytes | once per key ever |
| `G` graphics pipeline | CK | `windows u32`; stage refs `vs, ps, gs, hs, ds` as `fnv64` (0 = none); every state field of `D3D12_GRAPHICS_PIPELINE_STATE_DESC` that `graphics_blob_key` hashes, each written explicitly (no struct copy, no padding); input layout as `count u32` then per element `name_len u16, name, index u32, format u32, slot u32, offset u32, class u32, step u32` | once per key ever |
| `C` compute pipeline | CK | `windows u32`, `cs ref u64`, `flags u32`, `node_mask u32` | once per key ever |
| `U` first use | CK | `run u64` (FNV of process start time and pid), `scene_len u8, scene`, `ordinal u16`, `ms_in_scene u32`, `ms_in_run u32`, `presents u64`, `ready u8`, `must_draw u8` (0 now; the spec 8 design may set it) | once per key per run |
| `B` build | CK | `run u64`, `pk u64`, `identity u64`, `dxil_total u32`, `driver_ms u32`, `blob u8`, `source u8` (0 unknown, 1 cache, 2 dxc, 3 direct), `by u8` (0 title, 1 worker, 2 extra, 3 preload, 4 prewarm), `outcome u8` (0 built, 1 refused) | once per key per run per PK |

- **What spec 3.3 asks for, and where it is:** key (CK in frames, PK in `B`); DXIL per stage
  (by reference: the K1 entry is found from the shader bytes plus identity; not copied, to keep
  the file small); root signature (XCL's ABI root signature, derived from the static samplers of
  the translation, so the translator identity plus shader sources determine it; the title's
  console root signature is replaced and not recorded); full description (`G`/`C`); translator
  version and flags (`identity`, windows, split flags inside PK); source shader identity (`S`
  keys); first-use context (`U`); size class (derived from `dxil_total`, below); measured build
  time (`B.driver_ms`, with `blob` telling cold from warm).
- **Size class** (spec 7.4 thresholds, Proposed there, kept): S < 256 KiB, M < 768 KiB,
  L < 1,638,400 B, XL at or above, of the pipeline's total DXIL. Defined once as `SizeClass` in
  `capture.rs` (U5) for prewarm, precache and reports.
- **Merge and de-duplication:** the writer reads the frame headers of the existing file on its
  own thread at open (as #758 does) and keeps a set of `(tag, key)` for S/G/C and of `(key, run)`
  for U. Frames noted while it opens wait in memory (at most 64 MiB, then dropped and counted).
  `xcl prewarm --merge <other capture.bin>` appends the frames of another file that are new by the
  same rules (spec 3.3 merge across runs and machines). `xcl prewarm --compact` keeps per key the
  newest 8 runs' U and B frames and rewrites the file (temporary file, then rename).
- **Torn or bad frames:** the reader stops at a frame whose length runs past the end (a torn
  tail; the writer truncates there before appending). A frame whose checksum fails is skipped.
- **Relation to existing records:** `used-pipelines.txt` stays written as today (upstream
  feature; `--used-only` reads it). Precache prefers the capture when it exists (U9). The `U`
  frame mirrors a new `pipeline_use` event; the `B` frame mirrors the `pipeline` event, which gains
  `key` and `capture` fields so run folders join to the capture.
- **Cost when on:** on the title's creation thread, one more FNV over about 300 bytes of state per
  created pipeline (K0 is already computed there). On the worker after a build: building the
  frames (a few hundred bytes plus shader bytes the first time) and a channel send. Disk writes on
  thread `xcl-capture`. After the first run of a title, only U and B frames are new (about 60 bytes
  each, a few thousand per run). Checked by the acceptance runs' `xcl-smoke` frames and peak commit
  (within 2 %).
- **Opt-in or always on:** always on for built pipelines (deviation D1). The local path needs a
  capture from ordinary play; the spec's opt-in mode is `XCL_CAPTURE=all`.

Alternatives:
- (B2) Adopt #758 (creation-time journal of every pipeline). Rejected for size, conflicts, no
  first-use context, and 123 MB of mostly never-used pipelines per title.
- (B3) JSON lines plus a shader folder. Rejected: thousands of small files, no per-record
  checksum, slower to index at runtime start.
- (B4) Merge by PK as the spec says. Rejected: PK changes with every translator source edit, so
  the file would grow a copy of every pipeline per XCL build (deviation D2).

#### C. The prewarm runner

**Recommendation (C1): a new subcommand `xcl prewarm` that replays the capture through the
runtime's own builders (`build_graphics`, `build_compute`, `real.rs:6020-6028`), on threads named
`xcl-pso-pw-{k}` so the runtime gate (`real.rs:4203`) applies to them unchanged.**

- **Why not extend `xcl shaders --precompile --pipelines`:** it is pack-driven and has no
  pipeline state, so it cannot build graphics; it uses its own concurrency and cap, not the
  runtime gate; and its compute root signature path is a second copy of the runtime's
  (`shaders.rs:1480-1497` against `real.rs:4006-4030`). Graphics needs the full description
  (blend, rasterizer, depth-stencil, input layout, render-target and depth formats, sample desc,
  topology, flags). The runtime has it in `OwnedDesc::Graphics` (`real.rs:5732-5736`,
  `own_graphics` `5802`); the capture's `G` frame gives it back. `--precompile --pipelines` stays
  as the pack-wide compute warmer for titles without a capture.
- **Steps:**
  1. Resolve the data dir from the game folder (`launcher::resolve_target`, `launcher.rs:253`,
     then `data_root().join(title)`).
  2. Read the capture (U5 reader), plan the sets (U7), write `prewarm-trying.txt` handling.
  3. Create the process device as the runtime does (`real::init`, `real.rs:476`; `XCL_ADAPTER`
     and the title's `xcl.toml` adapter apply). `dxc::set_memory_reserve(2)` as `--precompile` does.
  4. For each planned record, rebuild an `OwnedDesc` from the frames and call the runtime builder
     with the record's memory windows forced (`pso::with_mem_windows`, U6). The builder translates
     (fills K1/K2), builds through the gate and keeps K3 blobs as the runtime does. A prewarm build
     also keeps blobs of builds under 250 ms (`driver_cache::keep_always`, U6), so the next run
     gets a blob for every captured pipeline of 16 KiB DXIL or more.
  5. Write the manifest every 30 s and at the end.
- **Same lanes and caps as the runtime:** the gate's slots, commit headroom and the 4 MiB giant
  lane apply because the threads are `xcl-pso*`. A prewarm build is never "wanted" and never a
  preload, so it waits for commit headroom and does not get the preload give-way rule.
- **Timeouts (no hang on one build):** a watchdog thread checks running builds every second.
  Soft timeout: log `slow`. Hard timeout: the entry is marked `abandoned`, the worker is no longer
  counted, a replacement thread starts (at most `n` replacements), and the build's result is kept
  if it returns (spec 7.6 in-process wording). At the end the runner waits `--grace-s` (default 30)
  for abandoned builds, writes the manifest and exits the process, which ends any driver call
  still running. Table (Proposed; deviation D10):

| Class | Soft | Hard |
|---|---|---|
| S/M graphics | 1 s | 30 s |
| S/M compute | 2 s | 60 s |
| L compute | 60 s | 600 s |
| XL compute | 60 s | 900 s |

  Reason for longer hard timeouts than spec 7.6: run 12 measured M graphics cold builds up to
  21.3 s and L compute builds up to 374.5 s in the driver (helper script, 29 and 6 builds). A
  prewarm has no frame to protect, and an abandoned build keeps running anyway.
- **XL skip policy:** XL compute is planned only in the background set, at most one XL build at a
  time in the runner (in addition to the gate). Programs over `XCL_PIPELINE_MAX_KB` (8 MiB,
  `shaders.rs:1226-1231`) are skipped unless `--include-huge`, the same cap `--pipelines` uses.
- **Sets (spec 4.2):** the blocking set is the must-draw set plus the scene-start set; the
  background set is everything else. L/XL compute go to the background set unless must-draw. The
  CLI runs `--set blocking`, `--set background` or `--set all` (default `all`, blocking first).
- **Prewarm manifest** (spec 5.2 item 4): `<capture dir>\prewarm-manifest.json`, schema 1, with
  `xcl_version`, `translator_identity`, `split_compute`, `split_graphics`, `store_id`, adapter
  `{vendor, device, driver}`, start and end time, counts per outcome, and `entries`:
  `{capture, key, kind, class, set, outcome, ms, dxil_bytes}`. Outcomes: `built`, `reused`
  (a K3 blob was accepted), `refused`, `translation_failed`, `abandoned`, `built_late` (finished
  after its hard timeout), `skipped_cap`, `skipped_stream`, `replay_failed`, `crashed_before`.
- **Crash safety:** before a build, its CK is appended to `prewarm-trying.txt`; after it, removed
  (the file is rewritten every 30 s from memory). Keys found there at start are moved to the end of
  the background set and marked `crashed_before` unless `--retry-crashed` (idea from #759).
- **When it runs:** on demand in v1. Automatic runs at first launch and after a driver change
  (store id changed) are a later unit (U14, deviation D6).
- **Identical runtime:** prewarm changes caches only. The runtime reads the manifest for telemetry
  only (U9b).

Alternatives:
- (C2) Extend `shaders.rs` `Pipelines` with graphics. Rejected: duplicates the runtime's graphics
  description building (`real.rs:5570-5622`) and its keys; any drift makes prewarm fill keys the
  runtime never asks for.
- (C3) An in-title prewarm (`xcl run` holds the title while the preload builds). Rejected for v1:
  the title must run to create its pipelines; it reuses U6/U8's code later as U14.
- (C4) Adopt #759. Rejected (section 2.7).

#### D. Load-time precache and validation mode

**Recommendation: keep the existing preload as the precache lane; give it the capture's order and
more threads; classify every first use per scene.**

- **Prediction (spec 6.2 source 1):** at the first `submit_lazy`, the runtime reads the capture's
  index (U and G/C headers only, S payloads skipped by seeking) into a map CK → rank. Rank =
  (earliest scene ordinal of first use over the newest 8 runs, then the smallest `ms_in_scene`).
  A pipeline whose CK is in the map goes to the preload queue at that rank. A pipeline whose CK is
  unknown but whose K0 is in `used-pipelines.txt` goes in at the lowest rank (today's behaviour,
  the "same shaders, other state" tier, the closest analogue of spec 6.2 source 2). Others stay
  lazy.
- **Why prediction happens at creation, not at "scene load begins":** the title creates the
  pipelines of what it loads, with their full state, while it loads (section 2.4). Queueing at
  creation is the earliest moment XCL can build them. Building all created pipelines
  speculatively is what lazy mode exists to avoid (commit; `pso_async.rs:456-460`).
- **Priority below frame-critical (P2):** unchanged mechanism. Preload threads are separate from
  the pool, at the lowest OS priority, and give way at the gate. The worker pool design's lanes do
  not change this.
- **Queue:** the `VecDeque` (`pso_async.rs:588`) becomes a `BinaryHeap` ordered by rank, then by
  arrival.
- **Threads:** `XCL_PSO_PRELOAD_THREADS`, default `clamp(compile_threads / 2, 2, 8)` (7 on the
  16-thread PC; today `clamp(n / 4, 2, 4)` = 3). Proposed. Reason: AC5 asks for a few hundred
  graphics pipelines within 120 s on Cold; at 1.5-5 s each cold, 300 × 3 s / 3 threads = 300 s, /
  7 threads = 130 s. On Warm they take about 13 ms each. To be validated by the Cold run.
- **De-duplication:** unchanged (whoever takes the job builds; section 2.4). Cached entries build
  in milliseconds from K1/K2/K3.
- **Validation classification (spec 6.2):** each pipeline's first use gets one class, counted per
  scene:
  - `precache_hit`: predicted (CK or K0 tier) and ready at first use;
  - `precache_late`: predicted, not ready at first use;
  - `prior_cache`: not predicted; its build was served from caches (`source` = cache and either a
    blob was supplied or the driver took under 250 ms);
  - `missed`: not predicted; its build needed DXC/translation or a cold driver compile, or it was
    never built before the run ended.
  The first two are decided at first use; the last two when the build completes. The counts go to a
  `precache` event at each scene change (for the scene that ends) and at the end, and to one
  `xcl.log` line at the end. Always computed when telemetry tier `pass` is on (deviation D14).
- **Load completion held for must-draw (spec 6.2, 10 s cap):** belongs to the spec 8.5 design.
  Not here.
- **Fast variant first (spec 6.3):** deferred. Any D3D12 graphics variant is a full driver PSO
  build; for driver-bound pipelines a second variant doubles driver work. The only "fast" output
  XCL has is the direct DXIL backend, whose NVIDIA driver cost is unmeasured
  (freeze-analysis Q4) and whose VS/PS output differs from the HLSL path's. Revisit after U10
  measures direct against HLSL for VS/PS (#2438).

#### E. Measurements (spec 9.3 items 3-5, 10, Q5, Q6)

**Recommendation: one offline report over a run folder from existing telemetry plus two new
fields; controlled form timing through adopted #1830 plus a records filter and a DXC level option.**

- **Already measurable today** (prototype run on run 12, cold builds without an XCL blob):

| Stage | Class | Form (backend/split/windows) | n | DXIL KB p50 | driver ms p50 | ms max | DXIL instr per console instr p50 |
|---|---|---|---|---|---|---|---|
| compute | L | hlsl/whole/switch | 6 | 1,215 | 227,576 | 374,461 | 9.0 |
| compute | M | hlsl/whole/switch | 4 | 302 | 36,964 | 48,016 | 10.6 |
| compute | S | hlsl/split/switch | 237 | 15 | 1 | 14,996 | 12.0 |
| compute | S | hlsl/whole/switch | 81 | 20 | 1 | 22,581 | 9.8 |
| graphics | M | hlsl/whole/switch | 29 | 306 | 13,967 | 21,253 | 8.4 |
| graphics | S | hlsl/whole/switch | 1,425 | 15 | 0 | 8,897 | 5.3 |

  Cold builds over 200 ms joined to a shape: compute n = 40, correlation of driver ms with DXIL
  bytes 0.93 and with switch count 0.93; graphics n = 38, 0.87 and 0.71. Every L program was a
  whole (not split) form. Split forms appear only in small programs here. This supports the
  existing finding that DXIL size follows console program size and is not mainly split inflation
  (spec 10.2 hypothesis not supported on this data; one run, correlation only).
- **Missing fields (U10):** the `shaders.jsonl` `seen` line gets `hlsl_bytes` and `dxc_opt`
  (`O3`, `O1` or `O0`, from the same size rule as `dxc.rs:628-640`), so the DXC level of every
  program is known. The `pipeline` event gets `key`/`capture` (U4) so builds join to the capture.
- **Form matrix (controlled):** adopt #1830 (`--time-forms`, compute: per-lane, per-instruction,
  HLSL switch windows, HLSL heap windows; driver cache off) and add `--records FILE` (a list of
  record hashes from the report) so the slowest programs of a run are timed in every form. Add
  `--dxc-opt 0|1|3|auto` to `--measure-pso` (passes `-O0`/`-O1`/`-O3`; `compile_whole` honours an
  explicit level, `dxc.rs:629`) for spec 9.3 item 3. Split against whole: `--split 0|1` exists in
  both tools. Direct backend (item 5): the per-instruction and per-lane columns of #1830.
- **Q5 (wave sizes):** already answered by every run's `manifest.json` (`wave_lane_count_min/max`,
  `report.rs:434-438`). No work.
- **Q6:** split against whole, optimisation level, heap against switch windows and direct
  against HLSL are measurable as above. The EXEC-emulation share is not measurable without a
  translator switch that removes EXEC masking; none exists, and adding one is a translator change
  (deviation D20).
- **The report (U11, fork-only script):** `scripts/dev/pipeline-report.py <run folder>` writes
  `pipeline-report.csv` (one row per cold build: stage, class, form, dxil bytes, console
  instructions, DXIL instructions, switches, DXC level, DXC ms, driver ms, blob) into the run
  folder (never the repository) and prints the aggregate table above, per-class p50/p95/max and the
  correlations.

#### F. Run configurations (spec 12.1)

**Recommendation: isolate XCL's caches per series with `XCL_DXIL_CACHE`, clear the driver cache
through the D3D12 API for `xcl.exe` only, and prove coldness from telemetry.**

- **Our folders.** A series is `<RunRoot>\<series id>\` with `dxil\` (passed as
  `XCL_DXIL_CACHE`; holds `translated\v2\`, `*.dxil`, `driver\<store_id>\`, `used-pipelines.txt`,
  `pso-manifest.txt`) and `capture\` (passed as `XCL_CAPTURE_DIR`; a copy of the title's
  `pipelines\` folder taken at series start, so every config of a series starts from the same
  capture). The user's normal caches and capture are never deleted.
- **The driver's cache.** `xcl cache --clear-driver` (U1) creates a device on the run's adapter and
  calls `ID3D12Device9::ShaderCacheControl` with `D3D12_SHADER_CACHE_CONTROL_FLAG_CLEAR` on the
  three implicit kinds (driver-managed, D3D cache for the driver, D3D conversions). That clears
  the caches of the current application, which is `xcl.exe` (the title runs in an `xcl.exe` host
  process, `launcher.rs:456-460`). It needs Windows Developer Mode; without it the command exits
  with code 3 and says so. No driver cache folder is located or deleted: vendor folders such as
  `%LOCALAPPDATA%\NVIDIA\DXCache`, `%LOCALAPPDATA%\D3DSCache` and AMD's are shared by every
  application and are never touched.
- **Fallback when Developer Mode is off:** the script copies `xcl.exe` to
  `xcl-s<series id>.exe` in the same folder (so DLL lookup is unchanged) and runs the series
  through that copy, which is a new application for the driver. It deletes the copy at the end of
  the series. The driver's entries for the copy are left to its own eviction.
- **Validity check (both methods):** a Cold run is valid only if, among its graphics `pipeline`
  events with `blob: false` and `dxil_bytes` of 64 KiB or more, at most 5 % took under 5 ms
  (Proposed). Run 13 shows why: small graphics builds without an XCL blob took 0 ms at the median,
  so the driver's cache was serving them. `ac-eval.py` marks a run that fails as `not cold`.
- **Configs:**

| Config | Steps |
|---|---|
| Cold | new series folder; copy the capture; `xcl cache --clear-driver` (or new exe copy); run |
| Warm | the same series folder, the same exe; run again |
| Prewarmed | new series folder; copy the capture; clear (or new copy); `xcl prewarm <game> --set all` with the same environment; run |

- **The scripted path:** `XCL_RUN_SECONDS=900` (13+ min, spec 12.1), `XCL_TELEMETRY=pass`,
  `XCL_ACTIVITY=1`, `XCL_NETWORK=0`, `XCL_PHASE_MARK` from `-Schedule`, and `-InputScript <path>`
  started with the run (our input automation lives outside the repository). Every run in a series
  gets the same environment block, written to `<series>\env.txt`.
- **AC9 is computed from** `pipeline_use` (first use per PK, `ready`) joined to the first
  `pipeline` event with the same `key`. Definitions in section 9.

#### G. What to defer or change in the spec

| Spec item | Recommendation | Reason | What is lost |
|---|---|---|---|
| 3.3 export to SODB | defer | no Microsoft tool on an offline PC; eligibility open (Q1). The capture keeps full descriptions and shader references, so an exporter can be written later without changing it | cloud-precompiled pipelines |
| 3.3 consume PSDB | defer | no PSDB exists for this title or configuration; offline policy | the "near-100 % hit" tier |
| 4.1 B crowdsourced caches | defer (spec agrees) | needs distribution and trust infrastructure | cross-machine sharing |
| 4.1 D idle precompile | defer | needs idle detection; risk of competing with play. `xcl prewarm --set background` on demand and the lowest-priority preload cover most of it | no-wait completion without a user step |
| 4.2 automatic first-launch / driver-change prewarm | later unit U14 | measure on-demand prewarm first; the run path should not block until its cost is known | the user must run `xcl prewarm` |
| 5.3 detect NVIDIA Auto Shader Compilation | drop | vendor-specific; our rule is capability checks only. The driver version is logged and `driver_cache_hit_estimate` counts fast builds | knowing whether the feature was on |
| 6.2 source 2 permutations | reduce to the K0 tier | speculative permutations cost commit (lazy mode exists for that) | pipelines of new state combinations are not predicted |
| 6.3 fast variant first | defer | doubles driver work for driver-bound pipelines; the fast output is unmeasured | a draw-something-early option |
| 6.3 Q4 stage linking | out | no D3D12 equivalent to evaluate offline | none now |
| 9.3 item 6 / 7.5 memory guard | later unit U15 (prewarm helper process in a job object) | in-process compiles cannot be capped | the per-compile 4 GB part of AC11 |

---

### 5. Dependencies

#### 5.1 Upstream PRs (through `python scripts/dev/merge-pr.py <N>` only; the orchestrator decides)

- **#2613** (stage 0): independent of every unit. Touches `pso.rs` and `shaders.rs`; land it
  before U6 and U10 so they rebase on it.
- **#1830**: needed by U10's `--records` option only. Land before U10.
- Not adopted: #757, #758, #759, #898, #1006, #2001, #2577, #1849 (section 2.7).

#### 5.2 Other designs

- **Worker pool design** (`scratchpad/worker-pool-spec.md` U1-U3) edits `pso_async.rs` heavily
  (lanes replace `queue()`). U4, U5 and U9 also edit `pso_async.rs` (`Pending`, `submit_as`,
  `note_use`, `preload`). Land the worker pool U1 and U2 first; U4/U5/U9 rebase on them. Their U3
  adds queue gauges to `activity`; U9 adds `preload_queue` to the same gauge, so U9 comes after
  their U3. AC5's P0/P1 wait part reads their `pipeline_ready` event.
- **Spec 7-8 design** (timeouts, quarantine, must-draw): `SizeClass` is defined in U5's
  `capture.rs`; that design should reuse it rather than define a second one (orchestrator to
  confirm). When it adds a runtime must-draw flag, the `U` frame's `must_draw` byte carries it. The
  prewarm timeout table should become the runtime's once that design lands.

#### 5.3 Order between units

```
U1 ──────────────────────────────┐
U3 ──────────────┐               │
U4 ──► U5 ──┬──► U9 ──► U9b      │
            ├──► U6 ──┐          │
            └──► U7 ──┴──► U8 ◄──┘ (main.rs after U1)
U10 (after #1830)     U11 (any time)    U12a (after U1)   U12b (after U4, U9)   U13 (last)
```

---

### 6. Constraints and considerations

- **Disk.** Capture: bounded by the shader bytes of the pipelines a title builds; #758 measured
  123 MB for every created pipeline of a 60 s run (57,807 pipelines, 5,659 shaders), so the
  built-only capture is smaller. Driver blobs: one folder per store id; `xcl cache --status`
  prints the size of each cache part, and the acceptance runs record them. Old store folders stay
  until `--prune-stores`.
- **Time.** Prewarm on Cold, estimated from our measurements: a few hundred graphics at 1.5-5 s on
  15 threads, about 1-2 min; L compute about 13 programs × 225 s at 4 slots, about 12 min; within
  the spec's Appendix A boot warmup range (10-25 min). Measured by the Prewarmed run.
- **Memory per compile.** The gate's commit headroom applies to prewarm threads. The README's
  measurement: about 0.34 GiB per additional huge build; up to about 1.5 GB for the biggest
  (`shaders.rs:1216-1217` comment). The prewarm process also reserves the UMA address range, as the
  runtime does, because `shader::readable` creates it (`gpu/shader.rs:215`, `uma.rs:850`); this is
  address space plus the coherent window, measured by the run.
- **Privacy and the repository rule.** The capture, its summary, the manifest, run folders and
  the report CSV hold title-derived data or hashes of it. They stay under the title's data folder,
  the series folder or the run folder. Unit tests use synthetic shader bytes only.
- **Driver cache eviction.** The driver's own cache has a size limit outside XCL's control. U9b
  counts `driver_cache_evicted`: a pipeline the manifest says was prewarmed that builds with
  `blob: false` in 250 ms or more. The README recommends a large driver cache setting (docs only).
- **Cache-key mistakes that would serve a wrong pipeline.** None of the new keys selects a
  pipeline at runtime: the runtime always builds from the title's own description, and only the
  content-keyed caches (K1 input bytes plus identity, K2 HLSL, K3 DXIL plus full state) feed a
  build. A CK collision (64-bit) can only make prewarm build an extra pipeline. A prewarm run under
  other switches than the title run produces other K1 keys: misses, not wrong pipelines; the
  runtime logs `prewarm made for another configuration` when the manifest's identity or split flags
  differ (U9b). Existing gap, not widened: K3 graphics keys omit the root signature, which is
  derived from the static samplers that are in the key.
- **Failure modes.** Capture write error: capture off for the run, one warning. Corrupt capture
  frame: skipped. Corrupt blob: deleted, rebuilt. Prewarm crash: partial manifest (written every
  30 s) and `prewarm-trying.txt`. A build that never returns: `abandoned`, process exits at the end.
  Developer Mode off: fresh exe copy.
- **Cross-vendor.** AMD blobs are references into the driver's own cache, and a stale one removed
  the device (#2577). The store id folder and checksum do not prevent that. The cross-vendor config
  of spec 12.1 must not be run with XCL blobs on until a vendor-neutral fix exists (for example,
  writing a blob only after its pipeline has been used without a device removal). Open risk.
- **Open risks.** (1) Whether the driver's per-application cache identity is the image path of
  `xcl.exe` (assumed for both the clear and the exe-copy fallback; checked by the validity rule).
  (2) Whether CLEAR needs Developer Mode like DISABLE does (assumed). (3) More preload threads
  compete with the title's own threads on Cold; measured by AC5 and frame times. (4) Prewarm and
  run must share switches; the script passes one environment to both.

---

### 7. Work breakdown

Every unit: one logical change, under about 400 changed lines, tests in the file's
`#[cfg(test)] mod tests` unless stated, no `cargo fmt`, GPU tests call
`gpu::recomp::runner::skip_test` when the hardware is missing. Runtime units need the smoke pair
(CONTRIBUTING.md). Line numbers are at 20941768; rebase on the worker pool units where noted.
There is no U2: the run script it would have been is U12a.

#### U1. `xcl cache`: status, clear the driver's cache, prune stores

- **Inputs:** dev. **Outputs:** a CLI subcommand.
- **Files:** new `crates/xcl/src/cache_cmd.rs`; `crates/xcl/src/lib.rs` (+1 `pub mod cache_cmd;`);
  `crates/xcl/src/main.rs` (+1 arm next to `"shaders"`, `main.rs:77`: `Some("cache") =>
  xcl::cache_cmd::run(&args[1..])`), plus the usage text where `xcl --help` lists subcommands.
- **Changes:**
  - `pub fn run(args: &[String]) -> i32`: parses `--status`, `--clear-driver`, `--prune-stores`
    with `crate::cli::Parser` (as `shaders.rs:867-895`). No option: prints usage, returns 2.
  - `fn status() -> i32`: prints `dxc::cache_dir()` and the byte totals of `translated\`, `*.dxil`
    files, `driver\` (each subfolder with its name), `used-pipelines.txt`. Pure helper
    `fn folder_bytes(dir: &Path) -> u64` (recursive, ignores errors).
  - `fn clear_driver() -> i32`: picks the adapter as `shaders.rs` `Pipelines::new` does
    (`crate::gpu::adapter::candidates`), creates a device with
    `crate::gpu::agility::create_device`, casts to `ID3D12Device9`, calls
    `ShaderCacheControl(D3D12_SHADER_CACHE_KIND_FLAG_IMPLICIT_DRIVER_MANAGED |
    D3D12_SHADER_CACHE_KIND_FLAG_IMPLICIT_D3D_CACHE_FOR_DRIVER |
    D3D12_SHADER_CACHE_KIND_FLAG_IMPLICIT_D3D_CONVERSIONS, D3D12_SHADER_CACHE_CONTROL_FLAG_CLEAR)`.
    Success: prints `driver shader caches of this application cleared (<adapter>)`, returns 0.
    Failure: prints the HRESULT and `Windows Developer Mode is required`, returns 3. No device: 2.
  - `fn prune_stores() -> i32`: until U3 lands, prints `no store folders yet` and returns 0; U3
    fills it (U3 lists the change).
- **Acceptance:** `xcl cache --status` prints the sizes; `xcl cache --clear-driver` returns 0 with
  Developer Mode on (PC check, written in the PR's Tested section).
- **Tests:** `cache_args_are_parsed_and_unknown_ones_refused` (each option, an unknown option → 2);
  `folder_bytes_sums_nested_files` (temp folder with two files in two levels);
  `clear_driver_returns_0_or_a_developer_mode_refusal` (GPU; skip without a GPU; asserts the code
  is 0 or 3, never a panic).

#### U3. Native store key and checksummed driver blobs (spec 4.3)

- **Inputs:** dev. **Outputs:** blobs under `driver\<store_id>\` with a header.
- **Files:** `crates/xcl/src/hle/d3d12x/real.rs` (module `driver_cache`, `4841-4960`, and its
  callers' paths only through `path_for`/`graphics_path`); `crates/xcl/src/gpu/adapter.rs`
  (new fn); `crates/xcl/src/report.rs` (`device_facts` calls the new fn for the driver version);
  `crates/xcl/src/cache_cmd.rs` (`prune_stores`, if U1 landed; else this item moves to U1's follow-up).
- **Changes:**
  - `gpu/adapter.rs`: `pub struct AdapterIdentity { pub vendor: u32, pub device: u32, pub subsys:
    u32, pub revision: u32, pub umd: u64 }` and `pub fn identity_of(dev: &ID3D12Device) ->
    Option<AdapterIdentity>` (LUID → `EnumAdapterByLuid` → `GetDesc1` → `CheckInterfaceSupport`, the
    code of `report.rs:440-452` moved here). `report.rs` `device_facts` formats `umd` from it.
  - `driver_cache`: `const BLOB_FORMAT: u32 = 1; const MAGIC: &[u8; 8] = b"XCLBLOB1";`
    `pub fn store_id_of(id: &AdapterIdentity, os_build: u32) -> u64` (pure; FNV-1a 64 of the six
    fields and `BLOB_FORMAT`, `| 1`). `static STORE: OnceLock<Option<u64>>`;
    `pub fn set_device(dev: &ID3D12Device)` sets it once from `identity_of` and
    `kernelx::host_os_version().2`; called from `real::init` after the device is made and from
    `shaders.rs` `Pipelines::new`. `fn dir() -> Option<PathBuf>` = `cache_dir()\driver\<16 hex>`;
    none until `set_device` ran (then nothing is read or written: no blob is better than a blob
    of an unknown device).
  - `path_for` and `graphics_path` use `dir()`. `name_for`/`graphics_name` unchanged.
  - `pub fn wrap(payload: &[u8]) -> Vec<u8>` (header + payload) and
    `pub fn unwrap(file: &[u8]) -> Option<&[u8]>` (pure). `write` writes `wrap(bytes)`; `load`
    reads, calls `unwrap`; on `None` deletes the file, counts `CORRUPT`, logs once, returns `None`.
  - `pub fn keep_always(p: &Path, blob: &[u8])`: writes regardless of `KEEP` (U6 uses it).
  - `cache_cmd::prune_stores`: deletes `driver\<x>\` folders whose name is not the current store
    id (computed with a device as in `clear_driver`) and old flat `*.pso` files in `driver\`; prints
    what it deleted and the bytes freed.
- **Acceptance:** a run writes blobs only under `driver\<store_id>\`; a second run reuses them
  (`xcl.log` "made from the driver's blob"); a corrupted file is deleted and the pipeline built.
- **Tests:** `store_id_changes_with_each_identity_field_and_not_otherwise`;
  `wrap_then_unwrap_returns_the_payload`; `unwrap_refuses_a_flipped_byte_a_short_file_and_a_wrong_magic`;
  `load_deletes_a_corrupt_blob_file` (temp dir); `no_store_dir_before_set_device`;
  existing `driver_cache` tests adjusted only where they built paths by hand (explicit paths keep
  working because `write`/`load` are path-based).

#### U4. Capture key, pipeline key, first-use context on telemetry

- **Inputs:** dev after the worker pool U1/U2. **Outputs:** CK and PK on `Pending`; `pipeline`
  events carry `key` and `capture`; a new `pipeline_use` event; a scene clock.
- **Files:** `real.rs` (state hasher, `capture_key`, `pipeline_key`, `go_background`);
  `pso_async.rs` (`Pending` fields, `submit_lazy`/`submit_as` signature, `note_use`);
  `gpu/recomp/pso.rs` (`pub fn translator_identity() -> u64` returning `translated::identity()`);
  `telemetry_gpu.rs` (thread-local build keys, `pipeline_use`); `log.rs` (fields);
  `telemetry_run.rs` (scene clock).
- **Changes:**
  - `real.rs`: split the state part of `graphics_blob_key` (`5244-5328`) into
    `fn feed_graphics_state(h: &mut u64, d: &D3D12_GRAPHICS_PIPELINE_STATE_DESC)`; `graphics_blob_key`
    calls it at the same point, so its output is unchanged. Add
    `pub(crate) fn capture_key(kind: u8, k0: u64, state: impl FnOnce(&mut u64), windows: u32) -> u64`
    and `impl OwnedDesc { fn capture_key(&self) -> u64 }` (graphics: `feed_graphics_state` on the
    owned desc; compute: `Flags`, `NodeMask`; stream: 0).
    `pub(crate) fn pipeline_key(ck: u64, identity: u64, split_compute: bool, split_graphics: bool) -> u64`.
    In `go_background` after `let key = owned.shaders_key();` (`real.rs:5988`): compute `ck` and
    `pk` (device flags from `pso::wave32_only_for_device` and `graphics_wave32(this)`); pass both to
    `submit_lazy`/`submit`; wrap the job so it runs inside
    `telemetry_gpu::with_build_keys(ck, pk, || build(...))`.
  - `pso_async.rs`: `Pending` gains `capture: u64, pkey: u64`; `submit_lazy(standin, compute, job,
    key, capture, pkey)`; `submit(standin, compute, job)` passes zeros; test call sites updated.
    `note_use` calls `crate::report::telemetry_pipeline_use(self.capture, self.pkey, self.compute,
    ready, false)` after the manifest line, only when `capture != 0`.
  - `telemetry_gpu.rs`: `thread_local! static BUILD_KEYS: Cell<(u64, u64)>`;
    `pub fn with_build_keys<R>(ck: u64, pk: u64, f: impl FnOnce() -> R) -> R` (sets, runs, restores);
    `telemetry_pipeline_built` adds `key` and `capture` (16 hex, or null when 0) to its facts.
    `pub fn telemetry_pipeline_use(ck, pk, compute, ready, predicted)` writes `pipeline_use`
    (tier pass): `capture`, `key`, `stage`, `ready`, `predicted`, `scene`, `ordinal`,
    `ms_in_scene`, `presents`.
  - `log.rs`: `PipelineFacts` gains `key: Option<u64>, capture: Option<u64>`;
    `pipeline_fields_built` emits them.
  - `telemetry_run.rs`: `static SCENE: Mutex<(String, u16, Instant)>` initialised to
    `("run", 1, first call)`; `write_phase` (`687-698`) sets it to `(name, count of that name so
    far, now)`; `pub fn scene_now() -> (String, u16, u32)` returns name, ordinal and ms since the
    scene began.
- **Acceptance:** a telemetry run shows `pipeline_use` events and `pipeline` events with `key`;
  every `pipeline_use.key` that was built has a `pipeline` event with the same `key`.
- **Tests:** `graphics_blob_key_is_unchanged_by_the_state_split` (golden value computed from the
  pre-change function on a fixed desc, pasted as a constant in the test);
  `capture_key_follows_shaders_state_and_windows_only` (same desc twice equal; change one blend
  field, one RTV format, the windows, a shader byte: each differs; struct padding garbage: equal);
  `pipeline_key_changes_with_identity_and_split_flags`; `scene_ordinal_counts_each_name`
  (`write_phase` twice with `world`, once with `menu`); `pipeline_use_fields_are_complete`
  (fields builder, pure); `with_build_keys_restores_the_previous_keys`.

#### U5. The capture writer and reader

- **Inputs:** U4. **Outputs:** `capture.bin`; a reader API for U7, U9.
- **Files:** new `crates/xcl/src/hle/d3d12x/capture.rs`; `hle/d3d12x/mod.rs` (+1 line);
  `real.rs` (job wrapper in `go_background`; `OwnedDesc::capture_record`); `pso_async.rs`
  (`note_use` → `capture::first_use`); `telemetry_gpu.rs` (`take_last_build`).
- **Changes in `capture.rs`:**
  - `pub enum SizeClass { S, M, L, Xl }`, `pub fn size_class(dxil_total: usize) -> SizeClass` with
    the thresholds of section 4.B.
  - Frame encode/decode (pure): `pub fn frame(tag: u8, key: u64, payload: &[u8]) -> Vec<u8>`;
    `pub fn for_each_frame(bytes: &[u8], f: impl FnMut(u8, u64, &[u8])) -> usize` (returns the end
    of the last whole frame; skips bad checksums; stops at a torn tail).
  - Records: `pub struct GraphicsRec { windows, vs, ps, gs, hs, ds: u64, state: Vec<u8>, layout:
    Vec<Element> }` with `encode`/`decode`; `pub struct ComputeRec { windows, cs, flags, node_mask }`;
    `pub struct UseRec { run, scene, ordinal, ms_in_scene, ms_in_run, presents, ready, must_draw }`;
    `pub struct BuildRec { run, pk, identity, dxil_total, driver_ms, blob, source, by, outcome }`.
    The `state` bytes are produced in `real.rs` by `fn graphics_state_bytes(d) -> Vec<u8>` writing
    the same fields `feed_graphics_state` reads, in the same order, and read back by
    `fn apply_graphics_state(bytes, &mut desc)` (both in `real.rs`, U5).
  - Writer: `fn mode() -> Mode` (`Off`, `Used`, `All` from `XCL_CAPTURE`); `fn dir() ->
    Option<PathBuf>` (`XCL_CAPTURE_DIR`, else `state::boot()?.data_dir.join("pipelines")`);
    `static W: OnceLock<Writer>` with an `mpsc::Sender<Vec<u8>>` and thread `xcl-capture` that
    opens the file (header check; truncate a torn tail; read existing `(tag, key)` sets), then
    appends. Public calls: `pub fn built(ck: u64, rec: Record, shaders: &[&[u8]], b: BuildRec)`,
    `pub fn created(ck: u64, rec: Record, shaders: &[&[u8]])` (mode `All`),
    `pub fn first_use(ck: u64, compute: bool, ready: bool)` (builds `UseRec` from
    `telemetry_run::scene_now()`, `present::presents()` and process time). De-duplication: S/G/C by
    key in a `HashSet` held by the writer thread; U by `(key, run)`. Memory cap 64 MiB of queued
    frames, then drop and count. A write error turns the writer off with one warning.
  - Reader: `pub struct Capture { pub pipelines: HashMap<u64, Record>, pub uses: HashMap<u64,
    Vec<UseRec>>, pub builds: HashMap<u64, Vec<BuildRec>>, shader_at: HashMap<u64, (u64, u32)> }`;
    `pub fn read(path: &Path) -> Result<Capture, String>` (reads all; shader payload offsets
    recorded); `pub fn shader(&self, file: &mut File, key: u64) -> Option<Vec<u8>>` (seek + read +
    verify fnv64); `pub fn read_index(path) -> Result<Capture, String>` (skips S payloads by seeking).
- **Changes in `real.rs`:** `impl OwnedDesc { fn capture_record(&self) -> Option<(capture::Record,
  Vec<&[u8]>)> }` (graphics: state bytes, layout from the owned `D3D12_INPUT_ELEMENT_DESC` and
  `CString`s, stage refs; compute: CS; stream: `None`). In `go_background`'s job wrapper (U4), after
  a successful build: `capture::built(ck, rec, shaders, BuildRec { .. from
  telemetry_gpu::take_last_build() .. })`. With mode `All`, `capture::created` at creation.
- **`telemetry_gpu.rs`:** `thread_local! static LAST_BUILD: Cell<Option<(u32 dxil, u32 ms, bool
  blob, u8 source)>>`, set in `telemetry_pipeline_built` (also when telemetry is off: move the
  set before the early return); `pub fn take_last_build()`.
- **Acceptance:** a run writes `capture.bin`; a second run adds only U and B frames (file growth
  under 1 MB for a run that builds no new pipeline).
- **Tests:** `frames_round_trip_and_a_flipped_byte_is_skipped`;
  `a_torn_tail_is_cut_and_earlier_frames_stay`; `graphics_record_round_trips_every_state_field`
  (`graphics_state_bytes` → `apply_graphics_state` gives a desc whose `feed_graphics_state` hash is
  equal); `shaders_and_pipelines_are_written_once_uses_once_per_run` (writer on a temp dir, two
  notes of the same pipeline, two runs); `size_class_thresholds`; `read_index_skips_shader_bytes`;
  `capture_is_off_without_a_boot_or_with_xcl_capture_0`.

#### U6. Replaying a capture record through the runtime builders

- **Inputs:** U5 (and U3 for `keep_always`). **Outputs:** `real::replay_build`.
- **Files:** `real.rs`; `gpu/recomp/pso.rs`.
- **Changes:**
  - `pso.rs`: `thread_local! static WINDOWS_OVERRIDE: Cell<Option<u32>>`;
    `pub fn with_mem_windows<R>(w: u32, f: impl FnOnce() -> R) -> R`; `mem_windows()` (`82-89`)
    returns the override when set (the `HEAP_WINDOWS` bit inside `w` is kept as recorded).
  - `real.rs`: `pub(crate) struct ReplayOutcome { pub ok: bool, pub reused: bool, pub dxil_total:
    usize, pub driver_ms: u32, pub refused: bool, pub translation_failed: bool }`.
    `unsafe fn owned_from_record(rec: &capture::Record, shader: &dyn Fn(u64) -> Option<Vec<u8>>) ->
    Option<OwnedDesc>`: builds the console-side description with owned shader bytes, input layout
    and names, `pRootSignature` null, `CachedPSO` empty, `apply_graphics_state`.
    `pub(crate) unsafe fn replay_build(rec: &capture::Record, shader: &dyn Fn(u64) ->
    Option<Vec<u8>>) -> ReplayOutcome`: `owned_from_record`, then with the record's windows forced,
    `build_graphics`/`build_compute` with `Dev(device_raw() as usize, orig_fn(device,
    slot_of(device, "CreateGraphicsPipelineState" | "CreateComputePipelineState")) as usize)`;
    releases the built pipeline; reads `telemetry_gpu::take_last_build()` for the outcome.
  - Thread-local `PREWARM_KEEP_ALL: Cell<bool>` set by `replay_build`; `driver_cache::keep` callers
    (`real.rs:4833`, `5644`) call `keep_always` when it is set.
- **Acceptance:** a test replays a synthetic captured compute and graphics pipeline on the GPU and
  gets `ok`; the second replay is `reused` for a pipeline of 16 KiB DXIL or more.
- **Tests:** `with_mem_windows_overrides_and_restores`; `owned_from_record_rebuilds_the_desc`
  (round trip against a desc built by hand from a synthetic console VS/PS used by existing GPU
  tests); `replay_build_makes_a_compute_and_a_graphics_pipeline` (GPU; skip without GPU or DXC);
  `replay_without_a_shader_is_replay_failed`.

#### U7. Prewarm plan and manifest (pure)

- **Inputs:** U5 reader. **Outputs:** the sets, their order, the manifest types and writer, the
  summary export.
- **Files:** new `crates/xcl/src/prewarm_plan.rs`; `lib.rs` (+1).
- **Changes:**
  - `pub struct PlanOptions { scene_start_s: u32 (30), include_huge: bool, cap_bytes: usize
    (pipeline_cap_kb), must_draw: HashSet<u64> }`. `scene_start_s` is Proposed: the first world
    frames meet hundreds of first uses at once (`pso_async.rs:510-516` doc); to be tuned from the
    capture's `ms_in_scene` histogram.
  - `pub fn must_draw(uses: &[UseRec], listed: bool) -> bool`: listed, or any use in a scene named
    `boot`, `menu` or `loading`, or with `presents == 0`.
  - `pub fn scene_start(uses, opts) -> bool`: any use with `ms_in_scene <= scene_start_s * 1000` in
    a scene named `world`; if the capture has no `world` scene, in ordinal 1 of the first scene.
  - `pub enum Set { Blocking, Background, Skip(&'static str) }`; `pub fn plan(c: &Capture, opts:
    &PlanOptions) -> Vec<Planned>` with `Planned { ck, kind, class, set, rank }`: blocking =
    must-draw ∪ scene-start, except L/XL compute that is not must-draw; skip `skipped_cap` over the
    cap unless `include_huge`; background = the rest; order: blocking by (must-draw first, ordinal,
    ms_in_scene); background L/XL compute first (long poles start early), then by (ordinal,
    ms_in_scene). The class comes from the newest `B` frame's `dxil_total` (unknown → M).
  - `pub fn read_must_draw(path) -> HashSet<u64>` (hex per line, `#` comments).
  - `pub struct Manifest { .. }` (section 4.C) with serde; `pub fn write_manifest(path, &Manifest)`
    (temporary file, then rename); `pub fn read_manifest(path) -> Option<Manifest>`.
  - `pub fn summary(c: &Capture, opts) -> serde_json::Value`: per CK `{capture, kind, class,
    must_draw, scenes: [[name, ordinal, ms_in_scene]], keys: [pk…]}` for `ac-eval.py`.
- **Tests:** `must_draw_from_scene_presents_and_list`; `scene_start_uses_world_or_the_first_scene`;
  `l_and_xl_compute_go_to_background_unless_must_draw`; `over_the_cap_is_skipped_unless_include_huge`;
  `blocking_order_is_must_draw_then_earliest`; `manifest_round_trips`.

#### U8. `xcl prewarm`: the runner

- **Inputs:** U6, U7, U1 (main.rs). **Outputs:** the subcommand.
- **Files:** new `crates/xcl/src/prewarm.rs`; `lib.rs` (+1); `main.rs` (+1 arm); `telemetry_gpu.rs`
  (`built_by`: `xcl-pso-pw` → `"prewarm"`, before the `xcl-pso` arm at `2416`).
- **Changes:**
  - `pub fn run(target: &Path, args: &[String]) -> i32`: options `--set blocking|background|all`,
    `--scene-start-s N`, `--include-huge`, `--retry-crashed`, `--grace-s N`, `--max-minutes N`,
    `--dry-run` (print the plan counts per set and class, build nothing), `--merge FILE`,
    `--compact`, `--summary FILE`. Resolves the data dir; `--merge`/`--compact`/`--summary` act on
    the capture and return.
  - Build loop: `real::init()`; `dxc::set_memory_reserve(2)`; `n = compile_threads()` threads named
    `xcl-pso-pw-{k}` (64 MiB stacks like the pool) taking `Planned` entries from a shared index,
    blocking set first; an XL semaphore of 1; each build calls `real::replay_build` with the
    capture's shader reader.
  - Watchdog thread: `running: Mutex<HashMap<thread id, (ck, class, Instant)>>`; every second,
    soft and hard timeouts per the table in 4.C; on hard: outcome `abandoned`, start one
    replacement thread (at most `n` in all).
  - `prewarm-trying.txt` handling; manifest every 30 s and at the end; progress line every 5 s
    (`done/total, per set, ETA`); at the end wait `--grace-s` for abandoned builds, then write and
    `std::process::exit(0)` (`3` if the blocking set had a `refused` or `translation_failed`).
- **Acceptance:** on the PC, `xcl prewarm <game> --set all` on a capture from one Cold run
  finishes, writes the manifest, and the next run's `pipeline` events for captured keys show
  `blob: true` or under 250 ms.
- **Tests:** `prewarm_args_are_parsed`; `the_watchdog_marks_abandoned_and_starts_a_replacement`
  (with a fake build closure that sleeps past a 50 ms hard timeout: the watchdog logic is a pure
  function `fn overdue(running, now, table) -> Vec<..>` plus a small loop);
  `trying_keys_go_last_unless_retry_crashed`; `dry_run_builds_nothing` (empty device not needed);
  e2e in `crates/xcl/tests/` is not added (needs title data).

#### U9. Precache ordered by the capture, with validation counts

- **Inputs:** U5, worker pool U3 (for the gauge). **Outputs:** ranked preload, thread count,
  classification, `precache` event, `preload_queue` gauge.
- **Files:** `pso_async.rs` (`Preload`, `preload()`, `submit_as`, `Pending`, `note_use`, `build`);
  `capture.rs` (`predicted`); `telemetry_gpu.rs` (event and counters).
- **Changes:**
  - `capture.rs`: `pub fn predicted(ck: u64) -> Option<(u16, u32)>` backed by a `OnceLock` map
    built from `read_index` of the capture (newest 8 runs per key; min ordinal, then min
    `ms_in_scene`); built on first call (synchronous; index only).
  - `pso_async.rs`: `Preload.list` becomes `Mutex<BinaryHeap<Ranked>>` with `struct Ranked { rank:
    (u16, u32), seq: u64, p: Arc<Pending> }` ordered smallest rank first, then `seq`. `submit_as`:
    if `capture::predicted(capture)` is `Some(r)` push at `r` and set `p.predicted`; else if
    `used::known(key)` push at `(u16::MAX, u32::MAX)` and set `p.predicted`. Preload thread count:
    `fn preload_threads(n: usize, env: Option<&str>) -> usize` = env value (1-16) else
    `(n / 2).clamp(2, 8)`. `Pending` gains `predicted: AtomicBool`, `class: AtomicU8` (0 none,
    1 hit, 2 late, 3 prior, 4 missed), `scene: (String, u16)` captured at first use.
    `note_use`: if predicted: class hit or late by `ready`; telemetry `pipeline_use` gets
    `predicted`. In `build` (`pso_async.rs:76-98`) after `job()`: for a pending with class 0 that was
    used before it was built, read `take_last_build()`: prior if source is cache and (blob or
    ms < 250), else missed. A pipeline used but never built by the end counts as missed (counted
    at the end from `PENDING`).
  - `telemetry_gpu.rs`: per-scene counters `Mutex<Vec<((String, u16), [u64; 4])>>`;
    `pub fn precache_count(scene, class)`; at each `write_phase` (hook from U4's scene clock) and at
    the end, write `precache` (tier pass) for the scene that ended: `scene`, `ordinal`, `hit`,
    `late`, `prior`, `missed`. The `activity` gauge (worker pool U3) gets `preload_queue`.
- **Acceptance:** a Warm run's `precache` events show `hit` for most first uses; a Cold run with a
  capture shows hit + late equal to the predicted count.
- **Tests:** `ranked_preloads_come_out_earliest_first_then_in_arrival_order`;
  `preload_threads_default_and_override`; `a_predicted_ready_first_use_is_a_hit_and_unready_is_late`;
  `an_unpredicted_build_from_caches_is_prior_and_a_cold_one_missed` (fake last-build facts);
  `precache_counts_are_kept_per_scene`.

#### U9b. Prewarm manifest awareness and eviction detection (spec 5.4)

- **Inputs:** U7 (`read_manifest`), U4. **Files:** `telemetry_gpu.rs`, `real.rs` (one call at
  `init`).
- **Changes:** at `real::init` (after U3's `set_device`), read the manifest; if its `store_id`,
  translator identity and split flags equal the current ones, keep a `HashSet<u64>` of PKs with
  outcome `built`/`reused`/`built_late`; else log once `prewarm made for another configuration
  (<which field>)`. `telemetry_pipeline_built` adds `prewarmed: true` for those PKs and increments
  `DRIVER_CACHE_EVICTED` when such a build has `blob: false` and took 250 ms or more;
  `driver_cache_hit_estimate` counts builds with `blob: false` under 5 ms whose PK has a `B` frame
  with a cold time over 50 ms (Proposed thresholds from spec 11.1). Both counters go into
  `report.json` under `d3d12` and the `end` event.
- **Tests:** `manifest_mismatch_names_the_field`; `evicted_is_counted_only_for_prewarmed_cold_builds`.

#### U10. Missing measurement fields and options (spec 9.3 items 3, 5; Q6)

- **Inputs:** #1830 adopted. **Files:** `report.rs` (`telemetry_shader` seen fields,
  `report.rs:2115`); `gpu/recomp/dxc.rs` (`pub fn opt_level_for(src_len: usize) -> &'static str`,
  the rule of `dxc.rs:628-640` moved into a function `compile_whole` calls); `shaders.rs`
  (`--dxc-opt` for `--measure-pso`; `--records FILE` for `--time-forms`).
- **Changes:** `seen` lines get `hlsl_bytes` (null for direct DXIL) and `dxc_opt`;
  `measure_pso_result` passes `-O0|-O1|-O3` when `--dxc-opt` is given and prints the level used;
  `--records FILE` restricts `--time-forms` to the listed record hashes.
- **Tests:** `opt_level_rule_matches_the_old_thresholds` (1 MB and 8 MB boundaries);
  `seen_line_has_hlsl_bytes_and_dxc_opt`; `dxc_opt_and_records_are_parsed`.

#### U11. `scripts/dev/pipeline-report.py` (fork-only)

- The prototype in `scratchpad/spec-capture-tools/dxil_breakdown.py`, extended with the CSV, the
  `dxc_opt` column (U10) and per-PK join (U4). `scripts/dev/test_pipeline_report.py` with a
  synthetic run folder (three events, two shapes) checks the table and the correlations.

#### U12. Run configurations and AC evaluation (fork-only scripts)

- **U12a `scripts/dev/run-config.ps1`:** parameters `-Config Cold|Warm|Prewarmed -Game -RunRoot
  [-Series] [-Seconds 900] [-Schedule] [-InputScript] [-Xcl]`. Implements section 4.F: series
  folder, capture copy, `XCL_DXIL_CACHE`, `XCL_CAPTURE_DIR`, `XCL_TELEMETRY_DIR=<series>\<config>`,
  `xcl cache --clear-driver` or the exe copy, `xcl prewarm` for Prewarmed, the run (via
  `ProcessStartInfo` as `smoke.ps1:40-62` does), `env.txt`, and a call to `ac-eval.py`.
  `scripts/dev/run-config.tests.ps1`: parameter validation, the folder layout on a dry run
  (`-WhatIf`).
- **U12b `scripts/dev/ac-eval.py`:** reads a run folder (and the Warm baseline folder for AC11,
  and the `--summary` JSON for AC5); writes `ac.json` and prints a table. Definitions in section 9.
  `scripts/dev/test_ac_eval.py` with synthetic events for each AC, pass and fail.

#### U13. Docs

- `README.md`: `xcl prewarm`, `xcl cache`, `XCL_CAPTURE`, `XCL_CAPTURE_DIR`,
  `XCL_PSO_PRELOAD_THREADS`, the title data folder's `pipelines\`, a note on driver cache size.
- `docs/guide/telemetry.md`: `pipeline_use`, `precache`, new `pipeline` fields, report counters.
- `docs/guide/d3d12x.md`: capture and prewarm data flow (section 3 of this file, shortened).

#### Later (not in this design's scope of approval)

- **U14:** `xcl run` runs the blocking set before the title when the manifest is missing or its
  store id differs (first launch, driver change), with a progress line; opt out with
  `XCL_PREWARM=0`.
- **U15:** the prewarm runner builds XL compute in a child `xcl prewarm --one <ck>` inside a job
  object with a 4 GB process memory limit (spec 9.3 item 6), killed at the hard timeout.

---

### 8. Concurrency plan

| Wave | Units in parallel | Files written (no overlap within a wave) |
|---|---|---|
| 1 | U1; U3; U11; #2613 and #1830 through the gate (orchestrator) | U1: `cache_cmd.rs`, `main.rs`, `lib.rs`. U3: `real.rs` (`driver_cache` only), `gpu/adapter.rs`, `report.rs` (`device_facts`). U11: `scripts/dev/` |
| 2 | U4; U10; U12a | U4: `real.rs` (`graphics_blob_key`, `go_background`), `pso_async.rs`, `pso.rs` (`translator_identity`), `telemetry_gpu.rs`, `log.rs`, `telemetry_run.rs`. U10: `report.rs` (`telemetry_shader`), `dxc.rs`, `shaders.rs`. U12a: `scripts/dev/run-config*.ps1`. U3 must be merged before U4 (both in `real.rs`) |
| 3 | U5 | `capture.rs`, `mod.rs`, `real.rs`, `pso_async.rs`, `telemetry_gpu.rs` |
| 4 | U6; U7; U9 | U6: `real.rs` (replay), `pso.rs` (override). U7: `prewarm_plan.rs`, `lib.rs`. U9: `pso_async.rs`, `capture.rs` (`predicted` only), `telemetry_gpu.rs`. `lib.rs` is touched by U7 only in this wave |
| 5 | U8; U9b | U8: `prewarm.rs`, `lib.rs`, `main.rs`, `telemetry_gpu.rs` (`built_by`). U9b: `telemetry_gpu.rs` (build counters), `real.rs` (`init`). Both touch `telemetry_gpu.rs` in different functions: run them one after the other if the orchestrator prefers no shared file |
| 6 | U12b; U13 | scripts; docs |

The worker pool units that edit `pso_async.rs` go before wave 2.

---

### 9. Acceptance runs on the retail title

**Machine:** the Windows PC (RTX 5080, driver as installed, 16 logical CPUs, 63 GB). Release
build of one commit. Same `-Schedule` and input script for every run. `-Seconds 900`.

**Series:** Cold → Warm (same series), then a new series: Prewarmed. Each gives a run folder and
`ac.json`. A Cold run that fails the validity rule (section 4.F) is repeated, not evaluated.

**Definitions computed by `ac-eval.py`:**

- **AC9 (cache hit rate at bind time; Warm ≥ 99 %, Prewarmed ≥ 95 %).** Population: distinct `key`
  values of `pipeline_use` events. Hit: `ready: true`, or the first `pipeline` event with that
  `key` has `source: "cache"` and (`blob: true` or `compile_ms < 250`). A used key with no
  `pipeline` event by the end is a miss. The 250 ms rule follows `KEEP` (`real.rs:4856`): XCL
  deliberately keeps no blob for faster builds and leaves them to the driver's cache. The report
  also gives the share of each hit kind.
- **AC5 (scene's predicted graphics set within 120 s of scene start on Cold; P0/P1 p95 ≤ 100 ms).**
  Predicted set: graphics CKs in the series' `--summary` with a use in a `world` scene. t0: the
  first `phase` event named `world`. For each predicted CK that the run creates (has a
  `pipeline_use` or `pipeline` event with that `capture`), t_built is its first graphics
  `pipeline` event's time; before t0 counts as t0. Pass: max(t_built − t0) ≤ 120 s. Predicted CKs
  never created in the run are reported, not counted. The wait part uses the worker pool design's
  `pipeline_ready` events for wanted builds (p95 of need-to-built); without them, the p95 of
  `frame.pso_wait_ms` is reported as a proxy and the criterion is marked partial.
- **AC6 (background queue drains within 10 min of scene start on Cold).** From `activity`
  gauges: `preload_queue` plus the unwanted lanes of the worker pool design. Pass: the sum reaches
  0 by t0 + 600 s and, after the last `loading` phase ends, never rises by more than the number
  of `pipeline_use` events with `predicted: false` in the same window (newly discovered pipelines).
  Quarantined keys (spec 7 design) are excluded when that design exists.
- **AC11 (commit within +10 % of Warm during world load; per-compile 4 GB guard).** Max process
  commit (`memory` telemetry events or `activity` `committed_mb`) from the last `loading` phase
  start to 60 s after the first `world` phase, against the same window of the Warm run of the same
  series. The 4 GB per-compile part is reported as "not evaluated" until U15.
- **Validity and extras:** coldness check; `driver_cache_hit_estimate`, `driver_cache_evicted`;
  `precache` counts per scene; prewarm wall time and manifest outcome counts; `xcl cache --status`
  sizes before and after each config.

**Expectations to check (hypotheses, not results):** Warm and Prewarmed reach AC9 thresholds
because small graphics pipelines come from the driver's cache (run 13) and larger ones from K3
blobs; AC5 on Cold depends on the preload thread count and the DXC share; AC6 on Cold depends on
the L compute tail (run 12: up to 374 s per program in the driver), so it is the criterion most
likely to fail without prewarm.

---

### 10. Deviations from the user's spec

| # | Spec | What the spec says | What this design does | Why |
|---|---|---|---|---|
| D1 | 3.3 | capture mode is opt-in, for dev/QA runs | on by default for pipelines a run builds; `XCL_CAPTURE=all` records every created pipeline (the spec's mode); `XCL_CAPTURE=0` off | the local path (4.2 C) needs a capture from ordinary play; the default cost is small (B) |
| D2 | 3.3 | merge and de-duplicate by pipeline key | merge by capture key (translator-independent); pipeline keys kept per build record | the translator identity changes with every translator source edit; merging by PK would copy every pipeline per build |
| D3 | 3.3, 3.5 | export to SODB; consume a PSDB first when present | not designed | offline PC, no tool, no PSDB, eligibility open (Q1); the capture keeps what an exporter would need |
| D4 | 3.4 | key includes a root signature hash | the title's console root signature is excluded; XCL's ABI root signature is determined by translator identity and shader sources, which are in the keys | the title's root signature is replaced at build (`real.rs:5595-5597`) |
| D5 | 3.4 | stable across machines | true for the capture key; the translated and DXC caches stay machine-specific (DXC DLL modification time) | the capture is the portable artefact; changing the toolchain identity invalidates every cache once for no v1 gain |
| D6 | 4.2, 5.2 | prewarm at first launch and after a driver change (blocking or progress-barred) | on demand (`xcl prewarm`) in v1; automatic is later unit U14 | measure prewarm cost before putting it on the run path |
| D7 | 4.1 D, 4.2 | idle precompile hands off the rest | deferred; `--set background` on demand plus the lowest-priority preload | idle detection and its risks are not justified before measurement |
| D8 | 4.3 | store key includes OS build | Windows major build only, not the monthly revision | a monthly update would otherwise discard every blob |
| D9 | 4.3 | checksum every cache entry | checksums on driver blobs and capture frames; translated and DXC caches keep structural checks plus the D3D12 runtime's DXIL hash check | a payload checksum there changes both formats and invalidates both caches; refusal already rebuilds |
| D10 | 5.2, 7.6 | prewarm uses the runtime's timeouts: 10 / 20 / 300 / 300 s hard | prewarm table 30 / 60 / 600 / 900 s hard; a hard timeout abandons the wait, the build continues | runtime timeouts do not exist yet; run 12 measured M graphics up to 21.3 s and L compute up to 374.5 s, above the spec's values (also relevant to Q8) |
| D11 | 5.3 | detect NVIDIA Auto Shader Compilation | not done; driver version logged, fast builds counted | vendor-specific; our rule is capability checks only |
| D12 | 6.2 | runtime discovery queues plausible permutations | only same-shaders-other-state (K0) tier, lowest rank | speculative builds of created pipelines cost commit (lazy mode's reason) |
| D13 | 6.2 | load completion may be held for must-draw (10 s cap) | not in this design | belongs to the spec 8.5 design |
| D14 | 6.2 | validation mode when enabled | computed whenever telemetry tier `pass` is on | cost is a few counters; one switch less |
| D15 | 6.3 | fast generic variant first | deferred | doubles driver work; the fast output is unmeasured on NVIDIA |
| D16 | 6.2, 4.2 | must-draw tag at capture | derived at plan time from scene, frames shown and a hand-kept list; the capture reserves a byte for a runtime flag | no runtime must-draw classification exists yet |
| D17 | 12.1 | Cold clears the driver's shader cache | cleared through the D3D12 API for `xcl.exe` only, or a fresh `xcl.exe` copy; never deletes driver folders; a telemetry rule proves coldness | vendor folders are shared by all applications |
| D18 | 12.1 | PSDB and cross-vendor configurations | not run | no PSDB; no AMD GPU here, and #2577's stale-reference risk |
| D19 | AC11 | no compile exceeds 4 GB | reported as not evaluated until U15 | an in-process compile cannot be measured or capped alone |
| D20 | 9.3 item 4, Q6 | measure the EXEC-emulation share | not measured | needs a translator switch, which is out of scope |
| D21 | 3.3 | every pipeline state object, graphics and compute | mesh/stream pipelines counted, not recorded or prewarmed | their replay needs the stream path; v1 covers VS/PS/GS graphics and compute |
| D22 | 6.2, AC5 | precache priority P2 | existing preload threads stay; their count rises from 3 to 7 on the PC (Proposed) | AC5 arithmetic (section 4.D) |

## 2026-11-26 Ray tracing: on/off toggle for the whole chain, leave-out switch, loop-work bound per dispatch (user approved all; default "off until proven")

### 0. Approval and orchestrator review

- **User decisions (2026-11-26):** "Approve all": build U1-U3 first and run with ray tracing off,
  then the loop cap, the on path and the hit checks. Default of `raytracing = "auto"`: "Off until
  proven" (the chain is left out until acceptance run R4 passes; a later one-line change flips it).
- **Why this design exists:** run 16 (every used compute pipeline prebuilt, 0 draws and 0
  dispatches left out) lost the device 3-4 s into the scene; runs with fewer compute passes
  running lasted longer. The title's passes hang the GPU when they run.
- **Citations spot-checked by the orchestrator at 2265e889:** `gpu/effects.rs:27-79` (`of`
  decodes the whole program; `Effects` is filled per instruction), `gpu/effects.rs:97-105`
  (`BIG_CODE`, `big_and_image_only`), `config.rs:250-257` and `:281-291` (tier override and its
  parser), `hle/d3d12x/real.rs:6354-6370` (`create_compute_pso`, `attempt`, `own_compute`,
  `go_background`), `hle/waits.rs:83-92` (`caller`), `tests/console_dlls_e2e.rs:628` (the builder
  constructor symbol), `telemetry_gpu.rs:2254-2264` (`COUNT_NAMES`, 33 names, `reserved_0`),
  `hle/d3d12x/pso_async.rs:18-42` (`Pending`). All hold.
- **Refinements made in review (inside the approved design):**
  - U1: a loop is `rdna2::branch_target(d).is_some_and(|t| t <= d.pc)` (`branch_target` already
    returns the far target when there is one, `gpu/rdna2.rs:588-591`).
  - U2 names "`modules::find_export` style lookups"; no such function exists. The lookup is
    `modules::mapped_modules()` + `resolver::resolve_export_of_mapped`; the designer pins the
    exact calls and the seam in a refinement entry before U2 is routed.
  - U2/U3/U8: a pipeline made from the cache has no `Pending` entry, so a role kept only in
    `Pending` is lost for it. Where roles live and when they are dropped goes back to the
    designer before U2 is routed.
  - R1 needs the consumer list, which U4's pack-tool option generates: R1 runs after U2 and U4.
- **Title data:** nothing in this entry or in the code built from it names a title's shaders or
  record keys. Lists of a title's programs live in the title's data folder on the user's machine.
- **Evidence status:** correlation over runs 10-16 on one machine (RTX 5080 16 GB, driver
  32.0.16.2140, Windows 11). Not tested on AMD. Which programs hang the GPU is not yet known.

Design for the dev branch at 2265e889. Written on macOS: nothing here was built or run. All
`path:line` references are under `crates/xcl/src/` unless they say otherwise. Run facts cite
`events.jsonl` / `log.txt` lines of runs 13-15 (read here) and run 16 (facts the orchestrator
sent; its folder was not on this machine).

### 1. Summary (recommendation first)

**Run 16 changes the picture.** With every compute pipeline prebuilt and nothing left out
(log: "65536 dispatches (0 skipped)"), the device was lost 3-4 s into the scene. Runs where
fewer passes ran lasted longer (runs 10/11: 600-900 s, most compute never built; run 15:
16 s; run 16: 3-4 s). So the title's passes hang the GPU **when they run**. A left-out
producer (reading a) is not the main cause. The cause is (b) wrong inputs on our runtime, or
(c) work that is too long as XCL emulates it. Both show up as loops that run to the guard.

**Recommendation, in build order:**

1. **G1 + A together (units U1-U3): ray tracing OFF and a leave-out switch. No shader change, so
   the cache stays warm.** The `raytracing` setting (`xcl.toml`, `XCL_RAYTRACING`) gets a second
   job. `off` now also leaves out the whole ray-tracing chain:
   - the title's acceleration-structure builds (pipelines made by the console BVH builder
     module);
   - every trace program (a program whose code has `IMAGE_BVH*_INTERSECT_RAY`; the translator
     can see this);
   - the consumers named in a list on the user's machine.

   The same machinery serves `XCL_LEAVE_OUT` / `leave_out`, which leaves out chosen programs by
   record key, name pattern or class (`class:trace`, `class:build`, `class:consumer`,
   `class:loops`). Dispatches that are left out go through the existing skip path and are
   counted under new reasons. **This is the first picture: run 17 with `raytracing=off`.**
2. **D (one run each, no rebuild):** find which programs hang. Leave out `class:trace`, then
   the consumer list, then `class:loops` (section 5.D).
3. **B + E + G2 (units U5-U7): one shader-text change, so one cold rebuild.**
   - B: a loop budget per dispatch, passed in an io word: lanes x budget <= 2^28, with a floor
     of 1,024 passes per lane.
   - E: the loop-guard trip counter always compiled in and switched by the io word. Trips can
     then be counted in any run without a rebuild.
   - G2: the BVH emulation returns a miss on unmapped memory (as upstream #2542) and moves into
     its own prelude.

   **With ray tracing ON, B is a precondition, not a safety net:** run 16 shows that the passes
   as they run today hang the GPU.
4. **G3 (unit U8): ray tracing ON with a per-frame chain gate.** The chain runs in a frame only
   when every builder, trace and producer pipeline is built and none of them was left out
   earlier in that frame. Otherwise all of its consumers are left out for that frame.
5. **G4 (unit U9): checks that the hits are right.** A structural check of the acceleration
   structure, per-module slot counts for the builder, and the existing BVH GPU tests run on the
   32-lane PC (they run there; they do not print SKIPPED: section 3).

**Defaults:**
- `raytracing = "auto"`. On a machine where the chain is not proven, `auto` means chain
  **off**. A later one-line PR makes `auto` mean "on with the gate" once acceptance run R4
  passes.
- `XCL_PSO_IMAGE_SKIP` stays on.
- B stays on by default once it has landed.

### 2. Goal and scope

**In scope:**
- The ray-tracing toggle, with its OFF and ON semantics and how chain parts are recognised.
- The leave-out switch.
- A per-dispatch loop-work bound on the HLSL and direct DXIL paths.
- The telemetry these need: per-frame chain counters, loop trips without a rebuild, and the last
  dispatches written from the CPU side when the device is removed.
- The run plan.

**Out of scope:**
- Making the emulated traversal faster.
- Clearing a left-out consumer's outputs to "no hit" values (rejected in section 5.G).
- Bounding a whole frame's GPU time.
- ExecuteIndirect argument clamps (#1795, not adopted).
- Graphics-shader loop budgets (they keep 65,536).
- Hooking `BuildRaytracingAccelerationStructure`.
- Any title data in the repository.

### 3. What exists today (verified)

| Fact | Where |
|---|---|
| `raytracing` / `XCL_RAYTRACING` (`auto`/`off`/`tier1_0`/`tier1_1`) only sets the tier `CheckFeatureSupport(OPTIONS5)` reports | `config.rs:52-56`, `:183`, `:250-257`, `:281-291`; used at `hle/d3d12x/real.rs:3048`, `null.rs:419` |
| Loop guard: 65,536 passes per lane, all loops together; `XCL_LOOP_BUDGET` changes it, and a translation made with it set is not cached | `gpu/recomp/mod.rs:645-652`, `:665-674`, `:690-692`; `gpu/recomp/pso.rs:922-925`; prelude `gpu/recomp/emit.rs:4861-4865`, `:6381-6382` |
| Direct DXIL backend has its own budget slot | `gpu/dxil/compact.rs:240`, `:303-316`; `gpu/dxil/lower.rs` `LOOP_BUDGET` |
| io words: `KERNEL_SGPRS` 128, used slots 110-112 and 120 (loop counter); 113-119 free on dev | `gpu/recomp/mod.rs:100`, `:204-208` |
| io words are written per dispatch on the CPU, into the list's UMA arena | `hle/d3d12x/bind.rs:1083-1089`; `console_list.rs:902` |
| Image-only rule: >= 16 KiB of code and writes only images, on unless `XCL_PSO_IMAGE_SKIP=0`; it targets the ray-tracing resolve programs, which "write no buffer" | `gpu/effects.rs:97-105`; `hle/d3d12x/pso_async.rs:2897-2914`; decided at `real.rs:6258`, `:6275-6277` |
| `effects::of` already decodes the whole program, so a `traces`/`loops` flag costs one more comparison per instruction | `gpu/effects.rs:27-79` |
| Quarantine by shader key (`shaders_key`, not the record) | `pso_async.rs:303-314`, `:113-120`; `real.rs:6003-6009` |
| Reasons a pipeline is unbuilt, and the dispatch skip counters (33 slots, `reserved_0` free) | `pso_async.rs:208-262`; `telemetry_gpu.rs:2253-2321` |
| Skip sites already count by reason: direct dispatch, ExecuteIndirect, console lists | `bind.rs:905-930`, `:299`, `:330`; `console_list.rs:890` |
| Deferred dispatches replay through `bind::dispatch` | `list_x.rs:2763` |
| Record key = `fnv64` of the console CS bytes (what `pipeline_bind.records` shows) | `telemetry_gpu.rs:2609-2615` |
| Ray-tracing SRVs: an in-UMA acceleration-structure SRV gets an RDNA2 BVH descriptor; outside UMA it gets an empty one and a warning. Runs 13-15 log no such warning | `real.rs:2251-2265` |
| Trace emulation: `bvh_intersect` is in `PRELUDE_MEM`, which every memory-using program gets; instance nodes are left to the shader | `emit.rs:2520-2555`, `:4893`, `:6046-6050` |
| XCL's own builder now has instance nodes and a top level (the HANDOFF "missing" note at HANDOFF.md:248-253 is old). The title uses its own builder DLL instead, and `BuildRaytracingAccelerationStructure` is not hooked (it appears only in the layout tables) | `gpu/bvh.rs:18`, `:85`, `:98`; `hle/d3d12_layout.rs:57-66` |
| Builder list methods of the older layout: #146 UAV barrier, #148 AllocateUploadMemory. The newer-layout remap table does not include them | `real.rs:1047-1049`; `NEWER_LIST_SLOTS` `real.rs:7728` |
| Unmodelled methods in run 14 (`report.json` `called_stubs`): list #126 is RSSetShadingRate on the newer layout (comment at `real.rs:7725-7727`); allocator #9 registers the title's command-memory callback (HANDOFF.md:1754); list #142 and queue #31 are not named anywhere in the source. None was first called from the builder module (every `first_caller` is the game exe) | run14 `report.json` |
| Culprit markers sit in a persistently mapped readback buffer, but are read only on a watchdog release | `hle/d3d12x/culprit.rs:1-13`, `:180`, `:269-300`; caller `list_x.rs:1670` |
| Device-removed telemetry hook | `telemetry_run.rs:202-208` |
| The BVH GPU tests use `gpu_ready()`, which accepts a 32-lane-only GPU, so `gpu_bvh_intersect_ray`, `gpu_built_bvh_nodes_match_the_cpu_node_test` and `gpu_triangle_pair_and_box16_nodes` run on the RTX 5080 | `gpu/recomp/tests.rs:13-22`, `:5003`, `:5279`, `:5349`; `runner.rs:562-566` |
| `xbox_bvh_builder_builds_on_the_gpu` needs `XCL_SAMPLE_DLLS`, `XCL_CONSOLE_LISTS=1` and `XCL_PROBE_BVH_GPU=1`; otherwise it returns at once (counted as passed) | `tests/console_dlls_e2e.rs:614-620` |
| Console lists are opt-in. In the retail runs the title and the builder record through `title_list` (inline writes become method calls), not `console_list` | `console_list.rs:48-51`; `device_x.rs:95-121`; `title_list.rs:1-14` |
| Names: the container parser has no name. The pack tool's name comes from the pack index | `gpu/xbox.rs:163-175`; `shaders.rs:721`. `gpu/shader.rs:19` knows the `ILDN` part fourcc; no parser reads it |

**Run facts that bear on the suspect program:**
- It finished building 12 s before the loss in run 13 (`pipeline` id 5440 at 223.0 s, bind 5441)
  and 22 s before it in run 14 (cache, bind 5625 at 265.4 s).
- **It does not appear in any sampled `pass` event after its bind.** In run 13, bind 5441 is
  absent through frame 2460 (the loss was at frame 2487). In run 14, bind 5625 is absent from
  frames 2610 and 2640, while the later binds 5627 and 5629 do appear in frame 2610. Passes are
  sampled one frame in 30.
- So the suspect runs rarely, or only in the final frames. The timing alone does not convict it.
- Its `shape` record says `split:false, wave64:false, group_threads:32`: a native 32-lane
  program, not the split form. Its `seen` record says `split:true`, which contradicts this.
- The longest sampled single dispatches in run 14 are 54-70 ms (pass events, bind 4440, frames
  2460-2640) and grow over time. That program has 0 loops.

### 4. Readings (a)-(c), ranked

| Rank | Reading | Evidence |
|---|---|---|
| 1 | **(b) wrong inputs make loops run to the guard.** Concrete mechanisms: (b1) the BVH emulation reads unmapped or zero memory, and zero nodes look like a box whose child is node 0, so the traversal never ends (upstream #2542 states this); (b2) the builder's barrier and upload calls on the newer layout land on the wrong handlers (#146/#148 are not remapped); (b3) wave32 cross-lane errors in trace or consumer code; (b4) unknown list #142 (33,515 calls) | Run 16: the loss comes faster the more passes run. The upstream 4060 Ti tester's culprit was a screen-probe filter downstream of a BVH trace pass, with loop bounds read from memory. Today's guard allows 65,536 passes per lane, which is about 10^11 lane-passes for a full-screen dispatch: seconds of GPU time |
| 2 | (c) correct data, but legitimately long as emulated (software traversal per lane; split programs) | Single dispatches of 54-70 ms with no loops (run 14). Twice the dispatch rate in run 16. Cannot be told apart from (b) without trip counts |
| 3 | (a) a left-out producer | Run 16 left nothing out and still lost the device. It may still add hangs in runs 13-15 (run 15: `dispatch_skip_image_only` 28, `ei_after_skipped_dispatch` 216 in its last `frame` event) |

**What run 16 means:** (a) is not needed to explain the loss, and (b)/(c) remain. Telling (b)
from (c) needs per-dispatch loop trips (E). A trip under a budget that a correct program never
reaches means (b). A dispatch that is long with no trips means (c).

### 5. Decisions

#### G. The ray-tracing toggle

**Switch.** Extend the existing setting rather than add a new one. Users ask for one ray-tracing
switch, and reporting tier 0 when the chain is off is consistent with it.

| Value | Tier reported | Chain |
|---|---|---|
| `off` / `none` / `0` | 0 (as today) | left out |
| `on` (new) | host tier | runs, behind the gate (G3) |
| `tier1_0` / `tier1_1` | as today | runs, behind the gate |
| `auto` (default) | host tier (as today) | left out until `rt_proven()` is true. In this release `rt_proven()` is a constant `false`; U8's follow-up changes it after acceptance run R4 |

`XCL_RAYTRACING` overrides `xcl.toml` as today. The new pure function is
`config::parse_raytracing_chain(mode) -> ChainMode { Off, On }` (`auto` maps to `Off` while
`rt_proven()` is false). The log line, once:
`ray tracing: chain off (raytracing=auto); set raytracing=on to run it`.

**Recognising each part (no title data in the repository).**

| Part | Rule | Why |
|---|---|---|
| Builder | a compute pipeline made by a call whose first return address in a mapped image (the `hle/waits.rs:86-92` pattern) lies in the module that exports the console BVH builder constructor (the symbol already used by `tests/console_dlls_e2e.rs:629`, an SDK library name, not title data) | Exact, and cheap (pipeline creation is not hot) |
| Trace | `Effects.traces`: the program has `IMAGE_BVH_INTERSECT_RAY` or `IMAGE_BVH64_INTERSECT_RAY` | The translator knows this. HANDOFF.md:208 counts 8 such programs for this title |
| Producer / consumer | a list on the user's machine: `raytracing_chain` in `xcl.toml` and/or the env `XCL_RT_CHAIN`, `@file` allowed; lines `<16-hex record> [producer\|consumer]` or `name:<glob> [role]` (default role consumer) | No program property marks a resolve or activation program: resolve programs are only "image-only and big" (`effects.rs:102`), and the suspect is neither |
| Acceleration-structure views | counted only (`as_srv` counter at `real.rs:2251`) | The title may write BVH descriptors itself, and runs 13-15 show no SRV warning; not reliable as a rule |

Resource-dependency discovery was rejected: the title is bindless, and XCL does not know a
dispatch's writes without walking its descriptors (cf. `trace_fullscreen`, `bind.rs:938`).

The list file is produced locally by a new pack-tool option (U4):
`xcl shaders <pack> --list-records <glob> [--role consumer] > %LOCALAPPDATA%\XCL\<title>\rt-chain.txt`.
It reads the pack index names (`shaders.rs:721`) and writes record keys. Name rules at run time
need the name inside the container. U4 adds `xbox::debug_name(blob)`, which reads an `ILDN` part
when one is present. If the title's containers have none, `name:` rules match nothing, and a
warning is logged once. Record-key rules always work.

**OFF.** Builder, trace and producer/consumer pipelines are flagged left out when they are made:
- they are never built (no compile time, which spares about a thousand large resolve builds);
- their dispatches skip through the existing path and count as `dispatch_skip_rt_off`.

What the picture loses: ray-traced shadows, reflections and screen-probe lighting from traced
rays. Consumers' output images keep stale or undefined contents (possible artifacts, never a
hang).

No clearing in v1, for two reasons. XCL does not know a left-out dispatch's output resources.
And the neutral value differs per pass (lit vs. unlit, no reflection), so a wrong guess is as bad
as stale data.

A pass that reads the chain's outputs but is not on the list still runs. B bounds it, and the
D procedure finds it (then it goes on the list).

**ON (G3, after B).** "Complete" for a frame means:
1. every builder, trace and producer pipeline seen so far has its real pipeline (`why_unbuilt`
   is None); **and**
2. no dispatch of those roles has been left out yet in this frame (for any reason: building,
   late, image-only, quarantined).

The rules:
- Builder pipelines are **must-run** under ON. They never get `spared`/late treatment, and binds
  wait as for any non-spared compute today. They are few and small, and leaving a build out
  corrupts every later frame.
- Trace and producer pipelines: a skip of one sets the frame's `CHAIN_BROKEN` flag. From then on,
  every consumer dispatch in that frame is left out (`dispatch_skip_rt_incomplete`).
- At each `PresentX` the flag clears. Condition 1 is checked when a consumer dispatches.
- Waiting for producers as must-run was rejected: the large ray-tracing programs take 53-374 s
  each in the driver.

`XCL_PSO_IMAGE_SKIP` stays on. It leaves out only big image-only programs (consumers). Under ON
their absence cannot corrupt another chain pass, and turning it off brings back the start-up
stall it was made for (`pso_async.rs:2901-2906`).

**Hits right on a 32-lane GPU (G4), checked in one run with `XCL_RT_VERIFY=1`:**
1. *Builder*: per-module slot counts. Each console list or device slot called from the builder
   module is logged once, with its handler name, which shows whether #146/#148 or shifted slots
   are used. Plus a structural walk of each acceleration structure whose VA XCL learns (in-UMA
   SRVs, `real.rs:2257`): `gpu::bvh::validate(bytes, root)` checks that child ids are inside the
   buffer, that box min <= max, that values are finite, that node types are known, and that
   there are no cycles (bounded visit count).
2. *Trace*: the existing GPU tests on the PC (they run, not SKIPPED). Plus G2's unmapped-memory
   miss, which removes (b1).
3. *Consumers*: loop trips per program from E.
4. *Unmodelled methods*: #126, #9, #101 and #75 are not ray-tracing methods. #142 and queue #31
   are unknown. One run with `XCL_D3D12_TRACE=1` limited to the scene names their arguments. If
   #142's arguments match the #146 barrier record, it is a barrier on the newer layout and goes
   into `NEWER_LIST_SLOTS`.
5. *Tests*: run `cargo test -p xcl --lib bvh` and the e2e test with its variables on the PC, and
   report the SKIPPED lines.

**Telemetry per frame** (on `frame` events; U3): `rt_build_dispatches`, `rt_trace_dispatches`,
`rt_consumer_dispatches` (run), `dispatch_skip_rt_off`, `dispatch_skip_rt_incomplete`,
`dispatch_skip_left_out`, plus `as_srv` (total). Loop-guard trips of chain programs come from E's
`loop_guard` events, joined by record.

#### A. Diagnostic leave-out switch

- **Source:** `XCL_LEAVE_OUT` (env) and `leave_out` (`xcl.toml`), merged. Comma-, space- or
  line-separated, with `@path` for a file (relative paths resolve against the title's data
  folder) and `#` comments.
- **Tokens:**
  - a 16-hex record key;
  - `name:<glob>`;
  - `class:trace`, `class:build`, `class:consumer` (the chain list's entries), `class:loops` (a
    program with a backward branch, from `Effects.loops`);
  - `!<token>` to exclude, for bisecting.
- Read once, at the first pipeline creation.
- **Effect:** the pipeline is flagged when made (never built; dispatches skip with
  `left_out`). Built-in classes cost nothing to evaluate: `effects::of` already decodes the
  program.
- It does not use the quarantine set. Quarantine is keyed by `shaders_key` and marks a build as
  done-with-nothing, which reports `quarantined` and hides the cause. A separate reason keeps the
  counters honest.
- **No shader change:** cache identity is untouched, so runs stay warm.
- **Telemetry:**
  - one `left_out` event per pipeline: `record`, `rule` (the token that matched), `stage`;
  - the per-frame `dispatch_skip_left_out`;
  - the existing `ei_after_skipped_dispatch`;
  - at run end a log line: `left out: N pipelines, M dispatches`.

#### B. Loop-work bound per dispatch

| Option | Correct programs | Cache | Paths | Verdict |
|---|---|---|---|---|
| Lower fixed per-lane budget (`XCL_LOOP_BUDGET`) | cuts legitimate loops in small dispatches too | **not cached at all** (`pso.rs:923`): every run is cold, hours on this title | both | diagnostic only |
| **Per-dispatch budget in an io word**: per-lane budget = clamp(2^28 / lanes, 1024, 65536), lanes = groups x threads per group | loops in small dispatches are untouched (up to 4,096 lanes nothing changes); large dispatches get >= 1,024 passes per lane | one cold rebuild (prelude text changes), then the value changes at run time with no rebuild | HLSL + direct DXIL, compute; EI and graphics write 0, which means 65,536 | **chosen** |
| Per-loop-entry share (#2291) | nested loops keep their own share | one rebuild | HLSL only in #2291 | a runaway nested loop still gets 65,536 per lane (its "Not tested"); more text per loop |
| Budget from program shape | needs a model of legitimate trip counts that does not exist | rebuild | — | rejected |
| Time-based stop | DXIL has no portable shader clock | — | — | rejected |

**Cap derivation.** Upstream #2291 measured 0.47 ms per pass at 262,144 lanes on an RTX 3060 Ti,
which is about 5.6x10^8 lane-passes/s. An RTX 5080 is about 2-2.5x that: about 1.3x10^9/s. 2^28
lane-passes (2.7x10^8) then take about 0.2 s on the 5080 and about 0.48 s on a 3060 Ti. That keeps
one dispatch under about 0.5 s. Heavier loop bodies scale linearly, so U5 adds a GPU measurement
test that prints ms per pass on the adapter.

**The floor of 1,024 breaks the cap above 262,144 lanes.** A 1080p full-screen dispatch (about
2.07 M lanes) may still run 2.1x10^9 lane-passes, about 1.5 s on the 5080. Today it may run
1.4x10^11. The floor exists so that legitimate loops (sample loops, traversals of hundreds of
steps) are not cut. A legitimate trip shows as a `loop_guard` trip on a program whose output
looks right with `XCL_LOOP_FLOOR` raised. That switch (default 1024) reads at run time, so it
costs no rebuild.

**Still unbounded:**
- ExecuteIndirect dispatches (their size is known only on the GPU; they keep 65,536 unless
  `XCL_EI_LOOP_BUDGET` is set);
- graphics shaders;
- the sum over a frame's ~4,000 dispatches (TDR acts on a single non-preemptible stretch, so
  dispatch boundaries help).

**Split form:** lanes count hardware threads per console group (the group size from the
pipeline's thread-group facts). Both halves of a split wave read the same io word.

**Cost, stated plainly:** U5-U7 change the prelude of every translated program. Every cached
translation, DXIL and driver blob is rebuilt once. On this title that took the hours of the
offline precompile. That is why B, E and G2 ship as one release (section 8).

**ON depends on B.** With B, a runaway chain dispatch at 1080p ends in about 1.5 s or less instead
of minutes. Shadows and reflections from cut traversals are wrong (rays that stop early count as
misses), but the device survives, and the trips show which programs to fix.

#### C. Ray-tracing passes as a class

C is replaced by G's OFF. The class is the chain (builder + trace + listed consumers). Its
default is `auto` = off until proven. Compile queue: chain pipelines are never built under OFF.

The size/image-only rule is not extended:
- the suspect (about 14.6 KiB, which is under `BIG_CODE`) shows that size does not mark the
  class;
- leaving out a non-image program by size alone is the ExecuteIndirect hazard
  (`effects.rs:3-8`).

"The title built no acceleration structure through XCL" cannot gate anything: the builder runs
inside the title's own DLL, and XCL sees only its pipelines and dispatches. That is why the
builder role is the module rule.

#### D. Order of experiments (one run each, warm cache, 600 s of scene)

1. **R1 `raytracing=off`** with the chain list containing the resolve and probe families. A
   picture and no loss means the chain is the cause, and OFF is the shipping workaround.
2. If R1 still loses the device, the cause is outside the listed chain. Bisect with `A`:
   1. `XCL_LEAVE_OUT=class:loops`. No loss means the cause is a looping program.
   2. Then halve: `class:loops,!name:...` or key lists in a file, halving each run (about 10 runs
      for about 1,000 programs).
3. If R1 is clean, find the programs inside the chain (`raytracing=on` plus a leave-out):
   1. `class:trace` left out. No loss: a trace program hangs. Then halve the 8.
   2. Otherwise leave out the consumer list. Then halve it.
   3. Otherwise `class:build`.
4. **If leaving one program out only moves the hang to the next:**
   - the hang is a class (b1/b3) rather than one program;
   - stop bisecting and go to U5-U7 (bound, BVH miss fix, trip counter);
   - rerun with the chain on; the `loop_guard` events then name every tripping program in one run.

**What telemetry must show:**
- `dispatch_skip_rt_off` or `dispatch_skip_left_out` > 0 every frame;
- `left_out` events for the intended records;
- no `device_removed` event;
- R1 also needs a screenshot at 300 s; the user labels what is missing.

#### E. Telemetry

- **Left out by rule:** `left_out` events and the per-frame counters (U3).
- **Trips without a rebuild:**
  - the trip call is always compiled in;
  - `xcl_lc_trip()` returns at once when the io word `IO_LOOP_COUNTER` is 0, so the cost is one
    branch on the trip path only;
  - `XCL_LOOP_COUNTERS=1` then fills the word at run time;
  - cache identity no longer depends on `loop_counters()` (`emit.rs:4827`).

  This ships inside the U5-U7 rebuild.
- **Last dispatches before a removal (CPU side):**
  - a ring of the last 256 dispatches per queue: record, groups, list, frame, rule state. It is
    filled where `telemetry_dispatch` is called;
  - in `telemetry_device_removed` (`telemetry_run.rs:203`), dump the ring as `last_dispatch`
    events, and call a new `culprit::on_device_removed()`. That function reads the
    still-mapped readback markers (`culprit.rs:180`), as the watchdog path does (`:269-300`), so
    "started and not done" is reported even when the reset comes first.
  - The ring is on by default at tier `frame`: a write of a few words per dispatch.

#### F. The user's document (sections 7.6, 8.3, 9.3)

| Section of the document | This design |
|---|---|
| 8.2 "every skip is counted by reason, key and kind" | Kept: new reasons `rt_off`, `rt_incomplete`, `left_out`, plus `left_out` events |
| 8.3 compute dispatch, may-skip: skip and count; must-draw: blocking wait; "dependent passes treated as must-draw" | Departs. The chain is all-or-nothing per frame instead: dependants are left out, not waited for (the producers take minutes to build). Builder pipelines are must-run under ON |
| 7.6 quarantine of keys after hard timeouts, "feeds the section 9 controls (skip)" | Departs. Leave-out uses its own reason, not the quarantine set, so a run tells a deliberate omission from a timeout |
| 9.3(2) skip threshold by size, "a stability lever, not a fix" | Not used for this class: size does not mark it. The leave-out switch is the lever and is diagnostic, except OFF, which is a supported mode |
| 9.3 (all) concern compile cost; loop work at run time is not covered | B is new; it is not in the document |

### 6. Architecture and data flow after the change

```
pipeline creation (real.rs create_compute_pso / dev_create_compute_pso_x)
  -> rt_chain::classify(cs_bytes, caller_module) -> Role {None, Build, Trace, Producer, Consumer}
     + leave_out::matches(record, name, effects) -> Option<rule>
  -> decision = rt_chain::decide(role, rule, chain_mode)
       LeftOut(reason): skip `attempt`, go to go_background (stand-in), pso_async::leave_out(standin, reason)
       Gated(role): normal build; pso_async::set_role(standin, role)
       Normal
dispatch (bind::dispatch, execute_indirect, console_list close, deferred replay)
  -> pso_async::resolve/resolve_wait: LeftOut -> stand-in, no wait, no enqueue
     Gated consumer + rt_chain::chain_open()==false -> treated as LeftOut(rt_incomplete)
  -> existing "not translated -> skip" branch -> telemetry_skip(raw) -> why name -> counter
  -> producer/trace skip -> rt_chain::note_producer_skip() sets CHAIN_BROKEN
  -> dispatch run -> io[IO_LOOP_BUDGET] = budget(groups, threads); io[IO_LOOP_COUNTER] = word or 0
PresentX -> rt_chain::on_frame() clears CHAIN_BROKEN; frame event carries rt_* counters
device removed -> dispatch ring dump + culprit::on_device_removed()
```

### 7. Dependencies

- **Upstream:**
  - #2527 (`XCL_REFUSE_CS`, open) overlaps A but refuses at the driver: compile still runs for
    warm paths, and there are no classes or reasons. A supersedes it; do not adopt both.
  - #2291 (open) is the per-entry alternative to B. Its 2^28 constant and measurements are
    reused; its code is not adopted (B uses a per-lane total and covers direct DXIL).
  - #2542 (open) is the unmapped-window BVH miss. U7 adopts its behaviour; adoption goes through
    `scripts/dev/merge-pr.py` if taken as-is, otherwise it is reimplemented and credited.
  - #1284 is already on dev (`loop_budget`).
  - #2793 (per-region budget) raises the worst case; do not adopt.
  - #1795 was not adopted.
  - #505 is the title's watchdog issue.
- **Order:**
  - U1 -> U2 -> U3 (first picture). U4 can run in parallel with U2.
  - U5 -> U6, and U7 independently, all in one release.
  - U8 after U5 + U2. U9 after U2.

### 8. Constraints and considerations

- **Cache invalidation:**
  - U1-U4, U8 and U9 touch no translator source listed in `build.rs:8-21`. Exception: U1's edit
    to `gpu/effects.rs`, which is not in that list; `gpu/xbox.rs` (U4) **is** in it.
  - A change to a listed file re-translates, but keeps DXIL and driver blobs while the emitted
    text is unchanged (DXIL keyed by HLSL, `emit.rs:4861`).
  - U4 adds a function to `xbox.rs` and emits nothing new, so it costs a re-translation, not
    recompiles.
  - U5-U7 change emitted text: a full cold rebuild, once. Merge them in one batch; run the
    offline precompile right after.
- **Legitimate loops:** the floor 1,024 and `XCL_LOOP_FLOOR` cover them. Trips are visible
  through E.
- **Title-data rule:**
  - lists, names and keys live only in `xcl.toml`, env and the data folder;
  - tests use made-up keys (`0123456789abcdef`) and names (`CS_TestResolve`);
  - the builder rule uses an SDK library symbol already in the tests.
- **Dispatch-path cost:**
  - one atomic load (`any_rules`) when no rule and the chain is off;
  - role and left-out state live on the existing `Pending` entry, which `resolve` already looks
    up: no new map on the hot path;
  - the ring is 4 words per dispatch.
- **Failure modes:**
  - a wrong chain list leaves out non-chain passes (visible as `left_out` events);
  - a missing consumer on the list runs on garbage under OFF (bounded only after B);
  - a module rule that matches nothing means builder dispatches run under OFF (U3 logs
    `rt_build_dispatches`; it should be 0 under OFF).
- **Open risks:**
  - ILDN names may be absent;
  - the floor may be too low for traversals;
  - TDR may still fire on 1080p dispatches at the floor (about 1.5 s);
  - the suspect's split flag contradiction.

### 9. Work breakdown

Each unit has its own PR, under 400 changed lines, and runs `cargo test --workspace` on the PC.
Runtime units need the smoke runs from `CONTRIBUTING.md`.

#### U1. Program facts and rule parsing (pure; about 220 lines)
- **Files:** `gpu/effects.rs`, `config.rs`, new `hle/d3d12x/leave_out.rs` (parsing only),
  `hle/d3d12x/mod.rs` (mod line).
- **Changes:**
  - `Effects` gets `traces: bool` (instruction name `IMAGE_BVH_INTERSECT_RAY` or
    `IMAGE_BVH64_INTERSECT_RAY`) and `loops: bool` (`rdna2::branch_target(d)` < `d.pc`). Set both
    in `of` (`effects.rs:58-77`).
  - Add `pub fn compute_facts(blob) -> Option<Effects>` (wraps `compute_code` + `of`).
  - `config.rs`: fields `leave_out: String` and `raytracing_chain: String` (defaults ""),
    `pub enum ChainMode { Off, On }`, `pub fn parse_raytracing_chain(mode: &str, proven: bool) -> ChainMode`
    (off/none/0 -> Off; on/tier1_0/tier1_1/1.0/1.1/10/11 -> On; auto/"" -> On if proven else
    Off; other -> Off), and `Config::raytracing_chain_mode()` (env `XCL_RAYTRACING` first, as
    `raytracing_tier_override`). `parse_raytracing_tier` also accepts `on` as None (host tier).
  - `leave_out.rs`:
    - `pub enum Token { Record(u64), Name(String), Class(Class), Not(Box<Token>) }`,
      `pub enum Class { Trace, Build, Consumer, Loops }`;
    - `pub fn parse(text: &str, base: &Path) -> (Vec<Token>, Vec<String> /*warnings*/)` expands
      `@path` once (no nesting), `#` comments, BOM;
    - `pub struct Rules { tokens }` with
      `pub fn matches(&self, record: u64, name: Option<&str>, fx: Option<&Effects>, role: Role) -> Option<String>`
      (the matching token's text; a `!` token that matches wins);
    - `pub fn parse_chain(text, base) -> Vec<(Token, Role)>` for chain lines `<token> [producer|consumer]`;
    - `pub enum Role { None, Build, Trace, Producer, Consumer }`.
- **Tests:**
  - `effects_flag_bvh_programs_as_traces` (a made-up code buffer with one IMAGE_BVH instruction
    has `traces`; one without does not);
  - `effects_flag_backward_branches_as_loops`;
  - `raytracing_chain_parsing` (every value above, auto with proven true and false);
  - `tier_parsing_accepts_on_as_host_tier`;
  - `leave_out_parses_keys_names_classes_and_negation`;
  - `leave_out_reads_at_files_relative_to_base_with_comments_and_bom`;
  - `leave_out_negation_wins`;
  - `chain_lines_default_to_consumer`;
  - `bad_tokens_warn_and_are_dropped`.
- **Acceptance:** the tests pass; nothing is wired in yet (no behaviour change).

#### U2. Leaving out at creation and dispatch: G-OFF and A (about 330 lines)
- **Files:** new `hle/d3d12x/rt_chain.rs`, `hle/d3d12x/real.rs`, `hle/d3d12x/pso_async.rs`,
  `telemetry_gpu.rs`.
- **Changes:**
  - `rt_chain.rs`:
    - `static RULES: OnceLock<(Rules, Vec<(Token, Role)>, ChainMode)>` loaded from config + env at
      first use;
    - `pub fn builder_module() -> Option<(usize, usize)>`: the base and size of the mapped module
      exporting `??0XboxBvhBuilder@@QEAA@PEAUID3D12Device11@@_N@Z`, found with
      `modules::find_export` style lookups, cached;
    - `fn caller_in_image() -> usize` (copy of the `hle/waits.rs:86-92` pattern);
    - `pub fn classify(cs: &[u8]) -> (Role, Option<String>)`: role Build if the caller is in the
      builder module, else Trace if `fx.traces`, else the chain list by record (`gpu::shader::fnv64(cs)`)
      or name (U4 later), else None; then
      `left_out = RULES.rules.matches(...)`, else `Some("rt_off")` if role != None and mode == Off;
    - log one line per left-out pipeline, the first 64, then powers of two.
  - `real.rs`, in `create_compute_pso` (`:6355`): before `attempt`, read the CS bytes of
    `desc`. If `classify` gives a left-out reason and `pso_async::enabled()`:
    - call `own_compute(desc).and_then(|o| go_background(...))` directly;
    - then `pso_async::leave_out(*ppv, reason)`;
    - else `pso_async::set_role(*ppv, role)` after the normal path, when a `Pending` entry
      exists.

    `dev_create_compute_pso_x` (`:2851`) goes through `dev_create_compute_pso`, so it is covered.
  - `pso_async.rs`:
    - `Pending` gains `left_out: AtomicU8` (0 none, 1 left_out, 2 rt_off) and `role: AtomicU8`;
    - `pub fn leave_out(standin, reason: &str)` sets the reason and `spared`, and marks the job
      never to be sent (in `enqueue`/`resolve`, return early when `left_out != 0`);
    - `why_of` checks `left_out` first and returns new variants `Unbuilt::LeftOut`
      (`"left_out"`) and `Unbuilt::RtOff` (`"rt_off"`);
    - `put_off_to_submit` already respects `is_spared`.
  - `telemetry_gpu.rs`:
    - `COUNT_NAMES` grows to 36 (new: `dispatch_skip_left_out`, `dispatch_skip_rt_off`,
      `dispatch_skip_rt_incomplete`; replace `reserved_0` with `ei_draw_skip_left_out`);
    - update `skip_index` for the names;
    - add `pub fn telemetry_left_out(record: u64, rule: &str, stage: &str)`, a `left_out` event
      at tier `frame`.
- **Tests (no GPU):**
  - `rt_chain_classifies_builder_by_caller_module` (a seam for the module range);
  - `rt_chain_classifies_trace_by_effects`;
  - `rt_chain_list_marks_consumer_by_record`;
  - `rt_off_leaves_out_every_role_and_keeps_none`;
  - `leave_out_rule_wins_over_chain_on`;
  - `left_out_pipeline_is_never_enqueued` (pso_async: `submit_lazy` + `leave_out` + `resolve`
    returns the stand-in and `job` never runs);
  - `why_unbuilt_names_left_out_and_rt_off`;
  - `skip_index_maps_new_reasons`;
  - extend `execute_indirect_with_a_pipeline_still_building_is_skipped` into
    `execute_indirect_with_a_left_out_pipeline_is_skipped`.
- **Acceptance:** the tests pass. Run R1 (section 10) shows `dispatch_skip_rt_off` > 0 per frame
  and no `pipeline` events for left-out records.

#### U3. Telemetry: chain counters, dispatch ring, culprit on removal (about 250 lines)
- **Files:** `telemetry_gpu.rs`, `telemetry_run.rs`, `hle/d3d12x/culprit.rs`,
  `hle/d3d12x/bind.rs` (one call), `hle/d3d12x/console_list.rs` (one call).
- **Changes:**
  - Per-frame counters `rt_build_dispatches`, `rt_trace_dispatches`, `rt_consumer_dispatches`:
    `telemetry_dispatch` gets `role: u8` from `pso_async::role_of(raw)` at the two call sites.
  - Total `as_srv`, bumped at `real.rs:2255`.
  - `DispatchRing`: 256 entries per queue of `(record, [x,y,z], list, frame, role, left_out)`,
    written in `telemetry_dispatch` when tier >= frame.
  - In `telemetry_device_removed`: emit `last_dispatch` events (newest first, at most 64 per
    queue), then `culprit::on_device_removed()`.
  - `culprit::on_device_removed()` is the body of `on_watchdog_release` without the burst limit,
    run once.
- **Tests:**
  - `frame_event_carries_rt_counters`;
  - `dispatch_ring_keeps_the_newest_256_per_queue`;
  - `device_removed_dumps_the_ring_once`;
  - `culprit_on_device_removed_reports_open_lists` (a seam over `open_lists`).
- **Acceptance:** the tests pass. In a run that loses the device, `last_dispatch` events precede
  `end`.

#### U4. Names and a local list generator (about 180 lines)
- **Files:** `gpu/xbox.rs`, `shaders.rs`, `hle/d3d12x/rt_chain.rs` (use the name).
- **Changes:**
  - `pub fn debug_name(blob: &[u8]) -> Option<String>`: the `ILDN` part via `container_parts`
    (u16 flags, u16 length, then bytes), ASCII only, at most 260 bytes.
  - `rt_chain::classify` passes the name to `matches`.
  - Warn once if no compute container had a name.
  - `xcl shaders <pack> --list-records <glob> [--role producer|consumer]` prints
    `<16-hex record> <role>  # <name>` for matching pack records (`records[i].name`). Output goes
    only to stdout.
- **Tests:**
  - `debug_name_reads_ildn` (a made-up container with `ILDN` "CS_TestResolve");
  - `debug_name_none_without_ildn`;
  - `list_records_matches_glob_and_prints_roles` (a made-up pack).
- **Acceptance:** the tests pass; README documents `--list-records` and the toggle values.

#### U5. Per-dispatch loop budget, HLSL path (about 260 lines; one cold rebuild)
- **Files:** `gpu/recomp/mod.rs`, `gpu/recomp/emit.rs`, `hle/d3d12x/bind.rs`,
  `hle/d3d12x/console_list.rs`, `gpu/recomp/tests.rs`.
- **Changes:**
  - `mod.rs`: `pub const IO_LOOP_BUDGET: usize = 113;`, `pub const DISPATCH_LOOP_PASSES: u64 = 1 << 28;`
    and
    `pub fn dispatch_loop_budget(groups: [u32; 3], threads: u32, floor: u32, max: u32) -> u32`
    = clamp(2^28 / max(1, groups product x threads), floor, max). Also `loop_floor()` (env
    `XCL_LOOP_FLOOR`, default 1024, min 1, read at run time, **not** in `cache_allowed`).
  - `emit.rs`: in compute programs, the prelude line becomes
    `static uint xcl_budget = 1u << 16;` plus, at the entry point, after `io` is readable,
    `{ uint b = io.Load(IO_LOOP_BUDGET*4); if (b != 0u) xcl_budget = b; }` (the default branch
    of `emit.rs:4862-4865`; keep the `XCL_LOOP_BUDGET` replacement as is).
  - `bind.rs` `bind_abi` (`:1088`) and `console_list.rs:902`: for compute,
    `io[IO_LOOP_BUDGET] = dispatch_loop_budget([x,y,z], pso_group_threads(real), loop_floor(), loop_budget())`.
    `bind_abi` gets the groups as a new parameter `groups: Option<[u32; 3]>`: `None` from the
    ExecuteIndirect and graphics paths writes 0. `XCL_EI_LOOP_BUDGET`, when set, writes that
    value for ExecuteIndirect.
- **Tests:**
  - `dispatch_loop_budget_clamps_to_floor_and_max`;
  - `small_dispatches_keep_the_full_budget` (4,096 lanes -> 65,536);
  - `compute_prelude_reads_the_io_budget`;
  - `gpu_compute_loop_stops_at_the_dispatch_budget` (a made-up endless loop, wave32 and split
    forms: io 5000 -> 5000 passes; io 0 -> 65,536);
  - `gpu_measure_ms_per_loop_pass` (prints only, no assert: the calibration for section 5.B).
- **Acceptance:** the tests pass on the PC, with SKIPPED lines reported.

#### U6. Same budget on the direct DXIL path, and the always-compiled trip counter (about 220 lines; same release)
- **Files:** `gpu/dxil/compact.rs`, `gpu/dxil/lower.rs`, `gpu/dxil/lower_window.rs`,
  `gpu/recomp/emit.rs`, `gpu/dxil/compact_tests.rs`.
- **Changes:**
  - The direct path initialises its budget variable (`compact.rs:240`, `VAR_BUDGET`) from the io
    word 113 when non-zero, like `loop_counter_word` (`lower_window.rs:55-60`).
  - Emit `LOOP_GUARD_COUNTED` always (`emit.rs:4827`). `xcl_lc_trip()` returns when
    `xcl_lc == 0`.
  - The cache identity no longer includes the counters flag.
- **Tests:**
  - `gpu_direct_loop_stops_at_the_dispatch_budget`;
  - `uncounted_dispatch_writes_no_trips` (io word 0: the slot stays 0 after an endless loop);
  - `counted_dispatch_counts_trips_once_per_lane`;
  - update `gpu_direct_loop_budget_follows_the_variable`.
- **Acceptance:** the tests pass. In a run with `XCL_LOOP_COUNTERS=1`, warm from a run without
  it, the `pipeline` events report `source: "cache"`.

#### U7. BVH prelude split and the unmapped-memory miss (about 150 lines; same release)
- **Files:** `gpu/recomp/emit.rs`, `gpu/dxil/lower_image.rs`, `gpu/recomp/tests.rs`.
- **Changes:**
  - Move `bvh_intersect` out of `PRELUDE_MEM` into `PRELUDE_BVH`, emitted only when the program
    uses `image_bvh*` (a flag set at `emit.rs:2520`). Later BVH fixes then rebuild only the trace
    programs.
  - Before reading a node, return the miss value when the node address is outside every mapped
    UMA window (the behaviour of #2542), on both paths.
- **Tests:**
  - `bvh_prelude_only_in_programs_that_trace`;
  - `gpu_bvh_unmapped_node_is_a_miss` (box and triangle lanes);
  - the existing three BVH GPU tests still pass.
- **Acceptance:** the tests pass; on the PC, `pass` events show trace dispatches under 50 ms in
  R4.

#### U8. Ray tracing ON with the chain gate (about 200 lines; after U2 and U5)
- **Files:** `hle/d3d12x/rt_chain.rs`, `hle/d3d12x/pso_async.rs`, `hle/d3d12x/present.rs` (one
  call), `telemetry_gpu.rs`.
- **Changes:**
  - `static CHAIN_BROKEN: AtomicBool`, `pub fn on_frame()` (clears it; called where PresentX
    counts a frame), `pub fn note_producer_skip()`, and
    `pub fn chain_open() -> bool = !CHAIN_BROKEN && PRODUCERS_UNBUILT.load()==0`.
    `PRODUCERS_UNBUILT` is incremented by `set_role` for Build/Trace/Producer and decremented when
    their real pipeline lands in `complete`.
  - `pso_async::why_of`: for role Consumer and mode On, when `!chain_open()`, return
    `Unbuilt::RtIncomplete` (`"rt_incomplete"`).
  - For a role Build pipeline: never `spared`, never `late` (skip `mark_late` for it).
  - The skip path calls `note_producer_skip()` when the skipped pipeline's role is
    Build/Trace/Producer.
- **Tests:**
  - `consumer_left_out_after_producer_skip_in_frame`;
  - `gate_reopens_at_next_frame`;
  - `gate_closed_while_a_producer_builds`;
  - `builder_pipeline_is_never_late`.
- **Acceptance:** run R4 (section 10).

#### U9. Verification aids (about 220 lines; after U2)
- **Files:** `gpu/bvh.rs`, `hle/d3d12x/rt_chain.rs`, `hle/d3d12x/real.rs` (an `XCL_RT_VERIFY`
  hook at `:2257`), `hle/d3d12x/guard.rs` (per-module slot log).
- **Changes:**
  - `pub fn validate(data: &[u8], root: u32) -> Result<ValidateStats, String>`: an iterative
    walk with a visit cap of `data.len()/64`, checking id bounds, node types, finite boxes with
    min <= max, and cycles.
  - With `XCL_RT_VERIFY=1`:
    - on each in-UMA acceleration-structure SRV, at the next present, validate and log
      `rt verify: as at VA ok/fault ...`;
    - log once per (module = builder, interface, slot) the handler a builder call reached.
- **Tests:**
  - `validate_accepts_built_bvh` (`bvh::build` of random triangles);
  - `validate_rejects_child_out_of_range`;
  - `validate_rejects_cycle`;
  - `validate_rejects_nan_box`.
- **Acceptance:** the tests pass. A run with `XCL_RT_VERIFY=1` logs at least one verdict, or
  `as_srv` is 0, which the report must state.

### 10. Concurrency plan

- **Wave 1:** U1 alone. It is the base for U2 and U4.
- **Wave 2:** U2 and U4 in parallel. U4 touches `rt_chain.rs` only after U2 lands: U4 does its
  `xbox.rs` and `shaders.rs` parts first and adds the one-line `classify` change as a rebase.
- **Wave 3:**
  - U3 (telemetry files, culprit);
  - U5 (recomp mod/emit, bind, console_list);
  - U7 (emit.rs PRELUDE_MEM section, lower_image.rs).

  U5 and U7 both edit `emit.rs` in different functions, so one worker does U5 then U7, or they
  are merged in that order.
- **Wave 4:** U6 (dxil files, emit.rs guard) after U5. U9 (bvh.rs, guard.rs) in parallel.
- **Wave 5:** U8.
- U5, U6 and U7 are released together.

### 11. Acceptance runs on the retail title (RTX 5080, warm cache unless stated)

Each run is 600 s of scene, with `XCL_TELEMETRY=frame` and `XCL_GPU_CULPRIT=1`.

| Run | Switches | Success |
|---|---|---|
| R1 (after U2) | `raytracing=off`, chain list on the data folder | no `device_removed` for 600 s of scene; `dispatch_skip_rt_off` > 0 in every scene `frame` event; 0 `pipeline` events for chain records; `rt_build_dispatches` = 0; user-labelled screenshot at 300 s |
| R2 (D bisect) | `raytracing=on` + `XCL_LEAVE_OUT=class:trace` (then the consumer list, then `class:build`) | the first leave-out that gives 600 s with no loss names the family |
| R3 (after U5-U7, cold rebuild then warm) | `raytracing=off`, `XCL_LOOP_COUNTERS=1`, `XCL_TELEMETRY=all` | as R1, and `pipeline` events from the cache in a second warm run with counters toggled |
| R4 (after U8) | `raytracing=on`, `XCL_LOOP_COUNTERS=1`, `XCL_RT_VERIFY=1` | no `device_removed` for 600 s of scene; no pass event over 500 ms; `rt_trace_dispatches` > 0 per frame; trips listed per chain record; zero or explained `dispatch_skip_rt_incomplete` after warm-up; verify lines ok |
| R5 (regression) | defaults (`auto`) | same as R1, and a smoke block from `scripts/smoke.ps1` on main and head |

If R4 fails, `auto` stays off, and the trips plus the verify lines say which of b1-b4 to fix next.

### 12. Deviations from the user's document

1. The chain is all-or-nothing per frame. Its dependants are left out, not waited for as
   must-draw (document 8.3).
2. Leave-out is a reason of its own, not the quarantine set (document 7.6).
3. Size is not used to recognise ray-tracing passes (document 9.3(2)).
4. A loop-work bound per dispatch is added. The document has no runtime-work control.
5. The default for the ray-tracing chain is off until proven. The document has no ray-tracing
   policy.

## 2026-11-26 Ray tracing toggle, refinement of U2, U3 and U8: builder lookup, roles, the left-out path, counter slots, list keys (orchestrator approved; inside the approved design)

### 0. Orchestrator review and the split of U2

- **Approved by the orchestrator** as internal structure inside the design the user approved.
  What a user can see is unchanged in kind: three per-frame counters
  (`dispatch_skip_left_out`, `dispatch_skip_rt_off`, `dispatch_skip_rt_incomplete`), a `left_out`
  event, one summary log line, and `raytracing=off` / `XCL_LEAVE_OUT` working with
  `XCL_ASYNC_PSO=0` too.
- **Citations spot-checked at 19ccb8af:** `hle/d3d12x/real.rs:1718-1729` and `:1833`
  (`standin_released`, `pso_release`, `forget_pso`), `hle/d3d12x/pso_async.rs:2560-2570`
  (`resolve` enqueues only when `sent` was false) and `:2596-2602` (`spared` returns through
  `resolve_spared`), `telemetry_gpu.rs:2300-2330` (`Call`, `is_dispatch`, `skip_index`),
  `shaders.rs:1045` (`fnv64` of the inflated blob), the three callers of `go_background`
  (`real.rs:6306`, `:6359`, `:7436`). All hold.
- **U2 is split by files so that three writers can work at once:**
  - **U2a** (`telemetry_gpu.rs`, `telemetry_run.rs`): section 5 below, plus
    `pub fn telemetry_left_out(record: u64, rule: &str, stage: &str)` (a `left_out` event at tier
    frame with `record` as 16 hex digits, `rule`, `stage`), plus one line in `write_end`
    (`telemetry_run.rs`) that logs `rt_chain::summary()` when it is Some.
  - **U2** (`hle/d3d12x/rt_chain.rs` new, `real.rs`, `pso_async.rs`, the `mod` line): sections
    1-4 below.
  - **U4** (`gpu/xbox.rs`, `shaders.rs`, `README.md`): `debug_name` and `--list-records` with
    the key of section 6. Wiring the name into `classify` is a later small unit (U4b): in U2
    `classify` passes no name, so only record keys match. R1 needs keys only.
- **Decisions the orchestrator pinned for U2:**
  - `pub enum LeftOut { Rule, RtOff }` lives in `pso_async.rs` (stored as 1 and 2).
  - `rt_chain::classify(cs: &[u8]) -> (Role, Option<LeftOut>)`. It logs one line per left-out
    pipeline (the first 64, then at powers of two) and calls
    `crate::report::telemetry_left_out(record, rule, "cs")`, where `rule` is the matching
    token's text, or `rt_off`.
  - The pure core is
    `decide(record, fx, builder, rules, chain, mode) -> (Role, Option<(LeftOut, String)>, Option<usize>)`
    (the last value is the index of the chain entry that matched). Role: Build when `builder`,
    else Trace when `fx.traces`, else the role of the first chain entry whose `Record` key
    equals `record`, else None. Left out: `rules.matches(record, None, fx, role)` gives
    `(Rule, text)`; else `(RtOff, "rt_off")` when the role is not None and the mode is Off.
  - The rules text is the config's `leave_out`, then a newline, then the env `XCL_LEAVE_OUT`;
    the chain text is the config's `raytracing_chain`, then a newline, then the env
    `XCL_RT_CHAIN`. `@file` paths are relative to `crate::state::data_dir()`. Parse warnings are
    logged once each. Loaded once, at the first `classify`.
  - `rt_chain::summary() -> Option<String>`: None when nothing was configured and no pipeline
    had a role; else
    `rt chain: <on|off>; list N entries, M matched a pipeline; left out P pipelines (R by rule, O by raytracing off)`.
  - `create_compute_pso` becomes a thin wrapper: the existing body moves unchanged into
    `create_compute_pso_normal`; the wrapper classifies, takes the left-out path of section 4
    or calls the normal one, then `note_role(*ppv, role, built)` for `hr >= 0` and a role that
    is not None, with `built = !pso_async::is_pending(*ppv as usize)`.
  - The `mod` line is `pub mod rt_chain;`.
- **Not built or run on macOS;** every unit is built and tested on the Windows PC by the
  orchestrator.

This replaces the named parts of U2, U3 and U8 in `gpu-hang-design.md`.
- Code citations are at dev tip 717ab78d, under `crates/xcl/src/`. The code is the same as at
  2265e889.
- Nothing was built or run.
- No title data appears here. Tests use made-up keys and names.

---

### 1. Finding the builder module (replaces "`modules::find_export` style lookups" in U2)

**There is no up-front lookup and no cached negative.** The builder is recognised from the
frames of the creating call, every time.

New in `hle/d3d12x/rt_chain.rs`:

```rust
/// The console BVH builder's constructor (an SDK library export, already named in
/// tests/console_dlls_e2e.rs:629): the module that exports it is the builder.
const BUILDER_CTOR: &str = "??0XboxBvhBuilder@@QEAA@PEAUID3D12Device11@@_N@Z";
/// Base of the module found to export BUILDER_CTOR (0: not found yet). Positive-only cache.
static BUILDER_BASE: AtomicUsize = AtomicUsize::new(0);

/// pure seam: whether the module at `base` with these exports is the builder
fn exports_builder(by_name: &HashMap<String, ExportTarget>) -> bool { by_name.contains_key(BUILDER_CTOR) }

/// Whether `addr` lies in the builder module.
fn in_builder(addr: usize) -> bool {
    let b = BUILDER_BASE.load(Ordering::Acquire);
    if b != 0 {
        return crate::modules::image_base_for_addr(addr) == Some(b);   // modules.rs:67-77, lock-free
    }
    match crate::modules::find_mapped_by_addr(addr) {                   // modules.rs:91-93
        Some(m) if exports_builder(&m.exports.by_name) => { BUILDER_BASE.store(m.base, Ordering::Release); true }
        _ => false,
    }
}
```

- **The exact calls:**
  - `modules::image_base_for_addr` (`modules.rs:67`) once the base is known.
  - Otherwise `modules::find_mapped_by_addr` (`modules.rs:91`) and a `HashMap::contains_key` on
    `MappedModule::exports.by_name` (`modules.rs:23`, `pe.rs:599-602`).
  - `resolver::resolve_export_of_mapped` (`resolver.rs:163`) is not needed: we only ask whether
    the module exports the name, not where the export points.
- **When the lookup runs:** at each compute pipeline creation, on the first title-image frame
  (§2). Only a positive result is cached.
  - The builder DLL may be mapped after the first pipeline is made. Every lookup before then
    still checks the frame's own module, so a late load is found at its first pipeline.
  - A module leaves the table only when its load fails (`unregister_mapped`, `modules.rs:56-64`).
    The builder had already loaded when it was cached, so the positive cache stays valid.
- **Cost per pipeline creation:**
  - before the builder is found: one `RwLock` read, plus a linear scan over the mapped images
    (tens), plus one hash lookup;
  - after it is found: a lock-free scan of `RANGE_BASE` (`modules.rs:67-77`).
  - Both are microseconds; the stack capture (§2) costs more.
- **Test seam:** `exports_builder` is pure. `classify_frames(frames: &[usize], module_of: &dyn Fn(usize) -> Option<(usize /*base*/, bool /*is builder*/)>) -> bool`
  holds the rule. `in_builder` is its production `module_of`.
- **Tests:**
  - `exports_builder_finds_the_constructor` (made-up export map);
  - `classify_frames_first_title_frame_in_builder`;
  - `classify_frames_skips_non_image_frames`;
  - `classify_frames_none_without_title_frame`;
  - `builder_found_after_late_load` (`module_of` returns no builder for the first call and the
    builder for the second: the second is Build, the first stays None).

### 2. `caller_in_image()`: frame count and rule (replaces U2's `fn caller_in_image`)

```rust
/// Return addresses of this call, skipping this function's own frame.
#[inline(never)]
fn frames() -> ([usize; 16], usize) {
    use windows_sys::Win32::System::Diagnostics::Debug::RtlCaptureStackBackTrace;
    let mut f = [std::ptr::null_mut::<c_void>(); 16];
    let n = unsafe { RtlCaptureStackBackTrace(1, 16, f.as_mut_ptr(), std::ptr::null_mut()) } as usize;
    (std::array::from_fn(|i| f[i] as usize), n)
}
```

- **Skip 1 and capture 16:**
  - `hle/waits.rs:86-92` captures 6. Here the XCL frames before the title are
    `rt_chain::classify` -> `create_compute_pso` (`real.rs:6354`) -> `dev_create_compute_pso`
    (`real.rs:6346`) -> `dev_create_compute_pso_x` (`real.rs:2851`, console slot #62 only).
  - The guard thunks in front of a console slot do not add a frame: `make_split` uses
    `Tail::JumpToResult` (`guard.rs:42-45`), a tail jump, so the title's return address is the
    next frame.
  - That is at most 4 XCL frames, fewer if inlined. 16 leaves room for a debug build and for
    DLL-internal frames inside the builder.
- **The rule: the first frame that lies in a mapped image (`find_mapped_by_addr` is Some)
  decides. If it is in the builder module, the role is Build.**
  - If no frame among the 16 is in an image, the role is None and a line is logged once
    (`rt chain: no title frame for a compute pipeline`).
- **When the builder calls through a game-exe wrapper:** the first image frame is the exe, so
  the role is not Build. This is deliberate. The builder holds the device itself and creates its
  pipelines in its own code (the constructor path probed in HANDOFF.md:330-345). A pipeline whose
  first image frame is the exe was made by the title.
- **"Any of the first N image frames" was rejected:**
  - it marks title pipelines made while a builder frame is merely deeper on the stack (a title
    callback run from builder code) as Build;
  - under OFF those would be left out, a silent loss outside the chain.
  - If a run shows builder dispatches under the exe, U9's per-module slot log
    (`XCL_RT_VERIFY=1`) shows it. The rule can then widen to "first two image frames", in a
    PR of its own.
- **Risk:** `RtlCaptureStackBackTrace` needs unwind data for the mapped images. `hle/waits.rs:86`
  relies on the same for title frames. If a run logs the "no title frame" line, that is the
  cause.

### 3. Where roles live (replaces `pso_async::set_role` / `role_of` in U2, U3 and U8)

**Roles live in `rt_chain`, keyed by the pointer the title holds, for both warm and stand-in
pipelines.** They are not kept on `Pending`.

```rust
/// title-visible pipeline pointer -> its chain role and whether its real pipeline exists
static ROLES: Mutex<Option<HashMap<usize, (Role, bool /*built*/)>>> = Mutex::new(None);
static ANY_ROLE: AtomicBool = AtomicBool::new(false);   // fast path: no map lookup while false
pub fn note_role(pso: usize, role: Role, built: bool)    // skips Role::None
pub fn role_of(pso: usize) -> Role                       // Role::None when !ANY_ROLE or absent
pub fn forget(pso: usize)                                // removes; U8: fixes PRODUCERS_UNBUILT
```

- **Which pointer:** the `*ppv` that `create_compute_pso` returns (`real.rs:6354-6370`).
  - For a warm pipeline it is the real PC pipeline: the success branch of `attempt` returns at
    `real.rs:6372-6374`, and `note_translated_compute` registers that pointer (`real.rs:6326-6330`).
  - For a stand-in it is the title's own object: a `standin::Shell` per pipeline
    (`standin.rs:106`) or a driver stand-in. `PENDING` is keyed by the same pointer
    (`pso_async.rs:975-990`).
  - This is the pointer **before** `console_list::wrap_pso` (`real.rs:2857-2859`), which runs
    only with `XCL_CONSOLE_LISTS=1`.
- **The dispatch paths see that pointer as `raw`:**
  - `bind::set_pipeline_state` stores the title's object: `s.pso = pso` (`bind.rs:891-894`).
    The list's initial pipeline comes through `bind::reset` (`device_x.rs:119`).
  - `bind::dispatch` reads `let pso = with(this, |s| s.pso)` and keeps it as `raw`
    (`bind.rs:905-911`).
  - `execute_indirect` reads `raw_pso` (`bind.rs:281`).
  - Console lists use `(*p).pc` (`console_list.rs:887`), the pre-wrap pointer.
  - So all three call `role_of(raw)`. None of them uses the resolved real pipeline, which differs
    for stand-ins.
- **Insertion:** at the end of `create_compute_pso`, for every result with `hr >= 0`:
  `note_role(*ppv, role, built)`. `built` is:
  - true for the warm path;
  - false for the stand-in (background) path, including the left-out path (§4).
- **Removal:** `rt_chain::forget(pso)` goes into `real::forget_pso` (`real.rs:1833-1858`). That
  covers both kinds of release:
  - real PC pipelines: `pso_release` (`real.rs:1724-1729`), installed as the Release override on
    every adopted pipeline (`real.rs:2921`), calls `forget_pso` when the count reaches 0;
  - shell stand-ins: `standin::release` (`standin.rs:157-162`) -> `real::standin_released`
    (`real.rs:1718-1721`) -> `forget_pso`;
  - driver-object stand-ins go through `pso_release`.

  `pso_async::released` (`pso_async.rs:3092`) and `release` (`pso_async.rs:948`) are called
  after `forget_pso` on those same paths. `release` frees XCL's own real pipeline, which the
  title never holds, so it needs no hook. A reused address therefore never inherits a role.
- **U8:**
  - `Pending` gains `standin: usize`, set in `submit_as` (`pso_async.rs:975`).
  - `complete(p, real)` (`pso_async.rs:176`) calls `rt_chain::note_built(p.standin)` when
    `real != 0`.
  - `PRODUCERS_UNBUILT` is incremented in `note_role` for an unbuilt Build, Trace or Producer
    entry, and decremented in `note_built` and in `forget` (unbuilt entry).
  - A left-out entry never counts, because it is not gated.
- **U3:** at the three run sites, a role-tagged count goes next to the existing
  `telemetry_dispatch(this, pso)` (`bind.rs:1046`, `console_list.rs:910`) and the
  ExecuteIndirect Ok branch (`bind.rs:363-370`):
  `crate::report::telemetry_rt_dispatch(rt_chain::role_of(raw))`.
- **Hot path:** one atomic load while no role exists. Otherwise one mutex lookup per dispatch,
  the same order as the existing `is_translated_compute` set lookup (`real.rs:1780-1784`).
- **Tests:**
  - `roles_keyed_by_title_pointer_warm_and_standin`;
  - `forget_drops_role_so_a_reused_pointer_has_none`;
  - `role_of_is_none_without_any_role`;
  - U8: `producer_count_follows_build_and_release`.

### 4. The left-out path (replaces the U2 bullet for `real.rs` / `pso_async.rs`)

**Confirmed: with the change below, a left-out pipeline gets a stand-in, is never built, and
never touches the translation, DXIL or driver-blob caches.**

- **`create_compute_pso` (`real.rs:6354`):**
  - First: `if let Some(cs) = shader_bytes(&(*desc).CS)` (`real.rs:3971`), then
    `let (role, left) = rt_chain::classify(cs);`.
  - If `left.is_some()`: go to the background branch directly, without `attempt`
    (`real.rs:6357`), so nothing is translated:
    `own_compute(desc).and_then(|o| go_background(this, f, desc, riid, ppv, true, o, standin_compute, compute_key, build_compute, left))`.
  - Then set `LAST_COMPUTE_TRANSLATED` to true and `LAST_COMPUTE_RS` to 0, and call
    `note_console_compute(hr, ppv)`, as the existing missed branch does (`real.rs:6359-6366`).
    The console-list wrapper then marks it translated, so `console_list.rs:883-891` takes the
    skip path. ExecuteIndirect's `is_console_compute` check (`bind.rs:283`, `:325`) stays true.
- **`go_background` (`real.rs:6216`)** gets a last parameter `left_out: Option<LeftOut>`. The
  graphics and stream callers pass `None`. With `Some(r)`:
  - the stand-in is made as today (`real.rs:6228-6253`): a `standin::shell` by `compute_key`, or
    `standin_compute`, a driver pipeline with the dummy CS (`real.rs:6153-6158`). Neither reads
    any cache of ours;
  - it skips `shaders_key`, the image-only test (`real.rs:6256-6258`), the job, and
    `submit`/`submit_lazy` (`real.rs:6266-6270`);
  - it calls `pso_async::submit_left_out(*ppv, r)`, then `pso_async::shared(*ppv, driver)` when
    `driver != 0` (`real.rs:6271-6273`).

  `go_background` needs three things to be true:
  - `own_compute` returns Some. It always does (`real.rs:6054-6060`).
  - The stand-in is made (`hr >= 0`). If not, it returns None (`real.rs:6248-6253`). Then
    `create_compute_pso` logs once
    `rt chain: no stand-in for a left-out pipeline: built and run normally` and falls through to
    today's path.
  - `ppv` is not null.
- **`pso_async::submit_left_out(standin: usize, why: LeftOut)`**, new. It inserts a `Pending`
  with:
  - `job: Mutex::new(None)`;
  - `sent: true` (`resolve` and `resolve_spared` never enqueue: `pso_async.rs:2564-2568`,
    `:2931-2933`);
  - `spared: true` (`resolve_wait_inner` returns through `resolve_spared` with no wait:
    `pso_async.rs:2600-2602`; `put_off_to_submit` is false: `bind.rs:899-901`);
  - `key: 0` (`note_use` records nothing in `used-pipelines.txt`, `pso_async.rs:158-168`, so a
    later run does not preload it);
  - `done: false`;
  - the new `left_out: AtomicU8` (1 left_out, 2 rt_off).

  `why_of` (`pso_async.rs:249`) tests `left_out` first and returns `Unbuilt::LeftOut` or
  `Unbuilt::RtOff`. `resolve_spared` does not log its "writes only images" line when `left_out`
  is not 0. When the title releases it, `released` (`pso_async.rs:3092-3105`) finds
  `done == false` and the job is None, so it only marks the entry orphaned.
- **A shared stand-in is harmless:**
  - Each title object is its own `Shell` (`standin.rs:106`). Only the driver dummy pipeline
    (`Shared`, `standin.rs:40-55`) is shared, by `standin::Key` (device, root signature, flags:
    `real.rs:6190-6194`).
  - The left-out state lives on the `Pending` of the shell pointer, and roles live under that
    pointer too.
  - A non-left-out pipeline with the same key has its own shell, its own `Pending` and its own
    real pipeline. Resolving either one returns the dummy until that one's own real pipeline
    exists.
  - The dummy fails `is_translated_compute`, so every dispatch of the left-out one is skipped.
- **With `pso_async::enabled()` false (`XCL_ASYNC_PSO=0`, `pso_async.rs:589-591`):** the
  left-out path is taken anyway.
  - It needs no worker: `submit_left_out` and `resolve*` do not consult `enabled()`.
  - So `raytracing=off` and `XCL_LEAVE_OUT` behave the same in both modes.
  - "Warn and run normally" was rejected because it would silently break `raytracing=off`.
- **Tests:**
  - `left_out_pipeline_is_never_enqueued` (`submit_left_out`, then `resolve` and
    `resolve_wait`: they return the stand-in and nothing is queued);
  - `left_out_records_no_used_key`;
  - `why_unbuilt_names_left_out_and_rt_off`;
  - `left_out_and_normal_share_a_driver_standin_independently` (two shells over one key: one
    left out, one built; the built one resolves to its real pipeline, the left-out one to the
    dummy);
  - `released_left_out_pipeline_is_forgotten`;
  - `go_background_left_out_submits_no_job` (a seam: a `left_out` argument with a job counter
    that must stay 0).

### 5. ExecuteIndirect, console lists, counter slots (replaces the `COUNT_NAMES` bullet in U2)

**Every skip site reaches the same reason path:**
- Direct dispatch, `bind.rs:905-920`:
  - `resolve_wait(raw)` gives the dummy (§4) and `is_translated_compute` is false, so the site
    calls `telemetry_skip(this, Call::Dispatch, raw)`;
  - `telemetry_skip` -> `pso_async::skipped(raw).why` -> `skip_index`
    (`telemetry_gpu.rs:2381-2386`, `2317`).
  - `list_x::dispatch_x` (`list_x.rs:3124-3126`) and deferred replays (`list_x.rs:2763`) call
    `bind::dispatch`, so they take the same path.
- ExecuteIndirect:
  - `resolve_draw(raw_pso)` -> for compute, `resolve_wait` plus `is_pending`, which is true
    because `done == false` (`pso_async.rs:2826-2829`);
  - `!gfx && !compute && unbuilt` -> `telemetry_skip(this, Call::Indirect(sig_dispatches), raw_pso)`
    (`bind.rs:297-299`).
  - `bind.rs:330` is the "no translation" branch. It is not reached for a left-out pipeline,
    because the `unbuilt` branch returns first.
- Console lists, `console_list.rs:887-891`:
  - `resolve_wait((*p).pc)` gives the dummy, so the site calls
    `telemetry_skip(list, Call::Dispatch, (*p).pc)`.
- **`Call::Indirect(true)` and `Call::Dispatch` share the dispatch counters** (`is_dispatch`,
  `telemetry_gpu.rs:2306-2308`). An indirect dispatch that is left out counts in
  `dispatch_skip_<reason>`, as image-only skips do today.
  - Left-out rules are compute-only, so neither `ei_draw_skip_left_out` nor
    `ei_dispatch_skip_left_out` is needed.
  - `reserved_0` stays.

**The final list.** It grows from 33 to 36 entries. Add `pub const COUNTS_LEN: usize = 36;`.

| Slot | Name |
|---|---|
| 32 | `reserved_0` (unchanged; its test `telemetry_gpu.rs:3631` stays) |
| 33 | `dispatch_skip_left_out` |
| 34 | `dispatch_skip_rt_off` |
| 35 | `dispatch_skip_rt_incomplete` (U8 sets it; U2 adds the slot) |

**`skip_index`, in the dispatch arm (`telemetry_gpu.rs:2311-2321`):**
- `Some("left_out") => 33`
- `Some("rt_off") => 34`
- `Some("rt_incomplete") => 35`

Draw kinds never receive these reasons. If one did, it would map to slot 4 of its base
("failed", as today).

**Other edits that follow from the new length:**
- `COUNTS`, `counts_now`, and the `counts` fields of the frame and activity sums: change `33` to
  `COUNTS_LEN` (`telemetry_gpu.rs:2254`, `2266`, `2292-2293`, `2967`, `3014`; `telemetry_run.rs:351`,
  `356`, `1359-1360`).
- `frame_count_fields` (`telemetry_gpu.rs:2466-2467`): iterate
  `(0..COUNTS_LEN).filter(|&i| i != 32 && i != MUSTDRAW_WAIT_US && (i < 4 || d[i] != 0))`.
- `activity_count_fields` (`telemetry_run.rs:356-364`): `dispatch_skipped` becomes
  `sum(17..26) + sum(33..36)`.
- `writer_note` (`telemetry_gpu.rs:2325-2327`) also runs for the new reasons, so
  `ei_after_skipped_dispatch` keeps counting the hazard after a left-out trace pass. This is
  intended.

**Tests:**
- `skip_index_maps_new_reasons` (direct and indirect);
- `frame_fields_include_new_slots_not_reserved`;
- `activity_dispatch_skipped_sums_new_slots`;
- `execute_indirect_with_a_left_out_pipeline_is_skipped` (built on `bind.rs:1154`'s test).

### 6. The record key `--list-records` prints (replaces the U4 key text; needed for R1)

- **Runtime:** `pipeline_bind.records` = `format!("{:016x}", fnv64(b))` (`telemetry_gpu.rs:2612`).
  `b` is the title's CS bytes: `dred::label(*ppv, &[("cs", Some(cs))])` (`real.rs:4976`,
  `:5013`), with `cs = shader_bytes(&d.CS)` (`real.rs:4942`, `:3971-3974`). Exactly the bytes in
  `D3D12_COMPUTE_PIPELINE_STATE_DESC.CS`.
- **Pack tool:** `let hash = shader::fnv64(&blob)` (`shaders.rs:1045`), with `blob` from
  `pack::inflate(data, &records[i])` (`shaders.rs:721`).
- **They are equal exactly when the title passes the inflated pack record unchanged.** The run
  data shows this for the one program checked: the record key telemetry gave was found by
  `--record-hash`. The code cannot prove it for every program, because the title could patch the
  bytes.
  - So U2 logs at run end: `rt chain list: N entries, M matched a pipeline`.
  - R1's acceptance adds: M > 0, and every listed key that the run's `pipeline_bind` events show
    has a `left_out` event.
- **FSPX driver containers** (`gpu/recomp/pso.rs:45-50`, read by `effects::compute_code`,
  `effects.rs:83-85`) come from the console upscaler DLL's pipelines, not from the title's shader
  pack. `--list-records` cannot list them, and they are not ray-tracing programs.
  - Their runtime record is still `fnv64` of the whole container, as passed.
  - To leave one out, use the 16 hex digits from its `pipeline_bind` event in `XCL_LEAVE_OUT`.
    This is the same format, so U4 needs no other key.
- **What U4 prints:**
  - one line per matching record: `<fnv64 of the inflated blob, 16 hex> <role>  # <records[i].name>`;
  - records that do not inflate are skipped, with a stderr line;
  - duplicates are printed once, by the same `exported` set logic as `shaders.rs:1034`, `:1049-1051`.
  - Matching is a glob on `records[i].name`, without the ` @offset` suffix.

---

### Changes to the U2/U3/U8 work items

- **U2:**
  - `rt_chain.rs` holds §1-§3;
  - `real.rs` gets §4: `create_compute_pso`, `go_background` with the new parameter (3 callers),
    and `forget_pso` with one line;
  - `pso_async.rs` gets `submit_left_out`, `LeftOut`, the `why_of` and `resolve_spared` lines,
    and `Unbuilt::LeftOut` and `Unbuilt::RtOff`;
  - `telemetry_gpu.rs` and `telemetry_run.rs` get §5.

  Estimate about 380 lines with tests. If it goes over 400, split the telemetry slots (§5) into
  U2a, before U2.
- **U3:** role-tagged counts at the three run sites (§3).
- **U8:** roles and producer counts come from `rt_chain`, not `Pending`. The only change to
  `Pending` is the `standin` field (§3).

## 2026-11-26 Instrumentation: shader code in use, device removal when and where, DRED, debug layer, per-pass output statistics (user: "go ahead ... the instrumentation")

### 0. Approval and orchestrator review

- **User decisions (2026-11-26):** "I think you should do research and figure out how we can add
  real instrumentation for Nvidia style cards, and get away from just looking at screenshots. Get
  real data and don't infer what is happening. Find real proof." Then, on the plan: "Go ahead and
  change the profile for XCL, and also the instrumentation." Then: "For a game like this, I think
  2gb heap makes sense." Installing other tools (Aftermath Monitor, PIX, WinDbg, RenderDoc,
  Nsight) is not approved and is out of this entry.
- **Why:** the device removal in the title's scene was first explained by inference (a hung GPU);
  the removed reason is 0x887A0005, Windows logged no timeout, and upstream issue #2403 documents
  the same removal as NVIDIA's limit on the driver code of pipelines in use. The profile that
  raises it was adopted through the gate (#2590, merge 106caf20) with a fix to load nvapi64.dll
  from System32 only (db9e6c5c). I1 below is the instrument that measures that cause on this PC.
- **Measured before this entry (RTX 5080, driver 32.0.16.2140, ray tracing on, every used compute
  pipeline prebuilt):** nothing left out: removed 0-2.6 s into the scene; first 250 of the 1,000
  per-material resolve programs left out: removed after 11 s of scene; the other 500 left out:
  after 33 s; the first 500 left out: none in 133 s; all 1,000: none in 131 s; builder, trace,
  ray-bin or shadow pipelines left out (19-64 each): removed at or before the first scene frame.
- **Citations spot-checked by the orchestrator at db9e6c5c:** `hle/d3d12x/dred.rs:40-49`
  (`label`), `:57-60` (`mark_pipeline`), `:114-119` (`forget_pso`), `hle/d3d12x/bind.rs:1105-1109`
  (`SetPipelineState` of the real pipeline), `telemetry_gpu.rs:2907-2911`
  (`telemetry_frame_shown`), `hle/d3d12x/pso_async.rs:2607-2611` (`shared`),
  `hle/d3d12x/present.rs:1269` (`presents`). All hold.
- **Evidence standard (binding on later work):** a cause is stated only with the artefact the
  table in section 1 names for it; otherwise it is "not known". Screenshots are used only to ask
  the user to verify by eye once the data says a fix works.
- **Limit values:** `XCL_CODE_LIMIT_MB` defaults to 448 (the #2403 measurement for a 512 MB
  reserve). For the profile at 2 GiB the orchestrator sets it per run; the proportional figure
  is an inference until I1 measures it.
- Nothing here was built or run on macOS; every unit is built and tested on the Windows PC.

Design, 2026-11-26. Code citations are on `rt-wave4` (`1fb5d6c6`). Nothing in the repository
was changed. The research behind it is in `instrumentation-research.md`, in the same folder.
"Inference" marks what no source states. No game-derived name or key appears here. Tests use
synthetic hashes only.

---

### 1. Summary

**Recommendation.** Build the units in the order I1 → I3 → I2, with I4 and the I5 kernel in
parallel in wave 1. I1 decides the question in front of us: does this PC hit NVIDIA's limit on
shader code in use (#2403)? It also costs the least to build. I3 comes before I2 because both
rewrite `log_device_removed` (`crates/xcl/src/hle/d3d12x/real.rs:8075-8121`), and I2's report
builds on I3's output. **I keep the brief's priority order with one change:** I2's
"last-dispatch ring" (I2b) is split from I2a and comes after I1, because both edit `bind.rs` and
`console_list.rs`.

**Today's new measurement fits #2403 and gives I1 a sharp prediction.** Leaving out all 1,000
resolve programs, or just the first 500 (504 pipelines), prevents the removal. Leaving out a
group of 19-64 builder, trace, ray-bin or shadow pipelines does not. A size wall predicts exactly
that: the resolve programs are the large ones (the code comment at `real.rs:4960-4962` gives
about 1 MB of DXIL each), and 19-64 small pipelines do not bring the sum under the wall. The
prediction I1 must confirm or refute:
- With the driver default (512 MB reserve), the counter crosses about 440-450 MB of blob bytes
  just before the removal at 97-99 s. #2403 measured 446-449 MB on four cards with
  `GetCachedBlob` sizes, so I take that as the 512 MB wall in blob bytes.
- With 500 resolve programs left out, it stays below that level for 230 s.
- With the profile at 1 GiB (#2590), the removal goes away, or moves to about twice the level.

What each unit proves, and the artefact a later claim must cite:

| unit | proves | the artefact a claim must cite |
|---|---|---|
| I1 code in use | how much driver code the pipelines in use hold (as #2403 counts it), per frame and at the removal | `code` events per frame; the `code_in_use` line and the `device_removed.code_*` fields. Claim "#2403 limit": removal with `in_use_bytes` ≥ 0.9 × limit in each removal run, AND no removal over the same scene time with the limit raised (1 GiB profile). Refuted by a removal with `in_use_bytes` < 0.6 × limit. |
| I2a removal when/where | the moment of removal, the Execute call after which it was first seen, the queue's submitted and observed-completed values, the removed reason and DRED device state | one `device_removed` event, written before `end`. Claim "removed during submission": `detected.site = "execute"` and `queues[q].pending = 0` at the last sample, OR culprit shows no open dispatch. |
| I2b dispatch ring | the last dispatches recorded into the lists of the last submissions, with record keys | `last_dispatch` events. Claim "program X was running": X in the ring of the detecting submission AND (with `XCL_GPU_CULPRIT=1`) in culprit's "started, not done" list. |
| I3 DRED | for a real GPU fault: the list, the op and the marker text (record key) at the fault; whether a page fault happened at all | `dred_*` log lines and `error` events. Claim "page fault": `device_state = "pagefault"` with a VA. Claim "no GPU fault": state ≠ hung/pagefault and "no list has unfinished work". |
| I4 debug layer | runtime-detected invalid use, removal-process messages 232-234, and optionally GPU-based validation findings | `debug_layer` events and log lines. Claim "invalid use caused it": an error-severity message within 1 s before `device_removed`. |
| I5 pass statistics | what each pass wrote in one frame: ranges, NaN/Inf, zero, unchanged, hash | `pass_stats` events + `scripts/dev/pass-stats-check.py` findings. Claim "pass P writes nothing": `unchanged=true` for its outputs in two armed runs. |
| I6 run recipe | GPU engine activity and resets (ETW), Windows' own TDR records, GPU memory and clock state | `run.etl` → `run-dump.txt`, `events.txt`, `smi.csv`. Claim "Windows timeout": a reset/TDR event in the ETW dump or a 4101 event at the time. |

**The user is asked to check by eye** (screenshots) only after the data says a fix works: no
removal over N runs of the same length, and the I5 invariants passing for the final target.

---

### 2. Goal and scope

**Goal.** Replace inference with recorded evidence for (a) why and where the D3D12 device is
removed, and (b) what each pass of a frame writes. It must work on NVIDIA now and stay
vendor-neutral, with vendor parts behind capability checks.

**In:** I1-I5 as runtime code (opt-in switches, off by default except where telemetry is
already on), one fork-only script (I5d), and the I6 recipe (no code).

**Out:**
- installs (Aftermath, PIX, WinDbg, RenderDoc, Nsight) and any registry change other than the
  approved NVIDIA profile, which the orchestrator handles through #2590;
- a fix for the code-size wall (#1269, #2526, #1901, #2496 upstream);
- shader-side write logging (it changes cache identity);
- graphics-pass read tracking in I5 v1 (§10);
- Aftermath SDK integration (a licence question).

---

### 3. What exists today

| piece | where | what it does / lacks |
|---|---|---|
| Device removal log | `real.rs:8073-8121` `log_device_removed`, `:8126-8139` `stop_if_device_removed` | reason + DRED 1.1; detected only after an Execute (`real.rs:7972`); a once-guard `AtomicBool` (`:8076-8082`) lets a second caller go on to `TerminateProcess` while the first still writes |
| Removal telemetry | `telemetry_run.rs:203-208` `telemetry_device_removed` | an `error` event, fatal: `telemetry_error` writes `end` at once (`:191-197`). The DRED `gpu_fault` events (`:211-243`) therefore land after `end`. |
| DRED enable | `real.rs:503-518` | breadcrumbs + page faults only; no contexts |
| DRED report | `real.rs:8030-8045` `report_dred_lists` | unfinished lists only; silent on getter failure; no `% 65536` (`:8106`, `:8108`) |
| DRED labels | `crates/xcl/src/hle/d3d12x/dred.rs:41-75` | `label` (called for every translated pipeline) and `mark_pipeline` (after every title bind, with the driver pointer `bound`); marker text is only `"xcl pipeline"` (`:70`) |
| Pipeline release | `real.rs:1724-1731` `pso_release` → `forget_pso` (`:1833`) → `dred::forget_pso` (`dred.rs:115`) | every adopted pipeline, including dispatch-chain tail pieces (`real/dispatch_chain.rs:102-105`: "releasing a tail PSO also goes through forget_pso") |
| Driver binds | `real.rs:7628-7632` (title SetPipelineState), `crates/xcl/src/hle/d3d12x/bind.rs:1107-1109` (`bind_abi`, every translated dispatch/draw), `bind.rs:358` (EI whole), `crates/xcl/src/hle/d3d12x/console_list.rs:917-919` (console list replay), `real/dispatch_chain.rs:223` (whole), `crates/xcl/src/gpu/recomp/pso.rs:1370` (pieces) | the points where a driver pipeline reaches a PC list |
| First use | `crates/xcl/src/hle/d3d12x/pso_async.rs:161-172` `note_use` | per title pipeline, not per driver pointer |
| Frame hook | `crates/xcl/src/telemetry_gpu.rs:2908` `telemetry_frame_shown`, called at `present.rs:1247` | once per shown frame |
| Pipeline build event | `telemetry_gpu.rs:2695` `telemetry_pipeline_built` (DXIL bytes, not driver bytes), called at `real.rs:4381` | has no pipeline pointer |
| Debug layer | `crates/xcl/src/gpu/agility.rs:358-367`, `real.rs:492-502`, `:547-552`, `:564-587` | no GBV; 8-per-id cap; log only |
| Culprit markers | `crates/xcl/src/hle/d3d12x/culprit.rs:161-300` | GPU progress per dispatch, read only on a wait timeout |
| Last-dispatch ring | designed as U3 in `.flowestra/decisions.md` (entry "2026-11-26 Ray tracing: on/off toggle …", §U3; refined in the next entry) | **not built** (grep: no `DispatchRing`, no `on_device_removed`) |
| Pass boundaries | `telemetry_gpu.rs:2180` `telemetry_pass_boundary`, from `crates/xcl/src/hle/d3d12x/list_x.rs:4258` (`resource_barrier_x`), `:3138`, `console_list.rs:810`, `:870` | timestamps at tier `pass` |
| Render-target binds | `real.rs:7596-7600` → `list_x::note_render_targets` (`list_x.rs:558`) | per-list RTV/DSV handles |
| UAV views | `real.rs:2631` (CreateUnorderedAccessView hook) | the resource behind a UAV descriptor |
| Bindless resolution | `bind.rs:936-1000` `trace_fullscreen` | root constants/buffers → descriptor slot → `console_heap::resource_at` (`console_heap.rs:532`) |
| Window copies | `list_x.rs:2232-2234` (COMMON↔COPY_SOURCE around `CopyBufferRegion`), `real.rs:206` `uma_window_buffer` | the precedent for copying title memory |
| Runtime HLSL | `crates/xcl/src/gpu/recomp/dxc.rs:492` `dxc::compile` | used by tests for XCL-owned kernels |
| BMP writing | `present.rs:1400-1470` (inline) | precedent for image files |

---

### 4. Architecture and data flow

```
pipeline creation ──dred::label──► code_use::note_label(pso, label)   (I1)
driver bind sites ─────────────► code_use::bound(driver_pso)         (I1: first use, per-frame use)
pso_release → forget_pso → dred::forget_pso ─► code_use::released(pso)
present (telemetry_frame_shown) ─► code_use::frame(n) → `code` event, threshold warnings

ExecuteCommandLists (real.rs:7960 block) ─► removal::after_execute(queue, lists)   (I2a: signal, sample)
                                        └► telemetry ring: submission entry       (I2b)
telemetry_dispatch sites ─► ring: dispatch entry per PC list                      (I2b)
device removed: fence(UINT64_MAX) callback  ─┐
                stop_if_device_removed      ─┴► real::log_device_removed (Once)
                       ├ I3: DRED 1.2/1.3 lines (contexts, device state, flags)
                       ├ I1: code_use::removal_line()
                       ├ I2b: ring dump (`last_dispatch`), culprit::on_device_removed()
                       └ I2a: one `device_removed` event, THEN the fatal error/`end`

debug layer callback ─► log (232-234 uncapped) + `debug_layer` events            (I4)

armed frame (XCL_PASS_STATS=N, tier pass):                                        (I5)
  write targets noted (OMSetRenderTargets, UAV descriptors of a dispatch)
  ├ pre-snapshot copy (first write of a resource in a pass)
  └ post-snapshot copy at the pass boundary (before the title's barrier batch)
  after the list's submission: XCL stats list on the same queue → partials → readback
  pass-stats thread → `pass_stats` events (+ BMP files) → pass-stats-check.py
```

---

### 5. Dependencies and order

| wave | units (parallel within a wave) | why this order |
|---|---|---|
| 1 | **I1** (code in use), **I4** (debug layer), **I5a** (stats kernel) | no shared files (§8) |
| 2 | **I3** (DRED) | edits `dred.rs` after I1 and `real.rs` `init` after I4 |
| 3 | **I2a** (removal when/where), **I5b** (snapshot capture) | I2a needs I3's `log_device_removed` and I1's `removal_line`. I5b is parallel: different files (§8). |
| 4 | **I2b** (ring + culprit on removal), **I5c** (stats list, events, images) | I2b edits `bind.rs`/`console_list.rs` after I1 and I5b. I5c needs I5a and I5b. |
| any time after this design | **I5d** script, **I6** recipe (no code) | the event schema is fixed here |

The upstream pieces the work leans on: #2590 (profile, being adopted by the orchestrator: not a
code dependency); #2566 (marker labels). I3 does what #2566 does for marker text inside its own
diff; if #2566 lands first, I3 keeps #2566's text format and drops its own.

---

### 6. Constraints and considerations

- **Hot path (5,000 dispatches a frame).** Every new call on a bind or dispatch path is one
  relaxed atomic load when its switch is off. I1 when on: a thread-local 64-entry direct-mapped
  cache of (pointer, frame) skips the shared map for repeat binds in a frame. The shared map is
  touched once per (thread, pointer, frame); about 100 ns each, so under 0.3 ms a frame for 3,000
  distinct binds (inference, to be measured in the acceptance run). I2b writes 4 words per
  dispatch into a per-list ring under the list's existing state lock. I5 costs only in the armed
  frame.
- **Cache identity must not change.** No unit changes HLSL, DXIL, root signatures of title
  pipelines, or the pipeline cache keys. `GetCachedBlob` only reads. DRED itself already rebuilds
  pipelines (`culprit.rs:11-13`). That is an existing property of `XCL_DRED`, unchanged.
- **Vendor neutrality.** I1 counts on every vendor; only the limit default (448 MB) is the
  NVIDIA figure, set by `XCL_CODE_LIMIT_MB`. DRED 1.2/1.3 interfaces are optional casts: a failed
  cast logs one line and falls back. GBV is a Windows feature. I5 uses SM 6.0 wave ops and 32-bit
  atomics only.
- **Title data rule.** Record keys (`fnv64` of console shaders) appear only in run output (log,
  `events.jsonl`, images in the run folder) on the user's machine. Tests use made-up hashes such
  as `0x1111_2222_3333_4444`. No dump, name or key goes into the repo, commit messages or PR text.
- **Threads.** `log_device_removed` can now run from a thread-pool thread (I2a's fence callback)
  and from the submitting thread. It becomes `std::sync::Once`-guarded, so the second caller
  blocks until the report is written. Then `stop_if_device_removed` terminates the process.
- **Telemetry ordering.** `end` must stay the last event. All removal details are written before
  the fatal `telemetry_error`.
- **Formatting.** No `cargo fmt`. Format only the changed lines, in the style around them.

---

### 7. Work breakdown

Each unit: files, function-level changes, acceptance criteria, tests (name: assertion). The
switch names and event fields here are final.

#### I1. Shader code in use (about 300 lines; wave 1)

**Definition (as #2403):** a driver pipeline object is *in use* from the first time it is set on
any PC command list until it is released. Its size is the byte length of its `GetCachedBlob()`.
Stand-ins count as well; they are small. A pointer shared by many title pipelines (a shared
stand-in driver, `pso_async::shared`, `pso_async.rs:2609`) counts once.

**Switch:** on when `XCL_CODE_IN_USE=1`, or when the telemetry tier is ≥ `frame`. Read once into a
`OnceLock<bool>`. `XCL_CODE_LIMIT_MB=<n>` (default 448) is the wall in blob bytes. For runs with
the 1 GiB profile, the orchestrator sets 896 (inference: proportional to the reserve).

**New file `crates/xcl/src/hle/d3d12x/code_use.rs`:**
- `pub fn on() -> bool`.
- `struct Entry { bytes: AtomicU64 /*u64::MAX = not measured*/, stage: u8 /*0 unknown,1 compute,2 graphics*/, label: Mutex<Option<String>>, first_frame: AtomicU64 /*0 = never bound*/, last_frame: AtomicU64 }`.
- `static MAP: RwLock<HashMap<usize, Arc<Entry>>>`. Totals (`AtomicU64`): `IN_USE_BYTES`,
  `IN_USE_COUNT`, `IN_USE_COMPUTE`, `IN_USE_GRAPHICS`, `PEAK_BYTES`, `PEAK_FRAME`,
  `NEW_FRAME_BYTES` (first uses this frame), `RELEASED_FRAME_BYTES`, `MEASURE_US` (time in
  `GetCachedBlob`), `RECENT_BYTES` (computed at frame end, see `frame`).
- `pub fn note_label(pso: usize, shaders: &[(&str, Option<&[u8]>)])`: if `on()`, create or update
  the entry. `label` = the same text `dred::label` builds (`"cs 0123…"`, `"vs … ps …"`); `stage` =
  1 if a `cs` key is present, else 2. Do not measure here; measuring on first use keeps unused
  pipelines (51,000 at load, `pso_async.rs:995-998`) at zero cost.
- `pub fn bound(pso: usize)`: if `!on() || pso == 0`, return. Let `f = present::presents()`.
  Thread-local cache `[(usize,u64);64]` at index `(pso >> 4) & 63`: if it equals `(pso, f)`,
  return. Else look up the entry (read lock; insert with the write lock if absent: stage 0, no
  label).
  - If `bytes` is `u64::MAX`, call `measure(pso)` and store the result.
  - If `first_frame` swaps from 0 to `f.max(1)`, it is a first use: add bytes to
    `IN_USE_BYTES`/`COUNT` and the stage total and to `NEW_FRAME_BYTES`, update the peak, and call
    `warn_threshold`.
  - Store `last_frame = f`.
- `fn measure(pso: usize) -> u64`: `ID3D12PipelineState::from_raw_borrowed(&(pso as *mut c_void))`
  → `GetCachedBlob()` → `GetBufferSize()`; 0 on error (counted in a `MEASURE_FAILED` atomic).
  Time it into `MEASURE_US`. The blob is dropped at once.
- `pub fn released(pso: usize)`: remove the entry (write lock). If it had been used
  (`first_frame != 0`), subtract its bytes from the totals and add them to `RELEASED_FRAME_BYTES`.
- `pub fn frame(n: u64)`: compute `RECENT_BYTES` (the sum of entries with `last_frame + 120 >= n`;
  one read-locked pass, only when `n % 30 == 0`, else keep the last value). Emit a `code` event at
  tier `frame` through `crate::log::event`: `{frame:n, in_use_bytes, in_use_count,
  in_use_compute_bytes, in_use_graphics_bytes, new_bytes, released_bytes, recent_bytes,
  peak_bytes, peak_frame, limit_bytes, measure_ms, measure_failed}`. Reset the two per-frame
  totals.
- `fn warn_threshold(bytes)`: warn once each at 75 %, 90 %, 100 % of the limit:
  `xlog_warn!("D3D12.X: driver code of pipelines in use: {mb} MB ({pct} % of {limit} MB) in {count} pipelines")`.
- `pub fn removal_line() -> Option<(String, Vec<(&'static str, serde_json::Value)>)>`: None when
  off. The text: `"driver code in use at removal: {in_use} MB in {count} pipelines (compute {c} MB, graphics {g} MB), bound in the last 120 frames {recent} MB, peak {peak} MB at frame {pf}, limit {limit} MB; largest: {label1} {mb1} MB, … (8)"`.
  The fields carry the same numbers with `code_` prefixes and `code_largest` as an array of
  `{label, bytes, first_frame}`. Unlabelled entries show `pipeline {ptr:#x}`.
- Pure helpers for the tests: `fn pct(bytes, limit) -> u32`, `fn thresholds_crossed(prev, now, limit) -> Vec<u32>`, `fn largest(entries, n)`.

**Hooks (one or two lines each):**
- `crates/xcl/src/hle/d3d12x/mod.rs`: `mod code_use;` (`pub(crate)`).
- `dred.rs:41-49` `label`: after the two existing calls (`:42-43`), add
  `super::code_use::note_label(pso, shaders);` before the `on()` check.
- `dred.rs:57` `mark_pipeline`: first line `super::code_use::bound(bound);` before the `on()`
  check. That covers `real.rs:7631`, `:7649`, `:3002`, `device_x.rs:104` and `list_x.rs:2885`.
- `dred.rs:115` `forget_pso`: first line `super::code_use::released(pso);` before the `on()` check.
- `bind.rs:1109`: after `list.SetPipelineState(pc);` add `super::code_use::bound(real as usize);`.
- `bind.rs:358`: `Ok(Some(whole)) => { list.SetPipelineState(&whole); super::code_use::bound(whole.as_raw() as usize); }`.
- `console_list.rs:919`: after it, `super::code_use::bound(real as usize);`.
- `real/dispatch_chain.rs:223`: bind the whole to a local first, then `code_use::bound(w.as_raw() as usize)`.
- `gpu/recomp/pso.rs:1370`: after `list.SetPipelineState(piece);`,
  `crate::hle::d3d12x::code_use::bound(piece.as_raw() as usize);` (make the path visible with
  `pub(crate)`).
- `telemetry_gpu.rs:2908` `telemetry_frame_shown`: add `crate::hle::d3d12x::code_use::frame(n);`
  outside the tier check (`frame` checks `on()` itself).

**Tests (in `code_use.rs`):**
- `first_bind_counts_once_per_pointer`: two `bound` calls (and a second thread's) on one fake
  entry with preset bytes add its bytes once.
- `release_subtracts_only_used_entries`: an entry released before any bind leaves totals at 0; a
  used one returns totals to 0.
- `shared_pointer_counts_once`: two labels with one pointer count once.
- `thresholds_fire_once_each`: `thresholds_crossed(0, 0.8L, L) == [75]`, then
  `(0.8L, 1.0L) == [90, 100]`, then nothing more.
- `largest_orders_by_bytes_and_caps`: 10 entries → the 8 largest in descending order.
- `removal_line_none_when_off` and `removal_line_names_unlabelled_by_pointer`.
- The tests use a seam: `fn bound_with(pso, frame, measure: impl Fn(usize)->u64)`, so no device
  is needed. A GPU test `gpu_measure_reads_cached_blob_size` creates a tiny compute PSO and
  asserts `measure > 0`; it calls `runner::skip_test` without a GPU.

**Acceptance:** the tests pass. On the retail title (§9), the `code` events are present each
frame and the removal line is printed.

#### I4. Debug layer (about 180 lines; wave 1)

**Files:** `crates/xcl/src/gpu/agility.rs`, `real.rs` (`init` debug block `:492-502`,
`debug_message` `:564-587`), `crates/xcl/src/telemetry_run.rs` (new fn), `docs/guide/cli-and-config.md`
(fork docs: two rows).

**Changes:**
- `agility.rs`: `pub fn enable_gpu_based_validation(no_state_tracking: bool) -> Result<(), String>`:
  `configuration_interface::<ID3D12Debug1>(&CLSID_D3D12Debug)` → `SetEnableGPUBasedValidation(true)`.
  With `no_state_tracking`, also cast to `ID3D12Debug2` and call `SetGPUBasedValidationFlags(D3D12_GPU_BASED_VALIDATION_FLAGS_DISABLE_STATE_TRACKING)`.
  A `OnceLock` like `DEBUG_LAYER` (`agility.rs:358-367`).
- `real.rs` `init`: `XCL_D3D12_GBV` = `1` | `nostate` turns on `debug` as well. After
  `enable_debug_layer` succeeds, call `enable_gpu_based_validation(v == "nostate")` and log
  `"D3D12: GPU-based validation enabled ({mode})"`, or warn on error.
- `debug_message`: pure `fn always_logged(id: i32) -> bool` (true for 232, 233, 234). Those skip
  both caps. For every message (before the caps), call
  `crate::report::telemetry_debug_message(id.0, severity.0, category.0, &t)`.
- `telemetry_run.rs`: `pub fn telemetry_debug_message(id, severity, category, text)`: tier frame.
  - A per-id count in a `Mutex<HashMap<i32,u64>>` (taken with `try_lock`; on contention, count
    into `DEBUG_DROPPED`).
  - The first 64 of an id, and every id 232-234: a `debug_layer` event `{id, severity:
    "corruption"|"error"|"warning"|"info"|"message", category, text (first 512 chars), n}`.
  - After that, a `debug_layer_count` event `{id, n}` at n = 128, 256, 512, ….
- Pure helper `fn debug_event_due(n: u64, id: i32) -> Option<&'static str>` returning
  `"debug_layer"`, `"debug_layer_count"` or None.

**SDKLayers crash: what to do (no code in this unit; the orchestrator does it on the PC):**
1. Find the crashed test binary's WER report: `C:\ProgramData\Microsoft\Windows\WER\ReportArchive\AppCrash_xcl-*`.
   `Report.wer` gives the faulting module offset and the exception code. Keep it outside the repo.
2. Rerun that binary alone with the same environment and `--test-threads=1 --nocapture`. With
   one thread, libtest prints `test <name> ...` before each test runs, so the last line names the
   test (inference from libtest's behaviour; confirm on the first run).
3. A lead from reading: `gpu/texture.rs:888-897`
   (`gpu_large_mixed_upload_keeps_tight_staging_and_exact_pixels`) calls `EnableDebugLayer()`
   directly when `XCL_D3D12_DEBUG` is set. That bypasses the once-guard in
   `agility::enable_debug_layer`, whose comment (`agility.rs:358-359`) says that enabling it
   again after a device exists removes that device. In a test process where another test already
   made a device, this can break the debug layer state (inference).
4. Whether it matters: only if the same crash happens in `xcl run` with `XCL_D3D12_DEBUG`.
   Acceptance run R4d (§9) answers that. A test-only bug gets its own small PR (outside this plan).

**Tests:** `always_logged_covers_removal_ids` (232, 233, 234 true; 0, 231, 235 false);
`debug_event_due_caps_per_id` (n = 1 and 64 give `debug_layer`, 65 gives None, 128 gives
`debug_layer_count`, id 233 at n = 1000 gives `debug_layer`).

**Acceptance:** the tests pass. Run R4d with `XCL_D3D12_GBV=1` on a reduced repro: the log says GBV is enabled, and `debug_layer` events
appear.

#### I3. DRED done properly (about 280 lines; wave 2)

**Files:** `real.rs` (`init` DRED block `:503-518`, `report_dred_lists` `:8030-8045`,
`log_device_removed` `:8075-8121`, the test module `dred_tests` `:8047-8071`), `dred.rs`
(`mark_pipeline` marker text `:70`; new pure fn), `telemetry_run.rs` (new fn).

**Changes:**
1. **Enable contexts.** After `SetPageFaultEnablement`, `if let Ok(d1) = d.cast::<ID3D12DeviceRemovedExtendedDataSettings1>() { d1.SetBreadcrumbContextEnablement(D3D12_DRED_ENABLEMENT_FORCED_ON) }`.
   Otherwise log `"D3D12: DRED 1.2 breadcrumb contexts unavailable"`.
2. **Marker text carries the label.** `dred.rs:70`: the marker string becomes
   `format!("xcl {text}")`, with `text` as computed at `:62` (`"cs 0123…"` or `"pipeline 0x… (a stand-in, or not translated)"`).
   The counting fallback (`markers_before`, `bound_at`) is unchanged.
3. **Getters with errors reported.** `log_device_removed` reads, in order:
   - `dev.cast::<ID3D12DeviceRemovedExtendedData2>()` → `GetDeviceState()`. The state text is
     `fn device_state_text(D3D12_DRED_DEVICE_STATE) -> &'static str` (`"unknown"|"hung"|"fault"|"pagefault"`).
     Without DRED2: `"unavailable"`.
   - The page fault: `GetPageFaultAllocationOutput2` (DRED2; `PageFaultFlags` printed as hex),
     else `GetPageFaultAllocationOutput1`, else `GetPageFaultAllocationOutput`. With DRED2 and a
     state other than `pagefault`, print `"DRED: no page fault recorded (device state {s})"`
     instead of a VA line. With state `pagefault`, or with no DRED2, print the VA line as today.
     Object names come from `ObjectNameW` of `ALLOCATION_NODE1` when available (this also covers
     upstream #2131).
   - Breadcrumbs: `GetAutoBreadcrumbsOutput1` (DRED1), else `GetAutoBreadcrumbsOutput`. A getter
     error prints `"DRED: {getter} failed: {e}"`.
4. **`report_dred_lists` becomes summary + detail.** A new pure
   `fn summarize(nodes: impl Iterator<Item=(u32 /*done*/, u32 /*count*/)>) -> (usize, usize, usize)`
   counts (complete, unfinished, not started). Always print
   `"DRED: {n} lists: {complete} complete, {unfinished} unfinished, {not_started} not started"`.
   When unfinished + not started is 0, also print `"DRED: no list has unfinished work"`. Detail
   lines stay as today (unfinished first, at most 64).
5. **Index modulo 65536.** A pure `fn history_index(op: u32) -> usize { (op % 65536) as usize }`
   and `fn history_len(count: u32) -> usize { count.min(65536) as usize }`. Use both at
   `:8106`/`:8108`. When `count > 65536`, pass to `dred::bound_at`/`bound_after` only the stored
   window: the op range `[count-65536, count)` mapped through `history_index`, and offset `done` by
   `count-65536`. Print `"(history truncated to the last 65536 ops)"`.
6. **Context text at the op.** For a `NODE1`, a pure
   `fn context_before(contexts: &[(u32, String)], op: u32) -> Option<&str>` returns the string of
   the last context whose `BreadcrumbIndex <= op`. The detail line gets `; marker: {text}` and the
   `gpu_fault_list` event gets `marker`. When a context exists, it replaces `bound_at`'s guess;
   `bound_after` is still printed.
7. **Telemetry.** `telemetry_run.rs`: `pub fn telemetry_dred_summary(state: &str, lists: (usize,usize,usize), page_fault: Option<u64>)`
   writes an `error` event `{kind:"gpu_fault", what:"dred_summary", device_state, complete,
   unfinished, not_started, va}` (`va` null when there is no fault).

**Tests (in `real.rs` `dred_tests` and `dred.rs` tests):**
- `history_index_wraps_at_65536`: `history_index(65535)==65535`, `history_index(65536)==0`,
  `history_len(70000)==65536`.
- `summarize_counts_complete_unfinished_unstarted`: (2,2),(1,2),(0,2) → (1,1,1).
- `no_unfinished_work_is_reported_when_all_complete`: a list of complete nodes yields the
  "no list has unfinished work" line (through a pure `fn summary_lines(...) -> Vec<String>`).
- `context_before_picks_the_last_marker_at_or_before_the_op`: contexts at 2 and 6; op 5 → index
  2's text, op 6 → index 6's text, op 1 → None.
- `device_state_text_names_all_states`.
- `marker_text_carries_the_label` (`dred.rs`): a pure `fn marker_text(label: &str) -> String` gives
  `"xcl cs 1111222233334444"`.
- The existing `a_late_partially_executed_list_is_reported_before_unstarted_lists` still passes.

**Acceptance:** the tests pass. Run R2 with `XCL_DRED=1` prints a `DRED: n lists: …` summary and
a device-state line.

#### I2a. Removal: when and where (about 300 lines; wave 3)

**Files:** new `crates/xcl/src/hle/d3d12x/removal.rs`; `hle/d3d12x/mod.rs` (one line);
`real.rs` (the execute block `:7960-7972`, `init` after the device is made, `log_device_removed`,
`stop_if_device_removed`); `telemetry_run.rs` (new fn, and an ordering change in the removal path).

**Switch:** on when the telemetry tier is ≥ `frame`, or with `XCL_REMOVAL_INFO=1`. The fence
callback is installed whenever the device is created; it is one fence and costs nothing per frame.

**`removal.rs`:**
- `pub fn arm(dev: &ID3D12Device)`, called at the end of `init` before `into_raw` (`real.rs:555`):
  - `CreateFence(0)`, then `CreateEventW` (auto-reset) and `SetEventOnCompletion(u64::MAX, event)`.
  - `RegisterWaitForSingleObject(&mut h, event, Some(on_removed), null, INFINITE, WT_EXECUTEONLYONCE)`.
    Microsoft documents this pattern as a device-removal callback
    (https://learn.microsoft.com/en-us/windows/win32/api/d3d12/nf-d3d12-id3d12device5-removedevice).
  - Keep the fence and handles in statics for the process lifetime.
- `unsafe extern "system" fn on_removed(_: *mut c_void, _: BOOLEAN)`: store `REMOVED_QPC = qpc()`,
  set `MOMENT_SOURCE = "fence"`, call `real::log_device_removed()`. The process is not stopped
  here; the next Execute's `stop_if_device_removed` does that, after the `Once` (below).
- Per-queue progress: `struct QueueProgress { fence: ID3D12Fence, submitted: u64, last_seq: u64, completed_seen: u64, seen_qpc: i64, executes: u64 }`
  in `Mutex<HashMap<usize, QueueProgress>>`.
  - `pub fn after_execute(queue: *mut c_void, lists: &[*mut c_void], seq: Option<u64>)`, called
    after the `execute_with_waits_on` closure (`real.rs:7968`) and before `stop_if_device_removed`:
    - If off, return.
    - Create the queue's fence on first use. Read `GetCompletedValue()`; if it is not `u64::MAX`,
      store it with `seen_qpc`.
    - `submitted += 1`, then `queue.Signal(fence, submitted)`.
    - Record `last_seq` and the list count.
  - `seq`: the telemetry submission number when telemetry is on (`GpuTel` `seq`); otherwise
    removal.rs's own counter. Expose the telemetry one with
    `pub(crate) fn telemetry_last_seq() -> Option<u64>` in `telemetry_gpu.rs`. That is the only
    edit there in this unit.
- `pub fn detected_at(site: &'static str, queue: *mut c_void)`: the first caller stores the site
  (`"execute"`, `"present"`, `"fence"`) and the queue. `stop_if_device_removed` calls it with
  `"execute"` and the queue. `stop_if_device_removed` gets the queue as a new parameter: the
  execute site passes `this`, other callers pass null with `"other"`.
- `pub fn report() -> Vec<(&'static str, serde_json::Value)>` and log lines:
  - `"removal: device removed at +{ms} ms (source {fence|detected}), first seen after {site} on queue {q:p}"`;
  - per queue: `"removal: queue {q:p}: {submitted} submissions, completed {completed_seen} as of {ms} ms before the removal ({pending} not seen complete)"`.
  - `pending = submitted - completed_seen`. The fields: `moment_ms`, `source`, `site`, `queue`,
    `queues: [{queue, submitted, completed_seen, pending, seen_ms_before}]`.
  - A pure `fn classify(pending_on_detecting_queue: u64, seen_ms_before: f64) -> &'static str`
    returns `"idle_at_last_sample"` (pending 0), `"work_outstanding"` (pending > 0 and seen
    within 100 ms), or `"unknown"`.

**`real.rs` changes:**
- `log_device_removed`: replace the `AtomicBool` once-guard (`:8076-8082`) with a
  `static ONCE: std::sync::Once`. The body runs inside `ONCE.call_once`, so a concurrent caller
  blocks until it is done. Order inside:
  1. the reason line;
  2. I3's DRED lines;
  3. `code_use::removal_line()` printed;
  4. (I2b adds the ring and culprit here);
  5. `crate::report::telemetry_device_removed_full(hr, msg, device_state, removal::report(), code_fields)`.
- `stop_if_device_removed(queue: *mut c_void, site: &'static str)`: calls
  `removal::detected_at(site, queue)` before `log_device_removed()`. Update all callers
  (`real.rs:7972` passes `this, "execute"`; grep for the others).
- The execute block: `super::removal::after_execute(this, &mapped, crate::report::telemetry_last_seq());`.

**`telemetry_run.rs`:**
- `pub fn telemetry_device_removed_full(hr, message, device_state, removal_fields, code_fields)`
  writes a `device_removed` event (tier frame): `{hresult, message, device_state, …removal fields,
  …code fields}`. Then it calls the existing fatal `telemetry_error(ErrKind::DeviceRemoved, …)`,
  which writes `end`. `telemetry_device_removed` stays as a thin wrapper for other callers.
- The DRED events from I3 (`gpu_fault`) are written before this call, so they precede `end`.

**Tests (in `removal.rs`):**
- `classify_names_idle_outstanding_unknown`: (0, 5.0) idle; (3, 20.0) outstanding; (3, 500.0) unknown.
- `detected_at_keeps_the_first_site`: a second call does not overwrite it.
- `report_fields_have_fixed_names`: `report()` on a seeded state yields the field names above.
- `log_once_blocks_second_caller` (in `real.rs` tests, through a seam
  `fn once_report(f: impl FnOnce())`): two threads, the second returns only after the first's
  closure has finished (an `AtomicBool` set at its end is true when the second returns).
- `device_removed_event_precedes_end` (`telemetry_run.rs` tests, with the existing test sink):
  `device_removed`, then `error`, then `end`.

**Acceptance:** the tests pass. Run R2 has one `device_removed` event with `moment_ms`, `site`
and `queues`, before `end`.

#### I2b. Last-dispatch ring (U3, amended) and culprit on removal (about 250 lines; wave 4)

**What is reused from U3 (`.flowestra/decisions.md`, §U3):** 4 words a dispatch; filled where
`telemetry_dispatch` is called; dumped as `last_dispatch` events (newest first, at most 64 per
queue); `culprit::on_device_removed()` = the body of `on_watchdog_release` without the burst
limit, run once.

**Amendment (why):** U3 says "256 entries per queue". At record time the queue is not known: a
list is recorded first and submitted to a queue later. So:
- each PC list keeps its last 64 dispatches of the current recording;
- each queue keeps the last 16 submissions (seq, list pointers) at Execute;
- at removal, for each queue, the newest submissions are expanded into their lists' dispatches,
  up to 64 per queue (U3's cap).

The rt-role counters (U3's first bullet, refined in the next entry, "U3: at the three run sites")
are out of this unit if not yet built. They are not part of the instrumentation.

**Files:** `telemetry_gpu.rs` (ring + entry points), `bind.rs` (one call at `:1046`, one in the
EI Ok branch near `:363`), `console_list.rs` (one call at `:920`), `culprit.rs` (new fn),
`real.rs` (`log_device_removed`, one call; the execute block, one call).

**Changes:**
- `telemetry_gpu.rs`:
  - `struct ListRing { entries: [(u64 /*record*/, [u32;3], u64 /*frame*/, u8 /*kind: 0 dispatch,1 indirect*/); 64], next: usize, len: usize }`
    in `Mutex<HashMap<usize, ListRing>>`.
  - `pub fn telemetry_ring_dispatch(list: *mut c_void, pso: usize, groups: [u32;3], indirect: bool)`
    (tier frame). The record is `crate::hle::d3d12x::culprit::cs_of(0, pso)`-style: make
    `culprit::cs_of` (`culprit.rs:139`) `pub(crate)`. The note there: `CS` is filled only with
    `XCL_GPU_CULPRIT=1` (`culprit.rs:37-39`). So add a second lookup: the label held by
    `code_use` (I1) for `pso` (`code_use::label_of(pso) -> Option<String>`). The ring stores
    whichever exists: the 64-bit record, else the label index into a small intern table.
  - Clear the ring on `telemetry_list_reset` (`:2195`).
  - `pub fn telemetry_ring_submit(queue, lists, seq)` keeps 16 per queue, called from the execute
    block.
  - `pub fn telemetry_ring_dump() -> Vec<String>` writes `last_dispatch` events
    `{queue, seq, list, index, record|label, groups, frame, indirect}`, newest first, ≤ 64 per
    queue. It returns the log lines.
  - A pure `fn newest(ring: &ListRing, n: usize) -> Vec<…>` for the tests.
- `culprit.rs`: `pub fn on_device_removed()` = the body of `on_watchdog_release` (`:269`) without
  its burst limit, guarded by a `Once`; a no-op when `!enabled()`.
- `real.rs` `log_device_removed`, step 4: `crate::report::telemetry_ring_dump()` (print its lines),
  then `super::culprit::on_device_removed()`.

**Tests:**
- `dispatch_ring_keeps_the_newest_64_per_list` (U3's test, renamed for the amended size).
- `ring_clears_on_list_reset`.
- `submission_ring_keeps_16_per_queue`.
- `device_removed_dumps_the_ring_once` (U3's test): a second dump writes nothing.
- `culprit_on_device_removed_reports_open_lists` (U3's test, the seam over `open_lists`).

**Acceptance:** the tests pass. Run R2 has `last_dispatch` events before `end`, with records or
labels.

#### I5a. Pass statistics kernel (about 300 lines; wave 1)

**Files:** new `crates/xcl/src/gpu/pass_stats.rs`; `crates/xcl/src/gpu/mod.rs` (one line).

**Content:**
- `pub enum FormatClass { Rgba8Unorm, Bgra8Unorm, Rgba16Float, Rg16Float, R16Float, Rgba32Float, Rg32Float, R32Float, R11G11B10Float, Rgb10A2Unorm, D32Float, D24UnormS8, R32Uint, Raw32 }`
  and `pub fn class_of(fmt: DXGI_FORMAT) -> (FormatClass, u32 /*bytes per element*/)`. Typeless
  formats map to their float/unorm sibling. Unknown formats map to `Raw32` with 4 bytes.
- `const HLSL: &str`: one `cs_6_0` kernel with `[numthreads(256,1,1)]`.
  - Root constants: `elements, row_elems, row_pitch, elem_bytes, class, base_offset`.
  - Root SRV `ByteAddressBuffer src`, root UAV `RWByteAddressBuffer partials`.
  - Each thread handles 16 elements: decode channels by `class`, then accumulate per channel
    `min`, `max` and `sum`, plus `zero` (all channels 0), `nan`, `inf`, `count`, and two hashes
    `h1 += mix1(word ^ idx*0x9E3779B9)`, `h2 += mix2(word + idx)` over the raw 32-bit words.
  - Wave reductions (`WaveActiveMin/Max/Sum`), then one lane per wave writes into groupshared;
    thread 0 writes the group's 32-word partial.
- `pub struct Partial { min: [f32;4], max: [f32;4], sum: [f32;4], zero: u32, nan: u32, inf: u32, count: u32, h1: u32, h2: u32 }`.
  `pub fn combine(parts: &[Partial]) -> Stats` gives min/max/mean per channel, the counts and
  `hash = (h1 as u64) << 32 | h2` summed with wrapping. Pure, CPU-side.
- `pub struct Kernel { rs: ID3D12RootSignature, pso: ID3D12PipelineState }` and
  `pub fn kernel(dev: &ID3D12Device) -> Result<&'static Kernel, String>`: compiled once with
  `dxc::compile(HLSL, "main", "cs_6_0", &[])`. XCL-owned; never a title pipeline.
- `pub fn record(list: &ID3D12GraphicsCommandList, k: &Kernel, src_va: u64, partials_va: u64, c: Consts) -> u32 /*groups*/`.

**Tests:**
- `class_of_maps_common_targets` (RGBA16F → (Rgba16Float, 8); R11G11B10 → 4; D32 → D32Float;
  BC7 → Raw32).
- `combine_merges_min_max_mean_and_counts`.
- `combine_hash_is_order_independent` (partials in two orders give the same hash).
- `gpu_kernel_reports_nan_zero_and_constant`: upload a 64×64 RGBA16F buffer with 3 NaN texels,
  100 zero texels and the rest 0.5. Assert nan = 3, zero = 100, min = 0, max = 0.5, and a hash
  that differs after changing one texel. Uses `runner::skip_test("no GPU"|"no DXC")`.

**Acceptance:** the tests pass on the RTX 5080 (GPU test not skipped).

#### I5b. Snapshot capture at pass boundaries (about 380 lines; wave 3)

**Switch:** `XCL_PASS_STATS=<frame>[,<frame>…]` (present counts), only at telemetry tier `pass`
(otherwise warn once and stay off). `XCL_PASS_STATS_MB` (default 1024) is the snapshot memory
budget. `XCL_PASS_STATS_PRE=0` turns off pre-snapshots.

**Files:** new `crates/xcl/src/hle/d3d12x/pass_capture.rs`; `hle/d3d12x/mod.rs` (one line);
`list_x.rs` (`note_render_targets` `:558`: one call; `resource_barrier_x` `:4258`: one call);
`console_list.rs` (`:810`, `:870`: one call each; before `:919`: one call); `bind.rs` (in
`translated_dispatch` before `bind_abi`, `:1043`: one call); `real.rs` (`:2631` UAV view hook:
one call). (`bind.rs`, `console_list.rs` and `real.rs` are also touched by I2a/I2b. The
concurrency plan puts I5b in wave 3 with I2a, which does not edit `bind.rs` or `console_list.rs`,
and only `real.rs` at other functions. See §8: `real.rs` is serialized by giving I5b only the
`:2631` line and landing it after I2a.)

**`pass_capture.rs`:**
- `pub fn armed_now() -> bool`: `present::presents()+1` is in the armed set and tier `pass`.
- Per-list state `Rec { frame, pass: u32, written: Vec<Target>, snapshots: Vec<Snap>, pre_done: HashSet<usize> }`
  in `Mutex<HashMap<usize, Rec>>`, reset by `list_reset` (call it from
  `telemetry_list_reset` in `telemetry_gpu.rs`. **This edit goes to I5c**, which already edits
  `telemetry_gpu.rs`. Until then, a reset is detected by a changed `frame` at the next boundary.)
- `enum Target { Texture { res: usize, kind: Kind /*Rt|Ds|Uav*/, state: D3D12_RESOURCE_STATES }, Memory { va: usize, bytes: usize, res: usize } }`.
- `pub fn note_targets(list, rtvs: &[usize], dsv: Option<usize>)`: resolve handles through
  `present::rtv_resource` (and a DSV twin: add `present::dsv_resource` if not present, the same
  lookup). Kinds Rt (state RENDER_TARGET) and Ds (DEPTH_WRITE). Pre-snapshot each new one.
- `pub fn note_uav_view(handle_slot: usize, res: usize)` from `real.rs:2631`: remember that slot →
  UAV resource (the set of UAV-created slots).
- `pub fn note_dispatch(list, this_state)`: resolve the dispatch's descriptors as
  `trace_fullscreen` does (`bind.rs:936-1000`). Factor the resolution loop into
  `bind::resolve_descriptors(this) -> (Vec<(slot, res)>, Vec<usize /*root VAs*/>)`. That is a pure
  move of the loop body, called from `trace_fullscreen` and here. Slots in the UAV set become
  targets:
  - a texture: `Texture{Uav, UNORDERED_ACCESS}`;
  - a buffer: `Memory{va: real::title_address(res), bytes: desc.Width}`.
  Root VAs become `Memory{va, bytes: 65536, res: 0}`, marked `unresolved`. Other slots are reads:
  stored as `reads: Vec<usize>`. Pre-snapshot new targets.
- `pub fn boundary(list, barriers: &[[u32;8]] /*the title's batch*/)`: if the list has targets
  for the pass that ended:
  - post-snapshot each one, using the `before` state from the batch when the resource is in it,
    else the target's state;
  - then `pass += 1` and clear `written`/`pre_done`.
- **Snapshots** (`fn snap(list, t, when: Pre|Post)`):
  - Texture: `GetCopyableFootprints` of subresource 0 → an XCL default-heap buffer from a pool
    (budget-checked). Transition `state → COPY_SOURCE`, `CopyTextureRegion` to the footprint,
    transition back. MSAA, array > 1 and 3D are skipped with `skipped: "shape"`.
  - Memory: `real::uma_window_buffer(va, bytes)`, then `list_x::transition(list, w,
    COMMON, COPY_SOURCE)`, `CopyBufferRegion` into the pool buffer, and the transition back, as
    `list_x.rs:2232-2234` does. Not in UMA: `skipped: "not_uma"`.
  - Over budget: `skipped: "budget"`.
  - The record: `Snap { pass, res, kind, when, fmt, w, h, row_pitch, buf, offset, bytes, skipped }`.
- `pub fn take(list) -> Option<Rec>`, called at submit by I5c.

**Tests:**
- `armed_frames_parse` (`"120,240"` → {120, 240}; garbage → empty, with a warning).
- `boundary_moves_targets_to_snapshots_and_advances_pass` (with a seam that records snap calls
  instead of recording commands).
- `pre_snapshot_once_per_resource_per_pass`.
- `state_from_barrier_batch_wins` (a batch naming the resource with `before = 0x8` gives a
  UAV-state snap).
- `budget_marks_skipped`.

**Acceptance:** the tests pass. With the switch on and frame N armed, the log says
`"pass stats: frame N armed: {k} snapshots in {lists} lists ({mb} MB)"`.

#### I5c. Stats list, events and images (about 330 lines; wave 4)

**Files:** `pass_capture.rs` (submit side); `telemetry_gpu.rs` (`telemetry_execute`: after
`tel.execute`, call `pass_capture::after_submit(queue, lists, seqs)`; `telemetry_list_reset`: one
call); `crates/xcl/src/gpu/pass_stats.rs` (BMP writer).

**Changes:**
- `after_submit(queue, lists, seq0)`: for lists with a taken `Rec`, record one XCL list (own
  allocator, from a small pool):
  - a UAV barrier on the pool buffers, then per snapshot `pass_stats::record` into a partials
    buffer, then `CopyBufferRegion` of the partials into a readback buffer;
  - `ExecuteCommandLists` on the same queue through the PC's original `ExecuteCommandLists`
    (bypassing hooks, as telemetry does: `QueueFns::orig`), then `Signal` an XCL fence.
  - Hand the job (fence value, readback, snapshot metadata, the list's telemetry seq) to an
    `xcl-pass-stats` thread.
- The thread waits on the fence (`SetEventOnCompletion` + wait). If the device is removed, it
  drops the job. Otherwise it maps the readback, runs `combine`, and writes one `pass_stats` event
  (tier pass) per snapshot: `{frame, list: seq, pass, when: "pre"|"post", res: "0x…", kind,
  format, w, h, min:[..], max:[..], mean:[..], zero, nan, inf, count, hash: "016x", skipped,
  unresolved, reads: ["0x…",…] (post only)}`. It then releases the pool buffers.
- `XCL_PASS_DUMP=1`: for post snapshots of classes Rgba8, Bgra8, Rgba16Float, R11G11B10, R32Float
  and D32Float, at most 64 per armed frame. A second readback copy of the snapshot (in the same
  XCL list) is decoded on the CPU and downsampled to at most 1024 px on the long side. It is
  written as `<run folder>/pass/f{frame}_l{seq}_p{pass}_{res:x}.bmp` (the run folder:
  `crate::log::run_dir()`, `log.rs:321`). Float formats are tone-mapped by clamping to [0,1].
  Depth is shown as grey.
- `pass_stats.rs`: `pub fn write_bmp(path, w, h, rgb: &[u8]) -> io::Result<()>` (24-bit, bottom-up).

**How it avoids changing the frame it measures:** it adds only copies and the state transitions
around them, restored to the state they came from. There are no pipeline, root-signature,
root-argument or descriptor-heap changes on title lists. The statistics run in XCL's own list
after the title's lists, on XCL-owned buffers. **Residual effects (stated in the run output):**
the transitions add GPU synchronisation and can hide a race, and the armed frame is slower. Only
the armed frame is touched. Resubmitted lists and bundles are skipped with `skipped:
"resubmitted"|"bundle"`.

**Tests:**
- `write_bmp_header_is_valid` (a 2×2 image: size field, offset 54, row padding).
- `stats_event_fields_are_fixed` (the field list from a seeded `Stats`).
- `job_dropped_when_device_removed` (a fence seam returning `u64::MAX`).
- A GPU test `gpu_snapshot_of_cleared_target_is_constant`: a small committed RT cleared to 0.25,
  run through `snap` + `after_submit` on a test queue; assert min = max = 0.25 and nan = 0. Skips
  without a GPU.

**Acceptance:** the tests pass. Run R5 (§9) gives `pass_stats` events for the armed frame.

#### I5d. `scripts/dev/pass-stats-check.py` (fork-only, about 200 lines; any wave after the schema)

**Input:** `--run <run folder>` (reads `events.jsonl`); `--frame N`. The script only parses
JSON; no eval. It prints findings as text lines: `FINDING <rule> frame=… list=… pass=… res=…
<detail>`, then a summary count per rule. Exit code 0 always (it reports, it does not judge).

**Rules:**
1. `nan_inf`: a post snapshot with `nan+inf > 0`.
2. `unchanged`: post `hash` == pre `hash` for the same (list, pass, res).
3. `constant`: post `min == max` on all channels, with `count > 1`.
4. `all_zero`: `zero == count`.
5. `read_unwritten`: a resource in `reads` of a pass that has no post snapshot earlier in the
   same frame (any list), and is not in the frame's first pass.
6. `first_constant_ancestor`: for the presented resource (the `res` of the last post snapshot
   whose kind is `rt` in the frame's last list), walk back through earlier passes that wrote it.
   Print the first pass after which it became `constant`.
7. `skipped`: a count of skipped snapshots by reason.

**Tests:** `scripts/dev/test_pass_stats_check.py` (the pattern of `test_smoke_events.py`): a
synthetic events file with one case per rule; asserts that each rule fires once and only there.

#### I6. Run recipe (no code; the orchestrator runs it)

All artefacts go to `C:\xcl-diag\<date>-<n>\` (outside the repo).

1. Elevated console, before the start: `wpr -profiles` (confirm `GPU`), then `wpr -start GPU -filemode`.
2. Second console: `nvidia-smi --help-query-gpu` once to confirm the names, then:
   `nvidia-smi --query-gpu=timestamp,memory.used,memory.total,utilization.gpu,temperature.gpu,power.draw,clocks_event_reasons.active --format=csv -lms 500 > smi.csv`
   (or `clocks_throttle_reasons.active` on older drivers).
3. Run the title with the switches of the run (§9).
4. After the removal or the end: `wpr -stop run.etl`, then `xperf -i run.etl -o run-dump.txt -a dumper`.
5. `wevtutil qe System /q:"*[System[Provider[@Name='nvlddmkm' or @Name='Display' or @Name='Microsoft-Windows-DxgKrnl']]]" /c:30 /rd:true /f:text > events.txt`.
6. `dir C:\ProgramData\Microsoft\Windows\WER\ReportArchive C:\ProgramData\Microsoft\Windows\WER\ReportQueue > wer.txt`
   (look for `LiveKernelEvent_*` and `AppCrash_*` with the run's time).

What each artefact proves:
- `run-dump.txt`: DMA packets per engine around the `device_removed.moment_ms` wall-clock time.
  A packet running ≥ 2 s, or a reset/TDR event, means a Windows timeout. All packets finished,
  with no reset, means a CPU-side removal. (The DxgKrnl event names must be read from the first
  dump.)
- `events.txt`: a Display 4101 event at the time proves an adapter-level TDR. No event is
  consistent with #2403 but not proof.
- `smi.csv`: rules memory exhaustion (memory.used at total) and thermal or power limits in or out
  at the time.
- `wer.txt`: LiveKernelEvent 117/141 at the time proves a timeout.

---

### 8. Concurrency plan

Shared hot spots: `telemetry_gpu.rs`, `telemetry_run.rs`, `real.rs`, `pso_async.rs`, `bind.rs`
(and `console_list.rs`, `dred.rs`, `hle/d3d12x/mod.rs`).

| unit | writes | wave |
|---|---|---|
| I1 | `code_use.rs` (new), `hle/d3d12x/mod.rs`, `dred.rs`, `bind.rs`, `console_list.rs`, `real/dispatch_chain.rs`, `gpu/recomp/pso.rs`, `telemetry_gpu.rs` (one line) | 1 |
| I4 | `gpu/agility.rs`, `real.rs` (`init` debug block, `debug_message`), `telemetry_run.rs`, `docs/guide/cli-and-config.md` | 1 |
| I5a | `gpu/pass_stats.rs` (new), `gpu/mod.rs` | 1 |
| I3 | `real.rs` (`init` DRED block, `report_dred_lists`, `log_device_removed`, `dred_tests`), `dred.rs`, `telemetry_run.rs` | 2 |
| I2a | `removal.rs` (new), `hle/d3d12x/mod.rs`, `real.rs` (execute block, `init` end, `log_device_removed`, `stop_if_device_removed` + callers), `telemetry_run.rs`, `telemetry_gpu.rs` (`telemetry_last_seq`) | 3 |
| I5b | `pass_capture.rs` (new), `list_x.rs`, `console_list.rs`, `bind.rs`, `present.rs` (`dsv_resource`), `real.rs` (`:2631` only), `hle/d3d12x/mod.rs` | 3, **landed after I2a** (`real.rs`, `mod.rs`) |
| I2b | `telemetry_gpu.rs`, `bind.rs`, `console_list.rs`, `culprit.rs`, `real.rs` (`log_device_removed`, execute block) | 4 |
| I5c | `pass_capture.rs`, `telemetry_gpu.rs`, `gpu/pass_stats.rs` | 4, **after I2b** (`telemetry_gpu.rs`) |
| I5d | `scripts/dev/pass-stats-check.py`, its test (dev branch only) | any |

Rule: within a wave, the writers of the same file are serialized. Wave 3: I2a and I5b both write
`real.rs` and `mod.rs`, so I5b lands second. Wave 4: I2b, then I5c. Wave 1 has no overlap
(`real.rs`: only I4; `telemetry_gpu.rs`: only I1; `telemetry_run.rs`: only I4). No unit touches
`pso_async.rs`.

---

### 9. Acceptance runs on the retail title (RTX 5080, driver 621.40)

The same scene start, warm caches, ray tracing on unless said. Each run uses I6. Pass = the
numbers below. The cost baseline: the same run with telemetry `frame` and no new switches
(before the units).

| run | switches | success means |
|---|---|---|
| R1 cost | `XCL_TELEMETRY=frame` (I1 on through the tier), RT off, 300 s | median frame `cpu_ms` within 2 % of the baseline; `measure_ms` total < 2,000 ms; `code` events every frame |
| R2 removal, 512 MB (no profile: `xcl nvprofile remove`) | `XCL_TELEMETRY=pass XCL_DRED=1 XCL_CODE_LIMIT_MB=448`, RT on | one `device_removed` event before `end` with `site`, `moment_ms`, `queues`, `device_state`, `code_in_use_bytes`; `DRED: n lists: …` summary; `last_dispatch` events. **#2403 supported** if `code_in_use_bytes` ≥ 0.9 × 448 MB (≥ 403 MB) at the removal in 2 of 2 runs, and the 75/90/100 % warnings come just before it. **Refuted** if < 270 MB. |
| R3 left-out half | as R2 + leave out the first 500 resolve programs, 230 s | no removal; peak `in_use_bytes` < 403 MB (if a removal-free run peaks above 448 MB, the 448 figure is wrong for this driver: report it) |
| R3b left-out 250 / other 500 | as R3 with those rules (the orchestrator's runs in progress) | the peak predicts the outcome: removal iff the peak crosses about 448 MB |
| R4 profile 1 GiB | as R2 with `xcl nvprofile install --mb 1024`, `XCL_CODE_LIMIT_MB=896`, 230 s | no removal; **or** a removal with `in_use_bytes` ≥ 806 MB. Either supports #2403. A removal below 448 MB with the profile active refutes it. |
| R4d debug layer | `XCL_D3D12_DEBUG=1`, RT off, 120 s of menus + scene | no crash in SDKLayers, or the crash located (test name or `xcl run` stack); `debug_layer` events present |
| R5 pass stats | `XCL_TELEMETRY=pass XCL_PASS_STATS=<first scene frame + 60> XCL_PASS_DUMP=1`, RT off | `pass_stats` events for ≥ 90 % of the armed frame's passes that have a target; the frame after the armed one is within 5 % of the frame before it; the script prints findings and names the first pass whose output becomes constant |

---

### 10. Deliberately left out

- **Shader-side write-address logging** (exact bindless write sets): it changes every shader's
  cache identity (hours of rebuild). It comes only if I5's root-argument resolution misses the
  writes that matter (`unresolved` counts in R5 show that).
- **Graphics read tracking** in I5 v1: the `read_unwritten` rule covers compute reads only.
- **Reading the NVIDIA profile value from inside `code_use`:** the limit is a switch.
  `#2590`'s `status` could feed it later.
- **Aftermath, PIX, RenderDoc, Nsight, WinDbg:** not approved installs.
- **Fixing the code-size wall** (less code in use, releasing unused pipelines): upstream work
  (#2403 "What would fix it"), not instrumentation.
- **U3's rt-role counters:** a ray-tracing feature metric, not removal evidence.
- **GBV on the full title:** too slow (research report §3.1). The switch is there for reduced
  repros.

## 2026-11-26 NVIDIA driver profile: a read-only `xcl nvprofile dump` / `list` (orchestrator decided; the user delegated profile decisions)

### 0. Decision and review

- **User (2026-11-26):** asked to "investigate other GPU profile settings that are typically
  adjusted for games" and to come back with findings; then: "I want you to make the decision
  yourself for now until I say otherwise, just remind me when you have made one. I'm trying to
  get a good starting profile". The shader heap is at 2048 MB by the user's choice.
- **Decision (orchestrator):** before any other setting is changed, read real data from the
  installed driver: what resolves for `xcl.exe`, which predefined profiles set the shader heap
  and to what, and the full predefined profiles of comparable titles. That needs the read-only
  command below. It never writes to the driver and runs unelevated. Testing a further setting
  needs a separate writing change (`install --set`), decided after the dump is read.
- **Measured on the PC today (read only):** the driver's shader cache folder
  (`%LOCALAPPDATA%\NVIDIA\DXCache`) holds 933 files, 25.0 GB; its largest files are 2,048 MB.
- **Research behind it** (outside the repository): candidates in order are the shader heap size
  from a measured peak, the shader disk cache (on/off and size), power management mode,
  background and idle frame limits (check only), and the per-application Resizable BAR policy.
  Low-latency, v-sync, frame limiter, threaded optimisation, CUDA fallback, texture filtering and
  feature overrides were judged not worth testing for a D3D12 process at 2-3 frames a second.
- **Checked by the orchestrator:** `gpu/nvprofile.rs` holds `struct Api`, `api()`, `Session`,
  `status`, `install`, `remove`, `parse`, `run` and the tests named below; the interface ids and
  status codes in the section come from NVIDIA's MIT-licensed headers and are verified on the PC
  by the command's first run (a wrong id fails with "NVAPI has no function").
- **One addition to the section:** the functions that load `ReadApi` use the same
  System32-only loading as `api()` (`LoadLibraryExW` with `LOAD_LIBRARY_SEARCH_SYSTEM32`).
### C. Read-only `xcl nvprofile dump` / `list`: work unit for a writer

Goal: print real profile data from the installed driver. Never calls `SetSetting`, `CreateProfile`,
`CreateApplication`, `DeleteProfile`, `SaveSettings`, or any `Restore*` / `Delete*` function. Runs unelevated.
Target: about 250 changed lines in `crates/xcl/src/gpu/nvprofile.rs` and 1 line of help in `crates/xcl/src/main.rs:53`.

#### C.1 Functions to add to `struct Api` (`nvprofile.rs:91-103`) and `api()` (`nvprofile.rs:121-133`)

Ids from `nvapi_interface.h` (NVIDIA/nvapi `main` `51e5aae`), signatures from `nvapi.h` of the same commit:

| function | id | signature (C) | header line |
|---|---|---|---|
| `NvAPI_DRS_EnumProfiles` | `0xBC371EE0` | `(NvDRSSessionHandle, NvU32 index, NvDRSProfileHandle*)` | nvapi.h 24993 |
| `NvAPI_DRS_GetNumProfiles` | `0x1DAE4FBC` | `(NvDRSSessionHandle, NvU32*)` | 25013 |
| `NvAPI_DRS_GetProfileInfo` | `0x61CD6FD6` | `(NvDRSSessionHandle, NvDRSProfileHandle, NVDRS_PROFILE*)` | 24928 |
| `NvAPI_DRS_EnumApplications` | `0x7FA2173A` | `(hSession, hProfile, NvU32 startIndex, NvU32* appCount, NVDRS_APPLICATION*)` | 25131 |
| `NvAPI_DRS_EnumSettings` | `0xAE3039DA` | `(hSession, hProfile, NvU32 startIndex, NvU32* settingsCount, NVDRS_SETTING*)` | 25223 |
| `NvAPI_DRS_GetSettingNameFromId` | `0xD61CBE6E` | `(NvU32 settingId, NvAPI_UnicodeString* name)` (pointer to `[u16; 2048]`) | 25305 |
| `NvAPI_DRS_GetBaseProfile` | `0xDA8466A0` | `(hSession, NvDRSProfileHandle*)` | 25405 |
| `NvAPI_DRS_GetCurrentGlobalProfile` | `0x617BFF9F` | `(hSession, NvDRSProfileHandle*)` | 24908 |

All `extern "C"` like the existing ones. Keep the read-only functions in a second struct `ReadApi` (or load them lazily
in `dump`/`list`) so an older driver lacking one does not break `install`/`status` – `api()` fails on any missing id
today (`nvprofile.rs:112-115`).

Status code to add: `END_ENUMERATION: Status = -7` (`nvapi_lite_common.h` l.273).

#### C.2 Structures (already in the file; versions)
- `Profile` = `NVDRS_PROFILE_V1`, version `ver::<Profile>(1)` (`NVDRS_PROFILE_VER1`). `gpu_support` is a bitfield u32.
- `Application` = `NVDRS_APPLICATION_V1`, `ver::<Application>(1)`. Enough for names; `EnumApplications` takes an array
  of the same struct, each element's `version` set. (V4 adds `fileInFolder`, flags, `commandLine`: optional, adds 2
  wide strings; not needed.)
- `Setting` = `NVDRS_SETTING_V1`, `ver::<Setting>(1)`, `#pragma pack(4)`. Size stays 12,320 (test at
  `nvprofile.rs:326-330`). **QWORD**: do not add a `u64` member to `Value` (it would raise the union's alignment to 8
  and its size to 4,104, breaking the layout). Read a QWORD as
  `binary.len as u64 | (u32::from_le_bytes(binary.data[0..4].try_into().unwrap()) as u64) << 32`:
  the low dword is at offset 0 (where `len` sits) and the high dword at offset 4. FLOAT: `f32::from_bits(u32)`.

#### C.3 New functions (names are suggestions)

1. `fn profiles(api, s) -> Result<Vec<Handle>, String>`: loop `index = 0..` calling `EnumProfiles`; stop on
   `END_ENUMERATION`; any other non-OK is an error. (`GetNumProfiles` only for a sanity print.)
2. `fn profile_info(api, s, h) -> Result<Box<Profile>, String>`: `GetProfileInfo` with version set.
3. `fn apps(api, s, h, n: u32) -> Result<Vec<String>, String>`: `vec![zeroed Application; n]` with versions set,
   `appCount = n`, `startIndex` advancing by the returned count until `END_ENUMERATION` or `n` reached.
4. `fn settings(api, s, h, n: u32) -> Result<Vec<Box<Setting>>, String>`: same paging with `EnumSettings`. Each element
   is 12 KB; profiles hold tens of settings. Use a `Vec<Setting>` allocated zeroed on the heap
   (`vec![unsafe{zeroed()}; n]` via a loop of `zeroed::<Setting>()` copies is fine).
5. `fn setting_name(api, id, from_struct: &Wide) -> String`: the struct's `name` if non-empty, else
   `GetSettingNameFromId`, else `"(unnamed)"`. Do not embed NPI's name table (keep it small; optional decision).
6. `fn format_value(kind: u32, v: &Value) -> String` (pure): 0 DWORD → `0x%08X (%u)`; 1 BINARY → `len N: ` + first 32
   bytes hex; 2 STRING / 3 WSTRING → UTF-16 up to NUL; 4 QWORD → `0x%016X`; 5 FLOAT → `%g`; other → `type K`.
7. `fn location_name(loc: u32) -> &'static str` (pure): 0 "this profile", 1 "global profile", 2 "base profile",
   3 "driver default" (`_NVDRS_SETTING_LOCATION`).
8. `fn matches(pattern: &str, text: &str) -> bool` (pure): case-insensitive substring.
9. `pub fn dump(target: &str) -> Result<(), String>`: open a session (`Session::open`, which calls `LoadSettings`:
   read-only). Find the profile: first `FindApplicationByName(target)` (an exe), then `FindProfileByName(target)`;
   neither found → error listing that `list <pattern>` exists. Print: profile name, predefined yes/no, number of apps
   and the app names, then one line per setting from `EnumSettings`:
   `0x%08X  <name>  <type>  current=<v>  predefined=<v or ->  <user-set | predefined>  [location]`.
   A setting whose `is_current_predefined == 0` is user-set; `is_predefined_valid == 0` prints predefined as `-`.
   Then a section "resolved for this profile" that calls the existing `get_setting` for a fixed list
   `CANDIDATES: [u32; N]` (the ids in section D: `0x001ABC0F, 0x00198FFF, 0x00AC8497, 0x1057EB71, 0x10835005,
   0x10835016, 0x10835017, 0x10835002, 0x00A879CF, 0x007BA09E, 0x000BFA21, 0x000F00BA, 0x000F00FF`) and prints the value
   and `location_name`. A `SETTING_NOT_FOUND` prints "driver default (not stored)".
   Add the line "values of internal settings may be masked by the driver" once at the end (A.2).
10. `pub fn list(pattern: Option<&str>, setting: Option<u32>) -> Result<(), String>`: for every profile, print
    `name | predefined | apps | settings` when the name or any app name `matches(pattern)`; with `--setting ID`, call
    `get_setting(profile, ID)` and print only profiles where `location == 0` (set in that profile), with the value.
    Also print the base profile (`GetBaseProfile`) and the current global profile name (`GetCurrentGlobalProfile` +
    `GetProfileInfo`) in the header.
11. `parse`: add `Command::Dump(String)`, `Command::List { pattern: Option<String>, setting: Option<u32> }`.
    `--setting` takes hex with or without `0x`. `run` (`nvprofile.rs:295-319`) dispatches them and skips the trailing
    `status` print for these two.

#### C.4 Unit tests (in `nvprofile.rs` `mod tests`)
- `the_command_line_is_checked` (extend): `dump xcl.exe` → `Dump("xcl.exe")`; `dump` alone → error; `list` →
  `List{None,None}`; `list rdr --setting 0x001abc0f` → pattern + `0x001ABC0F`; `--setting 1ABC0F` accepted;
  `--setting zz` and a stray extra argument → error.
- `values_are_formatted_by_type`: DWORD, WSTRING (with NUL), BINARY (len and hex, truncated), QWORD read from the two
  dwords (`0x0000000100000000` for 4 GB), FLOAT, unknown type.
- `locations_are_named`: 0-3 and an out-of-range value.
- `pattern_matching_ignores_case`.
- `struct_sizes_match_nvapi` stays unchanged (proves the QWORD approach did not touch the layout).
- A test that the dump path cannot write: e.g. a `const READ_ONLY_IDS` list checked against the ids used by
  `ReadApi`, asserting none of `0x577DD202` (SetSetting), `0xFCBC7E14` (SaveSettings), `0xCC176068`, `0x4347A9DE`,
  `0x17093206` is in it.
The DRS calls themselves are not mocked (as in PR #2590); the PC run is the integration test.

## 2026-11-26 Statistics overlay (HUD) in the top-left corner of the window (user asked for it; orchestrator reviewed)

### 0. Approval and orchestrator review

- **User (2026-11-26):** "include runn stats on the top left of our window. I want it to show
  all basic stats. RAM used, CPU percentage. Graphics card, FPS. Any useful information to me on
  the top left of the window with nice live graphs. Sleek and modern looking, and ultra light
  weight. Please have another agent work on this and send me a screenshot when done".
- **Orchestrator decisions:** the design below is built as written. Default `hud = "full"` (the
  user wants to see it), `XCL_HUD` overrides, F3 cycles full / compact / off. GPU utilisation
  stays out of the first version (unit U8, deferred until its cost is measured). Upstream PR #450
  (an on-screen panel drawn on the CPU and copied in: opaque, no graphs, 8-bit only, conflicting)
  is not adopted because it does not meet the request; the user is told.
- **U0 is narrowed** so that it can be written at the same time as wave 1: it creates
  `hud/mod.rs`, the `mod` line in the d3d12x `mod.rs`, and one-line placeholder files only for
  `hud/layout.rs`, `hud/sample.rs` and `hud/draw.rs`. The wave 1 units create `hud/font.rs`,
  `hud/model.rs` and `hud/text.rs` themselves.
- **Citations spot-checked at db3bc937:** `hle/d3d12x/present.rs:1007-1013` (the swap chain is
  made with `DXGI_SCALING_STRETCH`), `present.rs:373-378` (`wndproc`), `hle/d3d12x/pso_async.rs:3182-3185`
  (`counts` walks the pending map under its lock). All hold.
- **The bitmap font** (95 glyphs, 5x9) was drawn for this project by the designer; nothing is
  taken from a third-party font.
- Nothing here was built or run on macOS; every unit is built and tested on the Windows PC.

Status: draft for the orchestrator's review. Written on macOS from the source of branch `dev`
(head `b3527fd4`); nothing was built or run. Every `path:line` is on that commit.

### 1. Summary and recommendation

Draw a small translucent panel in the top-left corner of XCL's own swap-chain back buffer, after
the title's frame has been copied (or composited) into it and after any XCL frame-dump readback,
just before the back buffer goes to `PRESENT`. The panel is one instanced draw of quads on XCL's
present command list, from an upload buffer that is mapped once. Quads are of three kinds: a
rectangle, a rounded rectangle (signed distance in the pixel shader) and a glyph of a 5x9 bitmap
font written for this project (the bits sit in the pixel shader as a constant array, so there is no
texture and no descriptor heap). A sampler thread (`xcl-hud`, 4 Hz) reads CPU, memory and video
memory; the present path only pushes a time stamp per shown frame into a lock-free ring. Layout is
rebuilt only when the sampler publishes (4 Hz) or the frame size changes; other frames copy about
40 KB of cached quads and record about 12 D3D12 calls.

Recommendation on prior art (decision A): do **not** adopt upstream PR #450; write our own (section
3.1).

Default: `hud = "full"` in `xcl.toml`, `XCL_HUD` overrides, F3 cycles full / compact / off.

Decisions at a glance:
- A, prior art: PR #450 exists (CPU-drawn opaque panel, copied); not adopted (3.1).
- B, content: 13 rows in full mode, 4 figures and one graph in compact; GPU utilisation left out
  of v1 (4, 4.3).
- C, drawing: one instanced draw of quads, font bits in the pixel shader, recorded after the
  title's frame and any frame dump (5, 7, U3 10.5, U5 10.7).
- D, sampler: `xcl-hud` thread at 4 Hz (pipeline counts at 1 Hz), lock-free frame stamps, fixed
  buffers (4.1, 4.2, U4 10.6).
- E, switches: `hud` / `XCL_HUD` = full (default), compact, off; F3 cycles (8).
- F, tests: 60 named tests, two of them GPU tests on WARP (9 and each unit).
- G, docs: one README bullet, one Controls sentence, two rows in the fork's config guide (9).

Full panel at a 1080-line frame (1 dot = 2 px; panel 180 x 193 dots = 360 x 386 px):

```
.------------------------------------------------.
| XCL ScarlettGame                     00:12:34  |
| -----------------------------------------------|
| FPS  2.4           |      ||   |  |   ||  ||  ||   <- frame-time graph (cyan)
|      412 ms        |  |||||||||||||||||||||||| |
| CPU  38%           |  ....::::::::::::........ |   <- system (dim) + XCL (bright), violet
| sys  61%           |  ::::::::::::::::::::::::||
| RAM  9.8 GB        |  ======================== |   <- system used (dim) + XCL (bright), green
| sys  27.1/63.9G    |  ======================== |
| VRAM 15.1 GB       |  -------budget----------- |   <- amber; value red when over budget
| of   14.6 GB       |  ######################## |
| -----------------------------------------------|
| GPU    NVIDIA GeForce RTX 50                   |
| Commit 14.2 GB                                 |
| PSO    55,223 queued                           |
| Failed 12                                      |
| DXC    2 run, 46 done                          |
| Res    3840x2160 > 1280x720                    |
| RT     off                                     |
'------------------------------------------------'
```

(`GPU` is cut at 21 characters; the `Failed` row appears only when a pipeline failed, `Code ... MB`
and `GPU ms ...` only when their sources are on, section 4. The panel above has 7 detail rows,
193 dots tall; without `Failed`, 183.)

Compact panel (168 x 42 dots = 336 x 84 px at 1080 lines):

```
.-------------------------------------------.
| FPS 2.4   412 ms      |  |  ||  | ||  ||  |
| CPU 38%  RAM 9.8G     | ||||||||||||||||| |
| VRAM 15.1/14.6G       | ||||||||||||||||| |
'-------------------------------------------'
```

### 2. Goal and scope

Goal: while a title runs, the user sees at a glance how fast it presents, what XCL costs the
machine (CPU, RAM, video memory against its budget), which GPU runs it and whether pipelines are
still being built, with short live graphs, at a cost that does not show in the frame time.

In scope:
- the panel (full and compact), its sampler thread, its switches (`hud`, `XCL_HUD`, F3);
- 8-bit, 10-bit and FP16 swap chains (all formats `swap_format` gives, `present.rs:283`);
- unit tests of every pure part, two GPU tests on WARP;
- README line and the fork's config table rows.

Out of scope (section 13): GPU utilisation in v1 (a measured follow-up, unit U8), drawing when the
title does not present, a mouse-driven UI, per-pass or per-queue timings, an always-on GPU timer.

### 3. What exists today

#### 3.1 Prior art upstream (decision A)

`gh pr list -R xcl-dev/xcl --search overlay|HUD|on-screen|"fps counter"|stats|osd --state all` and the
same for issues found:

- **PR #450** (open, `CONFLICTING`, head `093f600`, 361 added lines: `hle/overlay.rs` 266,
  `present.rs` +86, `host.rs` +3, `README.md` +5): `XCL_OVERLAY=1` renders a panel **on the CPU**
  into BGRA/RGBA pixels with a 5x7 font and `CopyTextureRegion`s it to (8, 8) of the back buffer.
  Off by default, refreshed once a second, text only, opaque, no graphs; 10-bit and FP16 back
  buffers draw nothing. Written with "GPT-6.2 Astra"; reviewers found a back-buffer reference leak
  and an unaligned pitch (fixed in `cd41749`). Last comment (2026-11-25): conflicts with `main`.
- PR #516 (merged): the FPS in the window title (`present.rs:442`-`478`). PR #482 closed duplicate.
- No issue asks for an overlay. Issue #933 (closed) is about CapFrameX's injected overlay.

Adopt #450 or write our own? Write our own:
- A copy cannot blend: the user asked for a translucent, "sleek" panel with live graphs; a copy
  replaces pixels and would need the CPU to rasterise graphs too, every refresh.
- It skips 10-bit and FP16 frames; ours draws on all of `swap_format`'s formats.
- It does not apply to `dev`: it conflicts with upstream `main`, and `dev`'s `present.rs` has 163
  more commits; bringing it through `scripts/dev/merge-pr.py` means resolving `present.rs` anyway.
- What it teaches is kept: a child window does not work under flip-model presentation (the
  description says the flip draws over it), copy locations must borrow with `transmute_copy`
  rather than `ManuallyDrop::new(Some(x.clone()))`, and placed footprints need 256-byte pitches.
  We use none of these paths (no child window, no texture copy), so none of its bugs can recur.
- No code is taken from it.

#### 3.2 The window and its title bar

- The window is made on its own thread `xcl-window` (`present.rs:569`-`582`) by `window_thread`
  (`present.rs:481`-`528`): class `XCLGameWindow`, size from `cfg.window_size()`
  (`config.rs:279`), `kbm::attach` (`present.rs:520`), a 500 ms timer (`present.rs:521`).
- The title text "XCL – <game> | N FPS | compiling shaders: ..." is built by `show_compiling`
  (`present.rs:445`-`478`) on that timer (`present.rs:391`-`394`), from `FrameRate::sample`
  (`present.rs:419`-`437`), `pso_async::counts()` and `dxc::compile_counts()`. It stays.
- The process is not DPI-aware (no manifest or `SetProcessDpiAware*` anywhere in `crates/`), so
  client sizes are 96-DPI logical pixels and Windows scales the whole window. The HUD needs no DPI
  code.

#### 3.3 Input

- `wndproc` (`present.rs:374`-`396`) first gives every message to `kbm::on_message`
  (`kbm.rs:299`-`351`), which handles raw mouse input, focus, sizing and the cursor only; key
  messages fall through to `DefWindowProcW`.
- The game's keyboard-as-gamepad reads keys by polling `GetAsyncKeyState` (`kbm.rs:354`-`357`,
  `kbm.rs:392`-`445`). Handling `WM_KEYDOWN` in the window procedure therefore takes nothing from
  the game. Mapped keys are WASD, E, Q, Shift, Ctrl, the mouse, Space, Enter, Escape, Backspace,
  arrows, Tab and P (`kbm.rs:6`-`16`); no function key is used. F3 is free.

#### 3.4 How a frame reaches the swap chain

- `present_now` (`present.rs:1033`-`1257`) runs on the title's presenting thread under the
  `PRESENTER` mutex (`present.rs:937`, `present.rs:1046`-`1056`).
- The swap chain is sized to the **title's frame**, not the window: `ensure_swapchain(p, hwnd, w,
  h, fmt)` with the frame's `w`, `h` (`present.rs:1087`-`1094`, `present.rs:1111`) and
  `DXGI_SCALING_STRETCH` (`present.rs:1011`), so DWM stretches the back buffer to the client
  area. `DXGI_ALPHA_MODE_IGNORE` (`present.rs:1013`). Three buffers (`present.rs:592`).
- Format: `swap_format` (`present.rs:283`-`291`): R8G8B8A8_UNORM, B8G8R8A8_UNORM,
  R10G10B10A2_UNORM or R16G16B16A16_FLOAT. No `SetColorSpace1` call exists, so the default colour
  space applies (sRGB-encoded for UNORM, linear scRGB for FP16).
- XCL records on its own list `p.list` (`present.rs:1116`), executed on the title's queue
  (`present.rs:1217`-`1220`). One plane: `CopyResource` or `CopyTextureRegion` into the back
  buffer, then `COPY_DEST -> PRESENT` (`present.rs:1159`-`1190`). Several planes: the compositor
  draws into the back buffer (`present.rs:1122`-`1152`, `Compositor::record` `present.rs:845`-`901`),
  then `RENDER_TARGET -> PRESENT`, or `-> COPY_SOURCE` for the `XCL_FRAME_DUMP` readback and then
  `-> PRESENT` (`present.rs:1138`-`1147`).
- The compositor is the model for XCL-owned drawing: its own root signature, PSOs per format,
  RTV heap, FXC-compiled shaders (`compile_fx` `present.rs:678`-`700`, `D3DCompile` vs_5_0/ps_5_0)
  and premultiplied blending (`present.rs:827`-`835`). It touches no title state: it binds only its
  own objects on XCL's own list.
- Command-allocator slots: `prepare_pipelined_slot` (`present.rs:647`-`666`) waits for the slot's
  fence before reuse; `p.ring.cur` is the slot in use, and the compositor indexes its per-slot
  descriptors by it (`present.rs:856`). With `XCL_PRESENT_SYNC=1` there is one slot
  (`present.rs:639`-`645`).
- `frame_shown(w, h)` (`present.rs:1259`-`1268`) runs once per shown frame, also on the
  telemetry bypass paths (`present.rs:1095`, `present.rs:1100`).
- `Drop for Presenter` waits for the last fence before its fields are released
  (`present.rs:1488`-`1497`), so GPU objects kept in the presenter outlive their last use.
- GPU tests of present code already run on WARP with no skip: `compositor_blends_ui_plane_over_scene_plane`
  (`present.rs:1714`-`1854`), using `readback_copy` (`present.rs:1307`).

#### 3.5 The figures and where they come from

| Figure | Existing source | Cost / notes |
|---|---|---|
| Frames shown | `PRESENTS` (`present.rs:44`), `frame_shown` | one atomic |
| FPS in title bar | `FrameRate` (`present.rs:413`-`437`) | 1 s window, coarse at 2 fps |
| Process commit | `K32GetProcessMemoryInfo` `PrivateUsage` (`host.rs:92`-`98`, `telemetry_gpu.rs:3256`-`3262`) | microseconds |
| System RAM | `GlobalMemoryStatusEx` (`report.rs:406`-`414`, `uma.rs:816`) | microseconds |
| Video memory + budget | `real::video_memory()` (`real.rs:433`-`446`): `QueryVideoMemoryInfo` local and non-local | creates a DXGI factory on every call; the HUD caches the adapter instead |
| `memory:` log line | `host.rs:91`-`117`, only with `XCL_ACTIVITY`, every 2 s | — |
| Adapter name | `gpu::adapter::name(&DXGI_ADAPTER_DESC1)` (`adapter.rs:119`-`122`) | once |
| Pipelines | `pso_async::counts()` -> (built, failed, pending) (`pso_async.rs:3182`-`3185`) | locks `PENDING` and walks every entry, locking each `done`: O(pending), 55 223 entries in the user's run. The title bar already calls it every 500 ms |
| Pipeline queue gauges | `pso_async::queue_gauges()` (`pso_async.rs:912`-`934`), `real::gate_gauges()` (`real.rs:4308`-`4317`) | cheap, but "lanes" is not the title bar's "queued"; not used |
| DXC compiles | `dxc::compile_counts()` (`dxc.rs:1059`-`1061`) | two atomics |
| Driver code in use | `code_use` (`code_use.rs:1`-`8`), counted only when `on()` (`code_use.rs:25`-`28`: `XCL_CODE_IN_USE=1` or telemetry tier `frame`); limit `XCL_CODE_LIMIT_MB` (448) (`code_use.rs:31`-`34`) | no public getter; one is added (U4) |
| Ray-tracing chain | `Config::raytracing_chain_mode()` (`config.rs:271`-`275`), `ChainMode` (`config.rs:313`) | once |
| GPU time per frame | `frame_facts` computes `gpu_ms` (`telemetry_gpu.rs:3166`-`3213`), written by `write_frames` (`telemetry_gpu.rs:3217`-`3221`) | only at telemetry tier `frame` and up (`telemetry_gpu.rs:2910`-`2925`); not always available. Optional U7 exposes the last value |
| CPU times | `GetSystemTimes` used once (`kernelx.rs:1673`); `GetProcessTimes` nowhere | new |
| GPU utilisation | none | see 4.3 |

`windows-sys` already enables `Win32_System_Threading`, `Win32_System_ProcessStatus`,
`Win32_System_SystemInformation` and `Win32_System_Performance` (`crates/xcl/Cargo.toml`), so
`GetProcessTimes`, `GetSystemTimes`, `GetActiveProcessorCount`, `K32GetProcessMemoryInfo`,
`GlobalMemoryStatusEx` and PDH need no Cargo change. `windows` has Direct3D12, DXGI, Fxc and
WindowsAndMessaging (`GetClientRect`, `WM_KEYDOWN`).

#### 3.6 Other instruments that look at the final image

- `XCL_FRAME_DUMP`: one plane reads the title's resource (`present.rs:1183`-`1187`); several
  planes read the back buffer after compositing (`present.rs:1138`-`1145`). The HUD is recorded
  after both, so dumps never contain it.
- `XCL_TRACE_FRAME` readbacks (`present.rs:1193`-`1213`) copy the title's render targets, not the
  back buffer: unaffected.
- `XCL_PASS_STATS` / pass capture work on the title's command lists: unaffected.
- Outside screenshot tools (PrintWindow, the PC's `pcshot`) see the window, so they see the HUD.
  For a clean shot: `XCL_HUD=off` or press F3 until it is off.
- The title never reads XCL's swap-chain buffers, so the HUD cannot appear in anything it reads.

### 4. What it shows (decision B)

Order, top to bottom, full mode. "Tick" = one sampler wake-up (250 ms).

| # | Row | Value | Source (exact) | Rate | Cost |
|---|---|---|---|---|---|
| 1 | header | `XCL`, title name, run time `HH:MM:SS` | name passed by `window_thread` (`window_name()`), run time = `now_us()` since the sampler started | tick | none |
| 2 | FPS group | FPS (coloured), newest frame interval, graph of the last 72 frame intervals | time stamps pushed by `frame_shown` into `FrameStamps`; `model::fps`, `sample::new_intervals` | tick | one `Instant::now()` + two atomics per frame |
| 3 | CPU group | XCL process % of the whole machine, system %; graph of both | `GetProcessTimes(GetCurrentProcess())` kernel+user, `GetSystemTimes`, `GetActiveProcessorCount(0xFFFF)`; `model::cpu_percent` | tick | ~2 µs |
| 4 | RAM group | process working set; system used / total; graph of both against total | `K32GetProcessMemoryInfo` (`PROCESS_MEMORY_COUNTERS_EX.WorkingSetSize`, `.PrivateUsage`), `GlobalMemoryStatusEx` (`ullTotalPhys - ullAvailPhys`, `ullTotalPhys`) | tick | ~5 µs |
| 5 | VRAM group | local video memory in use (red when over budget); budget; graph with a budget line | cached `IDXGIAdapter3::QueryVideoMemoryInfo(0, LOCAL)`: `CurrentUsage`, `Budget` | tick | ~10-30 µs (kernel thunk) |
| 6 | GPU | adapter name, first 21 characters | `IDXGIAdapter1::GetDesc1` -> `gpu::adapter::name` | once, when the device exists | — |
| 7 | Commit | process commit | `PrivateUsage` (row 4's call) | tick | — |
| 8 | PSO, Failed | `N queued`; a `Failed M` row (red) only when M > 0 | `pso_async::counts()` `.2` and `.1` (same numbers as the title bar) | every 4th tick (1 Hz) | O(pending) under `PENDING`: see 7.1 |
| 9 | DXC | `R run, D done` | `dxc::compile_counts()` | 1 Hz | atomics |
| 10 | Code (only when counting is on) | `312 / 448 MB`, amber at 75 %, red at 90 % | new `code_use::in_use()` (U4) | 1 Hz | two atomics |
| 11 | GPU ms (only at telemetry tier `frame`+) | last frame's GPU time | new `report::last_gpu_ms()` (U7, optional) | 1 Hz | one atomic |
| 12 | Res | `<frame w>x<h> > <client w>x<h>` | frame size = the back buffer's (`record`'s `w`, `h`); `GetClientRect(hwnd)` from the sampler thread | 1 Hz | ~1 µs |
| 13 | RT | `on` / `off` | `state::config().raytracing_chain_mode()` | once | — |

Compact mode: FPS and frame interval; CPU process %; RAM working set; VRAM used / budget; one
graph (frame intervals, the newest 48).

Left out, and why:
- **GPU utilisation** in v1: see 4.3; a follow-up unit after a measurement.
- **Frame-time percentiles (1 % low)**: at 2-3 fps a 72-frame window is half a minute; the graph
  shows the spread better.
- **Draws / dispatches per second, DirectStorage throughput**: diagnostic counters for
  `XCL_ACTIVITY` logs (`host.rs:44`-`67`), not "basic stats"; they would widen the panel.
- **Non-local video memory**: visible in the `memory:` log line; one figure (local vs budget) is
  what decides spilling.
- **Pipeline lanes and gates** (`queue_gauges`): numbers that do not match the title bar's.
- **Frame clock target interval**: rarely changes; the FPS colour already says how far off 60 it is.
- **Temperatures, clocks, fan**: vendor APIs only (NVML, ADLX): not allowed without a capability
  check and not "ultra light".

#### 4.1 FPS (exact rule, `model::fps`)

Input: up to 128 frame time stamps in microseconds, newest first; `now` in microseconds.
1. `n1` = number of stamps in `(now - 1_000_000, now]`. If `n1 >= 10`, FPS = `n1` (shown as an integer).
2. Otherwise take the stamps in `(now - 5_000_000, now]`, at most the 5 newest. Fewer than 2: FPS = 0.
3. `mean` = (newest - oldest of those) / (count - 1); `gap` = now - newest.
4. FPS = 1 000 000 / max(mean, gap) (shown with one decimal below 10).

This is steady at 2-3 fps, falls while the title stalls, and needs no allocation.

#### 4.2 CPU percent (exact rule, `model::cpu_percent`)

Two samples `a`, `b` of `CpuTimes { wall, proc, idle, kernel, user }` (all in 100 ns units; `wall`
from the sampler's `Instant`, `proc` = process kernel + user, `idle`/`kernel`/`user` from
`GetSystemTimes`, where kernel includes idle):
- process % = `(b.proc - a.proc) / ((b.wall - a.wall) * cpus) * 100`, clamped to 0..100; 0 when the
  wall delta is 0.
- system % = `(1 - (b.idle - a.idle) / ((b.kernel - a.kernel) + (b.user - a.user))) * 100`, clamped
  to 0..100; 0 when the denominator is 0.
- All subtractions saturate.

#### 4.3 GPU utilisation: PDH or nothing

Vendor-neutral source: the Windows performance counters Task Manager uses, `\GPU Engine(*)\Utilization
Percentage` (instances `pid_<pid>_luid_<hi>_<lo>_phys_0_eng_<n>_engtype_<type>`) and `\GPU Adapter
Memory(*)\Dedicated Usage`, through PDH (`PdhOpenQueryW`, `PdhAddEnglishCounterW`,
`PdhCollectQueryData`, `PdhGetFormattedCounterArrayW`), Windows 10 1709+.

Tradeoffs:
- For: no vendor API; per process; matches Task Manager.
- Against: the wildcard re-enumerates every engine of every process on each collection (dozens to
  hundreds of instances); its cost on this PC is unknown and is reported in the literature as
  milliseconds; a rate counter needs two collections before the first value; the instance list
  changes when processes start; `PdhGetFormattedCounterArrayW` needs a buffer whose size changes
  (allocate once with headroom, refuse to grow on the fly). "GPU Adapter Memory" duplicates what
  `QueryVideoMemoryInfo` already gives cheaply.

Recommendation: **not in v1**. U8 (deferred) adds it at 1 Hz on the sampler thread only if a probe
on the PC measures `PdhCollectQueryData` under 2 ms with the title running; the GPU row then reads
`NVIDIA GeForce RTX 5080  87%`, the value being the maximum over the process's engines of type
`3D` / `Compute` (summed per engine, then max over engines). If PDH fails at any step, the row
shows the name only and PDH is not retried.

### 5. Architecture and data flow

New module `crates/xcl/src/hle/d3d12x/hud/`:

| File | Unit | Contents |
|---|---|---|
| `mod.rs` | U0, U6 | module list; mode; `start`, `note_frame`, `on_key`, `active`, `record`; the cached layout |
| `font.rs` | U1 | the 95-glyph table, `glyph_index`, `packed` |
| `model.rs` | U2a | `Mode`, `Inst`, `rgba`, `Ring`, `FrameStamps`, `fps`, `CpuTimes`, `cpu_percent`, `nice_ceiling`, `column_height`, `Snapshot` |
| `text.rs` | U2b | `FixedText`, number formatting, `Statics` |
| `layout.rs` | U3 | `InstBuf`, `dot_px`, colours, `build` (snapshot -> quads) |
| `sample.rs` | U4 | `Shared`/`SHARED`, `now_us`, the sampler thread |
| `draw.rs` | U5 | `HudDraw`: root signature, PSOs, upload ring, `record`; HLSL |

```
 title thread (present_now, under PRESENTER)          xcl-hud thread (4 Hz)            xcl-window thread
 ------------------------------------------           ---------------------            -----------------
 frame_shown --push t_us--> SHARED.frames  -------> new_intervals, fps               WM_KEYDOWN F3
                                                       GetProcessTimes/GetSystemTimes   -> hud::on_key
                                                       K32GetProcessMemoryInfo          -> MODE.store
 after copy/composite/dump:                            GlobalMemoryStatusEx
   hud::record(dev, list, bb, w, h, fmt, slot)         QueryVideoMemoryInfo (cached)
     try_lock SHARED.snapshot --(gen changed?)-->      1 Hz: counts(), compile_counts(),
       layout::build into CACHE.insts                        code_use::in_use(), GetClientRect
     HudDraw::record: memcpy quads to upload[slot]     --lock--> SHARED.snapshot = snap (gen+1)
       RTV, root sig, root SRV, PSO, DrawInstanced(6,n)
 -> barrier to PRESENT, ExecuteCommandLists, Present
```

Rules that keep the title's frame safe:
- The HUD binds only its own root signature, PSO, RTV and buffer, on XCL's present list `p.list`,
  after XCL's own copy into XCL's own back buffer. It never sees a title list, root argument,
  descriptor heap or resource. The next title list sets its own state (D3D12 command lists do not
  inherit state).
- It never blocks: `try_lock` on the snapshot (keeps the last layout when busy), no fence wait (the
  upload region of slot `p.ring.cur` is free because `prepare_pipelined_slot` already waited for
  that slot, `present.rs:647`-`666`), no allocation after the first frame.
- It never ends a frame early: every HUD failure is a `false` return; `present_now` continues to the
  barrier to `PRESENT` and `Present` exactly as today.
- A HUD that fails to build its root signature or PSO logs one warning and turns itself off for the
  rest of the run (`FAILED` flag).

### 6. Dependencies

None new. `windows` 0.62 and `windows-sys` 0.61 features already enabled cover every call (3.5).
FXC (`D3DCompile`, `d3dcompiler_47.dll`, part of Windows) compiles the two small shaders, as the
compositor does; DXC is not needed. No font, UI or graphing crate; the glyphs are authored in this
design (10.2), not taken from any font.

### 7. Constraints and considerations

#### 7.1 Cost

Per shown frame, present thread (target < 0.1 ms CPU, one draw):
- `note_frame`: `Instant::now()`, one store and one `fetch_add`.
- `record`: one `try_lock`; layout rebuild only when the generation, mode or frame size changed
  (at most 4 times a second, ~1 300 quads, estimated 20-40 µs); `memcpy` of n x 32 bytes
  (~40 KB, ~3 µs); `CreateRenderTargetView`, 2 barriers, ~10 list calls, one `DrawInstanced(6, n)`.
- GPU: ~1 300 small quads over ~360 x 370 px: a few microseconds.
- Once per run: FXC compiles the two shaders on the window thread at start (`draw::warm`), and the
  first HUD frame creates the PSO for the back buffer's format (one small driver compile).
- U6 logs, once after 600 drawn frames, the mean and maximum time of `record` so the cost is a
  measured number (section 12).

Sampler thread: ~50 µs per tick plus `pso_async::counts()` once a second. `counts()` walks every
pending build under the `PENDING` lock (`pso_async.rs:3183`), and title threads that create
pipelines take that lock; the title bar already pays this twice a second. To be checked on the PC
(claim): if the 1 Hz call shows in the title's frame time, U6 reads it at 0.5 Hz instead.

#### 7.2 Back-buffer size, window size and scale

The back buffer has the frame's size and is stretched to the window (3.4). The HUD is laid out in
**dots**: 1 dot = `P` back-buffer pixels, `P = max(1, round(frame_h / 540))` (`layout::dot_px`):
360-809 lines -> 1, 810-1349 -> 2, 1350-1889 -> 3, 2160 -> 4. Because `P` is an integer, glyph
edges are sharp in the back buffer; the panel always covers the same fraction of the window
(about 19 % of a 16:9 width and 34 % of the height in full mode; 18 % x 8 % compact), whatever the
window size. A window whose aspect differs from the frame's stretches the HUD as it stretches the
game (accepted). At 720 lines `P = 1` and caps are 7 px of a 720-line frame: small but readable;
on a 1280x720 window with a 1080-line frame they are 9 px.

#### 7.3 HDR and 10-bit back buffers

- UNORM 8-bit and 10-bit: colours are written sRGB-encoded (the default colour space treats the
  values as sRGB); blending happens in that space, as UI usually does.
- FP16: the default colour space for FP16 swap chains is linear scRGB, so the pixel shader converts
  each colour from sRGB to linear (exact piecewise curve) before premultiplying. White is 1.0
  (80 nits): on an HDR display the HUD looks dimmer than SDR white (accepted; no paper-white
  setting in v1).
- One PSO per format, made on first use (as `Compositor::psos_for`, `present.rs:803`-`849`).

#### 7.4 Failure modes

| Failure | Behaviour |
|---|---|
| FXC compile, root signature or PSO creation fails | warn once, `FAILED = true`, `record` returns false for the rest of the run |
| upload buffer creation or `Map` fails | same |
| snapshot lock busy | last layout is drawn again |
| no snapshot yet (first 250 ms) | panel drawn with zeros and `--` for names (EMPTY snapshot) |
| more than 2048 quads | the rest are dropped (`InstBuf::push` returns false) |
| device not created yet when the sampler starts | GPU name stays empty, retried every tick |
| `GetClientRect` fails or the window is minimised (0 x 0) | `Res` shows only the frame size |
| title stops presenting | nothing is drawn (the HUD is part of the title's frames); the title bar still updates |
| device removed | `present_now` already disables itself (`present.rs:1102`-`1107`); the HUD draws nothing |
| `XCL_HUD=off` / `hud = "off"` | no thread, no PSO, no upload buffer; `note_frame` returns at once |

### 8. Switches (decision E)

- `xcl.toml`: `hud = "full"` (new key, default `"full"`). Values `off`, `compact`, `full`.
- `XCL_HUD`: same values, overrides `xcl.toml`. Spellings (ASCII case-insensitive, trimmed):
  `off`/`0`/`false`/`no`/`none` -> Off; `compact`/`1`/`small` -> Compact; `full`/`2`/`on`/`true`/`yes` -> Full.
  Anything else: warn once (`hud: "<v>" is not off, compact or full – using full`) and use Full.
- Default **full**: the user asked to see all of it; the file default is written into new
  `xcl.toml` files and an older file without the key gets it by `#[serde(default)]`. For an
  upstream PR the maintainer may want `off` (PR #450 was off by default): a one-word change in
  `Config::default` (claim for the orchestrator).
- **F3** cycles Full -> Compact -> Off -> Full while the XCL window has the focus. It is safe because
  the game reads keys by polling `GetAsyncKeyState` (3.3): the window procedure handles
  `WM_KEYDOWN` with `wParam == 0x72` and bit 30 of `lParam` clear (not an auto-repeat) and returns 0;
  the key's state for `GetAsyncKeyState` is unchanged, and no game mapping uses F3. When the
  configured mode is Off, nothing is created and F3 is not handled (passes to `DefWindowProcW`).

### 9. Tests (decision F) and docs (decision G)

Tests by name are listed in each unit (section 10). Summary:
- font: coverage of all 95 printable ASCII characters, 5-bit rows, distinct glyphs, packing.
- model: ring, frame stamps, FPS (high rate, low rate, stall, empty), CPU percent, graph scale,
  column heights, modes, `Inst` size and colour packing.
- text: truncation, non-ASCII, each formatter.
- layout: dot size, panel geometry in both modes, glyph advance, colour rules, absent rows,
  capacity, every character the layout prints has a glyph.
- sample: interval extraction, FILETIME conversion, VRAM scale; `code_use::in_use`.
- draw: generated HLSL font table; two GPU tests on WARP (RGBA8: panel, rounded corner, glyph
  pixels; FP16: linear output). WARP is always present on Windows, as in
  `compositor_blends_ui_plane_over_scene_plane`; if WARP device creation fails the tests call
  `gpu::recomp::runner::skip_test("no GPU")` and return.
- wiring: mode resolution, F3 key filter, tail barriers per present path, config default.

Docs (U6):
- `README.md`, in the "Switches for how a title runs" list (after the `XCL_FRAME_INTERVAL_US`
  bullet, `README.md:257`-`260`), one bullet:
  `- XCL_HUD (or hud in the game's xcl.toml): the statistics panel in the top-left corner of the
  window – frames per second and frame time, CPU, memory, the graphics card and its video memory,
  pipelines being built, with live graphs. full (default), compact or off; F3 cycles them while
  the window has the focus. XCL's frame dumps never contain it; screenshots of the window do.`
  And in Controls (`README.md:182`-`186`), append: `F3 cycles the statistics panel (full, compact, off).`
- `docs/guide/cli-and-config.md` (fork-only file): a row in the `xcl.toml` table after
  `mouse_sensitivity` (`cli-and-config.md:179`):
  `| hud (crates/xcl/src/config.rs:<line>) | "full" | statistics panel in the window: "full", "compact" or "off"; XCL_HUD overrides; F3 cycles it |`
  and a row in "Settings that override xcl.toml" (`cli-and-config.md:249`):
  `| XCL_HUD | crates/xcl/src/hle/d3d12x/hud/mod.rs:<line> | Statistics panel: off, compact or full. Overrides hud. | hud from xcl.toml (full) |`
  (the writer fills `<line>` and matches the table's existing columns exactly).

### 10. Work breakdown

All paths below are under `crates/xcl/src/hle/d3d12x/` unless they start with `crates/`, `README` or
`docs/`. Every unit: no `cargo fmt` (format only the lines you write, in the style of the file
around them), no new dependency, tests in the file's `#[cfg(test)] mod tests`, English comments in
the repository's style (short `//!` module doc, `///` on public items). The crate builds only on
Windows: a writer on macOS writes the code and the orchestrator builds and runs the tests on the PC.

Shared constants used by several units (defined where noted, used as is by the others):
- `model::MAX_INST = 2048`, `model::HISTORY = 72`, `model::STAMPS = 128`, `model::GIB = 1073741824.0_f32`.
- `font::COUNT = 95`, `font::FIRST = 0x20`.

#### 10.1 U0 – scaffold (about 15 lines; orchestrator or any writer)

Files: new `hud/mod.rs`, `hud/font.rs`, `hud/model.rs`, `hud/text.rs`, `hud/layout.rs`,
`hud/sample.rs`, `hud/draw.rs`; edit `mod.rs` (the d3d12x one).

- `mod.rs` (d3d12x): after `pub mod guard;` (`mod.rs:24`) add `pub(crate) mod hud;`.
- `hud/mod.rs`:
  ```rust
  //! The statistics panel ("HUD") in the top-left corner of the XCL window: `sample` reads the
  //! figures on a thread of its own, `layout` turns them into quads, `draw` records one draw of
  //! them on XCL's present list, after the title's frame.
  #![allow(dead_code)] // until present.rs calls in

  pub(crate) mod draw;
  mod font;
  mod layout;
  mod model;
  mod sample;
  mod text;
  ```
- Each other file holds one line: `//! <one sentence of what the unit puts there>`.

Acceptance: `cargo build --workspace` passes on the PC.

#### 10.2 U1 – the bitmap font (`hud/font.rs`, about 190 lines)

The font is a 5 x 9 grid per character, drawn for this project on that grid (no font was looked
at). Specification:
- Characters: printable ASCII 0x20 (space) to 0x7E (`~`), 95 glyphs, index = byte - 0x20.
- Each glyph is 9 rows, row 0 at the top. Each row is one byte; its low 5 bits are the 5 columns,
  **bit 4 = leftmost column**, bit 0 = rightmost. Bits 5-7 are always 0.
- Rows 0-6 hold capitals and digits (row 6 is the baseline row); lowercase x-height is rows 2-6;
  descenders (`g j p q y`, `,`) use rows 7-8; `_` sits on row 7.
- In the layout, a glyph occupies 5 x 9 dots inside a 6-dot advance.

Code:
```rust
//! The statistics panel's font: 95 glyphs of 5 x 9 dots, printable ASCII.

pub const FIRST: u8 = 0x20;
pub const COUNT: usize = 95;

/// Row bytes of each glyph, top row first; bit 4 is the leftmost column.
pub const GLYPHS: [[u8; 9]; COUNT] = [
    /* the 95 lines of the table below, verbatim */
];

/// The glyph of byte `c`; anything outside 0x20..=0x7E is drawn as '?'.
pub fn glyph_index(c: u8) -> u8 {
    if (0x20..=0x7E).contains(&c) { c - FIRST } else { b'?' - FIRST }
}

/// Two words per glyph for the pixel shader: word 0 holds rows 0-5 (row r at bit 5 * r), word 1
/// rows 6-8 (row r at bit 5 * (r - 6)).
pub fn packed() -> [[u32; 2]; COUNT] { /* loop over GLYPHS */ }
```

The table (copy verbatim; the comment is the character):
```rust
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // ' '
    [0x04, 0x04, 0x04, 0x04, 0x04, 0x00, 0x04, 0x00, 0x00], // '!'
    [0x0A, 0x0A, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // '"'
    [0x0A, 0x0A, 0x1F, 0x0A, 0x1F, 0x0A, 0x0A, 0x00, 0x00], // '#'
    [0x04, 0x0F, 0x14, 0x0E, 0x05, 0x1E, 0x04, 0x00, 0x00], // '$'
    [0x19, 0x19, 0x02, 0x04, 0x08, 0x13, 0x13, 0x00, 0x00], // '%'
    [0x0C, 0x12, 0x14, 0x08, 0x15, 0x12, 0x0D, 0x00, 0x00], // '&'
    [0x04, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // "'"
    [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02, 0x00, 0x00], // '('
    [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08, 0x00, 0x00], // ')'
    [0x00, 0x04, 0x15, 0x0E, 0x15, 0x04, 0x00, 0x00, 0x00], // '*'
    [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00, 0x00, 0x00], // '+'
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x04, 0x08, 0x00], // ','
    [0x00, 0x00, 0x00, 0x0E, 0x00, 0x00, 0x00, 0x00, 0x00], // '-'
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C, 0x00, 0x00], // '.'
    [0x01, 0x02, 0x02, 0x04, 0x08, 0x08, 0x10, 0x00, 0x00], // '/'
    [0x0E, 0x11, 0x11, 0x15, 0x11, 0x11, 0x0E, 0x00, 0x00], // '0'
    [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E, 0x00, 0x00], // '1'
    [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F, 0x00, 0x00], // '2'
    [0x0E, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0E, 0x00, 0x00], // '3'
    [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02, 0x00, 0x00], // '4'
    [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E, 0x00, 0x00], // '5'
    [0x0E, 0x10, 0x10, 0x1E, 0x11, 0x11, 0x0E, 0x00, 0x00], // '6'
    [0x1F, 0x01, 0x02, 0x04, 0x04, 0x04, 0x04, 0x00, 0x00], // '7'
    [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E, 0x00, 0x00], // '8'
    [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x01, 0x0E, 0x00, 0x00], // '9'
    [0x00, 0x0C, 0x0C, 0x00, 0x0C, 0x0C, 0x00, 0x00, 0x00], // ':'
    [0x00, 0x0C, 0x0C, 0x00, 0x0C, 0x04, 0x08, 0x00, 0x00], // ';'
    [0x02, 0x04, 0x08, 0x10, 0x08, 0x04, 0x02, 0x00, 0x00], // '<'
    [0x00, 0x00, 0x1F, 0x00, 0x1F, 0x00, 0x00, 0x00, 0x00], // '='
    [0x08, 0x04, 0x02, 0x01, 0x02, 0x04, 0x08, 0x00, 0x00], // '>'
    [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04, 0x00, 0x00], // '?'
    [0x0E, 0x11, 0x17, 0x15, 0x17, 0x10, 0x0F, 0x00, 0x00], // '@'
    [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11, 0x00, 0x00], // 'A'
    [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E, 0x00, 0x00], // 'B'
    [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E, 0x00, 0x00], // 'C'
    [0x1C, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1C, 0x00, 0x00], // 'D'
    [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F, 0x00, 0x00], // 'E'
    [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10, 0x00, 0x00], // 'F'
    [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F, 0x00, 0x00], // 'G'
    [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11, 0x00, 0x00], // 'H'
    [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E, 0x00, 0x00], // 'I'
    [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C, 0x00, 0x00], // 'J'
    [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11, 0x00, 0x00], // 'K'
    [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F, 0x00, 0x00], // 'L'
    [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11, 0x00, 0x00], // 'M'
    [0x11, 0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x00, 0x00], // 'N'
    [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E, 0x00, 0x00], // 'O'
    [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10, 0x00, 0x00], // 'P'
    [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D, 0x00, 0x00], // 'Q'
    [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11, 0x00, 0x00], // 'R'
    [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E, 0x00, 0x00], // 'S'
    [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x00, 0x00], // 'T'
    [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E, 0x00, 0x00], // 'U'
    [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04, 0x00, 0x00], // 'V'
    [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0A, 0x00, 0x00], // 'W'
    [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11, 0x00, 0x00], // 'X'
    [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04, 0x00, 0x00], // 'Y'
    [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F, 0x00, 0x00], // 'Z'
    [0x0E, 0x08, 0x08, 0x08, 0x08, 0x08, 0x0E, 0x00, 0x00], // '['
    [0x10, 0x08, 0x08, 0x04, 0x02, 0x02, 0x01, 0x00, 0x00], // '\\'
    [0x0E, 0x02, 0x02, 0x02, 0x02, 0x02, 0x0E, 0x00, 0x00], // ']'
    [0x04, 0x0A, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // '^'
    [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1F, 0x00], // '_'
    [0x08, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], // '`'
    [0x00, 0x00, 0x0E, 0x01, 0x0F, 0x11, 0x0F, 0x00, 0x00], // 'a'
    [0x10, 0x10, 0x1E, 0x11, 0x11, 0x11, 0x1E, 0x00, 0x00], // 'b'
    [0x00, 0x00, 0x0F, 0x10, 0x10, 0x10, 0x0F, 0x00, 0x00], // 'c'
    [0x01, 0x01, 0x0F, 0x11, 0x11, 0x11, 0x0F, 0x00, 0x00], // 'd'
    [0x00, 0x00, 0x0E, 0x11, 0x1F, 0x10, 0x0F, 0x00, 0x00], // 'e'
    [0x06, 0x08, 0x1E, 0x08, 0x08, 0x08, 0x08, 0x00, 0x00], // 'f'
    [0x00, 0x00, 0x0F, 0x11, 0x11, 0x11, 0x0F, 0x01, 0x0E], // 'g'
    [0x10, 0x10, 0x1E, 0x11, 0x11, 0x11, 0x11, 0x00, 0x00], // 'h'
    [0x04, 0x00, 0x0C, 0x04, 0x04, 0x04, 0x0E, 0x00, 0x00], // 'i'
    [0x02, 0x00, 0x06, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C], // 'j'
    [0x10, 0x10, 0x12, 0x14, 0x18, 0x14, 0x12, 0x00, 0x00], // 'k'
    [0x0C, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E, 0x00, 0x00], // 'l'
    [0x00, 0x00, 0x1A, 0x15, 0x15, 0x15, 0x15, 0x00, 0x00], // 'm'
    [0x00, 0x00, 0x1E, 0x11, 0x11, 0x11, 0x11, 0x00, 0x00], // 'n'
    [0x00, 0x00, 0x0E, 0x11, 0x11, 0x11, 0x0E, 0x00, 0x00], // 'o'
    [0x00, 0x00, 0x1E, 0x11, 0x11, 0x11, 0x1E, 0x10, 0x10], // 'p'
    [0x00, 0x00, 0x0F, 0x11, 0x11, 0x11, 0x0F, 0x01, 0x01], // 'q'
    [0x00, 0x00, 0x16, 0x19, 0x10, 0x10, 0x10, 0x00, 0x00], // 'r'
    [0x00, 0x00, 0x0F, 0x10, 0x0E, 0x01, 0x1E, 0x00, 0x00], // 's'
    [0x08, 0x08, 0x1E, 0x08, 0x08, 0x08, 0x06, 0x00, 0x00], // 't'
    [0x00, 0x00, 0x11, 0x11, 0x11, 0x13, 0x0D, 0x00, 0x00], // 'u'
    [0x00, 0x00, 0x11, 0x11, 0x11, 0x0A, 0x04, 0x00, 0x00], // 'v'
    [0x00, 0x00, 0x11, 0x11, 0x15, 0x15, 0x0A, 0x00, 0x00], // 'w'
    [0x00, 0x00, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x00, 0x00], // 'x'
    [0x00, 0x00, 0x11, 0x11, 0x11, 0x11, 0x0F, 0x01, 0x0E], // 'y'
    [0x00, 0x00, 0x1F, 0x02, 0x04, 0x08, 0x1F, 0x00, 0x00], // 'z'
    [0x03, 0x04, 0x04, 0x08, 0x04, 0x04, 0x03, 0x00, 0x00], // '{'
    [0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04], // '|'
    [0x18, 0x04, 0x04, 0x02, 0x04, 0x04, 0x18, 0x00, 0x00], // '}'
    [0x00, 0x00, 0x08, 0x15, 0x02, 0x00, 0x00, 0x00, 0x00], // '~'
```

(The table was generated from a 5 x 9 `#`/`.` drawing by a throw-away script in the design's
scratch folder; it is the source of truth. `0` has a centre dot to differ from `O`.)

Tests (`mod tests`):
- `every_printable_ascii_has_a_glyph`: `GLYPHS.len() == 95`; `glyph_index(c) == c - 0x20` for every
  `c` in 0x20..=0x7E.
- `rows_use_five_bits`: every row byte < 32.
- `space_is_empty_and_every_other_glyph_has_ink`: glyph 0 all zero; each other glyph has a non-zero row.
- `glyphs_are_distinct`: no two of the 95 are equal.
- `unknown_bytes_map_to_question_mark`: 0x00, 0x1F, 0x7F, 0xC3 -> `b'?' - 0x20`.
- `caps_and_digits_stay_above_the_descender`: for `A..=Z` and `0..=9`, rows 7 and 8 are 0.
- `packed_decodes_back_to_the_rows`: decoding `packed()` with the shader's rule gives `GLYPHS`.

Acceptance: the 7 tests pass.

#### 10.3 U2a – model: modes, quads, rings, rates (`hud/model.rs`, about 370 lines)

Public items (exact signatures; everything `pub` inside the private module):

```rust
pub const MAX_INST: usize = 2048;
pub const HISTORY: usize = 72;
pub const STAMPS: usize = 128;
pub const GIB: f32 = 1_073_741_824.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Mode { Off = 0, Compact = 1, Full = 2 }
impl Mode {
    /// ASCII case-insensitive, trimmed: off/0/false/no/none, compact/1/small, full/2/on/true/yes.
    pub fn parse(s: &str) -> Option<Mode>;
    /// 0 Off, 1 Compact, anything else Full.
    pub fn from_u8(v: u8) -> Mode;
    /// F3: Full -> Compact -> Off -> Full.
    pub fn next(self) -> Mode;
    /// "off" / "compact" / "full".
    pub fn name(self) -> &'static str;
}

/// One quad: `rect` = x, y, width, height in back-buffer pixels; `color` = 0xAABBGGRR
/// (straight alpha); `param` bits 0-3 kind, bits 8-23 the corner radius in pixels (round) or
/// bits 8-15 the glyph index (glyph).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Inst { pub rect: [f32; 4], pub color: u32, pub param: u32, pub pad: [u32; 2] }
pub const KIND_RECT: u32 = 0;
pub const KIND_ROUND: u32 = 1;
pub const KIND_GLYPH: u32 = 2;
impl Inst {
    pub fn rect(x: f32, y: f32, w: f32, h: f32, color: u32) -> Inst;
    pub fn round(x: f32, y: f32, w: f32, h: f32, radius_px: u32, color: u32) -> Inst; // param = 1 | (min(r, 0xFFFF) << 8)
    pub fn glyph(x: f32, y: f32, w: f32, h: f32, index: u8, color: u32) -> Inst;      // param = 2 | (index << 8)
}
/// 0xRRGGBB and an alpha -> 0xAABBGGRR.
pub fn rgba(rgb: u32, alpha: u8) -> u32;

/// The newest N values; index 0 is the oldest kept.
#[derive(Clone, Copy, Debug)]
pub struct Ring<const N: usize> { vals: [f32; N], head: usize, len: usize }
impl<const N: usize> Ring<N> {
    pub const fn new() -> Self;
    pub fn push(&mut self, v: f32);          // overwrites the oldest when full
    pub fn len(&self) -> usize;
    pub fn get(&self, i: usize) -> f32;      // 0 = oldest .. len-1 = newest; 0.0 past len
    pub fn max(&self) -> f32;                // 0.0 when empty
    pub fn last(&self) -> Option<f32>;
}

/// Time stamps (µs) of shown frames: one writer (the presenting thread), any reader.
pub struct FrameStamps { t: [AtomicU64; STAMPS], n: AtomicU64 }
impl FrameStamps {
    pub const fn new() -> Self;               // [const { AtomicU64::new(0) }; STAMPS]
    pub fn push(&self, t_us: u64);            // t[n % STAMPS].store(t, Relaxed); n.fetch_add(1, Release)
    pub fn total(&self) -> u64;               // n.load(Acquire)
    /// Copies min(total, STAMPS) stamps into `out`, newest first; returns how many.
    pub fn newest_first(&self, out: &mut [u64; STAMPS]) -> usize;
}

/// Frames a second from stamps (newest first) at `now_us`: section 4.1 of the design.
pub fn fps(newest_first: &[u64], now_us: u64) -> f32;

/// 100 ns units: wall clock, process kernel + user, and GetSystemTimes' idle, kernel, user.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CpuTimes { pub wall: u64, pub proc: u64, pub idle: u64, pub kernel: u64, pub user: u64 }
/// (process %, system %) between two samples: section 4.2.
pub fn cpu_percent(a: CpuTimes, b: CpuTimes, cpus: u32) -> (f32, f32);

/// Top of the frame-time graph (ms): the first of 16.7, 33.3, 50, 100, 200, 500, 1000, 2000,
/// 5000, 10000 that is >= v; v itself above 10000; 16.7 for v <= 0 or NaN.
pub fn nice_ceiling(v: f32) -> f32;
/// Height in pixels of a column of `v` in a graph of `h` pixels whose top is `scale`:
/// 0 when v <= 0, scale <= 0 or not finite; else max(1, round(min(v / scale, 1) * h)).
pub fn column_height(v: f32, scale: f32, h: u32) -> u32;
/// Top of the video-memory graph (GiB): max(budget, peak) * 1.05, or 1.0 when both are 0.
pub fn vram_scale(budget_gib: f32, peak_gib: f32) -> f32;

/// What the sampler publishes (copied whole; no heap).
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub generation: u64,
    pub uptime_s: u64,
    pub frames_seen: bool,                 // at least two frames ever shown
    pub fps: f32,
    pub frame_ms: f32,                     // newest interval
    pub frame_ms_hist: Ring<HISTORY>,
    pub gpu_ms: Option<f32>,
    pub cpu_proc: f32,
    pub cpu_sys: f32,
    pub cpu_proc_hist: Ring<HISTORY>,
    pub cpu_sys_hist: Ring<HISTORY>,
    pub ws_bytes: u64,
    pub commit_bytes: u64,
    pub sys_used_bytes: u64,
    pub sys_total_bytes: u64,
    pub ws_hist: Ring<HISTORY>,            // GiB
    pub sys_used_hist: Ring<HISTORY>,      // GiB
    pub vram_bytes: u64,
    pub vram_budget_bytes: u64,
    pub vram_hist: Ring<HISTORY>,          // GiB
    pub pso_pending: u64,
    pub pso_failed: u64,
    pub dxc_running: u64,
    pub dxc_done: u64,
    pub code: Option<(u64, u64)>,          // driver code in use, limit (bytes)
    pub client_w: u32,
    pub client_h: u32,
    pub rt_on: bool,
}
impl Snapshot { pub const EMPTY: Snapshot = /* zeros, None, Ring::new(), false */; }
```

Tests:
- `mode_parses_every_spelling` (all spellings, case and spaces; `"maybe"` -> None).
- `mode_cycles_full_compact_off` (and `from_u8(Mode::X as u8) == X`).
- `inst_is_32_bytes_and_packs_params` (`size_of::<Inst>() == 32`; round radius 8 -> param `0x801`;
  glyph 40 -> `0x2802`).
- `rgba_puts_red_in_the_low_byte` (`rgba(0x4CC2FF, 0x80) == 0x80FFC24C`).
- `ring_keeps_the_newest_oldest_first` (push 1..=5 into `Ring<3>`: get 3, 4, 5; last 5; max 5).
- `ring_max_of_empty_is_zero`.
- `frame_stamps_wrap_and_copy_newest_first` (push 200 stamps; total 200; 128 copied; out[0] = the
  200th, out[127] = the 73rd).
- `fps_counts_the_last_second_at_high_rates` (60 stamps 16 667 µs apart ending at now -> 60).
- `fps_uses_the_mean_interval_at_low_rates` (stamps every 400 ms, now = newest + 100 ms -> 2.5).
- `fps_falls_while_the_title_stalls` (same stamps, now = newest + 2 s -> 0.5).
- `fps_is_zero_with_fewer_than_two_frames` (empty; one stamp; two stamps older than 5 s).
- `cpu_percent_from_two_samples` (wall +10 000 000, proc +20 000 000, 8 CPUs -> 25 %; idle +6, kernel
  +8 (incl. idle), user +2 -> 40 %).
- `cpu_percent_with_no_elapsed_time_is_zero`.
- `cpu_percent_clamps_to_100`.
- `nice_ceiling_picks_the_next_step` (10 -> 16.7, 16.7 -> 16.7, 17 -> 33.3, 412 -> 500, 20000 -> 20000, -1 -> 16.7).
- `column_height_scales_rounds_and_clamps` (50/100 of 18 -> 9; 0.01/100 of 18 -> 1; 200/100 -> 18; 0 -> 0; scale 0 -> 0).
- `vram_scale_covers_budget_and_peak` (14.6, 15.1 -> 15.855; 14.6, 2 -> 15.33; 0, 0 -> 1).

Acceptance: the 17 tests pass; no `unsafe`; no heap use in any function.

#### 10.4 U2b – text: fixed buffers and number formats (`hud/text.rs`, about 200 lines)

```rust
/// Up to N bytes of printable ASCII, on the stack.
#[derive(Clone, Copy)]
pub struct FixedText<const N: usize> { buf: [u8; N], len: usize }
impl<const N: usize> FixedText<N> {
    pub const fn new() -> Self;
    pub fn clear(&mut self);
    /// Each char: 0x20..=0x7E kept, anything else becomes '?'; stops silently at N bytes.
    pub fn push_str(&mut self, s: &str);
    pub fn as_str(&self) -> &str;            // always valid UTF-8 (ASCII only)
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
impl<const N: usize> core::fmt::Write for FixedText<N> { /* push_str, always Ok(()) */ }

// Each writes (appends) into `t` with write!(); none allocates.
pub fn gb<const N: usize>(t: &mut FixedText<N>, bytes: u64);              // GiB: < 100 -> "9.8 GB", else "128 GB"
pub fn gb_short<const N: usize>(t: &mut FixedText<N>, bytes: u64);        // "9.8G" / "128G"
pub fn gb_pair_short<const N: usize>(t: &mut FixedText<N>, a: u64, b: u64); // "15.1/14.6G" (each number by the < 100 rule)
pub fn mb_pair<const N: usize>(t: &mut FixedText<N>, a: u64, b: u64);     // "312 / 448 MB" (MiB, integer)
pub fn ms<const N: usize>(t: &mut FixedText<N>, v: f32);                  // NaN/neg "--"; < 10 "4.12 ms"; < 100 "41.2 ms"; else "412 ms"
pub fn fps<const N: usize>(t: &mut FixedText<N>, v: f32);                 // NaN/neg "0.0"; < 10 "2.4"; else "60"
pub fn percent<const N: usize>(t: &mut FixedText<N>, v: f32);             // clamp 0..=100, "38%"
pub fn count<const N: usize>(t: &mut FixedText<N>, n: u64);               // "55,223"
pub fn uptime<const N: usize>(t: &mut FixedText<N>, s: u64);              // "01:12:34", hours unbounded ("123:04:05")

/// Text that does not change during a run.
pub struct Statics { pub gpu: FixedText<40>, pub title: FixedText<32> }
impl Statics { pub const fn new() -> Self; }
```

Tests:
- `fixed_text_truncates_without_panicking` (`FixedText<4>` given "abcdef" -> "abcd").
- `fixed_text_replaces_non_printable_and_non_ascii` ("Ü\tx" -> "??x").
- `formats_gibibytes` (9.8 GiB -> "9.8 GB"; 128 GiB -> "128 GB"; short and pair forms; 0 -> "0.0 GB").
- `formats_milliseconds` (4.123, 41.23, 412.3, NaN).
- `formats_fps` (2.43 -> "2.4", 59.6 -> "60", NaN -> "0.0").
- `formats_percent_clamped` (-3 -> "0%", 38.4 -> "38%", 140 -> "100%").
- `groups_thousands` (0, 999, 1000, 55223, 1234567).
- `formats_uptime_past_99_hours` (4354 -> "01:12:34"; 443045 -> "123:04:05").

Acceptance: the 8 tests pass.

#### 10.5 U3 – layout (`hud/layout.rs`, about 400 lines)

Depends on U1, U2a, U2b.

```rust
pub struct InstBuf { pub items: [Inst; MAX_INST], pub len: usize }
impl InstBuf {
    pub const fn new() -> Self;
    pub fn clear(&mut self);
    pub fn push(&mut self, i: Inst) -> bool;   // false (and nothing written) when full
    pub fn as_slice(&self) -> &[Inst];
}
/// Back-buffer pixels per dot: max(1, round(frame_h / 540)).
pub fn dot_px(frame_h: u32) -> u32;
/// Clears `out` and fills it for `mode` (nothing for Off), frame of `frame_w` x `frame_h`.
pub fn build(s: &Snapshot, st: &Statics, mode: Mode, frame_w: u32, frame_h: u32, out: &mut InstBuf);
```

Colours (0xRRGGBB, alpha 0-255), as `const`s:

| Name | RGB | Alpha | Use |
|---|---|---|---|
| `BG` | `0x0E1117` | 199 (0.78) | panel |
| `TEXT` | `0xE6EAF2` | 255 | values |
| `LABEL` | `0x8B93A7` | 255 | labels, run time |
| `ACCENT` | `0x4CC2FF` | 255 | `XCL`, frame-time graph |
| `CPU` | `0xA78BFA` | — | CPU graph |
| `RAM` | `0x34D399` | — | RAM graph |
| `VRAM` | `0xF59E0B` | — | VRAM graph |
| `GOOD` | `0x34D399` | 255 | FPS >= 55 |
| `WARN` | `0xFBBF24` | 255 | FPS 25-55, code >= 75 % |
| `BAD` | `0xF87171` | 255 | FPS < 25, VRAM over budget, code >= 90 % |
| `GRAPH_BG` | `0xFFFFFF` | 13 (0.05) | graph background |
| `GRID` | `0xFFFFFF` | 15 (0.06) | graph mid line |
| `SEP` | `0xFFFFFF` | 20 (0.08) | separators |
| series fill | its colour | 89 (0.35) | primary series columns |
| secondary fill | its colour | 46 (0.18) | system series columns (no cap) |
| cap | its colour | 255 | 1-dot top of each primary column |
| budget line | `TEXT` 140, or `BAD` 255 when over budget | | VRAM budget |

Units: `P = dot_px(frame_h)`. A dot coordinate `(x, y)` relative to the panel maps to pixels
`(8P + x * P, 8P + y * P)` (the panel's top-left is 8 dots from the frame's corner). A hairline is
`max(1, P / 2)` pixels tall. Padding `PAD = 6` dots. Text: column `c` of a line starts at dot
`PAD + 6c`; glyph quad = 5P x 9P pixels at the line's top; spaces emit nothing; each byte goes
through `font::glyph_index`.

Full mode (panel 180 dots wide):
1. Panel: `Inst::round(8P, 8P, 180P, H*P, 4P, rgba(BG, 199))`, `H = 118 + 10 * rows + 5`, `rows` =
   number of detail lines (step 6). It is pushed first (computed before anything else).
2. Header at y = 6: `XCL` (`ACCENT`) column 0; `st.title` cut to 15 characters (`TEXT`) column 4;
   run time `uptime` (`LABEL`) right-aligned: column `28 - len`.
3. Separator: rect at dot (PAD, 18), 168 dots wide, hairline tall, `SEP`.
4. Groups g = 0..3 at `yg = 21 + 24g`; line 1 at `yg`, line 2 at `yg + 10`; label column 0 (`LABEL`),
   value column 5; values formatted into `FixedText<11>`. Graph: dot (PAD + 96, yg + 1), 72 x 18 dots.
   - g0: `FPS` / `fps` in `fps_colour`; line 2 `ms(frame_ms)` (`TEXT`; `--` when `!frames_seen`).
     Graph: `frame_ms_hist` primary `ACCENT`, scale `nice_ceiling(frame_ms_hist.max())`.
   - g1: `CPU` / `percent(cpu_proc)`; line 2 `sys` / `percent(cpu_sys)`. Graph: `cpu_sys_hist`
     secondary `CPU`, then `cpu_proc_hist` primary `CPU`; scale 100.
   - g2: `RAM` / `gb(ws_bytes)`; line 2 `sys` / `gb_pair_short(sys_used, sys_total)`. Graph:
     `sys_used_hist` secondary `RAM`, then `ws_hist` primary `RAM`; scale `sys_total_bytes / GIB`
     (1.0 when 0).
   - g3: `VRAM` / `gb(vram_bytes)` in `BAD` when `vram_budget_bytes > 0 && vram_bytes >
     vram_budget_bytes`, else `TEXT`; line 2 `of` / `gb(vram_budget_bytes)`. Graph: `vram_hist`
     primary `VRAM`; scale `vram_scale(budget / GIB, vram_hist.max())`; then the budget line.
5. Separator at y = 115.
6. Detail lines from y = 118, 10 dots apart; label column 0 (`LABEL`), value column 7 (`TEXT`
   unless said), values in `FixedText<21>`:
   `GPU` (`st.gpu`, `--` when empty); `Commit` (`gb(commit_bytes)`); `PSO` (`count(pending)` +
   ` queued`); `Failed` only when `pso_failed > 0` (`count(failed)` in `BAD`); `DXC` (`R run, ` +
   `count(done)` + ` done`); `Code` only when `code` is `Some((b, l))` (`mb_pair(b, l)` in
   `pct_colour(b, l)`); `GPU ms` only when `gpu_ms` is `Some(v)` (`ms(v)`); `Res`
   (`{frame_w}x{frame_h} > {client_w}x{client_h}`, only the frame part when client is 0 x 0);
   `RT` (`on` / `off`).

Compact mode (panel 168 x 42 dots): panel quad first; lines at y = 6, 16, 26:
- `FPS` column 0 (`LABEL`), `fps` column 4 (`fps_colour`), `ms(frame_ms)` column 10 (`TEXT`);
- `CPU` column 0, `percent(cpu_proc)` column 4, `RAM` column 9 (`LABEL`), `gb_short(ws)` column 13;
- `VRAM` column 0, `gb_pair_short(vram, budget)` column 5 (`BAD` when over budget).
- Graph at dot (PAD + 108, 7), 48 x 28 dots: `frame_ms_hist` primary `ACCENT`, scale `nice_ceiling`.

Graph drawing (both modes): background rect (`GRAPH_BG`); mid line at half height (hairline,
`GRID`); for each series in order, `m = min(len, columns)` newest values, value `j` (0 = oldest of
the m) in column `columns - m + j` (1 dot wide), height `column_height(v, scale, gh * P)` pixels
from the bottom; skip zero heights; fill rect with the series' fill alpha; for a primary series a
cap rect of `min(P, height)` pixels at the column's top. Budget line: hairline across the graph at
`bottom - column_height(budget_gib, scale, gh * P)`.

Colour rules: `fps_colour(fps, frames_seen)`: `LABEL` when `!frames_seen`; `GOOD` >= 55; `WARN` >= 25;
else `BAD`. `pct_colour(used, limit)`: `BAD` at >= 90 % of limit, `WARN` at >= 75 %, else `TEXT`
(`TEXT` when limit is 0).

Tests:
- `dot_size_follows_frame_height` (0, 360, 720 -> 1; 1080 -> 2; 1440 -> 3; 2160 -> 4).
- `full_panel_is_180_dots_wide_at_8_dots_from_the_corner` (h 1080, 6 rows: first item
  `round(16, 16, 360, 366)` with radius 8).
- `compact_panel_is_168_by_42_dots` (h 1080: `round(16, 16, 336, 84)`).
- `absent_rows_shorten_the_panel` (`code`, `gpu_ms` Some and `pso_failed` 3 -> 30 dots taller).
- `glyphs_advance_six_dots_and_skip_spaces` (header `XCL` + title "A B": glyph x positions).
- `uptime_is_right_aligned_to_the_inner_edge` (last glyph ends at dot 6 + 168 - 1).
- `fps_colour_thresholds` (0 unseen, 10, 25, 54.9, 55).
- `vram_over_budget_is_red`.
- `graph_columns_are_right_aligned_newest_last` (3 values in a 72-column graph: columns 69-71).
- `layout_never_exceeds_capacity` (every ring full of maximum values, both modes: `len <= MAX_INST`
  and the count is below 1 600 in full mode).
- `every_character_the_layout_prints_has_a_glyph` (ASCII statics, extreme numbers: no glyph quad
  carries the index of `?`).
- `off_mode_builds_nothing`.

Acceptance: the 12 tests pass; `build` does not allocate (only `FixedText` and `write!`).

#### 10.6 U4 – sampler thread (`hud/sample.rs`, about 260 lines; `code_use.rs`, about 25 lines)

Depends on U2a, U2b.

`hud/sample.rs`:
```rust
/// State shared by the presenting thread, the sampler and the window thread.
pub(super) struct Shared {
    pub snapshot: Mutex<Snapshot>,
    pub statics: Mutex<Statics>,
    pub frames: FrameStamps,
    pub mode: AtomicU8,        // Mode as u8
}
pub(super) static SHARED: Shared = Shared { /* Snapshot::EMPTY, Statics::new(), FrameStamps::new(), 0 */ };

/// Microseconds since the first call (one clock for stamps and the sampler).
pub(super) fn now_us() -> u64;          // OnceLock<Instant>
/// Copies `title` into SHARED.statics.title and starts thread "xcl-hud" (a spawn failure logs one
/// warning and nothing else).
pub(super) fn spawn(hwnd: isize, title: &str);
/// Pushes into `out`, oldest first, the interval (ms) of every frame f with seen < f <= total
/// (frames numbered from 1; newest_first[k] is frame total - k) whose own and previous stamps are
/// both in `newest_first`; returns `total`.
pub(super) fn new_intervals(newest_first: &[u64], total: u64, seen: u64, out: &mut Ring<HISTORY>) -> u64;
fn filetime(f: FILETIME) -> u64;        // (high << 32) | low
```
`new_intervals`: `lo = max(seen + 1, total + 2 - n, 2)` (saturating, `n = newest_first.len()`); for
`f in lo..=total`: `k = total - f`; push `(newest_first[k] - newest_first[k + 1]) / 1000.0`.

Thread body `run(hwnd)` (never returns; all buffers made before the loop):
- Before the loop: `cpus = max(1, GetActiveProcessorCount(0xFFFF))`; `rt_on =
  matches!(crate::state::config().raytracing_chain_mode(), crate::config::ChainMode::On)`; `start =
  now_us()`; a `Snapshot::EMPTY`; `[0u64; STAMPS]`; `adapter: Option<IDXGIAdapter3> = None`;
  `prev: Option<CpuTimes> = None`; `seen = 0`; `tick = 0`.
- Each loop: `sleep(250 ms)`; `tick += 1`; if `Mode::from_u8(SHARED.mode)` is Off, continue.
  1. Frames: `total`, `newest_first`; `fps`; `seen = new_intervals(...)`; `frame_ms` = newest
     interval (0 when fewer than 2); `frames_seen = total >= 2`.
  2. CPU: `GetProcessTimes(GetCurrentProcess(), ..)` (kernel + user), `GetSystemTimes`; `wall =
     now_us() * 10`; with `prev`, `cpu_percent`, push both histories; `prev = Some(now)`.
  3. Memory: `K32GetProcessMemoryInfo` with `PROCESS_MEMORY_COUNTERS_EX` (`WorkingSetSize`,
     `PrivateUsage`); `GlobalMemoryStatusEx` (`ullTotalPhys - ullAvailPhys`, `ullTotalPhys`); push
     `ws / GIB`, `used / GIB`.
  4. Adapter: while `adapter` is None and `crate::hle::d3d12x::real::device_raw()` is not null:
     `ID3D12Device::from_raw_borrowed`, `GetAdapterLuid`, `CreateDXGIFactory1::<IDXGIFactory4>`,
     `EnumAdapterByLuid::<IDXGIAdapter3>`; name = `crate::gpu::adapter::name(&a.cast::<IDXGIAdapter1>()?.GetDesc1()?)`
     into `SHARED.statics.gpu` (lock). Then each tick `QueryVideoMemoryInfo(0,
     DXGI_MEMORY_SEGMENT_GROUP_LOCAL)` -> `CurrentUsage`, `Budget`; push `usage / GIB`. Failure: the
     values stay as they were.
  5. Every 4th tick (`tick % 4 == 1`): `crate::hle::d3d12x::pso_async::counts()` -> `pso_failed =
     .1`, `pso_pending = .2`; `crate::gpu::recomp::dxc::compile_counts()`;
     `crate::hle::d3d12x::code_use::in_use()`; `GetClientRect(HWND(hwnd as _))` -> client size (0 x 0
     on failure).
  6. `uptime_s = (now_us() - start) / 1_000_000`; `rt_on`; `generation += 1`; `*SHARED.snapshot.lock()
     .unwrap_or_else(|e| e.into_inner()) = snap`.

`code_use.rs` additions (after `removal_line`, `code_use.rs:329`-`331`):
```rust
impl State {
    fn in_use_with(&self, on: bool, limit: u64) -> Option<(u64, u64)> {
        on.then(|| (self.in_use_bytes.load(Ordering::Relaxed), limit))
    }
}
/// Driver code of the pipelines in use and the limit, in bytes (the statistics panel); None
/// while it is not counted (`on()`).
pub fn in_use() -> Option<(u64, u64)> {
    state().in_use_with(on(), limit_bytes())
}
```

Tests:
- `sample.rs`: `new_intervals_pushes_each_frame_once` (stamps of 5 frames, seen 0 -> 4 intervals; again
  with seen 5 -> none; one more frame -> 1), `new_intervals_skips_frames_that_left_the_ring` (total
  300, 128 stamps, seen 10 -> 127 intervals, last 72 kept by the ring), `filetime_joins_high_and_low`.
- `code_use.rs`: `in_use_reports_bytes_and_limit_when_on` (a `State::default()` with 5 MiB in use ->
  `Some((5 MiB, 448 MiB))`), `in_use_is_none_when_off`.

Acceptance: the 5 tests pass; the loop allocates nothing after its first pass (no `format!`,
`Vec`, `String`; the adapter name is written once).

#### 10.7 U5 – the draw (`hud/draw.rs`, about 390 lines; `present.rs`, 3 words)

Depends on U1, U2a.

`present.rs`: make three existing functions visible to the HUD by changing `fn` to `pub(super) fn`:
`compile_fx` (`present.rs:678`), `transition` (`present.rs:939`), `readback_copy` (`present.rs:1307`).
Nothing else in `present.rs`.

`hud/draw.rs`:
```rust
const SLOTS: usize = 2; // one upload region per present slot (present.rs PRESENT_SLOTS)

pub(crate) struct HudDraw {
    root_sig: ID3D12RootSignature,
    psos: HashMap<i32, ID3D12PipelineState>,
    rtv_heap: ID3D12DescriptorHeap,      // 1 RTV, not shader visible
    upload: ID3D12Resource,              // UPLOAD heap, SLOTS * MAX_INST * 32 bytes, GENERIC_READ
    mapped: *mut Inst,                   // Map(0, Some(&D3D12_RANGE { Begin: 0, End: 0 })) once
}
impl HudDraw {
    /// Root signature, RTV heap, upload buffer; None on any failure.
    pub(crate) unsafe fn new(dev: &ID3D12Device) -> Option<HudDraw>;
    /// The PSO for `fmt`, made on first use (as Compositor::psos_for).
    unsafe fn pso_for(&mut self, dev: &ID3D12Device, fmt: DXGI_FORMAT) -> Option<ID3D12PipelineState>;
    /// Records the quads into `dst` (already in RENDER_TARGET state) on `list`, using upload
    /// region `slot % SLOTS`. False (and nothing recorded) when `insts` is empty or a PSO is missing.
    pub(crate) unsafe fn record(&mut self, dev: &ID3D12Device, list: &ID3D12GraphicsCommandList,
        dst: &ID3D12Resource, dst_w: u32, dst_h: u32, dst_fmt: DXGI_FORMAT, insts: &[Inst], slot: usize) -> bool;
}
fn shaders() -> Option<&'static (Vec<u8>, Vec<u8>)>;  // OnceLock: compile_fx(VS, "vs_5_0"), compile_fx(&ps_source(), "ps_5_0")
/// Compiles the two shaders now (called by `hud::start` on the window thread, so the first
/// HUD frame does not wait for FXC).
pub(crate) fn warm();
fn ps_source() -> String;                              // "static const uint FONT[190] = { 0x........u, ... };\n" + PS_BODY
```
- Root signature (v1.0, flags NONE): parameter 0 = 4 32-bit constants at `b0`, visibility ALL
  (`inv_w`, `inv_h`, `linear_out` 0.0/1.0, 0.0); parameter 1 = root SRV at `t0`, visibility VERTEX.
  No static samplers, no tables.
- PSO: as the compositor's blended one (`present.rs:808`-`845`): VS/PS, `SampleMask` all,
  triangles, one RT of `fmt`, cull none, no depth, blend `ONE` / `INV_SRC_ALPHA` for colour and
  alpha, write mask ALL, no input layout.
- `record`: `n = min(insts.len(), MAX_INST)`; 0 -> false; `pso_for` None -> false; `copy_nonoverlapping`
  into `mapped + (slot % SLOTS) * MAX_INST`; `CreateRenderTargetView(dst, {dst_fmt, TEXTURE2D},
  heap start)`; `OMSetRenderTargets`; viewport and scissor of `dst_w x dst_h`; root signature;
  `SetGraphicsRoot32BitConstants(0, 4, [1/w, 1/h, linear, 0], 0)` with `linear = 1.0` exactly when
  `dst_fmt == DXGI_FORMAT_R16G16B16A16_FLOAT`; `SetGraphicsRootShaderResourceView(1,
  upload.GetGPUVirtualAddress() + ((slot % SLOTS) * MAX_INST * 32) as u64)`; `SetPipelineState`;
  `IASetPrimitiveTopology(TRIANGLELIST)`; `DrawInstanced(6, n, 0, 0)`; true. No descriptor heap is
  set (none is used).
- It never waits and never touches any resource but `dst`, its upload buffer and its RTV heap.

HLSL (FXC, SM 5.0). Vertex shader `VS`:
```hlsl
struct Inst { float4 rect; uint color; uint param; uint pad0; uint pad1; };
StructuredBuffer<Inst> insts : register(t0);
cbuffer C : register(b0) { float inv_w; float inv_h; float linear_out; float unused; };
struct VSOut {
    float4 pos : SV_Position;
    float2 local : TEXCOORD0;
    nointerpolation float2 size : TEXCOORD1;
    nointerpolation float4 color : COLOR0;
    nointerpolation uint param : PARAM0;
};
VSOut main(uint vid : SV_VertexID, uint iid : SV_InstanceID) {
    Inst i = insts[iid];
    float2 c = float2((vid == 1 || vid == 2 || vid == 4) ? 1.0 : 0.0,
                      (vid == 2 || vid == 4 || vid == 5) ? 1.0 : 0.0);
    float2 px = i.rect.xy + c * i.rect.zw;
    VSOut o;
    o.pos = float4(px.x * inv_w * 2.0 - 1.0, 1.0 - px.y * inv_h * 2.0, 0.0, 1.0);
    o.local = c * i.rect.zw;
    o.size = i.rect.zw;
    o.color = float4(i.color & 0xFF, (i.color >> 8) & 0xFF, (i.color >> 16) & 0xFF, i.color >> 24) / 255.0;
    o.param = i.param;
    return o;
}
```
Pixel shader `PS_BODY` (after the generated `FONT` line; the same `cbuffer` and `VSOut` repeated):
```hlsl
cbuffer C : register(b0) { float inv_w; float inv_h; float linear_out; float unused; };
struct VSOut { /* identical to the vertex shader's */ };
float3 to_linear(float3 c) { return c <= 0.04045 ? c / 12.92 : pow((c + 0.055) / 1.055, 2.4); }
float4 main(VSOut v) : SV_Target {
    uint kind = v.param & 0xF;
    float a = v.color.a;
    if (kind == 1) {
        float r = (float)((v.param >> 8) & 0xFFFF);
        float2 h = v.size * 0.5;
        float2 q = abs(v.local - h) - (h - r);
        float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
        a *= saturate(0.5 - d);
    } else if (kind == 2) {
        uint g = (v.param >> 8) & 0xFF;
        uint2 cell = (uint2)min(floor(v.local / v.size * float2(5.0, 9.0)), float2(4.0, 8.0));
        uint word = cell.y < 6 ? FONT[g * 2] : FONT[g * 2 + 1];
        uint row = (word >> (5 * (cell.y < 6 ? cell.y : cell.y - 6))) & 31;
        if (((row >> (4 - cell.x)) & 1) == 0) discard;
    }
    float3 c = v.color.rgb;
    if (linear_out > 0.5) c = to_linear(c);
    return float4(c * a, a);
}
```

Tests (GPU tests on WARP, as `compositor_blends_ui_plane_over_scene_plane`, `present.rs:1714`;
when the WARP device cannot be made: `crate::gpu::recomp::runner::skip_test("no GPU")` and return):
- `ps_source_holds_the_whole_font`: the generated initializer has exactly 190 `0x` entries and
  starts with `static const uint FONT[190]`.
- `hud_draws_rects_rounded_corners_and_glyphs_on_warp`: RGBA8 target 64 x 32 (COMMON -> RENDER_TARGET);
  quads: rect (0,0,64,32) black opaque; rect (0,0,32,32) `rgba(0xFFFFFF, 128)`; round (32,0,32,32)
  radius 8 `rgba(0xFF0000, 255)`; glyph `H` (`glyph_index(b'H')`) at (8,2,15,27) `rgba(0x00FF00,255)`.
  `record(.., slot 0)` returns true; RT -> COPY_SOURCE; `readback_copy`; execute, signal, wait.
  Expect: (4,20) each of R,G,B in 126..=130; (33,1) R <= 2 (outside the rounded corner); (48,16)
  `[255,0,0,255]`; (9,3) `[0,255,0,255]` (H row 0, column 0 lit); (15,3) grey 126..=130 (row 0,
  column 2 unlit); (15,12) `[0,255,0,255]` (row 3 is the bar).
- `hud_writes_linear_colour_into_fp16_targets_on_warp`: R16G16B16A16_FLOAT target 8 x 8; one rect
  (0,0,8,8) `rgba(0x808080, 255)`; pixel (4,4): R, G, B in 0.211..=0.221 (linear of 128/255 =
  0.2159), A = 1.0 (a 6-line f16 decode in the test).
- `empty_quads_record_nothing` (`record` with `&[]` returns false).

Acceptance: the 4 tests pass on the PC, not skipped; the three words in `present.rs` are the only
change outside `hud/`.

#### 10.8 U6 – wiring, switches, docs (about 300 lines)

Depends on all above. Files: `hud/mod.rs`, `present.rs`, `crates/xcl/src/config.rs`, `README.md`,
`docs/guide/cli-and-config.md`.

`hud/mod.rs` (remove the `#![allow(dead_code)]` line; keep the module list):
```rust
static STARTED: AtomicBool;   // the sampler runs (configured mode is not Off)
static FAILED: AtomicBool;    // pipeline creation failed: no more drawing this run
struct Cache { insts: InstBuf, snap: Snapshot, statics: Statics, gen: u64, mode: Mode, wh: (u32, u32) }
static CACHE: Mutex<Cache>;   // only the presenting thread takes it
static TIMING: (AtomicU64 frames, AtomicU64 total_ns, AtomicU64 max_ns);

/// XCL_HUD first, else `hud` from xcl.toml; an invalid value gives a warning text and falls back
/// (invalid env -> the config's value; invalid config -> Full).
fn resolve(env: Option<&str>, cfg: &str) -> (Mode, Option<String>);
fn configured() -> Mode;      // OnceLock: resolve(std::env::var("XCL_HUD").ok(), &state::config().hud), warning logged once
/// Window thread, once the window exists: Off -> nothing (one info line "hud: off"); else
/// STARTED, SHARED.mode = mode, draw::warm(), sample::spawn(hwnd, title), info line
/// "hud: <mode> (F3: full / compact / off; XCL_HUD or hud in xcl.toml)".
pub(crate) fn start(hwnd: isize, title: &str);
/// Presenting thread, every shown frame: pushes `now_us()` into SHARED.frames (returns at once
/// when not STARTED).
pub(crate) fn note_frame();
/// F3 key-down, not a repeat: wParam == 0x72 and bit 30 of lParam clear.
fn is_toggle(vk: usize, lparam: isize) -> bool;
/// Window procedure, WM_KEYDOWN: true when it was F3 and the HUD runs (mode -> next, info line
/// "hud: <mode>").
pub(crate) fn on_key(vk: usize, lparam: isize) -> bool;
/// STARTED, not FAILED, mode not Off.
pub(crate) fn active() -> bool;
/// The barriers around the panel: with the HUD, `state -> RENDER_TARGET` (none when already
/// there) then `RENDER_TARGET -> PRESENT`; without, `state -> PRESENT` (none when already there).
pub(crate) fn tail_barriers(state: D3D12_RESOURCE_STATES, hud: bool)
    -> [Option<(D3D12_RESOURCE_STATES, D3D12_RESOURCE_STATES)>; 2];
/// Presenting thread: makes `draw` on first use (failure: FAILED, one warning "hud: off – <why>"),
/// rebuilds the layout when the snapshot generation (try_lock), the mode or (w, h) changed, then
/// HudDraw::record. Times itself; after 600 drawn frames logs once
/// "hud: 600 frames drawn, record mean <x> µs, max <y> µs, <n> quads".
pub(crate) unsafe fn record(dev: &ID3D12Device, list: &ID3D12GraphicsCommandList, bb: &ID3D12Resource,
    w: u32, h: u32, fmt: DXGI_FORMAT, slot: usize, draw: &mut Option<draw::HudDraw>) -> bool;
```
`record` takes `CACHE` with `try_lock` (busy -> return false), reads `SHARED.snapshot` and
`SHARED.statics` with `try_lock` (busy -> keep the cached copies), never waits.

`present.rs` (exact changes):
1. `struct Presenter` (`present.rs:904`-`925`): add field `hud: Option<super::hud::draw::HudDraw>,`;
   `new_presenter` (`present.rs:968`-`975`): `hud: None,`.
2. In `present_now` after `let mut composited = false;` (`present.rs:1121`): `let mut bb_state =
   D3D12_RESOURCE_STATE_PRESENT;`.
3. Composited success branch (`present.rs:1137`-`1148`): with `due`, keep `RENDER_TARGET ->
   COPY_SOURCE` and the readback, replace the `COPY_SOURCE -> PRESENT` barrier by `bb_state =
   D3D12_RESOURCE_STATE_COPY_SOURCE;`; without `due`, replace the `RENDER_TARGET -> PRESENT` barrier
   by `bb_state = D3D12_RESOURCE_STATE_RENDER_TARGET;`. The failure branch (`present.rs:1149`-`1151`)
   is unchanged (it returns the buffer to `PRESENT` for the copy path).
4. Copy path: replace `let b2 = ...; p.list.ResourceBarrier(&b2);` (`present.rs:1188`-`1189`) by
   `bb_state = D3D12_RESOURCE_STATE_COPY_DEST;`.
5. After the `if !composited { ... }` block, before the `XCL_TRACE_FRAME` comment (`present.rs:1191`):
   ```rust
   // the statistics panel: on XCL's back buffer, after the title's frame and any frame dump
   let hud = super::hud::active();
   let [before, after] = super::hud::tail_barriers(bb_state, hud);
   if let Some((b, a)) = before {
       p.list.ResourceBarrier(&[transition(&bb, b, a)]);
   }
   if hud {
       super::hud::record(&p.dev, &p.list, &bb, p.w, p.h, p.fmt, p.ring.cur, &mut p.hud);
   }
   if let Some((b, a)) = after {
       p.list.ResourceBarrier(&[transition(&bb, b, a)]);
   }
   ```
6. `frame_shown` (`present.rs:1259`): first line `super::hud::note_frame();`.
7. `wndproc` (`present.rs:374`-`396`): before `_ =>`, add
   `WM_KEYDOWN if super::hud::on_key(wp.0, lp.0) => LRESULT(0),`.
8. `window_thread` after `crate::hle::kbm::attach(hwnd.0);` (`present.rs:520`):
   `super::hud::start(hwnd.0 as isize, &title);` (`title` is `window_name()`, `present.rs:494`).

`config.rs`: field after `mouse_sensitivity` (`config.rs:82`):
```rust
    /// The statistics panel in the window's top-left corner: "full", "compact" or "off".
    /// `XCL_HUD` overrides; F3 cycles it while the window has the focus.
    pub hud: String,
```
and `hud: "full".into(),` in `Config::default` after `mouse_sensitivity: 1.0,` (`config.rs:199`).

Docs: the README and `cli-and-config.md` lines of section 9.

Tests:
- `hud/mod.rs`: `env_beats_config`, `invalid_env_falls_back_to_config`, `invalid_config_is_full`,
  `f3_down_toggles_and_repeats_do_not` (`is_toggle(0x72, 0)`, `!is_toggle(0x72, 1 << 30)`,
  `!is_toggle(0x71, 0)`), `tail_barriers_cover_every_present_path` (COPY_DEST, COPY_SOURCE,
  RENDER_TARGET, PRESENT, each with and without the HUD: 8 cases).
- `config.rs`: `hud_defaults_to_full` (default value and `hud = "full"` in the serialized TOML),
  `partial_file_without_hud_gets_full` (`toml::from_str::<Config>("gamertag = \"x\"")`).

Acceptance: whole suite passes on the PC; the acceptance run (section 12) shows the panel.

#### 10.9 U7 – GPU time per frame when telemetry measures it (optional, about 40 lines)

Depends on U6. Files: `crates/xcl/src/telemetry_gpu.rs`, `hud/sample.rs`.
- `telemetry_gpu.rs`: `static LAST_GPU_US: AtomicU64 = AtomicU64::new(u64::MAX);` In `write_frames`
  (`telemetry_gpu.rs:3217`), for each `f` with `f.gpu_ms == Some(ms)`, store `(ms * 1000.0) as u64`.
  `pub fn last_gpu_ms() -> Option<f32>`: None when `!xlog::telemetry_on(Tier::Frame)` or the value is
  `u64::MAX`, else `us / 1000`. It is reachable as `crate::report::last_gpu_ms()` (`report.rs:255`-`257`).
- `sample.rs`, every 4th tick: `snap.gpu_ms = crate::report::last_gpu_ms();`.
- Test (`telemetry_gpu.rs`): `last_gpu_ms_follows_the_newest_written_frame` (write two facts with
  3.0 and 4.5 ms through `write_frames` with a no-op sink; the raw atomic holds 4500).

#### 10.10 U8 – GPU utilisation through PDH (deferred)

Only after the acceptance run and a probe: a test binary or an ignored test that opens a PDH query
on `\GPU Engine(pid_<own pid>*)\Utilization Percentage` (fallback `\GPU Engine(*)\...` filtered by
the `pid_<pid>_` prefix) and times `PdhCollectQueryData` over 60 collections with the title running.
If the mean is under 2 ms: a `gpu_util.rs` in `hud/` (about 200 lines) owned by the sampler, 1 Hz,
buffer allocated once with room for 4 x the first instance count, value = max over engine
instances of type `3D` or `Compute` of this process; the `GPU` row appends ` 87%`. Any PDH error ->
None for the rest of the run. Its own decision; not part of v1.

### 11. Concurrency plan

| Wave | Units (in parallel) | Files written | Conflicts |
|---|---|---|---|
| 0 | U0 | `hud/*` (one line each), d3d12x `mod.rs` | — |
| 1 | U1, U2a, U2b | `hud/font.rs` / `hud/model.rs` / `hud/text.rs` | none: one file each |
| 2 | U3, U4, U5 | `hud/layout.rs` / `hud/sample.rs` + `code_use.rs` / `hud/draw.rs` + `present.rs` (3 words) | none: disjoint files |
| 3 | U6 | `hud/mod.rs`, `present.rs`, `config.rs`, `README.md`, `docs/guide/cli-and-config.md` | `present.rs` after U5 (sequential) |
| 4 | U7 (optional) | `telemetry_gpu.rs`, `hud/sample.rs` | after U6 |
| later | U8 (deferred) | `hud/gpu_util.rs`, `hud/sample.rs`, `hud/layout.rs` | after the probe |

Wave 2 needs wave 1 merged (types). Each unit builds on its own: U1-U5 add code that nothing calls
yet (`#![allow(dead_code)]` from U0 keeps the build quiet); U6 turns it on.

For an upstream PR (if the user ever wants one), the same order maps to the split CONTRIBUTING.md
asks for: (1) U0+U1+U2a+U2b, (2) U3+U4, (3) U5, (4) U6, (5) U7. `code_use` and the telemetry module
are fork-only today (`report.rs:250`-`257`), so U4's `in_use()` and U7 would need gating there
(claim).

### 12. Acceptance run on the PC (RTX 5080 16 GB, Windows 11)

1. Build: `cargo build --release --workspace`; tests: `cargo test -p xcl --lib hud`, `cargo test -p
   xcl --lib code_use`, `cargo test -p xcl --lib config`, then `cargo test --workspace --
   --test-threads=4`; report passed / failed / ignored and any `SKIPPED` (the two WARP tests must not
   skip).
2. Run the title with no `XCL_HUD` (default full) until the 3D scene shows; screenshot with `pcshot`.
   The screenshot must show, top-left, a dark translucent rounded panel with the game visible through
   it, and: `XCL`, the title name and a run time; `FPS` with a value (red at 2-3 fps) and a frame-time
   graph with columns; `CPU` and `sys` percentages with a violet graph; `RAM` and `sys` with a green
   graph; `VRAM` in red (over budget, as today) with `of <budget>`, an amber graph and the budget
   line under the top of the columns; `GPU NVIDIA GeForce RTX 50` (cut at 21); `Commit`; `PSO ...
   queued` (and `Failed` if any); `DXC ... run, ... done`; `Res <frame> > 1280x720`; `RT off`.
3. Press F3: compact panel (3 lines + graph), screenshot; F3 again: no panel, screenshot; F3 again:
   full. The game shows no reaction to F3 and `xcl.log` has `hud: compact`, `hud: off`, `hud: full`.
4. Cost, two runs of 120 s each on the same title and save: `XCL_TELEMETRY=frame XCL_HUD=off` and
   `XCL_TELEMETRY=frame XCL_HUD=full`. Compare the mean and 95th percentile of `present_ms` in the
   `frame` events (target: full minus off < 0.1 ms) and frames shown; read the `hud: 600 frames
   drawn, record mean ... µs, max ... µs` line (target: mean < 100 µs, max < 1 ms). If the title shows
   fewer than 600 frames in the run, run longer or note the line's absence.
5. `XCL_FRAME_DUMP=60` with the HUD on: the written `frame_<n>.bmp` has no panel.
6. Resize the window by dragging and maximise it: the panel keeps its share of the window; no
   `present: CreateSwapChainForHwnd` / resize error in `xcl.log`.
7. `XCL_HUD=off`: no `xcl-hud` thread (no `hud:` line but `hud: off`), panel absent.

### 13. What is left out

- GPU utilisation (U8, deferred behind a measurement); temperatures, clocks, fan speeds (vendor APIs).
- Drawing while the title does not present (a stalled title shows its last frame with the panel
  frozen; the title bar keeps updating).
- Sharp text when the window is not an integer multiple of the frame: DWM's stretch softens it;
  drawing in window space would need a window-sized swap chain and a scaled blit of the title's frame.
- HDR paper-white for FP16 output; per-monitor DPI (the process is DPI-unaware).
- Mouse interaction, dragging the panel, other corners, user-chosen rows or colours.
- Percentiles, per-queue or per-pass timings, DirectStorage throughput, draws per second.
- A HUD in `graphics = "null"` or `present = "off"` runs (no window).
- Changing the title bar text (it stays as it is).
