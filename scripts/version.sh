#!/bin/sh
# Prints lineit's version: the one in Cargo.toml's [workspace.package], the
# only one there is. CI, release.sh and the release's tag use it.
set -eu
cd "$(dirname "$0")/.."
sed -n '/^\[workspace\.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml
