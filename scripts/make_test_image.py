# /// script
# requires-python = ">=3.10"
# dependencies = ["pillow>=10.1"]
# ///
"""Generate core/tests/data/hello_world.png: black 'HELLO WORLD' on white."""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

out = Path("core/tests/data/hello_world.png")
out.parent.mkdir(parents=True, exist_ok=True)
img = Image.new("RGB", (640, 200), "white")
ImageDraw.Draw(img).text((40, 60), "HELLO WORLD", fill="black", font=ImageFont.load_default(size=72))
img.save(out)
print(f"wrote {out}")
