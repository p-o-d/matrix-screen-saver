use std::sync::mpsc;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, Surface},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::wlr_layer::{
        Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
        LayerSurfaceConfigure,
    },
};
use wayland_client::{
    globals::GlobalList,
    protocol::{wl_output, wl_surface},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1, ext_idle_notifier_v1,
};

#[allow(unused_imports)]
use matrix_core::config::Config;

// ---------------------------------------------------------------------------
// AppEvent
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum AppEvent {
    /// User has been idle long enough — show the screensaver.
    Idle,
    /// User activity detected while idle — hide the screensaver.
    Resume,
    /// Compositor sent a new surface size for surface at given index.
    Resize(usize, u32, u32),
}

/// One layer surface per output.
pub struct SurfaceSlot {
    pub layer_surface: LayerSurface,
    pub wl_surface: wl_surface::WlSurface,
    pub configured: bool,
    pub last_size: Option<(u32, u32)>,
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

pub struct AppState {
    pub registry_state: RegistryState,
    pub compositor_state: CompositorState,
    pub output_state: OutputState,
    pub layer_shell: LayerShell,

    /// One slot per active output (populated on Idle, cleared on Resume/Dismiss).
    pub surfaces: Vec<SurfaceSlot>,

    /// Idle notifier global (ext-idle-notify-v1).
    pub idle_notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
    /// Active idle notification object.
    pub idle_notification: Option<ext_idle_notification_v1::ExtIdleNotificationV1>,

    /// Channel used to forward events to the main event loop.
    pub event_tx: mpsc::Sender<AppEvent>,
    /// Queue handle kept for creating objects later.
    pub qh: QueueHandle<Self>,
}

impl AppState {
    pub fn new(
        globals: &GlobalList,
        qh: QueueHandle<Self>,
        _config: &Config,
        event_tx: mpsc::Sender<AppEvent>,
    ) -> Result<Self, String> {
        let compositor_state = CompositorState::bind(globals, &qh)
            .map_err(|e| format!("compositor not available: {e:?}"))?;
        let layer_shell = LayerShell::bind(globals, &qh)
            .map_err(|e| format!("wlr-layer-shell not available (try KDE Plasma or a wlr compositor): {e:?}"))?;
        let output_state = OutputState::new(globals, &qh);

        // Attempt to bind ext-idle-notify-v1 (optional — compositor may not advertise it).
        let idle_notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1> = globals
            .bind(&qh, 1..=1, ())
            .ok();

        Ok(Self {
            registry_state: RegistryState::new(globals),
            compositor_state,
            output_state,
            layer_shell,
            surfaces: Vec::new(),
            idle_notifier,
            idle_notification: None,
            event_tx,
            qh,
        })
    }

    // -----------------------------------------------------------------------
    // Layer surface lifecycle
    // -----------------------------------------------------------------------

    fn create_layer_surface_for(&mut self, output: Option<&wl_output::WlOutput>) {
        let wl_surface = self.compositor_state.create_surface(&self.qh);
        let sctk_surface = Surface::from(wl_surface.clone());
        let layer_surface = self.layer_shell.create_layer_surface(
            &self.qh,
            sctk_surface,
            Layer::Overlay,
            Some("matrix-screensaver"),
            output,
        );
        layer_surface.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer_surface.set_exclusive_zone(-1);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer_surface.set_size(0, 0);
        wl_surface.commit();
        self.surfaces.push(SurfaceSlot {
            layer_surface,
            wl_surface,
            configured: false,
            last_size: None,
        });
    }

    /// Create one layer surface per connected output.
    /// Falls back to a single surface without output binding if none are known yet.
    pub fn create_layer_surfaces_all(&mut self) {
        if !self.surfaces.is_empty() {
            return;
        }
        let outputs: Vec<wl_output::WlOutput> = self.output_state.outputs().collect();
        if outputs.is_empty() {
            self.create_layer_surface_for(None);
        } else {
            for output in &outputs {
                self.create_layer_surface_for(Some(output));
            }
        }
    }

    /// Destroy all active layer surfaces.
    pub fn destroy_layer_surfaces(&mut self) {
        self.surfaces.clear();
    }

    // -----------------------------------------------------------------------
    // Idle detection
    // -----------------------------------------------------------------------

    /// Set up an idle notification with the given timeout (milliseconds).
    ///
    /// `seat` must be the `wl_seat` to monitor.  If `ext-idle-notify-v1` is
    /// not available, this is a no-op.
    pub fn request_idle_notification(
        &mut self,
        seat: &wayland_client::protocol::wl_seat::WlSeat,
        timeout_ms: u32,
    ) {
        if let Some(ref notifier) = self.idle_notifier {
            self.idle_notification = None; // cancel previous subscription if any
            let notification = notifier.get_idle_notification(timeout_ms, seat, &self.qh, ());
            self.idle_notification = Some(notification);
        }
    }
}

// ---------------------------------------------------------------------------
// CompositorHandler
// ---------------------------------------------------------------------------

impl CompositorHandler for AppState {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
    }
}

// ---------------------------------------------------------------------------
// OutputHandler
// ---------------------------------------------------------------------------

impl OutputHandler for AppState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

// ---------------------------------------------------------------------------
// LayerShellHandler
// ---------------------------------------------------------------------------

impl LayerShellHandler for AppState {
    fn closed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
    ) {
        // Remove just the closed surface; signal main if all gone.
        self.surfaces.retain(|s| s.layer_surface != *layer);
        if self.surfaces.is_empty() {
            let _ = self.event_tx.send(AppEvent::Resume);
        }
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        // Find which surface slot this configure belongs to.
        // LayerSurface::PartialEq uses Arc::ptr_eq on the inner data.
        let idx = self.surfaces.iter().position(|s| s.layer_surface == *layer);
        if let Some(idx) = idx {
            let (w, h) = configure.new_size;
            if w > 0 && h > 0 {
                self.surfaces[idx].last_size = Some((w, h));
                let _ = self.event_tx.send(AppEvent::Resize(idx, w, h));
            }
            self.surfaces[idx].configured = true;
        }
    }
}

// ---------------------------------------------------------------------------
// ProvidesRegistryState
// ---------------------------------------------------------------------------

impl ProvidesRegistryState for AppState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    // `registry_handlers!` generates runtime_add_global and runtime_remove_global.
    registry_handlers![OutputState];
}

// ---------------------------------------------------------------------------
// Dispatch for ext-idle-notify-v1 globals
// ---------------------------------------------------------------------------

/// The compositor dispatches events to `ExtIdleNotifierV1`, but the notifier
/// has no events of its own — so this Dispatch impl is a no-op.
impl Dispatch<ext_idle_notifier_v1::ExtIdleNotifierV1, ()> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &ext_idle_notifier_v1::ExtIdleNotifierV1,
        _event: ext_idle_notifier_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // ExtIdleNotifierV1 has no events.
    }
}

/// `ExtIdleNotificationV1` fires `idled` and `resumed` events.
impl Dispatch<ext_idle_notification_v1::ExtIdleNotificationV1, ()> for AppState {
    fn event(
        state: &mut Self,
        _proxy: &ext_idle_notification_v1::ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            ext_idle_notification_v1::Event::Idled => {
                let _ = state.event_tx.send(AppEvent::Idle);
            }
            ext_idle_notification_v1::Event::Resumed => {
                let _ = state.event_tx.send(AppEvent::Resume);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Dispatch for wl_seat (no-op — we only bind a seat to pass it to idle-notify)
// ---------------------------------------------------------------------------

impl Dispatch<wayland_client::protocol::wl_seat::WlSeat, ()> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &wayland_client::protocol::wl_seat::WlSeat,
        _event: wayland_client::protocol::wl_seat::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // No-op: we don't process seat events, we only use the seat object
        // as a handle to identify user input for ext-idle-notify-v1.
    }
}

// ---------------------------------------------------------------------------
// Delegate macros
// ---------------------------------------------------------------------------

delegate_compositor!(AppState);
delegate_layer!(AppState);
delegate_output!(AppState);
delegate_registry!(AppState);
