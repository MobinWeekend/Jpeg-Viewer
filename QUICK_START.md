# Getting Started

## Installation

- **Windows installer:** choose the installer matching your CPU (x64 or ARM64). It installs for the current Windows user under `%LOCALAPPDATA%\Programs\JPEG Viewer` and does not require administrator rights.
- **Windows portable:** extract the matching Portable ZIP and run `jpeg_viewer.exe`; no installation is performed.
- **Linux portable:** extract the archive and run `./jpeg_viewer`, or use the included `install.sh` to add a launcher, icon, and file-type integration for the current user. Run `./uninstall.sh` from the extracted archive to remove that installation; user settings are preserved.


This guide covers the common workflows. For the supported formats, build instructions, and project overview, see [README.md](README.md). Press **F1** inside the app for a shortcut reference.

## Open an image

- **Ctrl+O:** open an image.
- **Ctrl+Shift+O:** open a folder and browse its supported images.
- Drag an image, folder, or supported archive (ZIP, 7z, RAR) into the window.
- From a terminal, pass a path: `jpeg_viewer image.jpg`. Pass several paths to open exactly that set of images: `jpeg_viewer one.jpg two.png three.webp`.

### Open a selection from your file manager

Select several supported images in your file manager and open them with JPEG Viewer. To keep the selection in one existing window when the file manager starts separate application processes, enable **Run only one instance** under **Settings → General → Startup**, then restart JPEG Viewer. Later launches are forwarded to the running window, which is restored/focused and displays the selected files as a collection rather than scanning neighboring files in the same folder. This option is disabled by default.

## Essential controls

| Action | Control |
|---|---|
| Previous / next image | **A / D**, **Left / Right**, or scroll wheel |
| First / last image | **Home / End**; **Ctrl+Left / Ctrl+Right** |
| Zoom in / out | **+ / -** or **Ctrl+Scroll** |
| Fit to window / 100% | **S**, **Down**, or **0** / **W** or **Up** |
| Pan zoomed image / zoom by dragging | Left-drag starting on the image / right-drag |
| Move window | Left-drag starting outside the image (windowed mode), or **Alt+Left-drag** in the main view |
| Resize window | **Shift+Left-drag** in the main view (windowed mode); custom-frame resize edges/corners when enabled |
| Fullscreen | **F**, **F11**, **Enter**, or middle-click |
| Settings / Help | **F5** / **F1** |
| Toggle sidebar | **T** or **F3** |
| Toggle compact mode / slideshow | **F4** / **L** |
| Copy image / copy path | **Ctrl+C** / **Ctrl+Shift+C** |
| Move current image to Trash / Recycle Bin | **Delete** |
| Exit fullscreen / close | **Escape** |

With **Invert Scroll Zoom** enabled in Settings, **Ctrl+Scroll** navigates instead of zooming. The cursor changes to a grab hand over the image and to a grabbing hand while panning. App shortcuts are ignored while a text field (for example search, rename, or a numeric setting) has keyboard focus, so typing does not trigger viewer commands. Mouse gestures and window controls can vary slightly by operating system.

## Viewing, padding, and window sizing

Open **Settings → Viewing** for image-view layout controls:

- **Padding:** choose **Use look defaults**, **Fine-tune current look**, or **Use custom padding**. Fine-tuning is saved separately for each Look profile. Top, right, bottom, and left values are shown in logical points.
- **Automatically fit window to each image:** when enabled, the window resizes as a new image is displayed. This only operates in windowed mode; maximized and fullscreen windows are left alone.
- **Size limits:** choose the monitor's available size or define custom minimum and maximum width and height. Custom values are in logical points.
- **Preserve image aspect ratio:** keep the image's proportions when its size is constrained.
- **Keep window centered:** center the window after an image-driven resize.

To run the same fit once without enabling automatic fitting, use **View → Wrap Window Around Image** or press **Ctrl+Alt+W**. The command also only resizes a window that is neither maximized nor fullscreen.

## Browse, Queue, and Favorites

Open the sidebar and choose **Browse**, **Queue**, **Favorites**, or **Info**.

- **Browse:** shows the current collection as thumbnails. Use the context menu for available file actions, including Open With, locate, rename, copy path, and adding an item to Queue or Favorites.
- **Queue:** assemble a sequence from selected files, the current image/list, or a folder (including recursive folders). Open the queue as the active collection, then return to the previous view when finished. Save/load `.jvq` files, export paths, remove unavailable entries, clear the queue, shuffle the next item, or repeat the queue.
- **Favorites:** keep a persistent list of images you want to revisit. Add files or the current image, or drag supported files into the tab. Reorder entries by dragging or entering a position number. Favorites are saved beside the settings file in `favorites.jvq`.
- **Reorder entries:** drag thumbnails to move them, or click the position number and enter a new order where supported.
- **Queue file paths:** if images moved, use **Remap missing root**. Queue files currently support filesystem images, not entries inside archives.

## Masonry and Compare

- Open **Masonry** from the desktop menu bar or its workspace controls to browse the current collection. It automatically scrolls to and highlights the image that was being viewed. Folder Masonry collections update when an image is moved into or out of a loaded folder, and deleted images are removed from the grid.
- Open **Compare** from the desktop menu-bar button or the hamburger menu. The current image fills the first pane when one is loaded; otherwise both panes start empty. Drop image files into the Compare area or choose **Add Images…** to select several files at once. Empty panes offer the same picker, and additional selected images create more panes.
- Use **Columns** or **Rows** to change the layout. **Sync pan** and **Sync zoom** control linked movement, while the two anchors in each pane can align matching points across images.

## Pop out an Image tab

Open an image in its own workspace tab with **+**. An Image tab reuses its image tile in the existing Browse grid, with a tab indicator, an in-tab **POP ON** / **POP OFF** status pill, and a close control. If its image is outside the current Browse collection, it appears as an additional tile in the same grid. Select the tab's pop-out status pill to open its right-hand **Flight Deck** settings sidebar. The sidebar can open or close the floating window and control click-through (PASS), bounds, stacking level, size, opacity, rotation, monitor selection, DVD movement, Ball physics, and Pass opacity effects. **Active monitor** is the default monitor setting: DVD/Ball movement uses the display the pop-out currently occupies most. You can drag the window to another display while either trick is active; movement pauses during the native drag and resumes using the new display's bounds. Choosing a named display instead pins the trick to that monitor. **Save .jvp** and **Load .jvp** are available there even while the native pop-out is closed; presets preserve the manual rotation angle. Open Image tabs reserve one additional full-image LRU slot each. Their loaded static textures remain pinned even when normal Browse preloading pauses, while the primary Browse preload window remains pinned across tab switches. GIFs stay on the existing animated decode path rather than this static-texture LRU. Minimizing the main window should leave already-open pop-outs visible and animating independently.

When PASS is off, **left-drag** the image to move the native window. In **Ball mode**, **right-drag** the image to throw it in the drag direction; the release speed sets the launch velocity. Left-drag continues to move the window instead of throwing. Use the four edges or corner handles with the left button to resize while preserving the image aspect ratio. The Size slider resizes the pop-out continuously, and manual resizing updates the slider. Hovering over the pop-out reveals compact close and stacking-level controls. Very short panoramic or portrait windows may hide that hover strip when there is not enough room; the Flight Deck remains available for those controls. Turn PASS off before moving, throwing, resizing, or using hover controls because click-through prevents the pop-out from receiving pointer input. Closing the Image tab also closes its pop-out. Animated GIFs loop in the pop-out using the tab's playback speed. Monitor positioning and window stacking depend on the window manager. Native Wayland allows manual window dragging but does not provide the absolute window-position control required by automatic DVD/Ball movement, so those modes are disabled there.

**Transparency note for Windows:** the pop-out requests a transparent native surface and preserves PNG/GIF alpha. However, the project's pinned eframe 0.33 Glow child-viewport backend has an upstream Windows transparency bug; this can make the native background appear black and make reduced image opacity look dark instead of showing the desktop. The upstream fix is in eframe 0.36.2, but this project intentionally retains its existing eframe 0.33 pin. True per-pixel desktop transparency on that backend therefore still requires a separate backend upgrade or backport.

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

If browsing feels slow, reduce **Cache Radius** or **Max Tasks** under Advanced. Increase **Thumbnail workers** if thumbnails are slow to appear, then restart the app. On Linux, JPEG Viewer also reuses valid thumbnails from the desktop's shared cache and available thumbnail services before using its own decoder. For very large images, keep virtual texturing enabled and adjust **Tile Size** and **VT Threshold** only if needed. The frame limiter can reduce GPU use while idle or unfocused.

## Linux portable install

For a `.tar.gz` release, extract the archive and run `./jpeg_viewer`. To add an application-menu entry and icon, run `./install.sh` from the extracted directory. The installer places the app under `~/.local/opt/jpeg-viewer` and registers a desktop entry, icon, and MIME types for the current user. `uninstall.sh` removes those files without deleting user settings. AppImage behavior and required libraries depend on the host distribution and desktop environment.

## Troubleshooting

- **Image looks jagged:** try Linear or Mipmap texture filtering.
- **Navigation is slow:** lower cache radius or preload task count; review frame limiter settings.
- **Large image uses too much memory:** check virtual-texture settings.
- **Thumbnails are slow:** increase thumbnail workers and restart.
- **Transparent areas are hard to inspect:** enable the checkerboard; it is hidden during slideshows.
- **Window resizing is difficult without a title bar:** enable the custom frame and adjust its thickness under Advanced → Window. In the main view, Shift+Left-drag starts a window resize while the window is in windowed mode.
- **A file-manager multi-selection opens in separate windows:** enable **Settings → General → Startup → Run only one instance** and restart the application. Subsequent launches are forwarded to the existing window.
- **Image-driven window resizing does not happen:** automatic fitting is disabled by default and only works in windowed mode. Enable it under **Settings → Viewing → Window sizing**, or use **View → Wrap Window Around Image** / **Ctrl+Alt+W** for a one-time fit.
- **Linux taskbar icon does not restore the window:** use the latest release package with the matching desktop entry; behavior should be checked on the specific desktop environment if it persists.
