{
  description = "Development environment for the Bevy animation graph editor.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      flake-utils,
      rust-overlay,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
          config.allowUnfree = true;
        };

        rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

        runtimeLibs = with pkgs; [
          alsa-lib
          dbus
          libxkbcommon
          udev
          vulkan-loader
          wayland
          xorg.libX11
          xorg.libXcursor
          xorg.libXi
          xorg.libXrandr
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            rustToolchain
            rust-analyzer
            rustfmt
            clippy
            cargo-edit
            cargo-watch
            bacon
            just

            clang
            lld
            pkg-config

            alsa-lib
            dbus
            jack2
            libjack2
            libxkbcommon
            udev
            vulkan-headers
            vulkan-loader
            vulkan-tools
            vulkan-validation-layers
            wayland
            xdotool
            xorg.libX11
            xorg.libXcursor
            xorg.libXi
            xorg.libXrandr
            zenity
          ];

          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;
        };
      }
    );
}
