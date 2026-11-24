# reverse-toolkit

Third-party tools for studying the Xbox Series (GDK) file formats and console behaviour that XCL deals with (RPF9 archives, BCPack and TTPF textures, models, audio, shaders). `update.sh` clones and updates them under `tools/` and checks the system tools you need. Per-tool notes are in [`../docs/guide/toolkit/`](../docs/guide/toolkit/).

## Rules

- The tools operate on files from **your own install only**. No leaked builds, no keys, no decryption.
- Nothing from `tools/` is ever committed to XCL (`tools/` is git-ignored).
- GPL-3, LGPL, no-licence and all-rights-reserved code must not be copied into XCL, which is GPL-2.0-only. Use these tools, read their notes, do not paste their code.

## Quick start

```bash
./reverse-toolkit/update.sh              # clone missing tools, fast-forward the rest
./reverse-toolkit/update.sh status       # branch, HEAD, commits behind, dirty, pinned (--offline skips the fetch)
./reverse-toolkit/update.sh doctor       # check the system tools for this platform; installs nothing
./reverse-toolkit/update.sh pin sixng <sha>   # freeze a tool at a commit
./reverse-toolkit/update.sh unpin sixng       # back to the tracked branch
./reverse-toolkit/update.sh list         # print the manifest
```

`update` and `status` accept `--only a,b`. A clone with local changes is skipped with a warning; a pinned one is left alone. `tools.lock` records the commit of every clone (and the pin flag) after each update, for reproducibility and rollback. Works in bash 3.2 (macOS) and Git Bash.

Note: `rage-tools-vi` has a submodule (`rdna2-spirv`) whose upstream repository is not found; the script then clones it without submodules and warns.

## Git tools

| name | category | licence | platform | purpose | notes |
|---|---|---|---|---|---|
| awesome-gta6-re | index | CC0-1.0 | any | Index of the ecosystem | [notes](../docs/guide/toolkit/awesome-gta6-re.md) |
| gta6-asset-info | format-notes | GPL-3.0 | any | Format notes: RPF9, textures, BCPack, models (reference only) | [notes](../docs/guide/toolkit/gta6-asset-info.md) |
| 0x52504639 | format-notes | MIT | any | RPF9 container, TTPF, BCPack entropy notes (docs only) | [notes](../docs/guide/toolkit/0x52504639.md) |
| rpf9-research | format-notes | GPL-3.0 | any | Header, TOC and name-table notes (Spanish) | [notes](../docs/guide/toolkit/rpf9-research.md) |
| gta6-pc-port-hub | coordination | CC0-1.0 | any | Shader census, root-signature fallback, blockers (archived) | [notes](../docs/guide/toolkit/gta6-pc-port-hub.md) |
| rage-tools-vi | archive | MIT | any | RPF9 extraction, audio decode, script schema (Python + Rust) | [notes](../docs/guide/toolkit/rage-tools-vi.md) |
| rage9-re | archive | MIT / Apache-2.0 | any | PSO data and animations to glTF | [notes](../docs/guide/toolkit/rage9-re.md) |
| bcpack-rs | texture | MIT / Apache-2.0 | any | BCPack texture decoder, relevant to XCL texture work | [notes](../docs/guide/toolkit/bcpack-rs.md) |
| sixng | viewer | GPL-3.0 | any (runs on macOS) | Models, textures, map to glTF. Use only, do not copy code | [notes](../docs/guide/toolkit/sixng.md) |
| gta6-explorer | archive | MIT | any | Read-only browse and extract (Python) | [notes](../docs/guide/toolkit/gta6-explorer.md) |
| ttpf-studio | texture | MIT | any | `.ttpf` texture view and export; build from source, not the unsigned exe | [notes](../docs/guide/toolkit/ttpf-studio.md) |
| percystar | archive | all rights reserved | any | RPF9 explorer, Bink2, ADAT Opus. Use only: no modification, no code reuse | [notes](../docs/guide/toolkit/percystar.md) |
| vice-lens | viewer | MIT | any | Map, models, animation, movies explorer; build from source (release bundles AGPL ffmpeg) | [notes](../docs/guide/toolkit/vice-lens.md) |
| 6stars | viewer | none | macOS | Metal world viewer. No licence: use only | [notes](../docs/guide/toolkit/6stars.md) |
| awc-from-rpf9 | audio | MIT | any | AWC music stream extraction; first run downloads the Leonida decoder; Android station packs not used | [notes](../docs/guide/toolkit/awc-from-rpf9.md) |

## System tools (checked by `doctor`, never installed by it)

| name | platform | install hint |
|---|---|---|
| git, gh | any | `brew install git gh` / `winget install Git.Git GitHub.cli` |
| python3 (3.11+) | any | `brew install python` / `winget install Python.Python.3.12` |
| uv | any | `brew install uv` / `winget install astral-sh.uv` |
| rustup, cargo | any | `brew install rustup` / `winget install Rustlang.Rustup` |
| xcode-select | macos | `xcode-select --install` (6stars, Swift) |
| imhex | any | `brew install --cask imhex` / `winget install WerWolv.ImHex` (hex and pattern language for RPF9) |
| ghidra | any | `brew install --cask ghidra` / https://ghidra-sre.org (PE disassembly) |
| rustup-msvc | windows | `rustup toolchain install stable-x86_64-pc-windows-msvc` |
| dxc | windows | Windows SDK or VS Build Tools |
| pwsh | windows | `winget install Microsoft.PowerShell` |
| pix, renderdoc | windows | `winget install Microsoft.PIX BaldurKarlsson.RenderDoc` (frame capture) |
| windbg, x64dbg | windows | `winget install Microsoft.WinDbg x64dbg.x64dbg` |
| rga | windows | Radeon GPU Analyzer (RDNA2 ISA): https://gpuopen.com/rga/ |
| llvm-mc | windows | LLVM with the AMDGPU target, matching `tests/data/llvm-amdgpu` |

## Excluded tools

| tool | reason |
|---|---|
| rpf9-explorer | Indexed mode needs an external decoder that reads the game executable to decode the encrypted index; no licence |
| RageForge | Author closed it, citing leaked builds; the only full copy is a third-party re-upload |
| xbox-hle-lab | Built around running the leaked game executable; auto-merging agent runner |
| xseriesrecomp | Repository not found |
| AnyBox, GTA6-BootTest, G6Recomp | Built around the leaked executable |

## Adding a tool

Add a row to `tools.tsv` (tab-separated: `name kind source branch category platform notes`; `git` rows take a URL and branch, `system` rows a read-only check command and an install hint in notes), then run `./reverse-toolkit/update.sh`. Add a notes page under `docs/guide/toolkit/`. Check the licence first and write the constraint in the notes column.
