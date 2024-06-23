{
  perSystem = {
    config,
    pkgs,
    lib,
    ...
  }: {
    overlayAttrs = {
      inherit (config.packages) realtime-stt-server;
    };

    packages.realtime-stt = pkgs.python3Packages.callPackage ./realtime-stt.nix {};
    packages.realtime-stt-server = pkgs.stdenv.mkDerivation {
      pname = "realtime-stt-server";
      inherit (config.packages.whisper-overlay) version;

      dontUnpack = true;
      propagatedBuildInputs = [
        (pkgs.python3.withPackages (_: [config.packages.realtime-stt]))
      ];

      installPhase = ''
        install -Dm755 ${../../realtime-stt-server.py} $out/bin/realtime-stt-server
      '';

      meta = {
        description = "A server for RealtimeSTT made to be used with whisper-overlay";
        homepage = "https://github.com/oddlama/whisper-overlay";
        license = lib.licenses.mit;
        maintainers = with lib.maintainers; [oddlama];
        mainProgram = "realtime-stt-server";
      };
    };

    devshells.default = {
      packages = [
        (pkgs.python3.withPackages (_: [config.packages.realtime-stt]))
      ];
    };
  };
}
