# JPEG Viewer

[![Latest release](https://img.shields.io/github/v/release/MobinWeekend/Jpeg-Viewer?label=release)](https://github.com/MobinWeekend/Jpeg-Viewer/releases)
[![License: GPL-3.0-only](https://img.shields.io/badge/License-GPL--3.0--only-blue.svg)](LICENSE)

A GPU-accelerated image viewer written in Rust with `egui` and `wgpu`. Browse image collections, inspect large images, run slideshows, organize queues and favorites, and make basic edits without a large editing interface getting in the way.

![JPEG Viewer screenshot](https://github.com/user-attachments/assets/8dd722cc-7994-40a5-84d2-474e1ee332f6)

**New users:** see [Getting Started](QUICK_START.md). For in-app controls, press **F1**.

## Features

- **Image formats:** JPEG, PNG, GIF, WebP, BMP, TIFF, AVIF, HEIC/HEIF, JPEG XL, SVG, HDR/EXR, and more; includes in-process camera RAW decoding (support varies by camera model).
- **Archives:** browse images inside ZIP, 7z, and RAR archives without extracting them.
- **Navigation:** keyboard and mouse controls, trackpad scrolling, zoom and pan with a 20% visibility clamp that remains valid at high zoom levels, fullscreen, and compact mode. In windowed mode, drag the image-view margins to move the window.
- **Collections:** Browse, Queue, Favorites, and Info sidebar tabs; thumbnail lists support drag reordering and direct position entry where applicable. Browse context menus include Share. With single-instance mode enabled, file-manager multi-selections are forwarded to the existing window and shown as the selected collection.
- **Queues:** create a viewing/slideshow sequence, save and load `.jvq` lists, export paths, shuffle, repeat, and remap moved folders.
- **Workspace tabs:** open images, Masonry collections, and comparisons in browser-style tabs. Switch with **Ctrl+PageUp**/**Ctrl+PageDown** and reorder the active tab with **Ctrl+Shift+PageUp/PageDown/ArrowLeft/ArrowRight**. The strip can render inline in the custom title bar, and it hides when only the primary viewer remains.
- **Image-tab pop-outs:** open an Image tab in an undecorated floating window with a compact 64-point, two-row settings panel below the image, aspect-ratio-preserving size slider (applied on release) and edge/corner resize, click-through, adjustable image opacity, hover-only close/stacking controls, and DVD-style bouncing with speed and independent left/top/right/bottom monitor-edge insets. DVD monitor and edge settings are grouped in a pop-up menu. Pop-outs close with their tabs, continue rendering independently while the main window is minimized, and loop animated GIFs at the tab playback speed. Each open Image tab reserves one additional full-image cache slot so its loaded static texture stays pinned while the primary Browse preload window remains warm across tab switches. GIFs continue to use their animated decode path rather than the static-image LRU. Open Image tabs reuse their existing thumbnail in the Browse grid, with a tab indicator, live pop-out marker, and close control; tabs whose images are outside the current Browse collection appear as additional tiles in that same grid without duplicating a Browse thumbnail. Windows transparency currently requires an eframe backend fix newer than the project's pinned 0.33 Glow child-viewport backend; see Quick Start.
- **Home page:** a Home surface in the primary viewer showing recently opened items and pinned shortcuts; open pinned images, folders, and `.jvq` queues in the Viewer or Masonry.
- **Masonry browsing:** grid browsing of a folder with optional recursive subfolders, search, extension filtering, and natural/name/newest/oldest/random sorting; select multiple entries or view a selected subset, and group results by collapsible folders. Opening Masonry scrolls to the currently viewed image and marks it with a centered eye badge. Ctrl+click always enters Select mode and toggles an entry; while Select mode is active, ordinary clicks also toggle entries without Ctrl. Hover highlighting and selection indicators remain visible independently of the current-image eye marker. Loaded folder collections stay in sync when files are moved or trashed.
- **Compare:** open it from the desktop menu bar or hamburger menu. It starts with the current image when available, always shows at least two panes, and accepts dropped images or multiple selections through **Add Images…**. Includes synchronized pan and zoom, columns or rows layout, right-click anchor placement, split reordering, and anchor alignment — two corresponding points per pane compute matching zoom, pan, and image rotation. Sidebar and Masonry entries can be sent to Compare or dragged into a specific split.
- **Editing:** crop, resize with optional aspect-ratio locking, rotate, flip, adjust brightness/contrast/saturation/hue, undo/redo, and export to a selected format. The source is unchanged until you save or overwrite it.
- **GIFs and slideshows:** playback speed controls, configurable intervals, transitions, pan/zoom motion, looping, and optional prevention of system sleep.
- **Appearance:** system/light/dark theme, UI and text scaling, accent colors, transparency checkerboard, translucent backgrounds where supported, and desktop-oriented looks for JPEG Viewer, COSMIC, GNOME/Adwaita, and Windows 98. Main-view padding can follow the active Look, be fine-tuned per Look, or use custom values.
- **Window sizing:** optionally fit the window to each viewed image, use monitor or custom size limits, preserve the image aspect ratio when constrained, and center the resized window.
- **Window gestures:** optionally move the window with **Alt+left-drag** and resize the nearest corner with **Shift+left-drag** anywhere in the app.
- **Single-instance mode:** optionally forward later file-open requests to the existing window, including multi-file selections.
- **Performance:** background decoding and preloading, configurable cache, virtual texturing for very large images, and an adjustable frame limiter. Native desktop thumbnails are reused where available, with the built-in thumbnail pipeline as a fallback.
- **File actions:** drag and drop, Open With, locate in file manager, rename supported filesystem images, copy image/path, and trash integration where supported.
- **File operations:** copy, extract archive entries, move, and delete to Trash with confirmation; name conflicts resolve with **Replace all**, **Skip all**, or **Keep Both all** (numbered suffix), with replacements backed up until committed.
- **Session restore:** restore the previous session's tabs, Masonry collections, comparisons, and viewing positions; choose **automatic**, **ask**, or **never** on startup.
- **Tricks:** optional decorative effects inside the main image view — snowfall, a text or image watermark, and a blurred image backdrop — each configurable for color, position, speed, opacity, and size.
- **RAW full quality:** toggle camera RAW files between the developed preview and the full-resolution RAW output.
- **Desktop menu bar:** an optional desktop-style menu bar (Compare, File, Edit, View, Queue) that can sit in its own row or inline in the custom title bar.

## Downloads

Get the latest artifacts from [GitHub Releases](https://github.com/MobinWeekend/Jpeg-Viewer/releases). Choose the build matching your operating system and CPU architecture.

| Platform | Available builds |
|---|---|
| Windows | x64 and ARM64 per-user installers, plus portable ZIPs |
| macOS | Intel (x86_64) and Apple Silicon (ARM64) |
| Linux | x64 and ARM64 `.tar.gz`, x64 and ARM64 AppImage, Flatpak, and distribution packages where published |
| Linux packages | Debian/Ubuntu `.deb`, Fedora-compatible `.rpm`, and Arch `.pkg.tar.zst` where published |

### Portable Linux archives

1. Download and extract the matching `.tar.gz` archive.
2. Run `./jpeg_viewer` to launch directly, or run `./install.sh` to install under `~/.local/opt/jpeg-viewer` and register an application-menu entry, icon, and supported MIME types for the current user.
3. To remove that per-user installation, run `./uninstall.sh` from the extracted archive. It removes the installed app and desktop integration but preserves user settings.

Windows Setup installers are per-user and do not require administrator rights; portable ZIPs are also available. Windows binaries are currently unsigned, so SmartScreen may display a reputation warning. Linux packages can depend on system libraries provided by the distribution. AppImage compatibility depends on the host system. macOS builds are currently unsigned and not notarized, so Gatekeeper may require manual approval in Security settings.

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

- [Rust and Cargo](https://rustup.rs). The toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml), so `rustup` installs the exact version automatically on first build.
- Platform libraries for the graphics and windowing stack, matching what the release builds install:
  - **Linux (Debian/Ubuntu):** `sudo apt-get install --no-install-recommends build-essential pkg-config libdbus-1-dev libx11-dev libxkbcommon-dev libwayland-dev libegl1-mesa-dev libgl1-mesa-dev`
  - **Windows:** VS Build Tools with the "Desktop development with C++" workload (MSVC linker)
  - **macOS:** Xcode command-line tools (`xcode-select --install`)

```bash
git clone https://github.com/MobinWeekend/Jpeg-Viewer.git
cd Jpeg-Viewer
cargo run --release       # build and run
cargo build --release     # build only
```

The executable is placed in `target/release/`. Unit tests can be run with `cargo test`. Open a file directly by passing its path. Pass multiple image paths to open only that explicit selection rather than indexing their containing directory:

```bash
jpeg_viewer image.jpg
jpeg_viewer one.jpg two.png three.webp
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

### Platform-native thumbnails

For filesystem images, the thumbnail worker first reuses valid operating-system or desktop thumbnails before falling back to JPEG Viewer's own embedded-preview and decoder pipeline. On Linux it checks the shared Freedesktop thumbnail cache, then uses the available desktop integration: Tumbler on Xfce, KDE's KIO worker for 128-pixel previews on Plasma, or installed XDG `.thumbnailer` plugins on GNOME, COSMIC, and compatible desktops such as Cinnamon, MATE, and Budgie. Larger KDE previews are reused from the shared cache when Dolphin or another KIO client has already generated them; otherwise JPEG Viewer falls back to an installed standard thumbnailer or its own decoder. Generated Linux previews are written to the shared cache with source URI, modification-time, and file-size metadata. These integrations are optional; missing services, plugins, or undersized previews simply fall back to the built-in path. No GTK or Qt development libraries are required to build JPEG Viewer.

## Queue files and limitations

Queue files use the `.jvq` extension and a versioned text format. Paths can be relative to the queue file or absolute. If a directory tree moves, use **Remap missing root** to reconnect missing entries. **Export Queue Paths** writes paths to a text file.

Queues currently store standalone filesystem image files only; images inside archives cannot be added to queues. Editing is limited to supported, fully loaded raster images with a direct filesystem path; archive entries, animated GIFs, virtual-texture images, and images still loading are not editable. Batch queue actions manage lists and paths; they do not perform destructive batch image editing.

## Documentation

- [Getting Started](QUICK_START.md) — essential controls, file-manager multi-selection, window gestures and sizing, sidebar collections, editing, appearance, and troubleshooting.
- In-app Help — press **F1** for shortcuts and mouse controls.

## License

Licensed under **GPL-3.0-only**. See [LICENSE](LICENSE).
