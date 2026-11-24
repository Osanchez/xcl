# bcpack-rs

Clean-room decoder for BCPack, the rANS entropy codec the Xbox Series X|S Media Decompression Unit uses for BC1-BC7 textures. Rust crate and CLI, a stdlib-only Python reference decoder and a written spec. Today only BC1 decodes end to end.

- Repo: https://github.com/Ahmet-Dedeler/bcpack-rs (branch `master`)
- Commit described: `c51ec52`, 2026-11-23 (`reverse-toolkit/tools.lock`)
- Licence: `MIT OR Apache-2.0` (`reverse-toolkit/tools/bcpack-rs/Cargo.toml:5`). The rANS tables and per-lane decoder are ported from Microsoft's MIT `silicon-rans` (`reverse-toolkit/tools/bcpack-rs/THIRD_PARTY_NOTICES.md:1`). Permissive, so code could be adopted into XCL (GPL-2.0-only) with the notices kept and a provenance note plus a `THIRD_PARTY.md` entry (`CONTRIBUTING.md`). That is a decision for the maintainer; until it is made, treat the crate as a reference and test oracle.

## What it is for in our work

The most direct overlap with XCL texture work.

- XCL detiles console textures (GFX10 swizzle) in `crates/xcl/src/gpu/tiling.rs` and uploads them in `crates/xcl/src/gpu/texture.rs`; the module doc says it handles textures the title wrote already tiled (`crates/xcl/src/gpu/texture.rs:1`).
- XCL does **not** decode BCPack: DirectStorage requests with a BCPack mode are refused and counted (`crates/xcl/src/hle/dstorage.rs:135`, `docs/guide/hle.md:388`).
- bcpack-rs shows what a decoder needs: the 12-lane rANS stage (exact) and the BC1 block rebuild with two fitted tables (`reverse-toolkit/tools/bcpack-rs/docs/BCPACK_SPEC.md:36` onward). `src/ttpf.rs` has an independent RDS5/ETPF reader and Gfx10 detiler, which is a cross-check for XCL's `tiling.rs`.
- Use `bcpack ttpf-export` to make a plain BC1 PNG/DDS from a store texture and compare it with what XCL renders for the same data.

## Prerequisites and install

Rust stable, 2021 edition (`reverse-toolkit/tools/bcpack-rs/Cargo.toml:4`). Dependencies: `flate2`, `png`, `rayon`, `serde_json`; dev: `sha2` (`reverse-toolkit/tools/bcpack-rs/Cargo.toml:26`). Default feature `cli` pulls `ttpf` and `rayon` (`reverse-toolkit/tools/bcpack-rs/Cargo.toml:19`). The same commands work on macOS and Windows. Python 3 for the reference decoder.

```sh
cd reverse-toolkit/tools/bcpack-rs && cargo build --release   # review the source first
# decoder-only library (serde_json only):  default-features = false  (reverse-toolkit/tools/bcpack-rs/README.md:79)
```

Binary: `target/release/bcpack`.

## Usage

Usage text: `reverse-toolkit/tools/bcpack-rs/src/main.rs:14`; command dispatch: `reverse-toolkit/tools/bcpack-rs/src/main.rs:109`.

```sh
bcpack decode-bc1 chunk.bin 4096 out.bc1          # one chunk (zero padding ok) + block count -> BC1 blocks
bcpack symbols chunk.bin 4096                     # the 12 lane symbol streams
bcpack ttpf-export pool.ttpf 318 out.png          # mip 0 of BC1 texture #318, detiled; .png or .dds
bcpack scan --jobs 8 --every 20 pools/*.ttpf      # convergence + speed over stores
bcpack bench chunk.bin 4096 [seconds]             # single-core speed
python3 -I python/bcpack.py decode-bc1 chunk.bin 4096 out.bc1   # slow reference (run from the clone)
```

`--window file.json` and `--quads file.json` replace the built-in fitted tables (`reverse-toolkit/tools/bcpack-rs/src/main.rs:67`, `reverse-toolkit/tools/bcpack-rs/src/main.rs:72`). Inputs are chunks and `.ttpf` stores from your own install; the retail checks read a pool root from `BCPACK_GAME_ROOT` (`reverse-toolkit/tools/bcpack-rs/tools/retail/crosscheck.py:8`). Tests: `cargo test --release` and `python3 -I python/bcpack.py selftest` (`reverse-toolkit/tools/bcpack-rs/README.md:99`); they use synthetic streams only.

## Caveats

- BC1 only. BC3, BC4, BC5, BC6H and BC7 have a solved entropy layer on research branches but no block mapping in this decoder (`reverse-toolkit/tools/bcpack-rs/README.md:24`). The checkout is the `decoder` branch's consolidated state; the research branches and bench scripts are not in the clone (`reverse-toolkit/tools/bcpack-rs/README.md:110`).
- The block layer uses fitted tables: 254 of 256 preset-window slots, 248 of 256 codebook entries agreed by two fits (`reverse-toolkit/tools/bcpack-rs/README.md:21`). Output is not byte-exact against hardware; the README reports about 34-35 dB median on mip checks against a 37.9 dB BC1 ceiling.
- Speed is 34-40 MB/s of BC1 per core, below the author's 50 MB/s target (`reverse-toolkit/tools/bcpack-rs/README.md:37`).
- `src/tables.rs` and `python/bcpack_tables.py` are generated (`tools/gen_tables.py`); do not edit them (`AGENTS.md`).
- `reverse-toolkit/tools/bcpack-rs/docs/BCPACK_SPEC.md:148` lists what is not known.

## Update

```sh
./reverse-toolkit/update.sh update --only bcpack-rs
```
