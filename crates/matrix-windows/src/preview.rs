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
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, MSG,
    PeekMessageW, RegisterClassExW, TranslateMessage, WNDCLASSEXW, CS_HREDRAW, CS_VREDRAW,
    PM_REMOVE, WM_QUIT, WINDOW_EX_STYLE, WS_CHILD, WS_VISIBLE,
};
use windows::core::PCWSTR;

pub fn run(parent_hwnd_raw: isize) {
    if parent_hwnd_raw == 0 {
        return;
    }
    let parent = HWND(parent_hwnd_raw as *mut core::ffi::c_void);

    let config = Config::load();
    let charset = get_charset(&config.rain.charset);
    let font_bytes = crate::font::load_font(&config.rain.charset);
    let atlas = Arc::new(GlyphAtlas::build(
        &charset,
        config.display.font_size as f32,
        &font_bytes,
    ));
    let frame_duration = Duration::from_secs_f64(1.0 / config.display.fps as f64);

    // Get parent window client size
    let (width, height) = {
        let mut rect = RECT::default();
        if unsafe { GetClientRect(parent, &mut rect) }.is_err() {
            return;
        }
        let w = (rect.right - rect.left).max(1) as u32;
        let h = (rect.bottom - rect.top).max(1) as u32;
        (w, h)
    };

    // Create child window inside parent
    let hwnd = create_child_window(parent, width, height);

    // Create wgpu instance + surface from child HWND
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
            .expect("preview surface creation failed")
    };

    // Initialise renderer
    let mut renderer = pollster::block_on(Renderer::new(
        instance,
        surface,
        width,
        height,
        atlas.clone(),
        &config,
        None,
        None,
    ));

    // Build rain simulators for each depth level
    let levels = depth_levels(&config.rain);
    let mut rains = make_rains(width, height, &atlas, &levels, &charset, &config.rain);

    let mut last_frame = Instant::now();

    // Main loop — runs until the preview dialog is closed (WM_QUIT)
    loop {
        // Drain pending messages
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
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        let now = Instant::now();
        let elapsed = now.duration_since(last_frame);

        if elapsed >= frame_duration {
            let dt = elapsed.as_secs_f32().min(0.1);
            last_frame = now;

            for sim in &mut rains {
                sim.update(dt);
            }

            let depth_layers: Vec<DepthLayer<'_>> = rains
                .iter()
                .zip(levels.iter())
                .map(|(sim, &(scale, bri))| DepthLayer {
                    cells: &sim.cells,
                    scale,
                    brightness_mult: bri,
                })
                .collect();

            renderer.render(&depth_layers);
        } else {
            let remaining = frame_duration - elapsed;
            let sleep_ms = (remaining.as_millis() as u64).min(16);
            std::thread::sleep(Duration::from_millis(sleep_ms));
        }
    }
}

fn create_child_window(parent: HWND, w: u32, h: u32) -> HWND {
    unsafe {
        let hinstance = GetModuleHandleW(None).expect("GetModuleHandleW failed");
        let class_name: Vec<u16> = "MatrixPreview\0".encode_utf16().collect();

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(preview_wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassExW(&wc);

        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(std::ptr::null()),
            WS_CHILD | WS_VISIBLE,
            0,
            0,
            w as i32,
            h as i32,
            parent,
            None,
            hinstance,
            None,
        )
        .expect("CreateWindowExW for preview child failed")
    }
}

unsafe extern "system" fn preview_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
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
