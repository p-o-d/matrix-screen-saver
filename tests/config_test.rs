use matrix_screensaver::config::{Config, CharsetKind};

#[test]
fn default_config_has_expected_values() {
    let cfg = Config::default();
    assert_eq!(cfg.display.fps, 60);
    assert_eq!(cfg.display.font_size, 36.0);
    assert_eq!(cfg.rain.charset, CharsetKind::Mixed);
    assert!((cfg.rain.speed - 1.0).abs() < f32::EPSILON);
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
