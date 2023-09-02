{
  inputs = {
    cargo2nix.url = "github:cargo2nix/cargo2nix/release-0.11.0";
    flake-utils.follows = "cargo2nix/flake-utils";
    nixpkgs.follows = "cargo2nix/nixpkgs";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, cargo2nix, rust-overlay, ... }:
    let
      pkgs = import nixpkgs {
        system = "x86_64-linux";
        overlays = [ cargo2nix.overlays.default rust-overlay.overlays.default ];
      };

      rustPkgs = pkgs.rustBuilder.makePackageSet {
        rustVersion = "1.70.0";
        packageFun = import ./Cargo.nix;
        extraRustComponents = [
          "clippy"
          "rustfmt"
        ];
      };
    in
    {
      packages.x86_64-linux = rec {
        fel = (rustPkgs.workspace.fel { });
      };

      devShells."x86_64-linux".default = (rustPkgs.workspaceShell {
        nativeBuildInputs = [
          cargo2nix.packages.x86_64-linux.default
          pkgs.go-containerregistry

          # Native dependencies for git2
          pkgs.pkg-config
          pkgs.libgit2
          pkgs.openssl
        ];
      });
    };
}
