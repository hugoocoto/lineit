#!/usr/bin/env bash
# Installs lineit from its GitHub releases, nothing to build, on Linux and
# macOS:
#
#   curl -fsSL https://github.com/hugoocoto/lineit/releases/latest/download/install.sh | bash
#
# It downloads the archive for this OS and processor, checks it against the
# release's SHA256SUMS, and puts lineit in PREFIX/bin (~/.local by default,
# /usr/local as root), its examples in PREFIX/share/lineit, and on Linux an
# applications menu entry. Nothing is replaced until everything is
# downloaded and checked. Arguments go after `bash -s --`: see usage().
# On Windows, download lineit-x86_64-pc-windows-msvc.zip from the releases.
set -euo pipefail

REPO=${REPO:-hugoocoto/lineit}
GITHUB=${GITHUB:-https://github.com}

usage() {
    cat <<EOF
Usage: ... | bash -s -- [--prefix DIR] [VERSION | nightly | uninstall]

  VERSION        that release (v1.2.3 or 1.2.3) instead of the latest one
  nightly        the build of the last commit on main (the latest, for now)
  uninstall      remove what this script installed
  --prefix DIR   where it goes: ~/.local by default, /usr/local as root
  -h, --help     this help

Compile (making a program of a canvas) needs Rust: https://rustup.rs
EOF
}

die() {
    printf 'install.sh: %s\n' "$*" >&2
    exit 1
}

prefix=''
what=latest
while [ $# -gt 0 ]; do
    case $1 in
    -h | --help)
        usage
        exit 0
        ;;
    --prefix)
        [ $# -ge 2 ] || die "--prefix needs a folder"
        prefix=$2
        shift
        ;;
    -*) die "$1: no such option (see --help)" ;;
    *) what=$1 ;;
    esac
    shift
done
if [ -z "$prefix" ]; then
    if [ "$(id -u)" = 0 ]; then prefix=/usr/local; else prefix=$HOME/.local; fi
fi
case $prefix in /*) ;; *) prefix=$PWD/$prefix ;; esac

bin=$prefix/bin/lineit
share=$prefix/share/lineit
desktop=$prefix/share/applications/lineit.desktop
icon=$prefix/share/icons/hicolor/256x256/apps/lineit.png

if [ "$what" = uninstall ]; then
    rm -rf "$bin" "$share" "$desktop" "$icon"
    echo "removed lineit from $prefix (your canvases, config and packages stay)"
    exit 0
fi

case "$(uname -s)-$(uname -m)" in
Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
Darwin-arm64) target=aarch64-apple-darwin ;;
Darwin-x86_64) target=x86_64-apple-darwin ;;
*) die "there's no build for $(uname -s) on $(uname -m): build it from source (see the README)" ;;
esac

case $what in
latest | nightly) url=$GITHUB/$REPO/releases/latest/download ;;
v*) url=$GITHUB/$REPO/releases/download/$what ;;
*) url=$GITHUB/$REPO/releases/download/v$what ;;
esac

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
archive=lineit-$target.tar.gz
echo "downloading $url/$archive"
curl -fsSL --retry 3 -o "$tmp/$archive" "$url/$archive" || die "couldn't download $url/$archive"
curl -fsSL --retry 3 -o "$tmp/SHA256SUMS" "$url/SHA256SUMS" || die "couldn't download $url/SHA256SUMS"
want=$(awk -v f="$archive" '$2 == f || $2 == "*" f { print $1 }' "$tmp/SHA256SUMS")
[ -n "$want" ] || die "SHA256SUMS doesn't list $archive"
if command -v sha256sum >/dev/null; then
    got=$(sha256sum "$tmp/$archive" | cut -d' ' -f1)
else
    got=$(shasum -a 256 "$tmp/$archive" | cut -d' ' -f1)
fi
[ "$got" = "$want" ] || die "$archive isn't what the release says it is (its checksum doesn't match)"
tar -C "$tmp" -xzf "$tmp/$archive"
src=$tmp/lineit-$target
"$src/lineit" --version >/dev/null 2>&1 || die "the downloaded lineit doesn't run here"

mkdir -p "$prefix/bin" "$share"
cp "$src/lineit" "$bin.new" && mv "$bin.new" "$bin"
rm -rf "$share/examples" && cp -r "$src/examples" "$share/examples"
if [ "$(uname -s)" = Linux ]; then
    mkdir -p "$(dirname "$desktop")" "$(dirname "$icon")"
    cp "$src/lineit.png" "$icon"
    cat >"$desktop" <<EOF
[Desktop Entry]
Type=Application
Name=lineit
Comment=A visual programming language
Exec=$bin %f
Icon=lineit
Terminal=false
Categories=Development;IDE;
EOF
fi

echo "installed $("$bin" --version) in $bin"
case ":$PATH:" in
*":$prefix/bin:"*) ;;
*) echo "$prefix/bin isn't in your PATH: add it, or run $bin" ;;
esac
echo "try it: lineit $share/examples/hello.lua"
