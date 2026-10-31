#!/bin/sh
# The Docker image's entrypoint (see the Dockerfile): a virtual X screen of
# 1280x720 (Xvfb), shown by x11vnc and served as a web page on port 6080 by
# noVNC (websockify), and lineit on it, with its arguments. No window
# manager: lineit is the only window, as big as the screen. If it quits it
# starts again; if the screen goes, the container stops.
#
# Everything lineit keeps is in /work, the folder mounted with -v: it's
# HOME, so ~/.config/lineit, ~/.local/share/lineit (modules, compiled
# programs) and ~/.cache/lineit too, and canvases are saved there.
set -eu

# Started as root (the default), it goes on as the owner of /work, so what
# it saves there is the host user's. If root owns it (no -v, Docker Desktop),
# as lineit, the image's user. Started as someone else (--user), as them.
if [ "$(id -u)" = 0 ]; then
        uid=$(stat -c %u /work) gid=$(stat -c %g /work)
        [ "$uid" != 0 ] || { uid=$(id -u lineit) gid=$(id -g lineit); }
        exec setpriv --reuid="$uid" --regid="$gid" --clear-groups "$0" "$@"
fi
[ -w /work ] || echo "lineit-docker: /work isn't writable by uid $(id -u): run with --user \"\$(id -u):\$(id -g)\"" >&2

export HOME=/work DISPLAY=:0 XDG_RUNTIME_DIR=/tmp/runtime-$(id -u)
mkdir -p -m700 "$XDG_RUNTIME_DIR"

pids= lineit=
trap 'trap - TERM INT; kill $pids $lineit 2>/dev/null; exit 0' TERM INT

# What the last one left, if the container is started again
rm -f /tmp/.X0-lock /tmp/.X11-unix/X0
Xvfb :0 -screen 0 1280x720x24 -nolisten tcp &
xvfb=$!
pids=$xvfb
i=0
until [ -S /tmp/.X11-unix/X0 ]; do
        kill -0 "$xvfb" 2>/dev/null && [ $i -lt 100 ] || { echo "lineit-docker: Xvfb didn't start" >&2; exit 1; }
        i=$((i + 1))
        sleep 0.1
done

x11vnc -display :0 -rfbport 5900 -localhost -forever -shared -nopw -quiet &
pids="$pids $!"
websockify --web /usr/share/novnc 6080 localhost:5900 >/dev/null &
pids="$pids $!"
echo "lineit-docker: open http://localhost:6080"

cd /work
while kill -0 "$xvfb" 2>/dev/null; do
        lineit "$@" &
        lineit=$!
        wait $lineit || true
        sleep 1
done
echo "lineit-docker: Xvfb quit" >&2
exit 1
