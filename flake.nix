{
  description = "Flake for Holochain app development";

  inputs = {
    holonix = {
        url = "github:holochain/holonix?ref=main-0.4";
        inputs.holochain.url = "github:holochain/holochain?ref=fix-storage-info-for-empty-db";
    };

    nixpkgs.follows = "holonix/nixpkgs";
    flake-parts.follows = "holonix/flake-parts";

    crane.url = "github:ipetkov/crane";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs@{ nixpkgs, flake-parts, crane, rust-overlay, ... }: flake-parts.lib.mkFlake { inherit inputs; } {
    systems = builtins.attrNames inputs.holonix.devShells;
    perSystem = { inputs', pkgs, system, ... }: {
      # Override the per system packages to include the rust overlay
      _module.args.pkgs = import nixpkgs { inherit system; overlays = [ (import rust-overlay) ]; };

      formatter = pkgs.nixpkgs-fmt;

      packages.hc-ops =
        let
          rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          craneLib = (crane.mkLib pkgs).overrideToolchain rust;

          nonCargoBuildFiles = path: _type: builtins.match ".*\.sql$" path != null;
          includeFilesFilter = path: type:
            (craneLib.filterCargoSources path type) || (nonCargoBuildFiles path type);
        in
        craneLib.buildPackage {
          pname = "hc-ops";
          cargoExtraArgs = "--features discover";
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = includeFilesFilter;
          };
          nativeBuildInputs = [
            pkgs.perl
          ];
          doCheck = false;
        };

      devShells.default = pkgs.mkShell {
        packages = (with inputs'.holonix.packages; [
          holochain
          lair-keystore
          hc-launch
          hc-scaffold
          hn-introspect
          rust # For Rust development, with the WASM target included for zome builds
        ]) ++ (with pkgs; [
          nodejs_20 # For UI development
          binaryen # For WASM optimisation
          # Add any other packages you need here
        ]);

        shellHook = ''
          export PS1='\[\033[1;34m\][holonix:\w]\$\[\033[0m\] '
        '';
      };
    };
  };
}
