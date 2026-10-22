# Sourced by the Android build scripts: NDK toolchain settings for aarch64-linux-android; the
# bergamot and ncnn crates' CMake builds read ANDROID_NDK and ANDROID_API_LEVEL from here. Expects ROOT to be the repo root.

TARGET=aarch64-linux-android
API="${ANDROID_API_LEVEL:-31}"

SDK="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-/opt/homebrew/share/android-commandlinetools}}"
if [[ -z "${ANDROID_NDK_HOME:-}" ]]; then
  ANDROID_NDK_HOME="$(ls -d "$SDK"/ndk/* 2>/dev/null | sort -V | tail -1 || true)"
fi
[[ -n "$ANDROID_NDK_HOME" && -d "$ANDROID_NDK_HOME" ]] || { echo "NDK not found; set ANDROID_NDK_HOME" >&2; exit 1; }
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

