# Unsafe code audit

Audited the supplied project snapshot (`Jpeg-Viewer-monitor-padding-current-monitor.zip`) for Rust `unsafe` blocks, unsafe functions, and FFI declarations.

## Unsafe areas reviewed

These areas fundamentally cross a native ABI, manipulate native handles/pointers, or access OpenGL state and therefore cannot be converted to safe Rust solely by rearranging code. A fully safe alternative would require a suitable safe wrapper crate, an architectural replacement, or loss of the feature.

- **OpenGL mipmap upload (`src/app/texture_mipmap.rs`)**: glow calls are unsafe because the code manages GPU context/texture state and GL lifetime invariants. Keep operations inside the egui Glow callback/context and texture lifetime. Potential improvement: encapsulate the upload operations into a narrowly scoped helper with documented invariants and test CPU-side mip generation separately.
- **Windows monitor enumeration (`src/platform/monitors.rs`)**: `EnumDisplayMonitors` callback, native monitor handles, and DPI APIs are FFI. The callback's opaque pointer is cast back to the vector passed by the caller. Invariant: callback receives that exact pointer during the synchronous enumeration call and does not retain it. Native handles are only used during the callback.
- **Windows thumbnail/COM (`src/platform/thumbnails.rs`)**: COM initialization, Shell image-factory calls, GDI bitmap extraction/deletion, property-store calls and PROPVARIANT conversion/clear are native APIs. The code handles COM initialization on its worker thread and clears owning PROPVARIANTs. Bitmap and COM object ownership should continue to be checked whenever these paths are changed.
- **Single-instance coordination (`src/platform/instance.rs`)**: Unix `flock`, Windows named mutex/handle functions, and foreground activation API are FFI. Handle lifetime is guarded by RAII (`InstanceLock`). The Unix file lock is tied to the open file lifetime. The Windows mutex handle is released/closed on Drop.
- **Power inhibition (`src/platform/power.rs`)**: Windows execution-state API is FFI. The API is simple but inherently native; call is limited to the power-inhibitor lifecycle.
- **Platform backdrop (`src/ui_theme/platform_backdrop.rs`)**: DWM attributes on Windows and Objective-C messaging on macOS require ABI calls and typed message signatures. Objective-C message dispatch via `objc_msgSend` and `transmute` cannot be made safe without a wrapper library. Keep selectors, argument types and message signatures tightly controlled.
- **Pop-out native window effects (`src/app/popout/platform.rs`)**: Windows `DwmSetWindowAttribute` calls adjust native corner preference, border color and system backdrop. The HWND is resolved from the generation-scoped pop-out title; each attribute pointer refers to a local value kept alive across the synchronous call. Unsupported DWM attributes on older Windows builds are intentionally treated as best-effort.
- **Wayland backdrop (`src/ui_theme/platform_backdrop/wayland_backdrop.rs`)**: imports an externally owned Wayland display and constructs protocol object IDs from native pointers. These are lifetime-sensitive FFI/foreign-display operations. Safety depends on the display and surface being live and owned by eframe for the duration of use, and not disconnecting the foreign display.
- **Windows image sorting**: unsafe Shell sorting call removed in this audit.

## Recommendation

Do not try to eliminate every unsafe block at the expense of feature reliability. The best follow-up is to centralize each native boundary, keep unsafe blocks as small as possible, and document a `SAFETY:` rationale adjacent to every block—especially the OpenGL, foreign Wayland display and Objective-C code. The present audit removes one avoidable unsafe call; remaining blocks mostly represent unavoidable native interfaces.

## Verification limitation

This environment does not have Cargo/Rust installed. The updated source was not compiled or tested here. Please run `cargo test` and the platform-specific thumbnail, monitor, backdrop, power-inhibitor and single-instance tests on Windows, Linux and macOS.
