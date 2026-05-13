use matrix_core::config::{Config, CharsetKind};

#[test]
fn default_config_has_expected_values() {
    let cfg = Config::default();
    assert_eq!(cfg.display.fps, 60);
    assert_eq!(cfg.display.font_size, 60.0);
    assert_eq!(cfg.rain.charset, CharsetKind::Mixed);
    assert!((cfg.rain.speed - 0.5).abs() < f32::EPSILON);
    assert!(cfg.colors.glow);
    assert_eq!(cfg.idle.timeout_seconds, 120);
}

#[test]
fn parse_color_green() {
    let [r, g, b, a] = Config::parse_color("#00ff41");
    assert_eq!(r, 0.0);
    assert!((g - 1.0).abs() < 0.01);
    assert!((b - 0.255).abs() < 0.01);
    assert_eq!(a, 1.0);
}

#[test]
fn load_from_toml_string() {
    let toml = r##"
        [display]
        fps = 30
        font_size = 24.0
        font = "JetBrains Mono"

        [rain]
        speed = 2.0
        charset = "katakana"

        [colors]
        primary = "#ff0000"
        glow = false

        [idle]
        timeout_seconds = 60
    "##;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.display.fps, 30);
    assert_eq!(cfg.rain.charset, CharsetKind::Katakana);
    assert!(!cfg.colors.glow);
    assert_eq!(cfg.idle.timeout_seconds, 60);
}

#[test]
fn partial_toml_uses_defaults() {
    let toml = "[display]\nfps = 30";
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.display.fps, 30);
    assert_eq!(cfg.rain.charset, CharsetKind::Mixed);
    assert!(cfg.colors.glow);
}

#[test]
fn parse_color_black() {
    let [r, g, b, a] = Config::parse_color("#000000");
    assert_eq!(r, 0.0);
    assert_eq!(g, 0.0);
    assert_eq!(b, 0.0);
    assert_eq!(a, 1.0);
}

#[test]
fn parse_color_uppercase_hex() {
    let [r, g, b, a] = Config::parse_color("#FF0000");
    assert!((r - 1.0).abs() < f32::EPSILON);
    assert_eq!(g, 0.0);
    assert_eq!(b, 0.0);
    assert_eq!(a, 1.0);
}

#[test]
fn parse_color_no_hash_prefix() {
    let with_hash = Config::parse_color("#00ff41");
    let without_hash = Config::parse_color("00ff41");
    assert_eq!(with_hash, without_hash);
}

#[test]
fn parse_color_short_hex_returns_black() {
    let [r, g, b, a] = Config::parse_color("#fff");
    assert_eq!([r, g, b, a], [0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn parse_color_invalid_chars_returns_zero_channels() {
    // from_str_radix fails on non-hex chars → unwrap_or(0) → 0.0
    let [r, g, b, a] = Config::parse_color("#gggggg");
    assert_eq!(r, 0.0);
    assert_eq!(g, 0.0);
    assert_eq!(b, 0.0);
    assert_eq!(a, 1.0);
}

#[test]
fn config_roundtrip_serialization() {
    let original = Config::default();
    let serialized = toml::to_string(&original).expect("serialize failed");
    let restored: Config = toml::from_str(&serialized).expect("deserialize failed");
    assert_eq!(restored.display.fps, original.display.fps);
    assert_eq!(restored.display.font_size, original.display.font_size);
    assert_eq!(restored.rain.charset, original.rain.charset);
    assert!((restored.rain.speed - original.rain.speed).abs() < f32::EPSILON);
    assert!((restored.rain.density - original.rain.density).abs() < 1e-6);
    assert_eq!(restored.rain.depth_levels, original.rain.depth_levels);
    assert_eq!(restored.colors.glow, original.colors.glow);
    assert!((restored.colors.glow_intensity - original.colors.glow_intensity).abs() < f32::EPSILON);
    assert_eq!(restored.idle.timeout_seconds, original.idle.timeout_seconds);
}

#[test]
fn validation_resets_fps_below_min() {
    let toml = "[display]\nfps = 0";
    let mut cfg: Config = toml::from_str(toml).unwrap();
    cfg.clamp_to_defaults();
    assert_eq!(cfg.display.fps, Config::default().display.fps);
}

#[test]
fn validation_resets_fps_above_max() {
    let toml = "[display]\nfps = 999";
    let mut cfg: Config = toml::from_str(toml).unwrap();
    cfg.clamp_to_defaults();
    assert_eq!(cfg.display.fps, Config::default().display.fps);
}

#[test]
fn validation_preserves_valid_fps() {
    let toml = "[display]\nfps = 30";
    let mut cfg: Config = toml::from_str(toml).unwrap();
    cfg.clamp_to_defaults();
    assert_eq!(cfg.display.fps, 30);
}

#[test]
fn validation_resets_speed_below_min() {
    let toml = "[rain]\nspeed = 0.0";
    let mut cfg: Config = toml::from_str(toml).unwrap();
    cfg.clamp_to_defaults();
    assert!((cfg.rain.speed - Config::default().rain.speed).abs() < f32::EPSILON);
}

#[test]
fn validation_resets_density_above_max() {
    let toml = "[rain]\ndensity = 2.0";
    let mut cfg: Config = toml::from_str(toml).unwrap();
    cfg.clamp_to_defaults();
    assert!((cfg.rain.density - Config::default().rain.density).abs() < 1e-6);
}

#[test]
fn validation_resets_inverted_drop_lengths() {
    let toml = "[rain]\ndrop_length_min = 30\ndrop_length_max = 5";
    let mut cfg: Config = toml::from_str(toml).unwrap();
    cfg.clamp_to_defaults();
    let d = Config::default();
    assert_eq!(cfg.rain.drop_length_min, d.rain.drop_length_min);
    assert_eq!(cfg.rain.drop_length_max, d.rain.drop_length_max);
}

#[test]
fn validation_resets_glow_intensity_above_max() {
    let toml = "[colors]\nglow_intensity = 5.0";
    let mut cfg: Config = toml::from_str(toml).unwrap();
    cfg.clamp_to_defaults();
    assert!((cfg.colors.glow_intensity - Config::default().colors.glow_intensity).abs() < f32::EPSILON);
}

#[test]
fn validation_resets_timeout_below_min() {
    let toml = "[idle]\ntimeout_seconds = 5";
    let mut cfg: Config = toml::from_str(toml).unwrap();
    cfg.clamp_to_defaults();
    assert_eq!(cfg.idle.timeout_seconds, Config::default().idle.timeout_seconds);
}

#[test]
fn validation_preserves_valid_values() {
    let toml = r#"
        [display]
        fps = 120
        font_size = 24.0
        [rain]
        speed = 1.5
        density = 0.05
        drop_length_min = 4
        drop_length_max = 20
        [colors]
        glow_intensity = 0.5
        [idle]
        timeout_seconds = 300
    "#;
    let mut cfg: Config = toml::from_str(toml).unwrap();
    cfg.clamp_to_defaults();
    assert_eq!(cfg.display.fps, 120);
    assert!((cfg.display.font_size - 24.0).abs() < f32::EPSILON);
    assert!((cfg.rain.speed - 1.5).abs() < f32::EPSILON);
    assert_eq!(cfg.rain.drop_length_min, 4);
    assert_eq!(cfg.rain.drop_length_max, 20);
    assert_eq!(cfg.idle.timeout_seconds, 300);
}
