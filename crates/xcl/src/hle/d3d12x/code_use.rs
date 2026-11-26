//! Driver code of the pipelines in use (`XCL_CODE_IN_USE=1`, or telemetry tier `frame` and up).
//!
//! NVIDIA drivers limit the code of the pipelines a process has in use (upstream #2403). A
//! driver pipeline object is *in use* from the first time it is set on a PC command list until
//! it is released; its size is the byte length of its `GetCachedBlob()`. XCL counts that sum,
//! writes it in a `code` event each frame, warns at 75, 90 and 100 % of `XCL_CODE_LIMIT_MB`
//! (default 448) and gives `removal_line()` for the device-removal report. Pipelines are
//! measured on their first bind, so the ones never bound cost nothing.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Instant;
use windows::core::Interface;
use windows::Win32::Graphics::Direct3D12::ID3D12PipelineState;

const NOT_MEASURED: u64 = u64::MAX;
const MB: u64 = 1024 * 1024;
/// A pipeline counts as recently bound when its last bind was at most this many frames ago.
const RECENT_FRAMES: u64 = 120;
/// Pipelines named in the removal line.
const LARGEST: usize = 8;

pub fn on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("XCL_CODE_IN_USE").is_some_and(|v| v == "1") || crate::log::telemetry_on(crate::log::Tier::Frame))
}

/// The wall in blob bytes: `XCL_CODE_LIMIT_MB` (448).
fn limit_bytes() -> u64 {
    static LIMIT: OnceLock<u64> = OnceLock::new();
    *LIMIT.get_or_init(|| std::env::var("XCL_CODE_LIMIT_MB").ok().and_then(|v| v.trim().parse::<u64>().ok()).filter(|&n| n > 0).unwrap_or(448) * MB)
}

struct Entry {
    /// `GetCachedBlob` size, `NOT_MEASURED` before the first bind
    bytes: AtomicU64,
    /// 0 unknown, 1 compute, 2 graphics
    stage: AtomicU8,
    label: Mutex<Option<String>>,
    /// frame of the first bind, 0 = never bound
    first_frame: AtomicU64,
    last_frame: AtomicU64,
}

impl Entry {
    fn new(stage: u8) -> Entry {
        Entry { bytes: AtomicU64::new(NOT_MEASURED), stage: AtomicU8::new(stage), label: Mutex::new(None), first_frame: AtomicU64::new(0), last_frame: AtomicU64::new(0) }
    }
    fn size(&self) -> u64 {
        match self.bytes.load(Ordering::Relaxed) {
            NOT_MEASURED => 0,
            b => b,
        }
    }
}

#[derive(Default)]
struct State {
    map: RwLock<HashMap<usize, Arc<Entry>>>,
    in_use_bytes: AtomicU64,
    in_use_count: AtomicU64,
    in_use_compute: AtomicU64,
    in_use_graphics: AtomicU64,
    peak_bytes: AtomicU64,
    peak_frame: AtomicU64,
    new_frame_bytes: AtomicU64,
    released_frame_bytes: AtomicU64,
    measure_us: AtomicU64,
    recent_bytes: AtomicU64,
    /// bit 0: 75 %, bit 1: 90 %, bit 2: 100 % warned
    warned: AtomicU32,
}

fn sub(a: &AtomicU64, n: u64) {
    let _ = a.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| Some(v.saturating_sub(n)));
}

impl State {
    fn entry(&self, pso: usize) -> Arc<Entry> {
        if let Some(e) = self.map.read().unwrap().get(&pso) {
            return e.clone();
        }
        self.map.write().unwrap().entry(pso).or_insert_with(|| Arc::new(Entry::new(0))).clone()
    }

    fn stage_total(&self, stage: u8) -> Option<&AtomicU64> {
        match stage {
            1 => Some(&self.in_use_compute),
            2 => Some(&self.in_use_graphics),
            _ => None,
        }
    }

    fn note_label(&self, pso: usize, shaders: &[(&str, Option<&[u8]>)]) {
        let text = label_text(shaders);
        let stage = if shaders.iter().any(|(k, _)| *k == "cs") { 1 } else { 2 };
        let e = self.entry(pso);
        *e.label.lock().unwrap() = Some(text);
        let old = e.stage.swap(stage, Ordering::Relaxed);
        // labelled after its first bind: its bytes were not in a stage total yet
        if old == 0 && e.first_frame.load(Ordering::Relaxed) != 0 {
            if let Some(t) = self.stage_total(stage) {
                t.fetch_add(e.size(), Ordering::Relaxed);
            }
        }
    }

    /// `pso` is set on a list in frame `f`; `measure` gives its blob size on its first use.
    fn bound_with(&self, pso: usize, f: u64, measure: impl Fn(usize) -> u64) {
        let e = self.entry(pso);
        if e.bytes.load(Ordering::Relaxed) == NOT_MEASURED {
            let t = Instant::now();
            let b = measure(pso);
            self.measure_us.fetch_add(t.elapsed().as_micros() as u64, Ordering::Relaxed);
            e.bytes.store(b, Ordering::Relaxed);
        }
        if e.first_frame.compare_exchange(0, f.max(1), Ordering::Relaxed, Ordering::Relaxed).is_ok() {
            let b = e.size();
            let prev = self.in_use_bytes.fetch_add(b, Ordering::Relaxed);
            self.in_use_count.fetch_add(1, Ordering::Relaxed);
            if let Some(t) = self.stage_total(e.stage.load(Ordering::Relaxed)) {
                t.fetch_add(b, Ordering::Relaxed);
            }
            self.new_frame_bytes.fetch_add(b, Ordering::Relaxed);
            let now = prev + b;
            if self.peak_bytes.fetch_max(now, Ordering::Relaxed) < now {
                self.peak_frame.store(f, Ordering::Relaxed);
            }
            self.warn_threshold(prev, now, self.in_use_count.load(Ordering::Relaxed));
        }
        e.last_frame.store(f, Ordering::Relaxed);
    }

    fn released(&self, pso: usize) {
        let Some(e) = self.map.write().unwrap().remove(&pso) else { return };
        if e.first_frame.load(Ordering::Relaxed) != 0 {
            let b = e.size();
            sub(&self.in_use_bytes, b);
            sub(&self.in_use_count, 1);
            if let Some(t) = self.stage_total(e.stage.load(Ordering::Relaxed)) {
                sub(t, b);
            }
            self.released_frame_bytes.fetch_add(b, Ordering::Relaxed);
        }
    }

    /// The bytes of the entries bound within the last `RECENT_FRAMES` frames before `n`.
    fn recent(&self, n: u64) -> u64 {
        self.map.read().unwrap().values().filter(|e| e.first_frame.load(Ordering::Relaxed) != 0 && e.last_frame.load(Ordering::Relaxed) + RECENT_FRAMES >= n).map(|e| e.size()).sum()
    }

    fn warn_threshold(&self, prev: u64, now: u64, count: u64) {
        let limit = limit_bytes();
        for t in thresholds_crossed(prev, now, limit) {
            let bit = match t {
                75 => 1,
                90 => 2,
                _ => 4,
            };
            if self.warned.fetch_or(bit, Ordering::Relaxed) & bit == 0 {
                crate::xlog_warn!("D3D12.X: driver code of pipelines in use: {} MB ({} % of {} MB) in {} pipelines", now / MB, pct(now, limit), limit / MB, count);
            }
        }
    }

    /// The used entries, as `(label, bytes, first_frame)`.
    fn used(&self) -> Vec<Largest> {
        self.map
            .read()
            .unwrap()
            .iter()
            .filter(|(_, e)| e.first_frame.load(Ordering::Relaxed) != 0)
            .map(|(p, e)| Largest {
                label: e.label.lock().unwrap().clone().unwrap_or_else(|| format!("pipeline {p:#x}")),
                bytes: e.size(),
                first_frame: e.first_frame.load(Ordering::Relaxed),
            })
            .collect()
    }

    fn removal_line_with(&self, on: bool, limit: u64, now: u64) -> Option<(String, Vec<(&'static str, serde_json::Value)>)> {
        if !on {
            return None;
        }
        let (in_use, count) = (self.in_use_bytes.load(Ordering::Relaxed), self.in_use_count.load(Ordering::Relaxed));
        let (c, g) = (self.in_use_compute.load(Ordering::Relaxed), self.in_use_graphics.load(Ordering::Relaxed));
        let (peak, pf) = (self.peak_bytes.load(Ordering::Relaxed), self.peak_frame.load(Ordering::Relaxed));
        let recent = self.recent(now);
        let top = largest(self.used(), LARGEST);
        let names = top.iter().map(|l| format!("{} {:.1} MB", l.label, l.bytes as f64 / MB as f64)).collect::<Vec<_>>().join(", ");
        let text = format!(
            "driver code in use at removal: {} MB in {count} pipelines (compute {} MB, graphics {} MB), bound in the last {RECENT_FRAMES} frames {} MB, peak {} MB at frame {pf}, limit {} MB; largest: {names}",
            in_use / MB, c / MB, g / MB, recent / MB, peak / MB, limit / MB
        );
        let fields = vec![
            ("code_in_use_bytes", in_use.into()),
            ("code_in_use_count", count.into()),
            ("code_compute_bytes", c.into()),
            ("code_graphics_bytes", g.into()),
            ("code_recent_bytes", recent.into()),
            ("code_peak_bytes", peak.into()),
            ("code_peak_frame", pf.into()),
            ("code_limit_bytes", limit.into()),
            ("code_largest", top.iter().map(|l| serde_json::json!({"label": l.label, "bytes": l.bytes, "first_frame": l.first_frame})).collect::<Vec<_>>().into()),
        ];
        Some((text, fields))
    }
}

fn state() -> &'static State {
    static STATE: OnceLock<State> = OnceLock::new();
    STATE.get_or_init(State::default)
}

/// The same text `dred::label` builds: `"cs 0123456789abcdef"`, `"vs … ps …"`.
fn label_text(shaders: &[(&str, Option<&[u8]>)]) -> String {
    shaders.iter().filter_map(|(k, b)| b.map(|b| format!("{k} {:016x}", crate::gpu::shader::fnv64(b)))).collect::<Vec<_>>().join(" ")
}

pub struct Largest {
    pub label: String,
    pub bytes: u64,
    pub first_frame: u64,
}

/// The `n` largest entries, biggest first.
fn largest(mut entries: Vec<Largest>, n: usize) -> Vec<Largest> {
    entries.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    entries.truncate(n);
    entries
}

fn pct(bytes: u64, limit: u64) -> u32 {
    if limit == 0 {
        return 0;
    }
    (bytes as u128 * 100 / limit as u128).min(u32::MAX as u128) as u32
}

/// Which of 75, 90 and 100 % of `limit` the sum passed going from `prev` to `now`.
fn thresholds_crossed(prev: u64, now: u64, limit: u64) -> Vec<u32> {
    [75u32, 90, 100].into_iter().filter(|&t| (prev as u128) * 100 < t as u128 * limit as u128 && (now as u128) * 100 >= t as u128 * limit as u128).collect()
}

/// The blob size of a PC pipeline; 0 when the driver gives none.
unsafe fn measure(pso: usize) -> u64 {
    let p = pso as *mut c_void;
    match ID3D12PipelineState::from_raw_borrowed(&p).and_then(|o| o.GetCachedBlob().ok()) {
        Some(blob) => blob.GetBufferSize() as u64,
        None => {
            MEASURE_FAILED.fetch_add(1, Ordering::Relaxed);
            0
        }
    }
}

static MEASURE_FAILED: AtomicU64 = AtomicU64::new(0);

thread_local! {
    /// `(pipeline, frame)` last counted on this thread, by `(pso >> 4) & 63`
    static SEEN: std::cell::RefCell<[(usize, u64); 64]> = const { std::cell::RefCell::new([(0, 0); 64]) };
}

/// A pipeline named by its console shaders, `shaders` as for `dred::label`.
pub fn note_label(pso: usize, shaders: &[(&str, Option<&[u8]>)]) {
    if on() && pso != 0 {
        state().note_label(pso, shaders);
    }
}

/// `pso` (a driver pipeline) is set on a PC command list.
pub fn bound(pso: usize) {
    if pso == 0 || !on() {
        return;
    }
    let f = super::present::presents();
    let i = (pso >> 4) & 63;
    let seen = SEEN.with(|s| {
        let mut s = s.borrow_mut();
        let hit = s[i] == (pso, f);
        s[i] = (pso, f);
        hit
    });
    if !seen {
        state().bound_with(pso, f, |p| unsafe { measure(p) });
    }
}

/// `pso` was released.
pub fn released(pso: usize) {
    if on() && pso != 0 {
        // its address may be another pipeline's next, in this same frame
        SEEN.with(|s| s.borrow_mut()[(pso >> 4) & 63] = (0, 0));
        state().released(pso);
    }
}

/// Frame `n` was shown: the `code` event, and the per-frame totals start again.
pub fn frame(n: u64) {
    if !on() {
        return;
    }
    let s = state();
    if n % 30 == 0 {
        s.recent_bytes.store(s.recent(n), Ordering::Relaxed);
    }
    let get = |a: &AtomicU64| a.load(Ordering::Relaxed);
    let fields: Vec<(&str, serde_json::Value)> = vec![
        ("frame", n.into()),
        ("in_use_bytes", get(&s.in_use_bytes).into()),
        ("in_use_count", get(&s.in_use_count).into()),
        ("in_use_compute_bytes", get(&s.in_use_compute).into()),
        ("in_use_graphics_bytes", get(&s.in_use_graphics).into()),
        ("new_bytes", s.new_frame_bytes.swap(0, Ordering::Relaxed).into()),
        ("released_bytes", s.released_frame_bytes.swap(0, Ordering::Relaxed).into()),
        ("recent_bytes", get(&s.recent_bytes).into()),
        ("peak_bytes", get(&s.peak_bytes).into()),
        ("peak_frame", get(&s.peak_frame).into()),
        ("limit_bytes", limit_bytes().into()),
        ("measure_ms", (get(&s.measure_us) as f64 / 1000.0).into()),
        ("measure_failed", MEASURE_FAILED.load(Ordering::Relaxed).into()),
    ];
    crate::log::event(crate::log::Tier::Frame, "code", &fields);
}

/// For the device-removal report: the line and its telemetry fields; `None` when off.
pub fn removal_line() -> Option<(String, Vec<(&'static str, serde_json::Value)>)> {
    state().removal_line_with(on(), limit_bytes(), super::present::presents())
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: u64 = 1000 * MB;

    fn bound_once(s: &State, pso: usize, f: u64, bytes: u64) {
        s.bound_with(pso, f, |_| bytes);
    }

    #[test]
    fn first_bind_counts_once_per_pointer() {
        let s = State::default();
        bound_once(&s, 0x1000, 1, 10 * MB);
        bound_once(&s, 0x1000, 1, 10 * MB);
        bound_once(&s, 0x1000, 2, 10 * MB);
        std::thread::scope(|t| {
            t.spawn(|| bound_once(&s, 0x1000, 3, 10 * MB));
        });
        assert_eq!(s.in_use_bytes.load(Ordering::Relaxed), 10 * MB);
        assert_eq!(s.in_use_count.load(Ordering::Relaxed), 1);
        assert_eq!(s.new_frame_bytes.load(Ordering::Relaxed), 10 * MB);
    }

    #[test]
    fn release_subtracts_only_used_entries() {
        let s = State::default();
        s.note_label(0x2000, &[("cs", Some(b"abc"))]);
        s.released(0x2000);
        assert_eq!(s.in_use_bytes.load(Ordering::Relaxed), 0);
        assert_eq!(s.in_use_count.load(Ordering::Relaxed), 0);
        assert_eq!(s.released_frame_bytes.load(Ordering::Relaxed), 0);
        s.note_label(0x2000, &[("cs", Some(b"abc"))]);
        bound_once(&s, 0x2000, 1, 5 * MB);
        assert_eq!(s.in_use_compute.load(Ordering::Relaxed), 5 * MB);
        s.released(0x2000);
        for a in [&s.in_use_bytes, &s.in_use_count, &s.in_use_compute, &s.in_use_graphics] {
            assert_eq!(a.load(Ordering::Relaxed), 0);
        }
        assert_eq!(s.released_frame_bytes.load(Ordering::Relaxed), 5 * MB);
    }

    #[test]
    fn shared_pointer_counts_once() {
        let s = State::default();
        s.note_label(0x3000, &[("cs", Some(b"one"))]);
        s.note_label(0x3000, &[("cs", Some(b"two"))]);
        bound_once(&s, 0x3000, 1, 7 * MB);
        assert_eq!(s.in_use_count.load(Ordering::Relaxed), 1);
        assert_eq!(s.in_use_bytes.load(Ordering::Relaxed), 7 * MB);
    }

    #[test]
    fn thresholds_fire_once_each() {
        let eighty = L / 10 * 8;
        assert_eq!(thresholds_crossed(0, eighty, L), vec![75]);
        assert_eq!(thresholds_crossed(eighty, L, L), vec![90, 100]);
        assert!(thresholds_crossed(L, L, L).is_empty());
        assert_eq!(pct(eighty, L), 80);
    }

    #[test]
    fn largest_orders_by_bytes_and_caps() {
        let v = (0..10u64).map(|i| Largest { label: format!("p{i}"), bytes: (i * 7 % 10) * MB, first_frame: 1 }).collect();
        let top = largest(v, 8);
        assert_eq!(top.len(), 8);
        assert!(top.windows(2).all(|w| w[0].bytes >= w[1].bytes));
        assert_eq!(top[0].bytes, 9 * MB);
        assert_eq!(top[7].bytes, 2 * MB);
    }

    #[test]
    fn removal_line_none_when_off() {
        assert!(State::default().removal_line_with(false, L, 1).is_none());
    }

    #[test]
    fn removal_line_names_unlabelled_by_pointer() {
        let s = State::default();
        bound_once(&s, 0x4000, 1, 3 * MB);
        let (text, fields) = s.removal_line_with(true, L, 1).unwrap();
        assert!(text.contains("pipeline 0x4000 3.0 MB"), "{text}");
        assert!(text.contains("in 1 pipelines"), "{text}");
        let find = |k: &str| fields.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
        assert_eq!(find("code_in_use_bytes"), Some(serde_json::json!(3 * MB)));
        assert_eq!(find("code_largest").unwrap()[0]["label"], "pipeline 0x4000");
    }

    #[test]
    fn gpu_measure_reads_cached_blob_size() {
        use crate::gpu::recomp::{dxc, pso, runner};
        use windows::Win32::Graphics::Direct3D12::*;
        let Ok(dev) = (unsafe { crate::gpu::agility::create_device(None, windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_11_0) }) else {
            runner::skip_test("no GPU");
            return;
        };
        let Ok(dxil) = dxc::compile("[numthreads(1, 1, 1)] void main() {}", "main", "cs_6_6", &[]) else {
            runner::skip_test("no DXC");
            return;
        };
        let rs = pso::dispatch_chain_root_signature(&dev, &[]).expect("root signature");
        let state: ID3D12PipelineState = unsafe {
            dev.CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
                pRootSignature: std::mem::transmute_copy(&rs),
                CS: D3D12_SHADER_BYTECODE { pShaderBytecode: dxil.as_ptr().cast(), BytecodeLength: dxil.len() },
                ..Default::default()
            })
        }
        .expect("compute pipeline");
        assert!(unsafe { measure(state.as_raw() as usize) } > 0);
    }
}
