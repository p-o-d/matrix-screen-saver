use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CharsetKind {
    Mixed,
    Katakana,
    Latin,
    Binary,
}

impl Default for CharsetKind {
    fn default() -> Self {
        Self::Mixed
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    pub font: String,
    pub font_size: f32,
    pub fps: u32,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            font: "monospace".into(),
            font_size: 18.0,
            fps: 60,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RainConfig {
    pub speed: f32,
    pub density: f32,
    pub charset: CharsetKind,
    pub drop_length_min: usize,
    pub drop_length_max: usize,
}

impl Default for RainConfig {
    fn default() -> Self {
        Self {
            speed: 1.0,
            density: 0.05,
            charset: CharsetKind::Mixed,
            drop_length_min: 5,
            drop_length_max: 25,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ColorsConfig {
    pub primary: String,
    pub background: String,
    pub glow: bool,
    pub glow_intensity: f32,
}

impl Default for ColorsConfig {
    fn default() -> Self {
        Self {
            primary: "#00ff41".into(),
            background: "#000000".into(),
            glow: true,
            glow_intensity: 0.8,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct IdleConfig {
    pub timeout_seconds: u64,
}

impl Default for IdleConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 120,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub display: DisplayConfig,
    pub rain: RainConfig,
    pub colors: ColorsConfig,
    pub idle: IdleConfig,
}

impl Config {
    pub fn load() -> Self {
        let path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("matrix-screensaver/config.toml");

        if let Ok(content) = std::fs::read_to_string(&path) {
            toml::from_str(&content).unwrap_or_else(|e| {
                eprintln!("config parse error: {e}, using defaults");
                Config::default()
            })
        } else {
            Config::default()
        }
    }

    /// Parse "#rrggbb" hex color → [r, g, b, 1.0] normalized floats.
    pub fn parse_color(hex: &str) -> [f32; 4] {
        let hex = hex.trim_start_matches('#');
        if hex.len() < 6 {
            return [0.0, 0.0, 0.0, 1.0];
        }
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0) as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0) as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0) as f32 / 255.0;
        [r, g, b, 1.0]
    }
}
