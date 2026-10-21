#!/usr/bin/env bash
set -euo pipefail

# Build one application for Apple Silicon and Intel.
mkdir -p "$OUT"
for b in materialcraft materialcraft-gui; do
  lipo -create \
    "target/aarch64-apple-darwin/release/$b" \
    "target/x86_64-apple-darwin/release/$b" \
    -output "$OUT/$b"
  lipo "$OUT/$b" -verify_arch arm64 x86_64
done

name="MaterialCraft_${VERSION}_${LABEL}"
stage="$RUNNER_TEMP/stage/dmg"
app="$stage/MaterialCraft.app"

mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" dist
install -m 0755 "$OUT/materialcraft-gui" "$app/Contents/MacOS/materialcraft-gui"
cp -R "$RUNNER_TEMP/materialcraft-notices/." "$app/Contents/Resources/"

set_="$RUNNER_TEMP/MaterialCraft.iconset"
mkdir -p "$set_"
for s in 16 32 128 256; do
  sips -z $s $s           assets/MaterialCraftLogo.png --out "$set_/icon_${s}x${s}.png"    >/dev/null
  sips -z $((s*2)) $((s*2)) assets/MaterialCraftLogo.png --out "$set_/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$set_" -o "$app/Contents/Resources/MaterialCraft.icns"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>MaterialCraft</string>
  <key>CFBundleDisplayName</key><string>MaterialCraft</string>
  <key>CFBundleExecutable</key><string>materialcraft-gui</string>
  <key>CFBundleIconFile</key><string>MaterialCraft</string>
  <key>CFBundleIdentifier</key><string>org.materialcraft.editor</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
plutil -lint "$app/Contents/Info.plist"

install -m 0755 "$OUT/materialcraft" "$stage/materialcraft"
cp -R "$RUNNER_TEMP/materialcraft-notices/." "$stage/"
cp README.md "$stage/"

codesign --force --sign - "$stage/materialcraft"
codesign --force --deep --sign - "$app"
codesign --verify --deep --strict --verbose=2 "$app"

ln -s /Applications "$stage/Applications"
hdiutil create -volname MaterialCraft -srcfolder "$stage" -ov -format UDZO "dist/$name.dmg"

# Verify the signed application and exercise both CLI slices from the DMG.
mnt="$RUNNER_TEMP/verify-dmg"
hdiutil attach -nobrowse -readonly -mountpoint "$mnt" "dist/$name.dmg"
trap 'hdiutil detach "$mnt" -force >/dev/null || true' EXIT
test -f "$mnt/MaterialCraft.app/Contents/Resources/MaterialCraft.icns"
codesign --verify --deep --strict "$mnt/MaterialCraft.app"
lipo "$mnt/MaterialCraft.app/Contents/MacOS/materialcraft-gui" -verify_arch arm64 x86_64
lipo "$mnt/materialcraft" -verify_arch arm64 x86_64
if ! arch -x86_64 /usr/bin/true 2>/dev/null; then
    sudo softwareupdate --install-rosetta --agree-to-license
fi
for architecture in arm64 x86_64; do
    bash .github/scripts/smoke-cli.sh arch "-$architecture" "$mnt/materialcraft"
done
