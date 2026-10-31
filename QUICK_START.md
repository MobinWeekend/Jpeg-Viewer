# Getting Started

This guide covers the common workflows. For the supported formats, build instructions, and project overview, see [README.md](README.md). Press **F1** inside the app for a shortcut reference.

## Open an image

- **Ctrl+O:** open an image.
- **Ctrl+Shift+O:** open a folder and browse its supported images.
- Drag an image, folder, or supported archive (ZIP, 7z, RAR) into the window.
- From a terminal, pass a path: `jpeg_viewer image.jpg`.

## Essential controls

| Action | Control |
|---|---|
| Previous / next image | **A / D**, **Left / Right**, or scroll wheel |
| First / last image | **Home / End**; **Ctrl+Left / Ctrl+Right** |
| Zoom in / out | **+ / -** or **Ctrl+Scroll** |
| Fit to window / 100% | **S**, **Down**, or **0** / **W** or **Up** |
| Pan zoomed image | Left-drag; right-drag adjusts zoom |
| Fullscreen | **F**, **F11**, **Enter**, or middle-click |
| Settings / Help | **Tab** / **F1** |
| Toggle compact mode / slideshow | **F4** / **L** |
| Copy image / copy path | **Ctrl+C** / **Ctrl+Shift+C** |
| Move current image to Trash / Recycle Bin | **Delete** |
| Exit fullscreen / close | **Escape** |

With **Invert Scroll Zoom** enabled in Settings, **Ctrl+Scroll** navigates instead of zooming. Mouse gestures and window controls can vary slightly by operating system.

## Browse, Queue, and Favorites

Open the sidebar and choose **Browse**, **Queue**, **Favorites**, or **Info**.

- **Browse:** shows the current collection as thumbnails. Use the context menu for available file actions, including Open With, locate, rename, copy path, and adding an item to Queue or Favorites.
- **Queue:** assemble a sequence from selected files, the current image/list, or a folder (including recursive folders). Open the queue as the active collection, then return to the previous view when finished. Save/load `.jvq` files, export paths, remove unavailable entries, clear the queue, shuffle the next item, or repeat the queue.
- **Favorites:** keep a persistent list of images you want to revisit. Add files or the current image, or drag supported files into the tab. Reorder entries by dragging or entering a position number. Favorites are saved beside the settings file in `favorites.jvq`.
- **Reorder entries:** drag thumbnails to move them, or click the position number and enter a new order where supported.
- **Queue file paths:** if images moved, use **Remap missing root**. Queue files currently support filesystem images, not entries inside archives.

## Edit an image

Enable **Edit mode** from the toolbar or menu. Editing is available for fully loaded, supported raster images with a direct filesystem path. Archive entries, animated GIFs, virtual-texture images, and images still loading are not editable.

- Rotate left/right; flip horizontally/vertically.
- Crop by dragging a rectangle or entering crop coordinates.
- Resize by entering pixel width and height; keep aspect ratio linked or edit each dimension independently.
- Adjust brightness, contrast, saturation, and hue.
- Use Undo/Redo to navigate edit history.
- Choose an export format and JPEG quality, then use **Save As** to create a new file. **Overwrite original** replaces the source after confirmation; **Discard working copy** abandons unsaved edits.

Edits remain in a working copy until you save them. Colour adjustments are processed in row tiles, and export writes to a temporary file rather than buffering the compressed output in RAM. The current image and encoder APIs still require a full working image for some operations, so this reduces peak memory but is not a fully disk-backed editor for arbitrarily large images.

## Appearance and desktop integration

Open **Settings → Look** to choose how the app is presented:

- **Automatic:** detect the desktop where possible.
- **JPEG Viewer:** the original application design.
- **COSMIC design:** COSMIC-style surfaces and controls.
- **GNOME / Adwaita design:** Adwaita-style surfaces and spacing.
- **Windows 98 classic:** gray classic controls, a full-width menu bar, and a toolbar between the sidebars. The optional **Dial-up!** effect reveals images in strips; its 1–56 kbps setting changes the reveal speed and timing jitter.

You can also adjust toolbar placement, desktop menu bar, navigation auto-hide, OS/desktop blur or glass, fullscreen blur behavior, and main-view opacity where supported. Actual native blur depends on the OS and compositor. In **Advanced → Window**, disable native decorations to use the custom frame and its resize edges/corners.

Other useful settings:

- **Theme:** Follow System, Light, or Dark.
- **UI scale / Text size:** change control scale and text scale separately; slider changes apply when adjustment finishes.
- **Main color:** select a preset accent or a custom color.
- **Main view background:** follow the theme, force dark/light, or choose a custom color.
- **Transparency checkerboard:** show transparent areas and tune checker size/brightness. It is hidden during slideshow playback.
- **Texture filter:** Nearest for crisp pixel edges, Linear for smooth general viewing, or Mipmap for downscaling.
- **Sort:** Natural, Newest, Oldest, or Shuffle; sorting applies to the current collection.

## GIFs and slideshows

- **Space:** play/pause a GIF; **[** slows it to 0.5×, **]** speeds it up to 2×, **P** resets to 1×.
- **L:** toggle slideshow. Use Settings to choose interval, looping, transitions, motion effects, and motion strength.
- **, / .:** decrease/increase slideshow speed.
- **Prevent system sleep:** enabled by default during slideshow playback. Slideshow playback is not restored automatically after restarting the app.

## Performance settings

If browsing feels slow, reduce **Cache Radius** or **Max Tasks** under Advanced. Increase **Thumbnail workers** if thumbnails are slow to appear, then restart the app. For very large images, keep virtual texturing enabled and adjust **Tile Size** and **VT Threshold** only if needed. The frame limiter can reduce GPU use while idle or unfocused.

## Linux portable install

For a `.tar.gz` release, extract the archive and run `./jpeg_viewer`. To add an application-menu entry and icon, run `./install.sh` from the extracted directory. The installer places the app under `~/.local/opt/jpeg-viewer` and registers a desktop entry for the current user. AppImage behavior and required libraries depend on the host distribution and desktop environment.

## Troubleshooting

- **Image looks jagged:** try Linear or Mipmap texture filtering.
- **Navigation is slow:** lower cache radius or preload task count; review frame limiter settings.
- **Large image uses too much memory:** check virtual-texture settings.
- **Thumbnails are slow:** increase thumbnail workers and restart.
- **Transparent areas are hard to inspect:** enable the checkerboard; it is hidden during slideshows.
- **Window resizing is difficult without a title bar:** enable the custom frame and adjust its thickness under Advanced → Window.
- **Linux taskbar icon does not restore the window:** use the latest release package with the matching desktop entry; behavior should be checked on the specific desktop environment if it persists.
