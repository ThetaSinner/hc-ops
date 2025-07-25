{
  description = "Flake for Holochain app development";

  inputs = {
    holonix = {
        url = "github:holochain/holonix?ref=main-0.5";
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

          meta = {
            description = "Holochain operations CLI tool";
            homepage = "https://github.com/ThetaSinner/hc-ops";
            mainProgram = "hc-ops";
          };
        };

      devShells.default = pkgs.mkShell {
        packages = [
            pkgs.llvmPackages_18.libunwind
        ] ++ (with inputs'.holonix.packages; [
          holochain
          hc
          lair-keystore
          hc-launch
          hc-scaffold
          hn-introspect
        ]) ++ (with pkgs; [
          nodejs_20 # For UI development
          binaryen # For WASM optimisation
          # Add any other packages you need here
        ]);

        shellHook = ''
          export PS1='\[\033[1;34m\][holonix:\w]\$\[\033[0m\] '

          export LIBCLANG_PATH="${pkgs.llvmPackages_18.libclang.lib}/lib"
        '';
      };
    };
  };
}
