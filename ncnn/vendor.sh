#!/usr/bin/env bash
# Regenerates ncnn/vendor/ncnn: Tencent/ncnn at a pinned release, only what the CPU library build
# needs (no Vulkan shaders, tools, tests, Python, or layers for architectures the apps don't target).
# Usage: ncnn/vendor.sh
set -euo pipefail

NCNN_TAG=20260526
NCNN_REV=e54f7b1f88434e1d844ea0551b880a1cfb079ce1

HERE="$(cd "$(dirname "$0")" && pwd)"
DEST="$HERE/vendor/ncnn"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

git init -q "$WORK/src"
cd "$WORK/src"
git remote add origin https://github.com/Tencent/ncnn
git fetch -q --depth 1 origin "refs/tags/$NCNN_TAG"
git checkout -q FETCH_HEAD
[[ "$(git rev-parse HEAD)" == "$NCNN_REV" ]] || { echo "tag $NCNN_TAG moved: $(git rev-parse HEAD)" >&2; exit 1; }

rm -rf "$DEST"
mkdir -p "$DEST"
rsync -a \
  --exclude /src/layer/vulkan \
  --exclude /src/layer/x86 \
  --exclude /src/layer/mips \
  --exclude /src/layer/loongarch \
  --exclude /src/layer/riscv \
  --exclude '/src/*.comp' \
  CMakeLists.txt LICENSE.txt cmake src "$DEST/"
echo "vendored $(du -sh "$DEST" | cut -f1) into $DEST"
