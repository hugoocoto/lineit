#!/usr/bin/env bash
# appimage.sh LINEIT OUT: OUT/lineit-x86_64.AppImage, of the lineit
# program LINEIT: one file that runs on most Linux desktops. It uses the
# system's OpenGL, X11 or Wayland libraries, as any desktop program does.
# appimagetool is downloaded, checked against its checksum.
set -euo pipefail

bin=$1 out=$2
here=$(cd "$(dirname "$0")/.." && pwd)
tool_url=https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage
tool_sha256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
curl -fsSL "$tool_url" -o "$work/appimagetool"
if ! echo "$tool_sha256  $work/appimagetool" | sha256sum -c --quiet; then
    echo "appimage.sh: appimagetool isn't what it should be (its checksum changed)" >&2
    exit 1
fi
chmod +x "$work/appimagetool"

app=$work/lineit.AppDir
mkdir -p "$app/usr/bin" "$app/usr/share/lineit"
cp "$bin" "$app/usr/bin/lineit"
cp -r "$here/examples" "$app/usr/share/lineit/"
cp "$here/assets/lineit.desktop" "$app/lineit.desktop"
cp "$here/assets/lineit.png" "$app/lineit.png"
ln -s usr/bin/lineit "$app/AppRun"

mkdir -p "$out"
# no FUSE on CI runners: appimagetool runs extracted
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$work/appimagetool" --no-appstream "$app" "$out/lineit-x86_64.AppImage"
ls -l "$out"
