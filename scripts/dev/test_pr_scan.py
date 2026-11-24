#!/usr/bin/env python3
"""Tests for pr-scan.py (pure functions only; no network). Run: python3 -I scripts/dev/test_pr_scan.py -v"""
import datetime as dt
import importlib.util
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
        md = ps.render_md(an, "o/r", NOW)
        self.assertNotIn("<script>", md)
        self.assertNotIn("javascript:", md)


if __name__ == "__main__":
    unittest.main()
