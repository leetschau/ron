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
          # The plain nixpkgs rustc only ships std for the host platform;
          # for musl targets use the musl64 cross rustc, which also has
          # x86_64-unknown-linux-musl std in its sysroot.
          muslEnv = nixpkgs.lib.optionalAttrs (system == "x86_64-linux") (
            let
              musl = pkgs.pkgsCross.musl64;
            in
            {
              RUSTC = "${musl.buildPackages.rustc}/bin/rustc";
              CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = "${musl.stdenv.cc}/bin/x86_64-unknown-linux-musl-cc";
              CC_x86_64_unknown_linux_musl = "${musl.stdenv.cc}/bin/x86_64-unknown-linux-musl-cc";
              AR_x86_64_unknown_linux_musl = "${musl.stdenv.cc}/bin/x86_64-unknown-linux-musl-ar";
              # The nixpkgs rustc wrapper disables crt-static by default.
              CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS = "-C target-feature=+crt-static";
            }
          );
        in
        {
          default = pkgs.mkShell ({
            packages = with pkgs; [
              rustc
              cargo
              # rusqlite "bundled" feature compiles SQLite from C source.
              gcc
              # Handy for ad-hoc YAML/JSON inspection.
              yq
            ];
            RUST_LOG = "info";
          } // muslEnv);
        });
    };
}
