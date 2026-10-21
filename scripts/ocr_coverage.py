# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Check which languages an OCR recognizer's character dictionary can read.

A language counts as covered by a dictionary when every letter of its alphabet (both cases) or every
character of its sample text is in the dictionary. Rerun this against a new recognizer's dict.txt (one
character per line, as written by `translatte fetch`) before changing script packs in the manifest.

    # which packs can read which languages
    python3 scripts/ocr_coverage.py --dict models-cache/rec-*/dict.txt
    # only some languages, with the missing characters for partial matches
    python3 scripts/ocr_coverage.py --dict models-cache/rec-latin/dict.txt --langs vi ro sv -v
    # check every manifest language against its own script pack
    python3 scripts/ocr_coverage.py --manifest models/manifest.json --models-dir models-cache
"""

import argparse
import json
import sys
import unicodedata
from pathlib import Path

BASIC = "abcdefghijklmnopqrstuvwxyz"


def block(*ranges: tuple[int, int]) -> str:
    """Assigned letters and combining marks in the given code point ranges (inclusive)."""
    return "".join(
        chr(c) for lo, hi in ranges for c in range(lo, hi + 1) if unicodedata.category(chr(c))[0] in "LM"
    )


ARABIC = "ابتثجحخدذرزسشصضطظعغفقكلمنهويءآأؤإئةى"

# Lowercase alphabets (uppercase is added automatically) or, for scripts too large to enumerate,
# sample text. Indic and Thai entries are the core letters and vowel signs of the Unicode block, minus
# obsolete Thai ฃ and the Japanese kana without modern use (ゐゑゔヰヱ).
ALPHABETS = {
    "en": BASIC,
    "af": BASIC + "áäéèêëíîïóôöúûüý",
    "az": BASIC + "çəğıöşüİ",
    "bs": BASIC + "čćđšž",
    "ca": BASIC + "àçèéíïòóúü·",
    "cs": BASIC + "áčďéěíňóřšťúůýž",
    "da": BASIC + "æøåé",
    "de": BASIC + "äöüß",
    "es": BASIC + "áéíñóúü",
    "et": BASIC + "äöõüšž",
    "eu": BASIC + "ñç",
    "fi": BASIC + "äöåšž",
    "fr": BASIC + "àâæçéèêëîïôœùûüÿ",
    "gl": BASIC + "áéíñóúü",
    "hr": BASIC + "čćđšž",
    "hu": BASIC + "áéíóöőúüű",
    "id": BASIC,
    "is": BASIC + "áðéíóúýþæö",
    "it": BASIC + "àèéìíîòóùú",
    "lt": BASIC + "ąčęėįšųūž",
    "lv": BASIC + "āčēģīķļņšūž",
    "ms": BASIC,
    "mt": BASIC + "ċġħżàèìòù",
    "nb": BASIC + "æøåé",
    "nl": BASIC + "éëïóöü",
    "nn": BASIC + "æøåé",
    "pl": BASIC + "ąćęłńóśźż",
    "pt": BASIC + "áâãàçéêíóôõú",
    "ro": BASIC + "ăâîșț",
    "sk": BASIC + "áäčďéíĺľňóôŕšťúýž",
    "sl": BASIC + "čšž",
    "sq": BASIC + "çë",
    "sv": BASIC + "åäöé",
    "tl": BASIC + "ñ",
    "tr": BASIC + "çğıöşüİ",
    "vi": BASIC
    + "ăâđêôơư"
    + "àáảãạằắẳẵặầấẩẫậèéẻẽẹềếểễệìíỉĩịòóỏõọồốổỗộờớởỡợùúủũụừứửữựỳýỷỹỵ",
    "be": "абвгдеёжзійклмнопрстуўфхцчшыьэюя’",
    "bg": "абвгдежзийклмнопрстуфхцчшщъьюя",
    "ru": "абвгдеёжзийклмнопрстуфхцчшщъыьэюя",
    "sr": BASIC + "čćđšž",
    "sr-Cyrl": "абвгдђежзијклљмнњопрстћуфхцчџш",
    "uk": "абвгґдеєжзиіїйклмнопрстуфхцчшщьюя’",
    "el": "αβγδεζηθικλμνξοπρσςτυφχψωάέήίόύώϊϋΐΰ",
    "he": "אבגדהוזחטיכךלמםנןסעפףצץקרשת",
    "ar": ARABIC,
    "fa": ARABIC.replace("ك", "").replace("ي", "") + "پچژگکی",
    "ur": "ابپتٹثجچحخدڈذرڑزژسشصضطظعغفقکگلمنںوہھءیےآأؤئۓ",
    "ug": "اەبپتجچخدرزژسشغفقكگڭلمنھوۇۆۈۋېىيئ",
    "hi": block((0x0905, 0x0914), (0x0915, 0x0939), (0x093E, 0x094D)),
    "mr": block((0x0905, 0x0914), (0x0915, 0x0939), (0x093E, 0x094D)),
    "bn": block((0x0985, 0x09B9), (0x09BE, 0x09CD)),
    "gu": block((0x0A85, 0x0AB9), (0x0ABE, 0x0ACD)),
    "kn": block((0x0C85, 0x0CB9), (0x0CBE, 0x0CCD)),
    "ml": block((0x0D05, 0x0D39), (0x0D3E, 0x0D4D)),
    "ta": block((0x0B85, 0x0BB9), (0x0BBE, 0x0BCD)),
    "te": block((0x0C05, 0x0C39), (0x0C3E, 0x0C4D)),
    "th": block((0x0E01, 0x0E02), (0x0E04, 0x0E3A), (0x0E40, 0x0E4D)),
    "ja": block((0x3041, 0x308F), (0x3092, 0x3093), (0x30A1, 0x30EF), (0x30F2, 0x30F6))
    + "日本語学生先私今時間電車駅出口入口東西南北",
    "zh": "的一是不了人我在有他这中大来上国个到说们为子和你地出道也时年得就那要下以生会自着去之过家学对可里后",
    "zh-TW": "這個們來說時國過學對會麼後裡為與點樣體開關還長門問題經興車站東西南北",
    "yue": "嘅咗唔係佢哋喺嚟咩乜啲冇嘢",
    "ko": "한국어번역사진카메라안녕하세요감사합니다출구입구역화장실",
}


def chars(lang: str) -> set[str]:
    text = ALPHABETS[lang]
    out = set(text)
    for c in text:
        if len(c.upper()) == 1:
            out.add(c.upper())
    return out


def load_dict(path: Path) -> set[str]:
    return {line for line in path.read_text(encoding="utf-8").split("\n") if line}


def pack_name(path: Path) -> str:
    return path.parent.name if path.name == "dict.txt" else path.stem


def missing(lang: str, dictionary: set[str]) -> list[str]:
    return sorted(chars(lang) - dictionary)


def by_dicts(dicts: list[Path], langs: list[str], verbose: bool) -> None:
    packs = {pack_name(p): load_dict(p) for p in dicts}
    for lang in langs:
        gaps = {name: missing(lang, d) for name, d in packs.items()}
        full = [name for name, m in gaps.items() if not m]
        best = min(gaps, key=lambda n: len(gaps[n]))
        if full:
            print(f"{lang:6} covered by {', '.join(full)}")
        else:
            print(f"{lang:6} not covered (closest {best}, missing {len(gaps[best])}/{len(chars(lang))})")
            if verbose:
                print(f"       missing: {' '.join(gaps[best])}")


def by_manifest(manifest: Path, models_dir: Path, verbose: bool) -> bool:
    ok = True
    for lang in json.loads(manifest.read_text(encoding="utf-8"))["languages"]:
        pack = lang.get("script_pack")
        if pack is None:
            continue
        code = lang["code"]
        if code not in ALPHABETS:
            print(f"{code:6} {pack}: no alphabet listed in {Path(__file__).name}")
            ok = False
            continue
        m = missing(code, load_dict(models_dir / pack / "dict.txt"))
        print(f"{code:6} {pack}: {'ok' if not m else f'missing {len(m)}/{len(chars(code))}'}")
        if m and verbose:
            print(f"       missing: {' '.join(m)}")
        ok &= not m
    return ok


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dict", type=Path, nargs="+", help="recognizer dict.txt files (named by their directory)")
    ap.add_argument("--langs", nargs="+", choices=sorted(ALPHABETS), default=sorted(ALPHABETS), metavar="LANG")
    ap.add_argument("--manifest", type=Path, help="check each manifest language against its script pack")
    ap.add_argument("--models-dir", type=Path, default=Path("models-cache"))
    ap.add_argument("-v", "--verbose", action="store_true", help="list missing characters")
    args = ap.parse_args()
    if args.manifest:
        sys.exit(0 if by_manifest(args.manifest, args.models_dir, args.verbose) else 1)
    if not args.dict:
        ap.error("pass --dict or --manifest")
    by_dicts(args.dict, args.langs, args.verbose)


if __name__ == "__main__":
    main()
