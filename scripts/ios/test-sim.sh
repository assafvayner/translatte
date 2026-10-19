#!/usr/bin/env bash
# Builds the iOS app for the simulator, runs the XCTest suite, then launches the app with an
# autotest image and saves its result JSON and a screenshot to out/ios/.
#
# Usage: scripts/ios/test-sim.sh [models-dir] [image] [from] [to]
# Env: TRANSLATTE_SIM_DEVICE (default "iPhone 18 Pro"), SKIP_RUST_BUILD=1 to reuse the xcframework.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
MODELS="$(cd "${1:-$ROOT/models-cache}" && pwd)"
IMAGE="$(cd "$(dirname "${2:-$ROOT/core/tests/data/hello_world.png}")" && pwd)/$(basename "${2:-hello_world.png}")"
FROM="${3:-en}"
TO="${4:-de}"
DEVICE="${TRANSLATTE_SIM_DEVICE:-iPhone 18 Pro}"
BUNDLE_ID=co.huggingface.translatte
OUT="$ROOT/out/ios"
DERIVED="$ROOT/target/ios-derived"
mkdir -p "$OUT"

[[ "${SKIP_RUST_BUILD:-0}" == 1 ]] || "$ROOT/scripts/ios/build-rust.sh" --sim-only
(cd "$ROOT/ios" && xcodegen generate --quiet)

UDID=$(xcrun simctl list devices available -j | python3 -c "
import json, sys
name = sys.argv[1]
for devices in json.load(sys.stdin)['devices'].values():
    for d in devices:
        if d['name'] == name:
            print(d['udid']); sys.exit()
sys.exit('no simulator named ' + name)
" "$DEVICE")
xcrun simctl boot "$UDID" 2>/dev/null || true
xcrun simctl bootstatus "$UDID" -b >/dev/null

DEST="platform=iOS Simulator,id=$UDID"
TRANSLATTE_MODELS="$MODELS" xcodebuild test -project "$ROOT/ios/Translatte.xcodeproj" -scheme Translatte \
  -destination "$DEST" -derivedDataPath "$DERIVED" CODE_SIGNING_ALLOWED=NO \
  TRANSLATTE_MODELS="$MODELS" -quiet

APP="$DERIVED/Build/Products/Debug-iphonesimulator/Translatte.app"
xcrun simctl install "$UDID" "$APP"
xcrun simctl terminate "$UDID" "$BUNDLE_ID" 2>/dev/null || true
DATA=$(xcrun simctl get_app_container "$UDID" "$BUNDLE_ID" data)
rm -f "$DATA/Documents/autotest.json"
SIMCTL_CHILD_TRANSLATTE_MODELS_DIR="$MODELS" xcrun simctl launch "$UDID" "$BUNDLE_ID" \
  -autotest-image "$IMAGE" -autotest-from "$FROM" -autotest-to "$TO" >/dev/null

for _ in $(seq 1 120); do
  [[ -f "$DATA/Documents/autotest.json" ]] && break
  sleep 1
done
[[ -f "$DATA/Documents/autotest.json" ]] || { echo "autotest timed out" >&2; exit 1; }
sleep 1
name="$(basename "${IMAGE%.*}")_${FROM}_${TO}"
cp "$DATA/Documents/autotest.json" "$OUT/$name.json"
xcrun simctl io "$UDID" screenshot "$OUT/$name.png" >/dev/null
cat "$OUT/$name.json"
echo "screenshot: $OUT/$name.png"
