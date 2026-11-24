# rage9-re

Rust crate/CLI and Python reference for two RAGE 9 archive types: `script.rpf` (PSIN/PSO script data and an object graph) and `anim.rpf` (animation clips), with a glTF 2.0 (`.glb`) exporter for decoded clips.

- Repo: https://github.com/Ahmet-Dedeler/rage9-re (branch `master`)
- Commit described: `ddf30a3`, 2026-11-23 (`reverse-toolkit/tools.lock`)
- Licence: `MIT OR Apache-2.0` (`reverse-toolkit/tools/rage9-re/Cargo.toml:5`). Both are permissive; credit is required if you vendor a file (each file header says so, `reverse-toolkit/tools/rage9-re/python/rage9_re/__init__.py:4`). Prefer to use it as a tool, not as XCL source.

## What it is for in our work

Format documentation and a working decoder for data XCL does not load: scripts and animation. XCL runs the game's native code, so these formats only matter for understanding what a title's data contains and for inspecting animation clips outside the game. There is no XCL code that depends on it. The format notes are the useful part: `docs/SCRIPT_FORMAT.md` (nested RPF9, `PSIN` v122, `LSCH`/`LENM`/`LSNS`, binary-XML DOM, object graph) and `docs/ANIM_FORMAT.md` (`crClipDictionary` to `crAnimSequence`, the 11 channel classes).

## Prerequisites and install

Rust stable (edition 2021; no `rust-version` is set), single dependency `flate2` (`reverse-toolkit/tools/rage9-re/Cargo.toml:18`). The Python package `python/rage9_re` uses the standard library. The same commands work on macOS and Windows.

```sh
cd reverse-toolkit/tools/rage9-re && cargo build --release   # review the source first
```

Binary: `target/release/rage9-re` (`.exe` on Windows).

## Usage

The CLI has two subcommands (`reverse-toolkit/tools/rage9-re/src/main.rs:333`; usage string at `:337`):

```sh
# Script data: summary; dump one PSIN as reconstructed XML, or one object-graph root
rage9-re inspect-script /path/to/script.rpf
rage9-re inspect-script /path/to/script.rpf --dump-xml 0
rage9-re inspect-script /path/to/script.rpf --dump-objgraph 0
# Animation: summary, or export a clip to glTF
rage9-re inspect-anim /path/to/anim.rpf
rage9-re inspect-anim /path/to/anim.rpf --export-glb out.glb
rage9-re inspect-anim /path/to/clipdict.bin --export-glb out.glb   # a carved clip dictionary
```

Flags are parsed by hand (`reverse-toolkit/tools/rage9-re/src/main.rs:343` for `inspect-script`, `reverse-toolkit/tools/rage9-re/src/main.rs:366` for `inspect-anim`); an unknown flag exits 1. Inputs are your own `script.rpf` / `anim.rpf`; the CLI writes only to stdout and the `--export-glb` file. Tests use synthetic data: `cargo test`, `python3 -m unittest discover -s tests -v` (`reverse-toolkit/tools/rage9-re/README.md:47`).

## Caveats

- The glTF exporter maps only a handful of named bones; the full skeleton is not rebuilt. About 1 % of frame steps jump more than 20 degrees, so some channel or quantization case is still wrong. Not every clip dictionary in the 4.4 GB `anim.rpf` has been checked (`reverse-toolkit/tools/rage9-re/README.md:30`).
- Field names in the script DOM are hashes; most are not resolved to readable names (`reverse-toolkit/tools/rage9-re/README.md:35`).
- `anim.rpf` is read in windows, so very large inputs are slow.
- The hub lists this repo's licence as "NOASSERTION"; the Cargo.toml and LICENSE files say `MIT OR Apache-2.0`.

## Update

```sh
./reverse-toolkit/update.sh update --only rage9-re
```
