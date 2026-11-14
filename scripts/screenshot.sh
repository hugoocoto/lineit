#!/bin/sh
# Opens a canvas in the editor on a virtual screen (Xvfb) and saves a
# screenshot of it, to check the editor without touching the real screen:
#
#     scripts/screenshot.sh CANVAS OUT.png [ZOOM] [SECONDS]
#
# ZOOM replaces the zoom of the canvas's camera (on a copy), SECONDS is how
# long the editor gets before the shot (default 6). The editor is
# target/debug/lineit, or $LINEIT. Its config, data and cache are in a
# folder of their own, so the user's settings and session aren't touched.
# Needs Xvfb and ImageMagick's import.
set -eu
[ $# -ge 2 ] || { echo "usage: $0 CANVAS OUT.png [ZOOM] [SECONDS]" >&2; exit 2; }
canvas=$(realpath "$1")
out=$(realpath -m "$2")
zoom=${3:-}
wait=${4:-6}
root=$(realpath "$(dirname "$0")/..")
lineit=${LINEIT:-$root/target/debug/lineit}
[ -x "$lineit" ] || { echo "no editor at $lineit: cargo build -p lineit" >&2; exit 1; }

tmp=$(mktemp -d)
xvfb=
editor=
cleanup() {
    [ -n "$editor" ] && kill "$editor" 2>/dev/null || true
    [ -n "$xvfb" ] && kill "$xvfb" 2>/dev/null || true
    rm -rf "$tmp"
}
trap cleanup EXIT INT TERM

if [ -n "$zoom" ]; then
    cp "$canvas" "$tmp/$(basename "$canvas")"
    canvas="$tmp/$(basename "$canvas")"
    sed -i "s/\(camera = {.*zoom = \)[0-9.]*/\1$zoom/" "$canvas"
    grep -q "zoom = $zoom" "$canvas" || { echo "no camera zoom in the canvas to change" >&2; exit 1; }
fi

# a display that's free: Xvfb picks it and writes its number
mkfifo "$tmp/display"
Xvfb -displayfd 3 -screen 0 1280x800x24 3>"$tmp/display" >/dev/null 2>&1 &
xvfb=$!
read -r display <"$tmp/display"

mkdir -p "$tmp/config" "$tmp/data" "$tmp/cache"
# without WAYLAND_DISPLAY, or the editor opens on the real screen
env -u WAYLAND_DISPLAY DISPLAY=":$display" XDG_CONFIG_HOME="$tmp/config" XDG_DATA_HOME="$tmp/data" \
    XDG_CACHE_HOME="$tmp/cache" LIBGL_ALWAYS_SOFTWARE=1 "$lineit" "$canvas" >"$tmp/log" 2>&1 &
editor=$!
sleep "$wait"
kill -0 "$editor" 2>/dev/null || { echo "the editor ended:" >&2; cat "$tmp/log" >&2; exit 1; }
DISPLAY=":$display" import -window root "$out"
echo "$out"
