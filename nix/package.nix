{ inputs, lib, ... }: {
  perSystem = { pkgs, inputs', ... }: let
    cargoToml = builtins.fromTOML (builtins.readFile ../Cargo.toml);
    pkgName = cargoToml.package.name;
    inherit (inputs.gitignore.lib) gitignoreSource;
    toolchain = inputs'.fenix.packages.stable.withComponents [ "cargo" "rustc" ];
    rustPlatform = pkgs.makeRustPlatform { cargo = toolchain; rustc = toolchain; };
  in rec {
    packages.default = packages.${pkgName};
    packages.${pkgName} = rustPlatform.buildRustPackage {
      name = pkgName;
      version = cargoToml.package.version;
      src = gitignoreSource ./..;
      cargoLock.lockFile = ../Cargo.lock;

      doCheck = true;
      nativeCheckInputs = with pkgs; [ bats bubblewrap parallel jq ];
      postCheck = ''
        POMIDORO_BIN="$(echo target/*/release/${pkgName})"
        bats -j "$NIX_BUILD_CORES" tests/
      '';

      meta = with lib; {
        description = "Pomidoro is a pomodoro timer with client-server architecture and statistic collection.";
        homepage = "https://github.com/joo-was-already-taken/pomidoro";
        license = licenses.mit;
        platforms = platforms.linux;
        mainProgram = pkgName;
      };
    };

    checks.${pkgName} = packages.${pkgName};
  };
}
