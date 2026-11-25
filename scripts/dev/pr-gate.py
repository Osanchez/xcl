#!/usr/bin/env python3
"""Review gate for code that comes from outside (a pull request, an upstream sync).

    pr-gate.py scan 2343 [--repo OWNER/NAME]      fetch PR 2343's head, save the full diff
    pr-gate.py scan --sync                        the same for upstream main
    pr-gate.py record-review <id> --report FILE   record the AI review of that diff
    pr-gate.py status <id>                        exit 0 only when scan and review match

The flow, which merge-pr.py then enforces:

  1. `scan` reads the head commit id, fetches exactly that commit into refs/pr-gate/<id>
     (never checked out, built or run), and saves `git diff <merge-base> <id>` in full as
     <id>.diff. No GitHub patch text is used, so nothing is truncated. It always ends with
     exit 2: a person or an AI reviewer still has to read the diff.
  2. The reviewer reads the WHOLE saved diff and writes a report: id, base, diff_sha256,
     reviewer, verdict (clean | findings | malicious), the line ranges it read (they must
     cover 1..line count with no gap) and its findings.
  3. `record-review` refuses a report that does not match the scan and the diff on disk, that
     leaves a gap, or whose verdict is malicious.
  4. merge-pr.py merges only when `status` passes, the head is still that id and the user
     passed --approve <id>.

State lives in <git-common-dir>/pr-gate/ (local, never committed, shared by worktrees):
<id>.diff, <id>.scan.json, <id>.review.json, <id>.approved.json.

The path buckets and added-line patterns below are hints to point a reviewer at the risky
files (build.rs and procedural macros run at build time; scripts/ run as you). They are not a
verdict: the review covers every line.

Exit codes: 0 ok, 2 refused / review needed, 3 could not scan (any git or gh error, empty diff,
missing object). The gate fails closed: nothing it cannot determine counts as a pass.
"""
from __future__ import annotations

import argparse
import fnmatch
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DEFAULT_REPO = "ivall/xcl"
SYNC_REF = "refs/heads/main"
ID_RE = re.compile(r"^[0-9a-f]{40}$")
VERDICTS = ("clean", "findings", "malicious")

# ---------------------------------------------------------------- risk buckets (by path)
# Each bucket: (key, title, why it is dangerous, what to check by hand). Order is report order.
BUCKETS = [
    (
        "compile-time",
        "Runs at compile time",
        "cargo executes build.rs (and any procedural macro it pulls in) as you, before a single "
        "test runs. This is the highest-value place to hide anything.",
        "Read the whole file. It must only read files and emit cargo directives: no "
        "std::process::Command, no network, no writes outside OUT_DIR.",
    ),
    (
        "ci",
        "Continuous integration / Actions",
        "A workflow runs on GitHub with whatever secrets that repository holds. In this fork "
        "SYNC_TOKEN has contents and workflows write.",
        "Check the trigger (pull_request_target and workflow_run run with secrets), every "
        "`secrets.` reference, the permissions block, and any third-party action and its pin.",
    ),
    (
        "cargo-config",
        "Cargo / toolchain configuration",
        "`.cargo/config.toml` can set a linker, a runner or rustflags, which means an arbitrary "
        "program runs during build or test.",
        "Look for `linker`, `runner`, `rustflags`, `target.*` overrides and replaced sources.",
    ),
    (
        "dependencies",
        "Dependencies",
        "A new or bumped crate is new code running in your process; a procedural macro also runs "
        "at compile time. Typosquatted names look right at a glance.",
        "Check every added package name character by character, and that each is a known crate "
        "from crates.io. A `source = \"git+...\"` entry is never acceptable here.",
    ),
    (
        "host-scripts",
        "Scripts that run on your machine",
        "Everything under scripts/ runs as you during build, test and the smoke run.",
        "Read the diff in full. Watch for downloads, base64 or hex decoding, writes outside the "
        "repository, and anything that invokes a shell.",
    ),
    (
        "generated",
        "Generated files",
        "These are tables produced by a generator. A hand-edited 'table' is unreviewable by "
        "eye - it is the ideal place to hide a payload.",
        "Do not read these. Re-run the generator named below and diff; if the file does not "
        "reproduce byte for byte, reject the pull request.",
    ),
    (
        "import-routing",
        "Game import routing",
        "stubdb.toml decides which implementation each console import resolves to. A changed "
        "route is a redirect that reads like a one-line config tweak.",
        "Diff every changed row and confirm the target is the intended implementation.",
    ),
    (
        "binary",
        "Binary files",
        "A binary cannot be reviewed in a diff, and CONTRIBUTING.md already forbids binaries "
        "outside tests/data/ or over 1 MB.",
        "Reject unless it is test data whose provenance the description explains.",
    ),
]

BUCKET_PATTERNS = {
    "compile-time": ["build.rs", "*/build.rs"],
    "ci": [".github/*", ".github/**"],
    "cargo-config": [".cargo/*", ".cargo/**", "rust-toolchain", "rust-toolchain.toml"],
    "dependencies": ["Cargo.toml", "*/Cargo.toml", "Cargo.lock", "*/Cargo.lock"],
    "host-scripts": ["scripts/*", "scripts/**", "*.ps1", "*.py", "*.sh", "*.bat", "*.cmd"],
    "import-routing": ["crates/xcl/data/stubdb.toml"],
}

# Generated files are detected from the local checkout (see generated_files) so that a file
# upstream adds later is picked up without editing this list. Used when detection finds nothing.
GENERATED_FALLBACK = {
    "crates/xcl/src/gpu/rdna2_spec.rs": "scripts/gen_rdna2.py",
    "crates/xcl/src/gpu/tiling_tables.rs": "scripts/gen_tiling.py",
    "crates/xcl/src/hle/bcpack/tables.rs": "scripts/gen_bcpack_victory_tables.py",
    "crates/xcl/src/hle/d3d12_layout.rs": "scripts/gen_d3d12_layout.py",
    "crates/xcl/src/hle/xgameruntime/gdk_table.rs": "scripts/gen_gdk_table.py",
}

GENERATED_MARKER = re.compile(
    r"(?:@generated|[Gg]enerated)\s+by\s+(scripts/[A-Za-z0-9_./-]+\.py)")

# ---------------------------------------------------------------- content patterns
# Blocking: idioms that are genuinely rare in this codebase and are the shape of a dropper.
# Each entry: (key, compiled pattern, what it means). Kept deliberately short - a long list of
# weak patterns would block every pull request and train you to --ack without reading.
SUSPICIOUS = [
    ("download", re.compile(
        r"\b(?:Invoke-WebRequest|Invoke-RestMethod|iwr\b|curl\s|wget\s|urllib\.request|"
        r"requests\.get|reqwest::|ureq::|TcpStream::connect|socket\.connect)"),
     "fetches something from the network"),
    ("decode-exec", re.compile(
        r"\b(?:FromBase64String|b64decode|base64\.decode|base64::decode|"
        r"powershell\s+-[eE]nc|-EncodedCommand|Invoke-Expression|\biex\b|eval\(|exec\()"),
     "decodes or evaluates code at run time"),
    ("spawn", re.compile(
        r"\b(?:Command::new|process::Command|subprocess\.(?:run|Popen|call)|os\.system|"
        r"Start-Process|ShellExecute|CreateProcess)"),
     "starts another program"),
    ("link-tricks", re.compile(
        r"(?:#\[\s*link_section|#\[\s*no_mangle|#\[\s*ctor|global_asm!|\.ctors|\.init_array)"),
     "places code where it runs outside the normal call path"),
    ("secret-reach", re.compile(
        r"(?:secrets\.[A-Z_]+|GITHUB_TOKEN|SYNC_TOKEN|\.ssh/|id_rsa|"
        r"AWS_SECRET|\.npmrc|\.netrc|credentials)"),
     "touches credentials or a token"),
]

# Non-blocking notices: common and legitimate here, but worth seeing in the report.
NOTICES = [
    ("unsafe", re.compile(r"\bunsafe\s*[{(]|\bunsafe\s+fn\b"), "adds unsafe code"),
    ("embed", re.compile(r"\binclude_bytes!|\binclude_str!"), "embeds a file into the binary"),
    ("env", re.compile(r"\benv::var|\bstd::env\b|getenv"), "reads an environment variable"),
    ("fs-write", re.compile(r"\bfs::write|\bfs::remove|\bFile::create|\bOpenOptions"),
     "writes or removes a file"),
]

LONG_BLOB = re.compile(r"[A-Za-z0-9+/=]{200,}")


# ---------------------------------------------------------------- errors and git
def fail(msg: str) -> "None":
    """Exit 3: the scan could not be completed. Never exit 0 on an error."""
    print(f"\nERROR: {msg}", file=sys.stderr)
    print("pr-gate could not complete, so nothing is cleared. Treat this as BLOCKED.",
          file=sys.stderr)
    sys.exit(3)


def refuse(msg: str) -> "None":
    print(f"\nREFUSED: {msg}", file=sys.stderr)
    sys.exit(2)


def git(*args: str, check: bool = True) -> subprocess.CompletedProcess:
    """git in the repository this script lives in; output as bytes (a diff is not text)."""
    try:
        r = subprocess.run(["git", "-C", ROOT, *args], capture_output=True, timeout=600)
    except (OSError, subprocess.SubprocessError) as e:
        fail(f"`git {' '.join(args[:2])}` could not run: {e}")
    if check and r.returncode != 0:
        detail = r.stderr.decode("utf-8", "replace").strip().splitlines()
        fail(f"`git {' '.join(args[:2])}` failed: {detail[-1] if detail else r.returncode}")
    return r


def git_text(*args: str, check: bool = True) -> str:
    return git(*args, check=check).stdout.decode("utf-8", "replace").strip()


def state_dir() -> str:
    """<git-common-dir>/pr-gate: one place for the main checkout and every worktree."""
    common = git_text("rev-parse", "--path-format=absolute", "--git-common-dir")
    return os.path.join(common, "pr-gate")


def state_path(commit: str, kind: str) -> str:
    if not ID_RE.match(commit):
        fail(f"`{commit}` is not a full 40-hex commit id.")
    return os.path.join(state_dir(), f"{commit}.{kind}")


def remote_url(repo: str) -> str:
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repo):
        fail(f"`{repo}` is not OWNER/NAME.")
    return f"https://github.com/{repo}.git"


def pr_head(repo: str, number: int) -> str:
    """The pull request's current head commit, from the GitHub API."""
    if not shutil.which("gh"):
        fail("the GitHub CLI `gh` is not on PATH.")
    try:
        r = subprocess.run(["gh", "api", f"repos/{repo}/pulls/{number}", "--jq", ".head.sha"],
                           capture_output=True, text=True, errors="replace", timeout=180)
    except (OSError, subprocess.SubprocessError) as e:
        fail(f"`gh api` could not run: {e}")
    head = r.stdout.strip()
    if r.returncode != 0 or not ID_RE.match(head):
        fail(f"could not read the head commit of {repo}#{number}: "
             f"{(r.stderr or head).strip()[:200]}")
    return head


def sync_head(repo: str) -> str:
    out = git_text("ls-remote", remote_url(repo), SYNC_REF)
    head = out.split()[0] if out else ""
    if not ID_RE.match(head):
        fail(f"could not read {SYNC_REF} of {repo}.")
    return head


def current_head(repo: str, number: "int | None", sync: bool) -> str:
    return sync_head(repo) if sync else pr_head(repo, number)


def sha256_file(path: str) -> str:
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def count_lines(data: bytes) -> int:
    return data.count(b"\n") + (0 if data.endswith(b"\n") else 1)


def write_json(path: str, doc: dict) -> None:
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8", newline="\n") as f:
        json.dump(doc, f, indent=1, sort_keys=True)
        f.write("\n")
    os.replace(tmp, path)


def read_json(path: str) -> dict:
    try:
        with open(path, encoding="utf-8") as f:
            doc = json.load(f)
    except (OSError, ValueError):
        return {}
    return doc if isinstance(doc, dict) else {}


# ---------------------------------------------------------------- hints
def generated_files(extra: dict) -> dict:
    """{path: generator}: files in the local checkout whose first lines name a generator (the
    marker alone is enough), the built-in list, and `extra` (new files in the diff)."""
    found = dict(GENERATED_FALLBACK)
    for folder, _, names in os.walk(os.path.join(ROOT, "crates")):
        for name in names:
            if not name.endswith((".rs", ".toml")):
                continue
            path = os.path.join(folder, name)
            try:
                with open(path, encoding="utf-8", errors="replace") as f:
                    head = "".join(next(f, "") for _ in range(8))
            except OSError:
                continue
            m = GENERATED_MARKER.search(head)
            if m:
                found[os.path.relpath(path, ROOT).replace("\\", "/")] = m.group(1)
    found.update(extra)
    return found


def classify(path: str, generated: dict) -> list:
    hits = [key for key, patterns in BUCKET_PATTERNS.items()
            if any(fnmatch.fnmatch(path, p) for p in patterns)]
    if path in generated:
        hits.append("generated")
    return hits


def scan_patch(patch: str) -> tuple:
    """(suspicious, notices) on the added lines only."""
    added = [ln[1:] for ln in patch.splitlines() if ln.startswith("+") and not ln.startswith("+++")]
    body = "\n".join(added)
    suspicious = [(k, why) for k, pat, why in SUSPICIOUS if pat.search(body)]
    notices = [(k, why) for k, pat, why in NOTICES if pat.search(body)]
    if LONG_BLOB.search(body):
        suspicious.append(("long-blob", "adds a very long encoded-looking literal"))
    return suspicious, notices


def lock_findings(patch: str) -> list:
    out = []
    for ln in patch.splitlines():
        if not ln.startswith("+") or ln.startswith("+++"):
            continue
        text = ln[1:].strip()
        m = re.match(r'name\s*=\s*"([^"]+)"', text)
        if m:
            out.append(["package", m.group(1)])
        m = re.match(r'source\s*=\s*"(git\+[^"]+)"', text)
        if m:
            out.append(["git-source", m.group(1)])
    return out


def split_diff(text: str) -> list:
    """[{path, binary, lines}] per file of a `git diff --no-renames --binary` text."""
    files, cur = [], None
    for ln in text.split("\n"):
        if ln.startswith("diff --git "):
            rest = ln[len("diff --git "):]
            n = (len(rest) - 5) // 2          # "a/P b/P" with no renames
            path = rest[2:2 + n] if rest.startswith("a/") and n > 0 else rest
            cur = {"path": path.strip('"'), "binary": False, "lines": []}
            files.append(cur)
        elif cur is not None:
            cur["lines"].append(ln)
            if ln.startswith("GIT binary patch") or ln.startswith("Binary files"):
                cur["binary"] = True
    return files


def build_hints(diff_text: str) -> dict:
    files = split_diff(diff_text)
    extra = {}
    for f in files:
        added = [ln[1:] for ln in f["lines"] if ln.startswith("+") and not ln.startswith("+++")]
        m = GENERATED_MARKER.search("\n".join(added[:8]))
        if m:
            extra[f["path"]] = m.group(1)
    generated = generated_files(extra)
    flagged, notices, deps = {}, [], []
    for f in files:
        keys = classify(f["path"], generated)
        if f["binary"]:
            keys.append("binary")
        patch = "\n".join(f["lines"])
        suspicious, notes = scan_patch(patch) if not f["binary"] else ([], [])
        if f["path"].endswith("Cargo.lock"):
            deps += lock_findings(patch)
        notices += [[f["path"], k, why] for k, why in notes]
        if suspicious:
            keys.append("suspicious")
        if keys:
            flagged[f["path"]] = {"buckets": sorted(set(keys)),
                                  "suspicious": [list(x) for x in suspicious],
                                  "generator": generated.get(f["path"])}
    return {"files": len(files), "flagged": flagged, "notices": notices, "dependencies": deps}


# ---------------------------------------------------------------- scan
def do_scan(repo: str, number: "int | None", sync: bool) -> int:
    head = current_head(repo, number, sync)
    url = remote_url(repo)
    ref = SYNC_REF if sync else f"pull/{number}/head"
    dest = f"refs/pr-gate/{head}"
    print(f"pr-gate: fetching {repo} {ref} (head {head}); nothing is checked out or run.")
    git("fetch", "--no-tags", "--", url, f"{ref}:{dest}")
    fetched = git_text("rev-parse", "--verify", "--quiet", f"{dest}^{{commit}}", check=False)
    if fetched != head:
        git("update-ref", "-d", dest, check=False)
        fail(f"fetched {fetched or 'nothing'}, but the head is {head}: it moved during the scan. "
             "Run the scan again.")
    base = git_text("merge-base", "HEAD", head, check=False)
    if not ID_RE.match(base):
        fail("no merge base between HEAD and the head: unrelated history.")
    diff = git("diff", "--no-ext-diff", "--no-textconv", "--no-renames", "--binary",
               base, head).stdout
    if not diff.strip():
        fail("the diff against the merge base is empty (already merged?).")
    os.makedirs(state_dir(), exist_ok=True)
    with open(state_path(head, "diff"), "wb") as f:
        f.write(diff)
    lines = count_lines(diff)
    hints = build_hints(diff.decode("utf-8", errors="replace"))
    write_json(state_path(head, "scan.json"), {
        "id": head, "base": base, "diff_sha256": hashlib.sha256(diff).hexdigest(),
        "lines": lines, "repo": repo, "number": number, "sync": sync, "hints": hints,
        "scanned_at": time.strftime("%Y-%m-%dT%H:%M:%S%z")})
    print(f"\nbase {base}\ndiff {state_path(head, 'diff')}  ({lines} lines, "
          f"{hints['files']} file(s), {len(hints['flagged'])} flagged)\n")
    titles = {k: t for k, t, _, _ in BUCKETS}
    for path, info in sorted(hints["flagged"].items()):
        tags = [titles.get(b, b) for b in info["buckets"]]
        print(f"  hint: {path}: {', '.join(tags)}")
        for key, why in info["suspicious"]:
            print(f"        {key}: {why}")
    for kind, value in hints["dependencies"]:
        print(f"  hint: Cargo.lock {kind}: {value}")
    print("\nREVIEW NEEDED: read the whole diff, then write a report and run\n"
          f"  python scripts/dev/pr-gate.py record-review {head} --report <file>\n"
          "Hints above are not a verdict; every line must be covered by the reported ranges.")
    return 2


# ---------------------------------------------------------------- review
def check_ranges(ranges: object, lines: int) -> str:
    """"" when the ranges cover 1..lines with no gap, else the reason."""
    spans = []
    if not isinstance(ranges, list) or not ranges:
        return "ranges must be a non-empty list of [first, last] line numbers"
    for r in ranges:
        if (not isinstance(r, list) or len(r) != 2
                or any(not isinstance(x, int) or isinstance(x, bool) for x in r)):
            return "every range must be [first, last] with two integers"
        if r[0] < 1 or r[1] < r[0] or r[1] > lines:
            return f"range {r} is outside 1..{lines}"
        spans.append((r[0], r[1]))
    spans.sort()
    reach = 0
    for a, b in spans:
        if a > reach + 1:
            return f"lines {reach + 1}..{a - 1} were not reviewed (gap)"
        reach = max(reach, b)
    if reach < lines:
        return f"lines {reach + 1}..{lines} were not reviewed"
    return ""


def verify(commit: str) -> tuple:
    """(problems, scan, review): no problems means the scan and the review match each other and
    the diff on disk. Re-hashes the diff every time, so an edited diff is caught."""
    problems = []
    scan = read_json(state_path(commit, "scan.json"))
    review = read_json(state_path(commit, "review.json"))
    if not scan:
        return ["there is no scan for this commit (run scan)"], scan, review
    diff_path = state_path(commit, "diff")
    try:
        with open(diff_path, "rb") as f:
            diff = f.read()
    except OSError:
        return ["the saved diff is missing"], scan, review
    if scan.get("id") != commit:
        problems.append("the scan is for another commit")
    if hashlib.sha256(diff).hexdigest() != scan.get("diff_sha256"):
        problems.append("the saved diff no longer matches the scan (it was changed)")
    if count_lines(diff) != scan.get("lines"):
        problems.append("the saved diff has a different line count than the scan")
    if not review:
        problems.append("there is no recorded review (run record-review)")
        return problems, scan, review
    for key in ("id", "base", "diff_sha256", "lines"):
        if review.get(key) != scan.get(key):
            problems.append(f"the review's {key} does not match the scan")
    if review.get("verdict") not in ("clean", "findings"):
        problems.append("the review verdict is not clean or findings")
    why = check_ranges(review.get("ranges"), scan.get("lines", 0))
    if why:
        problems.append(why)
    return problems, scan, review


def review_hash(commit: str) -> str:
    return sha256_file(state_path(commit, "review.json"))


def do_record(commit: str, report_path: str) -> int:
    scan = read_json(state_path(commit, "scan.json"))
    if not scan:
        refuse("there is no scan for this commit; run scan first.")
    try:
        with open(report_path, encoding="utf-8") as f:
            report = json.load(f)
    except (OSError, ValueError) as e:
        refuse(f"the report cannot be read: {e}")
    if not isinstance(report, dict):
        refuse("the report must be a JSON object.")
    for key in ("id", "base", "diff_sha256"):
        if report.get(key) != scan.get(key) or report.get(key) is None:
            refuse(f"the report's {key} does not match the scan (is it for another commit, "
                   "another base, or another diff?).")
    try:
        with open(state_path(commit, "diff"), "rb") as f:
            actual = hashlib.sha256(f.read()).hexdigest()
    except OSError:
        refuse("the saved diff is missing; scan again.")
    if actual != scan.get("diff_sha256"):
        refuse("the saved diff was changed after the scan; scan again.")
    reviewer = report.get("reviewer")
    if not isinstance(reviewer, str) or not reviewer.strip():
        refuse("the report names no reviewer.")
    verdict = report.get("verdict")
    if verdict not in VERDICTS:
        refuse(f"verdict must be one of {', '.join(VERDICTS)}.")
    if verdict == "malicious":
        refuse("the reviewer's verdict is MALICIOUS. Do not merge this commit.")
    findings = report.get("findings", [])
    if not isinstance(findings, list):
        refuse("findings must be a list.")
    why = check_ranges(report.get("ranges"), scan["lines"])
    if why:
        refuse(f"the review does not cover the whole diff: {why}.")
    write_json(state_path(commit, "review.json"), {
        "id": commit, "base": scan["base"], "diff_sha256": scan["diff_sha256"],
        "lines": scan["lines"], "reviewer": reviewer.strip(), "verdict": verdict,
        "ranges": report["ranges"], "findings": findings,
        "recorded_at": time.strftime("%Y-%m-%dT%H:%M:%S%z")})
    print(f"review recorded for {commit}: {verdict}, {len(findings)} finding(s), "
          f"by {reviewer.strip()}.")
    return 0


def do_status(commit: str) -> int:
    problems, scan, review = verify(commit)
    if problems:
        print(f"status {commit}: NOT READY", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 2
    print(f"status {commit}: ready (base {scan['base']}, review {review['verdict']} by "
          f"{review['reviewer']}, sha256 {review_hash(commit)})")
    return 0


def main(argv: "list | None" = None) -> int:
    ap = argparse.ArgumentParser(description="Review gate for pull requests and upstream syncs.")
    sub = ap.add_subparsers(dest="cmd", required=True)
    sp = sub.add_parser("scan", help="fetch a head, save its full diff")
    sp.add_argument("number", nargs="?", type=int, help="pull request number")
    sp.add_argument("--sync", action="store_true", help="scan upstream main instead of a PR")
    sp.add_argument("--repo", default=DEFAULT_REPO, help=f"OWNER/NAME (default {DEFAULT_REPO})")
    rp = sub.add_parser("record-review", help="record a review report for a scanned commit")
    rp.add_argument("id")
    rp.add_argument("--report", required=True, metavar="FILE")
    st = sub.add_parser("status", help="exit 0 only when scan and review match")
    st.add_argument("id")
    a = ap.parse_args(argv)
    if a.cmd == "scan":
        if (a.number is None) == (not a.sync):
            ap.error("give a pull request number or --sync, not both")
        return do_scan(a.repo, a.number, a.sync)
    if not ID_RE.match(a.id):
        ap.error("give the full 40-hex commit id")
    return do_record(a.id, a.report) if a.cmd == "record-review" else do_status(a.id)


if __name__ == "__main__":
    sys.exit(main())
