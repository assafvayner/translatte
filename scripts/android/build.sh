#!/usr/bin/env bash
# Cross-compile translatte-cli for aarch64-linux-android (see README "Android CLI smoke test").
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/scripts/android/env.sh"

cd "$ROOT"
# TLS=native (default here): OpenSSL-backed, test-only, since the rustls verifier needs JNI, which a bare
# `adb shell` binary lacks, so `fetch` panics under TLS=rustls. The app keeps rustls (see README "Android").
TLS="${TLS:-native}"
cargo build --locked -p translatte-cli --release --target "$TARGET" --no-default-features --features "tls-$TLS" "$@"

TARGET_DIR="$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
OUT="$TARGET_DIR/$TARGET/release"
cp "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$HOST_TAG/sysroot/usr/lib/$TARGET/libc++_shared.so" "$OUT/"
echo "built $OUT/translatte (+ libc++_shared.so)"
