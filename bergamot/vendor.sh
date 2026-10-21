#!/usr/bin/env bash
# Regenerates bergamot/vendor/inference: bergamot-translator from mozilla/translations' inference/ at a
# pinned commit, the submodules its CPU build needs, and pcre2 (which ssplit-cpp would otherwise download
# while configuring), pruned of tests and unused backends, then patched with bergamot/patches/*.diff.
# Usage: bergamot/vendor.sh
set -euo pipefail

TRANSLATIONS_REV=69455acaecbe8650cdba988dbcf7c10ca20e7c48
PCRE2_VERSION=10.39
PCRE2_SHA256=0781bd2536ef5279b1943471fdcdbd9961a2845e1d2c9ad849b9bd98ba1a9bd4

HERE="$(cd "$(dirname "$0")" && pwd)"
DEST="$HERE/vendor/inference"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

git init -q "$WORK/src"
cd "$WORK/src"
git remote add origin https://github.com/mozilla/translations
git fetch -q --depth 1 origin "$TRANSLATIONS_REV"
git checkout -q FETCH_HEAD
# Only what the CPU build uses; fbgemm, onnxjs, nccl, the websocket server and emsdk stay uninitialized.
git submodule update -q --init --depth 1 \
  inference/3rd_party/ssplit-cpp \
  inference/marian-fork/src/3rd_party/sentencepiece \
  inference/marian-fork/src/3rd_party/ruy \
  inference/marian-fork/src/3rd_party/intgemm \
  inference/marian-fork/src/3rd_party/simd_utils
git -C inference/marian-fork/src/3rd_party/ruy submodule update -q --init --depth 1 third_party/cpuinfo

rm -rf "$DEST"
mkdir -p "$DEST"
rsync -a \
  --exclude .git --exclude .gitmodules --exclude .gitignore --exclude .github \
  --exclude /3rd_party/emsdk \
  --exclude /marian-fork/src/3rd_party/fbgemm \
  --exclude /marian-fork/src/3rd_party/onnxjs \
  --exclude /marian-fork/src/3rd_party/nccl \
  --exclude /marian-fork/src/3rd_party/simple-websocket-server \
  --exclude /marian-fork/src/3rd_party/sentencepiece/data \
  --exclude /marian-fork/src/3rd_party/sentencepiece/python \
  --exclude /marian-fork/src/3rd_party/ruy/third_party/googletest \
  --exclude /marian-fork/src/3rd_party/ruy/third_party/cpuinfo/test \
  inference/ "$DEST/"
cp LICENSE "$DEST/LICENSE"

tarball="$WORK/pcre2-$PCRE2_VERSION.tar.gz"
curl -fsSL -o "$tarball" "https://github.com/PhilipHazel/pcre2/releases/download/pcre2-$PCRE2_VERSION/pcre2-$PCRE2_VERSION.tar.gz"
actual="$(shasum -a 256 "$tarball" | awk '{print $1}')"
[[ "$actual" == "$PCRE2_SHA256" ]] || { echo "pcre2 checksum mismatch: $actual" >&2; exit 1; }
tar -xzf "$tarball" -C "$DEST/3rd_party/ssplit-cpp/src/3rd-party"
[[ -f "$DEST/3rd_party/ssplit-cpp/src/3rd-party/pcre2-$PCRE2_VERSION/configure" ]]

# Nested ignore files (ssplit ignores its pcre2 copy) would keep vendored files out of git.
find "$DEST" -name .gitignore -delete

for p in "$HERE"/patches/*.diff; do
  patch -s -p1 -d "$HERE/vendor" < "$p"
done
echo "vendored $(du -sh "$DEST" | cut -f1) into $DEST"
