{
  description = "Pomidoro is a pomodoro timer with client-server architecture and statistic collection";

  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    gitignore = {
      url = "github:hercules-ci/gitignore.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, flake-parts, gitignore, fenix, ... }@inputs:
    flake-parts.lib.mkFlake { inherit inputs; } ({ ... }: {
      systems = [ "x86_64-linux" "aarch64-linux" ];
      perSystem = { system, pkgs, ... }: let
        packageName = "pomidoro";
        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
        inherit (gitignore.lib) gitignoreSource;
        toolchain = fenix.packages.${system}.stable.withComponents [ "cargo" "rustc" ];
        rustPlatform = pkgs.makeRustPlatform { cargo = toolchain; rustc = toolchain; };
      in rec {
        packages.default = packages.${packageName};
        packages.${packageName} = rustPlatform.buildRustPackage {
          name = packageName;
          version = cargoToml.package.version;
          src = gitignoreSource ./.;
          cargoLock.lockFile = ./Cargo.lock;

          doCheck = true;
          nativeCheckInputs = with pkgs; [ bats bubblewrap parallel jq ];
          postCheck = ''
            POMIDORO_BIN="$(echo target/*/release/${packageName})"
            bats -j "$NIX_BUILD_CORES" tests/
          '';
        };

        checks.${packageName} = packages.${packageName};

        devShells.default = pkgs.mkShell {
          packages = [
            (fenix.packages.${system}.stable.withComponents [
              "cargo"
              "clippy"
              "rust-src"
              "rustc"
              "rustfmt"
              "rust-analyzer"
            ])
            pkgs.bats
            pkgs.bubblewrap
            pkgs.parallel
            pkgs.jq
            pkgs.socat
          ];
        };
      };
    });
}
