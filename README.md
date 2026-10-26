# JPEG Viewer

**Open it. View it. Move on.**

A GPU-accelerated image viewer written in Rust, built to keep image browsing responsive and the interface out of the way.

<img width="851" height="686" alt="JPEG Viewer screenshot" src="https://github.com/user-attachments/assets/26e2fe70-7222-42a9-9ea7-b558ecd58899" />

> “Do I look like I know what a JPEG is?!”
>
> The goal is simple: the application should stay a tool, so you can focus on enjoying the content.

**New to JPEG Viewer?** Start with the **[Quick Start Guide](QUICK_START.md)** for common workflows, controls, editing, and a short explanation of the settings.

## Features

- **GPU-accelerated rendering** with `egui` and `wgpu`
- **Broad image-format support**, including AVIF, HEIC/HEIF, JPEG XL, SVG, and camera RAW formats
- **Archive browsing** for images in ZIP, 7z, and RAR archives without extracting them first
- **GIF playback** with playback and speed controls
- **Configurable slideshows** with timing, looping, transitions, pan/zoom motion effects, and optional system-sleep prevention (enabled by default)
- **Session-only slideshow playback** so closing and reopening the app never resumes playback automatically
- **Browse, Queue, and Favorites** collections for navigating and organizing image selections
- **Image editing** with crop, rotate, flip, color adjustments, and export options for supported images
- **Background decoding and preloading** for smoother navigation
- **Configurable caching** to balance responsiveness and memory use
- **Virtual texturing** for very large images
- **Fullscreen and compact modes**
- **Persistent settings** for appearance, behavior, and performance
- **Drag-and-drop** support for images, folders, and supported archives
- **File type mismatch detection** when the file extension does not match its contents
- **File actions** including Open With, Locate in File Manager, and trash/recycle-bin integration where supported
- **Adjustable frame limiting** for performance and battery efficiency
- **Aspect-ratio labels** for common ratios such as 16:9, 4:3, and CinemaScope

## Supported formats

Support can vary by decoder and, for camera RAW files, by camera model and file variant.

### Raster and animated images

| Format | Extensions |
|---|---|
| AVIF | `.avif` |
| BMP | `.bmp` |
| DDS | `.dds` |
| EXR | `.exr` |
| GIF | `.gif` |
| HDR | `.hdr` |
| HEIC / HEIF | `.heic`, `.heif` |
| ICO | `.ico` |
| JPEG | `.jpg`, `.jpeg` |
| JPEG XL | `.jxl` |
| PNG | `.png` |
| PNM | `.pnm` |
| QOI | `.qoi` |
| TGA | `.tga` |
| TIFF | `.tif`, `.tiff` |
| WebP | `.webp` |

### Camera RAW

RAW decoding and development are handled in-process through the Rust `zenraw` library and its `rawler` backend; a separately installed converter such as darktable or dcraw is not required.

Recognized extensions include `.cr2`, `.cr3`, `.nef`, `.nrw`, `.arw`, `.dng`, `.raf`, `.orf`, `.rw2`, `.pef`, `.srw`, `.3fr`, `.fff`, `.iiq`, `.kdc`, `.mos`, `.mrw`, `.rwl`, and `.x3f`. Recognition does not guarantee that every camera model or RAW variant can be decoded.

### Vector images

- **SVG** (`.svg`): rasterized for GPU display. Extremely large SVGs are rasterized with a maximum dimension of 4096 × 4096 pixels to limit memory use.

### Archives

Images can be opened directly from **ZIP** (`.zip`), **7z** (`.7z`), and **RAR** (`.rar`) archives without extracting the archive first.

## Building from source

### Requirements

- [Rust and Cargo](https://rustup.rs)
- The platform dependencies required by `eframe`/`wgpu` for your operating system and graphics stack

### Clone the repository

```bash
git clone https://github.com/MobinWeekend/Jpeg-Viewer.git
cd Jpeg-Viewer
```

### Run in release mode

```bash
cargo run --release
```

### Build without running

```bash
cargo build --release
```

The executable is placed in `target/release/`.

To open a specific image from the command line, pass its path as an argument:

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

## Queue format and limitations

Queue files use the `.jvq` extension and a versioned text format. Paths can be stored relative to the queue file's directory or as absolute paths, with a root path available to help remap files when a directory tree moves. Missing entries can be retained and resolved later with **Remap missing root**. **Export paths** writes the queue's paths to a plain-text file.

Queues currently contain standalone filesystem image files only; images inside ZIP, 7z, and RAR archives are not added to queues. Queue batch actions are focused on queue management and path export; destructive batch image operations are not included.

## Documentation

- **[Quick Start Guide](QUICK_START.md)** — opening images, navigation, mouse and keyboard controls, Browse/Queue/Favorites, editing, settings, and troubleshooting

## License

See [LICENSE](LICENSE) for license details.
