#!/usr/bin/env bash
# The release notes: how to verify the release, and what it does and does
# not claim. Printed to stdout by the release workflow's package job.
#
# Usage: scripts/release-notes.sh <version>   e.g. 0.1.0
set -euo pipefail

version="${1:?usage: $0 <version>}"
version="${version#v}"
tag="v$version"
repo="airdress-co/airdress-claude-code"

cat <<EOF2
## Airdress $version

Query your airdresses and run your functions dev loop from your editor.

**Verify before you trust.** Each \`.mcpb\` is signed by this repository's
release workflow at this tag (Sigstore keyless, recorded in Rekor). The
plugin's launcher checks the SHA-256 it pins, the certificate identity, the
Rekor inclusion proof and the withdrawal list before it runs anything. To
check by hand:

\`\`\`sh
cosign verify-blob --new-bundle-format \\
  --bundle airdress-linux-x86_64.mcpb.sigstore.json \\
  --certificate-identity "https://github.com/$repo/.github/workflows/release.yml@refs/tags/$tag" \\
  --certificate-oidc-issuer "https://token.actions.githubusercontent.com" \\
  airdress-linux-x86_64.mcpb

slsa-verifier verify-artifact airdress-linux-x86_64.mcpb \\
  --provenance-path airdress-claude-code.intoto.jsonl \\
  --source-uri github.com/$repo --source-tag $tag
\`\`\`

**Reproducible.** Every bundle was built twice, on two runner images with no
shared cache, and the two archives compared byte for byte before anything was
signed. Linux builds run in digest-pinned toolchain images. macOS is weaker:
Apple's toolchain cannot be pinned by digest, so the evidence is two runner
versions with one pinned Xcode agreeing, compared before the Developer ID
signature; with the signature removed (\`scripts/macho-unsigned.py\`) the
shipped server is that build. Bundles are stored, not compressed, because
compressed bytes depend on the compressor's version.

**Provenance.** SLSA provenance comes from slsa-github-generator's generic
generator, which signs it in an isolated reusable workflow; that project
documents Build L3 for the arrangement. One qualification: the macOS bundle
is re-packaged after notarization in a job of this workflow, so its subject
is the signed bundle, and the claim covering it is "the build that two
runners reproduced, plus Apple's signature".

The same bytes are at \`https://downloads.airdress.co/claude-code/$tag/\`,
which keeps no record of who downloaded what. GitHub keeps its own request
logs.
EOF2
