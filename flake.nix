{
  description = "jj-spr: Jujutsu subcommand for submitting pull requests to GitHub";

  inputs = {
    flake-parts.url = "github:hercules-ci/flake-parts";
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };

    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{ self, flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.git-hooks.flakeModule
        inputs.treefmt-nix.flakeModule
      ];
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
        "x86_64-darwin"
      ];
      perSystem =
        {
          config,
          pkgs,
          system,
          ...
        }:
        let
          rustToolchain = pkgs.fenix.stable;
        in
        {
          _module.args.pkgs = import inputs.nixpkgs {
            inherit system;
            overlays = [ inputs.fenix.overlays.default ];
            config = { };
          };

          formatter = config.treefmt.build.wrapper;
          checks.formatting = config.treefmt.build.check self;

          packages.default =
            let
              cargoToml = builtins.fromTOML (builtins.readFile ./spr/Cargo.toml);
            in
            pkgs.rustPlatform.buildRustPackage {
              pname = cargoToml.package.name;
              version = cargoToml.package.version;

              src = ./.;

              cargoLock = {
                lockFile = ./Cargo.lock;
              };

              buildInputs = with pkgs; [
                openssl
                zlib
              ];

              nativeBuildInputs = with pkgs; [
                pkg-config
                git
                jujutsu
              ];

              meta = with pkgs.lib; {
                description = "Jujutsu subcommand for submitting pull requests for individual, amendable, rebaseable commits to GitHub";
                homepage = "https://github.com/LucioFranco/spr";
                license = licenses.mit;
                maintainers = [ ];
              };
            };

          pre-commit = {
            check.enable = true;
            settings.hooks = {
              actionlint.enable = true;
              shellcheck.enable = true;
              treefmt.enable = true;
            };
          };

          treefmt = {
            settings = {
              rustfmt.enable = true;
            };
            projectRootFile = ".git/config";
            flakeCheck = false; # Covered by git-hooks check
          };

          devShells.default = pkgs.mkShell {
            packages = with pkgs; [
              # Rust toolchain
              (rustToolchain.withComponents [
                "cargo"
                "clippy"
                "rust-src"
                "rustc"
                "rustfmt"
                "rust-analyzer"
              ])

              # Build dependencies
              openssl
              pkg-config
              zlib

              # Required runtime dependencies for development and testing
              git
              jujutsu
            ];

            # Environment variables for development
            RUST_SRC_PATH = "${rustToolchain.rust-src}/lib/rustlib/src/rust/library";
            PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig:${pkgs.zlib.dev}/lib/pkgconfig";
          };
        };
    };
}
