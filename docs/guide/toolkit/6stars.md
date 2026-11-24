# 6stars

Swift and Metal world engine for macOS that streams the GTA VI map straight out of RPF9 archives, plus `rpftool`, a command-line tool for format research.

- Repo: https://github.com/meetox80/6stars (branch `main`)
- Commit described: `480d9a4`, 2026-11-22 (`reverse-toolkit/tools.lock`)
- Licence: **none**. The repository has no `LICENSE` file for its own code (only the vendored Dear ImGui and cimgui carry their licences, in `reverse-toolkit/tools/6stars/Sources/CImGui/`). With no licence, default copyright applies.
- **Constraint: use only. Do not copy code, tables or text into XCL, and do not redistribute it.** Read it for understanding at most.

## What it is for in our work

The only tool here that is macOS-native and Metal-based, so it runs where XCL's own tooling does not (XCL itself builds on Windows only, see `docs/guide/testing-and-scripts.md`). Two uses:

- `rpftool` is a compact way to scan an RPF9 archive into an index, list and extract zlib-carved resources by magic, and dump single resources or structures. It is the smallest reader to compare against our own `xcl rpf` (`crates/xcl/src/rpf.rs:1`).
- The engine shows what the map data looks like when drawn (LOD hierarchy, vegetation batches, timecycle lighting), a sanity check for map and texture formats.

It states it works from unencrypted data and contains no keys (`reverse-toolkit/tools/6stars/README.md:26`).

## Prerequisites and build from source

macOS 15 with the Xcode command-line tools (Swift 6, Metal): `xcode-select --install` (checked by `./reverse-toolkit/update.sh doctor`). Declared at `reverse-toolkit/tools/6stars/Package.swift:6` (`.macOS(.v15)`) and `reverse-toolkit/tools/6stars/Package.swift:1` (`swift-tools-version:6.0`). There is no Windows or Linux build; it needs Metal and AppKit. There is no prebuilt release to avoid; build it yourself.

```bash
cd reverse-toolkit/tools/6stars
make                 # release build of 6stars and rpftool into .build/release
make rpftool         # just the research tool
make app             # ~/testing/6stars.app, ad-hoc signed (override with APP=...)
make clean
```

The Makefile keeps SwiftPM and clang caches under `~/testing/.cache` by default (`reverse-toolkit/tools/6stars/Makefile:2`); pass `CACHE=/some/dir` to move them. `swift build` runs as the `release` configuration unless `CONFIG=debug` is given (`reverse-toolkit/tools/6stars/Makefile:1`). The package's C targets are Dear ImGui (vendored), cimgui, and a zlib module map (`reverse-toolkit/tools/6stars/Package.swift:12`).

## Usage

**Data location.** The engine hard-codes `~/testing` as the data root: `levels.rpf`, `textures.rpf`, `data/ttdb`, `pack/` and an index folder `~/testing/6stars-data` (`reverse-toolkit/tools/6stars/Sources/SixStars/main.swift:9`). Either put your own files there, or symlink your own install's `levels.rpf`, `textures.rpf`, `common.rpf`, `data.rpf`, `data/` and `pack/` into `~/testing`. `--archive` and `--index` override only the main archive and its index (`reverse-toolkit/tools/6stars/Sources/SixStars/main.swift:24`); the texture roots (`:11`, `:12`) are fixed.

```bash
cd reverse-toolkit/tools/6stars
.build/release/6stars                                                # fly through the world (click to capture the mouse, WASD, Shift, scroll for speed, ` for the debug panel)
.build/release/6stars --snapshot out.png --camera x,y,z,yaw,pitch    # headless 1600x900 render
make run ARGS="--snapshot out.png"

.build/release/rpftool scan ~/testing/levels.rpf /tmp/levels.idx     # build an index (no decryption: finds zlib streams)
.build/release/rpftool stats /tmp/levels.idx                         # counts by kind and magic, gaps
.build/release/rpftool list  /tmp/levels.idx <magic|kind> 20         # entries of one magic or kind
.build/release/rpftool dump  ~/testing/levels.rpf /tmp/levels.idx 0x<offset> /tmp/one.bin
.build/release/rpftool extract ~/testing/levels.rpf /tmp/levels.idx <magic> /tmp/out
```

Engine flags are parsed at `reverse-toolkit/tools/6stars/Sources/SixStars/main.swift:22` (`--archive`, `--index`, `--snapshot`, `--camera`, `--texture`, `--txd`, `--txd-only`, `--level`, `--paramstats`, `--pick`, `--probe`). `rpftool` dispatches on its first argument in `reverse-toolkit/tools/6stars/Sources/rpftool/main.swift:53` (usage string at `:53`); `scan` is at `:57`, `list` `:73`, `extract` `:80`, `dump` `:103`. Many further research subcommands exist in the same file (`entities`, `catalog`, `names`, `ttdbcheck`, `texparams`, `shaderdump`, ...); they open archives through a memory-mapped reader (`reverse-toolkit/tools/6stars/Sources/RPFKit/MappedFile.swift`); check a command's code before pointing it at an output path.

Inputs: your own `.rpf` and `.ttpf` files. Outputs: index files, extracted raw resources, PNGs from `--snapshot`/`--texture`. Keep every output outside the XCL tree.

## Caveats

- Index and cache folders are created under `~/testing`; the engine rebuilds missing caches on start, which takes time on a large install (`reverse-toolkit/tools/6stars/README.md:44`).
- `Sources/SixStars/BuildInfo.swift` is generated by `make` (`reverse-toolkit/tools/6stars/Makefile:28`); it is untracked in the clone, so `git status` there stays clean (the tool's `.gitignore` lists it).
- The `tools/` folder holds Python research scripts. We do not run them; they are unvetted and add nothing to `rpftool`.
- No network code was found in `Sources/` (searched for URL loading).

## Update

```bash
./reverse-toolkit/update.sh update --only 6stars
```

Then `make` again.
