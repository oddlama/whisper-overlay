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
    package = mkPackageOption pkgs "realtime-stt-server" {};

    openFirewall = mkOption {
      type = types.bool;
      default = false;
      description = "Whether to open the relevant port for realtime-stt-server in your firewall";
    };

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
    systemd.services.realtime-stt-server = {
      description = "A server for RealtimeSTT made to be used with whisper-overlay";
      wantedBy = ["multi-user.target"];
      after = ["network.target"];

      environment.HOME = "/var/lib/realtime-stt-server";
      environment.HF_HOME = "/var/lib/realtime-stt-server";
      serviceConfig = {
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
        DynamicUser = true;
        User = "realtime-stt-server";
        Group = "realtime-stt-server";

        WorkingDirectory = "/var/lib/realtime-stt-server";
        StateDirectory = "realtime-stt-server";
        StateDirectoryMode = "0750";

        # Hardening
        CapabilityBoundingSet = "";
        LockPersonality = true;
        #MemoryDenyWriteExecute = true;
        NoNewPrivileges = true;
        PrivateUsers = true;
        PrivateTmp = true;
        #PrivateDevices = true; # Needs CUDA
        PrivateMounts = true;
        ProtectClock = true;
        ProtectControlGroups = true;
        ProtectHome = true;
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

    networking.firewall.allowedTCPPorts = mkIf cfg.openFirewall [cfg.port];
  };
}
