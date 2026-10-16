#!/usr/bin/env bash
# Baut die Android-App und legt das APK unter app/build/outputs/apk/debug/.
# Aufruf aus dem Projektordner: bash android/build-apk.sh [--install] [--abi <abi>]
#
# ABIs: Default "arm64-v8a x86_64" (Handy + Emulator). Fuer schnelle Iteration
# nur eine ABI bauen, z.B.:
#   ABIS=x86_64    bash android/build-apk.sh      # Emulator
#   ABIS=arm64-v8a bash android/build-apk.sh      # Handy
# oder: bash android/build-apk.sh --abi x86_64
#
# Hinweis: Der Emulator ist x86_64; ein arm64-only-APK laesst sich dort nicht
# installieren (INSTALL_FAILED_NO_MATCHING_ABIS).
set -euo pipefail

cd "$(dirname "$0")"

ABIS="${ABIS:-arm64-v8a x86_64}"

install_after=0
while [ $# -gt 0 ]; do
  case "$1" in
    --install) install_after=1 ;;
    --abi) shift; ABIS="${1//,/ }" ;;
    --abi=*) ABIS="${1#--abi=}"; ABIS="${ABIS//,/ }" ;;
    *) echo "Unbekannte Option: $1" >&2; exit 2 ;;
  esac
  shift
done
export ABIS # build.gradle packt nur diese ABIs ein

export PATH="$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin:$PATH"
export RUSTC_WRAPPER=
export CARGO_BUILD_JOBS="$(nproc)"

# Ableton-Link (rusty_link) baut sein C++-SDK per CMake mit. Android gilt dort
# nicht als Linux-Plattform und Bionic hat kein htonll; beides ergaenzt die
# Toolchain-Datei (die cmake-Kiste liest CMAKE_TOOLCHAIN_FILE aus der Umgebung).
export CMAKE_TOOLCHAIN_FILE="$(pwd)/link-android-toolchain.cmake"

abi_triple() {
  case "$1" in
    arm64-v8a)   echo aarch64-linux-android ;;
    armeabi-v7a) echo arm-linux-androideabi ;;
    x86_64)      echo x86_64-linux-android ;;
    x86)         echo i686-linux-android ;;
    *) echo "unbekannte ABI: $1" >&2; return 1 ;;
  esac
}

for abi in $ABIS; do
  triple="$(abi_triple "$abi")"
  echo "=== $abi ($triple)"
  cargo ndk -t "$abi" --platform 26 -o app/src/main/jniLibs build --release

  for cxx in "$ANDROID_NDK_HOME"/toolchains/llvm/prebuilt/*/sysroot/usr/lib/"$triple"/libc++_shared.so; do
    [ -f "$cxx" ] && cp -f "$cxx" app/src/main/jniLibs/"$abi"/
  done
done

ls -l app/src/main/jniLibs/*/libjamandroid.so

gradle --no-daemon assembleDebug \
  -Pandroid.aapt2FromMavenOverride="$ANDROID_HOME/build-tools/34.0.0/aapt2"

APK=app/build/outputs/apk/debug/app-debug.apk
echo
echo "APK: $(pwd)/$APK ($(du -h "$APK" | cut -f1))"

if [ "$install_after" = 1 ]; then
  adb install -r "$APK"
  adb shell am start -n org.jamcore.app/android.app.NativeActivity
  echo "gestartet, logcat: adb logcat -s jamandroid"
fi
