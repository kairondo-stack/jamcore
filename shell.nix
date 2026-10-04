{ pkgs ? import <nixpkgs> { config.allowUnfree = true; config.android_sdk.accept_license = true; } }:

let
  androidPkgs = pkgs.androidenv.androidPkgs;
  # Nur was zum Bauen noetig ist: platform-tools (adb), build-tools, platform 34.
  # Kein Emulator/System-Images (riesig, nicht substituierbar).
  androidSdk = (pkgs.androidenv.composeAndroidPackages {
    buildToolsVersions = [ "34.0.0" ];
    platformVersions = [ "34" ];
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
  '';
}