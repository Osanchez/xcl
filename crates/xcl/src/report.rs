//! `report.json`: what the title imported, which stubs it actually called, which console
//! paths were not mapped, which game files were redirected – the to-do list for the
//! next iteration.

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::resolver::{self, Source};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct StubCall {
    pub dll: String,
    pub function: String,
    pub calls: u64,
    pub first_caller: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct MissingImport {
    pub importer: String,
    pub dll: String,
    pub function: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ModuleInfo {
    pub name: String,
    pub base: String,
    pub size: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct VfsInfo {
    pub unmapped_paths: Vec<String>,
    pub redirected_writes: Vec<String>,
    pub whiteouts: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Report {
    pub xcl_version: String,
    pub title: String,
    pub exe: String,
    pub crash: Option<String>,
    pub exit_code: Option<i64>,
    /// what the PC has of what a run needs (DXC, GPU features), checked by `xcl run` first
    #[serde(default)]
    pub environment: Vec<crate::preflight::Check>,
    /// counts of static imports by how they were resolved
    pub imports_by_source: BTreeMap<String, usize>,
    /// imports that ended up as auto-stubs (static analysis)
    pub stubbed_imports: Vec<MissingImport>,
    /// stubs the title actually called at run time (sorted by call count)
    pub called_stubs: Vec<StubCall>,
    pub modules: Vec<ModuleInfo>,
    pub vfs: VfsInfo,
    /// shader formats seen at pipeline creation (dxil / dxbc / container-unknown / rdna2-raw …)
    #[serde(default)]
    pub shaders: crate::gpu::shader::ShaderStats,
    /// the DXC that compiled them: folder and versions of dxcompiler.dll and dxil.dll (empty:
    /// none was loaded)
    #[serde(default)]
    pub dxc: crate::gpu::recomp::dxc::Toolchain,
    /// D3D12.X backend: adapter, interface layouts, unknown IIDs, fallbacks, presented frames
    #[serde(default)]
    pub d3d12: crate::hle::d3d12x::real::D3d12Stats,
    /// XAudio2: engines, mastering/source voices, XMA voices played as silence, dropped console flags
    #[serde(default)]
    pub audio: crate::hle::xaudio2::AudioStats,
    /// XGameRuntime through the GDK thunks: which classes the title asked for and who answered,
    /// XCL slots it called that XCL does not implement
    #[serde(default)]
    pub xgameruntime: crate::hle::xgameruntime::api::ApiStats,
    /// DirectStorage: BCPack requests unpacked and refused (no decoder) by mode, and bytes filled under
    /// `XCL_DSTORAGE_BCPACK_FILL`
    #[serde(default)]
    pub dstorage: crate::hle::dstorage::DStorageStats,
    pub notes: Vec<String>,
}

static CRASH: Mutex<Option<String>> = Mutex::new(None);
static WRITE_LOCK: Mutex<()> = Mutex::new(());

fn source_label(s: &Source) -> String {
    match s {
        Source::Hle(m) => format!("hle:{m}"),
        Source::HleOther(m) => format!("hle-cross:{m}"),
        Source::System(d) => format!("system:{d}"),
        Source::GameDll(d) => format!("game-dll:{d}"),
        Source::Stub => "stub".into(),
    }
}

pub fn build() -> Report {
    let boot = crate::state::boot();
    let records = resolver::records();
    let mut by_source: BTreeMap<String, usize> = BTreeMap::new();
    let mut stubbed = Vec::new();
    for r in &records {
        let label = match &r.source {
            Source::System(_) => "system".to_string(),
            other => source_label(other),
        };
        *by_source.entry(label).or_default() += 1;
        if r.source == Source::Stub {
            stubbed.push(MissingImport { importer: r.importer.clone(), dll: r.dll.clone(), function: r.function.clone() });
        }
    }
    let mut called: Vec<StubCall> = crate::stubs::all_stubs()
        .into_iter()
        .filter(|s| s.calls.load(Ordering::Relaxed) > 0)
        .map(|s| StubCall {
            dll: s.dll.clone(),
            function: s.name.clone(),
            calls: s.calls.load(Ordering::Relaxed),
            first_caller: crate::modules::describe_address(s.first_caller.load(Ordering::Relaxed)),
        })
        .collect();
    called.sort_by(|a, b| b.calls.cmp(&a.calls));
    let modules = crate::modules::mapped_modules()
        .iter()
        .map(|m| ModuleInfo { name: m.name.clone(), base: format!("{:#x}", m.base), size: format!("{:#x}", m.size) })
        .collect();
    let vfs = crate::vfs::get()
        .map(|v| VfsInfo { unmapped_paths: v.unmapped_paths(), redirected_writes: v.redirected_writes(), whiteouts: v.whiteouts() })
        .unwrap_or_default();
    Report {
        xcl_version: env!("CARGO_PKG_VERSION").to_string(),
        title: boot.map(|b| b.title.clone()).unwrap_or_default(),
        exe: boot.map(|b| b.exe_path.to_string_lossy().into_owned()).unwrap_or_default(),
        crash: CRASH.lock().ok().and_then(|g| g.clone()),
        exit_code: None,
        environment: Vec::new(),
        imports_by_source: by_source,
        stubbed_imports: stubbed,
        called_stubs: called,
        modules,
        vfs,
        shaders: crate::gpu::shader::stats(),
        dxc: crate::gpu::recomp::dxc::toolchain(),
        d3d12: crate::hle::d3d12x::real::stats(),
        audio: crate::hle::xaudio2::stats(),
        xgameruntime: crate::hle::xgameruntime::api::stats(),
        dstorage: crate::hle::dstorage::stats(),
        notes: vec![
            "D3D12.X methods missing on PC show up in called_stubs as '<Interface>::#slot (not on PC)'.".into(),
            "Console-format shaders are replaced (fallback_pipelines) until the RDNA2 recompiler is finished.".into(),
            "Direct Nt* calls on files do not go through the VFS.".into(),
        ],
    }
}

/// `m.try_lock()`, retried for `wait`: never an endless wait – the holder may be this very
/// thread, crashed inside `build()`.
fn lock_within<T>(m: &Mutex<T>, wait: Duration) -> Option<MutexGuard<'_, T>> {
    let end = Instant::now() + wait;
    loop {
        match m.try_lock() {
            Ok(g) => return Some(g),
            Err(TryLockError::WouldBlock) if Instant::now() < end => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => return None,
        }
    }
}

/// Write `report.json` (host side). `crash` is remembered for later writes.
pub fn write_final(crash: Option<String>) {
    // the process ends: a cursor held in the XCL window is let go
    crate::hle::kbm::release();
    // `XCL_DIRECT_DXIL_STATS=1`: the direct DXIL translator's live fallback summary
    crate::gpu::dxil::coverage::dump_fallbacks();
    // a crash is kept even while another thread writes the report, and is written once that
    // thread is done – the process ends right after (#106); other calls give way at once
    let wait = if crash.is_some() { Duration::from_secs(2) } else { Duration::ZERO };
    if let Some(c) = crash {
        if let Some(mut g) = lock_within(&CRASH, wait) {
            g.get_or_insert(c);
        }
    }
    let Some(_g) = lock_within(&WRITE_LOCK, wait) else { return };
    let Some(boot) = crate::state::boot() else { return };
    let r = build();
    if let Ok(s) = serde_json::to_string_pretty(&r) {
        telemetry_final(&s);
        // said once on stderr: a report that is not there would otherwise look like a silent
        // crash, and its log may be missing for the same reason (#2277)
        if let Err(e) = save(&boot.report_path(), &s) {
            if !WRITE_FAILED.swap(true, Ordering::Relaxed) {
                eprintln!("xcl: {e}");
            }
        }
    }
}

static WRITE_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Write the report text to `path`; the error names the path and the OS error.
fn save(path: &std::path::Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("cannot write the report {}: {e}", path.display()))
}

pub fn load(path: &std::path::Path) -> Option<Report> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crash_is_kept_while_another_thread_writes_the_report() {
        let (held, wait_held) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            let _g = WRITE_LOCK.lock().unwrap();
            held.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(100));
        });
        wait_held.recv().unwrap();
        // no boot record in unit tests: nothing is written to disk
        write_final(Some("boom".into()));
        writer.join().unwrap();
        assert_eq!(CRASH.lock().unwrap().as_deref(), Some("boom"));
        *CRASH.lock().unwrap() = None;
    }

    #[test]
    fn a_report_that_cannot_be_written_names_its_path() {
        let dir = std::env::temp_dir().join(format!("xcl-report-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let blocker = dir.join("not-a-folder");
        std::fs::write(&blocker, "x").unwrap();
        let e = save(&blocker.join("report.json"), "{}").unwrap_err();
        assert!(e.starts_with("cannot write the report ") && e.contains("not-a-folder"), "{e}");
        let good = dir.join("report.json");
        save(&good, "{}").unwrap();
        assert_eq!(std::fs::read_to_string(&good).unwrap(), "{}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ---- fork telemetry (docs/guide/telemetry.md, part 1) ----
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;

use crate::log::{self as xlog, Tier};

/// Fork-only telemetry code that lives in its own files (never referenced by upstream files except through one-line calls).
#[path = "telemetry_run.rs"]
mod run_tel;
pub use run_tel::*;

/// `manifest.json` of a telemetry run (schema 1). Unknown values stay JSON null.
#[derive(Serialize, Default, Clone)]
struct Manifest {
    schema: u32,
    run_id: String,
    xcl_version: String,
    commit: Option<String>,
    commit_source: Option<String>,
    dirty: Option<bool>,
    rust: Option<String>,
    profile: String,
    os: String,
    cpu: Option<String>,
    cores: Option<u32>,
    threads: u32,
    ram_mb: u64,
    adapter: Option<String>,
    driver: Option<String>,
    vram_mb: Option<u64>,
    wave_lane_count_min: Option<u32>,
    wave_lane_count_max: Option<u32>,
    dxc: ManifestDxc,
    title: String,
    title_version: Option<String>,
    exe: String,
    /// where `xcl.log` of this run is (it stays in the logs folder)
    log: String,
    tier: String,
    switches: BTreeMap<String, String>,
    started: String,
    events_dropped: u64,
}

#[derive(Serialize, Default, Clone)]
struct ManifestDxc {
    dxcompiler: Option<String>,
    dxil: Option<String>,
}

static MANIFEST: Mutex<Option<Manifest>> = Mutex::new(None);

/// `XCL_TELEMETRY` on: run directory, manifest.json, events.jsonl. Off: one env read, nothing made.
/// Failures: one warning, telemetry stays off.
pub fn telemetry_start(boot: &crate::config::Boot) {
    let value = std::env::var("XCL_TELEMETRY").ok();
    let tier = match xlog::parse_tier(value.as_deref()) {
        Ok(Tier::Off) => return,
        Ok(t) => t,
        Err(e) => {
            crate::xlog_warn!("telemetry: {e} - telemetry off");
            return;
        }
    };
    let root = std::env::var_os("XCL_TELEMETRY_DIR").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| boot.logs_dir());
    let mut starts: Vec<PathBuf> = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            starts.push(parent.to_path_buf());
        }
    }
    let start_refs: Vec<&Path> = starts.iter().map(|p| p.as_path()).collect();
    let commit = git_commit(&start_refs);
    let (compact, iso) = xlog::utc_stamps(std::time::SystemTime::now());
    let dir = match xlog::create_run_dir(&root, &compact, commit.as_deref()) {
        Ok(d) => d,
        Err(e) => {
            crate::xlog_warn!("telemetry: cannot make a run folder in {}: {e} - telemetry off", root.display());
            return;
        }
    };
    let run_id = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let run_id = run_id.strip_prefix("run-").unwrap_or(&run_id).to_string();
    let m = new_manifest(boot, &run_id, tier, &iso, xcl_switches(std::env::vars_os()), commit);
    if !xlog::telemetry_open(&dir, tier) {
        crate::xlog_warn!("telemetry: cannot open events.jsonl in {} - telemetry off", dir.display());
        return;
    }
    write_manifest(&dir, &m);
    if let Some(mut g) = lock_within(&MANIFEST, Duration::from_millis(200)) {
        *g = Some(m);
    }
    crate::xlog_info!("telemetry: {} -> {}", tier.name(), dir.display());
}

/// The manifest of a run at its start (device and DXC fields still null).
fn new_manifest(boot: &crate::config::Boot, run_id: &str, tier: Tier, started: &str, switches: BTreeMap<String, String>, commit: Option<String>) -> Manifest {
    let (major, minor, build) = crate::hle::kernelx::host_os_version();
    Manifest {
        schema: 1,
        run_id: run_id.to_string(),
        xcl_version: env!("CARGO_PKG_VERSION").to_string(),
        commit_source: commit.as_ref().map(|_| "git-head".to_string()),
        commit,
        dirty: None,
        rust: None,
        profile: if cfg!(debug_assertions) { "debug" } else { "release" }.to_string(),
        os: format!("Windows {major}.{minor}.{build}"),
        cpu: cpu_brand(),
        cores: None,
        threads: std::thread::available_parallelism().map_or(0, |n| n.get() as u32),
        ram_mb: total_ram_mb(),
        title: boot.title.clone(),
        title_version: None,
        exe: boot.exe_path.to_string_lossy().into_owned(),
        log: boot.logs_dir().join("xcl.log").to_string_lossy().into_owned(),
        tier: tier.name().to_string(),
        switches,
        started: started.to_string(),
        events_dropped: 0,
        ..Default::default()
    }
}

fn total_ram_mb() -> u64 {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    if unsafe { GlobalMemoryStatusEx(&mut m) } == 0 {
        return 0;
    }
    m.ullTotalPhys >> 20
}

/// Changes the manifest and writes it again (no-op while telemetry has no run folder).
fn update_manifest(f: impl FnOnce(&mut Manifest)) {
    let Some(dir) = xlog::run_dir() else { return };
    let Some(mut g) = lock_within(&MANIFEST, Duration::from_millis(200)) else { return };
    let Some(m) = g.as_mut() else { return };
    f(m);
    m.events_dropped = xlog::events_dropped();
    write_manifest(dir, m);
}

/// What the device says about its adapter: (driver, VRAM in MiB, wave lane range). A query that
/// fails leaves its value `None`.
fn device_facts() -> (Option<String>, Option<u64>, Option<(u32, u32)>) {
    use windows::core::Interface;
    use windows::Win32::Graphics::Direct3D12::*;
    use windows::Win32::Graphics::Dxgi::*;
    let (mut driver, mut vram, mut waves) = (None, None, None);
    let raw = crate::hle::d3d12x::real::device_raw();
    let Some(dev) = (unsafe { ID3D12Device::from_raw_borrowed(&raw) }) else { return (driver, vram, waves) };
    let mut options = D3D12_FEATURE_DATA_D3D12_OPTIONS1::default();
    let asked = unsafe { dev.CheckFeatureSupport(D3D12_FEATURE_D3D12_OPTIONS1, &mut options as *mut _ as _, std::mem::size_of_val(&options) as u32) };
    if asked.is_ok() {
        waves = Some((options.WaveLaneCountMin, options.WaveLaneCountMax));
    }
    unsafe {
        let luid = dev.GetAdapterLuid();
        let factory: windows::core::Result<IDXGIFactory4> = CreateDXGIFactory1();
        let adapter: Option<IDXGIAdapter1> = factory.ok().and_then(|f| f.EnumAdapterByLuid(luid).ok());
        if let Some(adapter) = adapter {
            if let Ok(desc) = adapter.GetDesc1() {
                vram = Some((desc.DedicatedVideoMemory as u64) >> 20);
            }
            if let Ok(v) = adapter.CheckInterfaceSupport(&IDXGIDevice::IID) {
                let v = v as u64;
                driver = Some(format!("{}.{}.{}.{}", (v >> 48) & 0xffff, (v >> 32) & 0xffff, (v >> 16) & 0xffff, v & 0xffff));
            }
        }
    }
    (driver, vram, waves)
}

/// The D3D12 device was created: adapter name, driver (UMD) version, VRAM, wave lane range.
pub fn telemetry_device(adapter: &str) {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    let (driver, vram, waves) = device_facts();
    update_manifest(|m| {
        m.adapter = Some(adapter.to_string());
        m.driver = driver;
        m.vram_mb = vram;
        m.wave_lane_count_min = waves.map(|w| w.0);
        m.wave_lane_count_max = waves.map(|w| w.1);
    });
}

/// From write_final: `report.json` into the run directory, DXC versions into the manifest.
fn telemetry_final(report_json: &str) {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    let Some(dir) = xlog::run_dir() else { return };
    let tmp = dir.join("report.json.tmp");
    if std::fs::write(&tmp, report_json).is_ok() && std::fs::rename(&tmp, dir.join("report.json")).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    let tc = crate::gpu::recomp::dxc::toolchain();
    let version = |s: &str| if s.is_empty() { None } else { Some(s.to_string()) };
    update_manifest(|m| {
        m.dxc = ManifestDxc { dxcompiler: version(&tc.dxcompiler.file_version), dxil: version(&tc.dxil.file_version) };
    });
}

/// A console shader XCL could not translate (`real::note_translation_error`).
pub fn telemetry_shader_refused(reason: &str) {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let Some(n) = xlog::sampled(&COUNT) else { return };
    xlog::event(Tier::Frame, "shader_refused", &shader_refused_fields(reason, n));
}

/// A PSO the driver refused (`real::note_fallback`); `standin_built`: the stand-in pipeline was made.
pub fn telemetry_pipeline_refused(what: &str, hr: i32, standin_built: bool) {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let Some(n) = xlog::sampled(&COUNT) else { return };
    xlog::event(Tier::Frame, "pipeline", &pipeline_fields(what, hr, standin_built, n));
}

/// The `XCL_STALL_DUMP` watcher fired: the run ends right after.
pub fn telemetry_stall(last_frame: u64, idle_s: f64, limit_s: f64) {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    xlog::event(Tier::Frame, "stall", &stall_fields(last_frame, idle_s, limit_s));
}

/// A stub was called: written at its 1st, 10th, 100th ... call.
pub fn telemetry_stub(info: &crate::stubs::StubInfo, calls: u64, caller: usize) {
    if !xlog::telemetry_on(Tier::All) {
        return;
    }
    if !stub_threshold(calls) {
        return;
    }
    let mut fields: Vec<(&'static str, serde_json::Value)> =
        vec![("dll", info.dll.as_str().into()), ("function", info.name.as_str().into()), ("calls", calls.into())];
    if calls == 1 {
        fields.push(("caller", crate::modules::describe_address(caller).into()));
    }
    xlog::event(Tier::All, "stub", &fields);
}

/// A DirectStorage request failed.
pub fn telemetry_io_failed(hr: i32, bytes: u64, path: Option<&[u16]>) {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let Some(n) = xlog::sampled(&COUNT) else { return };
    xlog::event(Tier::Frame, "error", &io_failed_fields(hr, bytes, path, n));
}

/// `"0x8007000E"`.
fn hresult_text(hr: i32) -> String {
    format!("0x{:08X}", hr as u32)
}

/// (stage, hresult) of a refusal message: the stage is the lowercased prefix before `": "` when it
/// is one of VS / PS / CS / MS / VS+GS, the HRESULT the last `(0x...)` group.
fn refusal_parts(reason: &str) -> (Option<String>, Option<String>) {
    let stage = reason
        .split_once(": ")
        .map(|(p, _)| p)
        .filter(|p| ["VS", "PS", "CS", "MS", "VS+GS"].contains(p))
        .map(|p| p.to_ascii_lowercase());
    let hresult = reason.rfind("(0x").and_then(|i| {
        let rest = &reason[i + 3..];
        let hex = &rest[..rest.find(')')?];
        if hex.is_empty() || hex.len() > 8 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(hex, 16).ok().map(|v| hresult_text(v as i32))
    });
    (stage, hresult)
}

fn shader_refused_fields(reason: &str, n: u64) -> Vec<(&'static str, serde_json::Value)> {
    let (stage, hresult) = refusal_parts(reason);
    let mut f: Vec<(&'static str, serde_json::Value)> = Vec::new();
    if let Some(s) = stage {
        f.push(("stage", s.into()));
    }
    f.push(("reason", reason.into()));
    if let Some(h) = hresult {
        f.push(("hresult", h.into()));
    }
    f.push(("n", n.into()));
    f
}

fn pipeline_fields(what: &str, hr: i32, standin_built: bool, n: u64) -> Vec<(&'static str, serde_json::Value)> {
    let mut f: Vec<(&'static str, serde_json::Value)> = vec![("refused", true.into())];
    let stage = ["graphics", "compute", "stream"].into_iter().find(|s| what.contains(s));
    if let Some(s) = stage {
        f.push(("stage", s.into()));
    }
    f.push(("hresult", hresult_text(hr).into()));
    f.push(("source", if standin_built { "standin" } else { "none" }.into()));
    f.push(("n", n.into()));
    f
}

fn stall_fields(last_frame: u64, idle_s: f64, limit_s: f64) -> Vec<(&'static str, serde_json::Value)> {
    vec![("last_frame", last_frame.into()), ("idle_s", idle_s.into()), ("limit_s", limit_s.into()), ("ends_run", true.into())]
}

/// 1, 10, 100, 1000 ...
fn stub_threshold(calls: u64) -> bool {
    let mut c = calls;
    if c == 0 {
        return false;
    }
    while c % 10 == 0 {
        c /= 10;
    }
    c == 1
}

fn io_failed_fields(hr: i32, bytes: u64, path: Option<&[u16]>, n: u64) -> Vec<(&'static str, serde_json::Value)> {
    let mut f: Vec<(&'static str, serde_json::Value)> = vec![
        ("kind", "io_failed".into()),
        ("source", "dstorage".into()),
        ("detail", "request failed".into()),
        ("hresult", hresult_text(hr).into()),
        ("fatal", false.into()),
    ];
    if let Some(p) = path {
        f.push(("path", String::from_utf16_lossy(p).into()));
    }
    f.push(("bytes", bytes.into()));
    f.push(("n", n.into()));
    f
}

/// Every `XCL_*` variable (prefix matched case-insensitively), values decoded lossily.
fn xcl_switches(vars: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>) -> BTreeMap<String, String> {
    vars.filter_map(|(k, v)| {
        let k = k.to_string_lossy().into_owned();
        if k.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("XCL_")) {
            Some((k, v.to_string_lossy().into_owned()))
        } else {
            None
        }
    })
    .collect()
}

fn is_hex40(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The commit a ref names: a loose file in the checkout's git dir or the common dir, else `packed-refs`.
fn git_resolve_ref(gitdir: &Path, common: &Path, name: &str) -> Option<String> {
    if name.contains("..") {
        return None;
    }
    for base in [gitdir, common] {
        if let Ok(s) = std::fs::read_to_string(base.join(name)) {
            let s = s.trim();
            if is_hex40(s) {
                return Some(s.to_ascii_lowercase());
            }
        }
    }
    for base in [common, gitdir] {
        let Ok(packed) = std::fs::read_to_string(base.join("packed-refs")) else { continue };
        for line in packed.lines() {
            if let Some((hash, r)) = line.split_once(' ') {
                if r.trim() == name && is_hex40(hash) {
                    return Some(hash.to_ascii_lowercase());
                }
            }
        }
    }
    None
}

/// HEAD of the checkout whose `.git` (folder, or file of a worktree) is `dot_git`.
fn git_head(dot_git: &Path) -> Option<String> {
    let (gitdir, common) = if dot_git.is_dir() {
        (dot_git.to_path_buf(), dot_git.to_path_buf())
    } else {
        let text = std::fs::read_to_string(dot_git).ok()?;
        let target = text.lines().find_map(|l| l.strip_prefix("gitdir:"))?.trim().to_string();
        let gitdir = dot_git.parent()?.join(target);
        let common = match std::fs::read_to_string(gitdir.join("commondir")) {
            Ok(c) => gitdir.join(c.trim()),
            Err(_) => gitdir.clone(),
        };
        (gitdir, common)
    };
    let head = std::fs::read_to_string(gitdir.join("HEAD")).ok()?;
    let head = head.trim();
    match head.strip_prefix("ref:") {
        Some(name) => git_resolve_ref(&gitdir, &common, name.trim()),
        None if is_hex40(head) => Some(head.to_ascii_lowercase()),
        None => None,
    }
}

/// 40-hex HEAD of the first checkout found, looking at each start and its parents. Read from the
/// files: no process is spawned.
fn git_commit(starts: &[&Path]) -> Option<String> {
    for start in starts {
        for dir in start.ancestors() {
            let dot_git = dir.join(".git");
            if dot_git.exists() {
                if let Some(c) = git_head(&dot_git) {
                    return Some(c);
                }
                break;
            }
        }
    }
    None
}

/// The CPU's brand string (CPUID 0x80000002..4).
#[cfg(target_arch = "x86_64")]
fn cpu_brand() -> Option<String> {
    use std::arch::x86_64::__cpuid;
    #[allow(unused_unsafe)]
    unsafe {
        if __cpuid(0x8000_0000).eax < 0x8000_0004 {
            return None;
        }
        let mut bytes = Vec::with_capacity(48);
        for leaf in 0x8000_0002u32..=0x8000_0004 {
            let r = __cpuid(leaf);
            for w in [r.eax, r.ebx, r.ecx, r.edx] {
                bytes.extend_from_slice(&w.to_le_bytes());
            }
        }
        let s = String::from_utf8_lossy(&bytes);
        let s = s.trim_matches(|c: char| c == '\0' || c.is_whitespace());
        if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        }
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn cpu_brand() -> Option<String> {
    None
}

/// Pretty JSON to `manifest.json.tmp`, then renamed over `manifest.json`.
fn write_manifest(dir: &Path, m: &Manifest) {
    let text = match serde_json::to_string_pretty(m) {
        Ok(t) => t,
        Err(e) => {
            crate::xlog_warn!("telemetry: manifest not serialised: {e}");
            return;
        }
    };
    let tmp = dir.join("manifest.json.tmp");
    if let Err(e) = std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, dir.join("manifest.json"))) {
        crate::xlog_warn!("telemetry: manifest not written in {}: {e}", dir.display());
        let _ = std::fs::remove_file(&tmp);
    }
}

#[cfg(test)]
mod telemetry_tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!("xcl-report-telemetry-{name}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn as_json(fields: Vec<(&'static str, serde_json::Value)>) -> serde_json::Value {
        let map: serde_json::Map<String, serde_json::Value> = fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        serde_json::from_str(&serde_json::to_string(&map).unwrap()).unwrap()
    }

    #[test]
    fn manifest_has_every_field_and_is_valid_json() {
        let boot = crate::config::Boot {
            title: "Test Title".into(),
            exe_path: PathBuf::from("C:\\games\\t\\game.exe"),
            game_root: PathBuf::from("C:\\games\\t"),
            data_dir: PathBuf::from("C:\\data"),
            config: crate::config::Config::default(),
        };
        let mut switches = BTreeMap::new();
        switches.insert("XCL_A".to_string(), "1".to_string());
        let m = new_manifest(&boot, "20261125T180411Z-918d42c1f0aa", Tier::Frame, "2026-11-25T18:04:11Z", switches, None);
        let v: serde_json::Value = serde_json::from_str(&serde_json::to_string_pretty(&m).unwrap()).unwrap();
        let o = v.as_object().unwrap();
        for k in [
            "schema", "run_id", "xcl_version", "commit", "commit_source", "dirty", "rust", "profile", "os", "cpu", "cores", "threads", "ram_mb", "adapter", "driver",
            "vram_mb", "wave_lane_count_min", "wave_lane_count_max", "dxc", "title", "title_version", "exe", "log", "tier", "switches", "started", "events_dropped",
        ] {
            assert!(o.contains_key(k), "manifest lacks {k}");
        }
        assert_eq!(o.len(), 27);
        assert_eq!(o["schema"], 1);
        assert!(o["commit"].is_null() && o["commit_source"].is_null() && o["dirty"].is_null() && o["rust"].is_null());
        assert!(o["cores"].is_null() && o["adapter"].is_null() && o["title_version"].is_null());
        assert!(o["dxc"]["dxcompiler"].is_null() && o["dxc"]["dxil"].is_null());
        assert_eq!(o["tier"], "frame");
        assert_eq!(o["title"], "Test Title");
        assert_eq!(o["switches"]["XCL_A"], "1");
        assert!(o["log"].as_str().unwrap().ends_with("xcl.log"));
    }

    #[test]
    fn switches_keep_only_xcl_variables() {
        let vars = [("XCL_A", "1"), ("xcl_b", "2"), ("PATH", "x"), ("XCLX", "3")].into_iter().map(|(k, v)| (std::ffi::OsString::from(k), std::ffi::OsString::from(v)));
        let got = xcl_switches(vars);
        let names: Vec<&str> = got.keys().map(|k| k.as_str()).collect();
        assert_eq!(names, ["XCL_A", "xcl_b"]);
        assert_eq!(got["xcl_b"], "2");
    }

    #[test]
    fn git_commit_reads_a_checkout() {
        let (a, b, c) = ("a".repeat(40), "b".repeat(40), "c".repeat(40));
        // a loose ref
        let t = temp("git-loose");
        std::fs::create_dir_all(t.join(".git/refs/heads")).unwrap();
        std::fs::write(t.join(".git/HEAD"), "ref: refs/heads/m\n").unwrap();
        std::fs::write(t.join(".git/refs/heads/m"), format!("{a}\n")).unwrap();
        assert_eq!(git_commit(&[t.as_path()]), Some(a.clone()));
        // a start below the root finds it too
        std::fs::create_dir_all(t.join("crates/x")).unwrap();
        assert_eq!(git_commit(&[t.join("crates/x").as_path()]), Some(a.clone()));
        // packed-refs only
        let t = temp("git-packed");
        std::fs::create_dir_all(t.join(".git")).unwrap();
        std::fs::write(t.join(".git/HEAD"), "ref: refs/heads/m\n").unwrap();
        std::fs::write(t.join(".git/packed-refs"), format!("# pack-refs with: peeled\n{c} refs/heads/other\n{b} refs/heads/m\n")).unwrap();
        assert_eq!(git_commit(&[t.as_path()]), Some(b.clone()));
        // detached HEAD
        let t = temp("git-detached");
        std::fs::create_dir_all(t.join(".git")).unwrap();
        std::fs::write(t.join(".git/HEAD"), format!("{c}\n")).unwrap();
        assert_eq!(git_commit(&[t.as_path()]), Some(c.clone()));
        // a worktree: `.git` is a file, refs live in the common dir
        let t = temp("git-worktree");
        let wt_git = t.join("main/.git/worktrees/w");
        std::fs::create_dir_all(&wt_git).unwrap();
        std::fs::create_dir_all(t.join("main/.git/refs/heads")).unwrap();
        std::fs::create_dir_all(t.join("wt")).unwrap();
        std::fs::write(wt_git.join("HEAD"), "ref: refs/heads/w\n").unwrap();
        std::fs::write(wt_git.join("commondir"), "../..\n").unwrap();
        std::fs::write(t.join("main/.git/refs/heads/w"), format!("{b}\n")).unwrap();
        std::fs::write(t.join("wt/.git"), format!("gitdir: {}\n", wt_git.display())).unwrap();
        assert_eq!(git_commit(&[t.join("wt").as_path()]), Some(b.clone()));
        // no .git
        let t = temp("git-none");
        assert_eq!(git_commit(&[t.as_path()]), None);
    }

    #[test]
    fn the_manifest_is_replaced_whole() {
        let dir = temp("manifest");
        let mut m = Manifest { schema: 1, title: "first".into(), ..Default::default() };
        write_manifest(&dir, &m);
        m.title = "second".into();
        m.adapter = Some("GPU".into());
        write_manifest(&dir, &m);
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(v["title"], "second");
        assert_eq!(v["adapter"], "GPU");
        assert!(!dir.join("manifest.json.tmp").exists());
    }

    #[test]
    fn a_refusal_carries_its_hresult() {
        assert_eq!(hresult_text(0x8007000Eu32 as i32), "0x8007000E");
        assert_eq!(
            refusal_parts("the driver rejected the compute PSO with the translated CS (0x8007000e)"),
            (None, Some("0x8007000E".to_string()))
        );
        assert_eq!(refusal_parts("CS: unsupported instruction"), (Some("cs".to_string()), None));
        assert_eq!(refusal_parts("VS+GS: unsupported instruction").0.as_deref(), Some("vs+gs"));
        assert_eq!(refusal_parts("XX: not a stage").0, None);
        let v = as_json(shader_refused_fields("PS: bad (0x80070057)", 3));
        assert_eq!(v["stage"], "ps");
        assert_eq!(v["reason"], "PS: bad (0x80070057)");
        assert_eq!(v["hresult"], "0x80070057");
        assert_eq!(v["n"], 3);
        assert!(as_json(shader_refused_fields("plain", 1)).get("hresult").is_none());
    }

    #[test]
    fn a_pipeline_refusal_names_stage_hresult_and_source() {
        let v = as_json(pipeline_fields("compute PSO", crate::util::E_INVALIDARG, true, 1));
        assert_eq!(v["refused"], true);
        assert_eq!(v["stage"], "compute");
        assert_eq!(v["hresult"], "0x80070057");
        assert_eq!(v["source"], "standin");
        assert_eq!(v["n"], 1);
        let v = as_json(pipeline_fields("PSO stream", 0x8007000Eu32 as i32, false, 2));
        assert_eq!(v["stage"], "stream");
        assert_eq!(v["source"], "none");
        assert_eq!(as_json(pipeline_fields("graphics PSO", 0, true, 1))["stage"], "graphics");
    }

    #[test]
    fn a_stall_event_has_frame_and_times() {
        let v = as_json(stall_fields(940, 6.012, 6.0));
        assert_eq!(v["last_frame"], 940);
        assert_eq!(v["idle_s"], 6.012);
        assert_eq!(v["limit_s"], 6.0);
        assert_eq!(v["ends_run"], true);
    }

    #[test]
    fn stub_events_come_at_powers_of_ten() {
        for n in [1u64, 10, 100, 1000, 1_000_000] {
            assert!(stub_threshold(n), "{n}");
        }
        for n in [0u64, 2, 11, 20, 999, 1001] {
            assert!(!stub_threshold(n), "{n}");
        }
    }

    #[test]
    fn an_io_failure_is_an_error_with_path_bytes_hresult() {
        let mut path: Vec<u16> = "C:\\data\\\"a\".pak".encode_utf16().collect();
        path.push(0xD800);
        let v = as_json(io_failed_fields(0x80070026u32 as i32, 65536, Some(&path), 12));
        assert_eq!(v["kind"], "io_failed");
        assert_eq!(v["source"], "dstorage");
        assert_eq!(v["fatal"], false);
        assert_eq!(v["hresult"], "0x80070026");
        assert_eq!(v["bytes"], 65536);
        assert_eq!(v["n"], 12);
        let p = v["path"].as_str().unwrap();
        assert!(p.contains('\\') && p.contains('"') && p.contains('\u{FFFD}'));
        assert!(as_json(io_failed_fields(0, 0, None, 1)).get("path").is_none());
    }
}
