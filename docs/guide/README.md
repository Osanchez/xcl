# XCL guide

XCL runs Xbox Series X|S (GDK) game files on a Windows PC without CPU emulation: the game's x64 code
runs natively, and XCL stands in for the console libraries the PC does not have (`kernelx.dll`,
XGameRuntime, `d3d12_xs.dll`, ...), in the spirit of Wine/Proton (`README.md:6`-`README.md:10`). The
console's RDNA2 GPU code is recompiled into PC shaders (HLSL, then DXIL).

This guide is a map of the code in `crates/xcl`, written so you can find the right file before you
read source. It was read from the source; the pages say where something was not run.

## Who this is for

- A developer new to the repository who wants the whole picture before touching code.
- A developer working on one area (loader, HLE, D3D12.X, recompiler) who needs that area's file map.
- Anyone running the fork's tooling (sync, PR scanner, reverse-toolkit).

## Where things run

| Task | Windows x64 | macOS |
|---|---|---|
| Build and test the crate (`cargo build`, `scripts/test.ps1`) | yes | no, the crate does not build |
| Run a title (`xcl run`) | yes | no |
| Edit code, read these docs, review diffs | yes | yes |
| Python scripts in `scripts/` (see [testing-and-scripts.md](testing-and-scripts.md)) | yes | yes |
| `scripts/dev/*` (sync, preflight, PR scanner) | yes (Git Bash) | yes |
| Most [reverse-toolkit](toolkit/README.md) tools | yes | yes (each tool's row says `any` or a platform) |

Requirements to build and run: Windows 10/11 x64, Rust `stable-x86_64-pc-windows-msvc`, the Windows SDK
(DXC), and a D3D12 GPU with Shader Model 6.6 (`AGENTS.md:20`, `README.md:94`). On macOS, say "not built /
not tested" in a PR (`AGENTS.md:27`).

## Reading order

1. This page, then [architecture.md](architecture.md): the end-to-end picture.
2. The page for the area you will touch (table below).
3. [testing-and-scripts.md](testing-and-scripts.md) before your first change, and
   [dev-workflow.md](dev-workflow.md) for the branch and PR routine.
4. `README.md`, `AGENTS.md` and `CONTRIBUTING.md` for the rules; `HANDOFF.md` for measurements.

## Pages

| Page | What it covers |
|---|---|
| [architecture.md](architecture.md) | End-to-end picture: process model, loading, runtime services, graphics, audio, input; diagrams and a "where to start reading" table |
| [dev-workflow.md](dev-workflow.md) | Branch model, setup, daily loop, hourly automation, one-time GitHub setup, toolkit updates |
| [runtime-core.md](runtime-core.md) | Launcher, host, PE loader, resolver, stubs, TLS, exceptions, VFS, report, log |
| [cli-and-config.md](cli-and-config.md) | The `xcl` command line, `xcl.toml`, files a run writes, every `XCL_*` variable |
| [hle.md](hle.md) | HLE modules: kernelx, xgameruntime, xaudio2, xma2, opus, dstorage, gameinput and the rest |
| [d3d12x.md](d3d12x.md) | D3D12.X on a PC D3D12 device: objects, command lists, PSOs, memory, present, frame clock |
| [gpu-recompiler.md](gpu-recompiler.md) | RDNA2 decode, HLSL emission, DXC, textures, UMA windows, BVH, how to add an instruction |
| [testing-and-scripts.md](testing-and-scripts.md) | Test layout, what each test needs, every file in `scripts/` |
| [fork-workflow-for-agents.md](fork-workflow-for-agents.md) | Short rules for coding agents working in the fork |
| [telemetry.md](telemetry.md) | The fork's telemetry: the run folder, the manifest, every event, the tiers and switches, how to read a run |
| [toolkit/README.md](toolkit/README.md) | Index of the third-party reverse-engineering tools, with one notes page per tool |

## Other documents

- [`README.md`](../../README.md): what works, games, controls, build and usage.
- [`HANDOFF.md`](../../HANDOFF.md): status, measurements, findings. It is long: search it by section name
  (the guide pages cite line numbers) instead of reading it all.
- [`docs/specs/2026-11-20-xcl-design.md`](../specs/2026-11-20-xcl-design.md): the design.
- [`docs/wave64-on-wave32.md`](../wave64-on-wave32.md): running wave64 shaders on 32-lane GPUs.
- [`AGENTS.md`](../../AGENTS.md) and [`CONTRIBUTING.md`](../../CONTRIBUTING.md): the rules for every
  contributor, human or agent.
