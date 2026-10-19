# OCR fixtures

Each fixture is an image plus a JSON file with the same stem:

```json
{ "image": "tokyo_menu.jpg", "lang": "ja", "lines": ["本日のおすすめ", "醤油ラーメン 850円"] }
```

- `lang`: source language code from `models/manifest.json`.
- `lines`: every text line a reader would expect to be translated, as written.
- Scoring: for each expected line, the CER (whitespace- and case-insensitive) against the best-matching recognized line, averaged. The metric is recall-oriented: extra recognized lines (noise, unlisted text) are not penalized.

`hello_world.*` is the tiny committed smoke fixture. Real photos are not committed.

## Real-photo set

```bash
uv run scripts/fetch_fixtures.py            # writes fixtures-cache/ (gitignored), ~70 images
cargo run -p translatte-cli --release -- eval fixtures-cache
```

The script downloads a deterministic sample (pinned dataset revisions, selection by hash of the file name), resizes to at most 2000px on the long side, and writes the fixture JSONs. Images and annotations are never redistributed here; each run fetches them from the Hub. Instances that are illegible or marked `#`/`*`, and text not in the target script, are dropped; images with more than 40 entries are skipped. Datasets that only have word-level boxes are grouped into reading-order lines by box geometry.

| Source (pinned revision) | License / terms | Languages |
| --- | --- | --- |
| [`Nexdata-kr/500000-Images-Multilingual-OCR-Dataset-in-21-Languages`](https://huggingface.co/datasets/Nexdata-kr/500000-Images-Multilingual-OCR-Dataset-in-21-Languages) `3cc359db` (natural-scene samples, line-level) | Nexdata sample, commercial license, no license metadata on the Hub; used for local evaluation only | zh, ja, ko, ru, hi, fr, de, es, it, pl, pt, id |
| [`interfaze-ai/hindi-scene-text`](https://huggingface.co/datasets/interfaze-ai/hindi-scene-text) `af0470e2` (word-level) | none stated | hi |
| [`Melaraby/EvArEST-dataset-for-Arabic-scene-text-detection`](https://huggingface.co/datasets/Melaraby/EvArEST-dataset-for-Arabic-scene-text-detection) `bede1fdb` (word-level, test split) | BSD-3-Clause | ar |
| [`Voxel51/Total-Text-Dataset`](https://huggingface.co/datasets/Voxel51/Total-Text-Dataset) `0ce21350` (word-level) | BSD-3-Clause | en |

Not covered (no suitable real-photo dataset with transcriptions found on the Hub): Thai, Greek, Tamil, Telugu, and other Indic scripts. `Bhashini-IITJ/Bharat-Scene-Text-Dataset` (hi, mr, ta, te, bn, ...; per-image Wikimedia licenses) is gated and would cover the Indic gaps once access is granted.

Because the images and ground truth are third-party and the Nexdata/Hindi terms are unclear, nothing from them is committed; only the downloader is.

Known limits: word-level sources (ar, hi, en) make the score sensitive to how the detector splits or merges words, and `interfaze-ai/hindi-scene-text` images are squashed to 512x512.
