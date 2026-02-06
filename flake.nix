{
  description = "Circuit Breaker Labs CLI development environment";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  # change to trigger ci

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
      in
      {
        devShells =
          let
            rustShell = pkgs.mkShell {
              name = "rust-development-shell";
              nativeBuildInputs = rustBin ++ (with pkgs; [ rust-analyzer ]);
            };
          in
          {
            rust = rustShell;
            default = rustShell;
          };

        packages.default =
          let
            cargoToml = fromTOML (builtins.readFile ./Cargo.toml);
          in
          pkgs.rustPlatform.buildRustPackage {
            pname = cargoToml.package.name;
            inherit (cargoToml.package) version;

            src = ./.;
            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            meta = {
              inherit (cargoToml.package) description;
              homepage = cargoToml.package.repository;
              mainProgram = (builtins.head cargoToml.bin).name;
            };
          };

        apps.default = {
          type = "app";
          program = "${lib.getExe self.packages.${system}.default}";
        };

        checks =
          let
            mkCheck =
              {
                name,
                cmds,
                src ? self,
                inputs ? [ ],
              }:
              pkgs.runCommand name { buildInputs = inputs; } ''
                cd ${src}
                ${pkgs.lib.strings.concatLines cmds}
                touch $out
              '';

            checkArgs = {
              rustFormatting = {
                inputs = rustBin;
                cmds = [ "cargo fmt --check" ];
              };

              clippy = {
                inputs = rustBin;
                cmds = [ "cargo check" ];
              };
            };
          in
          builtins.mapAttrs (name: args: mkCheck (args // { inherit name; })) checkArgs;
      }
    );
}
