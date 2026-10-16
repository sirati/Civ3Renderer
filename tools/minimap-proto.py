#!/usr/bin/env python3
"""Prototype: rebuild the Civ3 Conquests minimap (Navigator_Data, 0xA0E230)
from a civ3-bridge trace and compare it with the game screenshot.

Implements docs/civ3-map-rendering.md section 17 literally (RGB555 path):
  PCX_1 (+0x18)  : bilinear terrain colours            (0x57A760)
  PCX_3 (+0x588) : PCX_1 averaged with grey (56,56,56)  (0x578430)
  PCX_2 (+0x2D0) : PCX_3, owned land tinted 3:1 with the owner colour (0x579D90/0x578BA0),
                   cities = white squares                (0x579090)
  PCX_4 (+0x858) : black, explored cells copied from PCX_2 (0x578970)
  blit PCX_4 opaque at Rect, then the white viewport rectangle (0x579F20).

usage: minimap-proto.py TRACE.txt [GAME.png] [--out ours.png] [--diff diff.png]
                        [--origin X Y] [--size W H] [--terrain-mode]

Run with: nix shell --impure --expr 'let p = import <nixpkgs> {}; in
  p.python3.withPackages (ps: [ps.pillow ps.numpy])' -c python3 tools/minimap-proto.py ...
"""
import argparse
import os

import numpy as np
from PIL import Image

INSTALL = "/mnt/datasets/winvm/civ3-src/Civilization III Complete"

# Terrain colours (0x57A914..0x57AB1A, jump table 0x57ACA4 indexed by Tile::m50 = SquareType bits 12..15).
# The exe holds the RGB565 constant C and the RGB555 value C + K; the screen surface is RGB555.
TERRAIN_555 = {
    0: 0x7BD7,   # desert     565 0xF7B7
    1: 0x6AAA,   # plains     565 0xD56A
    2: 0x4646,   # grassland  565 0x8CA6
    3: 0x6FBD,   # tundra     565 0xDF5D
    4: 0x30C2,   # floodplain 565 0x61A2
    5: 0x1986,   # hills      565 0x3326
    6: 0x6739,   # mountains  565 0xCE59
    7: 0x25E6,   # forest     565 0x4BC6
    8: 0x0D60,   # jungle     565 0x1AE0
    9: 0x31EB,   # swamp      565 0x63CB
    10: 0x3401,  # volcano    565 0x6801
    11: 0x1295,  # coast      565 0x2515
    12: 0x0E11,  # sea        565 0x1C11
    13: 0x012B,  # ocean      565 0x026B
}
WHITE_555 = 0x7FFF
WATER = {11, 12, 13}


def red(c):   # 0x550880, 555 path
    return (c >> 7) & 0xF8


def green(c):  # 0x57AF60, 555 path
    return (c >> 2) & 0xF8


def blue(c):
    return (c << 3) & 0xFF


def rgb(r, g, b):  # 0x48D210, 555 path
    return ((r & 0xF8) << 7) | ((g & 0xF8) << 2) | ((b & 0xFF) >> 3)


def expand(c):  # RGB555 -> 8-bit, bit replication (what the screenshots show)
    r, g, b = (c >> 10) & 31, (c >> 5) & 31, c & 31
    return ((r << 3) | (r >> 2), (g << 3) | (g >> 2), (b << 3) | (b >> 2))


def cdiv(a, b):  # C integer division (truncates toward zero)
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b >= 0) else -q


def cmod(a, b):
    return a - cdiv(a, b) * b


def load_trace(path):
    t = {"tiles": {}, "cities": [], "color_tables": [0] * 32}
    for line in open(path, errors="replace"):
        f = line.split()
        if not f:
            continue
        k = f[0]
        if k == "map":
            t["W"], t["H"] = int(f[1]), int(f[2])
            t["flags"] = int(f[f.index("flags") + 1], 0)
        elif k == "view":
            i, j = f.index("visible_x"), f.index("visible_y")
            t["xmin"], t["xmax"] = int(f[i + 1]), int(f[i + 2])
            t["ymin"], t["ymax"] = int(f[j + 1]), int(f[j + 2])
        elif k == "player":
            t["player"] = int(f[1])
        elif k == "color_tables":
            t["color_tables"] = [int(v) for v in f[1:]]
        elif k == "city":
            t["cities"].append((int(f[2]), int(f[3]), int(f[4])))
        elif k == "tile":
            x, y = int(f[1]), int(f[2])
            st = int(f[3], 0)
            t["tiles"][(x, y)] = {
                "terrain": (st >> 12) & 0xF,
                "owner": int(f[9]) & 0xFF,
                "explored": int(f[13], 0) if len(f) > 13 else 0,
            }
    return t


def read_pcx_palette(path):
    data = open(path, "rb").read()
    pal = data[-768:]
    return [tuple(pal[3 * i:3 * i + 3]) for i in range(256)]


def civ_colour_555(table_id):
    """Leader 0x55A350 -> 0x539E20: entry 66 of ntpNN.pcx, as RGB555."""
    p = read_pcx_palette(os.path.join(INSTALL, "Art/Units/Palettes/ntp%02d.pcx" % table_id))
    r, g, b = p[0x42]
    return rgb(r, g, b)


class Minimap:
    def __init__(self, t, w, h):
        self.t, self.w, self.h = t, w, h
        self.W, self.H, self.flags = t["W"], t["H"], t["flags"]

    # Map_wrap_horiz / Map_wrap_vert (0x426C80 / 0x426CC0): one add/subtract, only if the flag is set
    def wrap_h(self, x):
        if self.flags & 1:
            if x < 0:
                return x + self.W
            if x >= self.W:
                return x - self.W
        return x

    def wrap_v(self, y):
        if self.flags & 2:
            if y < 0:
                return y + self.H
            if y >= self.H:
                return y - self.H
        return y

    def valid(self, x, y):
        return 0 <= x < self.W and 0 <= y < self.H

    def pixel_to_tile(self, px, py):
        """0x57A5D0: minimap pixel -> nearest tile (diamond rounding) + 2-D fractions."""
        w, h, W, H = self.w, self.h, self.W, self.H
        D = w * h * 2
        half = cdiv(D, 2)
        s = W * px * h - (H - 1) * py * w + half
        fx = cmod(s, D)
        if s < 0:
            s -= D
        A = cdiv(s, D)
        s2 = (H - 1) * py * w + W * px * h + half
        fy = cmod(s2, D)
        if s2 < 0:
            s2 -= D
        B = cdiv(s2, D)
        ox, oy = A + B, B - A
        if ox >= W:
            ox -= W
        if fx < 0:
            fx += D
        if fy < 0:
            fy += D
        bad = not (0 <= ox < W and 0 <= oy < H)
        return bad, ox, oy, fx, fy

    def terrain_colour(self, x, y):
        tile = self.t["tiles"].get((x, y))
        if tile is None:
            return 0
        return TERRAIN_555.get(tile["terrain"], 0)

    def build_pcx1(self):
        """0x57A760: bilinear blend of the 4 tile colours around each pixel."""
        w, h = self.w, self.h
        D = w * h * 2
        half = D >> 1
        out = np.zeros(w * h, np.uint32)
        ptr = 0  # the exe walks a linear pointer that only advances on written pixels
        for py in range(h):
            for px in range(w):
                bad, X, Y, fx, fy = self.pixel_to_tile(px, py)
                if bad:
                    continue
                if fx < half:
                    fx += half
                    X -= 1
                    Y += 1
                else:
                    fx -= half
                if fy < half:
                    fy += half
                    X -= 1
                    Y -= 1
                else:
                    fy -= half
                r = g = b = 0
                for i in range(4):
                    xx = self.wrap_h(X + ((i + 1) >> 1))
                    yy = self.wrap_v(Y + (i >> 1) - (i & 1))
                    if not self.valid(xx, yy):
                        c = WHITE_555 if (self.flags & 3) else 0
                    else:
                        c = self.terrain_colour(xx, yy)
                    wx = fx if (i & 1) else D - fx
                    wy = fy if (i >> 1) else D - fy
                    r += (red(c) * wx // D) * wy // D
                    g += (green(c) * wx // D) * wy // D
                    b += (blue(c) * wx // D) * wy // D
                out[ptr] = rgb(min(r, 255), min(g, 255), min(b, 255))
                ptr += 1
        return out.reshape(h, w)

    def build(self, terrain_mode=False):
        w, h, W, H = self.w, self.h, self.W, self.H
        t = self.t
        pcx1 = self.build_pcx1()
        # PCX_3 = average with (56,56,56) (0x578430)
        pcx3 = np.zeros_like(pcx1)
        for py in range(h):
            for px in range(w):
                c = int(pcx1[py, px])
                pcx3[py, px] = rgb((red(c) + 56) >> 1, (green(c) + 56) >> 1, (blue(c) + 56) >> 1)
        pcx2 = pcx3.copy()
        # cell lists (Navigator_Cell, 0x18 bytes per lattice cell)
        cells = {}
        for py in range(h):
            for px in range(w):
                _, X, Y, _, _ = self.pixel_to_tile(px, py)
                cells.setdefault((X, Y), []).append((px, py))
        # territory tint (0x579D90 -> 0x578BA0)
        colours = {}
        for y in range(H):
            for x in range(y & 1, W, 2):
                tile = t["tiles"].get((x, y))
                if not tile:
                    continue
                o = tile["owner"]
                if 0 < o < 32:
                    if o not in colours:
                        colours[o] = civ_colour_555(t["color_tables"][o])
                    cc = colours[o]
                    if tile["terrain"] in WATER:  # Tile::m35_Check_Is_Water
                        continue
                    for (px, py) in cells.get((x, y), ()):
                        c = int(pcx1[py, px])
                        pcx2[py, px] = rgb((3 * red(cc) + red(c)) >> 2, (3 * green(cc) + green(c)) >> 2,
                                           (3 * blue(cc) + blue(c)) >> 2)
        main = pcx1 if terrain_mode else pcx2
        pcx4 = np.zeros_like(pcx1)
        city_at = {(x, y) for (x, y, _) in t["cities"]}
        bit = 1 << t["player"]
        per_col = w // W  # Navigator +0x848
        s = min(max(per_col >> 2, 1), 4)
        for y in range(H):
            for x in range(y & 1, W, 2):
                tile = t["tiles"].get((x, y))
                if not tile or not (tile["explored"] & bit):
                    continue
                for (px, py) in cells.get((x, y), ()):
                    pcx4[py, px] = main[py, px]
                if (x, y) in city_at and not terrain_mode:  # 0x579090
                    xx, yy = self.wrap_h(x), self.wrap_v(y)
                    pxc = pyc = 0
                    if self.valid(xx, yy):
                        pxc = xx * w // W
                        pyc = yy * h // (H - 1)
                    x0 = (per_col >> 2) - s + pxc
                    y0 = (per_col >> 4) - s + pyc
                    for ry in range(y0, (per_col >> 4) + pyc + s + 1):
                        if ry >= h:
                            break
                        for rx in range(x0, (per_col >> 2) + pxc + s + 1):
                            if rx >= w:
                                break
                            if rx > 0 and ry > 0:
                                pcx2[ry, rx] = WHITE_555
                    for ry in range(max(y0, 0), min(y0 + 2 * s + 1, h)):
                        for rx in range(max(x0, 0), min(x0 + 2 * s + 1, w)):
                            pcx4[ry, rx] = pcx2[ry, rx]
        return pcx4

    def viewport_lines(self):
        """0x579F20: list of (x1, y1, x2, y2) inclusive white lines, minimap-relative."""
        w, h, W, H, fl = self.w, self.h, self.W, self.H, self.flags
        t = self.t

        def clampx(v):
            return v if fl & 1 else min(max(v, 0), W - 1)

        def clampy(v):
            return v if fl & 2 else min(max(v, 0), H - 1)

        xa, ya = self.wrap_h(self.wrap_h(clampx(t["xmin"]))), self.wrap_v(self.wrap_v(clampy(t["ymin"])))
        if not self.valid(xa, ya):
            return []
        x0, y0 = xa * w // W, ya * h // (H - 1)
        xb, yb = self.wrap_h(self.wrap_h(clampx(t["xmax"]))), self.wrap_v(self.wrap_v(clampy(t["ymax"])))
        if not self.valid(xb, yb):
            return []
        x1, y1 = xb * w // W, yb * h // (H - 1)
        L = []
        hl = lambda a, b, y: L.append((a, y, b, y))
        vl = lambda x, a, b: L.append((x, a, x, b))
        if x0 > x1 and y0 > y1:
            hl(0, x1 - 1, y0); hl(0, x1 - 1, y1 - 1); hl(x0, w - 1, y0); hl(x0, w - 1, y1 - 1)
            vl(x0, 0, y1 - 1); vl(x1 - 1, 0, y1 - 1); vl(x0, y0, h - 1); vl(x1 - 1, y0, h - 1)
        elif x0 <= x1 and y0 > y1:
            hl(x0, x1 - 1, y1 - 1); vl(x0, 0, y1 - 1); vl(x1 - 1, 0, y1 - 1)
            hl(x0, x1 - 1, y0); vl(x0, y0, h - 1); vl(x1 - 1, y0, h - 1)
        elif x0 > x1:
            vl(x1 - 1, y0, y1 - 1); hl(0, x1 - 1, y0); hl(0, x1 - 1, y1 - 1)
            vl(x0, y0, y1 - 1); hl(x0, w - 1, y0); hl(x0, w - 1, y1 - 1)
        else:  # 0x600940 outline of (x0, y0, x1, y1): right/bottom exclusive
            hl(x0, x1 - 1, y0); hl(x0, x1 - 1, y1 - 1); vl(x0, y0, y1 - 1); vl(x1 - 1, y0, y1 - 1)
        return L


def to_rgb(img):
    h, w = img.shape
    out = np.zeros((h, w, 3), np.uint8)
    lut = {}
    for py in range(h):
        for px in range(w):
            c = int(img[py, px]) & 0x7FFF
            if c not in lut:
                lut[c] = expand(c)
            out[py, px] = lut[c]
    return out


def compose(rgbimg, lines, screen, ox, oy):
    """Put the minimap and the rectangle onto a copy of the screenshot."""
    out = screen.copy()
    h, w = rgbimg.shape[:2]
    out[oy:oy + h, ox:ox + w] = rgbimg
    for (x1, y1, x2, y2) in lines:
        for y in range(min(y1, y2), max(y1, y2) + 1):
            for x in range(min(x1, x2), max(x1, x2) + 1):
                if 0 <= oy + y < out.shape[0] and 0 <= ox + x < out.shape[1]:
                    out[oy + y, ox + x] = (255, 255, 255)
    return out


def geometry(t, size, origin):
    """0x578050 with w, h < 0: aspect correction and centring."""
    w, h = size
    ox, oy = origin
    W, H = t["W"], t["H"]
    ratio = cdiv(W, H)
    if ratio > 1:
        nh = cdiv(h, ratio); oy += cdiv(h - nh, 2); h = nh
    elif ratio < 1:
        nw = cdiv(w, cdiv(H, W)); ox += cdiv(w - nw, 2); w = nw
    if w & 1:
        w += 1
    return w, h, ox, oy


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("trace")
    ap.add_argument("game", nargs="?")
    ap.add_argument("--origin", nargs=2, type=int, default=[31, 961],
                    help="screen position of the minimap rect (Main_GUI+0x13F38 + form origin); 1920x1080: 31 961")
    ap.add_argument("--size", nargs=2, type=int, default=[230, 108],
                    help="rect size before the aspect correction; 1920x1080: 230 108")
    ap.add_argument("--terrain-mode", action="store_true", help="PCX_0 = PCX_1 (0x579E70 toggle)")
    ap.add_argument("--out")
    ap.add_argument("--diff")
    a = ap.parse_args()

    t = load_trace(a.trace)
    w, h, ox, oy = geometry(t, a.size, a.origin)
    mm = Minimap(t, w, h)
    img = to_rgb(mm.build(a.terrain_mode))
    lines = mm.viewport_lines()
    if a.out:
        Image.fromarray(compose(img, lines, np.zeros((h + oy, w + ox, 3), np.uint8), ox, oy)[oy:, ox:]).save(a.out)
    if a.game:
        scr = np.array(Image.open(a.game).convert("RGB"))
        ours = compose(img, lines, scr, ox, oy)
        ref = scr[oy:oy + h, ox:ox + w].astype(int)
        mine = ours[oy:oy + h, ox:ox + w].astype(int)
        eq = (ref == mine).all(2)
        near = (np.abs(ref - mine) <= 8).all(2)
        print("%s: map %dx%d flags %#x minimap %dx%d at (%d,%d): exact %.2f%%  (<=8/channel %.2f%%)  mismatches %d"
              % (os.path.basename(a.trace), t["W"], t["H"], t["flags"], w, h, ox, oy,
                 100.0 * eq.mean(), 100.0 * near.mean(), (~eq).sum()))
        if a.diff:
            d = np.zeros((h, w * 3, 3), np.uint8)
            d[:, :w] = ref
            d[:, w:2 * w] = mine
            d[:, 2 * w:][~eq] = (255, 0, 255)
            Image.fromarray(d).resize((w * 3 * 4, h * 4), Image.NEAREST).save(a.diff)


if __name__ == "__main__":
    main()
