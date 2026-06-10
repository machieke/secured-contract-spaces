#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

for tool in git jq sha256sum awk wc; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "required tool missing: $tool" >&2
    exit 1
  fi
done

git_commit="$(git rev-parse HEAD)"
git_tree="$(git rev-parse 'HEAD^{tree}')"
tracked_change_count="$(git status --porcelain=v1 --untracked-files=no | wc -l | tr -d '[:space:]')"
untracked_file_count="$(git ls-files --others --exclude-standard | wc -l | tr -d '[:space:]')"
status_porcelain_sha256="$(
  git status --porcelain=v1 --untracked-files=all | sha256sum | awk '{print $1}'
)"
tracked_diff_sha256="$(
  git diff --binary HEAD -- . | sha256sum | awk '{print $1}'
)"
staged_diff_sha256="$(
  git diff --cached --binary HEAD -- . | sha256sum | awk '{print $1}'
)"
unstaged_diff_sha256="$(
  git diff --binary -- . | sha256sum | awk '{print $1}'
)"

worktree_clean=false
if [ "$tracked_change_count" -eq 0 ] && [ "$untracked_file_count" -eq 0 ]; then
  worktree_clean=true
fi

jq -cn \
  --arg schema "detta.source-state.v1" \
  --arg git_commit "$git_commit" \
  --arg git_tree "$git_tree" \
  --arg status_porcelain_sha256 "$status_porcelain_sha256" \
  --arg tracked_diff_sha256 "$tracked_diff_sha256" \
  --arg staged_diff_sha256 "$staged_diff_sha256" \
  --arg unstaged_diff_sha256 "$unstaged_diff_sha256" \
  --argjson tracked_change_count "$tracked_change_count" \
  --argjson untracked_file_count "$untracked_file_count" \
  --argjson worktree_clean "$worktree_clean" \
  '{
    schema: $schema,
    schema_version: 1,
    git_commit: $git_commit,
    git_tree: $git_tree,
    worktree_clean: $worktree_clean,
    tracked_change_count: $tracked_change_count,
    untracked_file_count: $untracked_file_count,
    status_porcelain_sha256: $status_porcelain_sha256,
    tracked_diff_sha256: $tracked_diff_sha256,
    staged_diff_sha256: $staged_diff_sha256,
    unstaged_diff_sha256: $unstaged_diff_sha256
  }'
