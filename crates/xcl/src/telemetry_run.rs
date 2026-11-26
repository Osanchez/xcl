//! Fork-only telemetry code (docs/guide/telemetry.md, part 5). Included from report.rs's fork block
//! by `#[path = "telemetry_run.rs"] mod run_tel; pub use run_tel::*;`, so its `pub` items are
//! reachable as `crate::report::<name>`. Upstream files call it only through one-line calls.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

// ---- part 5a: no-window switches ----

/// 0 window (default), 1 headless, 2 skip; 0xFF = not read yet. Read once from the environment.
static PRESENT_MODE: AtomicU8 = AtomicU8::new(0xFF);
/// The mode's log lines are written once, by the first `present_bypass`.
static PRESENT_LOGGED: AtomicBool = AtomicBool::new(false);

/// `XCL_HEADLESS=1` / `XCL_PRESENT_SKIP=1` (both set: headless wins, one warning).
/// `after_window == false` asks "headless?", `true` asks "skip?". One relaxed load once read.
pub fn present_bypass(after_window: bool) -> bool {
    let mode = present_mode_value();
    if !PRESENT_LOGGED.load(Ordering::Relaxed) && !PRESENT_LOGGED.swap(true, Ordering::Relaxed) {
        let headless = std::env::var("XCL_HEADLESS").ok();
        let skip = std::env::var("XCL_PRESENT_SKIP").ok();
        if let (_, Some(w)) = parse_present_mode(headless.as_deref(), skip.as_deref()) {
            crate::xlog_warn!("{w}");
        }
        match mode {
            1 => crate::xlog_info!("present: XCL_HEADLESS – no window, frames are counted, not drawn"),
            2 => crate::xlog_info!("present: XCL_PRESENT_SKIP – the window is kept, frames are counted, not drawn"),
            _ => {}
        }
        if mode != 0 && std::env::var_os("XCL_FRAME_DUMP").is_some() {
            crate::xlog_info!("present: XCL_FRAME_DUMP is ignored with XCL_HEADLESS / XCL_PRESENT_SKIP");
        }
    }
    mode == if after_window { 2 } else { 1 }
}

/// The present mode word; the first call reads the environment (the manifest may ask before the
/// first frame). A racing second thread reads the same values and stores the same word.
fn present_mode_value() -> u8 {
    let mode = PRESENT_MODE.load(Ordering::Relaxed);
    if mode != 0xFF {
        return mode;
    }
    let headless = std::env::var("XCL_HEADLESS").ok();
    let skip = std::env::var("XCL_PRESENT_SKIP").ok();
    let (m, _) = parse_present_mode(headless.as_deref(), skip.as_deref());
    PRESENT_MODE.store(m, Ordering::Relaxed);
    m
}

/// pure: (headless value, skip value) -> mode; "1" means on, anything else off
fn parse_present_mode(headless: Option<&str>, skip: Option<&str>) -> (u8, Option<String>) {
    let on = |v: Option<&str>| v == Some("1");
    match (on(headless), on(skip)) {
        (true, true) => (1, Some("XCL_HEADLESS and XCL_PRESENT_SKIP are both set: XCL_HEADLESS wins".to_string())),
        (true, false) => (1, None),
        (false, true) => (2, None),
        (false, false) => (0, None),
    }
}

/// "window" | "headless" | "skip", for the manifest (5b)
pub fn present_mode_name() -> &'static str {
    match present_mode_value() {
        1 => "headless",
        2 => "skip",
        _ => "window",
    }
}

#[cfg(test)]
mod telemetry_p5a_tests {
    use super::parse_present_mode;

    #[test]
    fn present_mode_truth_table() {
        assert_eq!(parse_present_mode(None, None), (0, None));
        assert_eq!(parse_present_mode(Some("1"), None), (1, None));
        assert_eq!(parse_present_mode(None, Some("1")), (2, None));
        assert_eq!(parse_present_mode(Some("0"), Some("0")), (0, None));
        assert_eq!(parse_present_mode(Some(""), Some("yes")), (0, None));
        assert_eq!(parse_present_mode(Some("true"), None), (0, None));
        assert_eq!(parse_present_mode(Some("garbage"), Some("1")), (2, None));
        let (m, warning) = parse_present_mode(Some("1"), Some("1"));
        assert_eq!(m, 1);
        assert!(warning.is_some());
    }
}

// ---- part 5b: error, end, exit and activity events ----
//
// Crash-path rules: every function below starts with the tier check (one relaxed load), takes no
// lock but the event writer's bounded one, never calls `update_manifest` and does no disk I/O
// other than the event line. `unhandled_filter` and `hk_nt_terminate_process` call them.

/// Set by the first fatal `error`: the run was ended by a failure.
static FATAL: AtomicBool = AtomicBool::new(false);
/// Set by the `end` event, which is written once.
static END: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ErrKind {
    DeviceRemoved,
    GpuFault,
    PipelineRejected,
    ShaderRefused,
    UnresolvedImport,
    /// reserved: no producer exists (a stub always returns its configured value)
    StubCalledFatal,
    VfsUnmapped,
    IoFailed,
    TitleException,
    /// reserved for the loop counters (part 5e)
    LoopGuardTripped,
    Timeout,
    LoadFailed,
}

impl ErrKind {
    pub fn name(self) -> &'static str {
        match self {
            ErrKind::DeviceRemoved => "device_removed",
            ErrKind::GpuFault => "gpu_fault",
            ErrKind::PipelineRejected => "pipeline_rejected",
            ErrKind::ShaderRefused => "shader_refused",
            ErrKind::UnresolvedImport => "unresolved_import",
            ErrKind::StubCalledFatal => "stub_called_fatal",
            ErrKind::VfsUnmapped => "vfs_unmapped",
            ErrKind::IoFailed => "io_failed",
            ErrKind::TitleException => "title_exception",
            ErrKind::LoopGuardTripped => "loop_guard_tripped",
            ErrKind::Timeout => "timeout",
            ErrKind::LoadFailed => "load_failed",
        }
    }

    /// The `end` reason of a kind that can end the run; `None` for the kinds that never do.
    fn end_reason(self) -> Option<&'static str> {
        match self {
            ErrKind::DeviceRemoved => Some("device_removed"),
            ErrKind::TitleException => Some("title_exception"),
            ErrKind::Timeout => Some("stall"),
            ErrKind::LoadFailed => Some("load_failed"),
            _ => None,
        }
    }
}

/// pure: the field list of one error event (the fatal-latch result is already decided)
fn error_fields(
    kind: ErrKind,
    source: &str,
    detail: &str,
    hresult: Option<i32>,
    fatal: bool,
    after_fatal: bool,
    extra: &[(&'static str, serde_json::Value)],
) -> Vec<(&'static str, serde_json::Value)> {
    let mut f: Vec<(&'static str, serde_json::Value)> = vec![("kind", kind.name().into()), ("source", source.into()), ("detail", detail.into())];
    if let Some(hr) = hresult {
        f.push(("hresult", super::hresult_text(hr).into()));
    }
    f.push(("fatal", fatal.into()));
    if after_fatal {
        f.push(("after_fatal", true.into()));
    }
    f.extend(extra.iter().cloned());
    f
}

/// pure: `(fatal, after_fatal)` of a candidate. The first wanted-fatal candidate takes the latch;
/// later ones are written as not fatal, with `after_fatal`. A candidate that is not fatal is neither.
fn fatal_decision(latch: &AtomicBool, wanted: bool) -> (bool, bool) {
    if !wanted {
        return (false, false);
    }
    if latch.swap(true, Ordering::AcqRel) {
        (false, true)
    } else {
        (true, false)
    }
}

/// Writes an `error` event (tier frame). Only the kinds that have an `end` reason can be fatal;
/// the first fatal one writes `fatal: true` and then the `end` event naming its kind.
pub fn telemetry_error(kind: ErrKind, source: &str, detail: &str, hresult: Option<i32>, fatal: bool, extra: &[(&'static str, serde_json::Value)]) {
    if !crate::log::telemetry_on(crate::log::Tier::Frame) {
        return;
    }
    let reason = kind.end_reason();
    let (is_fatal, after_fatal) = fatal_decision(&FATAL, fatal && reason.is_some());
    let fields = error_fields(kind, source, detail, hresult, is_fatal, after_fatal, extra);
    crate::log::event(crate::log::Tier::Frame, "error", &fields);
    if let (true, Some(reason)) = (is_fatal, reason) {
        write_end(reason, None, Some(kind.name()));
    }
}

/// `log_device_removed`: the device was removed (fatal, once).
pub fn telemetry_device_removed(hr: i32, message: &str) {
    if !crate::log::telemetry_on(crate::log::Tier::Frame) {
        return;
    }
    telemetry_error(ErrKind::DeviceRemoved, "d3d12", "device removed", Some(hr), true, &[("message", message.into())]);
}

/// DRED's page fault after a device removal (`va` is written as hex text).
pub fn telemetry_gpu_fault(va: u64, window: &str, allocations: &str, freed: &str) {
    if !crate::log::telemetry_on(crate::log::Tier::Frame) {
        return;
    }
    let extra: [(&'static str, serde_json::Value); 5] = [
        ("what", "page_fault".into()),
        ("va", format!("{va:#x}").into()),
        ("window", window.into()),
        ("allocations", allocations.into()),
        ("freed", freed.into()),
    ];
    telemetry_error(ErrKind::GpuFault, "dred", "page fault", None, false, &extra);
}

/// DRED's line of one command list (`list` is the pointer, written as hex text).
pub fn telemetry_gpu_fault_list(list: usize, done: usize, count: usize, bound: &str) {
    if !crate::log::telemetry_on(crate::log::Tier::Frame) {
        return;
    }
    let extra: [(&'static str, serde_json::Value); 5] = [
        ("what", "breadcrumb".into()),
        ("list", format!("{list:#x}").into()),
        ("done", done.into()),
        ("count", count.into()),
        ("bound", bound.into()),
    ];
    telemetry_error(ErrKind::GpuFault, "dred", "command list breadcrumbs", None, false, &extra);
}

/// `unhandled_filter`: the title raised an exception nobody handled (fatal unless one came first).
pub fn telemetry_title_exception(code: u32, at: usize, message: &str) {
    if !crate::log::telemetry_on(crate::log::Tier::Frame) {
        return;
    }
    let extra: [(&'static str, serde_json::Value); 3] =
        [("code", format!("0x{code:08X}").into()), ("at", crate::modules::describe_address(at).into()), ("message", message.into())];
    telemetry_error(ErrKind::TitleException, "eh", "unhandled exception", None, true, &extra);
}

/// `resolve_import`: called for every import, so it returns on the tier check and then on the source.
pub fn telemetry_unresolved_import(source: &crate::resolver::Source, dll: &str, function: &str, importer: &str, delay: bool) {
    if !crate::log::telemetry_on(crate::log::Tier::Frame) {
        return;
    }
    if *source != crate::resolver::Source::Stub {
        return;
    }
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let Some(n) = crate::log::sampled(&COUNT) else { return };
    let extra: [(&'static str, serde_json::Value); 5] =
        [("dll", dll.into()), ("function", function.into()), ("importer", importer.into()), ("delay", delay.into()), ("n", n.into())];
    telemetry_error(ErrKind::UnresolvedImport, "resolver", "import routed to a stub", None, false, &extra);
}

/// `[VFS] unmapped console path` (the caller already limits it to 1000 distinct paths).
pub fn telemetry_vfs_unmapped(path: &str) {
    if !crate::log::telemetry_on(crate::log::Tier::Frame) {
        return;
    }
    telemetry_error(ErrKind::VfsUnmapped, "vfs", "unmapped console path", None, false, &[("path", path.into())]);
}

/// `host_main`: the game could not be loaded (fatal).
pub fn telemetry_load_failed(message: &str) {
    if !crate::log::telemetry_on(crate::log::Tier::Frame) {
        return;
    }
    telemetry_error(ErrKind::LoadFailed, "loader", message, None, true, &[]);
}

/// The one `end` event: `reason`, `code`, the frame counters and, for a failure, `fatal_kind`.
/// Written once per run; later calls do nothing.
fn write_end(reason: &str, code: Option<i64>, fatal_kind: Option<&str>) {
    if END.swap(true, Ordering::AcqRel) {
        return;
    }
    let (shown, presented) = crate::hle::d3d12x::present::counters();
    let mut f: Vec<(&'static str, serde_json::Value)> = vec![("reason", reason.into())];
    if let Some(c) = code {
        f.push(("code", c.into()));
    }
    f.push(("frames_shown", shown.into()));
    f.push(("presentx", presented.into()));
    if let Some(k) = fatal_kind {
        f.push(("fatal_kind", k.into()));
    }
    crate::log::event(crate::log::Tier::Frame, "end", &f);
}

/// The run ends for a reason that is not a failure (`run_seconds`, `entry_returned`, `window_closed`,
/// `process_exit`); the failures write theirs through `telemetry_error`.
pub fn telemetry_end(reason: &str, code: Option<i64>) {
    if !crate::log::telemetry_on(crate::log::Tier::Frame) {
        return;
    }
    write_end(reason, code, None);
}

/// `hk_nt_terminate_process`, inside its once-latch: `end{process_exit}` when no `end` exists yet
/// (the title called `ExitProcess` itself), then `exit{status}`.
pub fn telemetry_exit(status: i32) {
    if !crate::log::telemetry_on(crate::log::Tier::Frame) {
        return;
    }
    write_end("process_exit", Some(status as i64), None);
    crate::log::event(crate::log::Tier::Frame, "exit", &[("status", status.into())]);
}

/// One per `XCL_ACTIVITY` iteration (the 2 s lines): presents in the window and committed MB.
pub fn telemetry_activity(presents_2s: u64, committed_mb: u64) {
    if !crate::log::telemetry_on(crate::log::Tier::Frame) {
        return;
    }
    crate::log::event(crate::log::Tier::Frame, "activity", &[("presents_2s", presents_2s.into()), ("committed_mb", committed_mb.into())]);
}

/// pure: the switches of `vars` (the manifest's `XCL_*` map) that change generated code or
/// presentation, as `NAME=value` sorted, and the compare key (the same joined by `,`; `""` for a
/// normal run). Names match case-insensitively; flags are on for anything but `""` / `0`
/// (`XCL_HEADLESS` and `XCL_PRESENT_SKIP`: exactly `1`, like `parse_present_mode`), valued
/// switches for any non-empty value.
pub(super) fn diagnostic_switches(vars: &BTreeMap<String, String>) -> (Vec<String>, String) {
    const FLAGS: [&str; 5] = ["XCL_LOOP_COUNTERS", "XCL_WAVE64_SPIN", "XCL_X64_ALL_MASKS", "XCL_RECOMP_NO_STRUCTURED", "XCL_DXIL_NO_OPT"];
    const EXACT_ONE: [&str; 2] = ["XCL_HEADLESS", "XCL_PRESENT_SKIP"];
    const VALUED: [&str; 5] = ["XCL_LOOP_BUDGET", "XCL_WINDOW_SIZE", "XCL_FRAME_INTERVAL_US", "XCL_PRESENT_SYNC", "XCL_SPLIT_SCRATCH_MB"];
    let mut list: Vec<String> = Vec::new();
    for (k, v) in vars {
        let upper = k.to_ascii_uppercase();
        let on = if EXACT_ONE.contains(&upper.as_str()) {
            v == "1"
        } else if FLAGS.contains(&upper.as_str()) {
            !v.is_empty() && v != "0"
        } else {
            VALUED.contains(&upper.as_str()) && !v.is_empty()
        };
        if on {
            list.push(format!("{upper}={v}"));
        }
    }
    list.sort();
    let key = list.join(",");
    (list, key)
}

#[cfg(test)]
mod telemetry_p5b_tests {
    use super::*;

    const ALL: [ErrKind; 12] = [
        ErrKind::DeviceRemoved,
        ErrKind::GpuFault,
        ErrKind::PipelineRejected,
        ErrKind::ShaderRefused,
        ErrKind::UnresolvedImport,
        ErrKind::StubCalledFatal,
        ErrKind::VfsUnmapped,
        ErrKind::IoFailed,
        ErrKind::TitleException,
        ErrKind::LoopGuardTripped,
        ErrKind::Timeout,
        ErrKind::LoadFailed,
    ];

    fn keys(f: &[(&'static str, serde_json::Value)]) -> Vec<&'static str> {
        f.iter().map(|(k, _)| *k).collect()
    }

    #[test]
    fn kind_names_are_unique_snake_case() {
        let mut seen = std::collections::BTreeSet::new();
        for k in ALL {
            let n = k.name();
            assert!(!n.is_empty() && n.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'), "{n}");
            assert!(!n.starts_with('_') && !n.ends_with('_') && !n.contains("__"), "{n}");
            assert!(seen.insert(n), "duplicate {n}");
        }
        assert_eq!(seen.len(), 12);
    }

    #[test]
    fn error_fields_for_every_kind() {
        let extra = [("path", serde_json::Value::from("x"))];
        for k in ALL {
            let f = error_fields(k, "src", "what happened", Some(0x8007000Eu32 as i32), false, false, &extra);
            assert_eq!(keys(&f), ["kind", "source", "detail", "hresult", "fatal", "path"], "{k:?}");
            assert_eq!(f[0].1, k.name());
            assert_eq!(f[3].1, "0x8007000E");
            assert_eq!(f[4].1, false);
            let f = error_fields(k, "src", "d", None, true, false, &[]);
            assert_eq!(keys(&f), ["kind", "source", "detail", "fatal"]);
            assert_eq!(f[3].1, true);
            let f = error_fields(k, "src", "d", None, false, true, &extra);
            assert_eq!(keys(&f), ["kind", "source", "detail", "fatal", "after_fatal", "path"]);
            assert_eq!(f[3].1, false);
            assert_eq!(f[4].1, true);
        }
    }

    #[test]
    fn hresult_is_eight_hex_digits() {
        let f = error_fields(ErrKind::DeviceRemoved, "d3d12", "d", Some(0x887A0006u32 as i32), true, false, &[]);
        assert_eq!(f[3].1, "0x887A0006");
        let f = error_fields(ErrKind::IoFailed, "dstorage", "d", Some(5), false, false, &[]);
        assert_eq!(f[3].1, "0x00000005");
    }

    #[test]
    fn only_four_kinds_end_the_run() {
        let enders: Vec<&str> = ALL.iter().filter_map(|k| k.end_reason()).collect();
        assert_eq!(enders, ["device_removed", "title_exception", "stall", "load_failed"]);
    }

    #[test]
    fn fatal_latch_takes_the_first_candidate_only() {
        let latch = AtomicBool::new(false);
        assert_eq!(fatal_decision(&latch, false), (false, false));
        assert_eq!(fatal_decision(&latch, true), (true, false));
        assert_eq!(fatal_decision(&latch, true), (false, true));
        assert_eq!(fatal_decision(&latch, false), (false, false));
    }

    fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn diagnostic_switches_normal_run_is_empty() {
        let (list, key) = diagnostic_switches(&vars(&[("XCL_TELEMETRY", "frame"), ("XCL_RUN_SECONDS", "60"), ("XCL_HEADLESS", "0"), ("XCL_DXIL_NO_OPT", "")]));
        assert!(list.is_empty());
        assert_eq!(key, "");
        assert_eq!(diagnostic_switches(&BTreeMap::new()), (Vec::new(), String::new()));
    }

    #[test]
    fn diagnostic_switches_sorted_and_order_independent() {
        let a = diagnostic_switches(&vars(&[("XCL_WINDOW_SIZE", "320x200"), ("XCL_HEADLESS", "1"), ("XCL_LOOP_BUDGET", "4"), ("XCL_TELEMETRY", "all")]));
        let b = diagnostic_switches(&vars(&[("XCL_TELEMETRY", "all"), ("xcl_loop_budget", "4"), ("XCL_HEADLESS", "1"), ("XCL_WINDOW_SIZE", "320x200")]));
        assert_eq!(a, b);
        assert_eq!(a.0, ["XCL_HEADLESS=1", "XCL_LOOP_BUDGET=4", "XCL_WINDOW_SIZE=320x200"]);
        assert_eq!(a.1, "XCL_HEADLESS=1,XCL_LOOP_BUDGET=4,XCL_WINDOW_SIZE=320x200");
    }
}

// ---- part 5c: phase events ----

/// Poll period of the thread; the heuristic classifies every `PHASE_WINDOW_MS`.
const PHASE_POLL_MS: u64 = 250;
const PHASE_WINDOW_MS: u64 = 1000;
/// DirectStorage rate (MiB/s) from which a window counts as loading.
const LOADING_MIB_S: f64 = 32.0;
/// Console draws per presented frame from which a window counts as `world` (a guess, no measurement).
const WORLD_DRAWS_PER_FRAME: f64 = 400.0;

/// The five names the heuristic and the docs use; custom names are `[a-z0-9_-]{1,32}`.
const KNOWN_PHASES: [&str; 5] = ["boot", "loading", "menu", "world", "cutscene"];

/// Starts the poll thread (control file, schedule, heuristic). Called once from `telemetry_start`;
/// nothing happens when telemetry is off.
pub(super) fn telemetry_poll_start(logs_dir: std::path::PathBuf) {
    if !crate::log::telemetry_on(crate::log::Tier::Frame) {
        return;
    }
    if let Err(e) = std::thread::Builder::new().name("xcl-telemetry-poll".to_string()).spawn(move || telemetry_poll_thread(logs_dir)) {
        crate::xlog_warn!("telemetry: cannot start the phase thread: {e}");
    }
}

/// One counter window of the heuristic. `frames` and `draws` are the deltas of the window,
/// `total_frames` is every frame shown so far (0 = the title never presented).
struct PhaseSample {
    frames: u64,
    draws: u64,
    io_bytes: u64,
    /// source voices alive; not used by the rules yet (5e and later readers may)
    #[allow(dead_code)]
    voices: u64,
    dt_s: f64,
    total_frames: u64,
}

fn io_mib_s(s: &PhaseSample) -> f64 {
    if s.dt_s > 0.0 {
        s.io_bytes as f64 / s.dt_s / (1024.0 * 1024.0)
    } else {
        0.0
    }
}

/// pure: the phase a window looks like and how sure that is. `prev` is the phase last written by
/// the heuristic (kept when the window says nothing); `last_io_rate` is the previous window's MiB/s.
fn classify_phase(prev: Option<&str>, s: &PhaseSample, last_io_rate: f64) -> (&'static str, f64) {
    if s.total_frames == 0 {
        return ("boot", 1.0);
    }
    let io = io_mib_s(s);
    if io >= LOADING_MIB_S && last_io_rate >= LOADING_MIB_S {
        return ("loading", 0.5 + (io / 512.0).min(0.4));
    }
    if s.frames > 0 {
        let per_frame = s.draws as f64 / s.frames as f64;
        return (if per_frame >= WORLD_DRAWS_PER_FRAME { "world" } else { "menu" }, 0.35);
    }
    // frames stand still and nothing is loading: no new information
    let kept = KNOWN_PHASES.iter().copied().find(|k| Some(*k) == prev).unwrap_or("boot");
    (kept, 0.0)
}

/// A candidate must win 2 consecutive windows before an automatic event is written.
struct PhaseHysteresis {
    shown: Option<&'static str>,
    candidate: Option<&'static str>,
    wins: u32,
}

impl PhaseHysteresis {
    fn new(shown: Option<&'static str>) -> Self {
        PhaseHysteresis { shown, candidate: None, wins: 0 }
    }

    /// Feeds one window's candidate; true when it must be written now (it is then the shown phase).
    fn step(&mut self, candidate: &'static str) -> bool {
        if self.shown == Some(candidate) {
            self.candidate = None;
            self.wins = 0;
            return false;
        }
        if self.candidate == Some(candidate) {
            self.wins += 1;
        } else {
            self.candidate = Some(candidate);
            self.wins = 1;
        }
        if self.wins >= 2 {
            self.shown = Some(candidate);
            self.candidate = None;
            self.wins = 0;
            return true;
        }
        false
    }
}

/// pure: the five known names or `[a-z0-9_-]{1,32}`.
fn valid_phase_name(s: &str) -> bool {
    KNOWN_PHASES.contains(&s) || (!s.is_empty() && s.len() <= 32 && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-'))
}

/// pure: "12.5:menu,40:world" | "world" -> [(0.0,"world")]; sorted by time (stable); invalid entries
/// are skipped and returned as messages.
fn parse_phase_schedule(v: &str) -> (Vec<(f64, String)>, Vec<String>) {
    let mut out: Vec<(f64, String)> = Vec::new();
    let mut bad: Vec<String> = Vec::new();
    for raw in v.split(',') {
        let entry = raw.trim();
        if entry.is_empty() {
            continue;
        }
        let (t, name) = match entry.split_once(':') {
            Some((t, name)) => match t.trim().parse::<f64>() {
                Ok(t) if t.is_finite() && t >= 0.0 => (t, name.trim()),
                _ => {
                    bad.push(format!("XCL_PHASE_MARK: bad time in '{entry}'"));
                    continue;
                }
            },
            None => (0.0, entry),
        };
        if !valid_phase_name(name) {
            bad.push(format!("XCL_PHASE_MARK: bad phase name in '{entry}' (known names or [a-z0-9_-]{{1,32}})"));
            continue;
        }
        out.push((t, name.to_string()));
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    (out, bad)
}

/// pure: the control file's content -> Some(name) | Some("auto") (empty or `auto`: the mark ends)
/// | None (ignored).
fn parse_phase_file(text: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() || t == "auto" {
        Some("auto".to_string())
    } else if valid_phase_name(t) {
        Some(t.to_string())
    } else {
        None
    }
}

/// The control file a script writes while the title runs (`XCL_PHASE_FILE`, default
/// `<logs_dir>/phase`). The state at `new` is ignored, so a stale file from an earlier run marks nothing.
struct PhaseFile {
    path: std::path::PathBuf,
    /// modified time, length and content of the last read that was taken in
    last: Option<(std::time::SystemTime, u64, String)>,
    warned: bool,
}

impl PhaseFile {
    fn new(path: std::path::PathBuf) -> Self {
        let mut f = PhaseFile { path, last: None, warned: false };
        f.last = f.read();
        f
    }

    /// The file as it is now; None when missing or not readable as UTF-8 text.
    fn read(&self) -> Option<(std::time::SystemTime, u64, String)> {
        let bytes = std::fs::read(&self.path).ok()?;
        let modified = std::fs::metadata(&self.path).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        let text = String::from_utf8(bytes).ok()?;
        let len = text.len() as u64;
        Some((modified, len, text))
    }

    /// Some(name) or Some("auto") only when the content changed since the last poll. A file that
    /// cannot be read (torn write, binary) is ignored until the next poll and warned once; a missing
    /// file is fine.
    fn poll(&mut self) -> Option<String> {
        let bytes = match std::fs::read(&self.path) {
            Ok(b) => b,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    self.last = None;
                } else {
                    self.warn_once(&format!("cannot read {}: {e}", self.path.display()));
                }
                return None;
            }
        };
        let text = match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => {
                self.warn_once(&format!("{} is not text, ignored", self.path.display()));
                return None;
            }
        };
        if self.last.as_ref().is_some_and(|(_, _, old)| *old == text) {
            return None;
        }
        let modified = std::fs::metadata(&self.path).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
        let len = text.len() as u64;
        let parsed = parse_phase_file(&text);
        self.last = Some((modified, len, text));
        if parsed.is_none() {
            self.warn_once(&format!("{} holds no valid phase name, ignored", self.path.display()));
        }
        parsed
    }

    fn warn_once(&mut self, msg: &str) {
        if !self.warned {
            self.warned = true;
            crate::xlog_warn!("telemetry: phase file: {msg}");
        }
    }
}

/// Writes one `phase` event; `from` is the previous phase of the same source (null for the first).
fn write_phase(name: &str, from: &mut Option<String>, confidence: f64, source: &str, frames: u64) {
    let from_v = match from.as_deref() {
        Some(f) => serde_json::Value::from(f),
        None => serde_json::Value::Null,
    };
    crate::log::event(
        crate::log::Tier::Frame,
        "phase",
        &[("name", name.into()), ("from", from_v), ("confidence", confidence.into()), ("source", source.into()), ("frames", frames.into())],
    );
    *from = Some(name.to_string());
}

/// frames shown so far
fn phase_frames() -> u64 {
    crate::hle::d3d12x::present::presents()
}

/// Every 250 ms: the control file, due schedule entries, the counters; every 1 s: classify. Reads only
/// public counters, holds no lock across a sleep and ends with the process.
fn telemetry_poll_thread(logs_dir: std::path::PathBuf) {
    let started = std::time::Instant::now();
    let path = std::env::var_os("XCL_PHASE_FILE").filter(|v| !v.is_empty()).map(std::path::PathBuf::from).unwrap_or_else(|| logs_dir.join("phase"));
    let mut file = PhaseFile::new(path);
    let (schedule, bad) = match std::env::var("XCL_PHASE_MARK") {
        Ok(v) => parse_phase_schedule(&v),
        Err(_) => (Vec::new(), Vec::new()),
    };
    for msg in bad {
        crate::xlog_warn!("telemetry: {msg}");
    }
    let mut next_entry = 0usize;
    let (mut from_mark, mut from_schedule, mut from_auto): (Option<String>, Option<String>, Option<String>) = (None, None, None);
    write_phase("boot", &mut from_auto, 1.0, "auto", phase_frames());
    let mut hysteresis = PhaseHysteresis::new(Some("boot"));
    let mut window_start = std::time::Instant::now();
    let mut frames0 = phase_frames();
    let mut draws0 = crate::hle::d3d12x::bind::activity().0;
    let mut io0 = crate::hle::dstorage::counters().1;
    let mut last_io_rate = 0.0f64;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(PHASE_POLL_MS));
        let frames_now = phase_frames();
        if let Some(name) = file.poll() {
            if name != "auto" {
                write_phase(&name, &mut from_mark, 1.0, "mark", frames_now);
            }
        }
        let elapsed = started.elapsed().as_secs_f64();
        while next_entry < schedule.len() && schedule[next_entry].0 <= elapsed {
            write_phase(&schedule[next_entry].1, &mut from_schedule, 1.0, "schedule", frames_now);
            next_entry += 1;
        }
        // (5e: loop counter readback goes here)
        let dt = window_start.elapsed();
        if dt < std::time::Duration::from_millis(PHASE_WINDOW_MS) {
            continue;
        }
        let draws_now = crate::hle::d3d12x::bind::activity().0;
        let io_now = crate::hle::dstorage::counters().1;
        // the xaudio2 stats lock two small lists: a poisoned one must not end the thread
        let voices = std::panic::catch_unwind(|| crate::hle::xaudio2::stats().source_voices).unwrap_or(0);
        let sample = PhaseSample {
            frames: frames_now.saturating_sub(frames0),
            draws: draws_now.saturating_sub(draws0),
            io_bytes: io_now.saturating_sub(io0),
            voices,
            dt_s: dt.as_secs_f64(),
            total_frames: frames_now,
        };
        let (cand, confidence) = classify_phase(hysteresis.shown, &sample, last_io_rate);
        last_io_rate = io_mib_s(&sample);
        if hysteresis.step(cand) {
            write_phase(cand, &mut from_auto, confidence, "auto", frames_now);
        }
        window_start = std::time::Instant::now();
        frames0 = frames_now;
        draws0 = draws_now;
        io0 = io_now;
    }
}

#[cfg(test)]
mod telemetry_p5c_tests {
    use super::*;

    fn sample(frames: u64, draws: u64, mib: f64, total: u64) -> PhaseSample {
        PhaseSample { frames, draws, io_bytes: (mib * 1024.0 * 1024.0) as u64, voices: 0, dt_s: 1.0, total_frames: total }
    }

    #[test]
    fn schedule_order_decimals_bare_garbage_negative() {
        let (s, bad) = parse_phase_schedule("40:world, 12.5:menu ,0.5:bench1");
        assert!(bad.is_empty());
        assert_eq!(s, vec![(0.5, "bench1".to_string()), (12.5, "menu".to_string()), (40.0, "world".to_string())]);
        let (s, bad) = parse_phase_schedule("world");
        assert_eq!(s, vec![(0.0, "world".to_string())]);
        assert!(bad.is_empty());
        let (s, bad) = parse_phase_schedule("abc:menu,-1:world,5:BAD NAME,nan:x,3:ok,,");
        assert_eq!(s, vec![(3.0, "ok".to_string())]);
        assert_eq!(bad.len(), 4);
        let (s, bad) = parse_phase_schedule("");
        assert!(s.is_empty() && bad.is_empty());
    }

    #[test]
    fn phase_names() {
        for n in KNOWN_PHASES {
            assert!(valid_phase_name(n));
        }
        assert!(valid_phase_name("bench1"));
        assert!(valid_phase_name("a_b-c"));
        assert!(valid_phase_name(&"a".repeat(32)));
        assert!(!valid_phase_name(&"a".repeat(33)));
        assert!(!valid_phase_name(""));
        assert!(!valid_phase_name("World"));
        assert!(!valid_phase_name("a b"));
        assert!(!valid_phase_name("caf\u{e9}"));
    }

    #[test]
    fn phase_file_content() {
        assert_eq!(parse_phase_file("world\n"), Some("world".to_string()));
        assert_eq!(parse_phase_file("  bench1\r\n"), Some("bench1".to_string()));
        assert_eq!(parse_phase_file("auto"), Some("auto".to_string()));
        assert_eq!(parse_phase_file(""), Some("auto".to_string()));
        assert_eq!(parse_phase_file("\n"), Some("auto".to_string()));
        assert_eq!(parse_phase_file("\u{0}\u{1}\u{fffd}\u{fffd}"), None);
        assert_eq!(parse_phase_file("two words"), None);
    }

    #[test]
    fn classify_table() {
        // never presented: boot, whatever else moves
        assert_eq!(classify_phase(None, &sample(0, 0, 500.0, 0), 500.0), ("boot", 1.0));
        // loading needs this and the previous window
        let (n, c) = classify_phase(Some("menu"), &sample(30, 9000, 256.0, 100), 40.0);
        assert_eq!(n, "loading");
        // 256 MiB/s / 512 = 0.5, capped at 0.4: 0.5 + 0.4
        assert!((c - 0.9).abs() < 1e-9);
        // the previous window was quiet: not loading; 30 frames x 400 draws = world, x 300 = menu
        assert_eq!(classify_phase(Some("menu"), &sample(30, 30 * 400, 256.0, 100), 0.0).0, "world");
        assert_eq!(classify_phase(Some("menu"), &sample(30, 30 * 300, 256.0, 100), 0.0).0, "menu");
        let (_, c) = classify_phase(None, &sample(30, 0, 64.0, 100), 64.0);
        assert!((c - 0.625).abs() < 1e-9);
        // frames advancing: draws per frame decides
        assert_eq!(classify_phase(None, &sample(60, 60 * 400, 0.0, 100), 0.0), ("world", 0.35));
        assert_eq!(classify_phase(None, &sample(60, 60 * 399, 0.0, 100), 0.0), ("menu", 0.35));
        // frames stand still: the previous phase stays, with no confidence
        assert_eq!(classify_phase(Some("world"), &sample(0, 0, 0.0, 100), 0.0), ("world", 0.0));
        assert_eq!(classify_phase(None, &sample(0, 0, 0.0, 100), 0.0), ("boot", 0.0));
    }

    #[test]
    fn hysteresis_boot_loading_world() {
        let mut h = PhaseHysteresis::new(Some("boot"));
        let mut written: Vec<&str> = Vec::new();
        let mut io_rate = 0.0;
        let windows = [
            sample(0, 0, 0.0, 0),
            sample(0, 0, 0.0, 0),
            sample(10, 100, 200.0, 10),
            sample(10, 100, 200.0, 20),
            sample(10, 100, 200.0, 30),
            sample(60, 60 * 800, 0.0, 90),
            sample(60, 60 * 800, 0.0, 150),
            sample(60, 60 * 800, 0.0, 210),
        ];
        for s in &windows {
            let (c, _) = classify_phase(h.shown, s, io_rate);
            io_rate = io_mib_s(s);
            if h.step(c) {
                written.push(c);
            }
        }
        assert_eq!(written, ["loading", "world"]);
        // a single odd window never writes
        let mut h = PhaseHysteresis::new(Some("world"));
        assert!(!h.step("menu"));
        assert!(!h.step("world"));
        assert!(!h.step("menu"));
        assert!(h.step("menu"));
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("xcl-p5c-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn phase_file_polling() {
        let dir = temp_dir("poll");
        let path = dir.join("phase");
        // missing file is fine
        let mut f = PhaseFile::new(path.clone());
        assert_eq!(f.poll(), None);
        // a stale file does not mark
        std::fs::write(&path, "old\n").unwrap();
        let mut f = PhaseFile::new(path.clone());
        assert_eq!(f.poll(), None);
        // a new content marks once
        std::fs::write(&path, "world\n").unwrap();
        assert_eq!(f.poll(), Some("world".to_string()));
        assert_eq!(f.poll(), None);
        // the same content again does not
        std::fs::write(&path, "world\n").unwrap();
        assert_eq!(f.poll(), None);
        std::fs::write(&path, "bench1").unwrap();
        assert_eq!(f.poll(), Some("bench1".to_string()));
        // auto ends the mark
        std::fs::write(&path, "auto\n").unwrap();
        assert_eq!(f.poll(), Some("auto".to_string()));
        assert_eq!(f.poll(), None);
        // garbage is ignored, binary too
        std::fs::write(&path, "not a name!").unwrap();
        assert_eq!(f.poll(), None);
        std::fs::write(&path, [0xffu8, 0xfe, 0x00]).unwrap();
        assert_eq!(f.poll(), None);
        std::fs::write(&path, "menu").unwrap();
        assert_eq!(f.poll(), Some("menu".to_string()));
        // removed: nothing, and the same name written again marks again
        std::fs::remove_file(&path).unwrap();
        assert_eq!(f.poll(), None);
        std::fs::write(&path, "menu").unwrap();
        assert_eq!(f.poll(), Some("menu".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
