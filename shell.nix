{ pkgs ? import <nixpkgs> {} }:

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
  ];

  shellHook = ''
    export CARGO_BUILD_JOBS=$(nproc)
    export RUSTC_WRAPPER=sccache
    export CARGO_INCREMENTAL=0
    export SCCACHE_CACHE_SIZE="10G"
  '';
}