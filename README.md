# JPEG Viewer

[![Latest release](https://img.shields.io/github/v/release/MobinWeekend/Jpeg-Viewer?label=release)](https://github.com/MobinWeekend/Jpeg-Viewer/releases)
[![License: GPL-3.0-only](https://img.shields.io/badge/License-GPL--3.0--only-blue.svg)](LICENSE)

A GPU-accelerated image viewer written in Rust using `egui` and `wgpu`. Browse image collections, inspect large images, organize queues and favorites, run slideshows, and make basic edits.

![JPEG Viewer screenshot](https://github.com/user-attachments/assets/8dd722cc-7994-40a5-84d2-474e1ee332f6)

**New users:** See [Getting Started](QUICK_START.md). Press **F1** for in-app controls.

## Features

- **Formats & archives:** JPEG, PNG, GIF, WebP, TIFF, AVIF, HEIC, JPEG XL, SVG, HDR/EXR, camera RAW, ZIP, 7z, and RAR.
- **Browsing & navigation:** Keyboard, mouse, and trackpad controls; zoom, pan, fullscreen, compact mode, recent files, and pinned shortcuts.
- **Collections & queues:** Browse, Favorites, Info, and Queue sidebars; searchable Masonry grids, sorting, filtering, grouping, multi-selection, and saved viewing lists.
- **Tabs & comparison:** Browser-style image, Masonry, and Compare tabs; synchronized zoom and pan, flexible layouts, image alignment, and floating image windows.
- **Image editing:** Crop, resize, rotate, flip, color adjustments, undo/redo, and export.
- **Slideshows & playback:** Configurable intervals, playback speed, transitions, pan/zoom motion, and looping.
- **Appearance & window management:** Light/dark themes, desktop-inspired looks, accent colors, transparency, UI scaling, automatic window sizing, and drag gestures.
- **Performance:** GPU rendering, background decoding, preloading, configurable caching, virtual texturing, and frame limiting.
- **File management:** Open With, rename, copy, move, extract, Trash integration, and native desktop thumbnails.
- **Pop-out controls:** Floating image windows with click-through, positioning, opacity, rotation, movement effects, and saved presets.
- **Session & desktop integration:** Restore previous workspaces, optional desktop menu bar, single-instance mode, and file-manager integration.
- **RAW quality:** Switch between developed previews and full-resolution RAW output.
- **Visual effects:** Snowfall, image/text watermarks, and blurred image backdrops.

## Downloads

Download the latest builds from [GitHub Releases](https://github.com/MobinWeekend/Jpeg-Viewer/releases).

| Platform | Available builds |
|---|---|
| Windows | x64 and ARM64 per-user installers and portable ZIPs |
| macOS | Intel (x86_64) and Apple Silicon (ARM64) |
| Linux | x64 and ARM64 `.tar.gz` and AppImage builds, Flatpak, and distribution packages where published |
| Linux packages | Debian/Ubuntu `.deb`, Fedora-compatible `.rpm`, and Arch `.pkg.tar.zst` where published |

### Portable Linux archives

1. Download and extract the matching `.tar.gz` archive.
2. Run `./jpeg_viewer` to launch, or `./install.sh` to install under `~/.local/opt/jpeg-viewer` and register the application, icon, and supported MIME types.
3. Run `./uninstall.sh` from the extracted archive to remove the installation. User settings are preserved.

**Platform notes:**
- Windows installers are per-user and do not require administrator rights. Binaries are currently unsigned, so SmartScreen may show a warning.
- Linux packages may depend on system libraries supplied by the distribution. AppImage compatibility varies by host.
- macOS builds are currently unsigned and not notarized. Gatekeeper may require manual approval.

## Supported formats

Support depends on the decoder and, for camera RAW, the specific camera model and file variant.

| Type | Formats / extensions |
|---|---|
| Raster and animated | AVIF, BMP, DDS, EXR, GIF, HDR, HEIC/HEIF, ICO, JPEG, JPEG XL, PNG, PNM, QOI, TGA, TIFF, WebP |
| Vector | SVG (rasterized for display; maximum 4096 × 4096 pixels) |
| Camera RAW | CR2, CR3, NEF, NRW, ARW, DNG, RAF, ORF, RW2, PEF, SRW, 3FR, FFF, IIQ, KDC, MOS, MRW, RWL, X3F |
| Archives | ZIP, 7z, RAR |

RAW decoding uses the Rust `zenraw` library and its `rawler` backend. No separately installed RAW converter is required. Recognized extensions do not guarantee support for every RAW variant.

## Build from source

### Requirements

- [Rust and Cargo](https://rustup.rs). The pinned toolchain in `rust-toolchain.toml` is installed automatically by rustup.
- **Linux (Debian/Ubuntu):**
  ```bash
  sudo apt-get install --no-install-recommends build-essential pkg-config libdbus-1-dev libx11-dev libxkbcommon-dev libwayland-dev libegl1-mesa-dev libgl1-mesa-dev
  ```
- **Windows:** Visual Studio Build Tools with the Desktop development with C++ workload.
- **macOS:** Xcode command-line tools (`xcode-select --install`).

### Build and run

```bash
git clone https://github.com/MobinWeekend/Jpeg-Viewer.git
cd Jpeg-Viewer

cargo run --release       # Build and run
cargo build --release     # Build only
cargo test                # Run tests
```

The executable is placed in `target/release/`.

Open one or more files directly:

```bash
jpeg_viewer image.jpg
jpeg_viewer one.jpg two.png three.webp
```

Passing multiple paths opens only the specified files rather than indexing their containing directory.

## Architecture

### Image decoding

Images are handled through a format-independent decoding pipeline:

`ImageEntry → Raw bytes → Format detection → Decoder registry → DecodedImage → Application / cache / renderer`

The common `DecodedImage` representation keeps individual decoder implementations independent of the rest of the application.

### Large images and caching

Large images can use virtual texturing, loading individual tiles on demand instead of keeping the entire decoded image in memory. Tile size and activation thresholds are configurable.

Background workers decode and preload nearby images. Cache size, preload radius, concurrency, and timing can be adjusted in Advanced settings.

### Native thumbnails

JPEG Viewer reuses operating-system and desktop-generated thumbnails where possible, falling back to its built-in pipeline when needed.

Linux integrations include the Freedesktop thumbnail cache, Tumbler, KDE KIO, and compatible XDG thumbnailer plugins. Generated previews use the shared cache with source metadata. These integrations are optional and do not require GTK or Qt development libraries.

## Queue files and limitations

Queue files (`.jvq`) use a versioned text format and support relative or absolute paths. Use **Remap missing root** when a directory tree has moved.

Queues currently support standalone filesystem images, not images inside archives.

Editing requires a supported, fully loaded raster image with a direct filesystem path. Archive entries, animated GIFs, virtual-texture images, and images still loading cannot be edited. Queue operations manage lists and paths; they do not perform batch image editing.

## Documentation

- [Getting Started](QUICK_START.md) — installation, controls, navigation, editing, appearance, and troubleshooting.
- **In-app Help:** Press **F1** for keyboard shortcuts and mouse controls.

## License

Licensed under **GPL-3.0-only**. See [LICENSE](LICENSE).
