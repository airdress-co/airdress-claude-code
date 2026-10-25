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

# The agent device binary of the same airdress-cli release the server is
# built from (server-src.json). The server installs it from these pins, so
# a person never fetches it (SPEC-166 F.2). Hashed from both origins, which
# must agree with each other and with the release's own .sha256.
import hashlib, urllib.request

src = json.loads(pathlib.Path("server-src.json").read_text())
cli_tag = src["tag"]
agent_origins = [
    f"https://downloads.airdress.co/airdress-cli/{cli_tag}/",
    f"https://github.com/airdress-co/airdress-cli/releases/download/{cli_tag}/",
]

def fetch(url):
    with urllib.request.urlopen(url, timeout=120) as r:
        return r.read()

agent_files = {}
for key, name in (
    ("linux-x86_64", "airdress-agent-linux-amd64"),
    ("linux-aarch64", "airdress-agent-linux-arm64"),
    ("darwin-x86_64", "airdress-agent-darwin-amd64"),
    ("darwin-aarch64", "airdress-agent-darwin-arm64"),
):
    digests = {hashlib.sha256(fetch(o + name)).hexdigest() for o in agent_origins}
    published = fetch(agent_origins[1] + name + ".sha256").decode().split()[0].lower()
    if digests != {published}:
        sys.exit(f"{name}: the origins and the published .sha256 disagree: {digests} vs {published}")
    agent_files[key] = {"name": name, "sha256": published}

pins = {
    "version": version,
    "platforms": platforms,
    "agent": {"version": cli_tag, "origins": agent_origins, "files": agent_files},
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
print(f"  airdress-agent {cli_tag}")
for key, f in agent_files.items():
    print(f"  {key} {f['sha256']}")
PY

echo
echo "Now commit it on main, then: scripts/marketplace-pins.py pin <that commit>, and commit that."
