#!/usr/bin/env bash
# Run the cross-compiled CLI on a connected Android device/emulator (see README "Android (emulator) smoke test").
# Usage: scripts/android/run.sh <image> <from> <to|-> [name]   ("-" as <to> runs OCR only)
set -euo pipefail

[[ $# -ge 3 ]] || { echo "usage: $0 <image> <from> <to|-> [name]" >&2; exit 2; }
IMAGE="$1" FROM="$2" TO="$3"
NAME="${4:-$(basename "${IMAGE%.*}")_${FROM}-${TO/#-/ocr}}"

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
REL="$ROOT/target/aarch64-linux-android/release"
MODELS="${TRANSLATTE_MODELS:-$ROOT/models-cache}"
MANIFEST="$ROOT/models/manifest.json"
DEV=/data/local/tmp/translatte
OUT="$ROOT/out/android"

[[ -x "$REL/translatte" ]] || { echo "build first: scripts/android/build.sh" >&2; exit 1; }
adb get-state >/dev/null

# Pushes $1 to $2 unless a file of the same size is already there.
push() {
  local want have
  want="$(stat -f %z "$1" 2>/dev/null || stat -c %s "$1")"
  have="$(adb shell "stat -c %s '$2' 2>/dev/null" | tr -d '\r' || true)"
  if [[ "$want" != "$have" ]]; then
    adb shell "mkdir -p '$(dirname "$2")'"
    adb push "$1" "$2" >/dev/null 2>&1
    echo "pushed $(basename "$1") ($want bytes)"
  fi
}

PACK="$(python3 -c 'import json,sys
m = json.load(open(sys.argv[1]))
print(next(l["script_pack"] for l in m["languages"] if l["code"] == sys.argv[2]))' "$MANIFEST" "$FROM")"
IDS=(det "$PACK")
[[ "$TO" == "-" ]] || IDS+=(translator)

for f in translatte libonnxruntime.so libc++_shared.so; do push "$REL/$f" "$DEV/$f"; done
push "$MANIFEST" "$DEV/manifest.json"
for id in "${IDS[@]}"; do
  for f in "$MODELS/$id"/*; do push "$f" "$DEV/models/$id/$(basename "$f")"; done
done
EXT="${IMAGE##*.}"
adb shell "rm -rf '$DEV/out' && mkdir -p '$DEV/out'"
adb push "$IMAGE" "$DEV/in.$EXT" >/dev/null 2>&1

mkdir -p "$OUT"
if [[ "$TO" == "-" ]]; then LANG_ARGS="--no-translate"; else LANG_ARGS="--to $TO"; fi
adb shell "cd $DEV && chmod 755 translatte && LD_LIBRARY_PATH=$DEV /system/bin/time -v ./translatte run in.$EXT \
  --from $FROM $LANG_ARGS --models models --manifest manifest.json -o out/out.png --json out/out.json" \
  2>&1 | tee "$OUT/$NAME.log"

adb pull "$DEV/out/out.png" "$OUT/$NAME.png" >/dev/null 2>&1
adb pull "$DEV/out/out.json" "$OUT/$NAME.json" >/dev/null 2>&1
echo "results in $OUT/$NAME.{png,json,log}"
