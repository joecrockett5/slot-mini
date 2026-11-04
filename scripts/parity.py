#!/usr/bin/env python3
"""Compose slot's reference frame and slot-mini's golden render side by side
for docs/DESIGN-PARITY.md. Needs Pillow.

    scripts/parity.py <name> <slot reference png> <slot-mini golden png>
"""
import sys
from PIL import Image, ImageDraw

name, ref, ours = sys.argv[1:4]
a = Image.open(ref).convert("RGB")
b = Image.open(ours).convert("RGB")
pad, label = 12, 22
W = a.width + b.width + pad * 3
H = max(a.height, b.height) + pad * 2 + label
out = Image.new("RGB", (W, H), (24, 24, 24))
d = ImageDraw.Draw(out)
d.text((pad, pad), f"slot ({a.width}x{a.height}, RG SP)", fill=(220, 220, 220))
d.text((pad * 2 + a.width, pad), f"slot-mini ({b.width}x{b.height})", fill=(220, 220, 220))
out.paste(a, (pad, pad + label))
out.paste(b, (pad * 2 + a.width, pad + label))
out.save(f"docs/parity/{name}.png", optimize=True)
