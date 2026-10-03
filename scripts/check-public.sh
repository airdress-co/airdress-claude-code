#!/usr/bin/env bash
# This repository is public from its first commit.
#
# Nothing in it may name our internal estate: the GCP project, a Secret
# Manager entry, a VM, a jump host, a private address, the QA airdress or
# a test account. The scan runs over the whole history, not the working
# tree, because publishing a repository publishes its history — and this
# repository is new, so there is no inherited history to argue about.
#
# This file holds the pattern list, so every version of this file is
# skipped by blob hash. That is exact: a scan that filtered its own
# matches out of its output would also filter a real one that happened
# to look like a list entry.
#
# Usage: scripts/check-public.sh
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

python3 - <<'PY'
import re
import subprocess
import sys

SELF = "scripts/check-public.sh"

PATTERNS = [
    (r"airdress-co-ops", "the GCP project"),
    (r"airdress-tfstate", "the state bucket"),
    (r"qa-device", "the QA airdress"),
    (r"ipv6-operator", "a fleet host name"),
    (r"synthetic-login", "a test account"),
    (r"billing-internal-bearer", "a secret name"),
    (r"operator-ingress-bearer", "a secret name"),
    (r"functions-signer-seed", "a secret name"),
    (r"100\.(?:6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7])\.\d+\.\d+", "a tailnet address"),
    (r"10\.\d+\.\d+\.\d+", "a private address"),
    (r"192\.168\.\d+\.\d+", "a private address"),
    (r"ai-nas-0", "an internal host"),
]


def run(*args):
    return subprocess.run(args, capture_output=True, text=True, check=True).stdout


def own_blobs():
    """Every version of this script, by blob hash."""
    blobs = set()
    try:
        commits = run("git", "log", "--all", "--format=%H", "--", SELF).split()
    except subprocess.CalledProcessError:
        commits = []
    for commit in commits:
        try:
            blobs.add(run("git", "rev-parse", f"{commit}:{SELF}").strip())
        except subprocess.CalledProcessError:
            pass
    # And the working-tree copy, which may not be committed yet.
    try:
        blobs.add(run("git", "hash-object", SELF).strip())
    except subprocess.CalledProcessError:
        pass
    return blobs


skip = own_blobs()

# Every object in every reachable commit, once.
listing = run("git", "rev-list", "--all", "--objects")
oids = sorted({line.split()[0] for line in listing.splitlines() if line.strip()})
if not oids:
    print("public-hygiene: nothing committed yet")
    raise SystemExit(0)

kinds = subprocess.run(
    ["git", "cat-file", "--batch-check=%(objectname) %(objecttype)"],
    input="\n".join(oids),
    capture_output=True,
    text=True,
    check=True,
).stdout

failures = []
for line in kinds.splitlines():
    parts = line.split()
    if len(parts) != 2 or parts[1] != "blob" or parts[0] in skip:
        continue
    oid = parts[0]
    body = subprocess.run(
        ["git", "cat-file", "blob", oid], capture_output=True, check=True
    ).stdout.decode("utf-8", errors="ignore")
    for pattern, what in PATTERNS:
        hit = re.search(pattern, body)
        if hit:
            failures.append((what, hit.group(0), oid))

if failures:
    for what, text, oid in failures:
        print(
            f"public-hygiene: found {what} ({text}) in blob {oid[:12]}",
            file=sys.stderr,
        )
    print(file=sys.stderr)
    print("Nothing in a public repository may name our internal estate.", file=sys.stderr)
    sys.exit(1)
print("public-hygiene: clean")
PY
