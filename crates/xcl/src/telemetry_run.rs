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
    /// a bind's wait for a compute pipeline ran out its limit
    PipelineWaitExpired,
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
            ErrKind::PipelineWaitExpired => "pipeline_wait_expired",
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
    loop_counters_end();
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
    if let Some(s) = crate::hle::d3d12x::rt_chain::summary() {
        crate::xlog_info!("{s}");
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
    let b = crate::hle::d3d12x::pso_async::blocked();
    let (pso_us, builds) = (b[0][1] + b[1][1], super::pipeline_builds_title());
    let pso_us_prev = ACTIVITY_PSO_US.swap(pso_us, Ordering::Relaxed);
    let builds_prev = ACTIVITY_BUILDS_TITLE.swap(builds, Ordering::Relaxed);
    let mut f = vec![("presents_2s", presents_2s.into()), ("committed_mb", committed_mb.into())];
    f.extend(activity_pso_fields(pso_us, pso_us_prev, builds, builds_prev));
    let counts = super::counts_now();
    let prev = std::mem::replace(&mut *ACTIVITY_COUNTS.lock().unwrap_or_else(|e| e.into_inner()), counts);
    f.extend(activity_count_fields(&counts, &prev));
    crate::log::event(crate::log::Tier::Frame, "activity", &f);
}

/// The PSO microseconds waited and the title's pipeline builds at the previous `activity` event.
static ACTIVITY_PSO_US: AtomicU64 = AtomicU64::new(0);
static ACTIVITY_BUILDS_TITLE: AtomicU64 = AtomicU64::new(0);

/// pure: `pso_wait_ms` (the waited microseconds since the previous event, in ms, 3 decimals) and
/// `builds_title` (the title's pipeline builds since then).
fn activity_pso_fields(pso_us: u64, pso_us_prev: u64, builds: u64, builds_prev: u64) -> [(&'static str, serde_json::Value); 2] {
    [
        ("pso_wait_ms", (pso_us.saturating_sub(pso_us_prev) as f64 / 1000.0).into()),
        ("builds_title", builds.saturating_sub(builds_prev).into()),
    ]
}

/// The call counts at the previous `activity` event.
static ACTIVITY_COUNTS: std::sync::Mutex<[u64; super::COUNTS_LEN]> = std::sync::Mutex::new([0; super::COUNTS_LEN]);

/// pure: the 2 s deltas of the call counts: `draw_total`, `draw_no_output` (the left-out and
/// stand-in draws, counts 4-15), `dispatch_total`, `dispatch_skipped` (counts 17-25),
/// `cache_hit_layer`, `cache_miss_layer` and `mustdraw_wait_ms` (3 decimals).
fn activity_count_fields(now: &[u64; super::COUNTS_LEN], prev: &[u64; super::COUNTS_LEN]) -> Vec<(&'static str, serde_json::Value)> {
    let d = |i: usize| now[i].saturating_sub(prev[i]);
    let sum = |r: std::ops::Range<usize>| r.map(d).sum::<u64>();
    vec![
        ("draw_total", d(0).into()),
        ("draw_no_output", sum(4..16).into()),
        ("dispatch_total", d(2).into()),
        ("dispatch_skipped", (sum(17..26) + sum(33..36)).into()),
        ("cache_hit_layer", d(30).into()),
        ("cache_miss_layer", d(31).into()),
        ("mustdraw_wait_ms", (d(29) as f64 / 1000.0).into()),
    ]
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

    const ALL: [ErrKind; 13] = [
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
        ErrKind::PipelineWaitExpired,
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
        assert_eq!(seen.len(), 13);
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

/// Phase classes of a mark: the title is still (not interactive), playing, or nothing is marked.
pub const PHASE_UNKNOWN: u8 = 0;
pub const PHASE_STILL: u8 = 1;
pub const PHASE_PLAY: u8 = 2;

/// pure: the class of a phase name. `world` plays; the non-interactive names are still; `auto` and
/// any other name are unknown.
pub fn phase_class_of(name: &str) -> u8 {
    match name {
        "world" => PHASE_PLAY,
        "boot" | "intro" | "loading" | "menu" | "cutscene" | "splash" | "credits" => PHASE_STILL,
        _ => PHASE_UNKNOWN,
    }
}

/// The class of the last mark (control file or schedule); the automatic guess never sets it.
static MARK_CLASS: AtomicU8 = AtomicU8::new(PHASE_UNKNOWN);

fn set_mark(name: &str) {
    MARK_CLASS.store(phase_class_of(name), Ordering::Relaxed);
}

/// The phase class the user marked (`PHASE_UNKNOWN` with no mark). One atomic load; with telemetry
/// off, the first call starts `xcl-phase-mark`, which reads the marks only (no events, no files).
pub fn phase_mark_class() -> u8 {
    static START: std::sync::Once = std::sync::Once::new();
    START.call_once(|| {
        if crate::log::telemetry_on(crate::log::Tier::Frame) {
            return;
        }
        let logs_dir = crate::state::boot().map(|b| b.logs_dir());
        if let Err(e) = std::thread::Builder::new().name("xcl-phase-mark".to_string()).spawn(move || phase_mark_thread(logs_dir)) {
            crate::xlog_warn!("telemetry: cannot start the phase mark thread: {e}");
        }
    });
    MARK_CLASS.load(Ordering::Relaxed)
}

/// Telemetry off: the same control file and schedule as `telemetry_poll_thread`, every 250 ms, but
/// only `set_mark`.
fn phase_mark_thread(logs_dir: Option<std::path::PathBuf>) {
    let started = std::time::Instant::now();
    let path = std::env::var_os("XCL_PHASE_FILE").filter(|v| !v.is_empty()).map(std::path::PathBuf::from).or_else(|| logs_dir.map(|d| d.join("phase")));
    let mut file = path.map(PhaseFile::new);
    let (schedule, bad) = match std::env::var("XCL_PHASE_MARK") {
        Ok(v) => parse_phase_schedule(&v),
        Err(_) => (Vec::new(), Vec::new()),
    };
    for msg in bad {
        crate::xlog_warn!("telemetry: {msg}");
    }
    let mut next_entry = 0usize;
    loop {
        if let Some(name) = file.as_mut().and_then(|f| f.poll()) {
            set_mark(&name);
        }
        let elapsed = started.elapsed().as_secs_f64();
        while next_entry < schedule.len() && schedule[next_entry].0 <= elapsed {
            set_mark(&schedule[next_entry].1);
            next_entry += 1;
        }
        std::thread::sleep(std::time::Duration::from_millis(PHASE_POLL_MS));
    }
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
            set_mark(&name);
            if name != "auto" {
                write_phase(&name, &mut from_mark, 1.0, "mark", frames_now);
            }
        }
        let elapsed = started.elapsed().as_secs_f64();
        while next_entry < schedule.len() && schedule[next_entry].0 <= elapsed {
            set_mark(&schedule[next_entry].1);
            write_phase(&schedule[next_entry].1, &mut from_schedule, 1.0, "schedule", frames_now);
            next_entry += 1;
        }
        loop_counters_tick(false);
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
    fn phase_names_map_to_classes() {
        assert_eq!(phase_class_of("world"), PHASE_PLAY);
        for n in ["boot", "intro", "loading", "menu", "cutscene", "splash", "credits"] {
            assert_eq!(phase_class_of(n), PHASE_STILL, "{n}");
        }
        assert_eq!(phase_class_of("auto"), PHASE_UNKNOWN);
        assert_eq!(phase_class_of("bench1"), PHASE_UNKNOWN);
    }

    #[test]
    fn a_mark_sets_the_class_and_auto_clears_it() {
        set_mark("intro");
        assert_eq!(MARK_CLASS.load(Ordering::Relaxed), PHASE_STILL);
        set_mark("world");
        assert_eq!(MARK_CLASS.load(Ordering::Relaxed), PHASE_PLAY);
        set_mark("auto");
        assert_eq!(MARK_CLASS.load(Ordering::Relaxed), PHASE_UNKNOWN);
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

// ---- part 5e: loop guard counters ----
//
// `XCL_LOOP_COUNTERS` (with `XCL_TELEMETRY=all`): translated code counts the lanes that leave a loop
// through its guard into a 16-byte slot per distinct shader set, in a region of the first committed
// UMA window (the only window the CPU sees GPU stores in). The poll thread reads the slots.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Whether translated code counts loop-guard exits (`XCL_LOOP_COUNTERS` with tier `all`): decided
/// once per process from the environment, before the first translation.
pub fn loop_counters_on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let counters = std::env::var("XCL_LOOP_COUNTERS").ok();
        let tier = std::env::var("XCL_TELEMETRY").ok();
        let (on, warning) = loop_counters_wanted(counters.as_deref(), tier.as_deref());
        if let Some(w) = warning {
            crate::xlog_warn!("{w}");
        }
        on
    })
}

/// pure: the decision from the two variables, and the warning when the counters were asked for
/// at a tier below `all`
fn loop_counters_wanted(counters: Option<&str>, tier: Option<&str>) -> (bool, Option<&'static str>) {
    if !counters.is_some_and(|v| !v.is_empty() && v != "0") {
        return (false, None);
    }
    if crate::log::parse_tier(tier) == Ok(crate::log::Tier::All) {
        (true, None)
    } else {
        (false, Some("telemetry: XCL_LOOP_COUNTERS needs XCL_TELEMETRY=all - loop counters off"))
    }
}

/// pure: the io word of byte offset `off` in window `k` (0 is reserved for "do not count")
pub(crate) fn loop_counter_word_of(k: usize, off: usize) -> u32 {
    debug_assert!(k < 16 && off % 16 == 0 && off >> 4 < 1 << 28, "loop counter slot {k}:{off:#x}");
    ((k as u32) << 28) | ((off >> 4) as u32 & 0x0FFF_FFFF)
}

/// bytes of the counter region: 65 536 slots of 16 bytes, slot 0 unused
const LC_REGION_BYTES: usize = 1 << 20;

/// Slot table: one 16-byte slot per distinct shader set, slot 0 never used. `labels[slot]` is the
/// label text and the (kind, fnv64) of each shader of the set.
struct LcSlots {
    by_label: HashMap<String, u32>,
    by_pso: HashMap<usize, u32>,
    labels: Vec<(String, Vec<(&'static str, u64)>)>,
    full_warned: bool,
}

impl LcSlots {
    fn new() -> Self {
        LcSlots { by_label: HashMap::new(), by_pso: HashMap::new(), labels: vec![(String::new(), Vec::new())], full_warned: false }
    }

    /// The slot of `pso`, a new one for a shader set not seen before; 0 when the table holds
    /// `capacity` slots (slot 0 included), with one warning.
    fn assign(&mut self, pso: usize, text: String, shaders: Vec<(&'static str, u64)>, capacity: u32) -> u32 {
        if let Some(&slot) = self.by_label.get(&text) {
            self.by_pso.insert(pso, slot);
            return slot;
        }
        if self.labels.len() as u32 >= capacity {
            self.by_pso.remove(&pso);
            if !self.full_warned {
                self.full_warned = true;
                crate::xlog_warn!("telemetry: more than {} distinct shader sets - the loop counters of later pipelines are not counted", capacity.saturating_sub(1));
            }
            return 0;
        }
        let slot = self.labels.len() as u32;
        self.labels.push((text.clone(), shaders));
        self.by_label.insert(text, slot);
        self.by_pso.insert(pso, slot);
        slot
    }
}

static LC_SLOTS: Mutex<Option<LcSlots>> = Mutex::new(None);

/// (window k, address of the region, its offset in window k); None: no region
static LC_REGION: OnceLock<Option<(usize, usize, usize)>> = OnceLock::new();

/// The region in the first committed window (`Uma::committed_window`), allocated once; None (with
/// one warning) when there is none, or when the counters are off.
pub(super) fn lc_region() -> Option<(usize, usize, usize)> {
    if !loop_counters_on() {
        return None;
    }
    *LC_REGION.get_or_init(|| {
        let Some(u) = crate::gpu::uma::get() else {
            crate::xlog_warn!("telemetry: loop counters: no UMA - not counted");
            return None;
        };
        let Some(k) = u.committed_window() else {
            crate::xlog_warn!("telemetry: loop counters: no committed UMA window - not counted");
            return None;
        };
        let Some(p) = u.alloc_in(k, LC_REGION_BYTES, 1 << 16) else {
            crate::xlog_warn!("telemetry: loop counters: no room for the counter region in window {k} - not counted");
            return None;
        };
        let addr = p as usize;
        // SAFETY: the allocation is LC_REGION_BYTES long, committed and writable
        unsafe { std::ptr::write_bytes(p, 0, LC_REGION_BYTES) };
        crate::xlog_info!("telemetry: loop counters on: {} slots in UMA window {k} at offset {:#x}", LC_REGION_BYTES / 16 - 1, addr - u.window(k).0);
        Some((k, addr, addr - u.window(k).0))
    })
}

/// The stage kinds of a label as the `&'static str` the table keeps.
fn lc_kind(k: &str) -> &'static str {
    match k {
        "cs" => "cs",
        "ms" => "ms",
        "vs" => "vs",
        "ps" => "ps",
        "gs" => "gs",
        _ => "?",
    }
}

/// From `dred::label`: give the pipeline its slot (a new one for a new shader set).
pub fn loop_counters_label(pso: usize, shaders: &[(&str, Option<&[u8]>)]) {
    if !loop_counters_on() || pso == 0 {
        return;
    }
    let mut text = String::new();
    let mut list: Vec<(&'static str, u64)> = Vec::new();
    for (k, b) in shaders {
        let Some(b) = *b else { continue };
        let h = crate::gpu::shader::fnv64(b);
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(&format!("{k} {h:016x}"));
        list.push((lc_kind(k), h));
    }
    if list.is_empty() || lc_region().is_none() {
        return;
    }
    let capacity = (LC_REGION_BYTES / 16) as u32;
    let mut g = LC_SLOTS.lock().unwrap_or_else(|e| e.into_inner());
    g.get_or_insert_with(LcSlots::new).assign(pso, text, list, capacity);
}

/// From the two bind sites: the io word of `pso` (0 when off, unknown or no region).
pub fn loop_counter_word(pso: usize) -> u32 {
    if !loop_counters_on() {
        return 0;
    }
    let Some((k, _, off)) = LC_REGION.get().copied().flatten() else { return 0 };
    let slot = {
        let g = LC_SLOTS.lock().unwrap_or_else(|e| e.into_inner());
        match g.as_ref().and_then(|s| s.by_pso.get(&pso)) {
            Some(&slot) => slot,
            None => return 0,
        }
    };
    loop_counter_word_of(k, off + 16 * slot as usize)
}

/// pure: per slot, the change since the last read: (slot, word, delta, total, first_trip).
/// `first_trip` marks the first changed word of a slot whose words were all 0 before.
fn lc_deltas(prev: &mut Vec<[u32; 4]>, now: &[[u32; 4]]) -> Vec<(u32, usize, u32, u32, bool)> {
    if prev.len() < now.len() {
        prev.resize(now.len(), [0; 4]);
    }
    let mut out = Vec::new();
    for (slot, words) in now.iter().enumerate() {
        let mut first = prev[slot] == [0; 4];
        for (w, &total) in words.iter().enumerate() {
            if total != prev[slot][w] {
                out.push((slot as u32, w, total.wrapping_sub(prev[slot][w]), total, first));
                first = false;
            }
        }
        prev[slot] = *words;
    }
    out
}

/// pure: the fields of one `loop_guard` event
fn loop_guard_fields(record: u64, stage: &str, slot: u32, delta: u32, total: u32) -> Vec<(&'static str, serde_json::Value)> {
    vec![("record", format!("{record:016x}").into()), ("stage", stage.into()), ("pipeline", slot.into()), ("trips", delta.into()), ("total", total.into())]
}

/// What the poll thread last read, and when.
struct LcRead {
    prev: Vec<[u32; 4]>,
    last: Option<std::time::Instant>,
}

static LC_READ: Mutex<LcRead> = Mutex::new(LcRead { prev: Vec::new(), last: None });

/// Poll thread (every call, reads at most once a second) and `telemetry_final` (`final_read`):
/// reads every assigned slot's 4 words, writes `loop_guard` events (tier all) for each changed
/// word, and a non-fatal `loop_guard_tripped` error on a slot's first trip.
pub(super) fn loop_counters_tick(final_read: bool) {
    if !loop_counters_on() {
        return;
    }
    if let Some((_, addr, _)) = LC_REGION.get().copied().flatten() {
        lc_read(final_read, addr);
    }
}

/// From `write_end` (the once-latched end of the run): a last read, so the trips of the final
/// second are written, then the `loop_counters` summary once, also when nothing tripped. The only
/// other holder of `LC_READ` is the poll thread's short read; poisoned locks are recovered.
fn loop_counters_end() {
    if !loop_counters_on() {
        return;
    }
    let region = LC_REGION.get().copied().flatten();
    let tripped = region.and_then(|(_, addr, _)| lc_read(true, addr)).unwrap_or(0);
    if !LC_SUMMARY.swap(true, Ordering::AcqRel) {
        let slots = LC_SLOTS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map_or(0, |s| s.labels.len().saturating_sub(1) as u32);
        crate::log::event(crate::log::Tier::All, "loop_counters", &loop_counters_summary_fields(region.map(|r| r.0), slots, tripped));
        crate::xlog_info!("telemetry: loop counters: {slots} shader sets labelled, {tripped} left a loop through its guard");
    }
}

/// The `loop_counters` event of the end of the run is written once.
static LC_SUMMARY: AtomicBool = AtomicBool::new(false);

/// pure: the fields of the `loop_counters` event (`window` None: no region)
fn loop_counters_summary_fields(window: Option<usize>, slots: u32, tripped: u32) -> Vec<(&'static str, serde_json::Value)> {
    let w = match window {
        Some(k) => serde_json::Value::from(k),
        None => serde_json::Value::Null,
    };
    vec![("window", w), ("slots", slots.into()), ("tripped", tripped.into())]
}

/// One read of the slots at `addr` and its events; the number of slots with a non-zero word, or
/// None when the read was skipped (not due, or another thread is reading).
fn lc_read(final_read: bool, addr: usize) -> Option<u32> {
    let mut rd = match LC_READ.try_lock() {
        Ok(g) => g,
        Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) if !final_read => return None,
        Err(std::sync::TryLockError::WouldBlock) => LC_READ.lock().unwrap_or_else(|e| e.into_inner()),
    };
    if !final_read && rd.last.is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(1)) {
        return None;
    }
    rd.last = Some(std::time::Instant::now());
    let count = LC_SLOTS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map_or(0, |s| s.labels.len());
    // the region is read outside the slot lock
    let mut now: Vec<[u32; 4]> = vec![[0; 4]; count];
    for (slot, words) in now.iter_mut().enumerate().skip(1) {
        for (w, word) in words.iter_mut().enumerate() {
            // SAFETY: slots 1..count lie inside the region (`LC_REGION_BYTES`), committed memory the GPU writes
            *word = unsafe { std::ptr::read_volatile((addr + 16 * slot + 4 * w) as *const u32) };
        }
    }
    let tripped = now.iter().filter(|w| **w != [0; 4]).count() as u32;
    let deltas = lc_deltas(&mut rd.prev, &now);
    drop(rd);
    for (slot, word, delta, total, first) in deltas {
        let (text, shaders) = {
            let g = LC_SLOTS.lock().unwrap_or_else(|e| e.into_inner());
            match g.as_ref().and_then(|s| s.labels.get(slot as usize)) {
                Some(l) => l.clone(),
                None => continue,
            }
        };
        let kinds: &[&str] = match word {
            0 => &["cs", "ms", "vs"],
            1 => &["ps"],
            2 => &["gs"],
            _ => continue,
        };
        let entry = shaders.iter().find(|(k, _)| kinds.contains(k));
        let (stage, record) = match entry {
            Some((k, h)) => (*k, *h),
            None => (kinds[0], 0),
        };
        let fields = loop_guard_fields(record, stage, slot, delta, total);
        crate::log::event(crate::log::Tier::All, "loop_guard", &fields);
        if first {
            telemetry_error(ErrKind::LoopGuardTripped, "loop_counters", &format!("pipeline {slot} ({text}) left a loop through its guard"), None, false, &fields);
        }
    }
    Some(tripped)
}

#[cfg(test)]
mod telemetry_p5e_tests {
    use super::*;

    #[test]
    fn counters_are_on_only_with_tier_all() {
        assert_eq!(loop_counters_wanted(Some("1"), Some("all")), (true, None));
        assert_eq!(loop_counters_wanted(Some("1"), Some("ALL")), (true, None));
        assert_eq!(loop_counters_wanted(None, Some("all")), (false, None));
        assert_eq!(loop_counters_wanted(Some("0"), Some("all")), (false, None));
        assert_eq!(loop_counters_wanted(Some(""), Some("all")), (false, None));
        assert_eq!(loop_counters_wanted(Some("0"), Some("frame")), (false, None));
        let (on, warning) = loop_counters_wanted(Some("1"), Some("frame"));
        assert!(!on && warning.is_some());
        let (on, warning) = loop_counters_wanted(Some("1"), None);
        assert!(!on && warning.is_some());
        let (on, warning) = loop_counters_wanted(Some("1"), Some("pass"));
        assert!(!on && warning.is_some());
    }

    #[test]
    fn activity_pso_fields_are_deltas() {
        let f = activity_pso_fields(7500, 2000, 9, 4);
        assert_eq!(f[0], ("pso_wait_ms", serde_json::json!(5.5)));
        assert_eq!(f[1], ("builds_title", serde_json::json!(5)));
        let f = activity_pso_fields(0, 0, 0, 0);
        assert_eq!((f[0].1.as_f64(), f[1].1.as_u64()), (Some(0.0), Some(0)));
        // a counter that went back never wraps
        let f = activity_pso_fields(10, 20, 1, 3);
        assert_eq!((f[0].1.as_f64(), f[1].1.as_u64()), (Some(0.0), Some(0)));
    }

    #[test]
    fn activity_count_fields_are_deltas_and_sums() {
        let mut prev = [0u64; super::super::COUNTS_LEN];
        let mut now = [0u64; super::super::COUNTS_LEN];
        now[0] = 500;
        prev[0] = 100;
        now[2] = 40;
        prev[2] = 10;
        // draw skips and stand-ins (4-15) and dispatch skips (17-25) are summed; others stay out
        now[4] = 3;
        now[9] = 2;
        prev[9] = 1;
        now[15] = 5;
        now[16] = 99;
        now[17] = 4;
        prev[17] = 1;
        now[25] = 6;
        now[26] = 77;
        now[29] = 2500;
        prev[29] = 500;
        now[30] = 8;
        now[31] = 9;
        prev[31] = 4;
        let f = activity_count_fields(&now, &prev);
        let get = |k: &str| f.iter().find(|(n, _)| *n == k).unwrap().1.clone();
        assert_eq!(get("draw_total"), serde_json::json!(400));
        assert_eq!(get("draw_no_output"), serde_json::json!(3 + 1 + 5));
        assert_eq!(get("dispatch_total"), serde_json::json!(30));
        assert_eq!(get("dispatch_skipped"), serde_json::json!(3 + 6));
        assert_eq!(get("cache_hit_layer"), serde_json::json!(8));
        assert_eq!(get("cache_miss_layer"), serde_json::json!(5));
        assert_eq!(get("mustdraw_wait_ms"), serde_json::json!(2.0));
        // a count that went back never wraps
        let f = activity_count_fields(&prev, &now);
        assert!(f.iter().all(|(_, v)| v.as_f64() == Some(0.0)));
    }

    #[test]
    fn activity_dispatch_skipped_sums_new_slots() {
        let prev = [0u64; super::super::COUNTS_LEN];
        let mut now = [0u64; super::super::COUNTS_LEN];
        now[17] = 1;
        now[25] = 2;
        now[26] = 50; // not a skip of a dispatch
        now[32] = 60; // reserved_0
        now[33] = 3;
        now[34] = 4;
        now[35] = 5;
        let f = activity_count_fields(&now, &prev);
        let skipped = f.iter().find(|(n, _)| *n == "dispatch_skipped").unwrap().1.clone();
        assert_eq!(skipped, serde_json::json!(1 + 2 + 3 + 4 + 5));
    }

    #[test]
    fn io_word_carries_window_and_slot_offset() {
        assert_eq!(loop_counter_word_of(3, 0x1230), (3 << 28) | 0x123);
        assert_eq!(loop_counter_word_of(0, 16), 1);
        assert_eq!(loop_counter_word_of(1, 0x4000), (1 << 28) | 0x400);
        for k in [0usize, 3, 15] {
            for slot in 1..4usize {
                assert_ne!(loop_counter_word_of(k, 16 * slot), 0);
            }
        }
    }

    #[test]
    fn slots_are_per_shader_set() {
        let mut t = LcSlots::new();
        assert_eq!(t.assign(0x10, "cs a".into(), vec![("cs", 1)], 3), 1);
        // the same shader set from another pipeline object shares the slot
        assert_eq!(t.assign(0x20, "cs a".into(), vec![("cs", 1)], 3), 1);
        assert_eq!(t.by_pso.get(&0x20), Some(&1));
        assert_eq!(t.assign(0x30, "cs b".into(), vec![("cs", 2)], 3), 2);
        assert!(!t.full_warned);
        // capacity 3: slot 0 + two slots; a third distinct text does not fit
        assert_eq!(t.assign(0x40, "cs c".into(), vec![("cs", 3)], 3), 0);
        assert!(t.full_warned);
        assert!(!t.by_pso.contains_key(&0x40));
        // known sets still resolve when the table is full
        assert_eq!(t.assign(0x50, "cs b".into(), vec![("cs", 2)], 3), 2);
    }

    #[test]
    fn deltas_since_the_last_read() {
        let mut prev: Vec<[u32; 4]> = Vec::new();
        assert!(lc_deltas(&mut prev, &[[0; 4], [0; 4]]).is_empty());
        let d = lc_deltas(&mut prev, &[[0; 4], [64, 0, 0, 0]]);
        assert_eq!(d, vec![(1, 0, 64, 64, true)]);
        // unchanged: nothing
        assert!(lc_deltas(&mut prev, &[[0; 4], [64, 0, 0, 0]]).is_empty());
        // grows again: the delta, not a first trip
        let d = lc_deltas(&mut prev, &[[0; 4], [96, 32, 0, 0], [0; 4]]);
        assert_eq!(d, vec![(1, 0, 32, 96, false), (1, 1, 32, 32, false)]);
        // a new slot with two words at once: only the first word is the first trip
        let d = lc_deltas(&mut prev, &[[0; 4], [96, 32, 0, 0], [5, 0, 7, 0]]);
        assert_eq!(d, vec![(2, 0, 5, 5, true), (2, 2, 7, 7, false)]);
    }

    #[test]
    fn loop_counters_summary_event_fields() {
        let f = loop_counters_summary_fields(Some(3), 12, 0);
        let keys: Vec<&str> = f.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, ["window", "slots", "tripped"]);
        assert_eq!(f[0].1, 3);
        assert_eq!(f[1].1, 12);
        assert_eq!(f[2].1, 0);
        let f = loop_counters_summary_fields(None, 0, 0);
        assert_eq!(f[0].1, serde_json::Value::Null);
        assert_eq!(f[1].1, 0);
        assert_eq!(f[2].1, 0);
    }

    #[test]
    fn loop_guard_event_fields() {
        let f = loop_guard_fields(0xABCDEF, "ps", 7, 64, 128);
        let keys: Vec<&str> = f.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, ["record", "stage", "pipeline", "trips", "total"]);
        assert_eq!(f[0].1, "0000000000abcdef");
        assert_eq!(f[1].1, "ps");
        assert_eq!(f[2].1, 7);
        assert_eq!(f[3].1, 64);
        assert_eq!(f[4].1, 128);
    }
}
