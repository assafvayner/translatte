#!/usr/bin/env bash
# Cross-compile translatte-cli for aarch64-linux-android (see README "Android (emulator) smoke test").
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TARGET=aarch64-linux-android
API="${ANDROID_API_LEVEL:-31}"
ORT_VERSION=1.22.0 # ONNX Runtime API version expected by ort 2.0.0-rc.10
ORT_AAR_SHA256=04a4617a9c797cf49225595e45b5546081cb34c86ac817581141577d3b7dbfe2

SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-/opt/homebrew/share/android-commandlinetools}}"
if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  ANDROID_NDK_HOME="$(ls -d "$SDK"/ndk/* 2>/dev/null | sort -V | tail -1)"
fi
[[ -d "$ANDROID_NDK_HOME" ]] || { echo "NDK not found; set ANDROID_NDK_HOME" >&2; exit 1; }
export ANDROID_NDK_HOME ANDROID_NDK="$ANDROID_NDK_HOME" ANDROID_API_LEVEL="$API"

case "$(uname -s)" in
  Darwin) HOST_TAG=darwin-x86_64 ;;
  Linux) HOST_TAG=linux-x86_64 ;;
  *) echo "unsupported host" >&2; exit 1 ;;
esac
BIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$HOST_TAG/bin"
export CC_aarch64_linux_android="$BIN/${TARGET}${API}-clang"
export CXX_aarch64_linux_android="$BIN/${TARGET}${API}-clang++"
export AR_aarch64_linux_android="$BIN/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"

# ONNX Runtime: the official Android AAR from Maven Central, linked dynamically.
DEPS="$ROOT/target/android-deps"
ORT_DIR="$DEPS/ort"
if [[ ! -f "$ORT_DIR/lib/libonnxruntime.so" ]]; then
  mkdir -p "$DEPS/aar" "$ORT_DIR/lib"
  AAR="$DEPS/aar/onnxruntime-android-$ORT_VERSION.aar"
  URL="https://repo1.maven.org/maven2/com/microsoft/onnxruntime/onnxruntime-android/$ORT_VERSION/onnxruntime-android-$ORT_VERSION.aar"
  curl -fsSL "$URL" -o "$AAR"
  published="$(curl -fsSL "$URL.sha256" | awk '{print $1}')"
  actual="$(shasum -a 256 "$AAR" | awk '{print $1}')"
  if [[ "$actual" != "$published" || "$actual" != "$ORT_AAR_SHA256" ]]; then
    echo "ORT AAR checksum mismatch: got $actual, published $published, pinned $ORT_AAR_SHA256" >&2
    rm -f "$AAR"
    exit 1
  fi
  unzip -o -j "$AAR" "jni/arm64-v8a/libonnxruntime.so" -d "$ORT_DIR/lib" >/dev/null
fi
export ORT_LIB_LOCATION="$ORT_DIR/lib" ORT_PREFER_DYNAMIC_LINK=1

cd "$ROOT"
cargo build -p translatte-cli --release --target "$TARGET" "$@"

OUT="$ROOT/target/$TARGET/release"
cp "$ORT_DIR/lib/libonnxruntime.so" "$OUT/"
cp "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$HOST_TAG/sysroot/usr/lib/$TARGET/libc++_shared.so" "$OUT/"
echo "built $OUT/translatte (+ libonnxruntime.so, libc++_shared.so)"
