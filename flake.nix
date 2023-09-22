
{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-23.05";
    utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
    ra-multiplex.url = "github:freexploit/ra-multiplex";

  };

  outputs = { self, nixpkgs, utils, rust-overlay, crane,ra-multiplex, ... }:
    utils.lib.eachDefaultSystem (system:
      let
        overlays = [
            (import rust-overlay)

        ];
        pkgs = import nixpkgs { inherit system overlays; };
        craneLib = crane.lib.${system};

        rustVersion = pkgs.rust-bin.stable."1.72.0".default;

        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustVersion;
          rustc = rustVersion;
        };
        scroll = craneLib.buildPackage {
          src = self;
        };

      in {
        packages = {
          inherit scroll;
          default = scroll;
        };
        devShell = pkgs.mkShell {
          buildInputs =
            [ (rustVersion.override { extensions = [ "rust-src" "rust-analyzer" ]; }) ];
            packages = [
                pkgs.cargo-watch
                pkgs.bacon
                pkgs.cargo-edit
                ra-multiplex.packages.x86_64-linux.ra-multiplex
            ];
        };
      });
}
