{ self, ... }: rec {
  flake.homeModules.default = flake.homeModules.pomidoro;
  flake.homeModules.pomidoro = { config, pkgs, lib, ... }: let
    cfg = config.programs.pomidoro;
    serviceEnabled = cfg.startService || config.services.pomidoro.enable;
    format = pkgs.formats.toml { };
  in {
    imports = [
      (import ./options.nix { inherit self; })
    ];

    config = lib.mkMerge [
      (lib.mkIf cfg.enable {
        home.packages = [ cfg.package ];

        xdg.configFile."pomidoro/config.toml".source = format.generate
          "pomidoro-config"
          (lib.filterAttrsRecursive (n: v: v != null) cfg.settings);
      })

      (lib.mkIf serviceEnabled {
        systemd.user.services.pomidoro = {
          Unit = {
            Description = "Pomidoro Server";
            Documentation = "https://github.com/joo-was-already-taken/pomidoro";
          };
          Service = {
            ExecStart = "${cfg.package}/bin/pomidoro start-server";
            Restart = "on-failure";
          };
          Install = {
            WantedBy = [ "default.target" ];
          };
        };
      })

      (lib.mkIf (cfg.tray.enable && cfg.tray.startService) {
        systemd.user.services.pomidoro-tray = {
          Unit = {
            Description = "Pomidoro Tray";
            Documentation = "https://github.com/joo-was-already-taken/pomidoro";
            After = [ "graphical-session.target" "pomidoro.service" ];
            PartOf = [ "graphical-session.target" ];
          };
          Service = {
            ExecStart = "${cfg.package}/bin/pomidoro-tray";
            Restart = "on-failure";
          };
          Install = {
            WantedBy = [ "graphical-session.target" ];
          };
        };
      })
    ];
  };
}
