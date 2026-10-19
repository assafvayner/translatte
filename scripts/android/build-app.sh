#!/usr/bin/env bash
# Build the Android app: cross-compile translatte-ffi, generate its Kotlin bindings, and assemble the APK.
# Usage: scripts/android/build-app.sh [debug|release] (default: debug)
set -euo pipefail

VARIANT="${1:-debug}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/scripts/android/env.sh"

cd "$ROOT"
cargo build -p translatte-ffi --release --target "$TARGET" --features download
LIB="$ROOT/target/$TARGET/release/libtranslatte_ffi.so"

GEN="$ROOT/android/app/src/generated"
JNI="$GEN/jniLibs/arm64-v8a"
rm -rf "$GEN"
mkdir -p "$JNI" "$GEN/kotlin"
cp "$LIB" "$ORT_DIR/lib/libonnxruntime.so" "$JNI/"
cp "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$HOST_TAG/sysroot/usr/lib/$TARGET/libc++_shared.so" "$JNI/"
# uniffi-bindgen is a host binary; keep the Android-only ORT/llama.cpp settings out of its build.
env -u ORT_LIB_LOCATION -u ORT_PREFER_DYNAMIC_LINK -u GGML_CPU_ARM_ARCH -u GGML_CPU_REPACK \
  cargo run -q -p translatte-ffi --bin uniffi-bindgen -- generate \
  --library "$LIB" --language kotlin --out-dir "$GEN/kotlin" --no-format

export ANDROID_HOME="$SDK"
TASK="assemble$(tr '[:lower:]' '[:upper:]' <<<"${VARIANT:0:1}")${VARIANT:1}"
(cd "$ROOT/android" && ./gradlew --quiet "$TASK")
APK="$ROOT/android/app/build/outputs/apk/$VARIANT/app-$VARIANT.apk"
echo "built $APK"
