{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-23.05";
    utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
    naersk.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    rust-overlay.inputs.flake-utils.follows = "utils";
    cross-naersk.url = "github:icewind1991/cross-naersk";
    cross-naersk.inputs.nixpkgs.follows = "nixpkgs";
    cross-naersk.inputs.naersk.follows = "naersk";
  };

  outputs = {
    self,
    nixpkgs,
    utils,
    naersk,
    rust-overlay,
    cross-naersk,
  }:
    utils.lib.eachDefaultSystem (system: let
      overlays = [(import rust-overlay)];
      pkgs = import nixpkgs {
        inherit system overlays;
      };
      lib = pkgs.lib;

      hostTarget = pkgs.hostPlatform.config;
      targets = [
        "x86_64-unknown-linux-musl"
        "i686-unknown-linux-musl"
        "armv7-unknown-linux-musleabihf"
        "aarch64-unknown-linux-musl"
        "x86_64-unknown-freebsd"
      ];

      artifactForTarget = target: "notify_push";
      assetNameForTarget = target: "notify_push-${target}";

      cross-naersk' = pkgs.callPackage cross-naersk {
        inherit naersk;
        toolchain = pkgs.rust-bin.beta.latest.default; # required for 32bit musl targets since nix uses musl 1.2
      };

      src = lib.sources.sourceByRegex (lib.cleanSource ./.) ["Cargo.*" "(src|tests|test_client|build.rs|appinfo)(/.*)?"];

      nearskOpt = {
        pname = "notify_push";
        inherit src;
      };
      buildTarget = target: (cross-naersk'.buildPackage target) nearskOpt;
      hostNaersk = cross-naersk'.hostNaersk;

      msrv = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.rust-version;
      naerskMsrv = let
        toolchain = pkgs.rust-bin.stable."${msrv}".default;
      in
        pkgs.callPackage naersk {
          cargo = toolchain;
          rustc = toolchain;
        };
    in rec {
      # `nix build`
      packages =
        (nixpkgs.lib.attrsets.genAttrs targets buildTarget)
        // rec {
          notify_push = hostNaersk.buildPackage nearskOpt;
          check = hostNaersk.buildPackage (nearskOpt
            // {
              mode = "check";
            });
          clippy = hostNaersk.buildPackage (nearskOpt
            // {
              mode = "clippy";
            });
          test = hostNaersk.buildPackage (nearskOpt
            // {
              mode = "test";
            });
          checkMsrv = naerskMsrv.buildPackage (nearskOpt
            // {
              mode = "check";
            });
          test_client = (cross-naersk'.buildPackage "x86_64-unknown-linux-musl") (nearskOpt
            // {
              cargoBuildOptions = x: x ++ ["-p" "test_client"];
            });
          default = notify_push;
        };

      inherit targets;

      devShells.default = cross-naersk'.mkShell targets {
        nativeBuildInputs = with pkgs; [
          (rust-bin.beta.latest.default.override {targets = targets ++ [hostTarget];})
          krankerl
        ];
      };
    });
}
