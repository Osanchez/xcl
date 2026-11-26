#!/usr/bin/env python3
"""Tests for pass-stats-check.py (synthetic run folders in a temp dir, made-up keys).
Run from the repo root: python3 -I scripts/dev/test_pass_stats_check.py -v"""
import contextlib
import importlib.util
import io
import json
import os
import shutil
import tempfile
import unittest

_spec = importlib.util.spec_from_file_location("pass_stats_check", os.path.join(os.path.dirname(os.path.abspath(__file__)), "pass-stats-check.py"))
pc = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pc)

A, B, C = "0x0123456789abcdef", "0x00000000000000b2", "0x00000000000000c3"


def snap(lst, p, when, res, **kw):
    e = {"ev": "pass_stats", "frame": 7, "list": lst, "pass": p, "when": when, "res": res,
         "kind": "rt", "format": "Rgba8", "w": 4, "h": 4, "min": [0.1], "max": [0.9],
         "mean": [0.5], "zero": 0, "nan": 0, "inf": 0, "count": 16, "hash": "00000000000000aa",
         "skipped": "", "unresolved": 0}
    if when == "post":
        kw.setdefault("reads", [])
    e.update(kw)
    return e


def clean():
    """Pass 0 writes A (no reads), pass 1 reads A and writes B (presented)."""
    return [snap(1, 0, "pre", A, hash="01"), snap(1, 0, "post", A, hash="02"),
            snap(1, 1, "pre", B, hash="03"), snap(1, 1, "post", B, hash="04", reads=[A])]


class Check(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix="pass-stats-check-test-")
        self.addCleanup(shutil.rmtree, self.dir, True)

    def run_main(self, events, *extra, raw=None):
        with open(os.path.join(self.dir, "events.jsonl"), "w", encoding="utf-8") as f:
            f.write(raw if raw is not None else "".join(json.dumps(e) + "\n" for e in events))
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = pc.main(["--run", self.dir, *extra])
        return code, out.getvalue()

    def rules(self, events):
        code, out = self.run_main(events)
        r = [l.split()[1] for l in out.splitlines() if l.startswith("FINDING")]
        return code, r, out

    def test_clean_input_has_no_finding(self):
        code, r, out = self.rules(clean())
        self.assertEqual((code, r), (0, []))
        self.assertIn("SUMMARY frames=1 snapshots=4 findings=0", out)

    def test_nan_inf(self):
        ev = clean()
        ev[3]["inf"] = 2
        code, r, out = self.rules(ev)
        self.assertEqual((code, r), (1, ["nan_inf"]))
        self.assertIn("frame=7 list=1 pass=1 res=" + B, out)

    def test_unchanged(self):
        ev = clean()
        ev[1]["hash"] = "01"
        code, r, out = self.rules(ev)
        self.assertEqual((code, r), (1, ["unchanged"]))
        self.assertIn("pass=0 res=" + A, out)

    def test_constant(self):
        ev = clean()
        ev[1].update(min=[0.5], max=[0.5])
        code, r, out = self.rules(ev)
        self.assertEqual((code, r), (1, ["constant"]))

    def test_constant_needs_more_than_one_value(self):
        ev = clean()
        ev[1].update(min=[0.5], max=[0.5], count=1)
        self.assertEqual(self.rules(ev)[1], [])

    def test_all_zero(self):
        ev = clean()
        ev[1].update(min=[0.0], max=[0.0], zero=16)
        code, r, out = self.rules(ev)
        self.assertEqual((code, r), (1, ["all_zero"]))

    def test_read_unwritten(self):
        ev = clean()
        ev[3]["reads"] = [A, C]
        code, r, out = self.rules(ev)
        self.assertEqual((code, r), (1, ["read_unwritten"]))
        self.assertIn(f"reads {C}", out)

    def test_read_in_first_pass_is_not_a_finding(self):
        ev = clean()
        ev[1]["reads"] = [C]
        self.assertEqual(self.rules(ev)[1], [])

    def test_first_constant_ancestor(self):
        # B is presented (last rt post of the last list) and a constant after pass 1; only that rule fires
        # when the constant check is satisfied by an all-zero-free, count-1 free target: use pass 2.
        ev = clean() + [snap(1, 2, "post", B, hash="05", min=[0.3], max=[0.3], reads=[B])]
        code, r, out = self.rules(ev)
        self.assertEqual(sorted(r), ["constant", "first_constant_ancestor"])
        self.assertIn("pass=2", [l for l in out.splitlines() if "first_constant_ancestor" in l][0])

    def test_skipped(self):
        ev = clean() + [snap(1, 3, "post", C, skipped="bundle"), snap(1, 4, "post", C, skipped="bundle")]
        code, r, out = self.rules(ev)
        self.assertEqual((code, r), (1, ["skipped"]))
        self.assertIn("2 snapshot(s) skipped: bundle", out)

    def test_frame_filter(self):
        ev = clean()
        ev[3]["nan"] = 1
        other = [dict(e, frame=8) for e in clean()]
        code, out = self.run_main(ev + other, "--frame", "8")
        self.assertEqual(code, 0)
        self.assertIn("frames=1", out)

    def test_bad_input_exits_2(self):
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(pc.main(["--run", os.path.join(self.dir, "missing")]), 2)
            self.assertEqual(pc.main(["--run", self.dir]), 2)  # no events.jsonl

    def test_malformed_lines_are_ignored(self):
        raw = "not json\n" + "".join(json.dumps(e) + "\n" for e in clean()) + "[1]\n"
        code, out = self.run_main(None, raw=raw)
        self.assertEqual(code, 0)


if __name__ == "__main__":
    unittest.main()
