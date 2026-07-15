//! 性能监控数据收集与面板渲染

use std::sync::OnceLock;
use std::time::Instant;

/// 总 CPU 核心数（0.0 ~ 100.0，100% = 所有核心满载）
fn num_cores() -> f64 {
    static CORES: OnceLock<f64> = OnceLock::new();
    *CORES.get_or_init(|| {
        std::thread::available_parallelism()
            .map(|n| n.get() as f64)
            .unwrap_or(1.0)
    })
}

/// 性能监控数据 — 重新导出自 lumino-message
pub use lumino_message::PerfData;

/// CPU 使用率监控器：计算进程 CPU 时间增量
pub struct CpuMonitor {
    last_cpu_time: u64,
    last_wall: Instant,
}

impl CpuMonitor {
    pub fn new() -> Self {
        Self {
            last_cpu_time: get_cpu_time_us(),
            last_wall: Instant::now(),
        }
    }

    /// 返回自上次调用以来的 CPU 使用率百分比（0.0 ~ 100.0，100% = 所有核心满载）
    pub fn usage(&mut self) -> f32 {
        let now = Instant::now();
        let cpu = get_cpu_time_us();
        let wall = now.duration_since(self.last_wall).as_micros() as f64;
        let cpu_delta = cpu.saturating_sub(self.last_cpu_time) as f64;
        self.last_cpu_time = cpu;
        self.last_wall = now;
        if wall > 0.0 {
            (((cpu_delta / wall) * 100.0 / num_cores()).min(100.0)) as f32
        } else {
            0.0
        }
    }
}

fn get_cpu_time_us() -> u64 {
    lumino_memory_monitor::platform::get_process_cpu_time_us()
}

// 性能监控面板 UI 已移除：其功能由工具栏检测仪表盘（toolbar::view::detection_dashboard）
// 承接，复用了下方保留的数据读取接口（CpuMonitor / PerfData / lumino_memory_monitor）。
