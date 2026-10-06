#!/usr/bin/env bash
# Baut die Android-App (arm64-v8a) und legt das APK unter app/build/outputs/apk/debug/.
# Aufruf aus dem Projektordner: bash android/build-apk.sh [--install]
set -euo pipefail

cd "$(dirname "$0")"

export PATH="$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH"
export RUSTC_WRAPPER=
export CARGO_BUILD_JOBS="$(nproc)"

# Ableton-Link (rusty_link) baut sein C++-SDK per CMake mit. Android gilt dort
# nicht als Linux-Plattform und Bionic hat kein htonll; beides ergaenzt die
# Toolchain-Datei (die cmake-Kiste liest CMAKE_TOOLCHAIN_FILE aus der Umgebung).
export CMAKE_TOOLCHAIN_FILE="$(pwd)/link-android-toolchain.cmake"

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
