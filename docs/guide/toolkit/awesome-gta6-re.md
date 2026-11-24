# awesome-gta6-re

An index (an "awesome list") of open-source GTA VI reverse-engineering projects: archive explorers, texture and audio decoders, format specs and Xbox Series translation layers.

- Repo: https://github.com/Ahmet-Dedeler/awesome-gta6-reverse-engineering (branch `main`)
- Commit described: `fa51a4a`, 2026-11-23 (`reverse-toolkit/tools.lock`)
- Licence: CC0-1.0 (`reverse-toolkit/tools/awesome-gta6-re/LICENSE:1`). **Reference only**: it is a list of links, there is no code to use or copy.

## What it is for in our work

It is the map of the ecosystem. Use it to find which project handles which payload (RPF9 archives, textures, audio, models, shaders) before you pick a tool from this toolkit, and to see who else works on running Xbox Series titles on Windows (`reverse-toolkit/tools/awesome-gta6-re/README.md:83`).

It holds no code that XCL could use. XCL's own RPF9 and texture code lives in `crates/xcl/src/rpf.rs` and `crates/xcl/src/gpu/texture.rs`; neither depends on this list.

## Prerequisites and install

None. It is Markdown. Read `README.md` in an editor or on GitHub.

Only the banner graphics need a build step, and you do not need them: `reverse-toolkit/tools/awesome-gta6-re/AGENTS.md:7` says the PNGs in `assets/` are rendered from `assets/src/*.html` with `uv run assets/src/render.py` (script: `reverse-toolkit/tools/awesome-gta6-re/assets/src/render.py:1`). Do not run it from the clone.

## Reading guide

All sections are in `reverse-toolkit/tools/awesome-gta6-re/README.md`:

| Lines | Section | Use it for |
|---|---|---|
| 34 | Ecosystem Overview | Why Xbox Series (x86-64 Zen 2, RDNA2, `d3d12_xs`, `kernelx`) is the main target |
| 44 | RPF9 Archive & Asset Explorers | Candidates for opening archives |
| 64 | Texture (`.ttpf`) & Audio (`ADAT` / AWC) Tools | Texture and audio decoders |
| 83 | Console Translation Layers & Static Recompilation | Other runtimes (same problem space as XCL) |
| 113 | Format Documentation & Research | Specs and notes |
| 130 | Visualizers & Related Experiments | Viewers |
| 142 | Format & Architecture Glossary | Terms: RPF9, TTPF, BCPack, ADAT |
| 158 | Safety & Scope | What the list refuses to link |

`CONTRIBUTING.md` and `.github/ISSUE_TEMPLATE/submit-project.yml` describe how entries are submitted; they matter only if you want to add a project.

## Caveats

- Entries are curated by one maintainer; the status text goes stale quickly. Check the linked repo and its licence before relying on an entry.
- The list covers more than we use. The toolkit excluded some listed projects (those built around running the leaked executable, those that need an external decoder reading the executable, closed or unlicensed ones). Presence in the list is not an endorsement by this toolkit.
- The Safety section (`reverse-toolkit/tools/awesome-gta6-re/README.md:158`) warns about fake "GTA 6 PC" downloads and launchers. Build tools from source you have read; the toolkit works only on files from your own install.

## Update

```sh
./reverse-toolkit/update.sh update --only awesome-gta6-re
```
