# GPU and shader recompiler

How XCL turns the Xbox Series GPU's code into something a PC GPU runs, and the supporting
pieces around it (texture detiling, unified memory windows, ray-tracing BVH, adapter selection).
Covers `crates/xcl/src/gpu/*`, `crates/xcl/src/gpu/recomp/*` and `crates/xcl/src/shaders.rs`.
The D3D12.X side that calls into it (command lists, PSO creation hooks, present) is in
[`d3d12x.md`](d3d12x.md). Product-level status is in `README.md` (rows "Shader recompiler",
"Pipelines", "Textures", "Ray tracing"); measurements and open items are in `HANDOFF.md`.

Design references: `docs/specs/2026-11-20-xcl-design.md:268` (5.7a, recompiler decision),
`docs/specs/2026-11-20-xcl-design.md:331` (5.7b, status), `docs/wave64-on-wave32.md:1`
(wave64 on 32-lane GPUs).

## Contents

- [Overview and pipeline](#overview-and-pipeline)
- [Wave64 on wave32 GPUs](#wave64-on-wave32-gpus)
- [Textures, UMA, BVH in one paragraph each](#textures-uma-bvh-in-one-paragraph-each)
- [Generated files: never edit by hand](#generated-files-never-edit-by-hand)
- [Environment variables](#environment-variables)
- [Per-file reference](#per-file-reference)
- [How to add support for a new RDNA2 instruction](#how-to-add-support-for-a-new-rdna2-instruction)
- [Known limits](#known-limits)

## Overview and pipeline

A console title hands D3D12.X a shader blob. On the console the blob is a DXBC container
whose DXIL part holds no bitcode, only native RDNA2 machine code
(`crates/xcl/src/gpu/xbox.rs:1`). XCL decodes that code, emits HLSL that reproduces its
semantics lane by lane, compiles the HLSL with DXC to DXIL, and creates the PC pipeline state
with its own root signature (the "shader ABI").

```text
 shader pack (RPF9 archive)          title's CreatePipelineState(blob)
   gpu/pack.rs  scan + inflate         gpu/shader.rs  classify + capture (dump)
            \                          /
             v                        v
      DXBC container  (parts: ISG1/OSG1/PSV0/DXIL ...)   gpu/shader.rs, gpu/dxbc.rs
             |  DXIL part: XDAT -> XBCH (zlib) -> FSSM
             v
      xbox::parse  ->  XboxShader { Fssm { programs, stages, code } }   gpu/xbox.rs
             |  recomp::interface(): VS / PS / CS / MS / GS interface from signatures + FSSM
             v
      rdna2::decode  (tables from rdna2_spec.rs + rdna2_xbox.rs)       gpu/rdna2.rs
             |  Vec<Decoded>, far jumps resolved
             v
      recomp::translate  --  basic blocks, structured control flow     gpu/recomp/mod.rs
             |  per instruction: emit::Em::inst                         gpu/recomp/emit.rs
             v
      HLSL text  (+ preludes, mem windows, static samplers)            Translation
             |  dxc::compile / compile_cached  (in-process DXC, disk cache)   recomp/dxc.rs
             v
      signed DXIL  ->  pso::console_to_dxil  ->  D3D12 PSO              recomp/pso.rs
```

Key entry points:

| Step | Function | Location |
|---|---|---|
| Classify a blob | `shader::classify` | `crates/xcl/src/gpu/shader.rs:88` |
| Parse console container | `xbox::parse` | `crates/xcl/src/gpu/xbox.rs:185` |
| Pick the translation mode | `recomp::interface` | `crates/xcl/src/gpu/recomp/mod.rs:434` |
| Translate | `recomp::translate` | `crates/xcl/src/gpu/recomp/mod.rs:1296` |
| Compile HLSL to DXIL | `dxc::compile`, `dxc::compile_cached` | `crates/xcl/src/gpu/recomp/dxc.rs:461`, `crates/xcl/src/gpu/recomp/dxc.rs:887` |
| Blob to DXIL for a stage | `pso::console_to_dxil_in` | `crates/xcl/src/gpu/recomp/pso.rs:108` |
| Run a program on the GPU in a test | `runner::run_code` | `crates/xcl/src/gpu/recomp/runner.rs:602` |

### Execution model (why the HLSL looks the way it does)

From the module doc of `crates/xcl/src/gpu/recomp/mod.rs:1`:

- Every branch in AMD GPU code is scalar, hence uniform across the wave, so control flow is
  exact without structurisation. SGPRs, VCC, EXEC, M0 and SCC are ordinary variables every lane
  computes alike; VGPRs are per-lane variables; vector writes are masked by the lane's EXEC bit;
  lane masks come from `WaveActiveBallot`; cross-lane reads use `WaveReadLaneAt`.
- Branch-free programs are emitted straight-line (the `branches == 0` path of `translate`,
  `crates/xcl/src/gpu/recomp/mod.rs:1364`). Otherwise `translate` first tries
  structured control flow (`structurable` at `crates/xcl/src/gpu/recomp/mod.rs:756`,
  `emit_structured` at `crates/xcl/src/gpu/recomp/mod.rs:888`: if / else, loops, rotated loops,
  forward jumps as `xcl_skip_<target>` flags, nesting capped by `MAX_NEST` at
  `crates/xcl/src/gpu/recomp/mod.rs:633`) and falls back to a guarded `if (pc == start)` block
  chain (`emit_range`, `crates/xcl/src/gpu/recomp/mod.rs:1041`).
- Every emitted loop is capped by an iteration budget (`LOOP_GUARD`,
  `crates/xcl/src/gpu/recomp/mod.rs:590`) so console code that loops on a wrongly passed
  value cannot hang the PC GPU.
- Memory is a flat model: base/descriptor addresses index `ByteAddressBuffer`s (`mem`, plus
  `mem1`.. for further 4 GiB windows); initial SGPRs (user data) come from `io`; kernel mode
  results go to `res` (`crates/xcl/src/gpu/recomp/mod.rs:66`, `crates/xcl/src/gpu/recomp/emit.rs:4079`).

### Modes

`Mode` (`crates/xcl/src/gpu/recomp/mod.rs:244`):

| Mode | What it is |
|---|---|
| `Kernel` | compute shader that loads the register file from a buffer, runs the program, stores registers and exports back; used by instruction tests |
| `Compute(CsInterface)` | real compute ABI: user SGPRs from `io`, group ids, thread ids (`crates/xcl/src/gpu/recomp/mod.rs:209`) |
| `Vertex` / `Pixel` | graphics entry points following DXBC signatures (`VsInterface` `crates/xcl/src/gpu/recomp/mod.rs:87`, `PsInterface` `crates/xcl/src/gpu/recomp/mod.rs:121`) |
| `Mesh(MsInterface)` | NGG / mesh program: translated as compute (one record per group in `res`) plus a generated pass-through mesh shader (`emit::mesh_passthrough`, `crates/xcl/src/gpu/recomp/emit.rs:4612`) |
| `Geometry(GsInterface)` | ES + GS merged into one compute shader, same record scheme (`geometry_program`, `crates/xcl/src/gpu/recomp/mod.rs:255`) |

DXC allows `WaveSize` only for compute, so MS and GS compile with a `cs_6_6` profile
(`crates/xcl/src/gpu/shader.rs:388`, and "Translator survey over the whole pack" in
`HANDOFF.md:1259`).

## Wave64 on wave32 GPUs

The console runs 64-lane waves. Some PC GPUs (NVIDIA) only run 32-lane waves, and D3D12
rejects `[WaveSize(64)]` there. The design is in `docs/wave64-on-wave32.md:1`; the code:

- `Options::split_wave64` (`crates/xcl/src/gpu/recomp/mod.rs:471`): one console wave64 becomes
  two hardware wave32 halves of the same thread group. The halves exchange masks
  (`ballot`), `v_readlane`, `v_readfirstlane`, `ds_bpermute` and GLC scalar loads through
  groupshared memory; DPP16/DPP8/`ds_swizzle`/`v_permlane16` stay inside a half
  (`docs/wave64-on-wave32.md:39`, table). The exchange machinery is the `PRELUDE_X64` HLSL in
  `crates/xcl/src/gpu/recomp/emit.rs:5198`.
- Which adapters need it: `pso::wave32_only` (`crates/xcl/src/gpu/recomp/pso.rs:534`) and
  `pso::wave32_only_for_device` (`crates/xcl/src/gpu/recomp/pso.rs:567`); `XCL_SPLIT_WAVE64=1`
  forces the split form on any GPU (`crates/xcl/src/gpu/recomp/pso.rs:540`). In tests
  `runner::options` / `runner::split_for` choose it (`crates/xcl/src/gpu/recomp/runner.rs:387`,
  `crates/xcl/src/gpu/recomp/runner.rs:396`).
- Fewer exchanges: `x64_masks.rs` is a backward liveness pass over SGPRs; a mask only read lane
  by lane (`v_cndmask` selector, carry-in) stays in its half
  (`crates/xcl/src/gpu/recomp/x64_masks.rs:1`, `by_pc` at
  `crates/xcl/src/gpu/recomp/x64_masks.rs:197`, wired in at
  `crates/xcl/src/gpu/recomp/mod.rs:1339`). `XCL_X64_ALL_MASKS` disables the optimisation.
- Wave64 vertex and pixel shaders cannot be paired (the driver picks the wave size):
  `upper_lanes.rs` is a forward analysis of what console lanes 32..63 hold where the program
  reads them; the emitter uses it when `WaveGetLaneCount() < 64`
  (`crates/xcl/src/gpu/recomp/upper_lanes.rs:1`, `analyze` at
  `crates/xcl/src/gpu/recomp/upper_lanes.rs:753`, wired in at
  `crates/xcl/src/gpu/recomp/mod.rs:1345`). `pso::graphics_wave32`
  (`crates/xcl/src/gpu/recomp/pso.rs:549`) says when this form is needed (also some RDNA2
  drivers pick 32).
- Hazard reports: `wave_hazards` / `wave_lane_reads`
  (`crates/xcl/src/gpu/recomp/mod.rs:1097`, `crates/xcl/src/gpu/recomp/mod.rs:1102`),
  `geometry_wave32_hazards` (`crates/xcl/src/gpu/recomp/mod.rs:1119`).
- `XCL_WAVE64_SPIN` runs every split program in the counter-based synchronisation variant
  (`crates/xcl/src/gpu/recomp/emit.rs:4039`).

Native (64-lane) HLSL is meant to stay unchanged by all of this; the form is generated only
when `split_wave64` is set (`docs/wave64-on-wave32.md:7`). Measurements and PR history:
`HANDOFF.md:1669` ("Wave64 on 32-lane GPUs (NVIDIA)") and `HANDOFF.md:1701` ("Wave64 VS / PS
in 32-lane waves").

## Textures, UMA, BVH in one paragraph each

- **Textures.** A console texture is memory the title wrote already tiled (GFX10 swizzle).
  `desc.rs` decodes T#/S#/V# descriptors into D3D12 terms; `tiling.rs` implements the address
  functions from AMD addrlib patterns (`tiling_tables.rs`, generated); `texture.rs` detiles every
  mip and slice into D3D12 subresources and uploads them. See `HANDOFF.md:1132` ("Tiled
  textures from console memory") and `HANDOFF.md:1114` ("3D and linear textures").
- **UMA windows.** Title graphics memory is kept in regions D3D12 can open as heaps. A heap is
  below 4 GiB, so memory is up to four windows at `base + k * 4 GiB`; the window of a console
  address is `(address >> 32) & 3` (`crates/xcl/src/gpu/uma.rs:1`). The recompiler's
  `Options::mem_windows` makes the HLSL pick the window from the high address bits
  (`crates/xcl/src/gpu/recomp/mod.rs:471`, `windowed` at
  `crates/xcl/src/gpu/recomp/emit.rs:4923`). See `HANDOFF.md:1065`.
- **BVH.** `bvh.rs` builds and traverses acceleration structures in the console's RDNA2 node
  format; the recompiler emulates `image_bvh(64)_intersect_ray` with the HLSL helper
  `bvh_intersect` (`crates/xcl/src/gpu/recomp/emit.rs:5107`, emitted at
  `crates/xcl/src/gpu/recomp/emit.rs:1965`). See `HANDOFF.md:458` ("TLAS on the GPU").

## Generated files: never edit by hand

| File | Generated by | Input |
|---|---|---|
| `crates/xcl/src/gpu/rdna2_spec.rs` | `scripts/gen_rdna2.py` (header `crates/xcl/src/gpu/rdna2_spec.rs:1`) | AMD Machine-Readable GPU ISA spec, RDNA 2 XML (`scripts/gen_rdna2.py:1`) |
| `crates/xcl/src/gpu/tiling_tables.rs` | `scripts/gen_tiling.py` (header `crates/xcl/src/gpu/tiling_tables.rs:1`) | Mesa addrlib `gfx10SwizzlePattern.h` (`scripts/gen_tiling.py:1`) |

Change the generator and rerun it; do not patch the output (`AGENTS.md`, "Never"). Console-only
instructions that the retail spec does not list live in the hand-written
`crates/xcl/src/gpu/rdna2_xbox.rs:1`, not in the generated file. (`crates/xcl/src/hle/d3d12_layout.rs`
is also generated, by `scripts/gen_d3d12_layout.py`, but is outside this page.)

## Environment variables

Read by code under this page's scope (non-test unless noted):

| Variable | Where | Meaning |
|---|---|---|
| `XCL_ADAPTER` | `crates/xcl/src/gpu/adapter.rs:20` | adapter selector (`auto`, `warp`, index, name substring); overrides `d3d12_adapter` in `xcl.toml` |
| `XCL_D3D12_DIR` | `crates/xcl/src/gpu/agility.rs:30` | folder with an Agility SDK `D3D12Core.dll` |
| `XCL_DXC_DIR` | `crates/xcl/src/gpu/recomp/dxc.rs:78` | folder with `dxcompiler.dll` / `dxil.dll` |
| `XCL_DXC_THREADS` | `crates/xcl/src/gpu/recomp/dxc.rs:385` | DXC pool size (default cores - 1, clamped 1..64) |
| `XCL_DXIL_CACHE` | `crates/xcl/src/gpu/recomp/dxc.rs:556` | DXIL cache folder (default `%LOCALAPPDATA%\xcl\dxil`) |
| `XCL_TRANSLATED_CACHE` | `crates/xcl/src/gpu/recomp/pso.rs:145` | `0` turns off the on-disk translation cache |
| `XCL_SPLIT_WAVE64` | `crates/xcl/src/gpu/recomp/pso.rs:541` | `1` forces the split wave64 form |
| `XCL_RECOMP_NO_STRUCTURED` | `crates/xcl/src/gpu/recomp/mod.rs:1387` | always use the guarded block chain |
| `XCL_RECOMP_WHY` | `crates/xcl/src/gpu/recomp/mod.rs:576` | print why a program stays unstructured |
| `XCL_X64_ALL_MASKS` | `crates/xcl/src/gpu/recomp/mod.rs:1339` | exchange every mask in the split form |
| `XCL_WAVE64_SPIN` | `crates/xcl/src/gpu/recomp/emit.rs:4039` | counter-based sync variant of the split form |
| `XCL_DEBUG_LOD0`, `XCL_DEBUG_PS_SUB`, `XCL_DEBUG_PS_EXPR`, `XCL_DEBUG_MAGENTA` | `crates/xcl/src/gpu/recomp/pso.rs:239`, `crates/xcl/src/gpu/recomp/pso.rs:244`, `crates/xcl/src/gpu/recomp/pso.rs:257`, `crates/xcl/src/gpu/recomp/pso.rs:304` | pixel-shader debugging switches |
| `XCL_REQUIRE_GPU` | `crates/xcl/src/gpu/recomp/runner.rs:432` | `wave64` / `mesh`: a GPU-test skip becomes a failure |
| `XCL_DUMP_SHADERS` | `crates/xcl/src/gpu/shader.rs:325` | `1`: write captured shader blobs and listings |
| `XCL_RECOMP_DXC` | `crates/xcl/src/gpu/shader.rs:382` | `1`: also compile translations during capture |
| `XCL_UMA_MB` | `crates/xcl/src/gpu/uma.rs:737` | UMA size in MB (default 2048) |
| `XCL_COHERENT_MB` | `crates/xcl/src/gpu/uma.rs:679` | coherent window size in MB |
| `XCL_TRACE_MEM` | `crates/xcl/src/gpu/uma.rs:534` | trace UMA allocations |
| `XCL_RECOMP_ONLY` | `crates/xcl/src/shaders.rs:355` | `xcl shaders --recomp`: one program by hash |
| `XCL_RECOMP_WINDOWS` | `crates/xcl/src/shaders.rs:392` | memory windows to translate for (default 1) |
| `XCL_PIPELINE_THREADS` | `crates/xcl/src/shaders.rs:763` | threads of `--precompile` pipeline creation (default 4) |
| `XCL_DUMP_HLSL`, `XCL_DUMP_EVERY` | `crates/xcl/src/shaders.rs:1277`, `crates/xcl/src/shaders.rs:1278` | write every N-th translation as HLSL |
| `XCL_EXAMPLE_HLSL` | `crates/xcl/src/shaders.rs:1214` | save the example HLSL |
| `XCL_SAMPLE_RPF` | `crates/xcl/src/shaders.rs:1259` | shader pack for the retail-sample test (tests: skipped when unset) |
| `XCL_SAMPLE_TTPF` | `crates/xcl/src/gpu/texture.rs:668` | texture archive for the retail detiling test |

Test-only variables in `crates/xcl/src/gpu/recomp/tests.rs`, documented there:
`XCL_SAMPLE_RPF` (`crates/xcl/src/gpu/recomp/tests.rs:1348`), the survey family
`XCL_SURVEY_EVERY`, `XCL_SURVEY_DXC` (`crates/xcl/src/gpu/recomp/tests.rs:4757`,
`crates/xcl/src/gpu/recomp/tests.rs:4760`), `XCL_SURVEY_SEED`
(`crates/xcl/src/gpu/recomp/tests.rs:4771`), `XCL_SURVEY_WINDOWS`
(`crates/xcl/src/gpu/recomp/tests.rs:4853`), `XCL_NAMES`, `XCL_DXC_ARGS`
(`crates/xcl/src/gpu/recomp/tests.rs:4984`), `XCL_HLSL_OUT` (`crates/xcl/src/gpu/recomp/tests.rs:4711`),
`XCL_SAMPLE_FSPX` (`crates/xcl/src/gpu/recomp/tests.rs:7157`), `XCL_FUZZ_ITERS`
(`crates/xcl/src/gpu/recomp/tests.rs:6162`). The full `XCL_*` list is in `cli-and-config.md`.

Names such as `XCL_PIXEL` and `XCL_X_SLOT` in `emit.rs` are HLSL macros and identifiers in
the generated shader (`crates/xcl/src/gpu/recomp/emit.rs:4084`), not environment variables.

## Per-file reference

### gpu/mod.rs

Declares the submodules and re-exports the generated ISA name as `RDNA2_ARCH`
(`crates/xcl/src/gpu/mod.rs:1`, `crates/xcl/src/gpu/mod.rs:22`). `rdna2_spec`, `rdna2_xbox`
and `tiling_tables` are private; `fence` is crate-private.

### gpu/rdna2.rs - decoder and disassembler

- Purpose: first stage of the recompiler. Tables come from the generated spec; this file
  implements encoding identification, literal constants, SDWA/DPP extension dwords and MIMG NSA
  (`crates/xcl/src/gpu/rdna2.rs:1`).
- Key items: `decode` (`crates/xcl/src/gpu/rdna2.rs:176`) returning `Decoded`
  (`crates/xcl/src/gpu/rdna2.rs:81`); `encode` assembler used by tests
  (`crates/xcl/src/gpu/rdna2.rs:362`); `sgpr_uses` / `vgpr_uses`
  (`crates/xcl/src/gpu/rdna2.rs:426`, `crates/xcl/src/gpu/rdna2.rs:451`); `branch_target` and
  `resolve_far_jumps` (`crates/xcl/src/gpu/rdna2.rs:491`, `crates/xcl/src/gpu/rdna2.rs:520`);
  `disassemble` and `find_programs` (`crates/xcl/src/gpu/rdna2.rs:613`,
  `crates/xcl/src/gpu/rdna2.rs:638`); flags `F_CONDITIONAL`, `F_TERMINATOR`
  (`crates/xcl/src/gpu/rdna2.rs:54`, `crates/xcl/src/gpu/rdna2.rs:56`).
- The instruction table merges the spec with console-only instructions
  (`crates/xcl/src/gpu/rdna2.rs:13`); the spec tables take priority on a clash
  (`crates/xcl/src/gpu/rdna2_xbox.rs:7`).
- Tests: unit tests in the file (`crates/xcl/src/gpu/rdna2.rs:665`), among them the LLVM gfx10
  corpus comparison reading `tests/data/llvm-amdgpu` (`crates/xcl/src/gpu/rdna2.rs:787`).

### gpu/rdna2_spec.rs - generated

ISA tables (`ENCODINGS`, `INSTRUCTIONS`, `ARCH`) from AMD's machine-readable spec, MIT licensed,
release 2026-02-20 (`crates/xcl/src/gpu/rdna2_spec.rs:1`). Generated by `scripts/gen_rdna2.py`
(usage in `scripts/gen_rdna2.py:3`). Do not edit.

### gpu/rdna2_xbox.rs

Instructions the console GPU executes that retail gfx10.3 does not list: the non-fused
multiply-add family of gfx10.1 that gfx10.3 removed. It was found because 1.3% of retail
dwords were undecodable (`crates/xcl/src/gpu/rdna2_xbox.rs:1`). The decoder names
`V_MAC_LEGACY_F32` / `V_MAD_LEGACY_F32` after the spec, so the recompiler decides the rounding
behaviour (`crates/xcl/src/gpu/rdna2_xbox.rs:10`).

### gpu/xbox.rs - console shader containers

- Purpose: parse DXBC containers whose DXIL part is XDAT -> XBCH (zlib) -> FSSM and extract the
  RDNA2 programs. The layout was reverse-engineered from a retail pack; every step has a
  fallback because most FSSM fields are unknown (`crates/xcl/src/gpu/xbox.rs:1`).
- Key items: `parse` -> `XboxShader` (`crates/xcl/src/gpu/xbox.rs:185`,
  `crates/xcl/src/gpu/xbox.rs:155`), `Fssm` (`crates/xcl/src/gpu/xbox.rs:118`), `HwStage`
  (`crates/xcl/src/gpu/xbox.rs:96`), `Program` (`crates/xcl/src/gpu/xbox.rs:110`), `is_xbox`
  (`crates/xcl/src/gpu/xbox.rs:179`), `split_segments` separating code from constant data
  (`crates/xcl/src/gpu/xbox.rs:450`), `InstStats` histogram (`crates/xcl/src/gpu/xbox.rs:499`),
  `listing` (`crates/xcl/src/gpu/xbox.rs:548`).
- Tests: `crates/xcl/src/gpu/xbox.rs:591` (container builders reused by other test modules,
  e.g. `retail`-shaped layout, GS with copy shader, bad header pointer).

### gpu/shader.rs - capture and classification

- Every blob a title hands to D3D12.X is classified and, once per unique hash, written to
  `<data>\shaders\` when `XCL_DUMP_SHADERS=1` (`crates/xcl/src/gpu/shader.rs:1`,
  `crates/xcl/src/gpu/shader.rs:323`).
- Key items: `Kind` (`crates/xcl/src/gpu/shader.rs:58`: `Dxil`, `Dxbc`, `XboxNative`,
  `ContainerUnknown`, `Elf`, `Rdna2Raw`, `Unknown`), `container_parts`
  (`crates/xcl/src/gpu/shader.rs:31`), `classify` (`crates/xcl/src/gpu/shader.rs:88`),
  `capture` (`crates/xcl/src/gpu/shader.rs:266`), `readable` (checks a title pointer without
  touching pages, `crates/xcl/src/gpu/shader.rs:206`), `fnv64` (the shader hash used as cache
  key, `crates/xcl/src/gpu/shader.rs:141`), `stats` / `ShaderStats`
  (`crates/xcl/src/gpu/shader.rs:421`, `crates/xcl/src/gpu/shader.rs:151`).
- Tests: `crates/xcl/src/gpu/shader.rs:434`.

### gpu/dxbc.rs

Reads DXBC input/output signatures (ISG1/OSG1/PSG1 and the older ISGN/OSGN/PCSG), the semantics
that link console VS outputs to PS inputs (`crates/xcl/src/gpu/dxbc.rs:1`). Entry:
`signatures` (`crates/xcl/src/gpu/dxbc.rs:70`). The PSV0 reader `dxbc::psv` (numthreads, mesh
topology and limits) is used by `interface` (`crates/xcl/src/gpu/recomp/mod.rs:434`). Tests:
`crates/xcl/src/gpu/dxbc.rs:127`.

### gpu/pack.rs

Extracts shader blobs from a packed archive without its (encrypted) table of contents: shaders
are plain records `u8 len | name | 0x00 | flag | sizes | deflate-raw payload`
(`crates/xcl/src/gpu/pack.rs:1`). `scan` finds records, `inflate` unpacks one
(`crates/xcl/src/gpu/pack.rs:43`, `crates/xcl/src/gpu/pack.rs:86`). Resuming after an accepted
record's payload skips the aliases that a name suffix would create (`crates/xcl/src/gpu/pack.rs:15`).
Tests: `crates/xcl/src/gpu/pack.rs:101` (including qualified names and hostile ranges). History:
`HANDOFF.md:1232` ("Shader pack: every record, every program").

### gpu/desc.rs - T# / S# / V# descriptors

GFX10 image (8 dwords), sampler (4) and buffer (4) descriptors decoded into D3D12 terms
(`crates/xcl/src/gpu/desc.rs:1`). Key items: `image` -> `Image` (`crates/xcl/src/gpu/desc.rs:48`,
`crates/xcl/src/gpu/desc.rs:24`), `dxgi_format` (`crates/xcl/src/gpu/desc.rs:79`),
`component_mapping` (`crates/xcl/src/gpu/desc.rs:150`), `image_d3d12`
(`crates/xcl/src/gpu/desc.rs:212`), `sampler_d3d12` (`crates/xcl/src/gpu/desc.rs:280`), `buffer`
(`crates/xcl/src/gpu/desc.rs:354`), and builders for the reverse direction (`xcl_image_words`,
`buffer_words`, `bvh_words` at `crates/xcl/src/gpu/desc.rs:329`,
`crates/xcl/src/gpu/desc.rs:680`, `crates/xcl/src/gpu/desc.rs:689`). Tests:
`crates/xcl/src/gpu/desc.rs` (11 `#[test]`).

### gpu/tiling.rs and gpu/tiling_tables.rs (generated)

- Purpose: GFX10 tiling from addrlib swizzle patterns. A pattern gives each address bit inside
  a block as the XOR of x/y/z bits, so the offset is linear over GF(2):
  `offset(x, y, z) = ox[x] ^ oy[y] ^ oz[z]` (`crates/xcl/src/gpu/tiling.rs:1`).
- Key items: `PipeConfig` (`crates/xcl/src/gpu/tiling.rs:13`), `Layout` with `new`, `new_3d`,
  `offset`, `detile`, `tile` (`crates/xcl/src/gpu/tiling.rs:55`, `crates/xcl/src/gpu/tiling.rs:80`,
  `crates/xcl/src/gpu/tiling.rs:101`, `crates/xcl/src/gpu/tiling.rs:164`,
  `crates/xcl/src/gpu/tiling.rs:179`, `crates/xcl/src/gpu/tiling.rs:195`), `element`
  (`crates/xcl/src/gpu/tiling.rs:211`), `Chain` for mip chains with tail blocks
  (`crates/xcl/src/gpu/tiling.rs:259`), `image_chain` (`crates/xcl/src/gpu/tiling.rs:427`).
- `tiling_tables.rs` is generated by `scripts/gen_tiling.py` from Mesa's `gfx10SwizzlePattern.h`
  (`crates/xcl/src/gpu/tiling_tables.rs:1`). Do not edit.
- Tests: `crates/xcl/src/gpu/tiling.rs` (11 `#[test]`); the retail check is in `texture.rs`.
- Limits: MSAA is not covered; modes and measurements in `HANDOFF.md:1114` and `HANDOFF.md:1132`.

### gpu/texture.rs

- Purpose: console memory -> PC texture. Every mip of every slice is detiled
  (`tiling::Chain`) into a D3D12 subresource and copied in (`crates/xcl/src/gpu/texture.rs:1`).
- Key items: `console_pipes` with `CONSOLE_PIPES` / `SERIES_S_PIPES`
  (`crates/xcl/src/gpu/texture.rs:26`, `crates/xcl/src/gpu/texture.rs:22`,
  `crates/xcl/src/gpu/texture.rs:23`), `placement` (`crates/xcl/src/gpu/texture.rs:54`),
  `subresources` (`crates/xcl/src/gpu/texture.rs:83`), `uma_bytes`
  (`crates/xcl/src/gpu/texture.rs:123`), `create` (`crates/xcl/src/gpu/texture.rs:140`), `upload`,
  `upload_batch`, `record_upload` (`crates/xcl/src/gpu/texture.rs:193`,
  `crates/xcl/src/gpu/texture.rs:207`, `crates/xcl/src/gpu/texture.rs:230`).
- Tests: GPU test `gpu_console_textures_upload_every_mip_and_slice`
  (`crates/xcl/src/gpu/texture.rs:604`, bit-exact readback) and the retail check
  `retail_ttpf_textures_detile_with_their_mip_chains` (`crates/xcl/src/gpu/texture.rs:667`,
  needs `XCL_SAMPLE_TTPF`, does nothing when unset).
- Limits: the console's pipe configuration for the `_X` modes was still inferred, and MSAA and the
  binding to the title's texture-creation API are not here (`crates/xcl/src/gpu/texture.rs:9`).

### gpu/uma.rs - unified memory windows

- Purpose and layout: see "UMA windows" above (`crates/xcl/src/gpu/uma.rs:1`). Measured: the heap
  must cover a fully committed `VirtualAlloc` allocation; up to 4095 MiB works.
- Key items: constants `GRAIN`, `MAX_SIZE`, `WINDOW`, `MAX_WINDOWS`
  (`crates/xcl/src/gpu/uma.rs:24`, `crates/xcl/src/gpu/uma.rs:26`, `crates/xcl/src/gpu/uma.rs:28`,
  `crates/xcl/src/gpu/uma.rs:29`), `Uma` (`crates/xcl/src/gpu/uma.rs:43`) with `new`,
  `with_windows`, `for_runtime`, `alloc`, `alloc_in`, `free`, `window`, `window_of`
  (`crates/xcl/src/gpu/uma.rs:113`, `crates/xcl/src/gpu/uma.rs:376`,
  `crates/xcl/src/gpu/uma.rs:164`, `crates/xcl/src/gpu/uma.rs:503`,
  `crates/xcl/src/gpu/uma.rs:517`, `crates/xcl/src/gpu/uma.rs:575`,
  `crates/xcl/src/gpu/uma.rs:437`, `crates/xcl/src/gpu/uma.rs:473`); sparse VRAM windows
  (`sparse_reserve`, `sparse_alloc`, `crates/xcl/src/gpu/uma.rs:283`,
  `crates/xcl/src/gpu/uma.rs:321`); `Arena` for XCL's own blocks (`crates/xcl/src/gpu/uma.rs:630`);
  the global `get` (`crates/xcl/src/gpu/uma.rs:734`); the coherent window sized from the title's
  last run (`coherent_hint`, `note_coherent_need`, `crates/xcl/src/gpu/uma.rs:678`,
  `crates/xcl/src/gpu/uma.rs:713`).
- Env: `XCL_UMA_MB`, `XCL_COHERENT_MB`, `XCL_TRACE_MEM` (table above).
- Tests: `crates/xcl/src/gpu/uma.rs:816` (`windows_are_4_gib_apart_and_allocations_stay_inside`);
  GPU test `gpu_memory_windows_above_4_gib` (`crates/xcl/src/gpu/recomp/tests.rs:3122`).

### gpu/bvh.rs

- Purpose: acceleration structures in the console's format, as `image_bvh(64)_intersect_ray`
  reads them and the game's ray-tracing shaders read them: 256-byte header, box32 (type 5),
  box16 (type 4), triangle pairs (types 0/1), instances (type 6)
  (`crates/xcl/src/gpu/bvh.rs:1`).
- Key items: `build`, `build_top` (`crates/xcl/src/gpu/bvh.rs:85`, `crates/xcl/src/gpu/bvh.rs:98`),
  `prebuild_size` (`crates/xcl/src/gpu/bvh.rs:72`), `intersect_node` (the CPU reference of the
  instruction, `crates/xcl/src/gpu/bvh.rs:287`), `triangle_hit`
  (`crates/xcl/src/gpu/bvh.rs:248`), `Bvh::trace` / `trace_scene`
  (`crates/xcl/src/gpu/bvh.rs:386`, `crates/xcl/src/gpu/bvh.rs:392`).
- Tests: `crates/xcl/src/gpu/bvh.rs:417` (traversal checked against brute force; node-size bound
  from the console builder in `tests/console_dlls_e2e.rs`).
- Notes: the instance node's object-to-world matrix at @0 is an assumption, not read by the
  shaders seen (`crates/xcl/src/gpu/bvh.rs:22`).

### gpu/agility.rs

Optional DirectX 12 Agility SDK loading through `ID3D12SDKConfiguration1`, needed because
Windows 10's inbox `D3D12Core.dll` stops at Shader Model 6.5. Search order: `%XCL_D3D12_DIR%`,
`<exe_dir>\D3D12`, `<exe_dir>`, `%LOCALAPPDATA%\XCL\D3D12`; if none is found, device creation
falls back to inbox `D3D12CreateDevice` (`crates/xcl/src/gpu/agility.rs:1`). Key items:
`candidates` (`crates/xcl/src/gpu/agility.rs:50`), `resolve_dir`
(`crates/xcl/src/gpu/agility.rs:189`), `create_device` (`crates/xcl/src/gpu/agility.rs:319`),
`format_no_sm66_message` (`crates/xcl/src/gpu/agility.rs:301`). Callers outside this page:
`crates/xcl/src/preflight.rs:116`, `crates/xcl/src/shaders.rs:762`. Tests:
`crates/xcl/src/gpu/agility_tests.rs:1`, included via `#[path]` at
`crates/xcl/src/gpu/agility.rs:360`.

### gpu/adapter.rs

Which adapter XCL uses: `Selector` (`crates/xcl/src/gpu/adapter.rs:23`) parsed from
`d3d12_adapter` in `xcl.toml`, overridden by `XCL_ADAPTER`
(`crates/xcl/src/gpu/adapter.rs:51`). Values: `auto`, `warp`, index, or a name substring; an
out-of-range number falls back to a numeric token in the name (`5070`, `3080`)
(`crates/xcl/src/gpu/adapter.rs:1`). `hardware` and `candidates` enumerate
(`crates/xcl/src/gpu/adapter.rs:125`, `crates/xcl/src/gpu/adapter.rs:142`). The same choice
drives the title's device and the GPU test bench (`runner::adapter`), so GPU tests can be run
on each adapter of a machine in turn. Tests: `crates/xcl/src/gpu/adapter_tests.rs:1`, included at
`crates/xcl/src/gpu/adapter.rs:159`.

### gpu/fence.rs

Crate-private completion check for XCL-owned D3D12 fences: `wait` and `FenceError`
(`crates/xcl/src/gpu/fence.rs:13`, `crates/xcl/src/gpu/fence.rs:56`). It reports device removal
and timeouts distinctly and tolerates a reused event waking for an earlier registration. Used by
`runner.rs` (`crates/xcl/src/gpu/recomp/runner.rs:18`) and by the D3D12.X layer
(`crates/xcl/src/hle/d3d12x/present.rs:34`, `crates/xcl/src/hle/d3d12x/real.rs:270`). Tests with a mock
fence/device in the file (`crates/xcl/src/gpu/fence.rs:110`).

### gpu/recomp/mod.rs - translator driver

- Purpose: the recompiler's front: interfaces, blocks, control flow. Execution model in the
  module doc (`crates/xcl/src/gpu/recomp/mod.rs:1`).
- Key items: kernel ABI constants (`KERNEL_SGPRS`, `KERNEL_VGPRS`, `KERNEL_LANES`,
  `KERNEL_EXPORTS` at `crates/xcl/src/gpu/recomp/mod.rs:66`..`crates/xcl/src/gpu/recomp/mod.rs:83`);
  interfaces `VsInterface`, `PsInterface`, `MsInterface`, `GsInterface`, `CsInterface`
  (`crates/xcl/src/gpu/recomp/mod.rs:87`, `crates/xcl/src/gpu/recomp/mod.rs:121`,
  `crates/xcl/src/gpu/recomp/mod.rs:132`, `crates/xcl/src/gpu/recomp/mod.rs:157`,
  `crates/xcl/src/gpu/recomp/mod.rs:209`); `vs_output_table` / `ps_input_table` read the
  console's attribute tables from the FSSM (`crates/xcl/src/gpu/recomp/mod.rs:370`,
  `crates/xcl/src/gpu/recomp/mod.rs:347`); `Options` and `Translation`
  (`crates/xcl/src/gpu/recomp/mod.rs:471`, `crates/xcl/src/gpu/recomp/mod.rs:505`;
  `Translation::complete` is true when `unsupported` is empty); `detect_wave64`
  (`crates/xcl/src/gpu/recomp/mod.rs:1215`); `translate_geometry` / `translate_mesh`
  (`crates/xcl/src/gpu/recomp/mod.rs:1203`, `crates/xcl/src/gpu/recomp/mod.rs:1210`);
  `constant_samplers` finds S# built from constants (static samplers,
  `crates/xcl/src/gpu/recomp/mod.rs:1256`).
- Env: `XCL_RECOMP_NO_STRUCTURED`, `XCL_RECOMP_WHY`, `XCL_X64_ALL_MASKS`.
- Tests: three unit tests here (`crates/xcl/src/gpu/recomp/mod.rs:1418`) and the large
  `tests.rs` below. Open items: "Translator survey over the whole pack" (`HANDOFF.md:1259`).

### gpu/recomp/emit.rs - instruction semantics (5.6k lines)

- Purpose: one RDNA2 instruction -> HLSL statements (`crates/xcl/src/gpu/recomp/emit.rs:1`).
  `Em` holds the emission state (`crates/xcl/src/gpu/recomp/emit.rs:19`).
- Dispatcher: `Em::inst` routes by encoding name (`ENC_SOPP`, `ENC_SOP*`, `ENC_SMEM`,
  `ENC_MUBUF/MTBUF`, `ENC_EXP`, `ENC_DS`, `ENC_VINTRP`, `ENC_MIMG`, `ENC_FLAT*`, `ENC_VOP*`) at
  `crates/xcl/src/gpu/recomp/emit.rs:832`; instructions with no effect in this model are listed in
  `NOOPS` (`crates/xcl/src/gpu/recomp/emit.rs:66`); an unhandled one is recorded by `unsupported`
  and left as a comment (`crates/xcl/src/gpu/recomp/emit.rs:595`).
- Per-class handlers: `sopp` (`crates/xcl/src/gpu/recomp/emit.rs:876`), `salu`
  (`crates/xcl/src/gpu/recomp/emit.rs:1020`), `salu64` (`crates/xcl/src/gpu/recomp/emit.rs:1313`),
  `smem` (`crates/xcl/src/gpu/recomp/emit.rs:1442`), `mubuf` (`crates/xcl/src/gpu/recomp/emit.rs:1565`),
  `export` (`crates/xcl/src/gpu/recomp/emit.rs:1713`), `interp`
  (`crates/xcl/src/gpu/recomp/emit.rs:1876`), `mimg` (images, samplers, BVH,
  `crates/xcl/src/gpu/recomp/emit.rs:1943`), `valu` (`crates/xcl/src/gpu/recomp/emit.rs:3086`) with
  `valu16`, `valu64`, `valu_pk` (`crates/xcl/src/gpu/recomp/emit.rs:2804`,
  `crates/xcl/src/gpu/recomp/emit.rs:2939`, `crates/xcl/src/gpu/recomp/emit.rs:2577`), `vopc`
  (`crates/xcl/src/gpu/recomp/emit.rs:3476`), `flat` (`crates/xcl/src/gpu/recomp/emit.rs:3638`),
  `ds` (`crates/xcl/src/gpu/recomp/emit.rs:3814`).
- Output assembly: `finish` writes the header, bindings (`io` u0, `res` u1, `mem` u2, `mem1`.. u3..)
  and preludes (`crates/xcl/src/gpu/recomp/emit.rs:4079`); the HLSL helper preludes are
  `PRELUDE_MEM` (`crates/xcl/src/gpu/recomp/emit.rs:5063`), `PRELUDE_X64`
  (`crates/xcl/src/gpu/recomp/emit.rs:5198`) and `PRELUDE` (`crates/xcl/src/gpu/recomp/emit.rs:5291`).
  Multi-window access goes through accessors built by `windowed` and `mem_accessors`
  (`crates/xcl/src/gpu/recomp/emit.rs:4923`, `crates/xcl/src/gpu/recomp/emit.rs:4945`).
- Mesh/GS pass-through shaders: `mesh_passthrough`, `mesh_passthrough_for`, `rect_list_gs`,
  `passthrough_split` (`crates/xcl/src/gpu/recomp/emit.rs:4612`,
  `crates/xcl/src/gpu/recomp/emit.rs:4623`, `crates/xcl/src/gpu/recomp/emit.rs:4575`,
  `crates/xcl/src/gpu/recomp/emit.rs:4719`). The 32 KB output limit per mesh group is why a
  record can be split across several mesh groups (`crates/xcl/src/gpu/recomp/emit.rs:4714`,
  `HANDOFF.md:1206`).
- Codegen conventions to keep: no `?:`, `&&`, `||` on scalars in generated HLSL, because HLSL
  2021 turns them into real branches; use `select()`, `and()`, `or()` and the loop-free helpers in
  the preludes (`HANDOFF.md:458`; helper example `mul_legacy` at
  `crates/xcl/src/gpu/recomp/emit.rs:5371`).
- Tests: the `tests.rs` file.

### gpu/recomp/dxc.rs - HLSL to DXIL

- `dxcompiler.dll` is loaded at run time from `%XCL_DXC_DIR%`, the folder of `xcl.exe`, or the
  newest Windows SDK; `dxil.dll` is loaded first so DXIL is signed
  (`crates/xcl/src/gpu/recomp/dxc.rs:1`, `location` at `crates/xcl/src/gpu/recomp/dxc.rs:290`).
- Compilation runs on its own thread pool (`compile_threads`,
  `crates/xcl/src/gpu/recomp/dxc.rs:381`) because DXC's LLVM passes overflowed small game-thread
  stacks (`HANDOFF.md:1259`); `wait_for_memory` limits concurrent compiles by free commit
  (`crates/xcl/src/gpu/recomp/dxc.rs:415`).
- Cache: key is an FNV-1a hash of the crate version, toolchain identity, profile, entry and HLSL
  (`crates/xcl/src/gpu/recomp/dxc.rs:540`); folder from `cache_dir`
  (`crates/xcl/src/gpu/recomp/dxc.rs:555`); `compile_cached_in`
  (`crates/xcl/src/gpu/recomp/dxc.rs:631`); `cached_only` / `defer` for non-blocking callers
  (`crates/xcl/src/gpu/recomp/dxc.rs:864`, `crates/xcl/src/gpu/recomp/dxc.rs:874`); counters in
  `cache_stats` (`crates/xcl/src/gpu/recomp/dxc.rs:621`).
- Toolchain check: `RECOMMENDED` is DXC 1.8; an older one is flagged `outdated`
  (`crates/xcl/src/gpu/recomp/dxc.rs:155`, `crates/xcl/src/gpu/recomp/dxc.rs:164`) with the cost
  message at `crates/xcl/src/gpu/recomp/dxc.rs:170`.
- Env: `XCL_DXC_DIR`, `XCL_DXC_THREADS`, `XCL_DXIL_CACHE`. Tests: 14 unit tests in the file.

### gpu/recomp/pso.rs - blobs in pipeline states

- Purpose: a console shader passed to `Create*PipelineState` becomes DXIL, and the pipeline gets
  XCL's ABI root signature (root UAVs u0 `io`, u1 `res`, u2.. `mem` windows, plus descriptor
  heaps) instead of the title's (`crates/xcl/src/gpu/recomp/pso.rs:1`).
- Key items: `Stage` (`crates/xcl/src/gpu/recomp/pso.rs:19`), `is_console`
  (`crates/xcl/src/gpu/recomp/pso.rs:26`), `console_to_dxil` / `_for` / `_in`
  (`crates/xcl/src/gpu/recomp/pso.rs:94`..`crates/xcl/src/gpu/recomp/pso.rs:108`),
  `console_to_hlsl` (`crates/xcl/src/gpu/recomp/pso.rs:321`), `console_mesh`,
  `console_geometry` returning `TwoPass` (`crates/xcl/src/gpu/recomp/pso.rs:521`,
  `crates/xcl/src/gpu/recomp/pso.rs:577`, `crates/xcl/src/gpu/recomp/pso.rs:494`),
  `abi_root_signature(_with)` (`crates/xcl/src/gpu/recomp/pso.rs:596`,
  `crates/xcl/src/gpu/recomp/pso.rs:627`), `static_sampler_desc`
  (`crates/xcl/src/gpu/recomp/pso.rs:602`), `mem_windows` (`crates/xcl/src/gpu/recomp/pso.rs:77`),
  FSR4's `FSPX` containers via `fspx` (`crates/xcl/src/gpu/recomp/pso.rs:46`).
- Caches: an in-memory `ShaderCache` (64 MB, `crates/xcl/src/gpu/recomp/pso.rs:350`) and the
  on-disk `translated` module keyed by blob bytes, stage, parameters and the XCL binary hash
  (`crates/xcl/src/gpu/recomp/pso.rs:134`).
- Env: `XCL_TRANSLATED_CACHE`, `XCL_SPLIT_WAVE64`, `XCL_DEBUG_*` (table above). Tests: 9 unit
  tests in the file; PSO-through-hook tests in `crates/xcl/tests/gpu_pso_e2e.rs`. Non-blocking
  creation is in `d3d12x.md` (`HANDOFF.md:1039`).

### gpu/recomp/runner.rs - GPU test bench

- Purpose: run translated programs on the real GPU. `KernelState` (initial registers, EXEC all
  ones by default, `crates/xcl/src/gpu/recomp/runner.rs:22`) in, `KernelResult` out
  (`crates/xcl/src/gpu/recomp/runner.rs:39`).
- Entry points by need: `run_code` (translate + DXC + run, `crates/xcl/src/gpu/recomp/runner.rs:602`),
  `run_program` (also places the code in `mem` at address 0 for `s_getpc_b64` constants,
  `crates/xcl/src/gpu/recomp/runner.rs:580`), `run_code_with` (test textures and samplers,
  `crates/xcl/src/gpu/recomp/runner.rs:592`), `run_in_arena` (UMA windows,
  `crates/xcl/src/gpu/recomp/runner.rs:1235`), `draw` / `draw_views` / `draw_gs`
  (`crates/xcl/src/gpu/recomp/runner.rs:623`, `crates/xcl/src/gpu/recomp/runner.rs:672`,
  `crates/xcl/src/gpu/recomp/runner.rs:639`), `mesh_draw` / `mesh_draw_split`
  (`crates/xcl/src/gpu/recomp/runner.rs:1052`, `crates/xcl/src/gpu/recomp/runner.rs:1068`),
  `dispatch_timed` (GPU timestamps, `crates/xcl/src/gpu/recomp/runner.rs:1363`).
- Capability and skip: `wave64_programs_supported`, `mesh_shaders_supported`, `split_for`,
  `skip_test` (`crates/xcl/src/gpu/recomp/runner.rs:369`, `crates/xcl/src/gpu/recomp/runner.rs:417`,
  `crates/xcl/src/gpu/recomp/runner.rs:387`, `crates/xcl/src/gpu/recomp/runner.rs:428`).
  `skip_test` writes `SKIPPED <test>: <reason>` to stderr; `XCL_REQUIRE_GPU` turns a skip into a
  failure (`crates/xcl/src/gpu/recomp/runner.rs:432`).
- A split wave64 kernel stores both halves' SGPRs and `split_halves_agree` fails the test if they
  differ (`crates/xcl/src/gpu/recomp/runner.rs:402`).

### gpu/recomp/mesh_dispatch.rs and mesh_wave.rs

- `mesh_wave.rs`: static analysis of what a logical wave64 mesh program needs on wave32
  hardware (mask-dependent branches, lane reads). It "only describes the required operations"
  (`crates/xcl/src/gpu/recomp/mesh_wave.rs:3`); `analyze` is called from
  `geometry_wave32_hazards` (`crates/xcl/src/gpu/recomp/mod.rs:1188`).
- `mesh_dispatch.rs`: a "foundation" for dispatching wave64 mesh programs on fixed-wave32
  hardware - a scratch layout and a rendezvous model; its header says it does not enable real
  programs (`crates/xcl/src/gpu/recomp/mesh_dispatch.rs:1`). A search of `crates` finds no use
  of it outside its own file (only the module declaration,
  `crates/xcl/src/gpu/recomp/mod.rs:56`). Treat it as a model/test fixture; the split form in
  `emit.rs` (`PRELUDE_X64`) is what actually runs.
- Tests: 13 in `mesh_wave.rs`, 5 in `mesh_dispatch.rs`.

### gpu/recomp/upper_lanes.rs and x64_masks.rs

See [Wave64 on wave32 GPUs](#wave64-on-wave32-gpus). `upper_lanes::Read`
(`crates/xcl/src/gpu/recomp/upper_lanes.rs:221`) classifies a read of lanes 32..63 as unwritten,
constant or shadow (a scalar copy kept for `v_writelane`); `UpperLanes::hazards` lists reads it
cannot follow (`crates/xcl/src/gpu/recomp/upper_lanes.rs:244`). `x64_masks::uses` models which
SGPRs each instruction reads or overwrites (`crates/xcl/src/gpu/recomp/x64_masks.rs:69`); its rules
follow what the emitter does, not only the ISA tables, so an emitter change that reads or writes
SGPRs differently must be mirrored there (`crates/xcl/src/gpu/recomp/x64_masks.rs:14`). Tests:
9 in `upper_lanes.rs`, 16 in `x64_masks.rs`.

### gpu/recomp/tests.rs - the instruction and pipeline tests

131 `#[test]` functions (`crates/xcl/src/gpu/recomp/tests.rs:1`). Structure:

- Helpers: `gpu_ready` (GPU + DXC + wave64 or split), `gpu_waves`, `mesh_ready`, `on_gpu`, `prog`,
  `run_wave` (`crates/xcl/src/gpu/recomp/tests.rs:13`, `crates/xcl/src/gpu/recomp/tests.rs:53`,
  `crates/xcl/src/gpu/recomp/tests.rs:40`, `crates/xcl/src/gpu/recomp/tests.rs:35`,
  `crates/xcl/src/gpu/recomp/tests.rs:76`, `crates/xcl/src/gpu/recomp/tests.rs:70`); `V = 256` is the
  VALU source code of `v0` (`crates/xcl/src/gpu/recomp/tests.rs:85`).
- Two test kinds per feature: a text test that checks the generated HLSL (and compiles it if DXC
  exists), and a `gpu_*` test that runs it on the GPU and compares to expected values.
- Retail-sample tests (ignored or skipped without `XCL_SAMPLE_*`): the translator survey
  `retail_translation_survey` (`crates/xcl/src/gpu/recomp/tests.rs:4764`); results in
  `HANDOFF.md:1259`.
- Performance: `bench_split_wave64` (`crates/xcl/src/gpu/recomp/tests.rs:5881`).

### crates/xcl/src/shaders.rs - `xcl shaders`

CLI subcommand `xcl shaders <file | folder | package.rpf> [--top N] [--json] [--recomp]
[--compile N]` (`crates/xcl/src/shaders.rs:1`, usage line in `crates/xcl/src/main.rs:32`,
dispatch at `crates/xcl/src/main.rs:67`). Entry `run` (`crates/xcl/src/shaders.rs:680`): formats,
stages, FSSM variants, the RDNA2 instruction histogram (priorities for the recompiler);
`--recomp` translates every program in kernel mode and reports what is still missing
(`RecompAgg`, `crates/xcl/src/shaders.rs:285`); `--compile N` compiles N complete translations
with DXC; `--precompile` (with `--limit`, `--windows`, `--split`) translates every VS / PS / MS /
CS program and fills the DXIL caches without a title (`crates/xcl/src/shaders.rs:715`,
`precompile` at `crates/xcl/src/shaders.rs:814`). `--iface` hypotheses on how programs
connect to their signatures live in `IfaceAgg` (`crates/xcl/src/shaders.rs:18`). Tests:
`crates/xcl/src/shaders.rs:1236` (synthetic container) and `crates/xcl/src/shaders.rs:1258`
(retail sample, needs `XCL_SAMPLE_RPF`).

### Integration tests that exercise this page

`crates/xcl/tests/gpu_compute_e2e.rs`, `crates/xcl/tests/gpu_pso_e2e.rs`,
`crates/xcl/tests/console_shaders.rs`, `crates/xcl/tests/console_dlls_e2e.rs`,
`crates/xcl/tests/texture_states.rs`, `crates/xcl/tests/predication_e2e.rs` reference `gpu::`
items. Layout and how to run them: `testing-and-scripts.md`.

## How to add support for a new RDNA2 instruction

Worked example: `v_swaprel_b32`, which has a handler, a text test and a GPU test.

1. **Check that it decodes.** Every instruction in the retail spec is already in
   `rdna2_spec.rs` (generated, do not touch). A console-only instruction (not in the retail spec) goes
   in the hand-written `rdna2_xbox.rs` (`crates/xcl/src/gpu/rdna2_xbox.rs:1`). Confirm with
   the disassembler: `rdna2::disassemble` / `xcl disasm`
   (`crates/xcl/src/gpu/rdna2.rs:613`) must show the mnemonic rather than an unknown word.
2. **Find the handler for its encoding.** `Em::inst` routes by encoding name
   (`crates/xcl/src/gpu/recomp/emit.rs:832`). VALU instructions go to `valu`
   (`crates/xcl/src/gpu/recomp/emit.rs:3086`); there, special families are peeled off early
   (`v_pk_`, `v_movrel`, `v_swaprel_b32` at `crates/xcl/src/gpu/recomp/emit.rs:3108`), 64-bit and
   16-bit forms go to `valu64` / `valu16`, and the plain 32-bit ones are `match` arms such as
   `"v_mul_legacy_f32" => fr(format!("mul_legacy(...)"))`
   (`crates/xcl/src/gpu/recomp/emit.rs:3242`) and `"v_med3_f32"`
   (`crates/xcl/src/gpu/recomp/emit.rs:3257`). Scalar, memory, DS, image and export
   instructions are in the handlers listed in the per-file reference.
3. **Write the arm.** Return `Ok(())` after emitting with `self.line(...)`; return
   `Err("reason")` for a case you do not handle, and `inst` records it through `unsupported`
   (`crates/xcl/src/gpu/recomp/emit.rs:595`) so it shows in `Translation::unsupported_why` and in
   `xcl shaders --recomp`. Use the existing helpers for operands and writes: `read`, `sources`,
   `vwrite` / `write32`, `act()` for the EXEC gate (`crates/xcl/src/gpu/recomp/emit.rs:686`,
   `crates/xcl/src/gpu/recomp/emit.rs:2478`, `crates/xcl/src/gpu/recomp/emit.rs:2554`,
   `crates/xcl/src/gpu/recomp/emit.rs:767`, `crates/xcl/src/gpu/recomp/emit.rs:586`). See `swaprel`
   (`crates/xcl/src/gpu/recomp/emit.rs:2770`) for a read-write instruction that captures both old
   values before writing.
4. **If the instruction has no effect in this model** (waits, prefetch, mode changes), add its
   name to `NOOPS` instead (`crates/xcl/src/gpu/recomp/emit.rs:66`).
5. **If it needs an HLSL helper**, add it to the prelude string (`PRELUDE`,
   `crates/xcl/src/gpu/recomp/emit.rs:5291`; for example `mul_legacy` at
   `crates/xcl/src/gpu/recomp/emit.rs:5371`). Keep generated HLSL branch-free: `select()`, no
   scalar `?:` / `&&` / `||`.
6. **Check the wave-size paths.** If it reads a mask or a lane, or touches SGPRs, update the
   analyses that mirror the emitter: `x64_masks::uses`
   (`crates/xcl/src/gpu/recomp/x64_masks.rs:69`) for the split form, the hazard tables in
   `geometry_wave32_hazards` / `upper_lanes` (`crates/xcl/src/gpu/recomp/mod.rs:1119`,
   `crates/xcl/src/gpu/recomp/upper_lanes.rs:283`), and `docs/wave64-on-wave32.md` if the exchange
   table changes.
7. **Add the tests** (`AGENTS.md` requires a test that fails without the change). Two layers, both
   in `crates/xcl/src/gpu/recomp/tests.rs`:
   - A text test, no GPU needed: assemble with `prog(&[("V_SWAPREL_B32", "ENC_VOP1", &[...], None)])`
     (`crates/xcl/src/gpu/recomp/tests.rs:76`, assembler `rdna2::encode` at
     `crates/xcl/src/gpu/rdna2.rs:362`), `translate`, assert `t.complete()` and that
     `t.hlsl` contains the expected statements, then compile with `dxc::compile` when
     `dxc::location().is_ok()` - `v_swaprel_b32_uses_the_m0_relative_switch`
     (`crates/xcl/src/gpu/recomp/tests.rs:294`).
   - A GPU test: start with `if !gpu_ready() { return; }` so the test skips cleanly
     (`crates/xcl/src/gpu/recomp/tests.rs:13`), build a `KernelState`, run with
     `runner::run_program` / `runner::run_code`, and assert per lane against an expected value,
     including EXEC-masked lanes - `gpu_v_swaprel_b32`
     (`crates/xcl/src/gpu/recomp/tests.rs:309`). For many 32-bit VALU ops one line in an existing
     table test is enough: `gpu_valu_float_and_int` asserts `v_mul_legacy_f32` 0 * inf = 0 next to
     others (`crates/xcl/src/gpu/recomp/tests.rs:956`); modifiers (`clamp`, `omod`, `neg`) have
     `gpu_clamp_and_omod_per_instruction` (`crates/xcl/src/gpu/recomp/tests.rs:1007`).
     Use `gpu_waves()` to run a per-lane test as wave64 and wave32
     (`crates/xcl/src/gpu/recomp/tests.rs:53`). A GPU test calls
     `runner::skip_test` through `gpu_ready` and must pass without a GPU.
8. **Run.** `cargo test -p xcl --lib recomp::tests::<name>` while iterating, then the whole
   suite (`AGENTS.md`, "Build and test"). On macOS or Linux the crate does not build: say "not
   built / not tested".
9. **Measure on real shaders** if you have a pack: `xcl shaders <pack> --recomp` lists
   instructions still missing and `XCL_SAMPLE_RPF` enables the retail survey test
   (`crates/xcl/src/gpu/recomp/tests.rs:4764`). A change to `crates/xcl/src` outside tests also
   needs the `scripts/smoke.ps1` regression run (`CLAUDE.md`).

## Known limits

Each item cites where it is stated; none was re-measured for this page.

- **Partial waves and whole-wave code.** The console runs whole-wave code in all 64 lanes; the PC
  runs it only in the launched lanes, so a min reduction (identity -1) in a partial wave can read 0.
  `docs/wave64-on-wave32.md:261` ("What is still not exact") and `HANDOFF.md:1701`.
- **Split wave64 cost.** Measured 1.3x with one console wave per group and about 5x with four in the
  worst case; `docs/wave64-on-wave32.md:138` ("Cost") and `HANDOFF.md:1669`.
- **Cube-map gradient sampling** (`image_sample_d` on cube textures) is an approximation:
  LOD from the largest gradient and face size, then `SampleLevel` (`HANDOFF.md:1259`).
- **DXC time.** The whole pack is about 6 h of CPU; the DXIL cache and background compilation
  exist for this (`HANDOFF.md:1259`, `crates/xcl/src/gpu/recomp/dxc.rs:631`).
- **Mesh output limit.** D3D12 allows 32 KB of output per mesh group, so large pixel-shader
  inputs use split mode; lines/points mesh topologies are not translated
  (`crates/xcl/src/gpu/recomp/mod.rs:457`, `HANDOFF.md:1206`). Amplification shaders are not
  supported (none in the pack, `HANDOFF.md:1206`).
- **GS** supports triangle input and triangle-strip output only
  (`crates/xcl/src/gpu/recomp/mod.rs:254`).
- **Textures.** MSAA is not handled; the console pipe configuration for `_X` modes was inferred;
  3 of 912 retail textures do not fit the model (`HANDOFF.md:1132`, `HANDOFF.md:1114`).
- **UMA.** Console descriptor heaps and descriptor-table copies stay in window 0 (32-bit console
  addresses); at most four windows (`HANDOFF.md:1065`, `crates/xcl/src/gpu/uma.rs:29`).
- **BVH.** Box16 nodes are read, not built; the instance matrix at @0 is assumed
  (`crates/xcl/src/gpu/bvh.rs:22`).
- **`mesh_dispatch.rs`** is a model only (see its section).
