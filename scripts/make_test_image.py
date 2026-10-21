# /// script
# requires-python = ">=3.10"
# dependencies = ["pillow>=10.1"]
# ///
"""Generate the smoke images in core/tests/data: black text on white."""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

IMAGES = {"hello_world.png": ("HELLO WORLD", 640), "bonjour.png": ("Bonjour le monde", 760)}

for name, (text, width) in IMAGES.items():
    out = Path("core/tests/data") / name
    out.parent.mkdir(parents=True, exist_ok=True)
    img = Image.new("RGB", (width, 200), "white")
    ImageDraw.Draw(img).text((40, 60), text, fill="black", font=ImageFont.load_default(size=72))
    img.save(out)
    print(f"wrote {out}")
