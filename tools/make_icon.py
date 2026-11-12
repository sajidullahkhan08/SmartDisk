#!/usr/bin/env python3
"""Generate SmartDisk icon assets (PNG + multi-size ICO) with no third-party deps."""
import math
import os
import struct
import zlib

OUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "resources", "icons")
os.makedirs(OUT, exist_ok=True)

BG_TOP = (37, 99, 235)
BG_BOTTOM = (14, 165, 233)
DISK = (241, 245, 249)
DISK_SHADE = (203, 213, 225)
ACCENT = (250, 204, 21)


def rounded_rect_alpha(x, y, w, h, r):
    # signed distance for rounded rect centered in w x h at pixel center
    cx, cy = w / 2.0, h / 2.0
    dx = abs(x + 0.5 - cx) - (w / 2.0 - r)
    dy = abs(y + 0.5 - cy) - (h / 2.0 - r)
    if dx <= 0 and dy <= 0:
        return 1.0
    qx, qy = max(dx, 0.0), max(dy, 0.0)
    d = math.hypot(qx, qy) - r
    return max(0.0, min(1.0, 0.5 - d))


def render(size):
    px = bytearray(size * size * 4)
    cx = cy = size / 2.0
    disk_r = size * 0.34
    hole_r = size * 0.105
    for y in range(size):
        for x in range(size):
            a = rounded_rect_alpha(x, y, size, size, size * 0.22)
            if a <= 0:
                continue
            t = y / max(1, size - 1)
            r = int(BG_TOP[0] * (1 - t) + BG_BOTTOM[0] * t)
            g = int(BG_TOP[1] * (1 - t) + BG_BOTTOM[1] * t)
            b = int(BG_TOP[2] * (1 - t) + BG_BOTTOM[2] * t)

            dx = x + 0.5 - cx
            dy = y + 0.5 - cy
            dist = math.hypot(dx, dy)

            # disk body with soft edge
            disk_a = max(0.0, min(1.0, disk_r - dist + 0.5))
            if disk_a > 0:
                dr, dg, db = DISK
                # subtle bottom shading
                if dy > 0:
                    k = min(1.0, dy / disk_r) * 0.35
                    dr = int(dr * (1 - k) + DISK_SHADE[0] * k)
                    dg = int(dg * (1 - k) + DISK_SHADE[1] * k)
                    db = int(db * (1 - k) + DISK_SHADE[2] * k)
                r = int(r * (1 - disk_a) + dr * disk_a)
                g = int(g * (1 - disk_a) + dg * disk_a)
                b = int(b * (1 - disk_a) + db * disk_a)

            # center hole
            hole_a = max(0.0, min(1.0, hole_r - dist + 0.5))
            if hole_a > 0:
                hr, hg, hb = BG_BOTTOM
                r = int(r * (1 - hole_a) + hr * hole_a)
                g = int(g * (1 - hole_a) + hg * hole_a)
                b = int(b * (1 - hole_a) + hb * hole_a)

            # accent arc / activity wedge on the disk
            ang = math.atan2(dy, dx)
            if 0.2 < ang < 1.25 and hole_r + size * 0.035 < dist < disk_r - size * 0.035:
                r, g, b = ACCENT

            i = (y * size + x) * 4
            px[i] = r
            px[i + 1] = g
            px[i + 2] = b
            px[i + 3] = int(255 * a)
    return bytes(px)


def png_bytes(size, pixels):
    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        c += struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        return c

    raw = b"".join(b"\x00" + pixels[y * size * 4:(y + 1) * size * 4] for y in range(size))
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def main():
    sizes = [16, 24, 32, 48, 64, 128, 256]
    pngs = {}
    for s in sizes:
        pngs[s] = png_bytes(s, render(s))
    with open(os.path.join(OUT, "smartdisk-64.png"), "wb") as f:
        f.write(pngs[64])
    with open(os.path.join(OUT, "smartdisk-256.png"), "wb") as f:
        f.write(pngs[256])

    # ICO with PNG payloads
    entries = b""
    offset = 6 + 16 * len(sizes)
    blobs = b""
    for s in sizes:
        data = pngs[s]
        w = 0 if s >= 256 else s
        h = 0 if s >= 256 else s
        entries += struct.pack("<BBBBHHII", w, h, 0, 0, 1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    ico = struct.pack("<HHH", 0, 1, len(sizes)) + entries + blobs
    with open(os.path.join(OUT, "smartdisk.ico"), "wb") as f:
        f.write(ico)
    print("wrote", OUT)


if __name__ == "__main__":
    main()
