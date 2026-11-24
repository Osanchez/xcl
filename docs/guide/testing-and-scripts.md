# Testing and scripts

How XCL is tested, which tests need what, and what every file in `scripts/` does. The rules for
a PR are in `CONTRIBUTING.md`; the short agent version is in `AGENTS.md`. This page is the map.

The crate builds and runs on Windows x64 only (`AGENTS.md:27`). On macOS or Linux nothing in
`crates/xcl` builds: say "not built / not tested" (`AGENTS.md:27`). Only the Python scripts can
run there (see [Python scripts on macOS](#python-scripts-on-macos)).

## Test layout

| Kind | Where | Notes |
|---|---|---|
| Unit tests | `#[cfg(test)] mod tests` in the file under test (`AGENTS.md:75`) | The biggest module is `crates/xcl/src/gpu/recomp/tests.rs` (7189 lines, 131 `#[test]`), pulled in at `crates/xcl/src/gpu/recomp/mod.rs:62-63`. |
| Integration tests | `crates/xcl/tests/*.rs` (`AGENTS.md:76`) | Table below. |
| Our own console title | `tests/testtitle`, `tests/testgamedll` | Driven by `crates/xcl/tests/e2e.rs`. |
| Test data | `tests/data/` | `testgame/`, `llvm-amdgpu/`. |

Workspace members: `crates/xcl`, `tests/testtitle`, `tests/testgamedll` (`Cargo.toml:2`).

Other unit-test modules that read game data (see the `XCL_SAMPLE_*` table): `crates/xcl/src/shaders.rs:1257`,
`crates/xcl/src/gpu/texture.rs:667`, `crates/xcl/src/hle/opus.rs:1618`.

### Integration tests (`crates/xcl/tests/`)

GPU = needs a D3D12 adapter, otherwise it prints `SKIPPED`. DXC = needs the Windows SDK compiler.
Sample = needs an `XCL_SAMPLE_*` variable, otherwise it passes without doing anything.

| File | What it tests | Needs | Representative test |
|---|---|---|---|
| `crates/xcl/tests/e2e.rs` | Runs the fake title through `xcl run`; the game folder stays byte-for-byte unchanged, writes land in the overlay, the report lists the stubs hit (`crates/xcl/tests/e2e.rs:1-3`). Also launcher EXE selection and the title's draws under the D3D12 debug layer. | Title built first (`cargo build --workspace`, `crates/xcl/tests/e2e.rs:5`). The debug-layer test skips without `d3d12SDKLayers.dll` / `xgameruntime.dll` (`crates/xcl/tests/e2e.rs:249-257`). | `crates/xcl/tests/e2e.rs:88` `test_title_runs_and_game_folder_is_untouched` |
| `crates/xcl/tests/console_dlls_e2e.rs` | Real console DLLs from a game package go through XCL's loader (map, imports, TLS, DllMain) and are then called: CRT DLLs, libHttpClient, Bink 2, the BVH builder (`crates/xcl/tests/console_dlls_e2e.rs:1-3`). | Sample: `XCL_SAMPLE_DLLS` (all), `XCL_SAMPLE_BK2`, `XCL_SAMPLE_BVH4` | `crates/xcl/tests/console_dlls_e2e.rs:61` `console_crt_dlls_load_and_run` |
| `crates/xcl/tests/console_shaders.rs` | The console's own shader compiler as a test generator: its output goes through RDNA2 to HLSL and runs on the PC GPU next to the PC-compiled HLSL; both must write the same memory (`crates/xcl/tests/console_shaders.rs:1-4`). | Sample: `XCL_SAMPLE_DXC`; GPU; DXC | `crates/xcl/tests/console_shaders.rs:319` `console_compiled_kernels_match_the_pc_compile` |
| `crates/xcl/tests/gpu_compute_e2e.rs` | A synthetic wave64 console compute shader through the `CreateComputePipelineState` hook, dispatched and read back; pipeline counters are checked so the fallback path cannot pass (`crates/xcl/tests/gpu_compute_e2e.rs:1-8`). | GPU; DXC; wave64 (`crates/xcl/tests/gpu_compute_e2e.rs:120-132`) | `crates/xcl/tests/gpu_compute_e2e.rs:116` |
| `crates/xcl/tests/gpu_pso_e2e.rs` | Real retail shaders through the `CreateGraphicsPipelineState` hook, drawn into a texture and compared with values computed in the test (`crates/xcl/tests/gpu_pso_e2e.rs:1-10`). | Sample: `XCL_SAMPLE_RPF`; GPU; DXC | `crates/xcl/tests/gpu_pso_e2e.rs:193` `retail_vs_ps_through_the_pso_hook_render_and_read_back`; `crates/xcl/tests/gpu_pso_e2e.rs:34` checks that an unreadable sample path fails instead of skipping |
| `crates/xcl/tests/predication_e2e.rs` | `SetPredicationBufferX` with the D3D12 debug layer: predicated copies, `KickoffX` mid-recording, `ClearState` (`crates/xcl/tests/predication_e2e.rs:1-11`). | GPU with UMA; skips "no GPU" / "no UMA" / "no GPU UMA" (`crates/xcl/tests/predication_e2e.rs:169-179`) | `crates/xcl/tests/predication_e2e.rs:163` |
| `crates/xcl/tests/texture_states.rs` | `gpu::texture::upload_batch` barriers start from each subresource's own state, checked by the debug layer; own process because the layer must be on before the first device (`crates/xcl/tests/texture_states.rs:1-4`). | GPU + debug layer; prints `SKIPPED` itself (`crates/xcl/tests/texture_states.rs:54-63`) | `crates/xcl/tests/texture_states.rs:49` |
| `crates/xcl/tests/xg_oracle.rs` | The console's `xg_xs.dll` texture-layout library loaded through XCL's loader as a reference for XCL's own `xg` implementation (`crates/xcl/tests/xg_oracle.rs:1-3`). | Sample: `XCL_SAMPLE_XG` | `crates/xcl/tests/xg_oracle.rs:108` `layouts_match_the_console_library` |
| `crates/xcl/tests/heap_probe.rs` | Probe of what D3D12 accepts as a heap over partly committed memory; all four tests are `#[ignore]`, run on purpose: `cargo test -p xcl --test heap_probe -- --ignored --nocapture` (`crates/xcl/tests/heap_probe.rs:1-4`). | GPU | `crates/xcl/tests/heap_probe.rs:51` |
| `crates/xcl/tests/pls_quota.rs` | Native A/W file-API hooks (`GetDiskFreeSpaceEx`, OEM/ANSI mode) without loading a title or graphics (`crates/xcl/tests/pls_quota.rs:1`). | Windows only | `crates/xcl/tests/pls_quota.rs:24` |
| `crates/xcl/tests/rpf_e2e.rs` | `xcl` CLI RPF probe on a synthetic archive: full scan and streaming probe agree (`crates/xcl/tests/rpf_e2e.rs:5`). | Nothing | `crates/xcl/tests/rpf_e2e.rs:5` |
| `crates/xcl/tests/disasm_e2e.rs` | `xcl disasm` honours byte-range options on a synthetic shader (`crates/xcl/tests/disasm_e2e.rs:5`). | Nothing | `crates/xcl/tests/disasm_e2e.rs:5` |

The `#[ignore]`d tests are `crates/xcl/tests/heap_probe.rs:50,155,224,283`, `crates/xcl/tests/console_shaders.rs:367,406,447,484`
and `crates/xcl/tests/xg_oracle.rs:64`; `crates/xcl/src/gpu/recomp/tests.rs:7157-7160` has one more
(`fspx_containers_translate_and_compile`, needs `XCL_SAMPLE_FSPX`). `AGENTS.md:91` forbids adding
`#[ignore]` to make a test pass.

### The test title: `tests/testtitle` and `tests/testgamedll`

- `tests/testtitle` builds `xcl_testtitle.exe`, a fake console executable: its import table names
  console DLLs (`kernelx.dll`, `xgameruntime.dll`, `d3d12_xs.dll`) and plain Windows cannot start it
  (`tests/testtitle/Cargo.toml:3`, `tests/testtitle/src/main.rs:1-5`). The imports are declared with
  `#[link(name = "kernelx", kind = "raw-dylib")]` (`tests/testtitle/src/main.rs:38`). Parts: `main.rs`
  (the checks, `fn main` at `tests/testtitle/src/main.rs:216`), `gpu.rs` (D3D12.X checks) and `audio.rs` (XAudio2, XMA2, Opus)
  (`tests/testtitle/src/main.rs:146-147`, `tests/testtitle/src/audio.rs:1-4`).
- Every check prints `[PASS]` or `[FAIL]`; the exit code is the number of failures
  (`tests/testtitle/src/main.rs:5`), and the last line is `xcl_testtitle: done, failures: N`
  (`tests/testtitle/src/main.rs:702`).
- `tests/testgamedll` builds `testgame_mw.dll`, a fake middleware DLL "shipped with the game", loaded by
  XCL's own loader, with a `kernelx.dll` import, static TLS and a `DllMain`
  (`tests/testgamedll/src/lib.rs:1-2`, `tests/testgamedll/Cargo.toml:6`). `test = false` because the
  test harness exe would keep the `kernelx.dll` import and not start (`tests/testgamedll/Cargo.toml:15-17`).
- How e2e uses them: it looks for both next to the `xcl` binary, so `cargo build --workspace` has to run
  first (`crates/xcl/tests/e2e.rs:89-99`). It copies `tests/data/testgame` into `target/tmp/e2e/game`, adds
  the two files, snapshots the folder, runs `xcl run <game> -- --hello` with `XCL_DATA_ROOT` and
  `XCL_DUMP_SHADERS=1`, then asserts no `[FAIL]` line, the `done, failures: 0` line and a successful exit
  (`crates/xcl/tests/e2e.rs:100-135`).
- `scripts/test.ps1` builds the workspace first for this reason (`scripts/test.ps1:1,11`).
- CI skips this one test: the runner has no `xgameruntime.dll` and only the Basic Render Driver
  (`.github/workflows/ci.yml:7-10`, `.github/workflows/ci.yml:54`).

### Test data (`tests/data/`)

- `tests/data/testgame/`: `MicrosoftGame.config` (fake GDK manifest, `MicrosoftGame.config:2`) and
  `data/{hello.txt,delete_me.txt,sub/nested.bin}`; the e2e game folder is copied from it
  (`crates/xcl/tests/e2e.rs:104`). `scripts/pr_check.py` lets this one `MicrosoftGame.config` through
  (`scripts/pr_check.py:69`).
- `tests/data/llvm-amdgpu/`: 22 text files of LLVM's gfx10 disassembler tests (`LICENSE.TXT` next to
  them). Used by `llvm_gfx10_corpus` (`crates/xcl/src/gpu/rdna2.rs:787-788`: every instruction decoded,
  mnemonic and length compared with LLVM) and `buffer_format_table_matches_llvm`
  (`crates/xcl/src/gpu/recomp/tests.rs:1455`). `HANDOFF.md:42` records the result.

## GPU tests and `SKIPPED`

- `gpu::recomp::runner::skip_test(reason)` (`crates/xcl/src/gpu/recomp/runner.rs:428`) writes
  `SKIPPED <test>: <reason>` to stderr, so it shows without `--nocapture`
  (`crates/xcl/src/gpu/recomp/runner.rs:423-427,441`). Reasons in use: "no GPU", "no DXC", "no wave64", "no mesh shaders"
  (`crates/xcl/src/gpu/recomp/runner.rs:423-424`), and for the predication test also "no UMA" / "no GPU UMA"
  (`crates/xcl/tests/predication_e2e.rs:174-179`).
- A skipped test still passes. `AGENTS.md:33` says to report skips and not count them as coverage.
- `XCL_REQUIRE_GPU=wave64`, `mesh` or `wave64,mesh` turns a skip that leaves that coverage out into a
  failure (`crates/xcl/src/gpu/recomp/runner.rs:426-440`).
- `gpu_ready()` in the recompiler tests picks the reason: no adapter or no wave64 programs gives "no GPU",
  missing DXC gives "no DXC" (`crates/xcl/src/gpu/recomp/tests.rs:13-21`).
- `XCL_ADAPTER` picks another adapter than the first high-performance one (`README.md:155`,
  `crates/xcl/src/config.rs:47`).
- `scripts/test.ps1` counts the lines and prints `GPU tests skipped on this adapter: N (reason), ...`
  (`scripts/test.ps1:13-25`).

## `XCL_SAMPLE_*` variables

Tests on game data read files you own; without the variable they print "not set" (or return silently)
and pass. Never commit that data (`AGENTS.md:96`).

| Variable | Tests | Points at |
|---|---|---|
| `XCL_SAMPLE_RPF` | `crates/xcl/tests/gpu_pso_e2e.rs:2,50,102`; unit tests in `crates/xcl/src/gpu/recomp/tests.rs:1351` (`gpu_retail_vs_blit_gen_amb_cube`), `:2892` (`retail_ps_gallery`), `:3409`, `:3846`, `:4764`, `:4988`, `:5176`, `:5296`; `crates/xcl/src/shaders.rs:1258` | The retail shader pack `shaders.rpf` (RPF9, `crates/xcl/src/shaders.rs:1256`; `crates/xcl/src/gpu/recomp/tests.rs:1348`) |
| `XCL_SAMPLE_DLLS` | `crates/xcl/tests/console_dlls_e2e.rs:14` | A folder with the game's console DLLs (`crates/xcl/tests/console_dlls_e2e.rs:1`) |
| `XCL_SAMPLE_BK2` | `crates/xcl/tests/console_dlls_e2e.rs:206` | An extracted `.bk2` movie (`crates/xcl/tests/console_dlls_e2e.rs:200`; also needs `XCL_SAMPLE_DLLS`) |
| `XCL_SAMPLE_BVH4` | `crates/xcl/tests/console_dlls_e2e.rs:279` | The package `core_bvh4.rpf` (`crates/xcl/tests/console_dlls_e2e.rs:270`; also needs `XCL_SAMPLE_DLLS`) |
| `XCL_SAMPLE_DXC` | `crates/xcl/tests/console_shaders.rs:20` | The console's `dxcompiler_xs.dll`, with `xbsc_xs.dll` next to it (`crates/xcl/tests/console_shaders.rs:7-8`) |
| `XCL_SAMPLE_XG` | `crates/xcl/tests/xg_oracle.rs:14` | The console's `xg_xs.dll` (`crates/xcl/tests/xg_oracle.rs:1-2`) |
| `XCL_SAMPLE_TTPF` | `crates/xcl/src/gpu/texture.rs:667` (`retail_ttpf_textures_detile_with_their_mip_chains`) | A retail TTPF texture archive (`crates/xcl/src/gpu/texture.rs:662`) |
| `XCL_SAMPLE_AWC` | `crates/xcl/src/hle/opus.rs:1618` (`decodes_a_game_sound`) | An AWC with one Opus stream (`crates/xcl/src/hle/opus.rs:1614`) |
| `XCL_SAMPLE_FSPX` | `crates/xcl/src/gpu/recomp/tests.rs:7157` (`#[ignore]`) | A folder of `*.bin` FSPX compute containers, e.g. the FSR4 pipelines XCL dumps (`crates/xcl/src/gpu/recomp/tests.rs:7157-7158`) |

`scripts/test.ps1` lists the first eight of these when unset, not `XCL_SAMPLE_FSPX`
(`scripts/test.ps1:6-7`); `README.md:158-160` names six of them.
Related knobs read by these tests: `XCL_TEST_MEM_GB` (memory cap, default 8,
`crates/xcl/tests/console_dlls_e2e.rs:15`), `XCL_BINK_DUMP` (`crates/xcl/tests/console_dlls_e2e.rs:202`), `XCL_OPUS_WAV` (`crates/xcl/src/hle/opus.rs:1616`),
`XCL_SURVEY_EVERY`, `XCL_NAMES`, `XCL_HLSL_OUT`, `XCL_CENSUS_OUT` (`crates/xcl/src/gpu/recomp/tests.rs:4757-4760,4984-4988,5170-5172`).

## How to run

```powershell
.\scripts\test.ps1                                         # build + all tests (AGENTS.md:19)
cargo build --workspace && cargo test --workspace -- --test-threads=4   # same, from bash (AGENTS.md:20)
cargo test -p xcl --lib <filter>                           # one area while iterating (AGENTS.md:21)
```

- `scripts/test.ps1` stops on a build error, then runs `cargo test --workspace -- --test-threads=4 @args`
  (`scripts/test.ps1:11-16`); extra arguments go to the test binaries, e.g.
  `.\scripts\test.ps1 --skip <name>` (`scripts/test.ps1:2`). CI uses exactly this with
  `--skip test_title_runs_and_game_folder_is_untouched` (`.github/workflows/ci.yml:54`).
- One integration file: `cargo test -p xcl --test e2e` (the file name without `.rs`; `crates/xcl/tests/heap_probe.rs:4` shows
  the form with `--test`).
- `--test-threads=4` is the setting in every documented command (`AGENTS.md:20`, `CONTRIBUTING.md`
  "Tests" section).
- Report the exact command and the passed / failed / ignored / skipped counts (`AGENTS.md:79-80`).
- Slow steps (many DXC compiles, whole-pack runs): start on a small sample (`AGENTS.md:30-31`); the survey
  tests take `XCL_SURVEY_EVERY=N` (`crates/xcl/src/gpu/recomp/tests.rs:4757-4759`).

## `scripts/`

### `scripts/smoke.ps1`

Regression run of a title for a PR: runs it for a fixed time, reads XCL's log and prints an `xcl-smoke`
block (`scripts/smoke.ps1:1-3`).

```powershell
.\scripts\smoke.ps1 -Game 'D:\Games\SomeTitle' [-Seconds 60] [-Xcl .\target\release\xcl.exe]
```

- Parameters: `-Game` (mandatory), `-Seconds` (default 60), `-Xcl` (default `target\release\xcl.exe`, so
  build with `cargo build --release -p xcl`) (`scripts/smoke.ps1:8-11,15`).
- It runs `xcl run "<Game>"` with `XCL_NETWORK=0`, `XCL_RUN_SECONDS=<Seconds>` and `XCL_ACTIVITY=1`
  (`scripts/smoke.ps1:25,34-36`), kills it after `Seconds + 120` s (`scripts/smoke.ps1:40-43`), and reads the `Log:` path
  XCL prints (`scripts/smoke.ps1:49-52`).
- Measured from the log: frames shown (`scripts/smoke.ps1:54`), presents per 2 s, last 3 (`scripts/smoke.ps1:55-56`), peak
  committed MB (`scripts/smoke.ps1:57-58`), `device removed` count (`scripts/smoke.ps1:59`), crashes
  (`unhandled exception|process ends with status`, `scripts/smoke.ps1:60`), the exit code or "killed (no exit)"
  (`scripts/smoke.ps1:69`), the GPU XCL ran on from `report.json` (`scripts/smoke.ps1:61-68`) and the current commit with an
  "(uncommitted changes)" flag from `crates`, `Cargo.toml`, `Cargo.lock` (`scripts/smoke.ps1:17-18,74`).
- Output (`scripts/smoke.ps1:71-84`), to paste unedited under **Regression run**:

  ```
  xcl-smoke v1
  commit / gpu / title / seconds / exit / frames shown /
  presents per 2 s (last 3) / device removals / crashes / peak commit
  ```

- Needs a Windows title and GPU, so it cannot run on macOS. `CLAUDE.md` requires pasting both blocks, from
  `main` and from the PR head, for a runtime change.

### `scripts/pr_check.py`

- Checks a PR description and file list against `CONTRIBUTING.md` "What a PR needs to be merged"
  (`scripts/pr_check.py:1-3`):
  - **Tested**, **Not tested**, **AI use** sections present and non-empty (`scripts/pr_check.py:12,55-57`);
  - no small / open-weight model named in **AI use** (`scripts/pr_check.py:13-14,59-63`);
  - no game data or binaries: extension and filename lists, binaries only allowed under `tests/data/` and
    up to 1 MiB (`scripts/pr_check.py:16-22,65-74`);
  - for a change under `crates/xcl/src/` outside tests (`scripts/pr_check.py:45-46,76`): an `xcl-smoke` block of the PR's
    head commit with no uncommitted changes, and one from a commit of `main` (`scripts/pr_check.py:77-87`); no
    device removals or crashes in the PR's run, not killed (`scripts/pr_check.py:90-94`); same title and seconds as
    `main`'s (`scripts/pr_check.py:96-99`); frames at least 80 % of `main`'s and peak commit at most 125 %
    (`scripts/pr_check.py:23-24,100-105`).
- Run locally (stdlib only, `scripts/pr_check.py:10`):

  ```
  python scripts/pr_check.py --body body.md --head <sha> --files files.tsv [--local]
  ```

  `files.tsv` has one line per file: path, status, binary (true/false), blob sha (`scripts/pr_check.py:5-8`).
  `--local` asks git instead of the GitHub API (`scripts/pr_check.py:118-123,132-134`; it compares with
  `origin/main`). Exit code 1 with the list of problems, or `description ok` (`scripts/pr_check.py:139-144`).
  `--help` prints the usage and nothing else (argparse, `scripts/pr_check.py:110-115`).
- The workflow feeds it the PR body and file list from the GitHub API
  (`.github/workflows/pr-check.yml:37-40`).

### `scripts/test-native-stderr.tests.ps1`

A test of `scripts/test.ps1` itself: it puts a fake `cargo.cmd` that writes to native stderr first on `PATH`,
runs `test.ps1` with output redirected, and expects exit code 0, the build output and
`GPU tests skipped on this adapter: 1 (synthetic reason)` (`scripts/test-native-stderr.tests.ps1:10-31`). It
guards the Windows PowerShell 5.1 problem noted at `scripts/test.ps1:8`. Run it with `powershell.exe` or
`pwsh.exe` (`scripts/test-native-stderr.tests.ps1:1`). No workflow calls it (checked in `.github/workflows/`).

### Generators

Generated files carry a "do not edit" header; change the generator and rerun it (`AGENTS.md:107-108`).
All use the standard library only.

| Script | Input | Output | Run |
|---|---|---|---|
| `scripts/gen_rdna2.py` | `amdgpu_isa_rdna2.xml`, AMD's machine-readable ISA spec from gpuopen.com (`scripts/gen_rdna2.py:3`) | `crates/xcl/src/gpu/rdna2_spec.rs` (`scripts/gen_rdna2.py:1`) | `python -I scripts/gen_rdna2.py <amdgpu_isa_rdna2.xml> crates/xcl/src/gpu/rdna2_spec.rs` (`scripts/gen_rdna2.py:4`) |
| `scripts/gen_tiling.py` | `gfx10SwizzlePattern.h` from Mesa's AMD addrlib (URL in `scripts/gen_tiling.py:6`) | `crates/xcl/src/gpu/tiling_tables.rs` (`scripts/gen_tiling.py:1,10`) | `python scripts/gen_tiling.py <gfx10SwizzlePattern.h>` (`scripts/gen_tiling.py:5`); the output path is fixed |
| `scripts/gen_gdk_table.py` | A text table of lines `NAME(function, "class guid", "interface guid", slot)` extracted from the PC GDK's `xgameruntime.lib` (`scripts/gen_gdk_table.py:1-4`) | `crates/xcl/src/hle/xgameruntime/gdk_table.rs` (`scripts/gen_gdk_table.py:14`) | `python scripts/gen_gdk_table.py <table file>` (`scripts/gen_gdk_table.py:6`); the output path is fixed |
| `scripts/gen_d3d12_layout.py` | `d3d12.h` from microsoft/DirectX-Headers (`scripts/gen_d3d12_layout.py:1`) | `crates/xcl/src/hle/d3d12_layout.rs`: per `ID3D12*` interface its IID, parent and vtable method order (`scripts/gen_d3d12_layout.py:5-8`) | `python -I scripts/gen_d3d12_layout.py <d3d12.h> crates/xcl/src/hle/d3d12_layout.rs` (`scripts/gen_d3d12_layout.py:3`) |

None of them has a `--help`: they read `sys.argv` directly (`scripts/gen_rdna2.py:138`, `scripts/gen_tiling.py:9`,
`scripts/gen_gdk_table.py:9`, `scripts/gen_d3d12_layout.py:52`), so a missing argument is an `IndexError`. These were not run
while writing this page. The generated-file list in `CONTRIBUTING.md` says the PR must say how the file was
generated.

### `scripts/re/` (reverse-engineering helpers)

Standard library only; none is needed for a build or a test.

| Script | Purpose | Usage |
|---|---|---|
| `scripts/re/bvhb_shaders.py` | Extracts the DXBC containers embedded in `XboxBvhBuilder.dll` and prints each one's root signature (`scripts/re/bvhb_shaders.py:1`) | `python bvhb_shaders.py <XboxBvhBuilder.dll> [out dir, default ./bvhb_shaders]` (`scripts/re/bvhb_shaders.py:4,8`) |
| `scripts/re/check_layout.py` | Checks the root-argument table hypothesis on all builder shaders (`scripts/re/check_layout.py:1`); runs `target/debug/xcl.exe` (`scripts/re/check_layout.py:6`), so it needs a Windows build | `python check_layout.py <dir with the .dxbc files of bvhb_shaders.py>` (`scripts/re/check_layout.py:6`) |
| `scripts/re/pe_exports.py` | Prints a PE's exports: ordinal, name, RVA, forwarders (`scripts/re/pe_exports.py:1`) | `python pe_exports.py <pe> [<pe> ...]` (`scripts/re/pe_exports.py:4`) |
| `scripts/re/record_writers.py` | Finds the D3D12.X inline command-list record writers in `XboxBvhBuilder.dll` from an objdump listing (`scripts/re/record_writers.py:1`) | `objdump -d --no-show-raw-insn XboxBvhBuilder.dll > bvhb.asm; python record_writers.py bvhb.asm` (`scripts/re/record_writers.py:6`) |

### Python scripts on macOS

Imports checked in the source: `scripts/gen_rdna2.py:12-13` (`sys`, `xml.etree`), `scripts/gen_tiling.py:7`
(`io`, `os`, `re`, `sys`), `scripts/gen_gdk_table.py:7` and `scripts/gen_d3d12_layout.py:11-12` (`io`, `os`, `re`, `sys`),
`scripts/pr_check.py:10` (`argparse`, `os`, `re`, `subprocess`, `sys`), `scripts/re/bvhb_shaders.py:5`,
`scripts/re/check_layout.py:7`, `scripts/re/pe_exports.py:2`, `scripts/re/record_writers.py:7`. All
standard library, so they can run under `python3` on macOS if you have the inputs. Exceptions at run time:
`check_layout.py` launches `xcl.exe` (Windows); `pr_check.py` without `--local` calls `gh api`
(`scripts/pr_check.py:122,135`), and `--local` needs a git checkout with an `origin/main` ref (`scripts/pr_check.py:134`).
The `.ps1` scripts need PowerShell and, apart from `test-native-stderr.tests.ps1`'s fake cargo, a Windows
build; do not claim they ran on macOS.

## CI (`.github/workflows/`)

- `ci.yml` (`CI`): on push to `main` and on every pull request, a `windows-2025` job without a GPU or game
  data runs `./scripts/test.ps1 --skip test_title_runs_and_game_folder_is_untouched`
  (`.github/workflows/ci.yml:13-16,28,54`). GPU tests skip, game-data tests pass without running; the
  build, the other tests and the recompiler's HLSL compile with the runner's DXC are what it checks
  (`.github/workflows/ci.yml:1-5`).
- `pr-check.yml` (`PR checks`): on `pull_request_target`, an `ubuntu-latest` job checks out only
  `scripts/pr_check.py` from the PR's base commit, so a PR cannot change its own check
  (`.github/workflows/pr-check.yml:6-8,22-28`), and runs it with the PR body and files
  (`.github/workflows/pr-check.yml:37-40`).

## What a PR needs: digest

Full list: `CONTRIBUTING.md` "What a PR needs to be merged". In short:

1. `CI` and `PR checks` green.
2. The whole suite ran on a GPU; counts and SKIPPED tests in **Tested**.
3. Runtime change (`crates/xcl/src`, not tests): `xcl-smoke` blocks from `main` and from the PR head
   (`scripts/smoke.ps1`), pasted unedited under **Regression run**.
4. No game data, no binaries outside `tests/data/` or over 1 MB.
5. Under about 400 changed lines, one logical change, no `cargo fmt` on files, no unrelated churn.
6. **Not tested** and **AI use** (tool and exact model name) filled in.
7. A bug fix comes with a test that fails without the fix (`AGENTS.md:73-74`).
