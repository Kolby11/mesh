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
          fontconfig
          freetype
          libxkbcommon
          wayland
          wayland-protocols
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
          ] ++ lib.optionals stdenv.isLinux [
            cargo-flamegraph
            heaptrack
            hotspot
            perf
            tracy
          ] ++ runtimeLibs;

          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibs;

          shellHook = ''
            echo "MESH dev shell ready"
            echo "Run: cargo run -p mesh-tools-cli --bin mesh-shell -- start"
            echo "Profile: ./tools/profile-shell live|cpu|memory"
          '';
        };
      });
}
