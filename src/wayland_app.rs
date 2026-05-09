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
use crate::config::Config;

// ---------------------------------------------------------------------------
// AppEvent
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum AppEvent {
    /// User has been idle long enough — show the screensaver.
    Idle,
    /// User activity detected while idle — hide the screensaver.
    Resume,
    /// Any key/button pressed while the screensaver is active — dismiss.
    Dismiss,
    /// Compositor sent a new surface size.
    Resize(u32, u32),
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

pub struct AppState {
    pub registry_state: RegistryState,
    pub compositor_state: CompositorState,
    pub output_state: OutputState,
    pub layer_shell: LayerShell,

    /// Active layer surface (present only while screensaver is shown).
    pub layer_surface: Option<LayerSurface>,
    /// The underlying `wl_surface` for the layer surface.
    pub wl_surface: Option<wl_surface::WlSurface>,
    /// `true` once the compositor has sent the first configure event.
    pub configured: bool,

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
            layer_surface: None,
            wl_surface: None,
            configured: false,
            idle_notifier,
            idle_notification: None,
            event_tx,
            qh,
        })
    }

    // -----------------------------------------------------------------------
    // Layer surface lifecycle
    // -----------------------------------------------------------------------

    /// Create the overlay layer surface (used when the screensaver becomes active).
    pub fn create_layer_surface(&mut self, output: Option<&wl_output::WlOutput>) {
        if self.layer_surface.is_some() {
            return;
        }

        // `compositor_state.create_surface` returns a `wl_surface::WlSurface`.
        // SCTK's `create_layer_surface` requires `impl Into<Surface>`.
        // `Surface` has `From<wl_surface::WlSurface>`, so we go via that.
        let wl_surface = self.compositor_state.create_surface(&self.qh);
        let sctk_surface = Surface::from(wl_surface.clone());

        let layer_surface = self.layer_shell.create_layer_surface(
            &self.qh,
            sctk_surface,
            Layer::Overlay,
            Some("matrix-screensaver"),
            output,
        );

        // Stretch to fill the entire output.
        layer_surface.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        // -1 means "take all space, don't reserve any".
        layer_surface.set_exclusive_zone(-1);
        // Grab keyboard focus so we can detect key presses for dismiss.
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        // Ask compositor to tell us the size via configure.
        layer_surface.set_size(0, 0);

        wl_surface.commit();

        self.wl_surface = Some(wl_surface);
        self.layer_surface = Some(layer_surface);
        self.configured = false;
    }

    /// Destroy the overlay layer surface (used when the screensaver is dismissed).
    pub fn destroy_layer_surface(&mut self) {
        // Drop the LayerSurface — SCTK destroys the zwlr_layer_surface_v1 role object on drop,
        // and the underlying wl_surface is wrapped in a `Surface` inside LayerSurface which
        // destroys it on drop.
        self.layer_surface = None;
        // The wl_surface reference we kept is now invalid; clear it.
        self.wl_surface = None;
        self.configured = false;
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
        _layer: &LayerSurface,
    ) {
        self.layer_surface = None;
        self.wl_surface = None;
        self.configured = false;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        // Note: SCTK calls ack_configure internally before dispatching to us.
        let (w, h) = configure.new_size;
        if w > 0 && h > 0 {
            let _ = self.event_tx.send(AppEvent::Resize(w, h));
        }
        self.configured = true;
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
// Delegate macros
// ---------------------------------------------------------------------------

delegate_compositor!(AppState);
delegate_layer!(AppState);
delegate_output!(AppState);
delegate_registry!(AppState);
