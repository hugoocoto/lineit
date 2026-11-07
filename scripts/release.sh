#!/usr/bin/env bash
# Tag HEAD as vX.Y.Z and push the tag: CI then builds and publishes the
# release. X.Y.Z is the version in Cargo.toml's [workspace.package], the
# only version of lineit (see Versions in CONTRIBUTING.md): bump it first,
# in a commit on main, minor if something was added, major if something
# changed or went away. The tag must be newer than every v* tag there is,
# here or on origin.
set -euo pipefail
cd "$(dirname "$0")/.."

die() {
    echo "release: $*" >&2
    exit 1
}

{ : </dev/tty; } 2>/dev/null || die "it needs a terminal, to confirm the tag"

git fetch --tags --quiet origin || die "can't fetch tags from origin"

branch=$(git branch --show-current)
[ "$branch" = main ] || die "on '$branch', releases are made from main"
[ -z "$(git status --porcelain --untracked-files=no)" ] || die "uncommitted changes"
git fetch --quiet origin main
[ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] ||
    die "HEAD is not origin/main: push or pull first"

version=$(scripts/version.sh)
[[ $version =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] ||
    die "the version in Cargo.toml is '$version', not X.Y.Z"
latest=$(git tag --list 'v[0-9]*.[0-9]*.[0-9]*' | sed 's/^v//' | sort -V | tail -1)
if [ -n "$latest" ]; then
    if [ "$latest" = "$version" ] ||
        [ "$(printf '%s\n%s\n' "$latest" "$version" | sort -V | tail -1)" != "$version" ]; then
        die "the version is $version, and v$latest is released: bump it in Cargo.toml first"
    fi
fi

echo "Latest release: ${latest:+v$latest}${latest:-none}"
echo "This one:       v$version"
echo
git log --oneline --no-merges ${latest:+"v$latest..HEAD"}
echo
read -rp "Tag $(git rev-parse --short HEAD) as v$version and push it? [y/N] " ok </dev/tty
[ "$ok" = y ] || [ "$ok" = Y ] || die "cancelled"

git tag -a "v$version" -m "v$version"
git push origin "v$version"
echo "Pushed v$version. CI publishes the release: $(gh repo view --json url -q .url 2>/dev/null || echo "see GitHub")/actions"
