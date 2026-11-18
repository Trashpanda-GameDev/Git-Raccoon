#!/usr/bin/env python3
"""Draws the hand cursors in crates/git-raccoon/assets/cursors/: an open
hand over something that can be taken hold of, and a closed one while
holding it. Pixel art in the classic style (white, a black outline, grey
corners), drawn below one character per pixel: edit the maps to change
them. Each is drawn at 32 pixels and, doubled, at 64 for high-DPI
screens.

Run it from the repository root after changing a map:

    python3 scripts/cursors.py
"""

import pathlib

from PIL import Image

OUT = pathlib.Path("crates/git-raccoon/assets/cursors")
SIZES = [32, 64]
COLOURS = {
    "#": (0, 0, 0, 255),
    "g": (128, 128, 128, 255),
    "o": (255, 255, 255, 255),
}

OPEN = """
........g##g......
.....g###oo###g...
.....#oo#oo#oo#...
.....#oo#oo#oo###g
.....#oo#oo#oo#oo#
.....#oo#oo#oo#oo#
.....#oo#oo#oo#oo#
.....#oo#oo#oo#oo#
.....#oo#oo#oo#oo#
.###.#oo#oo#oo#oo#
#ooo##oogoogoogoo#
#oooo#ooooooooooo#
g#ooooooooooooooo#
.#ooooooooooooooo#
..#ooooooooooooo#g
..#ooooooooooooo#.
...#ooooooooooo#..
...#ooooooooooo#..
....#ooooooooo#...
....###########...
"""

CLOSED = """
......##.##....
...###oo#oo###.
..#oo#oo#oo#oo#
.##oogoogoogoo#
#ooooooooooooo#
#ooooooooooooo#
.#oooooooooooo#
.#oooooooooooo#
..#ooooooooooo#
...#ooooooooo#.
...#oooooooo#..
....#ooooooo#..
....#########..
"""


def bitmap(art: str) -> Image.Image:
    """A map as an image, one pixel per character."""
    rows = art.strip("\n").split("\n")
    width = max(len(r) for r in rows)
    image = Image.new("RGBA", (width, len(rows)), (0, 0, 0, 0))
    for y, row in enumerate(rows):
        for x, pixel in enumerate(row):
            if pixel in COLOURS:
                image.putpixel((x, y), COLOURS[pixel])
    return image


def cursor(art: str, size: int) -> Image.Image:
    """The art in the middle of a `size` cursor, each pixel `size / 32` wide."""
    scale = size // 32
    hand = bitmap(art)
    hand = hand.resize((hand.width * scale, hand.height * scale), Image.NEAREST)
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    image.paste(hand, ((size - hand.width) // 2, (size - hand.height) // 2))
    return image


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for old in OUT.glob("*.png"):
        old.unlink()
    for size in SIZES:
        for name, art in [("grab", OPEN), ("grabbing", CLOSED)]:
            path = OUT / f"{name}-{size}.png"
            cursor(art, size).save(path)
            print(path)


if __name__ == "__main__":
    main()
