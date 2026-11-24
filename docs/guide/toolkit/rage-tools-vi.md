# rage-tools-vi

RPF9 archive extraction, entry reconstruction, name recovery, audio decoding and script-schema (PSIN/LSNS) tools for GTA VI. A Rust CLI plus a large set of Python scripts, committed measurement tables and a GUI explorer.

- Repo: https://github.com/lynxnathan/rage-tools-vi (branch `main`)
- Commit described: `51c4bd6`, 2026-11-23 (`reverse-toolkit/tools.lock`)
- Licence: MIT (`reverse-toolkit/tools/rage-tools-vi/LICENSE:1`). MIT code can be read and run. If you adapt any of it for XCL (GPL-2.0-only), keep the MIT notice and say so in the PR; prefer to write our own.

## What it is for in our work

- **RPF9 extraction without the table of contents**: it carves zlib streams, verifies them by checksum and indexes ranges (`reverse-toolkit/tools/rage-tools-vi/README.md:11`). This is the same payload-first idea as `xcl rpf` (`crates/xcl/src/rpf.rs:1`), at larger scale and with an entry model.
- **Audio**: `tools/extract_awc_all.py` decodes `ADAT` streams (Opus to WAV; XMA2 through ffmpeg). Result tables are in `handoff/audio-*.tsv`. Useful as reference data when XAudio2 work needs real stream shapes.
- **Script schema**: `tools/psin_extract.py` and `docs/architecture/object-graph.md`.
- **Hash names**: a registry of 36,952 JOAAT hash to name pairs (`handoff/names.tsv`, `tools/name_registry.py`).
- **Shaders**: not here. The README points to the `rdna2-spirv` submodule (`reverse-toolkit/tools/rage-tools-vi/README.md:34`), which is broken (see Caveats).

## Prerequisites and install

- Python 3.11+ and `uv`; the only dependency is `numpy>=2.0` (`reverse-toolkit/tools/rage-tools-vi/pyproject.toml:5`, `:7`). `[tool.uv] package = false`, so there is nothing to install as a package: `uv run python tools/<script>.py ...`, or plain `python3` for scripts that need no numpy.
- Rust stable, 2021 edition, dependency `flate2` only (`reverse-toolkit/tools/rage-tools-vi/Cargo.toml:6`). The explorer has its own crate with `iced 0.14` (GUI), `rfd`, `image` (`reverse-toolkit/tools/rage-tools-vi/explorer/Cargo.toml:14`). It enables the x11 and wayland features, so it was written for Linux; macOS/Windows builds are untested.
- Build the CLI: `cargo build` in the clone (the same on macOS and Windows; do it only after reviewing the source). Binary: `target/debug/rage-tools-vi`.
- The shell scripts (`tools/*.sh`) are bash and some hard-code the author's paths (`reverse-toolkit/tools/rage-tools-vi/tools/corpus_pass.sh:18` defaults to `/home/lynxnathan/gta6-source`; `tools/check.sh` writes to `/tmp`). On Windows use WSL/Git Bash, and always pass the source folder explicitly.

## Usage

Rust CLI usage text: `reverse-toolkit/tools/rage-tools-vi/src/main.rs:18`; dispatch: `reverse-toolkit/tools/rage-tools-vi/src/main.rs:34`. It insists on the `9FPR` signature at byte zero (`reverse-toolkit/tools/rage-tools-vi/src/main.rs:68`). It reads the whole archive into memory (`reverse-toolkit/tools/rage-tools-vi/src/main.rs:67`), so use it on small archives.

```sh
target/debug/rage-tools-vi inspect story000_bvh4.rpf
target/debug/rage-tools-vi list story000_bvh4.rpf                       # PSIN zlib streams
target/debug/rage-tools-vi packs story000_bvh4.rpf                      # header fields, metadata entropy
target/debug/rage-tools-vi extract-zlib a.rpf 0x340 0x4f1 out.xml       # end offset is exclusive
```

Python tools (argument parsers: `reverse-toolkit/tools/rage-tools-vi/tools/stream_index.py:50`, `reverse-toolkit/tools/rage-tools-vi/tools/extract_entry.py:12`, `reverse-toolkit/tools/rage-tools-vi/tools/extract_awc.py:23`, `reverse-toolkit/tools/rage-tools-vi/tools/extract_awc_all.py:27`, `reverse-toolkit/tools/rage-tools-vi/tools/name_registry.py:175`, `reverse-toolkit/tools/rage-tools-vi/tools/bulk_extract.py:28`):

```sh
python3 tools/stream_index.py ARCHIVE --out output/index/name.tsv       # mmap range index
python3 tools/extract_entry.py --archive A --kind zlib --start 0x0 --end 0x1000 --output out.bin --dry-run
python3 tools/bulk_extract.py ARCHIVE output/index/name.tsv out_dir --workers 8 --resume
python3 tools/extract_awc_all.py --framing handoff/audio-awc-framing.tsv --out-manifest out.tsv --no-wav
python3 tools/name_registry.py --lookup 0x01166164
python3 tools/psin_extract.py script.rpf --out cache/psin-properties.tsv
```

`extract_entry.py` calls the Rust CLI (`--cli` defaults to `target/debug/rage-tools-vi`, `reverse-toolkit/tools/rage-tools-vi/tools/extract_entry.py:18`). The GUI explorer and its helpers are listed in `reverse-toolkit/tools/rage-tools-vi/explorer/README.md:1` (`annoyed-index`, `annoyed-extract`, `annoyed-headers`, `annoyed-search`, `annoyed-verify`); they take the archive and an index TSV.

## Reading guide

- `reverse-toolkit/tools/rage-tools-vi/README.md:23` results table; `reverse-toolkit/tools/rage-tools-vi/README.md:36` what is and is not solved.
- `docs/index.md`, `docs/explained.md` (first principles), `docs/rpf9/format.md` (header, three metadata regions, payload), `docs/extraction/*.md` (audio, BVH4 entries, corpus, large archives), `docs/reference/tools.md` (tool list).
- `format.md`, `plan.md` (open work), `investigation.md` (append-only journal), `handoff/README.md` (every table and its columns).

## Caveats

- **The `rdna2-spirv` submodule is broken.** `reverse-toolkit/tools/rage-tools-vi/.gitmodules:1` points at `https://github.com/lynxnathan/rdna2-spirv.git`, which no longer exists. The folder `rdna2-spirv/` is empty in our clone. Do not run `git submodule update`. Any README claim about shader decompilation is not checkable here.
- It says the table of contents is encrypted and everything is offset-addressed (`reverse-toolkit/tools/rage-tools-vi/README.md:43`). Some scripts concern cipher and executable research (for example `tools/crypt_battery.py`, `tools/pe_scan.py`, `tools/watch_for_exe.sh`). The toolkit does not use keys, so skip them.
- `AGENTS.md` describes a two-agent workflow and mentions licensed NDA material kept outside git; none of that is in the repo and none of it applies to us.
- Many scripts assume a populated `output/`, `cache/` or the author's NVMe copy; commands that need them print `skipped` rather than fail (`reverse-toolkit/tools/rage-tools-vi/README.md:59`).
- `docs.sh` and the MkDocs site need `requirements-docs.txt`; not needed for the tools.

## Update

```sh
./reverse-toolkit/update.sh update --only rage-tools-vi
```
