# /// script
# requires-python = ">=3.10"
# dependencies = ["pillow>=10.1"]
# ///
"""Renders the vertical-text test images in core/tests/data.

    uv run scripts/make_vertical_fixtures.py
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

OUT = Path(__file__).resolve().parent.parent / "core/tests/data"
LATIN_FONTS = ["/System/Library/Fonts/Helvetica.ttc", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"]
CJK_FONTS = [
    "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
]
ROTATED = "ー"


def font(paths, size):
    for p in paths:
        if Path(p).exists():
            return ImageFont.truetype(p, size)
    raise SystemExit(f"none of {paths} found")


def horizontal(text, size):
    f = font(LATIN_FONTS, size)
    x0, y0, x1, y1 = f.getbbox(text)
    img = Image.new("RGB", (x1 - x0 + 40, y1 - y0 + 40), "white")
    ImageDraw.Draw(img).text((20 - x0, 20 - y0), text, font=f, fill="black")
    return img


def on_canvas(img, size=(400, 640), bg=(235, 235, 230)):
    canvas = Image.new("RGB", size, bg)
    canvas.paste(img, ((size[0] - img.width) // 2, (size[1] - img.height) // 2))
    return canvas


def upright_glyph(ch, f, size):
    """Long-vowel marks are drawn vertically in vertical writing."""
    glyph = Image.new("RGB", (size, size), "white")
    ImageDraw.Draw(glyph).text((0, 0), ch, font=f, fill="black")
    return glyph.rotate(-90) if ch in ROTATED else glyph


def columns(cols, size):
    """Top-to-bottom CJK columns, first column on the right."""
    f = font(CJK_FONTS, size)
    pitch = int(size * 1.6)
    rows = max(len(c) for c in cols)
    img = Image.new("RGB", (pitch * len(cols) + 80, int(size * 1.15) * rows + 80), "white")
    for i, col in enumerate(cols):
        x = img.width - 40 - pitch * (i + 1) + (pitch - size) // 2
        for j, ch in enumerate(col):
            img.paste(upright_glyph(ch, f, size), (x, 40 + j * int(size * 1.15)))
    return img


OUT.mkdir(parents=True, exist_ok=True)
line = horizontal("PAIN FRAIS DU JOUR", 44)
on_canvas(line.rotate(-90, expand=True)).save(OUT / "vertical_en_down.png")
on_canvas(line.rotate(90, expand=True)).save(OUT / "vertical_en_up.png")
on_canvas(horizontal("1", 120), bg="white").save(OUT / "lone_glyph.png")
columns(["本日のおすすめ", "醤油ラーメン", "味噌ラーメン"], 44).save(OUT / "vertical_ja.png")
