#!/usr/bin/env python3
"""Prüft eine 16-bit-WAV-Datei auf Signal (RMS/Peak).

Aufruf: wav_check.py <datei.wav>
Exit 0 wenn Peak > 0, sonst 1. Gibt Kennzahlen aus.
"""
import struct
import sys


def main():
    path = sys.argv[1]
    with open(path, "rb") as f:
        data = f.read()
    if data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        print("kein RIFF/WAVE")
        sys.exit(1)
    off = 12
    fmt = None
    pcm = None
    while off + 8 <= len(data):
        cid = data[off:off + 4]
        size = struct.unpack("<I", data[off + 4:off + 8])[0]
        body = data[off + 8:off + 8 + size]
        if cid == b"fmt ":
            fmt = struct.unpack("<HHIIHH", body[:16])
        elif cid == b"data":
            pcm = body
        off += 8 + size + (size & 1)
    if fmt is None or pcm is None:
        print("fmt-/data-Chunk fehlt")
        sys.exit(1)
    channels, bits = fmt[1], fmt[5]
    if bits != 16:
        print(f"unerwartete Bittiefe: {bits}")
        sys.exit(1)
    n = len(pcm) // 2
    samples = struct.unpack(f"<{n}h", pcm[:n * 2])
    peak = max(abs(s) for s in samples) if samples else 0
    rms = (sum(s * s for s in samples) / len(samples)) ** 0.5 if samples else 0.0
    print(f"frames={n // channels} channels={channels} peak={peak} rms={rms:.1f}")
    sys.exit(0 if peak > 0 else 1)


if __name__ == "__main__":
    main()
