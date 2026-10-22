# /// script
# requires-python = ">=3.10"
# dependencies = ["pillow>=10.1"]
# ///
"""Renders the diagonal-text test images in core/tests/data: paragraphs rotated counter-clockwise
(rising to the right), drawn straight onto a plain background since the detector boxes a
sharp-edged card as a whole.

    uv run scripts/make_diagonal_fixtures.py
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
BG = (250, 245, 225)
INK = (20, 30, 90, 255)


def font(paths, size):
    for p in paths:
        if Path(p).exists():
            return ImageFont.truetype(p, size)
    raise SystemExit(f"none of {paths} found")


def paragraph(lines, fonts, size, angle):
    f = font(fonts, size)
    lh = int(size * 1.4)
    img = Image.new("RGBA", (max(f.getbbox(l)[2] for l in lines) + 40, lh * len(lines) + 40))
    d = ImageDraw.Draw(img)
    for i, l in enumerate(lines):
        d.text((20, 20 + i * lh), l, font=f, fill=INK)
    return img.rotate(angle, expand=True, resample=Image.BICUBIC)


def canvas(size, *placed):
    img = Image.new("RGB", size, BG)
    for block, (cx, cy) in placed:
        img.paste(block, (cx - block.width // 2, cy - block.height // 2), block)
    return img.quantize(32)


OUT.mkdir(parents=True, exist_ok=True)
FR = ["Ne pas stationner devant", "la porte du garage"]
JA = ["ドアの前に駐車しないで", "ください。ご協力を", "お願いいたします。"]
canvas((640, 640), (paragraph(FR, LATIN_FONTS, 40, 30), (320, 320))).save(
    OUT / "diagonal_fr_30.png", optimize=True
)
canvas((640, 640), (paragraph(JA, CJK_FONTS, 40, 15), (320, 320))).save(
    OUT / "diagonal_ja_15.png", optimize=True
)
canvas(
    (720, 900),
    (paragraph(FR, LATIN_FONTS, 40, 60), (360, 300)),
    (paragraph(["Sortie de secours"], LATIN_FONTS, 40, 0), (360, 760)),
).save(OUT / "diagonal_mixed.png", optimize=True)
