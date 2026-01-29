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

        # New Darwin SDK pattern - just include the SDK, frameworks are auto-available
        darwinDeps = pkgs.lib.optionals isDarwin [
          pkgs.apple-sdk_15
          pkgs.libiconv
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
      });
}
