#!/usr/bin/env python3
"""The bytes of a macOS binary with its signature taken out.

Why this exists. The committed macOS launcher carries a Developer ID
signature, and a signature cannot be reproduced: it holds a timestamp
from Apple's server. So "the committed launcher is what the source
builds" is checked on the binary WITHOUT its signature, and anybody can
do the same.

Why it does not use `codesign --remove-signature`, measured 2026-10-05:

  * codesign grows `__LINKEDIT`'s vmsize to make room for a signature
    and removing the signature does not shrink it back — two bytes;
  * and what removal leaves differs between macOS versions: on
    macos-15, a stripped Developer ID binary and the stripped
    linker-signed build of the same code differed in the universal
    header's slice size, while on macOS 26 they matched. A comparison
    that depends on which codesign ran it is not one anybody can repeat.

So this reads the Mach-O itself, the same way on any machine (Linux
included). For each slice it keeps every byte before the code signature
— the signature is always the last thing in `__LINKEDIT` — removes the
LC_CODE_SIGNATURE load command, and zeroes `__LINKEDIT`'s file size and
memory size, which grow with the signature, and pads the slice with
zeros to the 16-byte boundary codesign starts a signature on. The slices are
then written one after another, without the universal header, whose
offsets and sizes are layout, not code. Every byte of code and data, and
every other load command, is compared in full.

Usage: scripts/macho-unsigned.py <binary> <output>
"""

import struct
import sys

FAT_MAGIC = 0xCAFEBABE
MH_MAGIC_64 = 0xFEEDFACF
LC_SEGMENT_64 = 0x19
LC_CODE_SIGNATURE = 0x1D


def unsigned_slice(data: bytes) -> bytes:
    """One thin 64-bit slice, signature removed and its traces zeroed."""
    if struct.unpack_from("<I", data, 0)[0] != MH_MAGIC_64:
        sys.exit("not a 64-bit Mach-O slice")
    out = bytearray(data)
    ncmds, sizeofcmds = struct.unpack_from("<II", data, 16)
    offset = 32
    end = len(data)
    signature_cmd = None
    for _ in range(ncmds):
        cmd, size = struct.unpack_from("<II", data, offset)
        if cmd == LC_SEGMENT_64:
            name = bytes(data[offset + 8 : offset + 24]).rstrip(b"\0")
            if name == b"__LINKEDIT":
                # segment_command_64: vmsize at +32, filesize at +48.
                struct.pack_into("<Q", out, offset + 32, 0)
                struct.pack_into("<Q", out, offset + 48, 0)
        elif cmd == LC_CODE_SIGNATURE:
            # linkedit_data_command: dataoff at +8.
            end = struct.unpack_from("<I", data, offset + 8)[0]
            signature_cmd = (offset, size)
        offset += size

    # Take the command out rather than zero it: the linker signs only
    # arm64 slices on its own, so an unsigned x86_64 slice has no such
    # command at all, and signing adds one (measured: 22 load commands
    # before, 23 after). The commands after it move up, and the bytes it
    # leaves at the end of the command area become zeros, which is what
    # the linker leaves there.
    if signature_cmd is not None:
        at, size = signature_cmd
        commands_end = 32 + sizeofcmds
        out[at:commands_end] = out[at + size : commands_end] + bytes(size)
        struct.pack_into("<II", out, 16, ncmds - 1, sizeofcmds - size)
    # codesign starts the signature on a 16-byte boundary, padding with
    # zeros (measured: the x86_64 slice grew from 3,839,880 to 3,839,888
    # bytes before its signature). Pad every slice the same way, so an
    # unsigned slice and a signed one end alike.
    body = bytes(out[:end])
    return body + bytes(-len(body) % 16)


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit("usage: scripts/macho-unsigned.py <binary> <output>")
    data = open(sys.argv[1], "rb").read()
    if struct.unpack_from(">I", data, 0)[0] == FAT_MAGIC:
        count = struct.unpack_from(">I", data, 4)[0]
        slices = []
        for i in range(count):
            # fat_arch: cputype, cpusubtype, offset, size, align (big endian)
            off, size = struct.unpack_from(">II", data, 8 + i * 20 + 8)
            slices.append(unsigned_slice(data[off : off + size]))
        result = b"".join(slices)
    else:
        result = unsigned_slice(data)
    open(sys.argv[2], "wb").write(result)


if __name__ == "__main__":
    main()
