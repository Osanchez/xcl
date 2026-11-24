# AWC from RPF9 Extractor

Python/Tkinter tool that scans RPF9 archives for AWC (`ADAT`) audio containers, lists the music streams, plays them and remuxes the original Opus packets into Ogg Opus (or FLAC/WAV/MP3 through ffmpeg). Includes a live-radio companion app and an Android app.

- Repo: https://github.com/Albotterin/AWC_from_RPF9_Extractor (branch `main`)
- Commit described: `2e5ac99`, 2026-11-24 (`reverse-toolkit/tools.lock`, committed 00:12 local time)
- Licence: MIT (`reverse-toolkit/tools/awc-from-rpf9/LICENSE`).
- Constraint: MIT, but it downloads and runs GPL-3 code from another project (see Caveats); that code is not ours to copy into XCL either.

## What it is for in our work

Audio format study: the AWC container layout (blocks, per-channel entries, Opus packets) is written up byte by byte in `reverse-toolkit/tools/awc-from-rpf9/docs/FORMAT.md`. That is the reference when we look at what the console audio path (XAudio2/XMA2/Opus in XCL's HLE) has to feed from game archives. It needs no table of contents: it scans raw bytes for `ADAT`, so it does not decrypt anything (`reverse-toolkit/tools/awc-from-rpf9/README.md:41`).

## Prerequisites and build from source

There is nothing to compile. Run from the clone (README targets Windows; the core modules are plain Python):

- Python 3 with Tkinter (developed on 3.14). Pillow is optional, only for station logos (`reverse-toolkit/tools/awc-from-rpf9/README.md:134`).
- `ffmpeg` and `ffplay`, only for playback and FLAC/WAV/MP3 export. Opus extraction works without them. Install them from your package manager (`brew install ffmpeg` on macOS), do not unpack random builds.
- Do not use the prebuilt release `.exe` files. If you want a standalone exe, `build_exe.bat` builds one with PyInstaller (`reverse-toolkit/tools/awc-from-rpf9/build_exe.bat:6`); we do not need one.
- The Android app (`android/`) needs the Android SDK and a JDK. **We do not build or use it** (see Caveats).

macOS: unverified (the project targets Windows, and stores state in `%APPDATA%\AWC-Tool`); Tkinter and ffmpeg make it likely to start but paths will be odd. Windows is the supported route.

```powershell
cd reverse-toolkit\tools\awc-from-rpf9
python awc_tool.py
```

## Usage

Entry points: `main` in `reverse-toolkit/tools/awc-from-rpf9/awc_tool.py:1421` (GUI; accepts a folder, `.rpf` or `.awc` paths, `--lang=en|de`, and `--selftest <report.txt> <file.awc>`), and `main` in `reverse-toolkit/tools/awc-from-rpf9/awc_radio.py:1031` (the radio app).

```powershell
python awc_tool.py "D:\your\own\install\audio.rpf"      # scan one archive (cached after the first scan)
python awc_tool.py "D:\your\own\install"                # or a whole folder, all subfolders
python awc_tool.py --lang=en
python awc_radio.py                                      # AWC Radio companion app
python awc_radio.py --build-index                        # no window: rebuild the station index from saved settings
```

- GUI flow: **Scan folder...** or **Open files...**, browse the tree (container, group, AWC, stream), double-click to play, pick a format and output folder, **Extract selected**. **Only streams longer than (sec.)** drops short snippets (`reverse-toolkit/tools/awc-from-rpf9/README.md:151`).
- A 28 GB `audio.rpf` scans in about 6 minutes; the result is cached under `%APPDATA%\AWC-Tool\cache` (`reverse-toolkit/tools/awc-from-rpf9/README.md:41`, `:83`).
- Output layout: `<out>/<container>/<group>/<awc>_stream<N>_<STREAMID>.<ext>` (`reverse-toolkit/tools/awc-from-rpf9/README.md:263`). Opus is a bit-exact remux (no re-encode); FLAC/WAV/MP3 are decoded from the lossy source.
- Grouping is a heuristic by archive position, because file names are unreadable (`reverse-toolkit/tools/awc-from-rpf9/README.md:291`).

Inputs: your own `audio.rpf` or `.awc` files. Outputs: `.opus`/Ogg, FLAC, WAV, MP3, tags and `.m3u8` playlists, only where you point it.

## Caveats

- **First-run downloads from the network, each triggered by a dialog or button, never silently:**
  - **Leonida decoder from GitHub.** For station logos the tool offers to download seven files (about 230 KB) from `ItsNotRexed/Leonida`, pinned to one commit, into `%APPDATA%\AWC-Tool\leonida\`, then imports and runs them (`reverse-toolkit/tools/awc-from-rpf9/awc_icons.py:32` to `:36`, download at `:74`, import at `:95`). There is no hash check on the files. That code is GPL-3 and third party. In AWC Radio the prompt appears on first start (`reverse-toolkit/tools/awc-from-rpf9/awc_radio.py:616`); in the main tool it is the **Station logos...** button (`reverse-toolkit/tools/awc-from-rpf9/awc_tool.py:670`). Decline it: logos are cosmetic.
  - **Soundtrack list from rockstargames.com.** **Songs from the credits page** (and AWC Radio on first use) fetches `https://www.rockstargames.com/VI/credits` to hash titles and name streams (`reverse-toolkit/tools/awc-from-rpf9/awc_credits.py:16`, download at `:30`; callers `reverse-toolkit/tools/awc-from-rpf9/awc_tool.py:735` and `reverse-toolkit/tools/awc-from-rpf9/awc_radio.py:635`). It is a public page and the list stays on your machine; if you want no network traffic, skip the button and use **Import names...** instead.
- **Android station packs are not used by us.** The Android app plays a `.dat` pack built from the game's audio (`awc_radio.py --export-pack out.dat`, `reverse-toolkit/tools/awc-from-rpf9/awc_radio.py:1040`). Do not make or copy one: it is bulk game audio, and the README says not to share it (`reverse-toolkit/tools/awc-from-rpf9/README.md:231`).
- Extracted music is the publisher's copyrighted audio. Keep it out of the repository and off shared drives.
- Windows paths and `%APPDATA%` are assumed throughout the GUI.

## Update

```bash
./reverse-toolkit/update.sh update --only awc-from-rpf9
```
