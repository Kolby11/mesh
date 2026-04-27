{
  description = "Development environment for the MESH shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
        };

        runtimeLibs = with pkgs; [
          libxkbcommon
          wayland
          wayland-protocols
          glib
          gdk-pixbuf
          gtk3
          gtk-layer-shell
          webkitgtk_4_1
          libsoup_3
          glib-networking
          cairo
          pango
          atk
          harfbuzz
          librsvg
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            rustfmt
            clippy
            pkg-config
            nodejs
            pnpm
          ] ++ runtimeLibs;

          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;

          shellHook = ''
            echo "MESH dev shell ready"
            echo "Run inside this shell so Tauri/WebKitGTK deps are available"
            echo "Example: cargo run -p mesh-cli -- start"
          '';
        };
      });
}
