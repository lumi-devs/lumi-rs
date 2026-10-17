{
  description = "Lumi (Rust) — scalable Discord bot: sharded gateway, worker tier, RPC/dashboard API";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default";
  };

  outputs = { self, nixpkgs, systems }:
    let
      forEachSystem = f: nixpkgs.lib.genAttrs (import systems) (system: f nixpkgs.legacyPackages.${system});
    in
    {
      devShells = forEachSystem (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
            pkg-config
            openssl
            sea-orm-cli
            postgresql
            redis
            jq
            curl
            git
            gh
          ];

          env = {
            RUST_BACKTRACE = "1";
            RUST_LOG = "info,lumi=debug";
          };

          shellHook = ''
            echo "========================================="
            echo "  Lumi (Rust) dev shell"
            echo "  rustc $(rustc --version | cut -d' ' -f2) · cargo $(cargo --version | cut -d' ' -f2)"
            echo "  psql $(psql --version | cut -d' ' -f3) · redis $(redis-server --version | awk '{print $3}' | cut -d= -f2)"
            echo "========================================="
          '';
        };
      });

      packages = forEachSystem (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "lumi";
          version = "0.1.0";
          src = ./.;
          useFetchCargoVendor = true;
          cargoHash = "";
          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs = with pkgs; [ openssl ];
          meta.description = "Lumi Discord bot (Rust)";
        };
      });
    };
}
