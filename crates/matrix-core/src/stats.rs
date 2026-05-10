#[derive(Debug, Clone, Default)]
pub struct SystemStats {
    pub ram_used_gb: f32,
    pub ram_total_gb: f32,
    pub cpu_pct: f32,
    pub vram_used_gb: f32,
    pub vram_total_gb: f32,
    pub gpu_pct: f32,
}
