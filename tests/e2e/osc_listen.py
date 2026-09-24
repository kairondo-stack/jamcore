#!/usr/bin/env python3
"""Minimaler OSC-UDP-Listener: schreibt empfangene Nachrichten als JSONL.

Aufruf: osc_listen.py <port> <ausgabedatei> <dauer_sekunden>
"""
import json
import socket
import struct
import sys
import time


def read_string(data, off):
    end = data.index(b"\x00", off)
    return data[off:end].decode("utf-8", "replace"), (end + 4) & ~3


def decode(data):
    addr, off = read_string(data, 0)
    args = []
    if off < len(data) and data[off:off + 1] == b",":
        tags, off = read_string(data, off)
        for t in tags[1:]:
            if t == "i":
                args.append(struct.unpack(">i", data[off:off + 4])[0]); off += 4
            elif t == "f":
                args.append(round(struct.unpack(">f", data[off:off + 4])[0], 6)); off += 4
            elif t == "s":
                s, off = read_string(data, off); args.append(s)
            elif t == "h":
                args.append(struct.unpack(">q", data[off:off + 8])[0]); off += 8
            elif t == "d":
                args.append(struct.unpack(">d", data[off:off + 8])[0]); off += 8
            elif t == "T":
                args.append(True)
            elif t == "F":
                args.append(False)
            else:
                args.append(f"<{t}>")
    return addr, args


def main():
    port = int(sys.argv[1])
    out = sys.argv[2]
    duration = float(sys.argv[3])
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.bind(("127.0.0.1", port))
    sock.settimeout(0.2)
    end = time.time() + duration
    with open(out, "w") as f:
        while time.time() < end:
            try:
                data, _ = sock.recvfrom(65535)
            except socket.timeout:
                continue
            try:
                addr, args = decode(data)
            except Exception as e:  # noqa: BLE001
                addr, args = f"<decode-error: {e}>", []
            f.write(json.dumps({"addr": addr, "args": args, "raw": data.hex()}) + "\n")
            f.flush()


if __name__ == "__main__":
    main()
