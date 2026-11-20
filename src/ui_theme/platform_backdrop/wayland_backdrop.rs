//! Wayland compositor blur support.
//!
//! Borrow eframe/winit's existing Wayland display and surface. Prefer the
//! standardized `ext-background-effect-v1` protocol (COSMIC and Plasma 6.7+),
//! then fall back to KWin's legacy protocol for older Plasma releases.

use std::ffi::c_void;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use wayland_client::backend::{Backend, ObjectId};
use wayland_client::protocol::{wl_callback, wl_compositor, wl_region, wl_registry, wl_surface};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols::ext::background_effect::v1::client::{
    ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1,
    ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1,
};
use wayland_protocols_plasma::blur::client::{
    org_kde_kwin_blur::OrgKdeKwinBlur, org_kde_kwin_blur_manager::OrgKdeKwinBlurManager,
};

static DISPLAY_PTR: AtomicUsize = AtomicUsize::new(0);
static SURFACE_PTR: AtomicUsize = AtomicUsize::new(0);
static BACKDROP: OnceLock<Mutex<Option<WaylandBackdrop>>> = OnceLock::new();

#[derive(Default)]
struct WaylandState {
    globals: Vec<(u32, String, u32)>,
    blur_supported: bool,
}

enum BackdropEffect {
    Standard(ExtBackgroundEffectSurfaceV1),
    /// KWin's legacy protocol was used by Plasma before 6.7.
    Kde {
        effect: OrgKdeKwinBlur,
        manager: OrgKdeKwinBlurManager,
        surface: wl_surface::WlSurface,
    },
}

struct WaylandBackdrop {
    connection: Connection,
    // Keep the foreign surface proxy alive for the lifetime of the backdrop.
    _surface: wl_surface::WlSurface,
    effect: BackdropEffect,
    region: wl_region::WlRegion,
}

impl WaylandBackdrop {
    fn new(display_ptr: *mut c_void, surface_ptr: *mut c_void) -> Option<Self> {
        if display_ptr.is_null() || surface_ptr.is_null() {
            return None;
        }

        // SAFETY: eframe owns the display for the native-window lifetime. This
        // guest backend shares it and does not disconnect it when dropped.
        let backend = unsafe { Backend::from_foreign_display(display_ptr.cast()) };
        let connection = Connection::from_backend(backend);
        let mut queue = connection.new_event_queue();
        let queue_handle = queue.handle();
        let registry = connection.display().get_registry(&queue_handle, ());
        let mut state = WaylandState::default();
        queue.roundtrip(&mut state).ok()?;

        let compositor_name = state
            .globals
            .iter()
            .find(|(_, interface, _)| interface == "wl_compositor")
            .map(|(name, _, _)| *name)?;

        // Bind the standard protocol first. Plasma 6.7+ implements this protocol,
        // as does COSMIC; prefer it whenever the compositor advertises blur.
        let standard_manager_name = state
            .globals
            .iter()
            .find(|(_, interface, _)| interface == "ext_background_effect_manager_v1")
            .map(|(name, _, _)| *name);
        let kde_manager_name = state
            .globals
            .iter()
            .find(|(_, interface, _)| interface == "org_kde_kwin_blur_manager")
            .map(|(name, _, _)| *name);

        let compositor = registry.bind::<wl_compositor::WlCompositor, _, _>(
            compositor_name,
            1,
            &queue_handle,
            (),
        );

        // wayland-client 0.31 uses the argument order:
        // (global_name, version, queue_handle, user_data).
        let standard_effect = if let Some(manager_name) = standard_manager_name {
            let manager = registry.bind::<ExtBackgroundEffectManagerV1, _, _>(
                manager_name,
                1,
                &queue_handle,
                (),
            );
            queue.roundtrip(&mut state).ok()?;
            let supported = state.blur_supported;
            if supported {
                // SAFETY: eframe supplied this live wl_surface from the same display.
                let surface_id = unsafe {
                    ObjectId::from_ptr(wl_surface::WlSurface::interface(), surface_ptr.cast())
                        .ok()?
                };
                let surface = wl_surface::WlSurface::from_id(&connection, surface_id).ok()?;
                let effect = manager.get_background_effect(&surface, &queue_handle, ());
                manager.destroy();
                Some((surface, BackdropEffect::Standard(effect)))
            } else {
                manager.destroy();
                None
            }
        } else {
            None
        };

        let (surface, effect) = if let Some(result) = standard_effect {
            result
        } else {
            // Plasma versions before 6.7 expose KWin's older private blur protocol.
            // Keep this as a fallback only: Plasma 6.7 removed it in favour of
            // ext-background-effect-v1.
            let manager_name = kde_manager_name?;
            let manager =
                registry.bind::<OrgKdeKwinBlurManager, _, _>(manager_name, 1, &queue_handle, ());
            // SAFETY: eframe supplied this live wl_surface from the same display.
            let surface_id = unsafe {
                ObjectId::from_ptr(wl_surface::WlSurface::interface(), surface_ptr.cast()).ok()?
            };
            let surface = wl_surface::WlSurface::from_id(&connection, surface_id).ok()?;
            let effect = manager.create(&surface, &queue_handle, ());
            (
                surface.clone(),
                BackdropEffect::Kde {
                    effect,
                    manager,
                    surface,
                },
            )
        };

        let region = compositor.create_region(&queue_handle, ());
        // The compositor clips this large surface-local region to the actual
        // window bounds, so no resize event integration is necessary.
        region.add(0, 0, 32_767, 32_767);
        match &effect {
            BackdropEffect::Standard(effect) => effect.set_blur_region(Some(&region)),
            BackdropEffect::Kde { effect, .. } => {
                effect.set_region(Some(&region));
                effect.commit();
            }
        }
        // The region is applied with the next surface commit. Flush requests now.
        connection.flush().ok()?;

        Some(Self {
            connection,
            _surface: surface,
            effect,
            region,
        })
    }

    fn set_blur(&self, enabled: bool) -> Option<()> {
        match &self.effect {
            BackdropEffect::Standard(effect) => {
                effect.set_blur_region(enabled.then_some(&self.region));
            }
            BackdropEffect::Kde { effect, .. } => {
                if enabled {
                    effect.set_region(Some(&self.region));
                    effect.commit();
                } else {
                    effect.set_region(None);
                    effect.commit();
                }
            }
        }
        // Let winit's next buffer commit apply the pending blur-region change.
        self.connection.flush().ok()?;
        Some(())
    }
}

pub fn initialize(display: *mut c_void, surface: *mut c_void) {
    DISPLAY_PTR.store(display as usize, Ordering::Relaxed);
    SURFACE_PTR.store(surface as usize, Ordering::Relaxed);
}

pub fn shutdown() {
    if let Some(lock) = BACKDROP.get() {
        if let Ok(mut backdrop) = lock.lock() {
            backdrop.take();
        }
    }
    DISPLAY_PTR.store(0, Ordering::Relaxed);
    SURFACE_PTR.store(0, Ordering::Relaxed);
}

pub fn set_enabled(enabled: bool) -> bool {
    let Some(mut backdrop) = BACKDROP.get_or_init(|| Mutex::new(None)).lock().ok() else {
        return false;
    };

    if enabled {
        if backdrop.is_none() {
            let display = DISPLAY_PTR.load(Ordering::Relaxed) as *mut c_void;
            let surface = SURFACE_PTR.load(Ordering::Relaxed) as *mut c_void;
            *backdrop = WaylandBackdrop::new(display, surface);
        }
        backdrop
            .as_ref()
            .is_some_and(|backdrop| backdrop.set_blur(true).is_some())
    } else {
        if let Some(backdrop) = backdrop.as_ref() {
            let _ = backdrop.set_blur(false);
        }
        false
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            state.globals.push((name, interface, version));
        }
    }
}

impl Dispatch<wl_callback::WlCallback, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_callback::WlCallback,
        _event: wl_callback::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_compositor::WlCompositor,
        _event: wl_compositor::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_region::WlRegion, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &wl_region::WlRegion,
        _event: wl_region::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtBackgroundEffectManagerV1, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _proxy: &ExtBackgroundEffectManagerV1,
        event: wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
        if let wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::Event::Capabilities { flags } = event {
            state.blur_supported = match flags {
                WEnum::Value(flags) => flags.bits() & 1 != 0,
                WEnum::Unknown(flags) => flags & 1 != 0,
            };
        }
    }
}

impl Dispatch<ExtBackgroundEffectSurfaceV1, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &ExtBackgroundEffectSurfaceV1,
        _event: wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<OrgKdeKwinBlurManager, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &OrgKdeKwinBlurManager,
        _event: wayland_protocols_plasma::blur::client::org_kde_kwin_blur_manager::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<OrgKdeKwinBlur, ()> for WaylandState {
    fn event(
        _state: &mut Self,
        _proxy: &OrgKdeKwinBlur,
        _event: wayland_protocols_plasma::blur::client::org_kde_kwin_blur::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
    }
}

impl Drop for WaylandBackdrop {
    fn drop(&mut self) {
        match &self.effect {
            BackdropEffect::Standard(effect) => effect.destroy(),
            BackdropEffect::Kde {
                effect,
                manager,
                surface,
            } => {
                manager.unset(surface);
                effect.release();
            }
        }
        self.region.destroy();
        let _ = self.connection.flush();
    }
}
