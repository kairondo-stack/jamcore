#!/usr/bin/env python3
"""Erzeugt kleine 24-bit-BMP-Testbilder (ohne externe Abhängigkeiten).

Aufruf: make_fixtures.py <zielverzeichnis>
"""
import os
import struct
import sys


def write_bmp(path, w, h, pixel_fn):
    row_bytes = (w * 3 + 3) & ~3
    data_size = row_bytes * h
    file_size = 54 + data_size
    header = b"BM" + struct.pack("<IHHI", file_size, 0, 0, 54)
    dib = struct.pack(
        "<IiiHHIIiiII", 40, w, h, 1, 24, 0, data_size, 2835, 2835, 0, 0
    )
    rows = bytearray()
    for y in range(h - 1, -1, -1):
        row = bytearray()
        for x in range(w):
            r, g, b = pixel_fn(x, y)
            row += bytes((b, g, r))
        row += b"\x00" * (row_bytes - len(row))
        rows += row
    with open(path, "wb") as f:
        f.write(header + dib + rows)


def main():
    outdir = sys.argv[1]
    os.makedirs(outdir, exist_ok=True)
    write_bmp(os.path.join(outdir, "img_01.bmp"), 64, 64, lambda x, y: (255, 0, 0))
    write_bmp(
        os.path.join(outdir, "img_02.bmp"), 64, 64, lambda x, y: (x * 4, y * 4, 128)
    )
    write_bmp(
        os.path.join(outdir, "img_03.bmp"),
        64,
        64,
        lambda x, y: ((255, 255, 255) if (x // 8 + y // 8) % 2 else (0, 0, 0)),
    )
    print(f"Fixtures in {outdir}")


if __name__ == "__main__":
    main()
