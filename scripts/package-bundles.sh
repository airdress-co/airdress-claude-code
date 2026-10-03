#!/usr/bin/env bash
# Package one `.mcpb` per platform from the built servers.
#
# A `.mcpb` is a zip. The launcher reads exactly one thing out of it —
# `airdress-mcp` at the root — and records what that file hashes to, so
# a cached bundle can be re-checked without the archive. Everything else
# in it is for a person who opens it.
#
# Usage: scripts/package-bundles.sh   (after the build jobs' artifacts
#        are downloaded into builds/)
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

mkdir -p dist
version=$(python3 -c "import json;print(json.load(open('plugins/airdress/.claude-plugin/plugin.json'))['version'])")

for platform in linux-x86_64 linux-aarch64 darwin-universal; do
    server="builds/server-$platform-A/airdress-mcp"
    [ -f "$server" ] || { echo "missing $server" >&2; exit 1; }

    staging=$(mktemp -d)
    install -m 0755 "$server" "$staging/airdress-mcp"
    cp LICENSE NOTICE "$staging/"
    cat > "$staging/README.md" <<EOF
# airdress-mcp $version ($platform)

The Airdress MCP server. Built from airdress-cli, reproducibly, by the
release workflow of https://github.com/airdress-co/airdress-claude-code — the
signature beside this bundle names that workflow and that tag, and the
plugin's launcher refuses anything else.

It contacts your hub, your airdresses' operators, and nothing else.

Source: https://github.com/airdress-co/airdress-cli
EOF

    # `-X` drops the extra attributes that differ between runners, so a
    # bundle built twice is the same bundle.
    (cd "$staging" && zip -qrX "$OLDPWD/dist/airdress-$platform.mcpb" .)
    rm -rf "$staging"
    echo "dist/airdress-$platform.mcpb $(wc -c < "dist/airdress-$platform.mcpb") bytes"
done

cat > dist/NOTES.md <<EOF
## Airdress $version

Query your airdresses and run your functions dev loop from your editor.

**Verify before you trust.** Every bundle is signed by this repository's
release workflow at this tag, recorded in Rekor, and the plugin's
launcher checks the hash, the certificate identity, the inclusion proof
and the withdrawal list before it runs anything:

\`\`\`sh
cosign verify-blob \\
  --bundle airdress-linux-x86_64.mcpb.sigstore.json \\
  --certificate-identity "https://github.com/airdress-co/airdress-claude-code/.github/workflows/release.yml@refs/tags/$version" \\
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \\
  airdress-linux-x86_64.mcpb
\`\`\`

The same bytes are at
\`https://downloads.airdress.co/claude-code/v$version/\`, which keeps no
record of who downloaded what. GitHub keeps its own request logs.

SLSA provenance is attached. The level actually reached by this
arrangement — a build split across Linux and macOS runners, with signing
secrets in the macOS job — is recorded in the release's own notes rather
than asserted.
EOF
echo "wrote dist/NOTES.md"
