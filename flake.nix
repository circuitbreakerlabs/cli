{
  description = "Circuit Breaker Labs CLI development environment";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    { self, nixpkgs, ... }@inputs:
    inputs.flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import inputs.rust-overlay) ];
        };

        inherit (nixpkgs) lib;

        rustBin = with pkgs; [
          (rust-bin.stable.latest.default.override {
            extensions = [
              "clippy"
              "rust-src"
            ];
          })
        ];

        cargoToml = fromTOML (builtins.readFile ./Cargo.toml);

        meta = {
          inherit (cargoToml.package) description;
          homepage = cargoToml.package.repository;
          mainProgram = (builtins.head cargoToml.bin).name;
        };
      in
      {
        devShells =
          let
            rustShell = pkgs.mkShell {
              name = "rust-development-shell";
              nativeBuildInputs = rustBin ++ (with pkgs; [ rust-analyzer ]);
            };

            nixShell = pkgs.mkShell {
              name = "nix-development-shell";
              nativeBuildInputs = with pkgs; [
                statix
                nixfmt
              ];
            };
          in
          {
            rust = rustShell;
            nix = nixShell;
            default = rustShell;
          };

        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          inherit (cargoToml.package) version;
          inherit meta;

          src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
        };

        apps.default = {
          type = "app";
          inherit meta;
          program = "${lib.getExe self.packages.${system}.default}";
        };
      }
    );
}
