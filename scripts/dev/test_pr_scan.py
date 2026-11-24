#!/usr/bin/env python3
"""Tests for pr-scan.py (pure functions only; no network). Run: python3 -I scripts/dev/test_pr_scan.py -v"""
import datetime as dt
import importlib.util
import json
import os
import re
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
        self.assertIn("data-panel=\"dup\"", out)
        self.assertIn("data-panel=\"ready\"", out)

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
        self.assertEqual(ps.render_html(self.an(), "o/r", NOW).count("no 24 h data yet"), 6)  # hero + 5 stats, not Stale or CI failing

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

    def test_panels_all_present_without_js(self):
        out = ps.render_html(self.an(), "o/r", NOW)
        for k in ("dup", "ready", "hot", "all"):
            self.assertEqual(out.count('data-panel="%s"' % k), 1)
        self.assertNotIn('class="panel" hidden', out)
        self.assertIn(".js .tabs{display:flex}", out)  # tab buttons only exist with JS
        self.assertEqual(out.count('<div class="panel"'), 4)

    def test_road_merged_step(self):
        an = self.an()
        self.assertIn("no data yet", ps.render_html(an, "o/r", NOW).split('class="road"')[1].split("</ol>")[0])
        an["merged24"] = 7
        road = ps.render_html(an, "o/r", NOW).split('class="road"')[1].split("</ol>")[0]
        self.assertIn("<b>7</b><span>Merged 24 h</span>", road)
        self.assertEqual(road.count('class="step'), 5)

    def test_tooltips_on_charts(self):
        h = [snap(t(48), [1]), snap(t(24), [1, 2]), snap(t(1), [2, 3, 4])]
        out = ps.render_html(self.an(), "o/r", NOW, h)
        self.assertIn("data-tip=", out)
        self.assertIn('id="tip"', out)

    def test_only_font_stylesheet_external(self):
        out = ps.render_html(self.an(), "o/r", NOW)
        self.assertEqual(out.count("<link rel=stylesheet"), 1)
        self.assertNotIn("<script src", out)


def analysis_of(counts):
    """Analysis with counts = {area: open PRs}; PR k of an area is a draft when k is even and looks done otherwise."""
    prs, n = [], 0
    for area, c in counts.items():
        for k in range(c):
            n += 1
            prs.append(pr(n, "%s: t%d" % (area, n), ["crates/xcl/src/vfs/f%d.rs" % n], isDraft=(k % 2 == 0)))
    an = ps.build_analysis(prs, NOW)
    # force the areas: build_analysis groups by path, so regroup by the requested area for the test
    an["areas"] = {}
    for p in an["prs"]:
        a = p["title"].split(":")[0]
        p["primary"] = a
        an["areas"].setdefault(a, []).append(p)
    return an


class TopAreaTests(unittest.TestCase):
    def test_cut_and_else(self):
        an = analysis_of({"A": 5, "B": 4, "C": 3, "D": 2, "E": 1})
        t = ps.top_areas(an, 3)
        self.assertEqual([e["name"] for e in t], ["A", "B", "C", ps.ELSE_NAME])
        self.assertEqual(t[-1]["members"], ["D", "E"])
        self.assertEqual(len(t[-1]["prs"]), 3)
        self.assertIsNone(t[0]["members"])

    def test_ties_by_name(self):
        an = analysis_of({"Zed": 2, "Abe": 2, "Mid": 2, "Low": 1})
        t = ps.top_areas(an, 2)
        self.assertEqual([e["name"] for e in t], ["Abe", "Mid", ps.ELSE_NAME])
        self.assertEqual(t[-1]["members"], ["Zed", "Low"])

    def test_fewer_or_equal_areas_has_no_else(self):
        an = analysis_of({"A": 2, "B": 1})
        self.assertEqual([e["name"] for e in ps.top_areas(an, 6)], ["A", "B"])
        self.assertEqual([e["name"] for e in ps.top_areas(an, 2)], ["A", "B"])
        self.assertEqual(ps.top_areas(analysis_of({}), 6), [])

    def test_standings_use_top_and_filters_keep_all(self):
        an = analysis_of({"A%d" % i: 10 - i for i in range(8)})
        out = ps.render_html(an, "o/r", NOW)
        st = out.split('class="a-standings"')[1].split("</section>")[0]
        self.assertEqual(st.count('<li class="st">'), 7)
        self.assertIn("Everything else", st)
        self.assertIn("2 areas: A6, A7", st)
        sel = out.split('<select id="area">')[1].split("</select>")[0]
        for i in range(8):
            self.assertIn(">A%d (" % i, sel)
        self.assertNotIn('class="chip"', out)


class ToolbarTests(unittest.TestCase):
    def page(self, counts):
        return ps.render_html(analysis_of(counts), "o/r", NOW)

    def select(self, out):
        return out.split('<select id="area">')[1].split("</select>")[0]

    def test_select_lists_every_area_once(self):
        counts = {"A%d" % i: 10 - i for i in range(9)}
        out = self.page(counts)
        sel = self.select(out)
        self.assertTrue(sel.startswith('<option value="">All areas</option>'))
        for a in counts:
            self.assertEqual(sel.count('value="%s"' % ps.slug(a)), 1, a)
        self.assertEqual(sel.count("<option "), 1 + 6 + 1 + 3)  # All, top 6, Everything else, the 3 inside it
        self.assertIn('value="%s" data-members="%s"' % (ps.slug(ps.ELSE_NAME), ",".join(ps.slug(a) for a in ("A6", "A7", "A8"))), sel)
        grp = sel.split("<optgroup")[1]
        self.assertEqual(grp.count("<option "), 3)
        self.assertLess(sel.index('value="%s"' % ps.slug("A5")), sel.index(ps.slug(ps.ELSE_NAME)))

    def test_select_without_rollup(self):
        sel = self.select(self.page({"A": 2, "B": 1}))
        self.assertEqual(sel.count("<option "), 3)
        self.assertNotIn("optgroup", sel)
        self.assertNotIn(ps.ELSE_NAME, sel)

    def test_select_escapes_area_names(self):
        an = analysis_of({"A": 2})
        an["areas"]["<b>x</b>"] = an["areas"].pop("A")
        out = ps.render_html(an, "o/r", NOW)
        self.assertNotIn("<b>x</b>", out.split('<select id="area">')[1].split("</select>")[0])

    def test_segmented_state_control(self):
        out = self.page({"A": 2})
        seg = out.split('class="seg"')[1].split("</div>")[0]
        self.assertEqual(re.findall(r">([^<]+)</button>", seg), ["All", "Unreviewed", "Ready", "Draft"])
        self.assertEqual(seg.count('aria-pressed="true"'), 1)
        self.assertEqual(out.count('id="q"'), 1)

    def test_toolbar_hidden_without_js_and_one_row(self):
        out = self.page({"A": 2})
        self.assertIn(".toolbar{display:none", out)
        self.assertIn(".js .toolbar{display:flex}", out)
        self.assertEqual(out.count('id="filters"'), 1)
        self.assertLess(out.index('id="filters"'), out.index('class="top-right"'))  # inside the top bar

    def test_hash_state_keys(self):
        for key in ("q=", "area=", "state=", "tab="):
            self.assertIn(key, ps.JS)
        self.assertIn("'unrev','done','draft'", ps.JS)  # old draft=1 / unrev=1 / done=1 links still parse


class FitTests(unittest.TestCase):
    def desktop(self):
        css = ps.CSS + ps.SCENE_CSS
        a = css.index("@media (min-width:900px){\n:root")
        return css[a:css.index("\n}\n", a)]

    def test_body_never_scrolls_on_desktop(self):
        d = self.desktop()
        self.assertIn("html,body{height:100%;overflow:hidden}", d)
        self.assertIn("height:100dvh", d)
        self.assertNotIn("min-height:660px", ps.CSS + ps.SCENE_CSS)

    def test_scene_panel_never_scrolls(self):
        self.assertNotIn("overflow:auto", ps.SCENE_CSS)
        self.assertNotIn("overflow-y:auto", ps.SCENE_CSS)
        self.assertNotIn("overflow:scroll", ps.SCENE_CSS)
        self.assertIn(".a-city,.a-trends,.a-road,.a-standings,.a-feed{overflow:hidden", self.desktop())

    def test_only_long_lists_scroll(self):
        css = ps.CSS + ps.SCENE_CSS
        scrolling = set()
        for rule in css.replace("}", "}\n").splitlines():
            if "overflow:auto" in rule or "overflow-y:auto" in rule:
                scrolling.add(rule.split("{")[0])
        for sel in scrolling:
            self.assertTrue(all(x in (".st-list", ".feed-body") for x in sel.split(",")), sel)

    def test_mobile_scene_height_and_single_column(self):
        self.assertIn(".stage{position:relative;flex:none;width:100%;height:min(60vh,420px)}", ps.SCENE_CSS)
        base = ps.CSS.split("@media (min-width:900px)")[0]
        self.assertIn("grid-template-columns:minmax(0,1fr)", base)

    def test_fluid_sizes_use_clamp_with_viewport_units(self):
        d = self.desktop()
        self.assertIn("clamp(", d)
        self.assertRegex(d, r"clamp\([^)]*v[wh]")

    def test_charts_fill_their_box(self):
        out = ps.render_html(analysis_of({"A": 2}), "o/r", NOW, [snap(t(48), [1]), snap(t(24), [1, 2]), snap(t(1), [2, 3, 4])])
        self.assertEqual(out.count('class="chart-box"'), 2)
        self.assertIn('preserveAspectRatio="xMidYMid meet"', out)
        self.assertIn('viewBox="0 0 %d %d"' % (ps.CHART_W, ps.CHART_H), out)
        self.assertIn(".chart-box .chart{position:absolute", self.desktop())

    def test_trends_have_their_own_panel(self):
        out = ps.render_html(analysis_of({"A": 2}), "o/r", NOW)
        self.assertEqual(out.count('class="a-trends"'), 1)
        self.assertNotIn("Trends", out.split('class="a-city"')[1].split("</section>")[0])

    def test_scrollbars_are_thin(self):
        self.assertIn("scrollbar-width:thin", ps.CSS)
        self.assertIn("::-webkit-scrollbar{width:8px}", ps.CSS)


class RendererSourceTests(unittest.TestCase):
    """The canvas renderer cannot run here; these check the rules of the spec that show in its source."""
    def test_no_expensive_canvas_features(self):
        for bad in ("shadowBlur", "filter", "getImageData", "willReadFrequently"):
            self.assertNotIn(bad, ps.SCENE_JS)

    def test_dpr_cap_and_fps_cap(self):
        self.assertIn("Math.min(2,window.devicePixelRatio||1)", ps.SCENE_JS)
        self.assertIn("MS=30", ps.SCENE_JS)

    def test_pauses_when_hidden_or_off_screen(self):
        self.assertIn("IntersectionObserver", ps.SCENE_JS)
        self.assertIn("visibilitychange", ps.SCENE_JS)
        self.assertIn("function shown(){return vis&&!d.hidden;}", ps.SCENE_JS)

    def test_resize_observer_and_cached_layers(self):
        self.assertIn("new ResizeObserver(resized)", ps.SCENE_JS)
        self.assertNotIn("buildBase", ps.SCENE_JS)  # nothing static is left to paint: the canvas is transparent
        self.assertIn("function buildScene()", ps.SCENE_JS)
        self.assertIn("function composite()", ps.SCENE_JS)
        self.assertIn("ordYaw===sceneYaw", ps.SCENE_JS)  # painter order is sorted once per yaw

    def test_reduced_motion_stops_the_idle_pan(self):
        self.assertIn("prefers-reduced-motion: reduce", ps.SCENE_JS)
        self.assertIn("var auto=!reduce", ps.SCENE_JS)
        self.assertIn("function animating(){return auto&&!reduce&&!inside&&!drag;}", ps.SCENE_JS)

    def test_yaw_is_clamped_everywhere_it_changes(self):
        js = ps.SCENE_JS
        self.assertIn("function clampYaw(v){return Math.max(Y0-ARC,Math.min(Y0+ARC,v));}", js)
        self.assertIn("yaw=clampYaw(yaw+(e.clientX-drag.x)*.01)", js)  # drag
        self.assertIn("yaw=clampYaw(yaw+k*.12)", js)  # keys
        self.assertIn("yaw=clampYaw(Y0+Math.round(Math.sin(ph)*ARC*IDLE/Q)*Q)", js)  # idle ease
        self.assertEqual(len(re.findall(r"\byaw\s*(?:\+|-)?=(?!=)", js.replace("var yaw=Y0;", ""))), 3)  # no other write to yaw

    def test_input_stops_the_idle_pan(self):
        self.assertIn("drag={x:e.clientX};auto=false;", ps.SCENE_JS)
        self.assertIn("e.preventDefault();auto=false;", ps.SCENE_JS)

    def test_only_the_idle_pan_keeps_the_loop_running(self):
        self.assertIn("if(animating())sched();else last=0;", ps.SCENE_JS)
        self.assertNotIn("function animating(){return !reduce;}", ps.SCENE_JS)

    def test_zoom_is_applied_and_centred(self):
        self.assertIn("K*=ZOOM;LH*=ZOOM;", ps.SCENE_JS)
        self.assertIn("CX=W/2;", ps.SCENE_JS)
        self.assertEqual(ps.SCENE_ZOOM, 0.85)

    def test_transparent_canvas_clears_instead_of_painting_a_backdrop(self):
        self.assertIn("g.clearRect(0,0,cv.width,cv.height);g.drawImage(yl,0,0)", ps.SCENE_JS)
        for gone in ("sky", "water", "palm", "dune", "sand", "foam", "HOR", "waves", "decor", "trunk", "frond"):
            self.assertNotIn(gone, ps.SCENE_JS, gone)


class SceneTests(unittest.TestCase):
    def test_layer_order_and_counts(self):
        an = analysis_of({"A": 6})
        for i, p in enumerate(an["prs"]):  # 2 drafts-style variety
            p["isDraft"], p["looksDone"], p["unreviewed"] = (i == 0), (i in (1, 2)), (i != 3)
        t = ps.effort_scene(an)["towers"][0]
        self.assertEqual([(ly["k"], ly["n"]) for ly in t["layers"]], [("done", 2), ("rev", 1), ("wait", 2), ("draft", 1)])
        self.assertEqual(t["height"], 6)
        self.assertEqual((t["open"], t["unreviewed"]), (6, 5))

    def test_no_scaling_up_to_cap(self):
        sc = ps.effort_scene(analysis_of({"A": 40, "B": 3}))
        self.assertEqual(sc["scale"], 1.0)
        self.assertEqual(sc["towers"][0]["height"], 40)
        self.assertTrue(sc["caption"].startswith("One layer is one open PR."))
        self.assertIn("ghost", sc["caption"])

    def test_scaling_above_cap(self):
        sc = ps.effort_scene(analysis_of({"A": 80, "B": 1}))
        a, b = sc["towers"]
        self.assertEqual(sc["scale"], 0.5)
        self.assertEqual(a["height"], 40)
        self.assertEqual(b["height"], 1)  # a non-empty layer keeps one layer
        self.assertIn("about 2 open PRs", sc["caption"])
        self.assertIn("40", sc["caption"])
        self.assertEqual(sum(ly["n"] for ly in a["layers"]), 80)

    def test_svg_one_group_per_tower_and_escapes(self):
        an = analysis_of({"A": 2, "B<i>": 1})
        svg = ps.scene_svg(ps.effort_scene(an))
        self.assertEqual(svg.count('<g class="tower"'), 2)
        self.assertIn("B&lt;i&gt;", svg)
        self.assertNotIn("<i>", svg)
        self.assertEqual(svg.count('<g class="callout">'), 2)
        self.assertIn('role="img"', svg)
        self.assertEqual(ps.scene_svg(ps.effort_scene(analysis_of({}))), "")

    def test_json_block_escapes_script_end(self):
        sc = ps.effort_scene(analysis_of({"</script><b>": 1}))
        js = ps.scene_json(sc)
        self.assertNotIn("<", js)
        self.assertEqual(json.loads(js)["towers"][0]["name"], "</script><b>")
        out = ps.render_html(analysis_of({"</script><b>": 1}), "o/r", NOW)
        block = out.split('id="scene-data">')[1].split("</script>")[0]
        self.assertEqual(json.loads(block)["towers"][0]["open"], 1)

    def test_ghost_split(self):
        an = analysis_of({"A": 6})
        for i, p in enumerate(an["prs"]):
            p["isDraft"], p["looksDone"], p["unreviewed"] = (i == 0), (i in (1, 2)), (i != 3)
        sc = ps.effort_scene(an)
        ly = {x["k"]: x["ghost"] for x in sc["towers"][0]["layers"]}
        self.assertEqual(ly, {"done": False, "rev": False, "wait": True, "draft": True})
        data = json.loads(ps.scene_json(sc))
        self.assertEqual({s_["k"]: s_["ghost"] for s_ in data["states"]}, {"done": False, "rev": False, "wait": True, "draft": True})
        svg = ps.scene_svg(sc)
        for k, ghost in (("done", False), ("rev", False), ("wait", True), ("draft", True)):
            polys = [x for x in svg.split("<polygon") if 'class="vx v-%s ' % k in x]
            self.assertTrue(polys, k)
            self.assertEqual(all(" ghost" in x.split('"')[1] for x in polys), ghost, k)
        self.assertIn("ghost", sc["caption"])

    def test_yaw_range_is_a_180_degree_arc_around_the_default(self):
        lo, hi = ps.yaw_range()
        self.assertEqual((lo, hi), (ps.SCENE_YAW - 90, ps.SCENE_YAW + 90))
        self.assertEqual(hi - lo, 180)
        self.assertEqual(ps.clamp_yaw(ps.SCENE_YAW), ps.SCENE_YAW)
        self.assertEqual(ps.clamp_yaw(1e6), hi)
        self.assertEqual(ps.clamp_yaw(-1e6), lo)
        self.assertEqual(ps.clamp_yaw(lo + 1), lo + 1)
        geom = json.loads(ps.scene_json(ps.effort_scene(analysis_of({"A": 3}))))["geom"]
        self.assertEqual((geom["yaw"], geom["arc"], geom["zoom"]), (ps.SCENE_YAW, 90.0, 0.85))
        self.assertLess(0, geom["idle"])
        self.assertLessEqual(geom["idle"], 1)  # the idle ease stays inside the arc

    def test_scene_has_no_backdrop_decor(self):
        self.assertFalse(hasattr(ps, "scene_decor"))
        self.assertFalse(hasattr(ps, "SCENE_TOKENS"))
        an = analysis_of({"A": 3, "B": 2})
        svg = ps.scene_svg(ps.effort_scene(an))
        for gone in ('class="sky"', 'class="sea"', "palm", "dune", "water", "sand", "wv", 'class="sun"', "rim"):
            self.assertNotIn(gone, svg, gone)
        self.assertEqual(svg.count('<polygon class="plate"'), 1)
        self.assertEqual(svg, ps.scene_svg(ps.effort_scene(an)))  # deterministic
        self.assertNotIn("decor", json.loads(ps.scene_json(ps.effort_scene(an))))
        css = ps.CSS + ps.SCENE_CSS
        for gone in ("--sky-", "--sun:", "--sand:", "--dune:", "--water:", "--foam:", "--trunk:", "--frond:", ".palm", ".sk0"):
            self.assertNotIn(gone, css, gone)

    def test_page_gradient_tokens_in_both_themes(self):
        light = ps.CSS.split("@media")[0]
        dark = ps._DARK_TOKENS
        names = [n for n, _, _ in ps.GRADIENT_TOKENS]
        self.assertEqual(names, ["dusk-1", "dusk-2", "dusk-3", "dusk-4", "surface"])
        for name, a, b in ps.GRADIENT_TOKENS:
            self.assertIn("--%s:%s;" % (name, a), light)
            self.assertIn("--%s:%s;" % (name, b), dark)
            self.assertNotEqual(a, b)
        tok = {n: (a, b) for n, a, b in ps.GRADIENT_TOKENS}
        self.assertEqual([tok["dusk-%d" % i][0] for i in (1, 2, 3, 4)], ["#ffd8a8", "#ff9f80", "#e9849c", "#b39ddb"])
        self.assertEqual([tok["dusk-%d" % i][1] for i in (1, 2, 3, 4)], ["#3b2a5a", "#5a2f55", "#2a1f3d", "#0e1219"])
        self.assertEqual(tok["surface"], ("rgba(255,255,255,.78)", "rgba(14,18,25,.74)"))
        out = ps.render_html(analysis_of({"A": 1}), "o/r", NOW)
        self.assertIn(":root[data-theme=dark]{color-scheme:dark;" + dark.split(";")[0], out)
        self.assertIn("linear-gradient(180deg,var(--dusk-1),var(--dusk-2),var(--dusk-3),var(--dusk-4))", out)
        self.assertIn("position:fixed", out.split("body::before{")[1].split("}")[0])  # fixed to the viewport
        self.assertNotIn("backdrop-filter", out)

    def test_panels_and_top_bar_use_the_translucent_surface(self):
        css = ps.CSS
        self.assertIn(".a-trends,.a-road,.a-standings,.a-feed,.a-meta,.foot{background:var(--surface)}", css)
        self.assertRegex(css, r"\.top\{[^}]*background:var\(--surface\)")

    def test_text_keeps_aa_contrast_on_the_surface(self):
        def lin(c):
            c /= 255.0
            return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4

        def lum(rgb):
            return 0.2126 * lin(rgb[0]) + 0.7152 * lin(rgb[1]) + 0.0722 * lin(rgb[2])

        def hexrgb(h):
            return [int(h[i:i + 2], 16) for i in (1, 3, 5)]

        def over(surface, bg):  # surface "rgba(r,g,b,a)" over an opaque bg
            r, g, b, a = [float(x) for x in re.findall(r"[\d.]+", surface)]
            return [r * a + bg[0] * (1 - a), g * a + bg[1] * (1 - a), b * a + bg[2] * (1 - a)]

        def ratio(a, b):
            la, lb = sorted((lum(a), lum(b)), reverse=True)
            return (la + 0.05) / (lb + 0.05)
        tok = {n: (a, b) for n, a, b in ps.GRADIENT_TOKENS}
        text = {0: ("#10141c", "#566070", "#1546e0", "#a94f00", "#456b12", "#c4261d"),
                1: ("#eef2f8", "#a2adbc", "#8ca8ff", "#ffab5c", "#b4e06a", "#ff8a80")}
        for theme in (0, 1):
            for i in (1, 2, 3, 4):
                bg = over(tok["surface"][theme], hexrgb(tok["dusk-%d" % i][theme]))
                for fg in text[theme]:
                    self.assertGreaterEqual(ratio(hexrgb(fg), bg), 4.5, (theme, i, fg))

    def test_page_empty_state_without_prs(self):
        out = ps.render_html(analysis_of({}), "o/r", NOW)
        self.assertNotIn('id="stage"', out)
        self.assertIn("No data yet", out.split('class="a-city"')[1].split("</section>")[0])


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
