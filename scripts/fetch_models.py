# /// script
# requires-python = ">=3.10"
# dependencies = ["huggingface_hub>=0.25", "pyyaml>=6"]
# ///
"""Download upstream model files into a local models dir and write models/manifest.json.

The output dir layout is exactly what gets mirrored to the translatte-models HF repo (M2),
so manifest paths are valid both locally and remotely.
"""

import argparse
import hashlib
import json
import shutil
from pathlib import Path

import yaml
from huggingface_hub import HfApi, hf_hub_download

DET_REPO = "PaddlePaddle/PP-OCRv5_mobile_det_onnx"
TRANSLATOR_REPO = "mradermacher/translategemma-4b-it-GGUF"
TRANSLATOR_FILE = "translategemma-4b-it.Q4_K_M.gguf"
DEFAULT_BASE_URL = "https://huggingface.co/assafvayner/translatte-models/resolve/main"

SCRIPT_PACKS = {
    "rec-cjk": ("PaddlePaddle/PP-OCRv5_mobile_rec_onnx", "Chinese / Japanese"),
    "rec-en": ("PaddlePaddle/en_PP-OCRv5_mobile_rec_onnx", "English"),
    "rec-latin": ("PaddlePaddle/latin_PP-OCRv5_mobile_rec_onnx", "Latin script"),
    "rec-eslav": ("PaddlePaddle/eslav_PP-OCRv5_mobile_rec_onnx", "Cyrillic (Russian, Ukrainian, Belarusian)"),
    "rec-cyrillic": ("PaddlePaddle/cyrillic_PP-OCRv5_mobile_rec_onnx", "Cyrillic (other)"),
    "rec-korean": ("PaddlePaddle/korean_PP-OCRv5_mobile_rec_onnx", "Korean"),
    "rec-arabic": ("PaddlePaddle/arabic_PP-OCRv5_mobile_rec_onnx", "Arabic script"),
    "rec-devanagari": ("PaddlePaddle/devanagari_PP-OCRv5_mobile_rec_onnx", "Devanagari"),
    "rec-th": ("PaddlePaddle/th_PP-OCRv5_mobile_rec_onnx", "Thai"),
    "rec-el": ("PaddlePaddle/el_PP-OCRv5_mobile_rec_onnx", "Greek"),
    "rec-ta": ("PaddlePaddle/ta_PP-OCRv5_mobile_rec_onnx", "Tamil"),
    "rec-te": ("PaddlePaddle/te_PP-OCRv5_mobile_rec_onnx", "Telugu"),
}

# (code, name, script pack or None for target-only, rtl, spaced)
LATIN = [
    ("fr", "French"), ("de", "German"), ("es", "Spanish"), ("it", "Italian"), ("pt", "Portuguese"),
    ("nl", "Dutch"), ("pl", "Polish"), ("cs", "Czech"), ("ro", "Romanian"), ("sv", "Swedish"),
    ("da", "Danish"), ("no", "Norwegian"), ("fi", "Finnish"), ("hu", "Hungarian"),
    ("tr", "Turkish"), ("id", "Indonesian"), ("hr", "Croatian"), ("sk", "Slovak"),
    ("sl", "Slovenian"), ("lt", "Lithuanian"), ("lv", "Latvian"), ("et", "Estonian"), ("ca", "Catalan"),
]
LANGUAGES = (
    [("en", "English", "rec-en", False, True)]
    + [(c, n, "rec-latin", False, True) for c, n in LATIN]
    + [
        ("zh", "Chinese", "rec-cjk", False, False),
        ("zh-TW", "Chinese (Traditional)", "rec-cjk", False, False),
        ("ja", "Japanese", "rec-cjk", False, False),
        ("ko", "Korean", "rec-korean", False, True),
        ("ru", "Russian", "rec-eslav", False, True),
        ("uk", "Ukrainian", "rec-eslav", False, True),
        ("bg", "Bulgarian", "rec-cyrillic", False, True),
        ("sr", "Serbian", "rec-cyrillic", False, True),
        ("ar", "Arabic", "rec-arabic", True, True),
        ("fa", "Persian", "rec-arabic", True, True),
        ("ur", "Urdu", "rec-arabic", True, True),
        ("hi", "Hindi", "rec-devanagari", False, True),
        ("mr", "Marathi", "rec-devanagari", False, True),
        ("th", "Thai", "rec-th", False, False),
        ("el", "Greek", "rec-el", False, True),
        ("ta", "Tamil", "rec-ta", False, True),
        ("te", "Telugu", "rec-te", False, True),
        ("he", "Hebrew", None, True, True),
        ("vi", "Vietnamese", None, False, True),
    ]
)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def file_entry(out_dir: Path, rel: str) -> dict:
    p = out_dir / rel
    return {"path": rel, "size": p.stat().st_size, "sha256": sha256_file(p)}


def fetch(repo: str, filename: str, dest: Path) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    if dest.exists():
        return
    src = hf_hub_download(repo, filename)
    shutil.copyfile(src, dest)


def write_dict(yml_path: Path, dest: Path) -> None:
    cfg = yaml.safe_load(yml_path.read_text(encoding="utf-8"))
    chars = cfg["PostProcess"]["character_dict"]
    assert all(isinstance(c, str) and "\n" not in c for c in chars), "unexpected dict entry"
    dest.write_text("\n".join(chars) + "\n", encoding="utf-8")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out-dir", type=Path, default=Path("models-cache"))
    ap.add_argument("--manifest", type=Path, default=Path("models/manifest.json"))
    ap.add_argument("--base-url", default=DEFAULT_BASE_URL)
    ap.add_argument("--skip-translator", action="store_true", help="don't download the 2.5GB GGUF; manifest uses HF metadata")
    args = ap.parse_args()
    out: Path = args.out_dir

    models = []

    fetch(DET_REPO, "inference.onnx", out / "det/inference.onnx")
    models.append({"id": "det", "kind": "core", "display_name": "Text detector",
                   "files": [file_entry(out, "det/inference.onnx")]})

    translator_rel = "translator/model.gguf"
    if args.skip_translator:
        info = HfApi().get_paths_info(TRANSLATOR_REPO, [TRANSLATOR_FILE], expand=True)[0]
        translator_file = {"path": translator_rel, "size": info.size, "sha256": info.lfs.sha256}
    else:
        fetch(TRANSLATOR_REPO, TRANSLATOR_FILE, out / translator_rel)
        translator_file = file_entry(out, translator_rel)
    models.append({"id": "translator", "kind": "core", "display_name": "Translator (TranslateGemma 4B)",
                   "files": [translator_file]})

    for pack, (repo, display) in SCRIPT_PACKS.items():
        fetch(repo, "inference.onnx", out / pack / "inference.onnx")
        yml = Path(hf_hub_download(repo, "inference.yml"))
        write_dict(yml, out / pack / "dict.txt")
        models.append({"id": pack, "kind": "script_pack", "display_name": display,
                       "files": [file_entry(out, f"{pack}/inference.onnx"), file_entry(out, f"{pack}/dict.txt")]})

    languages = [{"code": c, "name": n, "script_pack": p, "rtl": rtl, "spaced": sp}
                 for c, n, p, rtl, sp in LANGUAGES]
    manifest = {"version": 1, "base_url": args.base_url, "models": models, "languages": languages}
    args.manifest.parent.mkdir(parents=True, exist_ok=True)
    args.manifest.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"wrote {args.manifest} ({len(models)} models, {len(languages)} languages)")


if __name__ == "__main__":
    main()
