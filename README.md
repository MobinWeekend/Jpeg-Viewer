# Jpeg-Viewer
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
- **Smart caching system** with configurable cache radius for smooth navigation
- **Background preloading** of adjacent images for seamless browsing
- **Full screen mode** for immersive viewing
- **Persistent settings** saved automatically
- **Drag and drop** support for images, folders, and archives
- **Trash/Recycle Bin integration** for deleting images

## Supported Formats

| Format | Support |
|--------|---------|
| JPEG/JPG | ✅ Full |
| PNG | ✅ Full |
| GIF | ✅ Full (with Speed controls) |
| BMP | ✅ Full |
| WEBP | ✅ Full |

### Archive Support
- ZIP archives (`.zip`)
- 7-Zip archives (`.7z`)
- RAR archives (`.rar`)

## Keyboard & Mouse Shortcuts

### Keyboard Shortcuts

| Action | Shortcut |
|---|---|
| Open File | `Ctrl + O` |
| Next Image | `Right Arrow` / `D` |
| Previous Image | `Left Arrow` / `A` |
| Jump to First Image | `Home` / `Ctrl + Left Arrow` / `Ctrl + A` |
| Jump to Last Image | `End` / `Ctrl + Right Arrow` / `Ctrl + D` |
| Zoom In | `+` |
| Zoom Out | `-` |
| Reset Zoom (100%) | `W` / `Up Arrow` |
| Fit Image to Window | `S` / `Down Arrow` / `Num 0` |
| Toggle Fullscreen | `F11` / `Enter` / `F` |
| Move Image to Trash | `Delete` |
| Exit Fullscreen/Program | `Esc` |

#### GIF Controls

| Action | Shortcut |
|---|---|
| Play/Pause GIF | `Space` |
| Slow Down GIF | `[` |
| Speed Up GIF | `]` |
| Reset GIF Speed | `P` |

### Mouse Controls

| Action | Mouse Input |
|---|---|
| Open File | Double-click Left Mouse Button |
| Fit Image to Window | Double-click Right Mouse Button |
| Toggle Full screen | Click Middle Mouse Button |

### Mouse Wheel

The mouse wheel can be used for either image navigation or zooming:

| Action | Shortcut |
|---|---|
| Next Image | Scroll Down |
| Previous Image | Scroll Up |
| Zoom In / Out | Hold `Ctrl` while scrolling |

**Note**: The mouse wheel behavior can be inverted using the "Invert Ctrl Scroll" toggle in the toolbar or by changing the `b_ctrl_invert` setting.

## Caching & Performance

JPEG Viewer uses an intelligent caching system to ensure smooth navigation:

- **Configurable cache radius** (1-100): Controls how many images are kept in memory on each side of the current image
- **Delta threshold**: Smart preloading that focuses on images near your current position
- **Navigation timer**: Pause for 1.2 seconds (configurable) before shifting the cache origin
- **Memory-efficient**: Automatically evicts images outside the cache radius

The cache status is displayed in the toolbar as: `Cache: X/Y (r:N, Δ:M)` where:
- `X` = currently cached images
- `Y` = maximum cache capacity
- `r` = cache radius setting
- `Δ` = delta threshold (3/5 of radius)

## Tips

- Mouse actions only work when the cursor is over the image area.
- GIFs load progressively: first frame appears immediately, full animation loads in the background.
- The cache radius can be adjusted in real-time from the toolbar.
- Window position and size are saved automatically on exit.
