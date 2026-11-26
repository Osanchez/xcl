//! Fork-only telemetry code (docs/guide/telemetry.md, part 5). Included from report.rs's fork block
//! by `#[path = "telemetry_run.rs"] mod run_tel; pub use run_tel::*;`, so its `pub` items are
//! reachable as `crate::report::<name>`. Upstream files call it only through one-line calls.

use std::sync::atomic::{AtomicU8, Ordering};

// ---- part 5a: no-window switches ----

/// 0 window (default), 1 headless, 2 skip; 0xFF = not read yet. Read once from the environment.
static PRESENT_MODE: AtomicU8 = AtomicU8::new(0xFF);

/// `XCL_HEADLESS=1` / `XCL_PRESENT_SKIP=1` (both set: headless wins, one warning).
/// `after_window == false` asks "headless?", `true` asks "skip?". One relaxed load once read.
pub fn present_bypass(after_window: bool) -> bool {
    let mut mode = PRESENT_MODE.load(Ordering::Relaxed);
    if mode == 0xFF {
        let headless = std::env::var("XCL_HEADLESS").ok();
        let skip = std::env::var("XCL_PRESENT_SKIP").ok();
        let (m, warning) = parse_present_mode(headless.as_deref(), skip.as_deref());
        if let Some(w) = warning {
            crate::xlog_warn!("{w}");
        }
        match m {
            1 => crate::xlog_info!("present: XCL_HEADLESS – no window, frames are counted, not drawn"),
            2 => crate::xlog_info!("present: XCL_PRESENT_SKIP – the window is kept, frames are counted, not drawn"),
            _ => {}
        }
        if m != 0 && std::env::var_os("XCL_FRAME_DUMP").is_some() {
            crate::xlog_info!("present: XCL_FRAME_DUMP is ignored with XCL_HEADLESS / XCL_PRESENT_SKIP");
        }
        // a racing second thread may log twice at most; both store the same value
        PRESENT_MODE.store(m, Ordering::Relaxed);
        mode = m;
    }
    mode == if after_window { 2 } else { 1 }
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
    match PRESENT_MODE.load(Ordering::Relaxed) {
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
