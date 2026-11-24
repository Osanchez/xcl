#!/usr/bin/env bash
# Mirror ivall/xcl (branches, tags, every PR head) and the fork's main/dev into the archive repo.
# Never deletes anything. A ref whose new tip does not descend from the archived tip first has the
# old tip preserved under rewritten/<STAMP>/... See README.md.
set -euo pipefail

UPSTREAM_URL="${UPSTREAM_URL:-https://github.com/ivall/xcl.git}"
FORK_URL="${FORK_URL:-https://github.com/Osanchez/xcl.git}"
ARCHIVE_URL="${ARCHIVE_URL:?ARCHIVE_URL is required}"
BUNDLE="${BUNDLE:-0}"
BUNDLE_DIR="${BUNDLE_DIR:-.}"
STAMP="${STAMP:-$(date -u +%Y%m%d-%H%M)}"
BATCH=200

export GIT_TERMINAL_PROMPT=0
export ARCHIVE_URL

# Mask the push URL (and its credentials) in anything printed.
if [ -n "${GITHUB_ACTIONS:-}" ]; then echo "::add-mask::$ARCHIVE_URL"; fi
mask() {
  awk 'BEGIN { s = ENVIRON["ARCHIVE_URL"]; u = s; sub(/^[a-z]+:\/\//, "", u); sub(/@.*$/, "", u) }
       { for (p = 0; ; ) { i = index($0, s); if (!i) break; $0 = substr($0, 1, i - 1) "***" substr($0, i + length(s)) }
         if (u != "" && u != s) { while ((i = index($0, u)) > 0) $0 = substr($0, 1, i - 1) "***" substr($0, i + length(u)) }
         print }'
}

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
git init -q --bare "$WORK/r"
cd "$WORK/r"

echo "Fetching the archive's current refs"
if ! git fetch -q --no-tags "$ARCHIVE_URL" '+refs/heads/*:refs/arc/heads/*' '+refs/tags/*:refs/arc/tags/*' 2>&1 | mask; then
  echo "::error::cannot read the archive repo; refusing to push blind"
  exit 1
fi

echo "Fetching upstream $UPSTREAM_URL"
if ! git fetch -q --no-tags "$UPSTREAM_URL" '+refs/heads/*:refs/up/heads/*' '+refs/tags/*:refs/up/tags/*' '+refs/pull/*/head:refs/up/pr/*'; then
  echo "::warning::upstream $UPSTREAM_URL is unreachable or the fetch failed; archive left untouched"
  exit 0
fi

for b in main dev; do
  if ! git fetch -q --no-tags "$FORK_URL" "+refs/heads/$b:refs/up/fork/$b"; then
    echo "::warning::fork branch $b not fetched from $FORK_URL; continuing"
  fi
done

# Map each fetched ref to its archive ref.
git for-each-ref --format='%(objectname) %(refname)' refs/up/ > "$WORK/up.txt"
if [ ! -s "$WORK/up.txt" ]; then
  echo "::warning::upstream returned no refs; archive left untouched"
  exit 0
fi

: > "$WORK/specs.txt"
n_new=0; n_upd=0; n_pres=0; n_same=0
while read -r sha ref; do
  case "$ref" in
    refs/up/heads/*) name="upstream/${ref#refs/up/heads/}"; kind=heads ;;
    refs/up/tags/*)  name="${ref#refs/up/tags/}";           kind=tags ;;
    refs/up/pr/*)    name="pr/${ref#refs/up/pr/}";           kind=heads ;;
    refs/up/fork/*)  name="fork/${ref#refs/up/fork/}";       kind=heads ;;
    *) continue ;;
  esac
  target="refs/$kind/$name"
  old="$(git rev-parse -q --verify "refs/arc/$kind/$name" 2>/dev/null || true)"
  if [ "$BUNDLE" = "1" ] && [ "$kind" = heads ]; then git update-ref "$target" "$sha"; fi
  if [ "$BUNDLE" = "1" ] && [ "$kind" = tags ]; then git update-ref "$target" "$sha"; fi
  if [ -z "$old" ]; then
    echo "+$ref:$target" >> "$WORK/specs.txt"; n_new=$((n_new + 1)); continue
  fi
  if [ "$old" = "$sha" ]; then n_same=$((n_same + 1)); continue; fi
  keep=0
  if [ "$kind" = tags ]; then
    keep=1
  elif ! git merge-base --is-ancestor "$old" "$sha" 2>/dev/null; then
    keep=1
  fi
  if [ "$keep" = 1 ]; then
    echo "refs/arc/$kind/$name:refs/$kind/rewritten/$STAMP/$name" >> "$WORK/specs.txt"
    n_pres=$((n_pres + 1))
  fi
  echo "+$ref:$target" >> "$WORK/specs.txt"; n_upd=$((n_upd + 1))
done < "$WORK/up.txt"

total="$(wc -l < "$WORK/specs.txt" | tr -d ' ')"
if [ "$total" -gt 0 ]; then
  rm -f "$WORK"/chunk.*
  split -l "$BATCH" "$WORK/specs.txt" "$WORK/chunk."
  for c in "$WORK"/chunk.*; do
    specs=()
    while read -r s; do specs+=("$s"); done < "$c"
    echo "Pushing ${#specs[@]} refspecs"
    if ! git push -q --no-verify "$ARCHIVE_URL" "${specs[@]}" 2>&1 | mask; then
      echo "::error::push to the archive failed"
      exit 1
    fi
  done
else
  echo "Nothing to push"
fi

echo "Summary: $n_new new, $n_upd updated, $n_pres preserved-as-rewritten, $n_same unchanged"

if [ "$BUNDLE" = "1" ]; then
  mkdir -p "$BUNDLE_DIR"
  file="$(cd "$BUNDLE_DIR" && pwd)/xcl-${STAMP%%-*}.bundle"
  if git rev-parse -q --verify refs/heads/upstream/main >/dev/null; then
    git symbolic-ref HEAD refs/heads/upstream/main
  elif git rev-parse -q --verify refs/heads/upstream/master >/dev/null; then
    git symbolic-ref HEAD refs/heads/upstream/master
  fi
  git bundle create -q "$file" --branches --tags HEAD
  git bundle verify "$file" >/dev/null
  echo "Bundle: $file"
fi
