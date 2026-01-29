{
  description = "Aeromessage - Batch-reply to iMessages, beautifully";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };

        toolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };

        isDarwin = pkgs.stdenv.isDarwin;

        darwinDeps = with pkgs; pkgs.lib.optionals isDarwin [
          darwin.apple_sdk.frameworks.Security
          darwin.apple_sdk.frameworks.CoreServices
          darwin.apple_sdk.frameworks.CoreFoundation
          darwin.apple_sdk.frameworks.AppKit
          darwin.apple_sdk.frameworks.WebKit
          darwin.libobjc
        ];

        linuxDeps = with pkgs; pkgs.lib.optionals (!isDarwin) [
          webkitgtk
          gtk3
          libsoup
          glib
          dbus
        ];

        commonDeps = with pkgs; [
          openssl
          pkg-config
        ];

      in {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            toolchain
            nodejs_20
            cargo-tauri
          ] ++ commonDeps ++ darwinDeps ++ linuxDeps;

          shellHook = ''
            export RUST_SRC_PATH="${toolchain}/lib/rustlib/src/rust/library"
          '' + pkgs.lib.optionalString (!isDarwin) ''
            export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath linuxDeps}:$LD_LIBRARY_PATH"
          '';
        };

        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "aeromessage";
          version = "0.1.14";
          src = ./oxidized;
          cargoLock.lockFile = ./oxidized/Cargo.lock;

          nativeBuildInputs = with pkgs; [
            pkg-config
            nodejs_20
          ] ++ pkgs.lib.optionals isDarwin [ pkgs.xcbuild ];

          buildInputs = commonDeps ++ darwinDeps ++ linuxDeps;

          # Skip Tauri bundling in Nix build - just build the binary
          buildPhase = ''
            cargo build --release
          '';

          installPhase = ''
            mkdir -p $out/bin
            cp target/release/aeromessage $out/bin/
          '';

          meta = with pkgs.lib; {
            description = "Batch-reply to iMessages, beautifully";
            homepage = "https://github.com/slightknack/aeromessage";
            license = licenses.cc0;
            platforms = platforms.darwin;  # macOS only (iMessage)
          };
        };
      });
}
