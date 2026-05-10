#[cfg(windows)]
mod imp {
    use matrix_core::config::{CharsetKind, Config};
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{GetStockObject, HBRUSH, WHITE_BRUSH};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Controls::{
        BST_CHECKED, BST_UNCHECKED, DLG_BUTTON_CHECK_STATE,
        TBM_SETPOS, TBM_SETRANGE,
    };
    use windows::Win32::UI::Controls::Dialogs::{
        ChooseColorW, CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        BM_GETCHECK, BM_SETCHECK, BS_AUTOCHECKBOX, BS_DEFPUSHBUTTON, BS_PUSHBUTTON,
        CB_ADDSTRING, CB_GETCURSEL, CB_SETCURSEL, CBS_DROPDOWNLIST, CreateWindowExW,
        DefWindowProcW, DestroyWindow, DispatchMessageW, GetDlgItem, GetMessageW,
        HMENU, IDCANCEL, IDOK, LoadCursorW, MSG, PostQuitMessage, RegisterClassExW,
        SendMessageW, ShowWindow, SW_SHOW, TranslateMessage, WM_COMMAND, WM_DESTROY,
        WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_EX_APPWINDOW,
        WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, IDC_ARROW, WINDOW_STYLE,
    };

    // Control IDs
    const IDC_SPEED: i32 = 1001;
    const IDC_DENSITY: i32 = 1002;
    const IDC_FPS: i32 = 1003;
    const IDC_CHARSET: i32 = 1004;
    const IDC_COLOR: i32 = 1005;
    const IDC_GLOW: i32 = 1006;

    // TBM_GETPOS = WM_USER + 0 = 1024 (not exported by windows 0.52)
    const TBM_GETPOS: u32 = 1024u32;

    static mut CUSTOM_COLORS: [COLORREF; 16] = [COLORREF(0u32); 16];

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
            Err(e) => { eprintln!("Failed to serialize config: {e}"); }
        }
    }

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Combine a WINDOW_STYLE with an i32 style constant (like BS_AUTOCHECKBOX).
    fn ws_with(base: WINDOW_STYLE, extra: i32) -> WINDOW_STYLE {
        WINDOW_STYLE(base.0 | extra as u32)
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            windows::Win32::UI::WindowsAndMessaging::WM_CREATE => {
                let config = Config::load();

                // Initialize primary color into custom colors slot 0
                CUSTOM_COLORS[0] = color_to_colorref(&config.colors.primary);

                let hinstance = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();

                // Helper: create a STATIC text label
                let make_label = |text: &str, x: i32, y: i32, w: i32, h: i32| {
                    let wide = to_wide(text);
                    let _ = CreateWindowExW(
                        windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
                        w!("STATIC"),
                        PCWSTR(wide.as_ptr()),
                        WS_CHILD | WS_VISIBLE,
                        x, y, w, h,
                        hwnd,
                        HMENU(std::ptr::null_mut()),
                        hinstance,
                        None,
                    );
                };

                // ── Speed trackbar (0–200 maps to 0.0–2.0) ──────────────────
                make_label("Speed:", 10, 15, 80, 20);
                {
                    let _ = CreateWindowExW(
                        windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
                        w!("msctls_trackbar32"),
                        PCWSTR::null(),
                        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                        95, 10, 220, 30,
                        hwnd,
                        HMENU(IDC_SPEED as isize as *mut _),
                        hinstance,
                        None,
                    );
                    if let Ok(htb) = GetDlgItem(hwnd, IDC_SPEED) {
                        // LPARAM high word = max, low word = min
                        SendMessageW(htb, TBM_SETRANGE, WPARAM(1), LPARAM(200 << 16));
                        let pos = (config.rain.speed * 100.0) as isize;
                        SendMessageW(htb, TBM_SETPOS, WPARAM(1), LPARAM(pos));
                    }
                }

                // ── Density trackbar (0–100 maps to 0.0–1.0) ────────────────
                make_label("Density:", 10, 60, 80, 20);
                {
                    let _ = CreateWindowExW(
                        windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
                        w!("msctls_trackbar32"),
                        PCWSTR::null(),
                        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                        95, 55, 220, 30,
                        hwnd,
                        HMENU(IDC_DENSITY as isize as *mut _),
                        hinstance,
                        None,
                    );
                    if let Ok(htb) = GetDlgItem(hwnd, IDC_DENSITY) {
                        SendMessageW(htb, TBM_SETRANGE, WPARAM(1), LPARAM(100 << 16));
                        let pos = (config.rain.density * 100.0) as isize;
                        SendMessageW(htb, TBM_SETPOS, WPARAM(1), LPARAM(pos));
                    }
                }

                // ── FPS combobox ─────────────────────────────────────────────
                make_label("FPS:", 10, 103, 80, 20);
                {
                    let _ = CreateWindowExW(
                        windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
                        w!("COMBOBOX"),
                        PCWSTR::null(),
                        ws_with(WS_CHILD | WS_VISIBLE | WS_TABSTOP, CBS_DROPDOWNLIST),
                        95, 100, 120, 120,
                        hwnd,
                        HMENU(IDC_FPS as isize as *mut _),
                        hinstance,
                        None,
                    );
                    if let Ok(hcb) = GetDlgItem(hwnd, IDC_FPS) {
                        let fps_items: Vec<Vec<u16>> = ["30\0", "60\0", "120\0"]
                            .iter().map(|s| s.encode_utf16().collect())
                            .collect();
                        for wide in &fps_items {
                            SendMessageW(hcb, CB_ADDSTRING, WPARAM(0), LPARAM(wide.as_ptr() as isize));
                        }
                        let sel: usize = match config.display.fps {
                            30 => 0,
                            120 => 2,
                            _ => 1,
                        };
                        SendMessageW(hcb, CB_SETCURSEL, WPARAM(sel), LPARAM(0));
                    }
                }

                // ── Charset combobox ─────────────────────────────────────────
                make_label("Charset:", 10, 148, 80, 20);
                {
                    let _ = CreateWindowExW(
                        windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
                        w!("COMBOBOX"),
                        PCWSTR::null(),
                        ws_with(WS_CHILD | WS_VISIBLE | WS_TABSTOP, CBS_DROPDOWNLIST),
                        95, 145, 120, 120,
                        hwnd,
                        HMENU(IDC_CHARSET as isize as *mut _),
                        hinstance,
                        None,
                    );
                    if let Ok(hcb) = GetDlgItem(hwnd, IDC_CHARSET) {
                        let cs_items: Vec<Vec<u16>> = ["Mixed\0", "Katakana\0", "Latin\0", "Binary\0"]
                            .iter().map(|s| s.encode_utf16().collect())
                            .collect();
                        for wide in &cs_items {
                            SendMessageW(hcb, CB_ADDSTRING, WPARAM(0), LPARAM(wide.as_ptr() as isize));
                        }
                        let sel: usize = match config.rain.charset {
                            CharsetKind::Mixed => 0,
                            CharsetKind::Katakana => 1,
                            CharsetKind::Latin => 2,
                            CharsetKind::Binary => 3,
                        };
                        SendMessageW(hcb, CB_SETCURSEL, WPARAM(sel), LPARAM(0));
                    }
                }

                // ── Color picker button ──────────────────────────────────────
                {
                    let _ = CreateWindowExW(
                        windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
                        w!("BUTTON"),
                        w!("Pick Color..."),
                        ws_with(WS_CHILD | WS_VISIBLE | WS_TABSTOP, BS_PUSHBUTTON),
                        10, 190, 120, 28,
                        hwnd,
                        HMENU(IDC_COLOR as isize as *mut _),
                        hinstance,
                        None,
                    );
                }

                // ── Glow checkbox ────────────────────────────────────────────
                {
                    let _ = CreateWindowExW(
                        windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
                        w!("BUTTON"),
                        w!("Enable Glow"),
                        ws_with(WS_CHILD | WS_VISIBLE | WS_TABSTOP, BS_AUTOCHECKBOX),
                        10, 232, 140, 24,
                        hwnd,
                        HMENU(IDC_GLOW as isize as *mut _),
                        hinstance,
                        None,
                    );
                    if let Ok(hchk) = GetDlgItem(hwnd, IDC_GLOW) {
                        let check: DLG_BUTTON_CHECK_STATE = if config.colors.glow {
                            BST_CHECKED
                        } else {
                            BST_UNCHECKED
                        };
                        SendMessageW(hchk, BM_SETCHECK, WPARAM(check.0 as usize), LPARAM(0));
                    }
                }

                // ── OK button ────────────────────────────────────────────────
                {
                    let _ = CreateWindowExW(
                        windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
                        w!("BUTTON"),
                        w!("OK"),
                        ws_with(WS_CHILD | WS_VISIBLE | WS_TABSTOP, BS_DEFPUSHBUTTON),
                        155, 275, 80, 28,
                        hwnd,
                        HMENU(IDOK.0 as isize as *mut _),
                        hinstance,
                        None,
                    );
                }

                // ── Cancel button ────────────────────────────────────────────
                {
                    let _ = CreateWindowExW(
                        windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
                        w!("BUTTON"),
                        w!("Cancel"),
                        ws_with(WS_CHILD | WS_VISIBLE | WS_TABSTOP, BS_PUSHBUTTON),
                        245, 275, 80, 28,
                        hwnd,
                        HMENU(IDCANCEL.0 as isize as *mut _),
                        hinstance,
                        None,
                    );
                }

                LRESULT(0)
            }

            WM_COMMAND => {
                let ctrl_id = (wparam.0 & 0xffff) as i32;

                if ctrl_id == IDOK.0 {
                    let mut config = Config::load();

                    // Speed
                    if let Ok(htb) = GetDlgItem(hwnd, IDC_SPEED) {
                        let pos = SendMessageW(htb, TBM_GETPOS, WPARAM(0), LPARAM(0));
                        config.rain.speed = pos.0 as f32 / 100.0;
                    }

                    // Density
                    if let Ok(htb) = GetDlgItem(hwnd, IDC_DENSITY) {
                        let pos = SendMessageW(htb, TBM_GETPOS, WPARAM(0), LPARAM(0));
                        config.rain.density = pos.0 as f32 / 100.0;
                    }

                    // FPS
                    if let Ok(hcb) = GetDlgItem(hwnd, IDC_FPS) {
                        let sel = SendMessageW(hcb, CB_GETCURSEL, WPARAM(0), LPARAM(0));
                        config.display.fps = match sel.0 {
                            0 => 30,
                            2 => 120,
                            _ => 60,
                        };
                    }

                    // Charset
                    if let Ok(hcb) = GetDlgItem(hwnd, IDC_CHARSET) {
                        let sel = SendMessageW(hcb, CB_GETCURSEL, WPARAM(0), LPARAM(0));
                        config.rain.charset = match sel.0 {
                            0 => CharsetKind::Mixed,
                            1 => CharsetKind::Katakana,
                            2 => CharsetKind::Latin,
                            3 => CharsetKind::Binary,
                            _ => CharsetKind::Mixed,
                        };
                    }

                    // Primary color from CUSTOM_COLORS[0]
                    config.colors.primary = colorref_to_hex(CUSTOM_COLORS[0]);

                    // Glow
                    if let Ok(hchk) = GetDlgItem(hwnd, IDC_GLOW) {
                        let state = SendMessageW(hchk, BM_GETCHECK, WPARAM(0), LPARAM(0));
                        config.colors.glow = state.0 == BST_CHECKED.0 as isize;
                    }

                    save_config(&config);
                    let _ = DestroyWindow(hwnd);
                } else if ctrl_id == IDCANCEL.0 {
                    let _ = DestroyWindow(hwnd);
                } else if ctrl_id == IDC_COLOR {
                    let mut cc = CHOOSECOLORW {
                        lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
                        hwndOwner: hwnd,
                        rgbResult: CUSTOM_COLORS[0],
                        lpCustColors: CUSTOM_COLORS.as_mut_ptr(),
                        Flags: CC_FULLOPEN | CC_RGBINIT,
                        ..Default::default()
                    };
                    if ChooseColorW(&mut cc).as_bool() {
                        CUSTOM_COLORS[0] = cc.rgbResult;
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
                100, 100, 360, 340,
                None,
                None,
                hinstance,
                None,
            ) {
                Ok(h) => h,
                Err(_) => return,
            };

            ShowWindow(hwnd, SW_SHOW);

            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                TranslateMessage(&msg);
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
