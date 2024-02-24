{ useSimulator }:
  { rustPlatform, lib, pkg-config, dbus, SDL2 }:
    rustPlatform.buildRustPackage rec {
      pname = "rust-cycle";
      version = "0.2.0";

      # blteplug requires the crate libdbus-sys, which requires pkg-config+bdus
      # embedded-graphics-simulator requires SDL2
      # TODO: why doesn't buildRustPackage already know this?
      nativeBuildInputs = [ pkg-config ];
      buildInputs = [ dbus ] ++ (if useSimulator then [ SDL2 ] else []);
      buildFeatures = if useSimulator then [ "simulator" ] else [];
      cargoLock = {
        lockFile = ./Cargo.lock;
        outputHashes = {
         "nmea0183-0.2.2" = "sha256-d0LnICwpsN6RaTDRkInicitQhTuRAmf4HKSllCyt7F4=";
        };
      };
      src = lib.cleanSource ./.;
    }
