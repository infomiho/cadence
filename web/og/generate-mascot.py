#!/usr/bin/env python3
"""Renders the Vespa Romeo mascot to SVG from the app's pixel map.

The mascot is defined in src/app/player_mascot.rs. This reads that file so the
site never carries a hand-maintained copy of the pixel art. Run it whenever the
mascot changes:

    ./web/og/generate-mascot.py
"""

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "src" / "app" / "player_mascot.rs"
OUTPUT = Path(__file__).resolve().parents[1] / "static" / "mascot.svg"

GRID_SIZE = 16


def read_source():
    return SOURCE.read_text()


def base_rows(source, name):
    body = re.search(rf"const {name}[^=]*= \[(.*?)\];", source, re.S).group(1)
    return re.findall(r'"([^"]*)"', body)


def first_frame_rects(source, name):
    body = re.search(rf"const {name}[^=]*= \[(.*?)\];", source, re.S).group(1)
    frame = body.split("],")[0]
    return [
        (key, int(x), int(y), int(w), int(h))
        for key, x, y, w, h in re.findall(
            r"\('(.)',\s*(\d+),\s*(\d+),\s*(\d+),\s*(\d+)\)", frame
        )
    ]


def colors(source):
    return {
        key: f"#{value.lower()}"
        for key, value in re.findall(r"'(.)' => 0x([0-9A-Fa-f]{6})", source)
    }


def build_grid(source):
    grid = [list(row) for row in base_rows(source, "ROMEO_BASE")]
    for key, x, y, width, height in first_frame_rects(source, "ROMEO_SCARF"):
        for dy in range(height):
            for dx in range(width):
                grid[y + dy][x + dx] = key
    for x, y in [(3, 13), (11, 13)]:
        grid[y][x] = "C"
    return grid


def runs(row, palette):
    start = 0
    while start < len(row):
        key = row[start]
        end = start
        while end < len(row) and row[end] == key:
            end += 1
        if key in palette:
            yield start, end - start, palette[key]
        start = end


def render_svg(grid, palette):
    lines = [
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" '
        'shape-rendering="crispEdges">'
    ]
    for y, row in enumerate(grid):
        for x, width, color in runs(row, palette):
            lines.append(f'<rect x="{x}" y="{y}" width="{width}" height="1" fill="{color}"/>')
    lines.append("</svg>")
    return "\n".join(lines) + "\n"


def main():
    source = read_source()
    grid = build_grid(source)
    assert len(grid) == GRID_SIZE and all(len(row) == GRID_SIZE for row in grid)
    OUTPUT.write_text(render_svg(grid, colors(source)))
    print(f"wrote {OUTPUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
