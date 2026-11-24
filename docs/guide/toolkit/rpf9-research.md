# rpf9-research

Evidence-driven reverse engineering of the RPF9 container (header, table region, payload streams, name recovery) from one Xbox Series package, with Python tools, header fixtures and Spanish/English docs.

- Repo: https://github.com/JoshRob297/rpf9-research (branch `main`)
- Commit described: `1de30d7`, 2026-11-23 (`reverse-toolkit/tools.lock`)
- Licence: GPL-3.0 (`reverse-toolkit/tools/rpf9-research/LICENSE:1`; the README says GPL-3.0-or-later at `reverse-toolkit/tools/rpf9-research/README.md:174`). **Reference only: do not copy code into XCL (GPL-2.0-only).** It publishes no key and no full path list (`reverse-toolkit/tools/rpf9-research/README.md:27`, `reverse-toolkit/tools/rpf9-research/README.md:66`).

## What it is for in our work

- **RPF9 header, TOC geometry, nested containers, PSIN chunks**: `docs/RPF9.md`, `docs/NESTED.md`, `docs/PSIN.md`. Compare with `crates/xcl/src/rpf.rs` and `crates/xcl/src/rpf_probe.rs`.
- **Fixtures**: `raw/cabeceras/` has 22 real 64-byte headers as `headers.tsv`, `headers.json`, `rpf9_headers.py` and `rpf9_headers.hpp` (`reverse-toolkit/tools/rpf9-research/raw/cabeceras/README.md:1`). Headers only, no payload; a possible model for a header parse test, but write our own data.
- **Textures and audio shape**: `docs/TEXTURES.md`, `docs/AUDIO.md`.
- **Shaders**: the README reports `sgd5_shader_dxbc` streams and compares counts with other projects, xcl among them (`reverse-toolkit/tools/rpf9-research/README.md:162`). That table is the author's claim, not verified here.

## Prerequisites and install

Python 3.11 (CI pins 3.11: `reverse-toolkit/tools/rpf9-research/.github/workflows/tests.yml:15`). The `tools/` scripts use only the standard library (`reverse-toolkit/tools/rpf9-research/tools/resolve_path.py:11`). Nothing to install. Do not run code from the clone unreviewed; read it first.

Most commands need your own `.rpf` files and, for the later stages, an index produced by an earlier stage.

## Usage

Argument parsers: `reverse-toolkit/tools/rpf9-research/tools/rpf9.py:785`, `reverse-toolkit/tools/rpf9-research/tools/name_recover.py:864`, `reverse-toolkit/tools/rpf9-research/tools/index_full_sqlite.py:458`, `reverse-toolkit/tools/rpf9-research/tools/merge_index.py:902`, `reverse-toolkit/tools/rpf9-research/tools/resolve_path.py:474`.

```sh
# 1. Header and TOC status; --payload adds zlib/SGD5/nested-RPF9 spans, --json for machines
python3 tools/rpf9.py game/data.rpf --payload --json
python3 tools/rpf9.py game/data.rpf --scan-nested
# 2. Recover names from payloads (does not need the TOC); --validate re-inflates entries
python3 tools/name_recover.py game/data.rpf --out index.json --validate --limit-files 4
# 3. Consolidate slices (<rpf>__<base>.json) into streams.tsv, paths.tsv, ids.tsv, summary.json
python3 tools/index_full_sqlite.py slices/ out/ --keep-db
# 4. Merge indexes from several sources; each source declares its type
python3 tools/merge_index.py out2/ --source name_recover=slices/ --source paths=names.txt
# 5. From a manifest path to the bytes (needs the index from step 3)
python3 tools/resolve_path.py --index-dir out/ --rpf-dir game/ --rpf data --info "<path>"
python3 tools/resolve_path.py --index-dir out/ --rpf-dir game/ --verify 200 --rpf data
```

Step 5 seeks and inflates only the slice recorded in the index and checks it against the recorded inflated size (`reverse-toolkit/tools/rpf9-research/tools/resolve_path.py:1`). Tests: `python3 -m unittest discover -s tests -v` (`reverse-toolkit/tools/rpf9-research/.github/workflows/tests.yml:18`). Run from the repo root of the clone; the relative `tools/...` paths above assume that.

## Caveats

- Documentation and transcripts in `raw/` are mostly Spanish (the commands were run in Spanish); `docs/*.md` come in `.es.md` pairs (`reverse-toolkit/tools/rpf9-research/README.md:50`).
- The repo says the TOC is encrypted and records that it could not be read without a key (`docs/RPF9.md`). It discusses cipher research only as unresolved findings. The toolkit does not use keys.
- `tools/README.md` and `docs/DETECCION.md` are about an unrelated question (an IDE plugin and its CLI binary), and `agy_*`, `go_*`, `plugin_*` scripts belong to that. Skip them.
- The full path index is deliberately not published (`reverse-toolkit/tools/rpf9-research/README.md:66`); you must build your own from your own files.
- The author's index numbers come from one 209 GiB package; the stride and geometry formulas may differ on other builds.

## Update

```sh
./reverse-toolkit/update.sh update --only rpf9-research
```
