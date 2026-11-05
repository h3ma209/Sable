#!/usr/bin/env python3
"""Build macOS iconset from the official Sable mascot image with deep emerald background."""

import sys
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter, ImageOps
import numpy as np

def generate_base_icon() -> Image.Image:
    repo_root = Path(__file__).resolve().parent.parent
    src_path = repo_root / "assets" / "sable_mascot.jpg"
    if not src_path.exists():
        src_path = repo_root.parent / "RustUtilityApp" / "assets" / "sable_mascot.jpg"

    src = Image.open(src_path).convert("RGBA")

    # Cut out exterior white background via floodfill
    gray = src.convert("L")
    bw = gray.point(lambda p: 255 if p > 235 else 0)
    expanded = ImageOps.expand(bw, border=2, fill=255)
    ImageDraw.floodfill(expanded, (0, 0), 128, thresh=10)
    exterior_mask = expanded.crop((2, 2, 1026, 1026))
    ext_arr = np.array(exterior_mask) == 128

    alpha = np.where(ext_arr, 0, 255).astype(np.uint8)
    mascot_alpha = Image.fromarray(alpha).filter(ImageFilter.GaussianBlur(0.8))

    mascot_cutout = src.copy()
    mascot_cutout.putalpha(mascot_alpha)

    # Rich, sophisticated deep emerald gradient background
    # Top: #10783e (16, 120, 62) -> Bottom: #083e20 (8, 62, 32)
    bg = Image.new("RGBA", (1024, 1024))
    for y in range(1024):
        t = y / 1023.0
        r = int(16 * (1 - t) + 8 * t)
        g = int(120 * (1 - t) + 62 * t)
        b = int(62 * (1 - t) + 32 * t)
        ImageDraw.Draw(bg).line([(0, y), (1024, y)], fill=(r, g, b, 255))

    # Squircle mask (standard macOS squircle)
    mask = Image.new("L", (1024, 1024), 0)
    draw = ImageDraw.Draw(mask)
    margin = 72
    draw.rounded_rectangle(
        [margin, margin, 1024 - margin, 1024 - margin],
        radius=195,
        fill=255,
    )

    icon = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
    icon.paste(bg, (0, 0), mask)

    # Subtle inner bevel outline
    rim_draw = ImageDraw.Draw(icon)
    rim_draw.rounded_rectangle(
        [margin, margin, 1024 - margin, 1024 - margin],
        radius=195,
        outline=(255, 255, 255, 45),
        width=2,
    )

    # Mascot overlay
    mascot_scaled = mascot_cutout.resize((740, 740), Image.Resampling.LANCZOS)
    offset_x = (1024 - 740) // 2
    offset_y = (1024 - 740) // 2 - 8
    icon.alpha_composite(mascot_scaled, (offset_x, offset_y))
    return icon

def main() -> None:
    if len(sys.argv) < 2:
        print("Usage: make_icon.py <output_iconset_dir>")
        sys.exit(1)

    out = Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)

    base = generate_base_icon()

    names = {
        16: ["icon_16x16.png"],
        32: ["icon_16x16@2x.png", "icon_32x32.png"],
        64: ["icon_32x32@2x.png"],
        128: ["icon_128x128.png"],
        256: ["icon_128x128@2x.png", "icon_256x256.png"],
        512: ["icon_256x256@2x.png", "icon_512x512.png"],
        1024: ["icon_512x512@2x.png"],
    }

    for size, files in names.items():
        resized = base.resize((size, size), Image.Resampling.LANCZOS)
        for name in files:
            resized.save(out / name, format="PNG")

if __name__ == "__main__":
    main()
