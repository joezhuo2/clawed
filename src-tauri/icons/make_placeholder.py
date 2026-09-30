"""Writes a placeholder app icon (a white capsule on dark gray) as
app-icon.png. Run `npx tauri icon src-tauri/icons/app-icon.png` afterwards
to produce every platform size. Replace during the branding pass."""
import struct
import zlib
from pathlib import Path

S = 1024


def inside_capsule(x, y, cx, cy, w, h):
    r = h / 2
    dx = max(abs(x - cx) - (w / 2 - r), 0)
    dy = y - cy
    return dx * dx + dy * dy <= r * r


def inside_rounded(x, y, size, radius):
    dx = max(radius - x, 0, x - (size - 1 - radius))
    dy = max(radius - y, 0, y - (size - 1 - radius))
    return dx * dx + dy * dy <= radius * radius


rows = []
for y in range(S):
    row = bytearray([0])
    for x in range(S):
        if not inside_rounded(x, y, S, 220):
            row += b"\x00\x00\x00\x00"
        elif inside_capsule(x, y, S / 2, S / 2, 640, 220):
            row += b"\xf2\xf2\xf2\xff"
        else:
            row += b"\x1c\x1c\x1e\xff"
    rows.append(bytes(row))


def chunk(tag, data):
    c = tag + data
    return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)


png = b"\x89PNG\r\n\x1a\n"
png += chunk(b"IHDR", struct.pack(">IIBBBBB", S, S, 8, 6, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(b"".join(rows), 9))
png += chunk(b"IEND", b"")
Path(__file__).with_name("app-icon.png").write_bytes(png)
print("ok")
