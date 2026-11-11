#!/usr/bin/env python3
"""Draws the README banner (docs/media/banner.png) from the app's own art:
the wood wallpaper, the menu typeface and a shelf screenshot recorded by
scripts/record-demo.sh (target/demo/shelf.png).

usage: scripts/banner.py [shelf.png] [out.png]
Needs Pillow.
"""

import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parent.parent
SHOT = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "target/demo/shelf.png"
OUT = Path(sys.argv[2]) if len(sys.argv) > 2 else ROOT / "docs/media/banner.png"
FONT = str(ROOT / "crates/slot-mini-ui/assets/label.ttf")  # Open Sans Bold

W, H = 1280, 460
INK = (246, 244, 239)
SOFT = (196, 190, 180)
GROUND = (11, 14, 17)


def cover(img, w, h):
    s = max(w / img.width, h / img.height)
    img = img.resize((round(img.width * s), round(img.height * s)), Image.LANCZOS)
    x, y = (img.width - w) // 2, (img.height - h) // 2
    return img.crop((x, y, x + w, y + h))


def tracked(draw, xy, text, font, fill, tracking):
    x, y = xy
    for ch in text:
        draw.text((x, y), ch, font=font, fill=fill)
        x += draw.textlength(ch, font=font) + tracking
    return x - tracking


def tracked_width(draw, text, font, tracking):
    return sum(draw.textlength(c, font=font) for c in text) + tracking * (len(text) - 1)


def main():
    wood = Image.open(ROOT / "assets/background_default.png").convert("RGB")
    base = cover(wood, W, H).filter(ImageFilter.GaussianBlur(5))

    # Darken towards the left so the type reads, leaving the wood on the right.
    shade = Image.new("L", (W, H))
    for x in range(W):
        t = min(1.0, max(0.0, (x - 360) / 620))
        a = round(236 - t * 140)
        shade.paste(a, (x, 0, x + 1, H))
    base = Image.composite(Image.new("RGB", (W, H), GROUND), base, shade).convert("RGBA")

    # The device screen: the shelf in a dark bezel with a soft shadow.
    shot = Image.open(SHOT).convert("RGB").resize((496, 372), Image.LANCZOS)
    pad, r = 14, 26
    bw, bh = shot.width + pad * 2, shot.height + pad * 2
    bx, by = W - bw - 80, (H - bh) // 2

    shadow = Image.new("RGBA", (W, H), (0, 0, 0, 0))
    ImageDraw.Draw(shadow).rounded_rectangle(
        (bx + 4, by + 18, bx + bw + 4, by + bh + 18), r, fill=(0, 0, 0, 170)
    )
    base = Image.alpha_composite(base, shadow.filter(ImageFilter.GaussianBlur(18)))

    d = ImageDraw.Draw(base)
    d.rounded_rectangle((bx, by, bx + bw, by + bh), r, fill=(22, 24, 28, 255))
    d.rounded_rectangle((bx, by, bx + bw, by + bh), r, outline=(58, 60, 66, 255), width=2)
    mask = Image.new("L", shot.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, shot.width - 1, shot.height - 1), 10, fill=255)
    base.paste(shot, (bx + pad, by + pad), mask)

    # Type.
    x = 84
    title = ImageFont.truetype(FONT, 96)
    lead = ImageFont.truetype(FONT, 24)
    chip = ImageFont.truetype(FONT, 15)

    d.text((x - 4, 106), "slot-mini", font=title, fill=INK)
    d.text((x, 232), "A cartridge-shelf launcher", font=lead, fill=SOFT)
    d.text((x, 266), "for the Miyoo Mini Plus", font=lead, fill=SOFT)

    cx, cy = x, 326
    for label in ("GAME BOY ADVANCE", "GAME BOY COLOR", "GAME BOY"):
        tw = tracked_width(d, label, chip, 2)
        d.rounded_rectangle((cx, cy, cx + tw + 28, cy + 32), 16, outline=(120, 116, 108), width=2)
        tracked(d, (cx + 14, cy + 6), label, chip, INK, 2)
        cx += tw + 28 + 10

    OUT.parent.mkdir(parents=True, exist_ok=True)
    base.convert("RGB").save(OUT, optimize=True)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
