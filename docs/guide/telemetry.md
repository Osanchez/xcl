# Telemetry and profiling for live runs: design and roadmap (2026-11-25)

**Status: awaiting the user's final review. Nothing is built.**

Fork tooling (upstream has no telemetry of its own): it lives in `docs/guide/`, on `dev`, never on
a branch cut from `main`. Checked against the tree at `a67c592`.
**Decisions (2026-11-25)**
- D1: JSON Lines (one JSON object per line) for the event stream, plus a separate manifest.
- D2: GPU timing per pass (a pass is the span between barrier batches); per list as a fallback.
- D3: build both the cheap loop-cap probe and the per-shader loop counters, probe first.
- D4: automatic phase guessing with a confidence value, plus a manual override.
- D5: no-window runs keep the title's own 30 fps frame clock.
- D6: extend the existing `log.rs` and `report.rs`; append the new code and call it from one line.
- D7: measure the overhead from part 2 on, on the test title; report it; sample if it exceeds 5 %.
  This replaces the earlier "no budget for now".
- D8: fork-only, except the split-wave report (section 5.6), which stays clean enough to share
  from a branch cut from `main`.

## Goal

One run of a title should leave a complete, machine-readable account: where GPU time went, where
CPU time went, what memory did, what was refused, what failed. Then the cause of a slow frame can
be named from that output, without a profiler GUI, screenshots or a person watching.

Non-goals: replacing vendor profilers (they stay the tool for one frame's hardware counters); any
visualisation; any change to what the title computes. Every producer here is off by default.

## Why: the question this has to answer

Numbers we have, and where each comes from:
- **No baseline has been measured on this machine.** Nothing in this document is a measurement.
- 1 fps is VIctory's figure on an RX 6700 XT (a different project, a different card).
- XCL's `README.md:47` (at `a67c592`) reports about 5 fps in the 3D world on an RTX 5090 with
  pull request #2057. Its 14 fps is an average over a whole run, menus and loading included, so
  it is not a world figure.
- The target is 30 fps. From the README figure the gap in the world is about 6x on that card.
  On this machine it is unknown.

Causes are all open. None is ranked before a measurement. What would confirm each one:

| # | Hypothesis | Confirmed by |
|---|---|---|
| H1 | Translated shaders spill registers to scratch memory (`docs/specs/2026-11-24-direct-dxil.md:49` explains why register arrays can land there) | pass GPU time concentrated in shaders whose static shape uses register arrays (5.6) |
| H2 | Loops run to their safety cap instead of their real exit | loop trip counters (5.7), or GPU time falling when the cap is lowered |
| H3 | Every console lane runs every instruction, so inactive lanes save nothing | pass time following lane count, not active lanes; the divergence count in 5.6 |
| H4 | Work the console did ahead of time happens inside the frame (textures created on first read, pipelines built on demand) | `pipeline` and `resource` events inside a frame's span |
| H5 | Conservative barriers serialise work the console overlapped | gaps between passes inside the frame's GPU span |
| H6 | CPU-side command-list translation is the limit | frame CPU time close to frame wall time while the GPU idles |
| H7 | Split wave64: the console runs 64 lanes per wave, the GPU here runs 32, so XCL splits each wave and pays exchanges between the halves | exchange counts in 5.6 against pass time; A/B runs in "Do these first" |
| H8 | Refused shaders or pipelines are replaced by stand-ins or skipped, so the frame does less or different work | `shader_refused` and `pipeline` events with their HRESULT (a Windows error code) |

## What exists today

| Thing | Where |
|---|---|
| Text log, 5 levels, file and optional stderr | `crates/xcl/src/log.rs:52` (`init`), `:72` (`write`) |
| End-of-run report `report.json` (a snapshot, not a series) | `crates/xcl/src/report.rs:170` |
| Periodic memory line (committed, video memory, arenas, sparse pools) | `crates/xcl/src/host.rs:75` |
| fps line every 300 frames | `crates/xcl/src/hle/d3d12x/present.rs:123` |
| Frame clock override `XCL_FRAME_INTERVAL_US` (`0` removes the cap) | `crates/xcl/src/hle/d3d12x/frame.rs:46` |
| Run length limit `XCL_RUN_SECONDS` | `crates/xcl/src/host.rs:106` |
| Device-removal diagnostics, `dred.rs` | `crates/xcl/src/hle/d3d12x/dred.rs:35` (`on`), `:41` (`label`), `:92` (`reset`) |
| GPU timestamps for tests only (`dispatch_timed`) | `crates/xcl/src/gpu/recomp/runner.rs:1652` |
| Timestamp pool for the title's own timestamp writes | `crates/xcl/src/hle/d3d12x/list_x.rs:2432`, `:2443` |
| Window size from config (`window_width`, `window_height`); position switch `XCL_WINDOW_POS` | `crates/xcl/src/config.rs:57`, `crates/xcl/src/hle/d3d12x/present.rs:519` |
| Present path with a compositor and a present ring | `crates/xcl/src/hle/d3d12x/present.rs:582` (`PresentRing`), `:715` (`Compositor`), `:624` (`XCL_PRESENT_SYNC`) |

Gaps: no GPU timing in a live run (only the test runner has it); no time series (`report.json` is
end-of-run, fps and memory are prose); perf data read by regex from prose; no attribution to a
pass or shader; no run identity; no no-window mode; no view of H1 to H3 and H7.

## Design

### 5.1 Outputs: one directory per run

```
<logs>/run-<utc>-<commit12>/
    manifest.json     identity and configuration of this run (5.2)
    events.jsonl      the event stream, one JSON object per line (5.3)
    report.json       the existing end-of-run report, plus a summary block
    shaders.json      static translation shapes, written once per run (5.6)
    xcl.log           the existing text log, unchanged
```

D1: each line is flushed as written, so a run killed by a GPU reset still yields everything up to
the kill. One JSON document would be truncated and unparseable.

Run directories hold title paths and names: they stay on the machine, never committed.

### 5.2 `manifest.json`: makes runs comparable

Written at startup, so it exists even if the title dies at once. Nested objects are allowed here;
events stay flat (5.3).

```json
{
  "schema": 1,
  "run_id": "2026-11-25T18:04:11Z-918d42c1f0aa",
  "xcl_version": "...", "commit": "918d42c...", "dirty": false,
  "rust": "...", "profile": "release",
  "os": "Windows 11 10.0.26300", "cpu": "...", "cores": 8, "threads": 16, "ram_mb": 63488,
  "adapter": "...", "driver": "...", "vram_mb": 16384,
  "wave_lane_count_min": 32, "wave_lane_count_max": 32,
  "dxc": {"dxcompiler": "...", "dxil": "..."},
  "title": "...", "title_version": "...", "exe": "...",
  "switches": {"XCL_TELEMETRY": "pass", "XCL_HEADLESS": "1"},
  "started": "2026-11-25T18:04:11Z"
}
```

`wave_lane_count_*` come from `D3D12_FEATURE_DATA_D3D12_OPTIONS1`, so a reader sees at once
whether the split-wave path was in play. Every `XCL_*` variable actually read is recorded, not a
fixed list; otherwise a measurement cannot be reproduced. Under D5 the example does not set
`XCL_FRAME_INTERVAL_US`: the title's frame clock is left alone.

### 5.3 `events.jsonl`: the stream

Every line: `{"t": <ms since start>, "ev": "<type>", ...}`.

| `ev` | fields | when |
|---|---|---|
| `frame` | `i`, `cpu_ms`, `gpu_ms`, `present_ms`, `wait_ms`, `lists`, `draws`, `dispatches`, `barriers` | each presented frame |
| `pass` | `frame`, `list`, `i`, `queue`, `start_tick`, `end_tick`, `kind` (`draw`/`dispatch`/`copy`/`clear`), `pipelines` (ids), `draws`, `barriers_before` | each barrier-delimited span, at tier `pass`; ticks null if unavailable |
| `list` | `frame`, `id`, `queue`, `start_tick`, `end_tick`, `cpu_us`, `translate_us`, `console_bytes`, `resubmitted` | each replayed command list, from tier `frame` |
| `clock` | `queue`, `gpu_tick`, `cpu_qpc`, `frequency` | at start and every 60 frames, from `GetClockCalibration` |
| `pipeline` | `id`, `stage`, `records` (shader hashes), `dxil_bytes`, `compile_ms`, `source` (`cache`/`dxc`/`direct`/`standin`), `hresult` on failure | on creation |
| `resource` | `kind`, `bytes`, `where` (`title`/`xcl`), `reason` (`first_read`/`explicit`), `ms` | on creation inside a frame (H4) |
| `memory` | `committed_mb`, `vram_local_mb`, `vram_nonlocal_mb`, `arenas_mb`, `pools_mb`, `sparse_mb`, `sparse_mapped_mb` | every N frames |
| `io` | `path`, `bytes`, `ms`, `queue`, `codec` | each DirectStorage request batch |
| `shader_refused` | `record`, `stage`, `reason`, `hresult`, `pc` | on refusal |
| `loop_guard` | `record`, `trips`, `iterations` | per shader per frame, when counters are on (5.7) |
| `stub` | `dll`, `function`, `calls` | first call, then at count thresholds |
| `error` | `kind`, `detail`, `source`, `hresult`, `fatal` | see 5.8 |
| `phase` | `name`, `from`, `confidence` | on a detected or marked transition (5.9) |

Rules:

- `frame` is always emitted when telemetry is on. Everything else is tiered (5.10).
- Field names never change meaning; new fields may be added. `schema` gates the format.
- Events stay flat: no nested objects, no arrays of objects. Every line stays easy to grep.
  (The array of pipeline ids in `pass` holds plain numbers.)
- A refusal carries the HRESULT the driver or compiler returned, for example `0x8007000E`, seen
  on NVIDIA Blackwell for a groupshared subtract. The code is an example of what the field holds,
  not a finding of this plan.

### 5.4 GPU timing: the core of this work

The title records console command lists and XCL replays them. Timestamps are added at replay.

- **Own query heaps, owned by part 2.** Telemetry creates its own
  `D3D12_QUERY_HEAP_TYPE_TIMESTAMP` heaps, one per frame in flight. It never uses the pool in
  `list_x.rs:2432`: that pool is shared by every caller and has 4096 slots handed out round robin
  (`list_x.rs:2443`, `:2513`), so telemetry entries would be overwritten by the title's own.
- **Part 2 stamps each list** with a start and an end. `frame.gpu_ms` is then real: the union of
  the calibrated list intervals, or null when unavailable. **Part 3 adds pass boundaries.** A pass
  is the span between barrier batches; it costs two timestamps, not two per draw.
- `ResolveQueryData` into a readback buffer at the end of each list, read N frames later
  (N = frames in flight + 1), never in the same frame, so nothing stalls.
- **Ticks are per queue.** `GetTimestampFrequency()` is read per queue (again if the device is
  recreated). Each queue's `GetClockCalibration` is logged as a `clock` event at start and every
  60 frames, and intervals on different queues are compared only after calibration. Direct and
  compute queues come first. Copy queues (`D3D12_QUERY_HEAP_TYPE_COPY_QUEUE_TIMESTAMP`) are
  timed only when the device supports copy-queue timestamps; otherwise their ticks are null.
- **Query slots belong to a submission, not a recording.** A list submitted again before its
  results were read is emitted with null ticks and `resubmitted: true`. Bundles get no timestamps.
- **Intervals can overlap**, across queues or within one. Events report `start_tick`, `end_tick`
  and the queue; a reader never adds durations to get a total, it takes the union.
- An interval whose resolve never arrives (device removed) is emitted with null ticks.

Frame CPU, present and clock-wait times are taken in `present.rs` and `frame.rs`, so the `frame`
event alone says whether a frame is CPU- or GPU-bound (H6).

### 5.5 Resource and pipeline events (H4, H8)

`pipeline` on every creation, with `source` telling a cache hit, a DXC compile, a direct-DXIL
build and a stand-in apart. `resource` on a texture or buffer created with `reason: first_read`.
Both carry `t`, so a reader can intersect them with `frame` spans and see whether a frame was
spent building what the console built ahead of time.

### 5.6 `shaders.json`: static translation shape (H1, H3, H7)

Written once per run from the recompiler, for every shader translated or loaded from cache. It
needs no GPU and no profiler. Per record: `record` (hash), `stage`, `wave64`, `form`
(`per_lane` or `switch_pc`), and

- `register_arrays`: whether the emission used register arrays (H1);
- `scratch_hint`: bytes of scratch the form could need, if derivable;
- `instructions`, `blocks`, `loops`, `max_loop_depth`, `unstructured` (needed `pc` dispatch);
- `divergent_branches`: branches on non-uniform conditions (H3);
- `exchanges`: for a wave64 program on a 32-lane GPU, the split-form exchange count by kind
  (`ballot`, `readlane`, `bpermute`, `coherent_load`) and `waves_per_group`;
- `dxil_bytes`, `compile_ms`.

Joining `shaders.json` to the measured `pass` times is the attribution loop: rank shaders by GPU
microseconds, then read off their shape. Read `scratch_hint` against `XCL_SPLIT_SCRATCH_MB`
(default 256, `crates/xcl/src/hle/d3d12x/real/dispatch_chain.rs:24`), the bound upstream puts on
split-dispatch scratch.

**The split-wave part of this report stays clean:** self-contained, no fork tooling in it, built
so it could be shared from a branch cut from `main`. Whether it is sent is decided once its
numbers are seen. The tool holds no title data and is safe to share. The numbers it produces for
a title come from that title's shaders, so what to publish is a separate decision.

### 5.7 Loop telemetry (H2)

The recompiler caps every translated loop per lane: `xcl_budget = 1u << 16` (65,536) in the
shader prelude (`crates/xcl/src/gpu/recomp/emit.rs:6220`). The direct DXIL path has its own
guard: `LOOP_BUDGET = 1 << 16` (`crates/xcl/src/gpu/dxil/lower.rs:36`), counted down in
`crates/xcl/src/gpu/dxil/kernel.rs:358`. Any other figure in `HANDOFF.md` is stale. Neither cap
is a switch today.

- **Probe first (D3).** Make the cap settable (`XCL_LOOP_GUARD=<n>`) on both paths and see
  whether a much lower value shortens frames. It is nearly free and the answer is close to
  yes or no.
- **Counters, built regardless (D3).** With `XCL_LOOP_COUNTERS=1`, a shader increments a
  per-record counter in a small UAV (a GPU buffer) when a loop exits by the guard instead of its
  own condition. These are emitted as `loop_guard` events. This changes the shader and costs GPU
  time, so it is a diagnostic mode, and runs with it are never compared with runs without it.
- **Caches.** Both switches change generated code, so on both paths they join the list of
  variables that bypass the shader caches (`cache_allowed`, `crates/xcl/src/gpu/recomp/pso.rs:804`).
  Test 5 checks that a changed value is never served from cache.

### 5.8 Errors: one channel

Every failure that ends in a prose line also becomes an `error` event with a `kind`:
`device_removed` (with the DRED data that `dred.rs` already collects; do not reimplement it),
`gpu_fault`, `pipeline_rejected`, `shader_refused`, `unresolved_import`, `stub_called_fatal`,
`vfs_unmapped`, `io_failed`, `title_exception` (module and symbol from the existing crash
reporting), `loop_guard_tripped`, `timeout`. `fatal: true` marks the one that ended the run.
The prose lines stay.

### 5.9 Phase detection

A reader needs to know whether a frame was a menu or the 3D world. Phases (`boot`, `menu`,
`loading`, `world`, `cutscene`) are guessed from signals XCL already sees: the first present, the
title's loading reads through DirectStorage, swapchain size changes, audio starting. This is a
heuristic and is never load-bearing: every `phase` event carries `confidence`.

D4: `XCL_PHASE_MARK=<name>` lets a scripted run label its own segments exactly.

### 5.10 Tiers and overhead

`XCL_TELEMETRY=<tier>`, cumulative:

| tier | emits | overhead |
|---|---|---|
| `off` (default) | nothing: no query heaps, no file | none |
| `frame` | manifest, `frame`, `list`, `clock`, `memory`, `error`, `phase`, `shader_refused`, `pipeline` refusals | measured, see below |
| `pass` | adds `pass`, `pipeline`, `resource`, `io`, `shaders.json` | measured, see below |
| `all` | adds `stub`, per-draw detail, `loop_guard` if built | diagnostic only |

**D7.** From part 2 on, run the test title (`tests/testtitle`) with telemetry `off` and then on,
compare, and report the difference in the run's own notes. If it exceeds 5 %, move to sampling
(`XCL_TELEMETRY_SAMPLE=<n>`, every nth frame). No overhead figure is claimed before that run.

### 5.11 No-window mode and reduced cost

Three separate switches, because they fail differently:

- `XCL_HEADLESS=1`: create no window and no swapchain. Present accounting, the frame clock and
  all timing continue; the presented image is just not drawn. Needs an end that does not depend
  on closing a window: `XCL_RUN_SECONDS` exists (`host.rs:106`).
- `XCL_WINDOW_SIZE=<w>x<h>`: a window of that size instead of the config default (today
  `config.rs:57`). The name matches `XCL_WINDOW_POS`. Read it in `config.rs`, not in new code.
- `XCL_PRESENT_SKIP=1`: keep the window, skip the draw. It separates the cost of drawing from
  the cost of having a window, and it must be checked against `XCL_PRESENT_SYNC`
  (`present.rs:624`).

**Re-estimate before starting.** `present.rs` now has a compositor and a present ring
(`present.rs:582`, `:715`), so "no window and no swapchain" also means bypassing a compositor with
its own shaders. Part 5 must be re-sized against that code before any work.

Risk, to be tested and not assumed: if a title reads the output size and changes its render
resolution, a tiny window changes what is rendered. Test 5 compares pass times at `1x1` and at
the default size. If they differ beyond noise, no-window runs are comparable only with each
other, and the manifest says so.

**D5.** No-window runs keep the title's own 30 fps frame clock (`XCL_FRAME_INTERVAL_US` is left
alone). The clock is a deadline the title waits on. While frames are slower than 33 ms it never
holds anything back; it begins to bind only once frames are faster, and that is the behaviour
worth measuring. It also makes `frame.wait_ms` a progress signal: while it is zero, the title is
never catching the clock. A one-off run with the clock removed can measure raw cost; it is
recorded in the manifest and never compared with clocked runs.

## Switch summary

| switch | effect |
|---|---|
| `XCL_TELEMETRY=off\|frame\|pass\|all` | tier (default `off`) |
| `XCL_TELEMETRY_DIR=<path>` | run directory root (default the logs folder) |
| `XCL_TELEMETRY_SAMPLE=<n>` | emit `pass` every nth frame (D7 fallback) |
| `XCL_HEADLESS=1` | no window, no swapchain |
| `XCL_WINDOW_SIZE=<w>x<h>` | window size override |
| `XCL_PRESENT_SKIP=1` | window, no draw |
| `XCL_LOOP_GUARD=<n>` | loop iteration cap (default 65,536) |
| `XCL_LOOP_COUNTERS=1` | loop trip counters in translated shaders |
| `XCL_PHASE_MARK=<name>` | label the current phase from outside |

## Once a title is chosen

These runs need a title, so they follow part 2 and none is part of this plan. Each uses a switch
that exists today and may remove parts of the plan. None has been run; no result is claimed.

| Run | Switch | Answers |
|---|---|---|
| Spin or barrier meeting of the two halves | `XCL_WAVE64_SPIN=0` against the default | Changes only groups of one console wave (`emit.rs:4691`, `x64_spin`); a group of more than one wave always spins. Tests H7 for those groups only |
| All masks exchanged or only the live ones | `XCL_X64_ALL_MASKS=1` against the default | What the mask-liveness pass is worth on real shaders (H7) |
| Scratch pressure | `XCL_SPLIT_SCRATCH_MB` above and below 256 | Whether split-dispatch scratch is near its bound (H1) |

## Work order

D6: the new code is appended as separate functions near the end of `log.rs` and `report.rs`, and
upstream files (`present.rs`, `host.rs`, `frame.rs`, the replay path, the recompiler) get a
one-line call site each, never logic inline. A conflict in one line is trivial; a conflict in
fifty interleaved lines is a standing cost. No new module is added.

D8: fork-only, so there is no upstream size limit and no upstream check; the parts are a work
order on `dev`. Sync from upstream is manual (`scripts/dev/merge-pr.py --sync` after review), so
a conflict shows up when a person merges, not silently. Sizes are guides.

| # | Part | Contents | Lines (est.) |
|---|---|---|---|
| 1 | Event log, manifest, refusals | appended in `log.rs` and `report.rs`: run directory, `manifest.json`, JSONL writer with per-line flush, event types, tier parsing, `XCL_TELEMETRY*`; `shader_refused` and `pipeline` events with HRESULT | ~400 |
| 2 | Frame timing | `frame` and `memory` events from `present.rs`, `frame.rs`, `host.rs`: CPU, GPU and wait time; owns the query heaps, list start/end stamps, deferred resolve and resubmission handling, so `gpu_ms` is real; `list` and `clock` events; converts the existing prose lines, which stay. First overhead run (D7). Separates CPU-bound from GPU-bound (H6) | ~250 |
| 3 | Pass timing | pass boundaries on top of part 2's heaps, `pass` events with queue and ticks | ~300 |
| 4 | Shader shape report | `shaders.json` from the recompiler, with split-wave exchange counts (clean, shareable, 5.6) | ~300 |
| 5 | No-window mode, errors, loop probe and counters | `XCL_HEADLESS`, `XCL_WINDOW_SIZE`, `XCL_PRESENT_SKIP` (re-estimate against the compositor first); `error` and `phase` events using `dred.rs`; `XCL_LOOP_GUARD`, then `XCL_LOOP_COUNTERS` | not yet estimated |
| 6 | `smoke.ps1` reads events | replace the log regexes with `events.jsonl`; re-read `scripts/smoke.ps1` and `scripts/test-smoke-artifacts.tests.ps1` first; last and alone | ~120 |

## Test plan

Rules: a new piece of logic gets a test of it. GPU tests call
`gpu::recomp::runner::skip_test(...)` and return when the adapter cannot run them. All tests use
`tests/testtitle` and synthetic shaders. **No game run is part of this plan.**
Which title to measure, if any, is an open question for the user.

| # | Tests |
|---|---|
| 1 | unit: manifest fields present and valid JSON; the writer emits one object per line and flushes (read back mid-write); tier parsing including invalid values; run directory created once; `off` opens no file; a refusal event carries its HRESULT |
| 2 | e2e on `tests/testtitle`: `frame` count in `events.jsonl` equals the `frames shown` the log reports; `memory` events have increasing timestamps; `frame.gpu_ms` equals the union of the `list` intervals and is null when none; `clock` events appear at start and every 60 frames; a list resubmitted before readback has null ticks and `resubmitted: true`; bundles get no timestamps; a device-removal case emits null ticks and does not hang; the resolve is never read in the frame that wrote it (by construction); overhead off against on is measured and reported |
| 3 | GPU test: a known dispatch timed through the new path matches `runner::dispatch_timed` within tolerance; overlapping passes keep their own start and end ticks and are not summed |
| 4 | unit: static shape over synthetic console programs: exchange counts equal hand-computed values for a wave64 program with `bpermute` and one with lane-local masks only; `register_arrays` set exactly for the `switch_pc` form |
| 5 | e2e: `tests/testtitle` completes with `XCL_HEADLESS=1` and creates no window; `XCL_WINDOW_SIZE=1x1` makes a window of that size; pass times at `1x1` against the default settle the resolution risk; GPU test: a synthetic loop runs to its own exit at a high cap and trips the guard at a low one, and a changed `XCL_LOOP_GUARD` or `XCL_LOOP_COUNTERS` value is never served from the shader caches; unit: each `error` kind serialises with its fields; e2e: a forced unresolved import gives one `error` event with `fatal` set correctly |
| 6 | `scripts/test-smoke-artifacts.tests.ps1` extended: the output built from `events.jsonl` equals the output built from the same run's log |

Whole suite after each part: `cargo test --workspace -- --test-threads=4`, reporting the real
counts and every `SKIPPED` line.

**Known test state at `a67c592`** (RTX 5080, driver 621.01, as reported to this plan): 1285
passed, 5 failed, 46 ignored, 26 skip messages. One failure is a stale barrier expectation
(#2074 together with #2203). Four are driver refusals of fuzz shaders. A security review of
`052fddf^1..052fddf^2` found no malicious code.

## Risks

| Risk | Mitigation |
|---|---|
| Timing changes what is measured | `off` is the default and creates nothing; overhead measured from part 2 (D7); sampling if above 5 % |
| Summed pass times mislead | passes carry ticks and queue, never a duration sum (5.4) |
| No-window mode changes the render resolution | measured (5.11, test 5); recorded in the manifest |
| The frame clock removed changes title pacing | recorded in the manifest; never compared with clocked runs |
| Upstream merges conflict with this code (D8) | appended code, one-line call sites (D6); merges are manual, so a conflict is seen |
| No upstream review of the measurement code (D8) | the test plan is the only check on correctness, above all for part 3, where a wrong number is worse than none |
| Phase detection is guesswork | `confidence` on every event; manual override |
| No baseline exists here | take one before reading any result as a regression or a gain |

## Decisions

| | Decision | Status |
|---|---|---|
| D1 | Output format | decided: JSON Lines stream plus a separate manifest |
| D2 | Timing granularity | decided: per pass; per list as fallback |
| D3 | Loop hypothesis | decided: build both; probe first |
| D4 | Phase detection | decided: heuristic with `confidence`, plus `XCL_PHASE_MARK` |
| D5 | Run defaults | decided: no-window, title's 30 fps frame clock kept |
| D6 | Where the code lives | decided: extend `log.rs` and `report.rs`; append, one-line call sites |
| D7 | Overhead | decided: measure from part 2 on the test title, report it, sample above 5 % (replaces "no budget") |
| D8 | Upstream posture | decided: fork-only; the split-wave report stays shareable |

Open for the user: which title, if any, to measure once the parts exist.

## Upstream state this plan depends on

- None of the nine open wave64 pull requests has merged: #1193, #2343, #1596, #1597, #1111,
  #2377, #2097, #2210, #917. #2203 and #2305 are other pull requests. The
  exchange baseline moved with #2203 only, so take counts on this tree.
- Split-dispatch scratch is bounded upstream (`XCL_SPLIT_SCRATCH_MB`,
  `hle/d3d12x/real/dispatch_chain.rs:24`). H1 therefore asks whether scratch is still the main
  cost after that bounding.

## What this produces

After parts 1 to 4, one run yields a manifest, a `frame` event per presented frame (CPU, GPU,
present and clock wait), `pass` events with ticks and pipelines, `pipeline` events with their
source, a memory series, every refusal and error with its reason, and a static shape for every
translated shader. From those files alone one can tell whether a frame is CPU- or GPU-bound
(H6), which passes cover it and which shaders they run (H1, H3, H7), whether it builds things
late (H4), whether shaders are refused (H8), and whether passes leave gaps (H5). Loop trips
(H2) come with part 5.
