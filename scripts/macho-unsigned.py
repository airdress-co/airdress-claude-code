#!/usr/bin/env python3
"""The bytes of a macOS binary with its signature taken out.

Why this exists. The committed macOS launcher carries a Developer ID
signature, and a signature cannot be reproduced: it holds a timestamp
from Apple's server. So "the committed launcher is what the source
builds" is checked on the binary WITHOUT its signature, and anybody can
do the same.

`codesign --remove-signature` alone is not enough, measured 2026-10-05:
codesign grows `__LINKEDIT`'s vmsize to make room for the signature
(0xC000 to 0x18000 on the x86_64 slice) and removing the signature does
not shrink it back. A linker-signed build and the same build after
signing, both stripped, differed in exactly those two bytes. So this
zeroes that one field as well, and nothing else. vmsize is the size of
the segment in memory, not code or data; the file's contents are
compared in full.

If a future codesign changes something more, the signing workflow's own
check — that this form of the signed binary equals this form of the
unsigned one — fails, which is where it should be found.

Usage: scripts/macho-unsigned.py <binary> <output>
Needs macOS (codesign).
"""

import shutil
import struct
import subprocess
import sys

FAT_MAGIC = 0xCAFEBABE
MH_MAGIC_64 = 0xFEEDFACF
LC_SEGMENT_64 = 0x19


def zero_linkedit_vmsize(data: bytearray, base: int) -> None:
    magic = struct.unpack_from("<I", data, base)[0]
    if magic != MH_MAGIC_64:
        sys.exit(f"not a 64-bit Mach-O slice at offset {base}")
    ncmds = struct.unpack_from("<I", data, base + 16)[0]
    offset = base + 32
    for _ in range(ncmds):
        cmd, size = struct.unpack_from("<II", data, offset)
        if cmd == LC_SEGMENT_64:
            name = bytes(data[offset + 8 : offset + 24]).rstrip(b"\0")
            if name == b"__LINKEDIT":
                struct.pack_into("<Q", data, offset + 32, 0)
                return
        offset += size
    sys.exit(f"no __LINKEDIT segment in the slice at offset {base}")


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(__doc__.strip().splitlines()[-2])
    source, output = sys.argv[1], sys.argv[2]
    shutil.copyfile(source, output)
    subprocess.run(["codesign", "--remove-signature", output], check=True)

    data = bytearray(open(output, "rb").read())
    if struct.unpack_from(">I", data, 0)[0] == FAT_MAGIC:
        count = struct.unpack_from(">I", data, 4)[0]
        for i in range(count):
            # fat_arch: cputype, cpusubtype, offset, size, align (big endian)
            slice_offset = struct.unpack_from(">I", data, 8 + i * 20 + 8)[0]
            zero_linkedit_vmsize(data, slice_offset)
    else:
        zero_linkedit_vmsize(data, 0)
    open(output, "wb").write(data)


if __name__ == "__main__":
    main()
