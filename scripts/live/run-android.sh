#!/usr/bin/env bash
# Replays a clip through live mode on a connected Android device running the debug app: pushes the
# clip to the app's external files dir, launches with the mock_video extra, records the screen for
# the clip's duration (+2s, plus LIVE_STARTUP_SECS for app start), then saves the recording, logcat,
# the liveev lines and the analyzer's summary to out-dir. Never starts an emulator; set
# ANDROID_SERIAL to pick a device.
#
# Usage: scripts/live/run-android.sh <clip.mp4> [out-dir]   (default out-dir: out/live/android-<clip>)
# Env: LIVE_SOURCE (default fr), LIVE_LOOP=1 to loop (records LIVE_SECS, default 30),
# LIVE_STARTUP_SECS (default 3), LIVE_RECORD=0 to skip the screen recording.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
[[ $# -ge 1 && -f "$1" ]] || { echo "usage: $0 <clip.mp4> [out-dir]" >&2; exit 2; }
CLIP="$1"
NAME="$(basename "${CLIP%.*}")"
OUT="${2:-$ROOT/out/live/android-$NAME}"
mkdir -p "$OUT"
SOURCE="${LIVE_SOURCE:-fr}"
PKG=co.huggingface.translatte
DEST="/sdcard/Android/data/$PKG/files/replay/$NAME.mp4"
REC=/sdcard/Movies/translatte-live.mp4

adb get-state >/dev/null
adb shell pm path "$PKG" >/dev/null || { echo "$PKG not installed (scripts/android/build-app.sh debug, then adb install)" >&2; exit 1; }
adb shell mkdir -p "$(dirname "$DEST")"
adb push "$CLIP" "$DEST" >/dev/null

DURATION=$(ffprobe -v error -show_entries format=duration -of csv=p=0 "$CLIP" 2>/dev/null || echo 30)
ANALYZE_ARGS=()
if [[ "${LIVE_LOOP:-0}" == 1 ]]; then
  SECS="${LIVE_SECS:-30}"
  LOOP_ARGS=(--ez mock_video_loop true)
  ANALYZE_ARGS=(--loop-ms "$(python3 -c "import sys; print(round(float(sys.argv[1]) * 1000))" "$DURATION")")
else
  SECS=$(python3 -c "import math, sys; print(math.ceil(float(sys.argv[1])) + 2 + int(sys.argv[2]))" "$DURATION" "${LIVE_STARTUP_SECS:-3}")
  LOOP_ARGS=()
fi
SECS=$((SECS > 180 ? 180 : SECS))

adb shell am force-stop "$PKG"
adb logcat -c
RECORD="${LIVE_RECORD:-1}"
if [[ "$RECORD" == 1 ]]; then
  adb shell rm -f "$REC"
  adb shell screenrecord --time-limit "$SECS" "$REC" &
else
  sleep "$SECS" &
fi
RECORDER=$!
sleep 1
adb shell am start -n "$PKG/.MainActivity" --es mock_video "$DEST" --es mock_video_source "$SOURCE" ${LOOP_ARGS[@]+"${LOOP_ARGS[@]}"} >/dev/null
wait "$RECORDER" || true
sleep 1

[[ "$RECORD" == 1 ]] && adb pull "$REC" "$OUT/screen.mp4" >/dev/null
adb logcat -d -s translatte:I >"$OUT/logcat.txt"
grep -o 'liveev .*' "$OUT/logcat.txt" | cut -c8- >"$OUT/liveev.jsonl" || true
grep -q "live replay done" "$OUT/logcat.txt" || [[ "${LIVE_LOOP:-0}" == 1 ]] || echo "no 'live replay done' in logcat" >&2
adb shell am force-stop "$PKG"
if [[ "$RECORD" == 1 ]]; then
  python3 "$ROOT/scripts/live/analyze.py" "$OUT/liveev.jsonl" --out "$OUT" ${ANALYZE_ARGS[@]+"${ANALYZE_ARGS[@]}"} --video "$OUT/screen.mp4"
  echo "recording: $OUT/screen.mp4"
else
  python3 "$ROOT/scripts/live/analyze.py" "$OUT/liveev.jsonl" --out "$OUT" ${ANALYZE_ARGS[@]+"${ANALYZE_ARGS[@]}"}
fi
echo "logcat: $OUT/logcat.txt"
