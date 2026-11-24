# gta6-pc-port-hub

A coordination repository for the open-source efforts to run GTA VI (Xbox Series build) on Windows: a project status table, a shader census convention, the root-signature-fallback problem, a conformance corpus idea and a licence matrix. Docs and one JSON file; no code to run in earnest.

- Repo: https://github.com/fishingpvalues/gta6-pc-port-hub (branch `main`)
- Commit described: `3a7d8d1`, 2026-11-22 (`reverse-toolkit/tools.lock`)
- Licence: CC0-1.0 (`reverse-toolkit/tools/gta6-pc-port-hub/LICENSE:1`). Archived on GitHub (per `reverse-toolkit/tools.tsv`): read-only reference. Its status tables are third-party claims that include XCL; do not quote them as measurements of XCL.

## What it is for in our work

This is the page that talks about XCL's own problem space.

- **Shader census**: why the counts differ (26,503 vs 24,064 vs 25,727 ...) and a proposed unit, one PSIN entry per program record (`reverse-toolkit/tools/gta6-pc-port-hub/docs/shader-census.md:1`). Compare with XCL's pack scan in `crates/xcl/src/gpu/pack.rs`.
- **Root-signature fallback**: GTA VI mesh programs without an RTS0 chunk, and what a memory-based fallback would have to define (`reverse-toolkit/tools/gta6-pc-port-hub/docs/root-signature-fallback.md:1`). Related to XCL's draw-time root-arg to SGPR binding, which the hub lists as an open item (`reverse-toolkit/tools/gta6-pc-port-hub/README.md:40`).
- **Blockers**: README says nothing boots because the circulating package lacks the executable (`reverse-toolkit/tools/gta6-pc-port-hub/README.md:14`). Package inventory: `docs/package-inventory.md`.
- **Licences**: `reverse-toolkit/tools/gta6-pc-port-hub/docs/license-matrix.md:1` records that XCL is GPL-2.0-only and one-way incompatible with GPL-3.0 projects. This is the reason for the "reference only" rule on several tools.

## Prerequisites and install

Nothing for reading. `scripts/render_status.py` is standard-library Python (`reverse-toolkit/tools/gta6-pc-port-hub/scripts/render_status.py:1`).

## Reading guide

| Path | Covers |
|---|---|
| `reverse-toolkit/tools/gta6-pc-port-hub/README.md:30` | Projects table (runtimes and asset tools), generated from `data/projects.json` |
| `reverse-toolkit/tools/gta6-pc-port-hub/README.md:65` | Six coordination workstreams C-1 to C-6 |
| `docs/package-inventory.md` | Files in the package, no executable |
| `docs/shader-census.md` | C-2 shader counting convention |
| `docs/root-signature-fallback.md` | C-3 RTS0-less shaders |
| `docs/conformance.md` | C-4 shared fixtures, C-6 wave64-on-wave32 |
| `docs/license-matrix.md` | C-5 who can reuse whose code |
| `docs/owners.md`, `CONTRIBUTING.md` | Claims and ground rules (clean-room, cite sources) |
| `data/projects.json` | Machine-readable status with references |

## Usage

The only script rewrites the README tables from the JSON (`reverse-toolkit/tools/gta6-pc-port-hub/scripts/render_status.py:80`):

```sh
python3 reverse-toolkit/tools/gta6-pc-port-hub/scripts/render_status.py --check   # exits 1 if README is stale
```

`--check` only reads. Without it the script rewrites `README.md` in the clone; do not run that.

## Caveats

- Snapshot is dated 2026-11-22 (`reverse-toolkit/tools/gta6-pc-port-hub/README.md:33`). Archived, so it will not be updated; the numbers age.
- The shader and RTS0 counts come from other projects' issues, quoted second-hand. The hub itself says its census numbers disagree (`reverse-toolkit/tools/gta6-pc-port-hub/CONTRIBUTING.md:9`).
- Its hardware and conformance pages propose plans, not results.

## Update

```sh
./reverse-toolkit/update.sh update --only gta6-pc-port-hub
```
