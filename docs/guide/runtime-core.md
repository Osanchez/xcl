# Runtime core

The part of `crates/xcl/src` that starts a title and keeps it running: the launcher, the PE
loader, import resolution, TLS and exception support, the virtual file system, state, report and
log, plus the small `xcl inspect` / `disasm` / `rpf` tools. The console libraries themselves are
in `hle/` (see `hle.md`), Direct3D 12.X in `hle/d3d12x/` (see `d3d12x.md`), and the GPU code
recompiler in `gpu/` (see `gpu-recompiler.md`); this page only says where they plug in.

**Platform.** XCL builds and runs on Windows only (x64, `stable-x86_64-pc-windows-msvc`); on macOS or
Linux the crate does not build (`AGENTS.md:30`, `README.md:94`). Everything below describes
Windows behaviour read from the source, not run on the machine that wrote this page.

Module list: `crates/xcl/src/lib.rs:9` to `crates/xcl/src/lib.rs:35`. `rpf_probe` is private
(`crates/xcl/src/lib.rs:28`), every other module is `pub`.

## How a title starts

`xcl run` is two processes: a launcher and a host. Both are the same `xcl.exe`; the environment
variable `XCL_HOST` tells them apart (`crates/xcl/src/main.rs:48`).

1. **Launcher** (`crates/xcl/src/launcher.rs:288`, `run`). Resolves the target to a package root,
   exe and title name (`crates/xcl/src/launcher.rs:237`), loads or creates `<data>\xcl.toml`
   (`crates/xcl/src/launcher.rs:297`), creates the data sub-folders (`crates/xcl/src/launcher.rs:320`), reads
   `MicrosoftGame.config` and fills the "auto" settings (`crates/xcl/src/launcher.rs:324`-`325`), writes
   `cache\boot.json` (`crates/xcl/src/launcher.rs:327`-`329`), rotates the previous log and report to
   `*.prev.*` (`crates/xcl/src/launcher.rs:335`-`338`), runs the PC checks (`crates/xcl/src/launcher.rs:353`) and the CPU
   check (`crates/xcl/src/launcher.rs:358`), then starts a child `xcl.exe` with `XCL_HOST=<boot.json>`, the
   game's command line and the package root as working directory (`crates/xcl/src/launcher.rs:433`-`470`).
   It waits, merges the exit code and the PC checks into `report.json` and prints a summary
   (`crates/xcl/src/launcher.rs:472`).
2. **Host** (`crates/xcl/src/host.rs:11`, `host_main`). Reads `boot.json`, opens the log
   (`crates/xcl/src/host.rs:19`), sets `XCL_UMA_MB` from the config if unset (`crates/xcl/src/host.rs:28`), starts the optional
   timers (`XCL_ACTIVITY`, `XCL_THREAD_DUMP_AT`, `XCL_RUN_SECONDS`: `crates/xcl/src/host.rs:33`, `crates/xcl/src/host.rs:81`,
   `crates/xcl/src/host.rs:96`), initialises the stub database and HLE registry (`crates/xcl/src/host.rs:106`-`107`), sets the
   DLL search folders (`crates/xcl/src/host.rs:115`), builds the VFS (`crates/xcl/src/host.rs:119`), installs the file hooks
   (`crates/xcl/src/host.rs:122`) and the exception hooks (`crates/xcl/src/host.rs:125`-`129`), and sets `HOST_MODE`
   (`crates/xcl/src/host.rs:130`) so thread notifications reach the loader.
3. **PE load** (`crates/xcl/src/loader.rs:312`, `load_main` -> `map_image_ex`,
   `crates/xcl/src/loader.rs:64`). Parses the file (`pe.rs`), reserves memory at the preferred base (or relocates),
   copies headers and sections, applies relocations, resolves **all** imports eagerly
   (normal and delay-load, `crates/xcl/src/loader.rs:163`-`184`), sets up static TLS (`crates/xcl/src/loader.rs:187`), applies
   section protections (`crates/xcl/src/loader.rs:207`), registers `.pdata` (`crates/xcl/src/loader.rs:225`) and hooks MSVC
   stack-cookie failure (`crates/xcl/src/loader.rs:230`).
4. **Import resolution** (`crates/xcl/src/resolver.rs:315`, `resolve_import`). Routing table
   (HLE module, `@system`, `@stub`) -> DLL shipped in the game folder (mapped recursively by the
   loader) -> real Windows DLL -> any HLE module that has the name -> logging stub
   (`crates/xcl/src/resolver.rs:211`-`262`).
5. **TLS and exceptions.** The image's TLS directory is served by a generated "carrier DLL"
   (`crates/xcl/src/tls.rs:451`); the title's exe takes TLS slot 0 (`crates/xcl/src/tls.rs:310`). Exceptions:
   `RtlAddFunctionTable` for `.pdata` (`crates/xcl/src/eh.rs:20`) and a hook on
   `RtlPcToFileHeader` (`crates/xcl/src/eh.rs:45`).
6. **Entry point** (`crates/xcl/src/loader.rs:337`, `run_main`). Back in the host: `kernelx::adopt_title_hooks`
   (`crates/xcl/src/host.rs:140`), a first `report.json` (`crates/xcl/src/host.rs:141`), then `run_main` sets
   `PEB.ImageBaseAddress` (not under Wine, `crates/xcl/src/loader.rs:344`-`348`), runs the exe's TLS callbacks,
   and runs the entry point on a new thread with the stack size from the PE header
   (`crates/xcl/src/loader.rs:352`-`367`). The entry normally never returns (the title calls `ExitProcess`).
   `xcl.exe`'s own TLS callback (`crates/xcl/src/main.rs:10`, `crates/xcl/src/main.rs:18`) forwards thread attach/detach to
   `loader::on_tls_event` (`crates/xcl/src/loader.rs:384`); process detach writes the final report
   (`crates/xcl/src/loader.rs:413`-`417`).

Exit codes of the host: `0x7C10001` boot record unreadable (`crates/xcl/src/host.rs:16`), `0x7C10002` loading
the exe failed (`crates/xcl/src/host.rs:137`). The launcher itself returns 2 for its own errors
(`crates/xcl/src/launcher.rs:293`, `crates/xcl/src/launcher.rs:301`, `crates/xcl/src/launcher.rs:316`).

## `pe.rs` - PE32+ parser

- **Purpose.** Small, bounds-checked x64 PE parser over either a file image (`Layout::File`) or a
  mapped image (`Layout::Mapped`) (`crates/xcl/src/pe.rs:1`-`8`, `crates/xcl/src/pe.rs:55`).
- **Key items.** `Pe::parse` (`crates/xcl/src/pe.rs:229`), `imports` (`crates/xcl/src/pe.rs:430`), `delay_imports` (`crates/xcl/src/pe.rs:455`),
  `relocations` (`crates/xcl/src/pe.rs:490`), `tls` (`crates/xcl/src/pe.rs:526`), `exports` (`crates/xcl/src/pe.rs:546`), `rva_to_offset`
  (`crates/xcl/src/pe.rs:325`), `OwnedExports` (`crates/xcl/src/pe.rs:599`), `PeError` (`crates/xcl/src/pe.rs:34`), data directory indices
  (`crates/xcl/src/pe.rs:16`-`23`). A malformed table gives a `PeError`, not undefined behaviour (`crates/xcl/src/pe.rs:8`).
- **Interacts.** Used by `loader.rs`, `inspect.rs`, `tls.rs` (the carrier is itself built as a PE),
  `gpu/agility.rs`.
- **Env vars.** None.
- **Tests.** `pe.rs` `mod tests` (`crates/xcl/src/pe.rs:625`): overflow checks on RVAs (`crates/xcl/src/pe.rs:671`-`724`), real
  system files (`crates/xcl/src/pe.rs:742`), malformed tables (`crates/xcl/src/pe.rs:761`), TLS of the test binary (`crates/xcl/src/pe.rs:792`).
- **Known limits.** x64 only (`IMAGE_FILE_MACHINE_AMD64`, `crates/xcl/src/pe.rs:12`). Files from an encrypted or
  XVC package do not parse; `xcl inspect` says so (`crates/xcl/src/inspect.rs:76`-`83`). See
  `HANDOFF.md:31` (PE loader).

## `loader.rs` - mapping and initialisation

- **Purpose.** Maps console executables and DLLs into the current process, wires imports, runs
  TLS callbacks and `DllMain` (`crates/xcl/src/loader.rs:1`-`2`).
- **Key items.** `map_image` (`crates/xcl/src/loader.rs:60`), `map_image_ex` (`crates/xcl/src/loader.rs:64`), `init_dll`
  (`crates/xcl/src/loader.rs:257`), `load_game_dll` (idempotent, `crates/xcl/src/loader.rs:291`), `load_main` (`crates/xcl/src/loader.rs:312`),
  `run_main` (`crates/xcl/src/loader.rs:337`), `on_tls_event` (`crates/xcl/src/loader.rs:384`), `set_cache_dir` (`crates/xcl/src/loader.rs:29`).
- **Behaviour worth knowing.**
  - If the preferred base is taken and the image has no relocations, loading fails
    (`crates/xcl/src/loader.rs:75`-`80`); only `DIR64` and `HIGHLOW` relocations are applied, others are warned
    about (`crates/xcl/src/loader.rs:119`-`127`).
  - The module is registered **before** its imports are resolved so cyclic game-DLL imports end
    (`crates/xcl/src/loader.rs:147`-`148`); on an import-table error it is unregistered again (`crates/xcl/src/loader.rs:163`-`170`).
  - A DLL whose `DllMain` returns FALSE is unregistered, as Windows would not keep it
    (`crates/xcl/src/loader.rs:301`-`307`). Loads are serialised by a lock with a re-entrancy counter
    (`crates/xcl/src/loader.rs:19`-`25`, `crates/xcl/src/loader.rs:272`).
  - Thread notifications are ignored until the main image is ready (`crates/xcl/src/loader.rs:26`, `crates/xcl/src/loader.rs:389`).
- **Interacts.** `pe`, `modules` (registry), `resolver` (imports), `tls`, `eh`, `report`.
- **Env vars.** None directly.
- **Tests.** `loader.rs` `mod tests` (`crates/xcl/src/loader.rs:422`): a failing `DllMain` is not left loaded
  (`crates/xcl/src/loader.rs:489`), a broken import table is not left registered (`crates/xcl/src/loader.rs:509`). The whole
  path is exercised by the end-to-end test with the test title (`crates/xcl/tests/e2e.rs:88`).
- **Known limits.** `HANDOFF.md:31`; under Wine `PEB.ImageBaseAddress` is left alone
  (`crates/xcl/src/loader.rs:344`; `HANDOFF.md:1696`, section "Retail game bring-up, 2026-11-23" at
  `HANDOFF.md:1728` lists later merges).

## `resolver.rs` - import policy

- **Purpose.** Decides what each `dll!function` import points at (`crates/xcl/src/resolver.rs:1`-`8`).
- **Key types and functions.** `Source` (`crates/xcl/src/resolver.rs:24`: `Hle`, `HleOther`, `System`, `GameDll`,
  `Stub`), `ImportRecord` (`crates/xcl/src/resolver.rs:37`), `resolve_import` (records the outcome for the
  report, `crates/xcl/src/resolver.rs:315`), `classify` (dry run for `xcl inspect`, `crates/xcl/src/resolver.rs:335`),
  `load_library` / `get_proc_address` (runtime semantics for the title's own calls,
  `crates/xcl/src/resolver.rs:370`, `crates/xcl/src/resolver.rs:414`), `find_game_dll` (`crates/xcl/src/resolver.rs:141`),
  `set_search_dirs` (`crates/xcl/src/resolver.rs:49`).
- **Order** (`crates/xcl/src/resolver.rs:211`-`262`): registry route (`hle::registry().route`, first matching glob wins,
  `crates/xcl/src/hle/mod.rs:207`) -> mapped or shipped game DLL -> real Windows DLL ->
  `find_any` across HLE modules -> stub. `get_proc_address` returns `None` instead of a stub, so
  title probing of optional APIs works (`crates/xcl/src/resolver.rs:412`-`413`).
- **Interacts.** `hle` (routes and modules), `stubs`, `loader` (maps game DLLs), `modules`, `vfs`
  (`load_library` consults the VFS, `crates/xcl/src/resolver.rs:392`-`394`), `report` (reads `records()`).
- **Env vars.** `XCL_TRACE_IMPORTS` (`crates/xcl/src/resolver.rs:280`): `1`/`hle` (default when set) traces every HLE
  function; `all` adds system functions; other words are `dll!Function` prefixes to include,
  `-word` to exclude (`crates/xcl/src/resolver.rs:264`-`268`, `crates/xcl/src/resolver.rs:283`-`296`). Tracing wraps the address
  with `hle::d3d12x::guard::make_call_tracer` (`crates/xcl/src/resolver.rs:319`-`322`).
- **Tests.** `resolver.rs` `mod tests` (`crates/xcl/src/resolver.rs:446`): user32 and legacy DLLs reach PC
  functions (`crates/xcl/src/resolver.rs:452`), advapi32 through kernelx (`crates/xcl/src/resolver.rs:483`), alias stubs inherit
  the canonical module default (`crates/xcl/src/resolver.rs:492`), ordinal imports (`crates/xcl/src/resolver.rs:517`), forwarders
  (`crates/xcl/src/resolver.rs:532`).
- **Known limits.** `HANDOFF.md:35` (resolver bullet).

## `modules.rs` - table of known modules

- **Purpose.** Process-wide table of mapped images plus pseudo handles for HLE modules
  (`crates/xcl/src/modules.rs:1`-`4`).
- **Key items.** `MappedModule` (`crates/xcl/src/modules.rs:15`), `register_mapped` (`crates/xcl/src/modules.rs:46`),
  `unregister_mapped` (`crates/xcl/src/modules.rs:57`), `image_base_for_addr` (lock-free, `crates/xcl/src/modules.rs:67`),
  `find_mapped_by_name` / `by_base` / `by_addr` (`crates/xcl/src/modules.rs:83`-`92`), `set_main` / `main_module`
  (`crates/xcl/src/modules.rs:96`, `crates/xcl/src/modules.rs:100`), `hle_handle` (`crates/xcl/src/modules.rs:111`), `describe_address`
  (`"game.exe+0x1234"`, `crates/xcl/src/modules.rs:152`).
- **Why lock-free.** Exception dispatch (`RtlPcToFileHeader` hook) looks addresses up here and must
  never deadlock, so the address ranges are a fixed array (`crates/xcl/src/modules.rs:5`-`6`, `crates/xcl/src/modules.rs:41`-`44`).
- **Interacts.** `loader`, `resolver`, `eh`, `stubs` and `report` (all use `describe_address`).
- **Env vars.** None.
- **Tests.** `modules.rs` `mod tests` (`crates/xcl/src/modules.rs:183`): a base above the address does not panic
  (`crates/xcl/src/modules.rs:187`), arbitrary stack words (`crates/xcl/src/modules.rs:199`).
- **Known limits.** The lock-free table holds 512 ranges (`crates/xcl/src/modules.rs:41`); images beyond that are
  in the locked list only.

## `stubs.rs` - automatic stubs and the stub database

- **Purpose.** Every unresolved import gets a 48-byte x64 thunk that logs the first call, counts
  calls and returns a configured value (`crates/xcl/src/stubs.rs:1`-`14`, `crates/xcl/src/stubs.rs:40`).
- **Key items.** `StubInfo` (`crates/xcl/src/stubs.rs:24`), `make_stub` / `make_stub_info` (`crates/xcl/src/stubs.rs:111`,
  `crates/xcl/src/stubs.rs:116`), `record_call` (`crates/xcl/src/stubs.rs:44`), `all_stubs` (`crates/xcl/src/stubs.rs:158`), `StubDb`
  (`crates/xcl/src/stubs.rs:176`), `return_for_with_fallback` (`crates/xcl/src/stubs.rs:226`), `init_db` (`crates/xcl/src/stubs.rs:257`),
  `alloc_code` (RWX arena, `crates/xcl/src/stubs.rs:84`).
- **Return value lookup** (`crates/xcl/src/stubs.rs:226`-`249`): `dll!name` -> canonical module's `dll!name` ->
  `*!name` -> module default -> canonical module default -> 0. The built-in data is
  `crates/xcl/data/stubdb.toml` (`[module_defaults]` at
  `crates/xcl/data/stubdb.toml:8`, `[functions]` at `crates/xcl/data/stubdb.toml:24`, ordinals at
  `crates/xcl/data/stubdb.toml:36`), loaded at `crates/xcl/src/stubs.rs:184`. Per-title overrides come from
  `stub_returns` in `xcl.toml` (`crates/xcl/src/stubs.rs:194`-`198`, `crates/xcl/src/host.rs:106`).
- **Interacts.** `resolver` (creates stubs), `report` (`called_stubs`, `crates/xcl/src/report.rs:113`-`123`),
  `hle::d3d12x` (uses `new_info` for slot guards, `crates/xcl/src/stubs.rs:70`), `eh` (`alloc_code` for the
  stack-cookie thunk, `crates/xcl/src/eh.rs:195`).
- **Env vars.** None.
- **Tests.** `stubs.rs` `mod tests` (`crates/xcl/src/stubs.rs:265`): configured return value and call count
  (`crates/xcl/src/stubs.rs:269`), built-in database with overrides (`crates/xcl/src/stubs.rs:282`), concurrent creation
  (`crates/xcl/src/stubs.rs:302`).
- **Known limits.** Argument registers are clobbered and ignored (`crates/xcl/src/stubs.rs:13`-`14`). Most GDK
  APIs default to `E_NOTIMPL` rather than a fake success (`crates/xcl/data/stubdb.toml:9`-`10`).
  `HANDOFF.md:35`.

## `hooks.rs` - inline hooks

- **Purpose.** One function, `hook(module, func, detour, orig)`, that installs a `retour` inline
  detour on a real Windows export and stores the trampoline in `orig`; the detour is leaked on
  purpose (`crates/xcl/src/hooks.rs:11`-`13`, `crates/xcl/src/hooks.rs:27`).
- **Used by.** `eh.rs` (`RtlPcToFileHeader`, `NtTerminateProcess`: `crates/xcl/src/eh.rs:47`, `crates/xcl/src/eh.rs:146`) and
  `vfs_hooks.rs` (`crates/xcl/src/vfs_hooks.rs:1224`).
- **Env vars / tests.** None. Covered through the VFS hook tests (`crates/xcl/src/vfs_hooks.rs:1237`) and the
  end-to-end test.
- **Known limits.** None recorded; a hook that fails is logged and the run continues
  (`crates/xcl/src/host.rs:122`-`127`).

## `tls.rs` - static TLS for manually mapped images

- **Problem.** The Windows loader allocates static-TLS blocks only for modules it loaded itself
  (`crates/xcl/src/tls.rs:3`-`5`).
- **Solution ("TLS carrier").** For each image with a TLS directory, XCL generates a tiny DLL whose
  TLS template is at least as big, writes it to `<data>\cache\xcl_tls_<pid>_<n>_<name>.dll` and loads
  it with `LoadLibraryW` (`crates/xcl/src/tls.rs:451`-`503`). Windows then owns a real TLS slot; XCL writes that
  index into the image's `_tls_index` and copies the image's template into the block of every
  thread (`crates/xcl/src/tls.rs:7`-`16`, `crates/xcl/src/tls.rs:397`). New threads are handled from `xcl.exe`'s TLS callback
  (`on_thread_attach`, `crates/xcl/src/tls.rs:385`; `crates/xcl/src/main.rs:10`).
- **Slot 0.** The title's exe is moved to slot 0 and `xcl.exe`'s own TLS to the carrier's slot,
  because code built with `/GA` reads slot 0 without `_tls_index`: `take_slot_zero`
  (`crates/xcl/src/tls.rs:310`), called from `crates/xcl/src/loader.rs:237`-`246`.
- **Key items.** `build_carrier_dll` (`crates/xcl/src/tls.rs:81`), `carrier_template_size_for` (`crates/xcl/src/tls.rs:76`),
  `install` (`crates/xcl/src/tls.rs:451`), `take_slot_zero` (`crates/xcl/src/tls.rs:310`), `on_thread_attach` / `on_thread_detach`
  (`crates/xcl/src/tls.rs:385`, `crates/xcl/src/tls.rs:374`), `cleanup_cache` (`crates/xcl/src/tls.rs:505`, called by the launcher at
  `crates/xcl/src/launcher.rs:326`).
- **Findings baked in.** ntdll processes a dynamic DLL's TLS directory only if it has an import table
  (the carrier imports `KERNEL32!GetTickCount`), and `SizeOfZeroFill` is ignored (`crates/xcl/src/tls.rs:17`-`23`).
- **Env vars.** None.
- **Tests.** `tls.rs` `mod tests` (`crates/xcl/src/tls.rs:516`): carrier is a valid PE (`crates/xcl/src/tls.rs:522`), size rounding
  (`crates/xcl/src/tls.rs:541`), variant probe (`crates/xcl/src/tls.rs:551`), Windows allocates the block for all threads
  (`crates/xcl/src/tls.rs:567`), install without an index variable (`crates/xcl/src/tls.rs:601`). Integration: the e2e test
  title's DLL has static TLS and a `DllMain` (`tests/testgamedll/src/lib.rs:2`, `tests/testgamedll/src/lib.rs:10`).
- **Known limits.** `HANDOFF.md:32` (Static TLS bullet). The slot-0 change is described in the
  bullet at `HANDOFF.md:1566`, inside the section "XDefiant (...) - first runs" (`HANDOFF.md:1311`).
  `take_slot_zero` fails (and the exe keeps its carrier slot) if `xcl.exe`'s TLS index is not 0
  (`crates/xcl/src/tls.rs:312`-`314`, warning at `crates/xcl/src/loader.rs:242`).

## `eh.rs` - exceptions, crashes and CPU-gap emulation

- **Purpose.** Everything that happens when something goes wrong in or around the title
  (`crates/xcl/src/eh.rs:1`-`7`).
- **Key items.**
  - `register_function_table` -> `RtlAddFunctionTable` (`crates/xcl/src/eh.rs:20`); `install_pc_hook` ->
    `RtlPcToFileHeader` so C++ `throw` finds the image base of mapped images (`crates/xcl/src/eh.rs:45`).
  - `install_crash_handler` (`crates/xcl/src/eh.rs:144`): hooks `NtTerminateProcess` to log how the process ends
    (`crates/xcl/src/eh.rs:121`-`142`), sets the unhandled-exception filter (logs module+RVA and registers,
    writes `report.json` with the crash, then calls the title's own filter, `crates/xcl/src/eh.rs:64`-`119`),
    installs an invalid-parameter handler (`crates/xcl/src/eh.rs:536`) and a vectored handler (`crates/xcl/src/eh.rs:151`).
  - Vectored handler (`cpp_exception_logger`, `crates/xcl/src/eh.rs:440`): names threads from the debugger
    thread-name exception, emulates console-only CPU instructions on `0xC000001D`
    (`emulate_console_only`, `crates/xcl/src/eh.rs:315`), logs C++ exceptions thrown by title code (first 16).
  - `hook_gs_failure` (`crates/xcl/src/eh.rs:187`): patches MSVC's `__security_check_cookie` failure path to report
    the caller before ending the process; pattern in `GS_CHECKS` (`crates/xcl/src/eh.rs:159`).
  - Diagnostics: `dump_threads` (`crates/xcl/src/eh.rs:652`), `watch_writes` (`crates/xcl/src/eh.rs:744`), `start_probes`
    (`crates/xcl/src/eh.rs:899`).
- **Interacts.** `loader` (registers tables, hooks), `modules` (`describe_address`, `image_base_for_addr`),
  `insn` (decoder), `report`, `hooks`, `hle::kernelx` (title's filter, `crates/xcl/src/hle/kernelx.rs:29`).
- **Env vars.** `XCL_TRACE_FAULTS` (`crates/xcl/src/eh.rs:467`), `XCL_PROBE` (`crates/xcl/src/eh.rs:911`), `XCL_WATCH`
  (`crates/xcl/src/eh.rs:902`); `XCL_THREAD_DUMP_AT` is read in `crates/xcl/src/host.rs:81` and calls `dump_threads`
  (`crates/xcl/src/host.rs:90`); `XCL_WATCH_DEVICE` is read in `crates/xcl/src/hle/d3d12x/real.rs:584` and calls `watch_writes`.
  Semantics are in the table in `cli-and-config.md`.
- **Tests.** `eh.rs` `mod tests` (`crates/xcl/src/eh.rs:973`): bounded string read (`crates/xcl/src/eh.rs:978`), cookie-check
  patterns (`crates/xcl/src/eh.rs:996`), `clzero` emulation (`crates/xcl/src/eh.rs:1007`, `crates/xcl/src/eh.rs:1050`), `cmp` flags
  (`crates/xcl/src/eh.rs:1038`), code bytes on the instructions page (`crates/xcl/src/eh.rs:1118`).
- **Known limits.** `HANDOFF.md:1817` (section "Console CPU instructions the PC lacks (2026-11-23)").
  SHA-NI is not emulated (`HANDOFF.md:1825`). A title whose own crash filter ends the process leaves
  no XCL crash line unless `XCL_TRACE_FAULTS=1` is set (`crates/xcl/src/eh.rs:462`-`463`). `HANDOFF.md:34` for the
  C++ exception mechanism.

## `insn.rs` - console-only CPU instructions

- **Purpose.** Decoder and semantics for Zen 2 instructions that some PCs lack: `clwb`, `clzero`
  (single and loop), `rdpru`, `monitorx` / `mwaitx`, `rdpid`, SSE4a `extrq` / `insertq`, and `ud2`
  (the title's own trap, not emulated) (`crates/xcl/src/insn.rs:1`-`5`, `crates/xcl/src/insn.rs:9`-`35`).
- **Key items.** `ConsoleOnly` (`crates/xcl/src/insn.rs:9`), `decode` (`crates/xcl/src/insn.rs:88`), `extrq` / `insertq` bit-field
  helpers (`crates/xcl/src/insn.rs:46`, `crates/xcl/src/insn.rs:57`), `hex` (`crates/xcl/src/insn.rs:165`).
- **Interacts.** Only `eh.rs` calls `decode` (`crates/xcl/src/eh.rs:323`). The launcher's CPU check lists the
  extensions it requires at `crates/xcl/src/launcher.rs:394`-`405`.
- **Env vars.** None (the launcher's `XCL_SKIP_CPU_CHECK` is separate, `crates/xcl/src/launcher.rs:360`).
- **Tests.** `insn.rs` `mod tests` (`crates/xcl/src/insn.rs:170`): `sse4a_bit_fields` (`crates/xcl/src/insn.rs:175`),
  `decodes_the_console_only_instructions` (`crates/xcl/src/insn.rs:188`).
- **Known limits.** `HANDOFF.md:1817`, `HANDOFF.md:1825`.

## `launcher.rs` - `xcl run`

- **Purpose.** Prepare a run and start the host (`crates/xcl/src/launcher.rs:1`-`2`).
- **Key items.**
  - `data_root` (`crates/xcl/src/launcher.rs:10`): `%XCL_DATA_ROOT%` or `%LOCALAPPDATA%\XCL`.
  - Target resolution: `resolve_location` (`crates/xcl/src/launcher.rs:237`), `resolve_target` (`crates/xcl/src/launcher.rs:253`),
    `pick_exe` (`crates/xcl/src/launcher.rs:180`), `config_root` (package folder up to 4 levels above the exe,
    `crates/xcl/src/launcher.rs:225`), `is_xcl_copy` (`xcl.exe` is never picked as a game, `crates/xcl/src/launcher.rs:142`).
  - `MicrosoftGame.config` text handling: `decode_game_config` (`crates/xcl/src/launcher.rs:91`; UTF-8 or UTF-16
    with BOM, comments stripped), `xml_attrs` (`crates/xcl/src/launcher.rs:19`), `read_game_config`
    (`crates/xcl/src/launcher.rs:135`), `display_name` (`crates/xcl/src/launcher.rs:130`).
  - `run` (`crates/xcl/src/launcher.rs:288`), `spawn_host` (`crates/xcl/src/launcher.rs:433`), `summarize` (`crates/xcl/src/launcher.rs:472`).
  - CPU check: `missing_console_cpu_features` (`crates/xcl/src/launcher.rs:417`) requires SSE4.2, POPCNT, AVX, AVX2,
    FMA, F16C, BMI1, BMI2, LZCNT, MOVBE (`crates/xcl/src/launcher.rs:394`-`405`).
- **Exe selection order** (`crates/xcl/src/launcher.rs:304`-`319`): the exe given on the command line, else the one
  `xcl.toml` names (`exe`, relative to the package root), else `pick_exe`: first
  `<Executable Name>` of `MicrosoftGame.config` that exists (`crates/xcl/src/launcher.rs:182`-`194`), else the
  single `.exe` in the folder or two levels down (`crates/xcl/src/launcher.rs:195`-`210`); several candidates is an
  error asking for the exe or `exe` in `xcl.toml`.
- **Title name.** `Identity Name` from `MicrosoftGame.config`, else the folder name, sanitised to
  `[alnum ._-]` (`crates/xcl/src/launcher.rs:248`-`249`, `crates/xcl/src/launcher.rs:213`). The data folder is
  `<data root>\<title>` (`crates/xcl/src/launcher.rs:296`).
- **Interacts.** `config`, `manifest`, `preflight`, `inspect::console_imports` (`crates/xcl/src/launcher.rs:346`),
  `report` (`crates/xcl/src/launcher.rs:474`), `tls::cleanup_cache`.
- **Env vars.** `XCL_DATA_ROOT` (`crates/xcl/src/launcher.rs:11`), `XCL_SKIP_CPU_CHECK` (`crates/xcl/src/launcher.rs:360`),
  `XCL_HOST` (set for the child, removed after `CreateProcessW`: `crates/xcl/src/launcher.rs:439`, `crates/xcl/src/launcher.rs:459`).
- **Tests.** `launcher.rs` `mod tests` (`crates/xcl/src/launcher.rs:521`, 12 tests: XML attribute and comment
  handling, exe picking, CPU message at `crates/xcl/src/launcher.rs:704`); integration:
  `launcher_uses_configured_exe_in_an_ambiguous_folder` (`crates/xcl/tests/e2e.rs:52`) and
  `test_title_runs_and_game_folder_is_untouched` (`crates/xcl/tests/e2e.rs:88`).
- **Known limits.** `HANDOFF.md:38` (Launcher/host/report bullet). The launcher is Windows-only
  (`CreateProcessW`, `crates/xcl/src/launcher.rs:447`).

## `host.rs` - the process the title runs in

- **Purpose.** Boot sequence of the child process (see "How a title starts", step 2)
  (`crates/xcl/src/host.rs:1`-`2`).
- **Key items.** `host_main` (`crates/xcl/src/host.rs:11`), `HOST_MODE` (`crates/xcl/src/host.rs:9`, read by `crates/xcl/src/main.rs:11`).
- **Env vars.** `XCL_UMA_MB` (written from `config.uma_mb` when unset, `crates/xcl/src/host.rs:28`), `XCL_ACTIVITY`
  (`crates/xcl/src/host.rs:33`: every 2 s a log line with presents, submits, draws, dispatches and memory),
  `XCL_THREAD_DUMP_AT` (`crates/xcl/src/host.rs:81`), `XCL_RUN_SECONDS` (`crates/xcl/src/host.rs:96`: ends the title after n seconds
  with the GPU drained and the report written, so a kill from outside cannot hang in the driver).
- **Interacts.** Everything in the start sequence; reads the `hle::d3d12x` counters for the activity line
  (`crates/xcl/src/host.rs:36`-`39`).
- **Tests.** No unit tests; covered by the e2e test (`crates/xcl/tests/e2e.rs:88`) and by
  `scripts/smoke.ps1`, which sets `XCL_RUN_SECONDS` and `XCL_ACTIVITY` (`scripts/smoke.ps1:35`-`36`).
- **Known limits.** None recorded.

## `lib.rs` and `main.rs`

- `crates/xcl/src/lib.rs:1`-`3`: crate doc; `#![allow(function_casts_as_integer)]` at
  `crates/xcl/src/lib.rs:6` because an import resolver casts functions to addresses. `log` is declared first with
  `#[macro_use]` (`crates/xcl/src/lib.rs:8`-`9`).
- `crates/xcl/src/main.rs` is the CLI and the host entry point; see `cli-and-config.md`. The
  `.CRT$XLY` TLS callback is `crates/xcl/src/main.rs:10`-`18`.

## `config.rs` - `xcl.toml` and the boot record

- **Purpose.** Per-title settings and the `Boot` record the launcher passes to the host
  (`crates/xcl/src/config.rs:1`-`2`).
- **Key items.** `Config` (`crates/xcl/src/config.rs:10`, defaults at `crates/xcl/src/config.rs:150`), `Config::load_or_create`
  (`crates/xcl/src/config.rs:207`), `Config::resolve` (fills "auto" from the manifest, `crates/xcl/src/config.rs:80`),
  `title_memory_budget_mb` (`crates/xcl/src/config.rs:117`), `pls_drive` (`crates/xcl/src/config.rs:140`),
  `raytracing_tier_override` (`crates/xcl/src/config.rs:243`), `Boot` (`crates/xcl/src/config.rs:264`: title, exe, game root, data
  dir, config), `Boot::expand` (`{game}` and `{data}`, `crates/xcl/src/config.rs:282`). Field-by-field table in
  `cli-and-config.md`.
- **Behaviour.** A missing file is created with defaults and a header
  (`crates/xcl/src/config.rs:224`-`231`); an unreadable or invalid one is an error, never overwritten
  (`crates/xcl/src/config.rs:221`-`223`). Default mounts missing from an old file are added (`crates/xcl/src/config.rs:211`-`214`).
  The old 41-character console id is rewritten to the 38-character one (`crates/xcl/src/config.rs:215`-`218`).
- **Interacts.** `launcher` (load, resolve, write `boot.json`), `host` (reads `boot.json`), `state`
  (global access), `hle::*` (read settings through `state::config()`), `vfs` (mounts).
- **Env vars.** `XCL_RAYTRACING` (`crates/xcl/src/config.rs:244`); `XCL_ADAPTER` and `XCL_UMA_MB` / `XCL_WINDOW_POS`
  override fields documented at `crates/xcl/src/config.rs:29`, `crates/xcl/src/config.rs:47`, `crates/xcl/src/config.rs:61`.
- **Tests.** `config.rs` `mod tests` (`crates/xcl/src/config.rs:289`): round trip (`crates/xcl/src/config.rs:292`), partial file
  (`crates/xcl/src/config.rs:301`), console id buffer (`crates/xcl/src/config.rs:309`), PLS drive (`crates/xcl/src/config.rs:324`), Series S memory
  (`crates/xcl/src/config.rs:339`), memory case-insensitivity (`crates/xcl/src/config.rs:356`), raytracing parsing (`crates/xcl/src/config.rs:366`).
  The e2e test checks `boot.json` (`crates/xcl/tests/e2e.rs:143`).
- **Known limits.** The "Advanced" title-memory budget is not public, so the standard figure stays
  unless `title_memory_mb` is set (`crates/xcl/src/config.rs:92`-`94`; `HANDOFF.md:1164`, section
  "MicrosoftGame.Config (session 5, 2026-11-21)").

## `manifest.rs` - `MicrosoftGame.config`

- **Purpose.** Reads the few manifest settings XCL uses with plain text search, not an XML parser
  (`crates/xcl/src/manifest.rs:1`-`4`).
- **Fields.** `<PersistentLocalStorage><SizeMB>`, `<XboxAnacondaTitleMemory>`,
  `<XboxLockhartTitleMemory>`, `<TitleId>`, and the `<Resources><Resource Language>` list
  (`crates/xcl/src/manifest.rs:8`-`19`, `parse` at `crates/xcl/src/manifest.rs:40`). `load` finds the file case-insensitively
  (`crates/xcl/src/manifest.rs:61`-`62`) and reuses the launcher's decoding (`crates/xcl/src/manifest.rs:64`-`65`).
- **Used by.** `Config::resolve` (`crates/xcl/src/config.rs:80`): `pls_mb`, `title_memory` (-> `uma_mb`), `title_id`;
  `crates/xcl/src/launcher.rs:324`. `languages` is consumed by the XGameRuntime locale selection (`crates/xcl/src/hle/xgameruntime/api.rs:1210`).
- **Env vars.** None.
- **Tests.** `manifest.rs` `mod tests` (`crates/xcl/src/manifest.rs:70`): settings (`crates/xcl/src/manifest.rs:73`), commented-out
  settings, UTF-16 big-endian (`crates/xcl/src/manifest.rs:91`-`110`); e2e checks the launcher line
  (`crates/xcl/tests/e2e.rs:140`).
- **Known limits.** `HANDOFF.md:1164`. Nothing else in the file is read; `<Executable Name>` and
  `Identity Name` are read separately by the launcher (`crates/xcl/src/launcher.rs:138`).

## `preflight.rs` - PC checks

- **Purpose.** Before the game starts, check what the PC has of what a run needs - DXC, the system
  `xgameruntime.dll`, the GPU (SM 6.6, mesh shaders, wave width) - print each gap with a fix and keep
  the list in `report.json` (`crates/xcl/src/preflight.rs:1`-`4`).
- **Key items.** `Level` (`Ok` / `Note` / `Warning` / `Error`, `crates/xcl/src/preflight.rs:11`), `Check`
  (`crates/xcl/src/preflight.rs:22`), `checks` (pure function over `Found`, `crates/xcl/src/preflight.rs:57`), `probe_gpu`
  (`crates/xcl/src/preflight.rs:101`; same adapter choice and WARP fallback as the D3D12 backend), `run`
  (`crates/xcl/src/preflight.rs:157`), `print` (`crates/xcl/src/preflight.rs:169`).
- **Interacts.** `gpu::adapter`, `gpu::agility`, `gpu::recomp::dxc`; `crates/xcl/src/launcher.rs:353` runs it and
  `report.environment` stores it (`crates/xcl/src/report.rs:50`-`52`).
- **Env vars.** `XCL_ADAPTER` through `Selector::from_env_or` (`crates/xcl/src/preflight.rs:108`,
  `crates/xcl/src/gpu/adapter.rs:20`); `XCL_DXC_DIR` is named in a message (`crates/xcl/src/preflight.rs:88`) and
  read in `crates/xcl/src/gpu/recomp/dxc.rs:78`.
- **Tests.** `preflight.rs` `mod tests` (`crates/xcl/src/preflight.rs:185`): complete PC (`crates/xcl/src/preflight.rs:206`), each
  gap has a fix (`crates/xcl/src/preflight.rs:211`), wave width (`crates/xcl/src/preflight.rs:225`), old DXC (`crates/xcl/src/preflight.rs:236`),
  WARP (`crates/xcl/src/preflight.rs:245`), report round trip (`crates/xcl/src/preflight.rs:252`). These do not need a GPU: they
  test `checks`, not `probe_gpu`.
- **Known limits.** The check only looks at `System32\xgameruntime.dll` under `%SystemRoot%`
  (`crates/xcl/src/preflight.rs:151`-`154`). A missing piece never stops the run (`crates/xcl/src/preflight.rs:3`-`4`), except the
  CPU check in the launcher.

## `vfs.rs` and `vfs_hooks.rs` - virtual file system

- **Purpose.** Console paths (`G:\`, `GAME:`, `D:`, `T:\`, `S:\`) map to PC locations, and the game
  folder is read-only: a write first copies that one file to `<data>\overlay\`, reads prefer the
  overlay, a delete records a "whiteout" (`crates/xcl/src/vfs.rs:1`-`8`). `vfs_hooks.rs` is the Win32 side:
  inline hooks on KERNELBASE exports, so CRT calls (`fopen` -> `CreateFileW`) are caught too
  (`crates/xcl/src/vfs_hooks.rs:1`-`7`).
- **Key types and functions (`vfs.rs`).** `Target` (`crates/xcl/src/vfs.rs:15`: `Pass`, `Translated`, `Game`),
  `Decision` (`crates/xcl/src/vfs.rs:25`), `Vfs` (`crates/xcl/src/vfs.rs:67`), `Vfs::new` (`crates/xcl/src/vfs.rs:159`), `classify_with`
  (`crates/xcl/src/vfs.rs:261`), `open_decision_for` (`crates/xcl/src/vfs.rs:536`), `stage_write` / `commit_write` / `prepare_write`
  (`crates/xcl/src/vfs.rs:483`, `crates/xcl/src/vfs.rs:497`, `crates/xcl/src/vfs.rs:512`), `delete_file` / `create_dir` / `remove_dir`
  (`crates/xcl/src/vfs.rs:603`, `crates/xcl/src/vfs.rs:625`, `crates/xcl/src/vfs.rs:635`), whiteouts (`crates/xcl/src/vfs.rs:352`-`397`, stored in
  `<data>\overlay\.xcl_whiteouts`, `crates/xcl/src/vfs.rs:162`), report accessors (`crates/xcl/src/vfs.rs:668`-`676`), `init` / `get`
  (`crates/xcl/src/vfs.rs:87`, `crates/xcl/src/vfs.rs:92`).
- **Key items (`vfs_hooks.rs`).** `install` (`crates/xcl/src/vfs_hooks.rs:1182`) hooks 28 KERNELBASE functions
  (`CreateFileW/A`, `CreateFile2`, attribute, disk-free, directory, delete, `ReadFile`, move, copy,
  `ReplaceFileW`, `FindFirstFile*`/`FindNextFile*`/`FindClose`: `crates/xcl/src/vfs_hooks.rs:1184`-`1213`) plus three
  kernel32 ANSI move functions (`crates/xcl/src/vfs_hooks.rs:1216`-`1220`). A thread-local guard keeps the hooks out of
  the VFS's own file I/O (`crates/xcl/src/vfs_hooks.rs:23`-`46`). Directory listings of game folders are merged with
  the overlay (`crates/xcl/src/vfs_hooks.rs:945`-`1180`). Disk-free queries for the PLS drive answer the configured
  quota (`crates/xcl/src/vfs_hooks.rs:309`).
- **Unmapped paths.** A path on a drive letter that does not exist on this PC and has no mount is passed
  through, logged once and listed in `report.vfs.unmapped_paths` (`crates/xcl/src/vfs.rs:281`-`285`).
- **Interacts.** `config` (mounts, expanded in `crates/xcl/src/host.rs:117`), `hooks`, `resolver::load_library`,
  `report` (`crates/xcl/src/report.rs:128`-`132`), `host` (`crates/xcl/src/host.rs:119`-`124`). The log file is opened before
  the hooks so logging never re-enters the VFS (`crates/xcl/src/log.rs:1`-`2`).
- **Env vars.** None.
- **Tests.** `vfs.rs` `mod tests` (`crates/xcl/src/vfs.rs:684`, 10 tests: mounts, copy-on-write, whiteouts,
  `TRUNCATE_EXISTING`, directory rules). `vfs_hooks.rs` `mod tests` (`crates/xcl/src/vfs_hooks.rs:1238`, 10 tests:
  PLS disk space, move, copy, replace, find). Integration: `crates/xcl/tests/pls_quota.rs:24`
  (A/W hooks without a title) and the e2e test, which checks the game folder is unchanged
  (`crates/xcl/tests/e2e.rs:137`-`157`).
- **Known limits.** Direct `Nt*` file calls bypass the VFS (`crates/xcl/src/vfs_hooks.rs:9`, note in
  `crates/xcl/src/report.rs:152`). Section history in `HANDOFF.md:37` (VFS bullet); later fixes (move/copy semantics,
  `TRUNCATE_EXISTING`) are listed in the merged-PR sections, e.g. `HANDOFF.md:862` and `HANDOFF.md:872`.

## `state.rs` - global host state

- `set_boot` / `boot` (`crates/xcl/src/state.rs:10`, `crates/xcl/src/state.rs:14`) hold the `Boot` record in a
  `OnceLock`; `config()` returns its config or `Config::default()` in unit tests (`crates/xcl/src/state.rs:18`-`23`);
  `data_dir()` returns the title's data folder, or `%TEMP%\xcl_unit_data` without a boot record
  (`crates/xcl/src/state.rs:26`-`30`).
- Used by `crates/xcl/src/report.rs:99`, `gpu/uma.rs`, and most HLE modules for settings. No env vars. No tests of its own.

## `report.rs` - `report.json`

- **Purpose.** The to-do list for the next iteration: what the title imported, which stubs it called,
  which console paths were unmapped, which writes were redirected (`crates/xcl/src/report.rs:1`-`3`).
- **Key items.** `Report` (`crates/xcl/src/report.rs:44`), `build` (`crates/xcl/src/report.rs:98`), `write_final` (`crates/xcl/src/report.rs:171`),
  `load` (`crates/xcl/src/report.rs:190`). Fields: `imports_by_source`, `stubbed_imports`, `called_stubs`, `modules`,
  `vfs`, `shaders`, `dxc`, `d3d12`, `audio`, `xgameruntime`, `dstorage`, `environment`, `crash`,
  `exit_code`, `notes` (`crates/xcl/src/report.rs:44`-`83`).
- **When it is written.** After loading (`crates/xcl/src/host.rs:141`), at the end of the entry point (`crates/xcl/src/host.rs:144`),
  on a crash (`crates/xcl/src/eh.rs:112`), at `XCL_RUN_SECONDS` (`crates/xcl/src/host.rs:101`), and at process detach
  (`crates/xcl/src/loader.rs:415`). The launcher adds `exit_code` and `environment` after the host exits
  (`crates/xcl/src/launcher.rs:474`-`482`).
- **Crash safety.** A crash is kept even while another thread writes the report, with a bounded wait,
  never an endless one (`crates/xcl/src/report.rs:159`-`186`).
- **Env vars.** None read here; `XCL_DSTORAGE_BCPACK_FILL` shapes the `dstorage` block (`crates/xcl/src/report.rs:79`).
- **Tests.** `report.rs` `mod tests` (`crates/xcl/src/report.rs:198`): a crash survives a concurrent write
  (`crates/xcl/src/report.rs:199`); the e2e test checks many fields (`crates/xcl/tests/e2e.rs:162`-`208`).
- **Known limits.** The fixed `notes` (`crates/xcl/src/report.rs:149`-`153`) say that console-format shader fallback and
  direct `Nt*` file calls are known gaps. `HANDOFF.md:38`.

## `log.rs` - logger

- **Purpose.** Process-wide logger; the file is opened once at startup so logging never goes through
  the VFS (`crates/xcl/src/log.rs:1`-`2`).
- **Key items.** `Level` (`crates/xcl/src/log.rs:13`; `parse` accepts `error`, `warn`/`warning`, `debug`, `trace`, anything
  else is `info`, `crates/xcl/src/log.rs:22`-`29`), `init` (`crates/xcl/src/log.rs:52`), `write` (`crates/xcl/src/log.rs:72`), `flush` (`crates/xcl/src/log.rs:92`),
  macros `xlog_error!` ... `xlog_trace!` (`crates/xcl/src/log.rs:100`-`109`).
- **Format.** `[seconds][thread id][LEVEL] message` (`crates/xcl/src/log.rs:78`). Warnings and errors flush at once
  (`crates/xcl/src/log.rs:82`-`84`); info-and-above is echoed to stderr as `xcl: <message>` when `log_to_stderr` is
  true, the default (`crates/xcl/src/log.rs:87`-`89`, `crates/xcl/src/config.rs:172`).
- **Interacts.** `crates/xcl/src/host.rs:19` opens `<data>\logs\xcl.log` with `config.log_level` and
  `config.log_to_stderr`.
- **Env vars.** None. The level comes from `log_level` in `xcl.toml`.
- **Tests.** None in the file; exercised by every run.
- **Known limits.** None recorded.

## `inspect.rs` - `xcl inspect`

- **Purpose.** Static analysis, read-only (`crates/xcl/src/inspect.rs:1`).
- **Key items.** `run` (`crates/xcl/src/inspect.rs:68`), `console_imports` (`crates/xcl/src/inspect.rs:37`; which of `kernelx.dll`,
  `xgameruntime*`, `d3d12_xs*` a PE imports - also used by the launcher's warning,
  `crates/xcl/src/launcher.rs:346`).
- **Output.** Type, image base, relocations, TLS size, `.pdata` count, stack, console-game verdict, import
  counts by source with the unimplemented ones listed (`crates/xcl/src/inspect.rs:125`-`156`); `--json` prints the same as
  JSON (`crates/xcl/src/inspect.rs:114`-`116`). Exit codes: 2 unreadable file, 3 not a valid PE (`crates/xcl/src/inspect.rs:73`,
  `crates/xcl/src/inspect.rs:83`).
- **Interacts.** `pe`, `resolver::classify` (does not map game DLLs, `crates/xcl/src/resolver.rs:333`-`335`), `hle`.
- **Env vars / tests.** None own. The classification logic is covered by the resolver tests.
- **Known limits.** It looks DLLs up in the exe's own folder only (`crates/xcl/src/inspect.rs:86`-`88`).

## `disasm.rs` - `xcl disasm`

- **Purpose.** RDNA2 disassembler for captured shader blobs (`crates/xcl/src/disasm.rs:1`-`3`).
- **Key items.** `run` (`crates/xcl/src/disasm.rs:105`), `number_option` (decimal or `0x` hex, `crates/xcl/src/disasm.rs:9`-`20`),
  `print_xbox` (`crates/xcl/src/disasm.rs:47`). The decoder itself is `gpu/rdna2.rs` (see `gpu-recompiler.md`).
- **Env vars.** None.
- **Tests.** `crates/xcl/tests/disasm_e2e.rs:5` (`--offset` / `--length`).
- **Known limits.** Without `--offset` / `--length` it looks for programs ending in `s_endpgm` and prints a
  hint when none are found (`crates/xcl/src/disasm.rs:150`).

## `rpf.rs` and `rpf_probe.rs` - `xcl rpf`

- **Purpose.** List and extract records of an RPF9 package; `rpf_probe` is a bounded-memory header probe
  for big archives (`crates/xcl/src/rpf.rs:1`-`3`, `crates/xcl/src/rpf_probe.rs:1`).
- **Key items.** `rpf::run` (`crates/xcl/src/rpf.rs:99`), `AUTO_PROBE_SIZE` = 1 GiB (`crates/xcl/src/rpf.rs:15`), `extraction_path`
  (keeps duplicate names, `crates/xcl/src/rpf.rs:31`), `atomic_write` (`crates/xcl/src/rpf.rs:95`), `rpf_probe::run` (`crates/xcl/src/rpf_probe.rs:120`),
  `summarize` (`crates/xcl/src/rpf_probe.rs:157`), probe chunk 4 MiB (`crates/xcl/src/rpf_probe.rs:8`). The record scan is shared with
  `gpu::pack`.
- **Env vars.** None.
- **Tests.** `rpf.rs` `mod tests` (`crates/xcl/src/rpf.rs:216`: duplicates, interrupted write, bad output folder,
  corrupt record, truncated headers); `rpf_probe.rs` `mod tests` (`crates/xcl/src/rpf_probe.rs:193`); integration
  `crates/xcl/tests/rpf_e2e.rs:5` and `crates/xcl/tests/rpf_e2e.rs:44`.
- **Known limits.** Over 1 GiB the detailed options need `--full` (`crates/xcl/src/rpf.rs:121`-`126`).

## `util.rs` - shared helpers

- **Contents.** HRESULT constants (`crates/xcl/src/util.rs:7`-`19`), `parse_ret` (stub-return specs such as
  `E_NOTIMPL`, `0x80004001`, `-1`, `crates/xcl/src/util.rs:31`), `wide` / `from_wide_ptr` / `from_cstr_ptr` / ANSI helpers
  (`crates/xcl/src/util.rs:57`-`107`), `under_wine` (checks `wine_get_version` in ntdll, `crates/xcl/src/util.rs:126`), `dll_key`
  (lower-case file name, `.dll` appended, `crates/xcl/src/util.rs:136`), `glob_match` (one `*` only, `crates/xcl/src/util.rs:146`),
  GDK-style output writers (`write_cstr_out`, `write_wide_out`, `write_ansi_out`, `crates/xcl/src/util.rs:154`-`299`),
  `SendPtr` (`crates/xcl/src/util.rs:305`).
- **Env vars.** None.
- **Tests.** `util.rs` `mod tests`: `dll_keys` (`crates/xcl/src/util.rs:314`), `globbing` (`crates/xcl/src/util.rs:321`),
  `return_specs` (`crates/xcl/src/util.rs:329`), `cstr_out` (`crates/xcl/src/util.rs:338`), `ansi_out_encodes_codepage_and_truncates_on_char_boundaries`
  (`crates/xcl/src/util.rs:351`).
- **Known limits.** `glob_match` supports a single `*` (`crates/xcl/src/util.rs:145`).

## Environment variables of this layer

`XCL_HOST` (internal, launcher to host), `XCL_DATA_ROOT`, `XCL_SKIP_CPU_CHECK`, `XCL_UMA_MB`, `XCL_ACTIVITY`,
`XCL_THREAD_DUMP_AT`, `XCL_RUN_SECONDS`, `XCL_TRACE_IMPORTS`, `XCL_TRACE_FAULTS`, `XCL_PROBE`, `XCL_WATCH`,
`XCL_RAYTRACING`, `XCL_ADAPTER`. All are described, with defaults and the line where each is read, in
`cli-and-config.md`.
