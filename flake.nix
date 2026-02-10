{
  description = "Dotdipper - A safe, deterministic, and feature-rich dotfiles manager";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default;

        dotdipper = pkgs.rustPlatform.buildRustPackage {
          pname = "dotdipper";
          version = "0.3.1";

          src = self;

          cargoLock = {
            lockFile = self + "/Cargo.lock";
          };

          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.openssl ];

          # age is a runtime dependency for secrets encryption
          postInstall = ''
            wrapProgram $out/bin/dotdipper \
              --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.age ]}
          '';

          meta = with pkgs.lib; {
            description = "A safe, deterministic, and feature-rich dotfiles manager built in Rust";
            homepage = "https://github.com/psyysp/dotdipper";
            license = licenses.mit;
            mainProgram = "dotdipper";
            platforms = platforms.unix;
          };
        };
      in
      {
        packages = {
          default = dotdipper;
          dotdipper = dotdipper;
        };

        apps.default = flake-utils.lib.mkApp {
          drv = dotdipper;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.pkg-config
            pkgs.openssl
            pkgs.age
          ];
        };
      }
    );
}
