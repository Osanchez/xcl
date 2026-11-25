#!/usr/bin/env python3
"""Security gate for a pull request, to run *before* anything of it is checked out or built.

    scripts/dev/pr-gate.py 2343                       # scan upstream PR 2343
    scripts/dev/pr-gate.py 2343 --repo Osanchez/xcl   # another repository
    scripts/dev/pr-gate.py --diff origin/main..HEAD   # a local branch instead of a PR
    scripts/dev/pr-gate.py 2343 --ack crates/xcl/build.rs   # after reviewing that file by hand
    scripts/dev/pr-gate.py 2343 --json                # machine-readable

Why this exists: reading a diff is safe, but `cargo build` runs the pull request's `build.rs`
and any procedural macro as you, `cargo test` runs its test code, and anything under
`scripts/` runs as you. So the dangerous files have to be found before the first build, not
after it.

What this does NOT do, by design:

* it never checks out, fetches, compiles or executes any code from the pull request;
* for a pull request it only reads the GitHub API's file list and text patches (`gh api`),
  and for `--diff` only `git diff` output of refs already in the local repository.

Exit codes (the gate is fail-closed: anything it cannot determine blocks the merge):

    0   clear - no blocking finding, or every blocking path was acknowledged with --ack
    2   BLOCKED - a high-risk path or a suspicious added line needs manual review
    3   could not scan (no `gh`, API error, unreadable diff) - treat exactly like BLOCKED

Intended flow for a manual merge:

    1. run this; if it exits 0, merge.
    2. if it exits 2, read every file it names, in the browser or with `gh pr diff`.
    3. re-run with one --ack per blocking path you have read and accepted. The acks are
       echoed into the report so the decision is auditable.
    4. only then merge.
"""
from __future__ import annotations

import argparse
import fnmatch
import json
import os
import re
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# Where a CLEAR verdict is recorded, one file per reviewed head commit. In .git/ on purpose:
# it is local state, must never be committed, and must not survive a fresh clone.
# merge-pr.py refuses to merge a commit with no receipt, so a force-pushed pull request (new
# head commit) has to pass the gate again - a receipt keyed by pull request *number* would
# silently bless whatever was pushed after the review.
RECEIPTS = os.path.join(ROOT, ".git", "pr-gate")


def write_receipt(head: str, repo: str, number: "int | None", acks: set) -> str:
    """Record that `head` passed. Returns the receipt path, or "" if it could not be written
    (which never turns a BLOCKED verdict into a pass - it only means merge-pr.py will ask for
    the gate again)."""
    if not head:
        return ""
    import time
    try:
        os.makedirs(RECEIPTS, exist_ok=True)
        path = os.path.join(RECEIPTS, f"{head}.json")
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            json.dump({"head": head, "repo": repo, "number": number, "verdict": "CLEAR",
                       "acknowledged": sorted(acks), "when": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
                       "gate": os.path.basename(__file__)}, f, indent=1)
            f.write("\n")
        return path
    except OSError:
        return ""


def pr_head(repo: str, number: int) -> str:
    """The pull request's current head commit."""
    doc = gh_json(f"repos/{repo}/pulls/{number}")
    head = doc.get("head", {}).get("sha", "") if isinstance(doc, dict) else ""
    if not head:
        fail(f"could not read the head commit of {repo}#{number}.")
    return head

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


def fail(msg: str) -> "None":
    """Exit 3: the scan could not be completed. Never exit 0 on an error - that would read as
    a pass and is exactly the case where a merge must not happen."""
    print(f"\nERROR: {msg}", file=sys.stderr)
    print("pr-gate could not complete its scan, so nothing is cleared. Treat this as BLOCKED.",
          file=sys.stderr)
    sys.exit(3)


def gh_json(path: str, paginate: bool = False) -> object:
    if not shutil.which("gh"):
        fail("the GitHub CLI `gh` is not on PATH; install it or use --diff for a local branch.")
    cmd = ["gh", "api", path]
    if paginate:
        cmd.append("--paginate")
        cmd += ["--slurp"]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, errors="replace", timeout=180)
    except (OSError, subprocess.SubprocessError) as e:
        fail(f"`gh api {path}` could not run: {e}")
    if r.returncode != 0:
        detail = (r.stderr or r.stdout or "").strip().splitlines()
        fail(f"`gh api {path}` failed: {detail[0] if detail else f'exit {r.returncode}'}")
    try:
        doc = json.loads(r.stdout)
    except ValueError as e:
        fail(f"`gh api {path}` did not return JSON: {e}")
    # --paginate --slurp gives a list of pages; flatten it.
    if paginate and isinstance(doc, list) and doc and isinstance(doc[0], list):
        return [item for page in doc for item in page]
    return doc


def generated_files() -> dict:
    """{path: generator} for every file in the local checkout that names its generator in its
    first lines. Falls back to GENERATED_FALLBACK when the checkout cannot be read (the gate
    may be run from outside the repository)."""
    found = {}
    src = os.path.join(ROOT, "crates")
    for folder, _, names in os.walk(src):
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
            # Only count a file whose marker says "do not edit": several files mention a
            # generator in prose while being hand-written logic (gpu/tiling.rs).
            if m and re.search(r"do not edit", head, re.I):
                rel = os.path.relpath(path, ROOT).replace("\\", "/")
                found[rel] = m.group(1)
    return found or dict(GENERATED_FALLBACK)


def classify(path: str, generated: dict) -> list:
    """Every bucket key the path falls into (a file can be in more than one)."""
    hits = []
    for key, patterns in BUCKET_PATTERNS.items():
        if any(fnmatch.fnmatch(path, p) for p in patterns):
            hits.append(key)
    if path in generated:
        hits.append("generated")
    return hits


def scan_patch(patch: str) -> tuple:
    """(suspicious, notices) found on the patch's added lines only. Removed and context lines
    are not scanned: what matters is what the pull request introduces."""
    added = [ln[1:] for ln in patch.splitlines()
             if ln.startswith("+") and not ln.startswith("+++")]
    body = "\n".join(added)
    suspicious = [(k, why) for k, pat, why in SUSPICIOUS if pat.search(body)]
    notices = [(k, why) for k, pat, why in NOTICES if pat.search(body)]
    if LONG_BLOB.search(body):
        suspicious.append(("long-blob", "adds a very long encoded-looking literal"))
    return suspicious, notices


def lock_findings(patch: str) -> list:
    """Added dependency names and any git source in a Cargo.lock patch."""
    out = []
    for ln in patch.splitlines():
        if not ln.startswith("+") or ln.startswith("+++"):
            continue
        text = ln[1:].strip()
        m = re.match(r'name\s*=\s*"([^"]+)"', text)
        if m:
            out.append(("package", m.group(1)))
        m = re.match(r'source\s*=\s*"(git\+[^"]+)"', text)
        if m:
            out.append(("git-source", m.group(1)))
    return out


def pr_files(repo: str, number: int) -> list:
    """[{path, status, binary, patch}] for a pull request, from the API only. No checkout."""
    rows = gh_json(f"repos/{repo}/pulls/{number}/files?per_page=100", paginate=True)
    if not isinstance(rows, list):
        fail("the pull request file list was not a JSON array.")
    files = []
    for row in rows:
        if not isinstance(row, dict) or "filename" not in row:
            continue
        patch = row.get("patch")
        files.append({
            "path": row["filename"],
            "status": row.get("status", "?"),
            # GitHub omits the patch for a binary file; changes == 0 with no patch is the
            # same signal scripts/pr_check.py uses.
            "binary": patch is None and row.get("changes", 0) == 0,
            "patch": patch or "",
        })
    return files


def diff_files(rev: str) -> list:
    """The same shape as pr_files, from `git diff` of refs already in the local repository."""
    try:
        names = subprocess.run(["git", "-C", ROOT, "diff", "--numstat", rev],
                               capture_output=True, text=True, errors="replace", timeout=120)
    except (OSError, subprocess.SubprocessError) as e:
        fail(f"`git diff --numstat {rev}` could not run: {e}")
    if names.returncode != 0:
        fail(f"`git diff --numstat {rev}` failed: {(names.stderr or '').strip()}")
    files = []
    for line in names.stdout.splitlines():
        parts = line.split("\t")
        if len(parts) != 3:
            continue
        adds, dels, path = parts
        binary = adds == "-" and dels == "-"   # git marks a binary file this way
        patch = ""
        if not binary:
            one = subprocess.run(["git", "-C", ROOT, "diff", rev, "--", path],
                                 capture_output=True, text=True, errors="replace", timeout=120)
            patch = one.stdout if one.returncode == 0 else ""
        files.append({"path": path, "status": "modified", "binary": binary, "patch": patch})
    return files


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Security gate for a pull request; run it before checking out or building.")
    ap.add_argument("number", nargs="?", type=int, help="pull request number")
    ap.add_argument("--repo", default="ivall/xcl", help="OWNER/NAME (default ivall/xcl)")
    ap.add_argument("--diff", metavar="REV", help="scan a local range instead, e.g. origin/main..HEAD")
    ap.add_argument("--ack", action="append", default=[], metavar="PATH",
                    help="a blocking path you have read and accepted (repeatable)")
    ap.add_argument("--json", action="store_true", help="machine-readable output")
    a = ap.parse_args()
    if (a.number is None) == (a.diff is None):
        ap.error("give either a pull request number or --diff REV")

    generated = generated_files()
    # Read the head commit first: the receipt is bound to it, and merge-pr.py re-checks that
    # the commit it fetches is the same one, so a push between the scan and the merge is caught.
    head = pr_head(a.repo, a.number) if a.number is not None else ""
    files = pr_files(a.repo, a.number) if a.number is not None else diff_files(a.diff)
    if not files:
        fail("the scan found no changed files, which is never true for a real pull request.")

    acked = {p.replace("\\", "/") for p in a.ack}
    findings, notices, deps, clean = {}, [], [], []
    for f in files:
        path = f["path"]
        keys = classify(path, generated)
        if f["binary"]:
            keys.append("binary")
        suspicious, notes = scan_patch(f["patch"]) if f["patch"] else ([], [])
        if "dependencies" in keys and path.endswith("Cargo.lock"):
            deps += lock_findings(f["patch"])
        for key, why in notes:
            notices.append((path, key, why))
        if suspicious:
            keys.append("suspicious")
        if keys:
            findings[path] = {"buckets": sorted(set(keys)), "status": f["status"],
                              "suspicious": suspicious,
                              "generator": generated.get(path)}
        else:
            clean.append(path)

    blocking = sorted(p for p in findings if p not in acked)
    verdict = "CLEAR" if not blocking else "BLOCKED"

    receipt = write_receipt(head, a.repo, a.number, acked) if verdict == "CLEAR" else ""

    if a.json:
        print(json.dumps({"verdict": verdict, "repo": a.repo, "number": a.number,
                          "diff": a.diff, "head": head, "receipt": receipt,
                          "files": len(files), "findings": findings,
                          "blocking": blocking, "acknowledged": sorted(acked),
                          "notices": notices, "dependencies": deps}, indent=1))
        return 0 if verdict == "CLEAR" else 2

    target = f"{a.repo}#{a.number}" if a.number is not None else a.diff
    print(f"pr-gate: {target} - {len(files)} changed file(s), {len(clean)} with no risk flag\n")

    by_bucket = {}
    for path, info in findings.items():
        for key in info["buckets"]:
            by_bucket.setdefault(key, []).append(path)

    for key, title, why, check in BUCKETS:
        paths = sorted(by_bucket.get(key, []))
        if not paths:
            continue
        print(f"[{key}] {title}")
        print(f"  why:   {why}")
        print(f"  check: {check}")
        for p in paths:
            mark = "  (acknowledged)" if p in acked else ""
            gen = findings[p].get("generator")
            print(f"    - {p} ({findings[p]['status']}){mark}")
            if key == "generated" and gen:
                print(f"        regenerate with: python {gen}   then diff; it must match exactly")
        print()

    if "suspicious" in by_bucket:
        print("[suspicious] Added lines that look like a dropper")
        print("  why:   these idioms are rare in this codebase and are how code fetches, decodes")
        print("         or launches something at run time.")
        print("  check: read every added line in these files before you build anything.")
        for p in sorted(by_bucket["suspicious"]):
            mark = "  (acknowledged)" if p in acked else ""
            print(f"    - {p}{mark}")
            for key, meaning in findings[p]["suspicious"]:
                print(f"        {key}: {meaning}")
        print()

    if deps:
        print("[dependencies] Changes in Cargo.lock")
        for kind, value in deps:
            flag = "  <-- never acceptable" if kind == "git-source" else ""
            print(f"    {kind}: {value}{flag}")
        print("  check: every added package name, character by character, against crates.io.\n")

    if notices:
        print("Notices (not blocking, but worth a look):")
        for path, key, why in notices:
            print(f"    {path}: {key} - {why}")
        print()

    if verdict == "CLEAR":
        if acked:
            print("Acknowledged by hand this run:")
            for p in sorted(acked):
                print(f"    - {p}")
        print("VERDICT: CLEAR - no unreviewed high-risk path. Safe to check out, build and merge.")
        if head:
            print(f"         head commit: {head}")
            if receipt:
                print(f"         receipt:     {receipt}")
                print("         merge it with: python scripts/dev/merge-pr.py "
                      f"{a.number} --repo {a.repo}")
            else:
                print("         NOTE: the receipt could not be written, so merge-pr.py will "
                      "ask for the gate again.")
        return 0

    print(f"VERDICT: BLOCKED - {len(blocking)} path(s) need a manual read first:")
    for p in blocking:
        print(f"    - {p}")
    print("\nRead them without building anything:")
    if a.number is not None:
        print(f"    gh pr diff {a.number} --repo {a.repo} -- {' '.join(blocking[:6])}")
    print("Then re-run with one --ack per path you accept, e.g.:")
    print("    " + " ".join([f"--ack {p}" for p in blocking[:3]]))
    print("\nDo not build or merge until this exits 0.")
    return 2


if __name__ == "__main__":
    sys.exit(main())
