{ self }: { config, pkgs, lib, ... }: let
  inherit (lib) mkEnableOption mkOption types;
  cfg = config.programs.pomidoro;
  serviceEnabled = cfg.startService || config.services.pomidoro.enable;
  format = pkgs.formats.toml { };

  hookType = types.either types.str (types.listOf types.str);

  overtimeType = types.submodule {
    options = {
      every = mkOption {
        type = types.str;
        description = "How often to execute the overtime hook, parsed by humantime.";
        example = "5m";
      };
      execute = mkOption {
        type = hookType;
        description = "Command or script to execute on overtime tick.";
      };
    };
  };

  hooksType = types.submodule {
    options = {
      on_start = mkOption {
        type = types.nullOr hookType;
        default = null;
        description = "Command or script to execute when the interval starts.";
      };
      on_completion = mkOption {
        type = types.nullOr hookType;
        default = null;
        description = "Command or script to execute when the interval completes.";
      };
      on_resume = mkOption {
        type = types.nullOr hookType;
        default = null;
        description = "Command or script to execute when the interval resumes from a pause.";
      };
      on_pause = mkOption {
        type = types.nullOr hookType;
        default = null;
        description = "Command or script to execute when the interval is paused.";
      };
      overtime = mkOption {
        type = types.nullOr overtimeType;
        default = null;
        description = "Overtime hook configuration.";
      };
    };
  };
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
                hooks = mkOption {
                  type = types.nullOr hooksType;
                  default = null;
                  description = "Interval specific hooks.";
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
          hooks = mkOption {
            type = types.nullOr hooksType;
            default = null;
            description = "Global hooks.";
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
}
