{ rustPlatform
, ffmpeg_7-headless
, openssl
, pkg-config
, llvmPackages
, libGL
, cmake
, libopus
, lib
, vulkan-loader
, mesa
, darwin
, stdenv
, makeWrapper
}:
let
  ffmpeg = ffmpeg_7-headless.override {
    withRtmp = false;
  };
  buildInputs = [
    ffmpeg
    openssl
    libopus
    libGL
    vulkan-loader
    stdenv.cc.cc
  ] ++ lib.optionals stdenv.isDarwin [
    darwin.apple_sdk.frameworks.Metal
    darwin.apple_sdk.frameworks.Foundation
    darwin.apple_sdk.frameworks.QuartzCore
    darwin.libobjc
  ] ++ lib.optionals stdenv.isLinux [
    mesa
  ];
  rpath = lib.makeLibraryPath buildInputs;
in
rustPlatform.buildRustPackage {
  pname = "smelter";
  version = "0.4.0";
  src = ../..;
  cargoLock = {
    lockFile = ../../Cargo.lock;
    allowBuiltinFetchGit = true;
  };

  buildNoDefaultFeatures = true;
  doCheck = false;

  inherit buildInputs;
  nativeBuildInputs = [ pkg-config llvmPackages.clang cmake makeWrapper ];

  env.LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";

  postFixup =
    ''
      rm -f $out/bin/smelter
      rm -f $out/bin/package_for_release

      mv $out/bin/main_process $out/bin/smelter
    '' + (
      lib.optionalString stdenv.isLinux ''
        patchelf --set-rpath ${rpath} $out/bin/smelter
        wrapProgram $out/bin/smelter \
        --prefix XDG_DATA_DIRS : "${mesa}/share" \
        --prefix LD_LIBRARY_PATH : "${mesa}/lib"
      ''
    );
}

