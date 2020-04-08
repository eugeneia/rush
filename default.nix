# Run like this:
#   nix-build /path/to/this/directory
# ... build products will be in ./result

{ source ? ./., version ? "dev" }:

let
  moz_overlay = import (builtins.fetchTarball https://github.com/mozilla/nixpkgs-mozilla/archive/master.tar.gz);
  nixpkgs = import <nixpkgs> { overlays = [ moz_overlay ]; };
in
  with nixpkgs;
  stdenv.mkDerivation {
    name = "rush-${version}";
    #src = lib.cleanSource (lib.sourceByRegex source ["target/*"]);

    buildInputs = [
      # to use a specific nighly:
      (nixpkgs.rustChannelOf { date = "2020-04-08"; channel = "nightly"; }).rust
    ];

  inherit version;

  # Set Environment Variables
  RUST_TEST_THREADS = 1;
  RUST_BACKTRACE = 1;

}
