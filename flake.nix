{
  description = "iidy - CloudFormation deployment tool (Rust rewrite)";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Native deps for openssl-sys (reqwest -> native-tls)
            openssl
            pkg-config

            # Fast linker -- GNU ld uses ~1 GB per instance and OOMs on 24-core
            mold

            # Test runner (Makefile uses cargo-nextest)
            cargo-nextest
          ];

          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";

          # Ensure test binaries (run as subprocesses by nextest) can find libssl
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [ pkgs.openssl ];
        };
      });
}
