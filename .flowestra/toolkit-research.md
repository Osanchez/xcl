# Toolkit research: 19 GTA VI / RPF9 reverse-engineering tools

Date of research: 2026-11-23/24. Read-only: nothing cloned, built or run. Metadata from `gh api repos/OWNER/NAME`, READMEs from `gh api repos/OWNER/NAME/readme`, languages from `/languages`. Every URL below was checked with `git ls-remote --symref https://github.com/OWNER/NAME HEAD` (HEAD sha recorded in each section). Stars/pushed are a snapshot and move fast (repos are days old).

Curated list found first: https://github.com/Ahmet-Dedeler/awesome-gta6-reverse-engineering (557 stars now; brief said ~516). It links 15 of the 19. NOT in it: RageForge, GTA VI PC port hub, xseriesrecomp, xbox-hle-lab.

## Summary table

| # | name | url | branch | lang | license | platform | verified |
|---|---|---|---|---|---|---|---|
| 1 | Rage Tools VI | https://github.com/lynxnathan/rage-tools-vi | main | Python, Rust, Shell | MIT | cross-platform (developed on Linux, uv) | yes |
| 2 | GTA VI PC port hub | https://github.com/fishingpvalues/gta6-pc-port-hub | main | docs (+ small Python) | CC0-1.0 (ARCHIVED) | n/a (docs) | yes |
| 3 | SixNG | https://github.com/holdmysocks/SixNG | master | Rust | GPL-3.0 | Windows, macOS | yes |
| 4 | PercyStar | https://github.com/rpatty-dev/PercyStar | main | Rust | custom all-rights-reserved (SPDX NOASSERTION) | Windows, macOS, Linux scripts | yes |
| 5 | rpf9-explorer | https://github.com/breadbbbb/rpf9-explorer | main | C# (+C++) | none | Windows | yes |
| 6 | bcpack-rs | https://github.com/Ahmet-Dedeler/bcpack-rs | master | Rust, Python | MIT OR Apache-2.0 (GH: NOASSERTION, two files) | cross-platform | yes |
| 7 | RageForge | https://github.com/zsy-miao/RageForge (content copy); https://github.com/nullbrik/RageForge (original, emptied) | master | C# | LGPL-3.0 | Windows x64 | yes (see section) |
| 8 | ttpf-studio | https://github.com/woreksami/ttpf-studio | main | Python | MIT | Windows (source: cross) | yes |
| 9 | GTA6-Asset-Info | https://github.com/TrinitronClient/GTA6-Asset-Info | main | docs + Python | GPL-3.0 | n/a (docs) | yes |
| 10 | awesome-gta6-RE | https://github.com/Ahmet-Dedeler/awesome-gta6-reverse-engineering | main | Markdown/HTML | CC0-1.0 per README (GH: NOASSERTION) | n/a | yes |
| 11 | gta6-explorer | https://github.com/HolyRoses/gta6-explorer | main | Python | MIT | cross-platform | yes |
| 12 | 6stars | https://github.com/meetox80/6stars | main | C++ (vendored ImGui), Swift, C | none | macOS 15 | yes |
| 13 | rage9-re | https://github.com/Ahmet-Dedeler/rage9-re | master | Rust, Python | MIT OR Apache-2.0 (GH: NOASSERTION) | cross-platform | yes |
| 14 | AWC from RPF9 | https://github.com/Albotterin/AWC_from_RPF9_Extractor | main | Python (+Java/Android) | MIT | Windows GUI; source portable | yes |
| 15 | 0x52504639 | https://github.com/SentinelInteractive/0x52504639 | main | Markdown + tiny Python/PS | MIT | n/a (docs) | yes |
| 16 | rpf9-research | https://github.com/JoshRob297/rpf9-research | main | Python, Shell, C++ | GPL-3.0 (README: GPL-3.0-or-later) | cross-platform | yes |
| 17 | Vice Lens | https://github.com/Librett0/vice-lens | main | Rust, JS | MIT | Linux, Windows | yes |
| 18 | xseriesrecomp | NOT_FOUND | - | - | - | - | no |
| 19 | xbox-hle-lab | https://github.com/sub-north/xbox-hle-lab | main | C++, Python, PowerShell | MIT (tools) / CC BY 4.0 (docs) / GPL-2.0 (patches) | Windows host + VM | yes |

Common context for all asset tools: every README says the RPF9 table of contents is encrypted and that the tool does NOT decrypt it (works by carving zlib streams). None bundles keys. Per README, the game package is "GTA VI Xbox Series 1.0.2.1".

---

## Rage Tools VI
- category: ARCHIVE TOOLKIT. Match: description "RPF9 extraction and analysis toolkit"; README "payload-verified entries, audio, PSIN/LSNS schema". Right project.
- repo: https://github.com/lynxnathan/rage-tools-vi ; branch main ; HEAD 51c4bd69c60cb70f334bce152a9ef9a5059f9815 ; verified yes
- stars 1 ; last push 2026-11-24T00:08:16Z ; license MIT ; langs Python 827K, Rust 180K, Shell, JS, C
- platform: cross-platform in principle; author paths are Linux (`/home/lynxnathan/gta6-source`); no Windows instructions
- prerequisites: `uv` (pyproject.toml, uv.lock; numpy for XMA2 wrapper), Rust/cargo for the CLI, MkDocs for docs. Has a submodule `rdna2-spirv/` (separate repo, README "Layout") so clone needs submodule handling. Needs ~209 GB of source archives for the corpus pass.
- build/install (README "Checking the repository", "Single-purpose tools"): `tools/check.sh` / `tools/check.sh --fast`; Rust CLI: `cargo build && target/debug/rage-tools-vi --help`
- usage (README "Using the names", "The corpus pass", "Single-purpose tools"):
  - `python3 tools/name_registry.py --lookup 0x01166164`
  - `tools/corpus_pass.sh /home/lynxnathan/gta6-source --entries`
  - `python3 tools/stream_index.py ARCHIVE --out output/index/name.tsv`
  - `python3 tools/extract_entry.py --archive A --kind zlib --start 0x0 --end 0x1000 --output out.bin`
  - `python3 tools/psin_extract.py script.rpf`
- inputs/outputs: reads `*.rpf`; writes TSV indexes, entry manifests, decoded audio streams (Opus/XMA2/ACE1), name registry `handoff/names.tsv`, PSIN schema. Manifests/hashes committed; archives not.
- relation to XCL: no xcl reference. Mentions rpf9 (core), ADAT/AWC, shaders (`rdna2-spirv` submodule). No bink/ttpf/bcpack focus. Credits Leonida, GTA6-Asset-Info, CodeWalker, GPUOpen isa_spec_manager.
- red flags: none seen. README: "The RPF9 **table of contents is encrypted**, so there are no original file names or paths". Corpus pass is built around a 161 GB archive set (no files bundled).

## GTA VI PC port hub
- category: COORDINATION HUB. Match: description "Coordination hub for running GTA VI (Xbox Series 1.0.2.1, RAGE 9) natively on Windows: package inventory, shader census convention, shared b..." Found via `gh api search/repositories?q=gta6 hub shader census...`. Right project.
- repo: https://github.com/fishingpvalues/gta6-pc-port-hub ; branch main ; HEAD 3a7d8d18f6fdc2071f8609f35153fb07ac25d1b5 ; verified yes
- stars 1 ; last push 2026-11-22T19:08:36Z ; license CC0-1.0 ; langs Python 3K (docs repo) ; **ARCHIVED = true** on GitHub
- platform: n/a (documentation + `scripts/render_status.py`)
- prerequisites: Python to run `scripts/render_status.py` (renders `data/projects.json` into the README status table). Nothing else.
- build/install: none. README "Projects": "Generated from `data/projects.json` by `scripts/render_status.py`. Edit the JSON, then run the script."
- usage: read-only reference. Docs: `docs/package-inventory.md`, `docs/shader-census.md`, `docs/root-signature-fallback.md`, `docs/conformance.md`, `docs/license-matrix.md`, `docs/owners.md`.
- inputs/outputs: JSON -> README table; markdown docs.
- relation to XCL: HIGH. Lists `ivall/xcl` (GPL-2.0-only; 24,064 shader programs translate); workstream C-5 says "`xcl` is GPL-2.0-**only**, which is one-way incompatible with GPL-3.0 `Leonida`/`xbox-emulator`" ; C-3 (root-signature-less shaders), C-6 (wave64-on-wave32, "xcl 749 programs at risk"). Also names the "shader census" denominators (26,503 / 25,727 / 21,522).
- red flags: none seen. README: "Clean-room only. No game executables, assets, keys, or DRM tooling live here". Note it states the package "does **not contain the game executable**", whereas the awesome list (line 164) says `game_scarlett_master.exe` leaked on 2026-11-23, so the hub's headline fact is stale.
- note: repo is archived, so no updates.

## SixNG
- category: ASSET EXTRACTOR. Match: description "Asset extractor for your own GTA VI install (Xbox Series, RPF9): CLI and desktop app; models to glTF, textures..." Right project.
- repo: https://github.com/holdmysocks/SixNG ; branch master ; HEAD 622f9e5a1aa04d66e29c11f9784cd08d97d1f9f7 ; verified yes
- stars 77 ; last push 2026-11-24T00:47:51Z ; license GPL-3.0 (README: "GNU General Public License v3.0 only"; AMD addrlib MIT notices) ; langs Rust 2.9M, WGSL, Shell
- platform: Windows (primary), macOS 11+ (README "Setup and build"); optional experimental Metal 4 path
- prerequisites: Rust 1.80+ stable; CMake (for libopus; without it build with `--no-default-features -p sixng-cli -p sixng-gui`); game folder via `--game`, `SIXNG_GAME`, or GUI wizard. Disk: README "Disk space" section (large work dir; collision output ~15 GB, batches ~5.8 GB, placements ~1.3 GB JSON).
- build (README "Setup and build"):
  - `cd SixNG`
  - `cargo build --release` -> `target\release\sixng.exe`, `target\release\sixng-gui.exe`
  - macOS: `scripts/macos-app.sh` builds `dist/SixNG.app`
- usage (README "CLI"):
  - `.\target\release\sixng.exe info`
  - `.\target\release\sixng.exe index levels textures packs data common`
  - `.\target\release\sixng.exe list levels --rtype txd --limit 20 --pretty`
  - `.\target\release\sixng.exe extract levels --rtype txd --name '^island' -o work\out --tex both`
  - `.\target\release\sixng.exe movies movies --out out`
  - also `placements`, `collision`, `navmesh`, `audio`, `ttpf`, `names`, `outfit`, etc.
- inputs/outputs: reads RPF9 archives (indexes by zlib carving); writes glTF/GLB, PNG/DDS, JSON/NDJSON, `.bk2` (carved, not decoded), Ogg Opus/WAV; work dir `./work`.
- relation to XCL: no xcl reference. Strong overlap with rpf9, ttpf, bcpack (decodes all six BC formats), bink (`movies` carves .bk2), shader/parameter name resolution (`sixng names`). Credited by PercyStar, bcpack-rs, ttpf-studio, gta6-explorer.
- red flags: none seen. README: "RPF9 TOC / file names | **not read**: the TOC is encrypted and SixNG does not decrypt it." README: "Game data never goes in git." Note: BCPack tables are "fitted" so "some blocks can be off". License GPL-3.0-only conflicts with vendoring into a GPL-2.0-only project (xcl).

## PercyStar
- category: ARCHIVE EXPLORER. Match: description "PercyStar - High-speed Rust RPF9 archive explorer, D3D11 Bink2 player, and ADAT Opus audio decoder". Right project.
- repo: https://github.com/rpatty-dev/PercyStar ; branch main ; HEAD 6a64e53f6d1c48aae250109232eebb02a2a68e36 ; verified yes
- stars 93 ; last push 2026-11-22T21:40:22Z ; license: GitHub SPDX NOASSERTION; README custom "PercyStar Attribution (@rpatty)" ; langs Rust 1.2M, HTML, Python, Shell, PowerShell
- platform: Windows x64 (primary), macOS universal (`scripts/build_macos.sh`), Linux (`scripts/build_linux.sh`)
- prerequisites: Rust 1.89+ (x86_64-pc-windows-gnu with MinGW windres/dlltool, or msvc); optional RAD `binkplay.exe` (via `scripts/setup_deps.ps1`), optional `dxcompiler.dll`; Python only for `scripts/awc_extract.py`, `rpf9_carve.py`.
- build (README "Building from Source"): `cargo test` ; `cargo build --release` -> `target/release/PercyStar.exe`
- usage (README "CLI Batch Exporters"):
  - `PercyStar.exe --extract-vehicles "<path-to-levels.rpf>" "<output-dir>"`
  - `PercyStar.exe --extract-models "<archive.rpf>" "<output-dir>" "filter1,filter2"`
  - `PercyStar.exe --dump-textures "<archive.rpf|.ttpf>" "<output-dir>" [limit]`
  - `PercyStar.exe --disasm-shaders "<shaders.rpf|zlib_dir|.ps09>" "<output-dir>" [limit]`
  - `PercyStar.exe --extract-sgd5 "<shaders.rpf|SGD5.bin>" "<output-dir>" [limit]`
- inputs/outputs: .rpf/.ttpf/.ps09; outputs OBJ/GLB, PNG/DDS, WAV, HLSL/SPIR-V/ASM, XML. Scan cache `%LOCALAPPDATA%\PercyStar\scan_cache_v8.tsv`.
- relation to XCL: no xcl reference. Heavy overlap on shaders: RDNA2 GFX10.3 disassembler + HLSL SM 6.6 / SPIR-V lifter (`src/rdna2.rs`, `PS\x00\x09` PSO cache, SGD5, DXBC). Also rpf9, ttpf, bink (docks binkplay), BCPack. Companion repo PercyScarlett (Xbox->Windows layer, non-commercial license).
- red flags: LICENSE blocks vendoring. Quote (README end): "Copyright (c) 2026 rpatty-dev - All Rights Reserved. ... you may not modify, alter, or redistribute modified versions of the code or binaries without explicit written permission." Releases ship prebuilt binaries (`PercyStar.exe`, `PercyStar_v1.5.2_Win64.zip`) and an in-app auto-updater ("Windows users on v1.0.3+ can click the golden Update Available pill ... 1-click self-update"). README claims "Zero Circumvention / Zero Keys". README also lists `--inspect-exe <example_binary.exe>` that inspects PE64/GDK imports. Version string inconsistency inside README (v1.5.1 badge, v1.4.7 download link, release v1.5.2).

## rpf9-explorer
- category: ARCHIVE EXPLORER. Match: description "Windows RPF archive explorer with model, texture, audio and video previews, asset export, and saved replacemen[t projects]". Right project. The blurb "Leonida builds on its decoders" could NOT be confirmed (Leonida's README has no mention; the claim may be in an issue/doc).
- repo: https://github.com/breadbbbb/rpf9-explorer ; branch main ; HEAD af549336624fdabf7d1bb6f00b8783e7e1ef692e ; verified yes
- stars 78 ; last push 2026-11-23T23:38:12Z ; license **none** (no LICENSE detected; the README has only third-party MIT notices for AMD addrlib and Microsoft silicon-rans) ; langs C# 966K, C++ 32K, PowerShell
- platform: Windows (WPF)
- prerequisites: .NET 10 SDK; optional media helpers need Visual C++ + Windows SDK; Bink needs a "separately installed licensed Unreal Bink SDK"; Opus needs libopus; optional FFmpeg build (`--disable-network`, `file,pipe` protocols).
- build (README "Build from source"): `dotnet build ./App -c Release`; PowerShell publish script in repo root makes self-contained x64 builds under `dist`.
- usage (README "Command line"):
  - `rpf9-cli search $source -q "type:model"`
  - `rpf9-cli export $source -t textures -q "name:glass" -o "$out\textures"`
  - `rpf9-cli export $source -q "<term>" --best --merge assembly -o "$out\assembly"`
  - `rpf9-cli kits $source --complete`
  - `rpf9-cli scan $source --kinds textures,psin`
- inputs/outputs: .rpf/.ttpf/.zip (up to 512 sources/workspace); outputs DDS/PNG textures, OBJ+MTL models, WAV, stored files, JSON manifests; `.rpfworkspace` project files (replacements staged, never written into archive).
- relation to XCL: no xcl reference. rpf9, ttpf, BCPack (BC1/3/4/5; BC6H/7 unsupported), Bink and PSIN. Hub says it has no licence (blocks reuse).
- red flags: **decoder-bridge that reads the game executable.** docs/rpf9-index-provider.md: "Use File > Configure local RPF9 decoder to select a configuration for a compatible local decoder you trust" ; config JSON has `"executable": "<game executable path>"` ; "The configured script reads the executable as data and uses its own compatible format decoder." ; the app sets `RPF9_EXPLORER_EXTENDED_STORAGE=1` advertising "layout 4 chunks, encrypted resource ch[unks]". The decoder itself is NOT in the repo ("The public source and app package contain the integration and recovery scanner..."), i.e. the indexed mode depends on an external, unspecified script that decodes the encrypted index. Treat as DRM/crypto-adjacent. The app also executes that Python script. Windows build ships helper executables (`Tools\`) not in the source ZIP.

## bcpack-rs
- category: TEXTURE CODEC. Match: description "Clean-room decoder for Xbox Series BCPack (MDU rANS) textures. BC1 decodes to PNG/DDS (Rust CLI); BC7 and other..." Right project.
- repo: https://github.com/Ahmet-Dedeler/bcpack-rs ; branch master ; HEAD c51ec521cbe6869136f29f5d157342374ef5cf33 ; verified yes
- stars 11 ; last push 2026-11-23T01:25:53Z ; license "MIT OR Apache-2.0" (README; repo has LICENSE-MIT and LICENSE-APACHE; GH SPDX NOASSERTION) ; langs Rust 139K, Python 78K
- platform: cross-platform
- prerequisites: Rust/cargo (features `ttpf` -> flate2, png; `cli` default -> rayon); Python 3 stdlib-only reference decoder.
- build (README "Usage", "Tests"): `cargo build --release` ; `cargo test --release` ; `python3 -I python/bcpack.py selftest`
- usage (README "Usage"):
  - `bcpack decode-bc1 chunk.bin 4096 out.bc1`
  - `bcpack symbols chunk.bin 4096`
  - `bcpack ttpf-export pool.ttpf 318 out.png`
  - `bcpack scan --jobs 8 --every 10 pools/*.ttpf`
  - `bcpack bench chunk.bin 4096`
  - library: `bcpack-rs = { git = "https://github.com/Ahmet-Dedeler/bcpack-rs", default-features = false }`
- inputs/outputs: BCPack chunk bytes or `.ttpf` stores -> BC1 blocks, PNG, DDS. Ships fitted tables `data/bc1_window.json`, `data/bc1_quad_table.json`. Only BC1 decodes in this crate (BC7/BC6H/BC5/BC4/BC3 entropy solved on research branches, block mapping not merged).
- relation to XCL: no xcl reference. bcpack + ttpf (RDS5/ETPF reader, Gfx10 detiling). Credits SixNG, Xbox2Windows, Leonida, ttpf-viewer.
- red flags: none seen. README: "The repository contains no game data; all tests use synthetic streams." Derived from Microsoft `silicon-rans` Verilog (MIT) and cites patent US11234023B2 (provenance, not a legal opinion). BC1 tables are "Fitted", not exact. Discord link in README.

## RageForge
- category: RE TOOLKIT. Match: both candidates have description "Reverse engineering toolkit for RAGE engine archives (RPF9/TTPF). Extract, explore and analyze GTA VI game assets." (the blurb is the same). A third unrelated `SimpleX-T/RageForge` (2025, empty description) and several WoW/other repos were rejected.
- AMBIGUITY: two repos with identical description and 30-commit history authored entirely by "nullbrik":
  - **https://github.com/nullbrik/RageForge** (original; created 2026-11-21T12:33Z, pushed 2026-11-23T17:16Z, 2 stars, HEAD 39a914531342f98582aac0d1a8f5479991fc4f21, branch master). The tree is now only `COPYING LICENSE README.md`; README says "### Project Status: Temporarily Closed ... we have decided to **temporarily suspend public access to RageForge**, including its source code and releases." Latest commits: "Add license files", "Add closure notice", "Empty repository".
  - **https://github.com/zsy-miao/RageForge** (content copy; created 2026-11-21T19:23Z, pushed 2026-11-22T04:19Z, 0 stars, HEAD 733580daec85d3361e9e1b208d85efe2ab246bbf, branch master, not marked as a GitHub fork). Holds the full source (src/, docs/, RageForge.slnx, .github). It has no releases listed via the API. This is a third-party re-upload of a repo the author deliberately closed (provenance/ethics issue; the `Download` section links `RageForge-gui-win-x64.zip` releases that are not present).
- verified yes for both (ls-remote OK). Document below describes the zsy-miao copy because the original has no code.
- license LGPL-3.0 (README: "LGPL-3.0-only ... supplements the GNU GPL v3.0"); langs C# 514K ; platform Windows x64 (GUI + CLI, README says self-contained)
- prerequisites (README "Build from source"): .NET 10 SDK
- build: `dotnet build RageForge.slnx -c Release`
- usage (README "CLI"):
  - `rageforge index <gameRoot>`
  - `rageforge search <gameRoot> <text> [n]`
  - `rageforge export <gameRoot> <name> <out>`
  - `rageforge ttpf dump <store.ttpf> <out>`
  - `rageforge shaders <shaders.rpf> [n] [--hlsl]`
  - `rageforge info <archive.rpf>`
- inputs/outputs: game root of .rpf/.ttpf; exports .glb, PNG, WAV, .bk2, text, raw; interiors/map blocks to .glb.
- relation to XCL: no xcl reference. rpf9, ttpf (Gfx10/RDNA2 detiling + BC1-BC7), Bink 2 (extract .bk2, plays via RAD `binkplay`), shaders (RDNA2 pipeline cache disassembly, pseudo-HLSL). PercyStar credits nullbrik/RageForge for UX.
- red flags: README (zsy-miao) "Status": "The RPF9 table of contents (file names and tree) is encrypted (AES-CBC) and its key is not yet public, so archive contents are read by carving rather than by the directory." Original README closure text: "game files circulate through many different channels, including unauthorized leaked builds that raise serious copyright and intellectual property concerns." The author closed the project; the zsy-miao copy circumvents that wish. LGPL-3.0 incompatible with GPL-2.0-only for vendoring.

## ttpf-studio
- category: TEXTURE TOOL. Match: description "Unofficial GTA 6 texture, archive and video tool: view/export/replace .ttpf textures, recover files from RPF9 ..." Right project.
- repo: https://github.com/woreksami/ttpf-studio ; branch main ; HEAD a2fe110d8e7aeb8ed3be37c365c75078a408c089 ; verified yes
- stars 11 ; last push 2026-11-23T11:06:22Z ; license MIT ; langs Python 335K
- platform: Windows (prebuilt zip); source says Python 3.10+ (awesome list: cross-platform)
- prerequisites (README "Running from source"): Python 3.10+, `pip install -r requirements.txt`; OpenGL 3D view (software fallback); optional RAD Video Tools (https://www.radgametools.com/bnkdown.htm) and ffmpeg for video conversion.
- build/install: `git clone https://github.com/woreksami/ttpf-studio` ; `cd ttpf-studio` ; `pip install -r requirements.txt` ; `python ttpf_tool.py` ; `python ttpf_tool.py --install`
- usage (README "Running from source"):
  - `python ttpf_tool.py --dump <packs folder> <output>` (add `--rpf` for archive contents)
  - `python ttpf_tool.py --rpf <archive.rpf>`
  - `python ttpf_tool.py --selftest`
  - `python ttpf_tool.py --glcheck`
- inputs/outputs: `.ttpf` packs, `.rpf`; exports PNG/DDS, OBJ, glTF/OBJ map, MP4 (via ffmpeg); can replace a texture and save a modded copy of the pack.
- relation to XCL: no xcl reference. ttpf, bcpack (credits bcpack-rs), bink (videos), models/map. Credits SixNG, RageForge (nullbrik), 6Stars.
- red flags: README "Getting started": "Download **`TTPF-Studio-windows.zip`** from the latest release ... Run **`TTPF Studio.exe`**" and "The app isn't code-signed, so Windows warns ... Click **More info -> Run anyway**" (unsigned prebuilt binary; also `Add desktop shortcut.bat`). Ability to "replace a texture and save a modded copy of the pack" (modding). README "File names | The archive file list is encrypted and this tool doesn't decrypt it". README Legal: "contains no game files, game assets or encryption keys".

## GTA6-Asset-Info
- category: FORMAT NOTES. Match: description "Format notes and tool guide for SixNG, an asset extractor for GTA VI (Xbox Series): RPF9, textures, BCPack, mo[dels]..." Right project. (README says 17-part spec; awesome list says 19-part.)
- repo: https://github.com/TrinitronClient/GTA6-Asset-Info ; branch main ; HEAD c62be04cf9fc406a8c0028a87edc1208b3bab99b ; verified yes
- stars 56 ; last push 2026-11-24T00:43:16Z ; license GPL-3.0 (notes only) ; langs Python 40K
- platform: n/a (markdown docs; `tool/README.md` documents SixNG commands)
- prerequisites: none (read only)
- build/install: none
- usage: read `formats/README.md` (index), `formats/01-rpf9.md`, `formats/05-ttpf.md`, `formats/06-bcpack.md`, `structure/00-INDEX.md`, `tool/README.md`. README has no commands.
- inputs/outputs: documentation only; confidence tags confirmed/inferred/unknown per field.
- relation to XCL: no xcl reference. rpf9, ttpf, bcpack, models, map, PSO, AWC/ADAT, video (Bink 2), shaders (cache layout, shader names; "decoding to a readable form" listed as not yet).
- red flags: none seen. README: "This repository holds documentation only: no game files, no extracted assets, no keys." Read LEGAL.md (not fetched). README says as of 2026-11-21 from one retail install.

## awesome-gta6-RE
- category: CURATED LIST. Match: description "Curated list of open-source GTA VI (GTA 6) reverse engineering tools, RAGE 9 / RPF9 archive explorers, TTPF texture & ADAT/AWC audio decoders, format docs, and Xbox Series / PS5 translation layers." Repo name is `awesome-gta6-reverse-engineering` (found via `gh search repos "awesome-gta6-RE"`).
- repo: https://github.com/Ahmet-Dedeler/awesome-gta6-reverse-engineering ; branch main ; HEAD fa51a4a421b804775e2f817e8f9697415382816b ; verified yes
- stars 557 (brief ~516) ; last push 2026-11-23T16:56:11Z ; license CC0-1.0 per README badge (GH SPDX NOASSERTION) ; langs HTML 8.8K, Python 2.6K
- platform: n/a
- prerequisites/build: none
- usage: README (189 lines) tables per category: Archive explorers, Texture/Audio/Video, Console translation layers, Format docs, plus bullets for weather-study, bink-player-mac, and "See also PortWatch VI". Submit via PR or issue form.
- inputs/outputs: list only.
- relation to XCL: lists `ivall/xcl` (Rust, GPL-2.0, "Wine-style compatibility layer").
- red flags: none. README has a "Safety & Scope" section: "**No playable GTA VI for PC exists.** Any download, installer, or launcher that says otherwise is fake or malware." Excluded: "decryption keys or key material". Line 164: "The game executable (`game_scarlett_master.exe`) leaked on 2026-11-23" (context for xbox-hle-lab, GTA6-BootTest, G6Recomp which use the leaked exe).

## gta6-explorer
- category: ASSET EXPLORER. Match: description "Read-only, dependency-free Python toolkit for exploring GTA VI Xbox packages and extracting textures, audio, v[ideo]..." Right project. Python package name is `gta6x`.
- repo: https://github.com/HolyRoses/gta6-explorer ; branch main ; HEAD d78a9a5b7f26739b54c56e198c00204a341ed113 ; verified yes
- stars 7 ; last push 2026-11-23T22:02:33Z ; license MIT (plus THIRD_PARTY_NOTICES.md) ; langs Python 1.58M
- platform: cross-platform
- prerequisites (README "Quick start"): Python 3.10+, stdlib only; optional system libopus (WAV export); optional SixNG executable via `GTA6X_SIXNG` / `--backend` for models and world exports.
- build/install: none ("No installation or build step is required").
- usage (README "Quick start", "Common workflows"):
  - `export GTA6_ROOT="/path/to/GTA6.Xbox.v1.0.2.1"`
  - `python3 -m gta6x info`
  - `python3 -m gta6x textures --pool vehicles --max 20 --out ./exports/textures --png`
  - `python3 -m gta6x awc --archive audio --limit 64 --ogg-out ./exports/audio`
  - `python3 -m gta6x movies --archive movies --limit 64 --out ./exports/movies`
  - `python3 -m gta6x shaders --metadata --out ./exports/shaders`
  - `python3 -m gta6x catalog --archive levels --limit 64 --db gta6.db`
- inputs/outputs: package folder; outputs DDS/PNG, Ogg Opus, `.bk2`, DXBC shaders, JSON/SQLite catalogs, HTML browser.
- relation to XCL: no xcl reference. rpf9, ttpf/bcpack (partial; BC6H/BC7 unsupported), bink (extract), shaders (DXBC extraction + metadata). Thanks SixNG, rpf9-explorer, Leonida.
- red flags: none seen. README: "the RPF9 index is not decrypted" ; "no game data, keys or external executables are included here".

## 6stars
- category: WORLD VIEWER. Match: description "[Metal] Independent engine for GTA VI"; README "A native macOS engine in Swift and Metal that streams the world straight out of the game's RPF9 archives". Right project (note: `Ahmet-Dedeler/Percy6Stars` is a different project, a PercyStar fork).
- repo: https://github.com/meetox80/6stars ; branch main ; HEAD 480d9a4d00ed11560f7b4d989643723a0f5c451c ; verified yes
- stars 16 ; last push 2026-11-22T12:37:16Z ; license **none** ; langs C++ 3.9M (vendored cimgui/ImGui), Swift 612K, C 536K, Python 103K
- platform: macOS 15 only
- prerequisites (README "Build"): macOS 15, Xcode command-line tools (Swift 6, Metal); archives expected at `~/testing` (`levels.rpf`, `textures.rpf`, `common.rpf`, `data.rpf`, `data/ttdb`, `pack/`); caches in `~/testing/6stars-data`.
- build: `make` ; `make rpftool` ; `make app` (~/testing/6stars.app, ad-hoc signed) ; `make run ARGS="--snapshot out.png"` ; `make clean`
- usage: `.build/release/6stars` ; `.build/release/6stars --snapshot out.png --camera x,y,z,yaw,pitch` ; `rpftool` is the format-research CLI (`Sources/rpftool`). Controls: WASD, Shift, scroll.
- inputs/outputs: reads RPF9 archives; renders live window / PNG snapshot. `Sources/RPFKit` is a Swift RPF9/texture/PSO reader.
- relation to XCL: none. rpf9, textures, PSO metadata; no bink/ttpf/shader-recompile mention in README.
- red flags: none seen. README: "This repository does not contain the game, its files, keys or an executable." Vendored `Sources/CImGui`. No license file (cannot be vendored safely).

## rage9-re
- category: DATA DECODER. Match: description "Decoder for RAGE9 script.rpf and anim.rpf (PSO script data, crAnimation channels) with glTF 2.0 export. Rust + Python". Right project.
- repo: https://github.com/Ahmet-Dedeler/rage9-re ; branch master ; HEAD ddf30a3d1493ab2918cbb2484d9e089d678feb94 ; verified yes
- stars 2 ; last push 2026-11-23T01:25:49Z ; license "MIT OR Apache-2.0" (README; GH SPDX NOASSERTION; hub lists "NOASSERTION (see repo)") ; langs Rust 115K, Python 44K
- platform: cross-platform
- prerequisites: Rust/cargo; Python 3 (package `python/rage9_re`)
- build (README "Usage", "Tests"): `cargo build --release` ; `cargo test` ; `python3 -m unittest discover -s tests -v`
- usage (README "Usage"):
  - `./target/release/rage9-re inspect-script /path/to/script.rpf --dump-xml 0 --dump-objgraph 0`
  - `./target/release/rage9-re inspect-anim /path/to/anim.rpf --export-glb out.glb`
- inputs/outputs: `script.rpf`, `anim.rpf` -> XML/JSON DOM, object graph, `.glb` clips. Known gaps: partial skeleton, ~1% frames with jumps, anim.rpf read in windows.
- relation to XCL: none. rpf9, PSIN/PSO. No ttpf/bcpack/bink/shaders.
- red flags: none seen. README: "Written from scratch from observed file structure; no game code or data is included."

## AWC from RPF9
- category: AUDIO EXTRACTOR. Match: description "A Tool to Extract Music Streams from RPF9 Files". Right project (repo name `AWC_from_RPF9_Extractor`).
- repo: https://github.com/Albotterin/AWC_from_RPF9_Extractor ; branch main ; HEAD 2e5ac99faa00e623942dacc1c9063e01f0760375 ; verified yes
- stars 4 ; last push 2026-11-23T23:12:56Z ; license MIT ; langs Python 219K, Java 94K (Android app), Batchfile
- platform: Windows GUI (tkinter); core modules portable Python
- prerequisites (README "Requirements & installation"): Python 3 with tkinter (tested 3.14); optional Pillow (station logos); ffmpeg + ffplay for playback and FLAC/WAV/MP3 export (Opus remux needs none).
- build/install: `git clone https://github.com/Albotterin/AWC_from_RPF9_Extractor.git` ; `cd AWC_from_RPF9_Extractor` ; `python awc_tool.py` (or `Start AWC-Tool.bat`). Standalone exe: `build_exe.bat` (PyInstaller).
- usage (README "Usage"):
  - `python awc_tool.py "D:\game\audio.rpf"`
  - `python awc_tool.py "D:\game"`
  - `python awc_tool.py --lang=de`
  - AWC Radio: `awc_radio.py` / `Start AWC-Radio.bat`
- inputs/outputs: `.rpf`/`.awc`; scans for `ADAT` headers (28 GB audio.rpf ~6 min, cached in `%APPDATA%\AWC-Tool\cache`); outputs Ogg Opus (lossless remux), FLAC, WAV, MP3, m3u8, tags; Android station pack `.dat`.
- relation to XCL: none. rpf9, AWC/ADAT only; uses Leonida's texture decoder for station logos.
- red flags: (1) network fetches at runtime: "On the first start a dialog offers to download the decoder of the Leonida project (a few files from GitHub, GPL-3 ... saved to `%APPDATA%\AWC-Tool\leonida\`)" and "Songs from the credits page downloads the soundtrack list of ... rockstargames.com/VI/credits". (2) Releases carry `AWC-Radio.apk` and likely an `.exe` ("Download (Releases)"); README: "The `.exe` and ffmpeg are intentionally **not** part of this repository." (3) Android "AWC Radio" station packs: "It holds the songs of those stations in the game's own quality ... about 3 GB for all 26 stations" (audio redistribution risk, README Legal notice says redistributing is generally not allowed).

## 0x52504639
- category: FORMAT RESEARCH. Match: description "RPF9 format research notes, catalogues and tooling for GTA VI. Own-copy research; no game assets." Right project (name = ASCII hex of "RPF9").
- repo: https://github.com/SentinelInteractive/0x52504639 ; branch main ; HEAD 625e2eb27280bea49fd2c31726388c6aaf6ef68e ; verified yes
- stars 6 ; last push 2026-11-23T21:31:39Z ; license MIT ; langs Python 10K, PowerShell 2K
- platform: n/a (docs; PowerShell publish script)
- prerequisites: none; `scripts/publish.ps1`, weekly workflow `.github/workflows/refresh.yml` only validates JSON.
- build/install: none
- usage: read `docs/rpf9-container.md`, `docs/ttpf-rds5-etpf.md`, `docs/bcpack-entropy.md`, `docs/cipher-constraints.md`, `docs/placement-resources.md`, `data/counts.md`, `data/catalog-summary.json`. No CLI examples in README.
- inputs/outputs: markdown + JSON aggregates (4,330 RPF9 headers; 357,115 texture descriptors).
- relation to XCL: none. rpf9, ttpf (RDS5/ETPF), bcpack (entropy), no bink/shaders.
- red flags: crypto-adjacent research: README status checklist has open items "[ ] Table cipher key" and "[ ] Exact cipher identity"; doc `docs/cipher-constraints.md` ("What the table cipher is not"); "[x] GTA V NG cipher ruled out against the RPF9 tables". README: "Not in this repository: ... Keys, key material, or anything about recovering an executable." Mentions "2,067,172 bin streams in a local zstd store" (the authors keep a local extracted copy; not published).

## rpf9-research
- category: FORMAT RESEARCH. Match: description "Ingenieria inversa del contenedor RPF9 de GTA VI (Xbox Series): cabecera, TOC y tabla de nombres". Right project. README is English with `README.es.md` Spanish pair.
- repo: https://github.com/JoshRob297/rpf9-research ; branch main ; HEAD 1de30d73a9f5118356190faf37527ec5fa1907e1 ; verified yes
- stars 2 ; last push 2026-11-23T19:00:49Z ; license GPL-3.0 (GH) / "GPL-3.0-or-later" (README) ; langs Python 296K, Shell 10K, C++ 9.7K
- platform: cross-platform (Python)
- prerequisites: Python (not stated); package of 209 GiB / 22 `.rpf` + 57 `.ttpf` needed for runs.
- build/install: none stated
- usage (README "Merging indexes", "From a path to the bytes"):
  - `tools/merge_index.py <chunks_dir> <output_dir> --skip packs_1.json,packs_2.json,packs_3.json`
  - `tools/merge_index.py <output_dir> --source name_recover=<dir_or_json> --source paths=<names.txt|names.json>`
  - `tools/resolve_path.py --index <idx> --containers <dir> --path "<path>" --out asset.bin`
  - `tools/resolve_path.py --index <idx> --containers <dir> --verify 200 --rpf data`
  - (tools also: `tools/rpf9.py`, `tools/name_recover.py`)
- inputs/outputs: .rpf/.ttpf -> `index.tsv`, `streams.tsv`, `paths.tsv`, `ids.tsv`, `report.json`; resolver returns raw asset bytes. Full path index withheld; only salted hashes published.
- relation to XCL: YES (reference). README table: "ivall/xcl | compatibility layer, RDNA2 shaders to DXIL | 25,043 `sgd5_shader_dxbc` streams, 100% of them named, against the 24,064 and 26,503 figures published elsewhere". License note: "Projects under GPL-2.0-only (`xcl`, `americas-recomp`) and non-commercial licenses (`PercyStar`) remain reading references, never a source of code." Also rpf9, ttpf, shaders; compares with rpf9-extract, PercyStar, rpf9.
- red flags: goal text "lists entries if it manages to decrypt it" and README: "In GTA V the RPF7 table of contents is AES-encrypted and public tools ... know that key. For RPF9 there is no public tool." (research toward TOC decryption; no key published). `docs/DETECCION.md` refers to an "Antigravity plugin". `raw/` holds 64-byte header slices (README: "no container bytes beyond 64-byte headers"). The repo derives 168,660 recovered paths but "The full path index is not published".

## Vice Lens
- category: ASSET EXPLORER. Match: description "An unofficial, open-source explorer for the files of GTA VI (Xbox Series data): map, 3D models, vehicles, peop[le]..." Right project (awesome list slug `vice-lens`).
- repo: https://github.com/Librett0/vice-lens ; branch main ; HEAD 8b8c8a847ef926d8d6298ecfade142db19cfce5d ; verified yes
- stars 4 ; last push 2026-11-22T20:35:26Z ; license MIT ; langs Rust 881K, JavaScript 213K, CSS, HTML, Shell
- platform: Linux and Windows (README)
- prerequisites (README "Getting started"): Rust stable (2024 edition); Linux: WebKitGTK 4.1 + GTK 3 dev packages (`libwebkit2gtk-4.1-dev libgtk-3-dev`) and a Vulkan driver (wgpu); game folder (with `levels.rpf`, `data.rpf`, `textures.rpf`...); building caches takes "about an hour on a fast disk; about 2 GB".
- build/run (README): `./vice-lens.sh` ; `./vice-lens.sh --serve` (browser, http://127.0.0.1:7878) ; `./vice-lens.sh --build-caches --game /path/to/your/game/folder`. Env: `VICE_LENS_GAME`, `VICE_LENS_PROBE=1`, `VICE_LENS_CAPTURE=DIR`.
- usage: GUI tabs Map, Models, Textures, Video, Sound, Files, Vehicles, People, Animations, Text, 3D world (Vulkan). Exports glTF/GLB, PNG, MP4/.bk2, Opus, TSV.
- inputs/outputs: reads game folder in place; caches (docs/caches.md) stay local; exports as above.
- relation to XCL: none. rpf9; textures (BCPack: "textures whose top level is BCPack (11% of them, drawn grey)" not decoded); bink (plays via FFmpeg with bink2video decoder); no shader recompilation.
- red flags: **bundled binaries from a separate project.** README "Getting started": "Download Vice Lens for Linux or Windows from the releases ... Everything it needs comes in the archive, the FFmpeg that plays the game's Bink 2 movies included." Licence section: the releases carry `ffmpeg` "under the GNU Affero General Public License 3.0 or later (`FFMPEG-LICENSE.txt` ...), and its complete source ... is Librett0/vice-lens-ffmpeg" (a custom FFmpeg fork with a Bink 2 decoder, AGPL-3). Releases: `vice-lens-linux-x86_64.tar.gz`, `vice-lens-windows-x86_64.zip`. Source build does not need them.

## xseriesrecomp
- category: FRONT END. Blurb: "one launcher over bundled snapshots of four tracked projects."
- **NOT_FOUND.**
- searches run (all returned nothing relevant): `gh search repos "xseriesrecomp"`; `gh api search/repositories?q=xseriesrecomp`; `XSeriesRecomp in:name,description`; `xseriesrecomp in:name`; `xseriesrecomp in:readme`; `xseries recomp`, `xseries-recomp`, `xseries launcher in:name`, `xseries in:name recomp`; `xbox series recomp launcher`, `xbox series recomp frontend`, `unified launcher xbox series recompilation projects`, `launcher bundled snapshots xcl OpenScarlett Leonida/xvalkyrie/Xbox2Windows`, `recomp launcher xcl xvalkyrie Xbox2Windows OpenScarlett`, `front end launcher xcl Leonida OpenScarlett xbox-emulator`; `gh api search/code?q=xseriesrecomp` (only hit: `4lwayz/portwatch-vi data/tracker.json`, a case-insensitive match on "anyxboxseries", not this tool); WebSearch `"xseriesrecomp"` and `xseriesrecomp github launcher Xbox Series recompilation GTA VI` (nothing). Also scanned the awesome list README, the hub README, the PortWatch tracker (`data/tracker.json` summaries for launcher/frontend/snapshot/bundle) and xcl's local markdown: no mention. May be private, deleted, renamed, or under another name; needs the owner/URL from the requester.
- fields (stars, license, etc.): n/a

## xbox-hle-lab
- category: TEST TOOLS. Match: description "Xbox Series to Windows runtime lab: xcl patches, crash-time probes, verified first-run notes and falsified hyp[otheses]" (README: "GPU probes, VM scripts and patches for XCL"). Found via `gh search repos "xbox-hle-lab"`. Right project.
- repo: https://github.com/sub-north/xbox-hle-lab ; branch main ; HEAD ead6762e1feed252a9e7cb32d3a851fe847a130a ; verified yes
- stars 0 ; last push 2026-11-24T00:44:08Z ; license per-scope: `tools/**` MIT, `docs/**` CC BY 4.0, `patches/xcl/**` GPL-2.0 (GH SPDX says MIT) ; langs C++ 21.6K, Python 6.9K, PowerShell 3.9K
- platform: Windows host (MSVC x64 probes; PowerShell); VM automation via "App Sandbox VM" (`asb` SDK), guest Windows ISO
- prerequisites (README "Quickstart"): MSVC 2019/2022 x64 Native Tools prompt; Python for `tools/vm/*.py`; `$env:LAB = "C:\lab"` with `tools\appsandbox\extracted\headless-api` (App Sandbox `asb` SDK), `iso\Windows.iso`, `vm\appsandbox-vm.json` (generated, git-ignored); git on PATH for the watcher. GPU-PV VM.
- build (README "Quickstart" 1), in `tools/gpu_probe`:
  - `cl /nologo /O2 /MT /EHsc main.cpp /link d3d12.lib dxgi.lib user32.lib`
  - same for `uma_probe.cpp`, `present_probe.cpp`, `rootsig_check.cpp` (`/link d3d12.lib user32.lib`)
- usage:
  - `gpu_probe.exe` (adapters, VRAM, feature level, wave lanes, tiers)
  - `uma_probe.exe` (4 GiB-aligned 2 GB region as D3D12 heap)
  - `present_probe.exe 12 0` ; `rootsig_check.exe blob.bin`
  - `python tools\vm\create_vm.py` ; `python tools\vm\assign_gpu.py` ; `python tools\vm\vm_exec.py "cmd /c ver" 60` ; `python tools\vm\push_file_cmd.py build\gpu_probe.exe C:/lab/gpu_probe.exe`
  - `powershell -File tools\watch\sweep.ps1` (head/delta watcher of upstream repos; needs network)
- inputs/outputs: probes print adapter/heap/rootsig results; VM scripts create/drive a guest; watcher writes `notes\watch\state.json`, `latest.json`, `deltas.md` (git-ignored); `patches/xcl/` = GPL-2.0 patches against ivall/xcl (d3d12_xs VM config, xmem virtual alloc).
- relation to XCL: HIGH. Patches against `ivall/xcl` ("distributed as patches against the upstream GPL-2.0 project, not as a fork"); status says xcl binds the title's HLE imports and reaches title init code; blocker `STATUS_ILLEGAL_INSTRUCTION` (0xC000001D) after 91 compute pipelines built. Shaders: root-signature deserializer oracle (`rootsig_check`). Does not touch rpf9/ttpf/bink.
- red flags: runs the (leaked) GTA VI executable inside xcl in a VM: README Status "The title's own code executed under xcl" ; "the run with the complete payload builds 91 compute pipelines" (title payload is not in repo: "Nothing here contains game binaries, game assets, decryption keys, or leaked packages"). Principle: "No host execution of untrusted console binaries; VMs only." README "Cleared batches are published by a deterministic runner and merged automatically when the pull request head is exactly the cleared commit" (automated agent-driven repo; xcl's own CONTRIBUTING forbids unattended-agent PRs). The VM scripts need a downloaded `asb` SDK and Windows ISO (external binaries, not in repo). `/MT` static CRT noted deliberately.

---

## Other tools in awesome-gta6-RE
(Not in the 19. URL | one-line blurb from the list, paraphrased. Count as listed in the README tables.)

Archive / asset explorers
- Percy6Stars | https://github.com/Ahmet-Dedeler/Percy6Stars | PercyStar fork with macOS/Linux support, in-app BK2 video and audio.
- 6to5m | https://github.com/DesignWithTommy/6to5m | GTA VI to GTA V / FiveM converter (Rust, GPL-3.0, CLI + MCP server).
- OpenRPF9 | https://github.com/shrewbrewsh/OpenRPF9 | Small C# WinForms stream carver/inspector (MIT).
- rpf9-extract | https://github.com/offs/rpf9-extract | Clean-room Rust CLI extractor/inspector.
- zeozeozeo/rpf9 | https://github.com/zeozeozeo/rpf9 | Minimal clean-room Rust library for the RPF9 format.

Textures / audio / video
- ttpf-viewer | https://github.com/paryzius/ttpf-viewer | Fast `.ttpf` gallery/extractor with exact Gfx10 layouts (Rust, MIT).
- rage-texture | https://github.com/ifBars/rage-texture | Texture library built on bcpack-rs (GPL-3.0-only).
- bcpack-decoder | https://github.com/Tim0219800/bcpack-decoder | Second independent clean-room BCPack decoder (Rust, MIT).
- rpf9-audio | https://github.com/jaylikesbunda/rpf9-audio | Dependency-free Python RPF9 / ADAT-AWC audio toolkit.
- bink2-decoder | https://github.com/codingkoala/bink2-decoder | Clean-room Bink 2 video + audio decoder (Rust, Apache-2.0).
- gta6-video-reader | https://github.com/hoshino-114514/gta6-video-reader | Bink 2 to MP4 pipeline that hosts the game's own Bink DLL (Python/C, MIT).
- BK2toMP4 | https://github.com/FluffyCyanNova/BK2toMP4 | Batch .bk2 to .mp4 using RAD Video Tools (Python, MIT).
- Ahmet-Dedeler/bink-player-mac | https://github.com/Ahmet-Dedeler/bink-player-mac | Native macOS Bink2 player + bk2mp4.sh.
- Ahmet-Dedeler/weather-study | https://github.com/Ahmet-Dedeler/weather-study | WebGL2 weather/timecycle visualizer.

Console translation layers / recompilation
- Leonida | https://github.com/ItsNotRexed/Leonida | Largest static recompilation effort; 26,503 RDNA2 shaders to SPIR-V (GPL-3.0).
- Xbox2Windows | https://github.com/R0Y00/Xbox2Windows | Static recompilation research; x64 to C++, gfx1013 shaders to DXIL (MIT).
- xcl | https://github.com/ivall/xcl | The project this repo is (GPL-2.0).
- xbox-emulator | https://github.com/omonkeyz/xbox-emulator | Continuation of TomBox (Rust, GPL-3.0-or-later).
- TomBox | https://github.com/drycheetah84/TomBox | Upstream runtime of xbox-emulator (Rust).
- americas-recomp | https://github.com/realdody/americas-recomp/tree/xbox-series | AnyPS5 fork testing Xbox shaders (PortWatch says repo gone since 2026-11-23, HTTP 404).
- OpenScarlett | https://github.com/californiaAlmonds/OpenScarlett | Clean-room Scarlett layer (C++20, MIT).
- ScarlettX | https://github.com/Freminet/ScarlettX | Clean-room HLE (C++, no license).
- XValkyrie | https://github.com/sammwyy/xvalkyrie | Wine-style layer for Xbox One + Series (C++20, GPL-3.0).
- TuxBox | https://github.com/brcly/TuxBox | Series games on Linux/Proton (Rust, no license).
- PercyScarlett | https://github.com/rpatty-dev/PercyScarlett | Translation layer from PercyStar's author (custom non-commercial).
- OpenScarlett (OpenCommunityEdition) | https://github.com/OpenCommunityEdition/OpenScarlett | Unrelated, same name (C++, AGPL-3.0).
- five-star-recomp | https://github.com/ThatRecompDev/five-star-recomp | GTA VI toolchain in C#/.NET 10 (GPL-3.0).
- AnyXboxX | https://github.com/LooperSalty/AnyXboxX | GDK loader via Unicorn or native C++ (GPL-3.0).
- AnyBox | https://github.com/Dark3581/AnyBox | Statically relinks decrypted GDK exe imports (C++20, GPL-2.0).
- G6Recomp | https://github.com/Kajcsi/G6Recomp | Native loader + x86-64 static recompiler (C++, GPL-3.0).
- rebox | https://github.com/puffnfresh/rebox | Series GDK titles on Linux via WineGDK/vkd3d-proton (GPL-2.0).
- GTA6-BootTest | https://github.com/OnlyMoisties/GTA6-BootTest | Stand-in console DLLs + debugger to step the leaked exe (C, no license).
- scarlett-layer | https://github.com/jadeowaowa/scarlett-layer | Loader + hook DLL for unpacked GDK exes (C++, Apache-2.0).
- GTAVI-Recomp | https://github.com/JamesCicada/GTAVI-Recomp | GTA VI layer on AnyPS5 (PS5 to PC).
- AnyPS5 | https://github.com/boykopovar/AnyPS5 | General PS5 system-library reimplementation (C++, GPL-2.0).

Format docs / research
- gta-vi-port-research | https://github.com/oguzhanozfe/gta-vi-port-research | Xbox-to-PC research handbook with Python inspection scripts.
- gta6-vr-port | https://github.com/dariulone/gta6-vr-port | Executable static-analysis notes for an OpenXR port (no license).
- RPF9-tools | https://github.com/frenzy42069/RPF9-tools | Byte-level format_desc.md + C/Python scripts (list says "Ignore the satirical disclaimer").
- 4lwayz/portwatch-vi | https://github.com/4lwayz/portwatch-vi | Daily tracker of PC port efforts (not an RE tool; "See also" in awesome list; same file mentions clone networks/re-uploads).

Extra repos seen outside awesome list (not in 19): `zsy-miao/RageForge` (see RageForge section).
