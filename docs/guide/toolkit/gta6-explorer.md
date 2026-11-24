# gta6-explorer (gta6x)

Pure-Python, read-only command-line explorer for a GTA VI Xbox package: lists and exports textures, audio, video, shaders and metadata, builds a searchable catalog, and can hand model and world exports to SixNG.

- Repo: https://github.com/HolyRoses/gta6-explorer (branch `main`)
- Commit described: `d78a9a5`, 2026-11-23 (`reverse-toolkit/tools.lock`)
- Licence: MIT (`reverse-toolkit/tools/gta6-explorer/LICENSE`). Vendored BC decoders carry their own notices in `reverse-toolkit/tools/gta6-explorer/LICENSES/`.
- Constraint: MIT, so reading is free, but keep the notice if anything is ever reused. Its optional model backend is SixNG (GPL-3.0): run it as a separate program only, never copy its code into XCL.

## What it is for in our work

A zero-install, cross-platform way to poke at the same data XCL deals with: RPF9 headers and zlib resource carving, `.ttpf` texture pools, Bink 2 movie metadata, AWC audio containers, DXBC shader containers and PSIN (PSO) schemas. Because it is standard-library Python it runs on the Mac without a build. The format notes it collected are in `reverse-toolkit/tools/gta6-explorer/recovered/FINDINGS.md` and `reverse-toolkit/tools/gta6-explorer/recovered/HASH_RESEARCH.md`.

It does not decrypt the RPF9 index; it scans and carves (`reverse-toolkit/tools/gta6-explorer/README.md:163`).

## Prerequisites and build from source

There is nothing to build or install. Python 3.10 or newer, standard library only (`reverse-toolkit/tools/gta6-explorer/README.md:27`). Optional extras:

- A libopus system library, only for WAV export (`--wav-out`, `--music-wav-out`). Raw and Ogg export need nothing.
- A SixNG executable for `models` and `world --via sixng`. Build SixNG from source (see [sixng.md](sixng.md)); do not download a prebuilt one.

macOS and Windows are the same: run from the clone's root with `python3 -m gta6x` (`python -m gta6x` on Windows).

## Usage

Syntax: `python3 -m gta6x [--root PATH] [--json] COMMAND [OPTIONS]`. Global options go before the command (`reverse-toolkit/tools/gta6-explorer/gta6x/cli.py:5334`). The package folder comes from `--root` or `GTA6_ROOT` (default `.`; `reverse-toolkit/tools/gta6-explorer/gta6x/cli.py:18`). Entry point: `reverse-toolkit/tools/gta6-explorer/gta6x/__main__.py:3`, `main` at `reverse-toolkit/tools/gta6-explorer/gta6x/cli.py:5784`.

```bash
cd reverse-toolkit/tools/gta6-explorer
export GTA6_ROOT=/path/to/your/own/package

python3 -m gta6x selftest                      # parser checks, no game data needed
python3 -m gta6x info                          # package identity and archives
python3 -m gta6x textures --pool vehicles --max 20 --out ~/exports/textures --png
python3 -m gta6x awc --archive audio --limit 64 --ogg-out ~/exports/audio
python3 -m gta6x movies --archive movies --limit 64 --out ~/exports/movies
python3 -m gta6x psin --archive levels --schema --out ~/exports/schemas.json
python3 -m gta6x catalog --archive levels --limit 64 --db ~/exports/gta6.db
python3 -m gta6x browse --db ~/exports/gta6.db --out ~/exports/site    # then open site/index.html
```

Subcommands used above are defined at: `textures` `reverse-toolkit/tools/gta6-explorer/gta6x/cli.py:5741`, `awc` `:5682`, `movies` `:5412`, `catalog` `:5478`, `browse` `:5491`, `selftest` `:5779`.

- `--limit` generally means MiB scanned per archive and `--max` only caps what is displayed; check `COMMAND --help` before a large export (`reverse-toolkit/tools/gta6-explorer/README.md:156`).
- Models and world data go through SixNG: set `GTA6X_SIXNG` or pass `--backend PATH` (lookup order at `reverse-toolkit/tools/gta6-explorer/gta6x/models.py:44`), then `python3 -m gta6x models extract --archive packs --max 5 --out ~/exports/models` or `world --via sixng --kind placements`. Backend indexes are cached in `.sixng-work/` inside the clone (git-ignored via the tool's own `.gitignore`; keep it out of XCL anyway).
- `textures --bcpack` tries native BCPack decoding of BC1/BC3/BC4/BC5; BC6H/BC7 BCPack is unsupported (`reverse-toolkit/tools/gta6-explorer/README.md:165`). `--bcpack-helper` / `GTA6X_BCPACK` names an optional external exporter; do not point it at anything you have not read.

Inputs: your own package folder, read only. Outputs: DDS and PNG, `.opus`/Ogg/WAV, Bink files, JSON, a SQLite catalog, an HTML browser, all under the paths you pass.

## Caveats

- Names are not recovered from a file table; unnamed textures keep hash identifiers.
- Texture and audio coverage is partial (XMA2 unsupported; some volume and BCPack cases refused). It states its limits in the README section "Current limits" (`reverse-toolkit/tools/gta6-explorer/README.md:161`).
- The very large `cli.py` (about 5800 lines) is one file; use `--help` per command rather than reading it.

## Update

```bash
./reverse-toolkit/update.sh update --only gta6-explorer
```
