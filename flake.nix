{
  description = "windows interactions";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-25.05";
  };

  outputs =
    { self, nixpkgs }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
      pkgsFor = nixpkgs.legacyPackages;
    in
    {
      packages = forAllSystems (system: {
        default = pkgsFor.${system}.callPackage ./. { };
      });

devShells = forAllSystems (system: {
  default = pkgsFor.${system}.mkShell {
    packages = with pkgsFor.${system}; [
      # Rust tooling
      rustc cargo rust-analyzer rustfmt clippy

      # Build tooling for -sys crates
      pkg-config
      gcc

      # GTK / GLib stack (dev outputs matter)
      glib.dev
      gtk3.dev
      gdk-pixbuf.dev
      cairo.dev
      pango.dev
      atk.dev

      # Often required transitively
      zlib.dev
    ];
  };
});
    
    };
}
