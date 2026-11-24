# TTPF Studio

Python/Tkinter desktop viewer and exporter for GTA VI data: `.ttpf` texture packs, 3D models, the map, `.rpf` archive contents, with a small CLI for bulk texture dumps.

- Repo: https://github.com/woreksami/ttpf-studio (branch `main`)
- Commit described: `a2fe110`, 2026-11-23 (`reverse-toolkit/tools.lock`)
- Licence: MIT (`reverse-toolkit/tools/ttpf-studio/LICENSE`).
- **Build from source. The release zip and its `TTPF Studio.exe` are unsigned** (the README itself says Windows will warn; `reverse-toolkit/tools/ttpf-studio/README.md:28`). Do not use the prebuilt binary.

## What it is for in our work

Looking at `.ttpf` (RDS5) texture packs and checking texture decodes by eye. The decoders are the tool's own code (`reverse-toolkit/tools/ttpf-studio/README.md:94`); the byte layouts it reverse-engineered are in `reverse-toolkit/tools/ttpf-studio/FORMATS.md`, which is worth reading next to the BCPack notes in other pages of this toolkit ([bcpack-rs](bcpack-rs.md), [0x52504639](0x52504639.md)). The tool does not decrypt the archive file list (`reverse-toolkit/tools/ttpf-studio/README.md:64`).

## Prerequisites and build from source

Python 3.10 or newer plus the packages in `reverse-toolkit/tools/ttpf-studio/requirements.txt:1` (numpy, Pillow, PyOpenGL, pyopengltk). Tkinter must be present (`reverse-toolkit/tools/ttpf-studio/ttpf_tool.py:1249`). Use a virtualenv so nothing lands in your system Python:

```bash
cd reverse-toolkit/tools/ttpf-studio
python3 -m venv .venv && . .venv/bin/activate        # Windows: py -m venv .venv ; .venv\Scripts\activate
pip install -r requirements.txt
python ttpf_tool.py --selftest                       # built-in tests, no game data
```

The README targets Windows; the repository's CI runs only the self-test on Windows and Ubuntu (`reverse-toolkit/tools/ttpf-studio/.github/workflows/test.yml:7`). The GUI on macOS is not verified by the project or by us. `--install` copies the app into `%LOCALAPPDATA%` and adds shortcuts (`reverse-toolkit/tools/ttpf-studio/ttpf_tool.py:1194`); skip it.

## Usage

Entry point: the `__main__` block, `reverse-toolkit/tools/ttpf-studio/ttpf_tool.py:4303`. Arguments are matched by hand, not argparse:

```bash
python ttpf_tool.py                                  # GUI; "Open your game folder", then Models / Textures / Library
python ttpf_tool.py <path>                           # GUI, opening a folder, .rpf or .ttpf
python ttpf_tool.py --rpf <archive.rpf>              # print an archive header
python ttpf_tool.py --dump <packs folder> <output>   # export every texture of the .ttpf files in a folder
python ttpf_tool.py --dump <folder> <out> --rpf      # also dump archive contents
python ttpf_tool.py --glcheck                        # exit 0 when the GPU 3D view works
```

- `--selftest` at `reverse-toolkit/tools/ttpf-studio/ttpf_tool.py:4306`, `--rpf` at `:4308`, `--install` at `:4310`, `--glcheck` at `:4312`, `--dump` at `:4315`; `--dump` has `--no-headerless` and `--rpf` modifiers.
- GUI flows (README, "What you can do"): open the game folder (the one with `levels.rpf`, `packs.rpf`, `data`); pick a model category and export OBJ; open a `.ttpf` to zoom, view channels, export PNG or DDS; **Library -> Update index** to catalogue everything once. The first scan of a large archive takes minutes (`reverse-toolkit/tools/ttpf-studio/README.md:30`).
- The texture codec is `reverse-toolkit/tools/ttpf-studio/texcodec.py`; models and map export are `reverse-toolkit/tools/ttpf-studio/models.py`.

Inputs: your own game folder, read only. Outputs: PNG/DDS/OBJ/glTF in folders you choose, and settings, caches and an error log in `%APPDATA%\TTPF Studio` (`reverse-toolkit/tools/ttpf-studio/ttpf_tool.py:1039`; on macOS the code falls back to the home directory).

## Caveats

- Unsigned prebuilt binaries: not used. The video-to-MP4 conversion needs RAD Video Tools, a separate download; skip it.
- "Replace a texture and save a modded copy of the pack" exists in the GUI; never do this on a pack from a title you do not own, and never commit the result.
- Not done yet, per the README: skin colours, vehicle tuning parts, textures inside model files, skeletons and animation (`reverse-toolkit/tools/ttpf-studio/README.md:68`).
- No network access in the code paths we read.

## Update

```bash
./reverse-toolkit/update.sh update --only ttpf-studio
```

Then `pip install -r requirements.txt` again in your venv if `requirements.txt` changed.
