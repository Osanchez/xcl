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
