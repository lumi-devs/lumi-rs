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
        default = pkgs.mkShell.override { stdenv = pkgs.clangStdenv; } {
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
            clang
            mold
            gh
            cargo-nextest
            cargo-watch
          ];

          env = {
            RUST_BACKTRACE = "1";
            RUST_LOG = "info,lumi=debug,lumi_kernel=debug";
            # mold as linker — 3-5x faster incremental links
            CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER = "${pkgs.clang}/bin/clang";
            CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS = "-C link-arg=-fuse-ld=${pkgs.mold}/bin/mold";
            # speed up proc-macro + codegen
            CARGO_INCREMENTAL = "1";
            # parallelise codegen units in debug builds
            RUSTFLAGS = "-C codegen-units=16";
            # pkg-config
            PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
            OPENSSL_NO_VENDOR = "1";
          };

          shellHook = ''
            echo "========================================="
            echo "  Lumi (Rust) dev shell"
            echo "  rustc $(rustc --version | cut -d' ' -f2) · cargo $(cargo --version | cut -d' ' -f2)"
            echo "  linker: $(mold --version 2>/dev/null | head -1)"
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
          nativeBuildInputs = with pkgs; [ pkg-config clang mold ];
          buildInputs = with pkgs; [ openssl ];
          env = {
            CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER = "${pkgs.clang}/bin/clang";
            CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS = "-C link-arg=-fuse-ld=${pkgs.mold}/bin/mold";
            OPENSSL_NO_VENDOR = "1";
          };
          meta.description = "Lumi Discord bot (Rust)";
        };
      });
    };
}
