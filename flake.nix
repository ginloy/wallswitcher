{
  inputs = {
    naersk.url = "github:nix-community/naersk/master";
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    utils,
    naersk,
  }:
    utils.lib.eachDefaultSystem (
      system: let
        pkgs = import nixpkgs {inherit system;};
        naersk-lib = pkgs.callPackage naersk {};
      in {
        defaultPackage = naersk-lib.buildPackage ./.;
        devShell = with pkgs;
          mkShell {
            nativeBuildInputs = [
              libxkbcommon
              pkg-config
              cargo
              rustc
              rustfmt
              pre-commit
              rustPackages.clippy
              wgpu-utils
            ];
            buildInputs = [
              wayland
              libGL
              vulkan-loader
            ];
            RUST_SRC_PATH = rustPlatform.rustLibSrc;
            LD_LIBRARY_PATH = ''
              ${lib.makeLibraryPath [
                wayland
                vulkan-loader
                libGL
              ]}
            '';
          };
      }
    );
}
