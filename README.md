# JPEG Viewer

<img width="802" height="632" alt="image" src="https://github.com/user-attachments/assets/303697e4-b0f5-4f86-8ec4-6b9276a8db0c" />

## Do I look like I know what a JPEG is?!

A GPU-accelerated image viewer written in Rust.

JPEG Viewer is a small passion project built around a simple idea: viewing images should feel fast, effortless, and enjoyable. It focuses on speed, simplicity, and a lightweight experience without unnecessary complexity.

Created primarily for personal use, this project aims to provide a responsive image viewer that stays out of the way and lets you focus on what matters — your images.

More formats, improvements, and features are coming as the project continues to grow.

## Features

- **GPU-accelerated rendering** using `egui` and `wgpu`
- **Archive support**: View images inside `.zip`, `.7z`, and `.rar` archives without extracting
- **GIF animation support** with speed control and playback controls
- **Slideshow mode** with adjustable timing, loop, and random order options
- **Smart caching system** with configurable cache radius for smooth navigation
- **Background preloading** of adjacent images for seamless browsing
- **Fullscreen mode** for immersive viewing
- **Persistent settings** saved automatically
- **Drag and drop** support for images, folders, and archives
- **Trash/Recycle Bin integration** for deleting images
- **Adjustable frame limiter** for optimal performance and battery life

## Supported Formats

AVIF, BMP, DDS, EXR, GIF (with speed controls), HDR, ICO, JPEG/JPG, PNG, PNM, QOI, TGA, TIFF, WEBP

### Archive Support
- ZIP archives (`.zip`)
- 7-Zip archives (`.7z`)
- RAR archives (`.rar`)

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
| Navigate images (with Ctrl: Zoom) | `Scroll` |

### Display
| Action | Shortcut |
|---|---|
| Toggle fullscreen | `F11` / `F` / `Enter` |
| Open settings menu | `Tab` |

### GIF Controls
| Action | Shortcut |
|---|---|
| Play / Pause GIF animation | `Space` |
| Slow down GIF (0.5x speed) | `[` |
| Speed up GIF (2x speed) | `]` |
| Reset GIF speed to 1x | `P` |

### Slideshow
| Action | Shortcut |
|---|---|
| Toggle slideshow on/off | `L` |
| Slideshow - slower speed (longer interval) | `,` |
| Slideshow - faster speed (shorter interval) | `.` |

### File Management
| Action | Shortcut |
|---|---|
| Open file/folder dialog | `Ctrl+O` |
| Move current image to trash | `Delete` |

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
| Toggle fullscreen | `Double-click Middle` |
| Fit image to window | `Double-click Right` |

## Tips

- Drag and drop images, folders, or archive files directly into the window
- The app remembers your window position and settings between sessions
- Adjust cache radius in settings for smoother navigation on large collections
- GIFs load a preview frame first, then the full animation in the background
- Use the settings menu to customize performance and behavior
- Start the app with an image path as an argument: `jpeg_viewer image.jpg`

## Caching & Performance

JPEG Viewer uses an intelligent caching system to ensure smooth navigation:

- **Configurable cache radius** (1-100): Controls how many images are kept in memory on each side of the current image
- **Delta threshold**: Smart preloading that focuses on images near your current position
- **Navigation timer**: Pause duration before shifting the cache origin (configurable in settings)
- **Memory-efficient**: Automatically evicts images outside the cache radius

## Slideshow

The slideshow feature allows you to view images automatically with:

- Adjustable interval (0.5s to 60s)
- Loop mode to repeat from the beginning
- Random order for varied viewing
- Speed controls in the toolbar (`↘` for slower, `↗` for faster)
- Toggle with the `L` key or the slideshow button in the toolbar

## Frame Limiter

To optimize performance and battery life, JPEG Viewer includes adjustable frame limiting:

- **Max FPS**: Maximum frames per second (0 = unlimited)
- **Idle FPS**: FPS limit when the app is idle (default: 15 FPS)
- **Idle Timeout**: Time of inactivity before entering idle mode (default: 2000ms)
- **Unfocused settings**: Separate FPS and timeout settings when the window is unfocused
