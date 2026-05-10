use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct SystemStats {
    pub ram_used_gb: f32,
    pub ram_total_gb: f32,
    pub cpu_pct: f32,
    pub vram_used_gb: f32,
    pub vram_total_gb: f32,
    pub gpu_pct: f32,
}

/// PCI vendor+device IDs of the wgpu-selected GPU adapter.
#[derive(Debug, Clone)]
pub struct GpuSpec {
    pub vendor: u32,
    pub device: u32,
}

enum GpuKind {
    Nvidia { pci_bdf: String },
    Amd { card: PathBuf },
    Intel { card: PathBuf },
    Unknown,
}

struct CpuSnapshot {
    idle: u64,
    total: u64,
}

fn read_cpu_snapshot() -> CpuSnapshot {
    let content = std::fs::read_to_string("/proc/stat").unwrap_or_default();
    let line = content.lines().next().unwrap_or("");
    let nums: Vec<u64> = line
        .split_whitespace()
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
    if total_delta == 0 {
        return 0.0;
    }
    (1.0 - idle_delta as f32 / total_delta as f32) * 100.0
}

fn read_mem() -> (f32, f32) {
    let content = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut total_kb = 0u64;
    let mut available_kb = 0u64;
    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            total_kb = line
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
        } else if line.starts_with("MemAvailable:") {
            available_kb = line
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
        }
    }
    let total_gb = total_kb as f32 / 1_048_576.0;
    let used_gb = total_kb.saturating_sub(available_kb) as f32 / 1_048_576.0;
    (used_gb, total_gb)
}

fn drm_cards() -> Vec<PathBuf> {
    let mut cards: Vec<PathBuf> = std::fs::read_dir("/sys/class/drm")
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map_or(false, |n| {
                    n.starts_with("card") && n[4..].chars().all(|c| c.is_ascii_digit())
                })
        })
        .collect();
    cards.sort();
    cards
}

fn sysfs_hex(path: &Path) -> u32 {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok())
        .unwrap_or(0)
}

fn card_pci_bdf(card: &Path) -> String {
    // /sys/class/drm/cardN/device is a symlink — last path component is the PCI BDF
    std::fs::canonicalize(card.join("device"))
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default()
}

fn make_gpu_kind(vendor: u32, card: &PathBuf) -> GpuKind {
    const AMD: u32 = 0x1002;
    const NVIDIA: u32 = 0x10DE;
    const INTEL: u32 = 0x8086;
    match vendor {
        AMD => GpuKind::Amd { card: card.clone() },
        NVIDIA => GpuKind::Nvidia { pci_bdf: card_pci_bdf(card) },
        INTEL => GpuKind::Intel { card: card.clone() },
        _ => GpuKind::Unknown,
    }
}

fn detect_gpu(spec: &GpuSpec) -> GpuKind {
    let cards = drm_cards();

    // First pass: exact vendor + device match
    for card in &cards {
        if sysfs_hex(&card.join("device/vendor")) != spec.vendor { continue; }
        if sysfs_hex(&card.join("device/device")) != spec.device { continue; }
        return make_gpu_kind(spec.vendor, card);
    }

    // Second pass: vendor-only match (handles sub-device ID mismatches)
    for card in &cards {
        if sysfs_hex(&card.join("device/vendor")) != spec.vendor { continue; }
        return make_gpu_kind(spec.vendor, card);
    }

    GpuKind::Unknown
}

fn read_amd_gpu(card: &Path) -> Option<(f32, f32, f32)> {
    let dev = card.join("device");
    let vram_used = std::fs::read_to_string(dev.join("mem_info_vram_used"))
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    let vram_total = std::fs::read_to_string(dev.join("mem_info_vram_total"))
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    let gpu_pct = std::fs::read_to_string(dev.join("gpu_busy_percent"))
        .ok()?
        .trim()
        .parse::<f32>()
        .ok()?;
    const GB: f32 = 1_073_741_824.0;
    Some((vram_used as f32 / GB, vram_total as f32 / GB, gpu_pct))
}

/// Read cumulative RC6 idle residency in ms. Tries per-GT path first (kernel ≥5.x),
/// then the legacy top-level path.
fn read_rc6_ms(card: &Path) -> Option<u64> {
    let per_gt = card.join("gt/gt0/rc6_residency_ms");
    let legacy = card.join("gt_RC6_residency_ms");
    for path in [per_gt, legacy] {
        if let Ok(s) = std::fs::read_to_string(&path) {
            if let Ok(v) = s.trim().parse::<u64>() {
                return Some(v);
            }
        }
    }
    None
}

fn read_nvidia_gpu(pci_bdf: &str) -> Option<(f32, f32, f32)> {
    let mut cmd = std::process::Command::new("nvidia-smi");
    cmd.args([
        "--query-gpu=memory.used,memory.total,utilization.gpu",
        "--format=csv,noheader,nounits",
    ]);
    if !pci_bdf.is_empty() {
        cmd.arg(format!("--id={pci_bdf}"));
    }
    let out = cmd.output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    let parts: Vec<f32> = s
        .trim()
        .split(',')
        .filter_map(|p| p.trim().parse().ok())
        .collect();
    if parts.len() >= 3 {
        Some((parts[0] / 1024.0, parts[1] / 1024.0, parts[2]))
    } else {
        None
    }
}

fn read_gpu_stateless(kind: &GpuKind) -> (f32, f32, f32) {
    match kind {
        GpuKind::Nvidia { pci_bdf } => read_nvidia_gpu(pci_bdf).unwrap_or_default(),
        GpuKind::Amd { card } => read_amd_gpu(card).unwrap_or_default(),
        GpuKind::Intel { .. } | GpuKind::Unknown => (0.0, 0.0, 0.0),
    }
}

/// Spawn background stats poller. `gpu_hint` is filled in later (once the wgpu
/// adapter is known) — GPU stats are silently zero until then.
pub fn start_stats_poller(gpu_hint: Arc<Mutex<Option<GpuSpec>>>) -> Arc<Mutex<SystemStats>> {
    let stats = Arc::new(Mutex::new(SystemStats::default()));
    let writer = Arc::clone(&stats);
    std::thread::Builder::new()
        .name("stats-poller".into())
        .spawn(move || {
            let mut prev = read_cpu_snapshot();
            let mut gpu_kind = GpuKind::Unknown;
            let mut detected_vendor: u32 = 0;
            // Intel RC6 delta state: (prev_rc6_ms, prev_sample_instant)
            let mut intel_prev: Option<(u64, Instant)> = None;
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let now = Instant::now();
                let curr = read_cpu_snapshot();
                let cpu = cpu_pct(&prev, &curr);
                prev = curr;
                let (ram_used_gb, ram_total_gb) = read_mem();

                if let Ok(hint) = gpu_hint.lock() {
                    if let Some(spec) = hint.as_ref() {
                        if spec.vendor != detected_vendor {
                            detected_vendor = spec.vendor;
                            gpu_kind = detect_gpu(spec);
                            intel_prev = None;
                        }
                    }
                }

                let (vram_used_gb, vram_total_gb, gpu_pct) = match &gpu_kind {
                    GpuKind::Intel { card } => {
                        let rc6 = read_rc6_ms(card);
                        let pct = match (rc6, intel_prev.as_ref()) {
                            (Some(cur), Some((prev_rc6, prev_t))) => {
                                let delta_rc6 = cur.saturating_sub(*prev_rc6) as f32;
                                let delta_wall = now.duration_since(*prev_t).as_millis() as f32;
                                if delta_wall > 0.0 {
                                    (1.0 - delta_rc6 / delta_wall).clamp(0.0, 1.0) * 100.0
                                } else {
                                    0.0
                                }
                            }
                            _ => 0.0,
                        };
                        intel_prev = rc6.map(|v| (v, now));
                        (0.0, 0.0, pct)
                    }
                    other => {
                        intel_prev = None;
                        read_gpu_stateless(other)
                    }
                };

                if let Ok(mut s) = writer.lock() {
                    *s = SystemStats {
                        ram_used_gb,
                        ram_total_gb,
                        cpu_pct: cpu,
                        vram_used_gb,
                        vram_total_gb,
                        gpu_pct,
                    };
                }
            }
        })
        .expect("stats poller thread spawn failed");
    stats
}
