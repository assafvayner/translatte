#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.10"
# dependencies = ["huggingface_hub>=0.24", "pyarrow", "pillow"]
# ///
"""Download a pinned, deterministic sample of real scene-text photos into fixtures-cache/.

Images and annotations are never committed; run this script to regenerate them.
Downloaded files are treated as data only (JSON/parquet/images), never executed.
"""

import argparse
import hashlib
import io
import json
import re
import sys
from pathlib import Path

import pyarrow.parquet as pq
from huggingface_hub import HfApi, hf_hub_download
from PIL import Image, ImageOps

ROOT = Path(__file__).resolve().parent.parent
MAX_SIDE = 2000
MAX_ENTRIES = 40
MIN_ENTRIES = 2

NEXDATA = ("Nexdata-kr/500000-Images-Multilingual-OCR-Dataset-in-21-Languages", "3cc359db8896b7c9ec06785237a38ad89244e4b7")
HINDI = ("interfaze-ai/hindi-scene-text", "af0470e2341581391555f7ebd6bab52c275e87fc")
EVAREST = ("Melaraby/EvArEST-dataset-for-Arabic-scene-text-detection", "bede1fdbaac01f5b3d67e2f51481ebe07727bffa")
TOTALTEXT = ("Voxel51/Total-Text-Dataset", "0ce21350d2ed3f47f7269c7a19d0eeb702952ace")

NEXDATA_LANGS = {
    "Chinese": "zh",
    "Japanese": "ja",
    "Korean": "ko",
    "Russian": "ru",
    "Hindi": "hi",
    "French": "fr",
    "German": "de",
    "Spanish": "es",
    "Italian": "it",
    "Polish": "pl",
    "Portuguese": "pt",
    "Indonesian": "id",
}

PER_LANG = {"en": 8, "ar": 8, "hi": 8, "ko": 6, "ru": 6, "zh": 6, "ja": 4}
DEFAULT_PER_LANG = 5

RTL = {"ar"}
LATIN_LANGS = {"en", "fr", "de", "es", "it", "pt", "nl", "pl", "cs", "ro", "sv", "da", "no", "fi", "hu", "tr", "id", "hr", "sk", "sl", "lt", "lv", "et", "ca"}


def script_of(ch):
    o = ord(ch)
    if 0x4E00 <= o <= 0x9FFF or 0x3400 <= o <= 0x4DBF:
        return "han"
    if 0x3040 <= o <= 0x30FF:
        return "kana"
    if 0xAC00 <= o <= 0xD7AF or 0x1100 <= o <= 0x11FF or 0x3130 <= o <= 0x318F:
        return "hangul"
    if 0x0400 <= o <= 0x04FF:
        return "cyrillic"
    if 0x0900 <= o <= 0x097F:
        return "devanagari"
    if 0x0600 <= o <= 0x06FF or 0x0750 <= o <= 0x077F or 0xFB50 <= o <= 0xFDFF or 0xFE70 <= o <= 0xFEFF:
        return "arabic"
    if 0x0E00 <= o <= 0x0E7F:
        return "thai"
    if 0x0370 <= o <= 0x03FF:
        return "greek"
    if ch.isalpha() and o < 0x250:
        return "latin"
    return None


def accepted_scripts(lang):
    if lang in LATIN_LANGS:
        return {"latin"}
    return {
        "zh": {"han"},
        "zh-TW": {"han"},
        "ja": {"han", "kana"},
        "ko": {"hangul"},
        "ru": {"cyrillic"},
        "hi": {"devanagari"},
        "ar": {"arabic"},
    }[lang]


def keep_text(text, lang):
    if not text or "#" in text or "*" in text:
        return False
    scripts = [s for s in map(script_of, text) if s]
    if not scripts:
        return False
    ok = accepted_scripts(lang)
    return sum(s in ok for s in scripts) * 2 >= len(scripts)


def stable_key(name):
    return hashlib.sha1(name.encode()).hexdigest()


def group_words(words, rtl):
    """Merge (text, (x0, y0, x1, y1)) words into reading-order lines."""
    lines = []
    for text, box in sorted(words, key=lambda w: (w[1][1] + w[1][3]) / 2):
        x0, y0, x1, y1 = box
        h = y1 - y0
        for line in lines:
            lx0, ly0, lx1, ly1 = line["box"]
            overlap = min(y1, ly1) - max(y0, ly0)
            gap = max(x0 - lx1, lx0 - x1, 0)
            if overlap > 0.5 * min(h, ly1 - ly0) and gap < 0.8 * max(h, ly1 - ly0):
                line["words"].append((text, box))
                line["box"] = (min(lx0, x0), min(ly0, y0), max(lx1, x1), max(ly1, y1))
                break
        else:
            lines.append({"words": [(text, box)], "box": box})
    out = []
    for line in sorted(lines, key=lambda l: (l["box"][1], l["box"][0])):
        ws = sorted(line["words"], key=lambda w: w[1][0], reverse=rtl)
        out.append(" ".join(w[0] for w in ws))
    return out


def bbox(points):
    xs = [p[0] for p in points]
    ys = [p[1] for p in points]
    return (min(xs), min(ys), max(xs), max(ys))


def save(img, name, lang, lines, out_dir):
    name = re.sub(r"[^\w.-]", "_", name)
    img = ImageOps.exif_transpose(img).convert("RGB")
    img.thumbnail((MAX_SIDE, MAX_SIDE), Image.LANCZOS)
    img.save(out_dir / f"{name}.jpg", quality=90)
    spec = {"image": f"{name}.jpg", "lang": lang, "lines": lines}
    (out_dir / f"{name}.json").write_text(json.dumps(spec, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")


def ok_count(lines):
    return MIN_ENTRIES <= len(lines) <= MAX_ENTRIES


def fetch_nexdata(out_dir, quotas):
    repo, rev = NEXDATA
    files = HfApi().list_repo_files(repo, repo_type="dataset", revision=rev)
    jsons = [f for f in files if f.endswith(".json") and "/Natural scenes/" in f]
    count = {}
    for path in sorted(jsons, key=stable_key):
        lang = NEXDATA_LANGS.get(path.split("/data/")[1].split("/")[0])
        if lang is None or count.get(lang, 0) >= quotas[lang]:
            continue
        spec = json.loads(Path(hf_hub_download(repo, path, repo_type="dataset", revision=rev)).read_text(encoding="utf-8"))
        lines = []
        for entry in spec.get("lines", []):
            for part in str(entry.get("transcription", "")).splitlines():
                part = part.strip()
                if keep_text(part, lang):
                    lines.append(part)
        if not ok_count(lines):
            continue
        img_path = path[: -len(".json")] + ".jpg"
        if img_path not in files:
            continue
        img = Image.open(hf_hub_download(repo, img_path, repo_type="dataset", revision=rev))
        save(img, f"{lang}_nexdata_{Path(path).stem}", lang, lines, out_dir)
        count[lang] = count.get(lang, 0) + 1
    return count


def fetch_hindi(out_dir, quota):
    repo, rev = HINDI
    table = pq.read_table(hf_hub_download(repo, "data/train-00000-of-00001.parquet", repo_type="dataset", revision=rev))
    rows = table.to_pylist()
    done = 0
    for idx in sorted(range(len(rows)), key=lambda i: stable_key(f"hindi{i}")):
        if done >= quota:
            break
        ann = rows[idx]["annotations"]
        words = []
        for poly, text, valid, illeg in zip(ann["polygon"], ann["text"], ann["valid"], ann["illegibility"]):
            if valid and not illeg and keep_text(text, "hi"):
                words.append((text, bbox(poly)))
        lines = group_words(words, rtl=False)
        if not ok_count(lines) or len(words) < 3:
            continue
        save(Image.open(io.BytesIO(rows[idx]["image"]["bytes"])), f"hi_hindiscenetext_{idx:04d}", "hi", lines, out_dir)
        done += 1
    return done


def fetch_evarest(out_dir, quota):
    repo, rev = EVAREST
    table = pq.read_table(hf_hub_download(repo, "test_detection.parquet", repo_type="dataset", revision=rev))
    rows = table.to_pylist()
    done = 0
    for idx in sorted(range(len(rows)), key=lambda i: stable_key(f"evarest{i}")):
        if done >= quota:
            break
        words = []
        for inst in json.loads(rows[idx]["instances"]):
            if inst.get("language") == "Arabic" and keep_text(inst.get("text", ""), "ar"):
                c = inst["coordinates"]
                words.append((inst["text"], bbox(list(zip(c[0::2], c[1::2])))))
        lines = group_words(words, rtl=True)
        if not ok_count(lines) or len(words) < 3:
            continue
        save(Image.open(io.BytesIO(rows[idx]["image_bytes"])), f"ar_evarest_{rows[idx]['image_name']}", "ar", lines, out_dir)
        done += 1
    return done


def fetch_totaltext(out_dir, quota):
    repo, rev = TOTALTEXT
    samples = json.loads(Path(hf_hub_download(repo, "samples.json", repo_type="dataset", revision=rev)).read_text(encoding="utf-8"))["samples"]
    done = 0
    for s in sorted(samples, key=lambda s: stable_key(s["filepath"])):
        if done >= quota:
            break
        polys = s.get("ground_truth_polylines", {}).get("polylines", [])
        if not 3 <= len(polys) <= MAX_ENTRIES * 3:
            continue
        img = Image.open(hf_hub_download(repo, s["filepath"], repo_type="dataset", revision=rev))
        img = ImageOps.exif_transpose(img)
        w, h = img.size
        words = []
        for p in polys:
            pts = [(x * w, y * h) for ring in p["points"] for x, y in ring]
            if keep_text(p["label"], "en") and pts:
                words.append((p["label"], bbox(pts)))
        lines = group_words(words, rtl=False)
        if not ok_count(lines) or len(words) < 3:
            continue
        save(img, f"en_totaltext_{Path(s['filepath']).stem}", "en", lines, out_dir)
        done += 1
    return done


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", type=Path, default=ROOT / "fixtures-cache")
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    for stale in [*args.out.glob("*.jpg"), *args.out.glob("*.png"), *args.out.glob("*.json")]:
        stale.unlink()
    quotas = {lang: PER_LANG.get(lang, DEFAULT_PER_LANG) for lang in NEXDATA_LANGS.values()}
    # Nexdata supplies at most 4 Hindi images; the rest of the hi quota is filled from hindi-scene-text.
    quotas["hi"] = 4
    totals = fetch_nexdata(args.out, quotas)
    totals["hi"] = totals.get("hi", 0) + fetch_hindi(args.out, PER_LANG["hi"] - totals.get("hi", 0))
    totals["ar"] = fetch_evarest(args.out, PER_LANG["ar"])
    totals["en"] = fetch_totaltext(args.out, PER_LANG["en"])
    for lang, n in sorted(totals.items()):
        print(f"{lang:<6}{n}")
    print(f"total {sum(totals.values())} fixtures in {args.out}")


if __name__ == "__main__":
    sys.exit(main())
