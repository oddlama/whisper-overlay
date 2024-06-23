{
  config,
  lib,
  pkgs,
  ...
}: let
  inherit
    (lib)
    escapeShellArgs
    getExe
    mkEnableOption
    mkIf
    optionals
    mkOption
    mkPackageOption
    types
    ;

  cfg = config.services.realtime-stt-server;
in {
  options.services.realtime-stt-server = {
    enable = mkEnableOption "realtime-stt-server";
    autoStart = mkOption {
      type = types.bool;
      default = false;
      description = ''
        Automatically start the server when graphical-session.target is reached.
        Not enabled by default due to high VRAM memory allocation - it is probably
        better to start and stop this service on demand.
      '';
    };
    package = mkPackageOption pkgs "realtime-stt-server" {};

    host = mkOption {
      type = types.str;
      description = "Host to bind to";
      default = "localhost";
    };

    port = mkOption {
      type = types.port;
      default = 7007;
      description = "Port to bind to";
    };

    model = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = "Main model used to generate the final transcription. Set to null to use default value.";
    };

    modelRealtime = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = "Faster model used to generate live transcriptions. Set to null to use default value.";
    };

    language = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = "Set the spoken language. Set to null to let the model auto-detect.";
    };

    extraOptions = mkOption {
      type = types.listOf types.str;
      default = [];
      example = ["--debug"];
      description = "Additional command-line arguments to pass to realtime-stt-server";
    };
  };

  config = mkIf cfg.enable {
    systemd.user.services.realtime-stt-server = {
      Install.WantedBy = mkIf cfg.autoStart ["graphical-session.target"];
      Unit = {
        Description = "A server for RealtimeSTT made to be used with whisper-overlay";
        PartOf = mkIf cfg.autoStart ["graphical-session.target"];
      };
      Service = {
        Restart = "on-failure";
        ExecStart =
          "${getExe cfg.package} "
          + escapeShellArgs (
            ["--host" cfg.host "--port" cfg.port]
            ++ optionals (cfg.model != null) ["--model" cfg.model]
            ++ optionals (cfg.modelRealtime != null) ["--model-realtime" cfg.modelRealtime]
            ++ optionals (cfg.language != null) ["--language" cfg.language]
            ++ cfg.extraOptions
          );
        Environment = [
          "HOME=%S/realtime-stt-server"
          "HF_HOME=%S/realtime-stt-server"
        ];
        WorkingDirectory = "%S/realtime-stt-server";
        StateDirectory = "realtime-stt-server";

        # Hardening
        #MemoryDenyWriteExecute = true;
        #PrivateTmp = true;     # Can't use as user service
        #PrivateDevices = true; # Needs CUDA
        #PrivateMounts = true;  # Can't use as user service
        ProtectClock = true;
        ProtectControlGroups = true;
        #ProtectHome = true;    # We need our home
        ProtectHostname = true;
        ProtectKernelLogs = true;
        ProtectKernelModules = true;
        ProtectKernelTunables = true;
        # ProtectProc = "invisible"; Needs /proc/cpuinfo
        ProtectSystem = "strict";
        RemoveIPC = true;
        RestrictAddressFamilies = [
          "AF_INET"
          "AF_INET6"
          "AF_UNIX"
        ];
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        SystemCallArchitectures = "native";
        SystemCallFilter = [
          "@system-service"
          "~@privileged"
        ];
        UMask = "0077";
      };
    };
  };
}
