{
  description = "jamtools: it_live, pattern_recorder, rrn5, imgsh, bundle_udp";

  inputs.nixpkgs.url = "flake:nixpkgs";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      appNames = [
        "it_live"
        "pattern_recorder"
        "rrn5"
        "imgsh"
        "bundle_udp"
        "lissa"
      ];
    in
    {
      devShells = forAllSystems (pkgs: {
        default = import ./shell.nix { inherit pkgs; };
      });

      packages = forAllSystems (
        pkgs:
        let
          p = import ./default.nix { inherit pkgs; };
        in
        {
          inherit (p) it_live pattern_recorder rrn5 imgsh bundle_udp lissa jam-tools;
          default = p.jam-tools;
        }
      );

      apps = forAllSystems (
        pkgs:
        let
          p = import ./default.nix { inherit pkgs; };
        in
        builtins.listToAttrs (
          map (n: nixpkgs.lib.nameValuePair n {
            type = "app";
            program = "${p.${n}}/bin/${n}";
          }) appNames
        )
      );
    };
}
