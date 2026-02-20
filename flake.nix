{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
      git-hooks,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [
            rust-overlay.overlays.default
            self.overlays.default
          ];
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default;

        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };

        pre-commit-check = git-hooks.lib.${system}.run {
          src = ./.;
          hooks = {
            check-merge-conflicts.enable = true;
            check-added-large-files.enable = true;
            detect-private-keys.enable = true;
            check-toml.enable = true;
            check-json.enable = true;
            typos.enable = true;
            nixfmt = {
              enable = true;
              package = pkgs.nixfmt-rfc-style;
            };
            deadnix.enable = true;
            statix.enable = true;
            rustfmt = {
              enable = true;
              entry = "${rustToolchain}/bin/cargo-fmt fmt --check";
            };
            clippy = {
              enable = true;
              entry = "${rustToolchain}/bin/cargo-clippy clippy -- -D warnings";
              types = [ "rust" ];
              pass_filenames = false;
            };
          };
        };
      in
      {
        packages = {
          grpc-proxier = rustPlatform.buildRustPackage {
            pname = "grpc-proxier";
            version = "0.1.0";
            src = ./.;
            cargoHash = "sha256-l42Gtrp1a68ZrXwztNu3FdlswUWMu2NSRYaj6V83K1o=";
          };
          default = self.packages.${system}.grpc-proxier;
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.cargo-watch
            pkgs.just
          ];

          shellHook = ''
            ${pre-commit-check.shellHook}
          '';
        };
      }
    )
    // {
      overlays.default = final: _prev: {
        inherit (self.packages.${final.system}) grpc-proxier;
      };

      nixosModules = {
        grpc-proxier = import ./nix/module.nix;
        monitoring = import ./nix/monitoring.nix;

        default =
          { ... }:
          {
            imports = [
              self.nixosModules.grpc-proxier
              self.nixosModules.monitoring
            ];
          };
      };
    };
}
