#!/usr/bin/env bash
# Mirror upstream main into local main (fast-forward only), push it to origin, and merge it
# into the work branch. Usage: sync-upstream.sh [--branch <name>] [--no-push] [--push-work] [--dry-run] [-h|--help]
# Env: XCL_UPSTREAM_URL (default https://github.com/ivall/xcl.git), XCL_UPSTREAM_BRANCH (default main).
set -euo pipefail

UPSTREAM_URL="${XCL_UPSTREAM_URL:-https://github.com/ivall/xcl.git}"
UPSTREAM_BRANCH="${XCL_UPSTREAM_BRANCH:-main}"
MIRROR=main

if [ -t 1 ]; then
  C_RED=$'\033[31m'; C_GRN=$'\033[32m'; C_YEL=$'\033[33m'; C_BLD=$'\033[1m'; C_OFF=$'\033[0m'
else
  C_RED=; C_GRN=; C_YEL=; C_BLD=; C_OFF=
fi
info() { printf '%s==>%s %s\n' "$C_BLD" "$C_OFF" "$*"; }
warn() { printf '%swarning:%s %s\n' "$C_YEL" "$C_OFF" "$*" >&2; }
die()  { printf '%serror:%s %s\n' "$C_RED" "$C_OFF" "$*" >&2; exit 1; }

usage() {
  cat <<USAGE
Usage: $(basename "$0") [--branch <name>] [--no-push] [--push-work] [--dry-run] [-h|--help]

Fast-forwards local '$MIRROR' to upstream/$UPSTREAM_BRANCH, pushes it to origin, and merges it
into the work branch (--branch, else the current branch, else 'dev' when on $MIRROR).

  --branch <name>  work branch to merge $MIRROR into
  --no-push        do not push $MIRROR to origin
  --push-work      after a successful merge, push the work branch to origin (no force)
  --dry-run        fetch, then print what would happen; change no refs, merge or push
Env: XCL_UPSTREAM_URL, XCL_UPSTREAM_BRANCH
USAGE
}

WORK=""; PUSH=1; PUSH_WORK=0; DRY=0
while [ $# -gt 0 ]; do
  case "$1" in
    --branch) [ $# -ge 2 ] || die "--branch needs a name"; WORK="$2"; shift 2 ;;
    --no-push) PUSH=0; shift ;;
    --push-work) PUSH_WORK=1; shift ;;
    --dry-run) DRY=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; die "unknown argument: $1" ;;
  esac
done

TOP=$(git rev-parse --show-toplevel 2>/dev/null) || die "not inside a git repository"
cd "$TOP"

# 1. Clean tree, no operation in progress.
for f in MERGE_HEAD CHERRY_PICK_HEAD REVERT_HEAD rebase-merge rebase-apply; do
  if [ -e "$(git rev-parse --git-path "$f")" ]; then
    die "a merge/rebase/cherry-pick is in progress ($f). Finish or abort it first."
  fi
done
if [ -n "$(git status --porcelain)" ]; then
  git status --short >&2
  die "the working tree has uncommitted or untracked changes. Commit, stash or remove them first."
fi

CURRENT=$(git symbolic-ref --short -q HEAD || true)
exists() { git rev-parse --verify --quiet "refs/heads/$1" >/dev/null; }

# 2. Remote 'upstream'.
REMOTE_OK=1
if OLD_URL=$(git remote get-url upstream 2>/dev/null); then
  if [ "$OLD_URL" != "$UPSTREAM_URL" ]; then
    if [ "$DRY" = 1 ]; then
      info "[dry-run] would change upstream URL: $OLD_URL -> $UPSTREAM_URL"; REMOTE_OK=0
    else
      info "upstream URL: $OLD_URL -> $UPSTREAM_URL"
      git remote set-url upstream "$UPSTREAM_URL"
    fi
  fi
else
  if [ "$DRY" = 1 ]; then
    info "[dry-run] would add remote upstream = $UPSTREAM_URL"; REMOTE_OK=0
  else
    info "adding remote upstream = $UPSTREAM_URL"
    git remote add upstream "$UPSTREAM_URL"
  fi
fi

# 3. Fetch.
HAS_ORIGIN=0
git remote get-url origin >/dev/null 2>&1 && HAS_ORIGIN=1
if [ "$REMOTE_OK" = 0 ]; then
  info "[dry-run] cannot compare without the upstream remote configured; stopping here."
  exit 0
fi
info "fetching upstream"
git fetch upstream --prune
if [ "$HAS_ORIGIN" = 1 ]; then
  info "fetching origin"
  git fetch origin --prune
else
  warn "no 'origin' remote: skipping origin fetch and push"
fi
git rev-parse --verify --quiet "refs/remotes/upstream/$UPSTREAM_BRANCH" >/dev/null \
  || die "upstream/$UPSTREAM_BRANCH not found after fetch"
UP_REF="upstream/$UPSTREAM_BRANCH"
UP_SHA=$(git rev-parse "$UP_REF")

# 4. Fast-forward local main.
OLD_MAIN=""
if exists "$MIRROR"; then
  OLD_MAIN=$(git rev-parse "refs/heads/$MIRROR")
  if [ "$OLD_MAIN" != "$UP_SHA" ] && ! git merge-base --is-ancestor "$OLD_MAIN" "$UP_SHA"; then
    echo "Local '$MIRROR' has commits that are not in $UP_REF:" >&2
    git log --oneline "$UP_REF..$MIRROR" >&2
    cat >&2 <<MSG

'$MIRROR' must stay a pure mirror of $UP_REF, so nothing was changed. To fix it, move those
commits to a branch first, then reset the mirror yourself (this script never forces):
  git branch rescue-main $MIRROR
  git checkout $MIRROR && git reset --hard $UP_REF     # deliberate, manual step
MSG
    exit 1
  fi
  if [ "$OLD_MAIN" = "$UP_SHA" ]; then
    info "$MIRROR already at $UP_REF ($(git rev-parse --short "$UP_SHA"))"
  elif [ "$DRY" = 1 ]; then
    info "[dry-run] would fast-forward $MIRROR to $UP_REF"
  elif [ "$CURRENT" = "$MIRROR" ]; then
    info "fast-forwarding $MIRROR (checked out)"
    git merge --ff-only "$UP_REF"
  else
    info "fast-forwarding $MIRROR"
    git fetch . "$UP_REF:$MIRROR"
  fi
else
  if [ "$DRY" = 1 ]; then
    info "[dry-run] would create $MIRROR from $UP_REF"
  else
    info "creating $MIRROR from $UP_REF"
    git branch "$MIRROR" "$UP_REF"
  fi
fi
if [ "$DRY" = 1 ]; then NEW_MAIN="$UP_SHA"; else NEW_MAIN=$(git rev-parse "refs/heads/$MIRROR"); fi

# 5. Push main.
PUSHED="no"
if [ "$PUSH" = 0 ]; then
  info "--no-push: not pushing $MIRROR"; PUSHED="no (--no-push)"
elif [ "$HAS_ORIGIN" = 0 ]; then
  PUSHED="no (no origin)"
elif [ "$DRY" = 1 ]; then
  info "[dry-run] would run: git push origin $MIRROR"; PUSHED="no (dry-run)"
else
  info "pushing $MIRROR to origin"
  git push origin "$MIRROR" || die "push of $MIRROR to origin was rejected (no force is used)."
  PUSHED="yes"
fi

# 6. Merge main into the work branch.
if [ -z "$WORK" ]; then
  if [ -n "$CURRENT" ] && [ "$CURRENT" != "$MIRROR" ]; then
    WORK="$CURRENT"
  elif exists dev; then
    WORK=dev
  fi
fi
MERGED="skipped"; WORK_PUSHED="no"
if [ -z "$WORK" ]; then
  info "no work branch (on $MIRROR and no 'dev' branch): skipping the merge. Use --branch <name>."
elif [ "$WORK" = "$MIRROR" ]; then
  info "work branch is $MIRROR: nothing to merge"
else
  exists "$WORK" || die "work branch '$WORK' does not exist"
  if [ "$DRY" = 1 ]; then
    N=$(git rev-list --count "$WORK..$NEW_MAIN")
    info "[dry-run] would check out $WORK (if needed) and merge $MIRROR ($N new commits)"
    MERGED="no (dry-run)"
    if [ "$PUSH_WORK" = 1 ] && [ "$HAS_ORIGIN" = 1 ]; then
      info "[dry-run] would run: git push origin $WORK"; WORK_PUSHED="no (dry-run)"
    fi
  else
    if [ "$CURRENT" != "$WORK" ]; then
      info "checking out $WORK"
      git checkout "$WORK"
    fi
    info "merging $MIRROR into $WORK"
    if ! git merge --no-edit "$MIRROR"; then
      FILES=$(git diff --name-only --diff-filter=U)
      [ -n "$FILES" ] || die "merge failed without conflicts; see the output above."
      {
        printf '\n%sMerge conflict%s in:\n' "$C_RED" "$C_OFF"
        printf '%s\n' "$FILES" | sed 's/^/  /'
        cat <<MSG

Resolve the files, then:
  git add <files>
  git merge --continue
or give up with:
  git merge --abort
MSG
      } >&2
      exit 2
    fi
    MERGED="yes"
    if [ "$PUSH_WORK" = 1 ]; then
      if [ "$HAS_ORIGIN" = 0 ]; then
        WORK_PUSHED="no (no origin)"
      else
        info "pushing $WORK to origin"
        git push origin "$WORK" || die "push of $WORK to origin was rejected (no force is used)."
        WORK_PUSHED="yes"
      fi
    fi
  fi
fi

# 7. Summary.
echo
info "Summary"
if [ -n "$OLD_MAIN" ]; then RANGE="$OLD_MAIN..$NEW_MAIN"; else RANGE="$NEW_MAIN"; fi
COUNT=$(git rev-list --count "$RANGE")
printf 'Commits pulled into %s: %s\n' "$MIRROR" "$COUNT"
[ "$COUNT" -eq 0 ] || git log --oneline -n 20 "$RANGE"
[ "$COUNT" -le 20 ] || printf '  ... and %s more\n' "$((COUNT - 20))"
printf '%s HEAD: %s\n' "$MIRROR" "$(git rev-parse --short "$NEW_MAIN")"
if [ -n "$WORK" ] && [ "$WORK" != "$MIRROR" ] && exists "$WORK"; then
  printf '%s HEAD: %s (merged: %s)\n' "$WORK" "$(git rev-parse --short "refs/heads/$WORK")" "$MERGED"
  if [ "$PUSH_WORK" = 1 ]; then
    printf '%s pushed: %s\n' "$WORK" "$WORK_PUSHED"
  elif [ "$DRY" = 0 ] && [ "$HAS_ORIGIN" = 1 ]; then
    echo "$WORK is not pushed. To push it: git push origin $WORK"
  fi
fi
printf 'main pushed: %s\n' "$PUSHED"
[ "$DRY" = 1 ] && info "dry run: nothing was changed"
[ "$DRY" = 1 ] || { CUR=$(git symbolic-ref --short -q HEAD || true); echo "You are on: ${CUR:-detached HEAD}"; }
exit 0
