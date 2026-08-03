{
  description = "Nix flake for Decapod: reproducible package build from the committed Cargo.lock, plus the optional development shell";

  inputs = {
    flake-utils.url = "github:numtide/flake-utils";
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      flake-utils,
      nixpkgs,
      rust-overlay,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

        # The package builds with the exact toolchain the repository pins
        # (rust-toolchain.toml -> .config/build/rust-toolchain.toml), so the
        # flake tracks the repo's MSRV instead of whatever rustc the pinned
        # nixpkgs happens to carry.
        buildToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        buildRustPlatform = pkgs.makeRustPlatform {
          cargo = buildToolchain;
          rustc = buildToolchain;
        };

        runtimeLibs = with pkgs; [
          sqlite
        ];
        toolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "clippy"
            "rustfmt"
          ];
        };
      in
      {
        packages = {
          decapod = buildRustPlatform.buildRustPackage {
            pname = cargoToml.package.name;
            inherit (cargoToml.package) version;

            src = self;

            # Cargo.lock is committed (and ships inside the published crate
            # tarball), so the vendored dependency closure derives from the
            # lockfile alone: no cargoHash to recompute on release, here or
            # for downstream packagers consuming this flake as an input.
            cargoLock.lockFile = ./Cargo.lock;

            # rusqlite builds its bundled SQLite (see Cargo.toml features),
            # so no external C libraries are needed beyond the stdenv
            # toolchain.

            # The test suite is exercised by the repository's primary CI
            # (Bazel + cargo). This derivation is the packaging proof:
            # compiling and linking the release binary is the gate.
            doCheck = false;

            meta = {
              inherit (cargoToml.package) description homepage;
              license = pkgs.lib.licenses.mit;
              mainProgram = "decapod";
            };
          };

          default = self.packages.${system}.decapod;
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            clang
            git
            lld
            nixfmt-rfc-style
            openssh
            pkg-config
            sqlite
            toolchain
          ];

          shellHook = ''
            export CARGO_TERM_COLOR=always
            export CARGO_INCREMENTAL=0
            export CARGO_NET_RETRY=10
            export CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
            export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath runtimeLibs}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
            if [ "$(uname -s)" = "Linux" ]; then
              export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang
              export RUSTFLAGS="-C link-arg=-fuse-ld=lld''${RUSTFLAGS:+ $RUSTFLAGS}"
            fi
          '';
        };

        devShells.ci = self.devShells.${system}.default;
      }
    );
}
