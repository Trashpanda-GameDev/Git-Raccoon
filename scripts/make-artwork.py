#!/usr/bin/env python3
"""Draws Git Raccoon's artwork from the app icon, in the Raccoon theme's
colours:

- installer/art/wizard-*.bmp: the Windows installer's side picture (welcome
  and finish pages), one per display scale, 100% to 250%;
- installer/art/wizard-small-*.bmp: its small picture (the other pages);
- site/social-preview.png: the card shown when the website or the
  repository is shared (1280x640, GitHub's size; also the website's).

Run it from the repository root after the icon changes:

    pip install pillow
    python3 scripts/make-artwork.py

It uses the Inter font if it's installed (otherwise DejaVu Sans, or
Pillow's own).
"""

import pathlib

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = pathlib.Path(__file__).resolve().parent.parent
ICON = Image.open(ROOT / "crates/git-raccoon/assets/icon.png").convert("RGBA")
ART = ROOT / "installer/art"

# The Raccoon theme.
BG_TOP = (44, 42, 40)
BG_BOTTOM = (30, 29, 28)
TEXT = (243, 233, 220)
MUTED = (163, 154, 144)
ACCENT = (208, 95, 63)
BORDER = (61, 57, 53)
LANES = [(208, 95, 63), (243, 233, 220), (143, 201, 122), (229, 185, 75)]

# Inno Setup's picture sizes for each display scale (100% ... 250%).
WIZARD = [(164, 314), (192, 386), (246, 459), (273, 556), (328, 604), (355, 700), (410, 797)]
SMALL = [(55, 55), (64, 68), (83, 80), (92, 97), (110, 106), (119, 123), (138, 140)]


def font(size, bold=False):
    names = (
        ["Inter-Bold.otf", "InterDisplay-Bold.otf", "DejaVuSans-Bold.ttf"]
        if bold
        else ["Inter-Medium.otf", "Inter-Regular.otf", "DejaVuSans.ttf"]
    )
    for base in ["/usr/share/fonts/opentype/inter", "/usr/share/fonts/truetype/dejavu",
                 "C:/Windows/Fonts", "/Library/Fonts"]:
        for name in names:
            path = pathlib.Path(base) / name
            if path.exists():
                return ImageFont.truetype(str(path), size)
    return ImageFont.load_default(size)


def gradient(w, h):
    img = Image.new("RGB", (w, h))
    draw = ImageDraw.Draw(img)
    for y in range(h):
        t = y / max(h - 1, 1)
        draw.line([(0, y), (w, y)], fill=tuple(round(a + (b - a) * t) for a, b in zip(BG_TOP, BG_BOTTOM)))
    return img


def fade(colour, amount):
    """`colour` mixed towards the background."""
    return tuple(round(c * amount + b * (1 - amount)) for c, b in zip(colour, BG_BOTTOM))


def curve(x1, y1, x2, y2, steps=24):
    """Points along an S-curve from (x1, y1) down to (x2, y2), like the app's graph."""
    out = []
    for i in range(steps + 1):
        t = i / steps
        e = t * t * (3 - 2 * t)
        out.append((x1 + (x2 - x1) * e, y1 + (y2 - y1) * t))
    return out


def lanes(draw, x0, y0, w, h, s, strength=0.4):
    """A small commit graph, as the app draws it: a main line, a branch
    that's merged back, and one still open."""
    xs = [x0 + w * f for f in (0.36, 0.5, 0.64)]
    r, lw = 4.5 * s, max(1, round(2 * s))
    col = [fade(c, strength) for c in LANES]
    step = h / 6
    ys = [y0 + step * (i + 0.5) for i in range(6)]
    draw.line([(xs[1], y0), (xs[1], y0 + h)], fill=col[0], width=lw)
    # Branched off at row 4, merged back at row 1.
    draw.line(curve(xs[1], ys[4], xs[0], ys[3]) + [(xs[0], ys[2])] + curve(xs[0], ys[2], xs[1], ys[1])[1:],
              fill=col[2], width=lw, joint="curve")
    # Branched off at row 2, still open (going up off the top).
    draw.line([(xs[2], y0)] + curve(xs[2], ys[0], xs[2], ys[0])[:1] + list(reversed(curve(xs[1], ys[2], xs[2], ys[0]))),
              fill=col[3], width=lw, joint="curve")
    for x, y, k in [(xs[1], ys[1], 0), (xs[2], ys[0], 3), (xs[0], ys[2], 2), (xs[0], ys[3], 2),
                    (xs[1], ys[4], 0), (xs[1], ys[5], 0)]:
        draw.ellipse([x - r, y - r, x + r, y + r], fill=BG_BOTTOM, outline=col[k], width=lw)


def centred(draw, text, y, w, f, fill):
    width = draw.textlength(text, font=f)
    draw.text(((w - width) / 2, y), text, font=f, fill=fill)


def wizard(w, h):
    """Drawn four times larger, then scaled down, for smooth edges."""
    return wizard_at(w * 4, h * 4).resize((w, h), Image.LANCZOS)


def wizard_at(w, h):
    s = w / 164
    img = gradient(w, h)
    draw = ImageDraw.Draw(img)
    size = round(116 * s)
    top = round(40 * s)
    icon = ICON.resize((size, size), Image.LANCZOS)
    img.paste(icon, ((w - size) // 2, top), icon)
    y = top + size + 14 * s
    centred(draw, "Git Raccoon", y, w, font(round(19 * s), bold=True), TEXT)
    y += 30 * s
    bar = 34 * s
    draw.rounded_rectangle([(w - bar) / 2, y, (w + bar) / 2, y + 2.5 * s], radius=1.2 * s, fill=ACCENT)
    y += 12 * s
    centred(draw, "Git, without the guesswork.", y, w, font(round(9.5 * s)), MUTED)
    # A little commit graph in the space below.
    y += 30 * s
    lanes(draw, 0, y, w, h - y, s)
    return img


def small(w, h):
    img = Image.new("RGB", (w, h), (255, 255, 255))
    size = min(w, h) - 2
    icon = ICON.resize((size, size), Image.LANCZOS)
    img.paste(icon, ((w - size) // 2, (h - size) // 2), icon)
    return img


def rounded(img, radius):
    mask = Image.new("L", img.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, img.width - 1, img.height - 1], radius=radius, fill=255)
    out = img.convert("RGBA")
    out.putalpha(mask)
    return out


def social(w=1280, h=640):
    img = gradient(w, h)
    draw = ImageDraw.Draw(img)
    # The app, in the Raccoon theme, running off the right edge.
    shot = Image.open(ROOT / "site/screenshots/history.png").convert("RGB")
    sw = 760
    shot = shot.resize((sw, round(shot.height * sw / shot.width)), Image.LANCZOS)
    shot = rounded(shot, 14)
    x, y = 620, 120
    shadow = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle([x, y + 18, x + sw, y + shot.height + 18], radius=18, fill=(0, 0, 0, 150))
    shadow = shadow.filter(ImageFilter.GaussianBlur(28))
    img.paste(shadow, (0, 0), shadow)
    img.paste(shot, (x, y), shot)
    draw = ImageDraw.Draw(img)
    draw.rounded_rectangle([x, y, x + sw, y + shot.height], radius=14, outline=BORDER, width=2)
    # The raccoon and the words.
    icon = ICON.resize((132, 132), Image.LANCZOS)
    img.paste(icon, (88, 104), icon)
    draw.text((88, 262), "Git Raccoon", font=font(70, bold=True), fill=TEXT)
    draw.rounded_rectangle([90, 356, 160, 361], radius=2, fill=ACCENT)
    draw.text((88, 384), "Git, without the guesswork.", font=font(32, bold=True), fill=(230, 132, 100))
    draw.text((88, 434), "A fast, friendly, open-source", font=font(27), fill=MUTED)
    draw.text((88, 470), "Git client.", font=font(27), fill=MUTED)
    draw.text((88, 540), "Windows  ·  macOS  ·  Linux", font=font(22), fill=fade(TEXT, 0.7))
    return img


def main():
    ART.mkdir(parents=True, exist_ok=True)
    for i, (w, h) in enumerate(WIZARD):
        wizard(w, h).save(ART / f"wizard-{i + 1}.bmp")
    for i, (w, h) in enumerate(SMALL):
        small(w, h).save(ART / f"wizard-small-{i + 1}.bmp")
    social().save(ROOT / "site/social-preview.png", optimize=True)
    print(f"{ART}: {len(WIZARD)} side and {len(SMALL)} small pictures; site/social-preview.png")


if __name__ == "__main__":
    main()
