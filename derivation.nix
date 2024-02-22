{ useSimulator }:
  { rustPlatform, lib, SDL2 }:
    rustPlatform.buildRustPackage rec {
      pname = "rust-cycle";
      version = "0.2.0";
      # embedded-graphics-simulator requires SDL2
      # TODO: why doesn't buildRustPackage already know this?
      buildInputs = if useSimulator then [ SDL2 ] else [];
      buildFeatures = if useSimulator then [ "simulator" ] else [];
      cargoLock = {
        lockFile = ./Cargo.lock;
        outputHashes = {
         "btleplug-0.4.0" = "sha256-L9xlCgT/AkSpy+ilZeoHzxiMbm6zfiR0UON9yLc4Xbk=";
         "nmea0183-0.2.2" = "sha256-d0LnICwpsN6RaTDRkInicitQhTuRAmf4HKSllCyt7F4=";
        };
      };
      src = lib.cleanSource ./.;
    }
