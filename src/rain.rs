use rand::{rngs::SmallRng, Rng, SeedableRng};
use crate::config::RainConfig;

#[derive(Clone)]
pub struct CellState {
    pub ch: char,
    pub brightness: f32,
    pub is_head: bool,
}

impl Default for CellState {
    fn default() -> Self {
        Self { ch: ' ', brightness: 0.0, is_head: false }
    }
}

struct Drop {
    column: usize,
    head_row: f32,
    length: usize,
    speed: f32,
    chars: Vec<char>,
    mutation_timer: f32,
}

pub struct RainSimulator {
    pub columns: usize,
    pub rows: usize,
    pub cells: Vec<Vec<CellState>>,
    drops: Vec<Drop>,
    charset: Vec<char>,
    density: f32,
    base_speed: f32,
    drop_length_min: usize,
    drop_length_max: usize,
    rng: SmallRng,
}

impl RainSimulator {
    pub fn new(columns: usize, rows: usize, charset: Vec<char>, config: &RainConfig) -> Self {
        let cells = vec![vec![CellState::default(); columns]; rows];
        Self {
            columns,
            rows,
            cells,
            drops: Vec::new(),
            charset,
            density: config.density,
            base_speed: config.speed * 8.0,
            drop_length_min: config.drop_length_min,
            drop_length_max: config.drop_length_max,
            rng: SmallRng::from_entropy(),
        }
    }

    pub fn update(&mut self, delta: f32) {
        for row in &mut self.cells {
            for cell in row.iter_mut() {
                cell.brightness = 0.0;
                cell.is_head = false;
            }
        }

        let spawn_prob = (self.density * delta * 60.0).min(1.0);
        for col in 0..self.columns {
            if self.rng.gen::<f32>() < spawn_prob {
                // Don't spawn if an existing drop's head is still near the top,
                // to avoid two heads occupying adjacent rows in the same column.
                let too_close = self.drops.iter().any(|d| {
                    d.column == col && d.head_row < (d.length as f32 + 1.0)
                });
                if too_close {
                    continue;
                }
                let length = self.rng.gen_range(self.drop_length_min..=self.drop_length_max);
                let speed = self.base_speed * self.rng.gen_range(0.5f32..1.5);
                let chars: Vec<char> = (0..length)
                    .map(|_| self.charset[self.rng.gen_range(0..self.charset.len())])
                    .collect();
                self.drops.push(Drop {
                    column: col,
                    head_row: -(length as f32),
                    length,
                    speed,
                    chars,
                    mutation_timer: 0.0,
                });
            }
        }

        let charset = self.charset.clone();
        let rng = &mut self.rng;
        let rows = self.rows;
        self.drops.retain_mut(|drop| {
            drop.head_row += drop.speed * delta;
            drop.mutation_timer += delta;
            if drop.mutation_timer > 0.08 {
                drop.mutation_timer = 0.0;
                if !drop.chars.is_empty() {
                    let last = drop.chars.len() - 1;
                    drop.chars[last] = charset[rng.gen_range(0..charset.len())];
                }
            }
            drop.head_row < (drop.length + rows) as f32
        });

        let rows = self.rows;
        let cols = self.columns;
        for drop in &self.drops {
            let head = drop.head_row as i32;
            for i in 0..drop.length {
                let row = head - i as i32;
                if row < 0 || row >= rows as i32 { continue; }
                let row = row as usize;
                let col = drop.column;
                if col >= cols { continue; }

                let brightness = if i == 0 {
                    1.0
                } else {
                    1.0 - (i as f32 / drop.length as f32)
                };

                if brightness > self.cells[row][col].brightness {
                    let char_idx = (drop.length - 1 - i).min(drop.chars.len() - 1);
                    self.cells[row][col] = CellState {
                        ch: drop.chars[char_idx],
                        brightness,
                        is_head: i == 0,
                    };
                }
            }
        }
    }
}
