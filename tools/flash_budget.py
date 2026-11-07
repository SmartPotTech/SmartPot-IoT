"""Checks how much flash the firmware image needs against the factory partition.

Wokwi (and the default one-megabyte factory partition) refuses images above 1 MiB, and the bootloader then
restarts in a loop before reaching `main`. This script sums every allocated section that carries data in the
ELF (code, read-only data, initialised RAM copied from flash) and fails when the budget is exceeded.

Usage:
    python tools/flash_budget.py target/xtensa-esp32-none-elf/debug/smartpot-iot [--limit 1048576] [--warn 0.9]
"""

import argparse
import struct
import sys

SHT_PROGBITS = 1
SHF_ALLOC = 0x2


def flash_sections(path):
    with open(path, "rb") as elf:
        data = elf.read()
    if data[:4] != b"\x7fELF" or data[4] != 1:
        raise SystemExit("not a 32-bit ELF file: " + path)
    endian = "<" if data[5] == 1 else ">"
    shoff = struct.unpack_from(endian + "I", data, 0x20)[0]
    shentsize, shnum, shstrndx = struct.unpack_from(endian + "HHH", data, 0x2E)

    headers = [struct.unpack_from(endian + "IIIIIIIIII", data, shoff + i * shentsize) for i in range(shnum)]
    names_offset = headers[shstrndx][4]

    def name(offset):
        end = data.index(b"\0", names_offset + offset)
        return data[names_offset + offset:end].decode()

    for header in headers:
        sh_name, sh_type, sh_flags, _addr, _offset, sh_size = header[:6]
        if sh_type == SHT_PROGBITS and sh_flags & SHF_ALLOC and sh_size:
            yield name(sh_name), sh_size


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("elf")
    parser.add_argument("--limit", type=int, default=1024 * 1024, help="factory partition size in bytes")
    parser.add_argument("--warn", type=float, default=0.9, help="usage ratio that prints a warning")
    args = parser.parse_args()

    sections = sorted(flash_sections(args.elf), key=lambda item: -item[1])
    total = sum(size for _, size in sections)
    for section, size in sections:
        print(f"{section:<24} {size:>9}")
    ratio = total / args.limit
    print(f"{'total':<24} {total:>9}  ({ratio:.1%} of {args.limit} bytes, {args.limit - total} free)")

    if total > args.limit:
        sys.exit("ERROR: the firmware does not fit in the factory partition")
    if ratio > args.warn:
        print(f"::warning::flash usage at {ratio:.1%} of the factory partition")


if __name__ == "__main__":
    main()
