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
