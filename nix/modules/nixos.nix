{ moduleWithSystem, ... }:
let
  otter-launcher-module =
    { self', ... }:
    { config, pkgs, lib, ... }:
    let
      otter-launcher = self'.packages.default;

      toml-format = pkgs.formats.toml { };
      toml = toml-format.type;

      cfg = config.programs.otter-launcher;
    in
    {
      options.programs.otter-launcher = {
        enable = lib.mkEnableOption "otter-launcher";

        settings = lib.mkOption {
          type = toml;
          default = { };
          defaultText = lib.literalExpression "{ }";
          description = ''
            Configuration written to {file}`/etc/otter-launcher/config.toml`.

            See <https://github.com/kuokuo123/otter-launcher/blob/main/README.md#configuration>.
          '';
        };
      };

      config = lib.mkIf cfg.enable {
        environment = {
          systemPackages = [ otter-launcher ];

          etc."otter-launcher/config.toml" = lib.mkIf (cfg.settings != { }) {
            source = toml-format.generate "otter-launcher-config" cfg.settings;
          };
        };
      };
    };

  otter-launcher = moduleWithSystem otter-launcher-module;
in
{
  flake.nixosModules = {
    inherit otter-launcher;
    default = otter-launcher;
  };
}
