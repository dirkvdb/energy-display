#!/usr/bin/env python3
"""Push an ESP-IDF application image to Energy Display's OTA service."""

import argparse
import hashlib
import hmac
import os
import socket
import struct
import sys
from pathlib import Path


MAGIC = b"EDOTA001"
PORT = 3232


def receive_line(connection: socket.socket) -> bytes:
    response = bytearray()
    while not response.endswith(b"\n"):
        chunk = connection.recv(1)
        if not chunk:
            raise RuntimeError("device closed the connection")
        response.extend(chunk)
        if len(response) > 128:
            raise RuntimeError("device returned an invalid response")
    return bytes(response)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("host", help="device IPv4 address or hostname")
    parser.add_argument("image", type=Path, help="ESP-IDF application image")
    parser.add_argument("--port", type=int, default=PORT)
    args = parser.parse_args()

    password = os.environ.get("MQTT_PASSWORD")
    if not password:
        parser.error("MQTT_PASSWORD is not set; run this command inside devenv")

    image_size = args.image.stat().st_size
    if image_size == 0 or image_size % 4:
        parser.error("image must be non-empty and four-byte aligned")

    digest = hashlib.sha256()
    with args.image.open("rb") as image:
        for chunk in iter(lambda: image.read(1024 * 1024), b""):
            digest.update(chunk)

    metadata = MAGIC + struct.pack("<I", image_size) + digest.digest()
    header = metadata + hmac.digest(password.encode(), metadata, "sha256")

    print(f"Connecting to {args.host}:{args.port}")
    with socket.create_connection((args.host, args.port), timeout=15) as connection:
        connection.settimeout(180)
        connection.sendall(header)
        response = receive_line(connection)
        if response != b"READY\n":
            raise RuntimeError(response.decode(errors="replace").strip())

        sent = 0
        with args.image.open("rb") as image:
            while chunk := image.read(64 * 1024):
                connection.sendall(chunk)
                sent += len(chunk)
                print(f"\rUploading: {sent * 100 // image_size:3d}%", end="", flush=True)
        print()

        response = receive_line(connection)
        if response != b"OK rebooting\n":
            raise RuntimeError(response.decode(errors="replace").strip())

    print("Update installed; device is rebooting")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, RuntimeError) as error:
        print(f"OTA update failed: {error}", file=sys.stderr)
        sys.exit(1)
