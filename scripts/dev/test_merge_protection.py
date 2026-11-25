#!/usr/bin/env python3
"""Tests for the merge protection: pr-gate.py, merge-pr.py, merge-guard.py and
.githooks/pre-merge-commit.

Run (Python 3.12+, standard library only, no network, nothing touches the real repository):

    python -I scripts/dev/test_merge_protection.py
    python -I -m unittest discover -s scripts/dev -p test_merge_protection.py

Every merge runs in a throwaway repository under the system temp directory, with a local path
standing in for the GitHub remote (`url.<path>.insteadOf`), and that directory is deleted
afterwards. The scripts under test are copied into the throwaway repository, because they find
their repository from their own location.

Set MERGE_GUARD_PATH to test another copy of merge-guard.py (default: the one next to this file).
"""
from __future__ import annotations

import base64
import contextlib
import importlib.util
import io
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
GUARD = os.environ.get("MERGE_GUARD_PATH") or os.path.join(HERE, "merge-guard.py")
HOOK = os.path.join(REPO, ".githooks", "pre-merge-commit")
UPSTREAM_URL = "https://github.com/ivall/xcl.git"
NO_ID = "a" * 40

_saved_env: dict = {}


def setUpModule() -> None:
    """A clean, offline git: no user or system config, a fixed identity, no prompts."""
    _saved_env.update(os.environ)
    for key in [k for k in os.environ if k.startswith("GIT_")]:
        del os.environ[key]
    os.environ.update({
        "GIT_CONFIG_GLOBAL": os.devnull, "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@example.invalid",
        "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@example.invalid",
        "GIT_TERMINAL_PROMPT": "0"})


def tearDownModule() -> None:
    os.environ.clear()
    os.environ.update(_saved_env)


def load(path: str, name: str):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _force_remove(func, path, _exc) -> None:
    os.chmod(path, stat.S_IWRITE)
    func(path)


def fwd(path: str) -> str:
    return path.replace("\\", "/")


class Sandbox:
    """remote/ (stands in for ivall/xcl) and work/ (a clone with the scripts committed)."""

    def __init__(self) -> None:
        self.tmp = tempfile.mkdtemp(prefix="xcl-mergeprot-")
        self.remote = os.path.join(self.tmp, "remote")
        self.work = os.path.join(self.tmp, "work")
        os.mkdir(self.remote)
        self.git(self.remote, "init", "-q", "-b", "main")
        self.configure(self.remote)
        self.write(self.remote, "README.md", "line one\nline two\nline three\n")
        self.git(self.remote, "add", "-A")
        self.git(self.remote, "commit", "-q", "-m", "c0")
        self.git(self.tmp, "clone", "-q", self.remote, self.work)
        self.configure(self.work)
        dev = os.path.join(self.work, "scripts", "dev")
        os.makedirs(dev)
        for name in ("pr-gate.py", "merge-pr.py"):
            shutil.copy(os.path.join(HERE, name), dev)
        os.makedirs(os.path.join(self.work, ".githooks"))
        shutil.copy(HOOK, os.path.join(self.work, ".githooks", "pre-merge-commit"))
        os.chmod(os.path.join(self.work, ".githooks", "pre-merge-commit"), 0o755)
        self.write(self.work, ".gitignore", "__pycache__/\n")
        self.git(self.work, "add", "-A")
        self.git(self.work, "commit", "-q", "-m", "tooling")
        self.git(self.work, "config", f"url.{fwd(self.remote)}.insteadOf", UPSTREAM_URL)
        self.git(self.work, "config", "core.hooksPath", ".githooks")

    def configure(self, cwd: str) -> None:
        for k, v in (("user.name", "t"), ("user.email", "t@example.invalid"),
                     ("commit.gpgsign", "false"), ("core.autocrlf", "false")):
            self.git(cwd, "config", k, v)

    def cleanup(self) -> None:
        shutil.rmtree(self.tmp, onerror=_force_remove)

    @staticmethod
    def write(cwd: str, rel: str, text: str) -> None:
        path = os.path.join(cwd, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)

    @staticmethod
    def git(cwd: str, *args: str, check: bool = True) -> subprocess.CompletedProcess:
        r = subprocess.run(["git", "-C", cwd, *args], capture_output=True, text=True,
                           errors="replace")
        if check and r.returncode != 0:
            raise AssertionError(f"git {' '.join(args)} failed: {r.stdout}{r.stderr}")
        return r

    def head(self, cwd: "str | None" = None) -> str:
        return self.git(cwd or self.work, "rev-parse", "HEAD").stdout.strip()

    def remote_commit(self, rel: str, text: str = "new\nfile\n") -> str:
        """A new commit on the remote's main (what an upstream push or a PR head looks like)."""
        self.write(self.remote, rel, text)
        self.git(self.remote, "add", "-A")
        self.git(self.remote, "commit", "-q", "-m", f"add {rel}")
        return self.head(self.remote)

    def state(self, cid: str, kind: str) -> str:
        return os.path.join(self.work, ".git", "pr-gate", f"{cid}.{kind}")

    def fetch_remote(self) -> str:
        """Fetch the remote's main into work; returns its id. (FETCH_HEAD is left behind.)"""
        self.git(self.work, "fetch", "-q", self.remote, "main")
        return self.git(self.work, "rev-parse", "FETCH_HEAD").stdout.strip()


class SandboxCase(unittest.TestCase):
    def setUp(self) -> None:
        self.sb = Sandbox()
        self.addCleanup(self.sb.cleanup)

    def py(self, script: str, *args: str) -> subprocess.CompletedProcess:
        return subprocess.run(
            [sys.executable, "-I", os.path.join(self.sb.work, "scripts", "dev", script), *args],
            cwd=self.sb.work, capture_output=True, text=True, errors="replace")

    def scan(self) -> subprocess.CompletedProcess:
        return self.py("pr-gate.py", "scan", "--sync")

    def scan_json(self, cid: str) -> dict:
        with open(self.sb.state(cid, "scan.json"), encoding="utf-8") as f:
            return json.load(f)

    def report(self, cid: str, **override) -> str:
        scan = self.scan_json(cid)
        doc = {"id": cid, "base": scan["base"], "diff_sha256": scan["diff_sha256"],
               "reviewer": "test-reviewer", "verdict": "clean",
               "ranges": [[1, scan["lines"]]], "findings": []}
        doc.update(override)
        path = os.path.join(self.sb.tmp, "report.json")
        with open(path, "w", encoding="utf-8") as f:
            json.dump(doc, f)
        return path

    def record(self, cid: str, **override) -> subprocess.CompletedProcess:
        return self.py("pr-gate.py", "record-review", cid, "--report", self.report(cid, **override))

    def scanned_and_reviewed(self, rel: str = "one.txt") -> str:
        cid = self.sb.remote_commit(rel)
        self.assertEqual(self.scan().returncode, 2)
        self.assertEqual(self.record(cid).returncode, 0)
        return cid

    def merge(self, cid: str, *extra: str) -> subprocess.CompletedProcess:
        return self.py("merge-pr.py", "--sync", "--approve", cid, *extra)

    def assertNotMerged(self, before: str) -> None:
        self.assertEqual(self.sb.head(), before)
        self.assertFalse(os.path.exists(os.path.join(self.sb.work, ".git", "MERGE_HEAD")))


# ------------------------------------------------------------------------------ the gate
class GateTests(SandboxCase):
    def test_scan_saves_the_full_diff_and_always_ends_in_review_needed(self):
        cid = self.sb.remote_commit("one.txt", "".join(f"row {i}\n" for i in range(300)))
        r = self.scan()
        self.assertEqual(r.returncode, 2, r.stdout + r.stderr)
        scan = self.scan_json(cid)
        with open(self.sb.state(cid, "diff"), "rb") as f:
            diff = f.read()
        self.assertEqual(scan["id"], cid)
        self.assertEqual(scan["base"], self.sb.git(self.sb.work, "merge-base", "HEAD", cid)
                         .stdout.strip())
        self.assertEqual(scan["lines"], diff.count(b"\n"))
        self.assertGreater(scan["lines"], 300)             # the whole 300-row file, not a cut
        self.assertEqual(len(scan["diff_sha256"]), 64)
        self.assertIn(b"+row 299", diff)
        self.assertEqual(scan["number"], None)
        self.assertTrue(scan["sync"])
        # fetched into refs/pr-gate/<id>, never checked out
        self.sb.git(self.sb.work, "rev-parse", "--verify", f"refs/pr-gate/{cid}^{{commit}}")
        self.assertFalse(os.path.exists(os.path.join(self.sb.work, "one.txt")))

    def test_scan_hints_point_at_build_scripts(self):
        cid = self.sb.remote_commit("crates/xcl/build.rs", "fn main() { std::process::Command::new(\"x\"); }\n")
        self.assertEqual(self.scan().returncode, 2)
        hints = self.scan_json(cid)["hints"]
        self.assertIn("compile-time", hints["flagged"]["crates/xcl/build.rs"]["buckets"])
        self.assertIn("suspicious", hints["flagged"]["crates/xcl/build.rs"]["buckets"])

    def test_scan_marker_alone_marks_a_generated_file(self):
        cid = self.sb.remote_commit("crates/x/table.rs", "// generated by scripts/gen_x.py\nconst A: u8 = 1;\n")
        self.assertEqual(self.scan().returncode, 2)
        self.assertIn("generated", self.scan_json(cid)["hints"]["flagged"]["crates/x/table.rs"]["buckets"])

    def test_scan_exits_3_when_the_head_moves_during_the_scan(self):
        old = self.sb.remote_commit("one.txt")
        self.sb.remote_commit("two.txt")                  # main is now one commit past `old`
        gate = load(os.path.join(self.sb.work, "scripts", "dev", "pr-gate.py"), "gate_moved")
        gate.sync_head = lambda repo: old                 # the head that was read first
        quiet = contextlib.redirect_stderr(io.StringIO())
        with quiet, contextlib.redirect_stdout(io.StringIO()), self.assertRaises(SystemExit) as ctx:
            gate.main(["scan", "--sync"])
        self.assertEqual(ctx.exception.code, 3)
        self.assertNotEqual(self.sb.git(self.sb.work, "show-ref", "--verify", "--quiet",
                                        f"refs/pr-gate/{old}", check=False).returncode, 0)

    def test_scan_exits_3_on_an_empty_diff(self):
        self.sb.remote_commit("one.txt")
        cid = self.sb.fetch_remote()
        off = os.path.join(self.sb.tmp, "nohooks")
        self.sb.git(self.sb.work, "-c", f"core.hooksPath={fwd(off)}", "merge", "-q", "--no-ff",
                    "-m", "x", cid)
        r = self.scan()
        self.assertEqual(r.returncode, 3, r.stdout + r.stderr)

    def test_scan_exits_3_on_a_git_error(self):
        self.sb.remote_commit("one.txt")
        self.sb.git(self.sb.work, "config", f"url.{fwd(self.sb.tmp)}/does-not-exist.insteadOf",
                    UPSTREAM_URL)
        self.sb.git(self.sb.work, "config", "--unset", f"url.{fwd(self.sb.remote)}.insteadOf")
        self.assertEqual(self.scan().returncode, 3)

    def test_pull_request_scan_uses_the_head_from_the_api_and_pull_ref(self):
        cid = self.sb.remote_commit("one.txt")
        self.sb.git(self.sb.remote, "update-ref", "refs/pull/7/head", cid)
        gate = load(os.path.join(self.sb.work, "scripts", "dev", "pr-gate.py"), "gate_pr")
        gate.pr_head = lambda repo, number: cid
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(gate.main(["scan", "7"]), 2)
        scan = self.scan_json(cid)
        self.assertEqual((scan["number"], scan["repo"], scan["sync"]), (7, "ivall/xcl", False))

    def test_scan_needs_exactly_one_target(self):
        for args in ([], ["--sync", "5"]):
            with self.subTest(args=args), contextlib.redirect_stderr(io.StringIO()):
                gate = load(os.path.join(self.sb.work, "scripts", "dev", "pr-gate.py"), "gate_args")
                with self.assertRaises(SystemExit):
                    gate.main(["scan", *args])

    def test_record_review_accepts_a_matching_report_and_status_passes(self):
        cid = self.sb.remote_commit("one.txt")
        self.scan()
        r = self.record(cid, verdict="findings", findings=["a note"])
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        with open(self.sb.state(cid, "review.json"), encoding="utf-8") as f:
            self.assertEqual(json.load(f)["verdict"], "findings")
        self.assertEqual(self.py("pr-gate.py", "status", cid).returncode, 0)

    def test_record_review_accepts_overlapping_ranges(self):
        cid = self.sb.remote_commit("one.txt", "".join(f"r{i}\n" for i in range(20)))
        self.scan()
        n = self.scan_json(cid)["lines"]
        self.assertEqual(self.record(cid, ranges=[[1, n // 2 + 3], [n // 2, n]]).returncode, 0)

    def test_record_review_refuses_a_gap_in_the_ranges(self):
        cid = self.sb.remote_commit("one.txt", "".join(f"r{i}\n" for i in range(20)))
        self.scan()
        n = self.scan_json(cid)["lines"]
        for ranges in ([[1, 5], [7, n]], [[2, n]], [[1, n - 1]], [], [[1, n + 1]], [[0, n]],
                       [["1", n]], [[1, n, 3]], [[True, n]]):
            with self.subTest(ranges=ranges):
                r = self.record(cid, ranges=ranges)
                self.assertEqual(r.returncode, 2, r.stdout + r.stderr)
        self.assertFalse(os.path.exists(self.sb.state(cid, "review.json")))

    def test_record_review_refuses_a_malicious_verdict(self):
        cid = self.sb.remote_commit("one.txt")
        self.scan()
        r = self.record(cid, verdict="malicious", findings=["drops a payload"])
        self.assertEqual(r.returncode, 2)
        self.assertFalse(os.path.exists(self.sb.state(cid, "review.json")))

    def test_record_review_refuses_a_mismatched_report(self):
        cid = self.sb.remote_commit("one.txt")
        self.scan()
        cases = {"id": NO_ID, "base": NO_ID, "diff_sha256": "0" * 64, "verdict": "approved",
                 "reviewer": "", "findings": "none"}
        for key, value in cases.items():
            with self.subTest(key=key):
                r = self.record(cid, **{key: value})
                self.assertEqual(r.returncode, 2, r.stdout + r.stderr)
        r = self.py("pr-gate.py", "record-review", cid, "--report", os.path.join(self.sb.tmp, "absent"))
        self.assertEqual(r.returncode, 2)

    def test_record_review_refuses_when_the_diff_on_disk_changed(self):
        cid = self.sb.remote_commit("one.txt")
        self.scan()
        report = self.report(cid)
        with open(self.sb.state(cid, "diff"), "ab") as f:
            f.write(b"+tampered\n")
        r = self.py("pr-gate.py", "record-review", cid, "--report", report)
        self.assertEqual(r.returncode, 2)

    def test_record_review_refuses_without_a_scan(self):
        r = self.py("pr-gate.py", "record-review", NO_ID, "--report", self.report_for_nothing())
        self.assertEqual(r.returncode, 2)

    def report_for_nothing(self) -> str:
        path = os.path.join(self.sb.tmp, "none.json")
        with open(path, "w", encoding="utf-8") as f:
            json.dump({"id": NO_ID}, f)
        return path

    def test_status_fails_without_review_and_after_the_diff_changes(self):
        cid = self.sb.remote_commit("one.txt")
        self.assertEqual(self.py("pr-gate.py", "status", cid).returncode, 2)   # nothing yet
        self.scan()
        self.assertEqual(self.py("pr-gate.py", "status", cid).returncode, 2)   # no review
        self.assertEqual(self.record(cid).returncode, 0)
        self.assertEqual(self.py("pr-gate.py", "status", cid).returncode, 0)
        with open(self.sb.state(cid, "diff"), "ab") as f:
            f.write(b"+tampered\n")
        self.assertEqual(self.py("pr-gate.py", "status", cid).returncode, 2)

    def test_status_fails_when_the_review_no_longer_matches_the_scan(self):
        cid = self.scanned_and_reviewed()
        path = self.sb.state(cid, "review.json")
        with open(path, encoding="utf-8") as f:
            review = json.load(f)
        review["ranges"] = [[1, 1]]                       # an edited review with a gap
        with open(path, "w", encoding="utf-8") as f:
            json.dump(review, f)
        self.assertEqual(self.py("pr-gate.py", "status", cid).returncode, 2)

    def test_the_ack_option_is_gone(self):
        cid = self.sb.remote_commit("one.txt")
        r = self.py("pr-gate.py", "scan", "--sync", "--ack", "build.rs")
        self.assertEqual(r.returncode, 2)
        self.assertFalse(os.path.exists(self.sb.state(cid, "scan.json")))

    def test_a_non_hex_id_is_not_used_as_a_path(self):
        r = self.py("pr-gate.py", "status", "../../x")
        self.assertEqual(r.returncode, 2)


# ------------------------------------------------------------------------------ the merge
class MergeTests(SandboxCase):
    def test_happy_path_merges_with_no_ff_and_a_message_naming_the_review(self):
        cid = self.scanned_and_reviewed()
        before = self.sb.head()
        r = self.merge(cid)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        parents = self.sb.git(self.sb.work, "rev-list", "--parents", "-n1", "HEAD").stdout.split()
        self.assertEqual(parents[1:], [before, cid])
        message = self.sb.git(self.sb.work, "log", "-1", "--format=%B").stdout
        digest = self.py("pr-gate.py", "status", cid).stdout.split("sha256 ")[1].strip(" )\n")
        self.assertIn(cid, message)
        self.assertIn("test-reviewer", message)
        self.assertIn(digest, message)
        self.assertTrue(os.path.exists(self.sb.state(cid, "approved.json")))
        self.assertTrue(os.path.exists(os.path.join(self.sb.work, "one.txt")))
        self.assertEqual(self.sb.git(self.sb.work, "status", "--porcelain").stdout.strip(), "")

    def test_dry_run_checks_everything_and_merges_nothing(self):
        cid = self.scanned_and_reviewed()
        before = self.sb.head()
        r = self.merge(cid, "--dry-run")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertNotMerged(before)
        self.assertFalse(os.path.exists(self.sb.state(cid, "approved.json")))

    def test_pull_request_merge_in_process(self):
        cid = self.sb.remote_commit("one.txt")
        self.sb.git(self.sb.remote, "update-ref", "refs/pull/7/head", cid)
        mp = load(os.path.join(self.sb.work, "scripts", "dev", "merge-pr.py"), "mp_pr")
        mp.gate.pr_head = lambda repo, number: cid
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(mp.gate.main(["scan", "7"]), 2)
            report = self.report(cid)
            self.assertEqual(mp.gate.main(["record-review", cid, "--report", report]), 0)
            before = self.sb.head()
            self.assertEqual(mp.main(["7", "--approve", cid]), 0)
        self.assertNotEqual(self.sb.head(), before)
        # the scan was of PR 7: a merge for another number is refused
        other = load(os.path.join(self.sb.work, "scripts", "dev", "merge-pr.py"), "mp_pr2")
        other.gate.pr_head = lambda repo, number: cid
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit):
                other.main(["8", "--approve", cid, "--dry-run"])

    def test_refuses_without_the_approve_option(self):
        cid = self.scanned_and_reviewed()
        before = self.sb.head()
        r = self.py("merge-pr.py", "--sync")
        self.assertEqual(r.returncode, 2)
        self.assertNotMerged(before)

    def test_refuses_an_approve_that_is_not_the_head_or_not_a_full_id(self):
        cid = self.scanned_and_reviewed()
        before = self.sb.head()
        for approve in (NO_ID, cid[:12], "HEAD", cid.upper()):
            with self.subTest(approve=approve):
                self.assertNotEqual(self.merge(approve).returncode, 0)
                self.assertNotMerged(before)

    def test_refuses_without_a_review(self):
        cid = self.sb.remote_commit("one.txt")
        self.scan()
        before = self.sb.head()
        r = self.merge(cid)
        self.assertEqual(r.returncode, 2, r.stdout + r.stderr)
        self.assertNotMerged(before)

    def test_refuses_without_a_scan(self):
        cid = self.sb.remote_commit("one.txt")
        before = self.sb.head()
        self.assertEqual(self.merge(cid).returncode, 2)
        self.assertNotMerged(before)

    def test_refuses_a_malicious_review(self):
        cid = self.sb.remote_commit("one.txt")
        self.scan()
        self.assertEqual(self.record(cid, verdict="malicious").returncode, 2)
        before = self.sb.head()
        self.assertEqual(self.merge(cid).returncode, 2)
        self.assertNotMerged(before)

    def test_refuses_when_the_head_moved_after_the_review(self):
        cid = self.scanned_and_reviewed()
        self.sb.remote_commit("two.txt")                  # pushed after the review
        before = self.sb.head()
        r = self.merge(cid)
        self.assertEqual(r.returncode, 2, r.stdout + r.stderr)
        self.assertIn("moved", r.stderr)
        self.assertNotMerged(before)

    def test_refuses_when_the_diff_was_changed_after_the_review(self):
        cid = self.scanned_and_reviewed()
        with open(self.sb.state(cid, "diff"), "ab") as f:
            f.write(b"+tampered\n")
        before = self.sb.head()
        self.assertEqual(self.merge(cid).returncode, 2)
        self.assertNotMerged(before)

    def test_refuses_a_stale_base(self):
        first = self.sb.remote_commit("one.txt")
        second = self.sb.remote_commit("two.txt")
        self.assertEqual(self.scan().returncode, 2)
        self.assertEqual(self.record(second).returncode, 0)
        self.sb.fetch_remote()
        off = os.path.join(self.sb.tmp, "nohooks")        # an unreviewed merge, done by hand
        self.sb.git(self.sb.work, "-c", f"core.hooksPath={fwd(off)}", "merge", "-q", "--no-ff",
                    "-m", "x", first)
        before = self.sb.head()
        r = self.merge(second)
        self.assertEqual(r.returncode, 2, r.stdout + r.stderr)
        self.assertIn("HEAD moved", r.stderr)
        self.assertNotMerged(before)

    def test_refuses_a_dirty_tree(self):
        cid = self.scanned_and_reviewed()
        self.sb.write(self.sb.work, "scratch.txt", "uncommitted\n")
        before = self.sb.head()
        self.assertNotEqual(self.merge(cid).returncode, 0)
        self.assertNotMerged(before)

    def test_a_conflict_keeps_the_approval_so_it_can_be_committed(self):
        self.sb.write(self.sb.work, "README.md", "line one\nlocal edit\nline three\n")
        self.sb.git(self.sb.work, "commit", "-q", "-am", "local")
        cid = self.sb.remote_commit("README.md", "line one\nremote edit\nline three\n")
        self.scan()
        self.record(cid)
        r = self.merge(cid)
        self.assertNotEqual(r.returncode, 0)
        self.assertTrue(os.path.exists(os.path.join(self.sb.work, ".git", "MERGE_HEAD")))
        self.assertTrue(os.path.exists(self.sb.state(cid, "approved.json")))
        self.sb.write(self.sb.work, "README.md", "line one\nboth\nline three\n")
        self.sb.git(self.sb.work, "add", "README.md")
        self.sb.git(self.sb.work, "commit", "-q", "-m", "resolved")   # the hook lets it through

    def test_a_merge_that_fails_without_conflicts_removes_the_approval(self):
        cid = self.scanned_and_reviewed()
        mp = load(os.path.join(self.sb.work, "scripts", "dev", "merge-pr.py"), "mp_fail")
        real = mp.git

        def fake(*args):
            if args and args[0] == "merge":
                return subprocess.CompletedProcess(args, 1, "", "boom")
            return real(*args)
        mp.git = fake
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(mp.main(["--sync", "--approve", cid]), 1)
        self.assertFalse(os.path.exists(self.sb.state(cid, "approved.json")))

    def test_there_is_no_cherry_pick_or_no_commit_mode(self):
        cid = self.scanned_and_reviewed()
        for flag in ("--cherry-pick", "--no-commit", "--ack"):
            with self.subTest(flag=flag):
                self.assertEqual(self.merge(cid, flag).returncode, 2)


# ------------------------------------------------------------------------------ the hook
def find_sh() -> "str | None":
    found = shutil.which("sh")
    if found:
        return found
    exec_path = subprocess.run(["git", "--exec-path"], capture_output=True, text=True).stdout.strip()
    for rel in ("../../../usr/bin/sh.exe", "../../bin/sh.exe", "../../../bin/sh"):
        candidate = os.path.normpath(os.path.join(exec_path, rel))
        if os.path.isfile(candidate):
            return candidate
    return None


class HookTests(SandboxCase):
    """git 2.53 runs pre-merge-commit during `git merge` BEFORE it writes MERGE_HEAD, and does not
    run it for `git commit` at all. So the MERGE_HEAD rules are tested by running the hook
    directly with a MERGE_HEAD file in place; the live-git behaviour is tested separately."""

    def setUp(self) -> None:
        super().setUp()
        self.sh = find_sh()
        self.sb.remote_commit("one.txt")
        self.incoming = self.sb.fetch_remote()

    def approve(self, cid: str, head: str) -> None:
        path = self.sb.state(cid, "approved.json")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            json.dump({"id": cid, "head": head}, f, indent=1)

    def merge_head(self, cwd: str, *ids: str) -> None:
        rel = self.sb.git(cwd, "rev-parse", "--git-path", "MERGE_HEAD").stdout.strip()
        with open(os.path.join(cwd, rel), "w", encoding="ascii", newline="\n") as f:
            f.write("".join(i + "\n" for i in ids))

    def hook(self, cwd: "str | None" = None) -> subprocess.CompletedProcess:
        if self.sh is None:
            self.skipTest("no POSIX sh to run the hook directly")
        cwd = cwd or self.sb.work
        return subprocess.run([self.sh, os.path.join(cwd, ".githooks", "pre-merge-commit")],
                              cwd=cwd, capture_output=True, text=True, errors="replace")

    def test_a_plain_git_merge_is_refused_and_leaves_head_alone(self):
        before = self.sb.head()
        r = self.sb.git(self.sb.work, "merge", "--no-ff", "-m", "x", self.incoming, check=False)
        self.assertNotEqual(r.returncode, 0)
        self.assertIn("pre-merge-commit: refusing", r.stderr)
        self.assertEqual(self.sb.head(), before)
        self.sb.git(self.sb.work, "merge", "--abort", check=False)

    def test_refuses_without_an_approval(self):
        self.merge_head(self.sb.work, self.incoming)
        r = self.hook()
        self.assertEqual(r.returncode, 1)
        self.assertIn("no approval", r.stderr)

    def test_allows_an_approval_naming_this_head(self):
        self.merge_head(self.sb.work, self.incoming)
        self.approve(self.incoming, self.sb.head())
        r = self.hook()
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)

    def test_refuses_an_approval_made_for_another_head(self):
        self.merge_head(self.sb.work, self.incoming)
        self.approve(self.incoming, NO_ID)
        self.assertEqual(self.hook().returncode, 1)

    def test_refuses_an_approval_for_another_commit(self):
        self.merge_head(self.sb.work, self.incoming)
        self.approve(NO_ID, self.sb.head())
        self.assertEqual(self.hook().returncode, 1)

    def test_remote_branches_are_no_longer_exempt(self):
        for ref in ("refs/remotes/origin/dev", "refs/remotes/origin/main"):
            self.sb.git(self.sb.work, "update-ref", ref, self.incoming)
        self.merge_head(self.sb.work, self.incoming)
        self.assertEqual(self.hook().returncode, 1)

    def test_allows_a_merge_head_that_is_already_an_ancestor(self):
        self.merge_head(self.sb.work, self.sb.head())
        self.assertEqual(self.hook().returncode, 0)
        self.merge_head(self.sb.work, self.sb.git(self.sb.work, "rev-parse", "HEAD~1").stdout.strip())
        self.assertEqual(self.hook().returncode, 0)

    def test_an_octopus_needs_every_commit_approved(self):
        ancestor = self.sb.git(self.sb.work, "rev-parse", "HEAD~1").stdout.strip()
        self.merge_head(self.sb.work, ancestor, self.incoming)
        self.assertEqual(self.hook().returncode, 1)
        self.approve(self.incoming, self.sb.head())
        self.assertEqual(self.hook().returncode, 0)

    def test_refuses_when_there_is_no_merge_head(self):
        self.assertEqual(self.hook().returncode, 1)

    def test_refuses_a_merge_head_that_is_not_a_commit_id(self):
        for text in ("not-a-commit\n", "\n", "", "abc123\n", ("g" * 40) + "\n",
                     self.incoming.upper() + "\n"):
            with self.subTest(text=text):
                with open(os.path.join(self.sb.work, ".git", "MERGE_HEAD"), "w", newline="\n") as f:
                    f.write(text)
                self.assertEqual(self.hook().returncode, 1)

    def test_works_in_a_linked_worktree(self):
        wt = os.path.join(self.sb.tmp, "wt")
        self.sb.git(self.sb.work, "worktree", "add", "-q", "-b", "wtb", wt)
        self.merge_head(wt, self.incoming)                          # per-worktree file
        self.assertEqual(self.hook(wt).returncode, 1)               # no approval yet
        self.approve(self.incoming, self.sb.head(wt))               # state is in the common dir
        r = self.hook(wt)
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.sb.write(wt, "w.txt", "w\n")                           # HEAD moves: approval is stale
        self.sb.git(wt, "add", "-A")
        self.sb.git(wt, "commit", "-q", "-m", "w")
        self.assertEqual(self.hook(wt).returncode, 1)


# ------------------------------------------------------------------------------ the guard
class GuardTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.sb = Sandbox()
        sb = cls.sb
        sb.git(sb.work, "branch", "dev")                           # a local branch
        sb.remote_commit("extra.txt")
        sb.git(sb.work, "fetch", "-q", sb.remote, "main:refs/remotes/origin/feat")   # not an ancestor
        sb.write(sb.work, "tracked.txt", "x\n")
        sb.git(sb.work, "add", "-A")
        sb.git(sb.work, "commit", "-q", "-m", "more")
        cls._old = os.environ.get("CLAUDE_PROJECT_DIR")
        os.environ["CLAUDE_PROJECT_DIR"] = sb.work
        cls.g = load(GUARD, "guard_under_test")

    @classmethod
    def tearDownClass(cls) -> None:
        if cls._old is None:
            os.environ.pop("CLAUDE_PROJECT_DIR", None)
        else:
            os.environ["CLAUDE_PROJECT_DIR"] = cls._old
        cls.sb.cleanup()

    def level(self, command: str) -> str:
        return self.g.decide(command)[0]

    def check(self, expected, commands):
        for command in commands:
            with self.subTest(command=command):
                got = self.level(command)
                if expected == "blocked":                      # the old bypasses: ask or deny
                    self.assertIn(got, ("ask", "deny"), command)
                else:
                    self.assertEqual(got, expected, command)

    def test_every_previous_bypass_is_ask_or_deny(self):
        self.check("blocked", [
            "git merge FETCH_HEAD; echo merge-pr.py",
            "git merge FETCH_HEAD # merge-pr.py",
            "(git merge FETCH_HEAD)",
            "git rebase FETCH_HEAD",
            "git rebase dev",
            "git reset --hard FETCH_HEAD",
            "git reset --hard origin/feat",
            "git checkout FETCH_HEAD && cargo build",
            "git checkout origin/feat && cargo build --workspace",
            "gh pr checkout 5",
            "bash -c 'git merge FETCH_HEAD'",
            "sh -c \"git merge FETCH_HEAD\"",
            "git merge FETCH_HEAD;ls",
            "ls;git merge FETCH_HEAD",
            "git merge FETCH_HEAD&&ls",
            "true||git merge FETCH_HEAD",
            "true|git merge FETCH_HEAD",
            "git merge FETCH_HEAD&",
            "git pull --ff-only",
            "git pull origin dev",
        ])

    def test_these_are_denied_outright(self):
        self.check("deny", [
            "git merge FETCH_HEAD", "git merge MERGE_HEAD", "git merge fetch_head", "git merge FETCH_HEAD~1",
            "git cherry-pick FETCH_HEAD", "git rebase FETCH_HEAD", "git reset --hard FETCH_HEAD",
            "git checkout FETCH_HEAD", "git checkout -b x FETCH_HEAD", "git switch FETCH_HEAD",
            "git switch --detach FETCH_HEAD", "git restore --source=FETCH_HEAD f", "git restore -s FETCH_HEAD f",
            "git worktree add ../w FETCH_HEAD", "git am FETCH_HEAD", "git apply FETCH_HEAD",
            "git merge refs/pr-gate/0123", "git checkout refs/pr-gate/0123",
            "git pull https://github.com/x/y main", "git merge git@github.com:x/y.git", "git pull ssh://x/y",
            "git pull origin pull/5/head", "git merge origin/pull/5/merge",
            "gh pr merge 3", "gh pr checkout 3", "gh pr co 3", "gh -R x/y pr merge 3", "gh repo sync",
            "bash scripts/dev/sync-upstream.sh", "./scripts/dev/sync-upstream.sh --push-work",
            "sh scripts/dev/sync-upstream.sh --branch dev",
            "git -c core.hooksPath=x commit", "git -ccore.hooksPath=x commit", "git -c core.hookspath=/dev/null merge x",
            "git config core.hooksPath foo", "git config --global core.hooksPath foo", "git config --unset core.hooksPath",
            "GIT_CONFIG_KEY_0=core.hooksPath git status",
            "git merge --no-verify x", "git merge x --no-verify", "git pull --no-verify",
            "gh workflow enable fork-sync.yml", "gh workflow enable fork-sync", "gh api -X PUT repos/o/r/actions/workflows/1/enable",
            "git branch -f x FETCH_HEAD", "git update-ref refs/heads/x FETCH_HEAD", "git fetch . FETCH_HEAD:refs/heads/x",
            "git read-tree -u FETCH_HEAD",
        ])

    def test_other_ways_to_run_the_same_thing_are_seen(self):
        self.check("deny", [
            "echo $(git merge FETCH_HEAD)", "echo `git merge FETCH_HEAD`", 'echo "$(git merge FETCH_HEAD)"',
            'echo "`git merge FETCH_HEAD`"', "(git merge FETCH_HEAD)", "{ git merge FETCH_HEAD; }",
            "ls\ngit merge FETCH_HEAD", "ls\r\ngit merge FETCH_HEAD", "ls &&\n git merge FETCH_HEAD",
            "FOO=1 git merge FETCH_HEAD", "FOO=1 BAR=2 git merge FETCH_HEAD", "env git merge FETCH_HEAD",
            "env -i FOO=1 git merge FETCH_HEAD", "command git merge FETCH_HEAD", "sudo -u root git merge FETCH_HEAD",
            "exec git merge FETCH_HEAD", "nohup git merge FETCH_HEAD", "time git merge FETCH_HEAD",
            "git -C . merge FETCH_HEAD", "git -C sub -c x=y merge FETCH_HEAD", "git --no-pager merge FETCH_HEAD",
            "git --git-dir=.git merge FETCH_HEAD", "git --git-dir .git merge FETCH_HEAD", "git.exe merge FETCH_HEAD",
            "/usr/bin/git merge FETCH_HEAD", "C:\\Git\\cmd\\git.exe merge FETCH_HEAD",
            '"C:\\Program Files\\Git\\cmd\\git.exe" merge FETCH_HEAD', "\\git merge FETCH_HEAD",
            "g\\it mer\\ge FETCH_HEAD", "git 'merge' FETCH_HEAD", 'git "me""rge" FETCH_HEAD', "git merge FETCH_HEAD>log",
            "git merge FETCH_HEAD 2>&1", "git merge FETCH_HEAD </dev/null", "git merge $'FETCH_HEAD'",
            "if true; then git merge FETCH_HEAD; fi", "for i in 1; do git merge FETCH_HEAD; done",
            "while true; do git merge FETCH_HEAD; done", "! git merge FETCH_HEAD", "true && ! git merge FETCH_HEAD",
            "find . -exec git merge FETCH_HEAD \\;", "echo x | xargs git merge FETCH_HEAD",
            'cmd /c "git merge FETCH_HEAD"', "cmd.exe /c git merge FETCH_HEAD && dir",
            'powershell -Command "git merge FETCH_HEAD"', 'pwsh -NoProfile -c "git merge FETCH_HEAD"',
            "powershell -NoProfile -EncodedCommand " + base64.b64encode(
                "git merge FETCH_HEAD".encode("utf-16-le")).decode(),
            'eval "git merge FETCH_HEAD"', "eval git merge FETCH_HEAD", "bash -lc 'git merge FETCH_HEAD'",
            "bash -c \"echo hi; git merge FETCH_HEAD\"", "bash -c 'bash -c \"git merge FETCH_HEAD\"'",
            "bash <<EOF\ngit merge FETCH_HEAD\nEOF", "bash <<'EOF'\ngit merge FETCH_HEAD\nEOF",
            "sh <<-EOF\n\tgit merge FETCH_HEAD\n\tEOF", "bash <<< 'git merge FETCH_HEAD'",
            "cat <<EOF\n$(git merge FETCH_HEAD)\nEOF", "git commit -m \"$(git merge FETCH_HEAD)\"",
            "ls <(git merge FETCH_HEAD)", "git merge FETCH_HEAD # harmless", "git merge FETCH_HEAD; echo 'merge-pr.py'",
        ])

    def test_these_need_the_users_approval(self):
        self.check("ask", [
            "python scripts/dev/merge-pr.py 5 --approve " + NO_ID,
            "python -I scripts/dev/merge-pr.py --sync --approve " + NO_ID,
            'python -I "$CLAUDE_PROJECT_DIR/scripts/dev/merge-pr.py" --sync --approve ' + NO_ID,
            "py scripts\\dev\\merge-pr.py 1 --approve " + NO_ID, "python3.12 -X utf8 scripts/dev/merge-pr.py 1",
            "./scripts/dev/merge-pr.py 1", "bash -c 'python scripts/dev/merge-pr.py 1'",
            "cd x && python scripts/dev/merge-pr.py 1", "FOO=1 python scripts/dev/merge-pr.py 1",
            "git merge dev", "git merge origin/dev", "git pull", "git pull --ff-only", "git pull origin dev",
            "git rebase dev", "git cherry-pick abc123", "git am p.patch", "git am", "git apply p.patch",
            "git reset --hard origin/feat", "git reset origin/feat", "git checkout origin/feat", "git switch feat",
            "git switch -c x origin/feat", "git checkout -b x origin/feat", "git checkout nosuchref",
            "git restore --source=origin/feat f", "git worktree add ../w origin/feat", "git branch x origin/feat",
            "git commit --no-verify -m x", "git -c alias.m=merge m x", "git config alias.m merge",
            "echo 'git merge FETCH_HEAD' | bash", "printf 'git merge dev' | sh", "git checkout $WHERE", "git reset --hard $(cat ref)",
            "$GIT merge FETCH_HEAD", "git $SUB dev",
            "python -c \"import os; os.system('git merge dev')\"",
            "git merge FETCH_HEAD \"unterminated",
            "python - <<EOF\nimport os\nos.system('git merge dev')\nEOF",
            "python <<< \"import os; os.system('git pull')\"",
        ])

    def test_text_it_cannot_parse_that_names_a_blocked_form_is_not_allowed(self):
        for command in ("echo 'unterminated && git merge FETCH_HEAD", "git merge FETCH_HEAD $(", "x=`git merge dev",
                        'echo "git merge FETCH_HEAD && (', "git reset --hard FETCH_HEAD '"):
            with self.subTest(command=command):
                self.assertNotEqual(self.level(command), "allow")

    def test_data_only_text_is_allowed(self):
        self.check("allow", [
            "", "   ", "ls", "ls;ls", "ls&&ls", "cargo build --workspace && cargo test", "cd x && git status",
            "git status", "git log --oneline -5", "git diff HEAD", "git show HEAD:README.md", "git stash list",
            "git merge-base HEAD FETCH_HEAD", "git merge-tree HEAD FETCH_HEAD", "git diff HEAD...FETCH_HEAD",
            "git diff --stat FETCH_HEAD", "git log FETCH_HEAD", "git show FETCH_HEAD", "git rev-parse FETCH_HEAD",
            "git cat-file -p FETCH_HEAD", "git fetch origin pull/5/head", "git fetch https://github.com/x/y main",
            "git ls-remote https://github.com/x/y", "git log HEAD@{1}", "echo ${HOME}/x", "cat .git/FETCH_HEAD",
            "git merge --abort", "git merge --continue", "git merge --quit", "git rebase --abort", "git rebase --continue",
            "git cherry-pick --abort", "git cherry-pick --continue", "git cherry-pick --skip", "git am --abort",
            "git am --skip", "git apply --check p.patch", "git apply --stat p.patch",
            "git checkout -- tracked.txt", "git checkout -- .", "git checkout tracked.txt", "git checkout -b newbranch",
            "git switch -c newbranch", "git branch newbranch", "git branch -d newbranch", "git worktree add ../w",
            "git worktree list", "git reset", "git reset --hard", "git reset --soft HEAD~1", "git reset HEAD~1",
            "git reset HEAD -- tracked.txt", "git checkout dev", "git switch dev", "git checkout HEAD~1",
            "git restore tracked.txt", "git restore --staged tracked.txt",
            "git commit -m 'git merge FETCH_HEAD'", 'git commit -m "see gh pr checkout 5"',
            "echo merge-pr.py", "echo 'python scripts/dev/merge-pr.py 5'", "# merge-pr.py", "ls # git merge FETCH_HEAD",
            "grep -r 'git merge' docs", "echo 'gh pr checkout 5'", 'echo "a;b && git merge FETCH_HEAD"',
            "echo git merge FETCH_HEAD", "cat scripts/dev/merge-pr.py", "git add scripts/dev/merge-pr.py",
            "python scripts/dev/merge-pr.py 5 --approve " + NO_ID + " --dry-run",
            "python scripts/dev/merge-pr.py --sync --approve x --dry-run",
            "python scripts/dev/pr-gate.py scan 5", "python scripts/dev/pr-gate.py scan --sync",
            "python scripts/dev/pr-gate.py status " + NO_ID, "python -I scripts/dev/test_merge_protection.py",
            "bash scripts/dev/sync-upstream.sh --dry-run", "bash scripts/dev/sync-upstream.sh --help",
            "gh pr view 5", "gh pr diff 5", "gh pr list --search merge", "gh workflow list", "gh workflow disable fork-sync.yml",
            "git commit -m \"$(cat <<'EOF'\nfix: do not git merge FETCH_HEAD, don't\nEOF\n)\"",
            "cat <<'EOF' > notes.md\ngit merge FETCH_HEAD\ngh pr checkout 5\nEOF",
            "cat <<EOF\ngit merge FETCH_HEAD\nEOF", "cat <<'EOF' | wc -l\ngit merge FETCH_HEAD\nEOF",
            "git log --grep='git merge' --oneline", "echo hi | grep 'git merge FETCH_HEAD'",
            "python -c \"print('hello')\"", "bash -c 'echo hi'", "bash script.sh", "node -e \"console.log(1)\"",
        ])

    def test_a_computed_program_is_asked_about_only_when_it_looks_like_a_merge(self):
        self.assertEqual(self.level("$(git rev-parse --show-toplevel)/scripts/x.sh"), "allow")
        self.assertEqual(self.level("$HOME/bin/tool --flag"), "allow")

    def test_malformed_input_does_not_allow(self):
        for raw in ("", "not json", "[]", "null", "42", '"git status"', "{}", '{"tool_name": 5}',
                    '{"tool_name": "Bash"}', '{"tool_name": "Bash", "tool_input": null}',
                    '{"tool_name": "Bash", "tool_input": {}}', '{"tool_name": "Bash", "tool_input": {"command": 5}}',
                    '{"tool_name": "Bash", "tool_input": {"command": null}}', '{"tool_name": "Bash", "tool_input": []}',
                    '{"tool_name": "Bash"', '\x00\x01', "git merge FETCH_HEAD"):
            with self.subTest(raw=raw):
                self.assertEqual(self.g.decide_event(raw)[0], "deny")

    def test_other_tools_and_empty_commands_pass(self):
        for raw in ('{"tool_name": "Read", "tool_input": {"file_path": "x"}}',
                    '{"tool_name": "Bash", "tool_input": {"command": ""}}',
                    '{"tool_name": "PowerShell", "tool_input": {"command": "Get-ChildItem"}}'):
            with self.subTest(raw=raw):
                self.assertEqual(self.g.decide_event(raw)[0], "allow")

    def test_the_powershell_tool_is_checked_too(self):
        raw = json.dumps({"tool_name": "PowerShell", "tool_input": {"command": "git merge FETCH_HEAD"}})
        self.assertEqual(self.g.decide_event(raw)[0], "deny")

    def test_an_internal_error_is_a_deny(self):
        original = self.g.analyze
        self.addCleanup(setattr, self.g, "analyze", original)

        def boom(*args, **kwargs):
            raise RuntimeError("simulated")
        self.g.analyze = boom
        level, why = self.g.decide_event(json.dumps({"tool_name": "Bash", "tool_input": {"command": "ls"}}))
        self.assertEqual(level, "deny")
        self.assertIn("guard itself failed", why)

    def test_deeply_nested_commands_do_not_pass_silently(self):
        command = "git merge FETCH_HEAD"
        for _ in range(12):
            command = "bash -c '" + command.replace("'", "'\\''") + "'"
        self.assertNotEqual(self.level(command), "allow")
        deep = self.event("echo " + "$(" * 3000 + "ls")
        self.assertEqual(self.g.decide_event(deep)[0], "deny")       # recursion error: fail closed

    def run_guard(self, stdin: "str | bytes") -> subprocess.CompletedProcess:
        data = stdin.encode("utf-8") if isinstance(stdin, str) else stdin
        env = dict(os.environ, CLAUDE_PROJECT_DIR=self.sb.work)
        return subprocess.run([sys.executable, "-I", GUARD], input=data, capture_output=True, env=env)

    def event(self, command: str) -> str:
        return json.dumps({"tool_name": "Bash", "tool_input": {"command": command}})

    def test_hook_protocol_allow_ask_deny(self):
        r = self.run_guard(self.event("git status"))
        self.assertEqual((r.returncode, r.stdout, r.stderr), (0, b"", b""))
        r = self.run_guard(self.event("git merge dev"))
        self.assertEqual((r.returncode, r.stderr), (0, b""))
        out = json.loads(r.stdout)["hookSpecificOutput"]
        self.assertEqual(out["hookEventName"], "PreToolUse")
        self.assertEqual(out["permissionDecision"], "ask")
        self.assertTrue(out["permissionDecisionReason"])
        r = self.run_guard(self.event("git merge FETCH_HEAD"))
        self.assertEqual((r.returncode, r.stdout), (2, b""))
        self.assertIn(b"BLOCKED", r.stderr)

    def test_hook_protocol_malformed_stdin_is_a_deny(self):
        for data in ("", "garbage", b"\xff\xfe\xfa", "{", self.event("ls")[:-3]):
            with self.subTest(data=data):
                self.assertEqual(self.run_guard(data).returncode, 2)

    def test_the_guard_never_crashes_on_random_text(self):
        import random
        rng = random.Random(7)
        alphabet = list("git merge FETCH_HEAD bash -c $()`\"'\\;&|<>{}# \n=cmd /c eval")
        for _ in range(1500):
            text = "".join(rng.choice(alphabet) for _ in range(rng.randint(1, 40)))
            level, why = self.g.decide_event(self.event(text))
            self.assertNotIn("guard itself failed", why, text)


if __name__ == "__main__":
    unittest.main(verbosity=1)
