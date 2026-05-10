pub fn find_font(family: &str) -> Vec<u8> {
    match try_find_font(family) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("matrix-screensaver: cannot find font '{}': {}", family, e);
            eprintln!("Install fontconfig and ensure a monospace font is available.");
            std::process::exit(1);
        }
    }
}

fn try_find_font(family: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let output = std::process::Command::new("fc-match")
        .args([family, "--format=%{file}"])
        .output()?;
    if !output.status.success() {
        return Err(format!("fc-match exited with status {}", output.status).into());
    }
    let path = std::str::from_utf8(&output.stdout)?.trim().to_string();
    if path.is_empty() {
        return Err("fc-match returned empty path".into());
    }
    Ok(std::fs::read(&path)?)
}
