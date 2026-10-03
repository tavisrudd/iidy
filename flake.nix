{
  description = "iidy - CloudFormation deployment tool (Rust rewrite)";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-26.05";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Fast linker -- GNU ld uses ~1 GB per instance and OOMs on 24-core
            mold

            # Test runner and coverage (Makefile targets)
            cargo-nextest
            cargo-tarpaulin
          ];
        };
      });
}
