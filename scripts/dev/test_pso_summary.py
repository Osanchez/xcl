#!/usr/bin/env python3
"""Tests for pso-summary.py (synthetic run folders in a temp dir).
Run from the repo root: python3 -m unittest scripts/dev/test_pso_summary.py -v

PART 1 tests: scene start, ratios and post-warmup sums, dispatch histogram, malformed lines.
PART 2 tests (marked): the criteria, --check and the exit codes."""
import contextlib
import importlib.util
import io
import json
import os
import shutil
import tempfile
import unittest

_spec = importlib.util.spec_from_file_location("pso_summary", os.path.join(os.path.dirname(os.path.abspath(__file__)), "pso-summary.py"))
ps = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ps)


def frame(t_s, **kw):
    e = {"ev": "frame", "t": t_s * 1000}
    e.update(kw)
    return e


def phase(t_s, name, source="mark"):
    return {"ev": "phase", "t": t_s * 1000, "name": name, "source": source}


class Fixture(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix="pso-summary-test-")
        self.addCleanup(shutil.rmtree, self.dir, True)

    def make_run(self, events=(), raw=None, report=None, manifest=None, name="run"):
        d = os.path.join(self.dir, name)
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, "events.jsonl"), "w", encoding="utf-8") as f:
            f.write(raw if raw is not None else "".join(json.dumps(e) + "\n" for e in events))
        with open(os.path.join(d, "manifest.json"), "w", encoding="utf-8") as f:
            json.dump(manifest or {"run_id": "r1", "adapter": "TestGPU"}, f)
        if report is not None:
            with open(os.path.join(d, "report.json"), "w", encoding="utf-8") as f:
                json.dump(report, f)
        return d

    def run_main(self, d, *args):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            try:
                rc = ps.main([d, "--out", os.path.join(self.dir, "summary.json")] + list(args))
            except SystemExit as e:  # argparse usage errors
                rc = e.code
        return rc, out.getvalue(), err.getvalue()

    def summary(self, d, *args):
        rc, out, _ = self.run_main(d, "--quiet", *args)
        self.assertEqual(rc, 0)
        with open(os.path.join(self.dir, "summary.json"), encoding="utf-8") as f:
            return json.load(f)

    def check_lines(self, d, *args):
        rc, out, _ = self.run_main(d, "--quiet", "--check", *args)
        lines = {l.split()[0]: l for l in out.splitlines()}
        return rc, lines


class Part1(Fixture):
    def test_scene_start_rules(self):
        heavy = [frame(10, draw_total=100), frame(20, draw_total=400)]
        d = self.make_run(heavy + [phase(5, "world", "auto"), phase(7, "world", "mark")])
        self.assertEqual(ps.find_scene_start(*[ps.read_events(os.path.join(d, "events.jsonl"))[0]], None), (7.0, "mark"))
        ev = heavy + [phase(5, "world", "auto")]
        self.assertEqual(ps.find_scene_start(ev, None), (20.0, "heavy_frame"))
        self.assertEqual(ps.find_scene_start(ev, 3.5), (3.5, "override"))
        self.assertEqual(ps.find_scene_start([frame(1, draw_total=399)], None), (None, "none"))
        self.assertEqual(ps.find_scene_start([phase(2, "world", "schedule")], None), (2.0, "mark"))
        s = self.summary(self.make_run([frame(1, draw_total=1)], name="none"))
        self.assertIsNone(s["scene_start_s"])
        self.assertIsNone(s["post_warmup"])

    def test_ratios_and_post_warmup_sums(self):
        ev = [phase(10, "world")]
        for t in (11, 12, 13):  # before the warm-up (scene 10 + 60 = 70)
            ev.append(frame(t, draw_total=100, ei_draw_total=40, ei_draw_skip_building=4, draw_standin_late=6,
                            draw_skip_other=2, dispatch_total=10, dispatch_skip_late=1))
        for t in (80, 90):
            ev.append(frame(t, draw_total=200, ei_draw_total=100, ei_draw_skip_failed=1, dispatch_total=20))
        s = self.summary(self.make_run(ev))
        tot, pw = s["totals"], s["post_warmup"]
        self.assertEqual((tot["draw_total"], tot["ei_draw_total"], tot["frames"]), (700, 320, 5))
        self.assertEqual((tot["ei_draw_skip"], tot["draw_standin"], tot["draw_no_output"]), (14, 18, 38))
        self.assertAlmostEqual(tot["ei_draw_skip_ratio"], 14 / 320)
        self.assertAlmostEqual(tot["draw_standin_ratio"], 18 / 380)
        self.assertAlmostEqual(tot["draw_no_output_ratio"], 38 / 700)
        self.assertAlmostEqual(tot["dispatch_skip_ratio"], 3 / 70)
        self.assertEqual((pw["frames"], pw["draw_total"], pw["ei_draw_skip"], pw["draw_no_output"]), (2, 400, 2, 2))
        self.assertAlmostEqual(pw["ei_draw_skip_ratio"], 2 / 200)

    def test_dispatch_histogram_buckets(self):
        counts = [0, 1, 9, 10, 99, 100, 999, 1000, 5000]
        ev = [frame(i, dispatch_total=10000, dispatch_skip_late=n) for i, n in enumerate(counts)]
        h = self.summary(self.make_run(ev))["dispatch_skips_per_frame"]["total"]
        self.assertEqual(h["buckets"], {"0": 1, "1-9": 2, "10-99": 2, "100-999": 2, "1000+": 2})
        self.assertEqual((h["p50"], h["max"]), (99, 5000))
        # a run without the counters has no histogram
        s = self.summary(self.make_run([frame(1, draws=3)], name="old"))
        self.assertIsNone(s["dispatch_skips_per_frame"]["total"])
        self.assertIsNone(s["totals"]["ei_draw_skip_ratio"])

    def test_malformed_lines_are_counted(self):
        raw = '{"ev":"frame","t":1000}\nnot json\n[1,2]\n{"t":5}\n\n{"ev":"frame","t":2000}\n'
        s = self.summary(self.make_run(raw=raw))
        self.assertEqual(s["malformed_lines"], 3)
        self.assertEqual(s["totals"]["frames"], 2)
        # invalid UTF-8 does not raise
        d = self.make_run(raw="", name="bytes")
        with open(os.path.join(d, "events.jsonl"), "wb") as f:
            f.write(b'{"ev":"frame","t":1,"x":"\xff"}\n\xff\xfe\n')
        self.assertEqual(self.summary(d)["malformed_lines"], 1)

    def test_pipeline_groups(self):
        ev = [{"ev": "pipeline", "t": 130000, "stage": "graphics", "class": "M", "blob": True, "compile_ms": 10, "dxil_bytes": 300000},
              {"ev": "pipeline", "t": 140000, "stage": "graphics", "class": "M", "blob": True, "compile_ms": 30, "dxil_bytes": 400000},
              {"ev": "pipeline_ready", "t": 5, "stage": "compute", "need_ms": 1, "queued_ms": 40, "gate_ms": 5, "must_draw": True,
               "waited_ms": 9, "held_ms": 3, "class": "S", "outcome": "built", "key": "aa"},
              {"ev": "pipeline_unbuilt", "t": 9, "key": "bb", "stage": "compute", "skips": 7, "class": "L"},
              phase(70, "world")]
        s = self.summary(self.make_run(ev, report={"d3d12": {"background_pipelines": [1, 2, 3]}}))
        self.assertEqual(s["compile_ms"]["graphics/M/True"]["max"], 30)
        self.assertEqual(s["pipelines_per_minute"], {"graphics/M": [0, 2]})
        self.assertEqual(s["queue_wait_ms"]["compute/must_draw"]["p50"], 45)
        self.assertEqual(s["failed_builds"]["report_background_pipelines_failed"], 2)
        self.assertEqual(s["unbuilt"]["top_by_skips"][0]["key"], "bb")
        self.assertEqual(s["mustdraw"]["largest_wait_ms"], 9)


# ---------------------------------------------------------------- PART 2 tests

class Part2(Fixture):
    def cold_run(self, skip_ei, total_ei=2000):
        ev = [phase(0, "world"), frame(10, ei_draw_total=total_ei, draw_total=total_ei, ei_draw_skip_late=skip_ei)]
        return self.make_run(ev, name=f"cold{skip_ei}")

    def test_check_ac1_cold_and_warm(self):
        d = self.cold_run(1, total_ei=2000)  # 0.05 %
        rc, lines = self.check_lines(d, "--config", "cold")
        self.assertIn("AC1 PASS", lines["AC1"])
        # warm: one skip after the warm-up fails
        ev = [phase(0, "world"), frame(10, ei_draw_total=100, draw_total=100), frame(70, ei_draw_total=100, draw_total=100, ei_draw_skip_late=1)]
        rc, lines = self.check_lines(self.make_run(ev, name="warm"), "--config", "warm")
        self.assertEqual(rc, 1)
        self.assertIn("AC1 FAIL", lines["AC1"])
        ev[-1].pop("ei_draw_skip_late")
        rc, lines = self.check_lines(self.make_run(ev, name="warm0"), "--config", "warm")
        self.assertIn("AC1 PASS", lines["AC1"])
        # more than 0.1 % cold fails; without --config it is N/A
        rc, lines = self.check_lines(self.cold_run(3), "--config", "cold")
        self.assertIn("AC1 FAIL", lines["AC1"])
        rc, lines = self.check_lines(d)
        self.assertIn("AC1 N/A", lines["AC1"])
        self.assertEqual(rc, 0)

    def test_check_ac7_available_workers(self):
        act = lambda t, avail_general: {"ev": "activity", "t": t * 1000, "pso_workers": {"reserved": 3, "general": avail_general, "parked": 0},
                                       "pso_workers_abandoned": 0, "pso_large_held": 0}
        rc, lines = self.check_lines(self.make_run([act(1, 12)]))
        self.assertNotIn("AC7 FAIL", lines["AC7"])
        # 3 reserved + general 12 - parked 0 - abandoned 0 - large_held 14 = 1 available
        low = act(3, 12)
        low["pso_large_held"] = 14
        rc, lines = self.check_lines(self.make_run([act(1, 12), low], name="low"))
        self.assertEqual(rc, 1)
        self.assertIn("AC7 FAIL", lines["AC7"])
        # flat form is read too; without the worker fields the criterion is N/A
        flat = {"ev": "activity", "t": 1000, "pso_workers_reserved": 1, "pso_workers_general": 0, "pso_workers_parked": 0,
                "pso_workers_abandoned": 0, "pso_large_held": 0}
        rc, lines = self.check_lines(self.make_run([flat], name="flat"))
        self.assertIn("AC7 FAIL", lines["AC7"])
        rc, lines = self.check_lines(self.make_run([{"ev": "activity", "t": 1000}], name="old"))
        self.assertIn("AC7 N/A", lines["AC7"])

    def test_check_ac7_compile_over_its_class_limit(self):
        e = {"ev": "pipeline", "t": 1, "stage": "graphics", "class": "M", "compile_ms": 61000}
        act = {"ev": "activity", "t": 1, "pso_workers": {"reserved": 3, "general": 5, "parked": 0}, "pso_workers_abandoned": 0, "pso_large_held": 0}
        rc, lines = self.check_lines(self.make_run([e, act]))
        self.assertIn("AC7 FAIL", lines["AC7"])
        hard = {"ev": "compile_timeout", "t": 2, "kind": "hard", "stage": "graphics", "class": "M"}
        rc, lines = self.check_lines(self.make_run([e, act, hard], name="hard"))
        self.assertIn("AC7 PASS", lines["AC7"])

    def test_check_ac8_uses_marks_and_pipeline_ready(self):
        ready = lambda w: {"ev": "pipeline_ready", "t": 20000, "stage": "graphics", "need_ms": 500, "must_draw": True, "waited_ms": w,
                           "outcome": "built", "queued_ms": 1, "gate_ms": 0}
        base = [phase(5, "intro"), phase(60, "world"), frame(30, ei_draw_total=10, draw_total=10)]
        rc, lines = self.check_lines(self.make_run(base + [ready(1500)]))
        self.assertIn("AC8 PASS", lines["AC8"])
        rc, lines = self.check_lines(self.make_run(base + [ready(10500)], name="slow"))
        self.assertEqual(rc, 1)
        self.assertIn("AC8 FAIL", lines["AC8"])
        # a skipped must-draw call in the Still window fails; the same frame after the window does not
        bad = base + [ready(100), frame(31, ei_draw_total=10, draw_total=10, mustdraw_skipped=2)]
        rc, lines = self.check_lines(self.make_run(bad, name="skip"))
        self.assertIn("AC8 FAIL", lines["AC8"])
        late = base + [ready(100), frame(61, ei_draw_total=10, draw_total=10, mustdraw_skipped=2)]
        rc, lines = self.check_lines(self.make_run(late, name="late"))
        self.assertIn("AC8 PASS", lines["AC8"])
        # no marks: N/A, until --still-span names the window
        nomark = [frame(30, ei_draw_total=10, draw_total=10, mustdraw_skipped=1), ready(100)]
        rc, lines = self.check_lines(self.make_run(nomark, name="nomark"))
        self.assertIn("AC8 N/A", lines["AC8"])
        rc, lines = self.check_lines(self.make_run(nomark, name="nomark"), "--still-span", "10-40")
        self.assertIn("AC8 FAIL", lines["AC8"])

    def test_check_ac10_reads_report_json(self):
        ready = {"ev": "pipeline_ready", "t": 1, "stage": "graphics", "outcome": "built", "need_ms": 1}
        rc, lines = self.check_lines(self.make_run([ready], report={"d3d12": {"background_pipelines": [10, 0, 5]}}))
        self.assertIn("AC10 PASS", lines["AC10"])
        rc, lines = self.check_lines(self.make_run([ready], report={"d3d12": {"background_pipelines": [10, 2, 5]}}, name="bad"))
        self.assertEqual(rc, 1)
        self.assertIn("AC10 FAIL", lines["AC10"])
        failed = dict(ready, outcome="failed")
        rc, lines = self.check_lines(self.make_run([failed], report={"d3d12": {"background_pipelines": [1, 0, 0]}}, name="fail"))
        self.assertIn("AC10 FAIL", lines["AC10"])
        rc, lines = self.check_lines(self.make_run([ready], name="noreport"))
        self.assertIn("AC10 N/A", lines["AC10"])

    def test_old_run_gives_na_not_pass(self):
        ev = [frame(1, draws=5, dispatches=2), {"ev": "activity", "t": 2000, "committed_mb": 10}]
        rc, lines = self.check_lines(self.make_run(ev), "--config", "cold")
        self.assertEqual(rc, 0)
        for k in ("AC1", "AC2", "AC3", "AC4", "AC5", "AC6", "AC7", "AC8", "AC10"):
            self.assertIn(f"{k} N/A", lines[k], k)
        self.assertEqual(lines["RUN12"], "RUN12 N/A")

    def test_exit_codes(self):
        d = self.make_run([frame(1, ei_draw_total=10, draw_total=10)])
        self.assertEqual(self.run_main(d)[0], 0)
        self.assertEqual(self.run_main(d, "--check", "--config", "cold")[0], 0)
        self.assertEqual(self.run_main(os.path.join(self.dir, "missing"))[0], 2)
        self.assertEqual(self.run_main(d, "--still-span", "9-3")[0], 2)
        self.assertEqual(self.run_main(d, "--config", "bogus")[0], 2)
        empty = os.path.join(self.dir, "empty")
        os.makedirs(empty)
        self.assertEqual(self.run_main(empty)[0], 2)  # no events.jsonl
        failing = [frame(1, ei_draw_total=10, draw_total=10, ei_draw_skip_late=5)]
        self.assertEqual(self.run_main(self.make_run(failing, name="f"), "--check", "--config", "cold")[0], 1)

    def test_default_output_is_the_run_folder(self):
        d = self.make_run([frame(1, draws=1)])
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            self.assertEqual(ps.main([d, "--quiet"]), 0)
        self.assertTrue(os.path.isfile(os.path.join(d, "summary.json")))
        self.assertEqual(out.getvalue(), "")


if __name__ == "__main__":
    unittest.main()
