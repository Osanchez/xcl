# PercyStar

Rust (egui/wgpu) RPF9 and TTPF explorer: archive scanner, 3D model viewer with OBJ/GLB export, texture decoders, Bink 2 and ADAT Opus audio, RDNA2 shader disassembler and lifter, plus batch CLI exporters.

- Repo: https://github.com/rpatty-dev/PercyStar (branch `main`)
- Commit described: `6a64e53`, 2026-11-22 (`reverse-toolkit/tools.lock`)
- Licence: "All Rights Reserved" per the README (`reverse-toolkit/tools/percystar/README.md:307`); the `LICENSE` file is a custom non-commercial licence (`reverse-toolkit/tools/percystar/LICENSE:1`).
- **Constraint (project decision): use only. No modification, no code reuse.** Do not fork it, patch it, or copy code, tables or text from it into XCL. It cannot go into a GPL-2.0-only project in any case. Run a build from a clean checkout as is.

## What it is for in our work

The widest single-binary view of the RPF9 data: browse archives, decode textures (BC1 to BC7, RDNA2 swizzle), view models, play ADAT Opus, parse Bink 2 containers, and read the console `PS\x00\x09` pipeline-state cache and `SGD5` shader groups with a GFX10.3 disassembler. The shader side overlaps what XCL does in `crates/xcl/src/gpu` (RDNA2 to HLSL), so its output is a useful second reading when we debug a shader, never a source to port. The README says it contains no decryption and skips encrypted data (`reverse-toolkit/tools/percystar/README.md:303`).

## Prerequisites and build from source

Build from source; do not run the prebuilt release zip or `PercyStar.exe` from the project's releases. They are unsigned, and the app contains a self-updater (see Caveats).

- Rust 1.89 or newer (`reverse-toolkit/tools/percystar/Cargo.toml:5`). Binary name `PercyStar` (`reverse-toolkit/tools/percystar/Cargo.toml:9`).
- Windows: `x86_64-pc-windows-msvc` or the GNU target with MinGW (`windres`, `dlltool`).
- The build pulls one git dependency pinned by revision, `bcpack-rs` (`reverse-toolkit/tools/percystar/Cargo.toml:23`).

```bash
cd reverse-toolkit/tools/percystar
cargo test --release            # the repo reports ~98 tests
cargo build --release           # target/release/PercyStar (PercyStar.exe on Windows)
```

macOS: `scripts/build_macos.sh` builds a universal `dist/PercyStar.app`, ad-hoc signed (`reverse-toolkit/tools/percystar/scripts/build_macos.sh:50`). It needs both Apple targets (`rustup target add aarch64-apple-darwin x86_64-apple-darwin`, done by the script). For our use `cargo build --release` and running the binary is enough.

Optional: `binkplay.exe` (RAD Video Tools) for Bink playback. `scripts/setup_deps.ps1` downloads `RADTools.exe` from radgametools.com (`reverse-toolkit/tools/percystar/scripts/setup_deps.ps1:18`). We skip that script; download RAD tools yourself if you need them.

## Usage

Entry point: `main` argument dispatch in `reverse-toolkit/tools/percystar/src/main.rs:6274` onward (hand-matched, no argparse). With no flag it opens the GUI; a path argument opens a game folder, `.rpf`, `.ttpf`, `.obj` or `.ps09` directly (`reverse-toolkit/tools/percystar/src/main.rs:6424`).

```bash
P=reverse-toolkit/tools/percystar/target/release/PercyStar
$P "/path/to/your/own/install"                                   # GUI: Explorer | Model Viewer | Shaders (RDNA2)
$P --dump-textures "<archive.rpf|.ttpf>" "<outdir>" [limit]      # line 6385
$P --extract-models "<archive.rpf>" "<outdir>" "filter1,filter2" # line 6394
$P --extract-vehicles "<levels.rpf>" "<outdir>"                  # line 6390
$P --decode-audio "<audio.rpf>" "<outdir>" [limit]               # line 6399
$P --disasm-shaders "<shaders.rpf|dir|.ps09>" "<outdir>" [limit] # line 6365
$P --disassemble-dxbc "<shader.dxbc>" ["<out.asm>"]              # line 6378
```

The line numbers in the comments are all `reverse-toolkit/tools/percystar/src/main.rs`. Other flags: `--extract-sgd5` (`:6370`), `--decode-navmesh` (`:6404`), `--decode-bk2` (needs a Bink 2 DLL given by `BINK2_DLL` or next to the exe; `:6321`), `--bench` (`:6302`), `--gpu-world` (`:6274`).

Inputs: your own archives. Outputs: PNG, OBJ/MTL, GLB, WAV, shader `.hlsl`/`.spv`/`.asm`, under the output folder. A scan cache (`scan_cache_*.tsv`) is written under `%LOCALAPPDATA%\PercyStar` on Windows (`reverse-toolkit/tools/percystar/README.md:193`).

The `scripts/awc_extract.py` and `scripts/rpf9_carve.py` helpers are standalone Python; we do not need them because [rage-tools-vi](rage-tools-vi.md) and [awc-from-rpf9](awc-from-rpf9.md) cover audio.

## Caveats

- **Unsigned prebuilt binaries.** Never run the release `.exe`.
- **Self-updater.** On startup the app checks the latest release of its GitHub repository with `curl`, and offers a "Update Available" button; installing downloads `PercyStar_v<ver>_Win64.zip`, extracts it with PowerShell and replaces the running exe (`reverse-toolkit/tools/percystar/src/main.rs:681`, `:718`, check started at `:1102`). A source build runs the same code, so it also phones home. Block the app's network access (firewall rule) when you run it, and never click the update pill.
- Licence is restrictive: no fork or modification is allowed by our rule, even where the `LICENSE` file would permit some of it for non-commercial use. When in doubt, do not touch the code.
- Windows is the primary target; the README also lists macOS and Linux build scripts. Windows-only code (updater download, `explorer.exe`, `curl.exe`) is gated with `cfg(windows)` in places only.
- Large archives: scanning `packs.rpf` takes about half a minute on a good disk, `levels.rpf` several minutes.

## Update

```bash
./reverse-toolkit/update.sh update --only percystar
```

Rebuild afterwards. Read the new commit's changes before running a build that follows a long gap.
