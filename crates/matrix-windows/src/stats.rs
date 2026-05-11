//! Windows system stats poller using Win32 / DXGI APIs.

use matrix_core::stats::SystemStats;
use std::sync::{Arc, Mutex};

fn filetime_to_u64(ft: windows::Win32::Foundation::FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

fn poll_cpu(prev: &mut (u64, u64)) -> f32 {
    unsafe {
        let mut idle = windows::Win32::Foundation::FILETIME::default();
        let mut kernel = windows::Win32::Foundation::FILETIME::default();
        let mut user = windows::Win32::Foundation::FILETIME::default();
        let _ = windows::Win32::System::Threading::GetSystemTimes(
            Some(&mut idle),
            Some(&mut kernel),
            Some(&mut user),
        );
        let idle_t = filetime_to_u64(idle);
        let kernel_t = filetime_to_u64(kernel);
        let user_t = filetime_to_u64(user);
        let total = kernel_t + user_t;
        let (prev_idle, prev_total) = *prev;
        *prev = (idle_t, total);
        let d_total = total.saturating_sub(prev_total);
        let d_idle = idle_t.saturating_sub(prev_idle);
        if d_total == 0 {
            return 0.0;
        }
        ((d_total - d_idle) as f32 / d_total as f32 * 100.0).clamp(0.0, 100.0)
    }
}

fn poll_ram() -> (f32, f32) {
    unsafe {
        let mut ms = windows::Win32::System::SystemInformation::MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<windows::Win32::System::SystemInformation::MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        let _ = windows::Win32::System::SystemInformation::GlobalMemoryStatusEx(&mut ms);
        let used = (ms.ullTotalPhys - ms.ullAvailPhys) as f32 / 1_073_741_824.0;
        let total = ms.ullTotalPhys as f32 / 1_073_741_824.0;
        (used, total)
    }
}

fn poll_vram() -> (f32, f32) {
    unsafe {
        use windows::Win32::Graphics::Dxgi::{
            CreateDXGIFactory1, IDXGIAdapter3, IDXGIFactory1,
            DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_QUERY_VIDEO_MEMORY_INFO,
        };
        use windows::core::Interface;

        let factory: IDXGIFactory1 = match CreateDXGIFactory1() {
            Ok(f) => f,
            Err(_) => return (0.0, 0.0),
        };
        let adapter = match factory.EnumAdapters1(0) {
            Ok(a) => a,
            Err(_) => return (0.0, 0.0),
        };
        let adapter3: IDXGIAdapter3 = match adapter.cast() {
            Ok(a) => a,
            Err(_) => return (0.0, 0.0),
        };
        let mut info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
        if adapter3.QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut info).is_err() {
            return (0.0, 0.0);
        }
        let used = info.CurrentUsage as f32 / 1_073_741_824.0;
        let budget = info.Budget as f32 / 1_073_741_824.0;
        (used, budget)
    }
}

pub fn start_stats_poller() -> Arc<Mutex<SystemStats>> {
    let stats = Arc::new(Mutex::new(SystemStats::default()));
    let stats_clone = stats.clone();
    std::thread::Builder::new()
        .name("stats-poller".into())
        .spawn(move || {
            let mut prev_cpu = (0u64, 0u64);
            loop {
                let cpu = poll_cpu(&mut prev_cpu);
                let (ram_used, ram_total) = poll_ram();
                let (vram_used, vram_total) = poll_vram();
                *stats_clone.lock().unwrap() = SystemStats {
                    cpu_pct: cpu,
                    ram_used_gb: ram_used,
                    ram_total_gb: ram_total,
                    vram_used_gb: vram_used,
                    vram_total_gb: vram_total,
                    gpu_pct: 0.0, // not available without vendor SDK
                };
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        })
        .expect("stats poller thread spawn failed");
    stats
}
