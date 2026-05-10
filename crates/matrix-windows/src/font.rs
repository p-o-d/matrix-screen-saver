use matrix_core::config::CharsetKind;

/// Load font bytes for the given charset.
/// Tries system monospace fonts first; falls back to embedded JetBrains Mono.
pub fn load_font(charset: &CharsetKind) -> Vec<u8> {
    let candidates: &[&str] = match charset {
        CharsetKind::Katakana | CharsetKind::Mixed => &[
            "C:\\Windows\\Fonts\\msgothic.ttc",
            "C:\\Windows\\Fonts\\yugothm.ttf",
            "C:\\Windows\\Fonts\\YuGothM.ttf",
            "C:\\Windows\\Fonts\\cour.ttf",
            "C:\\Windows\\Fonts\\consola.ttf",
        ],
        _ => &[
            "C:\\Windows\\Fonts\\consola.ttf",
            "C:\\Windows\\Fonts\\cour.ttf",
            "C:\\Windows\\Fonts\\lucon.ttf",
        ],
    };

    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            return bytes;
        }
    }

    load_embedded()
}

fn load_embedded() -> Vec<u8> {
    include_bytes!("../assets/JetBrainsMono-Regular.ttf").to_vec()
}
