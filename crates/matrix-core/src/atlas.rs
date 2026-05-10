use fontdue::{Font, FontSettings};
use std::path::PathBuf;

pub struct GlyphAtlas {
    /// Raw R8 pixel data (grayscale, one byte per pixel).
    pub data: Vec<u8>,
    pub atlas_width: u32,
    pub atlas_height: u32,
    /// Cell size — all glyphs fit within this bounding box.
    pub cell_width: u32,
    pub cell_height: u32,
    /// UV rects [u, v, w, h] in normalized [0,1] texture space, one per char.
    pub uvs: Vec<[f32; 4]>,
    /// Characters in atlas order (same order as uvs).
    pub chars: Vec<char>,
}

impl GlyphAtlas {
    pub fn build(chars: &[char], font_size: f32, font_family: &str) -> Self {
        let font_path = Self::find_font(font_family);
        let font_bytes = std::fs::read(&font_path)
            .unwrap_or_else(|e| panic!("cannot read font '{}': {e}", font_path.display()));
        let font = Font::from_bytes(font_bytes.as_slice(), FontSettings::default())
            .expect("invalid font file");

        // Rasterize all characters
        let per_char: Vec<(fontdue::Metrics, Vec<u8>)> = chars
            .iter()
            .map(|&ch| font.rasterize(ch, font_size))
            .collect();

        // Cell size = max bounding box across all glyphs
        let cell_width = per_char
            .iter()
            .map(|(m, _)| m.width)
            .max()
            .unwrap_or(10) as u32;
        let cell_height = per_char
            .iter()
            .map(|(m, _)| m.height)
            .max()
            .unwrap_or(18) as u32;

        // Guard: ensure at least 1x1 cell
        let cell_width = cell_width.max(1);
        let cell_height = cell_height.max(1);

        let num_chars = chars.len() as u32;
        let atlas_width = cell_width * num_chars;
        let atlas_height = cell_height;

        let mut atlas_data = vec![0u8; (atlas_width * atlas_height) as usize];
        let mut uvs = Vec::with_capacity(chars.len());

        for (i, (metrics, bitmap)) in per_char.iter().enumerate() {
            let x_base = i as u32 * cell_width;
            // Center glyph horizontally and vertically within cell
            let x_pad = (cell_width.saturating_sub(metrics.width as u32)) / 2;
            let y_pad = (cell_height.saturating_sub(metrics.height as u32)) / 2;

            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let src = row * metrics.width + col;
                    let dst_x = x_base + x_pad + col as u32;
                    let dst_y = y_pad + row as u32;
                    let dst = (dst_y * atlas_width + dst_x) as usize;
                    if dst < atlas_data.len() && src < bitmap.len() {
                        atlas_data[dst] = bitmap[src];
                    }
                }
            }

            // UV rect for this character
            let u = x_base as f32 / atlas_width as f32;
            let v = 0.0f32;
            let w = cell_width as f32 / atlas_width as f32;
            let h = 1.0f32;
            uvs.push([u, v, w, h]);
        }

        Self {
            data: atlas_data,
            atlas_width,
            atlas_height,
            cell_width,
            cell_height,
            uvs,
            chars: chars.to_vec(),
        }
    }

    /// Returns the UV rect [u, v, w, h] for a character.
    /// Falls back to first character if not found (space or first in charset).
    pub fn uv_for_char(&self, ch: char) -> [f32; 4] {
        self.chars
            .iter()
            .position(|&c| c == ch)
            .map(|i| self.uvs[i])
            .unwrap_or_else(|| self.uvs.first().copied().unwrap_or([0.0; 4]))
    }

    /// Find a font file path using `fc-match`.
    fn find_font(family: &str) -> PathBuf {
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
}
