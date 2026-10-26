{
  pkgs ? import <nixpkgs> { config.allowUnfree = true; },
}:

let
  inherit (pkgs) lib;

  hashes = import ./hashes.nix;

  crateDir = {
    it_live = ".";
    pattern_recorder = "pattern_recorder";
    rrn5 = "rrn5";
    imgsh = "imgsh";
    bundle_udp = "bundle_udp";
    lissa = "lissa";
    jamcast = "jamcast";
  };

  srcDirName = "jam-nix-source";

  src = lib.cleanSourceWith {
    src = ./.;
    name = srcDirName;
    filter = path: type:
      let base = baseNameOf (toString path);
      in !(lib.elem base [ "target" "result" ".git" "android" ])
        && !(lib.hasPrefix "result-" base);
  };

  nativeBuildInputs = with pkgs; [
    pkg-config
    cmake
    ninja
    clang
    llvmPackages.llvm
    llvmPackages.libclang
    python3
  ];

  LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
  LLVM_CONFIG_PATH = "${pkgs.llvmPackages.llvm}/bin/llvm-config";

  buildInputs = with pkgs; [
    alsa-lib
    SDL2
    SDL2_ttf
    opencv
    boost
    liblo
    libopenmpt
    xorg.libX11
    xorg.libXrandr
  ];

  runtimeLibPath = lib.makeSearchPathOutput "lib" "lib" (
    with pkgs; [
      alsa-lib
      SDL2
      SDL2_ttf
      opencv
      xorg.libX11
      xorg.libXrandr
    ]
  );

  mkCrate =
    name:
    pkgs.rustPlatform.buildRustPackage {
      pname = name;
      version = "0.1.0";
      inherit src;
      sourceRoot = "${srcDirName}/${crateDir.${name}}";
      cargoHash = hashes.${name};
      inherit LIBCLANG_PATH LLVM_CONFIG_PATH;
      nativeBuildInputs =
        nativeBuildInputs
        ++ (with pkgs; [ rustc cargo rustfmt clippy makeWrapper ]);
      buildInputs = buildInputs ++ (with pkgs; [ gcc ]);
      dontUseNinjaBuild = "1";
      dontUseNinjaCheck = "1";
      dontUseNinjaInstall = "1";
      postInstall = ''
        wrapProgram $out/bin/${name} --prefix LD_LIBRARY_PATH : ${runtimeLibPath}
      '' + lib.optionalString (name == "pattern_recorder") ''
        mkdir -p $out/share/jam-tools
        ln -sf ${src}/sagan.ttf $out/share/jam-tools/sagan.ttf
      '';
    };
in
let
  it_live = mkCrate "it_live";
  pattern_recorder = mkCrate "pattern_recorder";
  rrn5 = mkCrate "rrn5";
  imgsh = mkCrate "imgsh";
  bundle_udp = mkCrate "bundle_udp";
  lissa = mkCrate "lissa";
  jamcast = mkCrate "jamcast";
in
{
  inherit it_live pattern_recorder rrn5 imgsh bundle_udp lissa jamcast;

  jam-tools = pkgs.symlinkJoin {
    name = "jam-tools-0.1.0";
    paths = [
      it_live
      pattern_recorder
      rrn5
      imgsh
      bundle_udp
      lissa
      jamcast
    ];
    postBuild = ''
      mkdir -p $out/share/jam-tools
      ln -sf ${src}/sagan.ttf $out/share/jam-tools/sagan.ttf
    '';
  };
}
