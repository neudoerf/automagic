{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem
      (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        toolchain = pkgs.rust-bin.fromRustupToolchainFile ./toolchain.toml;
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = [
            pkgs.darwin.apple_sdk.frameworks.Security
            pkgs.rust-analyzer
            pkgs.lldb
          ];
          packages = [
            toolchain
          ];
        };
      }
      );
}

