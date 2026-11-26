//! Non-blocking pipeline creation. Translating a console shader takes milliseconds; compiling
//! the HLSL with DXC takes 0.2 s to seconds, and a title creates thousands of pipelines while
//! it loads. When a pipeline's DXIL is not in the disk cache yet, the title gets a stand-in
//! pipeline (shaders that draw nothing) at once and the real one is built on a worker; binding
//! (`SetPipelineState`, console list replay, the two-pass lookups) uses the real one as soon
//! as it is there. A draw and a compute bind wait for their pipeline at most their frame budget
//! (`frame_wait_budget`, `compute_wait`; all threads together), each pipeline once, and are then
//! left out until it is built; `XCL_PSO_COMPUTE_WAIT_MS` restores waiting for compute, a `Dispatch`
//! at its submit (a skipped dispatch would corrupt what it computes).
//! `XCL_ASYNC_PSO=0` builds every pipeline in the call, as before.
//! Workers take wanted builds before unwanted ones and graphics before compute; a few workers
//! build only graphics, so a queue of slow compute programs never holds up the scene's draws.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex, OnceLock};

struct Pending {
    /// the build, until a worker – or a bind that cannot wait for the queue – takes it
    job: Mutex<Option<Job>>,
    /// the build went to the workers (a lazy one goes on its first `resolve`)
    sent: std::sync::atomic::AtomicBool,
    /// the real pipeline state (raw pointer), 0 while building or when building failed
    real: AtomicUsize,
    /// the driver pipeline binds get till then (`shared`); 0: the stand-in is one itself
    driver: AtomicUsize,
    done: Mutex<bool>,
    cv: Condvar,
    compute: bool,
    /// of its shaders (`used`); 0 = none
    key: u64,
    /// it was used (`used::note` is told once)
    used: std::sync::atomic::AtomicBool,
    /// the stand-in was released before the real pipeline was built
    orphan: std::sync::atomic::AtomicBool,
    /// a bind waited for it longer than `wait_budget`: binds go on without it until it is built
    late: std::sync::atomic::AtomicBool,
    /// its dispatches may be left out while it is built (`image_only`): binds do not wait for it
    spared: std::sync::atomic::AtomicBool,
    demand: Mutex<Demand>,
}

/// Whether one of the title's threads waits for the pipeline, and the preload worker building it.
#[derive(Default)]
struct Demand {
    wanted: bool,
    /// a call put off to the submit needs it (`need_by_submit`)
    by_submit: bool,
    /// thread id of the preload worker (lowest priority) building it, 0 = none
    preload_thread: u32,
    /// when a title thread first needed it (`need_now`): its one chance to be waited for runs from here
    needed_at: Option<std::time::Instant>,
}

thread_local! {
    /// what this worker is building
    static BUILDING: std::cell::RefCell<Option<Arc<Pending>>> = const { std::cell::RefCell::new(None) };
}

/// This thread is a worker and one of the title's threads waits for what it is building: the
/// build must not stand back for the title's sake (`real::big_compile_turn`).
pub(super) fn build_is_wanted() -> bool {
    BUILDING.with(|b| b.borrow().as_ref().is_some_and(|p| p.demand.lock().unwrap().wanted))
}

/// Whether one of the title's threads waits for what this thread is doing, to be asked later and
/// from another thread: a worker's build as `build_is_wanted`; a thread that is not XCL's is the
/// title's own.
pub fn wanted_here() -> crate::gpu::recomp::dxc::Wanted {
    if let Some(p) = BUILDING.with(|b| b.borrow().clone()) {
        return Box::new(move || p.demand.lock().unwrap().wanted);
    }
    let own = crate::state::boot().is_some() && !std::thread::current().name().is_some_and(|n| n.starts_with("xcl-"));
    Box::new(move || own)
}

/// A worker's build of `p` (`preload`: by one of the lowest-priority workers).
fn build(p: &Arc<Pending>, job: Job, preload: bool) {
    use windows_sys::Win32::System::Threading::{GetCurrentThread, GetCurrentThreadId, SetThreadPriority, THREAD_PRIORITY_LOWEST, THREAD_PRIORITY_NORMAL};
    BUILDING.with(|b| *b.borrow_mut() = Some(p.clone()));
    if preload {
        let mut d = p.demand.lock().unwrap();
        d.preload_thread = unsafe { GetCurrentThreadId() };
        if d.wanted {
            unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_NORMAL) };
        }
    }
    complete(p, job());
    if preload {
        // (under the lock: `want` does not raise this thread after it)
        let mut d = p.demand.lock().unwrap();
        d.preload_thread = 0;
        if d.wanted {
            unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST) };
        }
    }
    BUILDING.with(|b| *b.borrow_mut() = None);
}

impl Pending {
    /// One of the title's threads is about to wait for it: the worker building it goes on now,
    /// at a normal priority.
    fn want(&self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{OpenThread, SetThreadPriority, THREAD_PRIORITY_NORMAL, THREAD_SET_LIMITED_INFORMATION};
        let mut d = self.demand.lock().unwrap();
        if !std::mem::replace(&mut d.wanted, true) && d.preload_thread != 0 {
            unsafe {
                let h = OpenThread(THREAD_SET_LIMITED_INFORMATION, 0, d.preload_thread);
                if !h.is_null() {
                    SetThreadPriority(h, THREAD_PRIORITY_NORMAL);
                    CloseHandle(h);
                }
            }
        }
    }

    fn note_use(&self) {
        if self.key == 0 {
            return;
        }
        // (the manifest's line goes in before `used` is set: a second bind that finds the pipeline
        // used, and what it records after, come after the line)
        let e = super::pso_manifest::Event::Use { key: self.key, compute: self.compute, ready: self.real.load(Ordering::Acquire) != 0 };
        if super::pso_manifest::note_first(&self.used, e) {
            used::note(self.key);
        }
    }
}

/// stand-in handed to the title → its real pipeline
static PENDING: Mutex<Option<HashMap<usize, Arc<Pending>>>> = Mutex::new(None);

/// The build is done (`real`: its pipeline, 0 = failed).
fn complete(p: &Pending, real: usize) {
    if real == 0 {
        FAILED.fetch_add(1, Ordering::Relaxed);
    } else {
        BUILT.fetch_add(1, Ordering::Relaxed);
    }
    let orphan = {
        let mut done = p.done.lock().unwrap();
        let orphan = p.orphan.load(Ordering::Acquire);
        if !orphan {
            p.real.store(real, Ordering::Release);
        }
        *done = true;
        p.cv.notify_all();
        orphan
    };
    // outside `done`: the Release override takes PENDING (`counts` takes PENDING, then `done`)
    if orphan {
        release(real);
    }
}
static BUILT: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicUsize = AtomicUsize::new(0);
/// binds waiting right now for a pipeline still being built (`title_held`)
static HELD: AtomicUsize = AtomicUsize::new(0);

/// The PC calls of the title's threads (`pc_call`): the ones going on, and how long the ones
/// that ended took since the last frame was shown.
struct PcCalls {
    /// (id, when it began)
    now: Vec<(u64, std::time::Instant)>,
    /// the frames shown when `spent` began to count (`None` before the first observation)
    frame: Option<u64>,
    /// when the latest frame advance was first observed; calls that started earlier are clamped here
    frame_since: Option<std::time::Instant>,
    /// the time of the PC calls that ended since then
    spent: std::time::Duration,
}

impl PcCalls {
    const fn new() -> Self {
        Self { now: Vec::new(), frame: None, frame_since: None, spent: std::time::Duration::ZERO }
    }

    /// `frame` frames have been shown: the calls before the last of them are not this wait's.
    fn at(&mut self, frame: u64, now: std::time::Instant) {
        match self.frame {
            None => self.frame = Some(frame),
            Some(prev) if frame > prev => {
                self.frame = Some(frame);
                self.frame_since = Some(now);
                self.spent = std::time::Duration::ZERO;
            }
            Some(_) => {}
        }
    }

    fn clamp(&self, since: std::time::Instant) -> std::time::Instant {
        self.frame_since.map_or(since, |f| since.max(f))
    }

    fn begin(&mut self, id: u64, frame: u64, now: std::time::Instant) {
        self.at(frame, now);
        self.now.push((id, now));
    }

    fn end(&mut self, id: u64, frame: u64, now: std::time::Instant) {
        self.at(frame, now);
        if let Some(i) = self.now.iter().position(|c| c.0 == id) {
            let (_, since) = self.now.swap_remove(i);
            self.spent += now.saturating_duration_since(self.clamp(since));
        }
    }

    /// Whether a call going on holds the title: the PC calls since the last frame, it included,
    /// have taken `PC_CALL_HOLDS_AFTER`.
    fn holds(&mut self, frame: u64, now: std::time::Instant) -> bool {
        self.at(frame, now);
        let spent = self.spent;
        self.now.iter().any(|(_, since)| spent + now.saturating_duration_since(self.clamp(*since)) >= PC_CALL_HOLDS_AFTER)
    }
}

static PC_CALLS: Mutex<PcCalls> = Mutex::new(PcCalls::new());
/// From this much time in PC calls since the last frame on, a PC call holds the title: the
/// console's calls return at once.
const PC_CALL_HOLDS_AFTER: std::time::Duration = std::time::Duration::from_millis(250);

/// Whether one of the title's threads is held where a console title never waits: a bind is
/// waiting for a pipeline still being built, or a PC call is going on after such calls have
/// kept the title's threads for long (`pc_call`). `kernelx::Sleep` does not count that time.
pub fn title_held() -> bool {
    HELD.load(Ordering::Acquire) != 0 || held_by_pc_call()
}

fn held_by_pc_call() -> bool {
    PC_CALLS.lock().unwrap().holds(super::present::presents(), std::time::Instant::now())
}

/// A call of the title into D3D12 that XCL or the PC's driver can keep for long where the
/// console's takes no time: a submission (XCL's uploads and tile mappings before the lists, the
/// driver's compiling in them), a texture's placement, a pipeline built in the call. While it
/// lasts the title is held (`title_held`), once the PC calls since the last frame shown, this
/// one included, have taken `PC_CALL_HOLDS_AFTER`: one long call, or many short ones in a row
/// (a retail title placed 16 384 textures in 29 s without a frame, 1 - 50 ms each). A title
/// that shows its frames is not held by its submissions.
pub struct PcCall(u64);

pub fn pc_call() -> PcCall {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    PC_CALLS.lock().unwrap().begin(id, super::present::presents(), std::time::Instant::now());
    PcCall(id)
}

impl Drop for PcCall {
    fn drop(&mut self) {
        PC_CALLS.lock().unwrap().end(self.0, super::present::presents(), std::time::Instant::now());
    }
}

/// Whether this thread is building a pipeline for `build` (a worker, not one of the title's).
pub(super) fn building_here() -> bool {
    BUILDING.with(|b| b.borrow().is_some())
}

/// A bind's wait for a pipeline still being built (`title_held`).
struct Held;

impl Held {
    fn new() -> Held {
        HELD.fetch_add(1, Ordering::AcqRel);
        Held
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        HELD.fetch_sub(1, Ordering::AcqRel);
    }
}

pub fn enabled() -> bool {
    std::env::var("XCL_ASYNC_PSO").map_or(true, |v| v != "0")
}

/// Builds the real pipeline: its raw pointer, 0 = failed.
pub type Job = Box<dyn FnOnce() -> usize + Send>;

/// Drop an unstarted orphaned job outside all locks and wake any concurrent waiter.
fn drop_unstarted_job(p: &Pending) {
    let job = p.job.lock().unwrap().take();
    if let Some(job) = job {
        {
            let mut done = p.done.lock().unwrap();
            *done = true;
            p.cv.notify_all();
        }
        drop(job);
    }
}

type Work = Box<dyn FnOnce() + Send>;

/// What a pool worker runs: a pipeline's build, or a helper of one (`build_pieces`).
enum Item {
    Build(Arc<Pending>),
    Help(Work),
}

/// The pool's lanes, in the order a general worker takes them (`pick`).
const WANTED_GRAPHICS: usize = 0;
const HELP: usize = 1;
const WANTED_COMPUTE: usize = 2;
const GRAPHICS: usize = 3;
const COMPUTE: usize = 4;
const LANES: usize = 5;

struct Pool {
    lanes: Mutex<[std::collections::VecDeque<Item>; LANES]>,
    /// the general workers wait here
    general: Condvar,
    /// the reserved graphics workers wait here
    reserved: Condvar,
}

static POOL: OnceLock<Pool> = OnceLock::new();
/// Reserved graphics workers started, and those running a build now.
static RESERVED: AtomicUsize = AtomicUsize::new(0);
static RESERVED_BUSY: AtomicUsize = AtomicUsize::new(0);
/// Worker threads started (the initial pool and the extra ones).
static WORKERS: AtomicUsize = AtomicUsize::new(0);
/// Workers now waiting at the big-compile gate.
static PARKED: AtomicUsize = AtomicUsize::new(0);
/// Extra workers started so far (names `xcl-pso-x{k}`).
static EXTRA_STARTED: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// this thread is one of the pool's workers (initial or extra)
    static IS_WORKER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// How many threads build only graphics pipelines, for a pool of `n` general workers.
fn reserved_workers(n: usize) -> usize {
    (n / 4).clamp(2, 8)
}

/// The lane a worker takes next, `None` when there is nothing for it. A general worker takes the
/// first lane with an item; a reserved one only the graphics lanes, wanted first.
fn pick(lens: &[usize; LANES], reserved: bool) -> Option<usize> {
    if reserved {
        return [WANTED_GRAPHICS, GRAPHICS].into_iter().find(|&l| lens[l] != 0);
    }
    (0..LANES).find(|&l| lens[l] != 0)
}

fn lane_for(compute: bool, wanted: bool) -> usize {
    match (compute, wanted) {
        (false, true) => WANTED_GRAPHICS,
        (true, true) => WANTED_COMPUTE,
        (false, false) => GRAPHICS,
        (true, false) => COMPUTE,
    }
}

fn pool() -> &'static Pool {
    static START: std::sync::Once = std::sync::Once::new();
    let pool = POOL.get_or_init(|| Pool { lanes: Mutex::new(Default::default()), general: Condvar::new(), reserved: Condvar::new() });
    START.call_once(|| {
        // translation recurses deeply on big programs; DXC itself runs on its own pool
        let n = crate::gpu::recomp::dxc::compile_threads();
        WORKERS.fetch_add(n, Ordering::SeqCst);
        for k in 0..n {
            if std::thread::Builder::new().name(format!("xcl-pso-{k}")).stack_size(64 << 20).spawn(|| worker_loop(false)).is_err() {
                WORKERS.fetch_sub(1, Ordering::SeqCst);
            }
        }
        // these take only graphics pipelines, so a queue of slow compute ones never holds them up
        for k in 0..reserved_workers(n) {
            if std::thread::Builder::new().name(format!("xcl-pso-g{k}")).stack_size(64 << 20).spawn(|| worker_loop(true)).is_ok() {
                RESERVED.fetch_add(1, Ordering::SeqCst);
            }
        }
    });
    pool
}

fn push(lane: usize, item: Item) {
    let pool = pool();
    pool.lanes.lock().unwrap()[lane].push_back(item);
    if lane == WANTED_GRAPHICS || lane == GRAPHICS {
        pool.reserved.notify_one();
    }
    pool.general.notify_one();
}

/// The next item for a worker of this class, waiting for one.
fn take(reserved: bool) -> Item {
    let pool = pool();
    let mut lanes = pool.lanes.lock().unwrap();
    loop {
        let mut lens = [0; LANES];
        for (len, lane) in lens.iter_mut().zip(lanes.iter()) {
            *len = lane.len();
        }
        if let Some(l) = pick(&lens, reserved) {
            return lanes[l].pop_front().unwrap();
        }
        let cv = if reserved { &pool.reserved } else { &pool.general };
        lanes = cv.wait(lanes).unwrap();
    }
}

fn worker_loop(reserved: bool) {
    // (reserved workers are not `IS_WORKER`: they never park at the gate)
    if !reserved {
        IS_WORKER.with(|w| w.set(true));
    }
    loop {
        let item = take(reserved);
        if reserved {
            RESERVED_BUSY.fetch_add(1, Ordering::SeqCst);
        }
        match item {
            Item::Build(p) => build_queued(&p),
            Item::Help(w) => w(),
        }
        if reserved {
            RESERVED_BUSY.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// A worker is short of a free place when fewer than `n` of its `workers` are not parked, up to
/// `2 * n` threads in all.
fn extra_needed(workers: usize, parked: usize, n: usize) -> bool {
    workers.saturating_sub(parked) < n && workers < 2 * n
}

/// A worker waiting at the big-compile gate, counted until it is dropped.
pub(super) struct Parked(bool);

impl Drop for Parked {
    fn drop(&mut self) {
        if self.0 {
            PARKED.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// The calling worker is about to wait at the gate: count it and, when the queued builds would
/// otherwise have too few workers, start one more (never exits). Other threads count nothing.
pub(super) fn park() -> Parked {
    if !IS_WORKER.with(|w| w.get()) {
        return Parked(false);
    }
    PARKED.fetch_add(1, Ordering::SeqCst);
    let parked = Parked(true);
    if POOL.get().is_none() {
        return parked;
    }
    let n = crate::gpu::recomp::dxc::compile_threads();
    // reserve the place with a compare-and-swap, so two parking workers never take the same one
    let mut w = WORKERS.load(Ordering::SeqCst);
    loop {
        if !extra_needed(w, PARKED.load(Ordering::SeqCst), n) {
            return parked;
        }
        match WORKERS.compare_exchange(w, w + 1, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => break,
            Err(now) => w = now,
        }
    }
    let k = EXTRA_STARTED.fetch_add(1, Ordering::SeqCst);
    if std::thread::Builder::new().name(format!("xcl-pso-x{k}")).stack_size(64 << 20).spawn(|| worker_loop(false)).is_err() {
        WORKERS.fetch_sub(1, Ordering::SeqCst);
    }
    parked
}

/// (worker threads started, workers parked at the gate now)
pub fn workers() -> (usize, usize) {
    (WORKERS.load(Ordering::SeqCst), PARKED.load(Ordering::SeqCst))
}

fn enqueue(p: Arc<Pending>) {
    push(lane_for(p.compute, false), Item::Build(p));
}

/// The pool's lane lengths and its workers' states (`queue_gauges`).
#[allow(dead_code)]
pub struct QueueGauges {
    pub lanes: [usize; LANES],
    pub general: usize,
    pub parked: usize,
    pub reserved: usize,
    pub reserved_busy: usize,
}

/// What the pool holds now. Starts nothing: all zero lanes before the first build is queued.
#[allow(dead_code)]
pub fn queue_gauges() -> QueueGauges {
    let mut lanes = [0; LANES];
    if let Some(pool) = POOL.get() {
        let held = pool.lanes.lock().unwrap();
        for (len, lane) in lanes.iter_mut().zip(held.iter()) {
            *len = lane.len();
        }
    }
    QueueGauges {
        lanes,
        general: WORKERS.load(Ordering::SeqCst),
        parked: PARKED.load(Ordering::SeqCst),
        reserved: RESERVED.load(Ordering::SeqCst),
        reserved_busy: RESERVED_BUSY.load(Ordering::SeqCst),
    }
}

fn build_queued(p: &Arc<Pending>) {
    // (released by the title meanwhile)
    if p.orphan.load(Ordering::Acquire) {
        drop_unstarted_job(p);
        return;
    }
    // (None: a bind took it and builds it itself)
    let job = p.job.lock().unwrap().take();
    if let Some(job) = job {
        build(p, job, false);
    }
}

/// The caller also drains its batch. Multiple workers can therefore build
/// different chains without ever waiting for a free worker to start a piece.
pub(super) fn build_pieces(jobs: Vec<Job>) -> Vec<usize> {
    let count = jobs.len();
    let jobs = Arc::new(Mutex::new(jobs.into_iter().enumerate().collect::<Vec<_>>()));
    let (send, recv) = mpsc::channel();
    let drain = move || loop {
        let job = jobs.lock().unwrap().pop();
        let Some((i, job)) = job else { return };
        let _ = send.send((i, job()));
    };
    // the helpers build for this worker's pipeline: the title waiting for it waits for them too
    let parent = BUILDING.with(|b| b.borrow().clone());
    for _ in 1..count.min(crate::gpu::recomp::dxc::compile_threads()) {
        let (drain, parent) = (drain.clone(), parent.clone());
        push(HELP, Item::Help(Box::new(move || {
            let before = BUILDING.with(|b| b.replace(parent));
            drain();
            BUILDING.with(|b| *b.borrow_mut() = before);
        })));
    }
    drain();
    let mut results = vec![0; count];
    for (i, result) in recv.iter().take(count) { results[i] = result; }
    results
}

/// Release a pipeline state XCL owns (raw pointer, 0 = none).
pub fn release(p: usize) {
    if p != 0 {
        unsafe { drop(<windows::Win32::Graphics::Direct3D12::ID3D12PipelineState as windows::core::Interface>::from_raw(p as *mut std::ffi::c_void)) };
    }
}

/// `standin` went to the title; `job` builds the real pipeline on a worker.
pub fn submit(standin: usize, compute: bool, job: Job) {
    submit_as(standin, compute, job, true, 0);
}

/// Lazy pipeline creation (`XCL_LAZY_PSO=0`: off): a title's pipelines with console shaders get
/// their stand-in when made and are built when first used – the first bind of a graphics one
/// sends it to the workers, a compute one is built at its first bind. A retail game makes
/// ~51 000 pipelines in its first 30 s and uses few; each one built stays in the driver
/// (~400 KB), and the process ran out of commit.
pub fn lazy() -> bool {
    static L: OnceLock<bool> = OnceLock::new();
    *L.get_or_init(|| enabled() && std::env::var("XCL_LAZY_PSO").map_or(true, |v| v != "0"))
}

/// `submit`, sent to the workers only on its first `resolve` (`lazy`) – or now, at a low
/// priority, when an earlier run used a pipeline of these shaders (`key`, 0 = none; `used`).
pub fn submit_lazy(standin: usize, compute: bool, job: Job, key: u64) {
    submit_as(standin, compute, job, false, key);
}

fn submit_as(standin: usize, compute: bool, job: Job, send: bool, key: u64) {
    let p = Arc::new(Pending {
        job: Mutex::new(Some(job)),
        sent: std::sync::atomic::AtomicBool::new(send),
        real: AtomicUsize::new(0),
        driver: AtomicUsize::new(0),
        done: Mutex::new(false),
        cv: Condvar::new(),
        compute,
        key,
        used: Default::default(),
        orphan: Default::default(),
        late: Default::default(),
        spared: Default::default(),
        demand: Default::default(),
    });
    PENDING.lock().unwrap().get_or_insert_with(HashMap::new).insert(standin, p.clone());
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        crate::xlog_info!("D3D12.X: pipelines whose shaders are not compiled yet get a stand-in and are built in the background");
    });
    if send {
        enqueue(p);
    } else if used::known(key) {
        if let Some(q) = preload() {
            q.list.lock().unwrap().push_back(p);
            q.cv.notify_one();
        }
    }
}

/// The keys of the pipelines earlier runs used (`used-pipelines.txt` beside the DXIL cache):
/// what `xcl shaders --precompile --used-only` builds.
pub fn earlier_used_keys() -> std::collections::HashSet<u64> {
    used::parse(&std::fs::read_to_string(crate::gpu::recomp::dxc::cache_dir().join("used-pipelines.txt")).unwrap_or_default())
}

/// The pipelines a title used, by a hash of their shaders, kept between runs
/// (`used-pipelines.txt` beside the DXIL cache, a key a line).
///
/// A lazy pipeline is built at its first use: until then a graphics one draws nothing, and a
/// compute one is waited for on the title's thread. A retail title's first frames of its world
/// use hundreds of pipelines for the first time at once. The pipelines of an earlier run are
/// built while the title loads instead (`preload`): measured on a build that skips a dispatch
/// instead of waiting for its pipeline, ~2 170 dispatches were skipped in those frames without
/// this and 94 – 150 with it.
mod used {
    use std::collections::HashSet;
    use std::io::Write;
    use std::sync::{Mutex, OnceLock, RwLock};

    struct Used {
        earlier: RwLock<HashSet<u64>>,
        now: Mutex<(HashSet<u64>, Option<std::fs::File>)>,
    }

    /// The keys in a `used-pipelines.txt` (lines that are not a hexadecimal key are passed over).
    pub fn parse(text: &str) -> HashSet<u64> {
        text.lines().filter_map(|l| u64::from_str_radix(l.trim(), 16).ok()).filter(|&k| k != 0).collect()
    }

    fn get() -> &'static Used {
        static U: OnceLock<Used> = OnceLock::new();
        U.get_or_init(|| {
            // (a test neither reads nor writes the user's file)
            if cfg!(test) {
                return Used { earlier: RwLock::new(HashSet::new()), now: Mutex::new((HashSet::new(), None)) };
            }
            let path = crate::gpu::recomp::dxc::cache_dir().join("used-pipelines.txt");
            let earlier = parse(&std::fs::read_to_string(&path).unwrap_or_default());
            if !earlier.is_empty() {
                crate::xlog_info!("D3D12.X: the pipelines of {} shader sets an earlier run used are built ahead of their first use", earlier.len());
            }
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let file = std::fs::OpenOptions::new().create(true).append(true).open(&path).ok();
            Used { now: Mutex::new((earlier.clone(), file)), earlier: RwLock::new(earlier) }
        })
    }

    /// An earlier run used a pipeline of these shaders.
    pub fn known(key: u64) -> bool {
        key != 0 && get().earlier.read().unwrap().contains(&key)
    }

    /// A pipeline of these shaders is used now: kept for the next run.
    pub fn note(key: u64) {
        if key == 0 {
            return;
        }
        let mut g = get().now.lock().unwrap();
        if g.0.insert(key) {
            if let Some(f) = g.1.as_mut() {
                let _ = writeln!(f, "{key:016x}");
            }
        }
    }

    /// (tests) This run used a pipeline of these shaders.
    #[cfg(test)]
    pub fn noted(key: u64) -> bool {
        get().now.lock().unwrap().0.contains(&key)
    }

    #[cfg(test)]
    pub fn earlier_run_used(key: u64) {
        get().earlier.write().unwrap().insert(key);
    }
}

/// The workers that build the pipelines of an earlier run ahead of their first use
/// (`XCL_PSO_PRELOAD=0`: off). Apart from the ones a bind sends to (`queue`) and at the lowest
/// priority: a pipeline that is bound is not held up by the ones made ahead (whoever takes
/// `job` first builds it), and the title's own threads are not either.
struct Preload {
    list: Mutex<std::collections::VecDeque<Arc<Pending>>>,
    cv: Condvar,
}

fn preload() -> Option<&'static Preload> {
    static Q: OnceLock<Option<Preload>> = OnceLock::new();
    static STARTED: std::sync::Once = std::sync::Once::new();
    let q = Q
        .get_or_init(|| {
            if std::env::var("XCL_PSO_PRELOAD").is_ok_and(|v| v.trim() == "0") {
                return None;
            }
            Some(Preload { list: Mutex::new(Default::default()), cv: Condvar::new() })
        })
        .as_ref()?;
    STARTED.call_once(|| {
        let n = (crate::gpu::recomp::dxc::compile_threads() / 4).clamp(2, 4);
        for k in 0..n {
            let _ = std::thread::Builder::new().name(format!("xcl-pso-pre-{k}")).stack_size(64 << 20).spawn(move || {
                unsafe {
                    use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_LOWEST};
                    SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST);
                }
                static DONE: AtomicUsize = AtomicUsize::new(0);
                loop {
                    let p = {
                        let mut g = q.list.lock().unwrap();
                        loop {
                            if let Some(p) = g.pop_front() {
                                break p;
                            }
                            g = q.cv.wait(g).unwrap();
                        }
                    };
                    // (released by the title meanwhile)
                    if p.orphan.load(Ordering::Acquire) {
                        drop_unstarted_job(&p);
                        continue;
                    }
                    // (None: a bind took it and builds it itself)
                    let job = p.job.lock().unwrap().take();
                    if let Some(job) = job {
                        build(&p, job, true);
                        let n = DONE.fetch_add(1, Ordering::Relaxed) + 1;
                        if n.is_power_of_two() {
                            crate::xlog_info!("D3D12.X: {n} pipelines built ahead of their first use");
                        }
                    }
                }
            });
        }
    });
    Some(q)
}

#[cfg(test)]
mod used_tests {
    use super::*;

    /// A PC call that has kept one of the title's threads for long holds the title, a short one
    /// does not, and the hold ends with the call.
    #[test]
    fn a_long_pc_call_holds_the_title() {
        let mut calls = PcCalls::new();
        let began = std::time::Instant::now();
        calls.begin(1, 0, began);
        calls.begin(2, 0, began + 2 * PC_CALL_HOLDS_AFTER);
        assert!(!calls.holds(0, began), "a call that has just begun holds the title");
        let later = began + 2 * PC_CALL_HOLDS_AFTER;
        assert!(calls.holds(0, later), "a PC call has lasted {:?}, but the title is not held", 2 * PC_CALL_HOLDS_AFTER);
        calls.end(1, 0, later);
        calls.end(2, 0, later);
        assert!(!calls.holds(0, later), "the calls are over, the title is still held");
    }

    /// Short PC calls add up between two frames: none of them is long, but after many in a row
    /// the one going on holds the title. The title's own time between them is not held, and a
    /// frame shown ends it.
    #[test]
    fn short_pc_calls_between_two_frames_hold_the_title() {
        let mut calls = PcCalls::new();
        let step = PC_CALL_HOLDS_AFTER / 10;
        let mut now = std::time::Instant::now();
        for id in 0..20 {
            calls.begin(id, 7, now);
            assert!(id >= 9 || !calls.holds(7, now + step), "{id} short calls hold the title already");
            now += step;
            calls.end(id, 7, now);
        }
        assert!(!calls.holds(7, now), "no PC call is going on, the title is held");
        calls.begin(20, 7, now);
        assert!(calls.holds(7, now), "twenty short PC calls since the last frame and one going on, but the title is not held");
        assert!(!calls.holds(8, now), "a frame was shown, the calls before it still hold the title");
        calls.end(20, 8, now);
    }

    /// While a bind waits for a pipeline still being built, the title is held (`title_held`):
    /// `kernelx::Sleep` does not count that time.
    #[test]
    fn a_bind_waiting_for_a_build_holds_the_title() {
        let standin = 0x7E57_B002usize;
        let (started, building) = mpsc::channel();
        let (go, finish) = mpsc::channel::<()>();
        submit_lazy(standin, true, Box::new(move || {
            let _ = started.send(());
            let _ = finish.recv();
            0x5252
        }), 0);
        let bind = std::thread::spawn(move || resolve_wait_for(standin, None));
        building.recv_timeout(std::time::Duration::from_secs(10)).expect("the pipeline is being built");
        // (built in the bind, or by a worker with the bind about to wait for it)
        let t = std::time::Instant::now();
        while !title_held() && t.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(title_held(), "a bind waits for a build, but the title is not held");
        go.send(()).unwrap();
        assert_eq!(bind.join().unwrap(), 0x5252);
        let _ = released(standin);
    }

    /// A stand-in that is no driver object (`shared`) never reaches the driver: binds get the
    /// driver pipeline shared with other stand-ins until the real one is built, and after a build
    /// that failed.
    #[test]
    fn a_bind_of_a_shared_stand_in_gets_its_driver_pipeline() {
        let (fails, builds, driver) = (0x7E57_B011usize, 0x7E57_B012usize, 0x7E57_B013usize);
        submit_lazy(fails, false, Box::new(|| 0), 0);
        submit_lazy(builds, false, Box::new(|| 0x1234), 0);
        for s in [fails, builds] {
            shared(s, driver);
            assert_eq!(entry(s).unwrap().stand(s), driver);
        }
        assert_eq!(resolve_wait_for(fails, None), driver, "its build failed");
        assert_eq!(resolve(fails), driver);
        assert_eq!(resolve_wait_for(builds, None), 0x1234);
        assert_eq!(resolve_for_bind(builds), 0x1234);
        // (0x1234 is no pipeline: taken, not released)
        assert_eq!((released(fails), released(builds)), (0, 0x1234));
    }

    /// A bind does not wait for a compute pipeline that writes only images: it gets the stand-in
    /// at once, with no budget, the build goes to the workers
    /// unwanted, and once built the bind gets the real one. Without `image_only` the same bind
    /// waits until the pipeline is built.
    #[test]
    fn a_bind_does_not_wait_for_a_pipeline_that_writes_only_images() {
        let (spared, waited) = (0x7E57_B0A1usize, 0x7E57_B0A2usize);
        let (go, finish) = mpsc::channel::<()>();
        let (tx, rx) = mpsc::channel();
        submit_lazy(spared, true, Box::new(move || {
            let _ = tx.send(wanted_here()());
            let _ = finish.recv();
            0x5A5A
        }), 0);
        image_only(spared);
        // (on a thread: a bind that waits would wait for `go`, below)
        let (bound_tx, bound) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = bound_tx.send((resolve_wait_for(spared, None), resolve_wait_for(spared, None)));
        });
        let binds = bound.recv_timeout(std::time::Duration::from_secs(5));
        if binds.is_err() {
            let _ = go.send(());
        }
        assert_eq!(binds.expect("the binds waited for the build"), (spared, spared), "still being built: the stand-in");
        assert!(is_pending(spared));
        assert!(!rx.recv_timeout(std::time::Duration::from_secs(10)).expect("a worker builds it"), "nobody waits: the build is not wanted");
        go.send(()).unwrap();
        let start = std::time::Instant::now();
        while is_pending(spared) && start.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(resolve_wait_for(spared, None), 0x5A5A, "built by a worker");
        // the same build without the mark: the bind waits for it
        submit_lazy(waited, true, Box::new(|| {
            std::thread::sleep(std::time::Duration::from_millis(100));
            0x5B5B
        }), 0);
        assert_eq!(resolve_wait_for(waited, None), 0x5B5B);
        let _ = released(spared);
        let _ = released(waited);
    }

    /// (the draw tests share the frame budget: one at a time)
    static DRAW_LOCK: Mutex<()> = Mutex::new(());

    fn ms(n: u64) -> std::time::Duration {
        std::time::Duration::from_millis(n)
    }

    fn wait_built(standin: usize) {
        let start = std::time::Instant::now();
        while is_pending(standin) && start.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(ms(5));
        }
    }

    /// A draw never builds the pipeline on its thread: a worker does, wanted and ahead of the
    /// queue, and the draw gets the stand-in once its allowance is over.
    #[test]
    fn a_draw_never_builds_the_pipeline_on_its_thread() {
        let _g = DRAW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let standin = 0x7E57_DD01usize;
        let (info_tx, info) = mpsc::channel();
        let (go, finish) = mpsc::channel::<()>();
        submit_lazy(standin, false, Box::new(move || {
            let _ = info_tx.send((std::thread::current().name().map(str::to_owned), build_is_wanted()));
            let _ = finish.recv();
            0x6001
        }), 0);
        let t = std::time::Instant::now();
        assert_eq!(resolve_draw_with(standin, 1_000_001, ms(100)), (standin, true));
        assert!(t.elapsed() < ms(400), "waited {:?}", t.elapsed());
        let (name, wanted) = info.recv_timeout(std::time::Duration::from_secs(10)).expect("a worker builds it");
        assert!(name.is_some_and(|n| n.starts_with("xcl-pso")), "built on a thread that is not a worker");
        assert!(wanted, "the build did not hear that a draw waits for it");
        go.send(()).unwrap();
        wait_built(standin);
        assert_eq!(resolve_draw_with(standin, 1_000_001, ms(100)), (0x6001, false));
        let _ = released(standin);
    }

    /// Two draws share one frame budget: what the first spent is gone for the second, and the
    /// next frame has it again for a pipeline first needed there; one that had its chance is not
    /// waited for again.
    #[test]
    fn the_frame_budget_is_shared_and_resets_on_the_next_frame() {
        let _g = DRAW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (a, b, c) = (0x7E57_DD11usize, 0x7E57_DD12usize, 0x7E57_DD13usize);
        let (go, finish) = mpsc::channel::<()>();
        let finish = Arc::new(Mutex::new(finish));
        for (s, real) in [(a, 0x6011), (b, 0x6012), (c, 0x6013)] {
            let finish = finish.clone();
            submit_lazy(s, false, Box::new(move || {
                let _ = finish.lock().unwrap().recv();
                real
            }), 0);
        }
        let t = std::time::Instant::now();
        assert_eq!(resolve_draw_with(a, 1_000_007, ms(60)), (a, true));
        assert!(t.elapsed() >= ms(55) && t.elapsed() < ms(400), "the first waited {:?}", t.elapsed());
        let t = std::time::Instant::now();
        assert_eq!(resolve_draw_with(b, 1_000_007, ms(60)), (b, true));
        assert!(t.elapsed() < ms(15), "the second waited {:?} in a spent budget", t.elapsed());
        // (a pipeline first needed in the next frame has the whole budget)
        let t = std::time::Instant::now();
        assert_eq!(resolve_draw_with(c, 1_000_008, ms(60)), (c, true));
        assert!(t.elapsed() >= ms(55) && t.elapsed() < ms(400), "a new frame has its budget again ({:?})", t.elapsed());
        // (the first one had its chance in the earlier frame)
        let t = std::time::Instant::now();
        assert_eq!(resolve_draw_with(a, 1_000_008, ms(60)), (a, true));
        assert!(t.elapsed() < ms(15), "waited again {:?}", t.elapsed());
        drop(go);
        wait_built(a);
        wait_built(b);
        wait_built(c);
        let _ = (released(a), released(b), released(c));
    }

    /// A pipeline is waited for during the first budget after it was first needed, and no
    /// longer: a burst of pipelines costs one budget, not one each.
    #[test]
    fn a_pipeline_gets_one_chance_to_be_waited_for() {
        let _g = DRAW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (a, b) = (0x7E57_F501usize, 0x7E57_F502usize);
        let (go, finish) = mpsc::channel::<()>();
        let finish = Arc::new(Mutex::new(finish));
        for s in [a, b] {
            let finish = finish.clone();
            submit_lazy(s, false, Box::new(move || {
                let _ = finish.lock().unwrap().recv();
                0x6F01
            }), 0);
        }
        let late = |s: usize| entry(s).unwrap().late.load(Ordering::Acquire);
        let t = std::time::Instant::now();
        assert_eq!(resolve_draw_with(a, 1_000_101, ms(60)), (a, true));
        assert!(t.elapsed() >= ms(55) && t.elapsed() < ms(400), "the first waited {:?}", t.elapsed());
        let t = std::time::Instant::now();
        assert_eq!(resolve_draw_with(b, 1_000_101, ms(60)), (b, true));
        assert!(t.elapsed() < ms(15), "the second waited {:?}", t.elapsed());
        assert!(!late(b), "it never waited, but it is late");
        // (the next frame, more than the budget after its first need)
        std::thread::sleep(ms(65));
        let t = std::time::Instant::now();
        assert_eq!(resolve_draw_with(b, 1_000_102, ms(60)), (b, true));
        assert!(t.elapsed() < ms(15), "waited after its chance {:?}", t.elapsed());
        assert!(late(b), "its chance is over, but it is not late");
        drop(go);
        wait_built(a);
        wait_built(b);
        let _ = (released(a), released(b));
    }

    /// A draw that found the budget spent does not mark the pipeline late; one whose wait ran out does.
    #[test]
    fn a_spent_budget_does_not_mark_the_pipeline_late() {
        let _g = DRAW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let standin = 0x7E57_DD21usize;
        let (go, finish) = mpsc::channel::<()>();
        submit_lazy(standin, false, Box::new(move || {
            let _ = finish.recv();
            0x6021
        }), 0);
        let late = || entry(standin).unwrap().late.load(Ordering::Acquire);
        // (frame 1_000_021 has no budget left: other draws waited in it)
        frame_allowance(&FRAME_SPENT, 1_000_021, ms(30));
        frame_spend(&FRAME_SPENT, 1_000_021, ms(1000));
        assert_eq!(resolve_draw_with(standin, 1_000_021, ms(30)), (standin, true));
        assert!(!late(), "no wait ran, but the pipeline is late");
        assert_eq!(resolve_draw_with(standin, 1_000_022, ms(30)), (standin, true));
        assert!(late(), "the wait ran out, but the pipeline is not late");
        go.send(()).unwrap();
        wait_built(standin);
        assert_eq!(resolve_draw_with(standin, 1_000_023, ms(30)), (0x6021, false));
        let _ = released(standin);
    }

    /// A built pipeline is returned at once, a failed one is the stand-in and not "unbuilt".
    #[test]
    fn a_built_graphics_pipeline_is_returned_at_once() {
        let _g = DRAW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (built, failed) = (0x7E57_DD31usize, 0x7E57_DD32usize);
        submit_lazy(built, false, Box::new(|| 0x6031), 0);
        submit_lazy(failed, false, Box::new(|| 0), 0);
        assert_eq!(resolve_draw_with(built, 1_000_031, ms(2000)), (0x6031, false));
        assert_eq!(resolve_draw_with(failed, 1_000_031, ms(2000)), (failed, false));
        let t = std::time::Instant::now();
        assert_eq!(resolve_draw_with(built, 1_000_031, std::time::Duration::ZERO), (0x6031, false));
        assert_eq!(resolve_draw_with(failed, 1_000_031, std::time::Duration::ZERO), (failed, false));
        assert!(t.elapsed() < ms(20), "{:?}", t.elapsed());
        assert_eq!(resolve_draw_with(0x7E57_DDFF, 1_000_031, ms(10)), (0x7E57_DDFF, false), "no entry");
        let _ = (released(built), released(failed));
    }

    #[test]
    fn a_pipeline_a_submit_needs_is_built_before_the_queue() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let backlog = 8 * crate::gpu::recomp::dxc::compile_threads();
        let standins: Vec<usize> = (0..=backlog).map(|i| 0x7E57_E000usize + i).collect();
        for &standin in &standins {
            let order = order.clone();
            submit(standin, true, Box::new(move || {
                std::thread::sleep(std::time::Duration::from_millis(50));
                order.lock().unwrap().push(standin);
                0x6161
            }));
        }
        let last = *standins.last().unwrap();
        need_by_submit(last);
        let start = std::time::Instant::now();
        while standins.iter().any(|&s| is_pending(s)) && start.elapsed() < std::time::Duration::from_secs(30) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let order = order.lock().unwrap();
        assert_eq!(order.len(), standins.len());
        let at = order.iter().position(|&s| s == last).unwrap();
        assert!(at < order.len() / 2, "made last and needed by a submit, it was built {at} of {}", order.len());
        for &standin in &standins {
            let _ = released(standin);
        }
    }

    /// A bind waits for a pipeline still being built no longer than its budget and goes on with
    /// the stand-in; later binds do not wait for it again; once built it is the real one.
    #[test]
    fn a_bind_waits_for_a_slow_pipeline_no_longer_than_its_budget() {
        let standin = 0x7E57_B001usize;
        submit(standin, true, Box::new(|| {
            std::thread::sleep(std::time::Duration::from_millis(600));
            0x5151
        }));
        let budget = Some(std::time::Duration::from_millis(30));
        let t = std::time::Instant::now();
        assert_eq!(resolve_wait_for(standin, budget), standin, "not built within the budget: the stand-in");
        assert!(t.elapsed() < std::time::Duration::from_millis(400), "waited {:?}", t.elapsed());
        let t = std::time::Instant::now();
        assert_eq!(resolve_wait_for(standin, budget), standin);
        assert!(t.elapsed() < std::time::Duration::from_millis(20), "a late pipeline is not waited for again ({:?})", t.elapsed());
        let start = std::time::Instant::now();
        while is_pending(standin) && start.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(resolve_wait_for(standin, budget), 0x5151, "built by a worker");
        let _ = released(standin);
    }

    /// A compile a worker's build starts is wanted once a bind waits for the build – asked from
    /// another thread, as the DXC workers do; a thread that builds nothing (no title runs here)
    /// wants nothing.
    #[test]
    fn a_waiting_bind_makes_the_builds_compiles_wanted() {
        let standin = 0x7E57_B005usize;
        let (tx, rx) = mpsc::channel();
        submit(standin, true, Box::new(move || {
            tx.send(wanted_here()).unwrap();
            let t = std::time::Instant::now();
            while !build_is_wanted() && t.elapsed() < std::time::Duration::from_secs(10) {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            0x5454
        }));
        let wanted = rx.recv().unwrap();
        assert!(!wanted(), "nobody waits yet");
        assert_eq!(resolve_wait_for(standin, None), 0x5454);
        assert!(wanted());
        assert!(!wanted_here()());
        let _ = released(standin);
    }

    #[test]
    fn a_build_a_submit_needs_is_wanted() {
        let standin = 0x7E57_B006usize;
        let (started_tx, started) = mpsc::channel();
        submit(standin, true, Box::new(move || {
            started_tx.send(()).unwrap();
            let t = std::time::Instant::now();
            while !build_is_wanted() && t.elapsed() < std::time::Duration::from_secs(10) {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            if build_is_wanted() { 0x5757 } else { 0 }
        }));
        started.recv().unwrap();
        need_by_submit(standin);
        let t = std::time::Instant::now();
        while is_pending(standin) && t.elapsed() < std::time::Duration::from_secs(15) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(resolve(standin), 0x5757, "the build did not hear that a submit waits for it");
        let _ = released(standin);
    }

    /// A worker's build hears that a bind waits for it (and only its own build does).
    #[test]
    fn a_waiting_bind_makes_the_build_wanted() {
        let standin = 0x7E57_B004usize;
        let (started_tx, started) = mpsc::channel();
        submit(standin, true, Box::new(move || {
            let unwanted_first = !build_is_wanted();
            started_tx.send(()).unwrap();
            let t = std::time::Instant::now();
            while !build_is_wanted() && t.elapsed() < std::time::Duration::from_secs(10) {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            if unwanted_first && build_is_wanted() { 0x5353 } else { 0 }
        }));
        started.recv().unwrap();
        assert!(!build_is_wanted(), "this thread builds nothing");
        assert_eq!(resolve_wait_for(standin, None), 0x5353, "the bind's wait reached the build");
        let _ = released(standin);
    }

    #[test]
    fn the_keys_of_a_file_are_read_and_other_lines_passed_over() {
        let keys = used::parse("00000000000000ff\n\nnot a key\n  1a2b \n0\n");
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&0xff) && keys.contains(&0x1a2b));
    }

    /// A lazy pipeline of shaders an earlier run used is built without a bind; another one is
    /// built only at its first use, and that use is kept for the next run.
    #[test]
    fn a_pipeline_an_earlier_run_used_is_built_ahead_of_its_first_use() {
        let (known, unknown) = (0x7E57_7001usize, 0x7E57_7002usize);
        let (known_key, unknown_key) = (0x5EED_0001u64, 0x5EED_0002u64);
        used::earlier_run_used(known_key);
        submit_lazy(known, false, Box::new(|| 0x4321), known_key);
        submit_lazy(unknown, false, Box::new(|| 0x4322), unknown_key);
        let start = std::time::Instant::now();
        while is_pending(known) && start.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!is_pending(known), "built by the workers that build ahead, with no bind");
        assert_eq!(resolve(known), 0x4321);
        assert!(is_pending(unknown), "nobody used it and no earlier run did");
        assert!(!used::noted(unknown_key));
        assert_eq!(resolve_wait(unknown), 0x4322);
        assert!(used::noted(unknown_key) && used::noted(known_key), "a first use is kept for the next run");
        released(known);
        released(unknown);
    }

    static WORKER_SATURATION_LOCK: Mutex<()> = Mutex::new(());

    /// Opens a gate when dropped, so a failed test never leaves the pool blocked on it.
    struct OpenGate(Arc<(Mutex<bool>, Condvar)>);

    impl Drop for OpenGate {
        fn drop(&mut self) {
            *self.0 .0.lock().unwrap_or_else(|e| e.into_inner()) = true;
            self.0 .1.notify_all();
        }
    }

    /// Occupies every general worker, the extra ones an earlier test started included, with a
    /// blocker that waits at `gate` (and never parks, so it starts no extra worker), and returns
    /// their stand-ins once no general worker is free. The blockers are compute builds, so the
    /// reserved graphics workers cannot take them; they return 0 (a failed build: nothing for
    /// `release`). The stand-ins are `base + k`, k < 2 * compile_threads(). The caller holds
    /// `WORKER_SATURATION_LOCK` and an `OpenGate` on `gate` before calling, and releases the
    /// stand-ins after opening it. Panics when the workers are not all held within the bounds.
    fn saturate_general_workers(base: usize, gate: &Arc<(Mutex<bool>, Condvar)>) -> Vec<usize> {
        use std::time::{Duration, Instant};

        let n = crate::gpu::recomp::dxc::compile_threads();
        let (started_tx, started_rx) = mpsc::channel();
        let mut blockers: Vec<usize> = Vec::with_capacity(2 * n);
        let mut started = 0;
        let mut stable = 0;
        let overall = Instant::now() + Duration::from_secs(120);
        loop {
            assert!(Instant::now() < overall, "the general workers were not all held within 120 s ({} blockers, {} workers)", blockers.len(), workers().0);
            let held = workers().0;
            assert!(held <= 2 * n, "more than 2 * compile_threads general workers ({held})");
            while blockers.len() < held {
                let standin = base + blockers.len();
                let (tx, gate) = (started_tx.clone(), gate.clone());
                submit(standin, true, Box::new(move || {
                    let _ = tx.send(());
                    let deadline = Instant::now() + Duration::from_secs(60);
                    let mut open = gate.0.lock().unwrap_or_else(|e| e.into_inner());
                    while !*open {
                        let left = deadline.saturating_duration_since(Instant::now());
                        if left.is_zero() {
                            break;
                        }
                        open = gate.1.wait_timeout(open, left).unwrap_or_else(|e| e.into_inner()).0;
                    }
                    0
                }));
                blockers.push(standin);
            }
            while started < blockers.len() {
                started_rx
                    .recv_timeout(Duration::from_secs(20))
                    .unwrap_or_else(|_| panic!("a blocker did not start within 20 s: {started} of {} started, {} general workers", blockers.len(), workers().0));
                started += 1;
            }
            if workers().0 == started {
                stable += 1;
                if stable == 2 {
                    return blockers;
                }
            } else {
                stable = 0;
            }
            // (a worker that starts now has had a moment to show itself)
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn extra_workers_follow_the_parked_count() {
        assert!(!extra_needed(15, 0, 15));
        assert!(extra_needed(15, 1, 15));
        assert!(!extra_needed(16, 1, 15));
        assert!(!extra_needed(30, 16, 15));
    }

    #[test]
    fn general_workers_take_lanes_in_order() {
        assert_eq!(pick(&[1, 1, 1, 1, 1], false), Some(0));
        assert_eq!(pick(&[0, 1, 1, 1, 1], false), Some(1));
        assert_eq!(pick(&[0, 0, 1, 1, 1], false), Some(2));
        assert_eq!(pick(&[0, 0, 0, 1, 1], false), Some(3));
        assert_eq!(pick(&[0, 0, 0, 0, 1], false), Some(4));
        assert_eq!(pick(&[0, 0, 0, 0, 0], false), None);
    }

    #[test]
    fn reserved_workers_take_only_graphics() {
        assert_eq!(pick(&[1, 1, 1, 1, 1], true), Some(WANTED_GRAPHICS));
        assert_eq!(pick(&[0, 1, 1, 1, 1], true), Some(GRAPHICS));
        assert_eq!(pick(&[0, 1, 1, 0, 1], true), None);
        assert_eq!(pick(&[0, 0, 0, 0, 0], true), None);
    }

    #[test]
    fn lanes_follow_kind_and_demand() {
        assert_eq!(lane_for(false, true), WANTED_GRAPHICS);
        assert_eq!(lane_for(true, true), WANTED_COMPUTE);
        assert_eq!(lane_for(false, false), GRAPHICS);
        assert_eq!(lane_for(true, false), COMPUTE);
    }

    #[test]
    fn reserved_workers_scale_with_the_pool() {
        assert_eq!(reserved_workers(1), 2);
        assert_eq!(reserved_workers(8), 2);
        assert_eq!(reserved_workers(15), 3);
        assert_eq!(reserved_workers(32), 8);
        assert_eq!(reserved_workers(64), 8);
    }

    /// With every general worker parked at the gate, a graphics build still runs (on a reserved
    /// worker) and a compute build does not.
    #[test]
    fn a_graphics_build_runs_while_every_general_worker_is_parked() {
        let _sat = WORKER_SATURATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use std::sync::atomic::AtomicBool;
        use std::time::{Duration, Instant};

        let n = crate::gpu::recomp::dxc::compile_threads();
        let start = Instant::now();
        while queue_gauges().lanes.iter().any(|&l| l != 0) && start.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(10));
        }
        let before = workers().1;
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let _open = OpenGate(gate.clone());
        let (started_tx, started_rx) = mpsc::channel();
        let mut standins = Vec::with_capacity(2 * n + 2);
        for k in 0..2 * n {
            let standin = 0x7E57_F100usize + k;
            let (gate, started_tx) = (gate.clone(), started_tx.clone());
            submit(standin, true, Box::new(move || {
                let _ = started_tx.send(());
                let _parked = park();
                let deadline = Instant::now() + Duration::from_secs(40);
                let mut open = gate.0.lock().unwrap_or_else(|e| e.into_inner());
                while !*open {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        break;
                    }
                    open = gate.1.wait_timeout(open, left).unwrap_or_else(|e| e.into_inner()).0;
                }
                // (0: a failed build, nothing for `release` whenever the stand-in is released)
                0
            }));
            standins.push(standin);
        }
        let mut started = 0;
        while started < 2 * n && started_rx.recv_timeout(Duration::from_secs(10)).is_ok() {
            started += 1;
        }

        let (compute, graphics) = (0x7E57_F1C0usize, 0x7E57_F1C1usize);
        let compute_ran = Arc::new(AtomicBool::new(false));
        let (graphics_tx, graphics_rx) = mpsc::channel();
        let flag = compute_ran.clone();
        submit(compute, true, Box::new(move || {
            flag.store(true, Ordering::Release);
            0
        }));
        submit(graphics, false, Box::new(move || {
            let _ = graphics_tx.send(());
            0
        }));
        standins.push(compute);
        standins.push(graphics);
        let graphics_ran = graphics_rx.recv_timeout(Duration::from_secs(20)).is_ok();
        let compute_ran_early = compute_ran.load(Ordering::Acquire);

        *gate.0.lock().unwrap() = true;
        gate.1.notify_all();
        for &standin in &standins {
            let start = Instant::now();
            while is_pending(standin) && start.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(5));
            }
            released(standin);
        }
        assert_eq!(started, 2 * n, "every blocker started");
        assert!(graphics_ran, "a graphics build ran while every general worker was parked");
        assert!(!compute_ran_early, "a reserved worker took a compute build");
        assert!(compute_ran.load(Ordering::Acquire), "the compute build ran once the gate opened");
        assert_eq!(workers().1, before, "parked count returns");
    }

    /// With every worker parked at the gate, a job queued meanwhile still runs.
    #[test]
    fn queued_work_runs_while_every_worker_is_parked() {
        let _sat = WORKER_SATURATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use std::time::{Duration, Instant};

        let n = crate::gpu::recomp::dxc::compile_threads();
        // (other tests queue builds on the same pool: their backlog goes first, so let it drain)
        let start = Instant::now();
        while queue_gauges().lanes.iter().any(|&l| l != 0) && start.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(10));
        }
        let before = workers().1;
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let _open = OpenGate(gate.clone());
        let mut blockers = Vec::with_capacity(n);
        for k in 0..n {
            let standin = 0x7E57_9730usize + k;
            let gate = gate.clone();
            submit(standin, true, Box::new(move || {
                let _parked = park();
                let deadline = Instant::now() + Duration::from_secs(40);
                let mut open = gate.0.lock().unwrap_or_else(|e| e.into_inner());
                while !*open {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        break;
                    }
                    open = gate.1.wait_timeout(open, left).unwrap_or_else(|e| e.into_inner()).0;
                }
                // (0: a failed build, nothing for `release` whenever the stand-in is released)
                0
            }));
            blockers.push(standin);
        }
        let start = Instant::now();
        while workers().1 < before + n && start.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(workers().1 >= before + n, "every job parked");
        assert!(workers().0 <= 2 * n, "never more than 2 * compile_threads workers");

        let (done_tx, done_rx) = mpsc::channel();
        let target = 0x7E57_973Fusize;
        submit(target, true, Box::new(move || {
            let _ = done_tx.send(());
            0
        }));
        let ran = done_rx.recv_timeout(Duration::from_secs(20)).is_ok();

        *gate.0.lock().unwrap() = true;
        gate.1.notify_all();
        for standin in blockers.iter().copied().chain(std::iter::once(target)) {
            let start = Instant::now();
            while is_pending(standin) && start.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(5));
            }
            released(standin);
        }
        assert!(ran, "queued work ran while every worker was parked");
        assert_eq!(workers().1, before, "parked count returns");
    }

    /// A queued normal worker job whose stand-in was released before a worker starts it is
    /// skipped as orphaned instead of entering the compiler.
    #[test]
    fn a_released_queued_pipeline_is_not_built_by_normal_workers() {
        let _sat = WORKER_SATURATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use std::sync::atomic::AtomicBool;
        use std::time::{Duration, Instant};

        struct DropFlag(Arc<AtomicBool>);
        impl Drop for DropFlag {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }

        // every general worker, the extra ones an earlier test started included, holds a blocker
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let _open = OpenGate(gate.clone());
        let blockers = saturate_general_workers(0x7E57_9A00, &gate);

        let target = 0x7E57_97FFusize;
        let ran = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicBool::new(false));
        let ran_job = ran.clone();
        let drop_flag = DropFlag(dropped.clone());
        submit(target, true, Box::new(move || {
            let _keep = &drop_flag;
            ran_job.store(true, Ordering::Release);
            // (0: a failed build, nothing for `release` if it ever ran)
            0
        }));
        assert_eq!(released(target), 0);

        *gate.0.lock().unwrap() = true;
        gate.1.notify_all();

        let deadline = Instant::now() + Duration::from_secs(10);
        while !dropped.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(dropped.load(Ordering::Acquire), "queued orphan job was never drained");
        assert!(!ran.load(Ordering::Acquire), "released job reached the compiler");

        for standin in blockers {
            let start = Instant::now();
            while is_pending(standin) && start.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(2));
            }
            let _ = released(standin);
        }
    }

    #[test]
    fn pc_call_crossing_frame_boundary_does_not_charge_prior_frame_time() {
        let mut calls = PcCalls::new();
        let t0 = std::time::Instant::now();
        let ms = std::time::Duration::from_millis;
        calls.begin(1, 7, t0);
        // Frame 8 shown after 290 ms; call 1 ends 10 ms later -> only 10 ms counts on frame 8.
        let t_frame8 = t0 + PC_CALL_HOLDS_AFTER + ms(40);
        assert!(!calls.holds(8, t_frame8), "call from frame 7 must not immediately hold frame 8");
        calls.end(1, 8, t_frame8 + ms(10));
        let t_next = t_frame8 + ms(11);
        calls.begin(2, 8, t_next);
        assert!(!calls.holds(8, t_next), "pre-frame duration leaked into frame 8's budget");
        // A call genuinely stuck >= PC_CALL_HOLDS_AFTER after frame 8 still holds the title.
        assert!(!calls.holds(8, t_next + PC_CALL_HOLDS_AFTER - ms(20)));
        assert!(calls.holds(8, t_next + PC_CALL_HOLDS_AFTER));
        calls.end(2, 8, t_next + PC_CALL_HOLDS_AFTER);

        // Also when frame 9 is first noticed inside end() itself, pre-frame time is clamped to 0.
        let t_f9 = t_next + PC_CALL_HOLDS_AFTER + ms(10);
        calls.begin(3, 8, t_f9);
        let t_f9_end = t_f9 + PC_CALL_HOLDS_AFTER + ms(50);
        calls.end(3, 9, t_f9_end);
        calls.begin(4, 9, t_f9_end + ms(1));
        assert!(!calls.holds(9, t_f9_end + ms(1)), "end() on new frame must not charge prior-frame duration");
        calls.end(4, 9, t_f9_end + ms(1) + PC_CALL_HOLDS_AFTER);

        // A stale lower frame number must never rewind self.frame or wipe frame 9's spent budget.
        let t_stale = t_f9_end + PC_CALL_HOLDS_AFTER + ms(10);
        calls.begin(5, 8, t_stale);
        assert!(calls.holds(9, t_stale), "stale lower frame rewound frame 9 or cleared its spent budget");
        calls.end(5, 9, t_stale);
    }

    #[test]
    fn released_queued_pipeline_drops_job_immediately_before_workers_drain() {
        let _sat = WORKER_SATURATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        use std::sync::atomic::AtomicBool;
        use std::time::{Duration, Instant};

        struct DropFlag(Arc<AtomicBool>);
        impl Drop for DropFlag {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        // Saturate normal workers (every general worker, the extra ones included) and verify
        // released() drops the queued job immediately.
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let open_normal = OpenGate(gate.clone());
        let blockers = saturate_general_workers(0x7E57_9B00, &gate);
        let target_n = 0x7E57_A1FFusize;
        let (ran_n, dropped_n) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
        let (ran_job, flag_n) = (ran_n.clone(), DropFlag(dropped_n.clone()));
        submit(target_n, true, Box::new(move || {
            let _keep = &flag_n;
            ran_job.store(true, Ordering::Release);
            // (0: a failed build, nothing for `release` if it ever ran)
            0
        }));
        assert_eq!(released(target_n), 0);
        assert!(dropped_n.load(Ordering::Acquire), "normal queued job was not dropped immediately on released()");
        drop(open_normal);
        assert!(!ran_n.load(Ordering::Acquire));
        for standin in blockers {
            let start = Instant::now();
            while is_pending(standin) && start.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(2));
            }
            let _ = released(standin);
        }

        // Saturate preload workers and verify released() drops the preloaded job immediately.
        let m = (crate::gpu::recomp::dxc::compile_threads() / 4).clamp(2, 4);
        let (p_tx, p_rx) = mpsc::channel();
        let p_gate = Arc::new((Mutex::new(false), Condvar::new()));
        let open_pre = OpenGate(p_gate.clone());
        let mut p_blockers = Vec::with_capacity(m);
        for k in 0..m {
            let standin = 0x7E57_A200usize + k;
            let key = 0x5EED_A200u64 + k as u64;
            used::earlier_run_used(key);
            let (tx, g) = (p_tx.clone(), p_gate.clone());
            submit_lazy(standin, false, Box::new(move || {
                let _ = tx.send(());
                let mut open = g.0.lock().unwrap();
                while !*open {
                    open = g.1.wait(open).unwrap();
                }
                // (0: a failed build, nothing for `release` whenever the stand-in is released)
                0
            }), key);
            p_blockers.push(standin);
        }
        for _ in 0..m {
            p_rx.recv_timeout(Duration::from_secs(10)).expect("preload workers saturated");
        }
        let target_p = 0x7E57_A2FFusize;
        let key_p = 0x5EED_A2FFu64;
        used::earlier_run_used(key_p);
        let (ran_p, dropped_p) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
        let (ran_job, flag_p) = (ran_p.clone(), DropFlag(dropped_p.clone()));
        submit_lazy(target_p, false, Box::new(move || {
            let _keep = &flag_p;
            ran_job.store(true, Ordering::Release);
            // (0: a failed build, nothing for `release` if it ever ran)
            0
        }), key_p);
        assert_eq!(released(target_p), 0);
        assert!(dropped_p.load(Ordering::Acquire), "preload queued job was not dropped immediately on released()");
        drop(open_pre);
        assert!(!ran_p.load(Ordering::Acquire));
        for standin in p_blockers {
            let start = Instant::now();
            while is_pending(standin) && start.elapsed() < Duration::from_secs(10) {
                std::thread::sleep(Duration::from_millis(2));
            }
            let _ = released(standin);
        }
    }

    /// A compute wait never builds on the waiting thread: a worker does, a `xcl-pso*` thread.
    #[test]
    fn a_compute_wait_never_builds_on_the_waiting_thread() {
        let standin = 0x7E57_C501usize;
        let (info_tx, info) = mpsc::channel();
        submit_lazy(standin, true, Box::new(move || {
            let _ = info_tx.send(std::thread::current().name().map(str::to_owned));
            0x7C01
        }), 0);
        assert_eq!(resolve_wait_limited(standin, Some(std::time::Duration::from_secs(20))), 0x7C01);
        let name = info.recv_timeout(std::time::Duration::from_secs(10)).expect("the job ran");
        assert!(name.is_some_and(|n| n.starts_with("xcl-pso")), "built on the waiting thread");
        let _ = released(standin);
    }

    /// A compute wait is built ahead of a long queue of graphics builds.
    #[test]
    fn a_compute_wait_goes_ahead_of_the_fifo() {
        let _sat = WORKER_SATURATION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let standins: Vec<usize> = (0..64).map(|i| 0x7E57_C510usize + i).collect();
        for &s in &standins {
            submit_lazy(s, false, Box::new(|| {
                std::thread::sleep(std::time::Duration::from_millis(20));
                0x7C10
            }), 0);
            resolve(s);
        }
        let compute = 0x7E57_C5A0usize;
        submit_lazy(compute, true, Box::new(|| 0x7C11), 0);
        let t = std::time::Instant::now();
        assert_eq!(resolve_wait_limited(compute, Some(std::time::Duration::from_secs(20))), 0x7C11);
        assert!(t.elapsed() < std::time::Duration::from_secs(1), "the compute wait queued behind the FIFO ({:?})", t.elapsed());
        for &s in &standins {
            wait_built(s);
            let _ = released(s);
        }
        let _ = released(compute);
    }

    /// A compute wait whose limit passes returns the stand-in, marks the pipeline late and does
    /// not wait again; once built, the real pipeline.
    #[test]
    fn a_compute_wait_ends_at_its_limit() {
        let standin = 0x7E57_C5B0usize;
        let (go, finish) = mpsc::channel::<()>();
        submit_lazy(standin, true, Box::new(move || {
            let _ = finish.recv_timeout(std::time::Duration::from_secs(20));
            0x7C20
        }), 0);
        let limit = Some(std::time::Duration::from_millis(50));
        let t = std::time::Instant::now();
        assert_eq!(resolve_wait_limited(standin, limit), standin, "not built within the limit: the stand-in");
        assert!(t.elapsed() >= std::time::Duration::from_millis(45) && t.elapsed() < std::time::Duration::from_secs(5), "waited {:?}", t.elapsed());
        assert!(entry(standin).unwrap().late.load(Ordering::Acquire), "the pipeline is not late");
        let t = std::time::Instant::now();
        assert_eq!(resolve_wait_limited(standin, limit), standin);
        assert!(t.elapsed() < std::time::Duration::from_millis(40), "a late pipeline is waited for again ({:?})", t.elapsed());
        go.send(()).unwrap();
        wait_built(standin);
        assert_eq!(resolve_wait_limited(standin, limit), 0x7C20, "built by a worker");
        let _ = released(standin);
    }

    /// A compute bind in frame mode waits for a worker inside the frame budget, then gets the
    /// stand-in and the pipeline is late; once built, the real one.
    #[test]
    fn a_compute_bind_waits_inside_the_frame_budget() {
        let _g = DRAW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let standin = 0x7E57_F511usize;
        let (info_tx, info) = mpsc::channel();
        let (go, finish) = mpsc::channel::<()>();
        submit_lazy(standin, true, Box::new(move || {
            let _ = info_tx.send(std::thread::current().name().map(str::to_owned));
            let _ = finish.recv_timeout(std::time::Duration::from_secs(20));
            0x7F11
        }), 0);
        let mode = ComputeWait::Frame(ms(60));
        let t = std::time::Instant::now();
        assert_eq!(resolve_wait_inner(standin, None, mode, 2_000_001), standin);
        assert!(t.elapsed() >= ms(55) && t.elapsed() < ms(400), "waited {:?}", t.elapsed());
        let name = info.recv_timeout(std::time::Duration::from_secs(10)).expect("a worker builds it");
        assert!(name.is_some_and(|n| n.starts_with("xcl-pso")), "built on the waiting thread");
        assert!(entry(standin).unwrap().late.load(Ordering::Acquire), "the wait ran out, but the pipeline is not late");
        go.send(()).unwrap();
        wait_built(standin);
        assert_eq!(resolve_wait_inner(standin, None, mode, 2_000_002), 0x7F11, "built by a worker");
        let _ = released(standin);
    }

    /// Compute binds of one frame share its compute budget: what the first spent is gone for the next.
    #[test]
    fn compute_binds_share_the_frame_budget() {
        let _g = DRAW_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (a, b) = (0x7E57_F521usize, 0x7E57_F522usize);
        let (go, finish) = mpsc::channel::<()>();
        let finish = Arc::new(Mutex::new(finish));
        for s in [a, b] {
            let finish = finish.clone();
            submit_lazy(s, true, Box::new(move || {
                let _ = finish.lock().unwrap().recv_timeout(std::time::Duration::from_secs(20));
                0x7F21
            }), 0);
        }
        let mode = ComputeWait::Frame(ms(60));
        let t = std::time::Instant::now();
        assert_eq!(resolve_wait_inner(a, None, mode, 2_000_011), a);
        assert!(t.elapsed() >= ms(55) && t.elapsed() < ms(400), "the first waited {:?}", t.elapsed());
        let t = std::time::Instant::now();
        assert_eq!(resolve_wait_inner(b, None, mode, 2_000_011), b);
        assert!(t.elapsed() < ms(15), "the second waited {:?} in a spent budget", t.elapsed());
        assert!(!entry(b).unwrap().late.load(Ordering::Acquire), "it never waited, but it is late");
        drop(go);
        wait_built(a);
        wait_built(b);
        let _ = (released(a), released(b));
    }

    #[test]
    fn compute_wait_mode_follows_the_switches() {
        let (frame, limit) = (|n| ComputeWait::Frame(ms(n)), |n| ComputeWait::Limit(Some(ms(n))));
        assert_eq!(compute_wait_from(None, None), frame(100));
        assert_eq!(compute_wait_from(Some("0"), None), ComputeWait::Limit(None));
        assert_eq!(compute_wait_from(Some("0"), Some("250")), ComputeWait::Limit(None));
        assert_eq!(compute_wait_from(Some("5000"), Some("250")), limit(5000));
        assert_eq!(compute_wait_from(None, Some("250")), frame(250));
        assert_eq!(compute_wait_from(None, Some("0")), frame(0));
        assert_eq!(compute_wait_from(Some(" 5000 "), None), limit(5000));
        assert_eq!(compute_wait_from(None, Some(" 250 ")), frame(250));
        assert_eq!(compute_wait_from(Some("soon"), Some("many")), frame(100), "values that are no numbers count as unset");
    }

    #[test]
    fn waits_are_bounded_in_frame_mode() {
        assert!(waits_bounded_for(&ComputeWait::Frame(ms(100)), false));
        assert!(waits_bounded_for(&ComputeWait::Frame(ms(0)), true));
        assert!(waits_bounded_for(&ComputeWait::Limit(None), true));
        assert!(!waits_bounded_for(&ComputeWait::Limit(None), false));
        assert!(!waits_bounded_for(&ComputeWait::Limit(Some(ms(5000))), false));
    }

    #[test]
    fn image_skip_is_on_unless_turned_off() {
        assert!(image_skip_from(None));
        assert!(image_skip_from(Some("1")));
        assert!(!image_skip_from(Some("0")));
        assert!(!image_skip_from(Some(" 0 ")));
    }
}

fn entry(pso: usize) -> Option<Arc<Pending>> {
    PENDING.lock().unwrap().as_ref()?.get(&pso).cloned()
}

impl Pending {
    /// What a bind of the stand-in `pso` gives the driver while the pipeline is not built.
    fn stand(&self, pso: usize) -> usize {
        match self.driver.load(Ordering::Acquire) {
            0 => pso,
            d => d,
        }
    }
}

/// Stand-in `standin` is no driver object (`standin::shell`): binds get `driver` in its place.
/// Told before the title has the stand-in.
pub fn shared(standin: usize, driver: usize) {
    if let Some(p) = entry(standin) {
        p.driver.store(driver, Ordering::Release);
    }
}

/// The pipeline to use for `pso` now: the real one once built, else the stand-in's driver
/// pipeline (`pso` itself when it is one).
pub fn resolve(pso: usize) -> usize {
    let Some(p) = entry(pso) else { return pso };
    p.note_use();
    match p.real.load(Ordering::Acquire) {
        0 => {
            // a lazy one is wanted now: to the workers
            let stand = p.stand(pso);
            if !p.sent.swap(true, Ordering::AcqRel) {
                enqueue(p);
            }
            stand
        }
        r => r,
    }
}

/// `resolve`, waiting for a pipeline still being built: a worker builds it, ahead of the queue
/// (a queue of a title's hundreds of pipelines must not hold up the one it binds now); this
/// thread never does.
pub fn resolve_wait(pso: usize) -> usize {
    resolve_wait_for(pso, wait_budget())
}

/// `resolve_wait`, waiting at most `budget` (None: until the pipeline is built, or a compute
/// pipeline as `compute_wait` says).
fn resolve_wait_for(pso: usize, budget: Option<std::time::Duration>) -> usize {
    resolve_wait_inner(pso, budget, compute_wait(), super::present::presents())
}

/// `resolve_wait_for` with no budget, a compute pipeline waited for at most `limit` (None: no limit).
#[cfg(test)]
fn resolve_wait_limited(pso: usize, limit: Option<std::time::Duration>) -> usize {
    resolve_wait_inner(pso, None, ComputeWait::Limit(limit), 0)
}

/// `resolve_wait_for` with no budget for frame `frame`; a compute pipeline waits as `mode` says.
fn resolve_wait_inner(pso: usize, budget: Option<std::time::Duration>, mode: ComputeWait, frame: u64) -> usize {
    let Some(p) = entry(pso) else { return pso };
    p.note_use();
    if p.spared.load(Ordering::Acquire) {
        return resolve_spared(&p, pso);
    }
    let already = *p.done.lock().unwrap();
    if let (None, true, ComputeWait::Frame(frame_budget)) = (budget, p.compute, mode) {
        let built = |p: &Pending| match p.real.load(Ordering::Acquire) {
            0 => p.stand(pso),
            r => r,
        };
        if already {
            return built(&p);
        }
        return match wait_in_frame(&p, pso, frame, frame_budget, &COMPUTE_FRAME_SPENT, None) {
            Some(true) => built(&p),
            _ => p.stand(pso),
        };
    }
    let started = std::time::Instant::now();
    // (the title's thread is held from here until the pipeline is built or the budget is over)
    let _held = (!already).then(Held::new);
    if let Some(budget) = budget {
        // a late pipeline is not waited for again: only a wait that ran is counted
        let late = p.late.load(Ordering::Acquire);
        let _open = (!already && !late).then(|| OpenWait::new(pso, p.compute, false, started));
        let r = resolve_within(&p, pso, budget);
        if !already && !late {
            note_blocked(p.compute, false, started.elapsed());
        }
        return r;
    }
    let limit = match mode {
        ComputeWait::Limit(limit) if p.compute => limit,
        _ => None,
    };
    // (its wait ran out earlier: its dispatches are skipped until a worker has built it)
    if !already && limit.is_some() && p.late.load(Ordering::Acquire) {
        return p.stand(pso);
    }
    // (no pipeline is built on this thread: a worker builds it, ahead of the queue)
    let _open = (!already).then(|| OpenWait::new(pso, p.compute, false, started));
    if !already {
        need_now(&p);
    }
    let mut done = p.done.lock().unwrap();
    let mut expired = false;
    match limit {
        Some(limit) => {
            let left = limit.saturating_sub(started.elapsed());
            let (d, t) = p.cv.wait_timeout_while(done, left, |d| !*d).unwrap();
            done = d;
            expired = t.timed_out() && !*done;
        }
        None => {
            while !*done {
                done = p.cv.wait(done).unwrap();
            }
        }
    }
    drop(done);
    if !already {
        note_blocked(p.compute, false, started.elapsed());
    }
    if expired {
        if let Some(limit) = limit {
            if !p.late.swap(true, Ordering::AcqRel) {
                let ms = limit.as_millis() as u64;
                crate::xlog_warn!("D3D12.X: a compute pipeline was not built after {ms} ms: its dispatches are skipped until it is");
                crate::report::telemetry_pipeline_wait_expired(true, ms);
            }
        }
        return p.stand(pso);
    }
    match p.real.load(Ordering::Acquire) {
        0 => p.stand(pso),
        r => r,
    }
}

/// How a compute bind waits for a pipeline a worker is building.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ComputeWait {
    /// at most this much per shown frame, all threads together, once per pipeline (`wait_in_frame`)
    Frame(std::time::Duration),
    /// at most this long per bind (None: until it is built); the pipeline is then `late`
    Limit(Option<std::time::Duration>),
}

/// The `ComputeWait` for the values of `XCL_PSO_COMPUTE_WAIT_MS` (0: no limit, n: n ms per bind)
/// and `XCL_PSO_COMPUTE_FRAME_WAIT_MS` (default 100; 0: never wait). With the first unset, a compute
/// bind waits inside the frame budget. A value that is not a number counts as unset.
fn compute_wait_from(wait_ms: Option<&str>, frame_ms: Option<&str>) -> ComputeWait {
    let ms = |v: Option<&str>| v.and_then(|v| v.trim().parse::<u64>().ok());
    match ms(wait_ms) {
        Some(0) => ComputeWait::Limit(None),
        Some(n) => ComputeWait::Limit(Some(std::time::Duration::from_millis(n))),
        None => ComputeWait::Frame(std::time::Duration::from_millis(ms(frame_ms).unwrap_or(100))),
    }
}

/// `compute_wait_from` for the environment, read once (a unit test is not affected by it: it
/// waits until built, as before frame mode).
fn compute_wait() -> ComputeWait {
    if cfg!(test) {
        return ComputeWait::Limit(None);
    }
    static W: OnceLock<ComputeWait> = OnceLock::new();
    *W.get_or_init(|| {
        compute_wait_from(std::env::var("XCL_PSO_COMPUTE_WAIT_MS").ok().as_deref(), std::env::var("XCL_PSO_COMPUTE_FRAME_WAIT_MS").ok().as_deref())
    })
}

/// Draw waits per shown frame, all threads together (`XCL_PSO_FRAME_WAIT_MS`, default 100;
/// 0: a draw never waits).
fn frame_wait_budget() -> std::time::Duration {
    static B: OnceLock<std::time::Duration> = OnceLock::new();
    *B.get_or_init(|| {
        let ms = std::env::var("XCL_PSO_FRAME_WAIT_MS").ok().and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(100);
        std::time::Duration::from_millis(ms)
    })
}

/// (frames shown when the spending began, µs spent)
static FRAME_SPENT: Mutex<(u64, u64)> = Mutex::new((0, 0));

/// the same for compute binds (`compute_wait`)
static COMPUTE_FRAME_SPENT: Mutex<(u64, u64)> = Mutex::new((0, 0));

/// What is left of the frame budget `budget` for frame `frame` in `counter` (it starts again when
/// `frame` differs from the stored one).
fn frame_allowance(counter: &Mutex<(u64, u64)>, frame: u64, budget: std::time::Duration) -> std::time::Duration {
    let mut g = counter.lock().unwrap();
    if g.0 != frame {
        *g = (frame, 0);
    }
    budget.saturating_sub(std::time::Duration::from_micros(g.1))
}

/// Adds `spent` to frame `frame` in `counter` (ignored if a newer frame has started).
fn frame_spend(counter: &Mutex<(u64, u64)>, frame: u64, spent: std::time::Duration) {
    let mut g = counter.lock().unwrap();
    if g.0 > frame {
        return;
    }
    if g.0 != frame {
        *g = (frame, 0);
    }
    g.1 = g.1.saturating_add(spent.as_micros() as u64);
}

/// A title thread needs `p` now: wanted, and ahead of the workers' FIFO.
fn need_now(p: &Arc<Pending>) {
    p.want();
    let first = {
        let mut d = p.demand.lock().unwrap();
        d.needed_at.get_or_insert_with(std::time::Instant::now);
        !std::mem::replace(&mut d.by_submit, true)
    };
    if first {
        push(lane_for(p.compute, true), Item::Build(p.clone()));
    }
    p.sent.store(true, Ordering::Release);
}

/// How much of `budget` is left of `p`'s one chance to be waited for, counted from when it was
/// first needed (all of it before that).
fn chance_left(p: &Pending, budget: std::time::Duration) -> std::time::Duration {
    match p.demand.lock().unwrap().needed_at {
        Some(at) => budget.saturating_sub(at.elapsed()),
        None => budget,
    }
}

/// A bind of `p` (not built) in frame `frame` with a frame budget of `budget` kept in `counter`:
/// its wait goes first to a worker (`need_now`, never this thread). In this order: a `late`
/// pipeline is not waited for; one whose chance (`budget` from when it was first needed) is over
/// becomes `late`; one that finds the frame's budget spent is left out for now, not late; else it
/// waits for the least of what is left of the frame's budget, its chance and `cap`, and becomes
/// `late` if that was not enough. `None`: it did not wait or was not built; `Some(done)` otherwise.
fn wait_in_frame(p: &Arc<Pending>, pso: usize, frame: u64, budget: std::time::Duration, counter: &Mutex<(u64, u64)>, cap: Option<std::time::Duration>) -> Option<bool> {
    need_now(p);
    if p.late.load(Ordering::Acquire) {
        return None;
    }
    let chance = chance_left(p, budget);
    if chance.is_zero() {
        p.late.store(true, Ordering::Release);
        return None;
    }
    let mut allowance = frame_allowance(counter, frame, budget);
    if allowance.is_zero() {
        return None;
    }
    allowance = allowance.min(chance);
    if let Some(cap) = cap {
        allowance = allowance.min(cap);
    }
    let started = std::time::Instant::now();
    let done = {
        let _held = Held::new();
        let _open = OpenWait::new(pso, p.compute, false, started);
        let (done, _) = p.cv.wait_timeout_while(p.done.lock().unwrap(), allowance, |d| !*d).unwrap();
        *done
    };
    let took = started.elapsed();
    frame_spend(counter, frame, took);
    note_blocked(p.compute, false, took);
    if !done {
        p.late.store(true, Ordering::Release);
    }
    Some(done)
}

/// The pipeline a draw or graphics ExecuteIndirect uses now, and whether it is not built (the
/// stand-in). Never builds on the calling thread: it waits for a worker only inside the frame
/// budget (`frame_wait_budget`), shared by all threads. A compute pipeline goes to `resolve_wait`.
pub fn resolve_draw(pso: usize) -> (usize, bool) {
    resolve_draw_with(pso, super::present::presents(), frame_wait_budget())
}

/// `resolve_draw` for frame `frame` with a frame budget of `budget` (what the frame's earlier
/// waits left of it is waited at most).
fn resolve_draw_with(pso: usize, frame: u64, budget: std::time::Duration) -> (usize, bool) {
    let Some(p) = entry(pso) else { return (pso, false) };
    if p.compute {
        let r = resolve_wait(pso);
        return (r, is_pending(pso));
    }
    p.note_use();
    let built = |p: &Pending| match p.real.load(Ordering::Acquire) {
        0 => (p.stand(pso), false),
        r => (r, false),
    };
    if p.real.load(Ordering::Acquire) != 0 || *p.done.lock().unwrap() {
        return built(&p);
    }
    match wait_in_frame(&p, pso, frame, budget, &FRAME_SPENT, wait_budget()) {
        Some(true) => built(&p),
        _ => (p.stand(pso), true),
    }
}

/// How long a bind waits for a pipeline still being built (`XCL_PSO_WAIT_MS`; unset or 0: until
/// it is built). A retail game builds compute programs on its own threads: the driver takes up to
/// 30 s for the biggest translated ones, and the title's watchdog ends it when its main thread
/// has been held 30 s.
fn wait_budget() -> Option<std::time::Duration> {
    static B: OnceLock<Option<std::time::Duration>> = OnceLock::new();
    *B.get_or_init(|| std::env::var("XCL_PSO_WAIT_MS").ok().and_then(|v| v.trim().parse::<u64>().ok()).filter(|&ms| ms > 0).map(std::time::Duration::from_millis))
}

/// Binds wait a bounded time (`XCL_PSO_WAIT_MS`, or the compute frame budget): a dispatch is then
/// not put off to the submit (`list_x::defer`), where it would only wait as long: it is recorded at
/// its bind, which waits inside the budget or leaves it out. Put-off to the submit remains only with
/// `XCL_PSO_COMPUTE_WAIT_MS` set and `XCL_PSO_WAIT_MS` unset.
pub fn waits_bounded() -> bool {
    waits_bounded_for(&compute_wait(), wait_budget().is_some())
}

fn waits_bounded_for(mode: &ComputeWait, wait_budget_set: bool) -> bool {
    wait_budget_set || matches!(mode, ComputeWait::Frame(_))
}

/// `resolve_wait` for at most `budget`: the build goes to the workers (not this thread), and a
/// pipeline not built by then is `late` – it and later binds of it get the stand-in (their
/// dispatches are skipped) until a worker has built it.
fn resolve_within(p: &Arc<Pending>, pso: usize, budget: std::time::Duration) -> usize {
    let built = |p: &Pending| match p.real.load(Ordering::Acquire) {
        0 => p.stand(pso),
        r => r,
    };
    if *p.done.lock().unwrap() {
        return built(p);
    }
    if !p.sent.swap(true, Ordering::AcqRel) {
        enqueue(p.clone());
    }
    if p.late.load(Ordering::Acquire) {
        return p.stand(pso);
    }
    p.want();
    let (done, _) = p.cv.wait_timeout_while(p.done.lock().unwrap(), budget, |d| !*d).unwrap();
    if *done {
        return built(p);
    }
    drop(done);
    p.late.store(true, Ordering::Release);
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    if n < 16 || n.is_power_of_two() {
        crate::xlog_warn!("D3D12.X: a pipeline still being built after {} ms: its binds go on without it until it is built ({} so far)", budget.as_millis(), n + 1);
    }
    p.stand(pso)
}

/// A bind does not wait for a big compute program that writes nothing but images
/// (`gpu::effects::big_and_image_only`); its dispatches are left out until a worker has built it.
/// On by default (`XCL_PSO_IMAGE_SKIP=0`: off): what such a program draws is missing for that long.
///
/// A retail title's world start binds one ray-tracing resolve program per material (73 – 118
/// KiB of console code, about 1 MB of DXIL each): an RDNA2 driver takes 22 – 172 s for each,
/// one bind at a time, and the title stands still for all of them. Skipping them cannot leave
/// stale counts for an `ExecuteIndirect`: they write no buffer. A bounded wait (`XCL_PSO_WAIT_MS`)
/// goes on without every late program, also the culling passes whose counts an `ExecuteIndirect`
/// reads next – a GPU hang.
pub fn image_skip() -> bool {
    static S: OnceLock<bool> = OnceLock::new();
    *S.get_or_init(|| image_skip_from(std::env::var("XCL_PSO_IMAGE_SKIP").ok().as_deref()))
}

/// `image_skip` for the value of `XCL_PSO_IMAGE_SKIP`: on unless it is `0`.
fn image_skip_from(v: Option<&str>) -> bool {
    v.map_or(true, |v| v.trim() != "0")
}

/// The compute pipeline behind stand-in `standin` writes nothing but images: binds go on
/// without it while it is built (`resolve_spared`). Told before the title has the stand-in.
pub fn image_only(standin: usize) {
    if let Some(p) = entry(standin) {
        p.spared.store(true, Ordering::Release);
    }
}

/// `resolve_wait` for a pipeline binds do not wait for (`image_only`): the real one once built,
/// else the stand-in, with the build sent to the workers. Nobody waits, so the build is not
/// `wanted`: a big one takes its turn among the driver's compiles and waits for free commit.
fn resolve_spared(p: &Arc<Pending>, pso: usize) -> usize {
    match p.real.load(Ordering::Acquire) {
        0 => {
            if !p.sent.swap(true, Ordering::AcqRel) {
                enqueue(p.clone());
            }
            if !*p.done.lock().unwrap() && !p.late.swap(true, Ordering::AcqRel) {
                static N: AtomicUsize = AtomicUsize::new(0);
                let n = N.fetch_add(1, Ordering::Relaxed);
                if n < 16 || n.is_power_of_two() {
                    crate::xlog_info!("D3D12.X: a compute program that writes only images is still being built: its dispatches are left out until it is ({} so far)", n + 1);
                }
            }
            p.stand(pso)
        }
        r => r,
    }
}

/// What `resolve_wait` cost the threads that called it – a title's, as a rule: for graphics
/// [0] and compute [1] pipelines, the waits, their total and longest µs, and those that built
/// the pipeline themselves. A retail title's render thread spent most of its frames in the
/// driver's compiler this way (on an NVIDIA GPU; its hang watchdog then ended it).
static BLOCKED: [[AtomicU64; 4]; 2] = [const { [const { AtomicU64::new(0) }; 4] }; 2];
const WAITS: usize = 0;
const TOTAL_US: usize = 1;
const LONGEST_US: usize = 2;
const BUILT_HERE: usize = 3;

/// (waits, total µs, longest µs, built by the caller) of `resolve_wait`, graphics then compute
pub fn blocked() -> [[u64; 4]; 2] {
    BLOCKED.each_ref().map(|k| k.each_ref().map(|c| c.load(Ordering::Relaxed)))
}

/// A `resolve_wait` that found its pipeline not built yet: counted; every power of two of them
/// and each wait of a second or more (the first 32) is logged.
fn note_blocked(compute: bool, built_here: bool, took: std::time::Duration) {
    let us = took.as_micros() as u64;
    let k = &BLOCKED[compute as usize];
    let n = k[WAITS].fetch_add(1, Ordering::Relaxed) + 1;
    k[TOTAL_US].fetch_add(us, Ordering::Relaxed);
    k[LONGEST_US].fetch_max(us, Ordering::Relaxed);
    if built_here {
        k[BUILT_HERE].fetch_add(1, Ordering::Relaxed);
    }
    let kind = if compute { "compute" } else { "graphics" };
    static LONG: AtomicUsize = AtomicUsize::new(0);
    if us >= 1_000_000 && LONG.fetch_add(1, Ordering::Relaxed) < 32 {
        crate::xlog_warn!(
            "D3D12.X: a {kind} pipeline held up its caller for {} ms ({})",
            us / 1000,
            if built_here { "built in the call" } else { "waited for a worker" }
        );
    }
    if n.is_power_of_two() {
        let b = blocked();
        crate::xlog_info!(
            "D3D12.X: pipelines waited for at a bind so far: {} graphics ({} ms, longest {} ms, {} built in the call), {} compute ({} ms, longest {} ms, {} built in the call)",
            b[0][WAITS], b[0][TOTAL_US] / 1000, b[0][LONGEST_US] / 1000, b[0][BUILT_HERE],
            b[1][WAITS], b[1][TOTAL_US] / 1000, b[1][LONGEST_US] / 1000, b[1][BUILT_HERE],
        );
    }
}

/// The `resolve_wait`s still going, by stand-in and serial number (two threads can wait for one
/// pipeline): `note_blocked` sees a wait only when it ends, and the one a title's hang watchdog
/// ends it in never does. `xcl-pso-waits` logs each once when it passes a second.
static OPEN: Mutex<std::collections::BTreeMap<(usize, u64), Open>> = Mutex::new(std::collections::BTreeMap::new());
/// a wait was opened (`xcl-pso-waits` sleeps while there is none)
static OPEN_CV: Condvar = Condvar::new();

/// A wait in `OPEN`: when it started, its kind, whether the caller builds the pipeline itself,
/// and whether it was logged.
#[derive(Clone, Copy)]
struct Open {
    started: std::time::Instant,
    compute: bool,
    built_here: bool,
    logged: bool,
}

/// A `resolve_wait` for a pipeline not built yet, in `OPEN` until it ends.
struct OpenWait((usize, u64));

impl OpenWait {
    fn new(pso: usize, compute: bool, built_here: bool, started: std::time::Instant) -> OpenWait {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        static WATCH: std::sync::Once = std::sync::Once::new();
        WATCH.call_once(|| {
            let _ = std::thread::Builder::new().name("xcl-pso-waits".into()).spawn(watch_open_waits);
        });
        let key = (pso, NEXT.fetch_add(1, Ordering::Relaxed));
        OPEN.lock().unwrap().insert(key, Open { started, compute, built_here, logged: false });
        OPEN_CV.notify_one();
        OpenWait(key)
    }
}

impl Drop for OpenWait {
    fn drop(&mut self) {
        OPEN.lock().unwrap().remove(&self.0);
    }
}

/// `xcl-pso-waits`: every 250 ms while a wait is open, logs the waits that passed a second
/// (the first 32).
fn watch_open_waits() {
    let mut logged = 0usize;
    loop {
        let mut open = OPEN.lock().unwrap();
        while open.is_empty() {
            open = OPEN_CV.wait(open).unwrap();
        }
        drop(open);
        std::thread::sleep(std::time::Duration::from_millis(250));
        let due = overdue(&mut OPEN.lock().unwrap(), std::time::Instant::now());
        for w in due {
            if logged >= 32 {
                return;
            }
            logged += 1;
            crate::xlog_warn!(
                "D3D12.X: a {} pipeline has held up its caller for {} ms so far ({})",
                if w.compute { "compute" } else { "graphics" },
                w.ms,
                if w.built_here { "being built in the call" } else { "waiting for a worker" }
            );
        }
    }
}

/// An open wait that passed a second: its kind, how it waits, and for how long so far.
struct Overdue {
    compute: bool,
    built_here: bool,
    ms: u128,
}

/// The waits in `open` that have run a second or more at `now` and were not logged yet (they
/// are marked logged).
fn overdue(open: &mut std::collections::BTreeMap<(usize, u64), Open>, now: std::time::Instant) -> Vec<Overdue> {
    let mut due = Vec::new();
    for w in open.values_mut() {
        let took = now.saturating_duration_since(w.started);
        if !w.logged && took >= std::time::Duration::from_secs(1) {
            w.logged = true;
            due.push(Overdue { compute: w.compute, built_here: w.built_here, ms: took.as_millis() });
        }
    }
    due
}

/// For binding: compute pipelines are waited for, graphics ones are not.
pub fn resolve_for_bind(pso: usize) -> usize {
    match entry(pso) {
        Some(p) if p.compute => resolve_wait(pso),
        Some(_) => resolve(pso),
        None => pso,
    }
}

/// The title released stand-in `pso` for good: forget it (its address may come back for
/// another object) and return its real pipeline to release (0: none, or still being built –
/// the worker's result is then released when it arrives).
pub fn released(pso: usize) -> usize {
    let Some(p) = PENDING.lock().unwrap().as_mut().and_then(|m| m.remove(&pso)) else { return 0 };
    {
        let done = p.done.lock().unwrap();
        if *done {
            return p.real.swap(0, Ordering::AcqRel);
        }
        // the worker stores its result into a Pending nobody will read: mark it orphaned
        p.orphan.store(true, Ordering::Release);
    }
    drop_unstarted_job(&p);
    0
}

/// `pso` is a stand-in whose compute pipeline's dispatches may be left out while it is built
/// (`image_only`).
pub fn is_spared(pso: usize) -> bool {
    entry(pso).is_some_and(|p| p.spared.load(Ordering::Acquire))
}

/// `pso` is a stand-in whose real pipeline is still being built.
pub fn is_pending(pso: usize) -> bool {
    entry(pso).is_some_and(|p| !*p.done.lock().unwrap())
}

/// A call with `pso` was put off to the submit, which will wait for it: its build goes before the
/// workers' queue, as a wanted one.
pub fn need_by_submit(pso: usize) {
    let Some(p) = entry(pso) else { return };
    need_now(&p);
}

/// (built in the background, failed, still building)
pub fn counts() -> (usize, usize, usize) {
    let pending = PENDING.lock().unwrap().as_ref().map_or(0, |m| m.values().filter(|p| !*p.done.lock().unwrap()).count());
    (BUILT.load(Ordering::Relaxed), FAILED.load(Ordering::Relaxed), pending)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bind that waits for a lazy compute pipeline nobody has started does not build it: a
    /// worker does, and the time it was held up is counted under its kind, with none built in
    /// the call (other tests wait too: increases are checked).
    #[test]
    fn a_wait_for_a_worker_is_counted_with_its_time_and_built_nowhere_in_the_call() {
        let standin = 0x7E57_8001usize;
        let before = blocked();
        let (name_tx, name) = mpsc::channel();
        submit_lazy(standin, true, Box::new(move || {
            let _ = name_tx.send(std::thread::current().name().map(str::to_owned));
            std::thread::sleep(std::time::Duration::from_millis(50));
            0x4401
        }), 0);
        assert_eq!(resolve_wait(standin), 0x4401);
        let after = blocked();
        let (b, a) = (before[1], after[1]);
        assert!(a[WAITS] > b[WAITS], "the compute wait is counted");
        assert_eq!(a[BUILT_HERE], b[BUILT_HERE], "nothing is built in the call");
        assert!(a[TOTAL_US] - b[TOTAL_US] >= 50_000, "its 50 ms are in the total");
        assert!(a[LONGEST_US] >= 50_000);
        let name = name.recv_timeout(std::time::Duration::from_secs(10)).expect("the job ran");
        assert!(name.is_some_and(|n| n.starts_with("xcl-pso")), "built on a thread that is not a worker");
        released(standin);
    }

    /// A bind with a wait budget (`XCL_PSO_WAIT_MS`) is counted with the time it waited, not
    /// as built in the call: the build goes to the workers (other tests wait too: increases
    /// are checked).
    #[test]
    fn a_bounded_wait_is_counted_with_the_time_it_waited() {
        let standin = 0x7E57_8002usize;
        submit(standin, true, Box::new(|| {
            std::thread::sleep(std::time::Duration::from_millis(400));
            0x4402
        }));
        let before = blocked();
        let budget = Some(std::time::Duration::from_millis(30));
        assert_eq!(resolve_wait_for(standin, budget), standin, "not built within the budget: the stand-in");
        let after = blocked();
        let (b, a) = (before[1], after[1]);
        assert!(a[WAITS] > b[WAITS], "the compute wait is counted");
        assert!(a[TOTAL_US] - b[TOTAL_US] >= 30_000, "with the budget it waited");
        let start = std::time::Instant::now();
        while is_pending(standin) && start.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let _ = released(standin);
    }

    /// A wait is in the table of open waits while it runs (here: while a worker builds the
    /// pipeline), and not after it.
    #[test]
    fn a_wait_is_open_while_it_runs() {
        let standin = 0x7E57_8003usize;
        let is_open = move || OPEN.lock().unwrap().iter().any(|(&(s, _), w)| s == standin && !w.built_here && w.compute);
        let seen = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seen_in_job = seen.clone();
        submit_lazy(standin, true, Box::new(move || {
            seen_in_job.store(is_open(), Ordering::Relaxed);
            0x4403
        }), 0);
        assert_eq!(resolve_wait_for(standin, None), 0x4403);
        assert!(seen.load(Ordering::Relaxed), "open while the pipeline was being built");
        assert!(!is_open(), "closed when the wait ended");
        released(standin);
    }

    /// An open wait is picked out once when it passes a second, not before and not again.
    #[test]
    fn an_open_wait_is_logged_once_when_it_passes_a_second() {
        let t0 = std::time::Instant::now();
        let ms = std::time::Duration::from_millis;
        let mut open = std::collections::BTreeMap::new();
        open.insert((1, 0), Open { started: t0, compute: true, built_here: true, logged: false });
        open.insert((2, 1), Open { started: t0 + ms(600), compute: false, built_here: false, logged: false });
        assert!(overdue(&mut open, t0 + ms(999)).is_empty(), "under a second: nothing");
        let due = overdue(&mut open, t0 + ms(1500));
        assert_eq!(due.len(), 1);
        assert!(due[0].compute && due[0].built_here);
        assert_eq!(due[0].ms, 1500);
        assert!(overdue(&mut open, t0 + ms(1500)).is_empty(), "logged once");
        let due = overdue(&mut open, t0 + ms(1700));
        assert_eq!(due.len(), 1, "the other one when it passes a second");
        assert!(!due[0].compute && !due[0].built_here);
        assert_eq!(due[0].ms, 1100);
    }
}
