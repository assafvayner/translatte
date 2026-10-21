# /// script
# requires-python = ">=3.10"
# dependencies = ["huggingface_hub>=0.25", "pyyaml>=6"]
# ///
"""Generate models/manifest.json from the pinned upstream repos in REVISIONS.

Sizes of upstream files come from Hub metadata; derived files (rec-*/dict.txt) are
computed from the pinned inference.yml. Models are downloaded by `translatte fetch`, which reads
`source`/`source_path`/`derive` from the manifest and fetches straight from these repos.
"""

import argparse
import json
from pathlib import Path

import yaml
from huggingface_hub import HfApi, hf_hub_download

DET_REPO = "PaddlePaddle/PP-OCRv5_mobile_det_onnx"
# Mozilla translations models (MPL-2.0), mirrored unmodified by scripts/mirror_pair_models.py.
PAIR_REPO = "assafvayner/translatte-pair-models"

SCRIPT_PACKS = {
    "rec-cjk": ("PaddlePaddle/PP-OCRv5_mobile_rec_onnx", "Chinese / Japanese"),
    "rec-latin": ("PaddlePaddle/latin_PP-OCRv5_mobile_rec_onnx", "Latin script"),
    "rec-eslav": ("PaddlePaddle/eslav_PP-OCRv5_mobile_rec_onnx", "Cyrillic script"),
    "rec-korean": ("PaddlePaddle/korean_PP-OCRv5_mobile_rec_onnx", "Korean"),
    "rec-arabic": ("PaddlePaddle/arabic_PP-OCRv5_mobile_rec_onnx", "Arabic script"),
    "rec-devanagari": ("PaddlePaddle/devanagari_PP-OCRv5_mobile_rec_onnx", "Devanagari"),
    "rec-th": ("PaddlePaddle/th_PP-OCRv5_mobile_rec_onnx", "Thai"),
    "rec-ta": ("PaddlePaddle/ta_PP-OCRv5_mobile_rec_onnx", "Tamil"),
    "rec-te": ("PaddlePaddle/te_PP-OCRv5_mobile_rec_onnx", "Telugu"),
}

REVISIONS = {
    "PaddlePaddle/PP-OCRv5_mobile_det_onnx": "e6f4fa85f00e168c862bc462aebca69eef9b3d3d",
    PAIR_REPO: "6b248f2c3d44fdb7f9febcc9ca8e1adbd3cb781f",
    "PaddlePaddle/PP-OCRv5_mobile_rec_onnx": "ed152b8b495f84de93cda5709d768548a9127622",
    "PaddlePaddle/latin_PP-OCRv5_mobile_rec_onnx": "89d3a50e2c27e2e7cceeab0e944c25c807d5db4f",
    "PaddlePaddle/eslav_PP-OCRv5_mobile_rec_onnx": "9a32171fc5718746875e1a261818884517975013",
    "PaddlePaddle/korean_PP-OCRv5_mobile_rec_onnx": "5c6f574b8e2230adf4287b33e736d71b9fabd28e",
    "PaddlePaddle/arabic_PP-OCRv5_mobile_rec_onnx": "14aaedcd75825982689ecf5cd64ab33ee083215a",
    "PaddlePaddle/devanagari_PP-OCRv5_mobile_rec_onnx": "251aec19e36739540d35e2cc943f6aa7503b98e5",
    "PaddlePaddle/th_PP-OCRv5_mobile_rec_onnx": "1d4adbbafb1034a2fd6618498575b81ea7b69f69",
    "PaddlePaddle/ta_PP-OCRv5_mobile_rec_onnx": "0db89798af0d5218fad9ea7ace0557526e9077f1",
    "PaddlePaddle/te_PP-OCRv5_mobile_rec_onnx": "f1197c83430241c58c64b0952b3a80366d04eeba",
}

# Local name of each file in a mirrored pair directory, by upstream name prefix.
PAIR_FILES = {
    "model.": "model.bin",
    "lex.": "lex.bin",
    "vocab.": "vocab.spm",
    "srcvocab.": "srcvocab.spm",
    "trgvocab.": "trgvocab.spm",
}

TARGET = {"code": "en", "name": "English", "script_pack": None, "rtl": False, "spaced": True}

# Source languages: (code, name, script pack, mirror directory of its X->en pair model, rtl, spaced).
# Only languages whose alphabet the pack's dictionary covers are listed (check with scripts/ocr_coverage.py);
# the other mirrored xx->en pairs (az, bn, el, gu, he, kn, ml, vi) have no pack that reads them. Mozilla's
# Serbian model takes both scripts, but only the Latin one has a pack.
LATIN = [
    ("fr", "French"), ("de", "German"), ("es", "Spanish"), ("it", "Italian"), ("pt", "Portuguese"),
    ("nl", "Dutch"), ("pl", "Polish"), ("cs", "Czech"), ("tr", "Turkish"), ("id", "Indonesian"),
    ("ms", "Malay"), ("tl", "Filipino"), ("af", "Afrikaans"), ("bs", "Bosnian"), ("ca", "Catalan"),
    ("da", "Danish"), ("et", "Estonian"), ("eu", "Basque"), ("fi", "Finnish"), ("gl", "Galician"),
    ("hr", "Croatian"), ("hu", "Hungarian"), ("is", "Icelandic"), ("lt", "Lithuanian"), ("lv", "Latvian"),
    ("mt", "Maltese"), ("nb", "Norwegian Bokmål"), ("nn", "Norwegian Nynorsk"), ("ro", "Romanian"),
    ("sk", "Slovak"), ("sl", "Slovenian"), ("sq", "Albanian"), ("sr", "Serbian (Latin script)"), ("sv", "Swedish"),
]
LANGUAGES = (
    [(c, n, "rec-latin", f"{c}-en", False, True) for c, n in LATIN]
    + [
        ("zh", "Chinese", "rec-cjk", "zh-en", False, False),
        # Traditional Chinese and Cantonese share Mozilla's zh-Hant model.
        ("zh-TW", "Chinese (Traditional)", "rec-cjk", "zh-Hant-en", False, False),
        ("yue", "Cantonese", "rec-cjk", "zh-Hant-en", False, False),
        ("ja", "Japanese", "rec-cjk", "ja-en", False, False),
        ("ko", "Korean", "rec-korean", "ko-en", False, True),
        ("ru", "Russian", "rec-eslav", "ru-en", False, True),
        ("uk", "Ukrainian", "rec-eslav", "uk-en", False, True),
        ("be", "Belarusian", "rec-eslav", "be-en", False, True),
        ("bg", "Bulgarian", "rec-eslav", "bg-en", False, True),
        ("ar", "Arabic", "rec-arabic", "ar-en", True, True),
        ("fa", "Persian", "rec-arabic", "fa-en", True, True),
        ("ur", "Urdu", "rec-arabic", "ur-en", True, True),
        ("ug", "Uyghur", "rec-arabic", "ug-en", True, True),
        ("hi", "Hindi", "rec-devanagari", "hi-en", False, True),
        ("mr", "Marathi", "rec-devanagari", "mr-en", False, True),
        ("th", "Thai", "rec-th", "th-en", False, False),
        ("ta", "Tamil", "rec-ta", "ta-en", False, True),
        ("te", "Telugu", "rec-te", "te-en", False, True),
    ]
)


def source(repo: str) -> dict:
    return {"repo": repo, "revision": REVISIONS[repo]}


def upstream_entry(repo: str, filename: str, rel: str) -> dict:
    """Size of an upstream file from Hub metadata."""
    rev = REVISIONS[repo]
    info = HfApi().get_paths_info(repo, [filename], expand=True, revision=rev)[0]
    size = info.lfs.size if info.lfs is not None else info.size
    return {"path": rel, "size": size, "source_path": filename}


def paddle_dict(yml_text: str) -> bytes:
    """Must stay byte-identical to translatte_core::download::paddle_dict."""
    cfg = yaml.safe_load(yml_text)
    chars = cfg["PostProcess"]["character_dict"]
    assert all(isinstance(c, str) and "\n" not in c for c in chars), "unexpected dict entry"
    return ("\n".join(chars) + "\n").encode("utf-8")


def dict_entry(repo: str, rel: str) -> dict:
    yml = Path(hf_hub_download(repo, "inference.yml", revision=REVISIONS[repo]))
    data = paddle_dict(yml.read_text(encoding="utf-8"))
    return {"path": rel, "size": len(data),
            "source_path": "inference.yml", "derive": "paddle_dict"}


def pair_entry(directory: str, display: str) -> dict:
    """A mirrored pair directory; sizes from Hub metadata."""
    rev = REVISIONS[PAIR_REPO]
    files = []
    for info in HfApi().list_repo_tree(PAIR_REPO, path_in_repo=directory, revision=rev, expand=True):
        name = info.path.rsplit("/", 1)[-1]
        local = next((v for k, v in PAIR_FILES.items() if name.startswith(k)), None)
        assert local, f"unexpected file {info.path}"
        size = info.lfs.size if info.lfs is not None else info.size
        files.append({"path": f"pair-{directory}/{local}", "size": size, "source_path": info.path})
    files.sort(key=lambda f: f["path"])
    return {"id": f"pair-{directory}", "kind": "pair_model", "display_name": display,
            "source": source(PAIR_REPO), "files": files}


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--manifest", type=Path, default=Path("models/manifest.json"))
    args = ap.parse_args()

    models = [
        {"id": "det", "kind": "core", "display_name": "Text detector", "source": source(DET_REPO),
         "files": [upstream_entry(DET_REPO, "inference.onnx", "det/inference.onnx")]},
    ]
    for pack, (repo, display) in SCRIPT_PACKS.items():
        models.append({"id": pack, "kind": "script_pack", "display_name": display, "source": source(repo),
                       "files": [upstream_entry(repo, "inference.onnx", f"{pack}/inference.onnx"),
                                 dict_entry(repo, f"{pack}/dict.txt")]})
    seen = set()
    for _, name, _, directory, _, _ in LANGUAGES:
        if directory not in seen:
            seen.add(directory)
            models.append(pair_entry(directory, f"{name} → English"))

    languages = [TARGET] + [
        {"code": c, "name": n, "script_pack": p, "pair_model": f"pair-{d}", "rtl": rtl, "spaced": sp}
        for c, n, p, d, rtl, sp in LANGUAGES
    ]
    manifest = {"version": 1, "models": models, "languages": languages}
    args.manifest.parent.mkdir(parents=True, exist_ok=True)
    args.manifest.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"wrote {args.manifest} ({len(models)} models, {len(languages)} languages)")


if __name__ == "__main__":
    main()
