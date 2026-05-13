use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    pub font: String,
    pub font_size: f32,
    pub fps: u32,
    pub scanlines: bool,
    pub scanline_intensity: f32,
    pub chromatic_aberration: f32,
    pub debug_overlay: bool,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            font: "monospace".into(),
            font_size: 60.0,
            fps: 60,
            scanlines: true,
            scanline_intensity: 0.30,
            chromatic_aberration: 0.004,
            debug_overlay: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RainConfig {
    pub speed: f32,
    pub density: f32,
    pub charset: CharsetKind,
    pub drop_length_min: usize,
    pub drop_length_max: usize,
    /// Number of depth planes (1 = flat, no perspective).
    pub depth_levels: u8,
    /// Cell scale of the farthest plane (nearest = 1.0).
    pub depth_scale_min: f32,
    /// Brightness multiplier of the farthest plane (nearest = 1.0).
    pub depth_brightness_min: f32,
    /// Controls how much a new drop boosts spawn probability in nearby columns.
    /// 0.0 = uniform, 0.2 = subtle clusters.
    pub cluster_strength: f32,
}

impl Default for RainConfig {
    fn default() -> Self {
        Self {
            speed: 0.5,
            density: 0.01,
            charset: CharsetKind::Mixed,
            drop_length_min: 5,
            drop_length_max: 15,
            depth_levels: 5,
            depth_scale_min: 0.35,
            depth_brightness_min: 0.25,
            cluster_strength: 0.2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
            let mut cfg = toml::from_str(&content).unwrap_or_else(|e| {
                eprintln!("config parse error: {e}, using defaults");
                Config::default()
            });
            cfg.clamp_to_defaults();
            cfg
        } else {
            Config::default()
        }
    }

    /// Reset any field outside its valid range to the struct's Default value.
    /// Called automatically by `Config::load()` after TOML parsing.
    pub fn clamp_to_defaults(&mut self) {
        let d = Config::default();

        // display
        if !(8.0_f32..=120.0).contains(&self.display.font_size) {
            eprintln!("config: font_size {} out of [8, 120], resetting to {}", self.display.font_size, d.display.font_size);
            self.display.font_size = d.display.font_size;
        }
        if !(1_u32..=240).contains(&self.display.fps) {
            eprintln!("config: fps {} out of [1, 240], resetting to {}", self.display.fps, d.display.fps);
            self.display.fps = d.display.fps;
        }
        if !(0.0_f32..=1.0).contains(&self.display.scanline_intensity) {
            eprintln!("config: scanline_intensity {} out of [0, 1], resetting to {}", self.display.scanline_intensity, d.display.scanline_intensity);
            self.display.scanline_intensity = d.display.scanline_intensity;
        }
        if !(0.0_f32..=0.05).contains(&self.display.chromatic_aberration) {
            eprintln!("config: chromatic_aberration {} out of [0, 0.05], resetting to {}", self.display.chromatic_aberration, d.display.chromatic_aberration);
            self.display.chromatic_aberration = d.display.chromatic_aberration;
        }

        // rain
        if !(0.1_f32..=10.0).contains(&self.rain.speed) {
            eprintln!("config: speed {} out of [0.1, 10], resetting to {}", self.rain.speed, d.rain.speed);
            self.rain.speed = d.rain.speed;
        }
        if !(0.001_f32..=1.0).contains(&self.rain.density) {
            eprintln!("config: density {} out of [0.001, 1], resetting to {}", self.rain.density, d.rain.density);
            self.rain.density = d.rain.density;
        }
        if !(1_usize..=50).contains(&self.rain.drop_length_min) {
            eprintln!("config: drop_length_min {} out of [1, 50], resetting to {}", self.rain.drop_length_min, d.rain.drop_length_min);
            self.rain.drop_length_min = d.rain.drop_length_min;
        }
        if !(1_usize..=100).contains(&self.rain.drop_length_max) {
            eprintln!("config: drop_length_max {} out of [1, 100], resetting to {}", self.rain.drop_length_max, d.rain.drop_length_max);
            self.rain.drop_length_max = d.rain.drop_length_max;
        }
        if self.rain.drop_length_min > self.rain.drop_length_max {
            eprintln!("config: drop_length_min {} > drop_length_max {}, resetting both to defaults", self.rain.drop_length_min, self.rain.drop_length_max);
            self.rain.drop_length_min = d.rain.drop_length_min;
            self.rain.drop_length_max = d.rain.drop_length_max;
        }
        if !(1_u8..=10).contains(&self.rain.depth_levels) {
            eprintln!("config: depth_levels {} out of [1, 10], resetting to {}", self.rain.depth_levels, d.rain.depth_levels);
            self.rain.depth_levels = d.rain.depth_levels;
        }
        if !(0.1_f32..=1.0).contains(&self.rain.depth_scale_min) {
            eprintln!("config: depth_scale_min {} out of [0.1, 1], resetting to {}", self.rain.depth_scale_min, d.rain.depth_scale_min);
            self.rain.depth_scale_min = d.rain.depth_scale_min;
        }
        if !(0.0_f32..=1.0).contains(&self.rain.depth_brightness_min) {
            eprintln!("config: depth_brightness_min {} out of [0, 1], resetting to {}", self.rain.depth_brightness_min, d.rain.depth_brightness_min);
            self.rain.depth_brightness_min = d.rain.depth_brightness_min;
        }
        if !(0.0_f32..=5.0).contains(&self.rain.cluster_strength) {
            eprintln!("config: cluster_strength {} out of [0, 5], resetting to {}", self.rain.cluster_strength, d.rain.cluster_strength);
            self.rain.cluster_strength = d.rain.cluster_strength;
        }

        // colors
        if !(0.0_f32..=1.0).contains(&self.colors.glow_intensity) {
            eprintln!("config: glow_intensity {} out of [0, 1], resetting to {}", self.colors.glow_intensity, d.colors.glow_intensity);
            self.colors.glow_intensity = d.colors.glow_intensity;
        }

        // idle
        if !(30_u64..=86400).contains(&self.idle.timeout_seconds) {
            eprintln!("config: timeout_seconds {} out of [30, 86400], resetting to {}", self.idle.timeout_seconds, d.idle.timeout_seconds);
            self.idle.timeout_seconds = d.idle.timeout_seconds;
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
