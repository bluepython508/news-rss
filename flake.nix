{
  description = "RTE rss reader";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs";
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, crane }: let
    inherit (nixpkgs) lib;
    
    forEachSystem = lib.genAttrs [
      "aarch64-linux"
      "aarch64-darwin"
      "x86_64-darwin"
      "x86_64-linux"
    ];
  in rec {
    packages = forEachSystem (system: let
      craneLib = crane.lib.${system};
    in {
      default = craneLib.buildPackage {
        src = craneLib.cleanCargoSource (craneLib.path ./.);
      };
    });

    devShells = forEachSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      default = pkgs.mkShell {
        inputsFrom = [ packages.${system}.default ];
        nativeBuildInputs = with pkgs; [ cargo rustc rust-analyzer ];
      };
    });

    nixosModules.news-rss = { lib, pkgs, config, ... }: with lib; let
      cfg = config.services.bluepython508.news-rss;
    in {
      options = {
        services.bluepython508.news-rss = {
          enable = mkEnableOption "news-rss feed";
          address = mkOption {
            type = types.str;
            default = "0.0.0.0:2048";
          };
        };
      };

      config.systemd.services.news-rss = mkIf cfg.enable {
          description = "RTE rss feed";          
          script = "${packages.${pkgs.system}.default}/bin/news-rss";
          scriptArgs = cfg.address;
          wantedBy = [ "multi-user.target" ];
      };
    };
  };
}
