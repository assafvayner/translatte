# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///
"""Build scripts/pair_models_upstream.json from Mozilla's translations Remote Settings collection.

For every xx->en and en->xx pair it picks the release Firefox for Android ships: records gated by a
filter_expression are ignored unless the expression targets Android (or the pair has nothing else, e.g. a
nightly-only alpha), and among the rest the highest version wins. Keys are "<src>-<tgt>" with our language codes (Remote Settings' zh-Hans is "zh").
"""

import argparse
import json
import re
import urllib.request
from collections import defaultdict
from pathlib import Path

RECORDS = "https://firefox.settings.services.mozilla.com/v1/buckets/main/collections/translations-models/records"
ATTACHMENTS = "https://firefox-settings-attachments.cdn.mozilla.net/"
ANDROID = "env.appinfo.OS == 'Android'"
OUR_CODE = {"zh-Hans": "zh"}


def version_key(v: str) -> tuple:
    m = re.fullmatch(r"(\d+)\.(\d+)(?:a(\d+))?", v)
    assert m, v
    major, minor, alpha = m.groups()
    return (int(major), int(minor), alpha is None, int(alpha or 0))


def ships_on_android(expr: str | None) -> bool:
    return not expr or expr.strip() == ANDROID


def build(records: list[dict]) -> dict:
    shipped = {(r["fromLang"], r["toLang"]) for r in records if ships_on_android(r.get("filter_expression"))}
    groups = defaultdict(list)
    for r in records:
        pair = (r["fromLang"], r["toLang"])
        if "en" in pair and (pair not in shipped or ships_on_android(r.get("filter_expression"))):
            groups[(*pair, r["version"])].append(r)
    latest = {}
    for src, tgt, version in groups:
        best = latest.get((src, tgt))
        if best is None or version_key(version) > version_key(best):
            latest[(src, tgt)] = version
    out = {}
    for (src, tgt), version in sorted(latest.items(), key=lambda kv: (kv[0][1] != "en", kv[0])):
        files = {}
        for r in groups[(src, tgt, version)]:
            assert r["fileType"] not in files, (src, tgt, version, r["fileType"])
            files[r["fileType"]] = {
                "name": r["attachment"]["filename"],
                "size": r["attachment"]["size"],
                "sha256": r["attachment"]["hash"],
                "url": ATTACHMENTS + r["attachment"]["location"],
                "record_id": r["id"],
            }
        vocab = {"vocab"} if "vocab" in files else {"srcvocab", "trgvocab"}
        assert set(files) == {"model", "lex"} | vocab, (src, tgt, version, sorted(files))
        exprs = {r.get("filter_expression") or "" for r in groups[(src, tgt, version)]}
        assert len(exprs) == 1, (src, tgt, exprs)
        key = f"{OUR_CODE.get(src, src)}-{OUR_CODE.get(tgt, tgt)}"
        out[key] = {
            "remote_settings_from": src,
            "remote_settings_to": tgt,
            "version": version,
            "filter_expression": exprs.pop(),
            "files": dict(sorted(files.items())),
            "total_size": sum(f["size"] for f in files.values()),
        }
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--records", type=Path, help="saved Remote Settings response (default: fetch it)")
    ap.add_argument("--out", type=Path, default=Path("scripts/pair_models_upstream.json"))
    args = ap.parse_args()
    if args.records:
        raw = args.records.read_bytes()
    else:
        with urllib.request.urlopen(RECORDS) as r:
            raw = r.read()
    upstream = build(json.loads(raw)["data"])
    args.out.write_text(json.dumps(upstream, indent=2) + "\n")
    to_en = sum(k.endswith("-en") for k in upstream)
    print(f"{to_en} xx->en and {len(upstream) - to_en} en->xx pairs in {args.out}")


if __name__ == "__main__":
    main()
