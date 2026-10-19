# OCR fixtures

Each fixture is an image plus a JSON file with the same stem:

```json
{ "image": "tokyo_menu.jpg", "lang": "ja", "lines": ["本日のおすすめ", "醤油ラーメン 850円"] }
```

- `lang`: source language code from `models/manifest.json`.
- `lines`: every text line a reader would expect to be translated, as written.
- Scoring: for each expected line, the CER (whitespace- and case-insensitive) against the best-matching recognized line, averaged. The metric is recall-oriented: extra recognized lines (noise, unlisted text) are not penalized.

Run: `cargo run -p translatte-cli --release -- eval fixtures --max-cer 0.15`

Aim for 20–30 real phone photos across scripts: signs, menus and labels, plus rotated, low-light and RTL (Arabic) cases. Keep images ≤2MB, so resize to ~2000px on the long side.
