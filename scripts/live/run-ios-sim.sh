#!/usr/bin/env bash
# Replays a clip through live mode on the "iPhone 18 Pro" simulator: builds and installs the debug
# app, launches it with -mock-video, records the screen until the replay ends (+2s), then saves the
# recording, the app's stderr, liveev.jsonl and the analyzer's summary to out-dir.
#
# Usage: scripts/live/run-ios-sim.sh <clip.mp4> [out-dir]   (default out-dir: out/live/ios-<clip>)
# Env: LIVE_SOURCE (default fr), LIVE_LOOP=1 to loop (records LIVE_SECS, default 30),
# TRANSLATTE_MODELS (default models-cache), SKIP_BUILD=1 to reuse the built app.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
[[ $# -ge 1 && -f "$1" ]] || { echo "usage: $0 <clip.mp4> [out-dir]" >&2; exit 2; }
CLIP="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
NAME="$(basename "${CLIP%.*}")"
OUT="$(mkdir -p "${2:-$ROOT/out/live/ios-$NAME}" && cd "${2:-$ROOT/out/live/ios-$NAME}" && pwd)"
MODELS="$(cd "${TRANSLATTE_MODELS:-$ROOT/models-cache}" && pwd)"
SOURCE="${LIVE_SOURCE:-fr}"
DEVICE="iPhone 18 Pro"
BUNDLE_ID=co.huggingface.translatte
DERIVED="$ROOT/target/ios-derived"
APP="$DERIVED/Build/Products/Debug-iphonesimulator/Translatte.app"

UDID=$(xcrun simctl list devices available -j | python3 -c "
import json, sys
for devices in json.load(sys.stdin)['devices'].values():
    for d in devices:
        if d['name'] == sys.argv[1]:
            print(d['udid']); sys.exit()
sys.exit('no simulator named ' + sys.argv[1])
" "$DEVICE")
xcrun simctl boot "$UDID" 2>/dev/null || true
xcrun simctl bootstatus "$UDID" -b >/dev/null

if [[ "${SKIP_BUILD:-0}" != 1 || ! -d "$APP" ]]; then
  [[ -d "$ROOT/ios/Frameworks/TranslatteFFI.xcframework" ]] || "$ROOT/scripts/ios/build-rust.sh" --sim-only
  (cd "$ROOT/ios" && xcodegen generate --quiet)
  xcodebuild build -project "$ROOT/ios/Translatte.xcodeproj" -scheme Translatte \
    -destination "platform=iOS Simulator,id=$UDID" -derivedDataPath "$DERIVED" CODE_SIGNING_ALLOWED=NO -quiet
fi
xcrun simctl install "$UDID" "$APP"
xcrun simctl terminate "$UDID" "$BUNDLE_ID" 2>/dev/null || true
DATA=$(xcrun simctl get_app_container "$UDID" "$BUNDLE_ID" data)
rm -f "$DATA/Documents/liveev.jsonl" "$OUT/screen.mp4" "$OUT/app.log" "$OUT/liveev.jsonl"

DURATION=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$CLIP" 2>/dev/null || echo 30)
LOOP_ARGS=()
[[ "${LIVE_LOOP:-0}" == 1 ]] && LOOP_ARGS=(-mock-video-loop YES)

xcrun simctl io "$UDID" recordVideo --codec h264 --force "$OUT/screen.mp4" 2>/dev/null &
REC=$!
sleep 1
SIMCTL_CHILD_TRANSLATTE_MODELS_DIR="$MODELS" xcrun simctl launch --stderr="$OUT/app.log" "$UDID" "$BUNDLE_ID" \
  -mock-video "$CLIP" ${LOOP_ARGS[@]+"${LOOP_ARGS[@]}"} -source "$SOURCE" -liveTranslation YES >/dev/null

if [[ "${LIVE_LOOP:-0}" == 1 ]]; then
  sleep "${LIVE_SECS:-30}"
else
  limit=$(python3 -c "import math, sys; print(math.ceil(float(sys.argv[1])) + 60)" "$DURATION")
  for _ in $(seq 1 "$limit"); do
    grep -q "live replay done" "$OUT/app.log" 2>/dev/null && break
    sleep 1
  done
  grep -q "live replay done" "$OUT/app.log" || echo "replay didn't finish within ${limit}s" >&2
  sleep 2
fi
kill -INT "$REC"
wait "$REC" || true

cp "$DATA/Documents/liveev.jsonl" "$OUT/liveev.jsonl" 2>/dev/null || grep '^liveev ' "$OUT/app.log" | cut -c8- >"$OUT/liveev.jsonl"
xcrun simctl terminate "$UDID" "$BUNDLE_ID" 2>/dev/null || true
python3 "$ROOT/scripts/live/analyze.py" "$OUT/liveev.jsonl" --out "$OUT" --video "$OUT/screen.mp4"
echo "recording: $OUT/screen.mp4"
echo "log: $OUT/app.log"
