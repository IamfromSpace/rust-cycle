let
  pkgs =
    import ./pinned.nix;

  mkPackage =
    import ./derivation.nix;

in
  pkgs.callPackage
    (mkPackage { useSimulator = true; })
    {}
