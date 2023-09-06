{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-23.05";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, utils, rust-overlay, crane, ... }:
  let
    supportedSystems = ["x86_64-linux" "x86_64-darwin" "aarch64-linux"];
    overlays = [
        (import rust-overlay) ];
  in
    utils.lib.eachSystem supportedSystems ( system: {
        pkgs = import nixpkgs { inherit system overlays; };
        rustVersion = pkgs.rust-bin.stable."1.70.0".default;
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustVersion;
          rustc = rustVersion;
        };

        packages.scrolls = crane.lib.${system}.buildPackage {
          src = self;
        };
        packages.default = self.packages.${system}.scrolls;
        devShell = pkgs.mkShell {
          buildInputs =
            [ (rustVersion.override { extensions = [ "rust-src" "rust-analyzer" ]; }) ];
            packages = [
                pkgs.cargo-edit
                pkgs.just
                pkgs.systemfd
                pkgs.cargo-watch
                pkgs.bacon
                pkgs.openssl
            ];
        };
      }
    );
}


