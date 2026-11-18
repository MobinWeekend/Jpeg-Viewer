# Workspace module map

`workspace/mod.rs` owns the workspace data model (`WorkspaceTab`, `WorkspaceManager`, and shared action/state types). The child modules add focused `ViewerApp` implementations around that shared state:

| Module | Responsibility |
| --- | --- |
| `core.rs` | Open workspace collections, snapshot the primary viewer, and stop viewer preloading when switching surfaces |
| `tabs.rs` | Activate, close, reorder, and render tabs; handle tab keyboard shortcuts |
| `new_tab.rs` | Recently opened paths, pinned shortcuts, Home data persistence, and Home actions |
| `session.rs` | Save and restore workspace sessions, including the restore prompt and session serialization |
| `masonry.rs` | Masonry workspace surface, filtering, sorting, selection, and grid layout |
| `masonry_group.rs` | Folder groups, thumbnail cards, context menus, and entry pointer interaction |
| `masonry_actions.rs` | Open entries and dispatch actions emitted by Masonry |
| `file_operations.rs` | Copy, move, delete, conflict handling, and confirmation dialogs |
| `compare.rs` | Compare image loading, Viewer-compatible per-pane zoom input, synchronized navigation, anchor alignment, and anchor-driven image rotation |
| `helpers.rs` | Shared entry labels/paths, archive reads, staging, and filesystem helpers |

Keep shared workspace state and types in `mod.rs`; place new behavior in the narrowest matching module. Methods shared across sibling workspace modules use `pub(super)`, while methods called by the rest of the application retain `pub(crate)` visibility.
