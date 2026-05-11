#![windows_subsystem = "windows"]

mod font;
mod screensaver;
mod preview;
mod config_dialog;
mod stats;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let debug = args.iter().any(|a| a == "--debug");

    if debug {
        #[cfg(windows)]
        unsafe {
            windows::Win32::System::Console::AllocConsole().ok();
        }
        // Safe: called before any threads spawn.
        unsafe { std::env::set_var("RUST_LOG", "debug"); }

        std::panic::set_hook(Box::new(|info| {
            let msg: Vec<u16> = format!("{info}\0").encode_utf16().collect();
            #[cfg(windows)]
            unsafe {
                windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                    None,
                    windows::core::PCWSTR(msg.as_ptr()),
                    windows::core::w!("matrix-screensaver panic"),
                    windows::Win32::UI::WindowsAndMessaging::MB_OK
                        | windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
                );
            }
            #[cfg(not(windows))]
            eprintln!("{info}");
        }));
    }

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Filter --debug out before mode dispatch.
    let plain_args: Vec<&String> = args.iter().skip(1).filter(|a| *a != "--debug").collect();
    let mode = plain_args.first().map(|s| s.to_ascii_lowercase());

    match mode.as_deref() {
        Some("/s") | Some("-s") => screensaver::run(),
        Some("/p") => {
            let hwnd = plain_args.get(1)
                .and_then(|s| s.parse::<isize>().ok())
                .unwrap_or(0);
            preview::run(hwnd);
        }
        Some("/c") | Some("-c") | None => config_dialog::run(),
        _ => config_dialog::run(),
    }
}
