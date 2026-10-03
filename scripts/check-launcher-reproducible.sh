#!/usr/bin/env bash
# Is the launcher's build deterministic?
#
# It builds twice, from a clean output directory each time, and compares
# the bytes. That is a real check and it is **narrower than it looks**:
# it proves determinism on ONE machine with ONE toolchain, which is not
# the same as the cross-machine byte-identity FR-69 asks for.
#
# Why the difference is not a detail (measured 2026-10-04). The
# committed `linux-x86_64` binary reproduced byte-for-byte twice on a
# workstation and then DIFFERED on a CI runner. Two causes, both real:
#
#   1. rustc 1.98.1 locally against 1.98.0 pinned in CI. Fixed — there
#      is a `rust-toolchain.toml` now, so everyone uses one compiler.
#   2. `sigstore`'s certificate verification pulls a **C** library
#      (`aws-lc-sys`, or `ring` if you try to avoid it; `rustls-webpki`
#      brings one either way). A C library's object code depends on the
#      builder's C compiler, so two machines with different `cc` cannot
#      produce the same bytes. Removing it was attempted and the crate's
#      feature graph does not allow it.
#
# So cross-machine byte-identity needs the C toolchain pinned too — in
# practice, building inside a container pinned by digest. That is not
# set up yet, which is why no launcher binary is committed: an
# executable in a public repository that nobody else can reproduce is
# worse than no executable at all, and the plugin cannot be installed
# until a release exists anyway.
#
# `--write` puts the binaries in place for the day that container exists
# (and for a release built inside one).
#
# Usage: scripts/check-launcher-reproducible.sh [--write]
#
# `PLATFORMS="linux-x86_64"` limits it, for a workstation without a
# cross-linker. CI never sets it.
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

WRITE=0
[[ "${1:-}" == "--write" ]] && WRITE=1

# The commit this source is at decides the timestamp, so a rebuild of an
# unchanged tree produces unchanged bytes. Before the first commit there
# is no timestamp to take, so a fixed one stands in — the binaries are
# rewritten by the commit that adds them anyway.
SOURCE_DATE_EPOCH=$(git log -1 --pretty=%ct 2>/dev/null || echo 1)
export SOURCE_DATE_EPOCH

ROOT=$(pwd)
export RUSTFLAGS="--remap-path-prefix=$ROOT=/airdress --remap-path-prefix=$HOME/.cargo=/cargo -C target-feature=+crt-static"
export CARGO_INCREMENTAL=0

declare -A TARGETS=(
    [linux-x86_64]=x86_64-unknown-linux-musl
    [linux-aarch64]=aarch64-unknown-linux-musl
)

WANTED="${PLATFORMS:-linux-x86_64 linux-aarch64}"

fail=0
for platform in $WANTED; do
    [[ -n "${TARGETS[$platform]:-}" ]] || { echo "unknown platform $platform" >&2; exit 1; }
    target="${TARGETS[$platform]}"
    committed="plugins/airdress/launcher/$platform/airdress-launch"

    echo "building $platform ($target)"
    cargo build \
        --manifest-path launcher-src/Cargo.toml \
        --release --locked --target "$target"
    built="launcher-src/target/$target/release/airdress-launch"

    if [[ $WRITE -eq 1 ]]; then
        mkdir -p "$(dirname "$committed")"
        install -m 0755 "$built" "$committed"
        echo "  wrote $committed ($(wc -c < "$committed") bytes)"
        continue
    fi

    # Keep the first build, wipe what produced it, build again.
    first=$(mktemp)
    cp "$built" "$first"
    rm -f "$built"
    rm -rf "launcher-src/target/$target/release/.fingerprint/airdress-launch-"*
    cargo build \
        --manifest-path launcher-src/Cargo.toml \
        --release --locked --target "$target"

    if cmp -s "$first" "$built"; then
        echo "  $platform is deterministic here ($(wc -c < "$built") bytes, $(sha256sum "$built" | cut -c1-16)…)"
    else
        echo "  $platform is NOT deterministic even on one machine:" >&2
        echo "    first:  $(sha256sum "$first" | cut -d' ' -f1)" >&2
        echo "    second: $(sha256sum "$built" | cut -d' ' -f1)" >&2
        fail=1
    fi
    rm -f "$first"

    # A committed binary nobody else can reproduce is worse than none.
    if [[ -f "$committed" ]]; then
        echo "  $committed is committed, and cannot be verified by anybody" >&2
        echo "    else until the C toolchain is pinned too — see this script's header" >&2
        fail=1
    fi
done

# macOS is built and compared in the release workflow, on a macOS
# runner, before signing — signing and notarization change the bytes, so
# reproducibility is checked on the pre-signing binary (FR-76). It cannot
# be checked here, and saying so is better than a check that quietly
# covers two platforms out of three.
echo "note: darwin-universal is compared in the release workflow, pre-signing"
echo "note: this proves determinism on ONE machine, not across machines"

if [[ $fail -ne 0 ]]; then
    cat >&2 <<'MSG'

Either the build is not deterministic even on one machine — which would
be a new problem, since it was on 2026-10-04 — or a launcher binary is
committed while cross-machine reproducibility is still unverifiable.
Read this script's header before doing anything about either.
MSG
    exit 1
fi
