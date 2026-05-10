use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct SystemStats {
    pub ram_used_gb: f32,
    pub ram_total_gb: f32,
    pub cpu_pct: f32,
    pub vram_used_gb: f32,
    pub vram_total_gb: f32,
    pub gpu_pct: f32,
}

struct CpuSnapshot {
    idle: u64,
    total: u64,
}

fn read_cpu_snapshot() -> CpuSnapshot {
    let content = std::fs::read_to_string("/proc/stat").unwrap_or_default();
    let line = content.lines().next().unwrap_or("");
    let nums: Vec<u64> = line.split_whitespace()
        .skip(1)
        .filter_map(|s| s.parse().ok())
        .collect();
    let total: u64 = nums.iter().sum();
    let idle = nums.get(3).copied().unwrap_or(0);
    CpuSnapshot { idle, total }
}

fn cpu_pct(a: &CpuSnapshot, b: &CpuSnapshot) -> f32 {
    let total_delta = b.total.saturating_sub(a.total);
    let idle_delta = b.idle.saturating_sub(a.idle);
    if total_delta == 0 { return 0.0; }
    (1.0 - idle_delta as f32 / total_delta as f32) * 100.0
}

fn read_mem() -> (f32, f32) {
    let content = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut total_kb = 0u64;
    let mut available_kb = 0u64;
    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            total_kb = line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        } else if line.starts_with("MemAvailable:") {
            available_kb = line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        }
    }
    let total_gb = total_kb as f32 / 1_048_576.0;
    let used_gb = total_kb.saturating_sub(available_kb) as f32 / 1_048_576.0;
    (used_gb, total_gb)
}

fn read_nvidia() -> Option<(f32, f32, f32)> {
    let out = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=memory.used,memory.total,utilization.gpu", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let parts: Vec<f32> = s.trim().split(',')
        .filter_map(|p| p.trim().parse().ok())
        .collect();
    if parts.len() >= 3 {
        Some((parts[0] / 1024.0, parts[1] / 1024.0, parts[2]))
    } else {
        None
    }
}

pub fn start_stats_poller() -> Arc<Mutex<SystemStats>> {
    let stats = Arc::new(Mutex::new(SystemStats::default()));
    let writer = Arc::clone(&stats);
    std::thread::Builder::new()
        .name("stats-poller".into())
        .spawn(move || {
            let mut prev = read_cpu_snapshot();
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let curr = read_cpu_snapshot();
                let cpu = cpu_pct(&prev, &curr);
                prev = curr;
                let (ram_used_gb, ram_total_gb) = read_mem();
                let (vram_used_gb, vram_total_gb, gpu_pct) = read_nvidia().unwrap_or_default();
                if let Ok(mut s) = writer.lock() {
                    *s = SystemStats { ram_used_gb, ram_total_gb, cpu_pct: cpu, vram_used_gb, vram_total_gb, gpu_pct };
                }
            }
        })
        .expect("stats poller thread spawn failed");
    stats
}
