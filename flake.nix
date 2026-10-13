{
  description = "File system utilities that work just as well for agents as they do for humans";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      forAllSystems = nixpkgs.lib.genAttrs [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
    in
    {
      packages = forAllSystems (system:
        let pkgs = nixpkgs.legacyPackages.${system}; in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "fs";
            version = "0.1.0";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            meta = {
              description = "File system utilities that work just as well for agents as they do for humans";
              license = pkgs.lib.licenses.mit;
              mainProgram = "fs";
            };
          };
        });
    };
}
