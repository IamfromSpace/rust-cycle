let
  pkgs =
    import ./pinned.nix;

  mkPackage =
    import ./derivation.nix;

in
  pkgs.pkgsCross.muslpi.pkgsStatic.callPackage
    (mkPackage { useSimulator = false; })
    {}
