#!/usr/bin/env bash
# dist.sh TARGET ARCHIVE OUT: the release archive of a build for TARGET
# (cargo build --release --target TARGET), as OUT/lineit-TARGET.ARCHIVE
# (tar.gz or zip): the program, the examples, the README, the license and
# the icon.
set -euo pipefail

target=$1 archive=$2 out=$3
exe=lineit
[[ $target == *windows* ]] && exe=lineit.exe
name=lineit-$target
stage=$(mktemp -d)
mkdir -p "$stage/$name" "$out"
cp "target/$target/release/$exe" "$stage/$name/"
cp -r README.md LICENSE examples assets/lineit.png "$stage/$name/"
rm -f "$stage/$name"/examples/symi-*.png
case $archive in
tar.gz) tar -C "$stage" -czf "$out/$name.tar.gz" "$name" ;;
zip)
    if command -v 7z >/dev/null; then
        (cd "$stage" && 7z a -tzip "$OLDPWD/$out/$name.zip" "$name" >/dev/null)
    else
        (cd "$stage" && zip -qr "$OLDPWD/$out/$name.zip" "$name")
    fi
    ;;
*)
    echo "dist.sh: $archive: tar.gz or zip" >&2
    exit 1
    ;;
esac
rm -rf "$stage"
ls -l "$out"
