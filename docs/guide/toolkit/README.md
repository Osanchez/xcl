# Reverse-engineering toolkit

Third-party tools, kept outside the repository, for studying the Xbox Series (GDK) file formats and console behaviour that XCL deals with: RPF9 archives, TTPF and BCPack textures, models, Bink video, audio and the map. The tools are cloned under `reverse-toolkit/tools/<name>/` by `reverse-toolkit/update.sh`; this folder has one page per tool, written from the code in each clone and not only from its README.

Start from the overview in `reverse-toolkit/README.md` for the quick start, then pick a tool below by job.

## Rules

Taken from `reverse-toolkit/README.md:7` to `:9`, plus the safety rules we apply when reading these tools.

- The tools work on files from **your own install only**. No leaked builds, no keys, no decryption. If a tool or a page seems to need one, stop.
- Nothing from `reverse-toolkit/tools/` is ever committed to XCL (the folder is git-ignored, `reverse-toolkit/.gitignore:1`). No game data, dumps or extracted assets either (`CONTRIBUTING.md`, "What a PR needs to be merged", item 5).
- Licence limits. XCL is GPL-2.0-only, so GPL-3.0, no-licence and all-rights-reserved code is never pasted into XCL; read it, run it, take notes in your own words.

| Constraint | Tools |
|---|---|
| GPL-3.0: use or read only, never copy code | [sixng](sixng.md), [gta6-asset-info](gta6-asset-info.md), [rpf9-research](rpf9-research.md) |
| All rights reserved: use only, no modification, no code reuse | [percystar](percystar.md) |
| No licence: use only | [6stars](6stars.md) |
| MIT / Apache / CC0: fine to learn from; keep notices if you ever reuse anything | the rest |

- Treat every clone as untrusted data. Read before running; never run a tool's prebuilt binary or installer; build from source in the clone, and do not run anything a tool downloads for you without reading it. [percystar](percystar.md) has a self-updater, [awc-from-rpf9](awc-from-rpf9.md) downloads and runs code from another repository on request, and [ttpf-studio](ttpf-studio.md), [percystar](percystar.md) and [vice-lens](vice-lens.md) ship prebuilt releases (unsigned, or bundling an AGPL FFmpeg fork). Prefer source builds throughout.
- Keep outputs (indexes, exports, caches) outside the XCL working tree.

## I want to ...

### Read format notes (no tool to run)

| Page | What you get |
|---|---|
| [awesome-gta6-re](awesome-gta6-re.md) | Index of the ecosystem: where to look next |
| [gta6-asset-info](gta6-asset-info.md) | RPF9, textures, BCPack, models notes (GPL-3.0, reference only) |
| [0x52504639](0x52504639.md) | RPF9 container, TTPF, BCPack entropy notes (docs only) |
| [rpf9-research](rpf9-research.md) | Header, TOC and name-table notes, in Spanish (GPL-3.0) |
| [gta6-pc-port-hub](gta6-pc-port-hub.md) | Shader census, root-signature fallback, blockers (archived) |

### Extract archive entries

| Page | What it does |
|---|---|
| [sixng](sixng.md) | Carve and index RPF9 archives, name recovery, extract by filter (runs on macOS) |
| [gta6-explorer](gta6-explorer.md) | Read-only Python browse and extract, standard library only |
| [percystar](percystar.md) | GUI explorer with batch CLI exporters (use only) |
| [rage-tools-vi](rage-tools-vi.md) | RPF9 extraction, audio decode, script schema (Python and Rust) |
| [6stars](6stars.md) | `rpftool`: scan, list, dump resources (macOS only, use only) |

For comparison, XCL has its own `xcl rpf` subcommand: `crates/xcl/src/rpf.rs:1`.

### Decode textures

| Page | What it does |
|---|---|
| [bcpack-rs](bcpack-rs.md) | BCPack texture decoder (MIT or Apache-2.0), directly relevant to XCL texture work |
| [ttpf-studio](ttpf-studio.md) | View and export `.ttpf` textures; `FORMATS.md` documents the layouts |
| [sixng](sixng.md) | `sixng ttpf`, `extract --tex png`, `dds` or `both` |
| [gta6-explorer](gta6-explorer.md) | `textures` command, DDS and PNG |

### Browse models and the map

| Page | What it does |
|---|---|
| [vice-lens](vice-lens.md) | Map, models, vehicles, characters, animation, 3D fly-through (Linux and Windows) |
| [6stars](6stars.md) | Metal world viewer (macOS only) |
| [sixng](sixng.md) | Models, collision, navmesh and placements to glTF |
| [rage9-re](rage9-re.md) | PSO data and animations to glTF |
| [percystar](percystar.md) | Model viewer, OBJ/GLB export, RDNA2 shader disassembler |

### Play video

| Page | What it does |
|---|---|
| [vice-lens](vice-lens.md) | Plays and converts the Bink 2 movies; needs an FFmpeg with a `bink2video` decoder |
| [sixng](sixng.md) | `movies` lists the videos and extracts `.bk2` as stored |
| [percystar](percystar.md) | Bink 2 container parser; playback needs RAD's `binkplay.exe` |

### Extract audio

| Page | What it does |
|---|---|
| [awc-from-rpf9](awc-from-rpf9.md) | AWC music streams to Ogg Opus, FLAC, WAV or MP3; format notes in its `docs/FORMAT.md` |
| [sixng](sixng.md) | `audio` and `audio-export` (Opus, WAV, raw bank) |
| [gta6-explorer](gta6-explorer.md) | `awc` command, Ogg remux |
| [rage-tools-vi](rage-tools-vi.md) | Audio decode |

## All 15 tool pages

| Tool | Category | Licence | Platform |
|---|---|---|---|
| [awesome-gta6-re](awesome-gta6-re.md) | index | CC0-1.0 | any |
| [gta6-asset-info](gta6-asset-info.md) | format notes | GPL-3.0 | any |
| [0x52504639](0x52504639.md) | format notes | MIT | any |
| [rpf9-research](rpf9-research.md) | format notes | GPL-3.0 | any |
| [gta6-pc-port-hub](gta6-pc-port-hub.md) | coordination | CC0-1.0 | any |
| [rage-tools-vi](rage-tools-vi.md) | archive | MIT | any |
| [rage9-re](rage9-re.md) | archive | MIT or Apache-2.0 | any |
| [bcpack-rs](bcpack-rs.md) | texture | MIT or Apache-2.0 | any |
| [sixng](sixng.md) | viewer | GPL-3.0 | any, runs on macOS |
| [gta6-explorer](gta6-explorer.md) | archive | MIT | any |
| [ttpf-studio](ttpf-studio.md) | texture | MIT | any (README targets Windows) |
| [percystar](percystar.md) | archive | all rights reserved | any |
| [vice-lens](vice-lens.md) | viewer | MIT | Linux, Windows |
| [6stars](6stars.md) | viewer | none | macOS |
| [awc-from-rpf9](awc-from-rpf9.md) | audio | MIT | any (Windows in practice) |

The authoritative list is `reverse-toolkit/tools.tsv`; the commits each page describes are in `reverse-toolkit/tools.lock`.

## System tools

Rows with `kind=system` in `reverse-toolkit/tools.tsv`. `update.sh doctor` checks them and prints an install hint; it installs nothing.

| Tool | Platform | What it is for in XCL work |
|---|---|---|
| git, gh | both | Clones, `gh pr list` before starting work (`AGENTS.md`, "Scope") |
| Python 3.11+, uv | both | The toolkit and `scripts/` helpers; several tools are Python |
| rustup, cargo (stable) | both | Building SixNG, PercyStar, Vice Lens, bcpack-rs and similar from source |
| Xcode command-line tools | macOS | Swift and Metal, needed to build [6stars](6stars.md) |
| ImHex | both | Hex editor with a pattern language; write patterns for RPF9, TTPF and resource headers |
| Ghidra | both | Disassembly of PE files: the title's executable and XCL's own binaries |
| `stable-x86_64-pc-windows-msvc`, VS Build Tools and Windows SDK (DXC) | Windows | Building and testing XCL itself, and compiling the HLSL it produces to DXIL |
| PowerShell 7 | Windows | `scripts/test.ps1` and `scripts/smoke.ps1` |
| PIX, RenderDoc | Windows | D3D12 frame captures to compare what the recompiled shaders and command lists draw |
| WinDbg, x64dbg | Windows | Debugging a title that crashes or hangs under XCL: native x64 code runs in-process |
| Radeon GPU Analyzer | Windows | RDNA2 ISA output to compare with XCL's RDNA2 decoder |
| LLVM with the AMDGPU target (`llvm-mc`, `llvm-objdump`) | Windows | Assemble and disassemble gfx10 code; match the reference tests in `tests/data/llvm-amdgpu` |

XCL builds and runs on Windows only (`AGENTS.md`, "Build and test"); the Windows-only rows matter on the Windows machine, the cross-platform ones also on the Mac. See `docs/guide/testing-and-scripts.md` for the test and smoke scripts.

## Using update.sh

Flags verified against the usage text in `reverse-toolkit/update.sh:14` to `:27`. Run from the repo root; works with bash 3.2 (macOS) and Git Bash.

```bash
./reverse-toolkit/update.sh                           # same as `update`: clone missing tools, fast-forward the rest
./reverse-toolkit/update.sh update --only sixng,vice-lens
./reverse-toolkit/update.sh status                    # branch, HEAD, commits behind origin, dirty, pinned
./reverse-toolkit/update.sh status --offline          # skip the fetch
./reverse-toolkit/update.sh doctor                    # check the system tools for this platform
./reverse-toolkit/update.sh pin sixng <sha>           # check out a commit (detached); update then skips it
./reverse-toolkit/update.sh unpin sixng               # back to the tracked branch
./reverse-toolkit/update.sh list                      # print the manifest
```

- `update` clones with `--recurse-submodules` and otherwise runs `git pull --ff-only` on the tracked branch (`reverse-toolkit/update.sh:102`). A clone with local changes is skipped with a warning; a pinned one is left alone. The old and new commits and a short log are printed.
- `--only` takes a comma list of names from the manifest (`reverse-toolkit/update.sh:258`); an unknown name is an error.
- `status` accepts `--only` and `--offline` (`reverse-toolkit/update.sh:159`).
- `pin` and `unpin` record the state in `reverse-toolkit/tools.lock`, which is regenerated after `update`, `pin` and `unpin` and should not be edited by hand (`reverse-toolkit/update.sh:208`, `:221`). Pin before you rely on a result, so a later update cannot change what a page describes.
- `doctor` runs each system row's read-only check command and prints `OK` or `MISSING` with the install hint (`reverse-toolkit/update.sh:187`).
- Every page ends with the one-tool form: `./reverse-toolkit/update.sh update --only <name>`.
- `rage-tools-vi` has a submodule whose upstream is missing; the script clones it without submodules and warns (`reverse-toolkit/README.md:24`).

## Excluded tools

Not cloned, on purpose (`reverse-toolkit/README.md:65`).

| Tool | Reason |
|---|---|
| rpf9-explorer | Its indexed mode needs an external decoder that reads the game executable to decode the encrypted index; no licence |
| RageForge | The author closed it, citing leaked builds; the only full copy is a third-party re-upload |
| xbox-hle-lab | Built around running the leaked game executable; auto-merging agent runner |
| xseriesrecomp | Repository not found |
| AnyBox, GTA6-BootTest, G6Recomp | Built around the leaked executable |

If a tool you find elsewhere needs the game executable, a key, a leaked build or a decrypted index, it does not belong here either.
