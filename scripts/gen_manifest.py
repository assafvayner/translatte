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
TRANSLATOR_REPO = "tencent/Hy-MT2-1.8B-GGUF"
TRANSLATOR_FILE = "Hy-MT2-1.8B-Q4_K_M.gguf"

SCRIPT_PACKS = {
    "rec-cjk": ("PaddlePaddle/PP-OCRv5_mobile_rec_onnx", "Chinese / Japanese"),
    "rec-en": ("PaddlePaddle/en_PP-OCRv5_mobile_rec_onnx", "English"),
    "rec-latin": ("PaddlePaddle/latin_PP-OCRv5_mobile_rec_onnx", "Latin script"),
    "rec-eslav": ("PaddlePaddle/eslav_PP-OCRv5_mobile_rec_onnx", "Cyrillic (Russian, Ukrainian, Belarusian)"),
    "rec-korean": ("PaddlePaddle/korean_PP-OCRv5_mobile_rec_onnx", "Korean"),
    "rec-arabic": ("PaddlePaddle/arabic_PP-OCRv5_mobile_rec_onnx", "Arabic script"),
    "rec-devanagari": ("PaddlePaddle/devanagari_PP-OCRv5_mobile_rec_onnx", "Devanagari"),
    "rec-th": ("PaddlePaddle/th_PP-OCRv5_mobile_rec_onnx", "Thai"),
    "rec-ta": ("PaddlePaddle/ta_PP-OCRv5_mobile_rec_onnx", "Tamil"),
    "rec-te": ("PaddlePaddle/te_PP-OCRv5_mobile_rec_onnx", "Telugu"),
}

REVISIONS = {
    "PaddlePaddle/PP-OCRv5_mobile_det_onnx": "e6f4fa85f00e168c862bc462aebca69eef9b3d3d",
    "tencent/Hy-MT2-1.8B-GGUF": "a0c709d9fac510f2c807aa3af52872340dc37a4a",
    "PaddlePaddle/PP-OCRv5_mobile_rec_onnx": "ed152b8b495f84de93cda5709d768548a9127622",
    "PaddlePaddle/en_PP-OCRv5_mobile_rec_onnx": "3fafbc3b5dcf93dd72add9f48368be8a3a2cd33b",
    "PaddlePaddle/latin_PP-OCRv5_mobile_rec_onnx": "89d3a50e2c27e2e7cceeab0e944c25c807d5db4f",
    "PaddlePaddle/eslav_PP-OCRv5_mobile_rec_onnx": "9a32171fc5718746875e1a261818884517975013",
    "PaddlePaddle/korean_PP-OCRv5_mobile_rec_onnx": "5c6f574b8e2230adf4287b33e736d71b9fabd28e",
    "PaddlePaddle/arabic_PP-OCRv5_mobile_rec_onnx": "14aaedcd75825982689ecf5cd64ab33ee083215a",
    "PaddlePaddle/devanagari_PP-OCRv5_mobile_rec_onnx": "251aec19e36739540d35e2cc943f6aa7503b98e5",
    "PaddlePaddle/th_PP-OCRv5_mobile_rec_onnx": "1d4adbbafb1034a2fd6618498575b81ea7b69f69",
    "PaddlePaddle/ta_PP-OCRv5_mobile_rec_onnx": "0db89798af0d5218fad9ea7ace0557526e9077f1",
    "PaddlePaddle/te_PP-OCRv5_mobile_rec_onnx": "f1197c83430241c58c64b0952b3a80366d04eeba",
}

# English names Hy-MT2's prompt uses where they differ from our display name.
PROMPT_NAMES = {"zh-TW": "Traditional Chinese", "kk": "Kazakh", "mn": "Mongolian"}

# Chinese names for Hy-MT2's Chinese instruction (model card's language table).
PROMPT_NAMES_ZH = {
    "en": "英语", "fr": "法语", "de": "德语", "es": "西班牙语", "it": "意大利语", "pt": "葡萄牙语",
    "nl": "荷兰语", "pl": "波兰语", "cs": "捷克语", "tr": "土耳其语", "id": "印尼语", "ms": "马来语",
    "tl": "菲律宾语", "zh": "中文", "zh-TW": "繁体中文", "yue": "粤语", "ja": "日语", "ko": "韩语",
    "ru": "俄语", "uk": "乌克兰语", "ar": "阿拉伯语", "fa": "波斯语", "ur": "乌尔都语", "ug": "维吾尔语",
    "hi": "印地语", "mr": "马拉地语", "th": "泰语", "ta": "泰米尔语", "te": "泰卢固语", "he": "希伯来语",
    "vi": "越南语", "kk": "哈萨克语", "mn": "蒙古语", "bo": "藏语", "km": "高棉语", "my": "缅甸语",
    "bn": "孟加拉语", "gu": "古吉拉特语",
}

# Pairs involving these get Hy-MT2's Chinese instruction: Chinese varieties, plus languages it
# only writes when instructed in Chinese (in English it answers in English or the wrong script).
ZH_INSTRUCTION = {"zh", "zh-TW", "yue", "bo", "ug", "kk", "mn"}

# (code, name, script pack or None for target-only, rtl, spaced)
LATIN = [
    ("fr", "French"), ("de", "German"), ("es", "Spanish"), ("it", "Italian"), ("pt", "Portuguese"),
    ("nl", "Dutch"), ("pl", "Polish"), ("cs", "Czech"), ("tr", "Turkish"), ("id", "Indonesian"),
    ("ms", "Malay"), ("tl", "Filipino"),
]
LANGUAGES = (
    [("en", "English", "rec-en", False, True)]
    + [(c, n, "rec-latin", False, True) for c, n in LATIN]
    + [
        ("zh", "Chinese", "rec-cjk", False, False),
        ("zh-TW", "Chinese (Traditional)", "rec-cjk", False, False),
        ("yue", "Cantonese", "rec-cjk", False, False),
        ("ja", "Japanese", "rec-cjk", False, False),
        ("ko", "Korean", "rec-korean", False, True),
        ("ru", "Russian", "rec-eslav", False, True),
        ("uk", "Ukrainian", "rec-eslav", False, True),
        ("ar", "Arabic", "rec-arabic", True, True),
        ("fa", "Persian", "rec-arabic", True, True),
        ("ur", "Urdu", "rec-arabic", True, True),
        ("ug", "Uyghur", "rec-arabic", True, True),
        ("hi", "Hindi", "rec-devanagari", False, True),
        ("mr", "Marathi", "rec-devanagari", False, True),
        ("th", "Thai", "rec-th", False, False),
        ("ta", "Tamil", "rec-ta", False, True),
        ("te", "Telugu", "rec-te", False, True),
        ("he", "Hebrew", None, True, True),
        ("vi", "Vietnamese", None, False, True),
        # Hy-MT2 reads and writes Kazakh in Arabic script and Mongolian in the traditional script
        # (as used in China), so they are target-only; Cyrillic input only gets transliterated.
        ("kk", "Kazakh (Arabic script)", None, True, True),
        ("mn", "Mongolian (traditional script)", None, False, True),
        ("bo", "Tibetan", None, False, False),
        ("km", "Khmer", None, False, False),
        ("my", "Burmese", None, False, False),
        ("bn", "Bengali", None, False, True),
        ("gu", "Gujarati", None, False, True),
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


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--manifest", type=Path, default=Path("models/manifest.json"))
    args = ap.parse_args()

    models = [
        {"id": "det", "kind": "core", "display_name": "Text detector", "source": source(DET_REPO),
         "files": [upstream_entry(DET_REPO, "inference.onnx", "det/inference.onnx")]},
        {"id": "translator", "kind": "core", "display_name": "Translator (Hy-MT2 1.8B)",
         "source": source(TRANSLATOR_REPO),
         "files": [upstream_entry(TRANSLATOR_REPO, TRANSLATOR_FILE, "translator/model.gguf")]},
    ]
    for pack, (repo, display) in SCRIPT_PACKS.items():
        models.append({"id": pack, "kind": "script_pack", "display_name": display, "source": source(repo),
                       "files": [upstream_entry(repo, "inference.onnx", f"{pack}/inference.onnx"),
                                 dict_entry(repo, f"{pack}/dict.txt")]})

    languages = [{"code": c, "name": n, "script_pack": p, "rtl": rtl, "spaced": sp}
                 | ({"prompt_name": PROMPT_NAMES[c]} if c in PROMPT_NAMES else {})
                 | {"prompt_name_zh": PROMPT_NAMES_ZH[c]}
                 | ({"zh_instruction": True} if c in ZH_INSTRUCTION else {})
                 for c, n, p, rtl, sp in LANGUAGES]
    manifest = {"version": 1, "models": models, "languages": languages}
    args.manifest.parent.mkdir(parents=True, exist_ok=True)
    args.manifest.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"wrote {args.manifest} ({len(models)} models, {len(languages)} languages)")


if __name__ == "__main__":
    main()
