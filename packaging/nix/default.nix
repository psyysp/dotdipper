{ lib
, rustPlatform
, fetchFromGitHub
, pkg-config
, openssl
, age
}:

rustPlatform.buildRustPackage rec {
  pname = "dotdipper";
  version = "0.3.1";

  src = fetchFromGitHub {
    owner = "psyysp";
    repo = "dotdipper";
    rev = "v${version}";
    hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="; # Update with: nix-prefetch-github psyysp dotdipper --rev v0.3.1
  };

  cargoHash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="; # Update after first build attempt

  nativeBuildInputs = [ pkg-config ];
  buildInputs = [ openssl ];

  # age is a runtime dependency for secrets encryption
  postInstall = ''
    wrapProgram $out/bin/dotdipper \
      --prefix PATH : ${lib.makeBinPath [ age ]}
  '';

  meta = with lib; {
    description = "A safe, deterministic, and feature-rich dotfiles manager built in Rust";
    homepage = "https://github.com/psyysp/dotdipper";
    license = licenses.mit;
    maintainers = [ ];
    mainProgram = "dotdipper";
    platforms = platforms.unix;
  };
}
