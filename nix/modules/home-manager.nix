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
            padding = [0, 1, 0, 1]
            header = """
              \u001B[34;1m$USER@$(printf $HOSTNAME)\u001B[0m"""
            header_color = "blue"
            prompt_sign = "> "
            prompt_sign_color = "blue"
            placeholder = "type & search"
            suggestion_mode = "list"
            suggestion_position = "bottom"
            prefix_color = "yellow"
            prefix_padding = 3
            description_color = "reset"
            selection_prefix = "▌ "
            selection_prefix_color = "red"
            inverse_selection = true
            list_prefix = "  "
            list_prefix_color = "reset"
            indicator_args = ""
            indicator_bare = ""
            indicator_color = "red"
            scrollbar = true
            separator = true
            separator_label = "  otter "
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
