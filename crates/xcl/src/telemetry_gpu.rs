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
use std::collections::{HashMap, VecDeque};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, RwLock, TryLockError};
use std::time::{Duration, Instant};

use serde_json::Value;
use windows::core::{Interface, GUID};
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;

use crate::hle::d3d12x::{frame as dframe, layout, real};
use crate::log::{self as xlog, ListFacts, Tier};

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
fn clock_event(c: &QueueClock, qpf: i64) -> Vec<(&'static str, Value)> {
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

    /// Harvested list intervals since the last call (the frame glue drains it on the thread).
    pub(super) fn take_harvested(&self) -> Vec<HarvestedList> {
        plock(&self.harvested).drain(..).collect()
    }

    fn find_queue(&self, key: usize) -> Option<Arc<Mutex<QueueClock>>> {
        let g = self.queues.read().unwrap_or_else(|e| e.into_inner());
        g.iter().find(|(k, _)| *k == key).map(|(_, q)| q.clone())
    }

    fn all_queues(&self) -> Vec<Arc<Mutex<QueueClock>>> {
        self.queues.read().unwrap_or_else(|e| e.into_inner()).iter().map(|(_, q)| q.clone()).collect()
    }

    /// The clock of `key`, made (and its first `clock` event written) on first use.
    fn queue_clock(&self, key: *mut c_void, sink: Sink) -> Option<Arc<Mutex<QueueClock>>> {
        let k = key as usize;
        if let Some(q) = self.find_queue(k) {
            return Some(q);
        }
        let dev = self.dev()?;
        let qf = (self.queue_fns)(key)?;
        let (q, ev) = {
            let mut w = self.queues.write().unwrap_or_else(|e| e.into_inner());
            if let Some((_, q)) = w.iter().find(|(kk, _)| *kk == k) {
                return Some(q.clone());
            }
            if w.len() >= MAX_QUEUES {
                return None;
            }
            let id = self.next_queue.fetch_add(1, Ordering::Relaxed);
            let c = QueueClock::new(&dev, qf, id, self.slots, self.copy_ts);
            let ev = clock_event(&c, self.qpf);
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
        if n == 0 || self.stopped.load(Ordering::Relaxed) || self.removed.load(Ordering::Relaxed) {
            submit(lists);
            return;
        }
        let frame = self.frame.load(Ordering::Acquire);
        let sampled = xlog::sample_due(frame, self.sample);
        let Some(qc) = self.queue_clock(queue, sink) else {
            submit(lists);
            return;
        };
        let Some(mut guard) = lock_wait(&qc, Duration::from_secs(1)) else {
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
            c.q.signal(f, value);
            c.sets[i].state = SetState::Resolving(value);
        }

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
                clock_event(&c, self.qpf)
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
    let Some(tel) = GPU.get() else { return };
    if let Err(e) = std::thread::Builder::new().name("xcl-telemetry".into()).spawn(move || thread_main(tel)) {
        crate::xlog_warn!("telemetry: cannot start the xcl-telemetry thread: {e}");
    }
}

/// The last harvest and the null flush (called from `telemetry_final`).
pub(super) fn telemetry_gpu_final() {
    if let Some(t) = GPU.get() {
        t.finish(&mut global_sink);
    }
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
            break;
        }
        tel.harvest(&mut global_sink);
        let f = tel.frame();
        if f >= next_clock {
            tel.refresh_clocks(&mut global_sink);
            next_clock = (f / 60 + 1) * 60;
        }
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
}
