#!/usr/bin/env bash
set -euo pipefail
name="MaterialCraft_${VERSION}"
# Require the complete platform set before touching the published release.
assets=(
    "${name}_x86_64-linux.AppImage"
    "${name}_x86_64-linux.deb"
    "${name}_x86_64-linux.rpm"
    "${name}_x86_64-linux.tar.gz"
    "${name}_x64-windows.zip"
    "${name}_universal-macOS.dmg"
)
for asset in "${assets[@]}"; do
    test -s "dist/$asset"
done
(cd dist && sha256sum -- "${assets[@]}" > SHA256SUMS)
assets+=(SHA256SUMS)

title="Nightly $(date -u +%Y-%m-%d) (${GITHUB_SHA:0:7})"
notes="$RUNNER_TEMP/nightly-notes.md"
cat > "$notes" <<EOF
Automatic nightly build of $GITHUB_SHA from $GITHUB_REF_NAME.

Tests and packaged CLI smoke tests passed on Linux, Windows and macOS.
The Ubuntu package and Linux AppImage also passed GUI launch checks.
SHA256SUMS contains the download checksums.

Verify build provenance with:
    gh attestation verify <file> -R $GITHUB_REPOSITORY
EOF

# Create the first release as a draft so a failed first upload stays unpublished.
release_id=$(gh api "repos/$GITHUB_REPOSITORY/releases" --paginate \
    --jq '.[] | select(.tag_name == "latest") | .id')
if [ -z "$release_id" ]; then
    gh release create latest --repo "$GITHUB_REPOSITORY" --target "$GITHUB_SHA" \
        --draft --prerelease --title "$title" --notes-file "$notes"
fi

upload=()
for asset in "${assets[@]}"; do upload+=("dist/$asset"); done
gh release upload latest --repo "$GITHUB_REPOSITORY" --clobber "${upload[@]}"

# A version bump changes asset names. Remove older files only after uploads finish.
keep=$(printf '%s\n' "${assets[@]}")
existing=$(gh release view latest --repo "$GITHUB_REPOSITORY" --json assets --jq '.assets[].name')
while IFS= read -r asset; do
    [ -n "$asset" ] || continue
    if ! grep -qxF -- "$asset" <<< "$keep"; then
        gh release delete-asset latest "$asset" --repo "$GITHUB_REPOSITORY" --yes
    fi
done <<< "$existing"

# Listing refs returns an empty array for a missing tag; API errors remain fatal.
tag_exists=$(gh api "repos/$GITHUB_REPOSITORY/git/matching-refs/tags/latest" \
    --jq '[.[] | select(.ref == "refs/tags/latest")] | length')
if [ "$tag_exists" -eq 0 ]; then
    gh api --method POST "repos/$GITHUB_REPOSITORY/git/refs" \
        -f ref=refs/tags/latest -f sha="$GITHUB_SHA" >/dev/null
else
    gh api --method PATCH "repos/$GITHUB_REPOSITORY/git/refs/tags/latest" \
        -f sha="$GITHUB_SHA" -F force=true >/dev/null
fi
gh release edit latest --repo "$GITHUB_REPOSITORY" --draft=false --prerelease --latest=false \
    --title "$title" --notes-file "$notes"
