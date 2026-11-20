#!/usr/bin/env bash
# 交叉编译 tract 推理引擎为 Android 动态库，并投放到 jniLibs。
#
# 前置依赖：
#   rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
#   Android NDK r26+（通过 ANDROID_NDK_HOME / ANDROID_NDK_ROOT / ANDROID_HOME/ndk 自动探测）
#   cargo-ndk 可选：装了就用，没装则直接用 NDK 自带 clang 作为链接器
#
# 用法：bash build-android.sh [abi...]
#   默认构建 arm64-v8a armeabi-v7a x86_64（真机 + 模拟器）
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
JNI_LIBS_DIR="$SCRIPT_DIR/../../android/app/src/main/jniLibs"
API_LEVEL=21

if [ "$#" -gt 0 ]; then
  ABIS=("$@")
else
  ABIS=(arm64-v8a armeabi-v7a x86_64)
fi

find_ndk() {
  local candidates=()
  [ -n "${ANDROID_NDK_HOME:-}" ] && candidates+=("$ANDROID_NDK_HOME")
  [ -n "${ANDROID_NDK_ROOT:-}" ] && candidates+=("$ANDROID_NDK_ROOT")
  for sdk in "${ANDROID_HOME:-}" "${ANDROID_SDK_ROOT:-}" "$HOME/Library/Android/sdk" "$HOME/Android/Sdk"; do
    [ -n "$sdk" ] && [ -d "$sdk/ndk" ] && candidates+=("$(ls -d "$sdk"/ndk/* 2>/dev/null | sort -V | tail -n1)")
  done
  for c in "${candidates[@]}"; do
    [ -f "$c/source.properties" ] && echo "$c" && return 0
  done
  return 1
}

NDK="$(find_ndk || true)"
if [ -z "${NDK:-}" ]; then
  echo "未找到 Android NDK，请设置 ANDROID_NDK_HOME" >&2
  exit 1
fi

case "$(uname -s)" in
  Darwin) HOST_TAG=darwin-x86_64 ;;
  Linux)  HOST_TAG=linux-x86_64 ;;
  MINGW*|MSYS*|CYGWIN*) HOST_TAG=windows-x86_64 ;;
  *) echo "不支持的宿主平台: $(uname -s)" >&2; exit 1 ;;
esac
TOOLCHAIN="$NDK/toolchains/llvm/prebuilt/$HOST_TAG/bin"
EXT=""
[ "$HOST_TAG" = "windows-x86_64" ] && EXT=".cmd"

mkdir -p "$JNI_LIBS_DIR"
cd "$SCRIPT_DIR"

target_for_abi() {
  case "$1" in
    arm64-v8a)   echo "aarch64-linux-android" ;;
    armeabi-v7a) echo "armv7-linux-androideabi" ;;
    x86_64)      echo "x86_64-linux-android" ;;
    x86)         echo "i686-linux-android" ;;
    *) echo "未知 ABI: $1" >&2; exit 1 ;;
  esac
}

clang_prefix_for_target() {
  case "$1" in
    aarch64-linux-android) echo "aarch64-linux-android$API_LEVEL-clang" ;;
    armv7-linux-androideabi) echo "armv7a-linux-androideabi$API_LEVEL-clang" ;;
    x86_64-linux-android) echo "x86_64-linux-android$API_LEVEL-clang" ;;
    i686-linux-android) echo "i686-linux-android$API_LEVEL-clang" ;;
  esac
}

if command -v cargo-ndk >/dev/null 2>&1; then
  echo "使用 cargo-ndk 构建: ${ABIS[*]}"
  NDK_ARGS=()
  for abi in "${ABIS[@]}"; do
    NDK_ARGS+=(-t "$abi")
  done
  cargo ndk "${NDK_ARGS[@]}" --platform "$API_LEVEL" -o "$JNI_LIBS_DIR" build --release
else
  echo "未检测到 cargo-ndk，改用 NDK clang 直接链接: ${ABIS[*]}"
  for abi in "${ABIS[@]}"; do
    target="$(target_for_abi "$abi")"
    clang="$TOOLCHAIN/$(clang_prefix_for_target "$target")$EXT"
    if [ ! -x "$clang" ] && [ ! -f "$clang" ]; then
      echo "找不到编译器: $clang" >&2
      exit 1
    fi
    echo "==> $abi ($target)"
    env \
      "CARGO_TARGET_$(echo "$target" | tr 'a-z-' 'A-Z_')_LINKER=$clang" \
      "CC_$(echo "$target" | tr '-' '_')=$clang" \
      "AR_$(echo "$target" | tr '-' '_')=$TOOLCHAIN/llvm-ar$( [ "$HOST_TAG" = "windows-x86_64" ] && echo .exe )" \
      cargo build --release --target "$target"
    mkdir -p "$JNI_LIBS_DIR/$abi"
    cp "$SCRIPT_DIR/target/$target/release/libtract_engine.so" "$JNI_LIBS_DIR/$abi/"
  done
fi

echo "产物已写入: $JNI_LIBS_DIR"
find "$JNI_LIBS_DIR" -name 'libtract_engine.so' -exec ls -lh {} \;
