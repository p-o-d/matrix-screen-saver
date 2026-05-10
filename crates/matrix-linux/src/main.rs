use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use wayland_client::{globals::registry_queue_init, Connection};

use matrix_core::config::Config;
use matrix_core::chars::get_charset;
use matrix_core::rain::RainSimulator;
use matrix_core::atlas::GlyphAtlas;
use matrix_core::renderer::Renderer;
use crate::stats::{GpuSpec, SystemStats, start_stats_poller};
use crate::wayland_app::{AppEvent, AppState};

mod font;
mod stats;
mod wayland_app;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let test_mode = std::env::args().any(|a| a == "--test");
    let config = Config::load();

    // For katakana/mixed, request a font with Japanese support
    let font_family = match &config.rain.charset {
        matrix_core::config::CharsetKind::Katakana
        | matrix_core::config::CharsetKind::Mixed => {
            format!("{}:lang=ja", config.display.font)
        }
        _ => config.display.font.clone(),
    };

    let charset = get_charset(&config.rain.charset);
    let font_bytes = font::find_font(&font_family);
    let atlas = Arc::new(GlyphAtlas::build(&charset, config.display.font_size, &font_bytes));
    let frame_duration = Duration::from_secs_f64(1.0 / config.display.fps as f64);

    // Debug overlay: build fixed ASCII atlas + spawn stats poller
    let gpu_hint: Arc<Mutex<Option<GpuSpec>>> = Arc::new(Mutex::new(None));
    let (debug_atlas, debug_stats): (Option<Arc<GlyphAtlas>>, Option<Arc<Mutex<SystemStats>>>) =
        if config.display.debug_overlay {
            let debug_chars: Vec<char> = (0x20u8..=0x7eu8).map(|b| b as char)
                .chain(['█', '░'])
                .collect();
            let debug_font_bytes = font::find_font(&config.display.font);
            let da = Arc::new(GlyphAtlas::build(&debug_chars, 14.0, &debug_font_bytes));
            let ds = start_stats_poller(gpu_hint.clone());
            (Some(da), Some(ds))
        } else {
            (None, None)
        };

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

    let levels = depth_levels(&config.rain);
    let mut renderers: Vec<Option<Renderer>> = Vec::new();
    // Outer: per screen. Inner: one RainSimulator per depth level, far (0) → near (last).
    let mut rains: Vec<Vec<RainSimulator>> = Vec::new();
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
                    rains.resize_with(n, Vec::new);
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
                        rains.resize_with(idx + 1, Vec::new);
                    }
                    let screen_rains = make_rains(w, h, &atlas, &levels, &charset, &config.rain);
                    if let Some(r) = &mut renderers[idx] {
                        r.resize(w, h);
                        rains[idx] = screen_rains;
                    } else if app_state.surfaces.get(idx).map_or(false, |s| s.configured) {
                        let (wgpu_instance, wgpu_surface) = create_wgpu_surface(
                            &conn,
                            &app_state.surfaces[idx].wl_surface,
                        );
                        let r = pollster::block_on(Renderer::new(
                            wgpu_instance, wgpu_surface, w, h, atlas.clone(), &config,
                            debug_atlas.clone(), debug_stats.clone(),
                        ));
                        if debug_stats.is_some() {
                            if let Ok(mut hint) = gpu_hint.lock() {
                                if hint.is_none() {
                                    *hint = Some(GpuSpec {
                                        vendor: r.adapter_info.vendor,
                                        device: r.adapter_info.device,
                                    });
                                }
                            }
                        }
                        rains[idx] = screen_rains;
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
                if let Some(r) = &mut renderers[i] {
                    if rains[i].is_empty() { continue; }
                    for sim in &mut rains[i] {
                        sim.update(delta);
                    }
                    let depth_layers: Vec<matrix_core::renderer::DepthLayer<'_>> = rains[i]
                        .iter()
                        .zip(levels.iter())
                        .map(|(sim, &(scale, brightness_mult))| {
                            matrix_core::renderer::DepthLayer {
                                cells: &sim.cells,
                                scale,
                                brightness_mult,
                            }
                        })
                        .collect();
                    r.render(&depth_layers);
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
            rains.resize_with(n, Vec::new);
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

/// Compute per-level (scale, brightness_mult) pairs, far (index 0) → near (index N-1).
fn depth_levels(config: &matrix_core::config::RainConfig) -> Vec<(f32, f32)> {
    let n = (config.depth_levels as usize).max(1);
    (0..n).map(|i| {
        let t = if n == 1 { 1.0 } else { i as f32 / (n - 1) as f32 };
        let scale = config.depth_scale_min + (1.0 - config.depth_scale_min) * t;
        let bri = config.depth_brightness_min + (1.0 - config.depth_brightness_min) * t;
        (scale, bri)
    }).collect()
}

/// Build one RainSimulator per depth level for a given screen size.
fn make_rains(
    w: u32, h: u32,
    atlas: &matrix_core::atlas::GlyphAtlas,
    levels: &[(f32, f32)],
    charset: &[char],
    config: &matrix_core::config::RainConfig,
) -> Vec<RainSimulator> {
    let cw = atlas.cell_width as f32;
    let ch = atlas.cell_height as f32;
    levels.iter().map(|&(scale, _)| {
        let cols = ((w as f32 / (cw * scale)) as usize).max(1);
        let rows = ((h as f32 / (ch * scale)) as usize).max(1);
        RainSimulator::new(cols, rows, charset.to_vec(), config)
    }).collect()
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

fn create_wgpu_surface(
    conn: &Connection,
    wl_surface: &wayland_client::protocol::wl_surface::WlSurface,
) -> (wgpu::Instance, wgpu::Surface<'static>) {
    use raw_window_handle::{
        RawDisplayHandle, RawWindowHandle,
        WaylandDisplayHandle, WaylandWindowHandle,
    };
    use std::ptr::NonNull;

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN | wgpu::Backends::GL,
        ..Default::default()
    });

    let display_ptr = conn.backend().display_ptr() as *mut std::ffi::c_void;
    let surface_ptr = {
        use wayland_client::Proxy;
        wl_surface.id().as_ptr() as *mut std::ffi::c_void
    };

    let surface = unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: RawDisplayHandle::Wayland(
                WaylandDisplayHandle::new(NonNull::new(display_ptr).unwrap()),
            ),
            raw_window_handle: RawWindowHandle::Wayland(
                WaylandWindowHandle::new(NonNull::new(surface_ptr).unwrap()),
            ),
        })
    }
    .expect("wgpu surface creation failed");

    (instance, surface)
}
