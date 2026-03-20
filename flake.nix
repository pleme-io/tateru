{
  description = "Tateru — direct libkrun FFI control for macOS VMs";

  nixConfig = {
    allow-import-from-derivation = true;
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    crate2nix.url = "github:nix-community/crate2nix";
    flake-utils.url = "github:numtide/flake-utils";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.fenix.follows = "fenix";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crate2nix,
    flake-utils,
    fenix,
    substrate,
    ...
  }:
    flake-utils.lib.eachSystem ["aarch64-darwin"] (system: let
      pkgs = import nixpkgs {inherit system;};

      generatedCargoNix = crate2nix.tools.${system}.appliedCargoNix {
        name = "tateru";
        src = ./.;
      };

      tateruOverrides = pkgs.defaultCrateOverrides // {
        tateru = attrs: {
          nativeBuildInputs = (attrs.nativeBuildInputs or []) ++ [
            pkgs.darwin.sigtool
          ];
          buildInputs = (attrs.buildInputs or []) ++ [
            pkgs.libkrun-efi
            pkgs.libiconv
          ];
          LIBKRUN_EFI = "${pkgs.libkrun-efi}/lib";
        };
      };

      project = generatedCargoNix.rootCrate.build.override {
        defaultCrateOverrides = tateruOverrides;
      };
    in {
      packages.default = project;

      devShells.default = pkgs.mkShellNoCC {
        packages = with pkgs; [
          cargo
          rustc
          clippy
          rust-analyzer
          libkrun-efi
        ];
        LIBKRUN_EFI = "${pkgs.libkrun-efi}/lib";
      };
    }) // {
      overlays.default = final: prev: {
        tateru = self.packages.${final.system}.default;
      };
    };
}
