#!/usr/bin/env bash
set -euo pipefail
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
echo "version=$version" >> "$GITHUB_OUTPUT"

# Compare successful runs, not the moving tag: a failed upload must be retried.
previous=$(gh api --method GET "repos/$GITHUB_REPOSITORY/actions/workflows/nightly.yml/runs" \
    -f branch="$GITHUB_REF_NAME" -f event=schedule -f status=success -f per_page=1 \
    --jq '.workflow_runs[0].head_sha // ""')
if [ "$previous" = "$GITHUB_SHA" ]; then
    echo "changed=false" >> "$GITHUB_OUTPUT"
    echo "This commit already has a successful nightly; skipping." >> "$GITHUB_STEP_SUMMARY"
else
    echo "changed=true" >> "$GITHUB_OUTPUT"
fi
