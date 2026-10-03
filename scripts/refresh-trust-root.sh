#!/usr/bin/env bash
# Refresh the embedded Sigstore trust root.
#
# The launcher verifies offline against the copy in
# `plugins/airdress/launcher/trust/trusted_root.json`, embedded into the
# binary at build time and never refreshed at run time (SPEC-133 D-30):
# a program that fetches its own idea of who to trust can be told to
# trust somebody else.
#
# So the refresh is a reviewed commit. Run this, read the diff, and say
# in the commit message what changed — a new Fulcio intermediate, a new
# Rekor shard. The diff is the review.
#
# Usage: scripts/refresh-trust-root.sh
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

TARGET=plugins/airdress/launcher/trust/trusted_root.json
SOURCE=https://raw.githubusercontent.com/sigstore/root-signing/main/targets/trusted_root.json

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT

curl -sSfL "$SOURCE" -o "$tmp"

# Refuse a file that is not the thing: a truncated download that still
# had a 200 would otherwise be committed and break every verification.
python3 - "$tmp" <<'PY'
import json, sys

doc = json.load(open(sys.argv[1]))
cas = doc.get("certificateAuthorities") or []
tlogs = doc.get("tlogs") or []
ctlogs = doc.get("ctlogs") or []
certs = sum(len(ca.get("certChain", {}).get("certificates", [])) for ca in cas)
if certs == 0:
    sys.exit("refusing: no Fulcio certificates")
if not tlogs:
    sys.exit("refusing: no Rekor logs")
if not ctlogs:
    sys.exit("refusing: no certificate-transparency logs")
print(f"{certs} Fulcio certificates, {len(tlogs)} Rekor logs, {len(ctlogs)} CT logs")
PY

if cmp -s "$tmp" "$TARGET"; then
    echo "trust root unchanged"
    exit 0
fi

cp "$tmp" "$TARGET"
echo "wrote $TARGET — read the diff before committing:"
git --no-pager diff --stat -- "$TARGET"
