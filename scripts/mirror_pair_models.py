# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Stage Mozilla's translations xx->en and en->xx models for the assafvayner/translatte-pair-models mirror.

Downloads every file in scripts/pair_models_upstream.json (the latest release of each pair, as picked by
scripts/build_pair_models_upstream.py from Remote Settings), checks its size and sha256, and writes
<staging>/<src>-<tgt>/<file> plus the model card (README.md) and the MPL-2.0 text (LICENSE). Files are
copied byte for byte. Upload the result with `hf upload`.
"""

import argparse
import hashlib
import json
import urllib.request
from pathlib import Path

REPO = "assafvayner/translatte-pair-models"
PROJECT = "https://github.com/mozilla/translations"
REGISTRY = "https://mozilla.github.io/translations/model-registry/"
MPL_URL = "https://www.mozilla.org/media/MPL/2.0/index.txt"


def fetch(url: str) -> bytes:
    with urllib.request.urlopen(url) as r:
        return r.read()


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def model_card(upstream: dict) -> str:
    codes = {lang for p in upstream.values() for lang in (p["remote_settings_from"], p["remote_settings_to"])}
    langs = sorted({lang.split("-")[0] for lang in codes})
    rows = "\n".join(
        f"| `{p['remote_settings_from']}` | `{p['remote_settings_to']}` | `{key}/` | {p['version']} "
        f"| {p['total_size'] / 1e6:.1f} MB |"
        for key, p in upstream.items()
    )
    to_en = sum(key.endswith("-en") for key in upstream)
    return f"""---
license: mpl-2.0
language: [{", ".join(langs)}]
pipeline_tag: translation
tags: [translation, marian, bergamot, on-device]
---

# translatte pair models

Small int8 translation models, one per language pair to and from English ({to_en} `xx → en` and
{len(upstream) - to_en} `en → xx`), mirrored unmodified for on-device downloads by the translatte app.

All models were trained and released by Mozilla's [translations]({PROJECT}) project, the engine behind
Firefox's on-device translations, under the [Mozilla Public License 2.0](LICENSE). Each directory holds the
latest release of that pair listed in the project's [model registry]({REGISTRY}), byte for byte
(sizes and sha256 checked against the release records). Thanks to the Mozilla translations team.

Each `<src>-<tgt>/` directory contains:

- `model.<pair>.intgemm.alphas.bin`: the Marian student model (int8)
- `vocab.<pair>.spm`: the SentencePiece vocabulary (some CJK pairs have separate `srcvocab`/`trgvocab` files)
- `lex.50.50.<pair>.s2t.bin`: the lexical shortlist

Run them with [bergamot-translator]({PROJECT}/tree/main/inference) (MPL-2.0) using
`gemm-precision: int8shiftAlphaAll` and the shortlist.

Directories use the release records' language codes except `zh` for Simplified Chinese (`zh-Hans`).
Pairs without a general release (currently `mt-en` and `nn-en`) carry the latest alpha.

| From | To | Directory | Release | Size |
| --- | --- | --- | --- | --- |
{rows}
"""


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--upstream", type=Path, default=Path("scripts/pair_models_upstream.json"))
    ap.add_argument("--staging", type=Path, default=Path("out/pair-models-mirror"))
    args = ap.parse_args()
    upstream = json.loads(args.upstream.read_text())
    for key, pair in upstream.items():
        d = args.staging / key
        d.mkdir(parents=True, exist_ok=True)
        for kind, f in sorted(pair["files"].items()):
            dest = d / f["name"]
            if dest.exists() and dest.stat().st_size == f["size"] and sha256(dest) == f["sha256"]:
                continue
            data = fetch(f["url"])
            assert len(data) == f["size"], (key, kind, len(data))
            assert hashlib.sha256(data).hexdigest() == f["sha256"], (key, kind)
            assert data[:2] != b"\x1f\x8b", f"{key} {kind} is gzipped; the engine reads plain files"
            dest.write_bytes(data)
            print(f"{key}/{f['name']} {len(data)}")
    license_text = fetch(MPL_URL).decode()
    assert license_text.startswith("Mozilla Public License Version 2.0")
    (args.staging / "LICENSE").write_text(license_text)
    (args.staging / "README.md").write_text(model_card(upstream))
    print(f"staged {len(upstream)} pairs in {args.staging} for {REPO}")


if __name__ == "__main__":
    main()
