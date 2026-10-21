#!/usr/bin/env bash
# Run `translatte bench` on a connected Android device over adb (build first: scripts/android/build.sh).
# Usage: scripts/android/bench.sh <fixtures dir> <langs, e.g. fr,de,zh> [bench args...]
# e.g.   scripts/android/bench.sh fixtures-cache fr,de,zh --iters 5 --max-side 960 --translate
set -euo pipefail

[[ $# -ge 2 ]] || { echo "usage: $0 <fixtures dir> <langs> [bench args...]" >&2; exit 2; }
FIXTURES="$1" LANGS="$2"
shift 2

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TARGET_DIR="$(cd "$ROOT" && cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
REL="$TARGET_DIR/aarch64-linux-android/release"
MODELS="${TRANSLATTE_MODELS:-$ROOT/models-cache}"
MANIFEST="$ROOT/models/manifest.json"
DEV=/data/local/tmp/translatte

[[ -x "$REL/translatte" ]] || { echo "build first: scripts/android/build.sh" >&2; exit 1; }
adb get-state >/dev/null

read -r -a IDS <<<"$(python3 -c 'import json,sys
m = json.load(open(sys.argv[1]))
ids = {"det"}
for code in sys.argv[2].split(","):
    l = next(l for l in m["languages"] if l["code"] == code)
    ids.add(l["script_pack"]); ids.add(l["pair_model"])
print(" ".join(sorted(ids)))' "$MANIFEST" "$LANGS")"

push() {
  adb shell "mkdir -p '$(dirname "$2")'"
  adb push "$1" "$2" >/dev/null 2>&1
}

# Pushes $1 to $2 unless a file of the same size is already there.
push_if_changed() {
  local want have
  want="$(stat -c %s "$1" 2>/dev/null || stat -f %z "$1")"
  have="$(adb shell "stat -c %s '$2' 2>/dev/null" | tr -d '\r' || true)"
  [[ "$want" == "$have" ]] || push "$1" "$2"
}

for f in translatte libonnxruntime.so libc++_shared.so; do push "$REL/$f" "$DEV/$f"; done
push "$MANIFEST" "$DEV/manifest.json"
for id in "${IDS[@]}"; do
  [[ -d "$MODELS/$id" ]] || { echo "model dir missing: $MODELS/$id" >&2; exit 1; }
  for f in "$MODELS/$id"/*; do push_if_changed "$f" "$DEV/models/$id/$(basename "$f")"; done
done
adb shell "rm -rf '$DEV/bench' && mkdir -p '$DEV/bench'"
for json in "$FIXTURES"/*.json; do
  read -r lang image < <(python3 -c 'import json,sys; j=json.load(open(sys.argv[1])); print(j["lang"], j["image"])' "$json")
  [[ ",$LANGS," == *",$lang,"* ]] || continue
  push "$json" "$DEV/bench/$(basename "$json")"
  push "$FIXTURES/$image" "$DEV/bench/$image"
done

adb shell "cd $DEV && chmod 755 translatte && LD_LIBRARY_PATH=$DEV ${TRANSLATTE_OCR_THREADS:+TRANSLATTE_OCR_THREADS=$TRANSLATTE_OCR_THREADS} \
  ./translatte bench bench --only '$LANGS' --models models --manifest manifest.json $*"
