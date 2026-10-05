#!/usr/bin/env python3
"""Package one platform's MCP server as a reproducible `.mcpb`.

A `.mcpb` is a zip with a `manifest.json` at its root (the MCP Bundle
format, manifest 0.3). The launcher pins the SHA-256 of the WHOLE archive,
so the archive, not only the server inside it, has to come out byte for
byte the same from two independent builds. Everything a zip records about
the machine that wrote it is therefore fixed here:

  * entry order: sorted, never the order a directory listing returned;
  * timestamps: one, from SOURCE_DATE_EPOCH (the commit), for every entry;
  * permissions: 0755 for the server, 0644 for everything else, and the
    "made by" host fixed to Unix, whatever host this runs on;
  * compression: NONE. Deflate output depends on the zlib that produced
    it, and the two builds run on different runner images (and macOS has
    its own zlib), so a compressed archive could differ with identical
    contents. Stored entries cost download size and buy an archive whose
    bytes are a function of its contents alone. The README says so.

Usage:
  package-mcpb.py --platform linux-x86_64 --server path/to/airdress-mcp \\
                  --version 0.1.0 --out dist/airdress-linux-x86_64.mcpb
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import sys
import time
import zipfile

PLATFORMS = ("linux-x86_64", "linux-aarch64", "darwin-universal")
SERVER = "airdress-mcp"

# The MCP Bundle manifest version this file conforms to. Read from the
# format's own specification (modelcontextprotocol/mcpb MANIFEST.md,
# "Current version: 0.3"), not assumed.
MANIFEST_VERSION = "0.3"


def manifest(version: str, platform: str) -> bytes:
    os_name = "darwin" if platform.startswith("darwin") else "linux"
    doc = {
        "manifest_version": MANIFEST_VERSION,
        "name": "airdress",
        "display_name": "Airdress",
        "version": version,
        "description": "Query your airdresses and run your functions dev loop.",
        "author": {"name": "Airdress", "url": "https://airdress.co"},
        "repository": {
            "type": "git",
            "url": "https://github.com/airdress-co/airdress-claude-code.git",
        },
        "homepage": "https://airdress.co",
        "license": "Apache-2.0",
        "server": {
            "type": "binary",
            "entry_point": SERVER,
            "mcp_config": {
                "command": "${__dirname}/" + SERVER,
                "args": [],
            },
        },
        "compatibility": {"platforms": [os_name]},
    }
    # Sorted keys and a trailing newline: the manifest is part of the
    # hashed archive, so its serialization is fixed too.
    return (json.dumps(doc, indent=2, sort_keys=True) + "\n").encode()


def readme(version: str, platform: str) -> bytes:
    return f"""# airdress-mcp {version} ({platform})

The Airdress MCP server. Built from airdress-cli, reproducibly, by the
release workflow of https://github.com/airdress-co/airdress-claude-code.
The Sigstore bundle beside this archive names that workflow and that tag,
and the plugin's launcher refuses anything else.

It contacts your hub, your airdresses' operators, and nothing else.

Source: https://github.com/airdress-co/airdress-cli
""".encode()


def zip_time(epoch: int) -> tuple[int, int, int, int, int, int]:
    # A zip cannot represent a time before 1980; clamp rather than fail,
    # and always in UTC so the builder's timezone never reaches the bytes.
    epoch = max(epoch, 315532800)
    t = time.gmtime(epoch)
    return (t.tm_year, t.tm_mon, t.tm_mday, t.tm_hour, t.tm_min, t.tm_sec)


def package(
    *,
    platform: str,
    server: pathlib.Path,
    version: str,
    out: pathlib.Path,
    epoch: int,
    extra: dict[str, bytes],
) -> None:
    if platform not in PLATFORMS:
        raise SystemExit(f"unknown platform {platform!r}; expected one of {PLATFORMS}")
    entries: dict[str, tuple[bytes, int]] = {
        SERVER: (server.read_bytes(), 0o755),
        "manifest.json": (manifest(version, platform), 0o644),
        "README.md": (readme(version, platform), 0o644),
    }
    for name, body in extra.items():
        entries[name] = (body, 0o644)

    date_time = zip_time(epoch)
    out.parent.mkdir(parents=True, exist_ok=True)
    tmp = out.with_suffix(out.suffix + ".partial")
    with zipfile.ZipFile(tmp, "w", compression=zipfile.ZIP_STORED) as zf:
        for name in sorted(entries):
            body, mode = entries[name]
            info = zipfile.ZipInfo(name, date_time=date_time)
            info.compress_type = zipfile.ZIP_STORED
            info.create_system = 3  # Unix, whatever host wrote it
            info.external_attr = (0o100000 | mode) << 16  # regular file
            info.extra = b""
            zf.writestr(info, body)
    os.replace(tmp, out)


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--platform", required=True, choices=PLATFORMS)
    p.add_argument("--server", required=True, type=pathlib.Path)
    p.add_argument("--version", required=True)
    p.add_argument("--out", required=True, type=pathlib.Path)
    p.add_argument(
        "--include",
        action="append",
        default=[],
        type=pathlib.Path,
        help="another file for the archive root (LICENSE, NOTICE)",
    )
    args = p.parse_args(argv)

    epoch_s = os.environ.get("SOURCE_DATE_EPOCH")
    if not epoch_s:
        print("SOURCE_DATE_EPOCH is not set: the archive would carry this moment", file=sys.stderr)
        return 1
    extra = {f.name: f.read_bytes() for f in args.include}
    package(
        platform=args.platform,
        server=args.server,
        version=args.version.removeprefix("v"),
        out=args.out,
        epoch=int(epoch_s),
        extra=extra,
    )
    print(f"{args.out} {args.out.stat().st_size} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
