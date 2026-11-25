# UI SVG icons

These are the editable source assets for the application's custom-drawn UI icons.
They are embedded into the executable at compile time and rasterized by the
project's existing `resvg` dependency when first used. The resulting textures
are cached per egui context and tinted with the active theme's text/foreground
color.

## Asset conventions

- Keep the `24 × 24` `viewBox` unless a new aspect ratio is intentional.
- Use a single-color fill/stroke (black or white); the renderer applies the current UI tint. Favorite marks intentionally retain their embedded colors.
- Keep transparent backgrounds; do not add an opaque canvas.
- Preserve the filenames referenced by `src/app/ui/svg_icons.rs`.
- Replace the placeholder paths/shapes in each SVG to customize an icon.

## Icons

| File | UI purpose |
|---|---|
| `app-logo.svg` | Fallback mark shown while the application logo texture is unavailable |
| `hamburger.svg` | Main application menu button |
| `navigation-previous.svg`, `navigation-next.svg` | Main-view image navigation |
| `zoom-in.svg`, `zoom-out.svg` | Incremental zoom controls |
| `one-to-one.svg`, `fit.svg` | Reset to 1:1 and fit-to-window actions |
| `caption-*.svg` | Custom minimize, maximize, restore, and close controls |
| `eye.svg` | Current-image marker in Masonry |
| `undo.svg`, `redo.svg`, `rotate-*.svg`, `flip-*.svg` | Edit actions |
| `crop.svg`, `resize.svg`, `brightness.svg`, `contrast.svg`, `saturation.svg`, `hue.svg` | Edit tools and adjustment headings |
| `sidebar-show.svg`, `sidebar-hide.svg` | Sidebar drawer buttons |
| `masonry.svg` | Masonry mode and workspace buttons |
| `home.svg` | Home heading |
| `context-share.svg` | Share action |
| `context-locate.svg` | Locate in file manager |
| `context-queue.svg` | Queue actions |
| `context-reorder.svg` | Move to position |
| `context-archive.svg` | Archive entry actions |
| `context-copy-name.svg` | Copy entry name |

Favorite state uses separate outlined and filled star or heart SVG variants. The combined heart + star style is no longer offered.
Main-view and hamburger menu actions use the custom SVG icon assets referenced
by `src/app/ui/svg_icons.rs`.
