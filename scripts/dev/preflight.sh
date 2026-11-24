#!/usr/bin/env bash
# XCL preflight: read-only status of the fork against ivall/xcl. Always exits 0.
# Fetches upstream/origin (quiet, 15 s guard); never adds remotes or touches branches.
# Env: XCL_UPSTREAM_URL (default https://github.com/ivall/xcl.git), XCL_FORK_REPO (default Osanchez/xcl).
trap 'exit 0' EXIT ERR
set +e
UPSTREAM_URL="${XCL_UPSTREAM_URL:-https://github.com/ivall/xcl.git}"
FORK="${XCL_FORK_REPO:-Osanchez/xcl}"
cd "$(dirname "$0")/../.." 2>/dev/null || exit 0
echo "XCL preflight"
git rev-parse --git-dir >/dev/null 2>&1 || { echo "not a git repository"; exit 0; }

# guarded: run a command with a 15 s limit
guard() {
  if command -v timeout >/dev/null 2>&1; then timeout 15 "$@"; return $?
  elif command -v gtimeout >/dev/null 2>&1; then gtimeout 15 "$@"; return $?; fi
  "$@" & local pid=$!
  ( sleep 15; kill "$pid" 2>/dev/null ) >/dev/null 2>&1 & local w=$!
  wait "$pid" 2>/dev/null; local rc=$?
  kill "$w" 2>/dev/null; wait "$w" 2>/dev/null
  return $rc
}

export GIT_TERMINAL_PROMPT=0
branch=$(git symbolic-ref --short -q HEAD || echo "(detached)")
dirty=clean; [ -n "$(git status --porcelain --untracked-files=no 2>/dev/null)" ] && dirty=DIRTY
echo "branch: $branch ($dirty)"

online=1
HAS_UP=0; git remote get-url upstream >/dev/null 2>&1 && HAS_UP=1
if [ "$HAS_UP" = 1 ]; then
  guard git fetch -q upstream >/dev/null 2>&1 || online=0
  UPREF=upstream/main
else
  if guard git fetch -q "$UPSTREAM_URL" main >/dev/null 2>&1; then UPREF=FETCH_HEAD; else online=0; UPREF=; fi
  echo "no 'upstream' remote: run scripts/dev/sync-upstream.sh once to add it"
fi
git remote get-url origin >/dev/null 2>&1 && { guard git fetch -q origin >/dev/null 2>&1 || online=0; }
[ "$online" = 0 ] && echo "offline or fetch failed: showing last known state"

if [ "$HAS_UP" = 1 ] && ! git rev-parse -q --verify "$UPREF" >/dev/null 2>&1; then UPREF=; fi
if [ -n "$UPREF" ]; then
  # FETCH_HEAD is only valid right after the fetch above; resolve it now
  UPSHA=$(git rev-parse -q --verify "$UPREF" 2>/dev/null)
  for b in main dev; do
    if git rev-parse -q --verify "refs/heads/$b" >/dev/null 2>&1; then
      n=$(git rev-list --count "$b..$UPSHA" 2>/dev/null)
      echo "$b: ${n:-?} commit(s) behind upstream/main"
    fi
  done
  echo "newest upstream commits:"
  git log -5 --format='  %h %s' "$UPSHA" 2>/dev/null | cut -c1-90
fi

if git rev-parse -q --verify refs/remotes/origin/dev >/dev/null 2>&1 && git rev-parse -q --verify refs/heads/dev >/dev/null 2>&1; then
  a=$(git rev-list --count dev..origin/dev 2>/dev/null)
  if [ "${a:-0}" -gt 0 ]; then echo "origin/dev is $a commit(s) ahead of dev (fork-sync merged): git pull --ff-only on dev"
  else echo "origin/dev: not ahead of dev"; fi
fi

if command -v gh >/dev/null 2>&1 && [ "$online" = 1 ]; then
  run=$(guard gh run list -R "$FORK" --workflow fork-sync.yml -L 1 --json conclusion,createdAt,url \
    --jq '.[0] | "\(.conclusion) \(.createdAt) \(.url)"' 2>/dev/null)
  if [ -n "$run" ] && [ "$run" != "null" ]; then echo "last fork-sync run: $run"; fi
  iss=$(guard gh issue list -R "$FORK" --label sync-conflict --state open --json number,title \
    --jq '.[] | "#\(.number) \(.title)"' 2>/dev/null)
  if [ -n "$iss" ]; then echo "OPEN sync-conflict issue(s):"; echo "$iss" | head -3 | sed 's/^/  /'; fi
fi

echo "before a task: scripts/dev/sync-upstream.sh (clean tree), then"
echo "  scripts/dev/pr-scan.py covers <keywords>; gh pr list -R ivall/xcl --search \"<kw>\" --state all"
exit 0
