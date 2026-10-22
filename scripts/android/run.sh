#!/usr/bin/env bash
# Run the cross-compiled CLI on a connected Android device over adb (see README "Android CLI smoke test").
# Usage: scripts/android/run.sh <image> <from> [-] [name]   ("-" runs OCR only)
set -euo pipefail

[[ $# -ge 2 ]] || { echo "usage: $0 <image> <from> [-] [name]" >&2; exit 2; }
IMAGE="$1" FROM="$2" OCR="${3:-}"
[[ -z "$OCR" || "$OCR" == "-" ]] || { echo "the third argument is - (OCR only) or absent" >&2; exit 2; }
NAME="${4:-$(basename "${IMAGE%.*}")_${FROM}${OCR:+-ocr}}"

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TARGET_DIR="$(cd "$ROOT" && cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
REL="$TARGET_DIR/aarch64-linux-android/release"
MODELS="${TRANSLATTE_MODELS:-$ROOT/models-cache}"
MANIFEST="$ROOT/models/manifest.json"
DEV=/data/local/tmp/translatte
OUT="$ROOT/out/android"

[[ -x "$REL/translatte" ]] || { echo "build first: scripts/android/build.sh" >&2; exit 1; }
[[ -f "$IMAGE" ]] || { echo "image not found: $IMAGE" >&2; exit 1; }
adb get-state >/dev/null

read -r PACK PAIR <<<"$(python3 -c 'import json,sys
m = json.load(open(sys.argv[1]))
l = next((l for l in m["languages"] if l["code"] == sys.argv[2]), {})
print(l.get("script_pack") or "-", l.get("pair_model") or "-")' "$MANIFEST" "$FROM")"
[[ "$PACK" != "-" && "$PAIR" != "-" ]] || { echo "not a source language: $FROM" >&2; exit 1; }
IDS=(det "$PACK")
[[ -n "$OCR" ]] || IDS+=("$PAIR")
for id in "${IDS[@]}"; do
  [[ -d "$MODELS/$id" ]] || { echo "model dir missing: $MODELS/$id (run: translatte fetch --langs $FROM)" >&2; exit 1; }
done

push() {
  adb shell "mkdir -p '$(dirname "$2")'"
  adb push "$1" "$2" >/dev/null 2>&1
}

# Pushes model file $1 to $2 unless a file of the same size is already there.
push_model() {
  local want have
  want="$(stat -c %s "$1" 2>/dev/null || stat -f %z "$1")"
  have="$(adb shell "stat -c %s '$2' 2>/dev/null" | tr -d '\r' || true)"
  if [[ "$want" != "$have" ]]; then
    push "$1" "$2"
    echo "pushed $(basename "$1") ($want bytes)"
  fi
}

for f in translatte libc++_shared.so; do push "$REL/$f" "$DEV/$f"; done
push "$MANIFEST" "$DEV/manifest.json"
for id in "${IDS[@]}"; do
  for f in "$MODELS/$id"/*; do push_model "$f" "$DEV/models/$id/$(basename "$f")"; done
done
EXT="${IMAGE##*.}"
adb shell "rm -rf '$DEV/out' && mkdir -p '$DEV/out'"
push "$IMAGE" "$DEV/in.$EXT"

mkdir -p "$OUT"
LANG_ARGS=""
[[ -z "$OCR" ]] || LANG_ARGS="--no-translate"
adb shell "cd $DEV && chmod 755 translatte && LD_LIBRARY_PATH=$DEV /system/bin/time -v ${TASKSET:+taskset $TASKSET} timeout ${RUN_TIMEOUT:-60} ./translatte run 'in.$EXT' \
  --from '$FROM' $LANG_ARGS --models models --manifest manifest.json -o out/out.png --json out/out.json" \
  2>&1 | tee "$OUT/$NAME.log"

adb pull "$DEV/out/out.png" "$OUT/$NAME.png" >/dev/null 2>&1
adb pull "$DEV/out/out.json" "$OUT/$NAME.json" >/dev/null 2>&1
echo "results in $OUT/$NAME.{png,json,log}"
