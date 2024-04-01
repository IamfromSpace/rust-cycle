let
  pkgs = import ./pinned.nix;
in
  pkgs.mkShell {
    nativeBuildInputs =
      [ pkgs.cargo
        pkgs.cargo-audit
        pkgs.SDL2
        pkgs.pkg-config
        pkgs.dbus
      ];
  }
