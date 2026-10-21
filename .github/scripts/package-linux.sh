#!/usr/bin/env bash
# Build all Linux formats from one AppDir. VERSION, LABEL and OUT come from the workflow.
set -euo pipefail
name="MaterialCraft_${VERSION}_${LABEL}"

A=AppDir
rm -rf "$A"
mkdir -p "$A/usr/bin" "$A/usr/share/applications" "$A/usr/share/icons/hicolor/256x256/apps" dist

install -m 0755 "$OUT/materialcraft"     "$A/usr/bin/materialcraft"
install -m 0755 "$OUT/materialcraft-gui" "$A/usr/bin/materialcraft-gui"
install -d "$A/usr/share/doc/materialcraft"
cp -R "$RUNNER_TEMP/materialcraft-notices/." "$A/usr/share/doc/materialcraft/"
install -m 0644 assets/MaterialCraftLogo.png \
  "$A/usr/share/icons/hicolor/256x256/apps/materialcraft.png"

cat > "$A/materialcraft.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=MaterialCraft
Comment=Procedural, node-based material authoring
Exec=materialcraft-gui
Icon=materialcraft
Categories=Graphics;2DGraphics;3DGraphics;
Terminal=false
EOF

install -m 0644 "$A/materialcraft.desktop" "$A/usr/share/applications/materialcraft.desktop"
install -m 0644 assets/MaterialCraftLogo.png "$A/materialcraft.png"

# Bundle optional XKB libraries; graphics drivers must come from the host.
mkdir -p "$A/lib"
for so in libxkbcommon-x11.so.0 libxcb-xkb.so.1; do
  src=$(ldconfig -p | awk -v n="$so" '$1 == n && /x86-64/ && !found {print $NF; found=1}')
  [ -n "$src" ] || { echo "$so not found on the build host"; exit 1; }
  install -m 0644 "$(readlink -f "$src")" "$A/lib/$so"
done

cat > "$A/AppRun" <<'APPRUN'
#!/bin/sh
HERE="${APPDIR:-$(dirname "$(readlink -f "$0")")}"
export LD_LIBRARY_PATH="$HERE/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec "$HERE/usr/bin/materialcraft-gui" "$@"
APPRUN
chmod 0755 "$A/AppRun"

curl -sSfL -o "$RUNNER_TEMP/appimagetool" \
  https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
curl -sSfL -o "$RUNNER_TEMP/runtime-x86_64" \
  https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64
sha256sum --check --strict <<EOF
ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0  $RUNNER_TEMP/appimagetool
2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d  $RUNNER_TEMP/runtime-x86_64
EOF
chmod +x "$RUNNER_TEMP/appimagetool"

APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 "$RUNNER_TEMP/appimagetool" --no-appstream \
  --runtime-file "$RUNNER_TEMP/runtime-x86_64" \
  AppDir "dist/$name.AppImage"

pkg="$RUNNER_TEMP/debmaterialcraft"
rm -rf "$pkg"
mkdir -p "$pkg/DEBIAN"
cp -r AppDir/usr "$pkg/usr"

# dpkg-shlibdeps cannot detect the display libraries loaded with dlopen.
mkdir -p "$RUNNER_TEMP/shlib/debian"
printf 'Source: materialcraft\n\nPackage: materialcraft\nArchitecture: any\n' \
  > "$RUNNER_TEMP/shlib/debian/control"
linked=$(cd "$RUNNER_TEMP/shlib" && dpkg-shlibdeps -O \
    "$pkg/usr/bin/materialcraft" "$pkg/usr/bin/materialcraft-gui" \
  | sed -n 's/^shlibs:Depends=//p')
[ -n "$linked" ] || { echo "dpkg-shlibdeps found no dependencies"; exit 1; }
dlopened="libx11-6, libx11-xcb1, libxcursor1, libxi6, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libwayland-egl1, libegl1, libgl1"
echo "linked:    $linked"
echo "dlopened:  $dlopened"

cat > "$pkg/DEBIAN/control" <<EOF
Package: materialcraft
Version: $VERSION
Section: graphics
Priority: optional
Architecture: $(dpkg --print-architecture)
Maintainer: MaterialCraft <noreply@example.com>
Depends: $linked, $dlopened
Recommends: libgl1-mesa-dri, libvulkan1, xdg-desktop-portal
Description: Procedural, node-based material authoring engine
 MaterialCraft builds PBR materials from a node graph, with a
 headless renderer, a visual editor and an AI control interface.
EOF

dpkg-deb --root-owner-group --build "$pkg" "dist/$name.deb"
dpkg-deb --info "dist/$name.deb"

top="$RUNNER_TEMP/rpmbuild"
mkdir -p "$top/SPECS" "$top/BUILD"

cat > "$top/SPECS/materialcraft.spec" <<EOF
Name:           materialcraft
Version:        $VERSION
Release:        1%{?dist}
Summary:        Procedural, node-based material authoring engine
License:        MIT OR Apache-2.0
URL:            https://github.com/${GITHUB_REPOSITORY}
BuildArch:      x86_64
Requires:       libX11.so.6()(64bit)
Requires:       libXcursor.so.1()(64bit)
Requires:       libXi.so.6()(64bit)
Requires:       libX11-xcb.so.1()(64bit)
Requires:       libxkbcommon.so.0()(64bit)
Requires:       libxkbcommon-x11.so.0()(64bit)
Requires:       libwayland-client.so.0()(64bit)
Requires:       libwayland-egl.so.1()(64bit)
Requires:       libEGL.so.1()(64bit)
Requires:       libGL.so.1()(64bit)

%description
MaterialCraft builds PBR materials from a node graph, with a
headless renderer, a visual editor and an AI control interface.

%prep

%build

%install
mkdir -p %{buildroot}/usr/bin
mkdir -p %{buildroot}/usr/share/applications
mkdir -p %{buildroot}/usr/share/icons/hicolor/256x256/apps
mkdir -p %{buildroot}/usr/share/doc/materialcraft
install -m 0755 %{_appdir}/usr/bin/materialcraft     %{buildroot}/usr/bin/materialcraft
install -m 0755 %{_appdir}/usr/bin/materialcraft-gui %{buildroot}/usr/bin/materialcraft-gui
install -m 0644 %{_appdir}/usr/share/applications/materialcraft.desktop %{buildroot}/usr/share/applications/materialcraft.desktop
install -m 0644 %{_appdir}/materialcraft.png %{buildroot}/usr/share/icons/hicolor/256x256/apps/materialcraft.png
cp -a %{_appdir}/usr/share/doc/materialcraft/. %{buildroot}/usr/share/doc/materialcraft/

%files
/usr/bin/materialcraft
/usr/bin/materialcraft-gui
/usr/share/applications/materialcraft.desktop
/usr/share/icons/hicolor/256x256/apps/materialcraft.png
%license /usr/share/doc/materialcraft
EOF

rpmbuild --define "_topdir $top" \
         --define "_appdir $PWD/AppDir" \
         -bb "$top/SPECS/materialcraft.spec"
rpm=$(find "$top/RPMS" -name "materialcraft-$VERSION-*.rpm" -print -quit)
[ -n "$rpm" ] || { echo "rpmbuild produced no rpm under $top/RPMS"; exit 1; }
cp "$rpm" "dist/$name.rpm"
rpm -qpR "dist/$name.rpm"

stage="$RUNNER_TEMP/stage/$name"
mkdir -p "$stage" dist
install -m 0755 "$OUT/materialcraft"     "$stage/materialcraft"
install -m 0755 "$OUT/materialcraft-gui" "$stage/materialcraft-gui"
cp -R "$RUNNER_TEMP/materialcraft-notices/." "$stage/"
cp README.md "$stage/"
tar -czf "dist/$name.tar.gz" -C "$RUNNER_TEMP/stage" "$name"
