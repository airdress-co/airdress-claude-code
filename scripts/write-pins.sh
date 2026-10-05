#!/usr/bin/env bash
# Write `launcher/pins.json` for a published release.
#
# Run AFTER the release exists: the pins name hashes of artifacts, so
# they cannot be written before those artifacts are built. That is why a
# release is two commits — the tag, then the pins — and not one.
#
# Usage: scripts/write-pins.sh <version>   e.g. scripts/write-pins.sh 0.1.0
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

version="${1:?usage: $0 <version>}"
version="${version#v}"
sums="dist/SHA256SUMS"
[ -f "$sums" ] || { echo "no $sums: run this where the release was built" >&2; exit 1; }

python3 - "$version" <<'PY'
import json, pathlib, sys

version = sys.argv[1]
tag = f"v{version}"
cdn = f"https://downloads.airdress.co/claude-code/{tag}"
gh = f"https://github.com/airdress-co/airdress-claude-code/releases/download/{tag}"
identity = (
    "https://github.com/airdress-co/airdress-claude-code/.github/workflows/release.yml"
    f"@refs/tags/{tag}"
)
issuer = "https://token.actions.githubusercontent.com"

sums = {}
for line in pathlib.Path("dist/SHA256SUMS").read_text().splitlines():
    digest, name = line.split()
    sums[name.lstrip("*")] = digest

platforms = {}
for platform in ("linux-x86_64", "linux-aarch64", "darwin-universal"):
    artifact = f"airdress-{platform}.mcpb"
    if artifact not in sums:
        sys.exit(f"{artifact} is not in SHA256SUMS")
    platforms[platform] = {
        "sha256": sums[artifact],
        "bundle": {"cdn": f"{cdn}/{artifact}", "github": f"{gh}/{artifact}"},
        "sigstore": {
            "cdn": f"{cdn}/{artifact}.sigstore.json",
            "github": f"{gh}/{artifact}.sigstore.json",
        },
        "cert_identity": identity,
        "cert_issuer": issuer,
    }

pins = {
    "version": version,
    "platforms": platforms,
    "denylist": {
        "list": {
            "cdn": "https://downloads.airdress.co/claude-code/denylist.json",
            "github": "https://github.com/airdress-co/airdress-claude-code/raw/denylist/denylist.json",
        },
        "sigstore": {
            "cdn": "https://downloads.airdress.co/claude-code/denylist.json.sigstore.json",
            "github": "https://github.com/airdress-co/airdress-claude-code/raw/denylist/denylist.json.sigstore.json",
        },
        "cert_identity": (
            "https://github.com/airdress-co/airdress-claude-code/.github/workflows/denylist.yml"
            "@refs/heads/main"
        ),
        "cert_issuer": issuer,
    },
}

out = pathlib.Path("plugins/airdress/launcher/pins.json")
out.write_text(json.dumps(pins, indent=2) + "\n")
print(f"wrote {out} for {tag}")
for platform, p in platforms.items():
    print(f"  {platform} {p['sha256']}")
PY

echo
echo "Now commit it on main, then: scripts/marketplace-pins.py pin <that commit>, and commit that."
