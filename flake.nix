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
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];

      imports = [
        ./nix/package.nix
        ./nix/home-manager.nix
      ];

      perSystem = { pkgs, inputs', ... }: {
        devShells.default = pkgs.mkShell {
          packages = [
            (inputs'.fenix.packages.stable.withComponents [
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
            pkgs.cargo-audit
            pkgs.cargo-edit
          ];
        };
      };
    });
}
