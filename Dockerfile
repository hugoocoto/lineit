# lineit in a browser, on any system with Docker:
#
#   docker build -t lineit .
#   docker run --rm -p 127.0.0.1:6080:6080 -v "$PWD":/work lineit
#
# and open http://localhost:6080. A container has no screen, so lineit draws
# on a virtual one (Xvnc, drawn by the CPU), which noVNC shows as a web page,
# as big as the browser's window. Your canvases, config and compiled programs are in /work (see
# scripts/docker-entrypoint.sh). Arguments after the image go to lineit.

# Debian 13, pinned like the actions in CI (Dependabot updates both): glibc
# 2.41, so the raylib in the repo links (it needs 2.38, see the Makefile).
# On other machines than x86_64 there's no raylib in the repo: it's built
# from its source.
FROM debian:trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4 AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential pkg-config liblua5.1-0-dev libfontconfig-dev \
        libx11-dev zlib1g-dev libssl-dev libffi-dev \
        ca-certificates curl libxrandr-dev libxinerama-dev libxcursor-dev libxi-dev
WORKDIR /src
COPY . .
RUN raylib=repo; [ "$(uname -m)" = x86_64 ] || raylib=source; \
    make -j"$(nproc)" RAYLIB=$raylib && \
    make install RAYLIB=$raylib PREFIX=/usr DESTDIR=/out

FROM debian:trixie-slim@sha256:f6b08b09a05725f9431502d3b5714f5c4576e82c1a84fa1debe0c24a7e0c3ec4
# lineit's libraries, software GL (llvmpipe) and what GLFW loads; a C
# compiler, make and objcopy for Compile (libffi-dev: ffi/call, and the
# programs that use it link it), zip and lua5.1 for lineit-pack;
# the screen and the web page
RUN apt-get update && apt-get install -y --no-install-recommends \
        liblua5.1-0 libfontconfig1 libx11-6 zlib1g libssl3t64 libffi-dev fonts-dejavu-core \
        libgl1 libglx-mesa0 libgl1-mesa-dri libxrandr2 libxinerama1 libxcursor1 libxi6 \
        gcc libc6-dev make binutils zip lua5.1 \
        tigervnc-standalone-server matchbox-window-manager novnc websockify \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /out/usr /usr
COPY scripts/docker-entrypoint.sh /usr/bin/lineit-docker
# The default font isn't here: DejaVu Sans Mono instead. The web page opens
# connected, makes the screen the size of the browser's window, and
# reconnects if it drops. /tmp/.X11-unix as X wants it, made by root: Xvnc
# doesn't run as root.
RUN printf '%s\n' '<?xml version="1.0"?>' '<!DOCTYPE fontconfig SYSTEM "fonts.dtd">' \
        '<fontconfig><alias binding="same"><family>Iosevka NFM</family>' \
        '<prefer><family>DejaVu Sans Mono</family></prefer></alias></fontconfig>' \
        >/etc/fonts/local.conf && \
    printf '%s\n' '<!doctype html><title>lineit</title>' \
        '<meta http-equiv="refresh" content="0; url=vnc.html?autoconnect=true&amp;resize=remote&amp;reconnect=true">' \
        >/usr/share/novnc/index.html && \
    mkdir -m1777 -p /tmp/.X11-unix && \
    useradd --uid 1000 --user-group --home-dir /work --create-home --shell /bin/sh lineit

ENV HOME=/work
WORKDIR /work
EXPOSE 6080
ENTRYPOINT ["/usr/bin/lineit-docker"]
