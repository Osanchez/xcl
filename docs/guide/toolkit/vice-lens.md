# Vice Lens

Rust desktop app (a wry/tao web view over a wgpu renderer) for exploring the Xbox Series GTA VI data: the whole map with every placement, models, vehicles, characters and animals with animation, textures, Bink 2 movies, sound, raw archive files and text, plus a native 3D fly-through of the world.

- Repo: https://github.com/Librett0/vice-lens (branch `main`)
- Commit described: `8b8c8a8`, 2026-11-22 (`reverse-toolkit/tools.lock`)
- Licence: MIT (`reverse-toolkit/tools/vice-lens/LICENSE`).
- **Build from source, not the release archive.** The release archives bundle a separate `ffmpeg` built from the author's FFmpeg fork, which is **AGPL-3.0-or-later** (`reverse-toolkit/tools/vice-lens/README.md:247`). The app itself is MIT and shares no code with it. We do not redistribute anything from this tool, and we do not need that ffmpeg to read the data.

## What it is for in our work

The best browser for the map and for movies. It is where to look when we need to know what a placement, a model, an animation clip or a Bink 2 movie in the install really is. The `rage` crate documents formats from the reader side (`reverse-toolkit/tools/vice-lens/docs/formats.md`), and `docs/caches.md` lists every derived cache (`reverse-toolkit/tools/vice-lens/docs/caches.md:1`). Video: movies are shown through an FFmpeg that has a `bink2video` decoder; stock FFmpeg lacks one (`reverse-toolkit/tools/vice-lens/crates/vice-lens/src/lens/media.rs:185`).

## Prerequisites and build from source

- Rust stable with the 2024 edition (`reverse-toolkit/tools/vice-lens/Cargo.toml:7`).
- Linux: WebKitGTK 4.1 and GTK 3 dev packages (`libwebkit2gtk-4.1-dev libgtk-3-dev` on Debian/Ubuntu) and a Vulkan driver for the 3D world (`reverse-toolkit/tools/vice-lens/README.md:34`). The Linux build adds `webkit2gtk` (`reverse-toolkit/tools/vice-lens/crates/vice-lens/Cargo.toml:27`).
- Windows: the project's CI builds and smoke-tests it on `windows-latest` (`reverse-toolkit/tools/vice-lens/.github/workflows/smoke-windows.yml:19`); the README states Linux and Windows only.
- macOS: **not supported by the project** (`reverse-toolkit/tools/vice-lens/README.md:7`). The wrapper script uses `readlink -f`, which stock macOS lacks (`reverse-toolkit/tools/vice-lens/vice-lens.sh:7`). Treat it as unverified; use it on a Windows or Linux box.
- Optional, to watch movies: an FFmpeg build that has `bink2video`. If you want one, build it from source from the author's repo (the README links `Librett0/vice-lens-ffmpeg`) rather than unpacking a release. Without it everything else works; movies do not play.

```bash
cd reverse-toolkit/tools/vice-lens
cargo build --release -p vice-lens        # target/release/vice-lens  (vice-lens.exe on Windows)
./vice-lens.sh                            # Linux: builds if stale, then runs the app
```

## Usage

Entry point: `main` in `reverse-toolkit/tools/vice-lens/crates/vice-lens/src/main.rs:149`; flags are parsed by hand at `:116` to `:141` (`--game`, `--lang`, `--port`, `--serve`, `--build-caches`, `--make-map`, `--world`, `--window`, `--flight`, `--bench`, `--capture`, `--size`, `--input`). With no mode flag the binary is the desktop app (`reverse-toolkit/tools/vice-lens/crates/vice-lens/src/main.rs:110`).

The game folder comes from `--game DIR`, `VICE_LENS_GAME`, or the folder chosen once in the app's Setup page (`reverse-toolkit/tools/vice-lens/crates/vice-lens/src/main.rs:74`). There is no built-in default.

```bash
B=reverse-toolkit/tools/vice-lens/target/release/vice-lens
export VICE_LENS_GAME=/path/to/your/own/install

$B --build-caches                     # one-time: reads levels.rpf etc.; about an hour, about 2 GB under ~/.cache/vice-lens
$B                                    # the app: Map, Models, Textures, Video, Sound, Files, Vehicles, People, Animals, Text
$B --serve                            # same UI in a browser on the local port 7878 (or --port N)
$B --world -560,44,400 --window       # native 3D fly-through over downtown (WASD, Q/E, F12 saves a frame)
$B --make-map                         # render the top-down map images used by the minimap
$B --world -560,44,300 --flight 300 --capture frames/    # headless frames
```

- `--build-caches` is the same as Setup's "Build missing caches" (`reverse-toolkit/tools/vice-lens/docs/caches.md:8`); `reverse-toolkit/tools/vice-lens/tools/build-caches.sh` is a thin wrapper for it.
- In the app: `Ctrl K` searches everything or jumps to coordinates; Models export glTF with textures, textures save PNG, sounds save `.opus`, movies save MP4, `.bk2` as stored, or a PNG frame (`reverse-toolkit/tools/vice-lens/README.md:54`).
- `VICE_LENS_PROBE=1` measures frame rates and exits; `VICE_LENS_CAPTURE=DIR` lets the page save frames (`reverse-toolkit/tools/vice-lens/README.md:46`). Camera and sun are set with `VICE_LENS_CAM`, `VICE_LENS_SUN` (`reverse-toolkit/tools/vice-lens/crates/vice-lens/src/main.rs:345`).

Inputs: your own install, read in place and never modified. Outputs: caches under `~/.cache/vice-lens/`, exports where you choose, captures in `captures/`.

## Caveats

- Release bundle with AGPL FFmpeg: avoid it; build from source. If you compile a Bink 2 FFmpeg yourself, its AGPL terms apply to that program only, and it is never linked into XCL or committed.
- The first cache build is long and the 3D world needs a Vulkan device.
- Known gaps from its own "Where it stands" section: BCPack top mips (about 11 % of textures) are drawn grey, no terrain-layer blending, no night lights or time of day (`reverse-toolkit/tools/vice-lens/README.md:218`).
- Do not take the in-app colours of vehicles as data: the game makes its paint at runtime (`reverse-toolkit/tools/vice-lens/README.md:114`).

## Update

```bash
./reverse-toolkit/update.sh update --only vice-lens
```

Rebuild with `cargo build --release -p vice-lens`; delete a cache file under `~/.cache/vice-lens/` to rebuild that step.
