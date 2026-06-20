{ self, ... }: rec {
  flake.homeModules.default = flake.homeModules.pomidoro;
  flake.homeModules.pomidoro = { config, pkgs, lib, ... }: let
    inherit (lib) mkEnableOption mkOption types;
    cfg = config.programs.pomidoro;
    serviceEnabled = cfg.startService || config.services.pomidoro.enable;
    format = pkgs.formats.toml { };
  in {
    options.programs.pomidoro = {
      enable = mkEnableOption "Whether to enable Pomidoro.";
      startService = mkEnableOption "Whether to run Pomidoro server as a service.";

      package = mkOption {
        type = types.package;
        default = self.packages.${pkgs.stdenv.hostPlatform.system}.pomidoro.override { withTray = cfg.tray.enable; };
        defaultText = lib.literalExpression
          "inputs.pomidoro.packages.$${pkgs.stdenv.hostPlatform.system}.pomidoro.override { withTray = config.programs.pomidoro.tray.enable; }";
        description = "The pomidoro package to use.";
      };

      tray = {
        enable = mkOption {
          type = types.bool;
          default = true;
          description = "Whether to build the pomidoro-tray binary.";
        };
        startService = mkOption {
          type = types.bool;
          default = serviceEnabled;
          description = "Whether to run Pomidoro tray icon as a service.";
        };
      };

      settings = mkOption {
        type = types.submodule {
          freeformType = format.type;
          options = {
            cycle = mkOption {
              type = types.nullOr (types.listOf types.str);
              default = null;
              description = "The sequence of intervals to iterate through.";
              example = [ "work" "short break" "work" "long break" ];
            };
            intervals = mkOption {
              type = types.attrsOf (types.submodule {
                options = {
                  duration = mkOption {
                    type = types.str;
                    description = "Duration of the interval, parsed by humantime.";
                    example = [ "25m" "5s" "10m30s" ];
                  };
                };
              });
              default = { };
              description = "Definitions of intervals.";
            };
            socket = mkOption {
              type = types.nullOr (types.either types.str (types.submodule {
                options = {
                  addr = mkOption {
                    type = types.str;
                    description =
                      "The address or path for the socket. Can use environment variables and $${uid}.";
                    example = [
                      "$${XDG_RUNTIME_DIR}/pomidoro.sock"
                      "pomidoro-server-$${uid}"
                    ];
                  };
                  abstract = mkOption {
                    type = types.nullOr types.bool;
                    default = null;
                    description = "Whether to use an abstract socket (Linux only).";
                  };
                };
              }));
              default = null;
              description = "Configuration for the IPC socket.";
            };
          };
        };
        default = { };
        description =
          "Configuration for pomidoro, written to `$XDG_CONFIG_HOME/pomidoro/config.toml`.";
      };
    };

    options.services.pomidoro.enable = mkEnableOption
      "Whether to run Pomidoro server as a service.";

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
            After = [ "graphical-session.target" ];
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
