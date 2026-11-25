# Telemetry and profiling for live runs — design and roadmap (2026-11-25)

**Status: DRAFT for review.** Nothing in here is built yet.

**Decided 2026-11-25:** D1 JSON Lines + manifest · D2 per-pass timing · D3 build both the cheap
cap probe and the per-shader counters, probe first · D4 automatic phase guessing with a
confidence value plus a manual override · D5 headless with the title's 30 fps frame clock kept ·
D6 **extend the existing `log.rs` and `report.rs` rather than adding a new module** · D7 no
overhead budget for now, revisit the moment any frame drops under 100 ms · D8 **fork-only,
permanently**, except the split-wave swap analysis (§5.6), which is built clean enough that it
could be contributed.

**All eight decisions are made.** This is ready to implement.

Because of D8 this is fork tooling, so it lives in `docs/guide/` (with `scripts/dev/`,
`reverse-toolkit/` and `.claude/`) rather than `docs/specs/`, which is upstream's area, and it
belongs on `dev` — never on a branch cut from `main`.

## Goal

Make a single run of a title produce a complete, machine-readable account of what happened —
where GPU time went, where CPU time went, what memory did, what was refused, what failed — so
that the cause of a slow frame can be named from the output of one run, without a profiler GUI,
without screenshots, and without a human watching it.

Secondary goal: make such a run as cheap as possible, so it can be repeated often and
unattended — no window, reduced or no presentation, and an optional uncapped frame clock.

### Non-goals

- Not a replacement for vendor profilers. Those stay the right tool for a single frame's
  hardware counters. This is for *attribution over a whole run*, which they do badly.
- Not a visualisation. Output is for a reader (human or model) that can read JSON.
- No change to what the title computes. Every producer here is off by default and must cost
  nothing measurable when off (**D7**).

## Why: the question this has to answer

A frame today is roughly 1000 ms where the target is 33 ms. That is a **~30× gap**, and the
size of the number is itself diagnostic: conservative barriers, per-submit CPU cost and
mid-frame resource creation are real but produce 20 % here and 2× there. Only a few things
produce 30×. The live hypotheses, with what would confirm or kill each:

| # | Hypothesis | Confirmed by |
|---|---|---|
| H1 | Translated shaders spill registers to scratch memory (the `switch (pc)` dispatcher over register arrays; `docs/specs/2026-11-24-direct-dxil.md` warns an unoptimised emission "would leave register arrays in scratch memory") | per-pass GPU time concentrated in shaders whose static translation shape used register arrays (§5.6 + §5.7) |
| H2 | Loops run to their safety cap instead of their real exit condition (`LOOP_GUARD`, 4 M iterations per lane) | loop-guard trip counters per shader (§5.8); or GPU time collapsing when the cap is lowered |
| H3 | Every console lane executes every instruction (`EXEC` only masks writes), so no work is skipped for inactive lanes | per-pass time scaling with lane count rather than active lanes; correlation with divergence in the static shape report |
| H4 | Work the console did ahead of time happens inside the frame (textures created on first read, pipelines built on demand) | `pipeline` and `resource` events landing inside a frame's span (§5.5) |
| H5 | Conservative barriers serialise work the console overlapped | pass GPU time summing to far less than frame GPU time — i.e. gaps between passes |
| H6 | CPU-side command-list translation is the limit | frame CPU time ≈ frame wall time with GPU idle |

H1–H3 can each produce 30×. H4–H6 cannot on their own. The instrumentation is therefore
prioritised to discriminate H1–H3 first.

**Explicitly not a cause:** the 64-lane/32-lane split. VIctory reports ~1 fps on an RX 6700 XT,
which runs wave64 natively, so the ~1000 ms exists with that tax absent. It is a multiplier on
NVIDIA, not the cause. (It still gets measured, §5.7, because it is a multiplier *we* pay.)

## What exists today

| Thing | Where | Shape |
|---|---|---|
| Text log, 5 levels, file + optional stderr | `crates/xcl/src/log.rs` | prose |
| End-of-run report | `crates/xcl/src/report.rs` (`report.json`) | structured, but a snapshot: stubs called, missing imports, modules, VFS, shader stats, DXC toolchain, audio, XGameRuntime, DirectStorage, crash, exit code, environment checks |
| Periodic memory line | `crates/xcl/src/host.rs` | prose, one line |
| fps every 300 frames | `crates/xcl/src/hle/d3d12x/present.rs` | prose |
| Frame clock override (`0` = uncapped) | `crates/xcl/src/hle/d3d12x/frame.rs` (`XCL_FRAME_INTERVAL_US`) | switch |
| Run length limit | `host.rs` (`XCL_RUN_SECONDS`) | switch |
| Allocation trace ≥ 1 MB | `crates/xcl/src/gpu/uma.rs` (`XCL_TRACE_MEM`) | prose |
| Frame image dumps | `present.rs` (`XCL_FRAME_DUMP`) | BMP |
| ~30 trace switches | `XCL_D3D12_TRACE`, `XCL_LIST_TRACE`, `XCL_PSO_TRACE`, `XCL_CONSOLE_LISTS_LOG`, `XCL_DRED`, `XCL_WATCH_DEVICE`, `XCL_WATCH_DMA`, `XCL_RECOMP_WHY`, `XCL_DIRECT_DXIL_STATS`, `XCL_SURVEY_*`, `XCL_CENSUS_*` | prose |
| GPU timestamp timing | `crates/xcl/src/gpu/recomp/runner.rs` (`dispatch_timed`, `D3D12_QUERY_HEAP_TYPE_TIMESTAMP`) | **tests only — not on the title's path** |
| Regression harness | `scripts/smoke.ps1` | regexes over the prose log |

### Gaps

1. **No GPU timing in a live run.** The mechanism exists in the test runner and is absent from
   the replay path. This is where the 30× hides, and it is gap number one.
2. **No time series.** `report.json` is end-of-run; fps and memory are prose lines at fixed
   intervals. A frame cannot be located in time, and nothing can be correlated.
3. **Perf data is prose scraped by regex** (`smoke.ps1`). Fragile, lossy, and unusable by a
   reader that was not told the phrasing.
4. **No attribution.** Nothing says which pipeline, pass or shader consumed a frame.
5. **No run identity.** Two runs cannot be compared with confidence: commit, adapter, driver,
   switches and title version are not recorded together.
6. **No headless mode.** A window and swapchain are always created, and closing the window ends
   the process.
7. **No visibility for H1–H3.** No static report of a shader's translation shape, no loop-guard
   trip counters.

## Design

### 5.1 Outputs — one directory per run

```
<logs>/run-<utc>-<commit12>/
    manifest.json     identity and configuration of this run (§5.2)
    events.jsonl      the event stream, one JSON object per line (§5.3)
    report.json       the existing end-of-run report, unchanged in shape, plus a summary block
    shaders.json      static translation shapes, written once per run (§5.6)
    xcl.log           the existing text log, unchanged
```

**D1 — output format.** Recommendation: **JSON Lines for the stream plus a separate manifest
and summary.** One object per line, flushed per line, so a run killed by a GPU reset still
yields everything up to the kill — which is exactly the run you most want to read. A single
JSON document would be truncated and unparseable in that case. A reader can also stream it
without loading the whole file.

### 5.2 `manifest.json` — makes runs comparable

Written at startup, before the title runs, so it exists even if the title dies immediately.

```json
{
  "schema": 1,
  "run_id": "2026-11-25T18:04:11Z-918d42c1f0aa",
  "xcl_version": "...", "commit": "918d42c…", "dirty": false,
  "rust": "1.100.0", "profile": "release",
  "os": "Windows 11 10.0.26300", "cpu": "AMD Ryzen 7 9800X3D", "cores": 8, "threads": 16,
  "ram_mb": 63488,
  "adapter": "NVIDIA GeForce RTX 5080", "driver": "32.0.16.2101", "vram_mb": 16384,
  "wave_lane_count_min": 32, "wave_lane_count_max": 32,
  "dxc": {"dxcompiler": "1.8.2502.11", "dxil": "1.8.2502.11"},
  "title": "...", "title_version": "...", "exe": "...",
  "switches": {"XCL_TELEMETRY": "pass", "XCL_HEADLESS": "1", "XCL_FRAME_INTERVAL_US": "0"},
  "started": "2026-11-25T18:04:11Z"
}
```

`wave_lane_count_*` come from `D3D12_FEATURE_DATA_D3D12_OPTIONS1`, so a reader can tell
immediately whether the split-wave path was in play. Every `XCL_*` variable actually read
during the run is recorded, not a hardcoded list — otherwise a measurement cannot be reproduced.

### 5.3 `events.jsonl` — the stream

Every line: `{"t": <ms since start, float>, "ev": "<type>", …}`. Types, with their fields:

| `ev` | fields | when |
|---|---|---|
| `frame` | `i`, `cpu_ms`, `gpu_ms`, `present_ms`, `wait_ms`, `lists`, `draws`, `dispatches`, `barriers` | once per presented frame |
| `pass` | `frame`, `list`, `i`, `gpu_us`, `kind` (`draw`/`dispatch`/`copy`/`clear`), `pipelines` (ids), `draws`, `barriers_before` | per barrier-delimited span, when pass timing is on |
| `list` | `frame`, `id`, `queue`, `gpu_us`, `cpu_us`, `translate_us`, `console_bytes` | per replayed command list |
| `pipeline` | `id`, `stage`, `records` (shader record hashes), `dxil_bytes`, `compile_ms`, `source` (`cache`/`dxc`/`direct`/`standin`) | on creation |
| `resource` | `kind`, `bytes`, `where` (`title`/`xcl`), `reason` (`first_read`/`explicit`), `ms` | on creation inside a frame (H4) |
| `memory` | `committed_mb`, `vram_local_mb`, `vram_nonlocal_mb`, `arenas_mb`, `pools_mb`, `sparse_mb`, `sparse_mapped_mb` | every N frames |
| `io` | `path`, `bytes`, `ms`, `queue`, `codec` (`none`/`zlib`/`bcpack`) | per DirectStorage request batch |
| `shader_refused` | `record`, `stage`, `reason`, `pc` | on refusal |
| `loop_guard` | `record`, `trips`, `iterations` | per shader per frame, when loop counters are on (§5.8) |
| `stub` | `dll`, `function`, `calls` | first call, then on a count threshold |
| `error` | `kind`, `detail`, `source`, `fatal` | §5.9 |
| `phase` | `name` (`boot`/`menu`/`loading`/`world`/`cutscene`), `from` | on a detected transition (§5.10) |

Rules:
- `frame` is always emitted when telemetry is on at all. Everything else is tiered (§5.11).
- Field names never change meaning; new fields may be added. `schema` in the manifest gates it.
- No nested objects beyond one level, no arrays of objects — keeps every line trivially
  greppable and cheap to parse line by line.

**Volume note:** at 1 fps a run of 5 minutes is ~300 frames. Per-pass timing for *every* frame
is therefore entirely affordable at the frame rates that matter now; the volume problem only
appears once the title runs fast, which is a good problem. §5.11 still defines the tiers.

### 5.4 GPU timing — the core of this work

The title records command lists in the console format and XCL replays them. Timestamps are
inserted at replay, so nothing about the title changes.

- One `D3D12_QUERY_HEAP_TYPE_TIMESTAMP` heap per frame in flight, sized for
  `2 × (max passes + max lists)`; `D3D12_QUERY_HEAP_TYPE_COPY_QUEUE_TIMESTAMP` for copy queues.
- `EndQuery` at each list's begin and end, and at each pass boundary. A **pass** is defined as
  the span between barrier batches on a list — it maps to how a renderer is actually structured
  and costs two timestamps rather than two per draw.
- `ResolveQueryData` into a readback buffer at the end of each list; the buffer is read
  **N frames later** (N = frames in flight + 1), never in the same frame, so nothing stalls.
- Ticks → microseconds with `GetTimestampFrequency()` per queue, re-read if the device is
  recreated.
- A pass whose resolve never arrives (device removed) is emitted with `gpu_us: null` rather
  than dropped, so a reader can see the hole.

**D2 — timing granularity default.** Recommendation: **per-pass**, given the volume note above.
Per-draw timing stays unimplemented until something needs it; per-list only is the fallback if
pass detection proves unreliable.

Frame CPU time, present time, and the wait on the frame clock are taken on the CPU side in
`present.rs` and `frame.rs`, so H6 (CPU-bound) is separable from H1–H3 (GPU-bound) in the
`frame` event alone — that single event answers "is this CPU or GPU" before any pass is read.

### 5.5 Resource and pipeline events (H4)

`pipeline` on every creation, with `source` distinguishing a cache hit from a DXC compile from a
stand-in. `resource` on a texture or buffer created with `reason: first_read`. Because both
carry `t`, a reader can intersect them with `frame` spans and see directly whether the frame was
spent building things the console had built ahead of time.

### 5.6 `shaders.json` — static translation shape (H1, H3)

Written once per run from the recompiler, for every shader the run translated or loaded from
cache. This is the cheap half of the H1/H3 answer: it needs no GPU and no profiler, and it is
the thing neither project currently has.

Per record: `record` (hash), `stage`, `wave64`, `form` (`per_lane` / `switch_pc`), and:

- `register_arrays`: whether the emission used register arrays — the construct the direct-DXIL
  spec warns can land in scratch memory (**H1**)
- `scratch_hint`: bytes of scratch the form could require, if derivable
- `instructions`, `blocks`, `loops`, `max_loop_depth`, `unstructured` (needed `pc` dispatch)
- `divergent_branches`: count of branches on non-uniform conditions (**H3**)
- `exchanges`: for a wave64 program on a 32-lane adapter, the split-form exchange count by kind
  — `ballot`, `readlane` (1 word), `bpermute` (32 words), `coherent_load` (≤ 16 words) — and
  `waves_per_group`, which selects the cheap barrier variant or the expensive counter variant
- `dxil_bytes`, `compile_ms`

Correlating `shaders.json` with the measured `pass` times is the whole attribution loop: rank
shaders by GPU microseconds, then read off which shape they have. **This subsumes the separate
per-program exchange-cost report discussed earlier** — it is the same data, in the same file.

### 5.7 Generated-code comparison (optional, AMD only)

For any shader, the console's original instruction count is already obtainable (`xcl disasm`).
Where an AMD adapter is present, the DXIL can additionally be run through an offline analyser
to recover generated instruction count, register usage and occupancy, and the result stored
beside the static shape. **This cannot be done on NVIDIA** — no public instruction set, no open
compiler — so it is explicitly an AMD-only enrichment and must not be a dependency of anything
else. Noted because the machine has an AMD integrated adapter that can serve as the oracle.

### 5.8 Loop-guard telemetry (H2)

`LOOP_GUARD` already caps every translated loop at 4 M iterations per lane. What is missing is
knowing whether the cap is ever *reached*.

A `XCL_LOOP_COUNTERS=1` build of a shader increments a per-record counter in a small UAV when a
loop exits via the guard rather than its own condition, and optionally accumulates total
iterations. Emitted as `loop_guard` events. This costs real GPU time and changes the shader, so
it is a diagnostic mode, never default, and runs are never compared across it.

Cheaper first probe, no shader change needed: make the guard limit settable
(`XCL_LOOP_GUARD=<n>`) and observe whether a much lower limit collapses frame time. If it does,
H2 is confirmed before writing any counters — this is the approach VIctory's
`VICTORY_SHADER_LOOP_LIMIT` takes, and it is nearly free.

**D3 — DECIDED: build both, run the probe first.** The settable cap (`XCL_LOOP_GUARD=<n>`) is
built and run first because it is nearly free and the answer is close to binary. The per-shader
counters are built regardless, because knowing *which* shaders trip the guard is needed either
way once the probe says the guard is being reached at all.

### 5.9 Errors and failures — one channel

Everything that currently ends in a prose line goes to `error` events with a `kind`, so nothing
has to be found by reading: `device_removed` (with DRED allocation and in-flight commands where
available), `gpu_fault`, `pipeline_rejected`, `shader_refused`, `unresolved_import`,
`stub_called_fatal`, `vfs_unmapped`, `io_failed`, `title_exception` (the title's own faults,
with the faulting module and symbol from the existing crash reporting), `loop_guard_tripped`,
`timeout`. `fatal: true` marks the one that ended the run. The existing prose lines stay — this
adds a channel, it does not replace the log.

### 5.10 Phase detection

An event stream is far more useful if a reader knows whether a frame was a menu or the world.
Phases are derived from signals XCL already sees: first `PresentX`, the title's own loading
reads through DirectStorage, the swapchain contents changing size, audio starting. Heuristic,
explicitly labelled as such in the field (`phase.confidence`), never load-bearing.

**D4 — DECIDED: both.** Implement the heuristic with its `confidence` field, and also accept
`XCL_PHASE_MARK=<name>` written at any time by an external tool, so a scripted run can label
its own segments exactly.

### 5.11 Tiers and overhead

`XCL_TELEMETRY=<tier>`, cumulative:

| tier | emits | expected overhead |
|---|---|---|
| `off` (default) | nothing; no query heaps created, no file opened | zero |
| `frame` | manifest, `frame`, `memory`, `error`, `phase`, `shader_refused` | < 1 % |
| `pass` | + `pass`, `list`, `pipeline`, `resource`, `io`, `shaders.json` | target ≤ 5 % |
| `all` | + `stub`, per-draw detail, `loop_guard` if built in | unbounded; diagnostic only |

**D7 — DECIDED: no budget for now.** At ~1000 ms a frame the instrument's cost is rounding
error, so a budget would be ceremony. **Trigger for revisiting, so that "later" actually
arrives: the first time any frame is recorded under 100 ms.** At that point run the same scene
at `off` and at `pass`, compare frame GPU time, and move to sampling (every Nth frame) if the
difference is material. Until then every frame is timed in full.

### 5.12 Headless and reduced cost

Three separable switches, because they fail differently:

- `XCL_HEADLESS=1` — create no window and no swapchain. `PresentX` accounting, the frame clock
  and all timing continue; the presented resource is simply not blitted. Needs a termination
  path that does not depend on a window close: `XCL_RUN_SECONDS` already exists.
- `XCL_WINDOW=<w>x<h>` — create a window of that size (`1x1` for minimum cost) instead of the
  monitor-derived default.
- `XCL_PRESENT_SKIP=1` — keep the window, skip the blit. Useful to separate the blit's cost
  from the window's existence.

**Risk that must be tested, not assumed:** if the title queries the output size and adapts its
render resolution, a 1×1 window changes what is rendered and invalidates every comparison. The
test in §7 settles this by comparing `pass` times at `1x1` against the default size; if they
differ beyond noise, headless runs are only comparable to other headless runs, and the spec says
so in the manifest rather than pretending otherwise.

Input: with no window there is no keyboard-and-mouse gamepad. A gamepad still works, and a run
that needs no input needs neither.

**D5 — DECIDED: headless, with the title's own 30 fps frame clock kept.** `XCL_HEADLESS=1`, and
`XCL_FRAME_INTERVAL_US` left alone.

This is the more faithful choice and it costs nothing today: the clock is a deadline the title
waits on, and at ~1000 ms a frame the title is permanently late, so the cap never throttles
anything. It only begins to bind once a frame drops under 33 ms — at which point the capped
behaviour is the one worth measuring anyway.

It also turns `frame.wait_ms` into a progress signal: while it stays at zero the title is never
catching the clock, and the first run where it rises above zero is the first run that beat 30 fps
on that scene. An uncapped run can be taken later as a one-off to measure raw cost, recorded in
the manifest and never compared against capped runs.

## Switch summary

| switch | effect |
|---|---|
| `XCL_TELEMETRY=off\|frame\|pass\|all` | tier (default `off`) |
| `XCL_TELEMETRY_DIR=<path>` | run directory root (default the logs folder) |
| `XCL_TELEMETRY_SAMPLE=<n>` | emit `pass` every nth frame |
| `XCL_HEADLESS=1` | no window, no swapchain |
| `XCL_WINDOW=<w>x<h>` | window size override |
| `XCL_PRESENT_SKIP=1` | window, no blit |
| `XCL_LOOP_GUARD=<n>` | loop iteration cap (default 4 M) |
| `XCL_LOOP_COUNTERS=1` | loop-guard trip counters in translated shaders |
| `XCL_PHASE_MARK=<name>` | externally label the current phase |

Existing switches (`XCL_FRAME_INTERVAL_US`, `XCL_RUN_SECONDS`, `XCL_ADAPTER`, `XCL_TRACE_MEM`,
`XCL_RECOMP_WHY`, `XCL_DRED`, …) are unchanged and feed the new events where they overlap.

## Deliverables — the PR split

**D6 is decided: extend `log.rs` and `report.rs`** rather than adding a `telemetry/` module.
The upstream-merge cost is real and accepted; it is blunted by appending the new code as
separate functions near the end of each file and calling them from as few places as possible, so
a conflict lands in an added block rather than inside a function upstream has rewritten.

**D8 is decided: fork-only, permanently.** That removes the upstream constraints — no ~400-line
ceiling, no `xcl-smoke` regression blocks, no description checks — so the parts below are a work
order rather than a pull-request stack, and they all land on `dev`.

It also makes one design rule load-bearing, because every upstream merge now has to survive this
code: **touch upstream files as little as physically possible.** New code goes in appended
blocks at the end of a file; the call sites in `present.rs`, `host.rs`, `frame.rs`, the replay
path and the recompiler should each be a single line, never logic inline. Upstream churns hard (12,865 commits
on `main`, hourly sync into `dev`), and a conflict in a one-line call site is trivial while a
conflict in fifty lines of interleaved timing code is a recurring tax forever. Line estimates
below are kept because they are still a useful sizing guide.

| # | PR | Contents | Lines (est.) |
|---|---|---|---|
| 1 | **Telemetry core, not yet called** | appended to `log.rs` and `report.rs` (D6) — run directory, `manifest.json`, JSONL writer with per-line flush, event types, tier parsing, `XCL_TELEMETRY*` switches. No producers. | ~350 |
| 2 | **Frame and memory time series** | `frame` and `memory` events from `present.rs` and `host.rs`; existing prose lines untouched. First useful output: answers H6 (CPU vs GPU) alone. | ~150 |
| 3 | **GPU timing** | query heaps on the replay path, pass and list timing, deferred resolve, `pass`/`list` events. The core. | ~400 |
| 4 | **Static shape report** | `shaders.json` from the recompiler, including the split-wave exchange counts. Answers the static half of H1/H3 with no GPU. | ~300 |
| 5 | **Headless and reduced cost** | `XCL_HEADLESS`, `XCL_WINDOW`, `XCL_PRESENT_SKIP`; window-optional present path. | ~250 |
| 6 | **Error channel** | `error` and `phase` events; device removal, GPU fault, refusals, title exceptions routed into it. | ~250 |
| 7 | **Loop probe, then counters** | `XCL_LOOP_GUARD=<n>` settable cap first, run immediately; then `XCL_LOOP_COUNTERS` per-shader trip counters regardless of the probe's answer (D3). | ~120 + ~250 |
| 8 | **`smoke.ps1` reads events** | replace the log regexes with `events.jsonl`, keeping the printed block byte-identical in shape. | ~120 |

Part 8 touches a **required PR check** (`PR checks` refuses a runtime PR without `xcl-smoke`
blocks), so it goes last, alone, and keeps the output format identical.

**Fastest path to answering the 30× question: parts 1 → 2 → 3, then 4.** Part 2 alone separates
CPU-bound from GPU-bound. Part 3 localises the cost. Part 4 explains it.

## Test plan

Per the repo's rules: a bug fix gets a test that fails without it, new logic gets a test of it,
GPU tests call `gpu::recomp::runner::skip_test(...)` and return when the adapter cannot run them.

| # | Tests |
|---|---|
| 1 | unit: manifest fields present and valid JSON; JSONL writer emits one object per line and flushes (read back mid-write); tier parsing incl. invalid values; run directory created once; `off` opens no file |
| 2 | e2e on `tests/testtitle`: frame count in `events.jsonl` equals the `frames shown` the log reports; `memory` events monotonically timestamped |
| 3 | GPU test: a known dispatch timed through the new path matches `runner::dispatch_timed` within tolerance; a device-removal case emits `gpu_us: null` and does not hang; resolve never read in the same frame (asserted by construction) |
| 4 | unit: static shape over a fixture set of console programs — exchange counts match hand-computed values for a wave64 program with `bpermute` and with lane-local masks only; `register_arrays` set exactly for the `switch_pc` form |
| 5 | e2e: `tests/testtitle` runs to completion with `XCL_HEADLESS=1` and no window is created; `XCL_WINDOW=1x1` produces a window of that size; **comparison test** of pass times at `1x1` vs default to settle the resolution-adaptation risk |
| 6 | unit: each `error` kind serialises with its fields; e2e: a forced unresolved import produces one `error` event with `fatal` set correctly |
| 7 | GPU test: a shader with a known loop runs to its own exit at a high cap and trips the guard at a low one |
| 8 | `scripts/test_smoke_artifacts.tests.ps1` extended: the block built from `events.jsonl` equals the block built from the same run's log |

Whole suite before each PR: `cargo test --workspace -- --test-threads=4`, with real counts and
`SKIPPED` lines reported. Runtime PRs (parts 2–7) each need the two `xcl-smoke` regression blocks.

## Risks

| Risk | Mitigation |
|---|---|
| Timing changes what is measured | `off` is default and creates nothing; 5 % budget verified by A/B (§5.11); sampling fallback |
| Query resolve stalls the pipeline | resolve is read N frames later, never in-frame; asserted in tests |
| Headless changes the title's render resolution | measured, not assumed (§5.12, test 5); recorded in the manifest; headless runs compared only to headless runs |
| Uncapped frame clock changes title pacing | recorded in the manifest; capped and uncapped runs never compared |
| Event volume at high frame rates | tiers and `XCL_TELEMETRY_SAMPLE`; not a problem at today's frame rates |
| `smoke.ps1` is a required check | part 8 last, alone, output shape unchanged, with a test that both sources agree |
| **Fork-only (D8): upstream merges conflict with this code forever** | all logic inside `telemetry/`; one-line call sites in upstream files; the hourly sync opens a `sync-conflict` issue rather than guessing, so a conflict is visible, not silent |
| **Fork-only (D8): no upstream review of the measurement code** | the test plan below is therefore not optional — it is the only check on correctness, especially for the GPU timing in part 3, where a wrong number is worse than no number |
| Phase detection is guesswork | labelled with `confidence`, never load-bearing, with a manual override |

## Decisions needed

| | Decision | Status |
|---|---|---|
| **D1** | Output format | **DECIDED** — JSON Lines stream + separate manifest and summary |
| **D2** | Timing granularity | **DECIDED** — per-pass (barrier-delimited); per-list as fallback if pass detection proves unreliable |
| **D3** | Loop hypothesis approach | **DECIDED** — build both; run the settable-cap probe first |
| **D4** | Phase detection | **DECIDED** — heuristic with `confidence`, plus a manual `XCL_PHASE_MARK` override |
| **D5** | Profiling-run defaults | **DECIDED** — `XCL_HEADLESS=1`, title's 30 fps frame clock kept |
| **D6** | Where this lives | **DECIDED** — extend `log.rs` and `report.rs`; merge cost accepted, mitigated by appending rather than interleaving |
| **D7** | Overhead budget | **DECIDED** — none for now; revisit at the first frame under 100 ms |
| **D8** | Upstream posture | **DECIDED** — fork-only, permanently |

### Consequence of D8 — resolved 2026-11-25

§5.6's static shape report absorbs the per-program split-wave exchange analysis, which was
previously planned as an upstream contribution on the `shaders/wave64-exchange-report` branch
(cut from `main`). Under D8 that analysis becomes fork-only too, which means the nine open
upstream wave64 pull requests gain nothing from it and the 32-lane measurement nobody has stays
unpublished. **Resolved: that one piece is built clean enough to be contributed** — self-contained, no fork
tooling tangled in, on a branch cut from `main` (`shaders/wave64-exchange-report`). Whether it
is actually sent is decided once its numbers are seen; everything else stays fork-only.

Two things stay separable there: the **tool** contains no title data and is safe to share, while
the **numbers it produces for this title** are derived from the game's shaders. Upstream already
publishes aggregate counts of this kind, but what to publish is a separate decision from whether
to contribute the tool.

## What this produces, concretely

After parts 1–4, one five-minute headless run yields: a manifest naming the commit, adapter,
driver and every switch; a few hundred `frame` events splitting wall time into CPU, GPU, present
and frame-clock wait; a few thousand `pass` events with GPU microseconds and the pipelines in
each; one `pipeline` event per creation with its source; a memory time series; every refusal and
error with its reason; and a static shape for every shader translated.

From that, these questions are answerable by reading the files, with no GUI and no screenshots:

- is a frame CPU-bound or GPU-bound, and by how much
- which ten passes consume the frame, and which shaders they run
- do those shaders use the scratch-prone form (H1)
- do they trip the loop guard (H2)
- how divergent are they, and how many split-wave exchanges do they pay (H3)
- is the frame building resources or pipelines that should have been built earlier (H4)
- does pass time sum to frame GPU time, or are there gaps (H5)
