{
  description = "ron: notes/pulse/metric app (Rust)";

  inputs.nixpkgs.url = "path:/nix/store/y8q85gnyjrlfv3ylry3pjsiyvy2ksgzh-nixos-25.11.9840.a4bf06618f0b/nixos";

  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f system);
      pkgsFor = system: nixpkgs.legacyPackages.${system};
    in
    {
      devShells = forAll (system:
        let
          pkgs = pkgsFor system;
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              rustc
              cargo
              # rusqlite "bundled" feature compiles SQLite from C source.
              gcc
              # Handy for ad-hoc YAML/JSON inspection.
              yq
            ];
            RUST_LOG = "info";
          };
        });
    };
}
