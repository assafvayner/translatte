#!/usr/bin/env bash
# Run the cross-compiled CLI on a connected Android device/emulator (see README "Android (emulator) smoke test").
# Usage: scripts/android/run.sh <image> <from> <to|-> [name]   ("-" as <to> runs OCR only)
# TRANSLATOR_GGUF=<local .gguf> pushes that file and runs with it instead of the installed translator.
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
[[ -f "$IMAGE" ]] || { echo "image not found: $IMAGE" >&2; exit 1; }
adb get-state >/dev/null

PACK="$(python3 -c 'import json,sys
m = json.load(open(sys.argv[1]))
print(next((l.get("script_pack") or "" for l in m["languages"] if l["code"] == sys.argv[2]), ""))' "$MANIFEST" "$FROM")"
[[ -n "$PACK" ]] || { echo "unknown source language (or no recognizer pack): $FROM" >&2; exit 1; }
IDS=(det "$PACK")
if [[ "$TO" != "-" && -z "${TRANSLATOR_GGUF:-}" ]]; then IDS+=(translator); fi
[[ -z "${TRANSLATOR_GGUF:-}" || -f "$TRANSLATOR_GGUF" ]] || { echo "translator gguf not found: $TRANSLATOR_GGUF" >&2; exit 1; }
for id in "${IDS[@]}"; do
  [[ -d "$MODELS/$id" ]] || { echo "model dir missing: $MODELS/$id (run: translatte fetch)" >&2; exit 1; }
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

for f in translatte libonnxruntime.so libc++_shared.so; do push "$REL/$f" "$DEV/$f"; done
push "$MANIFEST" "$DEV/manifest.json"
for id in "${IDS[@]}"; do
  for f in "$MODELS/$id"/*; do push_model "$f" "$DEV/models/$id/$(basename "$f")"; done
done
ALT_ARGS=""
if [[ -n "${TRANSLATOR_GGUF:-}" && "$TO" != "-" ]]; then
  push_model "$TRANSLATOR_GGUF" "$DEV/models/translator-alt/model.gguf"
  ALT_ARGS="--translator-gguf models/translator-alt/model.gguf"
fi
EXT="${IMAGE##*.}"
adb shell "rm -rf '$DEV/out' && mkdir -p '$DEV/out'"
push "$IMAGE" "$DEV/in.$EXT"

mkdir -p "$OUT"
if [[ "$TO" == "-" ]]; then LANG_ARGS="--no-translate"; else LANG_ARGS="--to '$TO' $ALT_ARGS"; fi
adb shell "cd $DEV && chmod 755 translatte && LD_LIBRARY_PATH=$DEV TRANSLATTE_PERF=1 ${TRANSLATTE_THREADS:+TRANSLATTE_THREADS=$TRANSLATTE_THREADS} ${TRANSLATTE_PREFIX_CACHE:+TRANSLATTE_PREFIX_CACHE=$TRANSLATTE_PREFIX_CACHE} /system/bin/time -v ${TASKSET:+taskset $TASKSET} timeout ${RUN_TIMEOUT:-60} ./translatte run 'in.$EXT' \
  --from '$FROM' $LANG_ARGS --models models --manifest manifest.json -o out/out.png --json out/out.json" \
  2>&1 | tee "$OUT/$NAME.log"

adb pull "$DEV/out/out.png" "$OUT/$NAME.png" >/dev/null 2>&1
adb pull "$DEV/out/out.json" "$OUT/$NAME.json" >/dev/null 2>&1
echo "results in $OUT/$NAME.{png,json,log}"
