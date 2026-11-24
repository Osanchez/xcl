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
        self.assertEqual(out.count("No data yet"), 3)  # the bars, the merged-per-day chart and the Closed tab (no closed-PR data in this analysis)
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
        for k in ("dup", "ready", "hot", "all", "closed"):
            self.assertEqual(out.count('data-panel="%s"' % k), 1)
        self.assertNotIn('class="panel" hidden', out)
        self.assertIn(".js .tabs{display:flex}", out)  # tab buttons only exist with JS
        self.assertEqual(out.count('<div class="panel"'), 5)

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
        an = analysis_of({"A%d" % i: 10 - i for i in range(9)})
        out = ps.render_html(an, "o/r", NOW)
        st = out.split('class="a-standings"')[1].split("</section>")[0]
        self.assertEqual(st.count('<li class="st">'), 8)
        self.assertIn('<span class="nm">Other</span>', st)
        self.assertNotIn("Everything else", out)
        self.assertIn("2 areas: A7, A8", st)
        sel = out.split('<select id="area">')[1].split("</select>")[0]
        for i in range(9):
            self.assertIn(">A%d (" % i, sel)
        self.assertNotIn('class="chip"', out)


class GroupTests(unittest.TestCase):
    CORE = ["kernelx", "xcl run", "Runtime", "VFS", "TLS", "PE loader", "Memory"]
    TOOLING = ["Scripts", "CI", "README", "Docs"]

    def test_membership(self):
        for a in self.CORE:
            self.assertEqual(ps.group_of(a), "Core runtime", a)
        for a in self.TOOLING:
            self.assertEqual(ps.group_of(a), "Tooling", a)
        for a in ("Recompiler", "D3D12.X", "Tests/e2e", "XGameRuntime", "HLE", "GPU", "Other", "XAudio2", "Input", "Shaders"):
            self.assertEqual(ps.group_of(a), a, a)
        self.assertEqual(ps.TOP_AREAS, 7)

    def test_top_7_ranking_with_groups(self):
        counts = {"Recompiler": 30, "D3D12.X": 28, "kernelx": 6, "VFS": 5, "TLS": 4, "Memory": 3, "Scripts": 8, "CI": 4, "Docs": 3,
                  "Tests/e2e": 12, "XGameRuntime": 11, "HLE": 10, "GPU": 9, "Input": 2, "Other": 1}
        t = ps.top_areas(analysis_of(counts))
        self.assertEqual([(e["name"], len(e["prs"])) for e in t],
                         [("Recompiler", 30), ("D3D12.X", 28), ("Core runtime", 18), ("Tooling", 15), ("Tests/e2e", 12),
                          ("XGameRuntime", 11), ("HLE", 10), (ps.ELSE_NAME, 12)])
        self.assertEqual(t[2]["members"], ["kernelx", "VFS", "TLS", "Memory"])  # only member areas with PRs, ranked
        self.assertEqual(t[3]["members"], ["Scripts", "CI", "Docs"])
        self.assertIsNone(t[0]["members"])

    def test_everything_else_contents(self):
        counts = {"Recompiler": 30, "D3D12.X": 28, "kernelx": 6, "VFS": 5, "Scripts": 8, "Tests/e2e": 12, "XGameRuntime": 11,
                  "HLE": 10, "GPU": 9, "Input": 2, "Other": 1, "README": 1}
        t = ps.top_areas(analysis_of(counts))
        self.assertEqual([e["name"] for e in t][:-1], ["Recompiler", "D3D12.X", "Tests/e2e", "Core runtime", "XGameRuntime", "HLE", "GPU"])
        last = t[-1]
        self.assertEqual(last["name"], ps.ELSE_NAME)
        self.assertEqual(last["members"], ["Scripts", "Input", "Other", "README"])
        self.assertEqual(len(last["prs"]), 8 + 2 + 1 + 1)

    def test_a_group_can_land_in_everything_else_as_its_member_areas(self):
        counts = {"A%d" % i: 20 - i for i in range(7)}
        counts.update({"Docs": 2, "README": 1})
        t = ps.top_areas(analysis_of(counts))
        self.assertEqual(t[-1]["members"], ["Docs", "README"])
        self.assertEqual(t[-1]["name"], ps.ELSE_NAME)
        self.assertNotIn("Tooling", [e["name"] for e in t])

    def test_other_label(self):
        self.assertEqual(ps.area_label("Other"), "Other")
        self.assertEqual(ps.area_label("VFS"), "VFS")
        out = ps.render_html(analysis_of({"Other": 2, "A": 1}), "o/r", NOW)
        self.assertIn(">Other<", out)
        self.assertNotIn("unmapped paths", out)

    def test_select_optgroups_and_member_filter(self):
        out = ps.render_html(analysis_of({"Recompiler": 9, "kernelx": 4, "VFS": 3, "Scripts": 2}), "o/r", NOW)
        sel = out.split('<select id="area">')[1].split("</select>")[0]
        self.assertIn('<optgroup label="Core runtime"><option value="%s" data-members="%s,%s">All Core runtime (7)</option>' % (
            ps.slug("Core runtime"), ps.slug("kernelx"), ps.slug("VFS")), sel)
        self.assertIn('<option value="%s">kernelx (4)</option>' % ps.slug("kernelx"), sel)
        self.assertIn('<option value="%s">VFS (3)</option>' % ps.slug("VFS"), sel)
        self.assertIn('<optgroup label="Tooling">', sel)
        # the filter on one member area: rows carry the area slug, the option has no data-members, so only that area matches
        self.assertEqual(out.count('class="row pr" data-area="%s"' % ps.slug("kernelx")), 4)
        self.assertNotIn('value="%s" data-members' % ps.slug("kernelx"), sel)
        self.assertIn("a===st.area", ps.JS)
        self.assertIn("data-members", ps.JS)  # a group option matches any of its members


class ToolbarTests(unittest.TestCase):
    def page(self, counts):
        return ps.render_html(analysis_of(counts), "o/r", NOW)

    def select(self, out):
        return out.split('<select id="area">')[1].split("</select>")[0]

    def test_select_lists_every_area_once(self):
        counts = {"A%d" % i: 12 - i for i in range(10)}
        out = self.page(counts)
        sel = self.select(out)
        self.assertTrue(sel.startswith('<option value="">All areas</option>'))
        for a in counts:
            self.assertEqual(sel.count('value="%s"' % ps.slug(a)), 1, a)
        self.assertEqual(sel.count("<option "), 1 + 7 + 1 + 3)  # All, top 7, "All Other", the 3 inside it
        self.assertIn('<optgroup label="Other"><option value="%s" data-members="%s">All Other (12)</option>' % (
            ps.ROLLUP_SLUG, ",".join(ps.slug(a) for a in ("A7", "A8", "A9"))), sel)
        grp = sel.split("<optgroup")[1]
        self.assertEqual(grp.count("<option "), 4)
        self.assertLess(sel.index('value="%s"' % ps.slug("A6")), sel.index(ps.ROLLUP_SLUG))

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
        seg = out.split('id="states"')[1].split("</div>")[0]
        self.assertEqual(re.findall(r">([^<]+)</button>", seg), ["All", "Unreviewed", "Ready", "Draft", "Closed"])
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
        self.assertIn("'unrev','done','draft','closed'", ps.JS)  # old draft=1 / unrev=1 / done=1 links still parse


class FitTests(unittest.TestCase):
    def desktop(self):
        css = ps.CSS + ps.SCENE_CSS
        a = css.index("@media (min-width:900px){\n:root")
        return css[a:css.index("\n}\n", a)]

    def test_body_never_scrolls_on_desktop(self):
        d = self.desktop()
        self.assertIn(".js,.js body{height:100%;overflow:hidden}", d)  # html has the js class; without JS the page scrolls
        self.assertIn("height:100dvh", d)
        self.assertNotIn("min-height:660px", ps.CSS + ps.SCENE_CSS)

    def test_scene_panel_never_scrolls(self):
        self.assertNotIn("overflow:auto", ps.SCENE_CSS)
        self.assertNotIn("overflow-y:auto", ps.SCENE_CSS)
        self.assertNotIn("overflow:scroll", ps.SCENE_CSS)
        self.assertIn(".js .a-city,.js .a-trends,.js .a-road,.js .a-standings,.js .a-feed{overflow:hidden", self.desktop())

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
        self.assertEqual(out.count('class="chart-box"'), 3)  # open and unreviewed, opened and closed, merged per day
        self.assertIn('preserveAspectRatio="xMidYMid meet"', out)
        self.assertIn('viewBox="0 0 %d %d"' % ps.CHART_WIDE, out)
        self.assertIn('viewBox="0 0 %d %d"' % ps.CHART_HALF, out)
        self.assertIn(".js .chart-box .chart{position:absolute", self.desktop())

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
        self.assertIn("function shown(){return vis&&!d.hidden&&N>0;}", ps.SCENE_JS)

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
        self.assertEqual({s_["k"]: s_["ghost"] for s_ in data["states"]},
                         {"merged": False, "done": False, "rev": False, "wait": True, "draft": True})
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

    def test_light_is_plain_white_and_only_dark_has_the_sunset(self):
        light = ps.CSS.split("@media")[0]
        dark = ps._DARK_TOKENS
        tok = {n: (a, b) for n, a, b in ps.GRADIENT_TOKENS}
        self.assertEqual([n for n, _, _ in ps.GRADIENT_TOKENS], ["sky", "surface", "edge"])
        for name, a, b in ps.GRADIENT_TOKENS:
            self.assertIn("--%s:%s;" % (name, a), light)
            self.assertIn("--%s:%s;" % (name, b), dark)
        self.assertIn("--page:#fff;", light)
        self.assertEqual(tok["sky"][0], "none")  # no gradient in light
        self.assertEqual(tok["surface"], ("#fff", "rgba(14,18,25,.7)"))  # flat white panels in light
        self.assertEqual(ps.SUNSET_DARK, ("#ff7a1a", "#c2185b", "#4a148c", "#120b2e"))
        self.assertEqual(tok["sky"][1], "linear-gradient(180deg,#ff7a1a,#c2185b,#4a148c,#120b2e)")
        self.assertNotIn("dusk", ps.CSS + ps.SCENE_CSS)
        out = ps.render_html(analysis_of({"A": 1}), "o/r", NOW)
        # explicit light and auto with the OS in light mode both use the base :root tokens; dark applies only on the
        # explicit choice or auto + dark OS
        self.assertIn("@media (prefers-color-scheme:dark){:root:not([data-theme=light]){" + dark, out)
        self.assertIn(":root[data-theme=dark]{color-scheme:dark;" + dark, out)
        self.assertIn(":root[data-theme=light]{color-scheme:light}", out)
        self.assertEqual(out.count("--sky:linear-gradient"), 2)
        self.assertEqual(out.count("--sky:none"), 1)
        self.assertIn("background:var(--sky)", out.split("body::before{")[1].split("}")[0])
        self.assertIn("position:fixed", out.split("body::before{")[1].split("}")[0])  # fixed to the viewport
        self.assertNotIn("backdrop-filter", out)

    def test_panels_and_top_bar_use_the_surface_token(self):
        css = ps.CSS
        self.assertIn(".a-trends,.a-road,.a-standings,.a-feed,.a-meta,.foot{background:var(--surface);border:1px solid var(--edge)}", css)
        self.assertRegex(css, r"\.top\{[^}]*background:var\(--surface\)")

    def test_text_keeps_aa_contrast(self):
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
        light_text = ("#10141c", "#566070", "#1546e0", "#a94f00", "#456b12", "#c4261d")
        dark_text = ("#eef2f8", "#a2adbc", "#8ca8ff", "#ffab5c", "#b4e06a", "#ff8a80")
        for fg in light_text:  # light: text on the flat white surface and page
            self.assertGreaterEqual(ratio(hexrgb(fg), [255, 255, 255]), 4.5, ("light", fg))
        for stop in ps.SUNSET_DARK:  # dark: text on the translucent surface over every sunset stop
            bg = over(tok["surface"][1], hexrgb(stop))
            for fg in dark_text:
                self.assertGreaterEqual(ratio(hexrgb(fg), bg), 4.5, (stop, fg))

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


class ClosedTests(unittest.TestCase):
    VFS = ["crates/xcl/src/vfs/shared.rs"]

    def cpr(self, number, merged, days_ago=1, title="VFS: shared", files=None, **kw):
        end = (NOW - dt.timedelta(days=days_ago)).strftime(ps.TS_FMT)
        d = pr(number, title, self.VFS if files is None else files, state="MERGED" if merged else "CLOSED",
               closedAt=end, mergedAt=end if merged else None)
        d.update(kw)
        return d

    def an(self, closed, opens=()):
        return ps.build_analysis(list(opens), NOW, closed)

    def test_normalize_merged_vs_closed(self):
        base = {"number": 5, "title": "t", "url": "u", "createdAt": "2026-11-01T00:00:00Z", "updatedAt": "2026-11-02T00:00:00Z",
                "closedAt": "2026-11-20T00:00:00Z"}
        m = ps.normalize(dict(base, state="CLOSED", mergedAt="2026-11-20T00:00:00Z"))
        c = ps.normalize(dict(base, state="CLOSED", mergedAt=None))
        self.assertEqual((m["state"], m["mergedAt"], m["closedAt"]), ("MERGED", "2026-11-20T00:00:00Z", "2026-11-20T00:00:00Z"))
        self.assertEqual((c["state"], c["mergedAt"], c["closedAt"]), ("CLOSED", None, "2026-11-20T00:00:00Z"))
        self.assertEqual(ps.normalize(dict(base, state="OPEN"))["state"], "OPEN")
        self.assertIn("closedAt", ps.PR_FIELDS)

    def test_closed_window(self):
        prs = [self.cpr(1, True, 1), self.cpr(2, False, 0), self.cpr(3, True, 30), self.cpr(4, False, 31), pr(5, state="OPEN"),
               self.cpr(6, True, 1)]
        out = ps.closed_window(prs, NOW)
        self.assertEqual([p["number"] for p in out], [2, 1, 6, 3])  # newest first, ties by number; 31 d old and open dropped, 30 d kept
        self.assertEqual(ps.CLOSED_DAYS, 30)
        self.assertEqual(ps.closed_window([], NOW), [])

    def test_analysis_counts(self):
        an = self.an([self.cpr(1, True), self.cpr(2, False), self.cpr(3, True, 31)])
        self.assertEqual([p["number"] for p in an["closed"]], [1, 2])
        self.assertEqual([p["number"] for p in an["mergedPrs"]], [1])
        self.assertTrue(an["closed"][0]["merged"])
        self.assertFalse(an["closed"][1]["merged"])
        self.assertIsNone(self.an(None)["closed"])
        self.assertIsNone(self.an(None)["mergedPrs"])

    def test_query_and_failure(self):
        seen = []
        orig = ps.fetch_paged
        try:
            ps.fetch_paged = lambda q, v, g: seen.append(v["q"]) or []
            self.assertEqual(ps.fetch_closed("o/r", NOW), [])
            def boom(*a):
                raise SystemExit("nope")
            ps.fetch_paged = boom
            self.assertIsNone(ps.fetch_closed("o/r", NOW))
        finally:
            ps.fetch_paged = orig
        self.assertEqual(seen, ["repo:o/r is:pr is:closed closed:>=2026-10-24"])

    def test_scene_merged_base_layer(self):
        opens = [pr(1, "VFS: a", ["crates/xcl/src/vfs/a.rs"], isDraft=True)]
        an = self.an([self.cpr(2, True), self.cpr(3, True), self.cpr(4, False)], opens)
        sc = ps.effort_scene(an)
        t = sc["towers"][0]
        self.assertEqual(t["name"], "Core runtime")
        self.assertEqual([(ly["k"], ly["n"]) for ly in t["layers"]], [("merged", 2), ("draft", 1)])  # merged at the bottom
        self.assertFalse(t["layers"][0]["ghost"])
        self.assertEqual((t["open"], t["merged"], t["closed"], t["height"]), (1, 2, 1, 3))  # closed-unmerged is not stacked
        self.assertTrue(sc["hasClosed"])
        self.assertIn("violet base", sc["caption"])
        self.assertTrue(sc["caption"].startswith("One layer is one PR."))
        svg = ps.scene_svg(sc)
        self.assertIn("2 merged \u00b7 1 closed (7 d)", svg)
        self.assertIn('class="vx v-merged ', svg)
        data = json.loads(ps.scene_json(sc))
        self.assertTrue(data["hasClosed"])
        self.assertEqual(data["states"][0]["k"], "merged")
        self.assertEqual(data["states"][0]["v"], "--merged")

    def test_scene_scaling_counts_merged(self):
        an = self.an([self.cpr(100 + i, True) for i in range(60)], [pr(i, "VFS: a", ["crates/xcl/src/vfs/a%d.rs" % i]) for i in range(1, 41)])
        t = ps.effort_scene(an)["towers"][0]
        self.assertEqual(t["height"], 40)
        self.assertEqual(sum(ly["n"] for ly in t["layers"]), 100)

    def test_scene_without_closed_data_is_unchanged(self):
        sc = ps.effort_scene(ps.build_analysis([pr(1, "VFS: a", ["crates/xcl/src/vfs/a.rs"])], NOW))
        self.assertFalse(sc["hasClosed"])
        self.assertEqual([ly["k"] for ly in sc["towers"][0]["layers"]], ["done"])
        self.assertNotIn("merged", ps.scene_svg(sc))
        self.assertNotIn("violet", sc["caption"])

    def test_merged_per_tower_and_rollup(self):
        counts = {"A%d" % i: 3 for i in range(8)}
        an = analysis_of(counts)
        an["closed"] = [dict(pr(900 + i, "x", state="MERGED"), primary=a, merged=True, areas=[a])
                        for i, a in enumerate(["A0", "A0", "A7", "Gone"])]
        an["closed"].append(dict(pr(990, "x", state="CLOSED"), primary="A7", merged=False, areas=["A7"]))
        an["mergedPrs"] = [p for p in an["closed"] if p["merged"]]
        top = {e["name"]: e for e in ps.top_areas(an)}
        self.assertEqual(len(top["A0"]["merged"]), 2)
        self.assertEqual((len(top["A7"]["merged"]), len(top["A7"]["closed"])), (1, 1))  # ranks by merged after open
        self.assertEqual((len(top["Other"]["merged"]), len(top["Other"]["closed"])), (1, 0))  # the merged-only area rolls up
        self.assertEqual(sum(len(e["merged"]) for e in top.values()), 4)  # no merged PR is lost

    def test_closed_tab_markup(self):
        an = self.an([self.cpr(10, True, 3, "VFS: old <b>x</b>", author="al<i>"), self.cpr(11, False, 1, "TLS: new", ["crates/xcl/src/tls/a.rs"])])
        out = ps.render_html(an, "o/r", NOW)
        panel = out.split('data-panel="closed"')[1].split("</section>")[0]
        self.assertIn('data-tab="closed" aria-pressed="false">Closed</button>', out)
        self.assertIn('data-meta="1 merged, 1 closed, last 30 d"', out)
        self.assertLess(panel.index("#11"), panel.index("#10"))  # newest first
        self.assertEqual(panel.count('<div class="row cl rg" '), 2)
        self.assertIn('<span class="pill merged">merged</span>', panel)
        self.assertIn('<span class="pill muted">closed</span>', panel)
        self.assertIn("<b>3d</b>ago", panel)
        self.assertIn("by al&lt;i&gt;", panel)
        self.assertNotIn("<b>x</b>", out)
        self.assertIn("TLS", panel)

    def test_closed_empty_and_no_data(self):
        out = ps.render_html(self.an([]), "o/r", NOW)
        self.assertIn("None in the last 30 d", out)
        self.assertIn('data-meta="0 merged, 0 closed, last 30 d"', out)
        out = ps.render_html(self.an(None), "o/r", NOW)
        panel = out.split('data-panel="closed"')[1].split("</section>")[0]
        self.assertIn("No data yet", panel)
        self.assertIn('data-meta="no data yet"', out)
        self.assertNotIn('class="vx v-merged', out)
        self.assertNotIn('<i class="merged"', out)
        self.assertNotIn("sw k-merged", out)

    def test_closed_state_filter(self):
        an = self.an([self.cpr(10, True)], [pr(1, "VFS: a", ["crates/xcl/src/vfs/a.rs"])])
        out = ps.render_html(an, "o/r", NOW)
        seg = out.split('id="states"')[1].split("</div>")[0]
        self.assertIn('data-state="closed"', seg)
        self.assertLess(seg.index('data-state="draft"'), seg.index('data-state="closed"'))
        self.assertIn("STATES=['unrev','done','draft','closed']", ps.JS)
        self.assertIn("data-cl", ps.JS)
        self.assertIn("wc=st.state==='closed'", ps.JS)
        self.assertEqual(out.count('class="row pr" data-area="area-vfs" data-cl="0"'), 1)
        self.assertEqual(out.count('class="row pr" hidden data-area="area-vfs" data-cl="1"'), 1)  # hidden until Closed is chosen

    def test_closed_only_area_group_hidden(self):
        out = ps.render_html(self.an([self.cpr(10, True, files=["crates/xcl/src/tls/a.rs"], title="TLS: a")]), "o/r", NOW)
        self.assertIn('<details class="area" id="area-tls" hidden>', out)

    def test_standings_merged_segment(self):
        an = self.an([self.cpr(10, True), self.cpr(11, True)], [pr(1, "VFS: a", ["crates/xcl/src/vfs/a.rs"])])
        st = ps.render_html(an, "o/r", NOW).split('class="a-standings"')[1].split("</section>")[0]
        self.assertIn('<i class="merged" style="flex:2" data-tip="Merged in 7 d: 2"', st)
        self.assertIn("2 merged", st)
        self.assertIn("Merged in 7 d</li>", st)
        self.assertIn("<b>1</b>", st)  # the open count stays the number

    def test_superseded_marking(self):
        opens = [pr(1, "VFS: shared", self.VFS)]
        an = self.an([self.cpr(2, True)], opens)
        self.assertEqual(len(an["clusters"]), 1)
        c = an["clusters"][0]
        self.assertEqual((c["members"], c["merged"], c["superseded"]), ([1, 2], [2], True))
        out = ps.render_html(an, "o/r", NOW)
        dup = out.split('data-panel="dup"')[1].split('data-panel="ready"')[0]
        self.assertIn("Possibly superseded: ", dup)
        self.assertIn("#2</a> merged", dup)
        self.assertIn('<span class="pill merged">merged</span>', dup)  # the merged member is listed with the tag
        md = ps.render_md(an, "o/r", NOW)  # closed members are in the markdown cluster list too
        self.assertIn("[merged]", md)

    def test_closed_unmerged_member_is_not_superseded(self):
        an = self.an([self.cpr(2, False)], [pr(1, "VFS: shared", self.VFS)])
        c = an["clusters"][0]
        self.assertEqual((c["members"], c["merged"], c["superseded"]), ([1, 2], [], False))
        self.assertNotIn("Possibly superseded", ps.render_html(an, "o/r", NOW))

    def test_closed_only_clusters_dropped(self):
        an = self.an([self.cpr(2, True), self.cpr(3, False), self.cpr(4, True)])
        self.assertEqual(an["clusters"], [])
        out = ps.render_html(an, "o/r", NOW)
        self.assertIn("None found", out)
        self.assertEqual(ps.mark_superseded([{"members": [2, 3], "links": []}], {2: {"state": "MERGED", "merged": True}, 3: {"state": "CLOSED"}}), [])

    def test_open_only_clusters_unchanged(self):
        an = ps.build_analysis([pr(1, "VFS: shared", self.VFS), pr(2, "VFS: shared", self.VFS)], NOW)
        self.assertEqual(an["clusters"][0]["members"], [1, 2])
        self.assertFalse(an["clusters"][0]["superseded"])

    def test_snapshot_counts_open_only(self):
        an = self.an([self.cpr(10, True)], [pr(1, "VFS: a", ["crates/xcl/src/vfs/a.rs"])])
        snap_ = ps.snapshot(an, NOW)
        self.assertEqual((snap_["open"], snap_["prs"], snap_["areas"]), (1, [1], {"VFS": 1}))

    def test_merged_colour_tokens_are_aa(self):
        def lum(h):
            c = [int(h[i:i + 2], 16) / 255 for i in (1, 3, 5)]
            c = [x / 12.92 if x <= .03928 else ((x + .055) / 1.055) ** 2.4 for x in c]
            return .2126 * c[0] + .7152 * c[1] + .0722 * c[2]
        def ratio(a, b):
            a, b = sorted((lum(a), lum(b)), reverse=True)
            return (a + .05) / (b + .05)
        light = re.search(r"--merged:(#[0-9a-f]{6});", ps.CSS.split("prefers-color-scheme")[0]).group(1)
        dark = re.search(r"--merged:(#[0-9a-f]{6});", ps.CSS.split("prefers-color-scheme")[1]).group(1)
        self.assertGreaterEqual(ratio(light, "#ffffff"), 4.5)
        self.assertGreaterEqual(ratio(dark, "#0e1219"), 4.5)
        self.assertGreaterEqual(ratio(dark, "#1a212c"), 4.5)
        self.assertIn(".v-merged{--c:var(--merged)}", ps.CSS + ps.SCENE_CSS)

    def test_other_naming_everywhere(self):
        counts = {"A%d" % i: 12 - i for i in range(8)}
        counts["Other"] = 1
        an = analysis_of(counts)
        an["closed"] = [dict(pr(900, "Other: x", state="CLOSED"), primary="Other", merged=False, areas=["Other"], closedAgeDays=1)]
        an["mergedPrs"] = []
        out = ps.render_html(an, "o/r", NOW)
        md = ps.render_md(an, "o/r", NOW)
        for text in (out, md, ps.scene_svg(ps.effort_scene(an)), ps.scene_json(ps.effort_scene(an))):
            self.assertNotIn("Everything else", text)
            self.assertNotIn("unmapped paths", text)
            self.assertNotIn("(unmapped", text)
        sel = out.split('<select id="area">')[1].split("</select>")[0]
        self.assertIn('<optgroup label="Other">', sel)
        self.assertIn(">All Other (", sel)
        self.assertEqual(sel.count('value="area-other"'), 1)  # the unmapped area, in the rollup
        self.assertEqual(sel.count('value="%s"' % ps.ROLLUP_SLUG), 1)  # the rollup has its own value
        self.assertIn("| [Other](#area-other) |", md)
        self.assertEqual(ps.ELSE_NAME, "Other")
        self.assertEqual(ps.area_label(ps.OTHER), "Other")


class ClusterGuardTests(unittest.TestCase):
    def grp(self, start, n, files):
        return [pr(start + i, "w%da w%db" % (start + i, start + i), files) for i in range(n)]

    def test_hot_files_use_the_clustered_set(self):
        hub = "crates/xcl/src/vfs/hub.rs"
        opens = [pr(i, "o%da o%db" % (i, i), [hub, "crates/xcl/src/vfs/o%d.rs" % i]) for i in range(1, 4)]
        closed = []
        for i in range(10, 20):
            end = (NOW - dt.timedelta(days=1)).strftime(ps.TS_FMT)
            closed.append(pr(i, "c%da c%db" % (i, i), [hub], state="CLOSED", closedAt=end, mergedAt=None))
        open_only = ps.build_analysis(opens, NOW)
        self.assertEqual(open_only["hotFiles"], [])  # 3 open PRs: not above max(4, 5 %)
        an = ps.build_analysis(opens, NOW, closed)
        self.assertEqual(an["hotFiles"], [])  # the Hot files tab stays about open PRs
        self.assertEqual(an["clusterHot"], [hub])  # 13 PRs touch it: above max(4, 5 % of 13)
        self.assertEqual(an["clusters"], [])  # so the closed PRs do not chain the open ones together

    def test_resplit_oversized_component(self):
        a = self.grp(100, 8, ["a1", "a2", "a3", "a4"])
        b = self.grp(200, 8, ["b1", "b2", "b3", "b4"])
        bridge_a = pr(300, "bridge shared words", ["a1", "a2"])  # 2/4 = 0.5 with group A: links only at the base threshold
        bridge_b = pr(301, "bridge shared words", ["b1", "b2"])
        cs = ps.clusters(a + b + [bridge_a, bridge_b])
        self.assertEqual([c["members"] for c in cs], [list(range(100, 108)), list(range(200, 208)), [300, 301]])
        # the bridges are no longer glued to the groups; they stay together only through their identical title
        self.assertEqual(len(ps.clusters(a + b + [bridge_a, bridge_b], max_size=30)), 1)  # within the limit: one component
        weak_a, weak_b = pr(302, "pp qq rr ss", ["a1", "a2"]), pr(303, "pp qq rr tt", ["b1", "b2"])  # title Jaccard 0.6
        lone = ps.clusters(a + b + [weak_a, weak_b])
        self.assertEqual([c["members"] for c in lone], [list(range(100, 108)), list(range(200, 208))])  # single-member parts are dropped

    def test_resplit_stops_at_the_cap(self):
        same = self.grp(1, 14, ["x1", "x2"])  # identical files stay linked at every threshold
        cs = ps.clusters(same)
        self.assertEqual([len(c["members"]) for c in cs], [14])

    def test_small_components_untouched(self):
        cs = ps.clusters(self.grp(1, 12, ["x1", "x2"]))
        self.assertEqual([len(c["members"]) for c in cs], [12])
        self.assertEqual(ps.CLUSTER_MAX, 12)

    def test_snapshot_clusters_have_an_open_pr(self):
        end = (NOW - dt.timedelta(days=1)).strftime(ps.TS_FMT)
        closed = [pr(5, "VFS: x", ["crates/xcl/src/vfs/z.rs"], state="MERGED", closedAt=end, mergedAt=end),
                  pr(6, "VFS: x", ["crates/xcl/src/vfs/z.rs"], state="CLOSED", closedAt=end, mergedAt=None)]
        an = ps.build_analysis([pr(1, "VFS: y", ["crates/xcl/src/vfs/q.rs"])], NOW, closed)
        self.assertEqual(ps.snapshot(an, NOW)["clusters"], 0)  # a closed-only cluster is dropped, not counted


class OtherTowerTests(unittest.TestCase):
    def test_unmapped_area_never_its_own_tower(self):
        for counts in ({"Other": 30, "A": 1}, {"Other": 5}, {"Other": 9, "A": 3, "B": 2}):
            top = ps.top_areas(analysis_of(counts))
            others = [e for e in top if e["name"] == "Other"]
            self.assertEqual(len(others), 1, counts)
            self.assertTrue(others[0]["rollup"], counts)
            self.assertIn("Other", others[0]["members"])
            tw = [t["name"] for t in ps.effort_scene(analysis_of(counts))["towers"]]
            self.assertEqual(tw.count("Other"), 1, counts)

    def test_rollup_holds_unmapped_and_overflow_areas(self):
        counts = {"A%d" % i: 20 - i for i in range(8)}
        counts["Other"] = 50
        top = ps.top_areas(analysis_of(counts))
        self.assertEqual([e["name"] for e in top].count("Other"), 1)
        self.assertEqual(top[-1]["members"], ["Other", "A7"])
        self.assertEqual(len(top[-1]["prs"]), 50 + 13)
        self.assertEqual(len(top), 8)


def ago(**kw):
    return (NOW - dt.timedelta(**kw)).strftime(ps.TS_FMT)


class RangeTests(unittest.TestCase):
    VFS = ["crates/xcl/src/vfs/shared.rs"]

    def opn(self, number, created, updated, **kw):
        return pr(number, "VFS: a%d" % number, ["crates/xcl/src/vfs/f%d.rs" % number], createdAt=created, updatedAt=updated, **kw)

    def cl(self, number, when, merged=True, **kw):
        d = pr(number, "TLS: c%d" % number, ["crates/xcl/src/tls/c%d.rs" % number], state="MERGED" if merged else "CLOSED",
               closedAt=when, mergedAt=when if merged else None, createdAt=ago(days=40), updatedAt=ago(days=40))
        d.update(kw)
        return d

    def test_ranges_and_default(self):
        self.assertEqual([(k, s_, lb) for k, s_, lb in ps.RANGES],
                         [("1h", 3600, "1 h"), ("12h", 43200, "12 h"), ("24h", 86400, "24 h"), ("7d", 604800, "7 d"), ("30d", 2592000, "30 d")])
        self.assertEqual(ps.DEFAULT_RANGE, "7d")
        self.assertEqual(ps.empty_range("1 h"), "No PRs in the last 1 h.")

    def test_in_range_edges_for_each_range(self):
        for key, secs, _ in ps.RANGES:
            edge = NOW - dt.timedelta(seconds=secs)
            fmt = lambda t: t.strftime(ps.TS_FMT)
            old = fmt(edge - dt.timedelta(seconds=1))
            at = fmt(edge)
            # open: created or updated counts; the boundary is inclusive, one second older is out
            self.assertTrue(ps.in_range(self.opn(1, old, at), secs, NOW), key)
            self.assertTrue(ps.in_range(self.opn(1, at, old), secs, NOW), key)
            self.assertFalse(ps.in_range(self.opn(1, old, old), secs, NOW), key)
            self.assertTrue(ps.in_range(self.opn(1, old, fmt(NOW)), secs, NOW), key)
            # merged: mergedAt; closed: closedAt
            self.assertTrue(ps.in_range(self.cl(2, at), secs, NOW), key)
            self.assertFalse(ps.in_range(self.cl(2, old), secs, NOW), key)
            self.assertTrue(ps.in_range(self.cl(3, at, merged=False), secs, NOW), key)
            self.assertFalse(ps.in_range(self.cl(3, old, merged=False), secs, NOW), key)

    def test_in_range_closed_ignores_creation_and_missing_times(self):
        c = self.cl(1, ago(days=40))
        c["createdAt"] = c["updatedAt"] = ago(minutes=1)  # recently touched, but it ended long ago
        self.assertFalse(ps.in_range(c, 3600, NOW))
        self.assertFalse(ps.in_range(dict(c, mergedAt=None, closedAt=None), 30 * 86400, NOW))
        self.assertTrue(ps.in_range(pr(5), 7 * 86400, NOW))  # no "state" means open

    def an(self):
        opens = [self.opn(1, ago(days=20), ago(minutes=30)),      # updated 30 min ago
                 self.opn(2, ago(hours=5), ago(hours=5)),         # 5 h
                 self.opn(3, ago(days=3), ago(days=3)),           # 3 d
                 self.opn(4, ago(days=20), ago(days=20)),         # 20 d
                 self.opn(5, ago(days=60), ago(days=60))]         # older than every range
        closed = [self.cl(10, ago(minutes=10)), self.cl(11, ago(hours=11), merged=False), self.cl(12, ago(hours=20)),
                  self.cl(13, ago(days=6)), self.cl(14, ago(days=25), merged=False)]
        return ps.build_analysis(opens, NOW, closed)

    def test_counts_per_range(self):
        an = self.an()
        got = {k: ps.range_counts(an, secs, NOW) for k, secs, _ in ps.RANGES}
        self.assertEqual({k: (c["active"], c["merged"], c["closed"]) for k, c in got.items()},
                         {"1h": (1, 1, 0), "12h": (2, 1, 1), "24h": (2, 2, 1), "7d": (3, 3, 1), "30d": (4, 3, 2)})
        self.assertEqual(len(an["prs"]), 5)  # the open total never follows the range

    def test_scene_towers_follow_the_range(self):
        an = self.an()
        for key, secs, rl in ps.RANGES:
            view = ps.range_view(an, secs, NOW)
            sc = ps.effort_scene(view, rl=rl)
            c = ps.range_counts(an, secs, NOW)
            self.assertEqual(sum(t["open"] for t in sc["towers"]), c["active"], key)
            self.assertEqual(sum(t["merged"] for t in sc["towers"]), c["merged"], key)
            self.assertEqual(sum(t["closed"] for t in sc["towers"]), c["closed"], key)
            self.assertEqual(sc["rl"], rl)
        one = ps.effort_scene(ps.range_view(an, 3600, NOW), rl="1 h")
        self.assertEqual(sorted(t["name"] for t in one["towers"]), ["Core runtime"])  # only the area with activity in the hour
        self.assertEqual(one["towers"][0]["layers"][0]["label"], "Merged in 1 h")
        self.assertIn("merged in the last 1 h", one["caption"])
        self.assertIn("1 merged \u00b7 0 closed (1 h)", ps.scene_svg(one))

    def test_empty_range_has_no_towers(self):
        an = ps.build_analysis([self.opn(1, ago(days=60), ago(days=60))], NOW, [])
        view = ps.range_view(an, 3600, NOW)
        self.assertEqual(view["prs"], [])
        self.assertEqual(ps.effort_scene(view, rl="1 h")["towers"], [])
        self.assertEqual(ps.standings_html(view, "1 h", []), '<li class="empty-block">No PRs in the last 1 h.</li>')
        self.assertIsNone(ps.range_view(ps.build_analysis([], NOW, None), 3600, NOW)["closed"])

    def test_visible_clusters_and_hot_follow_the_range(self):
        a = self.opn(1, ago(days=20), ago(days=20))
        b = self.opn(2, ago(days=20), ago(minutes=5))
        m = dict(self.cl(3, ago(days=2)), files=[{"path": f, "additions": 1, "deletions": 0} for f in self.VFS])
        for p in (a, b):
            p["files"] = [{"path": f, "additions": 1, "deletions": 0} for f in self.VFS]
        an = ps.build_analysis([a, b], NOW, [m])
        self.assertEqual(len(an["clusters"]), 1)
        self.assertEqual(len(ps.visible_clusters(an, 30 * 86400, NOW)), 1)
        v7 = ps.visible_clusters(an, 7 * 86400, NOW)[0]
        self.assertEqual((v7["members"], v7["merged"], v7["superseded"]), ([2, 3], [3], True))  # PR 1 is out
        self.assertEqual(ps.visible_clusters(an, 3600, NOW), [])  # one member left
        an["hotFiles"] = [{"file": "x.rs", "count": 2, "prs": [1, 2]}, {"file": "y.rs", "count": 1, "prs": [1]}]
        self.assertEqual([(h["file"], ns) for h, ns in ps.hot_in_range(an, 3600, NOW)], [("x.rs", [2])])
        self.assertEqual(ps.range_counts(an, 3600, NOW)["hot"], 1)
        self.assertEqual(ps.range_counts(an, 30 * 86400, NOW)["hot"], 2)  # PR 1 (created 20 d ago) is inside 30 d

    def test_top_files_and_merged_per_day(self):
        prs = [pr(1, files=[("a.rs", 5), ("b.rs", 1)]), pr(2, files=[("a.rs", 1), ("c.rs", 9)]), pr(3, files=[("b.rs", 1), ("d.rs", 1)])]
        self.assertEqual(ps.top_files(prs), [("a.rs", 2, 6), ("b.rs", 2, 2), ("c.rs", 1, 9)])  # count, then lines, then path
        self.assertEqual(ps.top_files([]), [])
        closed = [self.cl(1, ago(hours=1)), self.cl(2, ago(hours=2)), self.cl(3, ago(days=1), merged=False), self.cl(4, ago(days=3))]
        days = ps.merged_per_day(closed, NOW, 5)
        self.assertEqual([x["day"] for x in days], ["2026-11-19", "2026-11-20", "2026-11-21", "2026-11-22", "2026-11-23"])
        self.assertEqual([x["merged"] for x in days], [0, 1, 0, 0, 2])  # closed-unmerged does not count
        self.assertEqual(ps.merged_per_day(None, NOW), [])
        self.assertEqual(len(ps.merged_per_day([], NOW)), ps.TREND_DAYS)

    def test_single_series_bars(self):
        svg = ps.svg_bars([{"day": "2026-11-22", "merged": 3}, {"day": "2026-11-23", "merged": 0}], "m", 320, 240,
                          series=(("merged", "b-merged", "merged"),))
        self.assertEqual(svg.count('<rect class="b-merged"'), 2)
        self.assertIn("2026-11-22: 3 merged", svg)


class PageRangeTests(unittest.TestCase):
    def an(self):
        return RangeTests().an()

    def page(self, an=None, hist=None):
        return ps.render_html(an or self.an(), "o/r", NOW, hist)

    def test_range_control_markup(self):
        out = self.page()
        seg = out.split('id="range"')[1].split("</div>")[0]
        self.assertEqual(re.findall(r">([^<]+)</button>", seg), ["1 h", "12 h", "24 h", "7 d", "30 d"])
        self.assertEqual(re.findall(r'data-range="([^"]+)"', seg), ["1h", "12h", "24h", "7d", "30d"])
        self.assertEqual(seg.count('aria-pressed="true"'), 1)
        self.assertIn('data-range="7d" aria-pressed="true"', seg)
        self.assertIn('aria-label="Time range"', out)
        self.assertLess(out.index('id="range"'), out.index('id="states"'))
        self.assertLess(out.index('id="range"'), out.index('class="top-right"'))  # in the toolbar, inside the top bar

    def test_page_tabs_markup_and_no_js_fallback(self):
        out = self.page()
        nav = out.split('id="ptabs"')[1].split("</nav>")[0]
        self.assertEqual(re.findall(r">([^<]+)</button>", nav), ["Effort", "Areas", "Trends", "Stats"])
        self.assertEqual(re.findall(r'data-page="([^"]+)"', nav), ["effort", "areas", "trends", "stats"])
        self.assertIn('data-page="effort" aria-pressed="true"', nav)
        self.assertLess(out.index('id="ptabs"'), out.index('id="filters"'))  # next to the title, before the toolbar
        self.assertLess(out.index("<h1>"), out.index('id="ptabs"'))
        pages = re.findall(r'<div class="page" id="page-(\w+)" data-page="(\w+)"', out)
        self.assertEqual(pages, [("effort", "effort"), ("areas", "areas"), ("trends", "trends"), ("stats", "stats")])
        self.assertNotIn('class="page" id="page-areas" data-page="areas" hidden', out)  # nothing is hidden without JS
        self.assertNotIn("<div class=\"page\" hidden", out)
        self.assertIn(".ptabs{display:none", out)
        self.assertIn(".js .ptabs{display:flex}", out)
        self.assertIn(".js .page-title{display:none}", out)  # the stacked pages keep a heading only without JS
        self.assertEqual(out.count('class="page-title"'), 4)
        for sec in ("a-city", "a-road", "a-standings", "a-trends", "a-meta"):
            self.assertEqual(out.count('class="%s"' % sec), 1, sec)
        self.assertIn("showPage", ps.JS)
        self.assertIn("p.hidden=p.getAttribute('data-page')!==n", ps.JS)

    def test_pages_hold_the_right_sections(self):
        out = self.page(hist=[snap(t(48), [1]), snap(t(24), [1, 2]), snap(t(1), [2, 3, 4])])
        parts = {k: out.split('data-page="%s"><h2' % k)[1].split('<div class="page"')[0] for k in ("effort", "areas", "trends", "stats")}
        self.assertIn("a-city", parts["effort"])
        self.assertIn("Road to merged", parts["effort"])
        self.assertIn("all open", parts["effort"])
        self.assertIn('id="stage"', parts["effort"])
        self.assertIn('id="standings"', parts["areas"])
        self.assertEqual(parts["trends"].count('class="chart-box"'), 3)
        self.assertIn("Merged per day", parts["trends"])
        for label in ("CI failing", "Stale (7d+)", "Drafts", "Possible duplicates", "Merged (7 d)", "Closed (7 d)", "Active (7 d)"):
            self.assertIn("<span>%s</span>" % label, parts["stats"])
        self.assertIn("no 24 h data yet", ps.render_html(self.an(), "o/r", NOW).split('data-page="stats"')[1])

    def test_feed_is_one_column_after_the_pages_on_every_page(self):
        out = self.page()
        self.assertEqual(out.count('class="a-feed"'), 1)
        self.assertLess(out.index("</main>"), out.index('class="a-feed"'))
        self.assertIn(".a-feed{grid-area:feed", ps.CSS)
        self.assertIn('grid-template-columns:minmax(0,1fr) 32%', ps.CSS)
        self.assertIn('grid-template-areas:"top top" "pages feed" "foot foot"', ps.CSS)
        self.assertIn('"top" "pages" "feed" "foot"', ps.CSS)  # below the page content when narrow

    def test_list_rows_carry_timestamps(self):
        an = self.an()
        out = ps.render_html(an, "o/r", NOW)
        ep = lambda x: int(dt.datetime.strptime(x, ps.TS_FMT).replace(tzinfo=dt.timezone.utc).timestamp())
        row = re.search(r'<div class="row pr" data-area="[^"]+" data-cl="0" data-created="(\d+)" data-updated="(\d+)" data-draft', out)
        self.assertTrue(row)
        self.assertEqual(sum(1 for _ in re.finditer(r'class="row pr" data-area="[^"]+" data-cl="0" data-created="\d+" data-updated="\d+"', out)), 5)
        self.assertIn('data-created="%d" data-updated="%d"' % (ep(ago(days=20)), ep(ago(minutes=30))), out)
        closed_rows = re.findall(r'class="row pr" hidden data-area="[^"]+" data-cl="1" data-closed="(\d+)"', out)
        self.assertEqual(sorted(int(x) for x in closed_rows), sorted(ep(c["mergedAt"] or c["closedAt"]) for c in an["closed"]))
        panel = out.split('data-panel="closed"')[1]
        self.assertEqual(len(re.findall(r'<div class="row cl rg" data-closed="\d+">', panel)), 5)
        self.assertEqual(ps.ts_attrs(pr(1, createdAt="2026-11-20T00:00:00Z", updatedAt="2026-11-22T00:00:00Z")),
                         'data-created="%d" data-updated="%d"' % (ep("2026-11-20T00:00:00Z"), ep("2026-11-22T00:00:00Z")))

    def test_hot_ready_and_duplicate_rows_are_filterable(self):
        prs = [pr(i, "t%d" % i, ["hot.rs", "f%d.rs" % i]) for i in range(6)]
        an = ps.build_analysis(prs, NOW)
        out = ps.render_html(an, "o/r", NOW)
        self.assertEqual(len(re.findall(r'<a class="rg" data-created="\d+" data-updated="\d+" href=', out)), 6)
        self.assertEqual(len(re.findall(r'<div class="row rd rg" data-created="\d+" data-updated="\d+">', out)), 6)
        dup = ps.build_analysis([pr(1, "VFS: a", self.vfs()), pr(2, "VFS: a", self.vfs())], NOW)
        self.assertEqual(len(re.findall(r'<div class="row rg" data-created="\d+" data-updated="\d+">', ps.render_html(dup, "o/r", NOW))), 2)

    @staticmethod
    def vfs():
        return ["crates/xcl/src/vfs/a.rs"]

    def test_range_data_block(self):
        an = self.an()
        out = ps.render_html(an, "o/<r>", NOW)
        data = json.loads(out.split('id="range-data">')[1].split("</script>")[0])
        self.assertEqual((data["default"], data["now"]), ("7d", int(NOW.timestamp())))
        self.assertEqual(list(data["ranges"]), ["1h", "12h", "24h", "7d", "30d"])
        r = data["ranges"]["1h"]
        self.assertEqual((r["s"], r["rl"]), (3600, "1 h"))
        self.assertEqual((r["counts"]["active"], r["counts"]["merged"], r["counts"]["closed"]), (1, 1, 0))
        self.assertEqual(r["metas"]["closed"], "1 merged, 0 closed, last 1 h")
        self.assertEqual(r["line"], "Last 1 h: 1 active \u00b7 1 merged \u00b7 0 closed")
        self.assertIn("<ol", r["standings"] + "<ol")
        sc = json.loads(out.split('id="scene-data">')[1].split("</script>")[0])
        self.assertEqual(list(sc["ranges"]), ["1h", "12h", "24h", "7d", "30d"])
        self.assertEqual(sc["towers"], sc["ranges"]["7d"]["towers"])  # the top level is the default range
        self.assertEqual(sc["ranges"]["1h"]["rl"], "1 h")
        self.assertEqual(sum(t["open"] for t in sc["ranges"]["30d"]["towers"]), 4)

    def test_range_data_escapes_untrusted_text(self):
        evil = "</script><b>x</b>"
        an = ps.build_analysis([pr(1, "VFS: " + evil, ["</script>.rs"], author=evil)], NOW)
        out = ps.render_html(an, "o/r", NOW)
        for sid in ("range-data", "scene-data"):
            block = out.split('id="%s">' % sid)[1].split("</script>")[0]
            self.assertNotIn("<", block)
            json.loads(block)
        self.assertNotIn("</script><b>", out)

    def test_default_range_is_what_the_static_page_shows(self):
        out = self.page()
        self.assertIn("Last 7 d: 3 active", out)
        self.assertIn('id="range-line"', out)
        self.assertIn("Merged in 7 d", out)
        sd = json.loads(out.split('id="scene-data">')[1].split("</script>")[0])
        self.assertEqual(sum(t["open"] for t in sd["towers"]), 3)
        self.assertIn("(7 d)", out.split('id="stage"')[1].split("</div>")[0])  # the SVG fallback is the 7 d scene

    def test_empty_range_messages(self):
        an = ps.build_analysis([RangeTests().opn(1, ago(days=60), ago(days=60))], NOW, [])
        out = ps.render_html(an, "o/r", NOW)
        data = json.loads(out.split('id="range-data">')[1].split("</script>")[0])
        self.assertEqual(data["ranges"]["1h"]["standings"], '<li class="empty-block">No PRs in the last 1 h.</li>')
        self.assertEqual(data["ranges"]["1h"]["counts"]["active"], 0)
        self.assertEqual(out.count('class="empty-block range-empty" hidden'), 5)  # one per list, shown by the JS when its rows are all out
        self.assertIn("'No PRs in the last '", ps.JS)
        self.assertIn("'No PRs in the last '", ps.SCENE_JS)

    def test_empty_default_range_with_data_elsewhere_shows_the_message(self):
        an = ps.build_analysis([RangeTests().opn(1, ago(days=20), ago(days=20))], NOW, [])
        out = ps.render_html(an, "o/r", NOW)
        self.assertIn('<p class="empty-block" id="scene-empty">No PRs in the last 7 d.</p>', out)
        self.assertIn('id="stage" hidden', out)
        self.assertIn("No PRs in the last 7 d.", json.loads(out.split('id="range-data">')[1].split("</script>")[0])["ranges"]["7d"]["standings"])

    def test_hash_state_keys_include_page_and_range(self):
        self.assertIn("k==='range'", ps.JS)
        self.assertIn("k==='page'", ps.JS)
        self.assertIn("p.push('range='+range)", ps.JS)
        self.assertIn("p.push('page='+page)", ps.JS)
        self.assertIn("range!==DEF", ps.JS)
        self.assertIn("page!=='effort'", ps.JS)
        self.assertIn("prscan-range", ps.JS)
        self.assertIn("prscan-range", ps.SCENE_JS)

    def test_areas_rows_have_merged_closed_and_top_files(self):
        an = RangeTests().an()
        data = json.loads(ps.render_html(an, "o/r", NOW).split('id="range-data">')[1].split("</script>")[0])
        row = data["ranges"]["30d"]["standings"]
        self.assertIn("3 merged \u00b7 2 closed", row)
        self.assertIn('<div class="stf"><span class="fchip"', row)
        self.assertEqual(ps.standings_html(ps.range_view(an, 3600, NOW), "1 h", []).count('class="st"'), 1)
        self.assertIn("title=", row)


if __name__ == "__main__":
    unittest.main()
