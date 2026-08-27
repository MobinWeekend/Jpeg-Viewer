# JPEG Viewer

<img width="977" height="763" alt="image" src="https://github.com/user-attachments/assets/2d8bca2e-5d48-49c0-938b-3aeb685c373b" />

A GPU-accelerated image viewer written in Rust.

JPEG Viewer is a small passion project built around a simple idea: viewing images should feel fast, effortless, and enjoyable. It focuses on speed, simplicity, and a lightweight experience without unnecessary complexity.

Created primarily for personal use, this project aims to provide a responsive image viewer that stays out of the way and lets you focus on what matters — your images.

More formats, improvements, and features are coming as the project continues to grow.

## Features

- **GPU-accelerated rendering** using `egui` and `wgpu`
- **Wide image format support** including modern formats such as AVIF, HEIC, JPEG XL, and SVG
- **Archive support**: View images inside `.zip`, `.7z`, and `.rar` archives without extracting
- **GIF animation support** with speed control and playback controls
- **Slideshow mode** with adjustable timing, loop, and random order options
- **Smart caching system** with configurable cache radius for smooth navigation
- **Background preloading** of adjacent images for seamless browsing
- **Virtual texturing** for extremely large images
- **Fullscreen mode** for immersive viewing
- **Persistent settings** saved automatically
- **Drag and drop** support for images, folders, and archives
- **Trash/Recycle Bin integration** for deleting images
- **Adjustable frame limiter** for performance and battery efficiency
- **File type detection**: Automatically detects when a file extension does not match its actual contents and offers to rename it
- **Aspect ratio labeling**: Displays common aspect ratio names (e.g. 16:9, 4:3, CinemaScope) for quick reference
- **Decoder abstraction**: Image decoding is isolated behind a common interface, making the format support easier to extend

## Supported Formats

### Raster & Animated Images

- **AVIF** (`.avif`)
- **BMP** (`.bmp`)
- **DDS** (`.dds`)
- **EXR** (`.exr`)
- **GIF** (`.gif`) — animation and playback controls
- **HDR** (`.hdr`)
- **HEIC / HEIF** (`.heic`, `.heif`)
- **ICO** (`.ico`)
- **JPEG / JPG** (`.jpg`, `.jpeg`)
- **JPEG XL** (`.jxl`)
- **PNG** (`.png`)
- **PNM** (`.pnm`)
- **QOI** (`.qoi`)
- **TGA** (`.tga`)
- **TIFF** (`.tif`, `.tiff`)
- **WebP** (`.webp`)

### Vector Images

- **SVG** (`.svg`)

SVG files are rasterized for GPU-based display. Extremely large SVGs are automatically rasterized at a maximum dimension of 4096×4096 pixels to prevent excessive memory usage.

### Archive Support

Images can be opened directly from:

- ZIP archives (`.zip`)
- 7-Zip archives (`.7z`)
- RAR archives (`.rar`)

Archives do not need to be extracted before viewing.

## Keyboard Shortcuts

### Navigation

| Action | Shortcut |
|---|---|
| Previous / Next image | `◀` / `▶` |
| Previous / Next image | `A` / `D` |
| Jump to first image | `Home` |
| Jump to last image | `End` |
| Jump to first / last image | `Ctrl+◀` / `Ctrl+▶` |

### Zoom & View

| Action | Shortcut |
|---|---|
| Zoom in / Zoom out | `+` / `-` |
| Reset zoom to 100% | `W` / `↑` |
| Fit image to window | `S` / `↓` / `0` |
| Navigate images | `Scroll` |
| Zoom while navigating | `Ctrl + Scroll` |

### Display

| Action | Shortcut |
|---|---|
| Toggle fullscreen | `F11` / `F` / `Enter` |
| Open settings menu | `Tab` |

### GIF Controls

| Action | Shortcut |
|---|---|
| Play / Pause GIF animation | `Space` |
| Slow down GIF (0.5× speed) | `[` |
| Speed up GIF (2× speed) | `]` |
| Reset GIF speed to 1× | `P` |

### Slideshow

| Action | Shortcut |
|---|---|
| Toggle slideshow on/off | `L` |
| Slideshow — slower speed | `,` |
| Slideshow — faster speed | `.` |

### File Management

| Action | Shortcut |
|---|---|
| Open file dialog | `Ctrl+O` |
| Open folder dialog | `Ctrl+Shift+O` |
| Move current image to trash | `Delete` |
| Copy image to clipboard | `Ctrl+C` |
| Copy image path to clipboard | `Ctrl+Shift+C` |

### General

| Action | Shortcut |
|---|---|
| Exit fullscreen / Close window | `Escape` |

## Mouse Controls

| Action | Mouse Input |
|---|---|
| Navigate between images | `Scroll` |
| Zoom in/out | `Ctrl + Scroll` |
| Pan image | `Left Drag` |
| Zoom in/out | `Right Drag` |
| Open file dialog (on empty screen) | `Double-click Left` |
| Toggle fullscreen | `Middle Click` |
| Fit image to window | `Double-click Right` |

## Tips

- Drag and drop images, folders, or archive files directly into the window
- The app remembers your window position and settings between sessions
- Adjust the cache radius in settings for smoother navigation on large collections
- GIFs load a preview frame first, then the full animation in the background
- Use the settings menu to customize performance and behavior
- Start the app with an image path as an argument:

```bash
jpeg_viewer image.jpg
```

- If the app detects a mismatch between a file's extension and its actual format, a rename suggestion will appear in the toolbar
- Very large images are rendered using tiled virtual textures for smooth performance
- SVG files are rasterized automatically for display

## Caching & Performance

JPEG Viewer uses an intelligent caching and preloading system to keep navigation responsive.

- **Configurable cache radius** (1–100): Controls how many images are kept in memory on each side of the current image
- **Delta threshold**: Prioritizes images near the current position
- **Navigation timer**: Controls when the cache origin shifts after navigation
- **Background preloading**: Loads nearby images without blocking the UI
- **Memory-efficient eviction**: Images outside the active cache range are automatically removed
- **Virtual texturing**: Extremely large images can be handled using tiled loading instead of keeping the entire decoded image in memory

## Slideshow

The slideshow feature allows you to view images automatically with:

- Adjustable interval (0.5s to 60s)
- Loop mode to repeat from the beginning
- Random order for varied viewing
- Speed controls in the toolbar (`↘` for slower, `↗` for faster)
- Toggle with the `L` key or the slideshow button in the toolbar

## Frame Limiter

To optimize performance and battery life, JPEG Viewer includes adjustable frame limiting:

- **Max FPS**: Maximum rendering rate (`0` = unlimited)
- **Idle FPS**: FPS limit when the application is idle
- **Idle Timeout**: Time of inactivity before entering idle mode
- **Unfocused FPS**: Separate FPS limit when the window is unfocused
- **Unfocused Idle Timeout**: Separate inactivity timeout for unfocused windows

The viewer dynamically requests repaints based on user interaction, image loading, animations, slideshows, and overlay visibility rather than continuously rendering at maximum speed when it is not necessary.

## File Type Detection

JPEG Viewer detects the actual image format from the file contents rather than relying solely on the filename extension.

For example, if a file is named:

```text
photo.jpg
```

but its contents are actually PNG data, JPEG Viewer can detect the mismatch and offer to rename the file to the appropriate extension.

This helps prevent incorrectly named image files from causing loading problems.

## Large Image Support

JPEG Viewer supports extremely large images through tiled virtual texturing.

Instead of decoding and keeping the entire image in memory, large images can be divided into smaller tiles and loaded as needed.

This makes it possible to work with images that would otherwise require hundreds of megabytes or several gigabytes of memory.

## Decoder Architecture

Image decoding is isolated behind a common decoder interface.

The loading pipeline is:

```text
ImageEntry
    │
    ▼
Raw Bytes
    │
    ▼
Format Detection
    │
    ▼
Decoder Registry
    │
    ▼
Image Decoder
    │
    ▼
DecodedImage
    │
    ▼
Application / Cache / Renderer
```

The application works with a format-independent `DecodedImage` representation rather than directly depending on individual image decoding libraries.

This makes it easier to add or replace decoders without changing the rest of the application.

## Building from Source

### 1. Install Rust

Install Rust and Cargo using **rustup**:

[Install Rustup](https://rustup.rs)

Follow the instructions for your operating system.

### 2. Clone the repository

Clone the repository and enter its directory:

```bash
git clone https://github.com/MobinWeekend/Jpeg-Viewer.git
cd Jpeg-Viewer
```

### 3. Build and run

Open a terminal in the repository directory and run:

```bash
cargo run --release
```

That's it! Cargo will download the required dependencies, build the application, and launch it.

### Building without running

If you only want to build the application:

```bash
cargo build --release
```

The compiled executable will be located in:

```text
target/release/
```
