#!/usr/bin/env bash
# Build the Linux launchers inside a digest-pinned toolchain image.
#
# Why a container, when `check-launcher-reproducible.sh` already builds
# twice and compares. That script proves determinism on ONE machine with
# ONE toolchain. FR-69 asks for more: that somebody else can rebuild the
# committed binary and get the same bytes. The obstacle is C — sigstore's
# verification reaches `aws-lc-sys` through three independent routes
# (rustls, rustls-webpki, and the launcher's own `ureq`), and a C
# library's object code depends on the builder's compiler. Pinning rustc
# is not enough; `cc` has to be pinned too, and a digest-pinned image is
# how.
#
# Measured 2026-10-04 on a second machine (Docker 28.5.2): `aarch64-unknown-linux-musl` builds
# here and had never built anywhere before. `aws-lc-sys` wants cmake and
# libclang on top of a musl cross compiler, and the release workflow was
# installing `gcc-aarch64-linux-gnu` + `musl-tools`, which provides none
# of the three — which is why that build failed inside its build script
# rather than at link time, and why the failure read as "ARM is hard"
# rather than "three packages are missing".
#
# What this does and does not prove:
#
#   * It DOES prove: these bytes are what this exact toolchain image
#     produces from this source, on any machine that can run it. Two
#     machines running the same digest agree.
#   * It does NOT prove: that the toolchain itself is derivable from
#     source. The claim rests on the registry still serving that digest.
#     Say that plainly rather than implying bit-for-bit provenance from
#     nothing.
#   * macOS is NOT covered. Apple's toolchain cannot be digest-pinned, so
#     `darwin-universal` keeps the evidence the server already uses: two
#     passes on two different runner versions. That asymmetry is real and
#     is stated in the README rather than left to be discovered.
#
# Usage:
#   scripts/build-launchers.sh                 # build each platform twice, compare
#   scripts/build-launchers.sh --write         # and install them under plugins/
#   PLATFORMS="linux-aarch64" scripts/build-launchers.sh
#   PASSES=1 scripts/build-launchers.sh --write
#
# `PASSES=1` skips the second local pass, and on a pull request that is
# the right choice rather than a saving: a hash that matches the one
# another machine recorded already proves the build reproduced, and
# proves it across machines, which two passes on one runner never can.
# Two passes stay the default for a release and for the day a pin moves,
# where there is no recorded hash to match yet.
#
# Needs a working container runtime. It does not install one, and it does
# not quietly skip when there is none: a reproducibility check that
# reports success without having built anything is the failure mode this
# whole exercise exists to avoid.
set -euo pipefail

# Say which of the two things is wrong rather than letting an empty
# `cd` do it: an unset PATH on a build host produced
# "cd: null directory", which names neither git nor the repository.
command -v git >/dev/null 2>&1 || { echo "git is not on PATH" >&2; exit 1; }
ROOT_DIR=$(git rev-parse --show-toplevel 2>/dev/null) || {
    echo "not inside a git repository, so there is no tree to build" >&2
    exit 1
}
cd "$ROOT_DIR"

WRITE=0
[[ "${1:-}" == "--write" ]] && WRITE=1

RUNTIME=""
for candidate in docker podman nerdctl; do
    if command -v "$candidate" >/dev/null 2>&1; then
        RUNTIME="$candidate"
        break
    fi
done
if [[ -z "$RUNTIME" ]]; then
    cat >&2 <<'MSG'
No container runtime (docker, podman or nerdctl) on this machine, so the
launcher cannot be built reproducibly here.

This is not a reason to build it without one: a binary produced by
whatever `cc` happens to be installed is exactly the thing that made the
committed launcher differ between a workstation and a CI runner.

Run it where a runtime exists, or let the `launchers` CI job do it.
MSG
    exit 1
fi

IMAGES="launcher-src/toolchain-images.json"
[[ -f "$IMAGES" ]] || { echo "missing $IMAGES" >&2; exit 1; }

mapfile -t WANTED < <(
    if [[ -n "${PLATFORMS:-}" ]]; then
        # Deliberate split: PLATFORMS is a space-separated list.
        read -ra requested <<<"${PLATFORMS}"
        printf '%s\n' "${requested[@]}"
    else
        python3 -c 'import json,sys; print("\n".join(json.load(open(sys.argv[1]))["platforms"]))' "$IMAGES"
    fi
)

# The commit decides the timestamp, so rebuilding an unchanged tree gives
# unchanged bytes. A dirty tree has no commit to speak for it, so say so
# rather than stamping a time that nothing records.
if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "note: the tree is dirty, so these bytes correspond to no commit" >&2
fi
EPOCH=$(git log -1 --pretty=%ct 2>/dev/null || echo 1700000000)

WORK=$(mktemp -d)

# The container builds as root and writes into this host directory, so
# without handing ownership back an unprivileged caller cannot delete it.
# That is not a tidiness problem: the first version left `rm -rf` as the
# EXIT trap's last command, so its "Permission denied" became the
# script's exit status and a run whose hashes BOTH matched reported
# failure. The chown happens inside each container, below; the trap now
# reports a cleanup it could not do instead of failing over it, because
# leftover scratch is worth a warning and never worth a false red.
cleanup() {
    local code=$?
    rm -rf "$WORK" 2>/dev/null || echo "note: could not remove $WORK" >&2
    return "$code"
}
trap cleanup EXIT

status=0
for platform in "${WANTED[@]}"; do
    target=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["platforms"][sys.argv[2]]["target"])' "$IMAGES" "$platform")
    image=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["platforms"][sys.argv[2]]["image"])' "$IMAGES" "$platform")
    case "$image" in
        *@sha256:*) ;;
        *)
            echo "$platform: the image is not pinned by digest ($image)" >&2
            echo "  A tag moves, and the launcher's bytes move with it." >&2
            status=1
            continue
            ;;
    esac

    echo "== $platform ($target)"
    echo "   image $image"
    hashes=()
    passes=(A B)
    [[ "${PASSES:-2}" == "1" ]] && passes=(A)
    for pass in "${passes[@]}"; do
        out="$WORK/$platform-$pass"
        mkdir -p "$out"
        # The source is copied INTO the container so the remapped prefix
        # is the same wherever the checkout lives on the host.
        "$RUNTIME" run --rm \
            -v "$PWD":/src:ro \
            -v "$out":/out \
            -e CARGO_TARGET_DIR=/out/target \
            -e CARGO_INCREMENTAL=0 \
            -e SOURCE_DATE_EPOCH="$EPOCH" \
            -e HOST_UID="$(id -u)" \
            -e HOST_GID="$(id -g)" \
            "$image" bash -lc "
                set -euo pipefail
                mkdir -p /work && cp -r /src/. /work/
                cd /work/launcher-src
                export RUSTFLAGS='--remap-path-prefix=/work=/plugin --remap-path-prefix=/root/.cargo=/cargo'
                cargo build --release --locked --target $target >/dev/null
                cp /out/target/$target/release/airdress-launch /out/airdress-launch
                # Hand the artefacts back, or the caller cannot clean up
                # after us. Not --user on the run: these images expect to
                # be root (CARGO_HOME lives under /root).
                chown -R \"\$HOST_UID:\$HOST_GID\" /out
            " || { echo "   pass $pass FAILED" >&2; status=1; continue 2; }
        hashes+=("$(sha256sum "$out/airdress-launch" | cut -d' ' -f1)")
        echo "   pass $pass  ${hashes[-1]}"
    done

    if [[ ${#hashes[@]} -gt 1 && "${hashes[0]}" != "${hashes[1]}" ]]; then
        echo "   NOT DETERMINISTIC — two passes in one image disagreed" >&2
        status=1
        continue
    fi
    [[ ${#hashes[@]} -gt 1 ]] && echo "   deterministic"

    if [[ $WRITE -eq 1 ]]; then
        dest="plugins/airdress/launcher/$platform"
        mkdir -p "$dest"
        install -m 0755 "$WORK/$platform-A/airdress-launch" "$dest/airdress-launch"
        echo "   wrote $dest/airdress-launch"
    fi
done

echo
if [[ $status -ne 0 ]]; then
    echo "at least one platform did not build reproducibly" >&2
    exit 1
fi
echo "every requested platform built reproducibly in its pinned image"
