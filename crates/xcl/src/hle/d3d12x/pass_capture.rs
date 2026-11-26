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
//! turns the pre-snapshots off. `XCL_PASS_DUMP=1` also writes the post snapshots of the common
//! colour and depth formats as BMP files (at most 64 per armed frame) under `<run folder>/pass/`.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::mem::{transmute_copy, ManuallyDrop};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;
use windows::core::{Interface, GUID};
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use super::console_list::Layout;
use crate::gpu::pass_stats::{self, class_of, combine, groups_for, Consts, FormatClass, Partial, Stats, GROUP_ELEMENTS, MAX_GROUPS, PARTIAL_WORDS};

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
    dump: bool,
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
        let dump = std::env::var("XCL_PASS_DUMP").is_ok_and(|v| v.trim() == "1");
        let on = tier && !frames.is_empty();
        Config { frames, budget: mb * MB, pre, dump, on }
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

/// The list was Reset: what it recorded and did not submit is dropped (its buffers are freed).
pub fn list_reset(list: *mut c_void) {
    if !cfg().on {
        return;
    }
    let old = RECS.lock().unwrap().as_mut().and_then(|m| m.remove(&(list as usize)));
    drop(old);
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

// ---------------------------------------------------------------------------------------
// the submit side: statistics of the snapshots of the lists just submitted
// ---------------------------------------------------------------------------------------

/// How long the thread waits for the statistics list before it gives the job up.
const JOB_TIMEOUT: Duration = Duration::from_secs(60);
/// Post snapshots written as images per armed frame.
const DUMPS_PER_FRAME: u32 = 64;
/// Longest side of an image, in pixels.
const DUMP_SIDE: u32 = 1024;
/// Command allocators kept for the next jobs.
const ALLOCS_MAX: usize = 8;

/// The PC objects the statistics list is made and submitted with, through their own (original)
/// functions so that nothing of ours is adopted. `get` finds a function of an object's vtable.
#[derive(Clone, Copy)]
struct Drv {
    dev: *mut c_void,
    queue: *mut c_void,
    get: unsafe fn(*mut c_void, usize) -> usize,
}

// SAFETY: the pointers are COM objects (free-threaded); `get` only reads a vtable.
unsafe impl Send for Drv {}
unsafe impl Sync for Drv {}

unsafe fn orig_get(obj: *mut c_void, slot: usize) -> usize {
    super::real::orig_fn(obj, slot)
}

#[cfg(test)]
unsafe fn plain_get(obj: *mut c_void, slot: usize) -> usize {
    *(*(obj as *const *const usize)).add(slot)
}

fn method_slot(iface: &str, method: &str) -> Option<usize> {
    super::layout::slot(super::layout::find(iface)?, method)
}

impl Drv {
    fn orig(queue: *mut c_void) -> Option<Drv> {
        let dev = super::real::device_raw();
        (!dev.is_null() && !queue.is_null()).then_some(Drv { dev, queue, get: orig_get })
    }

    unsafe fn dev_fn(&self, method: &str) -> Option<usize> {
        Some((self.get)(self.dev, method_slot("ID3D12Device", method)?))
    }

    unsafe fn queue_fn(&self, method: &str) -> Option<usize> {
        Some((self.get)(self.queue, method_slot("ID3D12CommandQueue", method)?))
    }

    unsafe fn make<T: Interface>(call: impl FnOnce(*const GUID, *mut *mut c_void) -> i32) -> Option<T> {
        let mut out: *mut c_void = std::ptr::null_mut();
        let hr = call(&T::IID, &mut out);
        if hr < 0 || out.is_null() {
            return None;
        }
        Some(T::from_raw(out))
    }

    /// The command list type of the queue.
    unsafe fn queue_type(&self) -> Option<D3D12_COMMAND_LIST_TYPE> {
        let mut d = D3D12_COMMAND_QUEUE_DESC::default();
        let f: unsafe extern "system" fn(*mut c_void, *mut D3D12_COMMAND_QUEUE_DESC) -> *mut D3D12_COMMAND_QUEUE_DESC = transmute_copy(&self.queue_fn("GetDesc")?);
        f(self.queue, &mut d);
        Some(d.Type)
    }

    unsafe fn buffer(&self, bytes: u64, heap: D3D12_HEAP_TYPE, flags: D3D12_RESOURCE_FLAGS, state: D3D12_RESOURCE_STATES) -> Option<ID3D12Resource> {
        type F = unsafe extern "system" fn(
            *mut c_void, *const D3D12_HEAP_PROPERTIES, i32, *const D3D12_RESOURCE_DESC, i32, *const c_void, *const GUID, *mut *mut c_void,
        ) -> i32;
        let props = D3D12_HEAP_PROPERTIES { Type: heap, ..Default::default() };
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: bytes.max(256),
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            Flags: flags,
            ..Default::default()
        };
        let f: F = transmute_copy(&self.dev_fn("CreateCommittedResource")?);
        Self::make(|iid, out| f(self.dev, &props, D3D12_HEAP_FLAG_NONE.0, &desc, state.0, std::ptr::null(), iid, out))
    }

    unsafe fn allocator(&self, ty: D3D12_COMMAND_LIST_TYPE) -> Option<ID3D12CommandAllocator> {
        type F = unsafe extern "system" fn(*mut c_void, i32, *const GUID, *mut *mut c_void) -> i32;
        let f: F = transmute_copy(&self.dev_fn("CreateCommandAllocator")?);
        Self::make(|iid, out| f(self.dev, ty.0, iid, out))
    }

    /// A command list on `alloc`, in the recording state.
    unsafe fn list(&self, ty: D3D12_COMMAND_LIST_TYPE, alloc: &ID3D12CommandAllocator) -> Option<ID3D12GraphicsCommandList> {
        type F = unsafe extern "system" fn(*mut c_void, u32, i32, *mut c_void, *mut c_void, *const GUID, *mut *mut c_void) -> i32;
        let f: F = transmute_copy(&self.dev_fn("CreateCommandList")?);
        Self::make(|iid, out| f(self.dev, 0, ty.0, alloc.as_raw(), std::ptr::null_mut(), iid, out))
    }

    unsafe fn fence(&self) -> Option<ID3D12Fence> {
        type F = unsafe extern "system" fn(*mut c_void, u64, i32, *const GUID, *mut *mut c_void) -> i32;
        let f: F = transmute_copy(&self.dev_fn("CreateFence")?);
        Self::make(|iid, out| f(self.dev, 0, D3D12_FENCE_FLAG_NONE.0, iid, out))
    }

    /// `ExecuteCommandLists` of the queue's original function (no hook, no telemetry stamps).
    unsafe fn execute(&self, list: &ID3D12GraphicsCommandList) -> Option<()> {
        let f: unsafe extern "system" fn(*mut c_void, u32, *const *mut c_void) = transmute_copy(&self.queue_fn("ExecuteCommandLists")?);
        let p = list.as_raw();
        f(self.queue, 1, &p);
        Some(())
    }

    unsafe fn signal(&self, fence: &ID3D12Fence, value: u64) -> Option<()> {
        let f: unsafe extern "system" fn(*mut c_void, *mut c_void, u64) -> i32 = transmute_copy(&self.queue_fn("Signal")?);
        (f(self.queue, fence.as_raw(), value) >= 0).then_some(())
    }
}

/// One snapshot the statistics list reads: its partials sit at `off` (bytes) of the partials
/// buffer, `groups` of them; `dump` is the readback buffer of an image.
struct Item {
    snap: usize,
    consts: Consts,
    off: u64,
    groups: u32,
    dump: Option<ID3D12Resource>,
}

/// The statistics list in flight.
struct Gpu {
    fence: ID3D12Fence,
    value: u64,
    /// the readback of the partials
    readback: ID3D12Resource,
    /// the partials buffer the list writes (kept until the list is done)
    _partials: ID3D12Resource,
    total: u64,
    items: Vec<Item>,
    alloc: ID3D12CommandAllocator,
    list_type: i32,
    _list: ID3D12GraphicsCommandList,
}

/// What the `xcl-pass-stats` thread gets: the record of one list and its statistics list (None:
/// nothing to compute, or it could not be made).
struct Job {
    seq: Option<u64>,
    rec: Rec,
    gpu: Option<Gpu>,
}

// SAFETY: D3D12 objects are free-threaded; the rest is plain data.
unsafe impl Send for Job {}

static ALLOCS: Mutex<Vec<(i32, ID3D12CommandAllocator)>> = Mutex::new(Vec::new());
/// (armed frame, images written in it)
static DUMPED: Mutex<(u64, u32)> = Mutex::new((0, 0));

fn dump_slot(frame: u64) -> bool {
    let mut d = DUMPED.lock().unwrap();
    if d.0 != frame {
        *d = (frame, 0);
    }
    if d.1 >= DUMPS_PER_FRAME {
        return false;
    }
    d.1 += 1;
    true
}

fn dump_class(c: FormatClass) -> bool {
    matches!(c, FormatClass::Rgba8Unorm | FormatClass::Bgra8Unorm | FormatClass::Rgba16Float | FormatClass::R11G11B10Float | FormatClass::R32Float | FormatClass::D32Float)
}

/// Channels the kernel reports for a class (as its `chans_of`).
fn chans(c: FormatClass) -> usize {
    use FormatClass::*;
    match c {
        Rgba8Unorm | Bgra8Unorm | Rgba16Float | Rgba32Float | Rgb10A2Unorm => 4,
        R11G11B10Float => 3,
        Rg16Float | Rg32Float | D24UnormS8 => 2,
        _ => 1,
    }
}

/// The class of a snapshot (memory and formats without one are read as 4-byte words).
fn class_of_snap(s: &Snap) -> (FormatClass, u32) {
    if s.kind == Kind::Mem {
        (FormatClass::Raw32, 4)
    } else {
        class_of(DXGI_FORMAT(s.fmt as i32))
    }
}

/// What the kernel reads of a snapshot's buffer; None when there is nothing to read.
fn consts_of(s: &Snap) -> Option<Consts> {
    let (class, eb) = class_of_snap(s);
    let cap = MAX_GROUPS * GROUP_ELEMENTS;
    let c = if class == FormatClass::Raw32 {
        // memory, or a format the kernel has no class for: the words of the buffer, in a row
        Consts { elements: (s.bytes / 4).min(u64::from(cap)) as u32, row_elems: 0, row_pitch: 0, elem_bytes: 4, class, base_offset: 0 }
    } else {
        Consts { elements: (u64::from(s.w) * u64::from(s.h)).min(u64::from(cap)) as u32, row_elems: s.w, row_pitch: s.row_pitch, elem_bytes: eb, class, base_offset: 0 }
    };
    (c.elements > 0).then_some(c)
}

fn transition(r: &ID3D12Resource, before: D3D12_RESOURCE_STATES, after: D3D12_RESOURCE_STATES) -> D3D12_RESOURCE_BARRIER {
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 {
            Transition: ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                // SAFETY: a borrowed pointer in a ManuallyDrop field: nothing is released
                pResource: unsafe { transmute_copy(r) },
                Subresource: D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
                StateBefore: before,
                StateAfter: after,
            }),
        },
    }
}

fn uav_all() -> D3D12_RESOURCE_BARRIER {
    D3D12_RESOURCE_BARRIER {
        Type: D3D12_RESOURCE_BARRIER_TYPE_UAV,
        Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
        Anonymous: D3D12_RESOURCE_BARRIER_0 { UAV: ManuallyDrop::new(D3D12_RESOURCE_UAV_BARRIER { pResource: ManuallyDrop::new(None) }) },
    }
}

/// Records and submits the statistics list of `rec`'s snapshots: the pool buffers are moved from
/// COMMON to a read state, the kernel runs over each into one partials buffer, and the partials
/// (and the images' bytes) are copied to readback buffers. None when there is nothing to
/// compute or an object could not be made.
unsafe fn record_gpu(drv: &Drv, ty: D3D12_COMMAND_LIST_TYPE, rec: &Rec, dumps: bool) -> Option<Gpu> {
    let dev = ID3D12Device::from_raw_borrowed(&drv.dev)?;
    let kernel = match pass_stats::kernel(dev) {
        Ok(k) => k,
        Err(e) => {
            crate::xlog_warn!("pass stats: {e}");
            return None;
        }
    };
    let mut items: Vec<Item> = Vec::new();
    let mut total = 0u64;
    for (i, s) in rec.snapshots.iter().enumerate() {
        if s.buf.is_none() || s.skipped.is_some() {
            continue;
        }
        let Some(consts) = consts_of(s) else { continue };
        let groups = groups_for(consts.elements);
        items.push(Item { snap: i, consts, off: total, groups, dump: None });
        total += u64::from(groups) * PARTIAL_WORDS as u64 * 4;
    }
    if items.is_empty() {
        return None;
    }
    let partials = drv.buffer(total, D3D12_HEAP_TYPE_DEFAULT, D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_UNORDERED_ACCESS)?;
    let readback = drv.buffer(total, D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_FLAG_NONE, D3D12_RESOURCE_STATE_COPY_DEST)?;
    for it in items.iter_mut() {
        let s = &rec.snapshots[it.snap];
        if dumps && s.when == When::Post && s.kind != Kind::Mem && dump_class(it.consts.class) && dump_slot(rec.frame) {
            it.dump = drv.buffer(s.bytes, D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_FLAG_NONE, D3D12_RESOURCE_STATE_COPY_DEST);
        }
    }
    let pooled = {
        let mut a = ALLOCS.lock().unwrap();
        a.iter().position(|(t, _)| *t == ty.0).map(|i| a.swap_remove(i).1)
    };
    let alloc = match pooled {
        Some(a) if a.Reset().is_ok() => a,
        _ => drv.allocator(ty)?,
    };
    let list = drv.list(ty, &alloc)?;
    let mut before: Vec<D3D12_RESOURCE_BARRIER> = Vec::new();
    for it in &items {
        let s = &rec.snapshots[it.snap];
        let state = if it.dump.is_some() { D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE | D3D12_RESOURCE_STATE_COPY_SOURCE } else { D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE };
        before.push(transition(s.buf.as_ref()?, D3D12_RESOURCE_STATE_COMMON, state));
    }
    before.push(uav_all());
    list.ResourceBarrier(&before);
    let base = partials.GetGPUVirtualAddress();
    for it in &items {
        let src = rec.snapshots[it.snap].buf.as_ref()?.GetGPUVirtualAddress();
        pass_stats::record(&list, kernel, src, base + it.off, it.consts);
    }
    list.ResourceBarrier(&[transition(&partials, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_COPY_SOURCE)]);
    list.CopyBufferRegion(&readback, 0, &partials, 0, total);
    for it in &items {
        if let Some(d) = &it.dump {
            let s = &rec.snapshots[it.snap];
            list.CopyBufferRegion(d, 0, s.buf.as_ref()?, 0, s.bytes);
        }
    }
    list.Close().ok()?;
    let fence = drv.fence()?;
    drv.execute(&list)?;
    drv.signal(&fence, 1)?;
    Some(Gpu { fence, value: 1, readback, _partials: partials, total, items, alloc, list_type: ty.0, _list: list })
}

/// Builds the job of one list's record.
unsafe fn build_job(drv: &Drv, ty: D3D12_COMMAND_LIST_TYPE, seq: Option<u64>, rec: Rec) -> Job {
    let gpu = record_gpu(drv, ty, &rec, cfg().dump);
    Job { seq, rec, gpu }
}

/// After a submission of `lists` on the PC queue `queue` (`seq0`: the telemetry number of the
/// first list when known): for each list whose record was taken, run the statistics kernel over
/// its snapshots on an XCL list on the same queue and hand the job to the `xcl-pass-stats`
/// thread. Nothing when the switch is off or no list has a record.
pub fn after_submit(queue: *mut c_void, lists: &[*mut c_void], seq0: Option<u64>) {
    if !cfg().on {
        return;
    }
    let mut recs: Vec<(usize, Rec)> = Vec::new();
    for (k, &l) in lists.iter().enumerate() {
        if let Some(rec) = take(l).filter(|r| !r.snapshots.is_empty()) {
            recs.push((k, rec));
        }
    }
    if recs.is_empty() {
        return;
    }
    let Some(drv) = Drv::orig(queue) else { return };
    // SAFETY: the queue is the PC queue the lists were just submitted to.
    let ty = unsafe { drv.queue_type() };
    let Some(tx) = sender() else { return };
    for (k, rec) in recs {
        // a copy queue cannot run the kernel: the events say so
        let job = match ty {
            Some(t) if t != D3D12_COMMAND_LIST_TYPE_COPY => unsafe { build_job(&drv, t, seq0.map(|s| s + k as u64), rec) },
            _ => Job { seq: seq0.map(|s| s + k as u64), rec, gpu: None },
        };
        let _ = tx.lock().unwrap_or_else(|e| e.into_inner()).send(job);
    }
}

fn sender() -> Option<&'static Mutex<Sender<Job>>> {
    static TX: OnceLock<Option<Mutex<Sender<Job>>>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = channel();
        match std::thread::Builder::new().name("xcl-pass-stats".into()).spawn(move || worker(rx)) {
            Ok(_) => Some(Mutex::new(tx)),
            Err(e) => {
                crate::xlog_warn!("pass stats: the xcl-pass-stats thread did not start: {e}");
                None
            }
        }
    })
    .as_ref()
}

fn worker(rx: Receiver<Job>) {
    for job in rx {
        finish(job, &mut |fields| crate::log::event(crate::log::Tier::Pass, "pass_stats", &fields));
    }
}

/// Polls `completed` until it reaches `target`. `Err("removed")` when it reads `u64::MAX` (what a
/// removed device does to its fences), `Err("timeout")` after `timeout`.
fn wait_fence(completed: &mut dyn FnMut() -> u64, target: u64, timeout: Duration, sleep: Duration) -> Result<(), &'static str> {
    let start = Instant::now();
    loop {
        let v = completed();
        if v == u64::MAX {
            return Err("removed");
        }
        if v >= target {
            return Ok(());
        }
        if start.elapsed() >= timeout {
            return Err("timeout");
        }
        std::thread::sleep(sleep);
    }
}

/// The thread's part of a job: wait for the statistics list, read the partials back, write one
/// event per snapshot (`emit`) and the images. A job whose device was removed is dropped.
fn finish(job: Job, emit: &mut dyn FnMut(Vec<(&'static str, Value)>)) {
    let Job { seq, rec, gpu } = job;
    let mut stats: Vec<Option<Stats>> = vec![None; rec.snapshots.len()];
    if let Some(g) = gpu {
        let fence = g.fence.clone();
        // SAFETY: GetCompletedValue of an XCL fence.
        let done = wait_fence(&mut || unsafe { fence.GetCompletedValue() }, g.value, JOB_TIMEOUT, Duration::from_millis(1));
        if let Err(why) = done {
            crate::xlog_warn!("pass stats: frame {}: a statistics job is dropped ({why})", rec.frame);
            return;
        }
        // SAFETY: the fence passed, so the readback buffers hold the copies; each is mapped for reading only.
        unsafe {
            let mut p = std::ptr::null_mut();
            if g.readback.Map(0, None, Some(&mut p)).is_ok() && !p.is_null() {
                let words = std::slice::from_raw_parts(p as *const u32, (g.total / 4) as usize).to_vec();
                g.readback.Unmap(0, None);
                for it in &g.items {
                    let at = (it.off / 4) as usize;
                    let parts: Vec<Partial> = words[at..at + it.groups as usize * PARTIAL_WORDS].chunks(PARTIAL_WORDS).map(Partial::from_words).collect();
                    stats[it.snap] = Some(combine(&parts));
                }
            }
            for it in &g.items {
                let Some(d) = &it.dump else { continue };
                let s = &rec.snapshots[it.snap];
                let mut p = std::ptr::null_mut();
                if d.Map(0, None, Some(&mut p)).is_ok() && !p.is_null() {
                    let bytes = std::slice::from_raw_parts(p as *const u8, s.bytes as usize);
                    write_image(rec.frame, seq, s, it.consts.class, bytes);
                    d.Unmap(0, None);
                }
            }
        }
        let mut a = ALLOCS.lock().unwrap();
        if a.len() < ALLOCS_MAX {
            a.push((g.list_type, g.alloc.clone()));
        }
    }
    for (s, st) in rec.snapshots.iter().zip(&stats) {
        emit(stats_fields(rec.frame, seq, s, st.as_ref()));
    }
}

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Rt => "rt",
        Kind::Ds => "ds",
        Kind::Uav => "uav",
        Kind::Mem => "mem",
    }
}

fn hex(v: usize) -> Value {
    Value::from(format!("{v:#x}"))
}

/// The fields of a `pass_stats` event (tier pass), always these, in this order. `min`, `max` and
/// `mean` have one number per channel of the format and `zero` .. `hash` are left out (and
/// `skipped` set, "error" when the snapshot has no reason of its own) when there are no `stats`.
/// `reads` is only in a post snapshot's event.
fn stats_fields(frame: u64, list: Option<u64>, s: &Snap, st: Option<&Stats>) -> Vec<(&'static str, Value)> {
    let (class, _) = class_of_snap(s);
    let mut f: Vec<(&'static str, Value)> = vec![
        ("frame", frame.into()),
        ("list", list.map_or(Value::Null, Value::from)),
        ("pass", s.pass.into()),
        ("when", (if s.when == When::Pre { "pre" } else { "post" }).into()),
        ("res", hex(s.res)),
        ("kind", kind_name(s.kind).into()),
        ("format", (if s.kind == Kind::Mem { "raw".to_string() } else { format!("{class:?}") }).into()),
        ("w", s.w.into()),
        ("h", s.h.into()),
    ];
    if let Some(st) = st {
        let n = chans(class);
        let arr = |v: &[f32; 4]| Value::from(v[..n].iter().map(|&x| Value::from(f64::from(x))).collect::<Vec<_>>());
        f.push(("min", arr(&st.min)));
        f.push(("max", arr(&st.max)));
        f.push(("mean", arr(&st.mean)));
        f.push(("zero", st.zero.into()));
        f.push(("nan", st.nan.into()));
        f.push(("inf", st.inf.into()));
        f.push(("count", st.count.into()));
        f.push(("hash", format!("{:016x}", st.hash).into()));
    }
    let skipped = s.skipped.or(if st.is_none() { Some("error") } else { None });
    f.push(("skipped", skipped.map_or(Value::Null, Value::from)));
    f.push(("unresolved", s.unresolved.into()));
    if s.when == When::Post {
        f.push(("reads", Value::from(s.reads.iter().map(|&r| hex(r)).collect::<Vec<_>>())));
    }
    f
}

fn f16_to_f32(h: u16) -> f32 {
    let (e, m) = (i32::from((h >> 10) & 0x1f), u32::from(h & 0x3ff));
    let v = match e {
        0 => m as f32 * 2f32.powi(-24),
        31 if m == 0 => f32::INFINITY,
        31 => f32::NAN,
        _ => (1.0 + m as f32 / 1024.0) * 2f32.powi(e - 15),
    };
    if h & 0x8000 != 0 {
        -v
    } else {
        v
    }
}

fn to_u8(v: f32) -> u8 {
    // NaN gives 0; the float formats are clamped to [0, 1]
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// The RGB of the element in `b` (4, 8 bytes ...), by class; depth and single channels are grey.
fn pixel_rgb(class: FormatClass, b: &[u8]) -> [u8; 3] {
    let u32at = |at: usize| u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
    let h = |at: usize| to_u8(f16_to_f32(u16::from_le_bytes([b[at], b[at + 1]])));
    match class {
        FormatClass::Rgba8Unorm => [b[0], b[1], b[2]],
        FormatClass::Bgra8Unorm => [b[2], b[1], b[0]],
        FormatClass::Rgba16Float => [h(0), h(2), h(4)],
        FormatClass::R11G11B10Float => {
            let v = u32at(0);
            let c = |bits: u32| to_u8(f16_to_f32(bits as u16));
            [c((v & 0x7FF) << 4), c(((v >> 11) & 0x7FF) << 4), c(((v >> 22) & 0x3FF) << 5)]
        }
        _ => {
            let g = to_u8(f32::from_bits(u32at(0)));
            [g, g, g]
        }
    }
}

/// An image of the snapshot's bytes (`w` x `h` elements, rows `row_pitch` apart), at most
/// `DUMP_SIDE` pixels on the long side (every n-th pixel). (width, height, RGB top row first).
fn decode_image(class: FormatClass, bytes: &[u8], w: u32, h: u32, row_pitch: u32) -> (u32, u32, Vec<u8>) {
    let eb = class_of_eb(class);
    let step = w.max(h).div_ceil(DUMP_SIDE).max(1);
    let (ow, oh) = (w.div_ceil(step), h.div_ceil(step));
    let mut rgb = Vec::with_capacity(ow as usize * oh as usize * 3);
    for y in 0..oh {
        for x in 0..ow {
            let at = (y * step) as usize * row_pitch as usize + (x * step) as usize * eb;
            match bytes.get(at..at + eb) {
                Some(b) => rgb.extend_from_slice(&pixel_rgb(class, b)),
                None => rgb.extend_from_slice(&[0, 0, 0]),
            }
        }
    }
    (ow, oh, rgb)
}

fn class_of_eb(c: FormatClass) -> usize {
    if c == FormatClass::Rgba16Float {
        8
    } else {
        4
    }
}

/// `<run folder>/pass/f{frame}_l{seq}_p{pass}_{res:x}.bmp`
fn write_image(frame: u64, seq: Option<u64>, s: &Snap, class: FormatClass, bytes: &[u8]) {
    let Some(dir) = crate::log::run_dir().map(|d| d.join("pass")) else { return };
    let (w, h, rgb) = decode_image(class, bytes, s.w, s.h, s.row_pitch);
    let name = format!("f{frame}_l{}_p{}_{:x}.bmp", seq.map_or("x".to_string(), |q| q.to_string()), s.pass, s.res);
    let r = std::fs::create_dir_all(&dir).and_then(|_| pass_stats::write_bmp(&dir.join(&name), w, h, &rgb));
    if let Err(e) = r {
        crate::xlog_warn!("pass stats: {name}: {e}");
    }
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

    fn seeded_stats() -> Stats {
        Stats { min: [0.0, 0.1, 0.2, 0.3], max: [1.0, 1.0, 1.0, 1.0], mean: [0.5; 4], zero: 2, nan: 1, inf: 0, count: 100, finite: 99, hash: 0xABC }
    }

    #[test]
    fn stats_event_fields_are_fixed() {
        let mut s = empty_snap(&rt(0x1000), 3, When::Post);
        s.fmt = DXGI_FORMAT_R16G16B16A16_FLOAT.0 as u32;
        s.w = 64;
        s.h = 32;
        s.reads = vec![0x2000, 0x3000];
        let names = |f: &Vec<(&'static str, Value)>| f.iter().map(|x| x.0).collect::<Vec<_>>();
        let f = stats_fields(120, Some(7), &s, Some(&seeded_stats()));
        assert_eq!(
            names(&f),
            ["frame", "list", "pass", "when", "res", "kind", "format", "w", "h", "min", "max", "mean", "zero", "nan", "inf", "count", "hash", "skipped", "unresolved", "reads"]
        );
        let get = |k: &str| f.iter().find(|x| x.0 == k).unwrap().1.clone();
        assert_eq!(get("res"), Value::from("0x1000"));
        assert_eq!(get("hash"), Value::from("0000000000000abc"));
        assert_eq!(get("when"), Value::from("post"));
        assert_eq!(get("kind"), Value::from("rt"));
        assert_eq!(get("list"), Value::from(7u64));
        assert_eq!(get("skipped"), Value::Null);
        assert_eq!(get("reads"), Value::from(vec![Value::from("0x2000"), Value::from("0x3000")]));
        // RGBA16F has four channels
        assert_eq!(get("min").as_array().unwrap().len(), 4);
        // a pre snapshot has no `reads`; one with a reason and no statistics has no numbers
        let mut p = empty_snap(&rt(0x1000), 3, When::Pre);
        p.skipped = Some("budget");
        let f = stats_fields(120, None, &p, None);
        assert_eq!(names(&f), ["frame", "list", "pass", "when", "res", "kind", "format", "w", "h", "skipped", "unresolved"]);
        assert_eq!(f[1].1, Value::Null);
        assert_eq!(f[9].1, Value::from("budget"));
        // no statistics and no reason of its own: "error"
        let f = stats_fields(120, None, &empty_snap(&rt(0x1000), 0, When::Pre), None);
        assert_eq!(f[9].1, Value::from("error"));
    }

    #[test]
    fn job_dropped_when_device_removed() {
        // a removed device reads u64::MAX from its fences: the wait gives up at once
        let mut calls = 0;
        let r = wait_fence(&mut || { calls += 1; u64::MAX }, 1, Duration::from_secs(5), Duration::ZERO);
        assert_eq!(r, Err("removed"));
        assert_eq!(calls, 1);
        // a fence that completes after a few polls
        let mut n = 0;
        assert_eq!(wait_fence(&mut || { n += 1; if n < 3 { 0 } else { 1 } }, 1, Duration::from_secs(5), Duration::ZERO), Ok(()));
        // one that never does
        assert_eq!(wait_fence(&mut || 0, 1, Duration::ZERO, Duration::ZERO), Err("timeout"));
    }

    #[test]
    fn dump_decodes_and_downsamples() {
        // 2 x 1 BGRA8, rows 8 bytes apart: B, G, R swapped to R, G, B
        let bytes = [10u8, 20, 30, 255, 40, 50, 60, 255];
        assert_eq!(decode_image(FormatClass::Bgra8Unorm, &bytes, 2, 1, 8), (2, 1, vec![30, 20, 10, 60, 50, 40]));
        // RGBA16F 1.0 (0x3C00), 0.5 (0x3800), -1.0 clamps to 0
        let px: Vec<u8> = [0x3C00u16, 0x3800, 0xBC00, 0x3C00].iter().flat_map(|v| v.to_le_bytes()).collect();
        assert_eq!(decode_image(FormatClass::Rgba16Float, &px, 1, 1, 8), (1, 1, vec![255, 128, 0]));
        // 2048 x 1 is sampled every 2nd pixel
        let big = vec![7u8; 2048 * 4];
        let (w, h, rgb) = decode_image(FormatClass::Rgba8Unorm, &big, 2048, 1, 2048 * 4);
        assert_eq!((w, h, rgb.len()), (1024, 1, 1024 * 3));
    }

    #[test]
    fn gpu_snapshot_of_cleared_target_is_constant() {
        let made = unsafe {
            let dev = crate::gpu::agility::create_device(None, windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_11_0).ok();
            dev.and_then(|d| d.CreateCommandQueue::<ID3D12CommandQueue>(&D3D12_COMMAND_QUEUE_DESC { Type: D3D12_COMMAND_LIST_TYPE_DIRECT, ..Default::default() }).ok().map(|q| (d, q)))
        };
        let Some((dev, queue)) = made else {
            crate::gpu::recomp::runner::skip_test("no GPU");
            return;
        };
        if crate::gpu::recomp::dxc::location().is_err() {
            crate::gpu::recomp::runner::skip_test("no DXC");
            return;
        }
        unsafe {
            // the bytes of a 64 x 64 RGBA16F target cleared to 0.25 (0x3400), in a default buffer
            // (COMMON, as the snapshot pool's buffers are after their copy)
            let bytes = 64u64 * 64 * 8;
            let data: Vec<u8> = (0..64 * 64 * 4).flat_map(|_| 0x3400u16.to_le_bytes()).collect();
            let mk = |heap: D3D12_HEAP_TYPE, state: D3D12_RESOURCE_STATES| {
                let mut r: Option<ID3D12Resource> = None;
                dev.CreateCommittedResource(
                    &D3D12_HEAP_PROPERTIES { Type: heap, ..Default::default() },
                    D3D12_HEAP_FLAG_NONE,
                    &D3D12_RESOURCE_DESC {
                        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                        Width: bytes,
                        Height: 1,
                        DepthOrArraySize: 1,
                        MipLevels: 1,
                        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                        ..Default::default()
                    },
                    state,
                    None,
                    &mut r,
                )
                .unwrap();
                r.unwrap()
            };
            let up = mk(D3D12_HEAP_TYPE_UPLOAD, D3D12_RESOURCE_STATE_GENERIC_READ);
            let mut p = std::ptr::null_mut();
            up.Map(0, None, Some(&mut p)).unwrap();
            std::ptr::copy_nonoverlapping(data.as_ptr(), p as *mut u8, data.len());
            up.Unmap(0, None);
            let buf = mk(D3D12_HEAP_TYPE_DEFAULT, D3D12_RESOURCE_STATE_COMMON);
            let alloc: ID3D12CommandAllocator = dev.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT).unwrap();
            let list: ID3D12GraphicsCommandList = dev.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &alloc, None).unwrap();
            list.CopyBufferRegion(&buf, 0, &up, 0, bytes);
            list.Close().unwrap();
            queue.ExecuteCommandLists(&[Some(list.cast().unwrap())]);
            crate::gpu::texture::wait(&dev, &queue).unwrap();

            let mut snap = empty_snap(&rt(0x1000), 0, When::Post);
            snap.fmt = DXGI_FORMAT_R16G16B16A16_FLOAT.0 as u32;
            snap.w = 64;
            snap.h = 64;
            snap.row_pitch = 64 * 8;
            snap.bytes = bytes;
            snap.buf = Some(buf);
            let mut rec = Rec::new(5);
            rec.snapshots.push(snap);
            let drv = Drv { dev: dev.as_raw(), queue: queue.as_raw(), get: plain_get };
            let job = build_job(&drv, D3D12_COMMAND_LIST_TYPE_DIRECT, Some(9), rec);
            assert!(job.gpu.is_some());
            let mut evs: Vec<Vec<(&'static str, Value)>> = Vec::new();
            finish(job, &mut |f| evs.push(f));
            assert_eq!(evs.len(), 1);
            let get = |k: &str| evs[0].iter().find(|x| x.0 == k).unwrap().1.clone();
            assert_eq!(get("min"), Value::from(vec![Value::from(0.25); 4]));
            assert_eq!(get("max"), Value::from(vec![Value::from(0.25); 4]));
            assert_eq!(get("nan"), Value::from(0u64));
            assert_eq!(get("count"), Value::from(4096u64));
            assert_eq!(get("list"), Value::from(9u64));
        }
    }
}
