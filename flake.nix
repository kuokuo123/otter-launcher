{
  description = "A hackable cli/tui launcher built for keyboard-centric wm users";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default-linux";
    flake-parts.url = "github:hercules-ci/flake-parts";
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs:
    let
      args = {
        inherit inputs;
      };

      module = {
        systems = import inputs.systems;

        imports = [
          inputs.home-manager.flakeModules.default
          ./nix
        ];
      };
    in
    inputs.flake-parts.lib.mkFlake args module;
}
