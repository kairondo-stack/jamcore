#!/usr/bin/env python3
"""Sendet OSC oder Rohbytes per UDP an 127.0.0.1.

Aufrufe:
  udp_send.py <port> <adresse> [i|f|s <wert> ...]
  udp_send.py <port> --raw <hex>
"""
import socket
import struct
import sys


def pad4(b):
    return b + b"\x00" * ((4 - len(b) % 4) % 4)


def encode(addr, args):
    out = pad4(addr.encode() + b"\x00")
    tags = ","
    body = b""
    for kind, value in args:
        if kind == "i":
            tags += "i"
            body += struct.pack(">i", int(value))
        elif kind == "f":
            tags += "f"
            body += struct.pack(">f", float(value))
        elif kind == "s":
            tags += "s"
            body += pad4(value.encode() + b"\x00")
    return out + pad4(tags.encode() + b"\x00") + body


def main():
    port = int(sys.argv[1])
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    if sys.argv[2] == "--raw":
        data = bytes.fromhex(sys.argv[3])
    else:
        addr = sys.argv[2]
        args = []
        rest = sys.argv[3:]
        while rest:
            kind = rest.pop(0)
            args.append((kind, rest.pop(0)))
        data = encode(addr, args)
    sock.sendto(data, ("127.0.0.1", port))
    print(data.hex())


if __name__ == "__main__":
    main()
