#!/usr/bin/env python3
"""Generate app icons and in-app logo images from design/logo.png.

Run from the repo root: uv run --no-project --with pillow python scripts/gen-icons.py
"""
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "design" / "logo.png"
ANDROID_RES = ROOT / "android" / "app" / "src" / "main" / "res"
IOS_ASSETS = ROOT / "ios" / "Translatte" / "Assets.xcassets"

# Cup geometry in the 1254px source. The cup is a slightly tall ellipse; the handle lies outside it.
CX, CY, CUP_R = 588, 632, 492
CUP_RX = 478
FOAM_R = 400
CREAM = (0xF3, 0xE6, 0xD3, 255)
SHADOW = (40, 20, 5)

ANDROID_DENSITIES = {"mdpi": 108, "hdpi": 162, "xhdpi": 216, "xxhdpi": 324, "xxxhdpi": 432}
# Adaptive icons are cropped to the inner 66/108; keep the cup (plus shadow) inside it.
ADAPTIVE_CUP_FRAC = 0.58
IOS_CUP_FRAC = 0.70
LOGO_PT = 132


def cup() -> Image.Image:
    im = Image.open(SRC).convert("RGBA")
    c = im.crop((CX - CUP_R, CY - CUP_R, CX + CUP_R, CY + CUP_R))
    mask = Image.new("L", c.size, 0)
    dx = CUP_R - CUP_RX
    ImageDraw.Draw(mask).ellipse((dx, 0, c.width - 1 - dx, c.height - 1), fill=255)
    mask = mask.filter(ImageFilter.GaussianBlur(1.5))
    c.putalpha(Image.composite(c.getchannel("A"), Image.new("L", c.size, 0), mask))
    return c


def place(fg: Image.Image, size: int, frac: float, bg, shadow: bool) -> Image.Image:
    canvas = Image.new("RGBA", (size, size), bg)
    d = round(size * frac)
    fg = fg.resize((d, d), Image.LANCZOS)
    off = ((size - d) // 2, (size - d) // 2)
    if shadow:
        blob = Image.new("RGBA", fg.size, SHADOW + (0,))
        blob.putalpha(fg.getchannel("A").point(lambda v: v * 90 // 255))
        layer = Image.new("RGBA", (size, size), (0, 0, 0, 0))
        layer.paste(blob, (off[0], off[1] + round(size * 0.018)), blob)
        canvas = Image.alpha_composite(canvas, layer.filter(ImageFilter.GaussianBlur(size * 0.021)))
    canvas.alpha_composite(fg, off)
    return canvas


def monochrome(c: Image.Image) -> Image.Image:
    """A clean ring plus the foam art, for Android 13 themed icons."""
    size = c.width
    mid = size / 2
    alpha = Image.new("L", c.size, 0)
    d = ImageDraw.Draw(alpha)
    ring = round(size * 0.035)
    d.ellipse((mid - CUP_RX + 4, 4, mid + CUP_RX - 4, size - 4), fill=255)
    d.ellipse((mid - CUP_RX + 4 + ring, 4 + ring, mid + CUP_RX - 4 - ring, size - 4 - ring), fill=0)
    inner = Image.new("L", c.size, 0)
    ImageDraw.Draw(inner).ellipse((mid - FOAM_R, mid - FOAM_R, mid + FOAM_R, mid + FOAM_R), fill=255)
    foam = c.convert("L").point(lambda v: 255 if v > 190 else 0)
    alpha.paste(255, mask=Image.composite(foam, Image.new("L", c.size, 0), inner))
    out = Image.new("RGBA", c.size, (255, 255, 255, 0))
    out.putalpha(alpha.filter(ImageFilter.GaussianBlur(1)))
    return out


def main() -> None:
    c = cup()
    mono = monochrome(c)
    for density, px in ANDROID_DENSITIES.items():
        out = ANDROID_RES / f"mipmap-{density}"
        out.mkdir(parents=True, exist_ok=True)
        place(c, px, ADAPTIVE_CUP_FRAC, (0, 0, 0, 0), True).save(out / "ic_launcher_foreground.png")
        place(mono, px, ADAPTIVE_CUP_FRAC, (0, 0, 0, 0), False).save(out / "ic_launcher_monochrome.png")
    nodpi = ANDROID_RES / "drawable-nodpi"
    nodpi.mkdir(parents=True, exist_ok=True)
    c.resize((LOGO_PT * 3, LOGO_PT * 3), Image.LANCZOS).save(nodpi / "logo_cup.png")

    icon = IOS_ASSETS / "AppIcon.appiconset"
    icon.mkdir(parents=True, exist_ok=True)
    place(c, 1024, IOS_CUP_FRAC, CREAM, True).convert("RGB").save(icon / "AppIcon-1024.png")
    logo = IOS_ASSETS / "LogoCup.imageset"
    logo.mkdir(parents=True, exist_ok=True)
    for scale in (1, 2, 3):
        c.resize((LOGO_PT * scale,) * 2, Image.LANCZOS).save(logo / f"logo_cup@{scale}x.png")


if __name__ == "__main__":
    main()
