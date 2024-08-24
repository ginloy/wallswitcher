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
        defaultPackage = with pkgs;
          naersk-lib.buildPackage {
            src = ./.;
            nativeBuildInputs = [
              pkg-config
            ];
            buildInputs = [
              libxkbcommon
              wayland
              vulkan-loader
            ];
            postInstall = ''
              patchelf --shrink-rpath $out/bin/wayland_test
              patchelf --add-rpath ${lib.makeLibraryPath [
                vulkan-loader
              ]} $out/bin/wayland_test
            '';
            dontPatchELF = true;
          };
        devShell = with pkgs;
          mkShell {
            nativeBuildInputs = [
              pkg-config
              cargo
              rustc
              rustfmt
              pre-commit
              rustPackages.clippy
              wgpu-utils
            ];
            buildInputs = [
              libxkbcommon
              wayland
              libGL
              vulkan-loader
              libglvnd
            ];
            RUST_SRC_PATH = rustPlatform.rustLibSrc;
            LD_LIBRARY_PATH = ''
              ${lib.makeLibraryPath [
                vulkan-loader
              ]}:
            '';
          };
      }
    );
}
