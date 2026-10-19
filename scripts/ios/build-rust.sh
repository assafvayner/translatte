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

DEPS="$ROOT/target/ios-deps"
[[ -f "$DEPS/lib-sim/libonnxruntime.a" ]] || "$ROOT/scripts/ios/fetch-ort.sh"

export IPHONEOS_DEPLOYMENT_TARGET=17.0
for target in "${TARGETS[@]}"; do
  lib_dir=$([[ $target == *-sim ]] && echo "$DEPS/lib-sim" || echo "$DEPS/lib-ios")
  ORT_LIB_LOCATION="$lib_dir" cargo build -p translatte-ffi --lib $PROFILE_FLAG --target "$target"
done

GEN="$ROOT/target/ios-gen"
rm -rf "$GEN" && mkdir -p "$GEN/headers"
cargo run -q -p translatte-ffi --bin uniffi-bindgen -- generate \
  --library "$ROOT/target/${TARGETS[0]}/$PROFILE/libtranslatte_ffi.a" \
  --language swift --out-dir "$GEN"
mv "$GEN"/*.h "$GEN/headers/"
mv "$GEN/translatte_ffiFFI.modulemap" "$GEN/headers/module.modulemap"
mkdir -p "$ROOT/ios/Translatte/Generated"
cp "$GEN/translatte_ffi.swift" "$ROOT/ios/Translatte/Generated/"

XCF="$ROOT/ios/Frameworks/TranslatteFFI.xcframework"
rm -rf "$XCF"
args=()
for target in "${TARGETS[@]}"; do
  args+=(-library "$ROOT/target/$target/$PROFILE/libtranslatte_ffi.a" -headers "$GEN/headers")
done
xcodebuild -create-xcframework "${args[@]}" -output "$XCF" >/dev/null
echo "built $XCF"
