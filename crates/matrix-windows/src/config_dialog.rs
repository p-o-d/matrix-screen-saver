#[cfg(windows)]
mod imp {
    use matrix_core::config::{CharsetKind, Config};
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{COLORREF, HMODULE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{GetStockObject, HBRUSH, WHITE_BRUSH};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Controls::{
        BST_CHECKED, BST_UNCHECKED, DLG_BUTTON_CHECK_STATE, TBM_SETPOS, TBM_SETRANGE,
    };
    use windows::Win32::UI::Controls::Dialogs::{
        ChooseColorW, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX, BS_DEFPUSHBUTTON, BS_PUSHBUTTON,
        CB_ADDSTRING, CB_GETCURSEL, CB_SETCURSEL, CBS_DROPDOWNLIST, CreateWindowExW,
        DefWindowProcW, DestroyWindow, DispatchMessageW, GetDlgItem, GetMessageW, HMENU,
        IDCANCEL, IDOK, IDC_ARROW, LoadCursorW, MSG, PostQuitMessage, RegisterClassExW,
        SendMessageW, ShowWindow, SW_SHOW, TranslateMessage, WM_COMMAND, WM_DESTROY,
        WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN,
        WS_EX_APPWINDOW, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
    };

    // Control IDs
    const IDC_FONT_SIZE: i32    = 1001;
    const IDC_SPEED: i32        = 1002;
    const IDC_DENSITY: i32      = 1003;
    const IDC_DROP_MIN: i32     = 1004;
    const IDC_DROP_MAX: i32     = 1005;
    const IDC_DEPTH_LEVELS: i32 = 1006;
    const IDC_DEPTH_SCALE: i32  = 1007;
    const IDC_DEPTH_BRI: i32    = 1008;
    const IDC_CLUSTER: i32      = 1009;
    const IDC_SCANLINE_INT: i32 = 1010;
    const IDC_CHROMA: i32       = 1011;
    const IDC_GLOW_INT: i32     = 1012;
    const IDC_FPS: i32          = 1013;
    const IDC_CHARSET: i32      = 1014;
    const IDC_COLOR: i32        = 1015;
    const IDC_BG_COLOR: i32     = 1016;
    const IDC_GLOW: i32         = 1017;
    const IDC_SCANLINES: i32    = 1018;
    const IDC_DEBUG: i32        = 1019;

    // TBM_GETPOS = WM_USER + 0 = 1024
    const TBM_GETPOS: u32 = 1024;

    static mut CUSTOM_COLORS: [COLORREF; 16] = [COLORREF(0); 16];
    static mut CUSTOM_COLORS_BG: [COLORREF; 16] = [COLORREF(0); 16];

    fn color_to_colorref(hex: &str) -> COLORREF {
        let [r, g, b, _] = Config::parse_color(hex);
        COLORREF(
            ((r * 255.0) as u32)
                | (((g * 255.0) as u32) << 8)
                | (((b * 255.0) as u32) << 16),
        )
    }

    fn colorref_to_hex(cr: COLORREF) -> String {
        format!(
            "#{:02x}{:02x}{:02x}",
            cr.0 & 0xff,
            (cr.0 >> 8) & 0xff,
            (cr.0 >> 16) & 0xff,
        )
    }

    fn save_config(config: &Config) {
        let dir = dirs::config_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("matrix-screensaver");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("config.toml");
        match toml::to_string(config) {
            Ok(content) => { std::fs::write(path, content).ok(); }
            Err(e) => eprintln!("Failed to serialize config: {e}"),
        }
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn ws_with(base: WINDOW_STYLE, extra: i32) -> WINDOW_STYLE {
        WINDOW_STYLE(base.0 | extra as u32)
    }

    // label: left-aligned static text
    unsafe fn lbl(hwnd: HWND, hi: HMODULE, text: &str, x: i32, y: i32) {
        let wide = to_wide(text);
        let _ = CreateWindowExW(
            WINDOW_EX_STYLE(0), w!("STATIC"), PCWSTR(wide.as_ptr()),
            WS_CHILD | WS_VISIBLE, x, y, 110, 16,
            hwnd, HMENU(std::ptr::null_mut()), hi, None,
        );
    }

    // trackbar: x=125, w=200, label at x=10
    // range: MAKELONG(min, max) → LPARAM((max<<16)|(min&0xFFFF))
    unsafe fn tbr(hwnd: HWND, hi: HMODULE, id: i32, y: i32, min: isize, max: isize, pos: isize) {
        let _ = CreateWindowExW(
            WINDOW_EX_STYLE(0), w!("msctls_trackbar32"), PCWSTR::null(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP, 125, y, 200, 26,
            hwnd, HMENU(id as isize as *mut _), hi, None,
        );
        if let Ok(h) = GetDlgItem(hwnd, id) {
            SendMessageW(h, TBM_SETRANGE, WPARAM(1), LPARAM((max << 16) | (min & 0xFFFF)));
            SendMessageW(h, TBM_SETPOS,   WPARAM(1), LPARAM(pos.clamp(min, max)));
        }
    }

    unsafe fn tbr_get(hwnd: HWND, id: i32) -> isize {
        GetDlgItem(hwnd, id)
            .map(|h| SendMessageW(h, TBM_GETPOS, WPARAM(0), LPARAM(0)).0)
            .unwrap_or(0)
    }

    unsafe fn chk_set(hwnd: HWND, id: i32, checked: bool) {
        if let Ok(h) = GetDlgItem(hwnd, id) {
            let s: DLG_BUTTON_CHECK_STATE = if checked { BST_CHECKED } else { BST_UNCHECKED };
            SendMessageW(h, BM_SETCHECK, WPARAM(s.0 as usize), LPARAM(0));
        }
    }

    unsafe fn chk_get(hwnd: HWND, id: i32) -> bool {
        GetDlgItem(hwnd, id)
            .map(|h| SendMessageW(h, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == BST_CHECKED.0 as isize)
            .unwrap_or(false)
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            windows::Win32::UI::WindowsAndMessaging::WM_CREATE => {
                let cfg = Config::load();
                CUSTOM_COLORS[0]    = color_to_colorref(&cfg.colors.primary);
                CUSTOM_COLORS_BG[0] = color_to_colorref(&cfg.colors.background);

                let hi = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();

                // ── Trackbar rows (label at y+8, trackbar at y, row=38px) ──────────
                lbl(hwnd, hi, "Font size:",       10, 18);
                tbr(hwnd, hi, IDC_FONT_SIZE,    10,  10, 120, cfg.display.font_size.clamp(10.0, 120.0) as isize);

                lbl(hwnd, hi, "Speed:",           10, 56);
                tbr(hwnd, hi, IDC_SPEED,        48,   0, 200, (cfg.rain.speed * 100.0) as isize);

                lbl(hwnd, hi, "Density:",         10, 94);
                tbr(hwnd, hi, IDC_DENSITY,      86,   0, 100, (cfg.rain.density * 100.0) as isize);

                lbl(hwnd, hi, "Drop len. min:",   10, 132);
                tbr(hwnd, hi, IDC_DROP_MIN,    124,   1,  50, cfg.rain.drop_length_min.clamp(1, 50) as isize);

                lbl(hwnd, hi, "Drop len. max:",   10, 170);
                tbr(hwnd, hi, IDC_DROP_MAX,    162,   1, 100, cfg.rain.drop_length_max.clamp(1, 100) as isize);

                lbl(hwnd, hi, "Depth levels:",    10, 208);
                tbr(hwnd, hi, IDC_DEPTH_LEVELS,200,   1,  10, cfg.rain.depth_levels.clamp(1, 10) as isize);

                lbl(hwnd, hi, "Depth scale:",     10, 246);
                tbr(hwnd, hi, IDC_DEPTH_SCALE, 238,  10, 100, (cfg.rain.depth_scale_min * 100.0) as isize);

                lbl(hwnd, hi, "Depth bright.:",   10, 284);
                tbr(hwnd, hi, IDC_DEPTH_BRI,   276,   0, 100, (cfg.rain.depth_brightness_min * 100.0) as isize);

                lbl(hwnd, hi, "Cluster str.:",    10, 322);
                tbr(hwnd, hi, IDC_CLUSTER,     314,   0, 100, (cfg.rain.cluster_strength * 100.0) as isize);

                lbl(hwnd, hi, "Scanline int.:",   10, 360);
                tbr(hwnd, hi, IDC_SCANLINE_INT,352,   0, 100, (cfg.display.scanline_intensity * 100.0) as isize);

                lbl(hwnd, hi, "Chromatic ab.:",   10, 398);
                tbr(hwnd, hi, IDC_CHROMA,      390,   0,  20, (cfg.display.chromatic_aberration * 1000.0) as isize);

                lbl(hwnd, hi, "Glow intensity:",  10, 436);
                tbr(hwnd, hi, IDC_GLOW_INT,    428,   0, 100, (cfg.colors.glow_intensity * 100.0) as isize);

                // ── FPS combobox (y=472) ─────────────────────────────────────────
                lbl(hwnd, hi, "FPS:", 10, 482);
                {
                    let _ = CreateWindowExW(
                        WINDOW_EX_STYLE(0), w!("COMBOBOX"), PCWSTR::null(),
                        ws_with(WS_CHILD | WS_VISIBLE | WS_TABSTOP, CBS_DROPDOWNLIST),
                        125, 474, 120, 120,
                        hwnd, HMENU(IDC_FPS as isize as *mut _), hi, None,
                    );
                    if let Ok(hcb) = GetDlgItem(hwnd, IDC_FPS) {
                        let items: Vec<Vec<u16>> = ["30\0","60\0","120\0"]
                            .iter().map(|s| s.encode_utf16().collect()).collect();
                        for w in &items {
                            SendMessageW(hcb, CB_ADDSTRING, WPARAM(0), LPARAM(w.as_ptr() as isize));
                        }
                        let sel: usize = match cfg.display.fps { 30 => 0, 120 => 2, _ => 1 };
                        SendMessageW(hcb, CB_SETCURSEL, WPARAM(sel), LPARAM(0));
                    }
                }

                // ── Charset combobox (y=518) ─────────────────────────────────────
                lbl(hwnd, hi, "Charset:", 10, 528);
                {
                    let _ = CreateWindowExW(
                        WINDOW_EX_STYLE(0), w!("COMBOBOX"), PCWSTR::null(),
                        ws_with(WS_CHILD | WS_VISIBLE | WS_TABSTOP, CBS_DROPDOWNLIST),
                        125, 520, 120, 120,
                        hwnd, HMENU(IDC_CHARSET as isize as *mut _), hi, None,
                    );
                    if let Ok(hcb) = GetDlgItem(hwnd, IDC_CHARSET) {
                        let items: Vec<Vec<u16>> = ["Mixed\0","Katakana\0","Latin\0","Binary\0"]
                            .iter().map(|s| s.encode_utf16().collect()).collect();
                        for w in &items {
                            SendMessageW(hcb, CB_ADDSTRING, WPARAM(0), LPARAM(w.as_ptr() as isize));
                        }
                        let sel: usize = match cfg.rain.charset {
                            CharsetKind::Mixed => 0, CharsetKind::Katakana => 1,
                            CharsetKind::Latin => 2, CharsetKind::Binary   => 3,
                        };
                        SendMessageW(hcb, CB_SETCURSEL, WPARAM(sel), LPARAM(0));
                    }
                }

                // ── Color buttons (y=562) ────────────────────────────────────────
                {
                    let _ = CreateWindowExW(
                        WINDOW_EX_STYLE(0), w!("BUTTON"), w!("Rain color..."),
                        ws_with(WS_CHILD | WS_VISIBLE | WS_TABSTOP, BS_PUSHBUTTON),
                        10, 562, 130, 28,
                        hwnd, HMENU(IDC_COLOR as isize as *mut _), hi, None,
                    );
                    let _ = CreateWindowExW(
                        WINDOW_EX_STYLE(0), w!("BUTTON"), w!("Background..."),
                        ws_with(WS_CHILD | WS_VISIBLE | WS_TABSTOP, BS_PUSHBUTTON),
                        148, 562, 130, 28,
                        hwnd, HMENU(IDC_BG_COLOR as isize as *mut _), hi, None,
                    );
                }

                // ── Checkboxes row (y=600) ───────────────────────────────────────
                {
                    let _ = CreateWindowExW(
                        WINDOW_EX_STYLE(0), w!("BUTTON"), w!("Glow"),
                        ws_with(WS_CHILD | WS_VISIBLE | WS_TABSTOP, BS_AUTOCHECKBOX),
                        10, 600, 90, 24,
                        hwnd, HMENU(IDC_GLOW as isize as *mut _), hi, None,
                    );
                    chk_set(hwnd, IDC_GLOW, cfg.colors.glow);

                    let _ = CreateWindowExW(
                        WINDOW_EX_STYLE(0), w!("BUTTON"), w!("Scanlines"),
                        ws_with(WS_CHILD | WS_VISIBLE | WS_TABSTOP, BS_AUTOCHECKBOX),
                        108, 600, 110, 24,
                        hwnd, HMENU(IDC_SCANLINES as isize as *mut _), hi, None,
                    );
                    chk_set(hwnd, IDC_SCANLINES, cfg.display.scanlines);

                    let _ = CreateWindowExW(
                        WINDOW_EX_STYLE(0), w!("BUTTON"), w!("Debug overlay"),
                        ws_with(WS_CHILD | WS_VISIBLE | WS_TABSTOP, BS_AUTOCHECKBOX),
                        226, 600, 120, 24,
                        hwnd, HMENU(IDC_DEBUG as isize as *mut _), hi, None,
                    );
                    chk_set(hwnd, IDC_DEBUG, cfg.display.debug_overlay);
                }

                // ── OK / Cancel (y=638) ──────────────────────────────────────────
                {
                    let _ = CreateWindowExW(
                        WINDOW_EX_STYLE(0), w!("BUTTON"), w!("OK"),
                        ws_with(WS_CHILD | WS_VISIBLE | WS_TABSTOP, BS_DEFPUSHBUTTON),
                        155, 638, 80, 28,
                        hwnd, HMENU(IDOK.0 as isize as *mut _), hi, None,
                    );
                    let _ = CreateWindowExW(
                        WINDOW_EX_STYLE(0), w!("BUTTON"), w!("Cancel"),
                        ws_with(WS_CHILD | WS_VISIBLE | WS_TABSTOP, BS_PUSHBUTTON),
                        245, 638, 80, 28,
                        hwnd, HMENU(IDCANCEL.0 as isize as *mut _), hi, None,
                    );
                }

                LRESULT(0)
            }

            WM_COMMAND => {
                let ctrl_id = (wparam.0 & 0xffff) as i32;

                if ctrl_id == IDOK.0 {
                    let mut cfg = Config::load();

                    cfg.display.font_size           = tbr_get(hwnd, IDC_FONT_SIZE) as f32;
                    cfg.rain.speed                  = tbr_get(hwnd, IDC_SPEED)     as f32 / 100.0;
                    cfg.rain.density                = tbr_get(hwnd, IDC_DENSITY)   as f32 / 100.0;
                    cfg.rain.drop_length_min        = (tbr_get(hwnd, IDC_DROP_MIN)    as usize).max(1);
                    cfg.rain.drop_length_max        = (tbr_get(hwnd, IDC_DROP_MAX)    as usize).max(1);
                    cfg.rain.depth_levels           = (tbr_get(hwnd, IDC_DEPTH_LEVELS) as u8).max(1);
                    cfg.rain.depth_scale_min        = tbr_get(hwnd, IDC_DEPTH_SCALE) as f32 / 100.0;
                    cfg.rain.depth_brightness_min   = tbr_get(hwnd, IDC_DEPTH_BRI)   as f32 / 100.0;
                    cfg.rain.cluster_strength       = tbr_get(hwnd, IDC_CLUSTER)      as f32 / 100.0;
                    cfg.display.scanline_intensity  = tbr_get(hwnd, IDC_SCANLINE_INT) as f32 / 100.0;
                    cfg.display.chromatic_aberration= tbr_get(hwnd, IDC_CHROMA)       as f32 / 1000.0;
                    cfg.colors.glow_intensity       = tbr_get(hwnd, IDC_GLOW_INT)     as f32 / 100.0;

                    // ensure drop_min <= drop_max
                    if cfg.rain.drop_length_min > cfg.rain.drop_length_max {
                        cfg.rain.drop_length_max = cfg.rain.drop_length_min;
                    }

                    if let Ok(hcb) = GetDlgItem(hwnd, IDC_FPS) {
                        let sel = SendMessageW(hcb, CB_GETCURSEL, WPARAM(0), LPARAM(0));
                        cfg.display.fps = match sel.0 { 0 => 30, 2 => 120, _ => 60 };
                    }

                    if let Ok(hcb) = GetDlgItem(hwnd, IDC_CHARSET) {
                        let sel = SendMessageW(hcb, CB_GETCURSEL, WPARAM(0), LPARAM(0));
                        cfg.rain.charset = match sel.0 {
                            0 => CharsetKind::Mixed,    1 => CharsetKind::Katakana,
                            2 => CharsetKind::Latin,    3 => CharsetKind::Binary,
                            _ => CharsetKind::Mixed,
                        };
                    }

                    cfg.colors.primary    = colorref_to_hex(CUSTOM_COLORS[0]);
                    cfg.colors.background = colorref_to_hex(CUSTOM_COLORS_BG[0]);
                    cfg.colors.glow       = chk_get(hwnd, IDC_GLOW);
                    cfg.display.scanlines = chk_get(hwnd, IDC_SCANLINES);
                    cfg.display.debug_overlay = chk_get(hwnd, IDC_DEBUG);

                    save_config(&cfg);
                    let _ = DestroyWindow(hwnd);

                } else if ctrl_id == IDCANCEL.0 {
                    let _ = DestroyWindow(hwnd);

                } else if ctrl_id == IDC_COLOR {
                    let mut cc = CHOOSECOLORW {
                        lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
                        hwndOwner: hwnd,
                        rgbResult: CUSTOM_COLORS[0],
                        lpCustColors: std::ptr::addr_of_mut!(CUSTOM_COLORS) as *mut _,
                        Flags: CC_FULLOPEN | CC_RGBINIT,
                        ..Default::default()
                    };
                    if ChooseColorW(&mut cc).as_bool() {
                        CUSTOM_COLORS[0] = cc.rgbResult;
                    }

                } else if ctrl_id == IDC_BG_COLOR {
                    let mut cc = CHOOSECOLORW {
                        lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
                        hwndOwner: hwnd,
                        rgbResult: CUSTOM_COLORS_BG[0],
                        lpCustColors: std::ptr::addr_of_mut!(CUSTOM_COLORS_BG) as *mut _,
                        Flags: CC_FULLOPEN | CC_RGBINIT,
                        ..Default::default()
                    };
                    if ChooseColorW(&mut cc).as_bool() {
                        CUSTOM_COLORS_BG[0] = cc.rgbResult;
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

    pub fn run() {
        unsafe {
            let hinstance = match GetModuleHandleW(PCWSTR::null()) {
                Ok(h) => h,
                Err(_) => return,
            };

            let class_name = w!("MatrixConfigDialog");
            let hcursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();

            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wnd_proc),
                hInstance: hinstance.into(),
                lpszClassName: class_name,
                hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
                hCursor: hcursor,
                ..Default::default()
            };
            RegisterClassExW(&wc);

            let hwnd = match CreateWindowExW(
                WS_EX_APPWINDOW,
                class_name,
                w!("Matrix Screensaver Settings"),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN | WS_VISIBLE,
                100, 100, 360, 720,
                None, None, hinstance, None,
            ) {
                Ok(h) => h,
                Err(_) => return,
            };

            let _ = ShowWindow(hwnd, SW_SHOW);

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

#[cfg(windows)]
pub use imp::run;

#[cfg(not(windows))]
pub fn run() {
    eprintln!("config_dialog::run() is only supported on Windows");
}
