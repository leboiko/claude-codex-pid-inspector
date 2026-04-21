{
  description = "agentop — a TUI process inspector for Claude Code and OpenAI Codex CLI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };

        rustToolchain = pkgs.rust-bin.fromRustupToolchainFile
          ./rust-toolchain.toml;

        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

        agentop = pkgs.rustPlatform.buildRustPackage {
          pname = cargoToml.package.name;
          version = cargoToml.package.version;

          src = pkgs.lib.cleanSource ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = [ rustToolchain ];

          # Integration tests that probe ~/.claude or ~/.codex don't work in
          # the Nix sandbox (no home directory, no user-owned paths). They
          # already pass in CI; skip them here and rely on the unit tests.
          doCheck = true;
          cargoTestFlags = [ "--lib" "--bins" ];

          meta = with pkgs.lib; {
            description = cargoToml.package.description;
            homepage = "https://github.com/leboiko/claude-codex-pid-inspector";
            license = licenses.mit;
            maintainers = [ ];
            mainProgram = "agentop";
            platforms = platforms.unix;
          };
        };
      in {
        packages.default = agentop;
        packages.agentop = agentop;

        apps.default = {
          type = "app";
          program = "${agentop}/bin/agentop";
        };

        devShells.default = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.cargo-deny
            pkgs.cargo-audit
            pkgs.mdbook
          ];
        };
      });
}
