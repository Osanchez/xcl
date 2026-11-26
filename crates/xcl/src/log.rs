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

// ---- fork telemetry (docs/guide/telemetry.md, parts 2-3: timing math and event fields) ----
type Fields = Vec<(&'static str, serde_json::Value)>;

/// `XCL_TELEMETRY_SAMPLE`: None/"" -> 1; "1".."10000" -> n; else Err (the caller warns and uses 1).
pub fn parse_sample(value: Option<&str>) -> Result<u32, String> {
    let v = value.unwrap_or("").trim();
    if v.is_empty() {
        return Ok(1);
    }
    match v.parse::<u32>() {
        Ok(n) if (1..=10_000).contains(&n) => Ok(n),
        _ => Err(format!("XCL_TELEMETRY_SAMPLE={v:?} is not a whole number from 1 to 10000")),
    }
}

/// Whether frame `i` (1-based) gets GPU stamps at sampling `n`.
pub fn sample_due(i: u64, n: u32) -> bool {
    n <= 1 || i % n as u64 == 0
}

/// Length of the union of [start, end) intervals in ms; no valid interval -> None. Sorts in place.
/// Never a sum: overlapping and nested intervals count once.
pub fn union_ms(iv: &mut [(f64, f64)]) -> Option<f64> {
    iv.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut total = 0.0;
    let mut any = false;
    let mut cur: Option<(f64, f64)> = None;
    for &(s, e) in iv.iter() {
        if !s.is_finite() || !e.is_finite() || e < s {
            continue;
        }
        any = true;
        cur = match cur {
            None => Some((s, e)),
            Some((cs, ce)) if s <= ce => Some((cs, ce.max(e))),
            Some((cs, ce)) => {
                total += ce - cs;
                Some((s, e))
            }
        };
    }
    if let Some((cs, ce)) = cur {
        total += ce - cs;
    }
    any.then_some(total)
}

/// GPU tick -> QPC through a calibration pair; None for tick 0, freq < 1000, qpf <= 0 or overflow.
/// Ticks before the calibration tick convert too (signed difference, i128 arithmetic).
pub fn tick_to_qpc(tick: u64, cal_tick: u64, cal_qpc: i64, freq: u64, qpf: i64) -> Option<i64> {
    if tick == 0 || freq < 1000 || qpf <= 0 {
        return None;
    }
    let delta = tick as i128 - cal_tick as i128;
    let scaled = delta.checked_mul(qpf as i128)? / freq as i128;
    i64::try_from((cal_qpc as i128).checked_add(scaled)?).ok()
}

/// The presenting thread's time neither presenting nor waiting on the frame clock; clamped at 0.
pub fn frame_cpu_ms(interval_ms: f64, present_ms: f64, thread_wait_ms: f64) -> f64 {
    let v = interval_ms - present_ms - thread_wait_ms;
    if v > 0.0 {
        v
    } else {
        0.0
    }
}

/// ms rounded to 3 decimals; a non-finite value becomes null.
fn ms_val(x: f64) -> serde_json::Value {
    if x.is_finite() {
        serde_json::Value::from((x * 1000.0).round() / 1000.0)
    } else {
        serde_json::Value::Null
    }
}

fn ms_opt(x: Option<f64>) -> serde_json::Value {
    x.map_or(serde_json::Value::Null, ms_val)
}

fn num_opt(x: Option<u64>) -> serde_json::Value {
    x.map_or(serde_json::Value::Null, serde_json::Value::from)
}

fn str_opt(x: Option<&str>) -> serde_json::Value {
    x.map_or(serde_json::Value::Null, serde_json::Value::from)
}

/// Per-frame facts for the `frame` event.
pub struct FrameFacts {
    pub i: u64,
    pub interval_ms: f64,
    pub cpu_ms: f64,
    pub present_ms: f64,
    pub wait_ms: f64,
    pub wait_calls: u64,
    pub gpu_ms: Option<f64>,
    pub gpu_first_ms: Option<f64>,
    pub gpu_last_ms: Option<f64>,
    pub gpu_lists_null: u32,
    pub lists: u32,
    pub draws: u64,
    pub dispatches: u64,
    pub barriers: Option<u64>,
    pub pso_wait_ms: f64,
    pub pso_wait_draw_ms: f64,
    pub pso_wait_dispatch_ms: f64,
    pub look_ms: f64,
    pub sampled: bool,
    pub why: Option<&'static str>,
}

/// Facts for the `list` event (one PC command list in one driver call).
pub struct ListFacts {
    pub frame: u64,
    pub seq: u64,
    pub id: u64,
    pub chunk: u64,
    pub queue: u32,
    pub queue_type: &'static str,
    pub start_tick: Option<u64>,
    pub end_tick: Option<u64>,
    pub start_ms: Option<f64>,
    pub end_ms: Option<f64>,
    pub cpu_us: u32,
    pub resubmitted: bool,
    pub origin: &'static str,
    pub why: Option<&'static str>,
}

/// Facts for the `memory` event; a source that failed is None.
pub struct MemoryFacts {
    pub frame: u64,
    pub committed_mb: Option<u64>,
    pub vram_local_mb: Option<u64>,
    pub vram_nonlocal_mb: Option<u64>,
    pub vram_budget_mb: Option<u64>,
    pub arenas_mb: Option<u64>,
    pub arenas_made_mb: Option<u64>,
    pub pages_mb: Option<u64>,
    pub page_pools_mb: Option<u64>,
    pub sparse_mb: Option<u64>,
    pub sparse_mapped_mb: Option<u64>,
}

/// Facts for the `pass` event (one barrier-delimited span of a stamped list).
pub struct PassFacts {
    pub frame: u64,
    pub list: u64,
    pub queue: u32,
    pub i: u32,
    pub start_tick: Option<u64>,
    pub end_tick: Option<u64>,
    pub start_ms: Option<f64>,
    pub end_ms: Option<f64>,
    pub kind: &'static str,
    pub draws: u32,
    pub dispatches: u32,
    pub indirect: u32,
    pub pipelines: Vec<u64>,
    pub pipeline_ids_truncated: bool,
    pub barriers_before: u32,
    pub resubmitted: bool,
    pub why: Option<&'static str>,
}

/// Facts for the `pipeline` creation event.
pub struct PipelineFacts {
    pub id: u64,
    pub stage: &'static str,
    pub record: Option<u64>,
    pub dxil_bytes: u64,
    pub compile_ms: f64,
    pub waited_ms: f64,
    pub source: Option<&'static str>,
    pub blob: bool,
    /// who built it: preload, extra, worker, xcl or title
    pub by: &'static str,
    /// whether one of the title's threads waited for it
    pub wanted: bool,
}

/// Facts for the `resource` creation event.
pub struct ResourceFacts {
    pub kind: &'static str,
    pub dim: Option<&'static str>,
    pub bytes: Option<u64>,
    pub ms: f64,
    pub frame: u64,
}

/// Facts for the per-frame aggregated `io` event.
pub struct IoFacts {
    pub requests: u32,
    pub bytes: u64,
    pub ms: f64,
    pub max_ms: f64,
    pub zlib: u32,
    pub frame: u64,
}

pub fn frame_fields(f: &FrameFacts) -> Fields {
    use serde_json::Value;
    let mut v: Fields = vec![
        ("i", Value::from(f.i)),
        ("interval_ms", ms_val(f.interval_ms)),
        ("cpu_ms", ms_val(f.cpu_ms)),
        ("present_ms", ms_val(f.present_ms)),
        ("wait_ms", ms_val(f.wait_ms)),
        ("wait_calls", Value::from(f.wait_calls)),
        ("gpu_ms", ms_opt(f.gpu_ms)),
        ("gpu_first_ms", ms_opt(f.gpu_first_ms)),
        ("gpu_last_ms", ms_opt(f.gpu_last_ms)),
        ("gpu_lists_null", Value::from(f.gpu_lists_null)),
        ("lists", Value::from(f.lists)),
        ("draws", Value::from(f.draws)),
        ("dispatches", Value::from(f.dispatches)),
        ("barriers", num_opt(f.barriers)),
        ("pso_wait_ms", ms_val(f.pso_wait_ms)),
        ("pso_wait_draw_ms", ms_val(f.pso_wait_draw_ms)),
        ("pso_wait_dispatch_ms", ms_val(f.pso_wait_dispatch_ms)),
        ("look_ms", ms_val(f.look_ms)),
        ("sampled", Value::from(f.sampled)),
    ];
    if let Some(w) = f.why {
        v.push(("why", Value::from(w)));
    }
    v
}

/// `translate_us` and `console_bytes` are always null (not measured, decision O-7).
pub fn list_fields(l: &ListFacts) -> Fields {
    use serde_json::Value;
    let mut v: Fields = vec![
        ("frame", Value::from(l.frame)),
        ("seq", Value::from(l.seq)),
        ("id", Value::from(l.id)),
        ("chunk", Value::from(l.chunk)),
        ("queue", Value::from(l.queue)),
        ("queue_type", Value::from(l.queue_type)),
        ("start_tick", num_opt(l.start_tick)),
        ("end_tick", num_opt(l.end_tick)),
        ("start_ms", ms_opt(l.start_ms)),
        ("end_ms", ms_opt(l.end_ms)),
        ("cpu_us", Value::from(l.cpu_us)),
        ("translate_us", Value::Null),
        ("console_bytes", Value::Null),
        ("resubmitted", Value::from(l.resubmitted)),
        ("origin", Value::from(l.origin)),
    ];
    if let Some(w) = l.why {
        v.push(("why", Value::from(w)));
    }
    v
}

/// `frequency` is null when absent or below 1000 Hz.
pub fn clock_fields(queue: u32, kind: &str, gpu_tick: u64, cpu_qpc: i64, freq: Option<u64>, qpf: i64) -> Fields {
    use serde_json::Value;
    vec![
        ("queue", Value::from(queue)),
        ("queue_type", Value::from(kind)),
        ("gpu_tick", Value::from(gpu_tick)),
        ("cpu_qpc", Value::from(cpu_qpc)),
        ("frequency", num_opt(freq.filter(|&f| f >= 1000))),
        ("qpc_frequency", Value::from(qpf)),
    ]
}

pub fn memory_fields(m: &MemoryFacts) -> Fields {
    use serde_json::Value;
    vec![
        ("frame", Value::from(m.frame)),
        ("committed_mb", num_opt(m.committed_mb)),
        ("vram_local_mb", num_opt(m.vram_local_mb)),
        ("vram_nonlocal_mb", num_opt(m.vram_nonlocal_mb)),
        ("vram_budget_mb", num_opt(m.vram_budget_mb)),
        ("arenas_mb", num_opt(m.arenas_mb)),
        ("arenas_made_mb", num_opt(m.arenas_made_mb)),
        ("pages_mb", num_opt(m.pages_mb)),
        ("page_pools_mb", num_opt(m.page_pools_mb)),
        ("sparse_mb", num_opt(m.sparse_mb)),
        ("sparse_mapped_mb", num_opt(m.sparse_mapped_mb)),
    ]
}

/// `pipelines` is an array of plain numbers; `pipeline_ids_truncated` is present only when true.
pub fn pass_fields(p: &PassFacts) -> Fields {
    use serde_json::Value;
    let mut v: Fields = vec![
        ("frame", Value::from(p.frame)),
        ("list", Value::from(p.list)),
        ("queue", Value::from(p.queue)),
        ("i", Value::from(p.i)),
        ("start_tick", num_opt(p.start_tick)),
        ("end_tick", num_opt(p.end_tick)),
        ("start_ms", ms_opt(p.start_ms)),
        ("end_ms", ms_opt(p.end_ms)),
        ("kind", Value::from(p.kind)),
        ("draws", Value::from(p.draws)),
        ("dispatches", Value::from(p.dispatches)),
        ("indirect", Value::from(p.indirect)),
        ("pipelines", Value::from(p.pipelines.clone())),
    ];
    if p.pipeline_ids_truncated {
        v.push(("pipeline_ids_truncated", Value::from(true)));
    }
    v.push(("barriers_before", Value::from(p.barriers_before)));
    v.push(("resubmitted", Value::from(p.resubmitted)));
    if let Some(w) = p.why {
        v.push(("why", Value::from(w)));
    }
    v
}

/// `records` holds the console record hash as 16 lowercase hex digits when known, else is empty.
pub fn pipeline_fields_built(p: &PipelineFacts) -> Fields {
    use serde_json::Value;
    let records: Vec<Value> = p.record.map(|r| Value::from(format!("{r:016x}"))).into_iter().collect();
    vec![
        ("id", Value::from(p.id)),
        ("stage", Value::from(p.stage)),
        ("records", Value::Array(records)),
        ("dxil_bytes", Value::from(p.dxil_bytes)),
        ("compile_ms", ms_val(p.compile_ms)),
        ("waited_ms", ms_val(p.waited_ms)),
        ("source", str_opt(p.source)),
        ("blob", Value::from(p.blob)),
        ("by", Value::from(p.by)),
        ("wanted", Value::from(p.wanted)),
    ]
}

/// The console-slot hooks are title-only, so `where` is always `title` and `reason` `explicit`.
pub fn resource_fields(r: &ResourceFacts) -> Fields {
    use serde_json::Value;
    vec![
        ("kind", Value::from(r.kind)),
        ("dim", str_opt(r.dim)),
        ("bytes", num_opt(r.bytes)),
        ("where", Value::from("title")),
        ("reason", Value::from("explicit")),
        ("ms", ms_val(r.ms)),
        ("frame", Value::from(r.frame)),
    ]
}

pub fn io_fields(i: &IoFacts) -> Fields {
    use serde_json::Value;
    vec![
        ("requests", Value::from(i.requests)),
        ("bytes", Value::from(i.bytes)),
        ("ms", ms_val(i.ms)),
        ("max_ms", ms_val(i.max_ms)),
        ("zlib", Value::from(i.zlib)),
        ("frame", Value::from(i.frame)),
    ]
}

#[cfg(test)]
mod timing_tests {
    use super::*;
    use serde_json::Value;

    /// The event line the writer would produce, parsed back.
    fn line(ev: &str, f: &[(&'static str, Value)]) -> serde_json::Map<String, Value> {
        let text = format!("{{\"t\":1.000,{}}}", event_body(ev, f));
        assert!(!text.contains('\n'), "{text:?}");
        match serde_json::from_str::<Value>(&text).unwrap() {
            Value::Object(o) => o,
            other => panic!("not an object: {other}"),
        }
    }

    fn close(a: Option<f64>, b: f64) {
        assert!((a.unwrap() - b).abs() < 1e-9, "{a:?} != {b}");
    }

    #[test]
    fn union_counts_overlap_once_and_never_sums() {
        close(union_ms(&mut [(0.0, 4.0), (2.0, 6.0)]), 6.0);
        close(union_ms(&mut [(0.0, 10.0), (2.0, 3.0), (4.0, 5.0)]), 10.0);
        close(union_ms(&mut [(0.0, 1.0), (2.0, 3.5)]), 2.5);
        close(union_ms(&mut [(5.0, 6.0), (0.0, 1.0), (0.5, 2.0)]), 3.0);
        close(union_ms(&mut [(0.0, 1.0), (1.0, 2.0)]), 2.0);
        close(union_ms(&mut [(3.0, 3.0)]), 0.0);
        assert_eq!(union_ms(&mut []), None);
        assert_eq!(union_ms(&mut [(f64::NAN, 1.0)]), None);
    }

    #[test]
    fn ticks_convert_at_any_frequency() {
        let qpf = 10_000_000i64;
        // 1 GHz: 1e9 ticks = 1 s = qpf counts.
        assert_eq!(tick_to_qpc(2_000_000_000, 1_000_000_000, 500, 1_000_000_000, qpf), Some(500 + qpf));
        // 25 MHz.
        assert_eq!(tick_to_qpc(50_000_000, 25_000_000, 0, 25_000_000, qpf), Some(qpf));
        // 100 MHz.
        assert_eq!(tick_to_qpc(300_000_000, 100_000_000, 1_000, 100_000_000, qpf), Some(1_000 + 2 * qpf));
        // Before the calibration tick.
        assert_eq!(tick_to_qpc(1_000_000_000, 2_000_000_000, qpf, 1_000_000_000, qpf), Some(0));
    }

    #[test]
    fn tick_conversion_refuses_what_it_cannot_do() {
        assert_eq!(tick_to_qpc(0, 5, 5, 1_000_000_000, 10_000_000), None);
        assert_eq!(tick_to_qpc(10, 5, 5, 999, 10_000_000), None);
        assert_eq!(tick_to_qpc(10, 5, 5, 0, 10_000_000), None);
        assert_eq!(tick_to_qpc(10, 5, 5, 1_000_000_000, 0), None);
        assert_eq!(tick_to_qpc(10, 5, 5, 1_000_000_000, -1), None);
        assert!(tick_to_qpc(10, 5, 5, 1000, 10_000_000).is_some());
        // Overflow of i64.
        assert_eq!(tick_to_qpc(u64::MAX, 1, i64::MAX, 1000, i64::MAX), None);
        // Huge but representable: no panic, exact.
        assert_eq!(tick_to_qpc(u64::MAX, u64::MAX - 1_000_000_000, 7, 1_000_000_000, 10_000_000), Some(7 + 10_000_000));
        assert_eq!(tick_to_qpc(1, u64::MAX, 0, 1000, 1), Some(-18_446_744_073_709_551));
    }

    #[test]
    fn sample_parsing_and_due_frames() {
        assert_eq!(parse_sample(None), Ok(1));
        assert_eq!(parse_sample(Some("")), Ok(1));
        assert_eq!(parse_sample(Some(" 4 ")), Ok(4));
        assert_eq!(parse_sample(Some("1")), Ok(1));
        assert_eq!(parse_sample(Some("10000")), Ok(10_000));
        for bad in ["0", "10001", "-1", "abc", "1.5", "99999999999"] {
            assert!(parse_sample(Some(bad)).is_err(), "{bad:?}");
        }
        assert!((1..=5).all(|i| sample_due(i, 1)));
        let due: Vec<u64> = (1..=10).filter(|&i| sample_due(i, 4)).collect();
        assert_eq!(due, [4, 8]);
        assert!(sample_due(7, 0));
    }

    #[test]
    fn cpu_ms_is_clamped_at_zero() {
        assert_eq!(frame_cpu_ms(16.0, 4.0, 2.0), 10.0);
        assert_eq!(frame_cpu_ms(5.0, 4.0, 4.0), 0.0);
        assert_eq!(frame_cpu_ms(0.0, 0.0, 0.0), 0.0);
    }

    fn frame() -> FrameFacts {
        FrameFacts {
            i: 3,
            interval_ms: 16.66666,
            cpu_ms: 5.0,
            present_ms: 1.0,
            wait_ms: 2.0,
            wait_calls: 1,
            gpu_ms: Some(7.12345),
            gpu_first_ms: Some(100.0),
            gpu_last_ms: Some(108.0),
            gpu_lists_null: 0,
            lists: 4,
            draws: 10,
            dispatches: 2,
            barriers: None,
            pso_wait_ms: 0.0,
            pso_wait_draw_ms: 0.0,
            pso_wait_dispatch_ms: 0.0,
            look_ms: 0.25,
            sampled: true,
            why: None,
        }
    }

    #[test]
    fn frame_fields_serialise_flat_with_nulls() {
        let o = line("frame", &frame_fields(&frame()));
        assert_eq!(o["ev"], "frame");
        assert_eq!(o["i"], 3);
        assert_eq!(o["interval_ms"], 16.667);
        assert_eq!(o["gpu_ms"], 7.123);
        assert!(o["barriers"].is_null());
        assert_eq!(o["sampled"], true);
        assert!(!o.contains_key("why"));
        assert_eq!(o["pso_wait_draw_ms"], 0.0);
        assert_eq!(o["pso_wait_dispatch_ms"], 0.0);
        let keys: Vec<&str> = o.keys().map(|k| k.as_str()).collect();
        assert!(keys.contains(&"wait_calls") && keys.contains(&"look_ms"));
        let mut f = frame();
        f.gpu_ms = None;
        f.gpu_first_ms = None;
        f.gpu_last_ms = None;
        f.barriers = Some(9);
        f.why = Some("late");
        f.wait_ms = f64::NAN;
        let o = line("frame", &frame_fields(&f));
        assert!(o["gpu_ms"].is_null() && o["gpu_first_ms"].is_null() && o["gpu_last_ms"].is_null());
        assert!(o["wait_ms"].is_null());
        assert_eq!(o["barriers"], 9);
        assert_eq!(o["why"], "late");
        // the two PSO waits are flat keys, null-safe, and pso_wait_ms is their sum
        let mut f = frame();
        f.pso_wait_draw_ms = 1.25;
        f.pso_wait_dispatch_ms = 0.5;
        f.pso_wait_ms = f.pso_wait_draw_ms + f.pso_wait_dispatch_ms;
        let o = line("frame", &frame_fields(&f));
        assert_eq!(o["pso_wait_draw_ms"], 1.25);
        assert_eq!(o["pso_wait_dispatch_ms"], 0.5);
        assert_eq!(o["pso_wait_ms"].as_f64().unwrap(), o["pso_wait_draw_ms"].as_f64().unwrap() + o["pso_wait_dispatch_ms"].as_f64().unwrap());
        f.pso_wait_draw_ms = f64::NAN;
        f.pso_wait_dispatch_ms = f64::INFINITY;
        let o = line("frame", &frame_fields(&f));
        assert!(o["pso_wait_draw_ms"].is_null() && o["pso_wait_dispatch_ms"].is_null());
    }

    #[test]
    fn list_fields_always_carry_null_translate_and_console_bytes() {
        let mut l = ListFacts {
            frame: 2,
            seq: 9,
            id: 0x1234,
            chunk: 5,
            queue: 1,
            queue_type: "direct",
            start_tick: Some(100),
            end_tick: Some(200),
            start_ms: Some(1.23456),
            end_ms: Some(2.0),
            cpu_us: 40,
            resubmitted: false,
            origin: "submit",
            why: None,
        };
        let o = line("list", &list_fields(&l));
        assert!(o["translate_us"].is_null() && o["console_bytes"].is_null());
        assert_eq!(o["start_tick"], 100);
        assert_eq!(o["start_ms"], 1.235);
        assert_eq!(o["queue_type"], "direct");
        assert_eq!(o["origin"], "submit");
        assert!(!o.contains_key("why"));
        l.start_tick = None;
        l.end_tick = None;
        l.start_ms = None;
        l.end_ms = None;
        l.why = Some("no_frequency");
        let o = line("list", &list_fields(&l));
        assert!(o["start_tick"].is_null() && o["end_tick"].is_null() && o["start_ms"].is_null() && o["end_ms"].is_null());
        assert_eq!(o["why"], "no_frequency");
        assert!(o["translate_us"].is_null());
    }

    #[test]
    fn clock_fields_name_the_calibration() {
        let o = line("clock", &clock_fields(2, "compute", 1234, -5, Some(25_000_000), 10_000_000));
        assert_eq!(o["queue"], 2);
        assert_eq!(o["queue_type"], "compute");
        assert_eq!(o["gpu_tick"], 1234);
        assert_eq!(o["cpu_qpc"], -5);
        assert_eq!(o["frequency"], 25_000_000);
        assert_eq!(o["qpc_frequency"], 10_000_000);
        assert!(line("clock", &clock_fields(0, "copy", 1, 1, None, 1))["frequency"].is_null());
        assert!(line("clock", &clock_fields(0, "copy", 1, 1, Some(999), 1))["frequency"].is_null());
    }

    #[test]
    fn memory_fields_keep_failed_sources_null() {
        let m = MemoryFacts {
            frame: 60,
            committed_mb: Some(4096),
            vram_local_mb: Some(1000),
            vram_nonlocal_mb: None,
            vram_budget_mb: Some(8000),
            arenas_mb: Some(1),
            arenas_made_mb: Some(2),
            pages_mb: None,
            page_pools_mb: Some(3),
            sparse_mb: Some(4),
            sparse_mapped_mb: None,
        };
        let o = line("memory", &memory_fields(&m));
        assert_eq!(o["frame"], 60);
        assert_eq!(o["committed_mb"], 4096);
        assert!(o["vram_nonlocal_mb"].is_null() && o["pages_mb"].is_null() && o["sparse_mapped_mb"].is_null());
        assert_eq!(o["page_pools_mb"], 3);
        assert_eq!(o.len(), 13); // t, ev, frame and the ten sources
    }

    #[test]
    fn pass_fields_flag_truncation_only_when_true() {
        let mut p = PassFacts {
            frame: 1,
            list: 7,
            queue: 0,
            i: 2,
            start_tick: Some(10),
            end_tick: Some(20),
            start_ms: Some(0.5),
            end_ms: Some(0.75),
            kind: "draw",
            draws: 3,
            dispatches: 0,
            indirect: 1,
            pipelines: vec![4, 5],
            pipeline_ids_truncated: false,
            barriers_before: 6,
            resubmitted: false,
            why: None,
        };
        let o = line("pass", &pass_fields(&p));
        assert_eq!(o["pipelines"], serde_json::json!([4, 5]));
        assert!(!o.contains_key("pipeline_ids_truncated") && !o.contains_key("why"));
        assert_eq!(o["kind"], "draw");
        assert_eq!(o["barriers_before"], 6);
        p.pipeline_ids_truncated = true;
        p.resubmitted = true;
        p.start_tick = None;
        p.end_tick = None;
        p.start_ms = None;
        p.end_ms = None;
        p.why = Some("resubmitted");
        let o = line("pass", &pass_fields(&p));
        assert_eq!(o["pipeline_ids_truncated"], true);
        assert!(o["start_tick"].is_null() && o["end_ms"].is_null());
        assert_eq!(o["why"], "resubmitted");
        assert_eq!(o["resubmitted"], true);
    }

    #[test]
    fn pipeline_fields_hex_the_record_hash() {
        let mut p = PipelineFacts { id: 3, stage: "graphics", record: Some(0xabc), dxil_bytes: 100, compile_ms: 1.23456, waited_ms: 0.0, source: Some("dxc"), blob: true, by: "worker", wanted: true };
        let o = line("pipeline", &pipeline_fields_built(&p));
        assert_eq!(o["records"], serde_json::json!(["0000000000000abc"]));
        assert_eq!(o["compile_ms"], 1.235);
        assert_eq!(o["source"], "dxc");
        assert_eq!(o["blob"], true);
        assert_eq!(o["by"], "worker");
        assert_eq!(o["wanted"], true);
        p.by = "title";
        p.wanted = false;
        let o = line("pipeline", &pipeline_fields_built(&p));
        assert_eq!(o["by"], "title");
        assert_eq!(o["wanted"], false);
        p.record = None;
        p.source = None;
        let o = line("pipeline", &pipeline_fields_built(&p));
        assert_eq!(o["records"], serde_json::json!([]));
        assert!(o["source"].is_null());
    }

    #[test]
    fn resource_fields_say_title_and_explicit() {
        let r = ResourceFacts { kind: "placed", dim: Some("texture"), bytes: None, ms: 0.5, frame: 12 };
        let o = line("resource", &resource_fields(&r));
        assert_eq!(o["where"], "title");
        assert_eq!(o["reason"], "explicit");
        assert!(o["bytes"].is_null());
        assert_eq!(o["dim"], "texture");
        assert_eq!(o["frame"], 12);
        let r = ResourceFacts { kind: "reserved", dim: None, bytes: Some(65536), ms: 0.0, frame: 1 };
        let o = line("resource", &resource_fields(&r));
        assert!(o["dim"].is_null());
        assert_eq!(o["bytes"], 65536);
    }

    #[test]
    fn io_fields_are_plain_numbers() {
        let i = IoFacts { requests: 3, bytes: 1 << 40, ms: 2.0004, max_ms: 1.5, zlib: 1, frame: 8 };
        let o = line("io", &io_fields(&i));
        assert_eq!(o["requests"], 3);
        assert_eq!(o["bytes"], 1u64 << 40);
        assert_eq!(o["ms"], 2.0);
        assert_eq!(o["max_ms"], 1.5);
        assert_eq!(o["zlib"], 1);
        assert_eq!(o["frame"], 8);
        assert!(o.values().all(|v| !v.is_object() && !v.is_array()));
    }
}
