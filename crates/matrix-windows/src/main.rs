#![windows_subsystem = "windows"]

mod font;
mod screensaver;
mod preview;
mod config_dialog;
mod stats;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.to_ascii_lowercase());

    match mode.as_deref() {
        Some("/s") | Some("-s") => screensaver::run(),
        Some("/p") => {
            let hwnd = args.get(2)
                .and_then(|s| s.parse::<isize>().ok())
                .unwrap_or(0);
            preview::run(hwnd);
        }
        Some("/c") | Some("-c") | None => config_dialog::run(),
        _ => config_dialog::run(),
    }
}
