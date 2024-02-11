let
  // TODO: Cross-compile
  pkgs =
    import (
      builtins.fetchTarball {
        name = "nixos-23.11";
        url = "https://github.com/nixos/nixpkgs/archive/04220ed6763637e5899980f98d5c8424b1079353.tar.gz";
      }
    ) {};
in
pkgs.rustPlatform.buildRustPackage rec {
  pname = "rust-cycle";
  version = "0.2.0";
  cargoLock = {
    lockFile = ./Cargo.lock;
    outputHashes = {
     "btleplug-0.4.0" = "sha256-L9xlCgT/AkSpy+ilZeoHzxiMbm6zfiR0UON9yLc4Xbk=";
     "nmea0183-0.2.2" = "sha256-d0LnICwpsN6RaTDRkInicitQhTuRAmf4HKSllCyt7F4=";
    };
  };
  src = pkgs.lib.cleanSource ./.;
}
