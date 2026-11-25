#!/usr/bin/env python3
"""PreToolUse hook: outside code (pull requests, upstream syncs) enters this fork only through
scripts/dev/merge-pr.py, after a full-diff review bound to the exact commit and the user's OK.

Claude Code hands this script the tool call as JSON on stdin. Three outcomes:

    allow   exit 0
    ask     exit 0 with {"hookSpecificOutput": {"permissionDecision": "ask", ...}} on stdout,
            so the user decides in Claude Code's own prompt
    deny    exit 2, the reason on stderr is shown to the agent

Any internal error is a deny: Claude Code reads a non-zero exit other than 2 as "allow", so an
unhandled exception here would silently permit exactly what this guard exists to stop.

Not a sandbox. It reads the command text the way a shell would (separators with or without
spaces, quotes, heredocs, $( ), backticks, bash -c, eval, cmd /c, powershell -Command) and
cannot see what a script file or a program it does not know will do. It makes the direct paths
loud; the pre-merge-commit hook and the review in pr-gate.py are the other layers.

    deny   merge/pull/rebase/cherry-pick/reset/checkout/switch/restore/worktree add/am/apply of
           FETCH_HEAD, MERGE_HEAD, a URL or refs/pr-gate/*; gh pr merge|checkout, gh repo sync;
           sync-upstream.sh (without --dry-run); core.hooksPath; --no-verify on merge
    ask    merge-pr.py (without --dry-run); any other merge, pull, rebase, cherry-pick, am,
           apply; reset/checkout/switch to something that is neither an ancestor of HEAD nor
           a local branch
    allow  everything else, including --abort/--continue/--quit/--skip and read-only git
"""
from __future__ import annotations

import base64
import json
import os
import re
import subprocess
import sys
from typing import NamedTuple

DENY, ASK, ALLOW = "deny", "ask", "allow"

# Project directory: used to resolve refs for the ancestor / local-branch questions.
ROOT = os.environ.get("CLAUDE_PROJECT_DIR") or os.path.dirname(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

MAX_DEPTH = 6


# ------------------------------------------------------------------------------ lexer
class LexError(Exception):
    """Unbalanced quote or substitution: the text cannot be read as shell."""


class Word(NamedTuple):
    text: str
    dyn: bool          # contains $var, $( ), backticks or $'..': its value is not known


class Cmd:
    def __init__(self) -> None:
        self.words: list[Word] = []
        self.stdin: list[str] = []      # heredoc and here-string bodies
        self.piped_in = False


class Lexer:
    """A small shell reader. Every command found anywhere, including inside $( ), backticks,
    subshells and double quotes, is appended to `out`: a command substitution runs, so it is
    treated like any other command. Single-quoted text and comments are data."""

    def __init__(self, text: str, out: list) -> None:
        self.s = text
        self.i = 0
        self.out = out
        self.cmd = Cmd()
        self.parts: list[str] = []
        self.have = False
        self.dyn = False
        self.quoted = False
        self.mode: str | None = None    # next word is: skip (redirect target) | here | delim
        self.dash = False
        self.pending: list = []         # heredocs waiting for their body at the next newline

    # -- word state
    def flush(self) -> None:
        if not self.have:
            return
        text, dyn, quoted = "".join(self.parts), self.dyn, self.quoted
        self.parts, self.have, self.dyn, self.quoted = [], False, False, False
        mode, self.mode = self.mode, None
        if mode == "skip":
            return
        if mode == "here":
            self.cmd.stdin.append(text)
        elif mode == "delim":
            self.pending.append((text, self.dash, quoted, self.cmd))
        else:
            self.cmd.words.append(Word(text, dyn))

    def end_cmd(self, pipe: bool = False) -> None:
        self.flush()
        self.mode = None
        c = self.cmd
        if c.words or c.stdin:
            self.out.append(c)
            self.cmd = Cmd()
            self.cmd.piped_in = pipe
        else:
            self.cmd.piped_in = pipe or c.piped_in   # `a |` newline `b` is still a pipe

    def add(self, text: str, dyn: bool = False, quoted: bool = False) -> None:
        self.parts.append(text)
        self.have = True
        self.dyn = self.dyn or dyn
        self.quoted = self.quoted or quoted

    def sub(self, term: str) -> None:
        inner = Lexer(self.s, self.out)
        inner.i = self.i
        inner.parse(term)
        self.i = inner.i

    # -- constructs
    def read_dq(self) -> None:
        s, n = self.s, len(self.s)
        self.i += 1
        self.add("", quoted=True)
        while self.i < n:
            c = s[self.i]
            nxt = s[self.i + 1:self.i + 2]
            if c == '"':
                self.i += 1
                return
            if c == "\\" and nxt and nxt in '$`"\\\n':
                if nxt != "\n":
                    self.add(nxt)
                self.i += 2
            elif c == "$" and nxt == "(":
                self.i += 2
                self.add("$(..)", dyn=True)
                self.sub(")")
            elif c == "`":
                self.i += 1
                self.add("`..`", dyn=True)
                self.sub("`")
            elif c == "$" and nxt == "{":
                self.dollar_brace()
            elif c == "$" and (nxt.isalnum() or nxt in "_@*#?!$-" and nxt):
                self.dollar_name()
            else:
                self.add(c)
                self.i += 1
        raise LexError("unterminated double quote")

    def dollar_brace(self) -> None:
        s, n = self.s, len(self.s)
        j, depth = self.i + 2, 1
        while j < n and depth:
            depth += {"{": 1, "}": -1}.get(s[j], 0)
            j += 1
        if depth:
            raise LexError("unterminated ${")
        self.add(s[self.i:j], dyn=True)
        self.i = j

    def dollar_name(self) -> None:
        s, n = self.s, len(self.s)
        j = self.i + 1
        if s[j].isalnum() or s[j] == "_":
            while j < n and (s[j].isalnum() or s[j] == "_"):
                j += 1
        else:
            j += 1
        self.add(s[self.i:j], dyn=True)
        self.i = j

    def redirect(self) -> None:
        s, n = self.s, len(self.s)
        if self.have and "".join(self.parts).isdigit() and not self.quoted:
            self.parts, self.have = [], False            # the fd number of `2>file`
        else:
            self.flush()
        i = self.i
        if s.startswith("<<<", i):
            self.i, self.mode = i + 3, "here"
        elif s.startswith("<<", i):
            i += 2
            self.dash = s[i:i + 1] == "-"
            self.i = i + (1 if self.dash else 0)
            self.mode = "delim"
        elif s[i] in "<>" and s[i + 1:i + 2] == "(":     # process substitution runs a command
            self.i = i + 2
            self.sub(")")
        else:
            j = i + 1 if s[i] == "&" else i
            while j < n and s[j] in "<>":
                j += 1
            if j < n and s[j] in "&|":
                j += 1
            self.i, self.mode = j, "skip"

    def read_heredocs(self) -> None:
        s, n = self.s, len(self.s)
        for delim, dash, quoted, cmd in self.pending:
            lines = []
            while self.i < n:
                j = s.find("\n", self.i)
                line = s[self.i:j if j != -1 else n]
                self.i = j + 1 if j != -1 else n
                check = line.rstrip("\r")
                if dash:
                    check = check.lstrip("\t")
                if check == delim:
                    break
                lines.append(line)
            body = "\n".join(lines)
            cmd.stdin.append(body)
            if not quoted:
                self.expand(body)          # an unquoted heredoc still runs $( ) and backticks
        self.pending = []

    def expand(self, text: str) -> None:
        i = 0
        while i < len(text):
            c = text[i]
            if c == "\\":
                i += 2
            elif c == "$" and text[i + 1:i + 2] == "(":
                inner = Lexer(text, self.out)
                inner.i = i + 2
                inner.parse(")")
                i = inner.i
            elif c == "`":
                inner = Lexer(text, self.out)
                inner.i = i + 1
                inner.parse("`")
                i = inner.i
            else:
                i += 1

    # -- main loop
    def parse(self, term: str | None) -> None:
        s, n = self.s, len(self.s)
        paren = 0
        while self.i < n:
            c = s[self.i]
            nxt = s[self.i + 1:self.i + 2]
            if c in " \t\r":
                self.flush()
                self.i += 1
            elif c == "\\":
                if nxt == "\n":
                    self.i += 2                      # line continuation
                elif nxt and nxt in " \t\"'\\$;&|()<>#`{}*?[]!~":
                    self.add(nxt, quoted=True)
                    self.i += 2
                else:                                # `C:\dir`: keep it, matching code cleans
                    self.add("\\")
                    self.i += 1
            elif c == "\n":
                self.end_cmd()
                self.i += 1
                if self.pending:
                    self.read_heredocs()
            elif c == "#" and not self.have:
                j = s.find("\n", self.i)
                self.i = n if j == -1 else j
            elif c == "&" and nxt == ">":
                self.redirect()
            elif c in ";|&":
                pipe = c == "|"
                self.i += 2 if nxt and nxt in ";|&" else 1
                self.end_cmd(pipe)
            elif c == "(":
                self.end_cmd()
                paren += 1
                self.i += 1
            elif c == ")":
                self.end_cmd()
                self.i += 1
                if term == ")" and paren == 0:
                    return
                paren = max(0, paren - 1)
            elif c == "`":
                self.i += 1
                if term == "`":
                    self.end_cmd()
                    return
                self.add("`..`", dyn=True)
                self.sub("`")
            elif c in "{}" and not self.have:
                self.end_cmd()                       # `{ cmd; }` group
                self.i += 1
            elif c in "<>":
                self.redirect()
            elif c == "'":
                j = s.find("'", self.i + 1)
                if j == -1:
                    raise LexError("unterminated single quote")
                self.add(s[self.i + 1:j], quoted=True)
                self.i = j + 1
            elif c == '"':
                self.read_dq()
            elif c == "$":
                if nxt == "(":
                    self.i += 2
                    self.add("$(..)", dyn=True)
                    self.sub(")")
                elif nxt == "{":
                    self.dollar_brace()
                elif nxt == "'":                     # $'..' may spell anything with escapes
                    j = self.i + 2
                    while j < n and s[j] != "'":
                        j += 2 if s[j] == "\\" else 1
                    if j >= n:
                        raise LexError("unterminated $'")
                    self.add(s[self.i + 2:j], dyn=True, quoted=True)
                    self.i = j + 1
                elif nxt == '"':
                    self.i += 1
                elif nxt and (nxt.isalnum() or nxt in "_@*#?!$-"):
                    self.dollar_name()
                else:
                    self.add("$")
                    self.i += 1
            else:
                self.add(c)
                self.i += 1
        if term is not None:
            raise LexError("unterminated substitution")
        self.end_cmd()


# ------------------------------------------------------------------------------ findings
class Findings:
    def __init__(self) -> None:
        self.items: list[tuple[str, str]] = []

    def add(self, level: str, why: str) -> None:
        self.items.append((level, why))

    def result(self) -> tuple[str, str]:
        for level in (DENY, ASK):
            hits = [why for lv, why in self.items if lv == level]
            if hits:
                return level, "; ".join(dict.fromkeys(hits))
        return ALLOW, ""


# ------------------------------------------------------------------------------ recognisers
ASSIGN = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")
LEAD_KEYWORDS = {"if", "then", "else", "elif", "while", "until", "do", "!", "time", "builtin"}
SKIP_CMDS = {"for", "case", "select", "fi", "done", "esac", "in", "function"}
WRAPPERS = {"env", "command", "exec", "nohup", "sudo", "doas", "xargs", "nice", "stdbuf",
            "setsid", "start", "call"}
WRAPPER_VALUE_OPTS = {"-u", "-g", "-C", "-D", "-n", "-I", "-L", "-P", "-s", "-E", "-a", "-S", "-h"}
SHELLS = {"bash", "sh", "zsh", "dash", "ksh", "ash"}
POWERSHELLS = {"powershell", "pwsh"}
PYTHON = re.compile(r"^(python[\d.]*|pythonw|py|pypy3?)$")
OTHER_INTERPS = {"node", "perl", "ruby", "php", "deno", "bun"}
DANGER_WORD = re.compile(r"(?i)^(merge|pull|rebase|cherry-pick|checkout|reset|switch|restore|"
                         r"apply|am|fetch_head|merge_head)$")
FOREIGN_REF = re.compile(r"(?i)\b(fetch_head|merge_head)\b|refs/pr-gate/")
PR_REF = re.compile(r"(?i)pull/\d+/(head|merge)")      # a pull request's own ref
FOREIGN_URL = re.compile(r"://|^[\w.+-]+@[\w.-]+:")
LOOSE = re.compile(
    r"(?is)\bgit\b[^\n;|&]{0,200}?\b(?:merge|pull|rebase|cherry-pick|apply|am|reset|checkout|"
    r"switch|restore)\b|\bfetch_head\b|\bmerge_head\b|refs/pr-gate/|"
    r"\bgh\b[^\n]{0,40}\b(?:pr\s+(?:merge|checkout|co)|repo\s+sync)\b|merge-pr\.py|"
    r"sync-upstream|hookspath|--no-verify")

# git subcommands that bring a commit into the tree or move HEAD / a branch.
FAMILY = {"merge", "pull", "rebase", "cherry-pick", "am", "apply", "revert", "reset", "checkout",
          "switch", "restore", "worktree"}
ASK_ALWAYS = {"merge", "pull", "rebase", "cherry-pick", "am", "apply"}
RECOVERY_FLAGS = {"--abort", "--continue", "--quit", "--skip"}
# Other commands that can point a branch or the index at a fetched commit. (Read-only ones such
# as log/diff/show/rev-parse take FETCH_HEAD all the time: that is how a review is done.)
REF_MOVERS = {"branch", "update-ref", "read-tree", "checkout-index", "symbolic-ref", "stash",
              "push", "replace", "archive", "bundle"}
GLOBAL_VALUE_OPTS = {"--git-dir", "--work-tree", "--namespace", "--super-prefix", "--attr-source"}
VALUE_OPTS = {
    "checkout": {"-b", "-B", "--orphan", "--conflict", "--pathspec-from-file"},
    "switch": {"-c", "-C", "--create", "--force-create", "--orphan", "--conflict"},
    "reset": {"--pathspec-from-file"},
    "restore": {"-s", "--source", "--pathspec-from-file"},
    "worktree": {"-b", "-B", "--reason"},
    "branch": {"-u", "--set-upstream-to", "-t", "--track"},
}
BRANCH_MODES = {"-m", "-M", "-d", "-D", "-c", "-C", "-u", "--set-upstream-to", "--unset-upstream",
                "--list", "-l", "-a", "-r", "--delete", "--move", "--copy", "--contains",
                "--merged", "--no-merged", "--points-at", "--show-current", "-v", "-vv"}


def clean(text: str) -> str:
    """The word as a shell would read it: unquoted backslashes before letters are dropped."""
    return text.replace("\\", "")


def exe_names(text: str) -> set:
    names = set()
    for t in (text, clean(text)):
        base = re.split(r"[\\/]", t)[-1].lower()
        for ext in (".exe", ".cmd", ".bat"):
            if base.endswith(ext):
                base = base[:-len(ext)]
        names.add(base)
    return names


def base_names(text: str) -> set:
    return {re.split(r"[\\/]", t)[-1] for t in (text, clean(text))}


def is_interpreter(names: set) -> bool:
    return bool(names & (SHELLS | POWERSHELLS | OTHER_INTERPS | {"cmd"})
                or any(PYTHON.match(n) for n in names))


def runs_script(names: set, args: list, script: str) -> bool:
    """The program actually run is `script`: either directly, or as an argument of an
    interpreter. A word that is merely text (echo, grep, a comment) never matches."""
    if script in names or script.lower() in names:
        return True
    return is_interpreter(names) and any(script in base_names(a.text) for a in args)


# ------------------------------------------------------------------------------ git helpers
def git_run(args: list, cwd: str) -> "subprocess.CompletedProcess | None":
    try:
        return subprocess.run(["git", "-C", cwd, *args], capture_output=True, timeout=15)
    except (OSError, subprocess.SubprocessError):
        return None


def rev_parse(target: str, cwd: str) -> "str | None":
    r = git_run(["rev-parse", "--verify", "--quiet", "--end-of-options", f"{target}^{{commit}}"],
                cwd)
    if r is None or r.returncode != 0:
        return None
    return r.stdout.decode("ascii", "replace").strip() or None


def check_target(target: Word, cwd: str, what: str, F: Findings) -> None:
    """Ask unless `target` is an ancestor of HEAD or a local branch (or just a path)."""
    t = clean(target.text)
    if target.dyn or "$" in t or "`" in t:
        F.add(ASK, f"{what}: the target is computed, so it cannot be checked")
        return
    commit = rev_parse(t, cwd)
    if commit is None:
        if os.path.exists(os.path.join(cwd, t)):
            return                                   # `git checkout file`: a path, not a ref
        F.add(ASK, f"{what}: `{t}` is not a known commit")
        return
    r = git_run(["merge-base", "--is-ancestor", commit, "HEAD"], cwd)
    if r is not None and r.returncode == 0:
        return
    r = git_run(["show-ref", "--verify", "--quiet", f"refs/heads/{t}"], cwd)
    if r is not None and r.returncode == 0:
        return
    F.add(ASK, f"{what}: `{t}` is neither an ancestor of HEAD nor a local branch")


def split_args(texts: list, value_opts: set) -> tuple:
    """(positionals before --, words after --, options seen, option values)."""
    pos, after, opts, values = [], [], [], {}
    i, dd = 0, False
    while i < len(texts):
        t = texts[i]
        if dd:
            after.append(t)
        elif t == "--":
            dd = True
        elif t.startswith("-") and t != "-":
            key, eq, val = t.partition("=")
            opts.append(key)
            if eq:
                values[key] = val
            elif t in value_opts and i + 1 < len(texts):
                values[t] = texts[i + 1]
                i += 1
        else:
            pos.append(t)
        i += 1
    return pos, after, opts, values


def analyze_git(words: list, depth: int, F: Findings) -> None:
    n = len(words)
    i, cwd = 1, ROOT
    while i < n:
        t = words[i].text
        if t == "-C" and i + 1 < n:
            cwd = os.path.join(cwd, clean(words[i + 1].text))
            i += 2
        elif t in ("-c", "--config-env") and i + 1 < n:
            check_config(words[i + 1].text, F)
            i += 2
        elif t.startswith("--config-env=") or (t.startswith("-c") and len(t) > 2):
            check_config(t.split("=", 1)[-1] if t.startswith("--") else t[2:], F)
            i += 1
        elif t in GLOBAL_VALUE_OPTS and i + 1 < n:
            i += 2
        elif t.startswith("-"):
            i += 1
        else:
            break
    if i >= n:
        return
    if words[i].dyn:
        F.add(ASK, "the git subcommand is computed, so it cannot be checked")
        return
    sub = clean(words[i].text)
    rest = words[i + 1:]
    texts = [clean(w.text) for w in rest]
    lower = [t.lower() for t in texts]

    if sub == "config":
        if any("hookspath" in t for t in lower):
            F.add(DENY, "git config core.hooksPath would switch off the pre-merge-commit hook")
        if any(t.startswith("alias.") for t in lower):
            F.add(ASK, "a git alias can hide a merge")
        return
    foreign_ref = [t for t in texts if FOREIGN_REF.search(t)]
    foreign_url = [t for t in texts if FOREIGN_URL.search(t) or PR_REF.search(t)]
    if sub in FAMILY and (foreign_ref or foreign_url):
        F.add(DENY, f"git {sub} of `{(foreign_ref or foreign_url)[0]}`: outside code goes "
                    "through scripts/dev/merge-pr.py")
        return
    if sub == "fetch" and foreign_ref:
        F.add(DENY, f"git fetch into/from `{foreign_ref[0]}`")
        return
    if sub in REF_MOVERS and foreign_ref:
        F.add(DENY, f"git {sub} with `{foreign_ref[0]}` would put fetched code in the tree")
        return

    if sub in ("merge", "pull") and "--no-verify" in texts:
        F.add(DENY, f"git {sub} --no-verify skips the pre-merge-commit hook")
        return
    if sub in ("commit", "am") and ("--no-verify" in texts or (sub == "commit" and "-n" in texts)):
        F.add(ASK, "--no-verify skips the pre-merge-commit hook")
    if sub in ("merge", "rebase", "cherry-pick", "am", "revert") and RECOVERY_FLAGS & set(texts):
        return
    if sub == "apply" and not {"--apply", "--index", "--cached"} & set(texts) \
            and {"--check", "--stat", "--numstat", "--summary"} & set(texts):
        return                                       # read-only look at a patch
    if sub in ASK_ALWAYS:
        F.add(ASK, f"git {sub} brings commits into this tree")
        return

    dyn_texts = {clean(w.text) for w in rest if w.dyn}
    if sub in ("reset", "checkout", "switch"):
        # The first positional is the commit-ish (for checkout -b / switch -c it is the start
        # point: the new name was consumed as an option value). `--` leaves only paths.
        pos, after, opts, values = split_args(texts, VALUE_OPTS[sub])
        if pos:
            check_target(Word(pos[0], pos[0] in dyn_texts), cwd, f"git {sub}", F)
    elif sub == "restore":
        pos, after, opts, values = split_args(texts, VALUE_OPTS["restore"])
        source = values.get("--source") or values.get("-s")
        if source:
            check_target(Word(source, any(w.dyn for w in rest)), cwd, "git restore --source", F)
    elif sub == "worktree" and texts[:1] == ["add"]:
        pos, after, opts, values = split_args(texts[1:], VALUE_OPTS["worktree"])
        if len(pos) > 1:
            check_target(Word(pos[1], False), cwd, "git worktree add", F)
    elif sub == "branch":
        pos, after, opts, values = split_args(texts, VALUE_OPTS["branch"])
        if len(pos) > 1 and not BRANCH_MODES & set(opts):
            check_target(Word(pos[1], False), cwd, "git branch", F)


def check_config(value: str, F: Findings) -> None:
    v = clean(value).lower()
    if "hookspath" in v:
        F.add(DENY, "core.hooksPath on the command line would skip the pre-merge-commit hook")
    elif v.startswith("alias."):
        F.add(ASK, "a git alias can hide a merge")


def analyze_gh(texts: list, F: Findings) -> None:
    rest, i = [], 0
    while i < len(texts):
        if texts[i] in ("-R", "--repo"):
            i += 2
            continue
        if not texts[i].startswith("-"):
            rest.append(texts[i].lower())
        i += 1
    joined = " ".join(t.lower() for t in texts)
    if rest[:2] in (["pr", "merge"], ["pr", "checkout"], ["pr", "co"], ["repo", "sync"]):
        F.add(DENY, f"gh {' '.join(rest[:2])}: outside code goes through scripts/dev/merge-pr.py")
    elif rest[:2] == ["workflow", "enable"]:
        if "fork-sync" in joined or "fork_sync" in joined:
            F.add(DENY, "the fork-sync workflow stays disabled: sync is manual and reviewed")
        else:
            F.add(ASK, "enabling a workflow")
    elif rest[:1] == ["api"] and re.search(r"workflows/[^ ]*/enable", joined):
        F.add(DENY, "enabling a workflow through the API")


# ------------------------------------------------------------------------------ commands
def loose(text: str) -> "str | None":
    m = LOOSE.search(text)
    return m.group(0)[:60] if m else None


def analyze_cmd(cmd: Cmd, depth: int, F: Findings, fulltext: str) -> None:
    words = list(cmd.words)
    while words:
        t = words[0].text
        if ASSIGN.match(t):
            if "hookspath" in t.lower():
                F.add(DENY, "core.hooksPath in the environment would skip the hook")
            words.pop(0)
        elif t in LEAD_KEYWORDS:
            words.pop(0)
        elif t in SKIP_CMDS:
            return
        elif not words[0].dyn and exe_names(t) & WRAPPERS:
            words.pop(0)
            while words and (words[0].text.startswith("-") or ASSIGN.match(words[0].text)):
                o = words.pop(0).text
                if "hookspath" in o.lower():
                    F.add(DENY, "core.hooksPath in the environment would skip the hook")
                if o in WRAPPER_VALUE_OPTS and words:
                    words.pop(0)
        else:
            break
    if not words:
        return
    prog, args = words[0], words[1:]
    names = exe_names(prog.text)
    texts = [clean(a.text) for a in args]

    if prog.dyn and any(DANGER_WORD.match(t) for t in texts):
        F.add(ASK, "the program is computed and the arguments look like a merge")
    if runs_script(names, args, "merge-pr.py") and "--dry-run" not in texts:
        F.add(ASK, "scripts/dev/merge-pr.py merges outside code into this tree")
    if runs_script(names, args, "sync-upstream.sh") and not {"--dry-run", "-h", "--help"} & set(texts):
        F.add(DENY, "sync-upstream.sh merges upstream unreviewed; use merge-pr.py --sync "
                    "after a review (it accepts only --dry-run here)")

    if "git" in names:
        analyze_git(words, depth, F)
    elif "gh" in names:
        analyze_gh(texts, F)
    elif names & SHELLS:
        analyze_shell(args, cmd, depth, F, fulltext)
    elif names & POWERSHELLS:
        analyze_powershell(args, cmd, depth, F, fulltext)
    elif "cmd" in names:
        for k, t in enumerate(texts):
            if t.lower() in ("/c", "/k", "/r"):
                analyze(" ".join(texts[k + 1:]), depth + 1, F)
                break
    elif "eval" in names:
        analyze(" ".join(texts), depth + 1, F)
    elif "find" in names:
        for k, t in enumerate(texts):
            if t in ("-exec", "-execdir", "-ok", "-okdir"):
                inner = Cmd()
                for w in args[k + 1:]:
                    if w.text in (";", "+"):
                        break
                    inner.words.append(w)
                analyze_cmd(inner, depth + 1, F, fulltext)
    elif any(PYTHON.match(n) for n in names) or names & OTHER_INTERPS:
        code = list(cmd.stdin)                        # a script fed on stdin
        if {"-c", "-e", "-E", "-p", "--eval", "--print"} & set(texts):
            code.append(" ".join(texts))
        hit = next(filter(None, (loose(c) for c in code)), None)
        if hit:
            F.add(ASK, f"inline code mentions `{hit}`, which cannot be checked")


def stdin_shell(cmd: Cmd, depth: int, F: Findings, fulltext: str) -> None:
    for body in cmd.stdin:
        analyze(body, depth + 1, F)
    if cmd.piped_in:
        hit = loose(fulltext)
        if hit:
            F.add(ASK, f"a shell reads piped text that mentions `{hit}`")


def analyze_shell(args: list, cmd: Cmd, depth: int, F: Findings, fulltext: str) -> None:
    texts = [a.text for a in args]
    for k, t in enumerate(texts):
        if re.fullmatch(r"-[A-Za-z]*c[A-Za-z]*", t):
            if k + 1 < len(texts):
                analyze(texts[k + 1], depth + 1, F)
            return
    if "-s" in texts or not any(not t.startswith("-") for t in texts):
        stdin_shell(cmd, depth, F, fulltext)


def analyze_powershell(args: list, cmd: Cmd, depth: int, F: Findings, fulltext: str) -> None:
    texts = [a.text for a in args]
    for k, t in enumerate(texts):
        tl = t.lower()
        if len(tl) < 2 or not tl.startswith("-"):
            continue
        if "-encodedcommand".startswith(tl) or tl == "-ec":
            data = texts[k + 1] if k + 1 < len(texts) else ""
            try:
                decoded = base64.b64decode(data, validate=True).decode("utf-16-le")
            except (ValueError, UnicodeDecodeError):
                F.add(ASK, "an encoded PowerShell command that cannot be decoded")
                return
            analyze(decoded, depth + 1, F)
            return
        if "-command".startswith(tl):
            analyze(" ".join(texts[k + 1:]), depth + 1, F)
            return
        if "-file".startswith(tl) and len(tl) > 2:
            return
    if not any(not t.startswith("-") for t in texts):
        stdin_shell(cmd, depth, F, fulltext)


def analyze(text: str, depth: int, F: Findings) -> None:
    if depth > MAX_DEPTH:
        F.add(ASK, "commands nested too deeply to check")
        return
    out: list = []
    try:
        Lexer(text, out).parse(None)
    except LexError:
        hit = loose(text)
        if hit:
            F.add(ASK, f"text that cannot be parsed mentions `{hit}`")
    for cmd in out:
        analyze_cmd(cmd, depth, F, text)


def decide(command: str) -> tuple:
    F = Findings()
    analyze(command, 0, F)
    return F.result()


def decide_event(raw: str) -> tuple:
    """(level, reason) for the hook's stdin text. Anything unexpected is a deny."""
    try:
        try:
            event = json.loads(raw)
        except ValueError:
            return DENY, "the hook input is not JSON, so the command cannot be checked"
        if not isinstance(event, dict) or not isinstance(event.get("tool_name"), str):
            return DENY, "the hook input has no tool_name"
        if event["tool_name"] not in ("Bash", "PowerShell"):
            return ALLOW, ""
        tool_input = event.get("tool_input")
        command = tool_input.get("command") if isinstance(tool_input, dict) else None
        if not isinstance(command, str):
            return DENY, "the hook input has no command text"
        if not command.strip():
            return ALLOW, ""
        return decide(command)
    except BaseException as error:           # noqa: BLE001 - fail closed on anything
        return DENY, f"the guard itself failed ({type(error).__name__}: {error})"


def main() -> int:
    try:
        raw = sys.stdin.buffer.read().decode("utf-8")
    except (OSError, UnicodeDecodeError):
        raw = "\x00"                          # not JSON: decide_event denies
    level, why = decide_event(raw)
    if level == ALLOW:
        return 0
    if level == ASK:
        print(json.dumps({"hookSpecificOutput": {
            "hookEventName": "PreToolUse", "permissionDecision": "ask",
            "permissionDecisionReason": f"merge-guard: {why}. Check the command, then approve "
                                        "only if you mean it."}}))
        return 0
    print(f"BLOCKED by scripts/dev/merge-guard.py: {why}.\n\n"
          "Outside code (a pull request, an upstream sync) enters this fork only through\n"
          "scripts/dev/merge-pr.py, after a full-diff review recorded with pr-gate.py and the\n"
          "user's approval. Do not work around this guard. Reviewing is allowed: git log/diff/\n"
          "show of FETCH_HEAD, pr-gate.py scan, merge-pr.py --dry-run.", file=sys.stderr)
    return 2


if __name__ == "__main__":
    try:
        sys.exit(main())
    except SystemExit:
        raise
    except BaseException as error:           # noqa: BLE001 - deliberate catch-all, fail closed
        print(f"BLOCKED by scripts/dev/merge-guard.py: the guard itself failed "
              f"({type(error).__name__}: {error}); blocking rather than guessing.",
              file=sys.stderr)
        sys.exit(2)
