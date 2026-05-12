use matrix_core::{chars::get_charset, config::CharsetKind};

#[test]
fn katakana_charset_has_correct_range() {
    let chars = get_charset(&CharsetKind::Katakana);
    assert!(!chars.is_empty());
    assert!(chars.contains(&'\u{FF66}')); // ｦ
    assert!(chars.contains(&'\u{FF9D}')); // ﾝ
    assert!(!chars.contains(&'A'));
}

#[test]
fn latin_charset_has_az_and_digits() {
    let chars = get_charset(&CharsetKind::Latin);
    assert!(chars.contains(&'A'));
    assert!(chars.contains(&'z'));
    assert!(chars.contains(&'0'));
    assert!(chars.contains(&'9'));
    assert!(!chars.contains(&'\u{FF66}'));
}

#[test]
fn binary_charset_only_zero_one() {
    let chars = get_charset(&CharsetKind::Binary);
    assert_eq!(chars, vec!['0', '1']);
}

#[test]
fn mixed_charset_contains_all() {
    let chars = get_charset(&CharsetKind::Mixed);
    assert!(chars.contains(&'\u{FF66}'));
    assert!(chars.contains(&'A'));
    assert!(chars.contains(&'0'));
}

use matrix_core::{
    config::RainConfig,
    rain::RainSimulator,
};

fn default_rain_config() -> RainConfig {
    RainConfig {
        speed: 1.0,
        density: 1.0,
        charset: CharsetKind::Latin,
        drop_length_min: 3,
        drop_length_max: 5,
        depth_levels: 1,
        depth_scale_min: 1.0,
        depth_brightness_min: 1.0,
        cluster_strength: 0.0,
    }
}

#[test]
fn simulator_initialises_empty() {
    let sim = RainSimulator::new(10, 20, vec!['A', 'B'], &default_rain_config());
    let all_dark = sim.cells.iter().flatten().all(|c| c.brightness == 0.0);
    assert!(all_dark);
}

#[test]
fn update_spawns_drops_at_max_density() {
    let mut sim = RainSimulator::new(10, 20, vec!['A', 'B', 'C'], &default_rain_config());
    for _ in 0..30 {
        sim.update(1.0 / 60.0);
    }
    let any_lit = sim.cells.iter().flatten().any(|c| c.brightness > 0.0);
    assert!(any_lit, "no cells lit after 30 frames at max density");
}

#[test]
fn head_cell_has_maximum_brightness() {
    let cfg = RainConfig {
        density: 1.0,
        drop_length_min: 5,
        drop_length_max: 5,
        ..default_rain_config()
    };
    let mut sim = RainSimulator::new(5, 40, vec!['X'], &cfg);
    for _ in 0..120 {
        sim.update(1.0 / 60.0);
    }
    let has_head = sim.cells.iter().flatten().any(|c| c.is_head);
    assert!(has_head, "no head cell found after 120 frames");
    for cell in sim.cells.iter().flatten().filter(|c| c.is_head) {
        assert!((cell.brightness - 1.0).abs() < f32::EPSILON);
    }
}

#[test]
fn inverted_drop_length_bounds_normalized() {
    // Constructor normalizes min/max so gen_range doesn't panic
    let cfg = RainConfig {
        density: 1.0,
        drop_length_min: 20,
        drop_length_max: 3, // intentionally inverted
        ..default_rain_config()
    };
    let mut sim = RainSimulator::new(5, 30, vec!['A'], &cfg);
    for _ in 0..60 {
        sim.update(1.0 / 60.0);
    }
    let any_lit = sim.cells.iter().flatten().any(|c| c.brightness > 0.0);
    assert!(any_lit, "sim with inverted drop bounds never spawned drops");
}

#[test]
fn drops_expire_and_cells_stay_in_range() {
    // Run for a long time; brightness must always stay in [0, 1]
    // and the sim must not OOM/hang (i.e., dead drops are pruned).
    let cfg = RainConfig {
        density: 1.0,
        speed: 2.0,
        drop_length_min: 3,
        drop_length_max: 5,
        ..default_rain_config()
    };
    let mut sim = RainSimulator::new(10, 20, vec!['A'], &cfg);
    for _ in 0..3000 {
        sim.update(1.0 / 60.0);
    }
    for cell in sim.cells.iter().flatten() {
        assert!(
            cell.brightness >= 0.0 && cell.brightness <= 1.0,
            "brightness out of [0,1]: {}",
            cell.brightness
        );
    }
}

#[test]
fn cluster_strength_nonzero_produces_output() {
    // Smoke test: clustering logic runs without panic and cells get lit.
    let cfg = RainConfig {
        density: 0.5,
        cluster_strength: 1.0,
        drop_length_min: 3,
        drop_length_max: 5,
        ..default_rain_config()
    };
    let mut sim = RainSimulator::new(10, 20, vec!['A'], &cfg);
    for _ in 0..120 {
        sim.update(1.0 / 60.0);
    }
    let any_lit = sim.cells.iter().flatten().any(|c| c.brightness > 0.0);
    assert!(any_lit, "no cells lit with cluster_strength=1.0");
}

#[test]
fn brightness_decreases_from_head_to_tail() {
    let cfg = RainConfig {
        density: 1.0,
        speed: 0.5,
        drop_length_min: 8,
        drop_length_max: 8,
        ..default_rain_config()
    };
    let mut sim = RainSimulator::new(1, 40, vec!['A'], &cfg);
    for _ in 0..300 {
        sim.update(1.0 / 60.0);
    }
    let col: Vec<f32> = (0..sim.rows).map(|r| sim.cells[r][0].brightness).collect();
    if let Some(head) = col.iter().position(|&b| b == 1.0) {
        if head + 3 < sim.rows {
            assert!(col[head] > col[head + 1]);
            assert!(col[head + 1] >= col[head + 2]);
        }
    }
}
