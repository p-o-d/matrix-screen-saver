use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use wayland_client::{globals::registry_queue_init, Connection};

use matrix_screensaver::config::Config;
use matrix_screensaver::chars::get_charset;
use matrix_screensaver::rain::RainSimulator;
use matrix_screensaver::atlas::GlyphAtlas;
use matrix_screensaver::renderer::Renderer;
use matrix_screensaver::wayland_app::{AppEvent, AppState};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let test_mode = std::env::args().any(|a| a == "--test");
    let config = Config::load();

    // For katakana/mixed, request a font with Japanese support
    let font_family = match &config.rain.charset {
        matrix_screensaver::config::CharsetKind::Katakana
        | matrix_screensaver::config::CharsetKind::Mixed => {
            format!("{}:lang=ja", config.display.font)
        }
        _ => config.display.font.clone(),
    };

    let charset = get_charset(&config.rain.charset);
    let atlas = Arc::new(GlyphAtlas::build(&charset, config.display.font_size, &font_family));
    let frame_duration = Duration::from_secs_f64(1.0 / config.display.fps as f64);

    // Wayland connection
    let conn = Connection::connect_to_env()
        .expect("Wayland connection failed — is WAYLAND_DISPLAY set?");
    let (globals, event_queue) = registry_queue_init(&conn).expect("registry_queue_init failed");
    let qh = event_queue.handle();

    let (event_tx, event_rx) = mpsc::channel::<AppEvent>();

    let mut app_state = AppState::new(&globals, qh.clone(), &config, event_tx)
        .expect("AppState init failed");

    let mut event_loop: EventLoop<AppState> = EventLoop::try_new().expect("event loop failed");
    WaylandSource::new(conn.clone(), event_queue)
        .insert(event_loop.handle())
        .expect("WaylandSource insert failed");

    let mut renderers: Vec<Option<Renderer>> = Vec::new();
    let mut rains: Vec<Option<RainSimulator>> = Vec::new();
    let mut screensaver_active = false;
    let mut last_frame = Instant::now();

    // Initial roundtrip to populate globals (outputs, seats)
    event_loop
        .dispatch(Some(Duration::from_millis(200)), &mut app_state)
        .unwrap();

    // Subscribe idle notification using the first available seat
    try_subscribe_idle(&globals, &mut app_state, &qh, &config);

    loop {
        // Process Wayland events (non-blocking, 1 ms timeout)
        event_loop
            .dispatch(Some(Duration::from_millis(1)), &mut app_state)
            .unwrap();

        // Process app events from the channel
        while let Ok(event) = event_rx.try_recv() {
            match event {
                AppEvent::Idle if !screensaver_active => {
                    tracing::info!("idle: activating screensaver");
                    app_state.create_layer_surfaces_all();
                    let n = app_state.surfaces.len();
                    renderers.resize_with(n, || None);
                    rains.resize_with(n, || None);
                    screensaver_active = true;
                }
                AppEvent::Resume | AppEvent::Dismiss if screensaver_active => {
                    tracing::info!("resume/dismiss: deactivating screensaver");
                    app_state.destroy_layer_surfaces();
                    renderers.clear();
                    rains.clear();
                    screensaver_active = false;
                }
                AppEvent::Resize(idx, w, h) if screensaver_active => {
                    if idx >= renderers.len() {
                        renderers.resize_with(idx + 1, || None);
                        rains.resize_with(idx + 1, || None);
                    }
                    if let Some(r) = &mut renderers[idx] {
                        r.resize(w, h);
                        let cols = (w / atlas.cell_width).max(1) as usize;
                        let rows = (h / atlas.cell_height).max(1) as usize;
                        rains[idx] = Some(RainSimulator::new(cols, rows, charset.clone(), &config.rain));
                    } else if app_state.surfaces.get(idx).map_or(false, |s| s.configured) {
                        let display_ptr = get_display_ptr(&conn);
                        let surface_ptr = get_surface_ptr(&app_state.surfaces[idx].wl_surface);
                        let r = pollster::block_on(Renderer::new(
                            display_ptr, surface_ptr, w, h, atlas.clone(), &config,
                        ));
                        let cols = (w / atlas.cell_width).max(1) as usize;
                        let rows = (h / atlas.cell_height).max(1) as usize;
                        rains[idx] = Some(RainSimulator::new(cols, rows, charset.clone(), &config.rain));
                        renderers[idx] = Some(r);
                    }
                }
                _ => {}
            }
        }

        // Render frame when active
        if screensaver_active {
            let now = Instant::now();
            let delta = now.duration_since(last_frame).as_secs_f32().min(0.1);
            last_frame = now;
            let mut rendered_any = false;
            for i in 0..renderers.len() {
                if let (Some(r), Some(sim)) = (&mut renderers[i], &mut rains[i]) {
                    sim.update(delta);
                    r.render(&sim.cells);
                    rendered_any = true;
                }
            }
            if rendered_any && test_mode {
                tracing::info!("--test: rendered one frame, exiting 0");
                std::process::exit(0);
            }
            let elapsed = Instant::now().duration_since(now);
            if elapsed < frame_duration {
                std::thread::sleep(frame_duration - elapsed);
            }
        } else if test_mode && !screensaver_active {
            // --test: bypass idle, force activate immediately
            tracing::info!("--test: forcing screensaver activation");
            app_state.create_layer_surfaces_all();
            let n = app_state.surfaces.len();
            renderers.resize_with(n, || None);
            rains.resize_with(n, || None);
            screensaver_active = true;
            // Dispatch until first surface is configured (up to 1 second)
            let deadline = Instant::now() + Duration::from_secs(1);
            while app_state.surfaces.iter().all(|s| !s.configured) && Instant::now() < deadline {
                event_loop.dispatch(Some(Duration::from_millis(50)), &mut app_state).unwrap();
            }
            // Resend Resize for each configured surface
            for (idx, slot) in app_state.surfaces.iter().enumerate() {
                if let Some((w, h)) = slot.last_size {
                    let _ = app_state.event_tx.send(AppEvent::Resize(idx, w, h));
                }
            }
        } else {
            last_frame = Instant::now();
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Bind a `wl_seat` from the global list and register an idle-notification with the
/// compositor's `ext-idle-notify-v1` object (if available).
fn try_subscribe_idle(
    globals: &wayland_client::globals::GlobalList,
    app_state: &mut AppState,
    qh: &wayland_client::QueueHandle<AppState>,
    config: &Config,
) {
    // Bind a wl_seat so we can hand it to ext-idle-notify-v1.
    // AppState implements Dispatch<WlSeat, ()> with a no-op handler.
    if let Ok(seat) = globals.bind::<wayland_client::protocol::wl_seat::WlSeat, _, _>(
        qh,
        1..=9,
        (),
    ) {
        let timeout_ms = (config.idle.timeout_seconds * 1000)
            .try_into()
            .unwrap_or(u32::MAX);
        app_state.request_idle_notification(&seat, timeout_ms);
    }
}

/// Get the raw `wl_display` pointer from the Wayland connection.
///
/// Requires `wayland-backend` with the `client_system` feature (which links against
/// libwayland-client and exposes the underlying C pointer).
fn get_display_ptr(conn: &Connection) -> *mut std::ffi::c_void {
    conn.backend().display_ptr() as *mut std::ffi::c_void
}

/// Get the raw `wl_proxy` pointer for a `wl_surface`.
///
/// `ObjectId::as_ptr()` is available when the `client_system` feature of
/// `wayland-backend` is enabled.
fn get_surface_ptr(
    wl_surface: &wayland_client::protocol::wl_surface::WlSurface,
) -> *mut std::ffi::c_void {
    use wayland_client::Proxy;
    wl_surface.id().as_ptr() as *mut std::ffi::c_void
}
