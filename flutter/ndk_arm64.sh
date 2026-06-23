#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ANDROID_NDK="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}"

if [ -z "${ANDROID_NDK}" ]; then
    echo "ANDROID_NDK_HOME or ANDROID_NDK_ROOT must point to the Android NDK" >&2
    exit 1
fi

cd "${REPO_DIR}"

cargo ndk --platform 21 --target aarch64-linux-android build --locked --release --features flutter,hwcodec,mediacodec

JNI_DIR="flutter/android/app/src/main/jniLibs/arm64-v8a"
mkdir -p "${JNI_DIR}"
cp "target/aarch64-linux-android/release/liblibrustdesk.so" "${JNI_DIR}/librustdesk.so"
cp "${ANDROID_NDK}/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so" "${JNI_DIR}/"
"${ANDROID_NDK}/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip" "${JNI_DIR}/librustdesk.so" "${JNI_DIR}/libc++_shared.so"
