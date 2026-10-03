#!/usr/bin/env bash
# The committed launcher binaries are the bytes this source builds.
#
# The marketplace pins a commit of this repository, and that commit
# carries the launcher binaries. That pin is the whole reason anybody
# should trust them — so this has to be provable, not assumed: rebuild
# from `launcher-src/` with the release controls and compare byte for
# byte (SPEC-133 FR-74).
#
# The controls, and what each is for:
#
#   --locked               the lockfile decides every version
#   SOURCE_DATE_EPOCH      no build timestamp in the binary
#   --remap-path-prefix    no absolute path from this machine in it
#   musl, static           no dependency on the builder's libc
#
# Usage: scripts/check-launcher-reproducible.sh [--write]
#   --write  rebuild and update the committed binaries (for a change to
#            the launcher's source, in the same commit)
#
# `PLATFORMS="linux-x86_64"` limits it, for a workstation without a
# cross-linker. CI never sets it: the point there is both platforms.
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

    if [[ ! -f "$committed" ]]; then
        echo "  $committed is missing: run $0 --write" >&2
        fail=1
        continue
    fi
    if cmp -s "$built" "$committed"; then
        echo "  $platform matches ($(wc -c < "$committed") bytes)"
    else
        echo "  $platform DIFFERS from a rebuild:" >&2
        echo "    committed: $(sha256sum "$committed" | cut -d' ' -f1)" >&2
        echo "    rebuilt:   $(sha256sum "$built" | cut -d' ' -f1)" >&2
        fail=1
    fi
done

# macOS is built and compared in the release workflow, on a macOS
# runner, before signing — signing and notarization change the bytes, so
# reproducibility is checked on the pre-signing binary (FR-76). It cannot
# be checked here, and saying so is better than a check that quietly
# covers two platforms out of three.
echo "note: darwin-universal is compared in the release workflow, pre-signing"

if [[ $fail -ne 0 ]]; then
    cat >&2 <<'MSG'

A committed launcher does not match what its source builds. Either the
source changed without the binaries being rebuilt — run this with
--write, in the same commit — or something else is in them.
MSG
    exit 1
fi
