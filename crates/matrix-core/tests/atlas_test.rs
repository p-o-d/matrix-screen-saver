#[cfg(unix)]
#[test]
fn build_atlas_from_bytes() {
    let font_bytes = std::fs::read("/usr/share/fonts/TTF/DejaVuSansMono.ttf")
        .or_else(|_| std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"))
        .or_else(|_| std::fs::read("/usr/share/fonts/noto/NotoSansMono-Regular.ttf"))
        .or_else(|_| {
            // Find any monospace font
            let output = std::process::Command::new("fc-match")
                .args(["monospace", "--format=%{file}"])
                .output()
                .expect("fc-match not available");
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            std::fs::read(&path)
        })
        .expect("no monospace font found for test");
    let chars: Vec<char> = "ABCabc".chars().collect();
    let atlas = matrix_core::atlas::GlyphAtlas::build(&chars, 14.0, &font_bytes);
    assert!(atlas.atlas_width > 0);
    assert!(atlas.atlas_height > 0);
    assert_eq!(atlas.chars.len(), 6);
}
