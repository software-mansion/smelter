{
  description = "Dev shell for Smelter";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin" ];
      perSystem = { config, self', inputs', pkgs, system, lib, ... }:
        let
          packageWithoutChromium = (pkgs.callPackage ./package.nix { });

          # https://github.com/NixOS/nixpkgs/blob/master/pkgs/development/libraries/libcef/default.nix#L33
          libcefDependencies = with pkgs;  [
            glib
            nss
            nspr
            atk
            at-spi2-atk
            expat
            xorg.libxcb
            libxkbcommon
            xorg.libX11
            xorg.libXcomposite
            xorg.libXdamage
            xorg.libXext
            xorg.libXfixes
            xorg.libXrandr
            mesa
            gtk3
            pango
            cairo
            dbus
            at-spi2-core
            cups
            xorg.libxshmfence
          ] ++ (
            pkgs.lib.optionals pkgs.stdenv.isLinux [
              libdrm
              libgbm
              udev
              alsa-lib
            ]
          );
          ffmpeg = pkgs.ffmpeg_7-full.override {
            withRtmp = false;
          };
          devDependencies = with pkgs; [
            ffmpeg

            gst_all_1.gstreamer
            gst_all_1.gst-plugins-base
            gst_all_1.gst-plugins-good
            gst_all_1.gst-plugins-bad
            gst_all_1.gst-plugins-ugly
            gst_all_1.gst-libav

            # Node.js
            nodejs_20
            pnpm
            yarn

            # Rust
            rustfmt
            clippy
            cargo-watch
            cargo-nextest
            rust-analyzer

            clang-tools
            llvmPackages.bintools
          ];
        in
        {
          devShells = {
            default = if pkgs.stdenv.isLinux then self'.devShells.linux else self'.devShells.macos;
            linux = pkgs.mkShell {
              packages = devDependencies ++ [ pkgs.mesa pkgs.blackmagic-desktop-video ];

              # Fixes "ffplay" used in examples on Linux (not needed on NixOS)
              env.LIBGL_DRIVERS_PATH = "${pkgs.mesa}/lib/dri";

              env.LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
              env.LD_LIBRARY_PATH = lib.makeLibraryPath (libcefDependencies ++ [ pkgs.libGL pkgs.blackmagic-desktop-video ]);

              inputsFrom = [ packageWithoutChromium ];
            };
            macos = pkgs.mkShell {
              packages = devDependencies;
              inputsFrom = [ packageWithoutChromium ];
            };
            nixos = pkgs.mkShell {
              packages = devDependencies ++ [ pkgs.mesa pkgs.blackmagic-desktop-video ];

              env.LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
              env.LD_LIBRARY_PATH = lib.makeLibraryPath (libcefDependencies ++ [ pkgs.libGL pkgs.blackmagic-desktop-video ]);

              inputsFrom = [ packageWithoutChromium ];
            };
          };
          packages = {
            default = packageWithoutChromium;
          };
        };
    };
}
