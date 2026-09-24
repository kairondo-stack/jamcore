#!/usr/bin/env python3
"""Liest MyImageShm (POSIX-Shm) und gibt Header + Prüfsumme der Pixeldaten aus.

Aufruf: shm_dump.py [pfad]   (Default: /dev/shm/MyImageShm)
"""
import struct
import sys


def fnv1a(data):
    h = 0xCBF29CE484222325
    for b in data:
        h ^= b
        h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def main():
    path = sys.argv[1] if len(sys.argv) > 1 else "/dev/shm/MyImageShm"
    with open(path, "rb") as f:
        raw = f.read()
    if len(raw) < 36:
        print("Segment zu klein")
        sys.exit(1)
    (width, height, pitch, fmt, rmask, gmask, bmask, amask, bpp) = struct.unpack(
        "<iiiIIIIIi", raw[:36]
    )
    px = raw[36:36 + pitch * height] if pitch > 0 and height > 0 else b""
    print(
        f"width={width} height={height} pitch={pitch} format=0x{fmt:08x} "
        f"bpp={bpp} rmask=0x{rmask:08x} gmask=0x{gmask:08x} bmask=0x{bmask:08x} "
        f"amask=0x{amask:08x} pixels={len(px)} checksum={fnv1a(px):016x}"
    )
    if len(px) >= 4:
        print(f"pixel0={px[0]},{px[1]},{px[2]},{px[3]}")


if __name__ == "__main__":
    main()
