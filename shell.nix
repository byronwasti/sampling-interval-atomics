{ pkgs ? import <nixpkgs> {} }:
with pkgs;
mkShell rec {
    buildInputs = with pkgs; [
        cmake pkg-config freetype expat fontconfig
    ];
}
