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
#[path = "telemetry_gpu.rs"]
mod gpu_tel;
pub use gpu_tel::*;

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
    gpu_tel::telemetry_gpu_start();
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
    gpu_tel::telemetry_gpu_final();
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
    shaders_final();
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

// ---- split-wave shader shape (docs/guide/telemetry.md, part 4; shareable) ----

/// What the split-wave report counts for one program: static exchange sites in the HLSL the
/// translator writes for it (how often each ran is not known here).
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Exchanges {
    /// `xcl_x_ballot(` sites attributed to a console pc
    pub ballot: u32,
    /// entry ballots (no pc: the prologue)
    pub entry: u32,
    /// `xcl_x_rla_uniform(` plus the folded meets
    pub readlane: u32,
    /// of `readlane`: folded reduction tails (each replaced two or more `v_readlane`)
    pub readlane_folded: u32,
    /// `xcl_x_rla_any(`
    pub bpermute: u32,
    /// `xcl_x_meet() && xcl_half` meets
    pub coherent_load: u32,
    /// `xcl_x_meet()` in no known form (must be 0; a test pins it)
    pub other: u32,
    /// all of the above except `readlane_folded`
    pub total: u32,
    /// sites whose pc lies in a loop range
    pub in_loops: u32,
    /// `#define XCL_X_THREADS`
    pub threads_per_group: u32,
    /// `#define XCL_X_WAVES`
    pub waves_per_group: u32,
    /// `#define XCL_X_SPIN 1`: the halves meet through counters, not group barriers
    pub spin: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct RegisterArray {
    pub name: String,
    pub dwords: u32,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Form {
    Straight,
    Structured,
    SwitchPc,
    Direct,
}

/// Console-side shape (backend independent), from `gpu::dxil::cfg::Cfg`.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct ProgramShape {
    pub instructions: u32,
    pub blocks: u32,
    pub loops: u32,
    pub max_loop_depth: u32,
    pub divergent_branches: u32,
    pub movrel_sites: u32,
    /// byte range [header pc, end of its farthest back-edge source) per loop header
    #[serde(skip)]
    pub loop_ranges: Vec<(u64, u64)>,
}

/// The program a container holds for a stage (mirrors `pso::console_translation` and
/// `pso::console_mesh_with`): code, mode, wave64, group threads (None for VS / PS).
/// `gs_start` is `Some` for a geometry program (its `GsInterface::gs_start`).
#[derive(Clone, Debug)]
pub struct ConsoleProgram {
    pub code: Vec<u8>,
    pub mode: crate::gpu::recomp::Mode,
    pub wave64: bool,
    pub group_threads: Option<u32>,
    pub gs_start: Option<Option<u64>>,
}

pub fn shape_console_program(blob: &[u8], stage: crate::gpu::recomp::pso::Stage) -> Result<ConsoleProgram, String> {
    use crate::gpu::recomp::{self, pso::Stage, CsInterface, Mode};
    if let Some(f) = recomp::pso::fspx(blob) {
        if stage != Stage::Compute {
            return Err("an FSPX (driver) container in a graphics slot".into());
        }
        // `CsInterface::from_rsrc2` is private to `gpu::recomp`: the same bits
        let r = f.rsrc2;
        let cs = CsInterface {
            threads: f.threads,
            user_sgprs: (r >> 1) & 31,
            tgid: [(r >> 7) & 1 == 1, (r >> 8) & 1 == 1, (r >> 9) & 1 == 1],
            tg_size: (r >> 10) & 1 == 1,
            tid_components: ((r >> 11) & 3) + 1,
        };
        return Ok(ConsoleProgram { code: f.code.to_vec(), mode: Mode::Compute(cs), wave64: recomp::detect_wave64(f.code), group_threads: Some(f.threads.iter().product()), gs_start: None });
    }
    let sh = crate::gpu::xbox::parse(blob).ok_or("not a console shader container")?;
    let want = match stage {
        Stage::Vertex => 4,
        Stage::Pixel => 5,
        Stage::Compute => 6,
        Stage::Mesh => 7,
    };
    let f = sh
        .fssm
        .iter()
        .find(|f| f.stages.first().is_some_and(|s| s.hw == want))
        .ok_or_else(|| format!("no hardware variant {}", crate::gpu::xbox::hw_stage_name(want)))?;
    let mode = recomp::interface(blob, &sh, f).ok_or("unknown shader interface")?;
    let fits = matches!((&mode, stage), (Mode::Vertex(_), Stage::Vertex) | (Mode::Pixel(_), Stage::Pixel) | (Mode::Compute(_), Stage::Compute) | (Mode::Mesh(_), Stage::Mesh));
    if !fits {
        return Err("the shader stage does not match its slot in the PSO".into());
    }
    let p = f.programs.first().ok_or("FSSM without a program")?;
    let code = p.offset.checked_add(p.len).and_then(|end| f.data.get(p.offset..end)).ok_or("FSSM program out of range")?.to_vec();
    let group_threads: Option<u32> = match &mode {
        Mode::Compute(cs) => Some(cs.threads.iter().product()),
        Mode::Mesh(ms) => Some(ms.threads),
        _ => None,
    };
    let wave64 = recomp::fssm_wave64(f, &code);
    Ok(ConsoleProgram { code, mode, wave64, group_threads, gs_start: None })
}

/// The ES variant of the console VS `vs` and the GS `gs` as one program (`geometry_program`).
pub fn shape_geometry_program(vs: &[u8], gs: &[u8]) -> Result<ConsoleProgram, String> {
    let (code, g) = crate::gpu::recomp::geometry_program(vs, gs)?;
    let gs_start = g.gs_start;
    Ok(ConsoleProgram { code, mode: crate::gpu::recomp::Mode::Geometry(g), wave64: true, group_threads: Some(64), gs_start: Some(gs_start) })
}

/// The console CFG's numbers for `code` (`gs_start` as in `ConsoleProgram`).
pub fn shape_program(code: &[u8], gs_start: Option<Option<u64>>) -> Result<ProgramShape, String> {
    use crate::gpu::dxil::cfg::{Cfg, Term};
    let cfg = match gs_start {
        Some(start) => Cfg::build_geometry(code, start)?,
        None => Cfg::build(code)?,
    };
    let (mut divergent, mut movrel) = (0u32, 0u32);
    for d in &cfg.insts {
        let n = d.inst.name;
        if matches!(n, "S_CBRANCH_EXECZ" | "S_CBRANCH_EXECNZ" | "S_CBRANCH_VCCZ" | "S_CBRANCH_VCCNZ") {
            divergent += 1;
        }
        if n.starts_with("V_MOVREL") || n == "V_SWAPREL_B32" {
            movrel += 1;
        }
    }
    let mut pos = vec![usize::MAX; cfg.blocks.len()];
    for (i, &b) in cfg.rpo.iter().enumerate() {
        pos[b] = i;
    }
    let code_end = cfg.insts.last().map_or(0, |d| d.pc + d.len as u64);
    let mut starts: Vec<u64> = cfg.blocks.iter().map(|b| b.pc).collect();
    starts.sort_unstable();
    starts.dedup();
    let end_of = |pc: u64| starts.iter().copied().find(|&s| s > pc).unwrap_or(code_end);
    // per loop header (a block): the end of its farthest back-edge source
    let mut far: BTreeMap<usize, u64> = BTreeMap::new();
    for &b in &cfg.rpo {
        let succ = match &cfg.blocks[b].term {
            Term::Jump(t) => vec![*t],
            Term::Cond(_, t, f) => vec![*t, *f],
            Term::End => vec![],
        };
        for t in succ {
            if cfg.headers.contains(&t) && pos[t] <= pos[b] {
                let e = far.entry(t).or_insert(0);
                *e = (*e).max(end_of(cfg.blocks[b].pc));
            }
        }
    }
    let loop_ranges: Vec<(u64, u64)> = far.into_iter().map(|(h, end)| (cfg.blocks[h].pc, end)).collect();
    // a range that starts inside another counts as nested in it (a partial overlap too)
    let max_loop_depth = loop_ranges.iter().map(|&(lo, _)| loop_ranges.iter().filter(|&&(a, b)| a <= lo && lo < b).count() as u32).max().unwrap_or(0);
    Ok(ProgramShape {
        instructions: cfg.insts.len() as u32,
        blocks: cfg.rpo.len() as u32,
        loops: cfg.headers.len() as u32,
        max_loop_depth,
        divergent_branches: divergent,
        movrel_sites: movrel,
        loop_ranges,
    })
}

/// The pc of a `// <pc>: <instruction>` comment line (`Em::inst` writes them with `{:04x}`).
fn shape_comment_pc(line: &str) -> Option<u64> {
    let rest = line.trim_start().strip_prefix("// ")?;
    let (hex, _) = rest.split_once(": ")?;
    if hex.len() < 4 || hex.len() > 8 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u64::from_str_radix(hex, 16).ok()
}

/// The number of `#define <name> <n>[u]`.
fn shape_hlsl_define(hlsl: &str, name: &str) -> Option<u32> {
    let key = format!("#define {name} ");
    let rest = &hlsl[hlsl.find(&key)? + key.len()..];
    rest.split_whitespace().next()?.trim_end_matches('u').parse().ok()
}

/// The exchange call sites of a split-wave64 program's HLSL. Only the text from `void main(`
/// on counts: the prelude before it defines the exchange functions. A site belongs to the pc
/// of the last `// <pc>:` comment before it, none yet: the entry.
pub fn shape_hlsl_exchanges(hlsl: &str, loop_ranges: &[(u64, u64)]) -> Exchanges {
    let mut e = Exchanges {
        threads_per_group: shape_hlsl_define(hlsl, "XCL_X_THREADS").unwrap_or(0),
        waves_per_group: shape_hlsl_define(hlsl, "XCL_X_WAVES").unwrap_or(0),
        spin: shape_hlsl_define(hlsl, "XCL_X_SPIN") == Some(1),
        ..Default::default()
    };
    let Some(main) = hlsl.find("void main(") else { return e };
    let mut pc: Option<u64> = None;
    for line in hlsl[main..].lines() {
        if let Some(p) = shape_comment_pc(line) {
            pc = Some(p);
            continue;
        }
        if line.trim_start().starts_with("//") {
            continue;
        }
        let ballots = line.matches("xcl_x_ballot(").count() as u32;
        let readlane = line.matches("xcl_x_rla_uniform(").count() as u32;
        let bpermute = line.matches("xcl_x_rla_any(").count() as u32;
        let meets = line.matches("xcl_x_meet()").count() as u32;
        let coherent = line.matches("xcl_x_meet() && xcl_half").count() as u32;
        let folded = (line.matches("xcl_x_meet() ? xcl_x_get(").count() + line.matches("_ok = xcl_x_meet();").count()) as u32;
        if pc.is_some() {
            e.ballot += ballots;
        } else {
            e.entry += ballots;
        }
        e.readlane += readlane + folded;
        e.readlane_folded += folded;
        e.bpermute += bpermute;
        e.coherent_load += coherent;
        e.other += meets.saturating_sub(coherent + folded);
        if pc.is_some_and(|p| loop_ranges.iter().any(|&(lo, hi)| lo <= p && p < hi)) {
            e.in_loops += ballots + readlane + bpermute + meets;
        }
    }
    e.total = e.ballot + e.entry + e.readlane + e.bpermute + e.coherent_load + e.other;
    e
}

/// Every non-const `static uint NAME[N];` of the HLSL (today only `xcl_scratch`).
pub fn shape_hlsl_register_arrays(hlsl: &str) -> Vec<RegisterArray> {
    hlsl.lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("static uint ")?;
            let (name, n) = rest.strip_suffix("];")?.split_once('[')?;
            if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                return None;
            }
            Some(RegisterArray { name: name.to_string(), dwords: n.parse().ok()? })
        })
        .collect()
}

/// The control-flow form of the HLSL: the `pc` chain (`uint pc = ` in `main`), structured
/// control flow, or a straight line (`branches`: the program has branches).
pub fn shape_hlsl_form(hlsl: &str, branches: bool) -> Form {
    let body = hlsl.find("void main(").map_or(hlsl, |i| &hlsl[i..]);
    if body.contains("\n    uint pc = ") {
        Form::SwitchPc
    } else if branches {
        Form::Structured
    } else {
        Form::Straight
    }
}

/// Whether the HLSL declares the upper-lanes form (`static bool xcl_hw32`).
pub fn shape_hlsl_upper_lanes(hlsl: &str) -> bool {
    hlsl.contains("static bool xcl_hw32")
}

/// The HLSL backend's shape of one program: what ran (`split` as passed) and, for a wave64
/// compute / mesh / geometry program, exchanges of the split form, emitted for counting when it did not run split.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct HlslShape {
    pub program: ProgramShape,
    pub form: Form,
    pub unstructured: bool,
    pub register_arrays: Vec<RegisterArray>,
    pub private_bytes_per_thread: u64,
    pub upper_lanes: Option<bool>,
    pub exchanges: Option<Exchanges>,
    pub hlsl_bytes: u64,
}

/// The program's HLSL as the translator writes it (no DXC), with `split` as the split flag.
fn shape_emit(p: &ConsoleProgram, windows: u32, split: bool) -> Result<crate::gpu::recomp::Translation, String> {
    use crate::gpu::recomp::{self, Mode, Options};
    match &p.mode {
        Mode::Mesh(ms) => recomp::translate_mesh(&p.code, ms.clone(), p.wave64, split, windows),
        Mode::Geometry(g) => recomp::translate_geometry(&p.code, g.clone(), split, windows),
        mode => recomp::translate(&p.code, &Options { mode: mode.clone(), wave64: p.wave64, split_wave64: split, mem_windows: windows, ..Default::default() }),
    }
}

/// Bytes of private memory per thread the arrays declare.
fn shape_private_bytes(arrays: &[RegisterArray]) -> u64 {
    arrays.iter().map(|a| a.dwords as u64 * 4).sum()
}

pub fn shape_hlsl(p: &ConsoleProgram, stage_is_graphics: bool, windows: u32, split: bool) -> Result<HlslShape, String> {
    let program = shape_program(&p.code, p.gs_start)?;
    let t = shape_emit(p, windows, split)?;
    let register_arrays = shape_hlsl_register_arrays(&t.hlsl);
    let form = shape_hlsl_form(&t.hlsl, t.blocks > 1);
    let exchanges = if stage_is_graphics || !p.wave64 {
        None
    } else if split {
        Some(shape_hlsl_exchanges(&t.hlsl, &program.loop_ranges))
    } else {
        // the split form is emitted for counting only: the numbers do not depend on the device
        Some(shape_hlsl_exchanges(&shape_emit(p, windows, true)?.hlsl, &program.loop_ranges))
    };
    Ok(HlslShape {
        form,
        unstructured: form == Form::SwitchPc,
        private_bytes_per_thread: shape_private_bytes(&register_arrays),
        upper_lanes: stage_is_graphics.then(|| shape_hlsl_upper_lanes(&t.hlsl)),
        hlsl_bytes: t.hlsl.len() as u64,
        register_arrays,
        exchanges,
        program,
    })
}

#[cfg(test)]
mod shape_tests {
    use super::*;
    use crate::gpu::recomp::{translate, CsInterface, Mode, Options, Translation};

    const V: u64 = 256;
    type I = (&'static str, &'static str, &'static [(&'static str, u64)]);
    const ENDPGM: I = ("S_ENDPGM", "ENC_SOPP", &[("SIMM16", 0)]);
    const MOV: I = ("V_MOV_B32", "ENC_VOP1", &[("VDST", 1), ("SRC0", V)]);
    const BPERM1: I = ("DS_BPERMUTE_B32", "ENC_DS", &[("VDST", 3), ("ADDR", 1), ("DATA0", 2)]);
    const BPERM2: I = ("DS_BPERMUTE_B32", "ENC_DS", &[("VDST", 4), ("ADDR", 1), ("DATA0", 2)]);
    const RLANE5: I = ("V_READLANE_B32", "ENC_VOP3", &[("VDST", 4), ("SRC0", V + 1), ("SRC1", 128 + 5)]);
    const RLANE3: I = ("V_READLANE_B32", "ENC_VOP3", &[("VDST", 4), ("SRC0", V + 1), ("SRC1", 128 + 3)]);
    const RLANE35: I = ("V_READLANE_B32", "ENC_VOP3", &[("VDST", 5), ("SRC0", V + 1), ("SRC1", 128 + 35)]);
    const CMP: I = ("V_CMP_GT_U32", "ENC_VOPC", &[("SRC0", V), ("VSRC1", 1)]);
    const CNDMASK: I = ("V_CNDMASK_B32", "ENC_VOP2", &[("VDST", 2), ("SRC0", V + 3), ("VSRC1", 4)]);
    const MOV_S2_VCC: I = ("S_MOV_B64", "ENC_SOP1", &[("SDST", 2), ("SSRC0", 106)]);
    const COHERENT: I = ("S_LOAD_DWORD", "ENC_SMEM", &[("SDATA", 4), ("SBASE", 0), ("OFFSET", 0), ("SOFFSET", 125), ("GLC", 1)]);
    const SCRATCH_ST: I = ("SCRATCH_STORE_DWORD", "ENC_FLAT_SCRATCH", &[("ADDR", 2), ("DATA", 1), ("SADDR", 0x7F)]);
    const SCRATCH_LD: I = ("SCRATCH_LOAD_DWORD", "ENC_FLAT_SCRATCH", &[("VDST", 3), ("ADDR", 2), ("SADDR", 0x7F)]);
    // branch offsets are in dwords from the next instruction; DS, VOP3, SMEM and FLAT are 8 bytes
    const SCC1_BACK_2: I = ("S_CBRANCH_SCC1", "ENC_SOPP", &[("SIMM16", 0xFFFE)]);
    const SCC1_BACK_4: I = ("S_CBRANCH_SCC1", "ENC_SOPP", &[("SIMM16", 0xFFFC)]);
    const SCC1_BACK_6: I = ("S_CBRANCH_SCC1", "ENC_SOPP", &[("SIMM16", 0xFFFA)]);
    const EXECZ_FWD_1: I = ("S_CBRANCH_EXECZ", "ENC_SOPP", &[("SIMM16", 1)]);
    const VCCNZ_BACK_4: I = ("S_CBRANCH_VCCNZ", "ENC_SOPP", &[("SIMM16", 0xFFFC)]);

    fn asm(lines: &[I]) -> Vec<u8> {
        lines.iter().flat_map(|(n, e, f)| crate::gpu::rdna2::encode(n, e, f, None).unwrap_or_else(|x| panic!("{n}: {x}"))).collect()
    }

    fn cs() -> Mode {
        Mode::Compute(CsInterface { threads: [64, 1, 1], user_sgprs: 0, tgid: [false; 3], tg_size: false, tid_components: 1 })
    }

    fn emit(code: &[u8], split: bool, structured: bool) -> Translation {
        translate(code, &Options { mode: cs(), wave64: true, split_wave64: split, structured, ..Default::default() }).unwrap()
    }

    fn exchanges(code: &[u8]) -> Exchanges {
        shape_hlsl_exchanges(&emit(code, true, true).hlsl, &[])
    }

    fn t1() -> Vec<u8> {
        asm(&[BPERM1, BPERM2, RLANE5, ENDPGM])
    }
    fn t2() -> Vec<u8> {
        asm(&[CMP, CNDMASK, ENDPGM])
    }
    fn t2_control() -> Vec<u8> {
        asm(&[CMP, CNDMASK, MOV_S2_VCC, ENDPGM])
    }
    fn t3_pair() -> Vec<u8> {
        asm(&[RLANE3, RLANE35, ENDPGM])
    }
    fn t3_coherent() -> Vec<u8> {
        asm(&[COHERENT, ENDPGM])
    }
    fn t4_loop() -> Vec<u8> {
        asm(&[MOV, SCC1_BACK_2, ENDPGM])
    }
    fn t4_scratch() -> Vec<u8> {
        asm(&[MOV, SCRATCH_ST, SCRATCH_LD, SCC1_BACK_6, ENDPGM])
    }
    /// two nested loops, a forward `execz` branch inside the inner one:
    /// 0 mov (outer header), 1 mov (inner header), 2 execz -> 4, 3 mov, 4 vccnz -> 1, 5 scc1 -> 0, 6 endpgm
    fn t5() -> Vec<u8> {
        asm(&[MOV, MOV, EXECZ_FWD_1, MOV, VCCNZ_BACK_4, SCC1_BACK_6, ENDPGM])
    }
    /// mov at pc 0 (header), bpermute at pc 4 (8 bytes), scc1 at pc 12 -> 0, endpgm at pc 16
    fn t6() -> Vec<u8> {
        asm(&[MOV, BPERM1, SCC1_BACK_4, ENDPGM])
    }

    #[test]
    fn t1_bpermute_and_readlane_are_counted_and_native_has_none() {
        let e = exchanges(&t1());
        assert_eq!((e.bpermute, e.readlane, e.readlane_folded, e.ballot, e.entry, e.coherent_load, e.other), (2, 1, 0, 0, 1, 0, 0), "{e:?}");
        assert_eq!((e.total, e.waves_per_group, e.threads_per_group), (4, 1, 64), "{e:?}");
        // native wave64: no exchange at all
        let native = emit(&t1(), false, true).hlsl;
        assert!(!native.contains("xcl_x_"), "the native form has no exchange call");
        assert_eq!(shape_hlsl_exchanges(&native, &[]).total, 0);
        // the shape counts the split form on a device that did not split, with the same numbers
        let p = ConsoleProgram { code: t1(), mode: cs(), wave64: true, group_threads: Some(64), gs_start: None };
        let (ran_split, ran_native) = (shape_hlsl(&p, false, 1, true).unwrap(), shape_hlsl(&p, false, 1, false).unwrap());
        assert_eq!(ran_split.exchanges, ran_native.exchanges);
        assert_eq!(ran_native.exchanges.as_ref().map(|e| e.total), Some(4));
        assert_eq!(ran_native.upper_lanes, None);
        assert_eq!(shape_hlsl(&p, true, 1, true).unwrap().exchanges, None, "VS / PS: no exchanges");
        let mut wave32 = p.clone();
        wave32.wave64 = false;
        assert_eq!(shape_hlsl(&wave32, false, 1, true).unwrap().exchanges, None, "wave32: no exchanges");
    }

    #[test]
    fn t2_a_mask_read_lane_by_lane_needs_no_exchange() {
        let e = exchanges(&t2());
        assert_eq!((e.ballot, e.entry, e.total), (0, 1, 1), "{e:?}");
        let e = exchanges(&t2_control());
        assert_eq!((e.ballot, e.entry, e.total), (1, 1, 2), "{e:?}");
    }

    #[test]
    fn t3_a_folded_pair_and_a_coherent_load() {
        let e = exchanges(&t3_pair());
        assert_eq!((e.readlane, e.readlane_folded, e.other), (1, 1, 0), "{e:?}");
        let e = exchanges(&t3_coherent());
        assert_eq!((e.coherent_load, e.other), (1, 0), "{e:?}");
    }

    #[test]
    fn t4_register_arrays_and_form() {
        let switch_pc = emit(&t4_loop(), true, false);
        assert_eq!(shape_hlsl_form(&switch_pc.hlsl, switch_pc.blocks > 1), Form::SwitchPc);
        let arrays = shape_hlsl_register_arrays(&switch_pc.hlsl);
        assert_eq!((arrays.clone(), shape_private_bytes(&arrays)), (vec![], 0));
        let scratch = emit(&t4_scratch(), true, false);
        let arrays = shape_hlsl_register_arrays(&scratch.hlsl);
        assert_eq!(arrays, vec![RegisterArray { name: "xcl_scratch".into(), dwords: 1024 }]);
        assert_eq!(shape_private_bytes(&arrays), 4096);
        let structured = emit(&t4_loop(), true, true);
        assert_eq!(shape_hlsl_form(&structured.hlsl, structured.blocks > 1), Form::Structured);
        let p = ConsoleProgram { code: t4_loop(), mode: cs(), wave64: true, group_threads: Some(64), gs_start: None };
        let h = shape_hlsl(&p, false, 1, true).unwrap();
        assert_eq!((h.form, h.unstructured, h.private_bytes_per_thread), (Form::Structured, false, 0));
        let straight = emit(&asm(&[MOV, ENDPGM]), true, true);
        assert_eq!(shape_hlsl_form(&straight.hlsl, straight.blocks > 1), Form::Straight);
    }

    #[test]
    fn t5_program_shape() {
        let code = t5();
        let s = shape_program(&code, None).unwrap();
        assert_eq!((s.divergent_branches, s.loops, s.max_loop_depth, s.movrel_sites), (2, 2, 2, 0), "{s:?}");
        assert_eq!(s.instructions as usize, emit(&code, true, true).instructions);
        assert_eq!(s.loop_ranges, vec![(0, 24), (4, 20)]);
        let flat = shape_program(&asm(&[MOV, ENDPGM]), None).unwrap();
        assert_eq!((flat.loops, flat.max_loop_depth, flat.blocks), (0, 0, 1));
    }

    #[test]
    fn t6_sites_in_loops_and_every_meet_classifies() {
        let code = t6();
        let s = shape_program(&code, None).unwrap();
        assert_eq!(s.loop_ranges, vec![(0, 16)]);
        let e = shape_hlsl_exchanges(&emit(&code, true, true).hlsl, &s.loop_ranges);
        assert_eq!((e.bpermute, e.in_loops, e.total), (1, 1, 2), "{e:?}");
        // drift guard: a new meet form in the emitter would land in `other`
        for (what, code) in [("t1", t1()), ("t2", t2()), ("t2c", t2_control()), ("t3p", t3_pair()), ("t3c", t3_coherent()), ("t4", t4_loop()), ("t4s", t4_scratch()), ("t5", t5()), ("t6", t6())] {
            let e = exchanges(&code);
            assert_eq!(e.other, 0, "{what}: an `xcl_x_meet()` in no known form: {e:?}");
        }
    }
}

// ---- fork telemetry (docs/guide/telemetry.md, part 4: shaders.json) ----
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::sync::{Arc, OnceLock};

use crate::gpu::recomp::pso::{Compiled, Stage, TwoPass};
use serde_json::{json, Map, Value};

/// Jobs the shape queue holds, and the bytes of the blob and DXIL copies in them.
const SHAPE_QUEUE_JOBS: usize = 1024;
const SHAPE_QUEUE_BYTES: u64 = 64 << 20;
/// Records `shaders.json` holds; more stay in `shaders.jsonl` only.
const SHAPE_RECORDS_CAP: usize = 50_000;

/// (fnv64 of the blob (geometry: of the GS xor the VS), stage, split argument, windows).
type ShapeKey = (u64, u8, bool, u32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Vs,
    Ps,
    Cs,
    Ms,
    Gs,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Vs => "vs",
            Kind::Ps => "ps",
            Kind::Cs => "cs",
            Kind::Ms => "ms",
            Kind::Gs => "gs",
        }
    }

    fn graphics(self) -> bool {
        matches!(self, Kind::Vs | Kind::Ps)
    }

    /// The pipeline slot of a container program (a geometry program has none: it goes through
    /// `shape_geometry_program`).
    fn stage(self) -> Stage {
        match self {
            Kind::Vs => Stage::Vertex,
            Kind::Ps => Stage::Pixel,
            Kind::Ms => Stage::Mesh,
            Kind::Cs | Kind::Gs => Stage::Compute,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Backend {
    Hlsl,
    Direct,
}

impl Backend {
    fn name(self) -> &'static str {
        match self {
            Backend::Hlsl => "hlsl",
            Backend::Direct => "direct",
        }
    }
}

/// One program to analyse, with copies of what the worker needs from the caller.
struct Job {
    key: ShapeKey,
    kind: Kind,
    /// the container (geometry: the GS)
    blob: Vec<u8>,
    /// geometry: the VS container
    vs: Option<Vec<u8>>,
    split_arg: bool,
    windows: u32,
    /// the DXIL the call returned (the compute pass for mesh / geometry)
    dxil: Vec<u8>,
    chain_dwords: Option<u32>,
    chain_pieces: Option<u32>,
    compile_ms: f64,
}

impl Job {
    #[allow(clippy::too_many_arguments)]
    fn new(kind: Kind, blob: &[u8], vs: Option<&[u8]>, split_arg: bool, windows: u32, dxil: &[u8], chain: Option<&crate::gpu::dxil::kernel::DispatchChain>, compile_ms: f64) -> Job {
        let hash = crate::gpu::shader::fnv64(blob);
        let hash = match vs {
            Some(v) => hash ^ crate::gpu::shader::fnv64(v).rotate_left(32),
            None => hash,
        };
        Job {
            key: (hash, kind as u8, split_arg, windows),
            kind,
            blob: blob.to_vec(),
            vs: vs.map(|v| v.to_vec()),
            split_arg,
            windows,
            dxil: dxil.to_vec(),
            chain_dwords: chain.map(|c| c.dwords_per_group),
            chain_pieces: chain.map(|c| c.pieces.len() as u32),
            compile_ms,
        }
    }

    /// Bytes of the copies, for the queue's budget.
    fn bytes(&self) -> u64 {
        (self.blob.len() + self.vs.as_ref().map_or(0, |v| v.len()) + self.dxil.len()) as u64
    }

    fn record_hex(&self) -> String {
        format!("{:016x}", crate::gpu::shader::fnv64(&self.blob))
    }

    /// The identity `seen` and `shape` lines share (what `shaders_final` merges them by).
    fn key_text(&self) -> String {
        format!("{:016x}.{}.{}.{}", self.key.0, self.kind.name(), self.key.2 as u8, self.key.3)
    }
}

#[derive(Default)]
struct ShapeCounts {
    seen: AtomicU64,
    analysed: AtomicU64,
    dropped: AtomicU64,
    failed: AtomicU64,
}

/// What the worker shares with the sink.
struct ShapeState {
    lines: xlog::Events,
    counts: ShapeCounts,
    queued_bytes: AtomicU64,
    /// jobs queued or being analysed
    outstanding: AtomicU64,
}

/// One sink (the global one, or a test's own: part 1's globals are OnceLocks, tests must not open them).
struct ShapeLog {
    dir: PathBuf,
    state: Arc<ShapeState>,
    seen: Mutex<HashSet<ShapeKey>>,
    queue: SyncSender<Job>,
    /// bytes the queue may hold
    budget: u64,
    /// set by `write_shaders_json`: no new program is taken
    closed: AtomicBool,
}

static SHAPE_LOG: OnceLock<Option<ShapeLog>> = OnceLock::new();

/// The global sink; created on first use when telemetry has a run folder.
fn shape_log() -> Option<&'static ShapeLog> {
    let dir = xlog::run_dir()?;
    SHAPE_LOG
        .get_or_init(|| match ShapeLog::create(dir, SHAPE_QUEUE_BYTES, true) {
            Ok(l) => Some(l),
            Err(e) => {
                crate::xlog_warn!("telemetry: {e} - shader shapes off");
                None
            }
        })
        .as_ref()
}

impl ShapeLog {
    /// `shaders.jsonl` in `dir`; `worker`: also start the analysing thread (tests without one
    /// write lines themselves).
    fn create(dir: &Path, budget: u64, worker: bool) -> Result<ShapeLog, String> {
        let lines = xlog::Events::create(&dir.join("shaders.jsonl")).map_err(|e| format!("cannot open shaders.jsonl in {}: {e}", dir.display()))?;
        let state = Arc::new(ShapeState { lines, counts: ShapeCounts::default(), queued_bytes: AtomicU64::new(0), outstanding: AtomicU64::new(0) });
        let (queue, rx) = std::sync::mpsc::sync_channel::<Job>(SHAPE_QUEUE_JOBS);
        if worker {
            let st = state.clone();
            // `translate` recurses (`MAX_NEST` 24): a big stack
            std::thread::Builder::new().name("xcl-shader-shape".into()).stack_size(64 << 20).spawn(move || shape_worker(rx, st)).map_err(|e| format!("cannot start the shape worker: {e}"))?;
        }
        Ok(ShapeLog { dir: dir.to_path_buf(), state, seen: Mutex::new(HashSet::new()), queue, budget, closed: AtomicBool::new(false) })
    }

    /// The first sighting of a program: a `seen` line, and the job for the worker. Never waits:
    /// a full queue (jobs or bytes) drops the job and says so in the line.
    fn admit(&self, job: Job) {
        if self.closed.load(Ordering::Relaxed) {
            return;
        }
        match self.seen.lock() {
            Ok(mut s) => {
                if !s.insert(job.key) {
                    return;
                }
            }
            Err(_) => return,
        }
        self.state.counts.seen.fetch_add(1, Ordering::Relaxed);
        let mut fields = seen_fields(&job);
        let bytes = job.bytes();
        let before = self.state.queued_bytes.fetch_add(bytes, Ordering::AcqRel);
        let mut dropped = before.saturating_add(bytes) > self.budget;
        if !dropped {
            self.state.outstanding.fetch_add(1, Ordering::AcqRel);
            if let Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) = self.queue.try_send(job) {
                self.state.outstanding.fetch_sub(1, Ordering::AcqRel);
                dropped = true;
            }
        }
        if dropped {
            self.state.queued_bytes.fetch_sub(bytes, Ordering::AcqRel);
            self.state.counts.dropped.fetch_add(1, Ordering::Relaxed);
            fields.push(("shape", "dropped".into()));
        }
        self.state.lines.write("seen", &fields);
    }

    /// Waits until the worker has nothing queued or running, for at most `limit`.
    fn wait_idle(&self, limit: Duration) -> bool {
        let end = Instant::now() + limit;
        while self.state.outstanding.load(Ordering::Acquire) > 0 {
            if Instant::now() >= end {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        true
    }

    /// `shaders.json` from `shaders.jsonl`: stops intake, waits up to 2 s for the queue, merges
    /// `seen` and `shape` lines by key (a program without its shape is `pending_at_exit`), and
    /// writes the file through a temporary one. Safe to call again.
    fn write_shaders_json(&self, mut header: Map<String, Value>) -> std::io::Result<()> {
        self.closed.store(true, Ordering::Release);
        self.wait_idle(Duration::from_secs(2));
        let text = std::fs::read_to_string(self.dir.join("shaders.jsonl"))?;
        let m = merge_shape_lines(&text);
        header.insert(
            "counts".into(),
            json!({
                "records": m.records.len(),
                "analysed": m.analysed,
                "dropped": m.dropped,
                "failed": m.failed,
                "pending_at_exit": m.pending,
                "kept_in_memory_cap_hit": m.over_cap,
            }),
        );
        // compact, one record per line inside the array
        let head = serde_json::to_string(&Value::Object(header)).map_err(std::io::Error::other)?;
        let head = head.strip_suffix('}').unwrap_or(&head);
        let records: Vec<String> = m.records.iter().filter_map(|r| serde_json::to_string(r).ok()).collect();
        let doc = format!("{head},\"records\":[\n{}\n]}}\n", records.join(",\n"));
        let tmp = self.dir.join("shaders.json.tmp");
        std::fs::write(&tmp, doc).and_then(|_| std::fs::rename(&tmp, self.dir.join("shaders.json"))).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
    }
}

/// The fields of a program's `seen` line. `split` is the split argument of the call (the
/// record's `split` needs the program's wave size).
fn seen_fields(job: &Job) -> Vec<(&'static str, Value)> {
    let mut f: Vec<(&'static str, Value)> = vec![("key", job.key_text().into()), ("record", job.record_hex().into())];
    if let Some(vs) = &job.vs {
        f.push(("vs", format!("{:016x}", crate::gpu::shader::fnv64(vs)).into()));
    }
    f.push(("stage", job.kind.name().into()));
    f.push(("windows", job.windows.into()));
    f.push(("split", job.split_arg.into()));
    f.push(("wave64", Value::Null));
    f.push(("dxil_bytes", (job.dxil.len() as u64).into()));
    f.push(("compile_ms", job.compile_ms.into()));
    f.push(("split_reason", split_reason().into()));
    f
}

/// The worker: one job at a time at below-normal priority; a panic is a failed record.
fn shape_worker(rx: Receiver<Job>, st: Arc<ShapeState>) {
    use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL};
    static DRAINED: AtomicU64 = AtomicU64::new(0);
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) };
    while let Ok(job) = rx.recv() {
        let bytes = job.bytes();
        let record = shape_job(job);
        let failed = record["shape_status"].as_str().is_some_and(|s| s.starts_with("failed"));
        if failed {
            st.counts.failed.fetch_add(1, Ordering::Relaxed);
        } else {
            st.counts.analysed.fetch_add(1, Ordering::Relaxed);
        }
        if let Some(o) = record.as_object() {
            let fields: Vec<(&str, Value)> = o.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
            st.lines.write("shape", &fields);
        }
        st.queued_bytes.fetch_sub(bytes, Ordering::AcqRel);
        // the queue is empty: a reader can tell `pass` timings taken while the worker was busy
        if st.outstanding.fetch_sub(1, Ordering::AcqRel) == 1 {
            if let Some(n) = xlog::sampled(&DRAINED) {
                let (analysed, failed) = (st.counts.analysed.load(Ordering::Relaxed), st.counts.failed.load(Ordering::Relaxed));
                xlog::event(Tier::Pass, "shape_queue", &[("n", n.into()), ("analysed", analysed.into()), ("failed", failed.into())]);
            }
        }
    }
}

fn first_line(e: &str) -> &str {
    e.lines().next().unwrap_or("")
}

/// The same text as `pso::wave_mode_line`'s reason (that one is private).
fn split_reason_text(range: Option<(u32, u32)>, forced: bool) -> String {
    match (forced, range) {
        (true, _) => "XCL_SPLIT_WAVE64=1".to_string(),
        (false, None) => "wave range unknown".to_string(),
        (false, Some((min, max))) => format!("device waves {min}-{max}"),
    }
}

/// The device's wave lane range from the manifest, when the device has reported it.
fn manifest_waves() -> Option<(u32, u32)> {
    let g = lock_within(&MANIFEST, Duration::from_millis(20))?;
    let m = g.as_ref()?;
    m.wave_lane_count_min.zip(m.wave_lane_count_max)
}

/// Why wave64 compute / mesh / geometry programs run as they do on this device.
fn split_reason() -> String {
    split_reason_text(manifest_waves(), crate::gpu::recomp::pso::split_wave64_forced())
}

/// Which translator made the DXIL: `!llvm.ident` is "xcl" in the direct translator's modules
/// (DXC writes its own). Any metadata string "xcl" of the module's metadata block counts. None:
/// the bitcode could not be read.
fn dxil_backend(container: &[u8]) -> Option<Backend> {
    use crate::gpu::dxil::bitreader::{BitReader, Entry as BitEntry};
    let bitcode = crate::gpu::dxil::stats::dxil_bitcode(container).ok()?;
    let mut r = BitReader::new(bitcode).ok()?;
    let mut ctx: Vec<u32> = Vec::new();
    let (mut strings, mut xcl) = (0u32, false);
    while let Some(e) = r.next_entry().ok()? {
        match e {
            // module block 8, its metadata block 15
            BitEntry::Enter(id) => {
                if matches!((ctx.as_slice(), id), ([], 8) | ([8], 15)) {
                    ctx.push(id);
                } else {
                    r.skip_block().ok()?;
                }
            }
            BitEntry::End(_) => {
                ctx.pop();
            }
            // METADATA_STRING: the characters
            BitEntry::Record(1) if ctx.as_slice() == [8, 15] => {
                strings += 1;
                xcl |= r.ops().len() == 3 && r.ops().iter().zip(b"xcl").all(|(&a, &b)| a == b as u64);
            }
            BitEntry::Record(_) => {}
        }
    }
    if xcl {
        Some(Backend::Direct)
    } else if strings > 0 {
        Some(Backend::Hlsl)
    } else {
        None
    }
}

/// Whether the stage is routed to the direct translator by `XCL_DIRECT_DXIL` (it may still fall back).
fn shape_routed_direct(kind: Kind) -> bool {
    if kind.graphics() {
        crate::gpu::dxil::kernel::enabled_graphics()
    } else {
        crate::gpu::dxil::kernel::enabled()
    }
}

/// HLSL when the stage is not routed to direct and the rebuilt HLSL is not over what DXC
/// compiles optimised (a compute program that big takes the direct translator); otherwise
/// what the DXIL says (`ident`).
fn pick_backend(kind: Kind, hlsl_bytes: Option<u64>, ident: Option<Backend>) -> Option<Backend> {
    let enormous = kind == Kind::Cs && hlsl_bytes.is_some_and(|n| n > crate::gpu::recomp::dxc::UNOPTIMISED_ABOVE as u64);
    if !shape_routed_direct(kind) && !enormous && hlsl_bytes.is_some() {
        Some(Backend::Hlsl)
    } else {
        ident
    }
}

/// A record with everything the call site knows and every analysed field null.
fn shape_skeleton(job: &Job) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("record".into(), json!(job.record_hex()));
    if let Some(vs) = &job.vs {
        m.insert("vs".into(), json!(format!("{:016x}", crate::gpu::shader::fnv64(vs))));
    }
    m.insert("stage".into(), json!(job.kind.name()));
    m.insert("windows".into(), json!(job.windows));
    for k in [
        "wave64",
        "split",
        "upper_lanes",
        "backend",
        "form",
        "unstructured",
        "register_arrays",
        "instructions",
        "blocks",
        "loops",
        "max_loop_depth",
        "divergent_branches",
        "movrel_sites",
        "exchanges",
        "dxil",
    ] {
        m.insert(k.into(), Value::Null);
    }
    m.insert("split_reason".into(), json!(split_reason()));
    let chain = job.chain_dwords.map_or(0u64, |d| d as u64 * 4);
    m.insert("scratch_hint".into(), json!({ "private_bytes_per_thread": null, "chain_bytes_per_group": chain, "group_threads": null, "bytes_per_group": null }));
    m.insert("dxil_bytes".into(), json!(job.dxil.len() as u64));
    m.insert("dispatch_chain_pieces".into(), json!(job.chain_pieces));
    m.insert("compile_ms".into(), json!(job.compile_ms));
    m.insert("shape_status".into(), json!("ok"));
    m.insert("key".into(), json!(job.key_text()));
    m
}

/// The record of a program (the worker's body): the backend that made its DXIL, the console
/// shape, the HLSL shape, the DXIL's own statistics. A panic is a `failed: panic` record.
fn shape_job(job: Job) -> Value {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| shape_analyse(&job))) {
        Ok(m) => Value::Object(m),
        Err(_) => {
            let mut m = shape_skeleton(&job);
            m.insert("shape_status".into(), json!("failed: panic"));
            Value::Object(m)
        }
    }
}

fn shape_analyse(job: &Job) -> Map<String, Value> {
    let mut m = shape_skeleton(job);
    let prog = match (job.kind, &job.vs) {
        (Kind::Gs, Some(vs)) => shape_geometry_program(vs, &job.blob),
        (Kind::Gs, None) => Err("a geometry program without its VS".to_string()),
        (kind, _) => shape_console_program(&job.blob, kind.stage()),
    };
    let ident = dxil_backend(&job.dxil);
    let stats = crate::gpu::dxil::stats::container_stats(&job.dxil).ok();
    if let Some(s) = &stats {
        m.insert(
            "dxil".into(),
            json!({
                "bitcode_bytes": s.bitcode_bytes,
                "blocks": s.blocks,
                "instructions": s.instructions,
                "phis": s.phis,
                "backward_branches": s.backward_branches,
                "allocas": s.allocas,
                "switches": s.switches,
            }),
        );
    }
    let prog = match prog {
        Ok(p) => p,
        Err(e) => {
            m.insert("backend".into(), json!(pick_backend(job.kind, None, ident).map(Backend::name)));
            m.insert("shape_status".into(), json!(format!("failed: {}", first_line(&e))));
            return m;
        }
    };
    let split = !job.kind.graphics() && prog.wave64 && job.split_arg;
    m.insert("wave64".into(), json!(prog.wave64));
    m.insert("split".into(), json!(split));
    let mut notes: Vec<String> = Vec::new();
    match shape_program(&prog.code, prog.gs_start) {
        Ok(p) => {
            m.insert("instructions".into(), json!(p.instructions));
            m.insert("blocks".into(), json!(p.blocks));
            m.insert("loops".into(), json!(p.loops));
            m.insert("max_loop_depth".into(), json!(p.max_loop_depth));
            m.insert("divergent_branches".into(), json!(p.divergent_branches));
            m.insert("movrel_sites".into(), json!(p.movrel_sites));
        }
        Err(e) => notes.push(format!("cfg: {}", first_line(&e))),
    }
    // the rebuilt HLSL describes a program that ran only on the HLSL backend: a program the
    // direct translator made is not rebuilt
    let hs = if shape_routed_direct(job.kind) && ident == Some(Backend::Direct) { None } else { Some(shape_hlsl(&prog, job.kind.graphics(), job.windows, job.split_arg)) };
    let hlsl_bytes = match &hs {
        Some(Ok(h)) => Some(h.hlsl_bytes),
        _ => None,
    };
    let backend = pick_backend(job.kind, hlsl_bytes, ident);
    m.insert("backend".into(), json!(backend.map(Backend::name)));
    let mut private: Option<u64> = None;
    match (backend, hs) {
        (Some(Backend::Hlsl), Some(Ok(h))) => {
            m.insert("form".into(), json!(h.form));
            m.insert("unstructured".into(), json!(h.unstructured));
            m.insert("register_arrays".into(), json!(h.register_arrays));
            m.insert("upper_lanes".into(), json!(h.upper_lanes));
            private = Some(h.private_bytes_per_thread);
            if let Some(x) = &h.exchanges {
                let mut v = serde_json::to_value(x).unwrap_or(Value::Null);
                if let Value::Object(o) = &mut v {
                    o.insert("paid".into(), json!(split));
                }
                m.insert("exchanges".into(), v);
            }
        }
        (Some(Backend::Hlsl), Some(Err(e))) => notes.push(format!("hlsl: {}", first_line(&e))),
        (Some(Backend::Hlsl), None) => {}
        (Some(Backend::Direct), _) => {
            m.insert("form".into(), json!(Form::Direct));
            // the allocas are not sized in the statistics: only "none" is known
            if stats.as_ref().is_some_and(|s| s.allocas == 0) {
                m.insert("register_arrays".into(), json!(Vec::<RegisterArray>::new()));
                private = Some(0);
            }
        }
        (None, _) => notes.push("backend unreadable".into()),
    }
    let chain = job.chain_dwords.map_or(0u64, |d| d as u64 * 4);
    let group = prog.group_threads;
    let total = match (private, group) {
        (Some(p), Some(g)) => Some(p * g as u64 + chain),
        _ => None,
    };
    m.insert("scratch_hint".into(), json!({ "private_bytes_per_thread": private, "chain_bytes_per_group": chain, "group_threads": group, "bytes_per_group": total }));
    if !notes.is_empty() {
        m.insert("shape_status".into(), json!(format!("partial: {}", notes.join("; "))));
    }
    m
}

/// `shaders.jsonl` read back: the records, merged by key.
struct MergedShapes {
    records: Vec<Value>,
    analysed: u64,
    dropped: u64,
    failed: u64,
    pending: u64,
    /// records left out of `records` by `SHAPE_RECORDS_CAP`
    over_cap: u64,
}

/// A record from a `seen` line alone (the program was queued, dropped, or the run ended first).
fn pending_record(seen: &Map<String, Value>) -> Map<String, Value> {
    let mut m = Map::new();
    for k in ["record", "vs", "stage", "windows", "split_reason", "dxil_bytes", "compile_ms"] {
        if let Some(v) = seen.get(k) {
            m.insert(k.into(), v.clone());
        }
    }
    for k in [
        "wave64",
        "split",
        "upper_lanes",
        "backend",
        "form",
        "unstructured",
        "register_arrays",
        "instructions",
        "blocks",
        "loops",
        "max_loop_depth",
        "divergent_branches",
        "movrel_sites",
        "exchanges",
        "dxil",
        "dispatch_chain_pieces",
    ] {
        m.insert(k.into(), Value::Null);
    }
    m.insert("scratch_hint".into(), json!({ "private_bytes_per_thread": null, "chain_bytes_per_group": null, "group_threads": null, "bytes_per_group": null }));
    let dropped = seen.get("shape").and_then(Value::as_str) == Some("dropped");
    m.insert("shape_status".into(), json!(if dropped { "dropped" } else { "pending_at_exit" }));
    m
}

/// Merges the `seen` and `shape` lines of `shaders.jsonl` by their `key`, in the order the
/// programs were first seen. Lines that are not JSON (a torn last line) are skipped.
fn merge_shape_lines(text: &str) -> MergedShapes {
    let mut order: Vec<String> = Vec::new();
    let mut by_key: BTreeMap<String, (Option<Map<String, Value>>, Option<Map<String, Value>>)> = BTreeMap::new();
    for line in text.lines() {
        let Ok(Value::Object(o)) = serde_json::from_str::<Value>(line) else { continue };
        let Some(key) = o.get("key").and_then(Value::as_str).map(str::to_string) else { continue };
        let ev = o.get("ev").and_then(Value::as_str).unwrap_or("").to_string();
        if !by_key.contains_key(&key) {
            order.push(key.clone());
        }
        let slot = by_key.entry(key).or_default();
        match ev.as_str() {
            "seen" => slot.0 = Some(o),
            "shape" => slot.1 = Some(o),
            _ => {}
        }
    }
    let mut out = MergedShapes { records: Vec::new(), analysed: 0, dropped: 0, failed: 0, pending: 0, over_cap: 0 };
    for key in order {
        let Some((seen, shape)) = by_key.remove(&key) else { continue };
        let rec = match (shape, seen) {
            (Some(mut s), _) => {
                for k in ["ev", "t", "key"] {
                    s.remove(k);
                }
                s
            }
            (None, Some(s)) => pending_record(&s),
            (None, None) => continue,
        };
        match rec.get("shape_status").and_then(Value::as_str).unwrap_or("") {
            "dropped" => out.dropped += 1,
            "pending_at_exit" => out.pending += 1,
            s if s.starts_with("failed") => out.failed += 1,
            _ => out.analysed += 1,
        }
        if out.records.len() < SHAPE_RECORDS_CAP {
            out.records.push(Value::Object(rec));
        } else {
            out.over_cap += 1;
        }
    }
    out
}

/// The top level of `shaders.json` without `counts` and `records`.
fn shaders_header(run_id: &str, range: Option<(u32, u32)>) -> Map<String, Value> {
    use crate::gpu::recomp::pso;
    let forced = pso::split_wave64_forced();
    let compute_split = if forced { Some(true) } else { range.map(|(min, max)| pso::wave32_only(min, max)) };
    let graphics_eligible = if forced { Some(true) } else { range.map(|(min, max)| pso::graphics_wave32(min, max)) };
    match json!({
        "schema": 1,
        "run_id": run_id,
        "translator_source_id": env!("XCL_TRANSLATOR_SOURCE_ID"),
        "wave_lane_count_min": range.map(|r| r.0),
        "wave_lane_count_max": range.map(|r| r.1),
        "split_wave64_forced": forced,
        "compute_split": compute_split,
        "graphics_upper_lanes_eligible": graphics_eligible,
        "split_reason": split_reason_text(range, forced),
        "direct_dxil": std::env::var("XCL_DIRECT_DXIL").ok(),
    }) {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

/// From `telemetry_final`: `shaders.json` of the run (nothing when no program was seen).
fn shaders_final() {
    let Some(log) = SHAPE_LOG.get().and_then(|l| l.as_ref()) else { return };
    let (run_id, range) = match lock_within(&MANIFEST, Duration::from_millis(200)) {
        Some(g) => match g.as_ref() {
            Some(m) => (m.run_id.clone(), m.wave_lane_count_min.zip(m.wave_lane_count_max)),
            None => (String::new(), None),
        },
        None => (String::new(), None),
    };
    if let Err(e) = log.write_shaders_json(shaders_header(&run_id, range)) {
        crate::xlog_warn!("telemetry: shaders.json not written in {}: {e}", log.dir.display());
    }
}

/// A VS / PS / CS the call returned: the first time its key comes, it is queued for analysis.
/// `start` is None unless the tier is `pass`; refusals (`r` an error) are not records.
pub fn telemetry_shader(blob: &[u8], stage: Stage, split_wave64: bool, windows: u32, start: Option<Instant>, r: Result<Compiled, String>) -> Result<Compiled, String> {
    let Some(start) = start else { return r };
    let Ok(c) = &r else { return r };
    let kind = match stage {
        Stage::Vertex => Kind::Vs,
        Stage::Pixel => Kind::Ps,
        Stage::Compute => Kind::Cs,
        Stage::Mesh => Kind::Ms,
    };
    if let Some(log) = shape_log() {
        log.admit(Job::new(kind, blob, None, split_wave64, windows, &c.dxil, c.dispatch_chain.as_deref(), start.elapsed().as_secs_f64() * 1000.0));
    }
    r
}

/// A mesh program (its compute pass is the record's DXIL).
pub fn telemetry_mesh(blob: &[u8], split_wave64: bool, windows: u32, start: Option<Instant>, r: Result<TwoPass, String>) -> Result<TwoPass, String> {
    let Some(start) = start else { return r };
    let Ok(t) = &r else { return r };
    if let Some(log) = shape_log() {
        log.admit(Job::new(Kind::Ms, blob, None, split_wave64, windows, &t.cs, None, start.elapsed().as_secs_f64() * 1000.0));
    }
    r
}

/// A geometry (ES + GS) program (its compute pass is the record's DXIL).
pub fn telemetry_geometry(vs: &[u8], gs: &[u8], split_wave64: bool, start: Option<Instant>, r: Result<TwoPass, String>) -> Result<TwoPass, String> {
    let Some(start) = start else { return r };
    let Ok(t) = &r else { return r };
    if let Some(log) = shape_log() {
        log.admit(Job::new(Kind::Gs, gs, Some(vs), split_wave64, crate::gpu::recomp::pso::mem_windows(), &t.cs, None, start.elapsed().as_secs_f64() * 1000.0));
    }
    r
}

#[cfg(test)]
mod shape_glue_tests {
    use super::*;
    use crate::gpu::rdna2::encode;

    fn shape_tmp(name: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!("xcl-report-shapes-{name}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// T1's program: two `ds_bpermute_b32`, a `v_readlane_b32`, `s_endpgm`.
    fn t1_code() -> Vec<u8> {
        let one = |n: &str, e: &str, f: &[(&str, u64)]| encode(n, e, f, None).unwrap();
        let mut code = one("DS_BPERMUTE_B32", "ENC_DS", &[("VDST", 3), ("ADDR", 1), ("DATA0", 2)]);
        code.extend(one("DS_BPERMUTE_B32", "ENC_DS", &[("VDST", 4), ("ADDR", 1), ("DATA0", 2)]));
        code.extend(one("V_READLANE_B32", "ENC_VOP3", &[("VDST", 4), ("SRC0", 256 + 1), ("SRC1", 128 + 5)]));
        code.extend(one("S_ENDPGM", "ENC_SOPP", &[("SIMM16", 0)]));
        code
    }

    /// An FSPX driver container (an `ACCC` part) holding `code`, 64 x 1 x 1 threads
    /// (`recomp::tests::fspx_compute_user_sgprs_from_rsrc2`).
    fn fspx_container(code: &[u8]) -> Vec<u8> {
        let total = 0x70 + code.len();
        let mut part = Vec::from(*b"FSPX");
        for word in [1u32, 2, total as u32, 0, 0x30, 0x1c, 0, 6, 0x4c, (0x24 + code.len()) as u32, 0] {
            part.extend_from_slice(&word.to_le_bytes());
        }
        part.resize(total, 0);
        part[0x3c..0x42].copy_from_slice(&[64, 0, 1, 0, 1, 0]);
        part[0x70..].copy_from_slice(code);
        crate::gpu::xbox::tests::container(&[(b"ACCC", &part)])
    }

    /// DXIL of the direct translator, made on the CPU.
    fn direct_dxil() -> Vec<u8> {
        let mut code = encode("V_MOV_B32", "ENC_VOP1", &[("VDST", 1), ("SRC0", 256)], None).unwrap();
        code.extend(encode("S_ENDPGM", "ENC_SOPP", &[("SIMM16", 0)], None).unwrap());
        crate::gpu::dxil::kernel::translate(&code, crate::gpu::dxil::kernel::KernelOptions { wave64: true, split: false, mem_windows: 1, sampler_base: 0 }).expect("direct translate")
    }

    fn cs_job(blob: &[u8], windows: u32, dxil: &[u8], compile_ms: f64) -> Job {
        Job::new(Kind::Cs, blob, None, true, windows, dxil, None, compile_ms)
    }

    fn json_lines(dir: &Path) -> Vec<Value> {
        std::fs::read_to_string(dir.join("shaders.jsonl")).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    #[test]
    fn g1_a_record_does_not_depend_on_who_translated_the_program() {
        let blob = fspx_container(&t1_code());
        let dxil = direct_dxil();
        // a fresh process state: nothing translated this blob yet
        let first = shape_job(cs_job(&blob, 1, &dxil, 1.0));
        // the translator ran on it (a cache miss), then the same job again (a hit)
        assert!(crate::gpu::recomp::pso::console_translation(&blob, Stage::Compute, 0, 1, true).is_ok());
        let second = shape_job(cs_job(&blob, 1, &dxil, 2.0));
        let (mut a, mut b) = (first, second);
        for v in [&mut a, &mut b] {
            v.as_object_mut().unwrap().remove("compile_ms");
        }
        assert_eq!(a, b);
        assert_eq!(a["record"], format!("{:016x}", crate::gpu::shader::fnv64(&blob)));
        assert_eq!((a["stage"].as_str(), a["wave64"].as_bool(), a["split"].as_bool(), a["windows"].as_u64()), (Some("cs"), Some(true), Some(true), Some(1)));
        assert_eq!(a["dxil_bytes"].as_u64(), Some(dxil.len() as u64));
        assert_eq!(a["scratch_hint"]["group_threads"], 64);
        // `XCL_DIRECT_DXIL` in the environment of the test run routes the stage to the direct backend
        if a["backend"] == "hlsl" {
            assert_eq!(a["shape_status"], "ok", "{a}");
            assert_eq!((a["exchanges"]["total"].as_u64(), a["exchanges"]["paid"].as_bool()), (Some(4), Some(true)), "{a}");
            assert_eq!(a["register_arrays"], json!([]));
        }
    }

    #[test]
    fn g2_one_program_is_one_record_per_key() {
        let dir = shape_tmp("dedupe");
        let log = ShapeLog::create(&dir, SHAPE_QUEUE_BYTES, true).unwrap();
        let blob = fspx_container(&t1_code());
        log.admit(cs_job(&blob, 1, &[], 1.0));
        log.admit(cs_job(&blob, 1, &[], 2.0));
        assert!(log.wait_idle(Duration::from_secs(120)));
        let count = |lines: &[Value], ev: &str| lines.iter().filter(|l| l["ev"] == ev).count();
        let lines = json_lines(&dir);
        assert_eq!((count(&lines, "seen"), count(&lines, "shape")), (1, 1), "{lines:?}");
        log.admit(cs_job(&blob, 2, &[], 3.0));
        assert!(log.wait_idle(Duration::from_secs(120)));
        let lines = json_lines(&dir);
        assert_eq!((count(&lines, "seen"), count(&lines, "shape")), (2, 2), "{lines:?}");
        let merged = merge_shape_lines(&std::fs::read_to_string(dir.join("shaders.jsonl")).unwrap());
        let mut windows: Vec<u64> = merged.records.iter().filter_map(|r| r["windows"].as_u64()).collect();
        windows.sort_unstable();
        assert_eq!(windows, [1, 2]);
        drop(log);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn g3_off_returns_the_result_untouched_and_opens_nothing() {
        let compiled = || Ok(Compiled { dispatch_chain: None, dxil: vec![1, 2, 3], samplers: vec![] });
        let r = telemetry_shader(&[9, 9], Stage::Compute, false, 1, None, compiled());
        assert_eq!(r.unwrap().dxil, [1, 2, 3]);
        let e = telemetry_shader(&[9, 9], Stage::Compute, false, 1, Some(Instant::now()), Err("refused".into()));
        assert_eq!(e.err().as_deref(), Some("refused"));
        let two = TwoPass { cs: vec![7], ms: vec![8], record: Default::default(), mesh_groups_per_record: 1, samplers: vec![] };
        assert_eq!(telemetry_mesh(&[1], false, 1, None, Ok(two)).unwrap().cs, [7]);
        assert_eq!(telemetry_geometry(&[1], &[2], false, None, Err("x".into())).err().as_deref(), Some("x"));
        // telemetry is not open in a unit test: even with a start time no sink is made
        assert!(telemetry_shader(&[9, 9], Stage::Compute, false, 1, Some(Instant::now()), compiled()).is_ok());
        assert!(SHAPE_LOG.get().is_none());
    }

    #[test]
    fn g4_a_full_queue_drops_the_job_and_says_so() {
        let dir = shape_tmp("bounds");
        let log = ShapeLog::create(&dir, 1, true).unwrap();
        let blob = fspx_container(&t1_code());
        log.admit(cs_job(&blob, 1, &[], 1.0));
        let lines = json_lines(&dir);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!((lines[0]["ev"].as_str(), lines[0]["shape"].as_str()), (Some("seen"), Some("dropped")));
        assert_eq!(log.state.counts.dropped.load(Ordering::Relaxed), 1);
        assert_eq!(log.state.outstanding.load(Ordering::Relaxed), 0);
        let m = merge_shape_lines(&std::fs::read_to_string(dir.join("shaders.jsonl")).unwrap());
        assert_eq!((m.dropped, m.records[0]["shape_status"].as_str()), (1, Some("dropped")));
        drop(log);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn g5_the_final_file_merges_seen_and_shape_lines() {
        let dir = shape_tmp("final");
        let log = ShapeLog::create(&dir, SHAPE_QUEUE_BYTES, false).unwrap();
        log.state.lines.write("seen", &[("key", "aa".into()), ("record", "00000000000000aa".into()), ("stage", "cs".into()), ("windows", 1.into()), ("split", true.into()), ("wave64", Value::Null)]);
        log.state.lines.write("seen", &[("key", "bb".into()), ("record", "00000000000000bb".into()), ("stage", "ps".into()), ("windows", 1.into()), ("split", false.into()), ("wave64", Value::Null)]);
        log.state.lines.write("shape", &[("key", "bb".into()), ("record", "00000000000000bb".into()), ("stage", "ps".into()), ("windows", 1.into()), ("wave64", true.into()), ("shape_status", "ok".into())]);
        // a torn last line is skipped
        {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new().append(true).open(dir.join("shaders.jsonl")).unwrap();
            f.write_all(b"{\"ev\":\"seen\",\"key\":\"cc\"").unwrap();
        }
        log.write_shaders_json(shaders_header("run-x", Some((32, 64)))).unwrap();
        // again: the same file
        log.write_shaders_json(shaders_header("run-x", Some((32, 64)))).unwrap();
        let text = std::fs::read_to_string(dir.join("shaders.json")).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!((v["schema"].as_u64(), v["run_id"].as_str(), v["split_reason"].as_str()), (Some(1), Some("run-x"), Some(if crate::gpu::recomp::pso::split_wave64_forced() { "XCL_SPLIT_WAVE64=1" } else { "device waves 32-64" })));
        assert_eq!((v["wave_lane_count_min"].as_u64(), v["wave_lane_count_max"].as_u64()), (Some(32), Some(64)));
        let records = v["records"].as_array().unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!((records[0]["record"].as_str(), records[0]["shape_status"].as_str()), (Some("00000000000000aa"), Some("pending_at_exit")));
        assert_eq!((records[1]["record"].as_str(), records[1]["shape_status"].as_str()), (Some("00000000000000bb"), Some("ok")));
        assert!(records.iter().all(|r| r.get("key").is_none() && r.get("ev").is_none() && r.get("t").is_none()));
        assert_eq!((v["counts"]["records"].as_u64(), v["counts"]["pending_at_exit"].as_u64(), v["counts"]["analysed"].as_u64()), (Some(2), Some(1), Some(1)));
        assert!(!dir.join("shaders.json.tmp").exists());
        drop(log);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn g6_the_backend_is_read_from_the_dxil() {
        assert_eq!(dxil_backend(&direct_dxil()), Some(Backend::Direct));
        assert_eq!(dxil_backend(&[1, 2, 3]), None);
        if crate::gpu::recomp::dxc::create_instance().is_none() {
            crate::gpu::recomp::runner::skip_test("no DXC");
            return;
        }
        let dxc = crate::gpu::recomp::dxc::compile("[numthreads(1, 1, 1)] void main() {}", "main", "cs_6_6", &[]).expect("DXC compile");
        assert_eq!(dxil_backend(&dxc), Some(Backend::Hlsl));
    }

    #[test]
    fn the_backend_follows_the_routing_and_the_size_of_the_hlsl() {
        let (d, h) = (Some(Backend::Direct), Some(Backend::Hlsl));
        if !crate::gpu::dxil::kernel::enabled() && !crate::gpu::dxil::kernel::enabled_graphics() {
            assert_eq!(pick_backend(Kind::Cs, Some(10), d), h);
            assert_eq!(pick_backend(Kind::Cs, Some(crate::gpu::recomp::dxc::UNOPTIMISED_ABOVE as u64 + 1), d), d);
            assert_eq!(pick_backend(Kind::Ps, Some(crate::gpu::recomp::dxc::UNOPTIMISED_ABOVE as u64 + 1), None), h);
            assert_eq!(pick_backend(Kind::Cs, None, d), d);
            assert_eq!(pick_backend(Kind::Cs, None, None), None);
        }
    }
}
