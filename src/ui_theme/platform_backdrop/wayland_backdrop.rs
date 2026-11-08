//! Wayland implementation of `ext-background-effect-v1`.
//!
//! Borrow eframe/winit's existing Wayland display and surface. The compositor
//! controls blur strength; the client marks the application surface as a blur region.

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

static DISPLAY_PTR: AtomicUsize = AtomicUsize::new(0);
static SURFACE_PTR: AtomicUsize = AtomicUsize::new(0);
static BACKDROP: OnceLock<Mutex<Option<WaylandBackdrop>>> = OnceLock::new();

#[derive(Default)]
struct WaylandState {
    globals: Vec<(u32, String, u32)>,
    blur_supported: bool,
}

struct WaylandBackdrop {
    connection: Connection,
    // Keep the foreign surface proxy alive for the lifetime of the backdrop.
    _surface: wl_surface::WlSurface,
    effect: ExtBackgroundEffectSurfaceV1,
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

        let manager_name = state
            .globals
            .iter()
            .find(|(_, interface, _)| interface == "ext_background_effect_manager_v1")
            .map(|(name, _, _)| *name)?;
        let compositor_name = state
            .globals
            .iter()
            .find(|(_, interface, _)| interface == "wl_compositor")
            .map(|(name, _, _)| *name)?;

        // wayland-client 0.31 uses the argument order:
        // (global_name, version, queue_handle, user_data).
        let manager =
            registry.bind::<ExtBackgroundEffectManagerV1, _, _>(manager_name, 1, &queue_handle, ());
        let compositor = registry.bind::<wl_compositor::WlCompositor, _, _>(
            compositor_name,
            1,
            &queue_handle,
            (),
        );
        queue.roundtrip(&mut state).ok()?;
        if !state.blur_supported {
            return None;
        }

        // SAFETY: this is the live wl_surface supplied by eframe/winit and it
        // belongs to the same wl_display used to construct `connection`.
        let surface_id = unsafe {
            ObjectId::from_ptr(wl_surface::WlSurface::interface(), surface_ptr.cast()).ok()?
        };
        let surface = wl_surface::WlSurface::from_id(&connection, surface_id).ok()?;
        let effect = manager.get_background_effect(&surface, &queue_handle, ());
        // Existing effect objects remain valid after their manager is destroyed.
        manager.destroy();
        let region = compositor.create_region(&queue_handle, ());
        // The compositor clips this large surface-local region to the actual
        // window bounds, so no resize event integration is necessary.
        region.add(0, 0, 32_767, 32_767);
        effect.set_blur_region(Some(&region));
        // eframe will commit the next rendered buffer; the blur region is
        // double-buffered with that surface commit. Flush the requests now.
        connection.flush().ok()?;

        Some(Self {
            connection,
            _surface: surface,
            effect,
            region,
        })
    }

    fn set_blur(&self, enabled: bool) -> Option<()> {
        if enabled {
            self.effect.set_blur_region(Some(&self.region));
        } else {
            self.effect.set_blur_region(None);
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

impl Drop for WaylandBackdrop {
    fn drop(&mut self) {
        self.effect.destroy();
        self.region.destroy();
        let _ = self.connection.flush();
    }
}
