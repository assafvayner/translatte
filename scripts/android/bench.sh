#!/usr/bin/env bash
# Run `translatte bench` on a connected Android device over adb (build first: scripts/android/build.sh).
# Usage: scripts/android/bench.sh <fixtures dir> <langs, e.g. fr,de,zh> [bench args...]
# e.g.   scripts/android/bench.sh fixtures-cache fr,de,zh --iters 5 --max-side 960 --translate
# Env: TRANSLATTE_OCR_* are forwarded to the device; TRANSLATTE_MANIFEST and TRANSLATTE_MODELS pick
# the manifest and models dir; BENCH_BIN_DIR the built binaries; BENCH_DEVICE_DIR the device work
# dir; BENCH_TASKSET a CPU mask (e.g. f0) to pin the run to.
set -euo pipefail

[[ $# -ge 2 ]] || { echo "usage: $0 <fixtures dir> <langs> [bench args...]" >&2; exit 2; }
FIXTURES="$1" LANGS="$2"
shift 2

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TARGET_DIR="$(cd "$ROOT" && cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
REL="${BENCH_BIN_DIR:-$TARGET_DIR/aarch64-linux-android/release}"
MODELS="${TRANSLATTE_MODELS:-$ROOT/models-cache}"
MANIFEST="${TRANSLATTE_MANIFEST:-$ROOT/models/manifest.json}"
DEV="${BENCH_DEVICE_DIR:-/data/local/tmp/translatte}"

[[ -x "$REL/translatte" ]] || { echo "build first: scripts/android/build.sh" >&2; exit 1; }
adb get-state >/dev/null

TRANSLATE=0
[[ " $* " == *" --translate "* ]] && TRANSLATE=1
read -r -a FILES <<<"$(python3 -c 'import json,sys
m = json.load(open(sys.argv[1]))
ids = {"det"}
for code in sys.argv[2].split(","):
    l = next(l for l in m["languages"] if l["code"] == code)
    ids.add(l["script_pack"])
    if sys.argv[3] == "1": ids.add(l["pair_model"])
print(" ".join(f["path"] for e in m["models"] if e["id"] in ids for f in e["files"]))' "$MANIFEST" "$LANGS" "$TRANSLATE")"

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
for f in "${FILES[@]}"; do
  [[ -f "$MODELS/$f" ]] || { echo "model file missing: $MODELS/$f" >&2; exit 1; }
  push_if_changed "$MODELS/$f" "$DEV/models/$f"
done
adb shell "rm -rf '$DEV/bench' && mkdir -p '$DEV/bench'"
for json in "$FIXTURES"/*.json; do
  read -r lang image < <(python3 -c 'import json,sys; j=json.load(open(sys.argv[1])); print(j["lang"], j["image"])' "$json")
  [[ ",$LANGS," == *",$lang,"* ]] || continue
  push "$json" "$DEV/bench/$(basename "$json")"
  push "$FIXTURES/$image" "$DEV/bench/$image"
done

ENVS="$(env | grep '^TRANSLATTE_OCR_' | tr '\n' ' ' || true)"
adb shell "cd $DEV && chmod 755 translatte && LD_LIBRARY_PATH=$DEV $ENVS \
  ${BENCH_TASKSET:+taskset $BENCH_TASKSET} ./translatte bench bench --only '$LANGS' --models models --manifest manifest.json $*"
