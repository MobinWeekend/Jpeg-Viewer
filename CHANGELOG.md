# Changelog

## Unreleased

- Fixed the Browse sidebar context menu so Share and the other actions are presented in one popup.
- Added a direct Compare button to the desktop menu bar and hamburger menu. Compare now starts with at least two panes, supports image drag-and-drop, and can add multiple images through the file picker.
- Masonry now scrolls to the image that was active when it opened and marks it with an eye badge. Loaded folder collections update when files are moved between folders or sent to the trash.
- Moving an image out of the currently loaded Browse folder now removes it from the sidebar when its destination folder is not loaded; already-loaded destination folders receive moved entries.
- Compare now owns image drops throughout its workspace (except explicit Queue/Favorites drops), preventing dropped files from replacing the main Viewer image.
- Added right-click entry actions to the main image view, matching the single-entry operations available in Sidebar and Masonry.
- Added per-split Compare context actions for anchors, pan/zoom and rotation alignment, view reset, split synchronization, image replacement, and opening or closing splits. Consolidated global synchronization controls under one “Sync:” label.
- Masonry Ctrl+click now always enables Select mode and toggles the clicked entry; while Select mode is active, plain clicks also toggle selection. The current image has a centered eye marker, while hover highlighting and multi-selection remain independently visible.
- Main Viewer double-left-click now fits the current image (the former double-left-click Open File gesture is removed). Pan clamping now uses 20% of the smaller of the image and viewport dimensions, preventing inverted bounds on zoomed-in images, and is reapplied after pointer, wheel, keyboard, and animated zoom changes.
- Added Compare access from Browse, Queue, Favorites, and Masonry entry context menus. Sidebar images can also be dragged directly into a specific Compare split.
- Added a Masonry button to the Compare toolbar, direct anchor placement at the right-click position, and per-split earlier/later/first/last ordering actions. Reset View preserves anchors; Clear Anchors remains separate.
- Masonry hover highlighting is now available on the current image, and its eye marker is centered without suppressing the selected-entry indicator.
