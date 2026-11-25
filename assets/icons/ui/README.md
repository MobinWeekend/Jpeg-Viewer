# UI SVG icons

These are the editable source assets for the application's custom-drawn UI icons.
They are embedded into the executable at compile time and rasterized by the
project's existing `resvg` dependency when first used. The resulting textures
are cached per egui context and tinted with the active theme's text/foreground
color.

## Asset conventions

- Keep the `24 × 24` `viewBox` unless a new aspect ratio is intentional.
- Use white (`#fff`) fills and strokes. The renderer applies the current UI tint.
- Keep transparent backgrounds; do not add an opaque canvas.
- Preserve the filenames referenced by `src/app/ui/svg_icons.rs`.
- Replace the placeholder paths/shapes in each SVG to customize an icon.

## Icons

| File | UI purpose |
|---|---|
| `app-logo.svg` | Fallback mark shown while the application logo texture is unavailable |
| `hamburger.svg` | Main application menu button |
| `navigation-previous.svg`, `navigation-next.svg` | Main-view image navigation |
| `zoom-in.svg`, `zoom-out.svg` | Main-view zoom controls |
| `sidebar-show.svg`, `sidebar-hide.svg` | Sidebar drawer buttons |
| `masonry.svg` | Masonry mode and workspace buttons |
| `home.svg` | Home heading |
| `context-share.svg` | Share action |
| `context-locate.svg` | Locate in file manager |
| `context-queue.svg` | Queue actions |
| `context-favorite.svg` | Favorite actions |
| `context-reorder.svg` | Move to position |
| `context-archive.svg` | Archive entry actions |
| `context-copy-name.svg` | Copy entry name |

Common main-view context-menu actions intentionally continue to use ordinary
font glyphs/emojis, as requested; they are not custom SVG assets.
