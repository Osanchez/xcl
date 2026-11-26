//! Device removal: when and where it was first seen, and what each queue had got to.
//!
//! On when the telemetry tier is `frame` or more, or with `XCL_REMOVAL_INFO=1`. `arm` registers a
//! fence event at the device's creation (Microsoft's documented removal callback: a fence waited
//! on for `u64::MAX` is signalled when the device is removed). After each `ExecuteCommandLists`
//! `after_execute` signals a fence of the queue and samples what it had completed, so the report
//! can say how many submissions the GPU was seen to finish before the removal.

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

use windows::core::Interface;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::System::Threading::CreateEventW;

/// A queue's submissions, as `after_execute` counts them (`last_seq`, `last_lists` and `executes`
/// are for a debugger and the ring unit).
#[allow(dead_code)]
struct QueueProgress {
    fence: Option<ID3D12Fence>,
    submitted: u64,
    last_seq: u64,
    last_lists: usize,
    completed_seen: u64,
    seen_qpc: i64,
    executes: u64,
}

impl QueueProgress {
    fn new() -> Self {
        QueueProgress { fence: None, submitted: 0, last_seq: 0, last_lists: 0, completed_seen: 0, seen_qpc: 0, executes: 0 }
    }
}

const SOURCE_NONE: u8 = 0;
const SOURCE_FENCE: u8 = 1;
const SOURCE_DETECTED: u8 = 2;

struct State {
    removed_qpc: AtomicI64,
    source: AtomicU8,
    site: Mutex<Option<(&'static str, usize)>>,
    queues: Mutex<BTreeMap<usize, QueueProgress>>,
    own_seq: AtomicU64,
}

impl State {
    const fn new() -> Self {
        State {
            removed_qpc: AtomicI64::new(0),
            source: AtomicU8::new(SOURCE_NONE),
            site: Mutex::new(None),
            queues: Mutex::new(BTreeMap::new()),
            own_seq: AtomicU64::new(0),
        }
    }

    /// The first caller stores the site, the queue and the moment; later ones change nothing.
    fn first_seen(&self, site: &'static str, queue: usize, source: u8, now: i64) {
        let mut s = self.site.lock().unwrap();
        if s.is_none() {
            *s = Some((site, queue));
            self.removed_qpc.store(now, Ordering::Release);
            self.source.store(source, Ordering::Release);
        }
    }

    fn after_execute(&self, queue: usize, lists: usize, seq: Option<u64>, now: i64, sample: impl FnOnce(&mut QueueProgress) -> Option<u64>) {
        let seq = seq.unwrap_or_else(|| self.own_seq.fetch_add(1, Ordering::Relaxed));
        let mut q = self.queues.lock().unwrap();
        let p = q.entry(queue).or_insert_with(QueueProgress::new);
        if let Some(done) = sample(p) {
            p.completed_seen = done;
            p.seen_qpc = now;
        }
        p.submitted += 1;
        p.last_seq = seq;
        p.last_lists = lists;
        p.executes += 1;
    }

    /// (log lines, event fields). `start` is the QPC the run's times count from.
    fn report(&self, start: i64, qpf: i64) -> (Vec<String>, Vec<(&'static str, serde_json::Value)>) {
        let removed = self.removed_qpc.load(Ordering::Acquire);
        let source = if self.source.load(Ordering::Acquire) == SOURCE_FENCE { "fence" } else { "detected" };
        let (site, queue) = self.site.lock().unwrap().unwrap_or(("other", 0));
        let ms = |ticks: i64| ticks as f64 * 1000.0 / qpf.max(1) as f64;
        let moment_ms = ms(removed - start);
        let qptr = queue as *const c_void;
        let mut lines = vec![format!("removal: device removed at +{moment_ms:.0} ms (source {source}), first seen after {site} on queue {qptr:p}")];
        let mut queues = Vec::new();
        let mut detecting = None;
        for (&q, p) in self.queues.lock().unwrap().iter() {
            let pending = p.submitted.saturating_sub(p.completed_seen);
            let seen_ms_before = if p.seen_qpc == 0 { f64::INFINITY } else { ms(removed - p.seen_qpc).max(0.0) };
            let before = if seen_ms_before.is_finite() { format!("{seen_ms_before:.1}") } else { "never".to_string() };
            lines.push(format!(
                "removal: queue {:p}: {} submissions, completed {} as of {before} ms before the removal ({pending} not seen complete)",
                q as *const c_void, p.submitted, p.completed_seen
            ));
            if q == queue {
                detecting = Some((pending, seen_ms_before));
            }
            queues.push(serde_json::json!({
                "queue": format!("{:#x}", q),
                "submitted": p.submitted,
                "completed_seen": p.completed_seen,
                "pending": pending,
                "seen_ms_before": if seen_ms_before.is_finite() { serde_json::Value::from(seen_ms_before) } else { serde_json::Value::Null },
            }));
        }
        if let Some((pending, seen)) = detecting {
            lines.push(format!("removal: queue {qptr:p} was {} at its last sample", classify(pending, seen)));
        }
        let fields = vec![
            ("moment_ms", serde_json::Value::from(moment_ms)),
            ("source", source.into()),
            ("site", site.into()),
            ("queue", format!("{queue:#x}").into()),
            ("queues", serde_json::Value::Array(queues)),
        ];
        (lines, fields)
    }
}

static STATE: State = State::new();
/// The QPC at `arm`: the removal's moment is counted from it.
static START_QPC: AtomicI64 = AtomicI64::new(0);

/// How the detecting queue looked at its last sample.
fn classify(pending_on_detecting_queue: u64, seen_ms_before: f64) -> &'static str {
    if pending_on_detecting_queue == 0 {
        "idle_at_last_sample"
    } else if seen_ms_before <= 100.0 {
        "work_outstanding"
    } else {
        "unknown"
    }
}

fn qpf() -> i64 {
    static F: OnceLock<i64> = OnceLock::new();
    *F.get_or_init(|| {
        let mut v = 0i64;
        unsafe { windows_sys::Win32::System::Performance::QueryPerformanceFrequency(&mut v) };
        v.max(1)
    })
}

/// On at telemetry tier `frame` or more, or with `XCL_REMOVAL_INFO=1`.
fn on() -> bool {
    static ENV: OnceLock<bool> = OnceLock::new();
    crate::log::telemetry_on(crate::log::Tier::Frame) || *ENV.get_or_init(|| std::env::var("XCL_REMOVAL_INFO").is_ok_and(|v| v == "1"))
}

/// Called at the end of `init`: a fence that is signalled with `u64::MAX` when the device is
/// removed, and a thread-pool wait on its event that calls `on_removed`.
pub fn arm(dev: &ID3D12Device) {
    use windows_sys::Win32::System::Threading::{RegisterWaitForSingleObject, INFINITE, WT_EXECUTEONLYONCE};
    unsafe {
        START_QPC.store(super::frame::qpc(), Ordering::Release);
        let fence: ID3D12Fence = match dev.CreateFence(0, D3D12_FENCE_FLAG_NONE) {
            Ok(f) => f,
            Err(e) => {
                crate::xlog_warn!("removal: no fence for the removal callback: {e}");
                return;
            }
        };
        let event = match CreateEventW(None, false, false, None) {
            Ok(h) => h,
            Err(e) => {
                crate::xlog_warn!("removal: no event for the removal callback: {e}");
                return;
            }
        };
        if let Err(e) = fence.SetEventOnCompletion(u64::MAX, event) {
            crate::xlog_warn!("removal: SetEventOnCompletion for the removal callback failed: {e}");
            return;
        }
        let mut wait = std::ptr::null_mut();
        if RegisterWaitForSingleObject(&mut wait, event.0, Some(on_removed), std::ptr::null(), INFINITE, WT_EXECUTEONLYONCE) == 0 {
            crate::xlog_warn!("removal: RegisterWaitForSingleObject for the removal callback failed");
            return;
        }
        // the fence, the event and the wait live as long as the process
        std::mem::forget(fence);
    }
}

/// The fence's event fired: the device was removed. The process is stopped by the next Execute's
/// `stop_if_device_removed`, after the report is written.
unsafe extern "system" fn on_removed(_ctx: *mut c_void, _timed_out: bool) {
    STATE.first_seen("fence", 0, SOURCE_FENCE, super::frame::qpc());
    super::real::log_device_removed();
}

/// After a submission: the queue's progress is sampled and a signal added behind its work.
pub fn after_execute(queue: *mut c_void, lists: &[*mut c_void], seq: Option<u64>) {
    if !on() {
        return;
    }
    let dev_raw = super::real::device_raw();
    let now = super::frame::qpc();
    STATE.after_execute(queue as usize, lists.len(), seq, now, |p| unsafe {
        if p.fence.is_none() {
            let dev = ID3D12Device::from_raw_borrowed(&dev_raw)?;
            p.fence = dev.CreateFence(0, D3D12_FENCE_FLAG_NONE).ok();
        }
        let fence = p.fence.as_ref()?;
        let done = fence.GetCompletedValue();
        let q = ID3D12CommandQueue::from_raw_borrowed(&queue)?;
        let _ = q.Signal(fence, p.submitted + 1);
        (done != u64::MAX).then_some(done)
    });
}

/// The first caller names where the removal was seen (`"execute"`, `"present"`, `"fence"`,
/// `"other"`) and the queue.
pub fn detected_at(site: &'static str, queue: *mut c_void) {
    STATE.first_seen(site, queue as usize, SOURCE_DETECTED, super::frame::qpc());
}

/// The log lines are written; the fields go into the `device_removed` event.
pub fn report() -> Vec<(&'static str, serde_json::Value)> {
    let (lines, fields) = STATE.report(START_QPC.load(Ordering::Acquire), qpf());
    for l in lines {
        crate::xlog_error!("{l}");
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_names_idle_outstanding_unknown() {
        assert_eq!(classify(0, 5.0), "idle_at_last_sample");
        assert_eq!(classify(3, 20.0), "work_outstanding");
        assert_eq!(classify(3, 500.0), "unknown");
    }

    #[test]
    fn detected_at_keeps_the_first_site() {
        let s = State::new();
        s.first_seen("execute", 0x10, SOURCE_DETECTED, 100);
        s.first_seen("present", 0x20, SOURCE_FENCE, 200);
        assert_eq!(*s.site.lock().unwrap(), Some(("execute", 0x10)));
        assert_eq!(s.removed_qpc.load(Ordering::Acquire), 100);
        assert_eq!(s.source.load(Ordering::Acquire), SOURCE_DETECTED);
    }

    #[test]
    fn report_fields_have_fixed_names() {
        let s = State::new();
        s.after_execute(0x10, 2, Some(7), 1_000, |_| Some(0));
        s.after_execute(0x10, 1, Some(8), 2_000, |_| Some(1));
        s.first_seen("execute", 0x10, SOURCE_DETECTED, 3_000);
        let (lines, fields) = s.report(0, 1_000_000);
        let names: Vec<&str> = fields.iter().map(|(k, _)| *k).collect();
        assert_eq!(names, ["moment_ms", "source", "site", "queue", "queues"]);
        assert_eq!(fields[1].1, "detected");
        assert_eq!(fields[2].1, "execute");
        let q = &fields[4].1[0];
        let qnames: Vec<&str> = q.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        for k in ["queue", "submitted", "completed_seen", "pending", "seen_ms_before"] {
            assert!(qnames.contains(&k), "{k} in {qnames:?}");
        }
        assert_eq!(q["submitted"], 2);
        assert_eq!(q["completed_seen"], 1);
        assert_eq!(q["pending"], 1);
        assert!(lines[0].starts_with("removal: device removed at +"), "{}", lines[0]);
        assert!(lines.iter().any(|l| l.contains("2 submissions, completed 1 as of")), "{lines:?}");
    }
}
