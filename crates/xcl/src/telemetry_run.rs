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
