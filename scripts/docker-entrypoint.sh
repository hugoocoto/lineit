#!/bin/sh
# The Docker image's entrypoint (see the Dockerfile): a virtual X screen
# with a VNC server (Xvnc), served as a web page on port 6080 by noVNC
# (websockify), and lineit on it, with its arguments. The page asks Xvnc for
# a screen as big as the browser's window, and the window manager
# (matchbox) keeps lineit, the only window, as big as the screen. Xvnc also
# passes the clipboard between them. If lineit quits it starts again; if the
# screen goes, the container stops.
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

XDG_RUNTIME_DIR=/tmp/runtime-$(id -u)
export HOME=/work DISPLAY=:0 XDG_RUNTIME_DIR
[ -d "$XDG_RUNTIME_DIR" ] || mkdir -m700 "$XDG_RUNTIME_DIR"

pids='' lineit=''
trap 'trap - TERM INT; kill $pids $lineit 2>/dev/null; exit 0' TERM INT

# What the last one left, if the container is started again
rm -f /tmp/.X0-lock /tmp/.X11-unix/X0
# No password: only websockify, in here, reaches it
Xvnc :0 -geometry 1280x720 -depth 24 -desktop lineit -nolisten tcp \
        -rfbport 5900 -localhost -SecurityTypes None -AlwaysShared -Log '*:stderr:0' &
xvnc=$!
pids=$xvnc
i=0
until [ -S /tmp/.X11-unix/X0 ]; do
        kill -0 "$xvnc" 2>/dev/null && [ $i -lt 100 ] || { echo "lineit-docker: Xvnc didn't start" >&2; exit 1; }
        i=$((i + 1))
        sleep 0.1
done

matchbox-window-manager -use_titlebar no &
pids="$pids $!"
websockify --web /usr/share/novnc 6080 localhost:5900 >/dev/null &
pids="$pids $!"
echo "lineit-docker: open http://localhost:6080"

cd /work
while kill -0 "$xvnc" 2>/dev/null; do
        lineit "$@" &
        lineit=$!
        wait $lineit || true
        sleep 1
done
echo "lineit-docker: Xvnc quit" >&2
exit 1
