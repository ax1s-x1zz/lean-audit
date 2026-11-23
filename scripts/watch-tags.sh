#!/usr/bin/env bash
#
# Watch a set of repositories for new tags and report only what changed.
#
# This is the operating arm of the standing invitation from the TEAM42
# maintainer ("reports like this one are welcome"): run the cheap checks on each
# new tag, diff against the previous observation, and stay silent when nothing
# changed. It never posts anything; it writes reports to a state directory.
#
# Usage:
#   scripts/watch-tags.sh <repos-dir> <state-dir> [owner] [repo...]
#
#   repos-dir  directory whose immediate subdirectories are git clones
#   state-dir  where the tag markers, JSON reports and drift reports are kept
#   owner      remote owner to query for tags        (default TEAM-42KR)
#   repo...    restrict to these repository names    (default: every clone)
#
# Environment:
#   LEAN_AUDIT   path to the lean-audit binary       (default: lean-audit)
#   AXIOMS       set to 1 to also run `axioms --build` (slow; needs elan)
#
# Exit codes:
#   0   no new tags, or new tags observed with no drift
#   3   drift was found in at least one repository
#
set -euo pipefail

LEAN_AUDIT="${LEAN_AUDIT:-lean-audit}"
AXIOMS="${AXIOMS:-0}"

REPOS_DIR="${1:?usage: watch-tags.sh <repos-dir> <state-dir> [owner] [repo...]}"
STATE_DIR="${2:?usage: watch-tags.sh <repos-dir> <state-dir> [owner] [repo...]}"
OWNER="${3:-TEAM-42KR}"
shift 3 2>/dev/null || shift $#
REPOS=("$@")

mkdir -p "$STATE_DIR"

if [ "${#REPOS[@]}" -eq 0 ]; then
  mapfile -t REPOS < <(find "$REPOS_DIR" -maxdepth 1 -mindepth 1 -type d -printf '%f\n' | sort)
fi

drift_found=0
new_tags=0

for repo in "${REPOS[@]}"; do
  dir="$REPOS_DIR/$repo"
  [ -d "$dir/.git" ] || continue

  tag="$(git -C "$dir" ls-remote --tags --refs "https://github.com/$OWNER/$repo.git" 2>/dev/null \
          | awk -F/ '{print $NF}' | sort -V | tail -1 || true)"
  if [ -z "$tag" ]; then
    continue
  fi

  marker="$STATE_DIR/$repo.tag"
  prev_tag=""
  if [ -f "$marker" ]; then
    prev_tag="$(cat "$marker")"
  fi
  if [ "$tag" = "$prev_tag" ]; then
    continue
  fi

  new_tags=$((new_tags + 1))
  echo "== $repo: ${prev_tag:-<none>} -> $tag"

  git -C "$dir" fetch -q --tags origin
  if ! git -C "$dir" checkout -q "$tag"; then
    echo "   checkout of $tag failed; skipping"
    continue
  fi

  # `inspect` verifies manifest hashes by default, so it covers the manifest
  # drift that motivated the first report. It exits 2 when forbidden tokens are
  # in scope; the report is still written.
  current="$STATE_DIR/$repo.json"
  "$LEAN_AUDIT" inspect "$dir" --format json --out "$current" >/dev/null 2>&1 || true
  if [ "$AXIOMS" = "1" ]; then
    "$LEAN_AUDIT" axioms "$dir" --build --format json \
      --out "$STATE_DIR/$repo.axioms.json" >/dev/null 2>&1 || true
  fi

  previous="$STATE_DIR/$repo.prev.json"
  if [ -f "$previous" ]; then
    if ! "$LEAN_AUDIT" drift "$previous" "$current" --out "$STATE_DIR/$repo.drift.md"; then
      drift_found=1
    fi
    cat "$STATE_DIR/$repo.drift.md"
  else
    echo "(first observation of $repo; nothing to diff yet)"
  fi

  mv "$current" "$previous"
  echo "$tag" > "$marker"
done

if [ "$new_tags" -eq 0 ]; then
  echo "no new tags"
fi

exit $([ "$drift_found" -eq 1 ] && echo 3 || echo 0)
