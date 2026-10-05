#!/usr/bin/env python3
"""Pin the marketplace entry to a commit, and check that it is pinned.

What a user installs is decided in two places, and both are pins:

  * the marketplace entry names the plugin by repository, subdirectory
    AND full commit SHA (a `git-subdir` source: a `github` source has no
    `path`, and the plugin is `plugins/airdress`), so the launcher, its
    trust root and `pins.json` are exactly the bytes of that commit — not
    whatever `main` holds when somebody installs. `ref` is `main`, where
    the pins commit lives: GitHub fetches the SHA directly, and a host
    that cannot still finds it reachable from `main`;
  * that commit's `launcher/pins.json` names every bundle by SHA-256, its
    two origin URLs and the signing identity.

A relative `./plugins/airdress` source pins nothing: it follows the
marketplace's own checkout. It is allowed only while no release exists
(no `pins.json` committed) — a checkout between releases cannot start a
server anyway, and says so.

  marketplace-pins.py pin <commit>   set the entry to that commit (after
                                     the pins commit has been pushed)
  marketplace-pins.py check          the CI check (exit 1 on any failure)
"""

from __future__ import annotations

import json
import pathlib
import re
import subprocess
import sys

REPO = "airdress-co/airdress-claude-code"
MARKET = pathlib.Path(".claude-plugin/marketplace.json")
SUBDIR = "plugins/airdress"
PINS = "plugins/airdress/launcher/pins.json"
PLUGIN = "plugins/airdress/.claude-plugin/plugin.json"
PLATFORMS = ("linux-x86_64", "linux-aarch64", "darwin-universal")
SHA1 = re.compile(r"^[0-9a-f]{40}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
ORIGINS = {
    "cdn": "https://downloads.airdress.co/claude-code/",
    "github": f"https://github.com/{REPO}/releases/download/",
}
ISSUER = "https://token.actions.githubusercontent.com"


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args], check=True, capture_output=True, text=True
    ).stdout


def show(commit: str, path: str) -> str | None:
    r = subprocess.run(
        ["git", "show", f"{commit}:{path}"], capture_output=True, text=True
    )
    return r.stdout if r.returncode == 0 else None


def check_pins(pins: dict, plugin_version: str) -> list[str]:
    """Every platform pinned by SHA-256, both origins, the exact identity."""
    errors = []
    version = pins.get("version")
    if version != plugin_version:
        errors.append(f"pins.json is for {version}, plugin.json says {plugin_version}")
    tag = f"v{version}"
    identity = (
        f"https://github.com/{REPO}/.github/workflows/release.yml@refs/tags/{tag}"
    )
    platforms = pins.get("platforms", {})
    if set(platforms) != set(PLATFORMS):
        errors.append(f"pins.json pins {sorted(platforms)}, expected {sorted(PLATFORMS)}")
    for name, p in sorted(platforms.items()):
        if not SHA256.match(p.get("sha256", "")):
            errors.append(f"{name}: no SHA-256 pin")
        for kind in ("bundle", "sigstore"):
            urls = p.get(kind, {})
            for origin, prefix in ORIGINS.items():
                url = urls.get(origin, "")
                if not url.startswith(prefix + tag + "/"):
                    errors.append(f"{name}: {kind} {origin} URL {url!r} is not under {prefix}{tag}/")
        if p.get("cert_identity") != identity:
            errors.append(f"{name}: identity {p.get('cert_identity')!r}, expected {identity!r}")
        if p.get("cert_issuer") != ISSUER:
            errors.append(f"{name}: issuer {p.get('cert_issuer')!r}")
    return errors


def check() -> int:
    market = json.loads(MARKET.read_text())
    entries = market.get("plugins", [])
    errors: list[str] = []
    if len(entries) != 1:
        errors.append(f"expected one marketplace entry, found {len(entries)}")
    entry = entries[0] if entries else {}
    source = entry.get("source")
    released = pathlib.Path(PINS).exists()

    if isinstance(source, str):
        if released:
            errors.append(
                "the entry is a relative path, which pins nothing, but a release "
                f"is pinned in {PINS}: run scripts/marketplace-pins.py pin <commit>"
            )
        else:
            print("no release pinned yet: the relative source is allowed until one is")
    elif isinstance(source, dict):
        if (
            source.get("source") != "git-subdir"
            or source.get("url") != REPO
            or source.get("path") != SUBDIR
        ):
            errors.append(f"the entry must be git-subdir {REPO} {SUBDIR}, found {source}")
        sha = source.get("sha", "")
        if not SHA1.match(sha):
            errors.append(f"the entry's sha {sha!r} is not a full commit")
        else:
            try:
                git("cat-file", "-e", f"{sha}^{{commit}}")
            except subprocess.CalledProcessError:
                errors.append(f"commit {sha} is not in this repository's history")
            else:
                pins_text = show(sha, PINS)
                plugin_text = show(sha, PLUGIN)
                if pins_text is None:
                    errors.append(f"commit {sha} carries no {PINS}: it pins no bundle")
                elif plugin_text is None:
                    errors.append(f"commit {sha} carries no {PLUGIN}")
                else:
                    pv = json.loads(plugin_text)["version"]
                    errors += [f"at {sha[:12]}: {e}" for e in check_pins(json.loads(pins_text), pv)]
                    if source.get("ref") != "main":
                        errors.append(f"ref {source.get('ref')!r}: the pins commit lives on main")
                    pins_doc = json.loads(pins_text)
                    want = {k: v["sha256"] for k, v in pins_doc.get("platforms", {}).items()}
                    got = entry.get("metadata", {}).get("bundles_sha256")
                    if got != want:
                        errors.append(
                            f"metadata.bundles_sha256 {got} does not repeat the pinned "
                            f"commit's SHA-256 pins {want}"
                        )
    else:
        errors.append(f"the entry has no usable source: {source!r}")

    if released:
        pv = json.loads(pathlib.Path(PLUGIN).read_text())["version"]
        errors += check_pins(json.loads(pathlib.Path(PINS).read_text()), pv)

    for e in errors:
        print(f"::error::{e}")
    if errors:
        return 1
    print("marketplace entry and pins are consistent")
    return 0


def pin(commit: str) -> int:
    sha = git("rev-parse", "--verify", f"{commit}^{{commit}}").strip()
    pins_text = show(sha, PINS)
    if pins_text is None:
        print(f"{sha} carries no {PINS}; pin the commit that added the pins", file=sys.stderr)
        return 1
    version = json.loads(pins_text)["version"]
    market = json.loads(MARKET.read_text())
    (entry,) = market["plugins"]
    entry["source"] = {
        "source": "git-subdir",
        "url": REPO,
        "path": SUBDIR,
        "ref": "main",
        "sha": sha,
    }
    # Claude Code does not read `metadata`; a person auditing the entry
    # does. The launcher enforces these hashes from the pinned commit's
    # pins.json, and the check holds the two equal.
    entry["metadata"] = {
        "bundles_sha256": {
            k: v["sha256"] for k, v in json.loads(pins_text)["platforms"].items()
        }
    }
    market.setdefault("metadata", {})["version"] = version
    MARKET.write_text(json.dumps(market, indent=2) + "\n")
    print(f"marketplace entry pinned to {sha} (v{version})")
    return 0


def main(argv: list[str]) -> int:
    if argv[:1] == ["check"]:
        return check()
    if len(argv) == 2 and argv[0] == "pin":
        return pin(argv[1])
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
