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

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, RwLock, TryLockError};
use std::time::{Duration, Instant};

use serde_json::Value;
use windows::core::{Interface, GUID};
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use crate::hle::d3d12x::{frame as dframe, layout, real};
use crate::log::{self as xlog, FrameFacts, ListFacts, MemoryFacts, Tier};

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
    fn record_resolve(&mut self) -> Option<*mut c_void> {
        // SAFETY: nothing of this set is in flight (its last resolve passed the fence before it was freed).
        unsafe {
            self.resolve_alloc.Reset().ok()?;
            self.resolve_list.Reset(&self.resolve_alloc, None::<&ID3D12PipelineState>).ok()?;
            self.resolve_list.ResolveQueryData(&self.heap, D3D12_QUERY_TYPE_TIMESTAMP, 0, self.used, &self.readback, 0);
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
    fn new_frame(&mut self, frame: u64, open_next: bool) -> Option<(usize, *mut c_void)> {
        let prev = self.cur.take();
        self.cur_frame = frame;
        let mut resolve = None;
        if let Some(i) = prev {
            let s = &mut self.sets[i];
            if s.state == SetState::Open {
                if s.used == 0 {
                    s.state = SetState::Free;
                    s.entries.clear();
                } else if let Some(raw) = s.record_resolve() {
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
        }
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
        let (q, ev) = {
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
            let q = Arc::new(Mutex::new(c));
            w.push((k, q.clone()));
            (q, ev)
        };
        sink("clock", ev);
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
        let resolve = if c.cur_frame != frame { c.new_frame(frame, sampled && c.why_off.is_none()) } else { None };

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
            return;
        }
        let mut inflight = plock(&self.in_flight);
        for (k, l) in lists.iter().enumerate() {
            let (list, seq) = (*l as usize, seq0 + k as u64);
            let resubmitted = inflight.contains_key(&list);
            if resubmitted {
                self.note(DROP_RESUBMITTED, 1);
            }
            let (slot, why) = match plan {
                Plan::Stamp(_, base) => (Some(base + k as u32), None),
                Plan::Null(w) => (None, Some(w)),
                Plan::Skip => (None, None),
            };
            if slot.is_some() && (resubmitted || inflight.len() < IN_FLIGHT_MAX) {
                inflight.insert(list, (c.id, seq));
            }
            let e = ListEntry { frame, seq, list, chunk, slot, cpu_us, resubmitted, origin, why };
            match plan {
                Plan::Stamp(i, _) => c.sets[i].entries.push(e),
                _ if c.nulls.len() < NULLS_MAX => c.nulls.push(e),
                _ => self.note(DROP_FULL, 1),
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
    fn emit(&self, d: Done, sink: Sink) {
        for e in &d.entries {
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
    if GPU.set(tel).is_err() {
        return;
    }
    super::update_manifest(|m| m.sample = sample);
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
    let names = ["full", "late", "unsupported", "resubmitted", "removed", "sampled_out"];
    let mut drops: BTreeMap<String, u64> = names.iter().zip(t.drops()).map(|(k, v)| (k.to_string(), v)).collect();
    drops.insert("frames".into(), FRAMES_DROPPED.load(Ordering::Relaxed));
    let sample = t.sample;
    super::update_manifest(|m| {
        m.sample = sample;
        m.gpu_drops = drops;
    });
}

/// Wakes on every frame (or every 50 ms) and harvests; every 60 frames it calibrates again.
fn thread_main(tel: &'static GpuTel) {
    let mut next_clock = 60u64;
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
        if f >= next_clock {
            tel.refresh_clocks(&mut global_sink);
            global_sink("memory", xlog::memory_fields(&memory_snapshot(f)));
            next_clock = (f / 60 + 1) * 60;
        }
    }
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
#[derive(Clone, Copy, Default)]
struct Counters {
    draws: u64,
    dispatches: u64,
    pso_us: u64,
    look_us: u64,
    wait_ns: u64,
    wait_calls: u64,
}

impl Counters {
    /// bind::activity (draws, dispatches), pso_async::blocked (total us, graphics + compute),
    /// sparse::look_counters()[0] (whole looks, us), and the wait counters above.
    fn now() -> Counters {
        let (draws, dispatches) = crate::hle::d3d12x::bind::activity();
        let b = crate::hle::d3d12x::pso_async::blocked();
        Counters {
            draws,
            dispatches,
            pso_us: b[0][1] + b[1][1],
            look_us: crate::hle::d3d12x::sparse::look_counters()[0],
            wait_ns: WAIT_NS.load(Ordering::Relaxed),
            wait_calls: WAIT_CALLS.load(Ordering::Relaxed),
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
    pso_wait_ms: f64,
    look_ms: f64,
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
            prev: Counters { draws: 0, dispatches: 0, pso_us: 0, look_us: 0, wait_ns: 0, wait_calls: 0 },
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
            pso_wait_ms: d(n.pso_us, p.pso_us) as f64 / 1000.0,
            look_ms: d(n.look_us, p.look_us) as f64 / 1000.0,
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
        barriers: None,
        pso_wait_ms: r.pso_wait_ms,
        look_ms: r.look_ms,
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
        let q = r.tel.find_queue(r.qraw as usize).expect("queue clock");
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
}
