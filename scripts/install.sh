#!/usr/bin/env bash
# Install lineit from its GitHub releases, nothing to build, on Linux and
# macOS:
#
#   curl -fsSL https://github.com/hugoocoto/lineit/releases/latest/download/install.sh | bash
#
# It asks what to install and where, suggesting what it finds: the latest
# release and ~/.local. Then it installs the archive for this OS and
# processor: lineit in PREFIX/bin, its examples in PREFIX/share/lineit, and
# on Linux an applications menu entry and icon. It checks the archive
# against the release's SHA256SUMS, and SHA256SUMS against the release's
# build attestation when gh is logged in. Nothing is replaced until
# everything is downloaded and checked. Without a terminal, or with --yes,
# it takes the suggestions without asking. See usage() for the arguments,
# which go after `bash -s --`. It runs on the bash macOS has (3.2).
#
# On Windows, download lineit-x86_64-pc-windows-msvc.zip from the releases.
set -euo pipefail

NAME=lineit
REPO=${REPO:-hugoocoto/lineit}
GITHUB=${GITHUB:-https://github.com}

usage() {
    cat <<EOF
Usage: ... | bash -s -- [OPTIONS] [VERSION | nightly | uninstall]

Installs $NAME from $GITHUB/$REPO/releases. It asks what to install and
where; the arguments change what it suggests.

  VERSION              that release (v1.2.3 or 1.2.3) instead of the latest one
  nightly              the build of the last commit on main
  uninstall            remove everything this script installed
  -y, --yes            don't ask, take the suggestions (also without a terminal)
  --strict             fail if the release's attestation can't be checked
                       (needs gh, logged in)
  --skip-attestation   don't check the attestation
  -h, --help           this help

PREFIX is where it goes: ~/.local by default, /usr/local as root
(... | sudo bash). A relative PREFIX is taken from the current directory.

Compile (making a program of a canvas) needs Rust: https://rustup.rs
EOF
}

tmp=''
staged=()
interactive=''

say() { printf '%s\n' "$*"; }
complain() { printf 'install.sh: %s\n' "$*" >&2; }
warn() { printf 'warning: %s\n' "$*" >&2; }
die() {
    printf 'install.sh: %s\n' "$*" >&2
    exit 1
}
fetch() { curl -fsSL --retry 3 -o "$2" "$1"; }

sha256() {
    if command -v sha256sum >/dev/null; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

# ask VAR QUESTION SUGGESTION: the answer, typed on the terminal (Enter
# takes the suggestion: bash 4 and newer have it filled in already). It reads
# /dev/tty because under `curl | bash` stdin is this script. Without a
# terminal, the suggestion.
ask() {
    local got
    if [ -z "$interactive" ]; then
        printf -v "$1" '%s' "$3"
        return
    fi
    if [ "${BASH_VERSINFO[0]}" -ge 4 ]; then
        read -rep "$2: " -i "$3" got </dev/tty || die "cancelled"
    else
        read -rp "$2 [$3]: " got </dev/tty || die "cancelled"
        [ -n "$got" ] || got=$3
    fi
    printf -v "$1" '%s' "$got"
}

# choose VAR QUESTION SUGGESTION CHOICE...: ask until it's one of the choices
choose() {
    local var=$1 question=$2 suggestion=$3 choice choices
    shift 3
    choices="$*"
    while :; do
        ask "$var" "$question (${choices// /, })" "$suggestion"
        for choice; do [ "${!var}" = "$choice" ] && return; done
        [ -n "$interactive" ] || die "'${!var}' is not one of: $*"
        say "It's one of: $*"
    done
}

# confirm QUESTION: yes unless answered no; always yes without a terminal
confirm() {
    local ok
    [ -n "$interactive" ] || return 0
    read -rp "$1 [Y/n] " ok </dev/tty || die "cancelled"
    case $ok in
    '' | [yY]*) return 0 ;;
    *) return 1 ;;
    esac
}

# The default PREFIX: what's set, else ~/.local, or /usr/local as root
default_prefix() {
    if [ -n "${PREFIX:-}" ]; then
        say "$PREFIX"
    elif [ "$(id -u)" = 0 ]; then
        say /usr/local
    else
        say "$HOME/.local"
    fi
}

# Where everything goes, from PREFIX $1: ~ expanded (it's typed), relative
# to the current dir (the install runs from a temp dir)
set_prefix() {
    PREFIX=$1
    # shellcheck disable=SC2088 # the ~ typed, not expanded yet
    case $PREFIX in
    '~') PREFIX=$HOME ;;
    '~/'*) PREFIX=$HOME/${PREFIX#'~/'} ;;
    esac
    [ -n "$PREFIX" ] || die "the install dir can't be empty"
    case $PREFIX in
    /*) ;;
    *) PREFIX=$PWD/$PREFIX ;;
    esac
    [ "$PREFIX" = / ] || PREFIX=${PREFIX%/}
    BINDIR=$PREFIX/bin
    SHAREDIR=$PREFIX/share/$NAME
    APPSDIR=$PREFIX/share/applications
    ICONSDIR=$PREFIX/share/icons
    DESKTOP=$APPSDIR/$NAME.desktop
    ICON=$ICONSDIR/hicolor/256x256/apps/$NAME.png
    FILES=("$BINDIR/$NAME" "$SHAREDIR" "$DESKTOP" "$ICON")
}

# The version installed in PREFIX, or nothing
installed() {
    local version
    [ -x "$BINDIR/$NAME" ] || return 0
    # "lineit 0.1.0"
    if version=$("$BINDIR/$NAME" --version 2>/dev/null | head -1) && [ -n "$version" ]; then
        say "${version##* }"
    else
        say unknown
    fi
}

# The latest release's tag: releases/latest redirects to releases/tag/<tag>,
# or to releases if there are none. Nothing if it can't tell.
latest() {
    local tag
    tag=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$GITHUB/$REPO/releases/latest" 2>/dev/null) || return 0
    tag=${tag##*/}
    [ "$tag" = releases ] || [ "$tag" = latest ] || say "$tag"
}

cleanup() {
    local f
    for f in ${staged[@]+"${staged[@]}"}; do rm -rf "$f.$NAME-new"; done
    [ -z "$tmp" ] || rm -rf "$tmp"
}

# Check FILE (in the current dir) against its line in SHA256SUMS
verify() {
    local want
    want=$(awk -v f="$1" '$2 == f || $2 == "*" f { print $1 }' SHA256SUMS)
    [ -n "$want" ] || {
        complain "$1 is not in the release's SHA256SUMS"
        return 1
    }
    [ "$(sha256 "$1")" = "$want" ] || {
        complain "$1 doesn't match the release's SHA256SUMS"
        return 1
    }
}

# Why the release's attestation can't be checked, if it can't
no_attest() {
    if [ "$GITHUB" != https://github.com ]; then
        say "$GITHUB is not GitHub"
    elif ! command -v gh >/dev/null; then
        say "gh is not installed"
    elif ! gh auth status >/dev/null 2>&1; then
        say "gh is not logged in (gh auth login)"
    fi
}

# Check that SHA256SUMS was made by this repo's CI
attest() {
    gh attestation verify SHA256SUMS --repo "$REPO" \
        --signer-workflow "$REPO/.github/workflows/ci.yml" >/dev/null 2>&1 || {
        complain "SHA256SUMS has no valid attestation from $REPO's CI"
        return 1
    }
    say "Checked the release's attestation"
}

# The archive into the current dir, checked: 1 if it doesn't check out. It
# runs in main's scope (url, archive, why, skip_attest).
download() {
    fetch "$url/SHA256SUMS" SHA256SUMS ||
        die "$REPO has no release $tag, or it has no SHA256SUMS (releases: $GITHUB/$REPO/releases)"
    if [ -z "$skip_attest" ] && [ -z "$why" ]; then attest || return 1; fi
    fetch "$url/$archive" "$archive" || die "release $tag has no $archive, though its SHA256SUMS has it"
    verify "$archive"
}

# Copy SRC (a file or a folder) as DEST, but beside it, until commit puts
# everything in place
stage() {
    mkdir -p "$(dirname "$2")"
    rm -rf "$2.$NAME-new"
    cp -R "$1" "$2.$NAME-new"
    staged+=("$2")
}

commit() {
    local f
    for f in ${staged[@]+"${staged[@]}"}; do
        rm -rf "$f"
        mv -f "$f.$NAME-new" "$f"
    done
    staged=()
}

uninstall() {
    local f found=()
    for f in "${FILES[@]}"; do
        if [ -e "$f" ] || [ -L "$f" ]; then found+=("$f"); fi
    done
    if [ ${#found[@]} = 0 ]; then
        say "$NAME is not installed in $PREFIX"
        return
    fi
    say "Installed in $PREFIX:"
    printf '  %s\n' "${found[@]}"
    say "(your canvases, config, modules and packages stay)"
    confirm "Remove them?" || die "cancelled"
    rm -rf "${found[@]}"
    refresh_desktop
    say "Removed $NAME from $PREFIX"
}

# Update the menu and icon caches, but only the ones that already exist:
# making an icon cache would hide the icons of later apps that don't update it
refresh_desktop() {
    if [ -f "$APPSDIR/mimeinfo.cache" ] && command -v update-desktop-database >/dev/null; then
        update-desktop-database -q "$APPSDIR" 2>/dev/null || true
    fi
    if [ -f "$ICONSDIR/hicolor/icon-theme.cache" ] && command -v gtk-update-icon-cache >/dev/null; then
        gtk-update-icon-cache -q -t "$ICONSDIR/hicolor" 2>/dev/null || true
    fi
}

# The applications menu entry, run from where it was installed
desktop_entry() {
    cat <<EOF
[Desktop Entry]
Type=Application
Name=$NAME
Comment=A visual programming language
Exec="$BINDIR/$NAME" %f
TryExec=$BINDIR/$NAME
Icon=$NAME
Terminal=false
Categories=Development;IDE;
EOF
}

# The whole script runs from here, at its last line: if the download is cut,
# bash runs nothing instead of half of it
main() {
    local tag='' yes='' strict='' skip_attest='' action=install
    local url archive target src version found arg prefix current newest why checks try

    for arg; do
        case $arg in
        -h | --help)
            usage
            return
            ;;
        -y | --yes) yes=1 ;;
        --strict) strict=1 ;;
        --skip-attestation) skip_attest=1 ;;
        -*) die "unknown option $arg (see --help)" ;;
        uninstall) action=uninstall ;;
        *)
            [ -z "$tag" ] || die "both $tag and $arg given, pick one"
            tag=$arg
            ;;
        esac
    done
    [ -z "$strict" ] || [ -z "$skip_attest" ] || die "--strict and --skip-attestation don't go together"
    if [ -z "$yes" ] && { : </dev/tty; } 2>/dev/null; then interactive=1; fi

    case "$(uname -s)-$(uname -m)" in
    Linux-x86_64 | Linux-amd64) target=x86_64-unknown-linux-gnu ;;
    Darwin-arm64) target=aarch64-apple-darwin ;;
    Darwin-x86_64) target=x86_64-apple-darwin ;;
    *) die "there's no build for $(uname -s) on $(uname -m): build it from source (see the README)" ;;
    esac
    archive=$NAME-$target.tar.gz

    [ -z "$interactive" ] || say "Installs $NAME from $GITHUB/$REPO (Enter takes the suggestion, Ctrl-C quits)"
    choose action "Install or uninstall" "$action" install uninstall
    prefix=$(default_prefix)
    # shellcheck disable=SC2088 # shown as typed, set_prefix expands it
    [ "$prefix" != "$HOME/.local" ] || prefix='~/.local'
    if [ "$action" = uninstall ]; then
        ask prefix "Uninstall from" "$prefix"
        set_prefix "$prefix"
        uninstall
        return
    fi
    ask prefix "Install to" "$prefix"
    set_prefix "$prefix"

    for arg in curl awk tar mktemp; do
        command -v "$arg" >/dev/null || die "needs $arg"
    done
    command -v sha256sum >/dev/null || command -v shasum >/dev/null || die "needs sha256sum or shasum"

    current=$(installed)
    newest=$(latest)
    if [ -n "$current" ]; then
        say "Installed now: $NAME $current, the latest release is ${newest:-none}"
    else
        say "The latest release is ${newest:-none}"
    fi
    [ -n "$tag" ] || tag=${newest:-nightly}
    ask tag "Version (vX.Y.Z or nightly)" "$tag"
    case $tag in
    [0-9]*) tag=v$tag ;;
    '') die "the version can't be empty" ;;
    esac
    url=$GITHUB/$REPO/releases/download/$tag

    why=''
    [ -n "$skip_attest" ] || why=$(no_attest)
    if [ -n "$skip_attest" ]; then
        checks="SHA256SUMS only (--skip-attestation)"
    elif [ -n "$why" ]; then
        [ -z "$strict" ] || die "can't check the release's attestation: $why"
        checks="SHA256SUMS only: $why. It catches broken downloads, not a release whose files were all replaced"
    else
        checks="SHA256SUMS and the release's attestation"
    fi
    say "Checks: $checks"
    confirm "Install $NAME $tag to $PREFIX?" || die "cancelled"

    tmp=$(mktemp -d)
    trap cleanup EXIT
    cd "$tmp"
    say "Downloading $NAME $tag"
    # CI replaces nightly's files one by one, so for a moment they may
    # not match its checksums yet: then it tries again
    for try in 1 2 3; do
        download && break
        if [ "$tag" != nightly ] || [ "$try" -ge 3 ]; then
            die "the download doesn't check out, nothing was installed"
        fi
        say "nightly may be being updated right now, trying again in 20 seconds"
        sleep 20
    done
    tar -xzf "$archive"
    src=$NAME-$target
    "$src/$NAME" --version >/dev/null 2>&1 || die "the $NAME downloaded doesn't run here, nothing was installed"

    # Beside their place first, so a failure leaves the old install as it was
    stage "$src/$NAME" "$BINDIR/$NAME"
    stage "$src/examples" "$SHAREDIR/examples"
    if [ "$(uname -s)" = Linux ]; then
        desktop_entry >desktop
        stage desktop "$DESKTOP"
        stage "$src/$NAME.png" "$ICON"
    fi
    commit
    if [ "$(uname -s)" = Linux ]; then
        refresh_desktop
        say "Added $NAME to the applications menu"
    fi

    version=$("$BINDIR/$NAME" --version | head -1)
    say "Installed $version to $BINDIR/$NAME, its examples to $SHAREDIR/examples"
    case ":$PATH:" in
    *":$BINDIR:"*) ;;
    *) say "$BINDIR is not in your PATH, add it to your shell's config" ;;
    esac
    found=$(type -P "$NAME" || true)
    if [ -n "$found" ] && ! [ "$found" -ef "$BINDIR/$NAME" ]; then
        warn "\`$NAME\` runs $found instead, it comes first in your PATH"
    fi
    say "Try it: $NAME $SHAREDIR/examples/hello.lua"
}

main "$@"
