# Changelog

All notable changes are listed here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Releases use date versions: `YYYY.MM.DD`, tagged `vYYYY.MM.DD`.

## [Unreleased]

### Changed

- Rewrote Shotnote from Python to Rust with gtk4-rs and libadwaita. Every tool, key, menu item and setting of the Python version is kept.
- The image is drawn as a GPU texture. Zoom and pan no longer repaint the image on the CPU.
- Save never overwrites an original. Ctrl+S writes a copy named `<name> (2).png` next to it, and later saves in the same window update that copy.
- Packaging moved from `install.sh` to a Nix flake. The Python version stays in `legacy/`.

### Added

- Image viewer: next and previous image in the folder, rotate, flip, fullscreen, file details and move to trash.
- Capture on Hyprland and other wlroots desktops through `grim` and `slurp`. The same path is the fallback when the portal fails.
- `SHOTNOTE_TIMING=1` prints the time to the first painted frame and exits.
- Desktop entry for png, jpeg, webp, gif, bmp and tiff.
