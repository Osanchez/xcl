//! Snapshots of what each pass writes, in an armed frame (`XCL_PASS_STATS=<frame>[,<frame>...]`,
//! telemetry tier `pass`).
//!
//! The title's list records what the current pass writes (render targets, depth, UAVs of a
//! dispatch, the memory ranges of root addresses). The first write of a resource in a pass is
//! copied into an XCL-owned buffer (`pre`), and at the barrier batch that ends the pass each
//! written target is copied again (`post`). The copies and the transitions around them are the
//! only commands added, and only in an armed frame; nothing here runs otherwise. The statistics
//! of the buffers are computed after the list's submission (`gpu::pass_stats`).
//!
//! `XCL_PASS_STATS_MB` (default 1024) is the memory the snapshots may hold; `XCL_PASS_STATS_PRE=0`
//! turns the pre-snapshots off.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::mem::transmute_copy;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use windows::core::Interface;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use super::console_list::Layout;

const MB: u64 = 1024 * 1024;
/// Bytes of a memory range named only by a root address.
const ROOT_RANGE: usize = 65536;
const PC_COPY_BUFFER_REGION: usize = 15;
const PC_COPY_TEXTURE_REGION: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Rt,
    Ds,
    Uav,
    /// a memory range (a buffer UAV or a root address)
    Mem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    Pre,
    Post,
}

#[derive(Clone, Debug)]
pub enum Target {
    Texture { res: usize, kind: Kind, state: D3D12_RESOURCE_STATES },
    Memory { va: usize, bytes: usize, res: usize, unresolved: bool },
}

impl Target {
    fn key(&self) -> usize {
        match *self {
            Target::Texture { res, .. } => res,
            Target::Memory { va, .. } => va,
        }
    }
}

/// One snapshot (or the reason there is none). The buffer's bytes count against the budget until
/// the snapshot is dropped.
pub struct Snap {
    pub pass: u32,
    pub res: usize,
    pub va: usize,
    pub kind: Kind,
    pub when: When,
    pub fmt: u32,
    pub w: u32,
    pub h: u32,
    pub row_pitch: u32,
    pub buf: Option<ID3D12Resource>,
    pub offset: u64,
    pub bytes: u64,
    /// "shape" | "not_uma" | "budget"
    pub skipped: Option<&'static str>,
    pub unresolved: bool,
    /// resources the pass read (post snapshots only)
    pub reads: Vec<usize>,
    charged: u64,
}

impl Drop for Snap {
    fn drop(&mut self) {
        if self.charged != 0 {
            USED.fetch_sub(self.charged, Ordering::Relaxed);
        }
    }
}

/// What one list recorded in the armed frame.
pub struct Rec {
    pub frame: u64,
    pub pass: u32,
    pub written: Vec<Target>,
    pub snapshots: Vec<Snap>,
    pub pre_done: HashSet<usize>,
    /// resources the current pass read through descriptors
    pub reads: Vec<usize>,
}

impl Rec {
    fn new(frame: u64) -> Rec {
        Rec { frame, pass: 0, written: Vec::new(), snapshots: Vec::new(), pre_done: HashSet::new(), reads: Vec::new() }
    }
}

struct Config {
    frames: HashSet<u64>,
    budget: u64,
    pre: bool,
    on: bool,
}

/// `"120,240"` → {120, 240}. A value that is not a frame number is warned about and left out.
pub fn parse_frames(s: &str) -> HashSet<u64> {
    let mut out = HashSet::new();
    for t in s.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        match t.parse::<u64>() {
            Ok(n) => {
                out.insert(n);
            }
            Err(_) => crate::xlog_warn!("pass stats: XCL_PASS_STATS: {t:?} is not a frame number"),
        }
    }
    out
}

fn cfg() -> &'static Config {
    static C: OnceLock<Config> = OnceLock::new();
    C.get_or_init(|| {
        let frames = std::env::var("XCL_PASS_STATS").map(|v| parse_frames(&v)).unwrap_or_default();
        let tier = crate::log::telemetry_on(crate::log::Tier::Pass);
        if !frames.is_empty() && !tier {
            crate::xlog_warn!("pass stats: XCL_PASS_STATS needs telemetry tier pass: off");
        }
        let mb = std::env::var("XCL_PASS_STATS_MB").ok().and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(1024);
        let pre = !std::env::var("XCL_PASS_STATS_PRE").is_ok_and(|v| v.trim() == "0");
        let on = tier && !frames.is_empty();
        Config { frames, budget: mb * MB, pre, on }
    })
}

/// Bytes held by live snapshot buffers.
static USED: AtomicU64 = AtomicU64::new(0);

/// Takes `bytes` from `used` when they fit in `limit`.
fn reserve(used: &AtomicU64, bytes: u64, limit: u64) -> bool {
    used.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |u| u.checked_add(bytes).filter(|&n| n <= limit)).is_ok()
}

static RECS: Mutex<Option<HashMap<usize, Rec>>> = Mutex::new(None);
/// slot (UMA address of a console descriptor) → the resource a UAV was made for
static UAV_SLOTS: Mutex<Option<HashMap<usize, usize>>> = Mutex::new(None);

#[derive(Default)]
struct Totals {
    frame: u64,
    snaps: u64,
    lists: u64,
    bytes: u64,
}
static TOTALS: Mutex<Totals> = Mutex::new(Totals { frame: 0, snaps: 0, lists: 0, bytes: 0 });
/// the armed frame whose summary line is not written yet (0: none)
static PENDING: AtomicU64 = AtomicU64::new(0);

/// Whether the frame being recorded (the present count + 1) is an armed one. One load while the
/// switch is off.
pub fn armed_now() -> bool {
    let c = cfg();
    if !c.on {
        return false;
    }
    let frame = super::present::presents() + 1;
    let p = PENDING.load(Ordering::Relaxed);
    if p != 0 && p != frame && PENDING.compare_exchange(p, 0, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
        let t = TOTALS.lock().unwrap();
        crate::xlog_info!("pass stats: frame {} armed: {} snapshots in {} lists ({:.1} MB)", t.frame, t.snaps, t.lists, t.bytes as f64 / MB as f64);
    }
    c.frames.contains(&frame)
}

fn with_rec<R>(list: *mut c_void, f: impl FnOnce(&mut Rec) -> R) -> R {
    let frame = super::present::presents() + 1;
    let mut g = RECS.lock().unwrap();
    let mut new = false;
    let rec = g.get_or_insert_with(HashMap::new).entry(list as usize).or_insert_with(|| {
        new = true;
        Rec::new(frame)
    });
    if rec.frame != frame {
        *rec = Rec::new(frame);
        new = true;
    }
    if new {
        let mut t = TOTALS.lock().unwrap();
        if t.frame != frame {
            *t = Totals { frame, ..Default::default() };
        }
        t.lists += 1;
        PENDING.store(frame, Ordering::Relaxed);
    }
    f(rec)
}

/// Takes the record of `list` (called at submit).
pub fn take(list: *mut c_void) -> Option<Rec> {
    RECS.lock().unwrap().as_mut()?.remove(&(list as usize))
}

/// What takes the copies: the recording of commands, or a test's list of calls.
trait Sink {
    fn snap(&mut self, t: &Target, pass: u32, when: When, state: D3D12_RESOURCE_STATES) -> Snap;
}

fn empty_snap(t: &Target, pass: u32, when: When) -> Snap {
    let (res, va, kind, bytes, unresolved) = match *t {
        Target::Texture { res, kind, .. } => (res, 0, kind, 0, false),
        Target::Memory { va, bytes, res, unresolved } => (if res != 0 { res } else { va }, va, Kind::Mem, bytes as u64, unresolved),
    };
    Snap { pass, res, va, kind, when, fmt: 0, w: 0, h: 0, row_pitch: 0, buf: None, offset: 0, bytes, skipped: None, unresolved, reads: Vec::new(), charged: 0 }
}

/// A write target is new in the pass: remember it and take its pre-snapshot.
fn add_target(rec: &mut Rec, sink: &mut dyn Sink, t: Target, pre: bool) {
    if rec.written.iter().any(|w| w.key() == t.key()) {
        return;
    }
    if pre && rec.pre_done.insert(t.key()) {
        let state = match t {
            Target::Texture { state, .. } => state,
            Target::Memory { .. } => D3D12_RESOURCE_STATE_COMMON,
        };
        let s = sink.snap(&t, rec.pass, When::Pre, state);
        rec.snapshots.push(s);
    }
    rec.written.push(t);
}

/// The state the title's barrier batch moves `res` from.
fn before_state(res: usize, barriers: &[[u32; 8]]) -> Option<D3D12_RESOURCE_STATES> {
    barriers.iter().find(|b| b[0] == 0 && (b[2] as u64 | (b[3] as u64) << 32) as usize == res).map(|b| D3D12_RESOURCE_STATES(b[5] as i32))
}

/// The pass that ended with `barriers`: post-snapshot each target, then the next pass.
fn end_pass(rec: &mut Rec, sink: &mut dyn Sink, barriers: &[[u32; 8]]) {
    if rec.written.is_empty() {
        return;
    }
    let reads = std::mem::take(&mut rec.reads);
    for t in std::mem::take(&mut rec.written) {
        let state = match t {
            Target::Texture { res, state, .. } => before_state(res, barriers).unwrap_or(state),
            Target::Memory { .. } => D3D12_RESOURCE_STATE_COMMON,
        };
        let mut s = sink.snap(&t, rec.pass, When::Post, state);
        s.reads = reads.clone();
        rec.snapshots.push(s);
    }
    rec.pass += 1;
    rec.pre_done.clear();
}

struct ListSink(*mut c_void);

impl Sink for ListSink {
    fn snap(&mut self, t: &Target, pass: u32, when: When, state: D3D12_RESOURCE_STATES) -> Snap {
        let mut s = empty_snap(t, pass, when);
        // SAFETY: the resources are the title's (borrowed for the call); the list is the one the
        // hooks run on, and the copies go through its own methods (`pc`), not the title's.
        unsafe {
            match *t {
                Target::Texture { res, .. } => snap_texture(self.0, &mut s, res, state),
                Target::Memory { va, bytes, .. } => snap_memory(self.0, &mut s, va, bytes),
            }
        }
        if s.buf.is_some() {
            let mut t = TOTALS.lock().unwrap();
            t.snaps += 1;
            t.bytes += s.charged;
        }
        s
    }
}

/// a PC method of the list (its runtime's own)
unsafe fn pc<F: Copy>(list: *mut c_void, slot: usize) -> F {
    transmute_copy(&super::real::orig_fn(list, slot))
}

/// A default-heap buffer of the snapshot pool (None: over budget or the call failed).
unsafe fn pool_buffer(dev: &ID3D12Device, bytes: u64) -> Option<ID3D12Resource> {
    let mut r: Option<ID3D12Resource> = None;
    dev.CreateCommittedResource(
        &D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_DEFAULT, ..Default::default() },
        D3D12_HEAP_FLAG_NONE,
        &D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: bytes,
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: D3D12_RESOURCE_FLAG_NONE,
            ..Default::default()
        },
        D3D12_RESOURCE_STATE_COMMON,
        None,
        &mut r,
    )
    .ok()?;
    r
}

unsafe fn snap_texture(list: *mut c_void, s: &mut Snap, res: usize, state: D3D12_RESOURCE_STATES) {
    let raw = res as *mut c_void;
    let dev_raw = super::real::device_raw();
    let (Some(r), Some(dev)) = (ID3D12Resource::from_raw_borrowed(&raw), ID3D12Device::from_raw_borrowed(&dev_raw)) else { return };
    let d = r.GetDesc();
    s.fmt = d.Format.0 as u32;
    s.w = d.Width as u32;
    s.h = d.Height;
    if d.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D || d.SampleDesc.Count > 1 || d.DepthOrArraySize > 1 {
        s.skipped = Some("shape");
        return;
    }
    let mut fp = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
    let mut total = 0u64;
    dev.GetCopyableFootprints(&d, 0, 1, 0, Some(&mut fp), None, None, Some(&mut total));
    s.row_pitch = fp.Footprint.RowPitch;
    s.bytes = total;
    if !reserve(&USED, total, cfg().budget) {
        s.skipped = Some("budget");
        return;
    }
    let Some(buf) = pool_buffer(dev, total) else {
        USED.fetch_sub(total, Ordering::Relaxed);
        return;
    };
    s.charged = total;
    let dst = D3D12_TEXTURE_COPY_LOCATION {
        pResource: transmute_copy(&buf),
        Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { PlacedFootprint: fp },
    };
    let src = D3D12_TEXTURE_COPY_LOCATION {
        pResource: transmute_copy(r),
        Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { SubresourceIndex: 0 },
    };
    super::list_x::transition(list, res, state, D3D12_RESOURCE_STATE_COPY_SOURCE);
    pc::<unsafe extern "system" fn(*mut c_void, *const D3D12_TEXTURE_COPY_LOCATION, u32, u32, u32, *const D3D12_TEXTURE_COPY_LOCATION, *const D3D12_BOX)>(list, PC_COPY_TEXTURE_REGION)(
        list, &dst, 0, 0, 0, &src, std::ptr::null(),
    );
    super::list_x::transition(list, res, D3D12_RESOURCE_STATE_COPY_SOURCE, state);
    s.buf = Some(buf);
    // the locations hold borrowed pointers (ManuallyDrop fields): nothing to release
}

unsafe fn snap_memory(list: *mut c_void, s: &mut Snap, va: usize, bytes: usize) {
    let Some((w, off)) = super::real::uma_window_buffer(va, bytes) else {
        s.skipped = Some("not_uma");
        return;
    };
    let dev_raw = super::real::device_raw();
    let Some(dev) = ID3D12Device::from_raw_borrowed(&dev_raw) else { return };
    if !reserve(&USED, bytes as u64, cfg().budget) {
        s.skipped = Some("budget");
        return;
    }
    let Some(buf) = pool_buffer(dev, bytes as u64) else {
        USED.fetch_sub(bytes as u64, Ordering::Relaxed);
        return;
    };
    s.charged = bytes as u64;
    s.offset = off;
    super::list_x::transition(list, w, D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_SOURCE);
    pc::<unsafe extern "system" fn(*mut c_void, usize, u64, usize, u64, u64)>(list, PC_COPY_BUFFER_REGION)(list, transmute_copy(&buf), 0, w, off, bytes as u64);
    super::list_x::transition(list, w, D3D12_RESOURCE_STATE_COPY_SOURCE, D3D12_RESOURCE_STATE_COMMON);
    s.buf = Some(buf);
}

/// OMSetRenderTargets on `list` (handles as the PC list got them).
pub fn note_targets(list: *mut c_void, rtvs: &[usize], dsv: Option<usize>) {
    if !armed_now() {
        return;
    }
    let mut ts: Vec<Target> = rtvs
        .iter()
        .filter_map(|&h| super::present::rtv_resource(h))
        .map(|res| Target::Texture { res, kind: Kind::Rt, state: D3D12_RESOURCE_STATE_RENDER_TARGET })
        .collect();
    if let Some(res) = dsv.and_then(super::present::dsv_resource) {
        ts.push(Target::Texture { res, kind: Kind::Ds, state: D3D12_RESOURCE_STATE_DEPTH_WRITE });
    }
    add_targets(list, ts);
}

fn add_targets(list: *mut c_void, ts: Vec<Target>) {
    let pre = cfg().pre;
    with_rec(list, |rec| {
        let mut sink = ListSink(list);
        for t in ts {
            add_target(rec, &mut sink, t, pre);
        }
    });
}

/// A UAV was created in console slot `slot` (the UMA address of its descriptor) for `res`.
pub fn note_uav_view(slot: usize, res: usize) {
    if !cfg().on {
        return;
    }
    UAV_SLOTS.lock().unwrap().get_or_insert_with(HashMap::new).insert(slot, res);
}

/// A dispatch on the console list `this` (its layout and root arguments, as `bind` keeps them).
pub unsafe fn note_dispatch(this: *mut c_void) {
    if !armed_now() {
        return;
    }
    if let Some(r) = super::bind::resolve_descriptors(this) {
        note_resolved(this, r);
    }
}

/// A dispatch replayed from a console list stream into `list`, with the arguments of its table.
pub unsafe fn note_dispatch_args(list: *mut c_void, layout: Option<&Layout>, args: &[u8]) {
    if !armed_now() {
        return;
    }
    if let Some(r) = layout.and_then(|l| super::bind::resolve_args(l, args)) {
        note_resolved(list, r);
    }
}

unsafe fn note_resolved(list: *mut c_void, r: super::bind::Resolved) {
    let mut ts: Vec<Target> = Vec::new();
    let mut reads: Vec<usize> = Vec::new();
    {
        let g = UAV_SLOTS.lock().unwrap();
        for &(_, slot, res) in &r.found {
            if g.as_ref().is_some_and(|m| m.contains_key(&slot)) {
                let raw = res as *mut c_void;
                let Some(d) = ID3D12Resource::from_raw_borrowed(&raw).map(|x| x.GetDesc()) else { continue };
                if d.Dimension == D3D12_RESOURCE_DIMENSION_BUFFER {
                    if let Some(va) = super::real::title_address(res) {
                        ts.push(Target::Memory { va, bytes: d.Width as usize, res, unresolved: false });
                    }
                } else {
                    ts.push(Target::Texture { res, kind: Kind::Uav, state: D3D12_RESOURCE_STATE_UNORDERED_ACCESS });
                }
            } else {
                reads.push(res);
            }
        }
    }
    for &va in r.vas.iter().filter(|&&v| v != 0) {
        ts.push(Target::Memory { va, bytes: ROOT_RANGE, res: 0, unresolved: true });
    }
    with_rec(list, |rec| {
        for x in reads {
            if !rec.reads.contains(&x) {
                rec.reads.push(x);
            }
        }
    });
    add_targets(list, ts);
}

/// A barrier batch (the title's, before XCL translates it) is about to be recorded into `list`.
pub unsafe fn boundary(list: *mut c_void, barriers: &[[u32; 8]]) {
    if !armed_now() {
        return;
    }
    with_rec(list, |rec| end_pass(rec, &mut ListSink(list), barriers));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Calls(Vec<(usize, When, i32, u32)>);

    impl Sink for Calls {
        fn snap(&mut self, t: &Target, pass: u32, when: When, state: D3D12_RESOURCE_STATES) -> Snap {
            self.0.push((t.key(), when, state.0, pass));
            empty_snap(t, pass, when)
        }
    }

    fn rt(res: usize) -> Target {
        Target::Texture { res, kind: Kind::Rt, state: D3D12_RESOURCE_STATE_RENDER_TARGET }
    }

    #[test]
    fn armed_frames_parse() {
        assert_eq!(parse_frames("120,240"), HashSet::from([120, 240]));
        assert_eq!(parse_frames(" 7 "), HashSet::from([7]));
        assert!(parse_frames("garbage").is_empty());
        assert!(parse_frames("").is_empty());
    }

    #[test]
    fn boundary_moves_targets_to_snapshots_and_advances_pass() {
        let mut rec = Rec::new(120);
        let mut c = Calls::default();
        add_target(&mut rec, &mut c, rt(0x1000), true);
        add_target(&mut rec, &mut c, Target::Memory { va: 0x2000, bytes: 64, res: 0, unresolved: true }, true);
        assert_eq!(rec.written.len(), 2);
        end_pass(&mut rec, &mut c, &[]);
        assert!(rec.written.is_empty() && rec.pre_done.is_empty());
        assert_eq!(rec.pass, 1);
        assert_eq!(rec.snapshots.len(), 4);
        assert_eq!(rec.snapshots.iter().filter(|s| s.when == When::Post && s.pass == 0).count(), 2);
        assert!(rec.snapshots.iter().any(|s| s.unresolved && s.kind == Kind::Mem));
        // a boundary with nothing written does not advance the pass
        end_pass(&mut rec, &mut c, &[]);
        assert_eq!(rec.pass, 1);
    }

    #[test]
    fn pre_snapshot_once_per_resource_per_pass() {
        let mut rec = Rec::new(1);
        let mut c = Calls::default();
        add_target(&mut rec, &mut c, rt(0x1000), true);
        add_target(&mut rec, &mut c, rt(0x1000), true);
        assert_eq!(c.0.iter().filter(|x| x.1 == When::Pre).count(), 1);
        end_pass(&mut rec, &mut c, &[]);
        add_target(&mut rec, &mut c, rt(0x1000), true);
        assert_eq!(c.0.iter().filter(|x| x.1 == When::Pre).count(), 2);
        // XCL_PASS_STATS_PRE=0
        let mut rec = Rec::new(1);
        let mut c = Calls::default();
        add_target(&mut rec, &mut c, rt(0x1000), false);
        assert!(c.0.is_empty());
        assert_eq!(rec.written.len(), 1);
    }

    #[test]
    fn state_from_barrier_batch_wins() {
        let mut rec = Rec::new(1);
        let mut c = Calls::default();
        add_target(&mut rec, &mut c, rt(0x1000), true);
        // transition of 0x1000: before = UNORDERED_ACCESS (0x8), after = 0x80
        let batch = [[0u32, 0, 0x1000, 0, 0xFFFF_FFFF, 0x8, 0x80, 0]];
        end_pass(&mut rec, &mut c, &batch);
        let post = c.0.iter().find(|x| x.1 == When::Post).unwrap();
        assert_eq!(post.2, 0x8);
        // a resource not in the batch keeps the state it was noted with
        add_target(&mut rec, &mut c, rt(0x3000), true);
        end_pass(&mut rec, &mut c, &batch);
        let post = c.0.iter().rev().find(|x| x.1 == When::Post).unwrap();
        assert_eq!(post.2, D3D12_RESOURCE_STATE_RENDER_TARGET.0);
    }

    #[test]
    fn budget_marks_skipped() {
        let used = AtomicU64::new(0);
        assert!(reserve(&used, 600, 1000));
        assert!(!reserve(&used, 600, 1000));
        assert_eq!(used.load(Ordering::Relaxed), 600);
        assert!(reserve(&used, 400, 1000));
        assert!(!reserve(&used, 1, 1000));
        // an overflowing request is refused, not wrapped
        assert!(!reserve(&used, u64::MAX, u64::MAX - 1));
    }
}
