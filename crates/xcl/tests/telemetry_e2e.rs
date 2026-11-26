//! Fork-only end-to-end test of the frame telemetry (docs/guide/telemetry.md, part 2): runs the
//! test title's frame scenario (`--frames N`: direct list with draws, an empty compute list,
//! DMA copy, PresentX, one list submitted twice) with `XCL_TELEMETRY=frame` and checks the files
//! the run leaves in `XCL_TELEMETRY_DIR` against the event schemas.
//!
//! Needs the test title and DLL built first:  cargo build --workspace
//! Without a real D3D12 backend (no GPU) the tests print `SKIPPED ...` and pass.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const FRAMES: u64 = 120;

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap().flatten() {
        let p = e.path();
        let t = dst.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &t);
        } else {
            std::fs::copy(&p, &t).unwrap();
        }
    }
}

struct TempDir(PathBuf);

impl std::ops::Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(prefix: &str) -> TempDir {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{prefix}-{}-{nanos}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    TempDir(path)
}

fn skipped(test: &str, reason: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr(), "SKIPPED telemetry_e2e::{test}: {reason}");
}

struct Run {
    _root: TempDir,
    telemetry: PathBuf,
    stdout: String,
    code: Option<i32>,
    report: Value,
    /// `xcl.log` of the run
    log: String,
}

/// How a run of the title is started: its arguments, extra environment variables, and whether the
/// frame clock is uncapped.
struct Setup<'a> {
    args: &'a [&'a str],
    env: &'a [(&'a str, &'a str)],
    uncapped: bool,
}

/// Runs the title's frame scenario through `xcl run`. `None` when the machine has no real D3D12
/// backend (the report says so), after printing the skip line.
fn run_scenario(test: &str, tier: Option<&str>) -> Option<Run> {
    let frames = FRAMES.to_string();
    run_title(test, tier, &Setup { args: &["--frames", &frames, "--uncapped"], env: &[], uncapped: true })
}

/// The title's normal run, with the extra environment variables. Its `argv` check wants the
/// argument `--hello`, as in e2e.rs.
fn run_normal(test: &str, tier: Option<&str>, env: &[(&str, &str)]) -> Option<Run> {
    run_title(test, tier, &Setup { args: &["--hello"], env, uncapped: false })
}

/// A frame scenario of `frames` frames at the frame clock's normal pace: long enough to cross
/// several 2 s activity ticks and the schedule of a phase mark.
fn run_paced(test: &str, tier: Option<&str>, frames: u64, env: &[(&str, &str)]) -> Option<Run> {
    let frames = frames.to_string();
    run_title(test, tier, &Setup { args: &["--frames", &frames], env, uncapped: false })
}

fn run_title(test: &str, tier: Option<&str>, setup: &Setup) -> Option<Run> {
    let xcl = PathBuf::from(env!("CARGO_BIN_EXE_xcl"));
    let bin_dir = xcl.parent().unwrap();
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = temp_dir("telemetry-e2e");
    let (game, data, telemetry) = (root.join("game"), root.join("data"), root.join("telemetry"));
    std::fs::create_dir_all(&telemetry).unwrap();
    copy_dir(&manifest.join("..").join("..").join("tests").join("data").join("testgame"), &game);
    for f in ["xcl_testtitle.exe", "testgame_mw.dll"] {
        std::fs::copy(bin_dir.join(f), game.join(f)).unwrap_or_else(|e| panic!("{f}: {e} – build first: cargo build --workspace"));
    }
    let mut cmd = Command::new(&xcl);
    cmd.arg("run").arg(&game).arg("--").args(setup.args).env("XCL_DATA_ROOT", &data).env("XCL_TELEMETRY_DIR", &telemetry).env_remove("XCL_TELEMETRY_SAMPLE");
    // none of the switches under test may leak in from the caller's environment
    for k in ["XCL_HEADLESS", "XCL_PRESENT_SKIP", "XCL_WINDOW_SIZE", "XCL_ACTIVITY", "XCL_PHASE_MARK", "XCL_PHASE_FILE", "XCL_RUN_SECONDS"] {
        cmd.env_remove(k);
    }
    if setup.uncapped {
        // the title's `--uncapped`: the frame clock hands out tokens without waiting (frame.rs)
        cmd.env("XCL_FRAME_INTERVAL_US", "uncapped");
    } else {
        cmd.env_remove("XCL_FRAME_INTERVAL_US");
    }
    for (k, v) in setup.env {
        cmd.env(k, v);
    }
    match tier {
        Some(t) => cmd.env("XCL_TELEMETRY", t),
        None => cmd.env_remove("XCL_TELEMETRY"),
    };
    let out = cmd.output().expect("spawn xcl");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    println!("--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}");
    let report_path = data.join("XCL.TestTitle").join("logs").join("report.json");
    let report: Value = serde_json::from_str(
        &std::fs::read_to_string(&report_path).unwrap_or_else(|e| panic!("{}: {e}\n{stdout}\n{stderr}", report_path.display())),
    )
    .unwrap();
    if report["d3d12"]["backend"] != "real" {
        skipped(test, &format!("no real D3D12 backend (d3d12: {})", report["d3d12"]));
        return None;
    }
    let log = std::fs::read_to_string(data.join("XCL.TestTitle").join("logs").join("xcl.log")).unwrap_or_default();
    Some(Run { _root: root, telemetry, stdout, code: out.status.code(), report, log })
}

/// The run's one folder: its manifest and its events (every line checked to be an object with `t` and `ev`).
fn read_run(run: &Run) -> (Value, Vec<Value>) {
    let dirs: Vec<PathBuf> = std::fs::read_dir(&run.telemetry).unwrap().flatten().map(|e| e.path()).collect();
    assert_eq!(dirs.len(), 1, "run folders: {dirs:?}");
    let manifest: Value = serde_json::from_str(&std::fs::read_to_string(dirs[0].join("manifest.json")).expect("manifest.json")).expect("manifest.json is JSON");
    let text = std::fs::read_to_string(dirs[0].join("events.jsonl")).expect("events.jsonl");
    let events: Vec<Value> = text
        .lines()
        .enumerate()
        .map(|(n, l)| serde_json::from_str::<Value>(l).unwrap_or_else(|e| panic!("events.jsonl line {}: {e}: {l}", n + 1)))
        .collect();
    for (n, v) in events.iter().enumerate() {
        assert!(v.is_object() && v["t"].is_number() && v["ev"].is_string(), "events.jsonl line {}: {v}", n + 1);
    }
    (manifest, events)
}

/// The title ran to its end: exit code 0 and its closing line.
fn assert_title_done(run: &Run) {
    assert_eq!(run.code, Some(0), "exit code {:?}", run.code);
    assert!(run.stdout.contains("xcl_testtitle: done, failures: 0"), "the test title did not run to the end");
}

fn num(v: &Value, key: &str) -> f64 {
    v[key].as_f64().unwrap_or_else(|| panic!("`{key}` is not a number in {v}"))
}

/// The events of one kind with their position in the file.
fn of<'a>(events: &'a [Value], ev: &str) -> Vec<(usize, &'a Value)> {
    events.iter().enumerate().filter(|(_, v)| v["ev"] == ev).collect()
}

/// Length of the union of `[start, end)` intervals.
fn union_ms(mut iv: Vec<(f64, f64)>) -> f64 {
    iv.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (mut total, mut cur): (f64, Option<(f64, f64)>) = (0.0, None);
    for (s, e) in iv {
        match cur {
            Some((cs, ce)) if s <= ce => cur = Some((cs, ce.max(e))),
            Some((cs, ce)) => {
                total += ce - cs;
                cur = Some((s, e));
            }
            None => cur = Some((s, e)),
        }
    }
    if let Some((cs, ce)) = cur {
        total += ce - cs;
    }
    total
}

#[test]
fn telemetry_frame_events_match_the_run() {
    let Some(run) = run_scenario("telemetry_frame_events_match_the_run", Some("frame")) else { return };

    // the title ran to the end and printed its own timing line
    assert_eq!(run.code, Some(0), "exit code {:?}", run.code);
    assert!(
        run.stdout.lines().any(|l| l.starts_with("frames: ") && l.contains("elapsed_ms: ") && l.contains("p50_ms: ") && l.contains("p95_ms: ")),
        "no `frames:` line in the title's output"
    );
    let fails: Vec<&str> = run.stdout.lines().filter(|l| l.starts_with("[FAIL]")).collect();
    assert!(fails.is_empty(), "failed checks in the test title:\n{}", fails.join("\n"));

    // exactly one run folder, with a manifest of tier frame
    let dirs: Vec<PathBuf> = std::fs::read_dir(&run.telemetry).unwrap().flatten().map(|e| e.path()).collect();
    assert_eq!(dirs.len(), 1, "run folders: {dirs:?}");
    let dir = &dirs[0];
    assert!(dir.file_name().unwrap().to_string_lossy().starts_with("run-"), "run folder {dir:?}");
    let manifest: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).expect("manifest.json")).expect("manifest.json is JSON");
    assert_eq!(manifest["tier"], "frame", "manifest: {manifest}");

    // every line of events.jsonl is an object with a numeric `t` and a string `ev`
    let text = std::fs::read_to_string(dir.join("events.jsonl")).expect("events.jsonl");
    let events: Vec<Value> = text
        .lines()
        .enumerate()
        .map(|(n, l)| serde_json::from_str::<Value>(l).unwrap_or_else(|e| panic!("events.jsonl line {}: {e}: {l}", n + 1)))
        .collect();
    for (n, v) in events.iter().enumerate() {
        assert!(v.is_object() && v["t"].is_number() && v["ev"].is_string(), "events.jsonl line {}: {v}", n + 1);
    }

    // one `frame` event per present, `i` = 1..N in a row
    let presents = run.report["d3d12"]["presents"].as_u64().expect("report d3d12.presents");
    let frames = of(&events, "frame");
    assert_eq!(frames.len() as u64, presents, "frame events vs report d3d12.presents");
    assert!(presents >= FRAMES, "the scenario presents {FRAMES} frames, the report counts {presents}");
    for (k, (_, f)) in frames.iter().enumerate() {
        assert_eq!(f["i"].as_u64(), Some(k as u64 + 1), "frame events must run 1..N without gaps: {f}");
    }

    // `memory` events only move forward
    let mem = of(&events, "memory");
    assert!(!mem.is_empty(), "no memory event in {} frames", presents);
    for w in mem.windows(2) {
        assert!(num(w[0].1, "t") <= num(w[1].1, "t"), "memory t went back: {} then {}", w[0].1, w[1].1);
        assert!(w[0].1["frame"].as_u64() <= w[1].1["frame"].as_u64(), "memory frame went back: {} then {}", w[0].1, w[1].1);
    }

    // the lists of each frame, and gpu_ms = the union of their calibrated intervals
    let lists = of(&events, "list");
    for (_, l) in &lists {
        if l["start_tick"].is_null() || l["end_tick"].is_null() {
            assert!(l["why"].is_string(), "a list with null ticks needs a why: {l}");
        }
    }
    let mut overlapping = 0u32;
    // list events carry the frame index at submit: list frame f belongs to frame.i = f + 1
    for (_, f) in &frames {
        if f["sampled"] != true {
            assert_eq!(f["why"], "sampled_out", "an unsampled frame says why: {f}");
            continue;
        }
        let i = f["i"].as_u64().unwrap();
        let iv: Vec<(f64, f64)> = lists
            .iter()
            .filter(|(_, l)| l["frame"].as_u64() == Some(i - 1) && !l["start_tick"].is_null() && !l["end_tick"].is_null())
            .map(|(_, l)| (num(l, "start_ms"), num(l, "end_ms")))
            .collect();
        if f["gpu_ms"].is_null() {
            let why = f["why"].as_str().unwrap_or_else(|| panic!("frame {i}: null gpu_ms without a why: {f}"));
            // exit / removed / late null the GPU fields whatever the lists hold
            let forced = ["late", "exit", "removed"].contains(&why);
            assert!(forced || (iv.is_empty() && ["no_lists", "no_ticks"].contains(&why)), "frame {i}: gpu_ms null (why {why}) but {} lists have ticks: {f}", iv.len());
        } else {
            assert!(!iv.is_empty(), "frame {i}: gpu_ms {} but no list event with ticks: {f}", f["gpu_ms"]);
            // every *_ms field is rounded to 3 decimals when written, and gpu_ms is the rounded
            // union of the unrounded intervals, while `iv` holds rounded endpoints: each endpoint
            // and gpu_ms itself can be off by 0.0005 ms
            let tol = 0.0005 * (2 * iv.len() + 1) as f64 + 1e-9;
            let sum: f64 = iv.iter().map(|(s, e)| e - s).sum();
            let want = union_ms(iv);
            let got = num(f, "gpu_ms");
            assert!((got - want).abs() <= tol, "frame {i}: gpu_ms {} but the union of its lists is {want} (tolerance {tol})", f["gpu_ms"]);
            // a union, never a sum: where the intervals overlap by more than the rounding, gpu_ms stays below the sum
            if sum - want > tol {
                overlapping += 1;
                assert!(sum - got > 0.0, "frame {i}: gpu_ms {got} is not below the sum of the list durations {sum} although they overlap");
            }
        }
    }
    if overlapping == 0 {
        println!("note: no frame had overlapping list intervals; the union-not-sum property was not exercised");
    }

    // a mid-run sampled frame has at least the scenario's three lists (direct, compute, DMA copy)
    let mid: Vec<u64> = frames.iter().filter(|(_, f)| f["sampled"] == true).filter_map(|(_, f)| f["i"].as_u64()).filter(|i| (40..=80).contains(i)).collect();
    assert!(!mid.is_empty(), "no sampled frame between 40 and 80");
    let per_frame = |i: u64| lists.iter().filter(|(_, l)| l["frame"].as_u64() == Some(i - 1)).count();
    assert!(mid.iter().any(|&i| per_frame(i) >= 3), "list events per mid-run frame: {:?}", mid.iter().map(|&i| (i, per_frame(i))).collect::<Vec<_>>());

    // clock events carry the frame of their calibration, so these checks do not depend on how fast
    // the telemetry thread wakes (it writes the 60-frame refreshes)
    let clocks = of(&events, "clock");
    let mut queues: Vec<u64> = lists.iter().filter(|(_, l)| !l["start_tick"].is_null()).filter_map(|(_, l)| l["queue"].as_u64()).collect();
    queues.sort_unstable();
    queues.dedup();
    assert!(!queues.is_empty(), "no list event with ticks at all");
    for q in queues {
        let stamped: Vec<u64> = lists.iter().filter(|(_, l)| l["queue"].as_u64() == Some(q) && !l["start_tick"].is_null()).filter_map(|(_, l)| l["frame"].as_u64()).collect();
        let first = *stamped.iter().min().unwrap();
        let last = *stamped.iter().max().unwrap();
        let mut cf: Vec<u64> = clocks.iter().filter(|(_, c)| c["queue"].as_u64() == Some(q)).map(|(_, c)| c["frame"].as_u64().unwrap_or_else(|| panic!("clock without a frame: {c}"))).collect();
        cf.sort_unstable();
        // (a) a calibration exists at or before the queue's first stamped list
        assert!(cf.first().is_some_and(|&c| c <= first), "queue {q}: clock frames {cf:?}, first stamped list in frame {first}");
        // (b) never refreshed more often than every 60 frames
        for w in cf.windows(2) {
            assert!(w[1] >= w[0] + 60, "queue {q}: clock events at frames {} and {} are less than 60 frames apart ({cf:?})", w[0], w[1]);
        }
        // (c) a queue stamped over 150 frames or more is refreshed at least once
        println!("note: queue {q}: {} clock events {cf:?}, stamped lists in frames {first}..={last}", cf.len());
        if last - first >= 150 {
            assert!(cf.len() >= 2, "queue {q}: stamped lists span frames {first}..={last} but only {} clock event(s) ({cf:?})", cf.len());
        }
    }
    // (d) frequencies
    for (_, c) in &clocks {
        assert!(c["queue_type"].is_string() && c["qpc_frequency"].as_i64().is_some_and(|f| f > 0), "clock: {c}");
        assert!(c["frequency"].is_null() || c["frequency"].as_u64().is_some_and(|f| f >= 1000), "clock frequency: {c}");
        if c["gpu_tick"].is_null() {
            assert!(c["why"].is_string(), "a clock without ticks needs a why: {c}");
        }
    }

    // the list the title submitted twice in a row: resubmitted, ticks still valid (or a why)
    let again: Vec<&Value> = lists.iter().map(|(_, l)| *l).filter(|l| l["resubmitted"] == true).collect();
    assert!(!again.is_empty(), "no list event with resubmitted == true");
    for l in again {
        assert!(!l["start_tick"].is_null() && !l["end_tick"].is_null() || l["why"].is_string(), "resubmitted list: {l}");
    }
}

#[test]
fn telemetry_off_creates_nothing() {
    let Some(run) = run_scenario("telemetry_off_creates_nothing", None) else { return };
    assert_eq!(run.code, Some(0), "exit code {:?}", run.code);
    assert!(run.stdout.lines().any(|l| l.starts_with("frames: ")), "no `frames:` line in the title's output");
    let left: Vec<PathBuf> = std::fs::read_dir(&run.telemetry).unwrap().flatten().map(|e| e.path()).collect();
    assert!(left.is_empty(), "XCL_TELEMETRY is unset but the telemetry dir holds {left:?}");
}

#[test]
fn headless_runs_without_a_window() {
    let Some(run) = run_normal("headless_runs_without_a_window", Some("frame"), &[("XCL_HEADLESS", "1")]) else { return };
    assert_title_done(&run);
    assert!(run.log.contains("present: first game frame shown"), "no first frame in the log:\n{}", run.log);
    assert!(!run.log.contains("XCL window created"), "XCL_HEADLESS=1 but a window was created:\n{}", run.log);
    let (manifest, _) = read_run(&run);
    assert_eq!(manifest["present_mode"], "headless", "manifest: {manifest}");
    let diag = manifest["diagnostic"].as_array().unwrap_or_else(|| panic!("diagnostic is not a list: {manifest}"));
    assert!(diag.iter().any(|d| d == "XCL_HEADLESS=1"), "diagnostic: {diag:?}");
    assert!(manifest["compare_key"].as_str().is_some_and(|k| !k.is_empty()), "compare_key: {}", manifest["compare_key"]);
}

#[test]
fn window_size_override() {
    let Some(run) = run_normal("window_size_override", None, &[("XCL_WINDOW_SIZE", "320x200")]) else { return };
    assert_title_done(&run);
    assert!(run.log.contains("XCL window created (320x200)"), "log:\n{}", run.log);
}

#[test]
fn present_skip_keeps_the_window() {
    let Some(run) = run_normal("present_skip_keeps_the_window", Some("frame"), &[("XCL_PRESENT_SKIP", "1")]) else { return };
    assert_title_done(&run);
    assert!(run.log.contains("XCL window created"), "XCL_PRESENT_SKIP=1 keeps the window:\n{}", run.log);
    assert!(run.log.contains("present: first game frame shown"), "no first frame in the log:\n{}", run.log);
    let (manifest, _) = read_run(&run);
    assert_eq!(manifest["present_mode"], "skip", "manifest: {manifest}");
}

#[test]
fn a_normal_run_ends_with_end_and_exit() {
    let Some(run) = run_normal("a_normal_run_ends_with_end_and_exit", Some("frame"), &[]) else { return };
    assert_title_done(&run);
    let (manifest, events) = read_run(&run);

    // the title imports a function XCL does not have (`XclDefinitelyMissingFunction`): one non-fatal error
    let errors = of(&events, "error");
    let missing: Vec<&Value> =
        errors.iter().map(|(_, e)| *e).filter(|e| e["kind"] == "unresolved_import" && e["function"] == "XclDefinitelyMissingFunction").collect();
    assert_eq!(missing.len(), 1, "unresolved_import events for XclDefinitelyMissingFunction: {missing:?}; errors: {errors:?}");
    assert_eq!(missing[0]["fatal"], false, "error: {}", missing[0]);

    // the run ended through the title's exit, and nothing was fatal
    let ends = of(&events, "end");
    assert_eq!(ends.len(), 1, "end events: {ends:?}");
    // this title is a Rust program: its `main` returning ends in `ExitProcess`, which XCL sees in
    // `hk_nt_terminate_process` before `host_main` reaches `entry_returned`, so `telemetry_exit`
    // writes `end{process_exit}`. `entry_returned` is the reason of a title whose entry point
    // returns to XCL.
    assert_eq!(ends[0].1["reason"], "process_exit", "end: {}", ends[0].1);
    assert_eq!(ends[0].1["code"].as_i64(), Some(0), "end: {}", ends[0].1);
    let fatal: Vec<&Value> = events.iter().filter(|v| v["fatal"] == true).collect();
    assert!(fatal.is_empty(), "fatal events in a normal run: {fatal:?}");
    let exits = of(&events, "exit");
    assert_eq!(exits.len(), 1, "exit events: {exits:?}");
    assert_eq!(exits[0].1["status"].as_i64(), Some(0), "exit: {}", exits[0].1);

    // a normal run: window, no diagnostic switch
    assert_eq!(manifest["present_mode"], "window", "manifest: {manifest}");
    assert_eq!(manifest["diagnostic"].as_array().map(|a| a.len()), Some(0), "diagnostic: {}", manifest["diagnostic"]);
    assert_eq!(manifest["compare_key"], "", "compare_key: {}", manifest["compare_key"]);
}

/// The `N` of every `activity (2 s): N presents, ...` line of the log, in order.
fn activity_log_presents(log: &str) -> Vec<u64> {
    log.lines()
        .filter_map(|l| {
            let rest = &l[l.find("activity (2 s): ")? + "activity (2 s): ".len()..];
            rest.split_whitespace().next()?.parse::<u64>().ok().filter(|_| rest.split_whitespace().nth(1).is_some_and(|w| w.starts_with("presents")))
        })
        .collect()
}

#[test]
fn activity_events_mirror_the_log() {
    // 600 frames at the frame clock's own pace last several 2 s ticks; the relation below holds
    // (and is asserted) for however many ticks the run crossed
    let Some(run) = run_paced("activity_events_mirror_the_log", Some("frame"), 600, &[("XCL_ACTIVITY", "1")]) else { return };
    assert_title_done(&run);
    let (_, events) = read_run(&run);
    let mut acts: Vec<&Value> = events.iter().filter(|v| v["ev"] == "activity").collect();
    acts.sort_by(|a, b| num(a, "t").total_cmp(&num(b, "t")));
    let lines = activity_log_presents(&run.log);
    println!("note: {} activity events, {} activity log lines", acts.len(), lines.len());
    // the line is logged, then the event written: the run can end between the two
    assert!(acts.len().abs_diff(lines.len()) <= 1, "{} activity events vs {} log lines", acts.len(), lines.len());
    for a in &acts {
        assert!(a["presents_2s"].is_u64() && a["committed_mb"].is_u64(), "activity: {a}");
    }
    if acts.len() == lines.len() {
        for (k, (a, n)) in acts.iter().zip(&lines).enumerate() {
            assert_eq!(a["presents_2s"].as_u64(), Some(*n), "activity #{k}: event {a} vs log line figure {n}");
        }
    } else {
        for a in &acts {
            let p = a["presents_2s"].as_u64().unwrap();
            assert!(lines.contains(&p), "activity event {a}: {p} is not among the log's figures {lines:?}");
        }
    }
}

#[test]
fn phase_schedule_marks_the_run() {
    let Some(run) = run_paced("phase_schedule_marks_the_run", Some("frame"), 600, &[("XCL_PHASE_MARK", "0:menu,0.3:bench1")]) else { return };
    assert_title_done(&run);
    let (_, events) = read_run(&run);
    let mut phases: Vec<&Value> = events.iter().filter(|v| v["ev"] == "phase").collect();
    phases.sort_by(|a, b| num(a, "t").total_cmp(&num(b, "t")));
    assert!(!phases.is_empty(), "no phase event");

    let first = phases[0];
    assert_eq!(first["name"], "boot", "first phase: {first}");
    assert_eq!(first["source"], "auto", "first phase: {first}");
    assert!(first["from"].is_null(), "first phase: {first}");

    let sched: Vec<&&Value> = phases.iter().filter(|p| p["source"] == "schedule").collect();
    assert!(sched.iter().any(|p| p["name"] == "menu"), "no schedule phase `menu`: {sched:?}");
    // the poll thread starts after `boot` is written and checks the schedule every 250 ms: with the
    // run's own clock showing 0.3 s plus a 0.3 s margin after `boot`, `bench1` is due
    let last_t = events.iter().map(|v| num(v, "t")).fold(0.0, f64::max);
    if last_t - num(first, "t") > 0.3 + 0.3 {
        assert!(sched.iter().any(|p| p["name"] == "bench1"), "run lasted to t={last_t} but no schedule phase `bench1`: {sched:?}");
    } else {
        println!("note: the run ended at t={last_t}, too early to require `bench1`");
    }

    for p in &phases {
        let c = p["confidence"].as_f64().unwrap_or_else(|| panic!("phase without a numeric confidence: {p}"));
        assert!((0.0..=1.0).contains(&c), "phase confidence out of range: {p}");
        if p["source"] == "schedule" {
            assert_eq!(c, 1.0, "schedule phase: {p}");
        }
    }
}
