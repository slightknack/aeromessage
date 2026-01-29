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

        buildScript = pkgs.writeShellScriptBin "build-aeromessage" ''
          set -e

          # Find oxidized directory: argument, current dir, or parent dir
          if [ -n "$1" ] && [ -d "$1/oxidized" ]; then
            cd "$1/oxidized"
          elif [ -d "./oxidized" ]; then
            cd "./oxidized"
          elif [ -d "../oxidized" ]; then
            cd "../oxidized"
          elif [ -f "./Cargo.toml" ] && grep -q "aeromessage" ./Cargo.toml 2>/dev/null; then
            : # already in oxidized
          else
            echo "Error: Could not find aeromessage/oxidized directory"
            echo "Run from the aeromessage repo root, or pass the path as an argument"
            exit 1
          fi

          # Auto-bump version based on git commit count and short hash
          COMMIT_COUNT=$(${pkgs.git}/bin/git rev-list --count HEAD 2>/dev/null || echo "0")
          COMMIT_HASH=$(${pkgs.git}/bin/git rev-parse --short HEAD 2>/dev/null || echo "unknown")
          VERSION="0.1.''${COMMIT_COUNT}"
          FULL_VERSION="''${VERSION}+''${COMMIT_HASH}"
          echo "Setting version to ''${FULL_VERSION}..."

          # Update tauri.conf.json
          ${pkgs.gnused}/bin/sed -i "s/\"version\": \"[^\"]*\"/\"version\": \"''${VERSION}\"/" tauri.conf.json

          # Update Cargo.toml
          ${pkgs.gnused}/bin/sed -i "s/^version = \"[^\"]*\"/version = \"''${VERSION}\"/" Cargo.toml

          # Write full version for frontend
          echo "''${FULL_VERSION}" > frontend/version.txt

          echo "Cleaning previous build..."
          rm -rf target/release/bundle

          echo "Building Aeromessage..."
          ${pkgs.cargo-tauri}/bin/cargo-tauri build

          APP_PATH="target/release/bundle/macos/Aeromessage.app"
          OUT_DIR="out"
          DMG_PATH="$OUT_DIR/Aeromessage.dmg"

          if [ ! -d "$APP_PATH" ]; then
              echo "Error: Build failed - app not found at $APP_PATH"
              exit 1
          fi

          echo "Build successful: $APP_PATH"

          mkdir -p "$OUT_DIR"

          echo "Creating DMG..."
          rm -f "$DMG_PATH"
          ${pkgs.create-dmg}/bin/create-dmg \
              --volname "Aeromessage" \
              --window-pos 200 120 \
              --window-size 600 400 \
              --icon-size 100 \
              --icon "Aeromessage.app" 150 190 \
              --app-drop-link 450 190 \
              --no-internet-enable \
              --skip-jenkins \
              "$DMG_PATH" \
              "$APP_PATH"

          echo "DMG created: $DMG_PATH"
          echo "Done!"
        '';

      in {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            toolchain
            nodejs_20
            cargo-tauri
            create-dmg
            git
          ] ++ commonDeps ++ darwinDeps ++ linuxDeps;

          shellHook = ''
            export RUST_SRC_PATH="${toolchain}/lib/rustlib/src/rust/library"
          '' + pkgs.lib.optionalString (!isDarwin) ''
            export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath linuxDeps}:$LD_LIBRARY_PATH"
          '';
        };

        # nix run .#build
        apps.build = {
          type = "app";
          program = "${buildScript}/bin/build-aeromessage";
        };

        apps.default = self.apps.${system}.build;
      });
}
