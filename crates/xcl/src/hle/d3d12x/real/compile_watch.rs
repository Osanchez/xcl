//! Driver compiles past their time: a soft limit logs, a hard one abandons the build (its waiters
//! go on, its slot's count is freed, its worker is replaced); an XL one also quarantines its key.
//!
//! The driver call cannot be cancelled, so a hard limit does not stop it: it takes the build out of
//! everyone's way. Every driver compile is registered here when it starts (`enter`) and removed
//! when the call returns (`Watched`'s drop, on every path). One thread, `xcl-pso-watch`, looks at
//! the registered compiles every 250 ms and sleeps on a condition variable while there is none.
//!
//! Lock order, the only one: `TABLE`, then a gate's `Slots.n` (`release_count`) or a pipeline's
//! `done` (`BuildHandle::abandon`), then `QUARANTINE`. Nothing that holds one of those takes
//! `TABLE` (`enter` and `Watched::drop` are called with none held), and the pool's lock is not
//! taken under `TABLE` (`worker_abandoned` only counts and may start a thread). Logging and
//! telemetry run after `TABLE` is released.
use super::{release_count, SizeClass, Slots};
use crate::hle::d3d12x::pso_async::{self, BuildHandle};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

/// When a driver compile of a class is logged (`soft`) and abandoned (`hard`), and whether an
/// abandoned one also quarantines its shader key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Limits {
    pub soft: Duration,
    pub hard: Duration,
    pub quarantine: bool,
}

/// The limits of a driver compile of `class`, from just before the driver call to its return
/// (the gate's wait is not counted). Measured cold builds were long: graphics M 14 - 21 s,
/// compute S up to 22.6 s, compute L up to 374 s.
pub(crate) fn limits(class: SizeClass, compute: bool) -> Limits {
    let secs = |soft: u64, hard: u64, quarantine: bool| Limits { soft: Duration::from_secs(soft), hard: Duration::from_secs(hard), quarantine };
    match (class, compute) {
        (SizeClass::S | SizeClass::M, false) => secs(2, 60, false),
        (SizeClass::S | SizeClass::M, true) => secs(5, 120, false),
        (SizeClass::L, _) => secs(60, 450, false),
        (SizeClass::Xl, _) => secs(60, 300, true),
    }
}

/// `XCL_PSO_HARD_TIMEOUTS` (`0`: off, anything else or unset: on).
fn hard_timeouts_from(v: Option<&str>) -> bool {
    v.map_or(true, |v| v.trim() != "0")
}

/// Hard timeouts are on (`XCL_PSO_HARD_TIMEOUTS=0`: soft limits only log), read once.
pub(crate) fn hard_timeouts_on() -> bool {
    static V: OnceLock<bool> = OnceLock::new();
    *V.get_or_init(|| hard_timeouts_from(std::env::var("XCL_PSO_HARD_TIMEOUTS").ok().as_deref()))
}

/// What gives a build's slot count back at its hard limit (`release_count`): the gate, what the
/// slot holds, and the flag the build's `SlotGuard` shares, so the count goes back once.
#[derive(Clone)]
pub(super) struct SlotRelease {
    pub slots: &'static Slots,
    pub preload: bool,
    pub large: bool,
    pub released: Arc<AtomicBool>,
}

/// A driver compile in progress.
struct Entry {
    started: Instant,
    class: SizeClass,
    compute: bool,
    bytes: usize,
    record: Option<u64>,
    /// the pipeline being built (`None`: no queued pipeline, a title thread's own build)
    build: Option<BuildHandle>,
    /// on a general pool worker (an abandoned one is replaced)
    is_worker: bool,
    soft_done: bool,
    /// abandoned: `Watched::drop` tells the pool when the call returns
    hard_done: bool,
    slot: Option<SlotRelease>,
}

static TABLE: Mutex<BTreeMap<u64, Entry>> = Mutex::new(BTreeMap::new());
/// a compile was registered in an empty table (`monitor` sleeps while there is none)
static CV: Condvar = Condvar::new();

/// (a poisoned lock is still a consistent table: nothing panics under it)
fn table() -> MutexGuard<'static, BTreeMap<u64, Entry>> {
    TABLE.lock().unwrap_or_else(|e| e.into_inner())
}

/// A driver compile in `TABLE` until it is dropped (on any path, a panic included).
pub(super) struct Watched(u64);

/// Register a driver compile of `bytes` of DXIL that starts now on this thread (`record`: the
/// console record's hash; `slot`: the gate slot it holds, if it waited at the gate).
pub(super) fn enter(class: SizeClass, compute: bool, bytes: usize, record: Option<u64>, slot: Option<SlotRelease>) -> Watched {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    static START: std::sync::Once = std::sync::Once::new();
    START.call_once(|| {
        let _ = std::thread::Builder::new().name("xcl-pso-watch".into()).spawn(monitor);
    });
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let entry = Entry {
        started: Instant::now(),
        class,
        compute,
        bytes,
        record,
        build: pso_async::current_build(),
        is_worker: pso_async::is_worker(),
        soft_done: false,
        hard_done: false,
        slot,
    };
    let mut entries = table();
    let was_empty = entries.is_empty();
    entries.insert(id, entry);
    drop(entries);
    if was_empty {
        CV.notify_one();
    }
    Watched(id)
}

impl Drop for Watched {
    fn drop(&mut self) {
        let left = leave(&mut table(), self.0);
        // (after the table's lock: an abandoned compile's worker counts as available again; the
        // count went up under the lock, before this entry could leave)
        if let Some(e) = left {
            if e.hard_done {
                pso_async::worker_returned(e.is_worker);
            }
        }
    }
}

/// Remove entry `id`: its state at the end of its compile.
fn leave(entries: &mut BTreeMap<u64, Entry>, id: u64) -> Option<Entry> {
    entries.remove(&id)
}

/// A limit an entry has passed, by its id in the table.
#[derive(Debug, PartialEq)]
enum Due {
    Soft(u64),
    Hard(u64),
}

/// The entries that passed a limit at `now` by `limits`, each once (they are marked). A hard limit
/// is reported only with `hard_on`, and replaces the soft one when both are passed at once.
fn due_with(entries: &mut BTreeMap<u64, Entry>, now: Instant, hard_on: bool, limits: &dyn Fn(SizeClass, bool) -> Limits) -> Vec<Due> {
    let mut due = Vec::new();
    for (&id, e) in entries.iter_mut() {
        let l = limits(e.class, e.compute);
        let took = now.saturating_duration_since(e.started);
        if hard_on && !e.hard_done && took >= l.hard {
            e.hard_done = true;
            e.soft_done = true;
            due.push(Due::Hard(id));
        } else if !e.soft_done && took >= l.soft {
            e.soft_done = true;
            due.push(Due::Soft(id));
        }
    }
    due
}

/// What to log and tell telemetry about an entry that passed a limit.
struct Report {
    hard: bool,
    compute: bool,
    class: SizeClass,
    bytes: usize,
    record: Option<u64>,
    /// the limit passed
    limit: Duration,
    /// how long the compile has run
    ms: u64,
    quarantined: bool,
}

/// Do what the limits passed at `now` ask, under the table's lock (`entries`): a hard limit gives
/// the build's slot count back (its commit stays reserved till the call returns), abandons its
/// pipeline and counts its worker as abandoned. Returns what to report once the lock is released.
fn act(entries: &mut BTreeMap<u64, Entry>, now: Instant, hard_on: bool, limits: &dyn Fn(SizeClass, bool) -> Limits) -> Vec<Report> {
    let mut reports = Vec::new();
    for d in due_with(entries, now, hard_on, limits) {
        let (id, hard) = match d {
            Due::Soft(id) => (id, false),
            Due::Hard(id) => (id, true),
        };
        let Some(e) = entries.get(&id) else { continue };
        let l = limits(e.class, e.compute);
        let mut quarantined = false;
        if hard {
            if let Some(s) = &e.slot {
                release_count(s.slots, s.preload, s.large, &s.released);
            }
            if let Some(b) = &e.build {
                quarantined = b.abandon(l.quarantine);
            }
            pso_async::worker_abandoned(e.is_worker);
        }
        reports.push(Report {
            hard,
            compute: e.compute,
            class: e.class,
            bytes: e.bytes,
            record: e.record,
            limit: if hard { l.hard } else { l.soft },
            ms: now.saturating_duration_since(e.started).as_millis() as u64,
            quarantined,
        });
    }
    reports
}

/// Log a report and tell telemetry.
fn emit(r: &Report) {
    static SOFT: AtomicU64 = AtomicU64::new(0);
    static HARD: AtomicU64 = AtomicU64::new(0);
    let kind = if r.compute { "compute" } else { "graphics" };
    let class = r.class.name();
    let record = r.record.map_or_else(|| "no record".to_string(), |h| format!("record {h:016x}"));
    if r.hard {
        // (the first 32)
        if HARD.fetch_add(1, Ordering::Relaxed) < 32 {
            crate::xlog_warn!(
                "D3D12.X: a {class} {kind} driver compile ({} B, {record}) is past its hard limit of {} s ({} ms so far): its calls go on without it and its worker is replaced{}",
                r.bytes,
                r.limit.as_secs(),
                r.ms,
                if r.quarantined { ", its key is quarantined" } else { "" }
            );
        }
    } else {
        // (the first 32, then powers of two)
        let n = SOFT.fetch_add(1, Ordering::Relaxed) + 1;
        if n <= 32 || n.is_power_of_two() {
            crate::xlog_info!("D3D12.X: a {class} {kind} driver compile ({} B, {record}) has run {} ms", r.bytes, r.ms);
        }
    }
    crate::report::telemetry_compile_timeout(r.hard, r.compute, class, r.bytes, r.record, r.ms, r.quarantined);
}

/// How often `monitor` looks at the registered compiles.
const TICK: Duration = Duration::from_millis(250);

/// `xcl-pso-watch`: asleep while no driver compile is registered; else every `TICK`, acts on the
/// compiles that passed a limit.
fn monitor() {
    let hard_on = hard_timeouts_on();
    let mut entries = table();
    loop {
        while entries.is_empty() {
            entries = CV.wait(entries).unwrap_or_else(|e| e.into_inner());
        }
        entries = CV.wait_timeout(entries, TICK).unwrap_or_else(|e| e.into_inner()).0;
        let reports = act(&mut entries, Instant::now(), hard_on, &limits);
        if !reports.is_empty() {
            drop(entries);
            reports.iter().for_each(emit);
            entries = table();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hle::d3d12x::pso_async::Unbuilt;
    use super::super::{big_compile_turn, Reserved};
    use std::sync::atomic::AtomicUsize;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// Limits of a few milliseconds, so no test waits for a real timeout; XL quarantines.
    fn tiny(class: SizeClass, _compute: bool) -> Limits {
        Limits { soft: ms(10), hard: ms(30), quarantine: class == SizeClass::Xl }
    }

    fn entry(class: SizeClass, compute: bool, build: Option<BuildHandle>, slot: Option<SlotRelease>, started: Instant) -> Entry {
        Entry { started, class, compute, bytes: 1 << 20, record: None, build, is_worker: false, soft_done: false, hard_done: false, slot }
    }

    #[test]
    fn limits_follow_class_and_kind() {
        let secs = |soft: u64, hard: u64, quarantine: bool| Limits { soft: Duration::from_secs(soft), hard: Duration::from_secs(hard), quarantine };
        assert_eq!(limits(SizeClass::S, false), secs(2, 60, false));
        assert_eq!(limits(SizeClass::M, false), secs(2, 60, false));
        assert_eq!(limits(SizeClass::S, true), secs(5, 120, false));
        assert_eq!(limits(SizeClass::M, true), secs(5, 120, false));
        assert_eq!(limits(SizeClass::L, false), secs(60, 450, false));
        assert_eq!(limits(SizeClass::L, true), secs(60, 450, false));
        assert_eq!(limits(SizeClass::Xl, false), secs(60, 300, true));
        assert_eq!(limits(SizeClass::Xl, true), secs(60, 300, true));
    }

    #[test]
    fn hard_timeouts_are_on_unless_zero() {
        assert!(hard_timeouts_from(None));
        assert!(hard_timeouts_from(Some("1")));
        assert!(hard_timeouts_from(Some("")));
        assert!(!hard_timeouts_from(Some("0")));
        assert!(!hard_timeouts_from(Some(" 0 ")));
    }

    #[test]
    fn due_gives_soft_then_hard_once() {
        let t0 = Instant::now();
        let at = |s: f64| t0 + Duration::from_secs_f64(s);
        // (the limits of the table above)
        let due = |e: &mut BTreeMap<u64, Entry>, now: Instant, hard_on: bool| due_with(e, now, hard_on, &limits);
        let one = || BTreeMap::from([(1u64, entry(SizeClass::L, true, None, None, t0))]);
        let mut e = one();
        assert!(due(&mut e, at(59.0), true).is_empty());
        assert_eq!(due(&mut e, at(61.0), true), vec![Due::Soft(1)]);
        assert!(due(&mut e, at(61.5), true).is_empty());
        assert_eq!(due(&mut e, at(451.0), true), vec![Due::Hard(1)]);
        assert!(due(&mut e, at(460.0), true).is_empty());
        // with hard timeouts off only the soft limit is ever reported
        let mut e = one();
        assert!(due(&mut e, at(59.0), false).is_empty());
        assert_eq!(due(&mut e, at(61.0), false), vec![Due::Soft(1)]);
        assert!(due(&mut e, at(451.0), false).is_empty());
        assert!(due(&mut e, at(9999.0), false).is_empty());
        // a compile first seen past its hard limit is reported once, as hard
        let mut e = one();
        assert_eq!(due(&mut e, at(500.0), true), vec![Due::Hard(1)]);
        assert!(due(&mut e, at(501.0), true).is_empty());
    }

    /// At its hard limit a build's slot count goes back and its commit stays reserved till its
    /// guard drops; the guard's drop then gives nothing back a second time.
    #[test]
    fn an_abandoned_build_gives_its_slot_back_and_keeps_its_reservation() {
        static RESERVED: Reserved = Reserved(Mutex::new(0));
        static S: Slots = Slots { n: Mutex::new(0), cv: Condvar::new(), most: 1, wanted_most: 1, reserved: &RESERVED, preloads: AtomicUsize::new(0), wanted_waiting: AtomicUsize::new(0), large: AtomicUsize::new(0) };
        let guard = big_compile_turn(&S, 100, |_| true, || false, true, false);
        assert_eq!((*S.n.lock().unwrap(), *RESERVED.0.lock().unwrap(), S.preloads.load(Ordering::SeqCst)), (1, 100, 1));
        let slot = SlotRelease { slots: &S, preload: true, large: false, released: guard.4.clone() };
        let t0 = Instant::now();
        let mut entries = BTreeMap::from([(1u64, entry(SizeClass::M, true, None, Some(slot), t0))]);

        assert!(act(&mut entries, t0 + ms(5), true, &tiny).is_empty());
        let soft = act(&mut entries, t0 + ms(15), true, &tiny);
        assert_eq!(soft.len(), 1);
        assert!(!soft[0].hard && !soft[0].quarantined);
        assert_eq!(*S.n.lock().unwrap(), 1, "a soft limit gives nothing back");

        let hard = act(&mut entries, t0 + ms(40), true, &tiny);
        assert_eq!(hard.len(), 1);
        assert!(hard[0].hard && !hard[0].quarantined && hard[0].limit == ms(30));
        assert_eq!(*S.n.lock().unwrap(), 0, "the count went back");
        assert_eq!(S.preloads.load(Ordering::SeqCst), 0);
        assert_eq!(*RESERVED.0.lock().unwrap(), 100, "the commit stays reserved");
        assert!(act(&mut entries, t0 + ms(80), true, &tiny).is_empty(), "a hard limit is acted on once");
        assert_eq!(*S.n.lock().unwrap(), 0);

        // the lane goes on: a second build is admitted while the first is in the driver
        let second = big_compile_turn(&S, 100, |_| true, || false, false, false);
        assert_eq!((*S.n.lock().unwrap(), *RESERVED.0.lock().unwrap()), (1, 200));
        drop(guard);
        assert_eq!((*S.n.lock().unwrap(), *RESERVED.0.lock().unwrap()), (1, 100), "the first guard gave its commit back and its count a second time");
        drop(second);
        assert_eq!((*S.n.lock().unwrap(), *RESERVED.0.lock().unwrap()), (0, 0));

        // the entry says it was abandoned when it leaves, once
        assert!(leave(&mut entries, 1).is_some_and(|e| e.hard_done));
        assert!(leave(&mut entries, 1).is_none());
    }

    /// With hard timeouts off nothing is given back or abandoned, however long the compile runs.
    #[test]
    fn nothing_is_abandoned_with_hard_timeouts_off() {
        static RESERVED: Reserved = Reserved(Mutex::new(0));
        static S: Slots = Slots { n: Mutex::new(0), cv: Condvar::new(), most: 1, wanted_most: 1, reserved: &RESERVED, preloads: AtomicUsize::new(0), wanted_waiting: AtomicUsize::new(0), large: AtomicUsize::new(0) };
        let guard = big_compile_turn(&S, 0, |_| true, || false, false, false);
        let slot = SlotRelease { slots: &S, preload: false, large: false, released: guard.4.clone() };
        let t0 = Instant::now();
        let mut entries = BTreeMap::from([(1u64, entry(SizeClass::M, true, None, Some(slot), t0))]);
        let reports = act(&mut entries, t0 + ms(500), false, &tiny);
        assert_eq!(reports.len(), 1);
        assert!(!reports[0].hard);
        assert_eq!(*S.n.lock().unwrap(), 1);
        assert!(!leave(&mut entries, 1).unwrap().hard_done);
        drop(guard);
        assert_eq!(*S.n.lock().unwrap(), 0);
    }

    /// An XL build past its hard limit quarantines its key; an L one does not, nor does one
    /// with no key. Each pipeline is abandoned.
    #[test]
    fn an_xl_build_past_its_hard_limit_quarantines_its_key() {
        let (xl, l, no_key) = (0x5C0A_0001usize, 0x5C0A_0002usize, 0x5C0A_0003usize);
        let (xl_key, l_key) = (0x5C0A_A001u64, 0x5C0A_A002u64);
        // (lazy and never sent: these jobs never run)
        pso_async::submit_lazy(xl, true, Box::new(|| 0), xl_key);
        pso_async::submit_lazy(l, true, Box::new(|| 0), l_key);
        pso_async::submit_lazy(no_key, true, Box::new(|| 0), 0);
        let handle = |standin| pso_async::build_handle_for(standin).expect("a pending pipeline");
        let t0 = Instant::now();
        let mut entries = BTreeMap::from([
            (1u64, entry(SizeClass::Xl, true, Some(handle(xl)), None, t0)),
            (2, entry(SizeClass::L, true, Some(handle(l)), None, t0)),
            (3, entry(SizeClass::Xl, true, Some(handle(no_key)), None, t0)),
        ]);
        assert!(!pso_async::is_quarantined(xl_key));
        let reports = act(&mut entries, t0 + ms(40), true, &tiny);
        assert_eq!(reports.iter().map(|r| (r.hard, r.quarantined)).collect::<Vec<_>>(), vec![(true, true), (true, false), (true, false)]);
        assert!(pso_async::is_quarantined(xl_key));
        assert!(!pso_async::is_quarantined(l_key));
        assert!(!pso_async::is_quarantined(0));
        for standin in [xl, l, no_key] {
            assert_eq!(pso_async::why_unbuilt(standin), Some(Unbuilt::Abandoned), "{standin:#x}");
        }
        for standin in [xl, l, no_key] {
            pso_async::released(standin);
        }
    }

    /// A compile is in the table from `enter` to its guard's drop.
    #[test]
    fn a_watched_compile_leaves_the_table_when_its_guard_drops() {
        let w = enter(SizeClass::M, false, 300_000, Some(7), None);
        let id = w.0;
        assert!(table().contains_key(&id));
        drop(w);
        assert!(!table().contains_key(&id));
        // (this thread is no worker and nothing was abandoned: no replacement was counted)
    }
}
