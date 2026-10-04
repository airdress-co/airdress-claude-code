#!/usr/bin/env bash
# Build the macOS launcher: one universal binary, x86_64 and arm64 lipo'd.
#
# Why this is not an entry in `launcher-src/toolchain-images.json`.
# Apple's toolchain cannot be pinned by digest — there is no image, and
# Xcode is whatever the machine has. So `build-launchers.sh`, which
# refuses anything not digest-pinned, is the wrong home for it, and the
# evidence here is weaker than Linux's: two passes, ideally on two
# machines or runner versions, compared. The README says so beside the
# hash rather than leaving it to be noticed.
#
# What it pins that it CAN pin:
#
#   * rustc, through `launcher-src/rust-toolchain.toml`;
#   * the deployment target, explicitly. Left alone it is a default of
#     rustc and of the `cc` crate, and it is written into the binary's
#     load commands, so two toolchains with different defaults would
#     differ there before any code did;
#   * every path the build can see — the checkout, cargo's home and the
#     build directory, for Rust and for C alike (see below);
#   * the timestamp, from the commit.
#
# What it cannot pin, and prints instead: the Xcode (clang, ld, the SDK
# version stamped into LC_BUILD_VERSION). Two Xcodes are expected to
# give two hashes. The CI job selects the same Xcode on both runner
# versions for exactly that reason, and this script prints the one it
# used so a mismatch can be read rather than guessed at.
#
# No signing here. The linker ad-hoc signs each slice (arm64 will not
# execute an unsigned binary at all), and that signature is a pure
# function of the bytes, so it does not break reproducibility. A
# Developer ID signature would, which is why the server compares
# pre-signing bytes; see the README for why the launcher has none.
#
# Usage:
#   scripts/build-launcher-macos.sh            # build twice, compare
#                                              # (pass B: other checkout path,
#                                              #  fresh cargo home)
#   scripts/build-launcher-macos.sh --write    # and install under plugins/
#   PASSES=1 scripts/build-launcher-macos.sh --write
set -euo pipefail

command -v git >/dev/null 2>&1 || { echo "git is not on PATH" >&2; exit 1; }
ROOT_DIR=$(git rev-parse --show-toplevel 2>/dev/null) || {
    echo "not inside a git repository, so there is no tree to build" >&2
    exit 1
}
cd "$ROOT_DIR"

[[ "$(uname -s)" == "Darwin" ]] || {
    echo "the macOS launcher builds on macOS only: it needs Apple's linker and lipo" >&2
    exit 1
}

WRITE=0
[[ "${1:-}" == "--write" ]] && WRITE=1

# One figure for both slices. 11.0 is the first macOS that runs on
# Apple silicon, so it costs the arm64 slice nothing and drops only
# pre-Big Sur Intel Macs, which Apple itself no longer patches.
export MACOSX_DEPLOYMENT_TARGET=11.0

if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "note: the tree is dirty, so these bytes correspond to no commit" >&2
fi
SOURCE_DATE_EPOCH=$(git log -1 --pretty=%ct 2>/dev/null || echo 1700000000)
export SOURCE_DATE_EPOCH
export CARGO_INCREMENTAL=0

CARGO_HOME_DIR="${CARGO_HOME:-$HOME/.cargo}"
TARGETS=(x86_64-apple-darwin aarch64-apple-darwin)

# The two Apple targets for the pinned compiler. Added here rather than
# in rust-toolchain.toml, which every Linux container build reads too:
# a macOS concern should not change what the digest-pinned images fetch.
(cd launcher-src && rustup target add "${TARGETS[@]}" >/dev/null)

echo "== darwin-universal"
echo "   xcode   $(xcodebuild -version 2>/dev/null | tr '\n' ' ' || echo unknown)"
echo "   sdk     $(xcrun --show-sdk-version 2>/dev/null || echo unknown)"
echo "   clang   $(xcrun clang --version 2>/dev/null | head -1 || echo unknown)"
echo "   rustc   $(cd launcher-src && rustc -V)"

# Physical, not the /var -> /private/var symlink: cargo resolves it,
# and a prefix map written with the other spelling silently matches
# nothing.
WORK=$(cd "$(mktemp -d)" && pwd -P)
cleanup() {
    local code=$?
    rm -rf "$WORK" 2>/dev/null || echo "note: could not remove $WORK" >&2
    return "$code"
}
trap cleanup EXIT

# Apple's ld derives LC_UUID from what it links, not only from the
# code, so a path that reaches any input changes the UUID — and the
# UUID sits in the first page the ad-hoc signature hashes. Measured
# 2026-10-04: two builds whose code was byte-identical differed in
# exactly 48 bytes, the UUID and that page's hash. Three things close
# it, and each is needed:
#
#   * C paths. RUSTFLAGS' remapping never reaches the `cc` crate, so
#     `ring`'s objects carried the builder's cargo home verbatim
#     (/Users/<name>/.cargo/...). CFLAGS remaps them the same way.
#   * Archive dates. `ar` stamps each member's mtime; ZERO_AR_DATE
#     makes Apple's ar write zero instead.
#   * One build directory. Object paths reach the linker as given, and
#     no flag remaps them, so every build uses the same absolute path.
#     /tmp is the same on every Mac; $TMPDIR is per user.
BUILD_DIR=/tmp/airdress-launch-macos
export ZERO_AR_DATE=1

passes=(A B)
[[ "${PASSES:-2}" == "1" ]] && passes=(A)
hashes=()
for pass in "${passes[@]}"; do
    # Pass B is a second machine, as far as one machine can be: the
    # source at another path and a cargo home of its own, so a path
    # that leaks shows up as a mismatch here rather than first in CI.
    if [[ "$pass" == "A" ]]; then
        src_root="$ROOT_DIR"
        cargo_home="$CARGO_HOME_DIR"
    else
        src_root="$WORK/a-second-checkout"
        cargo_home="$WORK/cargo-home"
        mkdir -p "$src_root" "$cargo_home"
        cp -R launcher-src "$src_root/"
        # The launcher includes the Sigstore trust root from outside its
        # own directory, so the copy needs it at the same relative path.
        mkdir -p "$src_root/plugins/airdress/launcher"
        cp -R plugins/airdress/launcher/trust "$src_root/plugins/airdress/launcher/"
    fi
    rm -rf "$BUILD_DIR"
    export CARGO_HOME="$cargo_home"
    export CARGO_TARGET_DIR="$BUILD_DIR"
    prefix_maps="$src_root=/plugin $cargo_home=/cargo $BUILD_DIR=/target"
    RUSTFLAGS=""
    CFLAGS=""
    for map in $prefix_maps; do
        RUSTFLAGS="$RUSTFLAGS --remap-path-prefix=$map"
        CFLAGS="$CFLAGS -ffile-prefix-map=$map"
    done
    export RUSTFLAGS CFLAGS
    for t in "${TARGETS[@]}"; do
        # From inside launcher-src/, or rustup never reads its
        # rust-toolchain.toml and the default compiler builds instead.
        (cd "$src_root/launcher-src" && cargo build --release --locked --target "$t") \
            >/dev/null 2>"$WORK/build-$pass-$t.log" || {
            cat "$WORK/build-$pass-$t.log" >&2
            echo "   pass $pass FAILED building $t" >&2
            exit 1
        }
    done
    out="$WORK/airdress-launch-$pass"
    lipo -create -output "$out" \
        "$BUILD_DIR/x86_64-apple-darwin/release/airdress-launch" \
        "$BUILD_DIR/aarch64-apple-darwin/release/airdress-launch"
    hashes+=("$(shasum -a 256 "$out" | cut -d' ' -f1)")
    echo "   pass $pass  ${hashes[${#hashes[@]}-1]}"
done
rm -rf "$BUILD_DIR"

if [[ ${#hashes[@]} -gt 1 ]]; then
    if [[ "${hashes[0]}" != "${hashes[1]}" ]]; then
        echo "   NOT REPRODUCIBLE — the second checkout and cargo home gave other bytes" >&2
        exit 1
    fi
    echo "   reproducible across two checkouts and cargo homes on this machine"
fi

if [[ $WRITE -eq 1 ]]; then
    dest="plugins/airdress/launcher/darwin-universal"
    mkdir -p "$dest"
    install -m 0755 "$WORK/airdress-launch-A" "$dest/airdress-launch"
    echo "   wrote $dest/airdress-launch"
fi
