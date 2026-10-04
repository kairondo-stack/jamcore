#!/usr/bin/env bash
# Baut die Android-App (arm64-v8a) und legt das APK unter app/build/outputs/apk/debug/.
# Aufruf aus dem Projektordner: bash android/build-apk.sh [--install]
set -euo pipefail

cd "$(dirname "$0")"

export PATH="$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH"
export RUSTC_WRAPPER=
export CARGO_BUILD_JOBS="$(nproc)"

cargo ndk -t arm64-v8a --platform 26 -o app/src/main/jniLibs build --release

for cxx in "$ANDROID_NDK_HOME"/toolchains/llvm/prebuilt/*/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so; do
  [ -f "$cxx" ] && cp -f "$cxx" app/src/main/jniLibs/arm64-v8a/
done

gradle --no-daemon assembleDebug \
  -Pandroid.aapt2FromMavenOverride="$ANDROID_HOME/build-tools/34.0.0/aapt2"

APK=app/build/outputs/apk/debug/app-debug.apk
echo
echo "APK: $(pwd)/$APK ($(du -h "$APK" | cut -f1))"

if [ "${1:-}" = "--install" ]; then
  adb install -r "$APK"
  adb shell am start -n org.jamcore.app/android.app.NativeActivity
  echo "gestartet, logcat: adb logcat -s jamandroid"
fi
