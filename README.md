# JPEG Viewer

A GPU-accelerated image viewer written in Rust with `egui` and `wgpu`. Browse image collections, inspect large images, run slideshows, organize queues and favorites, and make basic edits without a large editing interface getting in the way.

![JPEG Viewer screenshot](https://github.com/user-attachments/assets/26e2fe70-7222-42a9-9ea7-b558ecd58899)

**New users:** see [Getting Started](QUICK_START.md). For in-app controls, press **F1**.

## Features

- **Image formats:** JPEG, PNG, GIF, WebP, BMP, TIFF, AVIF, HEIC/HEIF, JPEG XL, SVG, HDR/EXR, and more; includes in-process camera RAW decoding (support varies by camera model).
- **Archives:** browse images inside ZIP, 7z, and RAR archives without extracting them.
- **Navigation:** keyboard and mouse controls, trackpad scrolling, zoom and pan, fullscreen, and compact mode.
- **Collections:** Browse, Queue, Favorites, and Info sidebar tabs; thumbnail lists support drag reordering and direct position entry where applicable.
- **Queues:** create a viewing/slideshow sequence, save and load `.jvq` lists, export paths, shuffle, repeat, and remap moved folders.
- **Editing:** crop, rotate, flip, adjust brightness/contrast/saturation/hue, undo/redo, and export to a selected format. The source is unchanged until you save or overwrite it.
- **GIFs and slideshows:** playback speed controls, configurable intervals, transitions, pan/zoom motion, looping, and optional prevention of system sleep.
- **Appearance:** system/light/dark theme, UI and text scaling, accent colors, transparency checkerboard, translucent backgrounds where supported, and desktop-oriented looks for JPEG Viewer, COSMIC, GNOME/Adwaita, and Windows 98.
- **Performance:** background decoding and preloading, configurable cache, virtual texturing for very large images, and an adjustable frame limiter.
- **File actions:** drag and drop, Open With, locate in file manager, rename supported filesystem images, copy image/path, and trash integration where supported.

## Downloads

Get the latest artifacts from [GitHub Releases](https://github.com/MobinWeekend/Jpeg-Viewer/releases). Choose the build matching your operating system and CPU architecture.

| Platform | Available builds |
|---|---|
| Windows | x64 and ARM64 |
| macOS | Intel (x86_64) and Apple Silicon (ARM64) |
| Linux | x64 and ARM64 `.tar.gz`, x64 and ARM64 AppImage, Flatpak, and distribution packages where published |
| Linux packages | Debian/Ubuntu `.deb`, Fedora-compatible `.rpm`, and Arch `.pkg.tar.zst` where published |

### Portable Linux archives

1. Download and extract the matching `.tar.gz` archive.
2. Run `./jpeg_viewer` to launch directly, or run `./install.sh` to install under `~/.local/opt/jpeg-viewer` and register an application-menu entry and icon.

Linux packages can depend on system libraries provided by the distribution. AppImage compatibility depends on the host system. macOS builds are currently unsigned and may require approval in Security settings.

## Supported formats

Format support depends on the decoder and, for camera RAW files, the specific camera model and file variant.

| Type | Formats / extensions |
|---|---|
| Raster and animated | AVIF (`.avif`), BMP (`.bmp`), DDS (`.dds`), EXR (`.exr`), GIF (`.gif`), HDR (`.hdr`), HEIC/HEIF (`.heic`, `.heif`), ICO (`.ico`), JPEG (`.jpg`, `.jpeg`), JPEG XL (`.jxl`), PNG (`.png`), PNM (`.pnm`), QOI (`.qoi`), TGA (`.tga`), TIFF (`.tif`, `.tiff`), WebP (`.webp`) |
| Vector | SVG (`.svg`); rasterized for display. Very large SVGs are limited to 4096 × 4096 pixels. |
| Camera RAW | `.cr2`, `.cr3`, `.nef`, `.nrw`, `.arw`, `.dng`, `.raf`, `.orf`, `.rw2`, `.pef`, `.srw`, `.3fr`, `.fff`, `.iiq`, `.kdc`, `.mos`, `.mrw`, `.rwl`, `.x3f` |
| Archives | ZIP (`.zip`), 7z (`.7z`), RAR (`.rar`) |

RAW decoding uses the Rust `zenraw` library and its `rawler` backend; a separately installed RAW converter is not required. Recognized extensions do not guarantee every RAW variant is supported.

## Build from source

Requirements: [Rust and Cargo](https://rustup.rs) and the platform dependencies needed by `eframe`/`wgpu` and your graphics stack.

```bash
git clone https://github.com/MobinWeekend/Jpeg-Viewer.git
cd Jpeg-Viewer
cargo run --release       # build and run
cargo build --release     # build only
```

The executable is placed in `target/release/`. Open a file directly by passing its path:

```bash
jpeg_viewer image.jpg
```

## Architecture notes

### Image decoding

Image decoding is isolated behind a common decoder interface. The loading pipeline is approximately:

```text
ImageEntry
    ↓
Raw bytes
    ↓
Format detection
    ↓
Decoder registry
    ↓
Image decoder
    ↓
DecodedImage
    ↓
Application / cache / renderer
```

The application works with a format-independent `DecodedImage` representation instead of depending directly on each decoder implementation. This makes it easier to add or replace decoders without changing the rest of the application.

### Large images and virtual texturing

Very large images can be split into tiles and loaded as needed, rather than requiring the entire decoded image to remain in memory at once. The virtual-texture threshold and tile size can be adjusted in Advanced settings; these settings trade memory use against rendering and loading behavior.

### Caching and preloading

The viewer caches nearby images and preloads adjacent items in background tasks. Cache radius, task concurrency, and preload timing can be tuned in Advanced settings to balance navigation speed, background activity, and memory consumption.

## Queue files and limitations

Queue files use the `.jvq` extension and a versioned text format. Paths can be relative to the queue file or absolute. If a directory tree moves, use **Remap missing root** to reconnect missing entries. **Export Queue Paths** writes paths to a text file.

Queues currently store standalone filesystem image files only; images inside archives cannot be added to queues. Editing is limited to supported, fully loaded raster images with a direct filesystem path; archive entries, animated GIFs, virtual-texture images, and images still loading are not editable. Batch queue actions manage lists and paths; they do not perform destructive batch image editing.

## Documentation

- [Getting Started](QUICK_START.md) — essential controls, sidebar collections, editing, appearance, and troubleshooting.
- In-app Help — press **F1** for shortcuts and mouse controls.

## License

See [LICENSE](LICENSE).
