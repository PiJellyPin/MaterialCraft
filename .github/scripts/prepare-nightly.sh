#!/usr/bin/env bash
set -euo pipefail
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
echo "version=$version" >> "$GITHUB_OUTPUT"

requested=${REQUESTED_VERSION:-}
if [ -n "$requested" ]; then
    if [[ ! "$requested" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
        echo "::error::Use a version like 0.4.0, without a v prefix or prerelease suffix."
        exit 1
    fi
    if [ "$GITHUB_EVENT_NAME" != workflow_dispatch ] ||
       { [ "$GITHUB_REF" != "refs/heads/$DEFAULT_BRANCH" ] && [ "$GITHUB_REF" != "refs/tags/$requested" ]; }; then
        echo "::error::Start versioned releases from $DEFAULT_BRANCH, or rerun their existing tag."
        exit 1
    fi
    release_state=$(gh api "repos/$GITHUB_REPOSITORY/releases" --paginate \
        --jq ".[] | select(.tag_name == \"$requested\") | if .draft then \"draft\" else \"published\" end")
    if [ "$release_state" = published ]; then
        echo "::error::Release $requested is already published; choose a new version."
        exit 1
    fi

    if [ "$GITHUB_REF" = "refs/tags/$requested" ]; then
        if [ "$version" != "$requested" ]; then
            echo "::error::Tag $requested does not match Cargo.toml version $version."
            exit 1
        fi
        echo "changed=true" >> "$GITHUB_OUTPUT"
        exit 0
    fi

    tag_exists=$(gh api "repos/$GITHUB_REPOSITORY/git/matching-refs/tags/$requested" \
        --jq "[.[] | select(.ref == \"refs/tags/$requested\")] | length")
    if [ "$tag_exists" -ne 0 ]; then
        echo "::error::Tag $requested already exists. To retry its build, run: gh workflow run nightly.yml --ref $requested -f version=$requested"
        exit 1
    fi

    python3 .github/scripts/set-version.py "$requested"
    git config user.name "github-actions[bot]"
    git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
    git add Cargo.toml Cargo.lock
    if ! git diff --cached --quiet; then
        git commit -m "Release $requested"
    fi
    git tag "$requested"
    # Push both refs together; a concurrent branch update must fail without moving either.
    git push --atomic origin "HEAD:refs/heads/$DEFAULT_BRANCH" "refs/tags/$requested"

    # Start the build at the new tag so provenance identifies the actual version commit.
    if ! gh workflow run nightly.yml --repo "$GITHUB_REPOSITORY" --ref "$requested" -f "version=$requested"; then
        echo "::error::Version/tag saved, but dispatch failed. Retry: gh workflow run nightly.yml --ref $requested -f version=$requested"
        exit 1
    fi
    echo "changed=false" >> "$GITHUB_OUTPUT"
    echo "Created $requested; its tagged build is queued in Actions." >> "$GITHUB_STEP_SUMMARY"
    exit 0
fi

if [ "$GITHUB_EVENT_NAME" = workflow_dispatch ]; then
    echo "changed=true" >> "$GITHUB_OUTPUT"
    echo "Manual rebuild of version $version." >> "$GITHUB_STEP_SUMMARY"
    exit 0
fi

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
