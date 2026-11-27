{
  description = "Shotnote: screenshot annotator and image viewer";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      lib = pkgs.lib;
      runtimeTools = with pkgs; [ wl-clipboard grim slurp ];
    in {
      packages.${system}.default = pkgs.rustPlatform.buildRustPackage {
        pname = "shotnote";
        version = "0.1.0";
        src = lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            let b = baseNameOf path; in
            !(builtins.elem b [ "legacy" "target" "result" ".git" "backup" ]);
        };
        cargoLock.lockFile = ./Cargo.lock;
        nativeBuildInputs = with pkgs; [ pkg-config wrapGAppsHook4 ];
        buildInputs = with pkgs; [
          gtk4 libadwaita cairo pango glib gdk-pixbuf webp-pixbuf-loader
        ];
        gappsWrapperArgs = [ "--prefix" "PATH" ":" (lib.makeBinPath runtimeTools) ];
        postInstall = ''
          install -Dm644 data/dev.local.Shotnote.desktop \
            $out/share/applications/dev.local.Shotnote.desktop
        '';
        meta = {
          description = "Screenshot annotator and image viewer";
          license = lib.licenses.mit;
          mainProgram = "shotnote";
        };
      };

      apps.${system}.default = {
        type = "app";
        program = "${self.packages.${system}.default}/bin/shotnote";
      };

      devShells.${system}.default = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [ pkg-config wrapGAppsHook4 ];
        buildInputs = with pkgs; [
          rustc cargo clippy rustfmt gtk4 libadwaita cairo pango glib
          gdk-pixbuf webp-pixbuf-loader adwaita-icon-theme
        ] ++ runtimeTools;
      };
    };
}
