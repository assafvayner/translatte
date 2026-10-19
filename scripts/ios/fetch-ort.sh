#!/usr/bin/env bash
# Downloads the official ONNX Runtime iOS build (static xcframework) matching the version
# ort-sys expects, and extracts per-target static libraries for linking the Rust core.
set -euo pipefail

ORT_VERSION=1.22.0
# Pinned on first download; ONNX Runtime doesn't publish checksums for the pod archive.
ORT_SHA256=90d9de5a139087a6b05a18125d01d01d198820e1731e6f0f11b38749b2ab181f

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DEPS="$ROOT/target/ios-deps"
ZIP="$DEPS/onnxruntime-c-$ORT_VERSION.zip"
mkdir -p "$DEPS"

if [[ ! -f "$ZIP" ]]; then
  curl -sSfL -o "$ZIP.part" "https://download.onnxruntime.ai/pod-archive-onnxruntime-c-$ORT_VERSION.zip"
  mv "$ZIP.part" "$ZIP"
fi
echo "$ORT_SHA256  $ZIP" | shasum -a 256 -c - >/dev/null

rm -rf "$DEPS/ort" "$DEPS/lib-ios" "$DEPS/lib-sim"
unzip -q "$ZIP" -d "$DEPS/ort"
XCF="$DEPS/ort/onnxruntime.xcframework"
mkdir -p "$DEPS/lib-ios" "$DEPS/lib-sim"
cp "$XCF/ios-arm64/onnxruntime.framework/onnxruntime" "$DEPS/lib-ios/libonnxruntime.a"
lipo "$XCF/ios-arm64_x86_64-simulator/onnxruntime.framework/onnxruntime" -thin arm64 \
  -output "$DEPS/lib-sim/libonnxruntime.a"
echo "ONNX Runtime $ORT_VERSION static libs in $DEPS/lib-{ios,sim}"
