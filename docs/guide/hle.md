# HLE modules (`crates/xcl/src/hle/`, except `d3d12x/`)

This page covers XCL's own implementations of console DLLs. The D3D12.X module has its own page:
[`d3d12x.md`](d3d12x.md). The GPU recompiler it feeds is in [`gpu-recompiler.md`](gpu-recompiler.md).
Paths are repo-relative; `file:line` points at the item described. Nothing here was run on
macOS: the crate builds and tests only on Windows (`AGENTS.md`).

## Overview

### What "HLE" means here

A console title imports functions from console DLLs (`kernelx.dll`, `xgameruntime.dll`,
`d3d12_xs.dll`, ...). XCL does not load those DLLs. Each one is replaced by an HLE module: "a
module is just a name -> function address table" (`crates/xcl/src/hle/mod.rs:1`). The functions
are ordinary Rust `extern "system"` functions (Win64 ABI), so the title's native x64 code calls
them directly. There is no CPU emulation.

### Three kinds of "thunk"

Do not mix these up:

1. **Auto-stub thunk** (`crates/xcl/src/stubs.rs:1`): a tiny generated x64 stub for an import XCL
   does not implement. It logs the call (`stub_report`, `crates/xcl/src/stubs.rs:63`) and returns a
   configured value. Made by `make_stub` (`crates/xcl/src/stubs.rs:111`). Return values come from
   `crates/xcl/data/stubdb.toml` (`crates/xcl/src/stubs.rs:184`).
2. **GDK thunk protocol** (`crates/xcl/src/hle/xgameruntime/api.rs:1`): a title's compiled-in GDK
   library loads `xgameruntime.dll`, calls `InitializeApiImpl*` and gets COM-style objects from
   `QueryApiImpl`. See the XGameRuntime section.
3. **Vtable patching** (`crates/xcl/src/hle/xaudio2.rs:1`): XAudio2 patches the Windows engine's
   vtables in place. See the XAudio2 section.

### How a console import reaches a Rust function

1. `hle::init(&config)` builds the registry once (`crates/xcl/src/hle/mod.rs:198`, called from
   `crates/xcl/src/host.rs:107`).
2. The PE loader walks each module's import and delay-import tables and calls
   `resolver::resolve_import` per entry, writing the result into the IAT
   (`crates/xcl/src/loader.rs:177`).
3. `resolve_import` normalises the DLL name and calls `resolve_inner`
   (`crates/xcl/src/resolver.rs:315`), which asks `Registry::route` (`crates/xcl/src/hle/mod.rs:207`):
   - `Route::Hle(i)`: look the name up in that module's `exports`; if it is missing there, look in
     every other module (`Registry::find_any`, `crates/xcl/src/hle/mod.rs:226`); if still missing,
     try the module's `fallback` list of real Windows DLLs; if still missing, make an auto-stub
     (`crates/xcl/src/resolver.rs:106`, `crates/xcl/src/resolver.rs:226`).
   - `Route::System` (`@system`): load the real DLL. `Route::Stub` (`@stub`): every import becomes
     an auto-stub (`crates/xcl/src/resolver.rs:212`).
   - No route: a DLL shipped in the game folder is mapped by XCL's loader; else a real Windows DLL;
     else `find_any`; else an auto-stub (`crates/xcl/src/resolver.rs:234`). The policy is described
     at the top of that file (`crates/xcl/src/resolver.rs:1`).
4. An import by ordinal is turned into a name through `HleModule::ordinals` or the stub DB
   (`crates/xcl/src/resolver.rs:94`).

### The registry

- `HleModule { name, exports, ordinals, fallback }` (`crates/xcl/src/hle/mod.rs:38`).
  `fallback` lists real DLLs for names XCL does not implement; `"@self"` means the DLL name the
  title imported (`crates/xcl/src/hle/mod.rs:53`).
- `hle_exports!` builds the export list from Rust function names
  (`crates/xcl/src/hle/mod.rs:32`): the Rust name is the exported name.
- Default routes are set in `build` (`crates/xcl/src/hle/mod.rs:98`). Config aliases
  (`[aliases]` in `xcl.toml`) are pushed first and win (`crates/xcl/src/hle/mod.rs:122`). Patterns
  are globs matched in order. Summary of the defaults:

  | Imported DLL (glob) | Module | Fallback to real DLL |
  |---|---|---|
  | `kernelx.dll`, `kernel32.dll`, `kernelbase.dll`, `advapi32.dll`, `kernel32legacy.dll`, `advapi32legacy.dll`, `minappmodelx.dll`, `api-ms-win-core-*`, `ext-ms-win-core-*` | kernelx | yes (`@self`, kernelbase, kernel32, advapi32, sechost) |
  | `xgameruntime*.dll`, `xgameplatform.dll` | xgameruntime | none |
  | `d3d12_xs.dll`, `d3d12_x.dll`, `d3d12x.dll` | d3d12x | none |
  | `xaudio2_9.dll`, `xaudio2_9redist.dll` | xaudio2 | `xaudio2_9.dll` |
  | `mfplat.dll` | mfplat | `mfplat.dll` |
  | `minuserx.dll`, `ext-ms-win-rtcore-ntuser-window-ansi-*`, `...-window-l1-*` | user | `user32.dll` |
  | `xcrdapi.dll` | dstorage | none |
  | `xg_xs.dll` | xg | none |
  | `acphal.dll`, `xapu.dll`, `chad.dll` | acphal | none |
  | `gameinput.dll` | gameinput | none |
  | `dxcompiler_xs.dll`, `dxcompiler_x.dll` | dxcompiler | none |
  | `winhttp.dll`, `ws2_32.dll` (only with `XCL_NETWORK=0`) | netoff | `@self` |

  Source: `crates/xcl/src/hle/mod.rs:98` to `crates/xcl/src/hle/mod.rs:196`.
- Canonical names are constants (`crates/xcl/src/hle/mod.rs:79` to `crates/xcl/src/hle/mod.rs:96`).
- `aliases_are_system()` marks the kernel family: names routed there are real Windows DLLs, so
  `LoadLibrary` / `GetModuleHandle` hand out the genuine module (`crates/xcl/src/hle/mod.rs:58`,
  `crates/xcl/src/hle/kernelx.rs:1602`).
- Tests: `default_routes`, `aliases_take_precedence`, `cross_module_lookup_finds_xmem`
  (`crates/xcl/src/hle/mod.rs:238`, `crates/xcl/src/hle/mod.rs:256`, `crates/xcl/src/hle/mod.rs:268`).

### How to add a new function

1. Pick the module whose console DLL exports it (see the table above). If the DLL is unknown, any
   module works: `find_any` searches all of them.
2. Write `pub unsafe extern "system" fn Name(...)` with the console's signature. Example shape:
   `crates/xcl/src/hle/dxcompiler.rs:20`.
3. Add `Name` to the module's `exports()` list inside `hle_exports![...]` (for example
   `crates/xcl/src/hle/kernelx.rs:32`).
4. For an XGameRuntime API reached through the GDK thunks, the function must also be in the class's
   `impls` and named in the generated GDK table (the table gives its slot):
   `crates/xcl/src/hle/xgameruntime/api.rs:513`. A function absent from the table but exported by
   name needs only the `exports()` entry in `crates/xcl/src/hle/xgameruntime/mod.rs:27`.
5. A new DLL needs a module file, a `pub mod` line, a canonical-name constant, a `HleModule::new`
   in `build` and route patterns (`crates/xcl/src/hle/mod.rs:98`).
6. Add a unit test in the file's `#[cfg(test)] mod tests` (`AGENTS.md`, Tests). Do not edit
   generated files (see "Generated files").

### Generated files: never edit by hand

| File | Generator | Input |
|---|---|---|
| `crates/xcl/src/hle/xgameruntime/gdk_table.rs` ("Generated by ... do not edit", `crates/xcl/src/hle/xgameruntime/gdk_table.rs:1`) | `scripts/gen_gdk_table.py` (`scripts/gen_gdk_table.py:1`) | a table of `NAME(function, "class guid", "interface guid", slot)` lines extracted from the public PC GDK `xgameruntime.lib` |
| `crates/xcl/src/hle/d3d12_layout.rs` ("@generated ... do not edit", `crates/xcl/src/hle/d3d12_layout.rs:1`) | `scripts/gen_d3d12_layout.py` (`scripts/gen_d3d12_layout.py:1`) | `d3d12.h` from microsoft/DirectX-Headers |

Change the generator and rerun it (`AGENTS.md`, Never). `crates/xcl/src/hle/xma2/tables.rs` is not
generated: it is ported by hand from FFmpeg (`crates/xcl/src/hle/xma2/tables.rs:1`).

### Reporting

`report.json` carries HLE statistics: `audio` from `xaudio2::stats()` and `xgameruntime` from
`xgameruntime::api::stats()` (`crates/xcl/src/report.rs:146`, `crates/xcl/src/report.rs:147`).

## kernelx (`hle/kernelx.rs`, `kernelx/lazy.rs`, `kernelx/placeholders.rs`)

**Replaces:** `kernelx.dll`, the console's kernel32/kernelbase, plus the console's `*legacy` and
`minappmodelx` DLLs. Only what differs from Windows is implemented; the rest falls back to the real
KERNELBASE / KERNEL32 export of the same name (`crates/xcl/src/hle/kernelx.rs:1`, fallback list at
`crates/xcl/src/hle/mod.rs:100`).

**Export list:** `crates/xcl/src/hle/kernelx.rs:32`. Groups:

- **Memory.** `VirtualAlloc` / `VirtualFree` / `VirtualProtect` (+`Ex`) with console flags
  stripped (`sanitize_alloc_type`, `sanitize_protect`: `crates/xcl/src/hle/kernelx.rs:114`,
  `crates/xcl/src/hle/kernelx.rs:127`). `VirtualAlloc` (`crates/xcl/src/hle/kernelx.rs:235`) tries, in
  order: placeholder reservations, commits inside the title's sparse reservations, the title's 64
  KiB-page reservations (placed in a UMA sparse window so the GPU sees them), `MEM_GRAPHICS`
  allocations, then plain Windows. `XMem*` allocators (`crates/xcl/src/hle/kernelx.rs:534` to
  `crates/xcl/src/hle/kernelx.rs:708`), title allocation hooks read from the exe's export
  `__XMemAllocationHooks__` (`crates/xcl/src/hle/kernelx.rs:591`), `XMemVirtualQuery`
  (`crates/xcl/src/hle/kernelx.rs:648`), `TitleMemoryStatus` (`crates/xcl/src/hle/kernelx.rs:748`).
- **Physical pages.** `AllocateTitlePhysicalPages`, `MapTitlePhysicalPages`, `XMemMapPhysicalPages*`
  (`crates/xcl/src/hle/kernelx.rs:1050`, `crates/xcl/src/hle/kernelx.rs:1125`,
  `crates/xcl/src/hle/kernelx.rs:1219`). Pages are tracked per page with a "home"
  (`crates/xcl/src/hle/kernelx.rs:799` to `crates/xcl/src/hle/kernelx.rs:870`).
- **GPU views of CPU memory.** `GRAPHICS_RESERVATIONS` and the `mirrors` module record which GPU
  reservation is a view of which CPU reservation, read by `d3d12x::title_textures`
  (`crates/xcl/src/hle/kernelx.rs:139`, `crates/xcl/src/hle/kernelx.rs:167`).
- **App state.** `Register/UnregisterAppStateChangeNotification` and the constrained variants:
  a title is never suspended or constrained (`crates/xcl/src/hle/kernelx.rs:1255`).
- **Threads.** `SetThreadName` / `GetThreadName`; affinity functions that fit console masks to the
  PC's processors (`fit_affinity_mask`, `crates/xcl/src/hle/kernelx.rs:1331`;
  `SetThreadAffinityMask`, `crates/xcl/src/hle/kernelx.rs:1366`). `SetUnhandledExceptionFilter`
  puts the title's filter behind XCL's crash report (`crates/xcl/src/hle/kernelx.rs:28`).
- **System.** `GetConsoleType` returns `config.console_type_raw`
  (`crates/xcl/src/hle/kernelx.rs:1392`); `GetSystemOSVersion` reports the host's build
  (`crates/xcl/src/hle/kernelx.rs:1428`).
- **Registry.** Only `HKLM\...\CurrentVersion\OEM` is answered, value `ConsoleMode` (0x64 Series X,
  0x60 Series S), which the GDK thunks read for `XSystemGetDeviceType`
  (`crates/xcl/src/hle/kernelx.rs:1450` to `crates/xcl/src/hle/kernelx.rs:1507`). Everything else
  goes to the real registry.
- **Modules.** `LoadLibrary*`, `GetProcAddress`, `GetModuleHandle*`, `GetModuleFileName*`,
  `FreeLibrary` route through the registry so console aliases resolve to HLE modules
  (`crates/xcl/src/hle/kernelx.rs:1521` to `crates/xcl/src/hle/kernelx.rs:1760`).

**`kernelx/lazy.rs`:** physical pages whose contents are still zero are mapped as reserved address
space only; the first CPU access faults, a vectored exception handler commits the page and execution
continues (`crates/xcl/src/hle/kernelx/lazy.rs:1`, `crates/xcl/src/hle/kernelx/lazy.rs:47`). This
avoids doubling memory for render targets that live in video memory
(`d3d12x::vram`). `mark` / `unmark` / `ensure` (`crates/xcl/src/hle/kernelx/lazy.rs:76`,
`crates/xcl/src/hle/kernelx/lazy.rs:82`, `crates/xcl/src/hle/kernelx/lazy.rs:87`): `ensure` commits a
range before a kernel call or XCL writes it, because kernel calls fail rather than fault. Callers:
`crates/xcl/src/hle/kernelx.rs:1029`, `crates/xcl/src/hle/kernelx.rs:1188`.

**`kernelx/placeholders.rs`:** placeholder reservations (`MEM_RESERVE_PLACEHOLDER`, split, replace,
coalesce) inside the title's GPU-visible sparse space. Splitting and replacing change logical
regions, never release addresses to Windows; the backing returns to UMA after its last region is
released (`crates/xcl/src/hle/kernelx/placeholders.rs:1`). Entry points `alloc` and `free`
(`crates/xcl/src/hle/kernelx/placeholders.rs:54`, `crates/xcl/src/hle/kernelx/placeholders.rs:83`),
called from `VirtualAlloc` / `VirtualFree` (`crates/xcl/src/hle/kernelx.rs:236`,
`crates/xcl/src/hle/kernelx.rs:393`).

**Env vars:**

| Variable | Effect | Where |
|---|---|---|
| `XCL_COHERENT_HEAPS` | `0` disables the GPU-visible fallback for graphics heaps | `crates/xcl/src/hle/kernelx.rs:306` |
| `XCL_TRACE_MEM` | log the title's allocations of 64 KiB or more (first 60; a number sets the limit) | `crates/xcl/src/hle/kernelx.rs:364`, `crates/xcl/src/hle/kernelx.rs:372` |

**Real vs stubbed:** memory, physical pages, registry OEM key and module functions are real logic.
App-state notifications and `XMemTransferMemory` / `XMemGetWorkingSetStatistics` are minimal
answers (`crates/xcl/src/hle/kernelx.rs:678`, `crates/xcl/src/hle/kernelx.rs:691`).

**Tests:** `crates/xcl/src/hle/kernelx.rs:1765` (23 `#[test]`: affinity fitting, console mode, XMem
alignment, placeholders, physical pages and scatter, GPU-view reservations, module handles);
`crates/xcl/src/hle/kernelx/lazy.rs:103` (2 tests). The e2e title and DLL tests load kernelx through the
resolver (`crates/xcl/tests/console_dlls_e2e.rs:1`).

**Known limits / HANDOFF:** sections "Title memory above 4 GB: UMA windows" and "Device as IDXGIDevice,
XMemVirtualAlloc" (`HANDOFF.md:1065`, `HANDOFF.md:703`). `XMemTransferMemory` always succeeds without
transferring (`crates/xcl/src/hle/kernelx.rs:691`).

## xgameruntime (`hle/xgameruntime/*`)

**Replaces:** `xgameruntime.dll` (and `xgameplatform.dll`), the GDK system API: XUser, XGameSave,
XPackage, XSystem, XTaskQueue, XAsync, XNetworking and others. Offline and XCL-managed.
Overview comment: `crates/xcl/src/hle/xgameruntime/api.rs:1`. Background:
`HANDOFF.md` sections "XGameRuntime through the GDK thunks" (`HANDOFF.md:1086`) and "XGameRuntime:
objects from the GDK table, persistent local storage, device type" (`HANDOFF.md:1001`).

**Two ways in:**

- **By name:** `exports()` (`crates/xcl/src/hle/xgameruntime/mod.rs:27`) for titles that import
  `XTaskQueueCreate`, `XUserAddAsync` ... directly.
- **GDK thunks (what titles use):** `InitializeApiImpl` / `Ex` / `Ex2`, `UninitializeApiImpl`,
  `QueryApiImpl`, `XErrorReport` (`crates/xcl/src/hle/xgameruntime/api.rs:175` to
  `crates/xcl/src/hle/xgameruntime/api.rs:219`). `QueryApiImpl(clsid, iid)`
  (`crates/xcl/src/hle/xgameruntime/api.rs:265`) answers in this order:
  1. refused classes: XStore, XAppBroadcast, XAppCapture return `REGDB_E_CLASSNOTREG`, so the thunk
     reports "API not available" (`REFUSED`, `crates/xcl/src/hle/xgameruntime/api.rs:100`);
  2. XCL's own classes (`own`, `crates/xcl/src/hle/xgameruntime/api.rs:680`; class list
     `crates/xcl/src/hle/xgameruntime/api.rs:513`): XUser, XUser devices, XGameSave, persistent
     local storage, XPackage, XGame, XSystem, analytics, runtime features, XDisplay, XGameEvent
     (dropped), XNetworking, XError, and XGameUi (dialogs without a user handle are forwarded to
     the PC runtime);
  3. everything else goes to `System32\xgameruntime.dll` (`pc()`,
     `crates/xcl/src/hle/xgameruntime/api.rs:137`);
  4. with no PC runtime, XAsync and XAccessibility fall back to XCL's own objects
     (`crates/xcl/src/hle/xgameruntime/api.rs:299` to `crates/xcl/src/hle/xgameruntime/api.rs:309`);
     other classes are "missing".
- **Objects.** An XCL object is a COM-style vtable built from the generated table by function name.
  Per-slot shims pass the call arguments without `this` (`per_slot!`,
  `crates/xcl/src/hle/xgameruntime/api.rs:450`). Unimplemented slots return `E_NOTIMPL` (or false for
  bool functions), log once and are counted in `report.json` as `xgameruntime.unimplemented`
  (`unknown_slot`, `crates/xcl/src/hle/xgameruntime/api.rs:387`; `stats`,
  `crates/xcl/src/hle/xgameruntime/api.rs:1381`). Nothing that takes an XCL user or save handle
  goes to the PC runtime, which does not validate foreign handles
  (`crates/xcl/src/hle/xgameruntime/api.rs:1`, `HANDOFF.md:1086`).

**Files:**

| File | Purpose | Key items |
|---|---|---|
| `xgameruntime/mod.rs` | by-name exports; runtime init, XThread, XSystem, XPackage, persistent local storage, XUser | `exports` (`crates/xcl/src/hle/xgameruntime/mod.rs:27`), `XSystemGetDeviceType` (`crates/xcl/src/hle/xgameruntime/mod.rs:234`), `XPackageMount` (`crates/xcl/src/hle/xgameruntime/mod.rs:318`), `XPersistentLocalStorageGetPath` (`crates/xcl/src/hle/xgameruntime/mod.rs:389`), single local `User` (`crates/xcl/src/hle/xgameruntime/mod.rs:500`), `XUserGetState` (`crates/xcl/src/hle/xgameruntime/mod.rs:611`), token and signature calls (`crates/xcl/src/hle/xgameruntime/mod.rs:756`) |
| `xgameruntime/api.rs` | GDK thunk protocol, vtable objects, PC runtime forwarding, `XGameGetXboxTitleId` | see above; `XGameGetXboxTitleId` at `crates/xcl/src/hle/xgameruntime/api.rs:1334` |
| `xgameruntime/gdk_table.rs` | generated (class, interface, slot) table | `API` (`crates/xcl/src/hle/xgameruntime/gdk_table.rs:6`); generated, never edit |
| `xgameruntime/taskqueue.rs` | `XTaskQueue*`: Manual, ThreadPool, SerializedThreadPool and Immediate dispatch; `Terminate` cancels pending work; ports are `Arc<Port>` so composite queues share them | `Port` (`crates/xcl/src/hle/xgameruntime/taskqueue.rs:63`), `Queue` (`crates/xcl/src/hle/xgameruntime/taskqueue.rs:86`), `XTaskQueueCreate` (`crates/xcl/src/hle/xgameruntime/taskqueue.rs:465`), `XTaskQueueDispatch` (`crates/xcl/src/hle/xgameruntime/taskqueue.rs:546`), `XTaskQueueTerminate` (`crates/xcl/src/hle/xgameruntime/taskqueue.rs:567`) |
| `xgameruntime/xasync.rs` | `XAsync*` / `XAsyncProvider`; `begin_with` starts an XCL async op whose result is a closure's bytes; `use_pc` redirects to the PC runtime's XAsync once it is in use | `XAsyncBegin` (`crates/xcl/src/hle/xgameruntime/xasync.rs:228`), `begin_with` (`crates/xcl/src/hle/xgameruntime/xasync.rs:545`), `begin_with_fixups` (`crates/xcl/src/hle/xgameruntime/xasync.rs:553`), `use_pc` (`crates/xcl/src/hle/xgameruntime/xasync.rs:63`) |
| `xgameruntime/gamesave.rs` | `XGameSave*`: containers of named blobs stored as files under `<data>/gamesave/<configuration id>/<container>/b/<blob>`; offline, no cloud sync | path at `crates/xcl/src/hle/xgameruntime/gamesave.rs:186`, `QUOTA` 256 MiB (`crates/xcl/src/hle/xgameruntime/gamesave.rs:22`), `XGameSaveInitializeProvider` (`crates/xcl/src/hle/xgameruntime/gamesave.rs:191`), `XGameSaveSubmitUpdate` (`crates/xcl/src/hle/xgameruntime/gamesave.rs:592`) |
| `xgameruntime/networking.rs` | `XNetworking*`: UDP port query, per-URL security info, server certificate verification through Windows CryptoAPI with SHA-1 thumbprint pins, connectivity hint via `iphlpapi.dll` | `XNetworkingVerifyServerCertificate` (`crates/xcl/src/hle/xgameruntime/networking.rs:452`), `XNetworkingGetConnectivityHint` (`crates/xcl/src/hle/xgameruntime/networking.rs:592`) |
| `xgameruntime/accessibility.rs` | `XClosedCaptionGetProperties`, `XHighContrastGetMode`; also the built-in COM fallback when the PC runtime is absent | `crates/xcl/src/hle/xgameruntime/accessibility.rs:88`, `crates/xcl/src/hle/xgameruntime/accessibility.rs:102` |
| `xgameruntime/xsign.rs` | offline XSTS v1 request signing for `XUserGetTokenAndSignature*`: a non-empty `XBL3.0` token and a Base64 signature; ephemeral per-process ECDSA P-256 key through Windows CNG | `offline_token` (`crates/xcl/src/hle/xgameruntime/xsign.rs:108`), `sign_request` (`crates/xcl/src/hle/xgameruntime/xsign.rs:230`); used at `crates/xcl/src/hle/xgameruntime/mod.rs:699` |

**Real vs stubbed:**

- Real: task queues, async blocks, one local user (always signed in, `User` at
  `crates/xcl/src/hle/xgameruntime/mod.rs:500`), game saves on disk, package mount, persistent
  storage, offline signing, networking helpers.
- Token calls are gated by `xbox_live_tokens` in `xcl.toml` (default off): off, the request fails as on a
  console that cannot reach Xbox Live (0x80072EFD) (`crates/xcl/src/config.rs:63`,
  `crates/xcl/src/config.rs:187`, `crates/xcl/src/hle/xgameruntime/mod.rs:741`).
- Dropped or refused: game events are accepted and dropped (`crates/xcl/src/hle/xgameruntime/api.rs:531`),
  XStore / app broadcast / app capture are refused.
- Needs Gaming Services: dialogs, text entry, game invites and any API XCL does not implement
  (warning text at `crates/xcl/src/hle/xgameruntime/api.rs:142`).

**Env vars:** none are read in this directory (checked by grep for `XCL_` and `env::var`).

**Tests:** `crates/xcl/src/hle/xgameruntime/api.rs:1385` (16 tests, including
`thunk_protocol_with_xcl_objects_on_pc_queues`, `crates/xcl/src/hle/xgameruntime/api.rs:1521`);
`crates/xcl/src/hle/xgameruntime/mod.rs:963` (9), `crates/xcl/src/hle/xgameruntime/taskqueue.rs:734` (10), `crates/xcl/src/hle/xgameruntime/xasync.rs:567` (14), `crates/xcl/src/hle/xgameruntime/gamesave.rs:612` (10),
`crates/xcl/src/hle/xgameruntime/networking.rs:635` (7), `crates/xcl/src/hle/xgameruntime/accessibility.rs:129` (3), `crates/xcl/src/hle/xgameruntime/xsign.rs:234` (4). End to end:
`libhttpclient_initialises_through_the_xgameruntime_thunks`
(`crates/xcl/tests/console_dlls_e2e.rs:121`, needs `XCL_SAMPLE_DLLS`). `crates/xcl/tests/pls_quota.rs`
exists for persistent-storage quota.

**Known limits:** `HANDOFF.md:1086` (section above) still lists XUser device as refused; the code now
has an XCL class for it (`crates/xcl/src/hle/xgameruntime/api.rs:516`), so treat that sentence as
out of date. The PC runtime is needed for dialogs and invites; without it those are
unavailable (`crates/xcl/src/hle/xgameruntime/api.rs:142`).

## xaudio2 (`hle/xaudio2.rs`)

**Replaces:** the console `xaudio2_9.dll`. The console API is Windows' XAudio2 2.9 plus compressed
source voices (XMA2, Opus) and a few console-only flags (`crates/xcl/src/hle/xaudio2.rs:1`).

**How:** only two exports, `XAudio2Create` and `XAudio2CreateWithVersionInfo`
(`crates/xcl/src/hle/xaudio2.rs:165`). They create the genuine Windows engine with console-only flags
dropped (`create_engine`, `crates/xcl/src/hle/xaudio2.rs:726`) and patch its vtables in place
(`patch`, `crates/xcl/src/hle/xaudio2.rs:681`). Patched slots:

- engine `CreateSourceVoice` (slot 5) and `CreateMasteringVoice` (slot 7)
  (`crates/xcl/src/hle/xaudio2.rs:748`, `crates/xcl/src/hle/xaudio2.rs:749`);
- voice `SubmitSourceBuffer`, `FlushSourceBuffers`, `GetState`, `DestroyVoice`
  (`crates/xcl/src/hle/xaudio2.rs:949` to `crates/xcl/src/hle/xaudio2.rs:952`).

Opus formats (`0x704F`, or EXTENSIBLE with the Opus subtype) become PCM16 voices backed by `hle::opus`;
XMA / XMA2 (`0x165` / `0x166`) by `hle::xma2` (decoder choice at
`crates/xcl/src/hle/xaudio2.rs:833`, `crates/xcl/src/hle/xaudio2.rs:867`). A format Windows rejects
becomes a silent voice of matching length so callbacks still fire. Decoded buffers live until
`OnBufferEnd`. Exports XCL does not implement (X3DAudio, CreateFX ...) fall back to the system
`xaudio2_9.dll`.

**Report:** `AudioStats` and `stats()` (`crates/xcl/src/hle/xaudio2.rs:175`,
`crates/xcl/src/hle/xaudio2.rs:227`) feed `report.json -> audio`.

**Env vars:** none in this file. **Tests:** `crates/xcl/src/hle/xaudio2.rs:1200` (9 tests).
**HANDOFF:** "Opus decoding" (`HANDOFF.md:964`), "Audio - conclusions from `audio.rpf`" (`HANDOFF.md:982`).

## xma2 (`hle/xma2/mod.rs`, `xma2/tables.rs`)

**Purpose:** a pure-Rust XMA1 (`0x0165`) and XMA2 (`0x0166`) decoder. It wraps the WMA Pro subframe
codec in 2048-byte packets with up to 8 streams and 16 channels (`crates/xcl/src/hle/xma2/mod.rs:1`).
Not a console DLL: used by `xaudio2`.

**Provenance:** ported from FFmpeg (LGPL-2.1+) to safe Rust, with the notice in the file header
(`crates/xcl/src/hle/xma2/mod.rs:1`, `crates/xcl/src/hle/xma2/tables.rs:1`). `tables.rs` holds the
static Huffman and decorrelation tables (hand-ported, not generated).

**Key API:** `StreamConfig` (`crates/xcl/src/hle/xma2/mod.rs:49`), `Decoder::new`
(`crates/xcl/src/hle/xma2/mod.rs:1644`), `decode_packets_f32` and `decode_buffer_i16`
(`crates/xcl/src/hle/xma2/mod.rs:1833`, `crates/xcl/src/hle/xma2/mod.rs:1885`), `flush`
(`crates/xcl/src/hle/xma2/mod.rs:1685`). Internal types: `BitReader`, `VlcTable`, `ImdctPlan`,
`StreamDecoder` (`crates/xcl/src/hle/xma2/mod.rs:774`).

**Env vars:** none. **Tests:** `crates/xcl/src/hle/xma2/mod.rs:1896` (7 tests).
**Known limit:** the sample game's audio is Opus, not XMA2, so the decoder was a low priority there
(`HANDOFF.md:982`).

## opus (`hle/opus.rs`)

**Purpose:** RFC 6716 packet framing plus real decoding, used by `xaudio2` and `acphal`
(`crates/xcl/src/hle/opus.rs:1`). Not a console DLL.

**Pieces:**

- Packet parsing, all TOC configurations (`parse_packet`, `crates/xcl/src/hle/opus.rs:159`),
  multistream split (`split_multistream_packet`, `crates/xcl/src/hle/opus.rs:365`), `OpusHead`
  (`build_opus_head`, `parse_opus_head`: `crates/xcl/src/hle/opus.rs:417`,
  `crates/xcl/src/hle/opus.rs:438`), Ogg timing, buffer framing (`extract_packets`,
  `estimate_buffer_frames`: `crates/xcl/src/hle/opus.rs:687`, `crates/xcl/src/hle/opus.rs:748`).
- Backends, tried per stream in this order: Windows' `MSOpusDecoder.dll` as a Media Foundation
  transform (`MftDecoder`, `crates/xcl/src/hle/opus.rs:1089`), then libopus loaded at run time from
  `opus.dll` (`crates/xcl/src/hle/opus.rs:975` to `crates/xcl/src/hle/opus.rs:990`). Both are loaded
  dynamically; libopus is not distributed with XCL. `is_available` and `backend` report the choice
  (`crates/xcl/src/hle/opus.rs:831`, `crates/xcl/src/hle/opus.rs:836`). With neither, decoding fails
  and callers produce silence.
- Multistream (3 to 8 channels) uses one MFT per stream (`crates/xcl/src/hle/opus.rs:1`).
- Public decoder: `Decoder::new`, `decode_packet`, `decode_buffer`, `flush`
  (`crates/xcl/src/hle/opus.rs:1336`, `crates/xcl/src/hle/opus.rs:1399`,
  `crates/xcl/src/hle/opus.rs:1474`, `crates/xcl/src/hle/opus.rs:1390`).

**Env vars:** `XCL_OPUS_DIR` (folder searched for libopus, then next to `xcl.exe`;
`crates/xcl/src/hle/opus.rs:877`). Test-only: `XCL_SAMPLE_AWC` and `XCL_OPUS_WAV`
(`crates/xcl/src/hle/opus.rs:1619`, `crates/xcl/src/hle/opus.rs:1638`).

**Tests:** `crates/xcl/src/hle/opus.rs:1526` (13 tests; the game-sound test does nothing without
`XCL_SAMPLE_AWC`). **HANDOFF:** "Opus decoding" (`HANDOFF.md:964`); streaming AWC containers are not
unpacked by XCL, the title does it.

## mfplat (`hle/mfplat.rs`)

**Replaces:** the console additions to `mfplat.dll`. Every standard Media Foundation export falls back
to the system `mfplat.dll` (`crates/xcl/src/hle/mfplat.rs:1`, fallback at
`crates/xcl/src/hle/mod.rs:104`).

**Implemented:** one function, `MFResetDXGIDeviceManagerX(manager, device, token)`
(`crates/xcl/src/hle/mfplat.rs:164`; export list `crates/xcl/src/hle/mfplat.rs:62`). On the PC,
`IMFDXGIDeviceManager::ResetDevice` wants an `ID3D11Device`. When the direct call fails for an
`ID3D12Device`, XCL creates a companion D3D11 device on the same adapter LUID, enables multithread
protection and resets the manager with it (`create_d3d11_bridge`,
`crates/xcl/src/hle/mfplat.rs:91`; `adapter_for_luid`, `crates/xcl/src/hle/mfplat.rs:81`;
`enable_multithread_protection`, `crates/xcl/src/hle/mfplat.rs:68`).

**Env vars:** none. **Tests:** `crates/xcl/src/hle/mfplat.rs:231` (3 tests).

## dstorage (`hle/dstorage.rs`)

**Replaces:** `xcrdapi.dll`, the console's DirectStorage. One export, `DStorageGetFactory`
(`crates/xcl/src/hle/dstorage.rs:106`, `crates/xcl/src/hle/dstorage.rs:357`), returning
`IDStorageFactoryX`.

**Model:** PC DirectStorage interfaces in the console's vtable order and structure layouts as
observed on a retail title. The file header documents every layout: factory, queue descriptor
(0x18 bytes), request (0x48 bytes), queue, file and status array
(`crates/xcl/src/hle/dstorage.rs:1`). Each queue has a worker thread. Submitted requests run in order:
file reads go through the VFS, memory copies, zlib decompression; a status entry gets the first
failure since the previous one; a signal sets the fence from the CPU. Queue state:
`QueueInner` (`crates/xcl/src/hle/dstorage.rs:925`); request: `Request`
(`crates/xcl/src/hle/dstorage.rs:584`); file: `FileInner` (`crates/xcl/src/hle/dstorage.rs:439`);
status array: `StatusInner` (`crates/xcl/src/hle/dstorage.rs:529`).

**Real vs stubbed:** methods not listed in the header log and answer `E_NOTIMPL`. BCPack and other
console formats are refused (`crates/xcl/src/hle/dstorage.rs:28`); refusals are counted
(`BcpackRefusals`, `crates/xcl/src/hle/dstorage.rs:135`; `stats`, `crates/xcl/src/hle/dstorage.rs:202`).

**Env vars:** `XCL_DSTORAGE_BCPACK_FILL=1`: a refused BCPack destination gets solid BCn blocks
instead of staying empty (`crates/xcl/src/hle/dstorage.rs:226`).

**Tests:** `crates/xcl/src/hle/dstorage.rs:902` (13 tests). **HANDOFF:** "Fortnite's console imports"
(`HANDOFF.md:625`).

## gameinput (`hle/gameinput.rs`)

**Replaces:** `GameInput.dll`. One export, `GameInputCreate`
(`crates/xcl/src/hle/gameinput.rs:987`, export list `crates/xcl/src/hle/gameinput.rs:997`).
Windows' own GameInput is not used: its interfaces differ from the console's
(`crates/xcl/src/hle/gameinput.rs:1`).

**Model:** the title gets the console's GameInput v0 COM interfaces (IGameInput, IGameInputDevice,
IGameInputReading, GUIDs at `crates/xcl/src/hle/gameinput.rs:33`) with one gamepad that is always
connected. A reading is the state when asked: the PC's first XInput controller, plus the keyboard
while an XCL window has focus, plus keyboard/mouse as gamepad (`poll`,
`crates/xcl/src/hle/gameinput.rs:303`, via `kbm::apply`), plus scripted test presses. Reading
callbacks fire on state change from a 250 Hz poller. Unlisted methods log their slot once and fail.
`GamepadState` and button constants: `crates/xcl/src/hle/gameinput.rs:50`.

**Env vars:** `XCL_PRESS_AT=s:BUTTON,...` and `XCL_PRESS_A_AT=s,...` press buttons for 200 ms at the
given seconds after start, for tests (`crates/xcl/src/hle/gameinput.rs:240`,
`crates/xcl/src/hle/gameinput.rs:258`). (The module header only mentions `XCL_PRESS_A_AT`:
`crates/xcl/src/hle/gameinput.rs:14`.)

**Tests:** `crates/xcl/src/hle/gameinput.rs:1001` (9 tests, including that `kbm::apply` is not
called when `keyboard_mouse = false`, `crates/xcl/src/hle/gameinput.rs:1062`).
Note: the file is not valid UTF-8 as a whole, so plain `grep` treats it as binary; use `grep -a`.

## kbm (`hle/kbm.rs`)

**Purpose:** keyboard and mouse played as the one gamepad XCL reports (`keyboard_mouse` in
`xcl.toml`, on by default: `crates/xcl/src/config.rs:71`, `crates/xcl/src/config.rs:188`).
Not a console DLL. The mapping table (WASD left stick, mouse right stick, buttons) is in the file
header (`crates/xcl/src/hle/kbm.rs:1`).

**Key items:** `enabled` (`crates/xcl/src/hle/kbm.rs:249`), `attach(hwnd)` registers Raw Input on the
XCL window (`crates/xcl/src/hle/kbm.rs:254`), `release` (`crates/xcl/src/hle/kbm.rs:294`, also called
from `crates/xcl/src/report.rs:173`), `on_message` handles window messages
(`crates/xcl/src/hle/kbm.rs:299`), `apply` merges into `GamepadState`
(`crates/xcl/src/hle/kbm.rs:392`), `stick_from_keys` and `stick_from_mouse`
(`crates/xcl/src/hle/kbm.rs:34`, `crates/xcl/src/hle/kbm.rs:56`). Mouse speed becomes stick
deflection scaled by `mouse_sensitivity` (`crates/xcl/src/config.rs:74`).

**Env vars:** none (config keys `keyboard_mouse`, `mouse_sensitivity`). **Tests:**
`crates/xcl/src/hle/kbm.rs:449` (6 tests).

## user (`hle/user.rs`)

**Replaces:** `minuserx.dll` and the console's ntuser window API sets; everything else is Windows'
`user32.dll` (`crates/xcl/src/hle/user.rs:1`, routes at `crates/xcl/src/hle/mod.rs:169`).

**Implemented:** `RegisterClassExA`, `CreateWindowExA`, and `CreateWindowExW` registered under that
name (`crates/xcl/src/hle/user.rs:36`, `crates/xcl/src/hle/user.rs:70`, `crates/xcl/src/hle/user.rs:98`,
`crates/xcl/src/hle/user.rs:108`).

- ANSI window calls are made as Unicode ones, because titles pair ANSI creation with `DefWindowProcW`
  and the title would show as CJK text otherwise.
- A title's top-level window is the console's screen: frameless, 1920x1080 client area
  (`CONSOLE_W` / `CONSOLE_H`, `crates/xcl/src/hle/user.rs:118`), kept frameless through style changes
  by a window procedure (`console_wndproc`, `crates/xcl/src/hle/user.rs:165`, handling
  `WM_NCCALCSIZE`), cloaked and off the taskbar. The game's frames show in XCL's own window.

**Env vars:** none. **Tests:** `crates/xcl/src/hle/user.rs:190` (3 tests). **HANDOFF:** "XDefiant's UI
text: the #107 regression" (`HANDOFF.md:906`).

## acphal (`hle/acphal.rs`)

**Replaces:** `acphal.dll`, `xapu.dll`, `chad.dll`: the console's audio processor (APU memory, its
command channel, SHAPE contexts), decoder (xapu) and DSP (chad)
(`crates/xcl/src/hle/mod.rs:180`, `crates/xcl/src/hle/acphal.rs:1`).

**Model:** the processor answers as hardware with little to do: commands are acknowledged, audio
frames tick (one per 128 samples at 48 kHz), SHAPE context arrays exist. A title's audio engine starts
and nothing waits forever. **The SHAPE mixer is not run.** The decoder runs: Opus packets sent through
`XApuEnqueueCommand` are decoded with `hle::opus` and the PCM is written to the title's output buffer.
XCL plays nothing here; the title mixes and plays its own output (`crates/xcl/src/hle/acphal.rs:1`).
Without a working Opus backend the decoder answers silence, logged once. The command formats (Activate
kind 1, Process kind 3, Deactivate kind 2) are documented in the file header as observed on a retail
game (`crates/xcl/src/hle/acphal.rs:19`).

**Key functions:** APU heap `ApuCreateHeap` / `ApuAlloc` / `ApuFree` / `ApuMapVirtualAddress`
(`crates/xcl/src/hle/acphal.rs:294` to `crates/xcl/src/hle/acphal.rs:361`), SHAPE
`AcpHalAllocateShapeContexts` (`crates/xcl/src/hle/acphal.rs:457`), `AcpHalCreate`
(`crates/xcl/src/hle/acphal.rs:623`), decoder `XApuConnect`, `XApuEnqueueCommand`, `XApuDequeueResult`
(`crates/xcl/src/hle/acphal.rs:857`, `crates/xcl/src/hle/acphal.rs:884`,
`crates/xcl/src/hle/acphal.rs:970`), DSP `XDspConnectWithMaximumStreamLimit` ...
`XDspSubmitCommand` (`crates/xcl/src/hle/acphal.rs:1016` to `crates/xcl/src/hle/acphal.rs:1052`; the
DSP is a connect/acknowledge model). Exports: `crates/xcl/src/hle/acphal.rs:1064`.

**Env vars:** `XCL_TRACE_APU=1` logs commands, their structures and the start of the Opus input
(`crates/xcl/src/hle/acphal.rs:85`).

**Tests:** `crates/xcl/src/hle/acphal.rs:1090` (7 tests). **Known limits:** SHAPE mixing is not run;
nothing is played from XCL. **HANDOFF:** "Fortnite's console imports" (`HANDOFF.md:625`).

## netoff (`hle/netoff.rs`)

**Purpose:** `XCL_NETWORK=0` runs the title as on a console without a network. Off by default
(`enabled`, `crates/xcl/src/hle/netoff.rs:58`).

**How:** with the switch on, a module `xcl-netoff.dll` (`crates/xcl/src/hle/netoff.rs:22`) is added
to the registry and `winhttp.dll` / `ws2_32.dll` route to it (`crates/xcl/src/hle/mod.rs:112`,
`crates/xcl/src/hle/mod.rs:190`). Its table (`crates/xcl/src/hle/netoff.rs:63`) fails WinHTTP sends
and socket connects, sends and name lookups the way a console without a network does, and carries
ws2_32 ordinals (`ordinals`, `crates/xcl/src/hle/netoff.rs:81`). Loopback (`127.0.0.0/8`, `::1`) and
local names still work. Only the title's own imports are affected by the module route.

`install_system_hooks` (`crates/xcl/src/hle/netoff.rs:152`) also hooks the system winhttp / wininet /
ws2_32 functions, because a retail game's crash reporter reached the system WinHTTP another way and
uploaded a 21.5 MB minidump with `XCL_NETWORK=0` (#305). Calls that are allowed go to the original
through the hook's trampoline.

**Env vars:** `XCL_NETWORK` (`crates/xcl/src/hle/netoff.rs:59`). **Tests:**
`crates/xcl/src/hle/netoff.rs:287` (2 tests). The config comment on the related offline mode is at
`crates/xcl/src/config.rs:63`.

## xg (`hle/xg.rs`)

**Replaces:** `xg_xs.dll`, the console's texture layout library (Fortnite imports
`XGCreateTextureComputer`). XCL answers from its own GFX10 tiling model (`gpu::tiling::Chain`) in the
console's structures (`crates/xcl/src/hle/xg.rs:1`).

**Implemented:** `XGCreateTextureComputer` (`crates/xcl/src/hle/xg.rs:540`) for 2D textures, arrays and
cubes with one sample, and `XGSuggestSwizzleMode` (`crates/xcl/src/hle/xg.rs:516`); exports
`crates/xcl/src/hle/xg.rs:45`. The returned computer implements `GetResourceLayout` (the
`XG_RESOURCE_LAYOUT`, 0x1B40 bytes), size and alignment, mip offsets, texel offsets,
`CopyIntoSubresource` / `CopyFromSubresource`; the layouts are in the file header
(`crates/xcl/src/hle/xg.rs:1`). Helpers: `optimal_mode` (`crates/xcl/src/hle/xg.rs:52`), `element`
(`crates/xcl/src/hle/xg.rs:116`).

**Limits:** 2D, arrays and cubes with one sample only; levels in the mip tail are reported as 256-byte
tiles (`crates/xcl/src/hle/xg.rs:1`).

**Env vars:** `XCL_SAMPLE_XG` (test oracle only, `crates/xcl/tests/xg_oracle.rs:14`). **Tests:**
`crates/xcl/src/hle/xg.rs:718` (3 tests); `crates/xcl/tests/xg_oracle.rs:1` compares the answers with the
console's own library when `XCL_SAMPLE_XG` is set.

## dxcompiler (`hle/dxcompiler.rs`)

**Replaces:** `dxcompiler_xs.dll`, the console DXC. Titles get the PC compiler XCL itself uses
(`crates/xcl/src/hle/dxcompiler.rs:1`). Exports `DxcCreateInstance` and `DxcCreateInstance2`
(`crates/xcl/src/hle/dxcompiler.rs:14`); both call `gpu::recomp::dxc::create_instance`
(`crates/xcl/src/hle/dxcompiler.rs:19`) and return `E_FAIL` with a warning if no DXC exists on the PC.
The `IMalloc` of `DxcCreateInstance2` is ignored (`crates/xcl/src/hle/dxcompiler.rs:32`).

**Env vars:** `XCL_DXC_DIR` locates DXC (`crates/xcl/src/gpu/recomp/dxc.rs:78`; named in the warning at
`crates/xcl/src/hle/dxcompiler.rs:25`). **Tests:** `the_console_compiler_is_the_pc_one`
(`crates/xcl/src/hle/dxcompiler.rs:36`); it returns early without DXC. DXC details belong in
[`gpu-recompiler.md`](gpu-recompiler.md).

## d3d12_layout (`hle/d3d12_layout.rs`)

**Purpose:** generated data, not a DLL. For every `ID3D12*` interface of the PC headers: name, IID,
parent and the full vtable method order (`Iface`, `IFACES`:
`crates/xcl/src/hle/d3d12_layout.rs:3`, `crates/xcl/src/hle/d3d12_layout.rs:10`). XCL uses the PC order
as a first guess for D3D12.X objects and names every slot in logs and reports
(`scripts/gen_d3d12_layout.py:1`).

**Generated:** `scripts/gen_d3d12_layout.py <d3d12.h> crates/xcl/src/hle/d3d12_layout.rs`
(`scripts/gen_d3d12_layout.py:3`). Never edit by hand.

**Used by:** `crates/xcl/src/hle/d3d12x/layout.rs:5` (see [`d3d12x.md`](d3d12x.md)). No env vars, no tests of
its own.
