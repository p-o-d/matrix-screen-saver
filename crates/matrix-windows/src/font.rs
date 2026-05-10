use matrix_core::config::CharsetKind;

pub fn load_font(_charset: &CharsetKind) -> Vec<u8> {
    load_embedded()
}

fn load_embedded() -> Vec<u8> {
    include_bytes!("../assets/JetBrainsMono-Regular.ttf").to_vec()
}
