#!/bin/sh
# Pick this platform's launcher and run it.
#
# Two jobs, and deliberately no third. It chooses a binary by `uname`,
# and it refuses an operating system this plugin has no launcher for
# with a sentence that says what to do instead. Everything else —
# downloading, hashes, signatures, the withdrawal list — is the
# launcher's, because all of that has to happen in one program that can
# be built reproducibly and pinned by commit.
#
# Nothing here writes to stdout: from the moment the server starts,
# stdout is the MCP protocol.
set -eu

here=$(cd "$(dirname "$0")" && pwd)

os=$(uname -s)
arch=$(uname -m)

case "$os" in
    Linux)
        case "$arch" in
            x86_64 | amd64) platform=linux-x86_64 ;;
            aarch64 | arm64) platform=linux-aarch64 ;;
            *)
                echo "airdress: no launcher for Linux on $arch." >&2
                echo "Supported: x86_64 and aarch64." >&2
                exit 1
                ;;
        esac
        ;;
    Darwin)
        platform=darwin-universal
        ;;
    *)
        # Windows and everything else: the local path needs a device
        # host and a keychain this plugin does not have there yet, and
        # pretending otherwise would fail halfway through a session.
        # Remote MCP needs none of it.
        echo "airdress: this plugin runs on Linux and macOS." >&2
        echo "On $os, connect to your airdress over remote MCP instead:" >&2
        echo "  https://airdress.co/docs/claude" >&2
        exit 1
        ;;
esac

binary="$here/$platform/airdress-launch"
if [ ! -x "$binary" ]; then
    echo "airdress: this plugin has no launcher for $platform at $binary." >&2
    echo "Reinstall the plugin, or report it: https://airdress.co/support" >&2
    exit 1
fi

exec "$binary" "$@"
