#!/usr/bin/env python3
"""Rasterize an ascii-aquarium ANSI (or plain-text) snapshot into a PNG.

Parses the subset of ANSI emitted by GlyphBuffer::to_ansi — cursor home,
SGR 24-bit foreground colors and reset — and draws the glyphs with a
monospace font on a black background, approximating a terminal preview.

Usage: ansi_to_png.py input.ansi output.png [cell_px]
"""

import glob
import sys

from PIL import Image, ImageDraw, ImageFont

FONT_CANDIDATES = [
    "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
    "/home/*/.local/share/fonts/JetBrainsMono/JetBrainsMonoNerdFont-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
]


def find_font(size: int):
    for pattern in FONT_CANDIDATES:
        for path in sorted(glob.glob(pattern)):
            try:
                return ImageFont.truetype(path, size)
            except OSError:
                continue
    return ImageFont.load_default()


def parse(text: str):
    """Return (rows, cols, grid) where grid[y][x] = (char, (r, g, b))."""
    grid = []
    row = []
    fg = (200, 200, 200)
    i = 0
    width = 0
    while i < len(text):
        ch = text[i]
        if ch == "\x1b":
            # Expecting a CSI sequence: ESC [ ... final-byte
            j = text.index("m", i) if "m" in text[i:] else None
            h = text.find("H", i, i + 8)
            if h != -1 and (j is None or h < j):
                i = h + 1
                continue
            seq = text[i + 2 : j]
            i = j + 1
            if seq in ("", "0"):
                fg = (200, 200, 200)
            elif seq.startswith("38;2;"):
                parts = seq.split(";")
                fg = (int(parts[2]), int(parts[3]), int(parts[4]))
            continue
        if ch == "\r":
            i += 1
            continue
        if ch == "\n":
            grid.append(row)
            width = max(width, len(row))
            row = []
            i += 1
            continue
        row.append((ch, fg))
        i += 1
    if row:
        grid.append(row)
        width = max(width, len(row))
    return grid, width


def main() -> None:
    src, dst = sys.argv[1], sys.argv[2]
    cell = int(sys.argv[3]) if len(sys.argv) > 3 else 14
    text = open(src, encoding="utf-8").read()
    grid, width = parse(text)
    rows = len(grid)
    if rows == 0 or width == 0:
        raise SystemExit("empty snapshot")

    font = find_font(cell * 2)  # cell height ~2x width for terminal feel
    cw = cell
    chh = cell * 2
    img = Image.new("RGB", (width * cw, rows * chh), (2, 2, 4))
    draw = ImageDraw.Draw(img)
    for y, row in enumerate(grid):
        for x, (glyph, color) in enumerate(row):
            if glyph != " ":
                draw.text((x * cw, y * chh), glyph, fill=color, font=font)
    img.save(dst)
    print(f"wrote {dst} ({img.width}x{img.height})")


if __name__ == "__main__":
    main()
