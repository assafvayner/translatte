#!/usr/bin/env python3
"""Renders a synthetic clip for testing live mode: a text image pans in from the left, holds still
(with a slight hand shake), then slides off the top right, on a portrait 720x1280 30fps frame.

Usage: scripts/live/make-clip.py [out.mp4] [--image core/tests/data/bonjour.png] [--rotate]
--rotate stores the frames landscape with display rotation metadata, as phones record portrait
clips. Needs ffmpeg.
"""
import argparse
import pathlib
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[2]
W, H, FPS = 720, 1280, 30
# Phases in seconds: blank, pan in, hold, slide off, blank.
BLANK, PAN, HOLD, OFF, TAIL = 1.0, 3.0, 4.0, 3.0, 1.0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("out", nargs="?", default=str(ROOT / "out/live/synthetic.mp4"))
    ap.add_argument("--image", default=str(ROOT / "core/tests/data/bonjour.png"))
    ap.add_argument("--rotate", action="store_true")
    args = ap.parse_args()
    if not shutil.which("ffmpeg"):
        sys.exit("ffmpeg not found (brew install ffmpeg)")

    duration = BLANK + PAN + HOLD + OFF + TAIL
    t1, t2, t3 = BLANK, BLANK + PAN, BLANK + PAN + HOLD
    left = 40
    mid = f"(H-h)/2"
    x = (
        f"if(lt(t,{t1}),-w,"
        f"if(lt(t,{t2}),-w+(t-{t1})/{PAN}*(w+{left}),"
        f"if(lt(t,{t3}),{left}+2*sin(t*9),"
        f"{left}+(t-{t3})/{OFF}*(W-{left}))))"
    )
    y = f"if(lt(t,{t3}),{mid}+2*sin(t*7),{mid}-(t-{t3})/{OFF}*({mid}+h))"
    graph = f"[1]scale={W - 2 * left}:-2[t];[0][t]overlay=x='{x}':y='{y}':eval=frame:shortest=1"
    if args.rotate:
        graph += ",transpose=2"

    out = pathlib.Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        rendered = pathlib.Path(tmp) / "rendered.mp4" if args.rotate else out
        subprocess.run(
            [
                "ffmpeg", "-v", "error", "-y",
                "-f", "lavfi", "-i", f"color=c=0xEDE7DF:s={W}x{H}:r={FPS}:d={duration}",
                "-loop", "1", "-framerate", str(FPS), "-i", args.image,
                "-filter_complex", graph,
                "-c:v", "libx264", "-pix_fmt", "yuv420p", "-r", str(FPS), "-t", str(duration),
                str(rendered),
            ],
            check=True,
        )
        if args.rotate:
            subprocess.run(
                ["ffmpeg", "-v", "error", "-y", "-display_rotation", "-90", "-i", str(rendered), "-c", "copy", str(out)],
                check=True,
            )
    print(out)


if __name__ == "__main__":
    main()
