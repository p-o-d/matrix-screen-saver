use std::num::NonZeroIsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use matrix_core::{
    atlas::GlyphAtlas,
    chars::get_charset,
    config::Config,
    rain::RainSimulator,
    renderer::{DepthLayer, Renderer},
};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
    GetSystemMetrics, MSG, PeekMessageW, PostQuitMessage, RegisterClassExW, ShowWindow,
    TranslateMessage, WNDCLASSEXW, CS_HREDRAW, CS_VREDRAW, PM_REMOVE,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SW_SHOW, WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_QUIT, WM_RBUTTONDOWN,
    WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};
use windows::core::PCWSTR;

/// Global mutable state shared between the WndProc and the event loop.
/// We use a raw static to avoid passing it through the lparam/userdata dance for simplicity.
static mut MOUSE_START: (i32, i32) = (0, 0);
static mut MOUSE_INITIALIZED: bool = false;
static mut MOUSE_MOVED_FAR: bool = false;
static mut SHOULD_QUIT: bool = false;

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_KEYDOWN | WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
            SHOULD_QUIT = true;
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            if !MOUSE_INITIALIZED {
                MOUSE_START = (x, y);
                MOUSE_INITIALIZED = true;
            } else if !MOUSE_MOVED_FAR {
                let dx = x - MOUSE_START.0;
                let dy = y - MOUSE_START.1;
                if dx * dx + dy * dy > 10 * 10 {
                    MOUSE_MOVED_FAR = true;
                    SHOULD_QUIT = true;
                    PostQuitMessage(0);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn depth_levels(config: &matrix_core::config::RainConfig) -> Vec<(f32, f32)> {
    let n = (config.depth_levels as usize).max(1);
    (0..n)
        .map(|i| {
            let t = if n == 1 {
                1.0
            } else {
                i as f32 / (n - 1) as f32
            };
            let scale = config.depth_scale_min + (1.0 - config.depth_scale_min) * t;
            let bri = config.depth_brightness_min + (1.0 - config.depth_brightness_min) * t;
            (scale, bri)
        })
        .collect()
}

fn make_rains(
    w: u32,
    h: u32,
    atlas: &GlyphAtlas,
    levels: &[(f32, f32)],
    charset: &[char],
    config: &matrix_core::config::RainConfig,
) -> Vec<RainSimulator> {
    let cw = atlas.cell_width as f32;
    let ch = atlas.cell_height as f32;
    levels
        .iter()
        .map(|&(scale, _)| {
            let cols = ((w as f32 / (cw * scale)) as usize).max(1);
            let rows = ((h as f32 / (ch * scale)) as usize).max(1);
            RainSimulator::new(cols, rows, charset.to_vec(), config)
        })
        .collect()
}

pub fn run() {
    // Load config
    let config = Config::load();

    // Build glyph atlas
    let charset = get_charset(&config.rain.charset);
    let font_bytes = crate::font::load_font(&config.rain.charset);
    let atlas = Arc::new(GlyphAtlas::build(
        &charset,
        config.display.font_size as f32,
        &font_bytes,
    ));

    // Register window class
    let class_name: Vec<u16> = "MatrixScreensaver\0".encode_utf16().collect();

    let hinstance = unsafe {
        GetModuleHandleW(None).expect("GetModuleHandleW failed")
    };

    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wnd_proc),
        hInstance: hinstance.into(),
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };

    unsafe {
        RegisterClassExW(&wc);
    }

    // Query virtual desktop spanning all monitors
    let (virt_x, virt_y, screen_w, screen_h) = unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        (x, y, w as u32, h as u32)
    };

    // Create window spanning all monitors via virtual screen coordinates
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST,
            PCWSTR(class_name.as_ptr()),
            PCWSTR("Matrix Screensaver\0".encode_utf16().collect::<Vec<u16>>().as_ptr()),
            WS_POPUP | WS_VISIBLE,
            virt_x,
            virt_y,
            screen_w as i32,
            screen_h as i32,
            None,
            None,
            hinstance,
            None,
        )
        .expect("CreateWindowExW failed")
    };

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
    }

    // Determine actual client rect (may differ from screen_w/h in theory)
    let (w, h) = unsafe {
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let w = (rect.right - rect.left) as u32;
        let h = (rect.bottom - rect.top) as u32;
        if w == 0 || h == 0 {
            (screen_w, screen_h)
        } else {
            (w, h)
        }
    };

    // Create wgpu instance + surface from HWND
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12 | wgpu::Backends::VULKAN,
        ..Default::default()
    });

    let surface = unsafe {
        instance
            .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
                raw_window_handle: RawWindowHandle::Win32(Win32WindowHandle::new(
                    NonZeroIsize::new(hwnd.0 as isize).unwrap(),
                )),
            })
            .expect("wgpu surface failed")
    };

    // Conditionally build debug atlas and start stats poller
    let (debug_atlas, stats) = if config.display.debug_overlay {
        let debug_font_bytes = crate::font::load_font(&config.rain.charset);
        let debug_charset: Vec<char> = (32u8..=126u8).map(|c| c as char).collect();
        let dbg_atlas = Arc::new(matrix_core::atlas::GlyphAtlas::build(
            &debug_charset,
            14.0,
            &debug_font_bytes,
        ));
        let s = crate::stats::start_stats_poller();
        (Some(dbg_atlas), Some(s))
    } else {
        (None, None)
    };

    // Initialise renderer
    let mut renderer = pollster::block_on(Renderer::new(
        instance,
        surface,
        w,
        h,
        atlas.clone(),
        &config,
        debug_atlas,
        stats,
    ));

    // Build rain simulators for each depth level
    let levels = depth_levels(&config.rain);
    let mut rains = make_rains(w, h, &atlas, &levels, &charset, &config.rain);

    // FPS timing
    let frame_duration = Duration::from_secs_f64(1.0 / config.display.fps as f64);
    let mut last_frame = Instant::now();

    // Main loop
    loop {
        // Drain all pending messages
        loop {
            let mut msg = MSG::default();
            let got = unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() };
            if !got {
                break;
            }
            if msg.message == WM_QUIT {
                unsafe { DestroyWindow(hwnd).ok() };
                return;
            }
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        // Check quit flag set by WndProc
        if unsafe { SHOULD_QUIT } {
            unsafe { DestroyWindow(hwnd).ok() };
            return;
        }

        let now = Instant::now();
        let elapsed = now.duration_since(last_frame);

        if elapsed >= frame_duration {
            let dt = elapsed.as_secs_f32().min(0.1);
            last_frame = now;

            // Step simulation
            for rain in &mut rains {
                rain.update(dt);
            }

            // Build depth layers
            let depth_layers: Vec<DepthLayer<'_>> = rains
                .iter()
                .zip(levels.iter())
                .map(|(rain, &(scale, bri))| DepthLayer {
                    cells: &rain.cells,
                    scale,
                    brightness_mult: bri,
                })
                .collect();

            renderer.render(&depth_layers);
        } else {
            // Sleep to avoid spinning the CPU
            let remaining = frame_duration - elapsed;
            let sleep_ms = (remaining.as_millis() as u64).min(16);
            std::thread::sleep(Duration::from_millis(sleep_ms));
        }
    }
}
