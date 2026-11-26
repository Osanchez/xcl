# CLI and configuration

The `xcl` command line, the per-title configuration file, the files a run writes, and a reference
of every `XCL_*` name found in the crate, the tests and the scripts.

**Platform.** XCL builds and runs on Windows only (x64, `stable-x86_64-pc-windows-msvc`); on macOS or
Linux the crate does not build (`AGENTS.md:30`). The commands below were read from the source, not run
on the machine that wrote this page. Build: `cargo build --release --workspace` gives
`target\release\xcl.exe` (`README.md:118`).

## The `xcl` command line

Dispatch is in `crates/xcl/src/main.rs:44`-`83`; the help text is `crates/xcl/src/main.rs:20`-`42`.

| Command | Handler | Exit codes |
|---|---|---|
| `xcl run <game folder or exe> [-- game args]` | `crates/xcl/src/launcher.rs:288` | the game's exit code; 2 for a launcher error |
| `xcl inspect <exe or dll> [--json]` | `crates/xcl/src/inspect.rs:68` | 0; 2 unreadable file; 3 not a valid x64 PE |
| `xcl disasm <file> [options]` | `crates/xcl/src/disasm.rs:105` | 0; 2 bad option or unreadable file |
| `xcl shaders <file, folder or package.rpf> [options]` | `crates/xcl/src/shaders.rs:680` | 0; 2 unreadable file |
| `xcl rpf <package.rpf> [options]` | `crates/xcl/src/rpf.rs:99` | 0; 2 on any error |
| `xcl --version` / `-V` | `crates/xcl/src/main.rs:69` | 0 |
| `xcl --help` / `-h` or no arguments | `crates/xcl/src/main.rs:74`-`80` | 0; any other unknown command prints help and exits 2 |

A subcommand given without its file argument falls through to the help text and exit code 2
(`crates/xcl/src/main.rs:55`, `crates/xcl/src/main.rs:62`, `crates/xcl/src/main.rs:66`-`68`,
`crates/xcl/src/main.rs:74`-`80`).

### `xcl run`

```powershell
xcl run D:\Games\SomeTitle                    # a folder (its MicrosoftGame.config names the exe)
xcl run D:\Games\SomeTitle\game.exe -- -windowed
```

- The target is a folder or an exe. Everything after `--` is the game's command line; with no `--`,
  everything after the target is (`crates/xcl/src/main.rs:55`-`61`).
- What it does, in order: resolve the package root, exe and title; load or create
  `<data>\xcl.toml`; fill "auto" settings from `MicrosoftGame.config`; write `cache\boot.json`; rotate
  the previous log and report; print the PC checks; check the CPU; start the host process and wait; print a summary
  (`crates/xcl/src/launcher.rs:288`-`382`). The two-process walkthrough is in `runtime-core.md`.
- Exe choice: command line, then `exe` in `xcl.toml`, then the first existing `<Executable Name>` of
  `MicrosoftGame.config`, then the only `.exe` in the folder (or up to two levels below)
  (`crates/xcl/src/launcher.rs:304`-`319`, `crates/xcl/src/launcher.rs:180`-`210`). `xcl.exe` itself is never picked
  (`crates/xcl/src/launcher.rs:142`).
- The game folder is only read. Writes, saves and logs go to `%XCL_DATA_ROOT%\<title>\` or
  `%LOCALAPPDATA%\XCL\<title>\` (`crates/xcl/src/launcher.rs:10`-`16`, `crates/xcl/src/launcher.rs:296`).
- Summary printed after the game exits: exit code, PC checks that were warnings or errors, crash line,
  import counts by source, the first 15 stubs called, unmapped console paths, redirected writes, and
  the report and log paths (`crates/xcl/src/launcher.rs:484`-`517`).

### Run with no arguments

When `xcl.exe` is opened from Explorer (its console is its own), a small task-dialog menu offers "Choose a
game..." (a file dialog for the exe or `MicrosoftGame.config`) and "Run again" (the last game, kept in
`<data root>\last-game.txt`). On a non-zero exit the console waits for Enter
(`crates/xcl/src/main.rs:73`, `crates/xcl/src/main.rs:87`-`120`, `crates/xcl/src/main.rs:196`, `crates/xcl/src/main.rs:234`;
`README.md:127`-`129`).

### `xcl inspect`

```powershell
xcl inspect D:\Games\SomeTitle\game.exe
xcl inspect D:\Games\SomeTitle\game.exe --json > imports.json
```

Static, read-only analysis: type, image base, entry, relocations, static TLS size, `.pdata` count, stack
reserve, whether the file imports console libraries, and every import with how XCL would resolve it (HLE
module, Windows DLL, game DLL, or `STUB`). `--json` prints the same as JSON. DLLs are looked up only next
to the file (`crates/xcl/src/inspect.rs:68`-`156`).

### `xcl disasm`

```powershell
xcl disasm shader.bin                       # containers (XDAT/XBCH/FSSM) are unpacked
xcl disasm shader.bin --offset 0x40 --length 0x200 --all --hex
xcl disasm shader.bin --fssm unpacked.fssm  # save the unpacked FSSM
```

| Flag | Meaning |
|---|---|
| `--offset N`, `--length N` | disassemble a byte range; decimal or `0x` hex (`crates/xcl/src/disasm.rs:9`-`20`) |
| `--all` | with a range, passes `stop = false` to the decoder (the default is `stop = !all`) (`crates/xcl/src/disasm.rs:120`, `crates/xcl/src/disasm.rs:22`) |
| `--hex` | full hex of the console container parts (`crates/xcl/src/disasm.rs:145`) |
| `--fssm FILE` | write the unpacked FSSM to FILE (`crates/xcl/src/disasm.rs:144`) |

An RDNA2 decoder for console GPU code; the decoder is described in `gpu-recompiler.md`.

### `xcl shaders`

```powershell
xcl shaders D:\data\shaders.rpf --top 50
xcl shaders D:\data\shaders.rpf --recomp
xcl shaders D:\data\shaders.rpf --compile 20
```

Shader corpus statistics: formats, stages and an RDNA2 instruction histogram. A folder is scanned
recursively; an RPF9 package is scanned for shader records (`crates/xcl/src/shaders.rs:692`-`713`).

| Flag | Meaning | In help text |
|---|---|---|
| `--top N` | rows per list, default 60 (`crates/xcl/src/shaders.rs:682`) | yes |
| `--json` | JSON output (`crates/xcl/src/shaders.rs:683`) | yes |
| `--recomp` | translate every RDNA2 program to HLSL and show what is missing (`crates/xcl/src/shaders.rs:685`) | yes |
| `--compile N` | also compile N complete translations with DXC; implies `--recomp` (`crates/xcl/src/shaders.rs:684`-`685`) | yes |
| `--iface` | how console programs connect to their DXBC signatures (`crates/xcl/src/shaders.rs:686`, `crates/xcl/src/shaders.rs:15`) | no |
| `--example MNEMONIC` or `--example name=SUBSTR` | keep an example program missing that mnemonic, or whose name contains SUBSTR (`crates/xcl/src/shaders.rs:687`-`689`, `crates/xcl/src/shaders.rs:410`-`416`) | no |
| `--precompile` | translate every VS/PS/MS/CS program and compile it into the DXIL cache so a title does not wait for DXC (`crates/xcl/src/shaders.rs:715`-`719`, `crates/xcl/src/shaders.rs:814`) | no |
| `--limit N`, `--windows N` (default 4), `--split 0\|1`, `--only STAGE`, `--pipelines` | options of `--precompile`: program limit, memory windows, force or forbid the split wave64 form, one stage (case-insensitive, `crates/xcl/src/shaders.rs:855`), also build each compute pipeline on the GPU (`crates/xcl/src/shaders.rs:716`-`718`) | no |

Related environment variables: `XCL_RECOMP_WINDOWS`, `XCL_RECOMP_ONLY`, `XCL_DUMP_HLSL`, `XCL_DUMP_EVERY`,
`XCL_EXAMPLE_HLSL`, `XCL_PIPELINE_THREADS`, `XCL_ADAPTER` (all in the table below). This command is
part of the recompiler tooling; see `gpu-recompiler.md`.

### `xcl rpf`

```powershell
xcl rpf D:\data\shaders.rpf --top 20
xcl rpf D:\data\shaders.rpf --find core_ --top 10
xcl rpf D:\data\shaders.rpf --extract NAME --out folder
xcl rpf D:\data\big.rpf --probe
```

| Flag | Meaning |
|---|---|
| `--top N` | records or headers listed, default 30 (`crates/xcl/src/rpf.rs:101`) |
| `--find NAME` | only records whose name contains NAME (`crates/xcl/src/rpf.rs:110`, `crates/xcl/src/rpf.rs:167`) |
| `--extract NAME` | unpack records whose name contains NAME (`crates/xcl/src/rpf.rs:111`) |
| `--out DIR` | where `--extract` writes, default `rpf_out` (`crates/xcl/src/rpf.rs:112`); duplicate names are kept apart (`crates/xcl/src/rpf.rs:31`) |
| `--probe` | bounded-memory header probe (`crates/xcl/src/rpf_probe.rs:120`); cannot be combined with `--full`, `--find`, `--extract` or `--out` (`crates/xcl/src/rpf.rs:103`-`108`) |
| `--full` | scan a package over 1 GiB in detail instead of probing (`crates/xcl/src/rpf.rs:102`, `crates/xcl/src/rpf.rs:120`-`126`) |

Archives over 1 GiB are probed automatically; `--find` and `--extract` on them need `--full`
(`crates/xcl/src/rpf.rs:15`, `crates/xcl/src/rpf.rs:120`-`126`). A file that does not start with the RPF9
magic is rejected (`crates/xcl/src/rpf.rs:135`-`136`).

## Per-title configuration: `xcl.toml`

Location: `<data>\xcl.toml`, i.e. `%LOCALAPPDATA%\XCL\<title>\xcl.toml` or
`%XCL_DATA_ROOT%\<title>\xcl.toml`. The launcher creates it with every default and a header when it is
missing, and rewrites nothing else (`crates/xcl/src/config.rs:207`-`234`, header at `crates/xcl/src/config.rs:194`).
A file that cannot be read or parsed stops the run with exit code 2 and is not replaced
(`crates/xcl/src/config.rs:221`-`223`, `crates/xcl/src/launcher.rs:297`-`303`). A partial file is fine: missing
keys take their defaults (`crates/xcl/src/config.rs:9`, test `crates/xcl/src/config.rs:301`).

After loading, `resolve` fills the "auto" settings from `MicrosoftGame.config` (below) and the launcher
stores the result, with `exe` set to the chosen exe relative to the package root, in `cache\boot.json`
for the host (`crates/xcl/src/launcher.rs:323`-`329`). `{game}` and `{data}` in mount targets are expanded
by `Boot::expand` (`crates/xcl/src/config.rs:282`).

| Key | Default | Meaning |
|---|---|---|
| `device_type` (`crates/xcl/src/config.rs:12`) | `"anaconda"` | `"anaconda"` (Series X) or `"lockhart"` (Series S; `"series s"` also works, `crates/xcl/src/config.rs:237`) |
| `console_type_raw` (`crates/xcl/src/config.rs:15`) | 6 | raw value of kernelx `GetConsoleType`; a guess, can be overridden |
| `gamertag` (`crates/xcl/src/config.rs:16`) | `"Player1"` | the signed-in user's gamertag |
| `user_id` (`crates/xcl/src/config.rs:17`) | `0x0009000000000001` | user id |
| `console_id` (`crates/xcl/src/config.rs:18`) | 38-character id of zeros (`crates/xcl/src/config.rs:202`) | `XSystemGetConsoleId` |
| `sandbox_id` (`crates/xcl/src/config.rs:19`) | `"RETAIL"` | sandbox id |
| `title_memory_mb` (`crates/xcl/src/config.rs:22`) | 0 | `TitleMemoryStatus` budget in MiB; 0 = 13 824 (Series X) or 8 192 (Series S) (`crates/xcl/src/config.rs:117`-`123`) |
| `title_memory` (`crates/xcl/src/config.rs:24`) | `"auto"` | `"auto"` (from the manifest), `"standard"` or `"advanced"`; read in any case |
| `pls_mb` (`crates/xcl/src/config.rs:26`) | 0 | persistent local storage quota in MiB; 0 = the manifest's `SizeMB`, else 4096 (`crates/xcl/src/config.rs:95`-`103`) |
| `uma_mb` (`crates/xcl/src/config.rs:30`) | 0 | unified (GPU-visible) memory in MiB; 0 = 4095 for `"advanced"`, else 2048 (`crates/xcl/src/config.rs:110`-`112`); `XCL_UMA_MB` overrides |
| `log_level` (`crates/xcl/src/config.rs:31`) | `"info"` | `error`, `warn`, `info`, `debug` or `trace` (`crates/xcl/src/log.rs:22`-`29`) |
| `log_to_stderr` (`crates/xcl/src/config.rs:32`) | `true` | echo info and above to stderr (`crates/xcl/src/log.rs:87`-`89`) |
| `exe` (`crates/xcl/src/config.rs:34`) | `""` | exe inside the game folder; auto-detected when empty, and rewritten on every run |
| `package_identifier` (`crates/xcl/src/config.rs:35`) | `""` | package identifier handed to the title |
| `title_id` (`crates/xcl/src/config.rs:37`) | `""` | hex title id for `XGameGetXboxTitleId`; empty = `<TitleId>` of the manifest (`crates/xcl/src/config.rs:104`-`109`) |
| `mounts` (`crates/xcl/src/config.rs:39`) | `GAME:`, `G:`, `D:` -> `{game}`; `T:` -> `{data}\temp`; `S:` -> `{data}\pls` (`crates/xcl/src/config.rs:152`-`159`) | console path prefix -> PC folder; defaults added to an older file (`crates/xcl/src/config.rs:211`-`214`) |
| `aliases` (`crates/xcl/src/config.rs:41`) | empty | import DLL pattern -> `"kernelx.dll"` (or another HLE module), `"@system"` or `"@stub"` |
| `stub_returns` (`crates/xcl/src/config.rs:43`) | empty | `"dll!Function"` or `"*!Function"` -> value an automatic stub returns (`E_NOTIMPL`, `0`, `0x1`, `-1`; `crates/xcl/src/util.rs:31`) |
| `graphics` (`crates/xcl/src/config.rs:45`) | `"d3d12"` | `"d3d12"` (real GPU) or `"null"` (no rendering) |
| `d3d12_adapter` (`crates/xcl/src/config.rs:48`) | `"auto"` | `"auto"`, `"warp"`, a number, or part of the adapter's name; `XCL_ADAPTER` overrides |
| `d3d12_debug` (`crates/xcl/src/config.rs:50`) | `false` | D3D12 debug layer (needs Graphics Tools); `XCL_D3D12_DEBUG` also turns it on |
| `present` (`crates/xcl/src/config.rs:52`) | `"auto"` | `"auto"` shows the title's frames in an XCL window, `"off"` does not |
| `raytracing` (`crates/xcl/src/config.rs:56`) | `"auto"` | tier reported to `CheckFeatureSupport(D3D12_OPTIONS5)`: `auto`, `tier1_1`, `tier1_0`, `off`; `XCL_RAYTRACING` overrides |
| `window_width`, `window_height` (`crates/xcl/src/config.rs:57`-`58`) | 1280, 720 | size of the XCL window |
| `window_pos` (`crates/xcl/src/config.rs:62`) | `"auto"` | `"auto"`, `"center"` or `"x,y"`; `XCL_WINDOW_POS` overrides |
| `xbox_live_tokens` (`crates/xcl/src/config.rs:67`) | `false` | answer `XUserGetTokenAndSignature*` with a local placeholder token; off: it fails as on an offline console (0x80072EFD) |
| `keyboard_mouse` (`crates/xcl/src/config.rs:71`) | `true` | keyboard and mouse play as the gamepad while the XCL window has focus |
| `mouse_sensitivity` (`crates/xcl/src/config.rs:74`) | 1.0 | right-stick deflection per mouse speed; 1.0 = full at 4000 counts a second |

All defaults: `crates/xcl/src/config.rs:150`-`192`.

### `MicrosoftGame.config`

Read from the package root (the folder the exe's config names, found up to four levels above the exe,
`crates/xcl/src/launcher.rs:225`). Text is UTF-8, or UTF-16 with a byte-order mark, with XML comments
removed so a commented-out setting is ignored (`crates/xcl/src/launcher.rs:91`-`125`).

| Element | Used for | Read at |
|---|---|---|
| `<Executable Name="...">` | which exe `xcl run <folder>` starts | `crates/xcl/src/launcher.rs:138` |
| `<Identity Name="...">` | the title name, hence the data folder | `crates/xcl/src/launcher.rs:138`, `crates/xcl/src/launcher.rs:248`-`249` |
| `<ShellVisuals DefaultDisplayName>` | display name (unless it is an `ms-resource:` reference) | `crates/xcl/src/launcher.rs:130`-`133` |
| `<PersistentLocalStorage><SizeMB>` | `pls_mb` when it is 0 | `crates/xcl/src/manifest.rs:41`, `crates/xcl/src/config.rs:95`-`103` |
| `<XboxAnacondaTitleMemory>` / `<XboxLockhartTitleMemory>` | `title_memory` when `"auto"` (picked by `device_type`) | `crates/xcl/src/manifest.rs:53`-`54`, `crates/xcl/src/config.rs:85`-`91` |
| `<TitleId>` | `title_id` when empty | `crates/xcl/src/manifest.rs:55`, `crates/xcl/src/config.rs:104`-`109` |
| `<Resources><Resource Language>` | package locales, for XGameRuntime locale selection | `crates/xcl/src/manifest.rs:42`-`50`, `crates/xcl/src/hle/xgameruntime/api.rs:1210` |

Nothing else in the file is read (`crates/xcl/src/manifest.rs:1`-`4`). What the manifest supplied is printed as
`MicrosoftGame.Config: ...` at the start of a run (`crates/xcl/src/launcher.rs:343`-`345`). Background:
`HANDOFF.md:1164`.

## Files a run writes

Under `<data>` = `%LOCALAPPDATA%\XCL\<title>\` (or `%XCL_DATA_ROOT%\<title>\`):

| Path | Written by | Content |
|---|---|---|
| `xcl.toml` | `crates/xcl/src/config.rs:230` | the title's configuration |
| `logs\xcl.log` | `crates/xcl/src/host.rs:19`-`22` | host log, lines `[seconds][thread][LEVEL] message` (`crates/xcl/src/log.rs:79`) |
| `logs\xcl.prev.log`, `logs\report.prev.json` | `crates/xcl/src/launcher.rs:335`-`337` | the previous run's log and report |
| `logs\report.json` | `crates/xcl/src/report.rs:171` | imports by source, stubbed imports, stubs actually called, modules, VFS (unmapped paths, redirected writes, whiteouts), shader statistics, DXC, D3D12.X, audio, XGameRuntime, DirectStorage, the PC checks, crash, exit code (`crates/xcl/src/report.rs:44`-`83`) |
| `logs\frame_<n>.bmp` | `crates/xcl/src/hle/d3d12x/present.rs:743` | frame dumps, with `XCL_FRAME_DUMP` |
| `cache\boot.json` | `crates/xcl/src/launcher.rs:327`-`329` | the `Boot` record the host reads |
| `cache\xcl_tls_*.dll` | `crates/xcl/src/tls.rs:451`-`503` | TLS carrier DLLs, removed at the next start (`crates/xcl/src/tls.rs:505`) |
| `cache\uma_coherent.txt` | `crates/xcl/src/gpu/uma.rs:672`-`675` | size of the coherent window the title asked for |
| `overlay\` | `crates/xcl/src/vfs.rs:161` | copies of game files the title wrote; `overlay\.xcl_whiteouts` lists deleted game files (`crates/xcl/src/vfs.rs:162`) |
| `saves\`, `pls\`, `temp\` | `crates/xcl/src/launcher.rs:320`-`322` | save folder, persistent local storage (`S:\`), temp (`T:\`) |
| `shaders\` | `crates/xcl/src/gpu/shader.rs:201` | shader dumps, with `XCL_DUMP_SHADERS=1` |

Outside the title folder: `<data root>\last-game.txt` (the menu's "Run again", `crates/xcl/src/main.rs:95`),
the DXIL cache `%LOCALAPPDATA%\xcl\dxil` (`crates/xcl/src/gpu/recomp/dxc.rs:555`-`560`; note the lower-case
`xcl`), and, in its `translated\` subfolder, the translated-program cache
(`crates/xcl/src/gpu/recomp/pso.rs:123`, `crates/xcl/src/gpu/recomp/pso.rs:158`). The game folder is never modified (`README.md:132`).

Console output of `xcl run` ends with the exit code, the `Report:` and `Log:` paths
(`crates/xcl/src/launcher.rs:485`, `crates/xcl/src/launcher.rs:516`-`517`).

## Environment variables

Defaults are what the code does when the variable is unset. "Where read" is the first place the
variable is read; some are read in more places. The recompiler, D3D12.X and test variables are listed here
for completeness; what they affect is explained in `gpu-recompiler.md`, `d3d12x.md` and `testing-and-scripts.md`.
`XCL_HOST` is set by XCL itself. Test and probe variables belong to `cargo test` and are not read by a normal run.

### Run, launcher and host

| Name | Where read | What it does | Default |
|---|---|---|---|
| `XCL_DATA_ROOT` | `crates/xcl/src/launcher.rs:11` | Folder that holds every title's data folder. | `%LOCALAPPDATA%\XCL` (temp folder if `LOCALAPPDATA` is unset, crates/xcl/src/launcher.rs:14) |
| `XCL_HOST` | `crates/xcl/src/main.rs:48` | Internal. The launcher sets it to the path of `boot.json` for the child process (crates/xcl/src/launcher.rs:439); a process that finds it set runs as the host and removes it. | unset |
| `XCL_SKIP_CPU_CHECK` | `crates/xcl/src/launcher.rs:360` | Run even though this CPU lacks extensions the console's Zen 2 has (AVX2, FMA, BMI, ...); without it `xcl run` exits with code 2. | unset (check on) |
| `XCL_RUN_SECONDS` | `crates/xcl/src/host.rs:96` | The title ends itself after n seconds with the GPU drained and the log and report written. Used by `scripts/smoke.ps1`. | unset (runs until the title exits) |
| `XCL_ACTIVITY` | `crates/xcl/src/host.rs:33` | Every 2 s logs presents, submits, draws, dispatches, frame-event waits, DirectStorage requests and a memory line. | unset (off) |
| `XCL_THREAD_DUMP_AT` | `crates/xcl/src/host.rs:81` | Comma-separated seconds; at each one the log gets where every thread is (RIP and stack calls). | unset (off) |
| `XCL_NETWORK` | `crates/xcl/src/hle/netoff.rs:59` | `0` makes the title's WinHTTP and Winsock calls fail as on a console without a network (loopback and local names still work). Used by `scripts/smoke.ps1` (scripts/smoke.ps1:34). | unset (network works) |

### Settings that override `xcl.toml`

| Name | Where read | What it does | Default |
|---|---|---|---|
| `XCL_ADAPTER` | `crates/xcl/src/gpu/adapter.rs:20` | Which D3D12 adapter to use: `auto`, `warp`, a number (N-th hardware adapter, or a model number matched in the name), or part of the adapter name. Overrides `d3d12_adapter`. Read by the title's device, the PC checks and the GPU tests. | `d3d12_adapter` from `xcl.toml` (`auto`) |
| `XCL_RAYTRACING` | `crates/xcl/src/config.rs:244` | Raytracing tier reported to the title: `auto`, `tier1_1` (`1.1`, `11`), `tier1_0` (`1.0`, `10`), `off` (`none`, `0`). Overrides `raytracing`. | `raytracing` from `xcl.toml` (`auto`) |
| `XCL_WINDOW_POS` | `crates/xcl/src/hle/d3d12x/present.rs:434` | XCL window position: `auto`, `center` or `x,y`. Overrides `window_pos`. | `window_pos` from `xcl.toml` (`auto`) |
| `XCL_UMA_MB` | `crates/xcl/src/gpu/uma.rs:737` | Size of the title's unified (GPU-visible) memory in MiB. The host copies `uma_mb` from the config into it when it is unset (crates/xcl/src/host.rs:28). | `uma_mb` from `xcl.toml`; 4095 for "advanced" title memory, else 2048 (crates/xcl/src/config.rs:110) |
| `XCL_D3D12_DEBUG` | `crates/xcl/src/hle/d3d12x/real.rs:471` | Turns on the D3D12 debug layer (needs Graphics Tools). Same as `d3d12_debug = true`. Errors and warnings go to the log (first 8 per message id, 3000 in all; ids 232-234 always); with telemetry on, every message also counts toward `debug_layer` / `debug_layer_count` events. | unset (off) |
| `XCL_D3D12_GBV` | `crates/xcl/src/hle/d3d12x/real.rs` (`init`) | GPU-based validation: `1` or `nostate` (`nostate` adds `DISABLE_STATE_TRACKING`). Turns the debug layer on too; slow. | unset (off) |

### Where tools and caches are

| Name | Where read | What it does | Default |
|---|---|---|---|
| `XCL_DXC_DIR` | `crates/xcl/src/gpu/recomp/dxc.rs:78` | Folder with `dxcompiler.dll` and `dxil.dll`; searched first, before the folder of `xcl.exe` and the Windows SDK. | unset |
| `XCL_D3D12_DIR` | `crates/xcl/src/gpu/agility.rs:30` | Folder with the Agility SDK `D3D12Core.dll`; searched first, then `<exe dir>\D3D12`, `<exe dir>`, `%LOCALAPPDATA%\XCL\D3D12` (crates/xcl/src/gpu/agility.rs:11-14). | unset |
| `XCL_OPUS_DIR` | `crates/xcl/src/hle/opus.rs:877` | Extra folder searched first for the libopus DLL. | unset (then the folder of `xcl.exe`, crates/xcl/src/hle/opus.rs:880) |
| `XCL_DXIL_CACHE` | `crates/xcl/src/gpu/recomp/dxc.rs:556` | Folder of the compiled-DXIL cache. | `%LOCALAPPDATA%\xcl\dxil` (crates/xcl/src/gpu/recomp/dxc.rs:559) |
| `XCL_DXC_THREADS` | `crates/xcl/src/gpu/recomp/dxc.rs:385` | Number of DXC compile threads (1 to 64). | CPU cores minus one |
| `XCL_PIPELINE_THREADS` | `crates/xcl/src/shaders.rs:763` | `xcl shaders --precompile --pipelines`: pipelines built on the GPU at once. | 4 |

### Memory

| Name | Where read | What it does | Default |
|---|---|---|---|
| `XCL_COHERENT_HEAPS` | `crates/xcl/src/hle/kernelx.rs:306` | `0` stops the title's GPU heaps (graphics-protection allocations) from going into the coherent unified-memory window. | on |
| `XCL_COHERENT_MB` | `crates/xcl/src/gpu/uma.rs:679` | Size of the coherent window in MiB (clamped to the window limits). | derived from `<data>\cache\uma_coherent.txt` (what the title asked for last run) plus XCL's own share, and only if enough commit is free (crates/xcl/src/gpu/uma.rs:683-693) |
| `XCL_VRAM_ARENAS` | `crates/xcl/src/hle/d3d12x/vram.rs:619` | When set, sampled title textures are not placed on shared 64 KiB video-memory pages (the caller makes a committed resource, crates/xcl/src/hle/d3d12x/vram.rs:619), and GPU-owned textures skip the shared-pages path and use the per-address arenas (crates/xcl/src/hle/d3d12x/vram.rs:634). | unset (pages used on a GPU with resource heap tier 2, crates/xcl/src/hle/d3d12x/vram.rs:599) |
| `XCL_UMA_CHECK` | `crates/xcl/src/hle/d3d12x/real.rs:338` | `0` skips the startup check that the GPU sees the CPU's writes to unified memory. | check on |
| `XCL_SYNC_SMALL_TARGETS` | `crates/xcl/src/hle/d3d12x/title_textures.rs:344` | `0` stops small CPU-filled colour render targets from being uploaded like textures. | on |

### Pipelines, shader translation and console lists

| Name | Where read | What it does | Default |
|---|---|---|---|
| `XCL_ASYNC_PSO` | `crates/xcl/src/hle/d3d12x/pso_async.rs:57` | `0` builds every pipeline inside the call instead of on a worker (a stand-in pipeline is used meanwhile otherwise). | on |
| `XCL_LAZY_PSO` | `crates/xcl/src/hle/d3d12x/pso_async.rs:105` | `0` builds pipelines when made instead of at first use. | on (when async pipelines are on) |
| `XCL_TRANSLATED_CACHE` | `crates/xcl/src/gpu/recomp/pso.rs:145` | `0` turns off the on-disk cache of translated programs. | on |
| `XCL_RECOMP_DXC` | `crates/xcl/src/gpu/shader.rs:382` | `1` runs DXC on the recompiler's translations as shaders are submitted (cached in the DXIL cache). | off |
| `XCL_SPLIT_WAVE64` | `crates/xcl/src/gpu/recomp/pso.rs:541` | `1` forces the split wave64 form (two 32-lane halves) also on GPUs with 64-lane waves. | off |
| `XCL_RECOMP_NO_STRUCTURED` | `crates/xcl/src/gpu/recomp/mod.rs:1387` | When set, programs stay on the guarded block chain instead of structured control flow. Disables the translated-program cache (crates/xcl/src/gpu/recomp/pso.rs:420). | unset |
| `XCL_RECOMP_WHY` | `crates/xcl/src/gpu/recomp/mod.rs:576` | Prints why a program stays on the guarded block chain. Disables the translated-program cache (crates/xcl/src/gpu/recomp/pso.rs:420). | unset |
| `XCL_RECOMP_WINDOWS` | `crates/xcl/src/shaders.rs:392` | `xcl shaders --recomp`: number of memory windows to translate for (a title with unified memory uses 4). | 1 |
| `XCL_RECOMP_ONLY` | `crates/xcl/src/shaders.rs:355` | `xcl shaders --recomp`: translate only the program whose name hash (16 hex digits) equals this. | unset |
| `XCL_X64_ALL_MASKS` | `crates/xcl/src/gpu/recomp/mod.rs:1339` | When set, the split wave64 form does not skip the halves' mask exchange for masks only read lane by lane. | unset |
| `XCL_WAVE64_SPIN` | `crates/xcl/src/gpu/recomp/emit.rs:4039` | When set, split wave64 kernels use spin-based sync instead of group barriers (for testing). | unset |
| `XCL_CONSOLE_LISTS` | `crates/xcl/src/hle/d3d12x/console_list.rs:50` | Enables the console command list path (command lists that console code builds inline are replayed). | off |
| `XCL_CONSOLE_LISTS_LOG` | `crates/xcl/src/hle/d3d12x/console_list.rs:887` | Logs a summary line per replayed console list. | off |
| `XCL_DSTORAGE_BCPACK_FILL` | `crates/xcl/src/hle/dstorage.rs:230` | Non-empty and not `0`: a refused BCPack destination is filled with solid BCn blocks (magenta; red for BC4/BC5) so the textures show; the request still fails. | off |

### Tracing and diagnostics (title runs)

| Name | Where read | What it does | Default |
|---|---|---|---|
| `XCL_TRACE_IMPORTS` | `crates/xcl/src/resolver.rs:280` | Logs arguments and result of calls to imports. `1`/`hle`: HLE functions; `all`: also system functions; other words are `dll!Function` prefixes, `-word` excludes. | unset (off) |
| `XCL_TRACE_FAULTS` | `crates/xcl/src/eh.rs:467` | Logs access violations as they are raised (the first 32), before the title's handlers. | off |
| `XCL_PROBE` | `crates/xcl/src/eh.rs:911` | Up to three hex RVAs in the title's exe: hardware breakpoints log the arguments each time they are hit with new values. | unset |
| `XCL_WATCH` | `crates/xcl/src/eh.rs:902` | One hex RVA in the title's exe: every write to those 8 bytes is logged with the writing code. | unset |
| `XCL_WATCH_DEVICE` | `crates/xcl/src/hle/d3d12x/real.rs:584` | Write-watches the title's D3D12 device object. | off |
| `XCL_WATCH_DMA` | `crates/xcl/src/hle/d3d12x/device_x.rs:107` | Write-watches each DMA command list the title creates. | off |
| `XCL_WATCH_FENCE` | `crates/xcl/src/hle/d3d12x/real.rs:2994` | n: logs writes over the first 8 bytes (the vtable) of the n-th fence. | unset |
| `XCL_DRED` | `crates/xcl/src/hle/d3d12x/real.rs:486` | Enables D3D12 DRED so a device removal logs the faulting address and the last GPU operations. | off |
| `XCL_D3D12_TRACE` | `crates/xcl/src/hle/d3d12x/real.rs:815` | Logs the D3D12 device methods the title calls. | off |
| `XCL_TRACE_FRAME` | `crates/xcl/src/hle/d3d12x/present.rs:183` | Logs what console code does with each resource it later presents (render-target binds, UAV views, copies, clears). | off |
| `XCL_TRACE_FRAME_EVENTS` | `crates/xcl/src/hle/d3d12x/frame.rs:317` | Logs the title's frame-event calls (the first 40). | off |
| `XCL_TRACE_ADDRESSES` | `crates/xcl/src/hle/d3d12x/bind.rs:194` | Counts and logs root descriptor addresses of console code. | off |
| `XCL_TRACE_SHADERS` | `crates/xcl/src/hle/d3d12x/bind.rs:472` | Comma-separated hex shader dump hashes: draws using them are traced with root arguments. | unset |
| `XCL_TRACE_MANY` | `crates/xcl/src/hle/d3d12x/bind.rs:563` | Number of draws traced. | 24 |
| `XCL_TRACE_UI_ANY` | `crates/xcl/src/hle/d3d12x/bind.rs:571` | Traces draws into any target, not only presented ones. | off |
| `XCL_LIST_TRACE` | `crates/xcl/src/hle/d3d12x/title_list.rs:46` | Keeps the last 64 slots called on the title's command list in the list's memory, with a counter. | off |
| `XCL_TRACE_APU` | `crates/xcl/src/hle/acphal.rs:85` | Non-empty and not `0`: logs the audio processor commands, structures and buffers a title sends. | off |
| `XCL_TRACE_MEM` | `crates/xcl/src/hle/kernelx.rs:372` | Logs the title's allocations of 64 KiB or more: the first 60, or the first n if n > 1. Also, set to anything, logs unified-memory window allocations of 1 MB or more (crates/xcl/src/gpu/uma.rs:534). | off |

### Dumps

| Name | Where read | What it does | Default |
|---|---|---|---|
| `XCL_DUMP_SHADERS` | `crates/xcl/src/gpu/shader.rs:325` | `1` writes every submitted shader (blob, FSSMs, disassembly, HLSL) to `<data>\shaders\` (crates/xcl/src/gpu/shader.rs:201). A retail game's dumps took 4.8 GB. | off |
| `XCL_DUMP_GS` | `crates/xcl/src/hle/d3d12x/real.rs:4076` | Folder where the VS and GS blobs of a VS+GS pipeline that could not be translated are written (the first 4). | unset |
| `XCL_FRAME_DUMP` | `crates/xcl/src/hle/d3d12x/present.rs:746` | `1` writes every 120th shown frame to the logs folder as `frame_<n>.bmp`; n > 1 means every n-th. | off |
| `XCL_FRAME_DUMP_FULL` | `crates/xcl/src/hle/d3d12x/present.rs:896` | With `XCL_FRAME_DUMP`: full-size dumps and sampling every pixel (default is every 4th). | off |
| `XCL_DUMP_HLSL` | `crates/xcl/src/shaders.rs:1277` | `xcl shaders --recomp`: folder where a sample of complete translations is written. | unset |
| `XCL_DUMP_EVERY` | `crates/xcl/src/shaders.rs:1278` | With `XCL_DUMP_HLSL`: every n-th translation by a hash of its name. | 100 |
| `XCL_EXAMPLE_HLSL` | `crates/xcl/src/shaders.rs:1214` | `xcl shaders --recomp --example ...`: file the example's HLSL is saved to. | unset |

### Per-pixel-shader debugging (hashes are shader dump hashes in hex)

| Name | Where read | What it does | Default |
|---|---|---|---|
| `XCL_DEBUG_LOD0` | `crates/xcl/src/gpu/recomp/pso.rs:239` | Comma-separated hashes: those pixel shaders sample at LOD 0. Disables the translated-program cache for pixel shaders (crates/xcl/src/gpu/recomp/pso.rs:422-430). | unset |
| `XCL_DEBUG_MAGENTA` | `crates/xcl/src/gpu/recomp/pso.rs:304` | `hash[:rrggbb],...`: those pixel shaders write one opaque colour (magenta by default) to target 0. | unset |
| `XCL_DEBUG_PS_EXPR` | `crates/xcl/src/gpu/recomp/pso.rs:257` | `hash:expression`: target 0 of that pixel shader gets the HLSL expression. | unset |
| `XCL_DEBUG_PS_SUB` | `crates/xcl/src/gpu/recomp/pso.rs:244` | `hash:from=>to\|\|from=>to`: text substitutions in that pixel shader's HLSL. | unset |
| `XCL_DEBUG_NO_CULL` | `crates/xcl/src/hle/d3d12x/real.rs:3461` | Comma-separated pixel-shader hashes: their pipelines draw without culling. | unset |
| `XCL_DEBUG_NO_STENCIL` | `crates/xcl/src/hle/d3d12x/real.rs:3458` | Comma-separated pixel-shader hashes: their pipelines draw without the stencil test. | unset |

### Tests: game data and GPU selection (tests pass without running when a sample variable is unset)

| Name | Where read | What it does | Default |
|---|---|---|---|
| `XCL_REQUIRE_GPU` | `crates/xcl/src/gpu/recomp/runner.rs:432` | `wave64`, `mesh` or both (comma-separated): a GPU test skipped for that reason fails instead. Set in scripts/test.ps1 (scripts/test.ps1:14). | unset |
| `XCL_SAMPLE_RPF` | `crates/xcl/src/shaders.rs:1259` | Path of the game's shader pack (`shaders.rpf`); many opt-in tests and the pack survey use it. | unset |
| `XCL_SAMPLE_DLLS` | `crates/xcl/tests/console_dlls_e2e.rs:14` | Folder of the game's console DLLs. | unset |
| `XCL_SAMPLE_TTPF` | `crates/xcl/src/gpu/texture.rs:668` | The game's texture archive. | unset |
| `XCL_SAMPLE_BK2` | `crates/xcl/tests/console_dlls_e2e.rs:206` | A Bink 2 movie of the game. | unset |
| `XCL_SAMPLE_BVH4` | `crates/xcl/tests/console_dlls_e2e.rs:279` | The game's `core_bvh4.rpf` package. | unset |
| `XCL_SAMPLE_AWC` | `crates/xcl/src/hle/opus.rs:1619` | An AWC file with one Opus stream. | unset |
| `XCL_SAMPLE_XG` | `crates/xcl/tests/xg_oracle.rs:14` | The console's own `xg_xs.dll`. | unset |
| `XCL_SAMPLE_DXC` | `crates/xcl/tests/console_shaders.rs:20` | The console's `dxcompiler_xs.dll` (with `xbsc_xs.dll` next to it). | unset |
| `XCL_SAMPLE_FSPX` | `crates/xcl/src/gpu/recomp/tests.rs:7163` | Folder of FSPX (driver) compute containers (`*.bin`). | unset |
| `XCL_TEST_MEM_GB` | `crates/xcl/tests/console_dlls_e2e.rs:15` | Memory the console-DLL tests reserve, in GiB. | 8 |
| `XCL_TEST_UNSET_8C1F` | `crates/xcl/src/hle/kernelx.rs:1813` | A name that must stay unset: a unit test checks reading it does not change the last error. | never set |

### Tests: probes and measurements (ignored or opt-in)

| Name | Where read | What it does | Default |
|---|---|---|---|
| `XCL_PROBE_BVH_DEVICE` | `crates/xcl/tests/console_dlls_e2e.rs:406` | Runs the BVH-builder device probe. | unset |
| `XCL_PROBE_BVH_FAKELIST` | `crates/xcl/tests/console_dlls_e2e.rs:523` | Runs the BVH-builder fake command-list probe. | unset |
| `XCL_PROBE_BVH_RECORD` | `crates/xcl/tests/console_dlls_e2e.rs:421` | Runs the BVH-builder command-list recording probe. | unset |
| `XCL_PROBE_BVH_GPU` | `crates/xcl/tests/console_dlls_e2e.rs:618` | Runs the BVH-builder GPU probe (also needs `XCL_CONSOLE_LISTS`). | unset |
| `XCL_PROBE_BVH_TLAS` | `crates/xcl/tests/console_dlls_e2e.rs:730` | Runs the TLAS probe (also needs `XCL_CONSOLE_LISTS`). | unset |
| `XCL_PROBE_FSR` | `crates/xcl/tests/console_dlls_e2e.rs:901` | Runs the FSR probe. | unset |
| `XCL_PROBE_FSR_DISPATCH` | `crates/xcl/tests/console_dlls_e2e.rs:923` | Makes the FSR probe go on to dispatch. | unset |
| `XCL_PROBE_FSR_EXECUTE` | `crates/xcl/tests/console_dlls_e2e.rs:982` | Makes the FSR probe execute the recorded work. | unset |
| `XCL_BVH_DUMP` | `crates/xcl/tests/console_dlls_e2e.rs:324` | File the BVH probes write their output to. | unset |
| `XCL_BINK_DUMP` | `crates/xcl/tests/console_dlls_e2e.rs:241` | `<file.ppm>`: saves the middle frame of the decoded movie. | unset |
| `XCL_PRINT` | `crates/xcl/tests/console_dlls_e2e.rs:177` | Prints the BVH size table. | unset |
| `XCL_MS_EVERY` | `crates/xcl/tests/gpu_pso_e2e.rs:615` | Samples every n-th mesh shader of the pack. | 1500 |
| `XCL_MS_SIGS` | `crates/xcl/tests/gpu_pso_e2e.rs:658` | Prints signatures of mesh/pixel pairs that fail to link. | unset |
| `XCL_KERNEL` | `crates/xcl/tests/console_shaders.rs:326` | Index of the kernel (in the test's table) to run through the console compiler. | 0 (S: all in the first test) |
| `XCL_KERNEL_HLSL` | `crates/xcl/tests/console_shaders.rs:393` | When set, prints the translated HLSL of that kernel. | unset |
| `XCL_BLOB` | `crates/xcl/src/gpu/recomp/tests.rs:4707` | A console compute container: prints the HLSL size and shape of its translation. | unset |
| `XCL_HLSL_OUT` | `crates/xcl/src/gpu/recomp/tests.rs:4711` | File (or folder, in the shader-cost test) where those tests write HLSL. | unset |
| `XCL_DXC_ARGS` | `crates/xcl/src/gpu/recomp/tests.rs:4734` | Extra DXC arguments for the timing tests (e.g. `-O3`). | empty |
| `XCL_RAW` | `crates/xcl/src/gpu/recomp/tests.rs:4748` | A file of raw RDNA2 code: prints its disassembly. | unset |
| `XCL_RAW_OUT` | `crates/xcl/src/gpu/recomp/tests.rs:5130` | Folder where the survey writes the raw programs it shows. | unset |
| `XCL_NAMES` | `crates/xcl/src/gpu/recomp/tests.rs:4989` | Comma-separated substrings of record names selecting shaders. | unset |
| `XCL_DUMP` | `crates/xcl/src/gpu/recomp/tests.rs:3410` | Comma-separated shader names whose signatures are printed. | unset |
| `XCL_SURVEY_DXC` | `crates/xcl/src/gpu/recomp/tests.rs:4815` | Also compile each surveyed program with DXC. | off |
| `XCL_SURVEY_EVERY` | `crates/xcl/src/gpu/recomp/tests.rs:4769` | Survey every n-th record. | 1 |
| `XCL_SURVEY_FIND` | `crates/xcl/src/gpu/recomp/tests.rs:4782` | Only records whose name contains this text. | unset |
| `XCL_SURVEY_SKIP` | `crates/xcl/src/gpu/recomp/tests.rs:4786` | `a\|b`: leave out records whose name contains one of these. | unset |
| `XCL_SURVEY_ONLY` | `crates/xcl/src/gpu/recomp/tests.rs:4874` | `a\|b`: DXC only for programs whose HLSL contains one of these. | unset |
| `XCL_SURVEY_SEED` | `crates/xcl/src/gpu/recomp/tests.rs:4772` | Random sampling seed (a non-number picks a time-based one). | unset |
| `XCL_SURVEY_TRACE` | `crates/xcl/src/gpu/recomp/tests.rs:4855` | Prints start, time and result per program. | off |
| `XCL_SURVEY_WINDOWS` | `crates/xcl/src/gpu/recomp/tests.rs:4854` | Memory windows the survey translates for. | 1 |
| `XCL_CENSUS_OUT` | `crates/xcl/src/gpu/recomp/tests.rs:5181` | File the wave64 lane census is written to (with `XCL_SAMPLE_RPF`). | unset |
| `XCL_CENSUS_FULL` | `crates/xcl/src/gpu/recomp/tests.rs:5182` | Whole programs in the census listing. | off |
| `XCL_CENSUS_UNKNOWN` | `crates/xcl/src/gpu/recomp/tests.rs:5183` | Only programs with an unresolved lane read. | off |
| `XCL_ISA_FULL` | `crates/xcl/src/gpu/recomp/tests.rs:5099` | Do not shorten the list of missing mnemonics per encoding. | off |
| `XCL_FORMS_NO_DXC` | `crates/xcl/src/gpu/recomp/tests.rs:5300` | Skip the DXC compiles in the forms test. | off |
| `XCL_FUZZ_ITERS` | `crates/xcl/src/gpu/recomp/tests.rs:6173` | Number of dispatches in the float fuzz tests. | 50 |
| `XCL_GALLERY_DIR` | `crates/xcl/src/gpu/recomp/tests.rs:2894` | Folder where the shader gallery test writes images. | unset |
| `XCL_TIME_HLSL` | `crates/xcl/src/gpu/recomp/tests.rs:2844` | A saved HLSL file to time at DXC optimisation levels O0 to O3. | unset |
| `XCL_TIME_PROFILE` | `crates/xcl/src/gpu/recomp/tests.rs:2846` | Shader profile for that timing. | `cs_6_6` |
| `XCL_EARLY_PAGES` | `crates/xcl/src/hle/d3d12x/sparse.rs:1872` | Comma-separated page sizes for the early-pages timing test. | 4096,8192,16384,32768 |
| `XCL_SCATTER_PAGES` | `crates/xcl/src/hle/kernelx.rs:2173` | Comma-separated page sizes for the scatter-mapping timing test. | 4096,8192,16384 |
| `XCL_OPUS_WAV` | `crates/xcl/src/hle/opus.rs:1638` | File where the Opus decode test writes its result to listen to. | unset |

### Scripts

| Name | Where read | What it does | Default |
|---|---|---|---|
| `XCL_UPSTREAM_URL` | `scripts/dev/sync-upstream.sh:7` | Upstream repository `scripts/dev/sync-upstream.sh` fetches. | `https://github.com/ivall/xcl.git` |
| `XCL_UPSTREAM_BRANCH` | `scripts/dev/sync-upstream.sh:8` | Upstream branch it mirrors. | `main` |

### Names that look like variables but are not

These `XCL_*` strings appear in the source but are not environment variables.

| Name | What it is | Where |
|---|---|---|
| `XCL_TLS_CALLBACK` | Rust static that places the TLS callback in `.CRT$XLY` | `crates/xcl/src/main.rs:18` |
| `XCL_SLOT` | Rust static holding the TLS slot xcl.exe's own TLS moved to | `crates/xcl/src/tls.rs:250` |
| `XCL_CONSOLE` | HLSL macro in the console-compiler test's shader source | `crates/xcl/tests/console_shaders.rs:89` |
| `XCL_PIXEL` | HLSL macro the recompiler defines for pixel-shader translations | `crates/xcl/src/gpu/recomp/emit.rs:4084` |
| `XCL_MEM_ACCESSORS` | placeholder comment in generated HLSL where memory accessors are inserted | `crates/xcl/src/gpu/recomp/emit.rs:4914` |
| `XCL_X_THREADS`, `XCL_X_WAVES`, `XCL_X_SLOT`, `XCL_X_SPIN`, `XCL_X_LDS_TOP`, `XCL_XS`, `XCL_XC`, `XCL_XW`, `XCL_WT` | HLSL macros of the split wave64 form | `crates/xcl/src/gpu/recomp/emit.rs:4057`-`4064`, `crates/xcl/src/gpu/recomp/emit.rs:4236` |

`HANDOFF.md:1594` mentions `XCL_PRESS_A_AT`; no code, script or test in the repository reads it.
