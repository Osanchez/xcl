# gta6-asset-info

Written notes on the asset formats of GTA VI (Xbox Series build): RPF9 archives, resources, texture dictionaries, Xbox tiling, `ttpf` stores, BCPack, models, map data. It also holds the user guide of the SixNG tool, whose source is not in this repo.

- Repo: https://github.com/TrinitronClient/GTA6-Asset-Info (branch `main`)
- Commit described: `c62be04`, 2026-11-23 (`reverse-toolkit/tools.lock`)
- Licence: GPL-3.0 (`reverse-toolkit/tools/gta6-asset-info/LICENSE:1`). **Reference only.** Read it; do not copy code or text into XCL (GPL-2.0-only; GPL-3.0 is not compatible). Write our own descriptions in our own words and cite the page.

## What it is for in our work

It is the best written description of the console data that XCL's DirectStorage and texture paths meet.

- **RPF9 container**: header layout and the plain-zlib payload (`formats/01-rpf9.md`, `structure/01-rpf9-container.md`). Compare with `xcl rpf` in `crates/xcl/src/rpf.rs`.
- **Texture tiling**: `reverse-toolkit/tools/gta6-asset-info/formats/05-ttpf.md:125` documents AMD Gfx10 (RDNA2) swizzle modes. XCL's own detiler is `crates/xcl/src/gpu/tiling.rs` (used by `crates/xcl/src/gpu/texture.rs`). Use the page as an independent cross-check of a mode, not as a source of code.
- **BCPack**: `reverse-toolkit/tools/gta6-asset-info/formats/06-bcpack.md:11` (status) and `:559` (how to resume). XCL has no BCPack decoder and refuses BCPack DirectStorage requests (`crates/xcl/src/hle/dstorage.rs:135`).
- **Shaders**: `structure/06-models-shaders.md` (shader cache layout, RDNA2 code spans inside DXBC wrappers).
- **Audio**: `structure/03-audio-awc.md` (`ADAT` container, Opus, XMA2, PCM).

## Prerequisites and install

None for the notes: they are Markdown. To use the SixNG tool they describe you must build SixNG from its own repository (`reverse-toolkit/tools/gta6-asset-info/tool/01-install.md:5` says Rust 1.80 or newer; `:9` says this repo holds documentation only; developed on Windows, other platforms untested). SixNG has its own toolkit entry (`sixng`, GPL-3.0, use only).

## Reading guide

Start at `reverse-toolkit/tools/gta6-asset-info/README.md:13` (page table) and `:31` (state of knowledge, as of 2026-11-21). `reverse-toolkit/tools/gta6-asset-info/formats/README.md:10` explains the confidence levels (confirmed, inferred, unknown) and conventions (little-endian, joaat hashes, Z-up to Y-up for glTF).

| Path | Covers |
|---|---|
| `formats/01-rpf9.md`, `formats/02-resources.md` | Containers, resource pages |
| `formats/03-texture-dictionaries.md`, `formats/04-xbox-tiling.md` | Dictionary textures, tiling |
| `formats/05-ttpf.md`, `formats/06-bcpack.md`, `formats/06a-bcpack-tables.md` | `ttpf` stores, BCPack, fitted tables (`formats/data/bcpack-tables.json`) |
| `formats/07-models.md` ... `formats/19-ped-physics-resources.md` | Models, map, collision, navmesh, lights, roads, PSO, audio, radio |
| `structure/00-INDEX.md` | Byte-level notes, each with the test behind it (labels OBSERVED / INFERRED / UNKNOWN) |
| `tool/02-cli.md` | SixNG CLI reference |
| `LEGAL.md` | What the repo contains and the own-copy rule |

## Usage (SixNG commands, as documented)

These need SixNG built from source; the commands are from `reverse-toolkit/tools/gta6-asset-info/tool/02-cli.md:62`. Output is one JSON document on stdout, progress as NDJSON on stderr (`reverse-toolkit/tools/gta6-asset-info/tool/02-cli.md:7`). Game root: `--game <dir>` or `SIXNG_GAME`; work dir: `--work <dir>` or `SIXNG_WORK` (`reverse-toolkit/tools/gta6-asset-info/tool/01-install.md:28`).

```powershell
sixng info --pretty                                   # archives, headers, index status
sixng index levels textures packs data                # build carving indexes, recover names
sixng list levels --rtype txd --limit 20 --pretty     # list entries of one archive
sixng search 'elegy|buffalo' --archive levels         # name search
sixng inspect 'levels.rpf@10f2d2d010' --pretty        # describe one entry
sixng extract levels --rtype txd --name '^island' -o out --tex both
sixng extract-category map --limit 200 -o out --dry-run   # try --dry-run first
```

Exports can be very large (`reverse-toolkit/tools/gta6-asset-info/tool/02-cli.md:100` gives sizes). Entry ids are `<archive>@<hex offset>` (`reverse-toolkit/tools/gta6-asset-info/tool/02-cli.md:13`).

## Caveats

- Everything was derived from one retail install and "nothing has been confirmed against game code" (`reverse-toolkit/tools/gta6-asset-info/README.md:7`). Treat each table with its confidence label.
- BCPack is not byte-exact for any codec; the fitted BC3-BC7 tables are inferred (`reverse-toolkit/tools/gta6-asset-info/README.md:58`).
- The notes say the RPF9 table of contents is encrypted, so names are recovered, not read. The toolkit does not use keys.
- `formats/` and `structure/` overlap in topic: `structure/` pages are raw byte facts with the test behind each, `formats/` pages interpret them.

## Update

```sh
./reverse-toolkit/update.sh update --only gta6-asset-info
```
