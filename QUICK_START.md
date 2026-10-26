# JPEG Viewer — Quick Start

A short guide to opening images, navigating collections, editing pictures, and understanding the settings.

## 1. Open your first image

- Start JPEG Viewer, then press **Ctrl+O** to choose an image.
- Press **Ctrl+Shift+O** to choose a folder and browse its supported images.
- You can also drag an image, folder, or supported archive into the app window.
- To launch directly into an image, run `jpeg_viewer image.jpg` from a terminal.

## 2. Navigate and control the image

| Action | Control |
|---|---|
| Previous / next image | **A / D**, **Left / Right**, or mouse wheel |
| First / last image | **Home / End** |
| Zoom in / out | **+ / -** or **Ctrl + mouse wheel** |
| Fit image to window | **S**, **Down**, or **0** |
| Reset to 100% | **W** or **Up** |
| Pan a zoomed image | **Left-drag** |
| Alternate zoom gesture | **Right-drag** |
| Fullscreen | **F**, **F11**, **Enter**, or middle-click |
| Open settings | **Tab** |
| Help | **F1** |
| Copy image / path | **Ctrl+C** / **Ctrl+Shift+C** |
| Move image to Trash / Recycle Bin | **Delete** |
| Exit fullscreen / close | **Escape** |

If the scroll direction feels wrong, change **Invert Scroll Zoom** in Settings. With it enabled, Ctrl+Scroll navigates; otherwise Ctrl+Scroll zooms.

## 3. Sidebar, Queue, and Favorites

Open the sidebar to browse the current image list, build a queue, or keep favorites. The exact tabs and available actions depend on the active sidebar view.

### Browse

- Browse the current folder or image collection using thumbnails.
- Select a thumbnail to open that image.
- Use the context menu for file actions such as opening with another app, locating the file, renaming, copying its path, or adding it to Favorites/Queue where available.
- Drag supported files into the appropriate list to add them.

### Queue

Use Queue to make a hand-picked sequence for viewing or a slideshow.

- **Add files**: choose files to add.
- **Current image**: add the image currently open.
- **Current view / Browse list if empty**: add the current collection or use the Browse list as a starting queue when the queue is empty.
- **Folder**: add images from a folder; the recursive folder option includes nested folders.
- **Open queue**: display the queue as the active collection. **Return** goes back to the previous collection.
- **Save / Load**: store a queue in a `.jvq` file or load one later. Loading lets you append unique items or replace the current queue.
- **Export paths**: export queue file paths as text.
- **Remap missing root**: point a queue at a new root folder if files have moved.
- **Remove unavailable / Clear queue**: clean up missing items or empty the queue.
- **Shuffle next**: randomize the next queue choice. **Repeat queue**: continue again after the end.
- **Entry size**: change thumbnail card size. Drag cards to reorder them; use the numbered position control for direct reordering where available.

### Favorites

- Add images from Browse or Queue using the context menu, or use **Add current image / Add files**.
- Favorites can be reordered and used as a curated collection.
- Remove an item from Favorites through its context menu when you no longer want it saved there.

## 4. Edit an image

Open **Edit mode** from the toolbar/menu. Editing is available for fully loaded, non-animated raster files with a direct file path; images inside archives, animated GIFs, virtual-texture images, or images still loading may not be editable.

- **Rotate left / right** and **Flip horizontal / vertical**: transform the working copy.
- **Undo / Redo**: move through edit history.
- **Crop**: draw a rectangle on the image, or expand the numeric crop controls to enter X, Y, width, and height. Apply or clear the crop selection as needed.
- **Brightness / Contrast / Saturation / Hue**: adjust the working preview. Use **Reset colour adjustments** to reset those sliders.
- **Export format**: choose the output image format.
- **JPEG quality**: set output quality when saving as JPEG; higher values generally retain more detail and produce larger files.
- **Save As**: save a new file without replacing the source.
- **Overwrite original**: replace the source after confirmation. Keep a backup if the original matters.
- **Discard working copy**: abandon unsaved edits.

The editor uses a working copy. Changes do not replace the source until you choose a save action.

## 5. Settings — every option, briefly

Open Settings with **Tab**. Settings are saved automatically unless otherwise noted.

### General and appearance

| Option | What it does |
|---|---|
| **Compact mode** | Toggles a more compact interface. **F4** also toggles it. |
| **Invert Scroll Zoom** | Reverses the Ctrl+Scroll behavior between navigation and zoom. |
| **Texture Filter — Nearest** | Fast, crisp pixel sampling; useful for pixel art. |
| **Texture Filter — Linear** | Smooth image sampling for general viewing. |
| **Texture Filter — Mipmap** | Uses mipmapping for smoother downscaling; may cost more resources. |
| **Theme — Follow System / Light / Dark** | Follows the operating system or forces a light/dark UI. |
| **UI scale** | Changes the overall size of interface controls. The value is applied after you finish adjusting the slider. |
| **Text size** | Changes UI text independently of overall control scaling. The value is applied after you finish adjusting the slider. |
| **Main color** | Sets the interface accent: Teal, Blue, Purple, Orange, Rose, or Custom. Custom lets you pick an accent color. |
| **Main view background — Follow theme** | Uses a dark or light image-view background according to the active UI theme. |
| **Main view background — Dark / Light** | Forces a dark or light background behind images. |
| **Main view background — Custom** | Choose a custom background color behind images. |
| **Show transparency checkerboard** | Shows a checker pattern through transparent parts of images that contain alpha. The checkerboard is suppressed during slideshow playback. |
| **Checker size** | Changes the checkerboard tile size. |
| **Checker brightness** | Changes checkerboard brightness/contrast. |
| **Start in Fullscreen Mode** | Launches the app in fullscreen. In this mode, Escape closes the app as noted in Settings. |
| **Start centered on screen** | Starts the window centered on the display. |
| **Thumbnail workers** | Sets how many background workers generate thumbnails. Restart the app for changes to take effect. |
| **Sort** | Sorts the current collection by Natural order, Newest, Oldest, or Shuffle. |

### Slideshow

| Option | What it does |
|---|---|
| **Enable Slideshow** | Starts/stops automatic image advance. Shortcut: **L**. |
| **Interval** | Sets how long each image remains on screen, from 0.5 to 60 seconds. |
| **Loop** | Restarts the slideshow after the last image. |
| **Transition — Instant** | Switches images without a transition. |
| **Transition — Fade** | Fades between images. |
| **Transition — Slide** | Slides images into/out of view. |
| **Transition — Random** | Chooses a transition automatically. |
| **Image motion — Off** | Keeps each image stationary. |
| **Image motion — Smart** | Chooses motion based on image aspect ratio. |
| **Pan wide images / Pan tall images** | Moves horizontally or vertically across an image. |
| **Zoom in / Zoom out** | Applies a gradual zoom during display. |
| **Image motion — Random** | Chooses a motion effect automatically. |
| **Motion strength** | Controls the amount of pan/zoom motion. |
| **Slideshow speed shortcuts** | **,** slows down; **.** speeds up. |

### Advanced — virtual textures

| Option | What it does |
|---|---|
| **Tile Size** | Sets the dimensions of tiles used to render very large images. Larger tiles may reduce tile count but use more memory per tile. |
| **VT Threshold** | Sets the image dimension threshold at which virtual texturing is used. Adjust only if you understand the memory/performance trade-off. |

### Advanced — cache and preloading

| Option | What it does |
|---|---|
| **Cache Radius** | Controls how many nearby images are included around the current position for caching/preloading. Higher values can improve navigation but use more memory. |
| **Delta Factor** | Tunes how strongly cache/preload priority follows distance from the current image. |
| **Max Tasks** | Limits concurrent cache/preload tasks. Lower values reduce background load; higher values may preload faster. |
| **Preload Throttle** | Sets a delay between preload work bursts, in milliseconds. Increase it to reduce background activity. |
| **Navigation Pause** | Sets how long navigation can settle before the cache origin is adjusted. |
| **Cache progress / Clear Cache** | Shows preload progress and clears cached/preload state when needed. |

### Advanced — frame limiter

| Option | What it does |
|---|---|
| **Max FPS** | Caps the active rendering frame rate. **0** means unlimited. |
| **Idle FPS** | Sets the frame limit while the app is idle. **0** stops idle frame drawing. |
| **Idle Timeout** | Time without activity before idle mode begins. |
| **Unfocused Timeout** | Delay before applying the unfocused-window behavior. |
| **Unfocused Idle FPS** | Frame limit while the window is unfocused/idle. **0** stops frame drawing in that state. |
| **Current status** | Displays whether the app is active, idle, animating, or loading, plus FPS and focus state. |

### Advanced — window and maintenance

| Option | What it does |
|---|---|
| **Show Title Bar** | Toggles native window decorations; marked work-in-progress in the UI. |
| **Reset Window Position** | Clears the saved window position so the app can use its default placement. |
| **Reset Window Size** | Clears the saved window size so the app can use its default size. |
| **Reset All** | Restores settings to defaults. Review the confirmation carefully before proceeding. |
| **File Info** | Shows the current file name, file size, and path. |
| **Settings File** | Shows the settings file location and opens it in the default text editor. Also reports settings read/write status. |
| **Copy Image / Copy Path** | Copies the current image or its path to the clipboard. |

## 6. GIFs and slideshows

- **Space** plays or pauses a GIF.
- **[** slows GIF playback to 0.5×; **]** speeds it up to 2×; **P** resets to 1×.
- Press **L** to toggle slideshow playback.
- The slideshow uses the active collection (such as Browse, Queue, or Favorites). Set interval, transition, and motion in Settings.
- Transparency checkerboard is intentionally hidden during slideshow playback.

## 7. Common troubleshooting

- **Image looks jagged when zoomed:** try Linear or Mipmap texture filtering.
- **Navigation feels slow:** reduce cache radius or max preload tasks; check the frame limiter settings.
- **Large image uses too much memory:** keep virtual texturing enabled and try a smaller tile size or a lower VT threshold.
- **Thumbnails take a long time to appear:** increase thumbnail workers (restart required), while considering CPU load.
- **Transparent image background is hard to judge:** enable the transparency checkerboard and adjust its size/brightness. It is hidden while a slideshow is playing.
- **Settings are not behaving as expected:** open Settings File to check its status, or open the settings file to inspect it.

For full format details, build instructions, architecture notes, and the complete shortcut list, see [README.md](README.md).
