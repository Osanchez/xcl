//! Process-wide logger. The file handle is opened once at startup (before any file
//! hooks are installed) so logging never re-enters the VFS.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum Level {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
    Trace = 4,
}

impl Level {
    pub fn parse(s: &str) -> Level {
        match s.to_ascii_lowercase().as_str() {
            "error" => Level::Error,
            "warn" | "warning" => Level::Warn,
            "debug" => Level::Debug,
            "trace" => Level::Trace,
            _ => Level::Info,
        }
    }
    fn tag(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn => "WARN ",
            Level::Info => "INFO ",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        }
    }
}

static FILE: Mutex<Option<File>> = Mutex::new(None);
static LEVEL: AtomicU8 = AtomicU8::new(Level::Info as u8);
static ECHO: AtomicBool = AtomicBool::new(true);
static WRITE_FAILED: AtomicBool = AtomicBool::new(false);
static START: OnceLock<Instant> = OnceLock::new();

fn report_write_result(
    reported: &AtomicBool,
    result: io::Result<()>,
    report: impl FnOnce(&io::Error),
) {
    if let Err(error) = result {
        if !reported.swap(true, Ordering::Relaxed) {
            report(&error);
        }
    }
}

fn report_log_write_error(error: &io::Error) {
    let _ = writeln!(std::io::stderr().lock(), "xcl: cannot write to the log: {error}");
}

fn flush_log_file_with(
    file: &mut impl Write,
    reported: &AtomicBool,
    report: impl FnOnce(&io::Error),
) {
    report_write_result(reported, file.flush(), report);
}

fn write_log_entry_with(
    file: &mut impl Write,
    line: &[u8],
    flush: bool,
    reported: &AtomicBool,
    mut report: impl FnMut(&io::Error),
) {
    report_write_result(reported, file.write_all(line), |error| report(error));
    if flush {
        flush_log_file_with(file, reported, |error| report(error));
    }
}

fn write_log_entry(file: &mut impl Write, line: &[u8], flush: bool) {
    write_log_entry_with(file, line, flush, &WRITE_FAILED, report_log_write_error);
}

fn flush_log_file(file: &mut impl Write) {
    flush_log_file_with(file, &WRITE_FAILED, report_log_write_error);
}

/// Seconds since the log started (the host's start) – the times log lines carry.
pub fn uptime() -> f64 {
    START.get_or_init(Instant::now).elapsed().as_secs_f64()
}

pub fn init(path: Option<&Path>, level: Level, echo_stderr: bool) {
    START.get_or_init(Instant::now);
    LEVEL.store(level as u8, Ordering::Relaxed);
    ECHO.store(echo_stderr, Ordering::Relaxed);
    if let Some(p) = path {
        // a log that cannot be opened is said once on stderr: `xlog_*` would go nowhere, and the
        // console has already been given its path (#2277); the run goes on without it
        match open(p) {
            Ok(f) => {
                if let Ok(mut g) = FILE.lock() {
                    *g = Some(f);
                }
            }
            Err(e) => eprintln!("xcl: {e}"),
        }
    }
}

/// The log file at `p`, its folder made first; the error names the path and the OS error.
fn open(p: &Path) -> Result<File, String> {
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create the log folder {}: {e}", dir.display()))?;
    }
    File::create(p).map_err(|e| format!("cannot open the log {}: {e}", p.display()))
}

pub fn enabled(level: Level) -> bool {
    level as u8 <= LEVEL.load(Ordering::Relaxed)
}

pub fn write(level: Level, args: std::fmt::Arguments<'_>) {
    if !enabled(level) {
        return;
    }
    let t = START.get_or_init(Instant::now).elapsed().as_secs_f64();
    let tid = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
    let line = format!("[{t:9.3}][{tid:5}][{}] {args}\n", level.tag());
    if let Ok(mut g) = FILE.try_lock().or_else(|_| FILE.lock()) {
        if let Some(f) = g.as_mut() {
            write_log_entry(f, line.as_bytes(), level <= Level::Warn);
        }
    }
    if ECHO.load(Ordering::Relaxed) && level <= Level::Info {
        let _ = std::io::stderr().write_all(format!("xcl: {args}\n").as_bytes());
    }
}

pub fn flush() {
    if let Ok(mut g) = FILE.lock() {
        if let Some(f) = g.as_mut() {
            flush_log_file(f);
        }
    }
}

/// A panic goes to the log as well as to stderr.
pub fn log_panics() {
    let print = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        write(Level::Error, format_args!("thread '{}' {}", std::thread::current().name().unwrap_or(""), info.to_string().replace('\n', " ")));
        print(info);
    }));
}

#[macro_export]
macro_rules! xlog_error { ($($a:tt)*) => { $crate::log::write($crate::log::Level::Error, format_args!($($a)*)) } }
#[macro_export]
macro_rules! xlog_warn { ($($a:tt)*) => { $crate::log::write($crate::log::Level::Warn, format_args!($($a)*)) } }
#[macro_export]
macro_rules! xlog_info { ($($a:tt)*) => { $crate::log::write($crate::log::Level::Info, format_args!($($a)*)) } }
#[macro_export]
macro_rules! xlog_debug { ($($a:tt)*) => { if $crate::log::enabled($crate::log::Level::Debug) { $crate::log::write($crate::log::Level::Debug, format_args!($($a)*)) } } }
#[macro_export]
macro_rules! xlog_trace { ($($a:tt)*) => { if $crate::log::enabled($crate::log::Level::Trace) { $crate::log::write($crate::log::Level::Trace, format_args!($($a)*)) } } }

#[cfg(test)]
mod tests {
    use super::*;

    struct FaultWriter {
        fail_write: bool,
        fail_flush: bool,
        writes: usize,
        flushes: usize,
    }

    impl Write for FaultWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes += 1;
            if self.fail_write {
                Err(io::Error::other("write failed"))
            } else {
                Ok(bytes.len())
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            if self.fail_flush {
                Err(io::Error::other("flush failed"))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn log_write_failures_are_reported_once() {
        let reported = AtomicBool::new(false);
        let mut messages = Vec::new();
        let mut file = FaultWriter { fail_write: true, fail_flush: true, writes: 0, flushes: 0 };
        write_log_entry_with(
            &mut file,
            b"log entry",
            true,
            &reported,
            |error| messages.push(error.to_string()),
        );

        assert_eq!(file.writes, 1);
        assert_eq!(file.flushes, 1, "flush is still attempted after a write failure");
        assert_eq!(messages, vec![String::from("write failed")]);
    }

    #[test]
    fn a_log_flush_failure_is_reported() {
        let reported = AtomicBool::new(false);
        let mut messages = Vec::new();
        let mut file = FaultWriter { fail_write: false, fail_flush: true, writes: 0, flushes: 0 };

        flush_log_file_with(&mut file, &reported, |error| messages.push(error.to_string()));

        assert_eq!(file.flushes, 1);
        assert_eq!(messages, vec![String::from("flush failed")]);
    }

    #[test]
    fn a_panic_is_in_the_log() {
        let path = std::env::temp_dir().join(format!("xcl-log-panics-{}.log", std::process::id()));
        *FILE.lock().unwrap() = Some(File::create(&path).unwrap());
        log_panics();
        let died = std::thread::Builder::new().name("xcl-dies".into()).spawn(|| panic!("gone for the run")).unwrap().join();
        *FILE.lock().unwrap() = None;
        let log = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(died.is_err());
        assert!(log.contains("thread 'xcl-dies' panicked at ") && log.contains("gone for the run"), "the log: {log:?}");
    }

    #[test]
    fn a_log_that_cannot_be_opened_names_its_path() {
        let dir = std::env::temp_dir().join(format!("xcl-log-open-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // a file where the log folder should be: neither the folder nor the log can be made
        let blocker = dir.join("not-a-folder");
        std::fs::write(&blocker, "x").unwrap();
        let bad = blocker.join("logs").join("xcl.log");
        let e = open(&bad).unwrap_err();
        assert!(e.starts_with("cannot create the log folder ") && e.contains("not-a-folder"), "{e}");
        let good = dir.join("logs").join("xcl.log");
        assert!(open(&good).is_ok());
        assert!(good.is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ---- fork telemetry (docs/guide/telemetry.md, part 1) ----
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;

/// How much telemetry a run writes. Each tier includes the ones before it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum Tier {
    Off = 0,
    Frame = 1,
    Pass = 2,
    All = 3,
}

impl Tier {
    pub fn name(self) -> &'static str {
        match self {
            Tier::Off => "off",
            Tier::Frame => "frame",
            Tier::Pass => "pass",
            Tier::All => "all",
        }
    }
}

/// `XCL_TELEMETRY`: None / "" / "off" -> Off; "frame" | "pass" | "all" (trimmed, any case);
/// anything else -> Err(message) – the caller warns and stays off.
pub fn parse_tier(value: Option<&str>) -> Result<Tier, String> {
    let v = match value {
        Some(v) => v.trim().to_ascii_lowercase(),
        None => return Ok(Tier::Off),
    };
    match v.as_str() {
        "" | "off" => Ok(Tier::Off),
        "frame" => Ok(Tier::Frame),
        "pass" => Ok(Tier::Pass),
        "all" => Ok(Tier::All),
        _ => Err(format!("XCL_TELEMETRY={value:?} is not one of off, frame, pass, all", value = value.unwrap_or(""))),
    }
}

static TIER: AtomicU8 = AtomicU8::new(0);
static EVENTS: OnceLock<Events> = OnceLock::new();
static RUN_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Whether `current` writes events of `wanted` (pure; `telemetry_on` with TIER).
fn tier_allows(current: u8, wanted: Tier) -> bool {
    current != 0 && wanted as u8 <= current
}

/// One relaxed load while off.
#[inline]
pub fn telemetry_on(wanted: Tier) -> bool {
    tier_allows(TIER.load(Ordering::Relaxed), wanted)
}

/// The run directory, once telemetry is open.
pub fn run_dir() -> Option<&'static Path> {
    RUN_DIR.get().map(|p| p.as_path())
}

/// `<root>/run-<utc>-<commit12|unknown>`, made with create_dir (not _all for the leaf); an existing
/// one gets `-2`, `-3` ... (two runs in one second). Creates `root` if missing.
pub fn create_run_dir(root: &Path, utc: &str, commit: Option<&str>) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(root)?;
    let short: String = commit.unwrap_or("unknown").chars().take(12).collect();
    let base = format!("run-{utc}-{short}");
    for i in 1..=100u32 {
        let dir = if i == 1 { root.join(&base) } else { root.join(format!("{base}-{i}")) };
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, format!("{base}: 100 run directories already exist")))
}

/// Opens `<dir>/events.jsonl` and turns telemetry on at `tier` (TIER stored after EVENTS is set).
/// Off: opens nothing, returns false. A second call: false, nothing changed.
pub fn telemetry_open(dir: &Path, tier: Tier) -> bool {
    if tier == Tier::Off || EVENTS.get().is_some() {
        return false;
    }
    let events = match Events::create(&dir.join("events.jsonl")) {
        Ok(e) => e,
        Err(e) => {
            xlog_warn!("telemetry: cannot open events.jsonl in {}: {e} – telemetry off", dir.display());
            return false;
        }
    };
    if EVENTS.set(events).is_err() {
        return false;
    }
    let _ = RUN_DIR.set(dir.to_path_buf());
    TIER.store(tier as u8, Ordering::Release);
    true
}

/// The event file: one unbuffered `File` behind a mutex, plus counters.
pub struct Events {
    file: Mutex<File>,
    written: AtomicU64,
    dropped: AtomicU64,
    failed: AtomicBool,
}

impl Events {
    pub fn create(path: &Path) -> std::io::Result<Events> {
        Ok(Events { file: Mutex::new(File::create(path)?), written: AtomicU64::new(0), dropped: AtomicU64::new(0), failed: AtomicBool::new(false) })
    }

    /// Lines written so far.
    pub fn written(&self) -> u64 {
        self.written.load(Ordering::Relaxed)
    }

    /// Events lost to a lock wait over 100 ms or a failed write.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// `{"t":<ms>,` + body + `}\n` in one write_all; `t` (ms since `START`, 3 decimals) taken under the
    /// lock; lock waited <= 100 ms, else the event is dropped and counted.
    pub fn write(&self, ev: &str, fields: &[(&str, serde_json::Value)]) {
        let body = event_body(ev, fields);
        let waited = Instant::now();
        let mut guard = loop {
            match self.file.try_lock() {
                Ok(g) => break g,
                Err(std::sync::TryLockError::Poisoned(p)) => break p.into_inner(),
                Err(std::sync::TryLockError::WouldBlock) => {
                    if waited.elapsed() >= std::time::Duration::from_millis(100) {
                        self.dropped.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
        };
        let t = START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0;
        let line = format!("{{\"t\":{t:.3},{body}}}\n");
        let res = guard.write_all(line.as_bytes());
        drop(guard);
        match res {
            Ok(()) => {
                self.written.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                if !self.failed.swap(true, Ordering::Relaxed) {
                    xlog_warn!("telemetry: writing events.jsonl failed: {e} – events are dropped");
                }
            }
        }
    }
}

/// `"ev":"<ev>","k":v,...` – keys are static identifiers, values serialised by serde_json (escapes
/// quotes, backslashes, control characters: one physical line guaranteed).
fn event_body(ev: &str, fields: &[(&str, serde_json::Value)]) -> String {
    let mut s = format!("\"ev\":{}", serde_json::Value::from(ev));
    for (k, v) in fields {
        s.push_str(&format!(",\"{k}\":{v}"));
    }
    s
}

/// Writes the event if `tier` is on (no-op otherwise).
pub fn event(tier: Tier, ev: &str, fields: &[(&str, serde_json::Value)]) {
    if !telemetry_on(tier) {
        return;
    }
    if let Some(e) = EVENTS.get() {
        e.write(ev, fields);
    }
}

/// Events dropped so far (0 when telemetry is off), for the manifest.
pub fn events_dropped() -> u64 {
    EVENTS.get().map_or(0, |e| e.dropped())
}

/// Volume limit per event kind: increments `counter`; Some(n) (1-based) for n <= 100 and every
/// 1000th after, None otherwise. Every emitted event carries `n`, so totals stay readable.
pub fn sampled(counter: &AtomicU64) -> Option<u64> {
    let n = counter.fetch_add(1, Ordering::Relaxed) + 1;
    if n <= 100 || n % 1000 == 0 {
        Some(n)
    } else {
        None
    }
}

/// (`20261125T180411Z`, `2026-11-25T18:04:11Z`) for `now` in UTC – no chrono (days-to-civil).
pub fn utc_stamps(now: std::time::SystemTime) -> (String, String) {
    let secs = now.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    let (hh, mm, ss) = (rem / 3600, rem % 3600 / 60, rem % 60);
    (format!("{y:04}{m:02}{d:02}T{hh:02}{mm:02}{ss:02}Z"), format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z"))
}

/// Days since 1970-01-01 -> (year, month 1-12, day 1-31), proleptic Gregorian.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

#[cfg(test)]
mod telemetry_tests {
    use super::*;
    use serde_json::{json, Value};
    use std::sync::atomic::AtomicUsize;
    use std::time::{Duration, UNIX_EPOCH};

    /// A fresh empty folder under the temp dir, unique per call.
    fn temp_folder(name: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!("xcl-telemetry-{name}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn tier_parsing_takes_four_names_and_refuses_the_rest() {
        for off in [None, Some(""), Some("off"), Some(" OFF ")] {
            assert_eq!(parse_tier(off), Ok(Tier::Off), "{off:?}");
        }
        assert_eq!(parse_tier(Some("frame")), Ok(Tier::Frame));
        assert_eq!(parse_tier(Some("Pass")), Ok(Tier::Pass));
        assert_eq!(parse_tier(Some("all")), Ok(Tier::All));
        for bad in ["on", "1", "verbose", "frames"] {
            assert!(parse_tier(Some(bad)).is_err(), "{bad:?}");
        }
        assert_eq!(Tier::Pass.name(), "pass");
    }

    #[test]
    fn tiers_are_cumulative() {
        assert!(!tier_allows(0, Tier::Frame));
        assert!(tier_allows(1, Tier::Frame));
        assert!(!tier_allows(1, Tier::Pass));
        assert!(tier_allows(3, Tier::Pass));
    }

    #[test]
    fn an_event_is_one_flat_json_line() {
        let fields = [("a", json!("quote \" back \\ nl \n fffd \u{FFFD}")), ("n", json!(7)), ("ok", json!(true)), ("none", Value::Null)];
        let body = event_body("probe", &fields);
        assert!(body.starts_with("\"ev\":\"probe\""), "{body}");
        let line = format!("{{\"t\":1.500,{body}}}");
        assert!(line.starts_with("{\"t\":"));
        assert!(!line.contains('\n'), "{line:?}");
        let v: Value = serde_json::from_str(&line).unwrap();
        let o = v.as_object().unwrap();
        assert_eq!(o["ev"], "probe");
        assert_eq!(o["a"], "quote \" back \\ nl \n fffd \u{FFFD}");
        assert!(o.values().all(|x| !x.is_object() && !x.is_array()));
    }

    #[test]
    fn the_writer_flushes_each_line() {
        let dir = temp_folder("flush");
        let path = dir.join("events.jsonl");
        let ev = Events::create(&path).unwrap();
        for i in 0..3 {
            ev.write("tick", &[("i", json!(i))]);
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text:?}");
        for (i, l) in lines.iter().enumerate() {
            let v: Value = serde_json::from_str(l).unwrap();
            assert_eq!(v["ev"], "tick");
            assert_eq!(v["i"], i as u64);
            assert!(v["t"].is_number());
        }
        assert_eq!((ev.written(), ev.dropped()), (3, 0));
        drop(ev);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_writers_keep_lines_whole_and_in_time_order() {
        let dir = temp_folder("threads");
        let path = dir.join("events.jsonl");
        let ev = std::sync::Arc::new(Events::create(&path).unwrap());
        let handles: Vec<_> = (0..8)
            .map(|w| {
                let ev = ev.clone();
                std::thread::spawn(move || {
                    for i in 0..200 {
                        ev.write("burst", &[("w", json!(w)), ("i", json!(i))]);
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let mut last = f64::MIN;
        let mut count = 0;
        for l in text.lines() {
            let v: Value = serde_json::from_str(l).unwrap();
            let t = v["t"].as_f64().unwrap();
            assert!(t >= last, "t went back: {t} after {last}");
            last = t;
            count += 1;
        }
        assert_eq!(count, 1600);
        assert_eq!((ev.written(), ev.dropped()), (1600, 0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_run_directory_is_made_once_per_run() {
        let root = temp_folder("rundir");
        let a = create_run_dir(&root, "20261125T180411Z", Some("0123456789abcdef0123")).unwrap();
        let b = create_run_dir(&root, "20261125T180411Z", Some("0123456789abcdef0123")).unwrap();
        let c = create_run_dir(&root, "20261125T180412Z", None).unwrap();
        assert_eq!(a.file_name().unwrap(), "run-20261125T180411Z-0123456789ab");
        assert_eq!(b.file_name().unwrap(), "run-20261125T180411Z-0123456789ab-2");
        assert_eq!(c.file_name().unwrap(), "run-20261125T180412Z-unknown");
        for d in [&a, &b, &c] {
            assert!(d.is_dir());
            assert!(!d.file_name().unwrap().to_string_lossy().contains(':'));
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn off_opens_no_file() {
        let dir = temp_folder("off");
        assert!(!telemetry_open(&dir, Tier::Off));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn utc_stamps_match_known_dates() {
        let at = |s: u64| utc_stamps(UNIX_EPOCH + Duration::from_secs(s));
        assert_eq!(at(0), ("19700101T000000Z".to_string(), "1970-01-01T00:00:00Z".to_string()));
        assert_eq!(at(1_835_438_400), ("20280229T120000Z".to_string(), "2028-02-29T12:00:00Z".to_string()));
        assert_eq!(at(1_795_629_851), ("20261125T180411Z".to_string(), "2026-11-25T18:04:11Z".to_string()));
    }

    #[test]
    fn sampling_keeps_the_first_hundred_then_every_thousandth() {
        let c = AtomicU64::new(0);
        let kept: Vec<u64> = (0..3000).filter_map(|_| sampled(&c)).collect();
        assert_eq!(kept.len(), 103);
        assert_eq!(&kept[..100], &(1..=100).collect::<Vec<u64>>()[..]);
        assert_eq!(&kept[100..], &[1000, 2000, 3000]);
    }
}
