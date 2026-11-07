# lineit in a browser, on any system with Docker:
#
#   docker build -t lineit .
#   docker run --rm -p 127.0.0.1:6080:6080 -v "$PWD":/work lineit
#
# and open http://localhost:6080. A container has no screen, so lineit draws
# on a virtual one (Xvnc, drawn by the CPU), which noVNC shows as a web page,
# as big as the browser's window. Your canvases, config, packages and
# compiled programs are in /work (see scripts/docker-entrypoint.sh).
# Arguments after the image go to lineit.

# The official Rust image: it builds lineit, and Compile needs cargo too
FROM rust:1-slim-trixie AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
        libx11-dev libxkbcommon-dev libgl1-mesa-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN cargo build --release --locked && cp target/release/lineit /usr/local/bin/lineit

FROM rust:1-slim-trixie
# software GL (llvmpipe) and what winit loads; the screen and the web page;
# a C compiler is in the image already (Lua modules in compiled programs)
RUN apt-get update && apt-get install -y --no-install-recommends \
        libgl1 libglx-mesa0 libgl1-mesa-dri libx11-6 libx11-xcb1 libxcursor1 libxrandr2 libxi6 libxkbcommon-x11-0 \
        tigervnc-standalone-server matchbox-window-manager novnc websockify \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /usr/local/bin/lineit /usr/local/bin/lineit
COPY examples /usr/share/lineit/examples
COPY scripts/docker-entrypoint.sh /usr/local/bin/lineit-docker
# The web page opens connected, makes the screen the size of the browser's
# window, and reconnects if it drops. /tmp/.X11-unix as X wants it, made by
# root: Xvnc doesn't run as root.
RUN printf '%s\n' '<!doctype html><title>lineit</title>' \
        '<meta http-equiv="refresh" content="0; url=vnc.html?autoconnect=true&amp;resize=remote&amp;reconnect=true">' \
        >/usr/share/novnc/index.html && \
    mkdir -m1777 -p /tmp/.X11-unix && \
    useradd --uid 1000 --user-group --home-dir /work --create-home --shell /bin/sh lineit && \
    chmod 0755 /usr/local/bin/lineit-docker && \
    chmod -R a+rwX /usr/local/cargo

ENV HOME=/work
WORKDIR /work
EXPOSE 6080
ENTRYPOINT ["/usr/local/bin/lineit-docker"]
