{ self, ... }:
{
  perSystem =
    { pkgs, lib, ... }:
    let
      meta = {
        mainProgram = "otter-launcher";
        homepage = "https://github.com/kuokuo123/otter-launcher";
        description = "A hackable cli/tui launcher built for keyboard-centric wm users";
        license = lib.licenses.gpl3;
        platforms = lib.platforms.linux;
      };

      otter-launcher = pkgs.rustPlatform.buildRustPackage {
        inherit meta;
        pname = "otter-launcher";
        version = "git";

        src = lib.sources.cleanSource self;

        cargoLock.lockFile = "${self}/Cargo.lock";
        cargoDepsName = "otter-launcher-deps";
      };
    in
    {
      packages = {
        inherit otter-launcher;
        default = otter-launcher;
      };
    };
}
