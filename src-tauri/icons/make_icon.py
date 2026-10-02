"""Draws the islet app icon as app-icon.png (1024x1024, RGBA).

A dark rounded tile with the island pill near the top and three claw
scratches under it, in the island's working-blue to approval-violet. Pure
standard library, anti-aliased with signed distance fields, so it can be
regenerated anywhere. Afterwards produce every platform size with

    npx tauri icon src-tauri/icons/app-icon.png
"""
import math
import struct
import zlib
from pathlib import Path

S = 1024
MARGIN = 64          # transparent border, like other macOS app icons
TILE_R = 200         # tile corner radius
BG_TOP = (0x26, 0x28, 0x31)
BG_BOTTOM = (0x0f, 0x10, 0x14)
PILL = (0xf2, 0xf2, 0xf5)
BLUE = (0x4c, 0x8d, 0xff)    # Colors::default().working
VIOLET = (0xa9, 0x70, 0xff)  # Colors::default().request

PILL_CX, PILL_CY, PILL_W, PILL_H = S / 2, 300, 520, 132
DOT = (PILL_CX - PILL_W / 2 + PILL_H / 2, PILL_CY, 30)  # status dot in the pill
# Three scratches: (top x, top y, bottom x, bottom y, half width).
SCRATCHES = [
    (410, 455, 300, 840, 34),
    (545, 440, 435, 880, 38),
    (680, 455, 570, 840, 34),
]


def sd_round_box(x, y, x0, y0, x1, y1, r):
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    hx, hy = (x1 - x0) / 2 - r, (y1 - y0) / 2 - r
    qx, qy = abs(x - cx) - hx, abs(y - cy) - hy
    outside = math.hypot(max(qx, 0), max(qy, 0))
    return outside + min(max(qx, qy), 0) - r


def sd_capsule(x, y, ax, ay, bx, by, r):
    px, py, dx, dy = x - ax, y - ay, bx - ax, by - ay
    h = max(0.0, min(1.0, (px * dx + py * dy) / (dx * dx + dy * dy)))
    return math.hypot(px - dx * h, py - dy * h) - r


def scratch_points(ax, ay, bx, by, r, bow=32, n=96):
    """Centerline of a scratch as (x, y, half width) samples: a slight
    curve that is widest just above the middle and comes to a point at both
    ends."""
    # Control point pushed sideways from the midpoint, perpendicular to a-b.
    mx, my = (ax + bx) / 2, (ay + by) / 2
    length = math.hypot(bx - ax, by - ay)
    nx, ny = (by - ay) / length, -(bx - ax) / length
    cx, cy = mx + nx * bow, my + ny * bow
    pts = []
    for i in range(n + 1):
        t = i / n
        x = (1 - t) ** 2 * ax + 2 * (1 - t) * t * cx + t * t * bx
        y = (1 - t) ** 2 * ay + 2 * (1 - t) * t * cy + t * t * by
        w = r * math.sin(math.pi * t ** 0.85) ** 0.8
        pts.append((x, y, max(w, 1.5)))
    return pts


def sd_points(x, y, pts):
    return min(math.hypot(x - px, y - py) - w for px, py, w in pts)


SCRATCH_SHAPES = []
for _s in SCRATCHES:
    _pts = scratch_points(*_s)
    _pad = _s[4] + 4
    SCRATCH_SHAPES.append((_pts, (min(p[0] for p in _pts) - _pad, min(p[1] for p in _pts) - _pad,
                                  max(p[0] for p in _pts) + _pad, max(p[1] for p in _pts) + _pad)))


def cov(d):
    return max(0.0, min(1.0, 0.5 - d))


def mix(a, b, t):
    return tuple(a[i] + (b[i] - a[i]) * t for i in range(3))


def over(dst, src, alpha):
    """Composites src (rgb) with coverage alpha over dst (r, g, b, a)."""
    r, g, b, a = dst
    out_a = alpha + a * (1 - alpha)
    if out_a == 0:
        return (0.0, 0.0, 0.0, 0.0)
    k = a * (1 - alpha)
    return (
        (src[0] * alpha + r * k) / out_a,
        (src[1] * alpha + g * k) / out_a,
        (src[2] * alpha + b * k) / out_a,
        out_a,
    )


def pixel(x, y):
    px = (0.0, 0.0, 0.0, 0.0)
    tile = cov(sd_round_box(x, y, MARGIN, MARGIN, S - MARGIN, S - MARGIN, TILE_R))
    if tile == 0:
        return px
    t = (y - MARGIN) / (S - 2 * MARGIN)
    px = over(px, mix(BG_TOP, BG_BOTTOM, t), tile)

    pill = cov(sd_capsule(x, y, PILL_CX - PILL_W / 2 + PILL_H / 2, PILL_CY,
                          PILL_CX + PILL_W / 2 - PILL_H / 2, PILL_CY, PILL_H / 2))
    if pill:
        px = over(px, PILL, pill * tile)
        dot = cov(math.hypot(x - DOT[0], y - DOT[1]) - DOT[2])
        if dot:
            px = over(px, BLUE, dot * tile)

    for pts, (x0, y0, x1, y1) in SCRATCH_SHAPES:
        if not (x0 <= x <= x1 and y0 <= y <= y1):
            continue
        c = cov(sd_points(x, y, pts))
        if c:
            k = max(0.0, min(1.0, (x - 280) / 420))
            px = over(px, mix(BLUE, VIOLET, k), c * tile)
    return px


def main():
    rows = []
    for y in range(S):
        row = bytearray([0])
        for x in range(S):
            r, g, b, a = pixel(x + 0.5, y + 0.5)
            row += bytes((round(r), round(g), round(b), round(a * 255)))
        rows.append(bytes(row))

    def chunk(tag, data):
        c = tag + data
        return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", S, S, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(b"".join(rows), 9))
    png += chunk(b"IEND", b"")
    Path(__file__).with_name("app-icon.png").write_bytes(png)
    print("wrote app-icon.png")


if __name__ == "__main__":
    main()
