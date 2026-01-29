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
            Configuration written to {file}`$XDG_CONFIG_DIR/otter-launcher/config.toml`.

            See <https://github.com/kuokuo123/otter-launcher/blob/main/README.md#configuration>.
          '';
        };

        settingsExtra = lib.mkOption {
          type = lib.types.lines;
          default = "";
          defaultText = lib.literalExpression ''""'';
          description = ''
            Raw additional configuration lines written to {file}`$XDG_CONFIG_DIR/otter-launcher/config.toml`.

            See <https://github.com/kuokuo123/otter-launcher/blob/main/README.md#configuration>.
          '';
          example = # toml
            ''
            [interface]
            # use three quotes to write longer codes
            header = """
              \u001B[34;1m$USER@$(printf $HOSTNAME)\u001B[0m     \u001B[31m\u001B[0m $(mpstat | awk 'FNR ==4 {print $4}')
              """
            header_cmd = ""
            header_cmd_trimmed_lines = 0
            place_holder = "otterly awesome"
            suggestion_mode = "list"
            separator = "                  \u001B[90mmodules ────────────────"
            footer = ""
            suggestion_lines = 3
            list_prefix = "  "
            selection_prefix = "\u001B[31;1m▌ "
            prefix_padding = 3
            default_module_message = "  \u001B[33msearch\u001B[0m the internet"
            empty_module_message = ""
            customized_list_order = false
            indicator_with_arg_module = ""
            indicator_no_arg_module = ""
            prefix_color = "\u001B[33m"
            description_color = "\u001B[39m"
            place_holder_color = "\u001B[30m"
            hint_color = "\u001B[30m"
            move_interface_right = 16
            move_interface_down = 1
            '';
        };
      };

      config = lib.mkIf cfg.enable {
        home.packages = [ otter-launcher ];

        xdg.configFile."otter-launcher/config.toml" = lib.mkIf (cfg.settings != { } || cfg.settingsExtra != "") {
          source =
            let
              write-settings = lib.optionalString (cfg.settings != { }) ''
                cat "${toml-format.generate "otter-launcher-settings" cfg.settings}" >> $out
                echo "" >> $out
              '';

              write-settings-extra = lib.optionalString (cfg.settingsExtra != "") ''
                cat "${pkgs.writeText "otter-launcher-settings-extra" cfg.settingsExtra}" >> $out
                echo "" >> $out
              '';

              # source = pkgs.runCommand "otter-launcher-config" { } (lib.concatStringsSep "\n" (config-file ++ config-extra));
              source = pkgs.runCommand "otter-launcher-config" { } ''
                ${write-settings}
                ${write-settings-extra}
              '';
            in source;
        };
      };
    };

  otter-launcher = moduleWithSystem otter-launcher-module;
in
{
  flake.homeModules = {
    inherit otter-launcher;
    default = otter-launcher;
  };
}
