# TODO

## Docker image

Done: Dockerfile, scripts/docker-entrypoint.sh, the docker and
docker-publish jobs in CI, the README section. Left:

- Check by hand in a real browser, through noVNC: drag a wire, type in a
  field, Run, Compile, save, and copy and paste through noVNC's clipboard
  panel (checked with xdotool inside the container, not through the page)
- After the first push to main: make the ghcr.io/hugoocoto/lineit package
  public (packages are private when first pushed)
- arm64 (Apple Silicon): the Dockerfile builds there (RAYLIB=source), but
  CI only builds amd64, which Docker Desktop runs emulated, slower. An arm
  runner (ubuntu-24.04-arm) and a multi-arch manifest would fix it.

Also possible, later: on Linux, the window straight on the screen (mount
/tmp/.X11-unix and pass DISPLAY), and on Windows through WSLg. Not instead of
the browser: on Mac it needs XQuartz.

Longer term: native Mac and Windows builds. raylib runs on both, but lineit
uses Linux things: dlopen'd modules, fontconfig, Compile running make and cc.
