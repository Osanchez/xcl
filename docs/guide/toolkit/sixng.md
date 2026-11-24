# SixNG

Rust asset extractor for an RPF9 install: carves archives, recovers names, and exports models, textures, map data, audio and video, with a CLI and a GUI.

- Repo: https://github.com/holdmysocks/SixNG (branch `master`)
- Commit described: `622f9e5`, 2026-11-23 (`reverse-toolkit/tools.lock`)
- Licence: GPL-3.0-only (`reverse-toolkit/tools/sixng/Cargo.toml:8`).
- **Constraint: use the tool, never copy its code or text into XCL.** XCL is GPL-2.0-only, which cannot take GPL-3.0 code. Reading it to learn a format is fine; pasting is not.

## What it is for in our work

SixNG is the most complete open reader for the GTA VI RPF9 data, and it runs on macOS, so it is the first tool to reach for when we need to look at a format on the Mac. It covers:

- RPF9 archives: header, zlib-stream carving and name recovery. It does not read or decrypt the table of contents; contents are found by carving (`reverse-toolkit/tools/sixng/README.md:42`).
- Textures: BC1 to BC7 including BCPack, as PNG or DDS (`reverse-toolkit/tools/sixng/README.md:44`). Useful as a reference result next to our own texture work.
- Models to glTF/GLB, collision, navmesh, map placements, lights, PSO metadata, timecycle.
- Audio banks (Opus remux, WAV) and Bink video carving (`.bk2` as stored).

For comparison, XCL has its own `xcl rpf` subcommand (list and extract RPF9 records, Windows only): `crates/xcl/src/rpf.rs:1`. SixNG is the independent second opinion for what the records are.

## Prerequisites and build from source

Build it yourself. Do not run the prebuilt release archives from the project's release workflow (`reverse-toolkit/tools/sixng/.github/workflows/release.yml`); the macOS app it produces is ad-hoc signed and not notarised (`reverse-toolkit/tools/sixng/README.md:90`).

- Rust stable 1.80 or newer and **CMake** (libopus is built with CMake; `reverse-toolkit/tools/sixng/.cargo/config.toml:4` sets the policy libopus needs under CMake 4).
- Without CMake: `--no-default-features` drops Opus-to-WAV and GUI audio playback; Opus still exports as `.opus` (`reverse-toolkit/tools/sixng/crates/sixng-cli/Cargo.toml:12`).

macOS (Apple Silicon, macOS 11 or newer):

```bash
brew install cmake
cd reverse-toolkit/tools/sixng
cargo build --release          # target/release/sixng and target/release/sixng-gui
```

Windows:

```powershell
cd reverse-toolkit\tools\sixng
cargo build --release          # target\release\sixng.exe and sixng-gui.exe
```

Workspace members: `sixng-core`, `sixng-cli`, `sixng-gui` (`reverse-toolkit/tools/sixng/Cargo.toml:3`). The CLI binary is named `sixng` (`reverse-toolkit/tools/sixng/crates/sixng-cli/Cargo.toml:8`). `scripts/macos-app.sh` builds a `dist/SixNG.app` if you want a bundle.

## Usage

Parser: `Cli` and the `Cmd` subcommands in `reverse-toolkit/tools/sixng/crates/sixng-cli/src/main.rs:15` and `:162`. Every command prints one JSON document on stdout; progress goes to stderr as NDJSON (`--quiet` silences it, `--pretty` indents).

The game folder has no default: `--game <dir>`, the `SIXNG_GAME` variable, or the folder the GUI saved (`reverse-toolkit/tools/sixng/crates/sixng-cli/src/main.rs:827`). The work folder (indexes and caches) is `--work` / `SIXNG_WORK`, default `./work` (`reverse-toolkit/tools/sixng/crates/sixng-cli/src/main.rs:20`). Point both at your own install and a scratch directory outside the XCL tree.

```bash
S=reverse-toolkit/tools/sixng/target/release/sixng
export SIXNG_GAME=/path/to/your/own/install SIXNG_WORK=$HOME/sixng-work

$S info --pretty                              # archives, headers, index status
$S index levels textures packs data common    # resumable carve index; slow (about 6 min for levels.rpf on NVMe)
$S list levels --rtype txd --limit 20 --pretty
$S search 'elegy|buffalo' --archive levels
$S extract levels --rtype txd --name '^island' -o $SIXNG_WORK/out --tex both
$S ttpf data/ttdb/ui/ttdb_ui.ttpf --first 10 --out $SIXNG_WORK/ui     # decode textures from a .ttpf store
$S movies movies --out $SIXNG_WORK/out        # list Bink videos; add --extract --only <hex offsets> for .bk2
$S audio --min-seconds 30 --limit 10 --pretty # scan audio.rpf once, then list banks
$S audio-export --bank <hex offset> --format both -o $SIXNG_WORK/out/audio
```

- `index`: `Cmd::Index` at `reverse-toolkit/tools/sixng/crates/sixng-cli/src/main.rs:166`; handler `:861`.
- `list` / `search` / `extract` share the filter flags `--kind`, `--rtype`, `--name`, `--exclude-name`, `--id`, `--id-file`, `--min-size`, `--limit`, `--bbox` (`reverse-toolkit/tools/sixng/crates/sixng-cli/src/main.rs:33`). Extract flags (`--tex png|dds|both`, `--raw`, `--force`, `--dry-run`) are in `OutArgs` at `:94`. Use `--dry-run` first on large selections.
- `ttpf` decodes a texture store (`reverse-toolkit/tools/sixng/crates/sixng-cli/src/main.rs:753`); `movies` at `:550`; `audio-export` at `:607` (formats `opus`, `wav`, `both`, `awc`).
- Map and world data: `placements`, `collision`, `navmesh --roads-only`, `lights`, `world all` write JSON/GLB under `-o`. They are big (collision about 15 GB, placements about 1.3 GB; see the disk-space table at `reverse-toolkit/tools/sixng/README.md:114`).
- Characters: `sixng characters --name deer` then `sixng outfit <name> -o out.glb [--clip dict/clip]` writes one rigged GLB (`reverse-toolkit/tools/sixng/crates/sixng-cli/src/main.rs:644`).
- GUI: run `sixng-gui`; a setup wizard asks for the game folder and work folder and saves them.

Inputs: your own install's `.rpf` / `.ttpf` files, read only. Outputs: JSON, PNG/DDS, GLB, WAV/Opus/`.bk2` under `--out`; indexes under `--work`. Nothing goes in git (the project ignores `work/`).

## Caveats

- Names are recovered from content, not from a file table, so many entries stay unnamed.
- BCPack is fitted, not specified: some BC7 and BC6H blocks can be off (`reverse-toolkit/tools/sixng/README.md:26`). Do not treat its output as ground truth for a BCPack bit-exact decoder.
- Disk: full exports reach tens of GB. Use `--bbox`, `--limit`, `--only`.
- The tool states it does not decrypt anything. If a run needs a key or an executable, stop; that is outside the toolkit rules.

## Update

```bash
./reverse-toolkit/update.sh update --only sixng
```

Then rebuild (`cargo build --release`) and check `reverse-toolkit/tools.lock` for the new commit.
