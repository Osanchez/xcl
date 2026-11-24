#!/usr/bin/env python3
"""Tests for pr-scan.py (pure functions only; no network). Run: python3 -I scripts/dev/test_pr_scan.py -v"""
import datetime as dt
import importlib.util
import json
import os
import unittest

_spec = importlib.util.spec_from_file_location("pr_scan", os.path.join(os.path.dirname(os.path.abspath(__file__)), "pr-scan.py"))
ps = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(ps)

NOW = dt.datetime(2026, 11, 23, 12, 0, 0, tzinfo=dt.timezone.utc)


def pr(number, title="x", files=(), **kw):
    d = {"number": number, "title": title, "url": "https://github.com/o/r/pull/%d" % number, "author": "a",
         "isDraft": False, "createdAt": "2026-11-20T00:00:00Z", "updatedAt": "2026-11-22T00:00:00Z",
         "reviewDecision": None, "reviews": 0, "ci": "SUCCESS", "additions": 1, "deletions": 0,
         "files": [{"path": f, "additions": 1, "deletions": 0} if isinstance(f, str) else
                   {"path": f[0], "additions": f[1], "deletions": 0} for f in files]}
    d.update(kw)
    return d


class AreaTests(unittest.TestCase):
    def test_table_rows(self):
        cases = {
            "crates/xcl/src/hle/d3d12x/device.rs": "D3D12.X",
            "crates/xcl/src/gpu/recomp/emit.rs": "Recompiler",
            "crates/xcl/src/hle/xgameruntime/mod.rs": "XGameRuntime",
            "crates/xcl/src/hle/kernelx/file.rs": "kernelx",
            "crates/xcl/src/hle/xaudio2/mod.rs": "XAudio2",
            "crates/xcl/src/hle/xma2.rs": "XAudio2",
            "crates/xcl/src/hle/opus/dec.rs": "XAudio2",
            "crates/xcl/src/hle/dstorage/q.rs": "DirectStorage",
            "crates/xcl/src/vfs/overlay.rs": "VFS",
            "crates/xcl/src/gpu/uma/heap.rs": "Memory",
            "crates/xcl/src/tls/mod.rs": "TLS",
            "crates/xcl/src/pe/load.rs": "PE loader",
            "crates/xcl/src/loader.rs": "PE loader",
            "crates/xcl/src/resolver/x.rs": "PE loader",
            "crates/xcl/src/shaders/a.hlsl": "Shaders",
            "crates/xcl/src/frame/clock.rs": "Frame clock",
            "crates/xcl/tests/e2e.rs": "Tests/e2e",
            "tests/testtitle/src/main.rs": "Tests/e2e",
            "scripts/smoke.ps1": "Scripts",
            "README.md": "README",
            "docs/specs/a.md": "Docs",
            "HANDOFF.md": "Docs",
            ".github/workflows/ci.yml": "CI",
            "crates/xcl/src/gpu/other.rs": "GPU",
            "Cargo.toml": "Other",
            "crates/xcl/src/main.rs": "xcl run",
            "crates/xcl/src/eh.rs": "xcl run",
            "crates/xcl/src/hooks/mod.rs": "xcl run",
            "crates/xcl/src/host.rs": "xcl run",
            "crates/xcl/src/launcher.rs": "xcl run",
            "crates/xcl/src/preflight.rs": "xcl run",
            "crates/xcl/src/report.rs": "xcl run",
            "crates/xcl/src/config.rs": "xcl run",
            "crates/xcl/src/manifest.rs": "xcl run",
            "crates/xcl/src/hle/gameinput.rs": "Input",
            "crates/xcl/src/hle/kbm/mod.rs": "Input",
            "THIRD_PARTY.md": "Docs",
            "tests/data/bcpack/a.bin": "Tests/e2e",
            "crates/xcl/src/hle/other.rs": "HLE",
            "crates/xcl/src/hle/mod.rs": "HLE",
            "crates/xcl/src/other.rs": "Runtime",
            "crates/xcl/src/lib.rs": "Runtime",
            "docs/x.txt": "Docs",
        }
        for path, area in cases.items():
            self.assertEqual(ps.area_for_path(path), area, path)

    def test_order_first_match_wins(self):
        self.assertEqual(ps.area_for_path("crates/xcl/src/gpu/recomp/x.rs"), "Recompiler")
        self.assertNotEqual(ps.area_for_path("crates/xcl/src/gpu/recomp/x.rs"), "GPU")

    def test_title_prefix_area(self):
        areas, primary = ps.areas_for_pr(pr(1, "VFS: keep the destination", ["README.md"]))
        self.assertEqual(areas, ["README", "VFS"])
        self.assertEqual(primary, "README")

    def test_title_prefix_only(self):
        self.assertEqual(ps.areas_for_pr(pr(1, "Recompiler: round f16"))[1], "Recompiler")
        self.assertEqual(ps.areas_for_pr(pr(1, "no prefix at all"))[1], "Other")
        self.assertEqual(ps.title_prefix_area("e2e: more"), "Tests/e2e")
        self.assertIsNone(ps.title_prefix_area("fix: thing"))

    def test_primary_most_lines(self):
        p = pr(1, "t", [("crates/xcl/src/vfs/a.rs", 5), ("crates/xcl/src/tls/a.rs", 50), ("README.md", 3)])
        areas, primary = ps.areas_for_pr(p)
        self.assertEqual(primary, "TLS")
        self.assertEqual(areas, ["README", "TLS", "VFS"])


class ClusterTests(unittest.TestCase):
    def test_file_overlap_link(self):
        a = pr(1, "alpha", ["a.rs", "b.rs"])
        b = pr(2, "omega", ["a.rs", "b.rs", "c.rs", "d.rs"])  # 2/4 = 0.5
        cs = ps.clusters([a, b])
        self.assertEqual(len(cs), 1)
        self.assertEqual(cs[0]["members"], [1, 2])
        self.assertEqual(cs[0]["links"][0]["kind"], "shared files")

    def test_title_link(self):
        a = pr(1, "VFS: preserve destination when overlay rename fails", ["a.rs"])
        b = pr(2, "VFS: preserve destination if overlay rename fails", ["z.rs"])
        cs = ps.clusters([a, b])
        self.assertEqual(len(cs), 1)
        self.assertEqual(cs[0]["links"][0]["kind"], "similar title")

    def test_no_link_below_threshold(self):
        a = pr(1, "alpha beta", ["a.rs", "b.rs", "c.rs"])  # 1/5 = 0.2
        b = pr(2, "gamma delta", ["a.rs", "d.rs", "e.rs"])
        self.assertEqual(ps.clusters([a, b]), [])
        self.assertAlmostEqual(ps.jaccard({"a", "b", "c"}, {"a", "d", "e"}), 0.2)

    def test_empty_sets_never_link(self):
        self.assertEqual(ps.clusters([pr(1, "", []), pr(2, "", [])]), [])

    def test_transitive_component(self):
        a = pr(1, "one", ["a", "b"])
        b = pr(2, "two", ["a", "b", "c", "d"])
        c = pr(3, "three", ["c", "d"])  # links to b (2/4=0.5), not to a
        loner = pr(4, "four", ["q"])
        cs = ps.clusters([a, b, c, loner])
        self.assertEqual([x["members"] for x in cs], [[1, 2, 3]])

    def test_hot_threshold(self):
        def mk(n_prs, n_touch):
            return [pr(i, "t%d" % i, ["hot.rs"] if i < n_touch else ["o%d.rs" % i]) for i in range(n_prs)]
        self.assertEqual(ps.hot_files(mk(10, 4)), {})                 # 4 is not > max(4, 0.5)
        self.assertEqual(list(ps.hot_files(mk(10, 5))), ["hot.rs"])   # 5 > 4
        self.assertEqual(ps.hot_files(mk(100, 5)), {})                # 5 is not > 5
        self.assertEqual(ps.hot_files(mk(100, 6))["hot.rs"], [0, 1, 2, 3, 4, 5])
        self.assertEqual(ps.hot_threshold(100), 5)
        self.assertEqual(ps.hot_threshold(10), 4)

    def test_pair_linked_only_via_hot_file_not_linked(self):
        a = pr(1, "alpha", ["hot.rs", "a.rs"])
        b = pr(2, "omega", ["hot.rs", "b.rs"])
        self.assertEqual(len(ps.clusters([a, b])), 0)  # 1/3, below 0.5 anyway
        a = pr(1, "alpha", ["hot.rs"])
        b = pr(2, "omega", ["hot.rs"])
        self.assertEqual(len(ps.clusters([a, b])), 1)  # identical sets link without hot handling
        self.assertEqual(ps.clusters([a, b], frozenset({"hot.rs"})), [])

    def test_pair_sharing_non_hot_file_linked(self):
        a = pr(1, "alpha", ["hot.rs", "a.rs", "x1", "x2"])
        b = pr(2, "omega", ["hot.rs", "a.rs", "y1", "y2"])
        self.assertEqual(ps.clusters([a, b]), [])  # 2/6
        a = pr(1, "alpha", ["hot.rs", "a.rs"])
        b = pr(2, "omega", ["hot.rs", "a.rs", "b.rs"])
        cs = ps.clusters([a, b], frozenset({"hot.rs"}))  # non-hot: 1/2
        self.assertEqual(len(cs), 1)
        self.assertEqual(cs[0]["links"][0]["score"], 0.5)

    def test_build_analysis_hot_files(self):
        prs = [pr(i, "t%d" % i, ["hot.rs", "f%d.rs" % i]) for i in range(6)]
        an = ps.build_analysis(prs, NOW)
        self.assertEqual(an["clusters"], [])
        self.assertEqual(an["hotFiles"][0]["file"], "hot.rs")
        self.assertEqual(an["hotFiles"][0]["count"], 6)
        self.assertIn("Hot files", ps.render_html(an, "o/r", NOW))
        self.assertIn("## Hot files", ps.render_md(an, "o/r", NOW))

    def test_title_tokens(self):
        self.assertEqual(ps.title_tokens("VFS: Fix the overlay rename"), frozenset({"overlay", "rename"}))
        self.assertIn("thing", ps.title_tokens("Thing: the overlay"))


class StateTests(unittest.TestCase):
    def test_unreviewed(self):
        self.assertTrue(ps.is_unreviewed(pr(1)))
        self.assertFalse(ps.is_unreviewed(pr(1, reviews=1)))
        self.assertFalse(ps.is_unreviewed(pr(1, reviewDecision="APPROVED")))
        self.assertTrue(ps.is_unreviewed(pr(1, reviewDecision="REVIEW_REQUIRED")))

    def test_looks_done(self):
        self.assertTrue(ps.looks_done(pr(1)))
        self.assertTrue(ps.looks_done(pr(1, reviews=2, reviewDecision="APPROVED")))
        self.assertFalse(ps.looks_done(pr(1, isDraft=True)))
        self.assertFalse(ps.looks_done(pr(1, ci="FAILURE")))
        self.assertFalse(ps.looks_done(pr(1, ci=None)))
        self.assertFalse(ps.looks_done(pr(1, reviews=1, reviewDecision="CHANGES_REQUESTED")))

    def test_stale(self):
        self.assertFalse(ps.is_stale(pr(1, updatedAt="2026-11-17T13:00:00Z"), NOW))  # 5d23h
        self.assertTrue(ps.is_stale(pr(1, updatedAt="2026-11-16T12:00:00Z"), NOW))   # 7d

    def test_score_cover(self):
        p = pr(1, "Recompiler: texture tiling", ["crates/xcl/src/gpu/recomp/tex.rs"], body="adds tiling")
        s, why = ps.score_cover(p, ["tiling"], ["crates/xcl/src/gpu/recomp/tex.rs"])
        self.assertEqual(s, 3 + 1 + 3)
        self.assertTrue(any(w.startswith("shares 1") for w in why))
        self.assertEqual(ps.score_cover(p, ["nothing"], [])[0], 0)


class RenderTests(unittest.TestCase):
    def test_html_escapes_untrusted_text(self):
        evil = '<script>alert(1)</script>"&'
        p = pr(7, "VFS: " + evil, ["<img src=x onerror=1>.rs"], author="<b>bob</b>",
               url="javascript:alert(1)")
        an = ps.build_analysis([p, pr(8, "VFS: " + evil, ["<img src=x onerror=1>.rs"])], NOW)
        out = ps.render_html(an, "o/<r>", NOW)
        self.assertNotIn("<script>alert", out)
        self.assertNotIn("<b>bob", out)
        self.assertNotIn("javascript:", out)
        self.assertIn("&lt;script&gt;", out)
        self.assertNotIn("<r>", out)
        self.assertIn("prefers-color-scheme:dark", out)
        self.assertNotIn("<script src", out)
        md = ps.render_md(an, "o/r", NOW)
        self.assertNotIn("<script>", md)
        self.assertNotIn("javascript:", md)


def snap(ts, prs, **kw):
    d = {"ts": ts, "open": len(prs), "unreviewed": 1, "draft": 0, "looksDone": 0, "clusters": 0,
         "areas": {"VFS": len(prs)}, "prs": sorted(prs)}
    d.update(kw)
    return d


def t(h):
    return (NOW - dt.timedelta(hours=h)).strftime("%Y-%m-%dT%H:%M:%SZ")


class HistoryTests(unittest.TestCase):
    def test_snapshot_fields(self):
        an = ps.build_analysis([pr(2, "VFS: a", ["crates/xcl/src/vfs/a.rs"], isDraft=True), pr(1, "VFS: b", ["crates/xcl/src/vfs/b.rs"])], NOW)
        s = ps.snapshot(an, NOW)
        self.assertEqual((s["open"], s["draft"], s["prs"], s["areas"]), (2, 1, [1, 2], {"VFS": 2}))
        self.assertEqual(s["ts"], "2026-11-23T12:00:00Z")

    def test_append_30_minute_rule(self):
        h = [snap("2026-11-23T11:45:00Z", [1])]
        self.assertEqual(len(ps.append_snapshot(h, snap("2026-11-23T12:00:00Z", [1]))), 1)
        self.assertEqual(len(ps.append_snapshot(h, snap("2026-11-23T12:15:00Z", [1]))), 2)
        self.assertEqual(len(ps.append_snapshot(h, snap("2026-11-23T11:00:00Z", [1]))), 1)  # clock went back
        self.assertEqual(len(ps.append_snapshot([], snap("2026-11-23T12:00:00Z", [1]))), 1)

    def test_prune_90_days(self):
        h = [snap(t(24 * 91), [1]), snap(t(24 * 89), [1]), snap(t(1), [1])]
        self.assertEqual(len(ps.prune_history(h, NOW)), 2)

    def test_parse_tolerates_bad_lines(self):
        text = "\n".join(["not json", "", "[1,2]", '{"ts":"yesterday"}', '{"open":3}', json.dumps(snap(t(2), [1, 2])),
                          '{"ts":"%s","open":"x","prs":"y","areas":5}' % t(1)])
        h = ps.parse_history(text)
        self.assertEqual([s["ts"] for s in h], [t(2), t(1)])
        self.assertEqual((h[1]["open"], h[1]["prs"], h[1]["areas"]), (0, None, {}))
        self.assertEqual(ps.parse_history(None), [])
        self.assertEqual(ps.read_history("/nonexistent/history.jsonl"), [])

    def test_roundtrip(self):
        h = [snap(t(3), [1, 2]), snap(t(2), [2, 3])]
        self.assertEqual(ps.parse_history(ps.history_text(h)), h)

    def test_new_and_closed(self):
        self.assertEqual(ps.new_and_closed([1, 2, 3], [2, 3, 4, 5]), (2, 1))
        self.assertEqual(ps.new_and_closed([], []), (0, 0))

    def test_daily_summary(self):
        h = [snap("2026-11-20T10:00:00Z", [1, 2]), snap("2026-11-20T11:00:00Z", [1, 2, 3]),
             snap("2026-11-21T00:00:00Z", [2, 3, 4, 5]), dict(snap("2026-11-21T01:00:00Z", [2, 3, 4, 5]), prs=None)]
        d = ps.daily_summary(h)
        self.assertEqual([(x["day"], x["open"], x["new"], x["closed"]) for x in d],
                         [("2026-11-20", 3, 1, 0), ("2026-11-21", 4, 2, 1)])

    def test_change_24h(self):
        h = [snap(t(30), [1], open=10), snap(t(25), [1], open=12), snap(t(2), [1], open=20)]
        self.assertEqual(ps.change_24h(h, "open", 15, NOW), 3)
        self.assertIsNone(ps.change_24h([snap(t(2), [1])], "open", 5, NOW))

    def test_change_24h_23h_rule(self):
        self.assertIsNone(ps.change_24h([snap(t(22.9), [1], open=1)], "open", 5, NOW))
        self.assertEqual(ps.change_24h([snap(t(23), [1], open=1)], "open", 5, NOW), 4)

    def test_change_24h_picks_closest_to_24h(self):
        h = [snap(t(48), [1], open=1), snap(t(26), [1], open=2), snap(t(23.5), [1], open=3), snap(t(23), [1], open=4)]
        self.assertEqual(ps.change_24h(h, "open", 10, NOW), 7)  # 23.5 h is 0.5 h from 24 h; 26 h is 2 h

    def test_daily_summary_pairs(self):
        d = ps.daily_summary([snap("2026-11-20T10:00:00Z", [1]), snap("2026-11-21T10:00:00Z", [1, 2])])
        self.assertEqual([x["pairs"] for x in d], [0, 1])

    def test_downsample(self):
        self.assertEqual(ps.downsample([1, 2, 3], 5), [1, 2, 3])
        v = ps.downsample(list(range(100)), 10)
        self.assertEqual((len(v), v[0], v[-1]), (10, 0, 99))


class ChartTests(unittest.TestCase):
    def series(self, n):
        return [{"name": "open", "cls": "s-open", "points": [(NOW - dt.timedelta(hours=n - i), 10 + i) for i in range(n)]}]

    def test_line_chart_points(self):
        self.assertIn("No data yet", ps.svg_line_chart(self.series(0), "x"))
        one = ps.svg_line_chart(self.series(1), "x")
        self.assertIn("No data yet", one)
        self.assertNotIn("<svg", one)
        self.assertIn("<polyline", ps.svg_line_chart(self.series(2), "x"))
        many = ps.svg_line_chart(self.series(200), "x")
        self.assertIn("<polyline", many)
        self.assertIn("<title>", many)
        self.assertEqual(many.count("<svg"), 1)

    def test_bars(self):
        self.assertIn("No data yet", ps.svg_bars([], "x"))
        rows = [{"day": "2026-11-%02d" % d, "new": d % 3, "closed": 0} for d in range(1, 15)]
        out = ps.svg_bars(rows, "x")
        self.assertEqual(out.count("<rect"), 28)
        self.assertIn("opened", out)
        self.assertIn("<rect", ps.svg_bars([{"day": "2026-11-01", "new": 0, "closed": 0}], "x"))

    def test_sparkline_and_ticks(self):
        for vals in ([3, 3, 3], [1, 5, 2], [1, 2]):
            self.assertIn("<polyline", ps.sparkline(vals))
        for vals in ([], [3]):
            self.assertIn("no data", ps.sparkline(vals))
            self.assertNotIn("<svg", ps.sparkline(vals))
        step, top = ps.nice_ticks(0)
        self.assertGreaterEqual(top, 1)
        step, top = ps.nice_ticks(37)
        self.assertEqual(top % step, 0)
        self.assertGreaterEqual(top, 37)

    def test_chart_title_escaped(self):
        self.assertNotIn("<img", ps.svg_line_chart(self.series(3), "<img src=x>"))


class PageTests(unittest.TestCase):
    def an(self):
        return ps.build_analysis([pr(1, "VFS: a", ["crates/xcl/src/vfs/a.rs"]),
                                  pr(2, "VFS: a", ["crates/xcl/src/vfs/a.rs"], author="x/y\"<z>")], NOW)

    def test_trends_empty_state(self):
        for h in ([], [snap(t(1), [1])]):
            out = ps.render_html(self.an(), "o/r", NOW, h)
            self.assertIn("<h2>Trends", out)
            self.assertIn("No data yet", out)
            self.assertIn("Trends start after the first hours of hourly snapshots (%d so far)." % len(h), out)
            self.assertNotIn("<polyline class=\"line", out)
            self.assertIn("Trends start after", ps.render_md(self.an(), "o/r", NOW, h))
        self.assertIn("Possible duplicate work", out)
        self.assertIn("Ready to merge", out)

    def test_bars_need_two_days(self):
        same_day = [snap("2026-11-23T01:00:00Z", [1]), snap("2026-11-23T09:00:00Z", [1, 2])]
        out = ps.render_html(self.an(), "o/r", NOW, same_day)
        self.assertIn("<polyline class=\"line", out)
        self.assertNotIn("<rect class=\"b-new", out)
        self.assertEqual(out.count("No data yet"), 1)
        two = [snap("2026-11-22T23:00:00Z", [1]), snap("2026-11-23T09:00:00Z", [1, 2])]
        self.assertIn("<rect class=\"b-new", ps.render_html(self.an(), "o/r", NOW, two))

    def test_area_sparkline_no_data(self):
        out = ps.render_html(self.an(), "o/r", NOW, [snap(t(1), [1])])
        self.assertIn("no data", out)
        self.assertNotIn('class="spark"', out)

    def test_no_delta_without_old_snapshot(self):
        out = ps.render_html(self.an(), "o/r", NOW, [snap(t(22), [1], open=1)])
        self.assertIn("no 24 h data yet", out)
        self.assertNotIn("/ 24 h", out)
        self.assertEqual(ps.render_html(self.an(), "o/r", NOW).count("no 24 h data yet"), 5)  # hero + 4 tiles, not Stale

    def test_trends_shown_with_history(self):
        h = [snap(t(48), [1]), snap(t(24), [1, 2]), snap(t(1), [2, 3, 4])]
        out = ps.render_html(self.an(), "o/r", NOW, h)
        self.assertIn("<h2>Trends", out)
        self.assertIn("/ 24 h", out)
        md = ps.render_md(self.an(), "o/r", NOW, h)
        self.assertIn("## Trends", md)
        self.assertIn("| 2026-11-21 |", md)

    def test_avatar_and_author_escaped(self):
        out = ps.render_html(self.an(), "o/r", NOW)
        self.assertNotIn('"<z>', out)
        self.assertIn("avatars.githubusercontent.com/x%2Fy%22%3Cz%3E?s=40", out)
        self.assertIn('loading="lazy"', out)

    def test_only_font_stylesheet_external(self):
        out = ps.render_html(self.an(), "o/r", NOW)
        self.assertEqual(out.count("<link rel=stylesheet"), 1)
        self.assertNotIn("<script src", out)


class ClusterLabelTests(unittest.TestCase):
    def m(self, *areas):
        return [{"primary": a, "additions": ln, "deletions": 0} for a, ln in areas]

    def test_majority(self):
        self.assertEqual(ps.cluster_label(self.m(("kernelx", 1), ("kernelx", 1), ("Memory", 50))), ("3 PRs, mostly kernelx", "kernelx"))

    def test_half_is_not_majority(self):
        label, area = ps.cluster_label(self.m(("VFS", 1), ("VFS", 1), ("TLS", 1), ("Memory", 1)))
        self.assertEqual((label, area), ("4 PRs across VFS, Memory and TLS", "VFS"))

    def test_tie_breaks_by_lines_then_name(self):
        self.assertEqual(ps.cluster_label(self.m(("B", 1), ("A", 5)))[0], "2 PRs across A and B")
        self.assertEqual(ps.cluster_label(self.m(("B", 5), ("A", 5)))[1], "A")

    def test_mixed_names_top_three_only(self):
        label, area = ps.cluster_label(self.m(("A", 1), ("A", 1), ("B", 1), ("B", 1), ("C", 1), ("D", 1), ("E", 1)))
        self.assertEqual((label, area), ("7 PRs across A, B and C", "A"))


if __name__ == "__main__":
    unittest.main()
