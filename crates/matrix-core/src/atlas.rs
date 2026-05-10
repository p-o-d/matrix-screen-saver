use fontdue::{Font, FontSettings};

pub struct GlyphAtlas {
    pub data: Vec<u8>,
    pub atlas_width: u32,
    pub atlas_height: u32,
    pub cell_width: u32,
    pub cell_height: u32,
    pub uvs: Vec<[f32; 4]>,
    pub chars: Vec<char>,
}

impl GlyphAtlas {
    pub fn build(chars: &[char], font_size: f32, font_bytes: &[u8]) -> Self {
        let font = Font::from_bytes(font_bytes, FontSettings::default())
            .expect("invalid font bytes");

        let per_char: Vec<(fontdue::Metrics, Vec<u8>)> = chars
            .iter()
            .map(|&ch| font.rasterize(ch, font_size))
            .collect();

        let cell_width = per_char.iter().map(|(m, _)| m.width).max().unwrap_or(10) as u32;
        let cell_height = per_char.iter().map(|(m, _)| m.height).max().unwrap_or(18) as u32;
        let cell_width = cell_width.max(1);
        let cell_height = cell_height.max(1);

        let num_chars = chars.len() as u32;
        let atlas_width = cell_width * num_chars;
        let atlas_height = cell_height;
        let mut atlas_data = vec![0u8; (atlas_width * atlas_height) as usize];
        let mut uvs = Vec::with_capacity(chars.len());

        for (i, (metrics, bitmap)) in per_char.iter().enumerate() {
            let x_base = i as u32 * cell_width;
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
            let u = x_base as f32 / atlas_width as f32;
            uvs.push([u, 0.0f32, cell_width as f32 / atlas_width as f32, 1.0f32]);
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

    pub fn uv_for_char(&self, ch: char) -> [f32; 4] {
        self.chars
            .iter()
            .position(|&c| c == ch)
            .map(|i| self.uvs[i])
            .unwrap_or_else(|| self.uvs.first().copied().unwrap_or([0.0; 4]))
    }
}
