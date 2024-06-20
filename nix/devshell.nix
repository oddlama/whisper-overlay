{inputs, ...}: {
  imports = [
    inputs.devshell.flakeModule
    inputs.pre-commit-hooks.flakeModule
  ];

  perSystem = {
    config,
    pkgs,
    ...
  }: {
    pre-commit.settings.hooks = {
      alejandra.enable = true;
      deadnix.enable = true;
      statix.enable = true;
    };

    devshells.default = {
      commands = [
        {
          package = pkgs.alejandra;
          help = "Format nix code";
        }
        {
          package = pkgs.statix;
          help = "Lint nix code";
        }
        {
          package = pkgs.deadnix;
          help = "Find unused expressions in nix code";
        }
      ];

      devshell.startup.pre-commit.text = config.pre-commit.installationScript;
    };

    # `nix fmt`
    formatter = pkgs.alejandra;
  };
}
