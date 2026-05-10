use std::path::PathBuf;

/// Resolve a font family name to font file bytes using fc-match.
pub fn find_font(family: &str) -> Vec<u8> {
    let path = resolve_path(family);
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("cannot read font '{}': {e}", path.display()))
}

fn resolve_path(family: &str) -> PathBuf {
    let output = std::process::Command::new("fc-match")
        .args([family, "--format=%{file}"])
        .output()
        .expect("fc-match not found — install fontconfig (pacman -S fontconfig)");
    if !output.status.success() {
        panic!("fc-match failed for family '{family}'");
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        panic!("fc-match returned empty path for family '{family}'");
    }
    PathBuf::from(path)
}
