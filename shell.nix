{ pkgs ? import <nixpkgs> { config.allowUnfree = true; config.android_sdk.accept_license = true; }
  # Emulator + System-Images (x86_64) fuer Geraetetests auf dem Rechner statt
  # auf dem Handy. Abschaltbar: nix-shell --arg emulator false
, emulator ? true
}:

let
  androidPkgs = pkgs.androidenv.androidPkgs;
  # Zum Bauen: platform-tools (adb), build-tools, platform 34 (compileSdk).
  # Fuer den Emulator zusaetzlich: Emulator 37.x und AOSP-System-Images
  # (`default`, ohne Google-Apps -> kleiner und `adb root` moeglich) fuer die
  # APIs, die hier aufgelistet sind. Die Images sind x86_64, damit KVM greift;
  # die App wird dafuer auch fuer x86_64 gebaut (siehe android/build-apk.sh).
  androidSdk = (pkgs.androidenv.composeAndroidPackages {
    buildToolsVersions = [ "34.0.0" ];
    platformVersions = [ "34" "36" ];
    includeEmulator = emulator;
    includeSystemImages = emulator;
    systemImageTypes = [ "default" ];
    abiVersions = [ "x86_64" ];
  }).androidsdk;
  androidNdk = androidPkgs.ndk-bundle;
in
pkgs.mkShell {
  packages = with pkgs; [
    rustc
    cargo
    sccache
    pkg-config
    gcc
  ];

  buildInputs = with pkgs; [
    alsa-lib
    clang
    llvmPackages.libclang
    cmake
    ninja
    opencv
    SDL2
    SDL2_ttf
    SDL2_image
    liblo
    libopenmpt
    boost
    xorg.libX11
    xorg.libXrandr
    xclip
    xsel
    wl-clipboard
    xdotool
    python3
    cargo-ndk
    rustup
    gradle
    jdk17
    androidSdk
    androidNdk
  ];

  shellHook = ''
    export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"
    export ANDROID_HOME="${androidSdk}/libexec/android-sdk"
    export ANDROID_SDK_ROOT="${androidSdk}/libexec/android-sdk"
    export ANDROID_NDK_HOME="${androidNdk}/libexec/android-sdk/ndk-bundle"
    export ANDROID_NDK_ROOT="${androidNdk}/libexec/android-sdk/ndk-bundle"
    export JAVA_HOME="${pkgs.jdk17.home}"
    export CARGO_BUILD_JOBS=$(nproc)
    export RUSTC_WRAPPER=sccache
    export CARGO_INCREMENTAL=0
    export SCCACHE_CACHE_SIZE="10G"

    # Emulator-Zustand (AVD, Snapshots, Einstellungen) liegt im Projekt unter
    # .android/ (in .gitignore): gut sichtbar, loeschbar, unabhängig vom
    # Handy-Setup. Steuerung ueber android/emulator.sh.
    # ANDROID_USER_HOME bleibt bewusst ungesetzt, damit adb dieselben Keys wie
    # fuers Handy benutzt (~/.android) und die Debug-Autorisierung gueltig bleibt.
    if [ -z "''${ANDROID_AVD_HOME:-}" ] || [ -z "''${ANDROID_EMULATOR_HOME:-}" ]; then
      _jamcore_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
      if [ -z "''${ANDROID_EMULATOR_HOME:-}" ]; then
        export ANDROID_EMULATOR_HOME="$_jamcore_root/.android"
      fi
      if [ -z "''${ANDROID_AVD_HOME:-}" ]; then
        export ANDROID_AVD_HOME="$_jamcore_root/.android/avd"
      fi
      unset _jamcore_root
    fi
  '';
}
