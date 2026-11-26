#!/usr/bin/env python3
"""Tests for smoke-events.py (fixtures in a temp dir; git and PowerShell are mocked).
Run from the repo root: python3 -m unittest scripts/dev/test_smoke_events.py -v"""
import contextlib
import hashlib
import importlib.util
import io
import json
import os
import shutil
import tempfile
import unittest
from unittest import mock

_spec = importlib.util.spec_from_file_location("smoke_events", os.path.join(os.path.dirname(os.path.abspath(__file__)), "smoke-events.py"))
se = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(se)

HEAD = "0123456789abcdef0123456789abcdef01234567"
LOG = "\n".join([
    "xcl: activity (2 s): 60 presents, 240 submits",
    "xcl: memory: 1234 MB committed by the process",
    "xcl: timed run over (118 frames shown, 120 PresentX calls)",
]) + "\n"


class FakeStdout:
    """A real stream stand-in (not a Mock): argparse on Python 3.14 calls fileno()/isatty() on it."""
    def __init__(self):
        self.buffer = io.BytesIO()

    def write(self, s):
        return len(s)

    def flush(self):
        pass

    def isatty(self):
        return False

    def fileno(self):
        raise io.UnsupportedOperation("fileno")


def fake_git(cmd, **kw):
    out = HEAD + "\n" if "rev-parse" in cmd else ""
    return mock.Mock(returncode=0, stdout=out, stderr="")


class Fixture(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix="smoke-events-test-")
        self.addCleanup(shutil.rmtree, self.dir, True)
        self.log = os.path.join(self.dir, "xcl.log")

    def write(self, path, text):
        with open(path, "w", encoding="utf-8") as f:
            f.write(text)

    def make_run(self, events=(), manifest=None, name="run-20261125T180411Z", raw_events=None, report=None):
        d = os.path.join(self.dir, name)
        os.makedirs(d, exist_ok=True)
        m = {"log": self.log, "switches": {"XCL_RUN_SECONDS": "7"}, "compare_key": ""}
        m.update(manifest or {})
        self.write(os.path.join(d, "manifest.json"), json.dumps(m))
        self.write(os.path.join(d, "events.jsonl"), raw_events if raw_events is not None else "".join(json.dumps(e) + "\n" for e in events))
        if report is not None:
            self.write(os.path.join(d, "report.json"), json.dumps(report))
        return d

    def run_main(self, *args):
        err = io.StringIO()
        fake = FakeStdout()
        buf = fake.buffer
        with mock.patch.object(se.subprocess, "run", side_effect=fake_git), \
                mock.patch.object(se.sys, "platform", "linux"), \
                mock.patch.object(se.sys, "stdout", fake), \
                contextlib.redirect_stderr(err):
            rc = se.main(["--log", self.log, "--title", "T"] + list(args))
        return rc, buf.getvalue().decode("utf-8"), err.getvalue()

    def block(self, *args):
        rc, out, _ = self.run_main(*args)
        self.assertEqual(rc, 0)
        return out.split("\n")

    def line(self, lines, label):
        for l in lines:
            if l.startswith(label + ":"):
                return l
        self.fail(f"no {label} line in {lines}")


class Regexes(unittest.TestCase):
    def test_frames_case_insensitive(self):
        self.assertTrue(se.FRAMES_RE.search("(118 Frames Shown, 120 PresentX calls)"))
        self.assertEqual(se.FRAMES_RE.search("x (118 frames shown, 120 PresentX calls)").groups(), ("118", "120"))

    def test_presents_case_sensitive(self):
        self.assertEqual(se.PRESENTS_RE.findall("activity (2 s): 60 presents, 1"), ["60"])
        self.assertEqual(se.PRESENTS_RE.findall("Activity (2 s): 60 presents"), [])

    def test_commit_removed_crash(self):
        self.assertEqual(se.COMMIT_RE.findall("memory: 99 MB committed by"), ["99"])
        self.assertEqual(se.COMMIT_RE.findall("Memory: 99 MB committed"), [])
        self.assertTrue(se.REMOVED_RE.search("D3D12.X: device removed (0x887a0006)"))
        self.assertFalse(se.REMOVED_RE.search("Device Removed"))
        self.assertEqual(len(se.CRASH_RE.findall("unhandled exception 0xc\nprocess ends with status 0xc0000005")), 2)
        self.assertEqual(len(se.CRASH_RE.findall("Unhandled Exception")), 0)

    def test_exit_regexes(self):
        self.assertEqual(se.EXIT_STATUS_RE.findall("process ends with status 0xc0000005"), ["0xc0000005"])
        self.assertTrue(se.EXIT_PLAIN_RE.search("process ends (thread 5)"))

    def test_vram_suffix(self):
        self.assertEqual(se.VRAM_RE.sub("", "Synthetic Adapter 9000 (32187 MiB VRAM)"), "Synthetic Adapter 9000")


class Title(unittest.TestCase):
    def test_game_leaf_hash(self):
        want = hashlib.sha256(b"SomeTitle").hexdigest()[:8]
        for game in ("D:\\Games\\SomeTitle", "/x/y/SomeTitle", "/x/y/SomeTitle/", "D:\\Games\\ SomeTitle ", "SomeTitle"):
            self.assertEqual(se.title_from(game=game), want, game)

    def test_label_checks(self):
        self.assertEqual(se.title_from(title="  My Title "), "My Title")
        for bad in ("", "  ", "a`b", "a\nb", "a\u200bb", "a\u2028b"):
            with self.assertRaises(se.UsageError):
                se.title_from(title=bad)


class Block(Fixture):
    def test_log_only_mode_matches_smoke_format(self):
        self.write(self.log, LOG)
        lines = self.block("--seconds", "1")
        self.assertEqual(lines, [
            "```", "xcl-smoke v1", "commit: " + HEAD, "gpu: unknown", "title: T", "seconds: 1", "exit: killed (no exit)",
            "frames shown: 118 (PresentX 120)", "presents per 2 s (last 3): 60", "device removals: 0", "crashes: 0",
            "peak commit: 1234 MB", "```", ""])

    def test_no_data_lines(self):
        self.write(self.log, "nothing\n")
        lines = self.block("--seconds", "1")
        self.assertEqual(self.line(lines, "frames shown"), "frames shown: none logged (the run did not reach its end)")
        self.assertEqual(self.line(lines, "presents per 2 s (last 3)"), "presents per 2 s (last 3): ")  # trailing space
        self.assertEqual(self.line(lines, "peak commit"), "peak commit: n/a MB")

    def test_dirty_commit_line(self):
        self.write(self.log, LOG)
        def dirty(cmd, **kw):
            return mock.Mock(returncode=0, stdout=HEAD if "rev-parse" in cmd else " M crates/x.rs\n", stderr="")
        with mock.patch.object(se.subprocess, "run", side_effect=dirty):
            lines = se.build_block(self.log, None, "T", 1, ".")
        self.assertEqual(lines[2], "commit: " + HEAD + " (uncommitted changes)")

    def test_git_commands_are_argument_lists(self):
        self.write(self.log, LOG)
        with mock.patch.object(se.subprocess, "run", side_effect=fake_git) as r:
            se.build_block(self.log, None, "T", 1, "/repo")
        cmds = [c.args[0] for c in r.call_args_list]
        self.assertEqual(cmds[0], ["git", "-C", "/repo", "rev-parse", "HEAD"])
        self.assertEqual(cmds[1], ["git", "-C", "/repo", "status", "--porcelain", "--untracked-files=no", "--", "crates", "Cargo.toml", "Cargo.lock"])
        self.assertTrue(all(isinstance(c, list) for c in cmds))

    def test_events_first_per_line(self):
        self.write(self.log, LOG + "xcl: unhandled exception 0xc\nxcl: D3D12.X: device removed\n")
        events = [
            {"t": 1, "ev": "activity", "presents_2s": 50, "committed_mb": 10},
            {"t": 2, "ev": "activity", "presents_2s": 51, "committed_mb": 900},
            {"t": 3, "ev": "activity", "presents_2s": 52, "committed_mb": 20},
            {"t": 4, "ev": "activity", "presents_2s": 53, "committed_mb": 30},
            {"t": 5, "ev": "end", "reason": "run_seconds", "frames_shown": 5, "presentx": 6},
            {"t": 6, "ev": "exit", "status": 0},
        ]
        self.make_run(events)
        lines = self.block()
        self.assertEqual(self.line(lines, "seconds"), "seconds: 7")  # from the manifest
        self.assertEqual(self.line(lines, "frames shown"), "frames shown: 5 (PresentX 6)")
        self.assertEqual(self.line(lines, "presents per 2 s (last 3)"), "presents per 2 s (last 3): 51 52 53")
        self.assertEqual(self.line(lines, "peak commit"), "peak commit: 900 MB")
        self.assertEqual(self.line(lines, "exit"), "exit: 0x0")
        self.assertEqual(self.line(lines, "crashes"), "crashes: 0")  # events win over the log's regex
        self.assertEqual(self.line(lines, "device removals"), "device removals: 1")  # always the log

    def test_falls_back_to_log_when_event_missing(self):
        self.write(self.log, LOG + "xcl: process ends with status 0xc0000005\n")
        self.make_run([{"t": 1, "ev": "end", "reason": "entry_returned", "frames_shown": 9, "presentx": 9}])
        lines = self.block("--seconds", "1")
        self.assertEqual(self.line(lines, "frames shown"), "frames shown: 118 (PresentX 120)")  # end is not run_seconds
        self.assertEqual(self.line(lines, "presents per 2 s (last 3)"), "presents per 2 s (last 3): 60")
        self.assertEqual(self.line(lines, "peak commit"), "peak commit: 1234 MB")
        self.assertEqual(self.line(lines, "exit"), "exit: 0xc0000005")
        self.assertEqual(self.line(lines, "crashes"), "crashes: 1")

    def test_exit_status_negative(self):
        self.write(self.log, LOG)
        self.make_run([{"t": 1, "ev": "exit", "status": -1073741819}, {"t": 0, "ev": "error", "kind": "title_exception", "fatal": True},
                       {"t": 0, "ev": "error", "kind": "other", "fatal": False}])
        lines = self.block()
        self.assertEqual(self.line(lines, "exit"), "exit: 0xc0000005")
        self.assertEqual(self.line(lines, "crashes"), "crashes: 2")  # one title_exception + nonzero exit

    def test_exit_from_plain_log_line(self):
        self.write(self.log, LOG + "process ends (thread 3)\n")
        self.assertEqual(self.line(self.block("--seconds", "1"), "exit"), "exit: 0x0")

    def test_malformed_line_skipped_and_reported(self):
        self.write(self.log, LOG)
        self.make_run(raw_events='{"ev":"activity","presents_2s":5,"committed_mb":7}\n{broken\n[1]\n\n{"ev":"exit","status":0}\n')
        rc, out, err = self.run_main("--explain")
        self.assertEqual(rc, 0)
        self.assertIn("presents per 2 s (last 3): 5\n", out)
        self.assertIn("2 malformed", err)
        self.assertIn("compare_key: ''", err)
        self.assertIn("peak commit: events", err)
        self.assertEqual(out.count("\n"), 13)
        self.assertNotIn("events:", out)

    def test_no_matching_run_is_log_only(self):
        self.write(self.log, LOG)
        self.make_run([{"t": 1, "ev": "exit", "status": 9}], manifest={"log": os.path.join(self.dir, "other.log")})
        lines = self.block("--seconds", "1")
        self.assertEqual(self.line(lines, "exit"), "exit: killed (no exit)")

    def test_run_matched_case_and_separator_insensitive(self):
        self.write(self.log, LOG)
        shown = self.log.upper().replace("/", "\\")
        self.make_run([{"t": 1, "ev": "exit", "status": 3}], manifest={"log": shown})
        self.assertEqual(self.line(self.block(), "exit"), "exit: 0x3")

    def test_newest_run_wins(self):
        self.write(self.log, LOG)
        old = self.make_run([{"t": 1, "ev": "exit", "status": 1}], name="run-a")
        os.utime(old, (1000, 1000))
        self.make_run([{"t": 1, "ev": "exit", "status": 2}], name="run-b")
        self.assertEqual(self.line(self.block(), "exit"), "exit: 0x2")

    def test_seconds_required_without_manifest(self):
        self.write(self.log, LOG)
        rc, _, err = self.run_main()
        self.assertEqual(rc, 2)
        self.assertIn("--seconds", err)

    def test_missing_log_is_usage_error(self):
        rc, _, _ = self.run_main("--seconds", "1")
        self.assertEqual(rc, 2)

    def test_gpu_sources(self):
        self.write(self.log, LOG)
        self.make_run([], manifest={"adapter": "Manifest GPU"}, report={"d3d12": {"adapter": "Report GPU (1 MiB VRAM)"}})
        self.assertEqual(self.line(self.block(), "gpu"), "gpu: Report GPU")
        self.make_run([], manifest={"adapter": "Manifest GPU"}, report={"d3d12": {"adapter": None}})
        self.assertEqual(self.line(self.block(), "gpu"), "gpu: Manifest GPU")

    def test_gpu_cim_on_windows(self):
        self.write(self.log, LOG)
        calls = []
        def run(cmd, **kw):
            calls.append(cmd)
            return mock.Mock(returncode=0, stdout=HEAD if "rev-parse" in cmd else ("Some GPU\r\n" if cmd[0] == "powershell" else ""), stderr="")
        with mock.patch.object(se.subprocess, "run", side_effect=run), mock.patch.object(se.sys, "platform", "win32"):
            lines = se.build_block(self.log, None, "T", 1, ".")
        self.assertIn("gpu: Some GPU", lines)
        ps = [c for c in calls if c[0] == "powershell"][0]
        self.assertEqual(ps[:3], ["powershell", "-NoProfile", "-Command"])
        self.assertEqual(ps[3], se.CIM_COMMAND)


if __name__ == "__main__":
    unittest.main()
