# Telemetry and profiling for live runs

**Status: implemented as of 2026-11-26.** Branch `adopt/diagnostics-2`, 14 commits (section 8).

- Tiers: `XCL_TELEMETRY=off|frame|pass|all`. `off` is the default and creates nothing.
- Where the code lives: the generic machinery is appended to `crates/xcl/src/log.rs` and
  `crates/xcl/src/report.rs`. The rest is in two fork-only files, `crates/xcl/src/telemetry_gpu.rs`
  (GPU stamp engine, pass, pipeline, resource and I/O events) and
  `crates/xcl/src/telemetry_run.rs` (no-window switches, errors, `end`, phases, loop counters).
  `report.rs` includes both with `#[path]`. Upstream files hold one-line call sites only. This
  replaces the earlier decision D6 ("no new module").
- Tested on an NVIDIA RTX 5080 only. **Not tested on AMD or on a 64-lane GPU.**
- This is a fork tool: it lives on `dev`, never on a branch cut from `main`.

## Goal

One run of a title leaves a machine-readable account: where GPU time went, where CPU time went,
what memory did, what was refused, what failed, how the run ended. A person or an agent can then
name the cause of a slow frame from the files, without a profiler GUI.

Non-goals: replacing vendor profilers (they stay the tool for one frame's hardware counters), any
visualisation, any change to what the title computes.

## Why: the question this answers

The open question is why a title runs far below 30 fps. Nothing in this document is a title
measurement. Each hypothesis below is open and is confirmed by a different part of the output:

| # | Hypothesis | Confirmed by |
|---|---|---|
| H1 | Translated shaders spill registers to scratch memory | pass GPU time concentrated in shaders whose `shaders.json` record has `register_arrays` or a large `scratch_hint` |
| H2 | Loops run to their safety cap instead of their real exit | `loop_guard` events (section 5.7), or GPU time falling when `XCL_LOOP_BUDGET` is lowered |
| H3 | Every console lane runs every instruction, so inactive lanes save nothing | pass time following lane count; `divergent_branches` in `shaders.json` |
| H4 | Work the console did ahead of time happens inside the frame | `pipeline` events (build time, `waited_ms`) and `resource` events whose `frame` equals a slow frame's list `frame` |
| H5 | Conservative barriers serialise work the console overlapped | gaps between lists inside a frame's GPU span; `barriers_before` |
| H6 | CPU-side command-list translation is the limit | `frame.cpu_ms` close to `interval_ms` while `gpu_ms` is small; `pso_wait_ms`, `look_ms` |
| H7 | Split wave64: the GPU runs 32 lanes, XCL splits each 64-lane wave and pays exchanges | `exchanges` in `shaders.json` against pass time |
| H8 | Refused shaders or pipelines are replaced or skipped | `shader_refused` and refused `pipeline` events with their HRESULT |

## 5.1 The run folder

With `XCL_TELEMETRY` set, a run makes `<root>/run-<utc>-<commit12>/`.

- `<root>` is `XCL_TELEMETRY_DIR`, else the title's logs folder (`<data root>\<title>\logs`).
- `<utc>` is `YYYYMMDDTHHMMSSZ`. `<commit12>` is the first 12 hex digits of the checkout's `HEAD`
  (read from the `.git` files, no process is started), or `unknown`. A second run in the same
  second gets `-2`, `-3`, and so on. `manifest.run_id` is the folder name without `run-`.
- If the folder cannot be made, or `events.jsonl` cannot be opened, XCL logs one warning and
  telemetry stays off.

| File | When it is written | Content |
|---|---|---|
| `manifest.json` | at start; rewritten (temp file, then rename) when the device is created, when the engine starts, and at the end | identity and configuration (5.2) |
| `events.jsonl` | one line per event, written whole and unbuffered, so a run killed by a GPU reset still yields everything up to the kill | the event stream (5.3) |
| `shaders.jsonl` | tier `pass`: while shaders are translated | raw shape log, lines `seen` and `shape` (5.6) |
| `shaders.json` | tier `pass`: at the end of the run | the merged shape report (5.6) |
| `report.json` | each time XCL writes its end-of-run report | a copy of the usual `report.json`, unchanged; the last write wins |

`xcl.log` is not copied. It stays in the logs folder; `manifest.log` holds its path. Run folders
hold title paths and names. They stay on the machine and are never committed.

## 5.2 `manifest.json`

Schema 1, 40 keys, pretty-printed. A value that is not known stays `null`. The unit test
`manifest_has_every_field_and_is_valid_json` lists all 40 keys.

| Key | Holds | Null or empty when |
|---|---|---|
| `schema` | `1` | never |
| `run_id` | folder name without `run-` | never |
| `xcl_version` | crate version | never |
| `commit` | 40-hex `HEAD` of the checkout found from the source tree or the executable | no checkout found |
| `commit_source` | `"git-head"` | no commit |
| `dirty` | reserved | always `null` in this build (nothing fills it) |
| `rust` | reserved | always `null` |
| `profile` | `debug` or `release` | never |
| `os` | `Windows <major>.<minor>.<build>` | never |
| `cpu` | CPU brand string (CPUID) | unreadable |
| `cores` | reserved | always `null` |
| `threads` | `available_parallelism` | 0 if unknown |
| `ram_mb` | total physical memory | 0 if the call fails |
| `adapter` | adapter name | before the D3D12 device exists |
| `driver` | user-mode driver version `a.b.c.d` | before the device; query failed |
| `vram_mb` | dedicated video memory | before the device; query failed |
| `wave_lane_count_min`, `wave_lane_count_max` | `WaveLaneCountMin/Max` of the adapter | before the device; query failed |
| `split_wave64` | `wave_lane_count_max == 32`: 64-lane waves are split in two | same as the wave counts |
| `dxc` | object `{dxcompiler, dxil}`: file versions of the compiler and validator | until the end of the run |
| `title` | title name | never |
| `title_version` | reserved | always `null` |
| `exe` | path of the title executable | never |
| `log` | path of this run's `xcl.log` | never |
| `tier` | `frame`, `pass` or `all` | never |
| `switches` | every `XCL_*` variable set at start (name prefix matched without case), name to value, not only the ones XCL reads | empty object if none |
| `started` | UTC time, `YYYY-MM-DDTHH:MM:SSZ` | never |
| `events_dropped` | events lost to an event-file lock wait over 100 ms or a failed write; refreshed at each manifest write | never |
| `present_mode` | `window`, `headless` or `skip` (5.11) | never |
| `window_size` | `<w>x<h>` the window would have (`XCL_WINDOW_SIZE` or `xcl.toml`); also set in headless | never |
| `loop_budget` | effective loop budget (`XCL_LOOP_BUDGET`, default 65536) | never |
| `loop_counters` | whether loop-guard counters are on (5.7) | never |
| `diagnostic` | list of `NAME=value` for switches that change generated code or presentation | empty list in a normal run |
| `compare_key` | `diagnostic` joined by `,` | `""` in a normal run |
| `sample` | `XCL_TELEMETRY_SAMPLE` | `0` in the first write only, until the engine starts |
| `gpu_drops` | counters of GPU data that came out null, by reason (below) | empty object until the end of the run |
| `pass_slots` | timestamp slots in the pass heap (65,536) | `0` below tier `pass`, and until the engine starts |
| `list_slots` | timestamp slots per stamp set and queue (1,024) | `0` until the engine starts |
| `gpu_sets` | stamp sets per queue (4: frames in flight plus one) | `0` until the engine starts |
| `copy_queue_timestamps` | `CopyQueueTimestampQueriesSupported`, once a copy queue was seen | no copy queue seen |

### `diagnostic` and `compare_key`

Two runs are comparable only when their `compare_key` is equal. A switch is listed when:

- it is one of `XCL_LOOP_COUNTERS`, `XCL_WAVE64_SPIN`, `XCL_X64_ALL_MASKS`,
  `XCL_RECOMP_NO_STRUCTURED`, `XCL_DXIL_NO_OPT` and its value is neither empty nor `0`;
- it is `XCL_HEADLESS` or `XCL_PRESENT_SKIP` and its value is exactly `1`;
- it is one of `XCL_LOOP_BUDGET`, `XCL_WINDOW_SIZE`, `XCL_FRAME_INTERVAL_US`, `XCL_PRESENT_SYNC`,
  `XCL_SPLIT_SCRATCH_MB` and its value is not empty.

Names are upper-cased and the list is sorted. `XCL_TELEMETRY*` is not in the list, so changing the
tier does not change the key.

### `gpu_drops`

Written at the end of the run. A list that is counted here still gets its `list` event, with null
ticks and a `why`, except where noted.

| Key | Counts |
|---|---|
| `full` | lists that got no stamps: the set's 1,024 slots were used (about 1,023 lists per queue per frame), no free set was open, a queue's stamp lock was busy for 1 s, or the queue's null-entry buffer (4,096) was full |
| `late` | lists whose stamps were not ready 16 frames after their frame, or whose read failed |
| `unsupported` | lists with no stamps because the engine had no device or queue functions, the queue type is not timed (a copy queue without copy-queue timestamps, or any other type), the queue has no usable timestamp frequency (under 1,000 Hz), a stamp set could not be made, or the resolve could not be recorded |
| `resubmitted` | submissions of a list that already had a submission whose stamps were not read, with no Reset in between (see 5.4) |
| `removed` | lists seen after the device was removed |
| `sampled_out` | lists submitted in frames skipped by `XCL_TELEMETRY_SAMPLE`. They get no `list` event |
| `frames` | `frame` records dropped because the frame book lock was busy for 1 ms. They get no `frame` event |
| `pass_busy` | pass notes dropped because the pass state lock was busy (2 ms) |
| `pass_full` | pass boundaries not stamped because the 65,536-slot pass heap or one recording's 16 blocks (4,096 stamps) ran out |

## 5.3 `events.jsonl`

Every line is one flat JSON object: `{"t": <ms>, "ev": "<name>", ...}`. `t` is milliseconds since
XCL's log clock started, with 3 decimals, taken under the writer's lock. Events stay flat: no
nested objects. The only arrays are of plain numbers or strings (`pipelines`, `records`,
`stages`). Field names never change meaning. New fields may be added. `schema` gates the format.

Tiers are cumulative: `frame` < `pass` < `all`.

| `ev` | Tier | Section |
|---|---|---|
| `frame`, `list`, `clock`, `memory` | frame | 5.3 below, 5.4 |
| `shader_refused`, `pipeline` (refusal shape), `error`, `stall`, `end`, `exit`, `activity`, `phase` | frame | 5.5, 5.8, 5.9 |
| `pass`, `pipeline` (build shape), `pipeline_bind`, `resource`, `io`, `shape_queue` | pass | 5.4, 5.5 |
| `stub`, `loop_guard`, `loop_counters` | all | 5.7, 5.8 |

Two names that one might look for are not separate events. `io_failed` is an `error` kind. The
shape log's `seen` and `shape` lines go to `shaders.jsonl`, not `events.jsonl`.

### Frame, list, clock, memory (tier `frame`)

**`frame`**: one per shown frame, `i` = 1, 2, ... A frame ends when the presenting call that showed
it returns. A list belongs to the frame in which its submit call was made. A frame is written when
the GPU data of all its lists has been read, which is at least two frames later (at most 16, then
it is written with nulls). Order of `i` is kept.

| Field | Unit | Rule |
|---|---|---|
| `i` | count | frames shown so far (1-based) |
| `interval_ms` | ms | end of the previous frame's present to the end of this one; the first frame counts from telemetry start |
| `cpu_ms` | ms | `interval_ms - present_ms -` the presenting thread's frame-clock wait, never below 0 |
| `present_ms` | ms | duration of the presenting call |
| `wait_ms`, `wait_calls` | ms, count | `WaitFrameEventX` time and calls in this interval, summed over all threads, so it can exceed `interval_ms` |
| `gpu_ms` | ms | union of the list intervals of this frame (never their sum), across queues; null when no list has valid ticks (see `why`) |
| `gpu_first_ms`, `gpu_last_ms` | ms on the `t` clock | earliest start and latest end of those intervals; null with `gpu_ms` |
| `gpu_lists_null` | count | lists of this frame with null ticks. If above 0 and `gpu_ms` is set, `gpu_ms` is a lower bound |
| `lists` | count | PC command lists submitted in the frame |
| `draws`, `dispatches` | count | console draws and dispatches in the interval, all threads |
| `barriers` | count | barrier batches the title recorded in the interval (not single barriers) |
| `pso_wait_ms` | ms | time threads were blocked waiting for asynchronous pipeline builds |
| `look_ms` | ms | XCL's own CPU time before the title's submits (sparse and texture look-ups) |
| `sampled` | bool | whether GPU stamps were taken this frame (`XCL_TELEMETRY_SAMPLE`) |
| `why` | text | present only when `gpu_ms` is null: `sampled_out`, `no_lists`, `late`, `no_ticks`, `exit`, `removed` |

**`list`**: one per submitted PC command list, in sampled frames. Copy and compute queues are
included. Bundles never reach `ExecuteCommandLists`, so they have no event.

| Field | Unit | Rule |
|---|---|---|
| `frame` | count | engine frame at submit: presents so far. It equals the `i` of the `frame` event minus 1 |
| `seq` | count | run-unique submission number. `pass.list` refers to it |
| `id` | address | PC list address. It joins the submissions of one list across frames |
| `chunk` | count | run-unique driver call. Lists with one `chunk` went in one call |
| `queue`, `queue_type` | id, text | queue id (see `clock`), and `direct`, `compute`, `copy` or `other` |
| `start_tick`, `end_tick` | GPU ticks | raw ticks on this queue; null when `why` is set |
| `start_ms`, `end_ms` | ms on the `t` clock | calibrated; null with the ticks |
| `cpu_us` | us | duration of the driver call for the whole chunk (the same on every list of the chunk) |
| `translate_us`, `console_bytes` | - | always null: not measured |
| `resubmitted` | bool | see 5.4 |
| `origin` | text | `present` if submitted inside the presenting call, else `submit` |
| `why` | text | absent when the ticks are valid. Else `full`, `late`, `unsupported`, `no_frequency`, `removed`, `exit` (still pending when the run ended), `zero` (a tick was 0), `order` (end before start) |

**`clock`**: calibration of one queue's GPU clock, written when the queue is first used and again
every 60 frames.

| Field | Rule |
|---|---|
| `queue`, `queue_type` | as in `list` |
| `gpu_tick`, `cpu_qpc` | one calibration pair; null when the call failed |
| `frequency` | GPU ticks per second; null when absent or under 1,000 Hz |
| `qpc_frequency` | `QueryPerformanceFrequency` |
| `frame` | engine frame of the calibration |
| `why` | only when the pair is null: `unsupported`, `no_frequency` or `calibration` |
| `copy_timestamps` | only on a copy queue: whether copy-queue timestamps are supported |

**`memory`**: written every 60 frames by the telemetry thread. Values are MiB. A source that
failed is null.

| Field | Holds |
|---|---|
| `frame` | engine frame |
| `committed_mb` | private bytes of the process |
| `vram_local_mb`, `vram_nonlocal_mb`, `vram_budget_mb` | DXGI local and non-local usage, local budget |
| `arenas_mb`, `arenas_made_mb` | video-memory arenas in use, and made |
| `pages_mb`, `page_pools_mb` | target pages, and their pools |
| `sparse_mb`, `sparse_mapped_mb` | sparse pools, and the part mapped |

### Example lines (made-up values)

```
{"t":1234.567,"ev":"frame","i":40,"interval_ms":33.4,"cpu_ms":4.1,"present_ms":1.2,"wait_ms":27.0,"wait_calls":1,"gpu_ms":6.5,"gpu_first_ms":1210.1,"gpu_last_ms":1216.6,"gpu_lists_null":0,"lists":12,"draws":300,"dispatches":20,"barriers":6,"pso_wait_ms":0.0,"look_ms":0.3,"sampled":true}
{"t":1236.001,"ev":"pass","frame":39,"list":812,"queue":0,"i":2,"start_tick":100,"end_tick":900,"start_ms":1211.0,"end_ms":1212.0,"kind":"draw","draws":14,"dispatches":0,"indirect":0,"pipelines":[7,9],"barriers_before":2,"resubmitted":false}
```

## 5.4 GPU timing as built

The title records console command lists. XCL replays them as PC command lists, and the engine in
`telemetry_gpu.rs` stamps them.

- **Own heaps.** The engine makes its own timestamp query heaps, readback buffers, command lists
  and fences through the device's original functions, so XCL's hooks never adopt them. It does not
  use the title's timestamp pool.
- **Stamps.** Per queue there are 4 stamp sets of 1,024 slots (`gpu_sets`, `list_slots`). Each
  stamp is a tiny pre-recorded command list with one `EndQuery`. In one `ExecuteCommandLists` call
  the engine submits `[stamp, list, stamp, list, ..., stamp]`, so each list has a stamp before and
  after it, and neighbours share one. The call stays one driver call.
- **One resolve per queue per frame.** The first submit of a queue in a new frame puts the
  previous set's `ResolveQueryData` first in the call and signals the queue's own fence. The
  telemetry thread reads a set only when its fence has passed and at least two frames after the
  frame that wrote it. Nothing waits on the GPU and nothing is read in the frame that wrote it.
- **Ticks are per queue.** Each queue has its own frequency and a `clock` event at start and every
  60 frames. Ticks become `start_ms` / `end_ms` through the latest calibration of that queue.
  Copy queues are timed only when the device supports copy-queue timestamps.
- **`frame.gpu_ms` is the union** of the calibrated list intervals of the frame, never the sum:
  lists on different queues overlap.
- **Passes** (tier `pass`). A pass is the span between two barrier batches of one list. A
  boundary is cut at each batch the title passes to `ResourceBarrier`. It is counted before XCL
  drops transitions that are no-ops on PC, so a batch counts even when XCL records nothing for it.
  One extra stamp is written into the PC list before the batch.
  - Passes tile their list exactly: pass 0 starts at the list's start stamp, the last pass ends at
    the list's end stamp, and pass `k` runs from boundary `k-1` to boundary `k`.
  - Each pass carries `draws`, `dispatches`, `indirect` (ExecuteIndirect), `pipelines` and
    `barriers_before` (barriers in the batch before the pass; 0 for pass 0).
  - `pipelines` means the distinct pipelines the pass drew or dispatched with, as resolved
    pointers mapped to telemetry ids, first seen first. At most 8; `pipeline_ids_truncated: true`
    appears when there were more.
  - `kind` is `draw` if `draws > 0`, else `dispatch` if `dispatches > 0`, else `indirect` if
    `indirect > 0`, else `empty`.
  - Copy lists (and bundles) get no pass stamps. Their one pass spans the list and carries
    `why: "copy_list"` (or `bundle`).
  - Counters are shared per list, not per thread, because the title records a list on one thread
    and submits it on another. Counting does not depend on sampling; stamps do.
- **`resubmitted`.** It is true only when the same recording is submitted again while its earlier
  stamps are unread and no `Reset` came in between. A `Reset` makes the next submit a new
  recording. For a resubmitted list the list ticks stay valid (the list stamps are separate per
  submission), but its pass stamps would overwrite each other, so its pass events have null ticks
  and `why: "resubmitted"`. The earlier submission's passes are poisoned the same way. A title may
  legitimately execute one closed list in every frame. `gpu_drops.resubmitted` then counts those
  submissions, and the flag is right.
- **Join.** `pass.frame` = `list.frame`, `pass.list` = `list.seq`, `pass.pipelines[i]` =
  `pipeline_bind.id`, `pipeline_bind.records[j]` = `shaders.json` `record`.
- **Sampling.** `XCL_TELEMETRY_SAMPLE=<n>` (1 to 10,000; default 1) stamps every n-th frame.
  Other frames still get a `frame` event (`sampled: false`, `why: "sampled_out"`) but no `list` or
  `pass` events.
- **Failures.** A device removal gives nulls with `why: "removed"` and one warning. Lists
  submitted after the engine's final read are not timed (one warning).

## 5.5 Pipeline, resource, I/O events

**`pipeline`** has two shapes. Build shape (tier `pass`), written when the driver builds a
pipeline:

| Field | Rule |
|---|---|
| `id` | build serial. It does not join to `pass.pipelines`; use `pipeline_bind` |
| `stage` | `compute` or `graphics` |
| `records` | array of 0 or 1 console record hashes, 16 lowercase hex digits |
| `dxil_bytes` | size of the DXIL given to the driver |
| `compile_ms` | time in the driver |
| `waited_ms` | time spent waiting for a compile slot before it (large pipelines only) |
| `source` | where the shader came from: `cache`, `direct` or `dxc`; the costliest of the stages wins; null if unknown |
| `blob` | whether a driver blob was supplied |

Refusal shape (tier `frame`), written when the driver refuses a pipeline:
`refused: true`, `stage` (`graphics`, `compute` or `stream`, if known), `hresult` (text
`0x8007000E`), `source` (`standin` if a stand-in pipeline was built, else `none`), `n`. The same
refusal also writes an `error` of kind `pipeline_rejected`.

**`pipeline_bind`** (tier `pass`): written when a translated pipeline is created. The pointer gets
a new telemetry id (a reused pointer is a new pipeline). `id`; `stages`: array of `vs`, `ps`,
`cs`, `ms`, `gs`, for the stages that have bytes; `records`: array of the same length, each the
16-hex fingerprint (FNV-64) of that stage's console shader container. A pass id with no
`pipeline_bind` event is a pointer whose creation was not seen.

**`resource`** (tier `pass`): a resource creation call of the title, written when it returns, only
after the first frame.

| Field | Rule |
|---|---|
| `kind` | `committed`, `placed` or `reserved` |
| `dim` | `buffer` or `texture`; null if the descriptor was not readable |
| `bytes` | buffer width; null for textures (the size needs allocation info) |
| `where`, `reason` | always `title` and `explicit`. No XCL-side or `first_read` producer exists |
| `ms` | duration of the call |
| `frame` | engine frame |

**`io`** (tier `pass`): DirectStorage requests completed since the last `io` event, aggregated.
Written when the frame changes or every 250 ms, whichever comes first, and only if a request
completed. `requests`, `bytes`, `ms` (sum of request durations), `max_ms` (longest), `zlib`
(requests compressed with zlib), `frame`.

**`shape_queue`** (tier `pass`): written when the shape worker's queue drains (volume rule in 5.8).
`n`, `analysed`, `failed`: running counts of the shader shapes analysed and failed.

## 5.6 `shaders.json` and `shaders.jsonl` (tier `pass`)

A background thread (`xcl-shader-shape`, below-normal priority) analyses every console program XCL
translates, once per (program, stage, split flag, memory windows). It needs no GPU.

- **Queue limits.** 1,024 jobs and 64 MiB of copied bytes. A program that does not fit is
  recorded with `shape_status: "dropped"`.
- **`shaders.jsonl`** gets a `seen` line when a program is first admitted (`key`, `record`, `vs`
  if geometry, `stage`, `windows`, `split`, `wave64` null, `dxil_bytes`, `compile_ms`,
  `split_reason`, plus `shape: "dropped"` if dropped) and a `shape` line when its analysis is done
  (the record below plus `key`).
- **`shaders.json`** is written at the end of the run from the log: it stops intake, waits up to
  2 s for the queue, and merges lines by `key`. This is safe because every report write after the
  title starts is an end-of-run path. At most 50,000 records are kept; more stay in
  `shaders.jsonl`. Nothing is written if no program was seen.

Header fields: `schema` (1), `run_id`, `translator_source_id`, `wave_lane_count_min`,
`wave_lane_count_max`, `split_wave64_forced` (`XCL_SPLIT_WAVE64=1`), `compute_split` (whether
compute programs run split on this adapter), `graphics_upper_lanes_eligible`, `split_reason`
(text), `direct_dxil` (value of `XCL_DIRECT_DXIL`, or null), `counts`, `records`.

`counts`: `records`, `analysed`, `dropped`, `failed`, `pending_at_exit` (admitted, analysis not
done), `kept_in_memory_cap_hit` (records beyond the 50,000 cap).

Record fields (null where the analysis could not produce them):

| Field | Holds |
|---|---|
| `record` | 16-hex FNV-64 of the console shader container (for a geometry program, of the GS) |
| `vs` | geometry programs only: hash of the paired VS |
| `stage` | `vs`, `ps`, `cs`, `ms` or `gs` |
| `windows` | UMA memory windows the program was translated for |
| `wave64`, `split` | the program is wave64; it runs split in two 32-lane halves |
| `upper_lanes` | graphics: the upper-lanes form |
| `backend` | `hlsl` or `direct` (direct DXIL) |
| `form` | `straight`, `structured`, `switch_pc` (a `pc` dispatch loop) or `direct` |
| `unstructured` | `form == switch_pc` |
| `register_arrays` | list of `{name, dwords}` of private arrays (H1) |
| `instructions`, `blocks`, `loops`, `max_loop_depth` | console program shape |
| `divergent_branches` | `S_CBRANCH_EXECZ/EXECNZ/VCCZ/VCCNZ` count (H3) |
| `movrel_sites` | relative-move instructions |
| `exchanges` | wave64 compute only: static exchange sites of the split form: `ballot`, `entry`, `readlane`, `readlane_folded`, `bpermute`, `coherent_load`, `other` (must be 0), `total`, `in_loops`, `threads_per_group`, `waves_per_group`, `spin`, and `paid` (the split ran) |
| `dxil` | stats of the DXIL given to the driver: `bitcode_bytes`, `blocks`, `instructions`, `phis`, `backward_branches`, `allocas`, `switches` |
| `scratch_hint` | `{private_bytes_per_thread, chain_bytes_per_group, group_threads, bytes_per_group}`; read it against `XCL_SPLIT_SCRATCH_MB` (default 256) |
| `dxil_bytes`, `compile_ms`, `dispatch_chain_pieces` | size of the DXIL, translate-plus-compile time, split-dispatch pieces |
| `split_reason` | why split is or is not used (`device waves 32-32`, `XCL_SPLIT_WAVE64=1`, `wave range unknown`) |
| `shape_status` | `ok`, `partial: <notes>`, `failed: <reason>`, `dropped`, `pending_at_exit` |

The attribution loop: rank passes by GPU time, follow `pipelines` to `pipeline_bind.records`, look
the record up in `shaders.json`.

## 5.7 Loops (H2)

The recompiler caps every translated loop per lane. At the top of each loop, a lane that has spent
its budget leaves the loop. The default budget is 65,536 iterations.

- **`XCL_LOOP_BUDGET=<n>`**: a whole number from 1; anything else gives the default, silently.
  Applies to the HLSL path and the direct DXIL path. It does **not** apply to the per-lane DXIL
  form (`XCL_PER_LANE=1`), which has no loop guard. A translation made with it set is not served
  from, or stored in, the shader caches. The effective value is `manifest.loop_budget`.
- **`XCL_LOOP_COUNTERS=1`**: needs `XCL_TELEMETRY=all`. At a lower tier the counters stay off with
  one warning. The flag is read once from the environment, before the first translation, and
  decides both things. The translation cache is keyed by it, so counted and uncounted code are
  never mixed.
  - Translated code counts the lanes that leave a loop through its guard, once per lane, with one
    atomic add per hardware wave (not per lane).
  - Counts go into a 16-byte slot per distinct shader set, in a 1 MiB region allocated in the
    first committed UMA window (the only window the CPU sees GPU stores in). The pipeline's slot
    number travels in an io word. Slot 0 is unused; there are 65,535 slots. When they run out, one
    warning, and later pipelines are not counted. A missing UMA, window or room gives a warning and
    no counting.
  - The telemetry poll thread reads the slots at most once a second, and once more at the end.
  - Word 0 of a slot counts compute, mesh and vertex stages, word 1 pixel, word 2 geometry.
  - This changes the shaders and costs GPU time. Runs with it are never compared with runs
    without it (`diagnostic`, `compare_key`).

Events (tier `all`):

| Event | When | Fields |
|---|---|---|
| `loop_guard` | a slot word changed since the last read | `record` (16 hex of the stage's shader, zeros if unknown), `stage`, `pipeline` (the counter slot number, not a `pipeline_bind.id`), `trips` (lanes since the last read), `total` |
| `error` kind `loop_guard_tripped` | the first trip of a slot; not fatal | the `loop_guard` fields, plus `source: "loop_counters"` and a `detail` naming the shader set |
| `loop_counters` | once, at the end, also when nothing tripped | `window` (the region's UMA window, or null if no region), `slots` (shader sets labelled), `tripped` (slots with a non-zero word) |

A run with counters on and no trips shows `loop_counters` with `tripped: 0` and `slots > 0`. A
`loop_counters` event with `slots: 0` or `window: null`, or none at all, means the counters never
worked. `manifest.loop_counters` alone does not prove it.

## 5.8 Errors, stubs, run end

**`error`** (tier `frame`): one channel for failures. The prose log lines stay.

Fields: `kind`, `source`, `detail`, `hresult` (text, only if there is one), `fatal`,
`after_fatal` (true when a later fatal candidate arrived after one had taken the latch; it is then
written with `fatal: false`), then kind-specific fields. Only the first fatal error ends the run
and writes the `end` event. Only `device_removed`, `title_exception`, `timeout` and `load_failed`
can be fatal.

| `kind` | `source` | Kind-specific fields | Fatal |
|---|---|---|---|
| `device_removed` | `d3d12` | `message` | yes |
| `gpu_fault` | `dred` | `what` = `page_fault`: `va`, `window`, `allocations`, `freed`; or `what` = `breadcrumb`: `list`, `done`, `count`, `bound` | no |
| `pipeline_rejected` | `d3d12` | `stage`, `standin`, `n` | no |
| `shader_refused` | `shader` | `stage`, `reason`, `n` | no |
| `unresolved_import` | `resolver` | `dll`, `function`, `importer`, `delay`, `n` (an import routed to a stub) | no |
| `vfs_unmapped` | `vfs` | `path` (at most 1,000 distinct paths) | no |
| `io_failed` | `dstorage` | `path` (if known), `bytes`, `n` | no |
| `title_exception` | `eh` | `code` (hex), `at` (module and offset), `message` | yes |
| `loop_guard_tripped` | `loop_counters` | see 5.7 | no |
| `timeout` | `stall_dump` | `last_frame`, `idle_s`, `limit_s` | yes |
| `load_failed` | `loader` | none (`detail` is the message) | yes |
| `stub_called_fatal` | - | reserved: no producer exists | - |

**Volume rule.** `shader_refused`, the refusal `pipeline`, `io_failed`, `unresolved_import` and
`shape_queue` write the first 100 events and then every 1,000th. Each carries `n`, the 1-based
count, so totals stay readable.

**`shader_refused`** (tier `frame`): `stage` (`vs`, `ps`, `cs`, `ms`, `vs+gs`; only if the message
names one), `reason` (the refusal text), `hresult` (only if the text ends with one), `n`. The same
refusal writes an `error` of kind `shader_refused`.

**`stall`** (tier `frame`): written when `XCL_STALL_DUMP` fires: `last_frame`, `idle_s`,
`limit_s`, `ends_run: true`. It is followed by a fatal `error` of kind `timeout`.

**`stub`** (tier `all`): a stub was called. Written at the 1st, 10th, 100th, 1,000th ... call:
`dll`, `function`, `calls`, and `caller` (module and offset) on the first call only.

**`activity`** (tier `frame`, and only with `XCL_ACTIVITY` set): one per 2-second iteration of the
activity thread: `presents_2s` (frames presented in the window), `committed_mb`. The smoke reader
takes its figures from it.

**`end`**: written once per run. Fields: `reason`; `code` (when there is one); `frames_shown`;
`presentx` (PresentX calls); `fatal_kind` (the error kind that ended the run, only for failures).

| `reason` | Meaning | `code` |
|---|---|---|
| `run_seconds` | `XCL_RUN_SECONDS` elapsed | 0 |
| `entry_returned` | the title's entry point returned to XCL | its return value |
| `window_closed` | the XCL window was closed | 0 |
| `process_exit` | the title called `ExitProcess` or terminated its own process, and no `end` was written before | the exit status |
| `device_removed`, `title_exception`, `load_failed` | the fatal error of that kind | none |
| `stall` | the fatal `timeout` from `XCL_STALL_DUMP` | none |

**`exit`**: written when the process terminates (the `NtTerminateProcess` hook), after `end`:
`status`. A run with no `end` and no `exit` was killed from outside. The file then ends at the last
event that was written.

## 5.9 Phases

`phase` events (tier `frame`). Fields: `name`, `from` (the previous phase of the same source; null
for the first), `confidence` (0 to 1), `source` (`auto`, `mark` or `schedule`), `frames` (frames
shown so far). A poll thread (every 250 ms) writes them from three sources.

- **Schedule.** `XCL_PHASE_MARK="12.5:menu,40:world"`: seconds since telemetry started, then a
  name. A bare name (`world`) means second 0. Entries are sorted by time. A bad entry is skipped
  with a warning. Names are `boot`, `menu`, `loading`, `world`, `cutscene`, or `[a-z0-9_-]{1,32}`.
  `source` is `schedule`, confidence 1.
- **Control file.** `XCL_PHASE_FILE=<path>`, default `<logs>\phase`. A script writes a phase name
  into it while the title runs. Each change of content writes an event with `source: "mark"`,
  confidence 1. Content `auto`, or an empty file, ends the mark and writes nothing. A name that is
  not valid is ignored, with one warning. The file's content at start is ignored, so a stale file
  from an earlier run marks nothing.
- **Automatic guess.** `source: "auto"`. The first event is `boot` (confidence 1) at start. Each
  second the poll thread classifies the last window:
  - `boot` until the title has shown a frame.
  - `loading` when DirectStorage reads at 32 MiB/s or more in this and the previous window
    (confidence 0.5 plus up to 0.4).
  - else, if frames advanced: `world` at 400 or more console draws per frame, else `menu`
    (confidence 0.35). The 400 is a guess, not a measurement.
  - else the previous phase, with confidence 0.
  - A new phase is written only after it wins 2 consecutive windows. `cutscene` is never guessed.

The guess is weak and is not load-bearing. What the title shows on screen (menu, world, cutscene)
comes from a person who looks at a screenshot and writes the label through the control file or the
schedule.

## 5.10 Tiers and overhead

| Tier | Writes | Creates |
|---|---|---|
| `off` (default) | nothing | nothing; one environment read at start and one relaxed load per call site |
| `frame` | manifest, `frame`, `list`, `clock`, `memory`, `shader_refused`, refusal `pipeline`, `error`, `stall`, `end`, `exit`, `activity`, `phase`, `report.json` | stamp sets (4 per queue), `xcl-telemetry` and poll threads |
| `pass` | adds `pass`, build `pipeline`, `pipeline_bind`, `resource`, `io`, `shape_queue`, `shaders.jsonl`, `shaders.json` | pass heap (65,536 slots), shape thread |
| `all` | adds `stub`, `loop_guard`, `loop_counters` (counters need `XCL_LOOP_COUNTERS=1`) | counter region |

**Measured overhead (D7).** Test title (`tests/testtitle`), `--frames 600 --uncapped`, release
build, RTX 5080, 5 runs per mode, medians of the title's own elapsed time:

| Mode | Elapsed | Per frame |
|---|---|---|
| `off` | 107.9 ms | - |
| `frame` | 169.9 ms | +0.10 ms |
| `frame`, `XCL_TELEMETRY_SAMPLE=10` | 139.6 ms | +0.05 ms |
| `pass` | 179.7 ms | +0.12 ms |

These percentages (57 %, 29 %, 66 %) are a worst case, because that title's frame takes 0.13 ms.
On a 33 ms frame the same absolute cost is about 0.3 to 0.4 %. A GPU test also measured the
interleaved stamps on a fixed 24-list workload at +0.0008 ms (no queue serialisation seen).

Recommendation: use tier `pass` for investigation runs. Run smoke comparisons with telemetry
`off`. A telemetry run and a non-telemetry run are not compared for frame time.

## 5.11 No-window mode and reduced cost

Three switches, because they fail differently. All three are in `diagnostic` and so in
`compare_key`; `manifest.present_mode` and `manifest.window_size` record the result.

- **`XCL_HEADLESS=1`** (exactly `1`): no window, no swap chain, no compositor, nothing recorded
  for the present. The frame is counted as shown. This feeds `XCL_STALL_DUMP`,
  `XCL_RUN_SECONDS`, the smoke "frames shown" line and `end.frames_shown` as usual.
  - The title's own frame clock is untouched (D5), so `frame.wait_ms` stays meaningful.
  - There is no window, so keyboard and mouse as gamepad give neutral input. A real controller
    still works.
  - The present ring's wait never runs, so `XCL_PRESENT_SYNC` has no effect. Back-pressure on the
    CPU is lost, so commit and frame times can differ from a windowed run. Compare headless runs
    only with headless runs.
  - `XCL_FRAME_DUMP` is ignored (one log line says so). There is no window title.
  - Nothing ends the run except the title, `XCL_RUN_SECONDS` or `XCL_STALL_DUMP`.
- **`XCL_PRESENT_SKIP=1`** (exactly `1`): the window is kept, but the copy and the present are
  skipped. The window shows its background. Keyboard and mouse input work. It separates the cost
  of drawing from the cost of having a window.
- **Both set**: headless wins, one warning.
- **`XCL_WINDOW_SIZE=<w>x<h>`**: 1 to 16,384 each, `x` in any case. It overrides the window size
  from `xcl.toml`. An invalid value warns once and keeps the config size. The swap chain follows
  the title's output plane, not the window, so the title does not see this size through XCL. This
  was read from the code; a comparison of pass times at `1x1` and at the default size was not run.

## Switch summary

Every `XCL_*` variable the telemetry code reads, or the host reads for telemetry. All are off or
unset by default. The tier column says which tier must be on for the switch to have an effect.

| Variable | Values | Default | Tier needed |
|---|---|---|---|
| `XCL_TELEMETRY` | `off`, `frame`, `pass`, `all` (trimmed, any case); anything else warns and stays off | `off` | - |
| `XCL_TELEMETRY_DIR` | folder for run folders | the title's logs folder | `frame` |
| `XCL_TELEMETRY_SAMPLE` | whole number 1 to 10,000: stamp every n-th frame; else warns and uses 1 | 1 | `frame` |
| `XCL_HEADLESS` | `1`: no window, no swap chain | off | none |
| `XCL_PRESENT_SKIP` | `1`: window, no draw | off | none |
| `XCL_WINDOW_SIZE` | `<w>x<h>` | `xcl.toml` size | none |
| `XCL_LOOP_BUDGET` | whole number from 1 | 65536 | none |
| `XCL_LOOP_COUNTERS` | any value except empty or `0` | off | `all` |
| `XCL_PHASE_MARK` | `[<seconds>:]<name>,...` | unset | `frame` |
| `XCL_PHASE_FILE` | path of the phase control file | `<logs>\phase` | `frame` |
| `XCL_STALL_DUMP` | seconds without a frame (positive number); dumps threads and ends the run | unset | none (`stall`, `error` need `frame`) |
| `XCL_ACTIVITY` | any value: log (and, with telemetry, write `activity`) every 2 s | unset | `frame` for the event |
| `XCL_RUN_SECONDS` | whole seconds; the title ends itself | unset | none (`end` needs `frame`) |
| `XCL_FRAME_DUMP` | read only to log that it is ignored with headless or skip | unset | none |
| `XCL_DIRECT_DXIL` | read only to record its value in the `shaders.json` header | unset | `pass` |

`XCL_FRAME_INTERVAL_US`, `XCL_PRESENT_SYNC`, `XCL_SPLIT_SCRATCH_MB`, `XCL_WAVE64_SPIN`,
`XCL_X64_ALL_MASKS`, `XCL_RECOMP_NO_STRUCTURED` and `XCL_DXIL_NO_OPT` are not read by telemetry.
They are listed in `diagnostic` when set (5.2).

## 6. Reading a run

### `scripts/dev/smoke-events.py`

Prints the `xcl-smoke v1` block of a finished run: the same 12 lines `scripts/smoke.ps1` prints.
stdout carries only the block. Each figure comes from `events.jsonl` when an event carries it
(`exit`, `end` with `run_seconds`, `activity`, `error`), and from the log otherwise. `gpu` comes
from `report.json`, then the manifest's `adapter`. `device removals` always comes from the log.
Python 3.9+, standard library only. Exit 0 on success, 2 on usage or unreadable input.

```
python scripts/dev/smoke-events.py --log <xcl.log> (--game <folder> | --title <label>)
       [--run <run-dir>] [--seconds N] [--repo <checkout>] [--explain]
```

- `--run` defaults to the newest `run-*` folder next to the log whose manifest `log` names this
  log. With none found, it reads the log only.
- `--seconds` defaults to the manifest's `switches.XCL_RUN_SECONDS`.
- `--explain` writes the source of each line, the manifest's `compare_key` and the number of
  malformed event lines to stderr.
- Known differences from `smoke.ps1` are in the script's docstring.

It prints the smoke block and nothing else. The questions below are answered by reading the files.

### Recipes

1. **Where did the frame budget go?** Take the `frame` events of the slow stretch. `interval_ms`
   is the wall time. `wait_ms` is time spent on the title's frame clock; a large value means the
   title was ahead of its clock. If `gpu_ms` is close to `interval_ms - wait_ms`, the frame is
   GPU-bound. If `cpu_ms` is close to it and `gpu_ms` is small, it is CPU-bound. `pso_wait_ms`
   and `look_ms` split off two XCL costs. A `gpu_ms` with `gpu_lists_null > 0` is a lower bound.
2. **Which list or pass is slow?** Take the frame's `i`. Its `list` events have `frame` = `i - 1`.
   Sort them by `end_ms - start_ms`. For the slowest, read the `pass` events with the same `frame`
   and `list` equal to the list's `seq`. Passes tile their list, so the pass with the longest
   `end_ms - start_ms` is the answer. Read its `kind`, `draws` and `barriers_before`.
3. **Which pipeline and shader did a slow pass run?** Take `pipelines` of the pass. For each id,
   find the `pipeline_bind` event with that `id`; its `records` are the console shader hashes.
   Look each up in `shaders.json` (`record`) for form, `register_arrays`, `scratch_hint`,
   `exchanges`, loops. The `pipeline` build events with the same `records` give the build cost.
4. **Did a loop hit its guard?** Check `manifest.loop_counters` and `loop_budget`. Read the
   `loop_counters` event: `slots > 0` means counters ran, and `tripped` says how many shader sets
   left a loop through the guard. `loop_guard` events name them (`record`, `stage`, `trips`).
5. **Why did the run end?** Read the last `end` event: `reason` and, for a failure, `fatal_kind`.
   Look before it for `error` events with `fatal: true` (and `stall` for a stall). The `exit` event
   gives the process status. With no `end` and no `exit`, the process was killed from outside:
   look at the last `frame`, `activity` and `error` events, and at `xcl.log`.

## 7. Known limits

- A queue that stays idle for 16 frames or more: its lists come out with null ticks and
  `why: "late"`.
- The first `manifest.json` write shows `sample: 0` and `list_slots: 0` until the engine starts.
- `shaders.json` closes shape intake when it is written. This is safe because every report write
  after the title starts is an end-of-run path (the one before the title starts finds no shape
  log).
- Command lists submitted after the engine's final read are not timed. One warning is logged.
- `dirty`, `rust`, `cores` and `title_version` in the manifest are always `null`.
- `resource` events carry only title-side creation calls (`committed`, `placed`, `reserved`).
  There is no `first_read` event.
- `list.translate_us` and `list.console_bytes` are always null.
- The automatic phase guess is weak (5.9).
- The pass-time comparison at `1x1` against the default window size was not run (5.11).
- A resubmitted list has valid list ticks but null pass ticks (5.4).
- Not tested on AMD or on a 64-lane GPU. The counters and the stamp engine were run on one RTX
  5080 only.
- The overhead numbers are from the test title, a worst case (5.10). No overhead was measured on
  a retail title.
- A telemetry run is not comparable with one without telemetry, and runs with different
  `compare_key` are not comparable with each other.

## 8. Work order and test status

All parts are built, on branch `adopt/diagnostics-2` (`git log --oneline --no-merges
a16cc528^..HEAD`). Tests: unit tests in each file's `#[cfg(test)]` modules; GPU tests print
`SKIPPED` and pass without a suitable GPU; e2e tests in `crates/xcl/tests/telemetry_e2e.rs` drive
`tests/testtitle` and need `cargo build --workspace` first.

| Part | Commit | Done | Tests |
|---|---|---|---|
| 1. Event log, manifest, refusals | `a16cc528` xcl run: XCL_TELEMETRY writes a run folder with manifest.json and events.jsonl | yes | unit: `log.rs` `telemetry_tests`, `report.rs` `telemetry_tests` (manifest keys, tiers, writer, run folder, refusals, stub thresholds, io failure) |
| 6. Events reader | `6c2260aa` Scripts: smoke-events.py builds the smoke block from a run's events.jsonl | yes | `scripts/dev/test_smoke_events.py`, `scripts/dev/smoke-events.tests.ps1` |
| 5a. No-window switches | `024cd51b` xcl run: XCL_HEADLESS, XCL_PRESENT_SKIP and XCL_WINDOW_SIZE | yes | unit: `telemetry_p5a_tests`; e2e: headless, window size, present skip |
| 4. Shader shape report | `c3aa3871` xcl run: shaders.json, the static shape of every translated console shader | yes | unit: `report.rs` `shape_tests`, `shape_glue_tests` |
| 5d. Loop budget on direct DXIL | `0a605ba3` Recompiler: XCL_LOOP_BUDGET also sets the loop budget of the direct DXIL path | yes | GPU: `gpu_direct_loop_budget_follows_the_variable` |
| 2. Timing math and fields | `9ac57fa4` xcl run: timing math and event fields for frame, list, pass and memory events | yes | unit: `log.rs` `timing_tests` |
| 2. GPU stamp engine | `23445025` xcl run: GPU stamp engine for per-list timing (not called yet) | yes | GPU: `telemetry_gpu.rs` `tests` (stamps chain, copy queue, full set, device removal, two-frame rule, exit flush) |
| 5b. Errors, end, exit, activity | `af6f718a` xcl run: error, end, exit and activity events; diagnostic runs are labelled | yes | unit: `telemetry_p5b_tests`; e2e: `a_normal_run_ends_with_end_and_exit`, `activity_events_mirror_the_log` |
| 2. Frame, list, clock, memory | `992ba819` xcl run: frame, list, clock and memory events with GPU time per command list | yes | e2e: `telemetry_frame_events_match_the_run`, `telemetry_off_creates_nothing`; unit: frame book tests in `telemetry_gpu.rs` |
| 5c. Phases | `a4d59d21` xcl run: phase events from a schedule, a control file and a guess | yes | unit: `telemetry_p5c_tests`; e2e: `phase_schedule_marks_the_run` |
| 5. End-to-end tests | `8b3397c2` e2e: telemetry tests for the no-window switches, error, end and phase events | yes | e2e: 8 tests in `telemetry_e2e.rs` |
| 5e. Loop counters, HLSL | `f7b1a311` Recompiler: count the lanes that leave a loop through its guard (XCL_LOOP_COUNTERS) | yes | unit: `telemetry_p5e_tests`, `loop_counters_tag_the_cache_identity`; GPU: `gpu_loop_counters_count_guard_exits` |
| 5e. Loop counters, direct DXIL | `96388e40` Recompiler: loop-guard counters on the direct DXIL path | yes | GPU: `gpu_direct_loop_counters_count_guard_exits` |
| 3. Pass timing, pipeline, resource, I/O | `99a76e3e` D3D12.X: time GPU passes and report pipeline, resource and I/O events (telemetry tier pass) | yes | unit and GPU: pass tests in `telemetry_gpu.rs` (tile, resubmitted, bounds, pipeline ids, io); GPU: `one_dispatch_timed_through_the_pass_path_matches_dispatch_timed`, `passes_of_lists_on_two_queues_overlap_and_the_frame_takes_the_union` |

Last whole-suite result (RTX 5080, `cargo test --workspace -- --test-threads=4`): **1603 passed,
4 failed, 47 ignored, 44 SKIPPED lines.** The 4 failures are
`gpu::dxil::fuzz_tests::fuzz_validate_and_gpu_0` to `_3`. They failed on NVIDIA before this work.
`cargo test -p xcl --lib gpu_tel`: 49 passed. `cargo test -p xcl --test telemetry_e2e`: 8 passed.
Not run on AMD.

## 9. Decisions

| | Decision |
|---|---|
| D1 | JSON Lines for the event stream, plus a separate manifest. Each line is written whole and unbuffered. |
| D2 | GPU timing per pass; per list as the fallback. Both are built. |
| D3 | Both the loop budget switch and the per-shader loop counters are built, budget first. |
| D4 | Automatic phase guess with a confidence value, plus manual marks (schedule and control file). |
| D5 | No-window runs keep the title's own frame clock. |
| D6 | **Corrected.** The generic code is appended to `log.rs` and `report.rs`. The GPU engine and the run-level code are in two fork-only files, `telemetry_gpu.rs` and `telemetry_run.rs`, included from `report.rs`. Upstream files get one-line call sites only. This replaces "no new module". |
| D7 | Overhead measured on the test title and reported (5.10). It exceeded 5 % in that worst case. The default sampling stays 1; `XCL_TELEMETRY_SAMPLE` is the switch. |
| D8 | Fork-only. The shape report stays self-contained. |
| D9 | Pass counters are per list and shared, not thread-local. The title records a list on one thread and submits it on another, and thread-local counters lost them. |
| D10 | Barriers are counted where the title passes a batch, before XCL drops transitions that are no-ops on PC. Counting after XCL's filtering gave 0 barriers in every frame. |
| D11 | `pipeline_bind` is a separate event. `pipeline` build events keep their own ids; the join from a pass to shader records goes through `pipeline_bind`. |
| D12 | `pass.pipelines` means pipelines drawn or dispatched with. `kind` gained `indirect`. |
| D13 | The loop-counter region is allocated in the first committed UMA window, the only one the CPU sees GPU stores in. One slot per distinct shader set. |
| D14 | `XCL_LOOP_COUNTERS` is read once from the environment, together with the tier, before the first translation. It joins the shader cache identity. |
| D15 | `XCL_LOOP_BUDGET` does not apply to the per-lane DXIL form, which has no loop guard. |
| D16 | `smoke.ps1` is not edited. A fork reader, `scripts/dev/smoke-events.py`, prints the same block from the events. |
| D17 | `XCL_RUN_SECONDS`, a window close and a title exit are not errors: they write `end` only. `XCL_STALL_DUMP` is a fatal `timeout`. |

## Risks

| Risk | Mitigation |
|---|---|
| Timing changes what is measured | `off` creates nothing; overhead measured (5.10); sampling is available |
| Summed pass times mislead | events carry ticks and queue; `gpu_ms` is a union |
| No-window mode changes pacing or resolution | `present_mode`, `window_size` and `compare_key` in the manifest; compare only equal keys |
| Upstream merges conflict with this code | appended code, own files, one-line call sites; merges are manual |
| No upstream review of the measurement code | unit, GPU and e2e tests; a wrong number is worse than none |
| Phase detection is guesswork | `confidence` on every event; `source` says who wrote it; manual marks override |
| No baseline on a retail title in this document | take one before reading any result as a regression or a gain |
