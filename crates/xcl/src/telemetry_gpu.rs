//! Fork-only telemetry code (docs/guide/telemetry.md, parts 2-3): the GPU stamp engine. Included from
//! `report.rs` (`#[path = "telemetry_gpu.rs"] mod gpu_tel;`), never from an upstream file.
//!
//! Every PC command list a queue is given gets a start and an end GPU timestamp. The stamps are
//! tiny pre-recorded command lists (one `EndQuery` each) put between the title's lists in the same
//! `ExecuteCommandLists` call. One `ResolveQueryData` per queue per frame copies the stamps to a
//! readback buffer, and the `xcl-telemetry` thread reads it at least two frames later, after its
//! own fence has passed. Nothing here waits on the GPU, maps per frame, or reads a set in the
//! frame that wrote it. All objects are made through the PC device's original functions, so XCL's
//! hooks never adopt them.

// P2-C (frame glue) and part 3 use the items that nothing calls yet.
#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, RwLock, TryLockError};
use std::time::{Duration, Instant};

use serde_json::Value;
use windows::core::{Interface, GUID};
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use crate::hle::d3d12x::{frame as dframe, layout, real};
use crate::log::{self as xlog, FrameFacts, ListFacts, MemoryFacts, PassFacts, Tier};

/// Stamp sets per queue: frames in flight + 1.
const SETS: usize = 4;
/// Stamps per set (about 1023 lists per queue per frame).
const LIST_SLOTS: u32 = 1024;
/// Frame records waiting for GPU data (the frame glue, P2-C, uses it).
const PENDING_FRAMES: usize = 64;
/// A set not harvested this many frames after its frame gives null ticks with `why: "late"`.
const LATE_FRAMES: u64 = 16;
/// Queues with their own clock.
const MAX_QUEUES: usize = 8;
/// Null-tick entries a queue keeps until the thread writes them.
const NULLS_MAX: usize = 4096;
/// Harvested list intervals waiting for the frame glue.
const HARVESTED_MAX: usize = 4096;
/// Lists with a submission whose stamps are not read yet (the resubmission check).
const IN_FLIGHT_MAX: usize = 16384;
/// Pass stamps (part 3): one global heap of `PASS_SLOTS`, handed out in blocks of `BLOCK_SLOTS`.
const PASS_SLOTS: u32 = 65_536;
const BLOCK_SLOTS: u32 = 256;
const BLOCKS: usize = (PASS_SLOTS / BLOCK_SLOTS) as usize;
/// Blocks one recording may take (then its passes stop: `full`).
const MAX_REC_BLOCKS: usize = 16;
/// Recordings with pass notes that are not submitted yet.
const RECS_MAX: usize = 8192;
/// Distinct pipeline ids one pass reports.
const MAX_PIPES: usize = 8;
/// Telemetry pipeline ids by pointer (bounded).
const PIPE_MAP_MAX: usize = 262_144;
/// How long a title thread waits for a telemetry lock before the note is dropped.
const NOTE_WAIT: Duration = Duration::from_millis(2);
/// `io` events are flushed at least this often.
const IO_FLUSH: Duration = Duration::from_millis(250);
/// Poisoned submissions kept (a resubmitted list's earlier entries).
const POISONED_MAX: usize = 4096;

// indexes of `GpuTel.drops`
const DROP_FULL: usize = 0;
const DROP_LATE: usize = 1;
const DROP_UNSUPPORTED: usize = 2;
const DROP_RESUBMITTED: usize = 3;
const DROP_REMOVED: usize = 4;
const DROP_SAMPLED_OUT: usize = 5;

thread_local! {
    /// True on a thread inside the present span: lists submitted there are `origin: "present"`.
    /// The frame glue (P2-C) sets it.
    pub(super) static IN_PRESENT_SPAN: Cell<bool> = const { Cell::new(false) };
}

/// Receives each event the engine writes: (event name, fields).
type Sink<'a> = &'a mut dyn FnMut(&str, Vec<(&'static str, Value)>);

fn qpf() -> i64 {
    static F: OnceLock<i64> = OnceLock::new();
    *F.get_or_init(|| {
        let mut v = 0i64;
        // SAFETY: plain out-parameter.
        unsafe { windows_sys::Win32::System::Performance::QueryPerformanceFrequency(&mut v) };
        v.max(1)
    })
}

/// `m.lock()`, retried for `wait` and then given up: the holder may be this very thread. A
/// poisoned lock is used as it is.
fn lock_wait<T>(m: &Mutex<T>, wait: Duration) -> Option<MutexGuard<'_, T>> {
    let end = Instant::now() + wait;
    loop {
        match m.try_lock() {
            Ok(g) => return Some(g),
            Err(TryLockError::Poisoned(p)) => return Some(p.into_inner()),
            Err(TryLockError::WouldBlock) if Instant::now() < end => std::thread::sleep(Duration::from_micros(50)),
            Err(TryLockError::WouldBlock) => return None,
        }
    }
}

/// `m.try_lock()` that uses a poisoned lock as it is.
fn try_lock<T>(m: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
    match m.try_lock() {
        Ok(g) => Some(g),
        Err(TryLockError::Poisoned(p)) => Some(p.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

fn plock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ---------------------------------------------------------------------------------------
// function tables
// ---------------------------------------------------------------------------------------

fn method_slot(iface: &str, method: &str) -> Option<usize> {
    layout::slot(layout::find(iface)?, method)
}

/// Function `slot` of the object's own vtable (an object XCL did not adopt).
unsafe fn plain_fn(obj: *mut c_void, slot: usize) -> usize {
    *(*(obj as *const *const usize)).add(slot)
}

/// Function `slot` of an adopted object's original vtable.
unsafe fn orig_slot(obj: *mut c_void, slot: usize) -> usize {
    real::orig_fn(obj, slot)
}

/// The PC device's functions the engine calls (addresses, so no object of ours is adopted).
#[derive(Clone, Copy)]
pub(super) struct DevFns {
    raw: *mut c_void,
    create_query_heap: usize,
    create_committed_resource: usize,
    create_command_allocator: usize,
    create_command_list: usize,
    create_fence: usize,
    check_feature_support: usize,
}

// SAFETY: the pointer is a COM device (free-threaded); the rest are code addresses.
unsafe impl Send for DevFns {}
unsafe impl Sync for DevFns {}

impl DevFns {
    fn build(raw: *mut c_void, get: unsafe fn(*mut c_void, usize) -> usize) -> Option<DevFns> {
        if raw.is_null() {
            return None;
        }
        let s = |m: &str| method_slot("ID3D12Device", m);
        // SAFETY: `raw` is a live device and the slots come from the PC interface table.
        unsafe {
            Some(DevFns {
                raw,
                create_query_heap: get(raw, s("CreateQueryHeap")?),
                create_committed_resource: get(raw, s("CreateCommittedResource")?),
                create_command_allocator: get(raw, s("CreateCommandAllocator")?),
                create_command_list: get(raw, s("CreateCommandList")?),
                create_fence: get(raw, s("CreateFence")?),
                check_feature_support: get(raw, s("CheckFeatureSupport")?),
            })
        }
    }

    /// The PC device's original functions (None until the device exists).
    pub(super) fn orig() -> Option<DevFns> {
        Self::build(real::device_raw(), orig_slot)
    }

    /// The functions of a device that XCL did not adopt (tests).
    pub(super) fn plain(dev: &ID3D12Device) -> Option<DevFns> {
        Self::build(dev.as_raw(), plain_fn)
    }

    unsafe fn make<T: Interface>(&self, call: impl FnOnce(*const GUID, *mut *mut c_void) -> i32) -> Option<T> {
        let mut out: *mut c_void = std::ptr::null_mut();
        let hr = call(&T::IID, &mut out);
        if hr < 0 || out.is_null() {
            return None;
        }
        Some(T::from_raw(out))
    }

    fn query_heap(&self, ty: D3D12_QUERY_HEAP_TYPE, count: u32) -> Option<ID3D12QueryHeap> {
        type F = unsafe extern "system" fn(*mut c_void, *const D3D12_QUERY_HEAP_DESC, *const GUID, *mut *mut c_void) -> i32;
        let desc = D3D12_QUERY_HEAP_DESC { Type: ty, Count: count, NodeMask: 0 };
        // SAFETY: the address is the device's CreateQueryHeap.
        unsafe {
            let f: F = std::mem::transmute(self.create_query_heap);
            self.make(|iid, out| f(self.raw, &desc, iid, out))
        }
    }

    /// A READBACK buffer of `bytes`.
    fn readback(&self, bytes: u64) -> Option<ID3D12Resource> {
        type F = unsafe extern "system" fn(
            *mut c_void, *const D3D12_HEAP_PROPERTIES, i32, *const D3D12_RESOURCE_DESC, i32, *const c_void, *const GUID, *mut *mut c_void,
        ) -> i32;
        let props = D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_READBACK, ..Default::default() };
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
            Width: bytes.max(256),
            Height: 1,
            DepthOrArraySize: 1,
            MipLevels: 1,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            ..Default::default()
        };
        // SAFETY: the address is the device's CreateCommittedResource.
        unsafe {
            let f: F = std::mem::transmute(self.create_committed_resource);
            self.make(|iid, out| f(self.raw, &props, D3D12_HEAP_FLAG_NONE.0, &desc, D3D12_RESOURCE_STATE_COPY_DEST.0, std::ptr::null(), iid, out))
        }
    }

    fn allocator(&self, ty: D3D12_COMMAND_LIST_TYPE) -> Option<ID3D12CommandAllocator> {
        type F = unsafe extern "system" fn(*mut c_void, i32, *const GUID, *mut *mut c_void) -> i32;
        // SAFETY: the address is the device's CreateCommandAllocator.
        unsafe {
            let f: F = std::mem::transmute(self.create_command_allocator);
            self.make(|iid, out| f(self.raw, ty.0, iid, out))
        }
    }

    /// A command list on `alloc`, in the recording state.
    fn list(&self, ty: D3D12_COMMAND_LIST_TYPE, alloc: &ID3D12CommandAllocator) -> Option<ID3D12GraphicsCommandList> {
        type F = unsafe extern "system" fn(*mut c_void, u32, i32, *mut c_void, *mut c_void, *const GUID, *mut *mut c_void) -> i32;
        // SAFETY: the address is the device's CreateCommandList.
        unsafe {
            let f: F = std::mem::transmute(self.create_command_list);
            self.make(|iid, out| f(self.raw, 0, ty.0, alloc.as_raw(), std::ptr::null_mut(), iid, out))
        }
    }

    fn fence(&self) -> Option<ID3D12Fence> {
        type F = unsafe extern "system" fn(*mut c_void, u64, i32, *const GUID, *mut *mut c_void) -> i32;
        // SAFETY: the address is the device's CreateFence.
        unsafe {
            let f: F = std::mem::transmute(self.create_fence);
            self.make(|iid, out| f(self.raw, 0, D3D12_FENCE_FLAG_NONE.0, iid, out))
        }
    }

    /// `D3D12_OPTIONS3.CopyQueueTimestampQueriesSupported`.
    fn copy_timestamps(&self) -> bool {
        type F = unsafe extern "system" fn(*mut c_void, i32, *mut c_void, u32) -> i32;
        let mut o = D3D12_FEATURE_DATA_D3D12_OPTIONS3::default();
        // SAFETY: the address is the device's CheckFeatureSupport; `o` is the structure of the feature.
        unsafe {
            let f: F = std::mem::transmute(self.check_feature_support);
            f(self.raw, D3D12_FEATURE_D3D12_OPTIONS3.0, &mut o as *mut _ as *mut c_void, std::mem::size_of_val(&o) as u32) >= 0
                && o.CopyQueueTimestampQueriesSupported.as_bool()
        }
    }
}

/// The PC queue's functions (the title-facing queue has the console layout: its slots 16 and 17
/// are not these).
#[derive(Clone, Copy)]
pub(super) struct QueueFns {
    raw: *mut c_void,
    ecl: usize,
    signal: usize,
    freq: usize,
    calib: usize,
    desc: usize,
}

// SAFETY: the pointer is a COM queue (free-threaded); the rest are code addresses.
unsafe impl Send for QueueFns {}
unsafe impl Sync for QueueFns {}

impl QueueFns {
    fn build(raw: *mut c_void, get: unsafe fn(*mut c_void, usize) -> usize) -> Option<QueueFns> {
        if raw.is_null() {
            return None;
        }
        let s = |m: &str| method_slot("ID3D12CommandQueue", m);
        // SAFETY: `raw` is a live queue and the slots come from the PC interface table.
        unsafe {
            Some(QueueFns {
                raw,
                ecl: get(raw, s("ExecuteCommandLists")?),
                signal: get(raw, s("Signal")?),
                freq: get(raw, s("GetTimestampFrequency")?),
                calib: get(raw, s("GetClockCalibration")?),
                desc: get(raw, s("GetDesc")?),
            })
        }
    }

    /// The original functions of an adopted PC queue.
    pub(super) fn orig(queue: *mut c_void) -> Option<QueueFns> {
        Self::build(queue, orig_slot)
    }

    /// The functions of a queue that XCL did not adopt (tests).
    pub(super) fn plain(queue: &ID3D12CommandQueue) -> Option<QueueFns> {
        Self::build(queue.as_raw(), plain_fn)
    }

    fn plain_raw(queue: *mut c_void) -> Option<QueueFns> {
        Self::build(queue, plain_fn)
    }

    /// Runs `ExecuteCommandLists` (tests).
    #[cfg(test)]
    fn execute(&self, lists: &[*mut c_void]) {
        // SAFETY: `ecl` is the queue's ExecuteCommandLists.
        unsafe {
            let f: unsafe extern "system" fn(*mut c_void, u32, *const *mut c_void) = std::mem::transmute(self.ecl);
            f(self.raw, lists.len() as u32, lists.as_ptr());
        }
    }

    fn signal(&self, fence: &ID3D12Fence, value: u64) -> i32 {
        // SAFETY: `signal` is the queue's Signal.
        unsafe {
            let f: unsafe extern "system" fn(*mut c_void, *mut c_void, u64) -> i32 = std::mem::transmute(self.signal);
            f(self.raw, fence.as_raw(), value)
        }
    }

    /// Ticks per second; None when the call fails.
    fn frequency(&self) -> Option<u64> {
        let mut hz = 0u64;
        // SAFETY: `freq` is the queue's GetTimestampFrequency.
        let hr = unsafe {
            let f: unsafe extern "system" fn(*mut c_void, *mut u64) -> i32 = std::mem::transmute(self.freq);
            f(self.raw, &mut hz)
        };
        (hr >= 0).then_some(hz)
    }

    /// (GPU tick, CPU QPC) taken together; None when the call fails.
    fn calibration(&self) -> Option<(u64, i64)> {
        let (mut gpu, mut cpu) = (0u64, 0u64);
        // SAFETY: `calib` is the queue's GetClockCalibration.
        let hr = unsafe {
            let f: unsafe extern "system" fn(*mut c_void, *mut u64, *mut u64) -> i32 = std::mem::transmute(self.calib);
            f(self.raw, &mut gpu, &mut cpu)
        };
        (hr >= 0).then_some((gpu, cpu as i64))
    }

    fn kind(&self) -> QueueKind {
        let mut d = D3D12_COMMAND_QUEUE_DESC::default();
        // SAFETY: `desc` is the queue's GetDesc: it returns its structure through a hidden pointer.
        unsafe {
            let f: unsafe extern "system" fn(*mut c_void, *mut D3D12_COMMAND_QUEUE_DESC) -> *mut D3D12_COMMAND_QUEUE_DESC = std::mem::transmute(self.desc);
            f(self.raw, &mut d);
        }
        match d.Type {
            D3D12_COMMAND_LIST_TYPE_DIRECT => QueueKind::Direct,
            D3D12_COMMAND_LIST_TYPE_COMPUTE => QueueKind::Compute,
            D3D12_COMMAND_LIST_TYPE_COPY => QueueKind::Copy,
            _ => QueueKind::Other,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum QueueKind {
    Direct,
    Compute,
    Copy,
    Other,
}

impl QueueKind {
    fn name(self) -> &'static str {
        match self {
            QueueKind::Direct => "direct",
            QueueKind::Compute => "compute",
            QueueKind::Copy => "copy",
            QueueKind::Other => "other",
        }
    }

    fn list_type(self) -> D3D12_COMMAND_LIST_TYPE {
        match self {
            QueueKind::Compute => D3D12_COMMAND_LIST_TYPE_COMPUTE,
            QueueKind::Copy => D3D12_COMMAND_LIST_TYPE_COPY,
            _ => D3D12_COMMAND_LIST_TYPE_DIRECT,
        }
    }
}

// ---------------------------------------------------------------------------------------
// per-queue state
// ---------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SetState {
    /// nothing in it; the next frame of the queue may take it
    Free,
    /// takes stamps (the queue's current frame), or waits for the queue's next frame to be resolved
    Open,
    /// resolve submitted; the queue fence reaches the value when it has run
    Resolving(u64),
    /// the fence has passed; waits for the frame rule
    Ready,
    /// given up on (late); free again when its fence passes
    Late(u64),
    /// the device was removed
    Dead,
}

/// The persistently mapped readback buffer of a set.
struct Mapped(*const u64);

// SAFETY: it points into a READBACK buffer the set owns; it is read only after the set's fence.
unsafe impl Send for Mapped {}
unsafe impl Sync for Mapped {}

/// One frame's stamps of one queue.
struct StampSet {
    frame: u64,
    state: SetState,
    heap: ID3D12QueryHeap,
    readback: ID3D12Resource,
    mapped: Mapped,
    /// `stamps[k]` = `EndQuery(heap, k)`, recorded once and kept
    stamps: Vec<ID3D12GraphicsCommandList>,
    stamp_alloc: ID3D12CommandAllocator,
    resolve_alloc: ID3D12CommandAllocator,
    resolve_list: ID3D12GraphicsCommandList,
    used: u32,
    entries: Vec<ListEntry>,
}

impl StampSet {
    fn new(dev: &DevFns, kind: QueueKind, slots: u32) -> Option<StampSet> {
        let ty = kind.list_type();
        let heap_ty = if kind == QueueKind::Copy { D3D12_QUERY_HEAP_TYPE_COPY_QUEUE_TIMESTAMP } else { D3D12_QUERY_HEAP_TYPE_TIMESTAMP };
        let heap = dev.query_heap(heap_ty, slots)?;
        let readback = dev.readback(slots as u64 * 8)?;
        let mut p: *mut c_void = std::ptr::null_mut();
        // SAFETY: mapped once, for the life of the buffer (read-back heaps stay mapped).
        unsafe { readback.Map(0, None, Some(&mut p)).ok()? };
        if p.is_null() {
            return None;
        }
        let stamp_alloc = dev.allocator(ty)?;
        let resolve_alloc = dev.allocator(ty)?;
        let resolve_list = dev.list(ty, &resolve_alloc)?;
        // SAFETY: a new list is recording; closed so that it can be reset each frame.
        unsafe { resolve_list.Close().ok()? };
        Some(StampSet {
            frame: 0,
            state: SetState::Free,
            heap,
            readback,
            mapped: Mapped(p as *const u64),
            stamps: Vec::new(),
            stamp_alloc,
            resolve_alloc,
            resolve_list,
            used: 0,
            entries: Vec::new(),
        })
    }

    /// Makes sure `stamps[..upto]` exist (recorded once, closed, reused after the set's fence).
    fn ensure_stamps(&mut self, dev: &DevFns, kind: QueueKind, upto: u32) -> bool {
        while (self.stamps.len() as u32) < upto {
            let k = self.stamps.len() as u32;
            let Some(list) = dev.list(kind.list_type(), &self.stamp_alloc) else { return false };
            // SAFETY: the list is recording on its own recording turn (one list at a time per allocator).
            let ok = unsafe {
                list.EndQuery(&self.heap, D3D12_QUERY_TYPE_TIMESTAMP, k);
                list.Close().is_ok()
            };
            if !ok {
                return false;
            }
            self.stamps.push(list);
        }
        true
    }

    /// Records the resolve of `[0, used)` into the readback buffer; the list to submit first in
    /// the queue's next call. The set is Open, so the GPU is done with the allocator.
    /// With `pass` (part 3) the same list also resolves every pass block range the set's
    /// submissions used, into the pass readback buffer.
    fn record_resolve(&mut self, pass: Option<&PassPool>) -> Option<*mut c_void> {
        let ranges = pass.map(|_| resolve_ranges(&self.entries)).unwrap_or_default();
        // SAFETY: nothing of this set is in flight (its last resolve passed the fence before it was freed).
        unsafe {
            self.resolve_alloc.Reset().ok()?;
            self.resolve_list.Reset(&self.resolve_alloc, None::<&ID3D12PipelineState>).ok()?;
            self.resolve_list.ResolveQueryData(&self.heap, D3D12_QUERY_TYPE_TIMESTAMP, 0, self.used, &self.readback, 0);
            if let Some(p) = pass {
                for (start, count) in ranges {
                    self.resolve_list.ResolveQueryData(&p.heap, D3D12_QUERY_TYPE_TIMESTAMP, start, count, &p.readback, start as u64 * 8);
                }
            }
            self.resolve_list.Close().ok()?;
        }
        Some(self.resolve_list.as_raw())
    }
}

/// One submitted PC list.
struct ListEntry {
    frame: u64,
    seq: u64,
    list: usize,
    chunk: u64,
    /// first stamp slot (the end stamp is `slot + 1`); None without stamps
    slot: Option<u32>,
    cpu_us: u32,
    resubmitted: bool,
    origin: &'static str,
    why: Option<&'static str>,
    /// the pass stamps of this submission (part 3; Some only at tier pass)
    pass: Option<Box<PassRec>>,
}

/// A queue's clock, stamp sets and fence. The mutex is held across the driver call, so only
/// submitters to this queue contend; the telemetry thread uses `try_lock`.
struct QueueClock {
    q: QueueFns,
    kind: QueueKind,
    id: u32,
    /// None: below 1000 Hz, the call failed, or a queue that is not timed
    freq: Option<u64>,
    /// (GPU tick, CPU QPC) of the latest calibration
    cal: Option<(u64, i64)>,
    cal_frame: u64,
    /// `CopyQueueTimestampQueriesSupported` (copy queues)
    cap: bool,
    /// why this queue gets no stamps: `unsupported` or `no_frequency`
    why_off: Option<&'static str>,
    fence: Option<ID3D12Fence>,
    fence_value: u64,
    sets: Vec<StampSet>,
    cur: Option<usize>,
    cur_frame: u64,
    /// entries without stamps (null ticks with a `why`), written by the thread
    nulls: Vec<ListEntry>,
}

impl QueueClock {
    fn new(dev: &DevFns, q: QueueFns, id: u32, slots: u32, copy_ts: Option<bool>) -> QueueClock {
        let kind = q.kind();
        let cap = kind == QueueKind::Copy && copy_ts.unwrap_or_else(|| dev.copy_timestamps());
        let mut c = QueueClock {
            q,
            kind,
            id,
            freq: None,
            cal: None,
            cal_frame: 0,
            cap,
            why_off: None,
            fence: None,
            fence_value: 0,
            sets: Vec::new(),
            cur: None,
            cur_frame: u64::MAX,
            nulls: Vec::new(),
        };
        if !(matches!(kind, QueueKind::Direct | QueueKind::Compute) || cap) {
            c.why_off = Some("unsupported");
            return c;
        }
        c.freq = q.frequency().filter(|&f| f >= 1000);
        if c.freq.is_none() {
            c.why_off = Some("no_frequency");
            return c;
        }
        c.cal = q.calibration();
        let sets: Option<Vec<StampSet>> = (0..SETS).map(|_| StampSet::new(dev, kind, slots)).collect();
        match (sets, dev.fence()) {
            (Some(s), Some(f)) => {
                c.sets = s;
                c.fence = Some(f);
            }
            _ => c.why_off = Some("unsupported"),
        }
        c
    }

    /// The queue's first submission in `frame`: the old set (if it holds stamps) gets its resolve,
    /// returned to go first in the call; the next free set is opened when `open_next`.
    fn new_frame(&mut self, frame: u64, open_next: bool, pass: Option<&PassPool>) -> Option<(usize, *mut c_void)> {
        let prev = self.cur.take();
        self.cur_frame = frame;
        let mut resolve = None;
        if let Some(i) = prev {
            let s = &mut self.sets[i];
            if s.state == SetState::Open {
                if s.used == 0 {
                    s.state = SetState::Free;
                    s.entries.clear();
                } else if let Some(raw) = s.record_resolve(pass) {
                    resolve = Some((i, raw));
                } else {
                    // the resolve could not be recorded: nothing will be read
                    for mut e in std::mem::take(&mut s.entries) {
                        e.slot = None;
                        e.why = Some("unsupported");
                        if self.nulls.len() < NULLS_MAX {
                            self.nulls.push(e);
                        }
                    }
                    s.state = SetState::Free;
                    s.used = 0;
                    if self.kind == QueueKind::Copy {
                        // a copy queue that cannot resolve its stamps is not timed from now on
                        self.why_off = Some("unsupported");
                    }
                }
            }
        }
        if open_next && !self.sets.is_empty() {
            let n = self.sets.len();
            let start = prev.map_or(0, |p| p + 1);
            for k in 0..n {
                let i = (start + k) % n;
                if self.sets[i].state == SetState::Free {
                    let s = &mut self.sets[i];
                    s.state = SetState::Open;
                    s.frame = frame;
                    s.used = 0;
                    s.entries.clear();
                    self.cur = Some(i);
                    break;
                }
            }
        }
        resolve
    }
}

/// The `clock` event of a queue.
fn clock_event(c: &QueueClock, qpf: i64, frame: u64) -> Vec<(&'static str, Value)> {
    let (tick, qpc) = c.cal.unwrap_or((0, 0));
    let mut f = xlog::clock_fields(c.id, c.kind.name(), tick, qpc, c.freq, qpf);
    if c.cal.is_none() {
        for (k, v) in f.iter_mut() {
            if *k == "gpu_tick" || *k == "cpu_qpc" {
                *v = Value::Null;
            }
        }
        f.push(("why", Value::from(c.why_off.unwrap_or("calibration"))));
    }
    if c.kind == QueueKind::Copy {
        f.push(("copy_timestamps", Value::Bool(c.cap)));
    }
    // the engine's frame index when the calibration was taken
    f.push(("frame", Value::from(frame)));
    f
}

// ---------------------------------------------------------------------------------------
// engine
// ---------------------------------------------------------------------------------------

/// A list interval the harvest handed over for the frame record (null ms: no valid ticks).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct HarvestedList {
    pub frame: u64,
    pub queue: u32,
    pub start_ms: Option<f64>,
    pub end_ms: Option<f64>,
}

/// Entries of one queue that are ready to be written.
struct Done {
    qid: u32,
    kind: QueueKind,
    freq: Option<u64>,
    cal: Option<(u64, i64)>,
    entries: Vec<ListEntry>,
    /// the set's ticks; None: null with `why`
    ticks: Option<Vec<u64>>,
    why: Option<&'static str>,
}

pub(super) struct GpuTel {
    dev_make: Box<dyn Fn() -> Option<DevFns> + Send + Sync>,
    dev: OnceLock<DevFns>,
    queue_fns: Box<dyn Fn(*mut c_void) -> Option<QueueFns> + Send + Sync>,
    queues: RwLock<Vec<(usize, Arc<Mutex<QueueClock>>)>>,
    next_queue: AtomicU32,
    /// the current frame index (presents so far)
    frame: AtomicU64,
    seq: AtomicU64,
    chunk: AtomicU64,
    /// PC list -> (queue id, seq) of its submission whose stamps are not read yet
    in_flight: Mutex<HashMap<usize, (u32, u64)>>,
    /// full, late, unsupported, resubmitted, removed, sampled_out
    drops: [AtomicU64; 6],
    wake: (Mutex<bool>, Condvar),
    harvested: Mutex<VecDeque<HarvestedList>>,
    qpc0: i64,
    qpf: i64,
    /// `XCL_TELEMETRY_SAMPLE`
    sample: u32,
    /// stamps per set
    slots: u32,
    /// overrides the copy-queue capability (tests)
    copy_ts: Option<bool>,
    removed: AtomicBool,
    stopped: AtomicBool,
    /// debug counters of the "never read in the writing frame" rule
    reads: AtomicU64,
    early_reads: AtomicU64,
    /// lists submitted per frame: (frame low 32 bits << 32 | count), one slot per frame modulo 64
    counts: [AtomicU64; 64],
    /// the pass engine (part 3)
    pass: PassTel,
    /// the PC list's function `slot` (the original vtable of an adopted list; the object's own in tests)
    list_fn: unsafe fn(*mut c_void, usize) -> usize,
    /// submissions (seq) whose list was submitted again before they were read: their pass ticks are unusable
    poisoned: Mutex<HashSet<u64>>,
}

impl GpuTel {
    pub(super) fn new(
        dev_make: Box<dyn Fn() -> Option<DevFns> + Send + Sync>,
        queue_fns: Box<dyn Fn(*mut c_void) -> Option<QueueFns> + Send + Sync>,
        sample: u32,
        slots: u32,
        copy_ts: Option<bool>,
    ) -> GpuTel {
        GpuTel {
            dev_make,
            dev: OnceLock::new(),
            queue_fns,
            queues: RwLock::new(Vec::new()),
            next_queue: AtomicU32::new(0),
            frame: AtomicU64::new(0),
            seq: AtomicU64::new(0),
            chunk: AtomicU64::new(0),
            in_flight: Mutex::new(HashMap::new()),
            drops: Default::default(),
            wake: (Mutex::new(false), Condvar::new()),
            harvested: Mutex::new(VecDeque::new()),
            // the QPC value of the log's start, so `*_ms` are on the clock of part 1's `t`
            qpc0: dframe::qpc() - (xlog::uptime() * qpf() as f64) as i64,
            qpf: qpf(),
            sample: sample.max(1),
            slots: slots.clamp(2, LIST_SLOTS),
            copy_ts,
            removed: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            reads: AtomicU64::new(0),
            early_reads: AtomicU64::new(0),
            counts: std::array::from_fn(|_| AtomicU64::new(0)),
            pass: PassTel::new(),
            list_fn: orig_slot,
            poisoned: Mutex::new(HashSet::new()),
        }
    }

    /// Turns the pass engine on (tier pass); `plain_lists`: the lists are the objects' own, not adopted (tests).
    pub(super) fn with_pass(mut self, plain_lists: bool) -> GpuTel {
        self.pass.on.store(true, Ordering::Relaxed);
        if plain_lists {
            self.list_fn = plain_fn;
        }
        self
    }

    fn dev(&self) -> Option<DevFns> {
        if let Some(d) = self.dev.get() {
            return Some(*d);
        }
        let d = (self.dev_make)()?;
        Some(*self.dev.get_or_init(|| d))
    }

    /// The frame index and a wake-up of the thread.
    pub(super) fn set_frame(&self, n: u64) {
        self.frame.store(n, Ordering::Release);
        self.wake();
    }

    pub(super) fn frame(&self) -> u64 {
        self.frame.load(Ordering::Acquire)
    }

    fn wake(&self) {
        if let Some(mut g) = try_lock(&self.wake.0) {
            *g = true;
        }
        self.wake.1.notify_one();
    }

    /// Counters of the manifest: full, late, unsupported, resubmitted, removed, sampled_out.
    pub(super) fn drops(&self) -> [u64; 6] {
        std::array::from_fn(|i| self.drops[i].load(Ordering::Relaxed))
    }

    fn note(&self, which: usize, n: u64) {
        self.drops[which].fetch_add(n, Ordering::Relaxed);
    }

    pub(super) fn is_removed(&self) -> bool {
        self.removed.load(Ordering::Acquire)
    }

    /// Whether the title has shown a frame or submitted to a queue. Before that the exit flush has
    /// nothing to write, and must not stop the engine: `write_final` also runs once before the
    /// title starts (host.rs, the report written in case the title crashes).
    pub(super) fn started(&self) -> bool {
        self.frame() > 0 || !self.queues.read().unwrap_or_else(|e| e.into_inner()).is_empty()
    }

    /// Harvested list intervals since the last call (the frame glue drains it on the thread).
    pub(super) fn take_harvested(&self) -> Vec<HarvestedList> {
        plock(&self.harvested).drain(..).collect()
    }

    /// Counts `n` lists submitted in `frame`.
    fn note_lists(&self, frame: u64, n: u32) {
        let slot = &self.counts[(frame % 64) as usize];
        let tag = frame & 0xffff_ffff;
        let mut cur = slot.load(Ordering::Relaxed);
        loop {
            let new = if cur >> 32 == tag { (tag << 32) | ((cur & 0xffff_ffff) + n as u64).min(0xffff_ffff) } else { (tag << 32) | n as u64 };
            match slot.compare_exchange_weak(cur, new, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => return,
                Err(v) => cur = v,
            }
        }
    }

    /// Lists submitted in `frame` (0 once its slot was reused).
    fn lists_in(&self, frame: u64) -> u32 {
        let v = self.counts[(frame % 64) as usize].load(Ordering::Relaxed);
        if v >> 32 == frame & 0xffff_ffff {
            (v & 0xffff_ffff) as u32
        } else {
            0
        }
    }

    fn find_queue(&self, key: usize) -> Option<Arc<Mutex<QueueClock>>> {
        let g = self.queues.read().unwrap_or_else(|e| e.into_inner());
        g.iter().find(|(k, _)| *k == key).map(|(_, q)| q.clone())
    }

    fn all_queues(&self) -> Vec<Arc<Mutex<QueueClock>>> {
        self.queues.read().unwrap_or_else(|e| e.into_inner()).iter().map(|(_, q)| q.clone()).collect()
    }

    /// The clock of `key`, made (and its first `clock` event written) on first use.
    fn queue_clock(&self, key: *mut c_void, frame: u64, sink: Sink) -> Option<Arc<Mutex<QueueClock>>> {
        let k = key as usize;
        if let Some(q) = self.find_queue(k) {
            return Some(q);
        }
        let Some(dev) = self.dev() else {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| crate::xlog_warn!("telemetry: no PC device functions (device not created or a slot missing in the layout table) - lists are not timed"));
            return None;
        };
        let Some(qf) = (self.queue_fns)(key) else {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| crate::xlog_warn!("telemetry: no PC queue functions for queue {key:p} - its lists are not timed"));
            return None;
        };
        let (q, ev, copy_cap) = {
            let mut w = self.queues.write().unwrap_or_else(|e| e.into_inner());
            if let Some((_, q)) = w.iter().find(|(kk, _)| *kk == k) {
                return Some(q.clone());
            }
            if w.len() >= MAX_QUEUES {
                static ONCE: std::sync::Once = std::sync::Once::new();
                ONCE.call_once(|| crate::xlog_warn!("telemetry: more than {MAX_QUEUES} queues - lists of the others are not timed"));
                return None;
            }
            let id = self.next_queue.fetch_add(1, Ordering::Relaxed);
            let c = QueueClock::new(&dev, qf, id, self.slots, self.copy_ts);
            let ev = clock_event(&c, self.qpf, frame);
            let copy_cap = (c.kind == QueueKind::Copy).then_some(c.cap);
            let q = Arc::new(Mutex::new(c));
            w.push((k, q.clone()));
            (q, ev, copy_cap)
        };
        sink("clock", ev);
        if let Some(cap) = copy_cap {
            super::update_manifest(|m| m.copy_queue_timestamps = Some(cap));
        }
        Some(q)
    }

    /// The submit path: `submit` gets `lists` as they are when the lists are not stamped, else
    /// `[R?, S0, L0, S1, L1, ..., Sn]`. One driver call either way.
    pub(super) fn execute(&self, queue: *mut c_void, lists: &[*mut c_void], origin: &'static str, submit: impl FnOnce(&[*mut c_void]), sink: Sink) {
        let n = lists.len();
        if n == 0 {
            submit(lists);
            return;
        }
        if self.stopped.load(Ordering::Relaxed) {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| crate::xlog_warn!("telemetry: command lists submitted after the stamp engine finished are not timed (frame {})", self.frame()));
            submit(lists);
            return;
        }
        if self.removed.load(Ordering::Relaxed) {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| crate::xlog_warn!("telemetry: the device was removed - command lists are no longer timed"));
            self.note(DROP_REMOVED, n as u64);
            submit(lists);
            return;
        }
        let frame = self.frame.load(Ordering::Acquire);
        let sampled = xlog::sample_due(frame, self.sample);
        let Some(qc) = self.queue_clock(queue, frame, sink) else {
            self.note(DROP_UNSUPPORTED, n as u64);
            submit(lists);
            return;
        };
        let Some(mut guard) = lock_wait(&qc, Duration::from_secs(1)) else {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| crate::xlog_warn!("telemetry: a queue's stamp lock was busy for 1 s - lists submitted untimed"));
            self.note(DROP_FULL, n as u64);
            submit(lists);
            return;
        };
        let c: &mut QueueClock = &mut guard;

        // the queue's first call in a new frame: the old set's resolve goes first
        let resolve = if c.cur_frame != frame { c.new_frame(frame, sampled && c.why_off.is_none(), self.pass.pool.get()) } else { None };

        #[derive(Clone, Copy)]
        enum Plan {
            Skip,
            Null(&'static str),
            Stamp(usize, u32),
        }
        let plan = if !sampled {
            self.note(DROP_SAMPLED_OUT, n as u64);
            Plan::Skip
        } else if let Some(w) = c.why_off {
            Plan::Null(w)
        } else {
            match c.cur {
                None => Plan::Null("full"),
                Some(i) => {
                    let used = c.sets[i].used;
                    let upto = used as u64 + n as u64 + 1;
                    if upto > self.slots as u64 {
                        Plan::Null("full")
                    } else {
                        match self.dev() {
                            Some(d) if c.sets[i].ensure_stamps(&d, c.kind, upto as u32) => Plan::Stamp(i, used),
                            _ => {
                                if c.kind == QueueKind::Copy {
                                    c.why_off = Some("unsupported");
                                }
                                Plan::Null("unsupported")
                            }
                        }
                    }
                }
            }
        };

        // the pass stamps recorded into these lists (tier pass): each list's recording goes with its entry
        let mut recs: Vec<Option<Box<PassRec>>> = if self.pass.on.load(Ordering::Relaxed) { self.pass_take(lists, frame) } else { Vec::new() };

        let mut v: Vec<*mut c_void> = Vec::new();
        let stamped = matches!(plan, Plan::Stamp(..));
        if resolve.is_some() || stamped {
            v.reserve(2 * n + 2);
            if let Some((_, r)) = resolve {
                v.push(r);
            }
            if let Plan::Stamp(i, base) = plan {
                let s = &c.sets[i];
                for (k, l) in lists.iter().enumerate() {
                    v.push(s.stamps[base as usize + k].as_raw());
                    v.push(*l);
                }
                v.push(s.stamps[base as usize + n].as_raw());
            } else {
                v.extend_from_slice(lists);
            }
        }
        let send: &[*mut c_void] = if v.is_empty() { lists } else { &v };

        let seq0 = self.seq.fetch_add(n as u64, Ordering::Relaxed);
        let chunk = self.chunk.fetch_add(1, Ordering::Relaxed);
        let t0 = dframe::qpc();
        submit(send);
        let dt = (dframe::qpc() - t0).max(0) as i128;
        let cpu_us = (dt * 1_000_000 / self.qpf as i128).min(u32::MAX as i128) as u32;

        // the resolve ran ahead of the lists: its fence value says when the old set can be read
        if let (Some((i, _)), Some(f)) = (resolve, c.fence.as_ref()) {
            let value = c.fence_value + 1;
            c.fence_value = value;
            let hr = c.q.signal(f, value);
            if hr < 0 {
                static ONCE: std::sync::Once = std::sync::Once::new();
                ONCE.call_once(|| crate::xlog_warn!("telemetry: Signal on the stamp fence failed ({hr:#x}) - that frame's stamps will be late"));
            }
            c.sets[i].state = SetState::Resolving(value);
        }

        self.note_lists(frame, n as u32);
        if matches!(plan, Plan::Skip) {
            // their blocks stay out of the pool until the GPU has surely run the lists
            for r in recs.iter_mut().filter_map(Option::take) {
                self.pass_park(r, frame);
            }
            return;
        }
        let mut inflight = plock(&self.in_flight);
        for (k, l) in lists.iter().enumerate() {
            let (list, seq) = (*l as usize, seq0 + k as u64);
            // the same recording again: a list that was Reset since its last submission is a new one
            let earlier = inflight.get(&list).copied();
            let resubmitted = earlier.is_some();
            if resubmitted {
                self.note(DROP_RESUBMITTED, 1);
                if let (Some((_, before)), true) = (earlier, self.pass.on.load(Ordering::Relaxed)) {
                    let mut p = plock(&self.poisoned);
                    if p.len() >= POISONED_MAX {
                        p.clear();
                    }
                    p.insert(before);
                }
            }
            let (slot, why) = match plan {
                Plan::Stamp(_, base) => (Some(base + k as u32), None),
                Plan::Null(w) => (None, Some(w)),
                Plan::Skip => (None, None),
            };
            if slot.is_some() && (resubmitted || inflight.len() < IN_FLIGHT_MAX) {
                inflight.insert(list, (c.id, seq));
            }
            let pass = recs.get_mut(k).and_then(Option::take);
            let e = ListEntry { frame, seq, list, chunk, slot, cpu_us, resubmitted, origin, why, pass };
            match plan {
                Plan::Stamp(i, _) => c.sets[i].entries.push(e),
                _ if c.nulls.len() < NULLS_MAX => c.nulls.push(e),
                _ => {
                    self.note(DROP_FULL, 1);
                    if let Some(r) = e.pass {
                        self.pass_park(r, frame);
                    }
                }
            }
        }
        drop(inflight);
        if let Plan::Stamp(i, _) = plan {
            c.sets[i].used += n as u32 + 1;
        }
        drop(guard);
        self.wake();
    }

    /// Steps 1-3 of the thread: fence check, the frame rule, ticks copied under the lock, `list`
    /// events written after it. Never waits (queues that are busy are left for the next time).
    pub(super) fn harvest(&self, sink: Sink) {
        let frame = self.frame.load(Ordering::Acquire);
        for q in self.all_queues() {
            let mut done = Vec::new();
            {
                let Some(mut c) = try_lock(&q) else { continue };
                self.collect(&mut c, frame, &mut done);
            }
            for d in done {
                self.emit(d, sink);
            }
        }
        self.pass_release_parked(frame);
    }

    fn collect(&self, c: &mut QueueClock, frame: u64, out: &mut Vec<Done>) {
        let (qid, kind, freq, cal) = (c.id, c.kind, c.freq, c.cal);
        let mk = |entries: Vec<ListEntry>, ticks: Option<Vec<u64>>, why: Option<&'static str>| Done { qid, kind, freq, cal, entries, ticks, why };
        if !c.nulls.is_empty() {
            out.push(mk(std::mem::take(&mut c.nulls), None, None));
        }
        // SAFETY: a fence of ours; GetCompletedValue is a CPU-side read.
        let completed = c.fence.as_ref().map(|f| unsafe { f.GetCompletedValue() });
        if completed == Some(u64::MAX) {
            self.removed.store(true, Ordering::Release);
        }
        let removed = self.removed.load(Ordering::Acquire);
        for s in c.sets.iter_mut() {
            if removed {
                if s.state != SetState::Dead {
                    if !s.entries.is_empty() {
                        out.push(mk(std::mem::take(&mut s.entries), None, Some("removed")));
                    }
                    s.state = SetState::Dead;
                    s.used = 0;
                }
                continue;
            }
            let done_value = completed.unwrap_or(0);
            if let SetState::Resolving(v) = s.state {
                if done_value >= v {
                    s.state = SetState::Ready;
                } else if frame >= s.frame + LATE_FRAMES {
                    out.push(mk(std::mem::take(&mut s.entries), None, Some("late")));
                    s.state = SetState::Late(v);
                    s.used = 0;
                }
            }
            match s.state {
                SetState::Open if frame >= s.frame + LATE_FRAMES => {
                    // a queue that went idle: its resolve was never submitted
                    if !s.entries.is_empty() {
                        out.push(mk(std::mem::take(&mut s.entries), None, Some("late")));
                    }
                    s.state = SetState::Free;
                    s.used = 0;
                }
                SetState::Late(v) if done_value >= v => s.state = SetState::Free,
                SetState::Ready if frame >= s.frame + 2 => {
                    let ticks = self.read_ticks(s, frame);
                    if ticks.is_some() {
                        for e in s.entries.iter_mut() {
                            if let Some(r) = e.pass.as_mut() {
                                let t = self.read_pass_ticks(r);
                                r.ticks = t;
                            }
                        }
                    }
                    let why = if ticks.is_none() { Some("late") } else { None };
                    out.push(mk(std::mem::take(&mut s.entries), ticks, why));
                    s.state = SetState::Free;
                    s.used = 0;
                }
                _ => {}
            }
        }
    }

    /// Copies the set's `used` ticks out of the mapped readback buffer. The one place a set is
    /// read: refused (and counted) before `frame >= set.frame + 2`.
    fn read_ticks(&self, s: &StampSet, frame: u64) -> Option<Vec<u64>> {
        if frame < s.frame + 2 {
            self.early_reads.fetch_add(1, Ordering::Relaxed);
            debug_assert!(false, "set read before frame >= set.frame + 2");
            return None;
        }
        self.reads.fetch_add(1, Ordering::Relaxed);
        // SAFETY: the buffer holds `slots` ticks and `used <= slots`; its resolve passed the fence.
        Some(unsafe { std::slice::from_raw_parts(s.mapped.0, s.used as usize) }.to_vec())
    }

    /// ms since the engine started for a GPU tick, through the queue's latest calibration.
    fn tick_ms(&self, d: &Done, tick: u64) -> Option<f64> {
        let (freq, (ct, cq)) = (d.freq?, d.cal?);
        let qpc = xlog::tick_to_qpc(tick, ct, cq, freq, self.qpf)?;
        Some((qpc - self.qpc0) as f64 * 1000.0 / self.qpf as f64)
    }

    /// Writes the `list` events of `d` and hands the intervals over.
    fn emit(&self, mut d: Done, sink: Sink) {
        let recs: Vec<Option<Box<PassRec>>> = d.entries.iter_mut().map(|e| e.pass.take()).collect();
        for (e, rec) in d.entries.iter().zip(recs) {
            let mut why = e.why.or(d.why);
            let (mut st, mut et, mut sm, mut em) = (None, None, None, None);
            if why.is_none() {
                match (&d.ticks, e.slot) {
                    (Some(t), Some(s)) => match (t.get(s as usize).copied(), t.get(s as usize + 1).copied()) {
                        (Some(a), Some(b)) if a == 0 || b == 0 => why = Some("zero"),
                        (Some(a), Some(b)) if b < a => why = Some("order"),
                        (Some(a), Some(b)) => match (self.tick_ms(&d, a), self.tick_ms(&d, b)) {
                            (Some(x), Some(y)) => (st, et, sm, em) = (Some(a), Some(b), Some(x), Some(y)),
                            _ => why = Some("no_frequency"),
                        },
                        _ => why = Some("zero"),
                    },
                    _ => why = Some("late"),
                }
            }
            match why {
                Some("full") => self.note(DROP_FULL, 1),
                Some("late") => self.note(DROP_LATE, 1),
                Some("unsupported") => self.note(DROP_UNSUPPORTED, 1),
                Some("removed") => self.note(DROP_REMOVED, 1),
                _ => {}
            }
            let facts = ListFacts {
                frame: e.frame,
                seq: e.seq,
                id: e.list as u64,
                chunk: e.chunk,
                queue: d.qid,
                queue_type: d.kind.name(),
                start_tick: st,
                end_tick: et,
                start_ms: sm,
                end_ms: em,
                cpu_us: e.cpu_us,
                resubmitted: e.resubmitted,
                origin: e.origin,
                why,
            };
            sink("list", xlog::list_fields(&facts));
            let mut h = plock(&self.harvested);
            if h.len() < HARVESTED_MAX {
                h.push_back(HarvestedList { frame: e.frame, queue: d.qid, start_ms: sm, end_ms: em });
            }
            drop(h);
            if let Some(mut rec) = rec {
                let resub = plock(&self.poisoned).remove(&e.seq) | e.resubmitted;
                let ctx = PassCtx { frame: e.frame, seq: e.seq, queue: d.qid, resubmitted: resub, list_why: why, start: st, end: et };
                for p in pass_facts(&rec, &ctx, &|t| self.tick_ms(&d, t)) {
                    sink("pass", xlog::pass_fields(&p));
                }
                // read (or never resolved because the set was given up on): free at once; a submission
                // that was not stamped may still be on the GPU: parked
                let blocks = std::mem::take(&mut rec.blocks);
                if e.slot.is_some() || d.why == Some("removed") {
                    self.pass_free(blocks);
                } else {
                    self.pass_park_blocks(blocks, self.frame());
                }
            }
        }
        // the stamps of these submissions are read: their lists are no longer "in flight"
        if d.entries.iter().any(|e| e.slot.is_some()) {
            let mut m = plock(&self.in_flight);
            for e in &d.entries {
                if m.get(&e.list) == Some(&(d.qid, e.seq)) {
                    m.remove(&e.list);
                }
            }
        }
    }

    /// Calibrates every timed queue again and writes its `clock` event (the thread, every 60 frames).
    pub(super) fn refresh_clocks(&self, sink: Sink) {
        let frame = self.frame();
        for q in self.all_queues() {
            let ev = {
                let Some(mut c) = try_lock(&q) else { continue };
                if c.freq.is_none() {
                    continue;
                }
                if let Some(cal) = c.q.calibration() {
                    c.cal = Some(cal);
                    c.cal_frame = frame;
                }
                clock_event(&c, self.qpf, frame)
            };
            sink("clock", ev);
        }
    }

    /// The exit flush: a last non-blocking harvest, then every pending list written with null
    /// ticks (`exit`, or `removed` when the device is gone). Locks are taken for at most 200 ms.
    pub(super) fn finish(&self, sink: Sink) {
        self.harvest(sink);
        self.stopped.store(true, Ordering::Release);
        for q in self.all_queues() {
            let mut done = Vec::new();
            {
                let Some(mut c) = lock_wait(&q, Duration::from_millis(200)) else { continue };
                let why = if self.removed.load(Ordering::Acquire) { "removed" } else { "exit" };
                let (qid, kind, freq, cal) = (c.id, c.kind, c.freq, c.cal);
                let mk = |entries: Vec<ListEntry>, why: Option<&'static str>| Done { qid, kind, freq, cal, entries, ticks: None, why };
                if !c.nulls.is_empty() {
                    done.push(mk(std::mem::take(&mut c.nulls), None));
                }
                for s in c.sets.iter_mut() {
                    if !s.entries.is_empty() {
                        done.push(mk(std::mem::take(&mut s.entries), Some(why)));
                    }
                    if s.state != SetState::Free {
                        s.state = SetState::Dead;
                    }
                    s.used = 0;
                }
            }
            for d in done {
                self.emit(d, sink);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// part 3: pass stamps
// ---------------------------------------------------------------------------------------

/// What one pass (the span between two barrier batches of a list) did.
#[derive(Clone, Default, Debug, PartialEq)]
struct Counts {
    draws: u32,
    dispatches: u32,
    indirect: u32,
    /// telemetry ids of the distinct pipelines drawn or dispatched with, first seen first (at most `MAX_PIPES`)
    pipes: Vec<u64>,
    truncated: bool,
}

impl Counts {
    fn pipe(&mut self, id: u64) {
        if self.pipes.contains(&id) {
            return;
        }
        if self.pipes.len() < MAX_PIPES {
            self.pipes.push(id);
        } else {
            self.truncated = true;
        }
    }

    fn merge(&mut self, o: &Counts) {
        self.draws = self.draws.saturating_add(o.draws);
        self.dispatches = self.dispatches.saturating_add(o.dispatches);
        self.indirect = self.indirect.saturating_add(o.indirect);
        for p in &o.pipes {
            self.pipe(*p);
        }
        self.truncated |= o.truncated;
    }

    fn is_empty(&self) -> bool {
        *self == Counts::default()
    }

    fn kind(&self) -> &'static str {
        if self.draws > 0 {
            "draw"
        } else if self.dispatches > 0 {
            "dispatch"
        } else if self.indirect > 0 {
            "indirect"
        } else {
            "empty"
        }
    }
}

/// The pass stamps of one recording of a PC list. Boundary `i` took local slot `i`; pass `k` runs
/// from boundary `k - 1` (pass 0: the list's start stamp) to boundary `k` (the last pass: the
/// list's end stamp).
struct PassRec {
    list: usize,
    /// the frame of the first note (a recording that was never submitted or Reset is swept after `LATE_FRAMES`)
    frame: u64,
    typed: bool,
    /// why no more stamps are taken: `copy_list`, `bundle`, `unsupported`, `full`, `lock_busy`
    why: Option<&'static str>,
    blocks: Vec<u16>,
    /// stamps taken
    total: u32,
    /// barriers of the batch each boundary stands before
    bounds: Vec<u32>,
    /// `bounds.len() + 1` passes
    passes: Vec<Counts>,
    /// the resolved ticks by local slot (the harvest fills them)
    ticks: Vec<u64>,
}

impl PassRec {
    fn new(list: usize, frame: u64) -> PassRec {
        PassRec { list, frame, typed: false, why: None, blocks: Vec::new(), total: 0, bounds: Vec::new(), passes: vec![Counts::default()], ticks: Vec::new() }
    }

    /// A recording that could not be taken (the one pass of the list says why).
    fn without_stamps(list: usize, frame: u64, why: &'static str) -> PassRec {
        PassRec { typed: true, why: Some(why), ..PassRec::new(list, frame) }
    }

    /// (first slot, count) of each block the stamps went into.
    fn ranges(&self) -> Vec<(u32, u32)> {
        self.blocks
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| {
                let n = self.total.saturating_sub(i as u32 * BLOCK_SLOTS).min(BLOCK_SLOTS);
                (n > 0).then_some((b as u32 * BLOCK_SLOTS, n))
            })
            .collect()
    }

    /// The global slot of local slot `j`.
    fn global(&self, j: u32) -> Option<u32> {
        let b = *self.blocks.get((j / BLOCK_SLOTS) as usize)?;
        Some(b as u32 * BLOCK_SLOTS + j % BLOCK_SLOTS)
    }
}

/// The block ranges of every pass recording of `entries`, adjacent ones joined.
fn resolve_ranges(entries: &[ListEntry]) -> Vec<(u32, u32)> {
    let mut all: Vec<(u32, u32)> = entries.iter().filter_map(|e| e.pass.as_ref()).flat_map(|r| r.ranges()).collect();
    all.sort_unstable();
    let mut out: Vec<(u32, u32)> = Vec::with_capacity(all.len());
    for (s, n) in all {
        match out.last_mut() {
            Some(l) if l.0 + l.1 == s => l.1 += n,
            _ => out.push((s, n)),
        }
    }
    out
}

/// Recordings by list and the pool of blocks, under one mutex. Nothing in a section that holds
/// it calls out.
struct PassState {
    free: Vec<u16>,
    recs: HashMap<usize, PassRec>,
    /// blocks of submissions that were not stamped: free again once the GPU has surely run them
    parked: VecDeque<(u64, Vec<u16>)>,
    last_sweep: Option<u64>,
}

impl PassState {
    fn new(blocks: usize) -> PassState {
        PassState { free: (0..blocks as u16).rev().collect(), recs: HashMap::new(), parked: VecDeque::new(), last_sweep: None }
    }

    /// Recordings that were never submitted or Reset for `LATE_FRAMES` give their blocks back (at most once per frame).
    fn sweep(&mut self, frame: u64) {
        if self.last_sweep == Some(frame) {
            return;
        }
        self.last_sweep = Some(frame);
        let stale: Vec<usize> = self.recs.iter().filter(|(_, r)| r.frame + LATE_FRAMES < frame).map(|(l, _)| *l).collect();
        for l in stale {
            if let Some(r) = self.recs.remove(&l) {
                self.free.extend(r.blocks);
            }
        }
    }

    /// Makes sure the list has a recording; false when `RECS_MAX` recordings wait.
    fn ensure(&mut self, list: usize, frame: u64) -> bool {
        if self.recs.contains_key(&list) {
            return true;
        }
        if self.recs.len() >= RECS_MAX {
            self.sweep(frame);
            if self.recs.len() >= RECS_MAX {
                return false;
            }
        }
        self.recs.insert(list, PassRec::new(list, frame));
        true
    }

    /// Adds counters to the list's current pass.
    fn merge_counts(&mut self, list: usize, c: Counts, frame: u64) {
        if self.ensure(list, frame) {
            if let Some(p) = self.recs.get_mut(&list).and_then(|r| r.passes.last_mut()) {
                p.merge(&c);
            }
        }
    }

    /// A pass boundary: the next stamp slot of the recording (a global slot), or why none is taken.
    fn boundary(&mut self, list: usize, barriers: u32, frame: u64) -> Result<u32, &'static str> {
        if !self.ensure(list, frame) {
            return Err("full");
        }
        let (why, total, nblocks) = match self.recs.get(&list) {
            Some(r) => (r.why, r.total, r.blocks.len()),
            None => return Err("full"),
        };
        if let Some(w) = why {
            return Err(w);
        }
        let new_block = total % BLOCK_SLOTS == 0;
        if new_block && nblocks < MAX_REC_BLOCKS && self.free.is_empty() {
            self.sweep(frame);
        }
        let block = if new_block {
            if nblocks >= MAX_REC_BLOCKS {
                None
            } else {
                self.free.pop()
            }
        } else {
            Some(0)
        };
        let Some(r) = self.recs.get_mut(&list) else { return Err("full") };
        if new_block {
            match block {
                Some(b) => r.blocks.push(b),
                None => {
                    r.why = Some("full");
                    return Err("full");
                }
            }
        }
        let slot = r.global(total).ok_or("full")?;
        r.total += 1;
        r.bounds.push(barriers);
        r.passes.push(Counts::default());
        Ok(slot)
    }

    /// The list was Reset: its recording was never submitted (a submission took it), so its stamps
    /// never run and its blocks are free. Returns the number of blocks given back.
    fn reset(&mut self, list: usize) -> usize {
        match self.recs.remove(&list) {
            Some(r) => {
                let n = r.blocks.len();
                self.free.extend(r.blocks);
                n
            }
            None => 0,
        }
    }

    /// The recording of a list that is being submitted (an empty one when it has none).
    fn take(&mut self, list: usize, frame: u64) -> Box<PassRec> {
        Box::new(self.recs.remove(&list).unwrap_or_else(|| PassRec::new(list, frame)))
    }

    fn free_blocks(&mut self, blocks: Vec<u16>) {
        self.free.extend(blocks);
    }

    fn park(&mut self, frame: u64, blocks: Vec<u16>) {
        if !blocks.is_empty() {
            self.parked.push_back((frame, blocks));
        }
    }

    fn release_parked(&mut self, frame: u64) {
        while self.parked.front().is_some_and(|(f, _)| f + LATE_FRAMES <= frame) {
            if let Some((_, b)) = self.parked.pop_front() {
                self.free.extend(b);
            }
        }
    }
}

/// The global pass heap and its readback buffer (never freed: lists in flight may name the heap).
struct PassPool {
    heap: ID3D12QueryHeap,
    readback: ID3D12Resource,
    mapped: Mapped,
}

// SAFETY: free-threaded D3D12 objects; the mapped pointer is read only after a set's fence.
unsafe impl Send for PassPool {}
unsafe impl Sync for PassPool {}

impl PassPool {
    fn new(dev: &DevFns) -> Option<PassPool> {
        let heap = dev.query_heap(D3D12_QUERY_HEAP_TYPE_TIMESTAMP, PASS_SLOTS)?;
        let readback = dev.readback(PASS_SLOTS as u64 * 8)?;
        let mut p: *mut c_void = std::ptr::null_mut();
        // SAFETY: mapped once, for the life of the buffer.
        unsafe { readback.Map(0, None, Some(&mut p)).ok()? };
        if p.is_null() {
            return None;
        }
        Some(PassPool { heap, readback, mapped: Mapped(p as *const u64) })
    }
}

/// The pass engine of a `GpuTel`.
struct PassTel {
    /// tier pass
    on: AtomicBool,
    state: Mutex<PassState>,
    pool: OnceLock<PassPool>,
    /// held while the pool is made
    init: Mutex<()>,
    failed: AtomicBool,
    /// notes dropped because the state lock was busy
    busy: AtomicU64,
    /// boundaries not stamped because the pool or the recording was full
    full: AtomicU64,
    /// the counters of the current pass of each PC list
    counts: CountMap,
}

impl PassTel {
    fn new() -> PassTel {
        PassTel {
            on: AtomicBool::new(false),
            state: Mutex::new(PassState::new(BLOCKS)),
            pool: OnceLock::new(),
            init: Mutex::new(()),
            failed: AtomicBool::new(false),
            busy: AtomicU64::new(0),
            full: AtomicU64::new(0),
            counts: CountMap::new(LIST_COUNTS_MAX),
        }
    }
}

/// Counters of the current pass of one PC list: written by the thread recording it (no lock),
/// drained under the pass state lock at a boundary, at submit (any thread) and at Reset.
struct ListCounts {
    draws: AtomicU32,
    dispatches: AtomicU32,
    indirect: AtomicU32,
    pipes: [AtomicU64; MAX_PIPES],
    npipes: AtomicU32,
    truncated: AtomicBool,
}

impl ListCounts {
    fn new() -> ListCounts {
        ListCounts {
            draws: AtomicU32::new(0),
            dispatches: AtomicU32::new(0),
            indirect: AtomicU32::new(0),
            pipes: std::array::from_fn(|_| AtomicU64::new(0)),
            npipes: AtomicU32::new(0),
            truncated: AtomicBool::new(false),
        }
    }

    /// Single writer (the recording thread): appended when not among `pipes[..npipes]`.
    fn note_pipe(&self, id: u64) {
        let n = self.npipes.load(Ordering::Acquire) as usize;
        if self.pipes.iter().take(n).any(|p| p.load(Ordering::Relaxed) == id) {
            return;
        }
        if n < MAX_PIPES {
            self.pipes[n].store(id, Ordering::Relaxed);
            self.npipes.store(n as u32 + 1, Ordering::Release);
        } else {
            self.truncated.store(true, Ordering::Relaxed);
        }
    }

    /// Takes the counters (they are zeroed) as a `Counts`.
    fn drain(&self) -> Counts {
        let draws = self.draws.swap(0, Ordering::AcqRel);
        let dispatches = self.dispatches.swap(0, Ordering::AcqRel);
        let indirect = self.indirect.swap(0, Ordering::AcqRel);
        let n = (self.npipes.swap(0, Ordering::AcqRel) as usize).min(MAX_PIPES);
        let pipes = self.pipes.iter().take(n).map(|p| p.load(Ordering::Relaxed)).collect();
        let truncated = self.truncated.swap(false, Ordering::AcqRel);
        Counts { draws, dispatches, indirect, pipes, truncated }
    }
}

const COUNT_SHARDS: usize = 16;
const LIST_COUNTS_MAX: usize = 65_536;

/// PC list pointer -> its counters. An entry is never removed (a pointer reused by a new list gets
/// its counters zeroed at creation / Reset), so a cached `Arc` stays valid.
struct CountMap {
    shards: [RwLock<HashMap<usize, Arc<ListCounts>>>; COUNT_SHARDS],
    len: AtomicUsize,
    cap: usize,
}

impl CountMap {
    fn new(cap: usize) -> CountMap {
        CountMap { shards: std::array::from_fn(|_| RwLock::new(HashMap::new())), len: AtomicUsize::new(0), cap }
    }

    fn shard(list: usize) -> usize {
        (list >> 6) % COUNT_SHARDS
    }

    fn get(&self, list: usize) -> Option<Arc<ListCounts>> {
        self.shards[Self::shard(list)].read().unwrap_or_else(|e| e.into_inner()).get(&list).cloned()
    }

    /// The counters of `list`, made when new; None when the map is full.
    fn get_or_insert(&self, list: usize) -> Option<Arc<ListCounts>> {
        if let Some(c) = self.get(list) {
            return Some(c);
        }
        let mut w = self.shards[Self::shard(list)].write().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = w.get(&list) {
            return Some(c.clone());
        }
        if self.len.load(Ordering::Relaxed) >= self.cap {
            return None;
        }
        let c = Arc::new(ListCounts::new());
        w.insert(list, c.clone());
        self.len.fetch_add(1, Ordering::Relaxed);
        Some(c)
    }
}

thread_local! {
    /// (engine address, list, its counters): the last list this thread noted
    static COUNT_CACHE: RefCell<(usize, usize, Option<Arc<ListCounts>>)> = const { RefCell::new((0, 0, None)) };
}

/// `GetType` and `EndQuery` of the list interface.
fn list_method_slots() -> Option<(usize, usize)> {
    static S: OnceLock<Option<(usize, usize)>> = OnceLock::new();
    *S.get_or_init(|| Some((method_slot("ID3D12GraphicsCommandList", "GetType")?, method_slot("ID3D12GraphicsCommandList", "EndQuery")?)))
}

/// Why a list of this `GetType` value gets no pass stamps.
fn list_type_why(ty: i32) -> Option<&'static str> {
    match ty {
        t if t == D3D12_COMMAND_LIST_TYPE_DIRECT.0 || t == D3D12_COMMAND_LIST_TYPE_COMPUTE.0 => None,
        t if t == D3D12_COMMAND_LIST_TYPE_BUNDLE.0 => Some("bundle"),
        t if t == D3D12_COMMAND_LIST_TYPE_COPY.0 => Some("copy_list"),
        _ => Some("unsupported"),
    }
}

impl GpuTel {
    /// Whether pass notes are taken now: tier pass, and a frame with stamps.
    fn pass_sampled(&self) -> bool {
        self.pass.on.load(Ordering::Relaxed) && xlog::sample_due(self.frame(), self.sample)
    }

    /// A pass note that was dropped because the state lock was busy.
    fn pass_busy(&self) {
        self.pass.busy.fetch_add(1, Ordering::Relaxed);
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| crate::xlog_warn!("telemetry: the pass state lock was busy for {NOTE_WAIT:?} - some pass notes are dropped (counted as pass_busy in the manifest)"));
    }

    fn pass_pool(&self) -> Option<&PassPool> {
        if let Some(p) = self.pass.pool.get() {
            return Some(p);
        }
        if self.pass.failed.load(Ordering::Relaxed) {
            return None;
        }
        let _g = try_lock(&self.pass.init)?;
        if let Some(p) = self.pass.pool.get() {
            return Some(p);
        }
        let dev = self.dev()?;
        match PassPool::new(&dev) {
            Some(p) => {
                let _ = self.pass.pool.set(p);
                self.pass.pool.get()
            }
            None => {
                self.pass.failed.store(true, Ordering::Relaxed);
                crate::xlog_warn!("telemetry: cannot make the pass timestamp heap or its readback buffer - no pass stamps this run");
                None
            }
        }
    }

    /// The list's `GetType` through the PC function; None when the slot is unknown.
    fn list_type(&self, list: usize) -> Option<i32> {
        let (get_type, _) = list_method_slots()?;
        // SAFETY: `GetType` of the list's own (original) vtable.
        unsafe {
            let f: unsafe extern "system" fn(*mut c_void) -> i32 = std::mem::transmute((self.list_fn)(list as *mut c_void, get_type));
            Some(f(list as *mut c_void))
        }
    }

    /// `EndQuery(heap, TIMESTAMP, slot)` on the PC list through its original function.
    unsafe fn end_query(&self, list: usize, heap: &ID3D12QueryHeap, slot: u32) -> bool {
        let Some((_, end_query)) = list_method_slots() else { return false };
        let f: unsafe extern "system" fn(*mut c_void, *mut c_void, i32, u32) = std::mem::transmute((self.list_fn)(list as *mut c_void, end_query));
        f(list as *mut c_void, heap.as_raw(), D3D12_QUERY_TYPE_TIMESTAMP.0, slot);
        true
    }

    /// Counters (draws, dispatches, pipelines) of the list being recorded, through this thread's
    /// cache: no lock on a hit. Counting does not depend on sampling (only stamps do).
    fn pass_note(&self, list: *mut c_void, apply: impl FnOnce(&ListCounts)) {
        if !self.pass.on.load(Ordering::Relaxed) || list.is_null() {
            return;
        }
        let (me, l) = (self as *const GpuTel as usize, list as usize);
        let mut apply = Some(apply);
        let hit = COUNT_CACHE.with(|c| {
            let c = c.borrow();
            match &c.2 {
                Some(a) if c.0 == me && c.1 == l => {
                    if let Some(f) = apply.take() {
                        f(&**a);
                    }
                    true
                }
                _ => false,
            }
        });
        if hit {
            return;
        }
        let Some(arc) = self.pass.counts.get_or_insert(l) else {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| crate::xlog_warn!("telemetry: more than {LIST_COUNTS_MAX} command lists - later ones get no pass counters"));
            return;
        };
        COUNT_CACHE.with(|c| *c.borrow_mut() = (me, l, Some(arc.clone())));
        if let Some(f) = apply.take() {
            f(&*arc);
        }
    }

    /// Moves the counters of `l` into its current pass in `st`.
    fn drain_into(&self, st: &mut PassState, l: usize, frame: u64) {
        if let Some(c) = self.pass.counts.get(l) {
            let d = c.drain();
            if !d.is_empty() {
                st.merge_counts(l, d, frame);
            }
        }
    }

    /// Takes the counters of `l` (empty when it has none).
    fn drain_counts(&self, l: usize) -> Counts {
        self.pass.counts.get(l).map(|c| c.drain()).unwrap_or_default()
    }

    /// A barrier batch of `barriers` barriers is about to be recorded into `list`: one timestamp
    /// before it, on the PC list through its original `EndQuery`. The batch drains the list anyway, so
    /// the stamp adds almost no serialisation.
    pub(super) fn pass_boundary(&self, list: *mut c_void, barriers: u32) {
        if !self.pass_sampled() || list.is_null() {
            return;
        }
        let (l, frame) = (list as usize, self.frame());
        let need_type = {
            let Some(mut st) = lock_wait(&self.pass.state, NOTE_WAIT) else { return self.pass_busy() };
            self.drain_into(&mut st, l, frame);
            st.ensure(l, frame) && st.recs.get(&l).is_some_and(|r| !r.typed)
        };
        if need_type {
            // once per recording, outside the lock: a copy list or a bundle gets no stamps
            let why = self.list_type(l).and_then(list_type_why);
            let Some(mut st) = lock_wait(&self.pass.state, NOTE_WAIT) else { return self.pass_busy() };
            if let Some(r) = st.recs.get_mut(&l) {
                r.typed = true;
                r.why = why;
            }
        }
        let Some(pool) = self.pass_pool() else { return };
        let slot = {
            let Some(mut st) = lock_wait(&self.pass.state, NOTE_WAIT) else { return self.pass_busy() };
            st.boundary(l, barriers, frame)
        };
        match slot {
            // SAFETY: the list is being recorded; the heap lives as long as the engine.
            Ok(s) => {
                if !unsafe { self.end_query(l, &pool.heap, s) } {
                    static ONCE: std::sync::Once = std::sync::Once::new();
                    ONCE.call_once(|| crate::xlog_warn!("telemetry: no EndQuery slot in the layout table - no pass stamps"));
                }
            }
            Err("full") => {
                self.pass.full.fetch_add(1, Ordering::Relaxed);
                static ONCE: std::sync::Once = std::sync::Once::new();
                ONCE.call_once(|| crate::xlog_warn!("telemetry: the pass stamp pool or a recording's {MAX_REC_BLOCKS} blocks are full - later passes of those lists are not split (counted as pass_full)"));
            }
            Err(_) => {} // a copy list or a bundle: its one pass says why
        }
    }

    /// The list was Reset: a new recording, never "resubmitted". Its unsubmitted pass stamps are dropped.
    pub(super) fn list_reset(&self, list: *mut c_void, pass: bool) {
        let l = list as usize;
        match lock_wait(&self.in_flight, NOTE_WAIT) {
            Some(mut m) => {
                m.remove(&l);
            }
            None => {
                static ONCE: std::sync::Once = std::sync::Once::new();
                ONCE.call_once(|| crate::xlog_warn!("telemetry: the in-flight lock was busy at a list Reset - that list may be flagged resubmitted"));
            }
        }
        if pass && self.pass.on.load(Ordering::Relaxed) {
            if let Some(c) = self.pass.counts.get(l) {
                c.drain();
            }
            match lock_wait(&self.pass.state, NOTE_WAIT) {
                Some(mut st) => {
                    st.reset(l);
                }
                None => self.pass_busy(),
            }
        }
    }

    /// Submit: each list's recording leaves the map with the list (an empty one for a list without).
    fn pass_take(&self, lists: &[*mut c_void], frame: u64) -> Vec<Option<Box<PassRec>>> {
        let Some(mut st) = lock_wait(&self.pass.state, Duration::from_millis(5)) else {
            self.pass_busy();
            return lists
                .iter()
                .map(|l| {
                    let d = self.drain_counts(*l as usize);
                    let mut rec = PassRec::without_stamps(*l as usize, frame, "lock_busy");
                    if let Some(p) = rec.passes.last_mut() {
                        p.merge(&d);
                    }
                    Some(Box::new(rec))
                })
                .collect();
        };
        lists
            .iter()
            .map(|l| {
                let d = self.drain_counts(*l as usize);
                let mut rec = st.take(*l as usize, frame);
                if let Some(p) = rec.passes.last_mut() {
                    p.merge(&d);
                }
                Some(rec)
            })
            .collect()
    }

    fn pass_free(&self, blocks: Vec<u16>) {
        if !blocks.is_empty() {
            plock(&self.pass.state).free_blocks(blocks);
        }
    }

    fn pass_park_blocks(&self, blocks: Vec<u16>, frame: u64) {
        if !blocks.is_empty() {
            plock(&self.pass.state).park(frame, blocks);
        }
    }

    fn pass_park(&self, mut rec: Box<PassRec>, frame: u64) {
        self.pass_park_blocks(std::mem::take(&mut rec.blocks), frame);
    }

    fn pass_release_parked(&self, frame: u64) {
        if self.pass.on.load(Ordering::Relaxed) {
            plock(&self.pass.state).release_parked(frame);
        }
    }

    /// The resolved ticks of a recording, by local slot (empty when the pool is gone).
    fn read_pass_ticks(&self, rec: &PassRec) -> Vec<u64> {
        let Some(p) = self.pass.pool.get() else { return Vec::new() };
        let mut out = Vec::with_capacity(rec.total as usize);
        for j in 0..rec.total {
            let Some(g) = rec.global(j) else { break };
            // SAFETY: `g < PASS_SLOTS` (a block is below `BLOCKS`); the set's resolve passed its fence.
            out.push(unsafe { *p.mapped.0.add(g as usize) });
        }
        out
    }
}

/// What a submission says about its list, for its passes.
struct PassCtx {
    frame: u64,
    seq: u64,
    queue: u32,
    resubmitted: bool,
    /// the list's own reason for null ticks
    list_why: Option<&'static str>,
    /// the list's start and end ticks (valid ones only)
    start: Option<u64>,
    end: Option<u64>,
}

/// The `pass` events of one submission. `ms` converts a tick to ms since the start (None: no calibration).
fn pass_facts(rec: &PassRec, c: &PassCtx, ms: &dyn Fn(u64) -> Option<f64>) -> Vec<PassFacts> {
    let n = rec.passes.len().max(1);
    let empty = Counts::default();
    (0..n)
        .map(|k| {
            let cnt = rec.passes.get(k).unwrap_or(&empty);
            let mut why = None;
            let (mut st, mut et, mut sm, mut em) = (None, None, None, None);
            if let Some(w) = c.list_why {
                why = Some(w);
            } else if c.resubmitted {
                // both submissions wrote the same slots: no in-list stamp can be trusted
                why = Some("resubmitted");
            } else {
                let a = if k == 0 { c.start } else { rec.ticks.get(k - 1).copied() };
                let b = if k + 1 == n { c.end } else { rec.ticks.get(k).copied() };
                match (a, b) {
                    (Some(a), Some(b)) if a == 0 || b == 0 => why = Some("zero"),
                    (Some(a), Some(b)) if b < a => why = Some("order"),
                    (Some(a), Some(b)) => match (ms(a), ms(b)) {
                        (Some(x), Some(y)) => (st, et, sm, em) = (Some(a), Some(b), Some(x), Some(y)),
                        _ => why = Some("no_frequency"),
                    },
                    _ => why = Some("late"),
                }
            }
            if why.is_none() && k + 1 == n {
                // valid ticks, but the passes were not split as far as the title's barriers go
                why = rec.why;
            }
            PassFacts {
                frame: c.frame,
                list: c.seq,
                queue: c.queue,
                i: k as u32,
                start_tick: st,
                end_tick: et,
                start_ms: sm,
                end_ms: em,
                kind: cnt.kind(),
                draws: cnt.draws,
                dispatches: cnt.dispatches,
                indirect: cnt.indirect,
                pipelines: cnt.pipes.clone(),
                pipeline_ids_truncated: cnt.truncated,
                barriers_before: if k == 0 { 0 } else { rec.bounds.get(k - 1).copied().unwrap_or(0) },
                resubmitted: c.resubmitted,
                why,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------------------
// the global engine
// ---------------------------------------------------------------------------------------

static GPU: OnceLock<GpuTel> = OnceLock::new();

fn global_sink(ev: &str, fields: Vec<(&'static str, Value)>) {
    xlog::event(Tier::Frame, ev, &fields);
}

/// Submits `lists` through `submit` (the PC's own `ExecuteCommandLists`). With telemetry off it is
/// one relaxed load and `submit(lists)`. At tier frame the lists are stamped (`GpuTel::execute`).
pub fn telemetry_execute(queue: *mut c_void, lists: &[*mut c_void], submit: impl FnOnce(&[*mut c_void])) {
    if !xlog::telemetry_on(Tier::Frame) {
        submit(lists);
        return;
    }
    let Some(tel) = GPU.get() else {
        if !lists.is_empty() {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| crate::xlog_warn!("telemetry: the GPU stamp engine is not running - command lists are not timed"));
        }
        submit(lists);
        return;
    };
    let origin = if IN_PRESENT_SPAN.with(|c| c.get()) { "present" } else { "submit" };
    tel.execute(queue, lists, origin, submit, &mut global_sink);
}

/// The engine, once started.
pub(super) fn gpu() -> Option<&'static GpuTel> {
    GPU.get()
}

/// The current frame index (presents so far); wakes the thread.
pub(super) fn set_frame(n: u64) {
    if let Some(t) = GPU.get() {
        t.set_frame(n);
    }
}

/// Harvested list intervals the frame glue has not taken yet.
pub(super) fn take_harvested() -> Vec<HarvestedList> {
    GPU.get().map(|t| t.take_harvested()).unwrap_or_default()
}

/// Creates the engine and the `xcl-telemetry` thread (called only when the tier is at least frame).
pub(super) fn telemetry_gpu_start() {
    let value = std::env::var("XCL_TELEMETRY_SAMPLE").ok();
    let sample = match xlog::parse_sample(value.as_deref()) {
        Ok(n) => n,
        Err(e) => {
            crate::xlog_warn!("telemetry: {e} - sampling every frame");
            1
        }
    };
    let tel = GpuTel::new(Box::new(DevFns::orig), Box::new(QueueFns::orig), sample, LIST_SLOTS, None);
    let pass = xlog::telemetry_on(Tier::Pass);
    tel.pass.on.store(pass, Ordering::Relaxed);
    if GPU.set(tel).is_err() {
        return;
    }
    super::update_manifest(|m| {
        m.sample = sample;
        m.list_slots = LIST_SLOTS;
        m.gpu_sets = SETS as u32;
        m.pass_slots = if pass { PASS_SLOTS } else { 0 };
    });
    let Some(tel) = GPU.get() else { return };
    if let Err(e) = std::thread::Builder::new().name("xcl-telemetry".into()).spawn(move || thread_main(tel)) {
        crate::xlog_warn!("telemetry: cannot start the xcl-telemetry thread: {e}");
    }
}

/// The last harvest and the null flush (called from `telemetry_final`).
pub(super) fn telemetry_gpu_final() {
    if let Some(t) = GPU.get() {
        gpu_final(t);
    }
}

/// The body of `telemetry_gpu_final` on an engine (the tests call it on their own).
fn gpu_final(t: &GpuTel) {
    // `write_final` also runs once before the title starts: nothing to flush, and `finish` would
    // stop the engine for the whole run
    if !t.started() {
        return;
    }
    // valid data first, then every frame still waiting is written with null GPU fields
    t.harvest(&mut global_sink);
    frames_step(t, &mut global_sink);
    let why = if t.is_removed() { "removed" } else { "exit" };
    let facts = lock_wait(&FRAMES, Duration::from_millis(200)).map(|mut b| b.flush(why)).unwrap_or_default();
    write_frames(facts, &mut global_sink);
    t.finish(&mut global_sink);
    if xlog::telemetry_on(Tier::Pass) {
        // the requests since the thread's last look
        if let Some(f) = IO.take(t.frame()) {
            pass_sink("io", xlog::io_fields(&f));
        }
    }
    let names = ["full", "late", "unsupported", "resubmitted", "removed", "sampled_out"];
    let mut drops: BTreeMap<String, u64> = names.iter().zip(t.drops()).map(|(k, v)| (k.to_string(), v)).collect();
    drops.insert("frames".into(), FRAMES_DROPPED.load(Ordering::Relaxed));
    drops.insert("pass_busy".into(), t.pass.busy.load(Ordering::Relaxed));
    drops.insert("pass_full".into(), t.pass.full.load(Ordering::Relaxed));
    let sample = t.sample;
    super::update_manifest(|m| {
        m.sample = sample;
        m.gpu_drops = drops;
    });
}

/// Wakes on every frame (or every 50 ms) and harvests; every 60 frames it calibrates again.
fn thread_main(tel: &'static GpuTel) {
    let mut next_clock = 60u64;
    let mut io = IoFlush { frame: tel.frame(), since: Duration::ZERO };
    let mut last = Instant::now();
    loop {
        {
            let g = tel.wake.0.lock().unwrap_or_else(|e| e.into_inner());
            let (mut g, _) = tel.wake.1.wait_timeout_while(g, Duration::from_millis(50), |w| !*w).unwrap_or_else(|e| e.into_inner());
            *g = false;
        }
        if tel.stopped.load(Ordering::Acquire) {
            crate::xlog_info!("telemetry: the xcl-telemetry thread ends (frame {})", tel.frame());
            break;
        }
        tel.harvest(&mut global_sink);
        frames_step(tel, &mut global_sink);
        let f = tel.frame();
        let now = Instant::now();
        let elapsed = now.duration_since(last);
        last = now;
        if xlog::telemetry_on(Tier::Pass) {
            io_step(&IO, &mut io, f, elapsed, &mut pass_sink);
        }
        if f >= next_clock {
            tel.refresh_clocks(&mut global_sink);
            global_sink("memory", xlog::memory_fields(&memory_snapshot(f)));
            next_clock = (f / 60 + 1) * 60;
        }
    }
}

// ---------------------------------------------------------------------------------------
// part 3: entry points, pipelines, resources, io
// ---------------------------------------------------------------------------------------

/// Barrier batches the title recorded (counted at tier frame; `frame.barriers` is a delta of it).
static BARRIER_BATCHES: AtomicU64 = AtomicU64::new(0);

fn pass_sink(ev: &str, fields: Vec<(&'static str, Value)>) {
    xlog::event(Tier::Pass, ev, &fields);
}

/// A barrier batch of `barriers` barriers is about to be recorded into the PC list `list`. At tier
/// frame it is only counted; at tier pass one timestamp is written into the list before it.
pub fn telemetry_pass_boundary(list: *mut c_void, barriers: u32) {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    BARRIER_BATCHES.fetch_add(1, Ordering::Relaxed);
    if !xlog::telemetry_on(Tier::Pass) {
        return;
    }
    if let Some(t) = GPU.get() {
        t.pass_boundary(list, barriers);
    }
}

/// The PC list was Reset: its next submission is a new recording, not a resubmission, and the
/// pass stamps it recorded without being submitted are dropped.
pub fn telemetry_list_reset(list: *mut c_void) {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    writer_clear(list as usize);
    if let Some(t) = GPU.get() {
        t.list_reset(list, xlog::telemetry_on(Tier::Pass));
    }
}

/// A draw with the pipeline `pso` (the resolved pointer) was recorded into `list`.
pub fn telemetry_draw(list: *mut c_void, pso: usize) {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    bump(DRAW_TOTAL);
    if !xlog::telemetry_on(Tier::Pass) {
        return;
    }
    if let Some(t) = GPU.get() {
        t.pass_note(list, |c| {
            c.draws.fetch_add(1, Ordering::Relaxed);
            if let Some(id) = pipe_id(pso) {
                c.note_pipe(id);
            }
        });
    }
}

/// An indirect execute was recorded into `list` (counted as `indirect`; `telemetry_draw` counts as a draw).
pub fn telemetry_indirect(list: *mut c_void) {
    if !xlog::telemetry_on(Tier::Pass) {
        return;
    }
    if let Some(t) = GPU.get() {
        t.pass_note(list, |c| {
            c.indirect.fetch_add(1, Ordering::Relaxed);
        });
    }
}

/// A dispatch of the pipeline `pso` (a pointer) was recorded into `list`.
pub fn telemetry_dispatch(list: *mut c_void, pso: usize) {
    if !xlog::telemetry_on(Tier::Pass) {
        return;
    }
    if let Some(t) = GPU.get() {
        t.pass_note(list, |c| {
            c.dispatches.fetch_add(1, Ordering::Relaxed);
            if let Some(id) = pipe_id(pso) {
                c.note_pipe(id);
            }
        });
    }
}

// ---- call counts: totals and calls left out, by kind and reason (part 3b) ----

/// Counted calls, in this order; frame and activity fields take these names.
pub const COUNT_NAMES: [&str; 33] = [
    "draw_total", "ei_draw_total", "dispatch_total", "ei_dispatch_total",            // 0-3 totals
    "ei_draw_skip_building", "ei_draw_skip_late", "ei_draw_skip_abandoned",          // 4-6
    "ei_draw_skip_quarantined", "ei_draw_skip_failed", "ei_draw_skip_other",         // 7-9
    "draw_standin_building", "draw_standin_late", "draw_standin_abandoned",          // 10-12
    "draw_standin_quarantined", "draw_standin_failed", "draw_skip_other", "draw_pc", // 13-16
    "dispatch_skip_building", "dispatch_skip_late", "dispatch_skip_image_only",      // 17-19
    "dispatch_skip_background", "dispatch_skip_size_gate", "dispatch_skip_abandoned",// 20-22
    "dispatch_skip_quarantined", "dispatch_skip_failed", "dispatch_skip_other",      // 23-25
    "ei_after_skipped_dispatch", "mustdraw_skipped", "mustdraw_wait_timeout",        // 26-28
    "mustdraw_wait_us", "cache_hit_layer", "cache_miss_layer", "reserved_0",         // 29-32
];
static COUNTS: [AtomicU64; 33] = [const { AtomicU64::new(0) }; 33];

const DRAW_TOTAL: usize = 0;
const EI_DRAW_TOTAL: usize = 1;
const DISPATCH_TOTAL: usize = 2;
const EI_DISPATCH_TOTAL: usize = 3;
const EI_DRAW_SKIP_BASE: usize = 4;
const EI_DRAW_SKIP_OTHER: usize = 9;
const DRAW_STANDIN_BASE: usize = 10;
const DRAW_SKIP_OTHER: usize = 15;
const DRAW_PC: usize = 16;
const DISPATCH_SKIP_BUILDING: usize = 17;
const DISPATCH_SKIP_FAILED: usize = 24;
const DISPATCH_SKIP_OTHER: usize = 25;
const EI_AFTER_SKIPPED_DISPATCH: usize = 26;
const MUSTDRAW_SKIPPED: usize = 27;
const MUSTDRAW_WAIT_TIMEOUT: usize = 28;
const MUSTDRAW_WAIT_US: usize = 29;
const CACHE_HIT_LAYER: usize = 30;
const CACHE_MISS_LAYER: usize = 31;

fn bump(i: usize) {
    COUNTS[i].fetch_add(1, Ordering::Relaxed);
}

/// The call counts now (relaxed loads).
pub fn counts_now() -> [u64; 33] {
    let mut a = [0u64; 33];
    for (x, c) in a.iter_mut().zip(COUNTS.iter()) {
        *x = c.load(Ordering::Relaxed);
    }
    a
}

/// What kind of call is left out: a direct draw, an ExecuteIndirect (`true`: its signature
/// dispatches) or a direct dispatch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Call {
    Draw,
    Indirect(bool),
    Dispatch,
}

fn is_dispatch(call: Call) -> bool {
    matches!(call, Call::Dispatch | Call::Indirect(true))
}

/// pure: the count a call of `call` left out for `why` (a `pso_async::Unbuilt` name) goes to. A draw
/// kind has five reasons (`None` and the reasons only a dispatch has map to `failed`), a dispatch
/// kind has one count per name (`None` maps to `failed`, a name not known to `other`).
fn skip_index(call: Call, why: Option<&str>) -> usize {
    if is_dispatch(call) {
        return match why {
            None => DISPATCH_SKIP_FAILED,
            Some("building") => DISPATCH_SKIP_BUILDING,
            Some("late") => DISPATCH_SKIP_BUILDING + 1,
            Some("image_only") => DISPATCH_SKIP_BUILDING + 2,
            Some("background") => DISPATCH_SKIP_BUILDING + 3,
            Some("size_gate") => DISPATCH_SKIP_BUILDING + 4,
            Some("abandoned") => DISPATCH_SKIP_BUILDING + 5,
            Some("quarantined") => DISPATCH_SKIP_BUILDING + 6,
            Some("failed") => DISPATCH_SKIP_FAILED,
            Some(_) => DISPATCH_SKIP_OTHER,
        };
    }
    let base = if call == Call::Draw { DRAW_STANDIN_BASE } else { EI_DRAW_SKIP_BASE };
    base + match why {
        Some("building") => 0,
        Some("late") => 1,
        Some("abandoned") => 2,
        Some("quarantined") => 3,
        _ => 4,
    }
}

/// Lists in which a dispatch that may write memory was left out since their last Reset.
static SKIPPED_WRITER: Mutex<Option<HashSet<usize>>> = Mutex::new(None);

/// pure: `list` had a dispatch left out.
fn writer_mark(set: &mut HashSet<usize>, list: usize) {
    if list != 0 {
        set.insert(list);
    }
}

/// pure: a dispatch was left out in `list` since its last Reset.
fn writer_hit(set: &HashSet<usize>, list: usize) -> bool {
    list != 0 && set.contains(&list)
}

fn writer_clear(list: usize) {
    if let Some(set) = SKIPPED_WRITER.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        set.remove(&list);
    }
}

fn writer_note(list: *mut c_void) {
    writer_mark(SKIPPED_WRITER.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashSet::new), list as usize);
}

/// A call of `call` on `list` is left out for the pipeline `pso` (a stand-in pointer): counted by
/// the reason `pso_async::skipped` gives (`s`).
fn note_skip(list: *mut c_void, call: Call, s: crate::hle::d3d12x::pso_async::Skipped) {
    let why = s.why.map(|w| w.name());
    bump(skip_index(call, why));
    if s.must_draw {
        bump(MUSTDRAW_SKIPPED);
    }
    if is_dispatch(call) && why != Some("image_only") {
        writer_note(list);
    }
}

/// A call of `call` on `list` is left out because the pipeline `pso` (a stand-in pointer, 0: none)
/// is not built or has no translation.
pub fn telemetry_skip(list: *mut c_void, call: Call, pso: usize) {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    note_skip(list, call, crate::hle::d3d12x::pso_async::skipped(pso));
}

/// A call of `call` on `list` is left out for a reason that is not a pipeline's (a two-pass
/// pipeline, an ABI error, root arguments the call changes).
pub fn telemetry_skip_other(list: *mut c_void, call: Call) {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    match call {
        Call::Draw => bump(DRAW_SKIP_OTHER),
        Call::Indirect(false) => bump(EI_DRAW_SKIP_OTHER),
        Call::Dispatch | Call::Indirect(true) => {
            bump(DISPATCH_SKIP_OTHER);
            writer_note(list);
        }
    }
}

/// A call of `call` goes to the PC list with the pipeline `pso` and no translation: it draws when
/// that pipeline is built (`draw_pc`), else it is left out like `telemetry_skip`.
pub fn telemetry_pc_call(list: *mut c_void, call: Call, pso: usize) {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    if crate::hle::d3d12x::pso_async::why_unbuilt(pso).is_some() {
        note_skip(list, call, crate::hle::d3d12x::pso_async::skipped(pso));
    } else if !is_dispatch(call) {
        bump(DRAW_PC);
    }
}

/// An ExecuteIndirect (`dispatches`: its signature dispatches) was recorded into `list`, whatever
/// became of it; counted after a left-out dispatch in the same list too.
pub fn telemetry_indirect_issued(list: *mut c_void, dispatches: bool) {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    if dispatches {
        bump(EI_DISPATCH_TOTAL);
        bump(DISPATCH_TOTAL);
    } else {
        bump(EI_DRAW_TOTAL);
        bump(DRAW_TOTAL);
    }
    let after = writer_hit(SKIPPED_WRITER.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashSet::new), list as usize);
    if after {
        bump(EI_AFTER_SKIPPED_DISPATCH);
    }
}

/// A direct dispatch was recorded (or left out: `telemetry_skip` counts that too).
pub fn telemetry_dispatch_issued() {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    bump(DISPATCH_TOTAL);
}

/// A put-off dispatch could not be recorded (`bind::dispatch_dropped`).
pub fn telemetry_dispatch_dropped() {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    bump(DISPATCH_TOTAL);
    bump(DISPATCH_SKIP_OTHER);
}

/// A bind waited `took` for a must-draw pipeline; `timed_out`: it used up its chance.
pub fn telemetry_mustdraw_wait(took: Duration, timed_out: bool) {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    COUNTS[MUSTDRAW_WAIT_US].fetch_add(took.as_micros() as u64, Ordering::Relaxed);
    if timed_out {
        bump(MUSTDRAW_WAIT_TIMEOUT);
    }
}

/// pure: the fields of a frame's call-count deltas `d`: the four totals always, the others when
/// non-zero. `mustdraw_wait_us` (29) is written by the frame as `mustdraw_wait_ms`; `reserved_0` never.
fn frame_count_fields(d: &[u64; 33]) -> Vec<(&'static str, u64)> {
    (0..32).filter(|&i| i != MUSTDRAW_WAIT_US && (i < 4 || d[i] != 0)).map(|i| (COUNT_NAMES[i], d[i])).collect()
}

/// The pipeline `pso` (a pointer) was bound on `list`.
pub fn telemetry_bind_pso(list: *mut c_void, pso: usize) {
    if !xlog::telemetry_on(Tier::Pass) {
        return;
    }
    if let Some(t) = GPU.get() {
        t.pass_note(list, |c| {
            if let Some(id) = pipe_id(pso) {
                c.note_pipe(id);
            }
        });
    }
}

// ---- pipelines ----

/// The next telemetry pipeline id (the `pipeline` event and the pointer map share the counter).
static PIPELINE_SEQ: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// The source of the shaders of the pipeline this thread is building.
    static SOURCE: Cell<Option<&'static str>> = const { Cell::new(None) };
    /// The last pipeline pointer this thread looked up, its id and the `PIPE_GEN` it was read at.
    static LAST_PSO: Cell<(usize, u64, u64)> = const { Cell::new((0, 0, 0)) };
}

fn source_rank(s: &str) -> u8 {
    match s {
        "dxc" => 3,
        "direct" => 2,
        "cache" => 1,
        _ => 0,
    }
}

/// The source of a pipeline built from several stages: the most expensive one wins (dxc > direct > cache).
fn merge_source(cur: Option<&'static str>, new: &'static str) -> Option<&'static str> {
    match cur {
        Some(c) if source_rank(c) >= source_rank(new) => Some(c),
        _ => Some(new),
    }
}

/// Where the shader of the pipeline being built on this thread came from: `cache`, `dxc` or `direct`.
pub fn telemetry_shader_source(source: &'static str) {
    if !xlog::telemetry_on(Tier::Pass) {
        return;
    }
    SOURCE.with(|c| c.set(merge_source(c.get(), source)));
}

/// Bumped at every relabel: the threads' `LAST_PSO` caches of an older generation are stale.
static PIPE_GEN: AtomicU64 = AtomicU64::new(0);

fn pipe_map() -> &'static Mutex<PipeMap> {
    static MAP: OnceLock<Mutex<PipeMap>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(PipeMap::new(PIPE_MAP_MAX)))
}

/// Pipeline pointers and their telemetry ids (bounded).
struct PipeMap {
    cap: usize,
    ids: HashMap<usize, u64>,
}

impl PipeMap {
    fn new(cap: usize) -> PipeMap {
        PipeMap { cap, ids: HashMap::new() }
    }

    /// The id of `ptr`; a pointer seen for the first time gets the next one. None when the map is full.
    fn id_of(&mut self, ptr: usize, seq: &AtomicU64) -> Option<u64> {
        if let Some(&i) = self.ids.get(&ptr) {
            return Some(i);
        }
        if self.ids.len() >= self.cap {
            return None;
        }
        let id = seq.fetch_add(1, Ordering::Relaxed);
        self.ids.insert(ptr, id);
        Some(id)
    }

    /// `ptr` is a new pipeline (a pointer that was reused): it gets a new id. None when the map is full and `ptr` is new.
    fn relabel(&mut self, ptr: usize, seq: &AtomicU64) -> Option<u64> {
        if self.ids.contains_key(&ptr) || self.ids.len() < self.cap {
            let id = seq.fetch_add(1, Ordering::Relaxed);
            self.ids.insert(ptr, id);
            return Some(id);
        }
        None
    }
}

/// The telemetry id of a pipeline pointer, made on first sight (no `pipeline` event: its creation
/// was not seen here). None for a null pointer, a full map or a busy lock.
fn pipe_id(pso: usize) -> Option<u64> {
    if pso == 0 {
        return None;
    }
    let gen = PIPE_GEN.load(Ordering::Acquire);
    let (last, id, last_gen) = LAST_PSO.with(|c| c.get());
    if last == pso && last_gen == gen {
        return Some(id);
    }
    let id = match lock_wait(pipe_map(), NOTE_WAIT) {
        Some(mut m) => m.id_of(pso, &PIPELINE_SEQ),
        None => {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| crate::xlog_warn!("telemetry: the pipeline id map lock was busy - some pipelines are not listed in passes"));
            None
        }
    };
    match id {
        Some(i) => LAST_PSO.with(|c| c.set((pso, i, gen))),
        None => {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| crate::xlog_warn!("telemetry: more than {PIPE_MAP_MAX} pipelines seen - later ones are not listed in passes"));
        }
    }
    id
}

/// `pso` is a new pipeline: a new id, and every thread's cached id of it goes stale. None when the
/// map is full or busy.
fn pipe_relabel(pso: usize) -> Option<u64> {
    let id = match lock_wait(pipe_map(), Duration::from_millis(50)) {
        Some(mut m) => m.relabel(pso, &PIPELINE_SEQ),
        None => None,
    };
    PIPE_GEN.fetch_add(1, Ordering::Release);
    if id.is_none() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| crate::xlog_warn!("telemetry: the pipeline id map is full or was busy - some pipelines get no pipeline_bind event"));
    }
    id
}

/// The fields of a `pipeline_bind` event: stages without bytes are omitted.
fn pipeline_bind_fields(id: u64, shaders: &[(&str, Option<&[u8]>)]) -> Vec<(&'static str, Value)> {
    let (stages, records): (Vec<Value>, Vec<Value>) = shaders
        .iter()
        .filter_map(|(k, b)| (*b).map(|b| (Value::from(*k), Value::from(format!("{:016x}", crate::gpu::shader::fnv64(b))))))
        .unzip();
    vec![("id", Value::from(id)), ("stages", Value::Array(stages)), ("records", Value::Array(records))]
}

/// A translated pipeline was made at `pso` (the real pipeline) from these console shaders: the
/// pointer gets a NEW telemetry id (a reused pointer is a new pipeline) and a `pipeline_bind` event.
pub fn telemetry_pipeline_label(pso: usize, shaders: &[(&str, Option<&[u8]>)]) {
    if !xlog::telemetry_on(Tier::Pass) || pso == 0 {
        return;
    }
    let Some(id) = pipe_relabel(pso) else { return };
    pass_sink("pipeline_bind", pipeline_bind_fields(id, shaders));
}

/// `pipeline` builds made by the title's own threads (`by` == "title"), since the run began.
static BUILDS_TITLE: AtomicU64 = AtomicU64::new(0);

/// How many `pipeline` events the title's own threads caused so far (the `activity` event takes
/// the delta).
pub fn pipeline_builds_title() -> u64 {
    BUILDS_TITLE.load(Ordering::Relaxed)
}

/// pure: who builds on a thread named `name`: `xcl-pso-pre*` preload workers, `xcl-pso-x*` extra
/// workers, other `xcl-pso*` workers, other `xcl-*` threads of XCL, anything else the title.
fn built_by(name: Option<&str>) -> &'static str {
    match name {
        Some(n) if n.starts_with("xcl-pso-pre") => "preload",
        Some(n) if n.starts_with("xcl-pso-x") => "extra",
        Some(n) if n.starts_with("xcl-pso") => "worker",
        Some(n) if n.starts_with("xcl-") => "xcl",
        _ => "title",
    }
}

/// A bind's wait for a pipeline ran out its limit (a later unit fills this in).
pub fn telemetry_pipeline_wait_expired(compute: bool, ms: u64) {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let Some(n) = xlog::sampled(&COUNT) else { return };
    let stage = if compute { "compute" } else { "graphics" };
    let extra: [(&'static str, Value); 3] = [("stage", stage.into()), ("ms", ms.into()), ("n", n.into())];
    crate::report::telemetry_error(crate::report::ErrKind::PipelineWaitExpired, "pso_async", "a compute pipeline was not built in time", None, false, &extra);
}

/// A pipeline's first bind found it built (`ready`) or not.
pub fn telemetry_first_use(ready: bool) {
    if !xlog::telemetry_on(Tier::Frame) {
        return;
    }
    bump(if ready { CACHE_HIT_LAYER } else { CACHE_MISS_LAYER });
}

/// A pipeline's build ended (a later unit fills this in).
pub fn telemetry_pipeline_ready(_r: &crate::hle::d3d12x::pso_async::BuildRecord) {}

/// A pipeline was built by the driver: writes its `pipeline` event. `kind` is the build's label
/// (`compute` anywhere in it makes the stage compute); `record` is the console record hash and
/// whether a driver blob was supplied.
pub fn telemetry_pipeline_built(kind: &str, dxil_bytes: usize, record: Option<(u64, bool)>, driver: Duration, waited: Duration) {
    if !xlog::telemetry_on(Tier::Pass) {
        return;
    }
    let wanted = crate::hle::d3d12x::pso_async::wanted_here()();
    let by = built_by(std::thread::current().name());
    if by == "title" {
        BUILDS_TITLE.fetch_add(1, Ordering::Relaxed);
    }
    let source = SOURCE.with(|c| c.take());
    let facts = xlog::PipelineFacts {
        id: PIPELINE_SEQ.fetch_add(1, Ordering::Relaxed),
        stage: if kind.contains("compute") { "compute" } else { "graphics" },
        record: record.map(|r| r.0),
        dxil_bytes: dxil_bytes as u64,
        compile_ms: driver.as_secs_f64() * 1000.0,
        waited_ms: waited.as_secs_f64() * 1000.0,
        source,
        blob: record.is_some_and(|r| r.1),
        by,
        wanted,
    };
    pass_sink("pipeline", xlog::pipeline_fields_built(&facts));
}

// ---- resources ----

/// A title's resource creation call. Writes its `resource` event when it ends.
pub struct ResourceSpan(Option<ResourceInfo>);

struct ResourceInfo {
    kind: &'static str,
    dim: Option<&'static str>,
    bytes: Option<u64>,
    t0: i64,
    frame: u64,
}

/// `dim` and `bytes` of the `D3D12_RESOURCE_DESC` at `desc`, when the pointer is readable. A
/// texture's size needs the allocation info: its `bytes` stay null.
fn resource_desc(desc: usize) -> (Option<&'static str>, Option<u64>) {
    if desc == 0 || !crate::gpu::shader::readable(desc, std::mem::size_of::<D3D12_RESOURCE_DESC>()) {
        return (None, None);
    }
    // SAFETY: readable for the structure's size; the title's memory may be unaligned.
    let d = unsafe { std::ptr::read_unaligned(desc as *const D3D12_RESOURCE_DESC) };
    match d.Dimension {
        D3D12_RESOURCE_DIMENSION_BUFFER => (Some("buffer"), Some(d.Width)),
        D3D12_RESOURCE_DIMENSION_TEXTURE1D | D3D12_RESOURCE_DIMENSION_TEXTURE2D | D3D12_RESOURCE_DIMENSION_TEXTURE3D => (Some("texture"), None),
        _ => (None, None),
    }
}

/// The start of a resource creation hook (`desc`: the title's `D3D12_RESOURCE_DESC*`, or 0). Only
/// while a frame is open (after the first present).
pub fn telemetry_resource_begin(kind: &'static str, desc: usize) -> ResourceSpan {
    if !xlog::telemetry_on(Tier::Pass) {
        return ResourceSpan(None);
    }
    let frame = GPU.get().map_or(0, |t| t.frame());
    if frame == 0 {
        return ResourceSpan(None);
    }
    let (dim, bytes) = resource_desc(desc);
    ResourceSpan(Some(ResourceInfo { kind, dim, bytes, t0: dframe::qpc(), frame }))
}

impl Drop for ResourceSpan {
    fn drop(&mut self) {
        let Some(i) = self.0.take() else { return };
        let ms = (dframe::qpc() - i.t0).max(0) as f64 * 1000.0 / qpf() as f64;
        let facts = xlog::ResourceFacts { kind: i.kind, dim: i.dim, bytes: i.bytes, ms, frame: i.frame };
        pass_sink("resource", xlog::resource_fields(&facts));
    }
}

// ---- io ----

/// DirectStorage requests completed since the last `io` event.
#[derive(Default)]
struct IoAgg {
    requests: AtomicU64,
    bytes: AtomicU64,
    ns: AtomicU64,
    max_ns: AtomicU64,
    zlib: AtomicU64,
}

impl IoAgg {
    const fn new() -> IoAgg {
        IoAgg { requests: AtomicU64::new(0), bytes: AtomicU64::new(0), ns: AtomicU64::new(0), max_ns: AtomicU64::new(0), zlib: AtomicU64::new(0) }
    }

    fn add(&self, bytes: u64, zlib: bool, ns: u64) {
        self.requests.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
        self.ns.fetch_add(ns, Ordering::Relaxed);
        self.max_ns.fetch_max(ns, Ordering::Relaxed);
        if zlib {
            self.zlib.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn pending(&self) -> bool {
        self.requests.load(Ordering::Relaxed) > 0
    }

    /// The aggregate so far, reset; None when no request completed.
    fn take(&self, frame: u64) -> Option<xlog::IoFacts> {
        let requests = self.requests.swap(0, Ordering::Relaxed);
        if requests == 0 {
            return None;
        }
        Some(xlog::IoFacts {
            requests: requests.min(u32::MAX as u64) as u32,
            bytes: self.bytes.swap(0, Ordering::Relaxed),
            ms: self.ns.swap(0, Ordering::Relaxed) as f64 / 1e6,
            max_ms: self.max_ns.swap(0, Ordering::Relaxed) as f64 / 1e6,
            zlib: self.zlib.swap(0, Ordering::Relaxed).min(u32::MAX as u64) as u32,
            frame,
        })
    }
}

static IO: IoAgg = IoAgg::new();

/// A DirectStorage request being run. Adds to the per-frame `io` aggregate when it ends.
pub struct IoSpan(Option<(u64, bool, i64)>);

/// The start of a DirectStorage request of `bytes` bytes (`zlib`: compressed with zlib).
pub fn telemetry_io_begin(bytes: u64, zlib: bool) -> IoSpan {
    if !xlog::telemetry_on(Tier::Pass) {
        return IoSpan(None);
    }
    IoSpan(Some((bytes, zlib, dframe::qpc())))
}

impl Drop for IoSpan {
    fn drop(&mut self) {
        let Some((bytes, zlib, t0)) = self.0.take() else { return };
        let ns = ((dframe::qpc() - t0).max(0) as i128 * 1_000_000_000 / qpf() as i128).min(u64::MAX as i128) as u64;
        IO.add(bytes, zlib, ns);
    }
}

/// When the telemetry thread last looked: the frame it saw and the time since its last flush.
struct IoFlush {
    frame: u64,
    since: Duration,
}

/// One step of the thread (`elapsed` since the last step): writes one `io` event when the frame
/// changed (the frame that ended) or `IO_FLUSH` has passed since the last flush, and requests
/// completed. Returns whether an event was written.
fn io_step(agg: &IoAgg, st: &mut IoFlush, frame: u64, elapsed: Duration, sink: Sink) -> bool {
    st.since += elapsed;
    let boundary = frame != st.frame;
    let due = boundary || st.since >= IO_FLUSH;
    let mut wrote = false;
    if due {
        if let Some(f) = agg.take(st.frame) {
            sink("io", xlog::io_fields(&f));
            wrote = true;
        }
        st.since = Duration::ZERO;
    }
    st.frame = frame;
    wrote
}

// ---------------------------------------------------------------------------------------
// frames: spans, frame records, the memory event
// ---------------------------------------------------------------------------------------

/// `WaitFrameEventX` time and calls, all threads.
static WAIT_NS: AtomicU64 = AtomicU64::new(0);
static WAIT_CALLS: AtomicU64 = AtomicU64::new(0);
/// Frame records dropped because their lock was busy.
static FRAMES_DROPPED: AtomicU64 = AtomicU64::new(0);
static FRAMES: Mutex<FrameBook> = Mutex::new(FrameBook::new());

thread_local! {
    /// The frame (1-based) a `present_now` on this thread showed; 0: none yet.
    static SHOWN: Cell<u64> = const { Cell::new(0) };
    /// This thread's own `WaitFrameEventX` time so far, and its value at its last frame close.
    static THREAD_WAIT_NS: Cell<u64> = const { Cell::new(0) };
    static THREAD_WAIT_MARK: Cell<u64> = const { Cell::new(0) };
}

fn set_shown(n: u64) {
    SHOWN.with(|c| c.set(n));
}

fn take_shown() -> u64 {
    SHOWN.with(|c| c.replace(0))
}

/// A `present_now` call. Closes the frame it showed (if it showed one) when it ends.
pub struct PresentSpan(Option<i64>, bool);

/// A `WaitFrameEventX` call.
pub struct WaitSpan(Option<i64>);

/// The start of `present_now`: marks submissions on this thread as `origin: "present"`.
pub fn telemetry_present_begin() -> PresentSpan {
    if !xlog::telemetry_on(Tier::Frame) {
        return PresentSpan(None, false);
    }
    let was = IN_PRESENT_SPAN.with(|c| c.replace(true));
    PresentSpan(Some(dframe::qpc()), was)
}

/// `present_now` showed frame `n` (the running count of shown frames, 1-based).
pub fn telemetry_frame_shown(n: u64) {
    if xlog::telemetry_on(Tier::Frame) {
        set_shown(n);
    }
}

/// The start of `WaitFrameEventX`.
pub fn telemetry_wait_begin() -> WaitSpan {
    if !xlog::telemetry_on(Tier::Frame) {
        return WaitSpan(None);
    }
    WaitSpan(Some(dframe::qpc()))
}

impl Drop for WaitSpan {
    fn drop(&mut self) {
        let Some(t0) = self.0 else { return };
        let ns = ((dframe::qpc() - t0).max(0) as i128 * 1_000_000_000 / qpf() as i128).min(u64::MAX as i128) as u64;
        WAIT_NS.fetch_add(ns, Ordering::Relaxed);
        WAIT_CALLS.fetch_add(1, Ordering::Relaxed);
        THREAD_WAIT_NS.with(|c| c.set(c.get().saturating_add(ns)));
    }
}

impl Drop for PresentSpan {
    fn drop(&mut self) {
        let Some(t0) = self.0 else { return };
        IN_PRESENT_SPAN.with(|c| c.set(self.1));
        let shown = take_shown();
        if shown == 0 {
            return; // returned early (re-entry, lock busy, nothing to show): not a frame
        }
        let Some(tel) = GPU.get() else { return };
        let qpc_end = dframe::qpc();
        let cum = THREAD_WAIT_NS.with(|c| c.get());
        let thread_wait_ms = cum.saturating_sub(THREAD_WAIT_MARK.with(|c| c.replace(cum))) as f64 / 1e6;
        let close = Close {
            n: shown,
            qpc_end,
            qpc0: tel.qpc0,
            qpf: tel.qpf,
            present_ms: (qpc_end - t0) as f64 * 1000.0 / tel.qpf as f64,
            thread_wait_ms,
            lists: tel.lists_in(shown - 1),
            sampled: xlog::sample_due(shown - 1, tel.sample),
            counters: Counters::now(),
        };
        close_if_shown(&FRAMES, shown, close, &mut global_sink);
        tel.set_frame(shown);
    }
}

/// What a frame record is made from at its close.
struct Close {
    n: u64,
    qpc_end: i64,
    qpc0: i64,
    qpf: i64,
    present_ms: f64,
    thread_wait_ms: f64,
    lists: u32,
    sampled: bool,
    counters: Counters,
}

/// The counters a frame record takes deltas of.
#[derive(Clone, Copy)]
struct Counters {
    draws: u64,
    dispatches: u64,
    pso_draw_us: u64,
    pso_dispatch_us: u64,
    look_us: u64,
    wait_ns: u64,
    wait_calls: u64,
    barriers: u64,
    counts: [u64; 33],
}

impl Default for Counters {
    fn default() -> Counters {
        Counters::ZERO
    }
}

impl Counters {
    const ZERO: Counters = Counters { draws: 0, dispatches: 0, pso_draw_us: 0, pso_dispatch_us: 0, look_us: 0, wait_ns: 0, wait_calls: 0, barriers: 0, counts: [0; 33] };

    /// bind::activity (draws, dispatches), pso_async::blocked (total us, graphics + compute),
    /// sparse::look_counters()[0] (whole looks, us), and the wait counters above.
    fn now() -> Counters {
        let (draws, dispatches) = crate::hle::d3d12x::bind::activity();
        let b = crate::hle::d3d12x::pso_async::blocked();
        Counters {
            draws,
            dispatches,
            pso_draw_us: b[0][1],
            pso_dispatch_us: b[1][1],
            look_us: crate::hle::d3d12x::sparse::look_counters()[0],
            wait_ns: WAIT_NS.load(Ordering::Relaxed),
            wait_calls: WAIT_CALLS.load(Ordering::Relaxed),
            barriers: BARRIER_BATCHES.load(Ordering::Relaxed),
            counts: counts_now(),
        }
    }
}

/// One shown frame waiting for its GPU data.
struct FrameRec {
    i: u64,
    interval_ms: f64,
    present_ms: f64,
    thread_wait_ms: f64,
    wait_ms: f64,
    wait_calls: u64,
    draws: u64,
    dispatches: u64,
    barriers: u64,
    pso_wait_ms: f64,
    pso_wait_draw_ms: f64,
    pso_wait_dispatch_ms: f64,
    look_ms: f64,
    /// call counts of the frame (deltas)
    counts: [u64; 33],
    lists: u32,
    sampled: bool,
    /// calibrated [start, end) of this frame's lists with valid ticks
    intervals: Vec<(f64, f64)>,
    /// lists of this frame the harvest has handed over (valid or null)
    got: u32,
}

/// Frames waiting (bounded `PENDING_FRAMES`), harvested intervals waiting for their frame.
struct FrameBook {
    pending: VecDeque<FrameRec>,
    waiting: Vec<HarvestedList>,
    prev: Counters,
    last_qpc: Option<i64>,
    last_written: u64,
}

impl FrameBook {
    const fn new() -> FrameBook {
        FrameBook {
            pending: VecDeque::new(),
            waiting: Vec::new(),
            prev: Counters::ZERO,
            last_qpc: None,
            last_written: 0,
        }
    }

    /// Adds the record of frame `c.n`. When `PENDING_FRAMES` are waiting, the oldest is written
    /// at once with null GPU fields (`late`): that is returned.
    fn close(&mut self, c: Close) -> Vec<FrameFacts> {
        let d = |a: u64, b: u64| a.saturating_sub(b);
        let interval = c.qpc_end - self.last_qpc.unwrap_or(c.qpc0);
        self.last_qpc = Some(c.qpc_end);
        let (p, n) = (self.prev, c.counters);
        self.prev = n;
        self.pending.push_back(FrameRec {
            i: c.n,
            interval_ms: interval as f64 * 1000.0 / c.qpf as f64,
            present_ms: c.present_ms,
            thread_wait_ms: c.thread_wait_ms,
            wait_ms: d(n.wait_ns, p.wait_ns) as f64 / 1e6,
            wait_calls: d(n.wait_calls, p.wait_calls),
            draws: d(n.draws, p.draws),
            dispatches: d(n.dispatches, p.dispatches),
            barriers: d(n.barriers, p.barriers),
            pso_wait_ms: (d(n.pso_draw_us, p.pso_draw_us) + d(n.pso_dispatch_us, p.pso_dispatch_us)) as f64 / 1000.0,
            pso_wait_draw_ms: d(n.pso_draw_us, p.pso_draw_us) as f64 / 1000.0,
            pso_wait_dispatch_ms: d(n.pso_dispatch_us, p.pso_dispatch_us) as f64 / 1000.0,
            look_ms: d(n.look_us, p.look_us) as f64 / 1000.0,
            counts: std::array::from_fn(|i| d(n.counts[i], p.counts[i])),
            lists: c.lists,
            sampled: c.sampled,
            intervals: Vec::new(),
            got: 0,
        });
        let mut out = Vec::new();
        while self.pending.len() > PENDING_FRAMES {
            if let Some(r) = self.pending.pop_front() {
                self.last_written = r.i;
                out.push(frame_facts(&r, r.lists, Some("late")));
            }
        }
        out
    }

    /// Hands harvested list intervals to the frames they belong to (list frame `f` is frame `f + 1`).
    fn attach(&mut self, new: Vec<HarvestedList>) {
        self.waiting.extend(new);
        let last = self.last_written;
        let pending = &mut self.pending;
        self.waiting.retain(|h| {
            let i = h.frame + 1;
            if i <= last {
                return false; // its frame was written already
            }
            match pending.iter_mut().find(|r| r.i == i) {
                Some(r) => {
                    r.got += 1;
                    if let (Some(s), Some(e)) = (h.start_ms, h.end_ms) {
                        r.intervals.push((s, e));
                    }
                    false
                }
                None => true, // its frame is not closed yet
            }
        });
        if self.waiting.len() > HARVESTED_MAX {
            let extra = self.waiting.len() - HARVESTED_MAX;
            self.waiting.drain(..extra);
        }
    }

    /// Frames, in order of `i`, whose lists have all been harvested (`lists_of(frame)` = lists
    /// submitted in that frame) or that are `LATE_FRAMES` old at frame `now`.
    fn complete(&mut self, now: u64, lists_of: &dyn Fn(u64) -> u32) -> Vec<FrameFacts> {
        let mut out = Vec::new();
        while let Some(r) = self.pending.front() {
            let want = if r.sampled { lists_of(r.i - 1).max(r.lists) } else { r.lists };
            if r.sampled && r.got < want && now < r.i + LATE_FRAMES {
                break;
            }
            if let Some(r) = self.pending.pop_front() {
                self.last_written = r.i;
                out.push(frame_facts(&r, want, None));
            }
        }
        out
    }

    /// Every waiting frame, GPU fields null with `why` (`exit` / `removed`).
    fn flush(&mut self, why: &'static str) -> Vec<FrameFacts> {
        let mut out = Vec::new();
        while let Some(r) = self.pending.pop_front() {
            self.last_written = r.i;
            out.push(frame_facts(&r, r.lists, Some(why)));
        }
        out
    }
}

/// The `frame` facts of a record. `forced`: GPU fields null with this reason.
fn frame_facts(r: &FrameRec, want: u32, forced: Option<&'static str>) -> FrameFacts {
    let lists = r.lists.max(want);
    let (mut gpu_ms, mut first, mut last, mut why) = (None, None, None, None);
    let mut lists_null = 0;
    if !r.sampled {
        why = Some("sampled_out");
    } else if let Some(w) = forced {
        why = Some(w);
        lists_null = lists;
    } else {
        let mut iv = r.intervals.clone();
        gpu_ms = xlog::union_ms(&mut iv);
        first = iv.first().map(|p| p.0);
        last = iv.iter().map(|p| p.1).reduce(f64::max);
        lists_null = lists.saturating_sub(r.intervals.len() as u32);
        if gpu_ms.is_none() {
            why = Some(if lists == 0 {
                "no_lists"
            } else if r.got < want {
                "late"
            } else {
                "no_ticks"
            });
        }
    }
    FrameFacts {
        i: r.i,
        interval_ms: r.interval_ms,
        cpu_ms: xlog::frame_cpu_ms(r.interval_ms, r.present_ms, r.thread_wait_ms),
        present_ms: r.present_ms,
        wait_ms: r.wait_ms,
        wait_calls: r.wait_calls,
        gpu_ms,
        gpu_first_ms: first,
        gpu_last_ms: last,
        gpu_lists_null: lists_null,
        lists,
        draws: r.draws,
        dispatches: r.dispatches,
        barriers: Some(r.barriers),
        pso_wait_ms: r.pso_wait_ms,
        pso_wait_draw_ms: r.pso_wait_draw_ms,
        pso_wait_dispatch_ms: r.pso_wait_dispatch_ms,
        look_ms: r.look_ms,
        counts: frame_count_fields(&r.counts),
        mustdraw_wait_ms: r.counts[MUSTDRAW_WAIT_US] as f64 / 1000.0,
        sampled: r.sampled,
        why,
    }
}

fn write_frames(facts: Vec<FrameFacts>, sink: Sink) {
    for f in &facts {
        sink("frame", xlog::frame_fields(f));
    }
}

/// The end of a present span: when it showed frame `shown` (non-zero), the frame is recorded.
/// The lock is waited for at most 1 ms (else the frame is counted as dropped). The events are
/// written after the lock is released. Returns whether a frame was recorded.
fn close_if_shown(book: &Mutex<FrameBook>, shown: u64, close: Close, sink: Sink) -> bool {
    if shown == 0 {
        return false;
    }
    let facts = match lock_wait(book, Duration::from_millis(1)) {
        Some(mut b) => b.close(close),
        None => {
            FRAMES_DROPPED.fetch_add(1, Ordering::Relaxed);
            return false;
        }
    };
    write_frames(facts, sink);
    true
}

/// On the thread: harvested intervals into their frames, finished frames written in order.
fn frames_step(tel: &GpuTel, sink: Sink) {
    let facts = {
        let Some(mut b) = lock_wait(&FRAMES, Duration::from_millis(5)) else { return };
        let h = tel.take_harvested();
        b.attach(h);
        b.complete(tel.frame(), &|f| tel.lists_in(f))
    };
    write_frames(facts, sink);
}

/// The `memory` event: a source the tree cannot give is None. MiB.
fn memory_snapshot(frame: u64) -> MemoryFacts {
    let mb = |b: u64| Some(b >> 20);
    let committed = {
        use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX};
        // SAFETY: a zeroed counters structure with its size set, filled for this process.
        unsafe {
            let mut c: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
            c.cb = std::mem::size_of_val(&c) as u32;
            let ok = K32GetProcessMemoryInfo(windows_sys::Win32::System::Threading::GetCurrentProcess(), &mut c as *mut _ as *mut _, c.cb);
            (ok != 0).then_some(c.PrivateUsage as u64)
        }
    };
    let video = real::video_memory();
    // these two lock title-side state that a panic may have poisoned: a panic is no value
    let arenas = std::panic::catch_unwind(crate::hle::d3d12x::vram::arena_bytes).ok();
    let pages = std::panic::catch_unwind(crate::hle::d3d12x::vram::page_bytes).ok();
    let pools = std::panic::catch_unwind(crate::hle::d3d12x::sparse::pool_bytes).ok();
    MemoryFacts {
        frame,
        committed_mb: committed.and_then(mb),
        vram_local_mb: video.and_then(|v| mb(v.0)),
        vram_nonlocal_mb: video.and_then(|v| mb(v.1)),
        vram_budget_mb: video.and_then(|v| mb(v.2)),
        arenas_mb: arenas.and_then(|a| mb(a.0 as u64)),
        arenas_made_mb: arenas.and_then(|a| mb(a.1 as u64)),
        pages_mb: pages.and_then(|p| mb(p.0 as u64)),
        page_pools_mb: pages.and_then(|p| mb(p.1 as u64)),
        sparse_mb: pools.and_then(|p| mb(p.0 as u64)),
        sparse_mapped_mb: pools.and_then(|p| mb(p.1 as u64)),
    }
}

// ---------------------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Direct3D::D3D_FEATURE_LEVEL_12_0;
    use windows::Win32::Graphics::Dxgi::*;

    type Ev = (String, Vec<(&'static str, Value)>);

    fn get<'a>(f: &'a [(&'static str, Value)], k: &str) -> Option<&'a Value> {
        f.iter().find(|(n, _)| *n == k).map(|(_, v)| v)
    }

    fn lists_of(evs: &[Ev]) -> Vec<&Vec<(&'static str, Value)>> {
        evs.iter().filter(|(n, _)| n == "list").map(|(_, f)| f).collect()
    }

    /// The hardware device, or the WARP one. D3D12 hands out one device per adapter and process, so
    /// a test that removes its device must not use the adapter that other tests (and the runner) share.
    fn device(warp: bool) -> Option<ID3D12Device> {
        // SAFETY: plain DXGI / D3D12 creation, as the test runner does it.
        unsafe {
            let factory: IDXGIFactory6 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0)).ok()?;
            if warp {
                let adapter: IDXGIAdapter1 = factory.EnumWarpAdapter().ok()?;
                return crate::gpu::agility::create_device(&adapter, D3D_FEATURE_LEVEL_12_0).ok();
            }
            let sel = crate::gpu::adapter::Selector::from_env_or("auto");
            let (adapter, _) = crate::gpu::adapter::candidates(&factory, &sel).into_iter().next()?;
            crate::gpu::agility::create_device(&adapter, D3D_FEATURE_LEVEL_12_0).ok()
        }
    }

    /// A device, a queue of `ty` and a test-local engine over them.
    struct Rig {
        dev: ID3D12Device,
        queue: ID3D12CommandQueue,
        qraw: *mut c_void,
        qf: QueueFns,
        tel: GpuTel,
    }

    fn rig(ty: D3D12_COMMAND_LIST_TYPE, slots: u32, copy_ts: Option<bool>) -> Option<Rig> {
        rig_on(false, ty, slots, copy_ts)
    }

    fn rig_on(warp: bool, ty: D3D12_COMMAND_LIST_TYPE, slots: u32, copy_ts: Option<bool>) -> Option<Rig> {
        let dev = device(warp)?;
        let dfns = DevFns::plain(&dev)?;
        // SAFETY: a queue on the test's own device.
        let queue: ID3D12CommandQueue = unsafe { dev.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC { Type: ty, ..Default::default() }) }.ok()?;
        let qf = QueueFns::plain(&queue)?;
        let tel = GpuTel::new(Box::new(move || Some(dfns)), Box::new(QueueFns::plain_raw), 1, slots, copy_ts);
        Some(Rig { qraw: queue.as_raw(), dev, queue, qf, tel })
    }

    /// A closed list (with some GPU work when `heavy`) and what it needs to stay valid.
    struct TestList {
        list: ID3D12GraphicsCommandList,
        _alloc: ID3D12CommandAllocator,
        _bufs: Vec<ID3D12Resource>,
    }

    fn test_list(dev: &ID3D12Device, ty: D3D12_COMMAND_LIST_TYPE, heavy: bool) -> Option<TestList> {
        // SAFETY: plain D3D12 calls on the test's device.
        unsafe {
            let alloc: ID3D12CommandAllocator = dev.CreateCommandAllocator(ty).ok()?;
            let list: ID3D12GraphicsCommandList = dev.CreateCommandList(0, ty, &alloc, None::<&ID3D12PipelineState>).ok()?;
            let mut bufs = Vec::new();
            if heavy {
                let bytes = 64u64 << 20;
                for _ in 0..2 {
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
                            ..Default::default()
                        },
                        D3D12_RESOURCE_STATE_COMMON,
                        None,
                        &mut r,
                    )
                    .ok()?;
                    bufs.push(r?);
                }
                for _ in 0..16 {
                    list.CopyBufferRegion(&bufs[1], 0, &bufs[0], 0, bytes);
                }
            }
            list.Close().ok()?;
            Some(TestList { list, _alloc: alloc, _bufs: bufs })
        }
    }

    fn submit(r: &Rig, lists: &[&TestList], evs: &mut Vec<Ev>) {
        let raw: Vec<*mut c_void> = lists.iter().map(|l| l.list.as_raw()).collect();
        let mut sink = |ev: &str, f: Vec<(&'static str, Value)>| evs.push((ev.to_string(), f));
        r.tel.execute(r.qraw, &raw, "submit", |c| r.qf.execute(c), &mut sink);
    }

    fn harvest(r: &Rig, evs: &mut Vec<Ev>) {
        let mut sink = |ev: &str, f: Vec<(&'static str, Value)>| evs.push((ev.to_string(), f));
        r.tel.harvest(&mut sink);
    }

    /// Waits (the test may) until the queue's last signalled fence value has passed, or the device is gone.
    fn settle(r: &Rig) {
        settle_key(&r.tel, r.qraw);
    }

    fn settle_key(tel: &GpuTel, key: *mut c_void) {
        let q = tel.find_queue(key as usize).expect("queue clock");
        let end = Instant::now() + Duration::from_secs(10);
        while Instant::now() < end {
            {
                let c = plock(&q);
                // SAFETY: CPU-side read of the queue's fence.
                if c.fence.is_none() {
                    return; // an untimed queue has nothing to wait for
                }
                if let Some(f) = &c.fence {
                    let done = unsafe { f.GetCompletedValue() };
                    assert_ne!(done, u64::MAX, "the test's device was removed");
                    if done >= c.fence_value {
                        return;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("the queue fence never passed");
    }

    /// Frame 1: `lists`; frame 2: one list (resolves frame 1); the fence passes.
    fn two_frames(r: &Rig, lists: &[&TestList], evs: &mut Vec<Ev>) {
        r.tel.set_frame(1);
        submit(r, lists, evs);
        r.tel.set_frame(2);
        submit(r, &lists[..1], evs);
        settle(r);
    }

    #[test]
    fn two_lists_get_ticks_that_chain() {
        let direct = D3D12_COMMAND_LIST_TYPE_DIRECT;
        let Some(r) = rig(direct, LIST_SLOTS, None) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let (Some(l0), Some(l1)) = (test_list(&r.dev, direct, true), test_list(&r.dev, direct, true)) else {
            return crate::gpu::recomp::runner::skip_test("no GPU");
        };
        let mut evs = Vec::new();
        two_frames(&r, &[&l0, &l1], &mut evs);
        r.tel.set_frame(3);
        harvest(&r, &mut evs);
        let ls = lists_of(&evs);
        let f1: Vec<_> = ls.iter().filter(|f| get(f, "frame").and_then(Value::as_u64) == Some(1)).collect();
        assert_eq!(f1.len(), 2, "{evs:?}");
        let tick = |f: &Vec<(&'static str, Value)>, k: &str| get(f, k).and_then(Value::as_u64);
        for f in &f1 {
            let (s, e) = (tick(f, "start_tick").expect("start"), tick(f, "end_tick").expect("end"));
            assert!(s < e, "start {s} end {e}");
            assert!(get(f, "why").is_none() || get(f, "why") == Some(&Value::Null));
        }
        assert_eq!(tick(f1[0], "end_tick"), tick(f1[1], "start_tick"));
        assert!(evs.iter().any(|(n, _)| n == "clock"), "the first submit writes a clock event");
    }

    #[test]
    fn copy_queue_without_the_cap_is_unsupported() {
        let copy = D3D12_COMMAND_LIST_TYPE_COPY;
        let Some(r) = rig(copy, LIST_SLOTS, Some(false)) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let Some(l) = test_list(&r.dev, copy, false) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let mut evs = Vec::new();
        r.tel.set_frame(1);
        submit(&r, &[&l], &mut evs);
        harvest(&r, &mut evs);
        let ls = lists_of(&evs);
        assert_eq!(ls.len(), 1, "{evs:?}");
        assert_eq!(get(ls[0], "why"), Some(&Value::from("unsupported")));
        assert!(matches!(get(ls[0], "start_tick"), None | Some(Value::Null)));
        assert_eq!(get(ls[0], "queue_type"), Some(&Value::from("copy")));
    }

    #[test]
    fn resubmitted_list_keeps_its_ticks() {
        let direct = D3D12_COMMAND_LIST_TYPE_DIRECT;
        let Some(r) = rig(direct, LIST_SLOTS, None) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let Some(l0) = test_list(&r.dev, direct, true) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let mut evs = Vec::new();
        r.tel.set_frame(1);
        submit(&r, &[&l0], &mut evs);
        submit(&r, &[&l0], &mut evs); // again before the first is read
        r.tel.set_frame(2);
        submit(&r, &[&l0], &mut evs);
        settle(&r);
        r.tel.set_frame(3);
        harvest(&r, &mut evs);
        let ls = lists_of(&evs);
        let f1: Vec<_> = ls.iter().filter(|f| get(f, "frame").and_then(Value::as_u64) == Some(1)).collect();
        assert_eq!(f1.len(), 2, "{evs:?}");
        assert_eq!(get(f1[0], "resubmitted"), Some(&Value::Bool(false)));
        assert_eq!(get(f1[1], "resubmitted"), Some(&Value::Bool(true)));
        for f in &f1 {
            assert!(get(f, "start_tick").and_then(Value::as_u64).is_some() && get(f, "end_tick").and_then(Value::as_u64).is_some(), "{f:?}");
        }
    }

    #[test]
    fn a_full_set_marks_the_rest_full() {
        let direct = D3D12_COMMAND_LIST_TYPE_DIRECT;
        let Some(r) = rig(direct, 4, None) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let (Some(l0), Some(l1)) = (test_list(&r.dev, direct, false), test_list(&r.dev, direct, false)) else {
            return crate::gpu::recomp::runner::skip_test("no GPU");
        };
        let mut evs = Vec::new();
        r.tel.set_frame(1);
        submit(&r, &[&l0, &l1], &mut evs); // 3 of the 4 slots
        submit(&r, &[&l0, &l1], &mut evs); // needs 3 more
        harvest(&r, &mut evs);
        let ls = lists_of(&evs);
        // the first submit was stamped (its two lists wait in the open set); the second did not fit
        assert_eq!(ls.len(), 2, "{evs:?}");
        assert!(ls.iter().all(|f| get(f, "why") == Some(&Value::from("full"))));
        // the same two PC lists, submitted again before the first stamps were read
        assert!(ls.iter().all(|f| get(f, "resubmitted") == Some(&Value::Bool(true))));
        assert_eq!(r.tel.drops()[DROP_FULL], 2);
    }

    #[test]
    fn device_removal_marks_removed_without_hanging() {
        let direct = D3D12_COMMAND_LIST_TYPE_DIRECT;
        let Some(r) = rig_on(true, direct, LIST_SLOTS, None) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let Ok(dev5) = r.dev.cast::<ID3D12Device5>() else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let Some(l0) = test_list(&r.dev, direct, false) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let mut evs = Vec::new();
        two_frames(&r, &[&l0], &mut evs);
        if r.tel.find_queue(r.qraw as usize).map_or(true, |q| plock(&q).why_off.is_some()) {
            return crate::gpu::recomp::runner::skip_test("no GPU"); // this adapter does not time its queue
        }
        // SAFETY: the test's own device.
        unsafe { dev5.RemoveDevice() };
        r.tel.set_frame(3);
        let t0 = Instant::now();
        harvest(&r, &mut evs);
        assert!(t0.elapsed() < Duration::from_secs(1), "harvest took {:?}", t0.elapsed());
        assert!(r.tel.is_removed());
        let ls = lists_of(&evs);
        assert_eq!(ls.len(), 2, "{evs:?}");
        assert!(ls.iter().all(|f| get(f, "why") == Some(&Value::from("removed"))));
        // the exit flush finds nothing more to write and does not hang either
        let mut more = Vec::new();
        r.tel.finish(&mut |ev: &str, f: Vec<(&'static str, Value)>| more.push((ev.to_string(), f)));
        assert!(lists_of(&more).is_empty());
    }

    #[test]
    fn a_set_is_not_read_before_two_frames_after_its_frame() {
        let direct = D3D12_COMMAND_LIST_TYPE_DIRECT;
        let Some(r) = rig(direct, LIST_SLOTS, None) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let Some(l0) = test_list(&r.dev, direct, false) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let mut evs = Vec::new();
        two_frames(&r, &[&l0], &mut evs);
        // frame 2: the fence has passed, but frame 1's set was written only one frame ago
        harvest(&r, &mut evs);
        assert_eq!(r.tel.reads.load(Ordering::Relaxed), 0);
        assert!(lists_of(&evs).is_empty(), "{evs:?}");
        r.tel.set_frame(3);
        harvest(&r, &mut evs);
        assert_eq!(r.tel.reads.load(Ordering::Relaxed), 1);
        assert_eq!(r.tel.early_reads.load(Ordering::Relaxed), 0);
        assert_eq!(lists_of(&evs).len(), 1);
    }

    #[test]
    fn exit_flush_writes_pending_lists_with_null_ticks() {
        let direct = D3D12_COMMAND_LIST_TYPE_DIRECT;
        let Some(r) = rig(direct, LIST_SLOTS, None) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let Some(l0) = test_list(&r.dev, direct, false) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let mut evs = Vec::new();
        r.tel.set_frame(1);
        submit(&r, &[&l0], &mut evs);
        r.tel.finish(&mut |ev: &str, f: Vec<(&'static str, Value)>| evs.push((ev.to_string(), f)));
        let ls = lists_of(&evs);
        assert_eq!(ls.len(), 1, "{evs:?}");
        assert_eq!(get(ls[0], "why"), Some(&Value::from("exit")));
    }

    #[test]
    fn started_is_true_once_a_frame_is_shown() {
        let tel = GpuTel::new(Box::new(|| None), Box::new(|_| None), 1, 8, None);
        assert!(!tel.started());
        tel.set_frame(1);
        assert!(tel.started());
    }

    #[test]
    fn built_by_names_the_builder_from_the_thread_name() {
        assert_eq!(built_by(Some("xcl-pso-pre-1")), "preload");
        assert_eq!(built_by(Some("xcl-pso-x3")), "extra");
        assert_eq!(built_by(Some("xcl-pso-7")), "worker");
        assert_eq!(built_by(Some("xcl-telemetry")), "xcl");
        assert_eq!(built_by(Some("main")), "title");
        assert_eq!(built_by(None), "title");
    }

    #[test]
    fn frame_pso_wait_splits_draw_and_dispatch_and_keeps_the_sum() {
        let mut b = FrameBook::new();
        let mut c = close_of(1, 16, 1, true);
        c.counters = Counters { pso_draw_us: 3000, pso_dispatch_us: 1500, ..Counters::default() };
        b.close(c);
        let r = &b.pending[0];
        assert_eq!((r.pso_wait_draw_ms, r.pso_wait_dispatch_ms, r.pso_wait_ms), (3.0, 1.5, 4.5));
        // the next frame takes deltas of the previous counters
        let mut c = close_of(2, 32, 1, true);
        c.counters = Counters { pso_draw_us: 3500, pso_dispatch_us: 1500, ..Counters::default() };
        b.close(c);
        let r = &b.pending[1];
        assert_eq!((r.pso_wait_draw_ms, r.pso_wait_dispatch_ms, r.pso_wait_ms), (0.5, 0.0, 0.5));
    }

    #[test]
    fn skip_index_follows_call_and_reason() {
        let name = |i: usize| COUNT_NAMES[i];
        // a direct draw: stand-in counts; the reasons only a dispatch has, and None, are failed
        for (why, want) in [
            (Some("building"), "draw_standin_building"),
            (Some("late"), "draw_standin_late"),
            (Some("abandoned"), "draw_standin_abandoned"),
            (Some("quarantined"), "draw_standin_quarantined"),
            (Some("failed"), "draw_standin_failed"),
            (None, "draw_standin_failed"),
            (Some("image_only"), "draw_standin_failed"),
            (Some("background"), "draw_standin_failed"),
            (Some("size_gate"), "draw_standin_failed"),
        ] {
            assert_eq!(name(skip_index(Call::Draw, why)), want, "{why:?}");
        }
        // an indirect draw
        for (why, want) in [
            (Some("building"), "ei_draw_skip_building"),
            (Some("late"), "ei_draw_skip_late"),
            (Some("abandoned"), "ei_draw_skip_abandoned"),
            (Some("quarantined"), "ei_draw_skip_quarantined"),
            (Some("failed"), "ei_draw_skip_failed"),
            (None, "ei_draw_skip_failed"),
            (Some("image_only"), "ei_draw_skip_failed"),
            (Some("background"), "ei_draw_skip_failed"),
            (Some("size_gate"), "ei_draw_skip_failed"),
        ] {
            assert_eq!(name(skip_index(Call::Indirect(false), why)), want, "{why:?}");
        }
        // a dispatch, direct or indirect: one count per reason name
        for call in [Call::Dispatch, Call::Indirect(true)] {
            for r in ["building", "late", "image_only", "background", "size_gate", "abandoned", "quarantined", "failed"] {
                assert_eq!(name(skip_index(call, Some(r))), format!("dispatch_skip_{r}"), "{call:?} {r}");
            }
            assert_eq!(name(skip_index(call, None)), "dispatch_skip_failed");
            assert_eq!(name(skip_index(call, Some("other"))), "dispatch_skip_other");
            assert_eq!(name(skip_index(call, Some("nothing known"))), "dispatch_skip_other");
        }
        // the table has a name for every slot, each once
        let mut seen = std::collections::BTreeSet::new();
        assert!(COUNT_NAMES.iter().all(|n| seen.insert(*n)));
        assert_eq!(COUNT_NAMES[MUSTDRAW_WAIT_US], "mustdraw_wait_us");
        assert_eq!(COUNT_NAMES[32], "reserved_0");
    }

    #[test]
    fn frame_count_fields_keep_totals_and_drop_zero_skips() {
        let mut d = [0u64; 33];
        let f = frame_count_fields(&d);
        assert_eq!(f, [("draw_total", 0), ("ei_draw_total", 0), ("dispatch_total", 0), ("ei_dispatch_total", 0)]);
        d[0] = 400;
        d[5] = 3; // ei_draw_skip_late
        d[24] = 7; // dispatch_skip_failed
        d[29] = 2500; // the wait goes out as mustdraw_wait_ms, not here
        d[32] = 9; // reserved_0 is never written
        let f = frame_count_fields(&d);
        assert_eq!(f, [("draw_total", 400), ("ei_draw_total", 0), ("dispatch_total", 0), ("ei_dispatch_total", 0), ("ei_draw_skip_late", 3), ("dispatch_skip_failed", 7)]);
        assert!(f.iter().all(|(n, _)| *n != "mustdraw_wait_us" && *n != "mustdraw_wait_ms" && *n != "reserved_0"));
    }

    #[test]
    fn frame_book_takes_count_deltas() {
        let mut b = FrameBook::new();
        let mut c = close_of(1, 16, 1, true);
        let mut counts = [0u64; 33];
        counts[0] = 100;
        counts[4] = 10;
        counts[29] = 1500;
        c.counters = Counters { counts, ..Counters::default() };
        b.close(c);
        assert_eq!((b.pending[0].counts[0], b.pending[0].counts[4], b.pending[0].counts[29]), (100, 10, 1500));
        let mut c = close_of(2, 32, 1, true);
        counts[0] = 150;
        counts[4] = 10;
        counts[5] = 2;
        counts[29] = 4000;
        c.counters = Counters { counts, ..Counters::default() };
        b.close(c);
        let r = &b.pending[1];
        assert_eq!((r.counts[0], r.counts[4], r.counts[5], r.counts[29]), (50, 0, 2, 2500));
        let facts = frame_facts(r, 1, None);
        assert_eq!(facts.mustdraw_wait_ms, 2.5);
        assert!(facts.counts.contains(&("ei_draw_skip_late", 2)));
        assert!(!facts.counts.iter().any(|(n, _)| *n == "ei_draw_skip_building"));
    }

    #[test]
    fn a_skipped_dispatch_marks_its_list_until_reset() {
        // the tier cannot be forced in a unit test: the pure helpers the entry points use
        let mut set = HashSet::new();
        let (a, b) = (0x10usize, 0x20usize);
        assert!(!writer_hit(&set, a));
        writer_mark(&mut set, a);
        assert!(writer_hit(&set, a) && !writer_hit(&set, b));
        writer_mark(&mut set, 0);
        assert!(!writer_hit(&set, 0), "a null list is never marked");
        set.remove(&a); // what writer_clear does for the list's Reset
        assert!(!writer_hit(&set, a));
        // only a dispatch kind marks a list
        assert!(is_dispatch(Call::Dispatch) && is_dispatch(Call::Indirect(true)));
        assert!(!is_dispatch(Call::Draw) && !is_dispatch(Call::Indirect(false)));
    }

    #[test]
    fn the_exit_flush_before_the_first_frame_does_not_stop_the_engine() {
        let tel = GpuTel::new(Box::new(|| None), Box::new(|_| None), 1, 8, None);
        gpu_final(&tel); // what write_final does before the title starts (host.rs)
        assert!(!tel.stopped.load(Ordering::Acquire), "the pre-run report write stopped the engine");
        // a list submitted afterwards is still handled by the engine, not passed through
        let mut evs: Vec<Ev> = Vec::new();
        let mut submitted = 0;
        let mut sink = |ev: &str, f: Vec<(&'static str, Value)>| evs.push((ev.to_string(), f));
        let l = 0x10usize as *mut c_void;
        tel.execute(0x20usize as *mut c_void, &[l], "submit", |_| submitted += 1, &mut sink);
        assert_eq!(submitted, 1);
        // no device in this engine: the list is counted as unsupported, which only the live path does
        assert_eq!(tel.drops()[DROP_UNSUPPORTED], 1);
    }

    // ---- frame glue (no GPU, no global tier) ----

    fn close_of(n: u64, qpc_end: i64, lists: u32, sampled: bool) -> Close {
        Close { n, qpc_end, qpc0: 0, qpf: 1000, present_ms: 1.0, thread_wait_ms: 0.0, lists, sampled, counters: Counters::default() }
    }

    fn hl(frame: u64, s: f64, e: f64) -> HarvestedList {
        HarvestedList { frame, queue: 0, start_ms: Some(s), end_ms: Some(e) }
    }

    #[test]
    fn a_present_span_closes_a_frame_only_when_it_showed_one() {
        assert_eq!(take_shown(), 0);
        set_shown(7);
        assert_eq!(take_shown(), 7);
        assert_eq!(take_shown(), 0, "taking it clears it");
        let book = Mutex::new(FrameBook::new());
        let mut evs: Vec<Ev> = Vec::new();
        let mut sink = |ev: &str, f: Vec<(&'static str, Value)>| evs.push((ev.to_string(), f));
        assert!(!close_if_shown(&book, 0, close_of(1, 100, 0, true), &mut sink));
        assert!(plock(&book).pending.is_empty());
        assert!(close_if_shown(&book, 5, close_of(5, 100, 0, true), &mut sink));
        let b = plock(&book);
        assert_eq!(b.pending.len(), 1);
        assert_eq!(b.pending[0].i, 5);
        assert_eq!(b.pending[0].interval_ms, 100.0, "the first frame counts from the start");
    }

    #[test]
    fn cpu_ms_is_clamped_at_zero() {
        assert_eq!(xlog::frame_cpu_ms(1.0, 2.0, 3.0), 0.0);
        let mut b = FrameBook::new();
        let mut c = close_of(1, 5, 0, false);
        c.present_ms = 10.0; // longer than the 5 ms interval
        b.close(c);
        let f = b.complete(1, &|_| 0);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].cpu_ms, 0.0);
        assert_eq!(f[0].why, Some("sampled_out"));
        assert!(f[0].gpu_ms.is_none() && !f[0].sampled);
    }

    #[test]
    fn a_frame_completes_when_its_lists_are_harvested() {
        let mut b = FrameBook::new();
        assert!(b.close(close_of(1, 16, 2, true)).is_empty());
        assert!(b.complete(2, &|_| 2).is_empty(), "nothing harvested yet");
        b.attach(vec![hl(0, 1.0, 2.0)]);
        assert!(b.complete(3, &|_| 2).is_empty(), "one of two lists");
        b.attach(vec![hl(0, 2.0, 4.0)]);
        let f = b.complete(3, &|_| 2);
        assert_eq!(f.len(), 1);
        assert_eq!((f[0].i, f[0].lists, f[0].gpu_lists_null, f[0].why), (1, 2, 0, None));
        assert_eq!((f[0].gpu_ms, f[0].gpu_first_ms, f[0].gpu_last_ms), (Some(3.0), Some(1.0), Some(4.0)));
    }

    #[test]
    fn overlapping_lists_count_once_and_null_lists_are_counted() {
        let mut b = FrameBook::new();
        b.close(close_of(1, 16, 3, true));
        b.attach(vec![hl(0, 1.0, 5.0), hl(0, 2.0, 4.0), HarvestedList { frame: 0, queue: 0, start_ms: None, end_ms: None }]);
        let f = b.complete(3, &|_| 3);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].gpu_ms, Some(4.0), "the union, not the sum");
        assert_eq!(f[0].gpu_lists_null, 1);
    }

    #[test]
    fn a_frame_gives_up_after_late_frames() {
        let mut b = FrameBook::new();
        b.close(close_of(1, 16, 2, true));
        b.attach(vec![hl(0, 1.0, 2.0)]);
        assert!(b.complete(1 + LATE_FRAMES - 1, &|_| 2).is_empty());
        let f = b.complete(1 + LATE_FRAMES, &|_| 2);
        assert_eq!(f.len(), 1);
        assert_eq!((f[0].gpu_ms, f[0].gpu_lists_null), (Some(1.0), 1), "a lower bound");
        b.close(close_of(2, 32, 1, true));
        let f = b.complete(2 + LATE_FRAMES, &|_| 1);
        assert_eq!((f[0].i, f[0].gpu_ms, f[0].why), (2, None, Some("late")));
    }

    #[test]
    fn frames_are_written_in_order_of_i() {
        let mut b = FrameBook::new();
        for n in 1..=3 {
            b.close(close_of(n, n as i64 * 16, 1, true));
        }
        b.attach(vec![hl(2, 5.0, 6.0), hl(1, 3.0, 4.0)]);
        assert!(b.complete(4, &|_| 1).is_empty(), "frame 1 is not done: 2 and 3 wait behind it");
        b.attach(vec![hl(0, 1.0, 2.0)]);
        let f = b.complete(4, &|_| 1);
        assert_eq!(f.iter().map(|x| x.i).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(b.last_written, 3);
        // an interval for a frame written already is dropped, not kept
        b.attach(vec![hl(0, 1.0, 2.0)]);
        assert!(b.waiting.is_empty());
    }

    #[test]
    fn the_pending_frames_are_bounded() {
        let mut b = FrameBook::new();
        let mut written = Vec::new();
        for n in 1..=(PENDING_FRAMES as u64 + 3) {
            written.extend(b.close(close_of(n, n as i64, 1, true)));
        }
        assert_eq!(b.pending.len(), PENDING_FRAMES);
        assert_eq!(written.iter().map(|f| f.i).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert!(written.iter().all(|f| f.why == Some("late") && f.gpu_ms.is_none() && f.gpu_lists_null == 1));
    }

    #[test]
    fn the_exit_flush_nulls_every_waiting_frame() {
        let mut b = FrameBook::new();
        b.close(close_of(1, 16, 2, true));
        b.close(close_of(2, 32, 0, false));
        let f = b.flush("exit");
        assert_eq!(f.len(), 2);
        assert_eq!((f[0].why, f[0].gpu_ms, f[0].gpu_lists_null), (Some("exit"), None, 2));
        assert_eq!(f[1].why, Some("sampled_out"));
        assert!(b.pending.is_empty());
    }

    #[test]
    fn lists_are_counted_per_frame() {
        let t = GpuTel::new(Box::new(|| None), Box::new(|_| None), 1, 4, None);
        t.note_lists(5, 2);
        t.note_lists(5, 1);
        assert_eq!(t.lists_in(5), 3);
        assert_eq!(t.lists_in(5 + 64), 0, "another frame on the same slot");
        t.note_lists(5 + 64, 1);
        assert_eq!((t.lists_in(5), t.lists_in(5 + 64)), (0, 1));
    }

    // ---- part 3: pass engine, pipelines, io (no GPU) ----

    /// An engine with the pass tier on and no device. The counters of a list are cached per thread by
    /// (engine address, list): a test uses list pointers no other test uses.
    fn tel_pass(sample: u32) -> GpuTel {
        GpuTel::new(Box::new(|| None), Box::new(|_| None), sample, 8, None).with_pass(false)
    }

    fn note_draws(tel: &GpuTel, l: usize, n: u32) {
        tel.pass_note(l as *mut c_void, |c| {
            c.draws.fetch_add(n, Ordering::Relaxed);
        });
    }

    /// The timing tests share the GPU with each other: one at a time.
    static GPU_TIMING: Mutex<()> = Mutex::new(());

    fn entry_with(rec: PassRec) -> ListEntry {
        ListEntry {
            frame: 1,
            seq: 0,
            list: rec.list,
            chunk: 0,
            slot: Some(0),
            cpu_us: 0,
            resubmitted: false,
            origin: "submit",
            why: None,
            pass: Some(Box::new(rec)),
        }
    }

    #[test]
    fn counters_of_a_list_recorded_on_one_thread_reach_its_submit_on_another() {
        let tel = tel_pass(1);
        let l = 0x1100usize;
        std::thread::scope(|s| {
            s.spawn(|| {
                note_draws(&tel, l, 3);
                tel.pass_note(l as *mut c_void, |c| {
                    c.dispatches.fetch_add(1, Ordering::Relaxed);
                    c.note_pipe(9);
                });
            });
        });
        let recs = tel.pass_take(&[l as *mut c_void], 1);
        let r = recs[0].as_ref().expect("a recording");
        assert_eq!((r.passes[0].draws, r.passes[0].dispatches, r.passes[0].pipes.clone()), (3, 1, vec![9]));
        let again = tel.pass_take(&[l as *mut c_void], 1);
        assert!(again[0].as_ref().is_some_and(|r| r.passes[0].is_empty()));
    }

    #[test]
    fn the_same_list_on_the_same_thread_every_frame_loses_nothing() {
        let tel = tel_pass(1);
        let l = 0x1200usize;
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let (ack_tx, ack_rx) = std::sync::mpsc::channel::<()>();
        let tel_ref = &tel;
        std::thread::scope(|s| {
            s.spawn(move || {
                for _ in 0..3 {
                    note_draws(tel_ref, l, 2);
                    let _ = tx.send(());
                    let _ = ack_rx.recv();
                }
            });
            for frame in 1..=3u64 {
                rx.recv().expect("the recording thread");
                let recs = tel.pass_take(&[l as *mut c_void], frame);
                assert_eq!(recs[0].as_ref().map(|r| r.passes[0].draws), Some(2), "frame {frame}");
                let _ = ack_tx.send(());
            }
        });
    }

    #[test]
    fn a_boundary_splits_the_counters_of_its_list() {
        let tel = tel_pass(1);
        let l = 0x1300usize;
        note_draws(&tel, l, 2);
        {
            let mut st = plock(&tel.pass.state);
            tel.drain_into(&mut st, l, 1);
            st.boundary(l, 1, 1).unwrap();
        }
        note_draws(&tel, l, 3);
        let recs = tel.pass_take(&[l as *mut c_void], 1);
        let r = recs[0].as_ref().expect("a recording");
        assert_eq!((r.passes.len(), r.passes[0].draws, r.passes[1].draws), (2, 2, 3));
    }

    #[test]
    fn a_reset_zeroes_the_counters() {
        let tel = tel_pass(1);
        let l = 0x1400usize;
        note_draws(&tel, l, 4);
        tel.list_reset(l as *mut c_void, true);
        let recs = tel.pass_take(&[l as *mut c_void], 1);
        assert_eq!(recs[0].as_ref().map(|r| r.passes[0].draws), Some(0));
    }

    #[test]
    fn the_count_map_is_bounded() {
        let m = CountMap::new(2);
        let a = m.get_or_insert(0x40).expect("first");
        assert!(m.get_or_insert(0x80).is_some());
        assert!(m.get_or_insert(0xC0).is_none(), "past the cap");
        assert!(m.get(0xC0).is_none());
        assert!(Arc::ptr_eq(&a, &m.get_or_insert(0x40).expect("an existing key still works")));
        assert!(m.get(0x80).is_some());
    }

    #[test]
    fn note_pipe_keeps_first_seen_order_and_truncates() {
        let c = ListCounts::new();
        for id in [5, 3, 5, 4] {
            c.note_pipe(id);
        }
        for id in 10..20 {
            c.note_pipe(id);
        }
        c.draws.fetch_add(1, Ordering::Relaxed);
        let d = c.drain();
        assert_eq!((d.draws, d.pipes.len(), d.truncated), (1, MAX_PIPES, true));
        assert_eq!(&d.pipes[..4], &[5, 3, 4, 10]);
        assert!(c.drain().is_empty(), "a drain zeroes the counters");
        c.note_pipe(5);
        assert_eq!(c.drain().pipes, vec![5]);
    }

    #[test]
    fn counting_does_not_depend_on_sampling() {
        let tel = tel_pass(2);
        let l = 0x1500usize;
        tel.set_frame(1); // unsampled
        note_draws(&tel, l, 2);
        tel.set_frame(2);
        note_draws(&tel, l, 1);
        let recs = tel.pass_take(&[l as *mut c_void], 2);
        assert_eq!(recs[0].as_ref().map(|r| r.passes[0].draws), Some(3));
        tel.set_frame(3);
        note_draws(&tel, l, 1);
        let recs = tel.pass_take(&[l as *mut c_void], 3);
        assert_eq!(recs[0].as_ref().map(|r| r.passes[0].draws), Some(1), "no spill-over");
    }

    #[test]
    fn a_pass_kind_follows_its_work_with_indirect_before_empty() {
        let k = |d, s, i| Counts { draws: d, dispatches: s, indirect: i, ..Default::default() }.kind();
        assert_eq!((k(1, 1, 1), k(0, 1, 1), k(0, 0, 1), k(0, 0, 0)), ("draw", "dispatch", "indirect", "empty"));
    }

    #[test]
    fn a_relabel_gives_a_reused_pointer_a_new_id() {
        let seq = AtomicU64::new(10);
        let mut m = PipeMap::new(2);
        let a = m.id_of(0x10, &seq).expect("a");
        let b = m.relabel(0x10, &seq).expect("b");
        assert_ne!(a, b);
        assert_eq!(m.id_of(0x10, &seq), Some(b));
        m.id_of(0x20, &seq).expect("second");
        assert_eq!(m.relabel(0x30, &seq), None, "at the cap an unknown pointer gets nothing");
        let c = m.relabel(0x20, &seq).expect("a known pointer is relabelled at the cap");
        assert!(c > b);
    }

    #[test]
    fn the_thread_cache_follows_a_relabel() {
        let p = 0x7E57_0000usize;
        let a = pipe_id(p).expect("a");
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
        std::thread::scope(|s| {
            let other = s.spawn(move || {
                assert_eq!(pipe_id(p), Some(a), "this thread caches the old id");
                let _ = tx.send(());
                let _ = go_rx.recv();
                pipe_id(p)
            });
            rx.recv().expect("the other thread");
            let b = pipe_relabel(p).expect("b");
            assert_ne!(a, b);
            assert_eq!(pipe_id(p), Some(b), "this thread follows");
            let _ = go_tx.send(());
            assert_eq!(other.join().expect("the other thread"), Some(b), "the other thread follows too");
        });
    }

    #[test]
    fn pipeline_bind_fields_list_the_stages_that_have_bytes() {
        let f = pipeline_bind_fields(7, &[("vs", Some(&[1u8, 2, 3][..])), ("ps", None)]);
        assert_eq!(get(&f, "id"), Some(&Value::from(7u64)));
        assert_eq!(get(&f, "stages"), Some(&Value::Array(vec![Value::from("vs")])));
        assert_eq!(get(&f, "records"), Some(&Value::Array(vec![Value::from(format!("{:016x}", crate::gpu::shader::fnv64(&[1, 2, 3])))])));
    }

    #[test]
    fn pass_notes_reach_the_recording_at_submit() {
        let tel = tel_pass(1);
        let (a, b) = (0x1600usize as *mut c_void, 0x1700usize as *mut c_void);
        note_draws(&tel, 0x1600, 3);
        tel.pass_note(b, |c| {
            c.dispatches.fetch_add(1, Ordering::Relaxed);
        });
        let recs = tel.pass_take(&[a, b], 5);
        let (ra, rb) = (recs[0].as_ref().expect("a"), recs[1].as_ref().expect("b"));
        assert_eq!((ra.passes[0].draws, ra.passes[0].dispatches), (3, 0));
        assert_eq!((rb.passes[0].draws, rb.passes[0].dispatches), (0, 1));
        // submitted: no recording is left in the map, and a second submit finds an empty one
        assert!(plock(&tel.pass.state).recs.is_empty());
        let again = tel.pass_take(&[a], 5);
        assert!(again[0].as_ref().is_some_and(|r| r.passes[0].is_empty()));
    }

    #[test]
    fn a_reset_drops_the_unsubmitted_blocks_and_returns_them() {
        let mut st = PassState::new(4);
        let l = 0x10usize;
        assert_eq!(st.boundary(l, 3, 1), Ok(0));
        assert_eq!(st.boundary(l, 1, 1), Ok(1));
        for _ in 2..BLOCK_SLOTS + 1 {
            st.boundary(l, 1, 1).unwrap();
        }
        assert_eq!(st.recs[&l].blocks.len(), 2, "the 257th boundary took a second block");
        assert_eq!(st.free.len(), 2);
        assert_eq!(st.reset(l), 2);
        assert_eq!(st.free.len(), 4);
        assert!(st.recs.is_empty());
        // a recording that was submitted is the entry's now: a Reset of the list does not touch its blocks
        assert!(st.boundary(l, 1, 2).is_ok());
        let rec = st.take(l, 2);
        assert_eq!(st.reset(l), 0);
        assert_eq!(st.free.len(), 3);
        st.free_blocks(rec.blocks.clone());
        assert_eq!(st.free.len(), 4);
    }

    #[test]
    fn a_recording_takes_at_most_16_blocks_and_the_pool_can_run_out() {
        let mut st = PassState::new(BLOCKS);
        let l = 0x10usize;
        for _ in 0..MAX_REC_BLOCKS as u32 * BLOCK_SLOTS {
            st.boundary(l, 1, 1).unwrap();
        }
        assert_eq!(st.boundary(l, 1, 1), Err("full"));
        assert_eq!(st.boundary(l, 1, 1), Err("full"), "it stays full");
        assert_eq!(st.recs[&l].blocks.len(), MAX_REC_BLOCKS);
        assert_eq!(st.recs[&l].why, Some("full"));
        // the passes it did split still have their counters
        assert_eq!(st.recs[&l].passes.len(), MAX_REC_BLOCKS * BLOCK_SLOTS as usize + 1);
        // an empty pool: no block, no stamp
        let mut one = PassState::new(1);
        assert!(one.boundary(0x1, 1, 1).is_ok());
        assert_eq!(one.boundary(0x2, 1, 1), Err("full"));
        // a recording nobody submitted or Reset gives its block back after LATE_FRAMES
        assert_eq!(one.boundary(0x3, 1, 1 + LATE_FRAMES + 1), Ok(0));
        assert!(!one.recs.contains_key(&0x1));
    }

    #[test]
    fn parked_blocks_wait_for_the_gpu() {
        let mut st = PassState::new(2);
        let rec = {
            st.boundary(0x10, 1, 3).unwrap();
            st.take(0x10, 3)
        };
        st.park(3, rec.blocks.clone());
        st.release_parked(3 + LATE_FRAMES - 1);
        assert_eq!(st.free.len(), 1);
        st.release_parked(3 + LATE_FRAMES);
        assert_eq!(st.free.len(), 2);
    }

    #[test]
    fn block_ranges_of_a_set_are_joined() {
        let mut a = PassRec::new(1, 1);
        a.blocks = vec![0, 1, 3];
        a.total = 2 * BLOCK_SLOTS + 10;
        assert_eq!(a.ranges(), vec![(0, 256), (256, 256), (768, 10)]);
        assert_eq!(a.global(BLOCK_SLOTS + 5), Some(BLOCK_SLOTS + 5));
        assert_eq!(a.global(2 * BLOCK_SLOTS), Some(3 * BLOCK_SLOTS));
        let mut b = PassRec::new(2, 1);
        b.blocks = vec![4];
        b.total = 3;
        let entries = vec![entry_with(a), entry_with(b)];
        assert_eq!(resolve_ranges(&entries), vec![(0, 512), (768, 10), (1024, 3)]);
        assert!(resolve_ranges(&[]).is_empty());
    }

    fn rec_of_three_passes() -> PassRec {
        let mut r = PassRec::new(0x10, 1);
        r.total = 2;
        r.bounds = vec![4, 2];
        r.passes = vec![Counts { draws: 5, pipes: vec![1, 2], ..Default::default() }, Counts { dispatches: 2, pipes: vec![9], ..Default::default() }, Counts::default()];
        r.ticks = vec![100, 200];
        r
    }

    fn ctx() -> PassCtx {
        PassCtx { frame: 3, seq: 11, queue: 1, resubmitted: false, list_why: None, start: Some(50), end: Some(300) }
    }

    #[test]
    fn passes_chain_from_the_list_start_to_the_list_end() {
        let rec = rec_of_three_passes();
        let ps = pass_facts(&rec, &ctx(), &|t| Some(t as f64 / 10.0));
        assert_eq!(ps.len(), 3);
        let t = |p: &PassFacts| (p.start_tick, p.end_tick);
        assert_eq!((t(&ps[0]), t(&ps[1]), t(&ps[2])), ((Some(50), Some(100)), (Some(100), Some(200)), (Some(200), Some(300))));
        assert_eq!((ps[0].start_ms, ps[2].end_ms), (Some(5.0), Some(30.0)));
        assert_eq!((ps[0].kind, ps[1].kind, ps[2].kind), ("draw", "dispatch", "empty"));
        assert_eq!((ps[0].barriers_before, ps[1].barriers_before, ps[2].barriers_before), (0, 4, 2));
        assert_eq!(ps[0].pipelines, vec![1, 2]);
        assert!(ps.iter().all(|p| p.why.is_none() && !p.resubmitted && p.frame == 3 && p.list == 11 && p.queue == 1));
        assert_eq!(ps.iter().map(|p| p.i).collect::<Vec<_>>(), vec![0, 1, 2]);
    }

    #[test]
    fn a_list_without_boundaries_gets_one_pass_over_the_list() {
        let mut rec = PassRec::new(0x10, 1);
        rec.passes[0].dispatches = 1;
        let ps = pass_facts(&rec, &ctx(), &|t| Some(t as f64));
        assert_eq!(ps.len(), 1);
        assert_eq!((ps[0].start_tick, ps[0].end_tick, ps[0].kind), (Some(50), Some(300), "dispatch"));
        // a list nobody noted anything on
        let ps = pass_facts(&PassRec::new(0x10, 1), &ctx(), &|t| Some(t as f64));
        assert_eq!((ps.len(), ps[0].kind), (1, "empty"));
    }

    #[test]
    fn a_resubmitted_list_has_null_pass_ticks() {
        let rec = rec_of_three_passes();
        let mut c = ctx();
        c.resubmitted = true;
        let ps = pass_facts(&rec, &c, &|t| Some(t as f64));
        assert_eq!(ps.len(), 3);
        assert!(ps.iter().all(|p| p.start_tick.is_none() && p.end_tick.is_none() && p.start_ms.is_none() && p.why == Some("resubmitted") && p.resubmitted));
        // the counters are still there
        assert_eq!(ps[0].draws, 5);
        // a list with null ticks of its own: its reason
        let mut c = ctx();
        c.list_why = Some("late");
        c.start = None;
        c.end = None;
        let ps = pass_facts(&rec, &c, &|t| Some(t as f64));
        assert!(ps.iter().all(|p| p.start_tick.is_none() && p.why == Some("late")));
    }

    #[test]
    fn bad_pass_ticks_are_null_with_a_reason() {
        let mut rec = rec_of_three_passes();
        rec.ticks = vec![100, 0];
        let ps = pass_facts(&rec, &ctx(), &|t| Some(t as f64));
        assert_eq!((ps[0].why, ps[1].why, ps[2].why), (None, Some("zero"), Some("zero")));
        rec.ticks = vec![200, 100];
        let ps = pass_facts(&rec, &ctx(), &|t| Some(t as f64));
        assert_eq!((ps[0].why, ps[1].why, ps[2].why), (None, Some("order"), None));
        rec.ticks = Vec::new();
        let ps = pass_facts(&rec, &ctx(), &|t| Some(t as f64));
        assert_eq!((ps[0].why, ps[1].why), (Some("late"), Some("late")));
        let ps = pass_facts(&rec, &ctx(), &|_| None);
        assert_eq!(ps[0].why, Some("late"));
        rec.ticks = vec![100, 200];
        let ps = pass_facts(&rec, &ctx(), &|_| None);
        assert!(ps.iter().all(|p| p.why == Some("no_frequency") && p.start_tick.is_none()));
    }

    #[test]
    fn a_full_recording_says_so_on_its_last_pass() {
        let mut rec = rec_of_three_passes();
        rec.why = Some("full");
        let ps = pass_facts(&rec, &ctx(), &|t| Some(t as f64));
        assert_eq!((ps[0].why, ps[1].why, ps[2].why), (None, None, Some("full")));
        assert!(ps[2].end_tick.is_some(), "the ticks of that pass are valid, it just spans more");
        let ps = pass_facts(&PassRec::without_stamps(1, 1, "copy_list"), &ctx(), &|t| Some(t as f64));
        assert_eq!((ps.len(), ps[0].why, ps[0].start_tick), (1, Some("copy_list"), Some(50)));
    }

    #[test]
    fn pipeline_ids_of_a_pass_are_distinct_first_seen_and_capped_at_eight() {
        let mut c = Counts::default();
        for id in [5, 3, 5, 4] {
            c.pipe(id);
        }
        assert_eq!((c.pipes.clone(), c.truncated), (vec![5, 3, 4], false));
        for id in 10..20 {
            c.pipe(id);
        }
        assert_eq!(c.pipes.len(), MAX_PIPES);
        assert!(c.truncated);
        assert_eq!(&c.pipes[..4], &[5, 3, 4, 10]);
        let mut d = Counts { draws: 1, ..Default::default() };
        d.merge(&c);
        assert_eq!((d.draws, d.pipes.len(), d.truncated), (1, MAX_PIPES, true));
    }

    #[test]
    fn a_reset_makes_the_next_submission_a_new_recording() {
        let tel = tel_pass(1);
        let l = 0x30usize as *mut c_void;
        plock(&tel.in_flight).insert(l as usize, (0, 1));
        note_draws(&tel, l as usize, 1);
        plock(&tel.pass.state).boundary(l as usize, 1, 0).unwrap();
        assert!(plock(&tel.pass.state).recs.contains_key(&(l as usize)));
        tel.list_reset(l, true);
        assert!(!plock(&tel.in_flight).contains_key(&(l as usize)), "a Reset list is not in flight");
        assert!(plock(&tel.pass.state).recs.is_empty(), "its unsubmitted recording is dropped");
        // another list is not touched
        plock(&tel.in_flight).insert(0x40, (0, 2));
        tel.list_reset(l, true);
        assert!(plock(&tel.in_flight).contains_key(&0x40));
        // and its counters are gone
        let left = tel.pass.counts.get(l as usize).map(|c| c.drain()).unwrap_or_default();
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn the_shader_source_with_the_most_work_wins() {
        assert_eq!(merge_source(None, "cache"), Some("cache"));
        assert_eq!(merge_source(Some("cache"), "dxc"), Some("dxc"));
        assert_eq!(merge_source(Some("dxc"), "cache"), Some("dxc"));
        assert_eq!(merge_source(Some("direct"), "cache"), Some("direct"));
        assert_eq!(merge_source(Some("cache"), "direct"), Some("direct"));
        assert_eq!(merge_source(Some("direct"), "dxc"), Some("dxc"));
        assert_eq!(merge_source(Some("dxc"), "direct"), Some("dxc"));
        // across the stages of one build, in any order
        let stages = ["cache", "direct", "cache", "dxc", "direct"];
        assert_eq!(stages.iter().fold(None, |c, s| merge_source(c, s)), Some("dxc"));
        assert_eq!(["cache", "cache"].iter().fold(None, |c, s| merge_source(c, s)), Some("cache"));
    }

    #[test]
    fn the_pipeline_pointer_map_is_bounded() {
        let seq = AtomicU64::new(10);
        let mut m = PipeMap::new(2);
        assert_eq!(m.id_of(0x1000, &seq), Some(10));
        assert_eq!(m.id_of(0x2000, &seq), Some(11));
        assert_eq!(m.id_of(0x1000, &seq), Some(10), "the same pointer, the same id");
        assert_eq!(m.id_of(0x3000, &seq), None, "full");
        assert_eq!(seq.load(Ordering::Relaxed), 12, "a refused pointer takes no id");
        assert_eq!(m.ids.len(), 2);
    }

    #[test]
    fn io_requests_are_aggregated_and_flushed_per_frame_or_every_250_ms() {
        let agg = IoAgg::new();
        let mut st = IoFlush { frame: 5, since: Duration::ZERO };
        let mut evs: Vec<Ev> = Vec::new();
        let mut sink = |ev: &str, f: Vec<(&'static str, Value)>| evs.push((ev.to_string(), f));
        // nothing completed: no event at a frame boundary
        assert!(!io_step(&agg, &mut st, 6, Duration::from_millis(16), &mut sink));
        agg.add(1000, false, 2_000_000);
        agg.add(3000, true, 6_000_000);
        agg.add(500, true, 1_000_000);
        // inside the frame, before 250 ms: nothing
        assert!(!io_step(&agg, &mut st, 6, Duration::from_millis(100), &mut sink));
        assert!(!io_step(&agg, &mut st, 6, Duration::from_millis(100), &mut sink));
        // 250 ms since the last flush: a batch of the load so far
        assert!(io_step(&agg, &mut st, 6, Duration::from_millis(60), &mut sink));
        agg.add(10, false, 4_000_000);
        // a frame boundary flushes what completed, labelled with the frame that ended
        assert!(io_step(&agg, &mut st, 7, Duration::from_millis(1), &mut sink));
        assert!(!io_step(&agg, &mut st, 8, Duration::from_millis(1), &mut sink), "nothing completed in frame 7");
        drop(sink);
        assert_eq!(evs.len(), 2);
        let (n0, f0) = (&evs[0].0, &evs[0].1);
        assert_eq!(n0, "io");
        assert_eq!(get(f0, "requests"), Some(&Value::from(3)));
        assert_eq!(get(f0, "bytes"), Some(&Value::from(4500)));
        assert_eq!(get(f0, "zlib"), Some(&Value::from(2)));
        assert_eq!(get(f0, "ms").and_then(Value::as_f64), Some(9.0));
        assert_eq!(get(f0, "max_ms").and_then(Value::as_f64), Some(6.0));
        assert_eq!(get(f0, "frame"), Some(&Value::from(6)));
        assert_eq!(get(&evs[1].1, "requests"), Some(&Value::from(1)));
        assert_eq!(get(&evs[1].1, "frame"), Some(&Value::from(6)));
        assert!(!agg.pending());
    }

    // ---- part 3: GPU ----

    fn rig_pass(ty: D3D12_COMMAND_LIST_TYPE) -> Option<Rig> {
        let mut r = rig(ty, LIST_SLOTS, None)?;
        r.tel = r.tel.with_pass(true);
        Some(r)
    }

    /// A compute kernel with the console dispatch ABI (u2 = memory) that does `iters` dependent steps.
    fn spin_hlsl(iters: u32) -> String {
        format!(
            "RWByteAddressBuffer mem : register(u2);\n[numthreads(64, 1, 1)]\nvoid main(uint3 t : SV_DispatchThreadID) {{\n  uint a = t.x;\n  [loop] for (uint i = 0; i < {iters}u; i++) {{ a = a * 1664525u + 1013904223u; a ^= a >> 7; }}\n  mem.Store(t.x * 4, a);\n}}\n"
        )
    }

    fn trans(r: &ID3D12Resource, before: D3D12_RESOURCE_STATES, after: D3D12_RESOURCE_STATES) -> D3D12_RESOURCE_BARRIER {
        D3D12_RESOURCE_BARRIER {
            Type: D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
            Flags: D3D12_RESOURCE_BARRIER_FLAG_NONE,
            Anonymous: D3D12_RESOURCE_BARRIER_0 {
                Transition: std::mem::ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {
                    pResource: unsafe { std::mem::transmute_copy(r) },
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
            Anonymous: D3D12_RESOURCE_BARRIER_0 { UAV: std::mem::ManuallyDrop::new(D3D12_RESOURCE_UAV_BARRIER { pResource: std::mem::ManuallyDrop::new(None) }) },
        }
    }

    /// A closed compute list: `reps` dispatches of `groups` groups, each after a UAV barrier, and
    /// with `passes` a pass boundary before each barrier (the call a title's barrier site makes).
    struct Work {
        list: ID3D12GraphicsCommandList,
        _alloc: ID3D12CommandAllocator,
        _buf: ID3D12Resource,
        _pso: ID3D12PipelineState,
        _rs: ID3D12RootSignature,
        _heaps: [ID3D12DescriptorHeap; 2],
    }

    fn compute_work(r: &Rig, ty: D3D12_COMMAND_LIST_TYPE, dxil: &[u8], groups: u32, reps: usize, passes: bool) -> Option<Work> {
        let dev = &r.dev;
        // SAFETY: plain D3D12 calls on the test's own device, as the test runner records a dispatch.
        unsafe {
            let rs = crate::gpu::recomp::pso::abi_root_signature(dev, false).ok()?;
            let pso: ID3D12PipelineState = dev
                .CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {
                    pRootSignature: std::mem::transmute_copy(&rs),
                    CS: D3D12_SHADER_BYTECODE { pShaderBytecode: dxil.as_ptr().cast(), BytecodeLength: dxil.len() },
                    ..Default::default()
                })
                .ok()?;
            let mut buf: Option<ID3D12Resource> = None;
            dev.CreateCommittedResource(
                &D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_DEFAULT, ..Default::default() },
                D3D12_HEAP_FLAG_NONE,
                &D3D12_RESOURCE_DESC {
                    Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
                    Width: 1 << 20,
                    Height: 1,
                    DepthOrArraySize: 1,
                    MipLevels: 1,
                    SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                    Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                    Flags: D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
                    ..Default::default()
                },
                D3D12_RESOURCE_STATE_COMMON,
                None,
                &mut buf,
            )
            .ok()?;
            let buf = buf?;
            let heap = |ty: D3D12_DESCRIPTOR_HEAP_TYPE| -> Option<ID3D12DescriptorHeap> {
                dev.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC { Type: ty, NumDescriptors: 1, Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE, NodeMask: 0 }).ok()
            };
            let heaps = [heap(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV)?, heap(D3D12_DESCRIPTOR_HEAP_TYPE_SAMPLER)?];
            let alloc: ID3D12CommandAllocator = dev.CreateCommandAllocator(ty).ok()?;
            let list: ID3D12GraphicsCommandList = dev.CreateCommandList(0, ty, &alloc, &pso).ok()?;
            list.SetDescriptorHeaps(&[Some(heaps[0].clone()), Some(heaps[1].clone())]);
            list.SetComputeRootSignature(&rs);
            let va = buf.GetGPUVirtualAddress();
            for i in 0..6 {
                list.SetComputeRootUnorderedAccessView(i, va);
            }
            list.ResourceBarrier(&[trans(&buf, D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_UNORDERED_ACCESS)]);
            for _ in 0..reps {
                if passes {
                    r.tel.pass_boundary(list.as_raw(), 1);
                }
                list.ResourceBarrier(&[uav_all()]);
                list.Dispatch(groups, 1, 1);
            }
            list.Close().ok()?;
            Some(Work { list, _alloc: alloc, _buf: buf, _pso: pso, _rs: rs, _heaps: heaps })
        }
    }

    fn submit_to(tel: &GpuTel, qraw: *mut c_void, qf: &QueueFns, raw: &[*mut c_void], evs: &mut Vec<Ev>) {
        let mut sink = |ev: &str, f: Vec<(&'static str, Value)>| evs.push((ev.to_string(), f));
        tel.execute(qraw, raw, "submit", |c| qf.execute(c), &mut sink);
    }

    /// The `seq` of the `list` event of PC list `list` in `frame`, nth submission first.
    fn seqs_of(evs: &[Ev], list: *mut c_void, frame: u64) -> Vec<u64> {
        lists_of(evs)
            .iter()
            .filter(|f| get(f, "id").and_then(Value::as_u64) == Some(list as u64) && get(f, "frame").and_then(Value::as_u64) == Some(frame))
            .filter_map(|f| get(f, "seq").and_then(Value::as_u64))
            .collect()
    }

    /// The `pass` events of submission `seq`, in pass order.
    fn passes_of(evs: &[Ev], seq: u64) -> Vec<&Vec<(&'static str, Value)>> {
        let mut v: Vec<_> = evs.iter().filter(|(n, f)| n == "pass" && get(f, "list").and_then(Value::as_u64) == Some(seq)).map(|(_, f)| f).collect();
        v.sort_by_key(|f| get(f, "i").and_then(Value::as_u64));
        v
    }

    fn span_ms(f: &[(&'static str, Value)]) -> Option<(f64, f64)> {
        Some((get(f, "start_ms")?.as_f64()?, get(f, "end_ms")?.as_f64()?))
    }

    fn median(mut v: Vec<f64>) -> f64 {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    }

    #[test]
    fn the_reset_makes_a_resubmission_a_new_recording_and_only_a_second_submit_is_resubmitted() {
        let direct = D3D12_COMMAND_LIST_TYPE_DIRECT;
        let Some(r) = rig(direct, LIST_SLOTS, None) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let (Some(l0), Some(tail)) = (test_list(&r.dev, direct, false), test_list(&r.dev, direct, false)) else {
            return crate::gpu::recomp::runner::skip_test("no GPU");
        };
        let mut evs = Vec::new();
        r.tel.set_frame(1);
        submit(&r, &[&l0], &mut evs);
        r.tel.list_reset(l0.list.as_raw(), false); // the title Resets the list and records it again
        submit(&r, &[&l0], &mut evs);
        submit(&r, &[&l0], &mut evs); // the same recording again
        r.tel.set_frame(2);
        submit(&r, &[&tail], &mut evs);
        settle(&r);
        r.tel.set_frame(3);
        harvest(&r, &mut evs);
        let ls = lists_of(&evs);
        let mine: Vec<_> = ls.iter().filter(|f| get(f, "id").and_then(Value::as_u64) == Some(l0.list.as_raw() as u64)).collect();
        assert_eq!(mine.len(), 3, "{evs:?}");
        let flags: Vec<_> = mine.iter().map(|f| get(f, "resubmitted").cloned()).collect();
        assert_eq!(flags, vec![Some(Value::Bool(false)), Some(Value::Bool(false)), Some(Value::Bool(true))]);
    }

    #[test]
    fn a_resubmitted_list_has_null_pass_ticks_and_valid_list_ticks() {
        let direct = D3D12_COMMAND_LIST_TYPE_DIRECT;
        let Some(r) = rig_pass(direct) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let Ok(dxil) = crate::gpu::recomp::dxc::compile(&spin_hlsl(1000), "main", "cs_6_6", &[]) else { return crate::gpu::recomp::runner::skip_test("no DXC") };
        let (Some(w), Some(tail)) = (compute_work(&r, direct, &dxil, 8, 2, true), test_list(&r.dev, direct, false)) else {
            return crate::gpu::recomp::runner::skip_test("no GPU");
        };
        let mut evs = Vec::new();
        let raw = w.list.as_raw();
        r.tel.set_frame(1);
        submit_to(&r.tel, r.qraw, &r.qf, &[raw], &mut evs);
        submit_to(&r.tel, r.qraw, &r.qf, &[raw], &mut evs); // before a Reset: the same recording
        r.tel.set_frame(2);
        submit(&r, &[&tail], &mut evs);
        settle(&r);
        r.tel.set_frame(3);
        harvest(&r, &mut evs);
        let seqs = seqs_of(&evs, raw, 1);
        assert_eq!(seqs.len(), 2, "{evs:?}");
        let ls = lists_of(&evs);
        for s in &seqs {
            let l = ls.iter().find(|f| get(f, "seq").and_then(Value::as_u64) == Some(*s)).expect("list event");
            assert!(span_ms(l).is_some(), "list ticks stay valid: {l:?}");
            let ps = passes_of(&evs, *s);
            assert!(!ps.is_empty());
            assert!(ps.iter().all(|p| get(p, "why") == Some(&Value::from("resubmitted")) && matches!(get(p, "start_tick"), Some(Value::Null)) && get(p, "resubmitted") == Some(&Value::Bool(true))), "{ps:?}");
        }
        assert_eq!(passes_of(&evs, seqs[0]).len(), 3, "two boundaries: three passes");
    }

    #[test]
    fn one_dispatch_timed_through_the_pass_path_matches_dispatch_timed() {
        let _t = GPU_TIMING.lock().unwrap_or_else(|e| e.into_inner());
        let direct = D3D12_COMMAND_LIST_TYPE_DIRECT;
        let Some(r) = rig_pass(direct) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let Ok(dxil) = crate::gpu::recomp::dxc::compile(&spin_hlsl(100_000), "main", "cs_6_6", &[]) else { return crate::gpu::recomp::runner::skip_test("no DXC") };
        let (groups, reps) = (256u32, 21usize);
        let mem = vec![0u32; groups as usize * 64];
        let base = match crate::gpu::recomp::runner::dispatch_timed(&dxil, [groups, 1, 1], &[], &mem, reps) {
            Ok(t) => t,
            Err(e) => {
                println!("dispatch_timed: {e}");
                return crate::gpu::recomp::runner::skip_test("no GPU");
            }
        };
        let (Some(w), Some(tail)) = (compute_work(&r, direct, &dxil, groups, reps, true), test_list(&r.dev, direct, false)) else {
            return crate::gpu::recomp::runner::skip_test("no GPU");
        };
        let mut evs = Vec::new();
        let raw = w.list.as_raw();
        {
            // the GPU is ours while the pass path runs (`dispatch_timed` above took the same lock)
            let _g = crate::gpu::recomp::runner::hold_gpu();
            r.tel.set_frame(1);
            submit_to(&r.tel, r.qraw, &r.qf, &[raw], &mut evs);
            r.tel.set_frame(2);
            submit(&r, &[&tail], &mut evs);
            settle(&r);
        }
        r.tel.set_frame(3);
        harvest(&r, &mut evs);
        let seqs = seqs_of(&evs, raw, 1);
        assert_eq!(seqs.len(), 1, "{evs:?}");
        let ps = passes_of(&evs, seqs[0]);
        assert_eq!(ps.len(), reps + 1, "one pass before the first barrier and one per dispatch");
        // pass 0 is the start of the list; pass k >= 1 is a barrier and a dispatch. The first of each is cold.
        let through: Vec<f64> = ps[2..].iter().map(|p| span_ms(p).map(|(s, e)| e - s).expect("pass ticks")).collect();
        let timed: Vec<f64> = base[1..].to_vec();
        assert_eq!((through.len(), timed.len()), (20, 20));
        let spans = || ps.iter().map(|p| span_ms(p).map(|(s, e)| e - s)).collect::<Vec<_>>();
        // the passes tile the list: their ticks add up to the list's, exactly
        let tick = |f: &[(&'static str, Value)], k: &str| get(f, k).and_then(Value::as_u64);
        let mut sum = 0u64;
        for p in &ps {
            let (Some(s0), Some(e0)) = (tick(p, "start_tick"), tick(p, "end_tick")) else { panic!("a pass has no ticks: {p:?}") };
            sum += e0 - s0;
        }
        let ls = lists_of(&evs);
        let l = ls.iter().find(|f| get(f, "seq").and_then(Value::as_u64) == Some(seqs[0])).expect("list event");
        let (Some(l0), Some(l1)) = (tick(l, "start_tick"), tick(l, "end_tick")) else { panic!("the list has no ticks: {l:?}") };
        assert_eq!(sum, l1 - l0, "the passes do not tile the list; pass spans (ms): {:?}", spans());
        // contention only lengthens a span: no pass is shorter than the kernel
        let (min_through, min_timed) = (through.iter().cloned().fold(f64::MAX, f64::min), timed.iter().cloned().fold(f64::MAX, f64::min));
        assert!(min_through >= 0.85 * min_timed, "a pass ({min_through:.4} ms) is shorter than the kernel ({min_timed:.4} ms); pass spans (ms): {:?}", spans());
        let (a, b) = (median(through), median(timed));
        println!("pass path median {a:.4} ms, dispatch_timed median {b:.4} ms");
        assert!((a - b).abs() <= (0.15 * b).max(0.05), "pass path {a:.4} ms vs dispatch_timed {b:.4} ms; pass spans (ms): {:?}", spans());
    }

    #[test]
    fn passes_of_lists_on_two_queues_overlap_and_the_frame_takes_the_union() {
        let _t = GPU_TIMING.lock().unwrap_or_else(|e| e.into_inner());
        let direct = D3D12_COMMAND_LIST_TYPE_DIRECT;
        let compute = D3D12_COMMAND_LIST_TYPE_COMPUTE;
        let Some(r) = rig_pass(direct) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        // SAFETY: a second queue on the test's own device.
        let Ok(q2): Result<ID3D12CommandQueue, _> = (unsafe { r.dev.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC { Type: compute, ..Default::default() }) }) else {
            return crate::gpu::recomp::runner::skip_test("no GPU");
        };
        let Some(qf2) = QueueFns::plain(&q2) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let Ok(dxil) = crate::gpu::recomp::dxc::compile(&spin_hlsl(1_500_000), "main", "cs_6_6", &[]) else { return crate::gpu::recomp::runner::skip_test("no DXC") };
        let (Some(wd), Some(wc), Some(t1), Some(t2)) = (
            compute_work(&r, direct, &dxil, 256, 3, true),
            compute_work(&r, compute, &dxil, 256, 3, true),
            test_list(&r.dev, direct, false),
            test_list(&r.dev, compute, false),
        ) else {
            return crate::gpu::recomp::runner::skip_test("no GPU");
        };
        let (rd, rc) = (wd.list.as_raw(), wc.list.as_raw());
        let q2raw = q2.as_raw();
        let mut evs = Vec::new();
        r.tel.set_frame(1);
        submit_to(&r.tel, r.qraw, &r.qf, &[rd], &mut evs);
        submit_to(&r.tel, q2raw, &qf2, &[rc], &mut evs);
        r.tel.set_frame(2);
        submit_to(&r.tel, r.qraw, &r.qf, &[t1.list.as_raw()], &mut evs);
        submit_to(&r.tel, q2raw, &qf2, &[t2.list.as_raw()], &mut evs);
        settle_key(&r.tel, r.qraw);
        settle_key(&r.tel, q2raw);
        r.tel.set_frame(3);
        harvest(&r, &mut evs);
        let ls = lists_of(&evs);
        let seq_d = seqs_of(&evs, rd, 1);
        let seq_c = seqs_of(&evs, rc, 1);
        assert_eq!((seq_d.len(), seq_c.len()), (1, 1), "{evs:?}");
        let list_span = |s: u64| ls.iter().find(|f| get(f, "seq").and_then(Value::as_u64) == Some(s)).and_then(|f| span_ms(f));
        let (Some(a), Some(b)) = (list_span(seq_d[0]), list_span(seq_c[0])) else { panic!("a list has no ticks: {evs:?}") };
        for s in [seq_d[0], seq_c[0]] {
            let ps = passes_of(&evs, s);
            assert_eq!(ps.len(), 4);
            for (i, p) in ps.iter().enumerate() {
                let (st, en) = span_ms(p).unwrap_or_else(|| panic!("pass {i} of {s} has no ticks: {p:?}"));
                assert!(if i == 0 { st <= en } else { st < en }, "pass {i} of {s}: {st} .. {en}");
            }
        }
        println!("direct {a:?} ms, compute {b:?} ms");
        let mut iv = [a, b];
        let union = xlog::union_ms(&mut iv).expect("union");
        let sum = (a.1 - a.0) + (b.1 - b.0);
        let overlap = (a.1.min(b.1) - a.0.max(b.0)).max(0.0);
        println!("overlap {overlap:.4} ms ({})", if overlap > 0.0 { "the queues ran together" } else { "this GPU serialised them" });
        assert!((union - (sum - overlap)).abs() <= 1e-6, "union {union} ms != sum {sum} ms - overlap {overlap} ms");
        assert!(union >= (a.1 - a.0).max(b.1 - b.0) - 1e-6 && union <= sum + 1e-6);
    }

    #[test]
    fn engine_stamps_around_a_fixed_workload_cost_what() {
        let _t = GPU_TIMING.lock().unwrap_or_else(|e| e.into_inner());
        let direct = D3D12_COMMAND_LIST_TYPE_DIRECT;
        let Some(r) = rig(direct, LIST_SLOTS, None) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let Ok(dxil) = crate::gpu::recomp::dxc::compile(&spin_hlsl(20_000), "main", "cs_6_6", &[]) else { return crate::gpu::recomp::runner::skip_test("no DXC") };
        let Some(freq) = r.qf.frequency().filter(|f| *f >= 1000) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let Some(dfns) = DevFns::plain(&r.dev) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
        let works: Option<Vec<Work>> = (0..24).map(|_| compute_work(&r, direct, &dxil, 64, 2, false)).collect();
        // the test's own outer stamp pair: EndQuery 0 in the first list, EndQuery 1 and the resolve in the last
        let (Some(works), Some(heap), Some(readback), Some(fence)) = (works, dfns.query_heap(D3D12_QUERY_HEAP_TYPE_TIMESTAMP, 2), dfns.readback(16), dfns.fence()) else {
            return crate::gpu::recomp::runner::skip_test("no GPU");
        };
        let mut mapped: *mut c_void = std::ptr::null_mut();
        // SAFETY: plain D3D12 calls on the test's own device.
        let (a, b) = unsafe {
            if readback.Map(0, None, Some(&mut mapped)).is_err() || mapped.is_null() {
                return crate::gpu::recomp::runner::skip_test("no GPU");
            }
            let mk = |close_with_resolve: bool| -> Option<(ID3D12GraphicsCommandList, ID3D12CommandAllocator)> {
                let alloc: ID3D12CommandAllocator = r.dev.CreateCommandAllocator(direct).ok()?;
                let list: ID3D12GraphicsCommandList = r.dev.CreateCommandList(0, direct, &alloc, None::<&ID3D12PipelineState>).ok()?;
                list.EndQuery(&heap, D3D12_QUERY_TYPE_TIMESTAMP, close_with_resolve as u32);
                if close_with_resolve {
                    list.ResolveQueryData(&heap, D3D12_QUERY_TYPE_TIMESTAMP, 0, 2, &readback, 0);
                }
                list.Close().ok()?;
                Some((list, alloc))
            };
            let (Some(a), Some(b)) = (mk(false), mk(true)) else { return crate::gpu::recomp::runner::skip_test("no GPU") };
            (a, b)
        };
        let mut raw: Vec<*mut c_void> = vec![a.0.as_raw()];
        raw.extend(works.iter().map(|w| w.list.as_raw()));
        raw.push(b.0.as_raw());
        let mut value = 0u64;
        let ticks = mapped as *const u64;
        // the outer duration in ms of one run; None when the fence never passed or the stamps are bad
        let mut run = |engine: Option<(&mut u64, &mut Vec<Ev>)>| -> Option<f64> {
            match engine {
                None => r.qf.execute(&raw),
                Some((frame, evs)) => {
                    *frame += 1;
                    r.tel.set_frame(*frame);
                    submit_to(&r.tel, r.qraw, &r.qf, &raw, evs);
                }
            }
            value += 1;
            r.qf.signal(&fence, value);
            let end = Instant::now() + Duration::from_secs(10);
            // SAFETY: CPU-side read of the test's fence.
            while unsafe { fence.GetCompletedValue() } < value {
                if Instant::now() > end {
                    return None;
                }
                std::thread::sleep(Duration::from_micros(200));
            }
            // SAFETY: the resolve ran before the fence value passed.
            let (t0, t1) = unsafe { (*ticks, *ticks.add(1)) };
            (t0 != 0 && t1 >= t0).then(|| (t1 - t0) as f64 * 1000.0 / freq as f64)
        };
        let (mut plain, mut stamped) = (Vec::new(), Vec::new());
        let (mut frame, mut evs) = (0u64, Vec::new());
        for _ in 0..8 {
            plain.push(run(None));
            stamped.push(run(Some((&mut frame, &mut evs))));
            harvest(&r, &mut evs);
            evs.clear();
        }
        assert!(plain.iter().chain(stamped.iter()).all(Option::is_some), "a run did not complete: {plain:?} {stamped:?}");
        let (p, s) = (median(plain.into_iter().flatten().collect()), median(stamped.into_iter().flatten().collect()));
        println!("serialisation check, 24 lists: without the engine's stamps {p:.4} ms, with {s:.4} ms, difference {:+.4} ms ({:+.1} %)", s - p, (s - p) / p * 100.0);
    }
}
