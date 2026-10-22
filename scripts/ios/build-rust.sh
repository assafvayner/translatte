#!/usr/bin/env bash
# Builds translatte-ffi as a static library for iOS device + simulator, generates the Swift
# bindings with UniFFI and packages everything as ios/Frameworks/TranslatteFFI.xcframework.
#
# Usage: scripts/ios/build-rust.sh [--sim-only] [--debug]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim)
PROFILE=release
for arg in "$@"; do
  case "$arg" in
    --sim-only) TARGETS=(aarch64-apple-ios-sim) ;;
    --debug) PROFILE=debug ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done
PROFILE_FLAG=$([[ $PROFILE == release ]] && echo --release || true)

export IPHONEOS_DEPLOYMENT_TARGET=17.0
for target in "${TARGETS[@]}"; do
  cargo build --locked -p translatte-ffi --lib $PROFILE_FLAG --target "$target" --features download
done
TARGET_DIR="$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"

GEN="$ROOT/target/ios-gen"
rm -rf "$GEN" && mkdir -p "$GEN/headers"
cargo run -q --locked -p translatte-ffi --bin uniffi-bindgen -- generate \
  --library "$TARGET_DIR/${TARGETS[0]}/$PROFILE/libtranslatte_ffi.a" \
  --language swift --out-dir "$GEN"
mv "$GEN"/*.h "$GEN/headers/"
mv "$GEN/translatte_ffiFFI.modulemap" "$GEN/headers/module.modulemap"
mkdir -p "$ROOT/ios/Translatte/Generated"
cp "$GEN/translatte_ffi.swift" "$ROOT/ios/Translatte/Generated/"

XCF="$ROOT/ios/Frameworks/TranslatteFFI.xcframework"
rm -rf "$XCF"
args=()
for target in "${TARGETS[@]}"; do
  args+=(-library "$TARGET_DIR/$target/$PROFILE/libtranslatte_ffi.a" -headers "$GEN/headers")
done
xcodebuild -create-xcframework "${args[@]}" -output "$XCF" >/dev/null
echo "built $XCF"
