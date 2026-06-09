//! 性能监控数据收集与面板渲染

use std::sync::OnceLock;
use std::time::Instant;

use iced_core::Alignment;
use iced_widget::{column, container, row, text};

use crate::root::{Element, Theme};

/// 总 CPU 核心数（0.0 ~ 100.0，100% = 所有核心满载）
fn num_cores() -> f64 {
    static CORES: OnceLock<f64> = OnceLock::new();
    *CORES.get_or_init(|| {
        std::thread::available_parallelism()
            .map(|n| n.get() as f64)
            .unwrap_or(1.0)
    })
}

/// 性能监控数据
#[derive(Debug, Clone, Copy, Default)]
pub struct PerfData {
    /// 当前 FPS
    pub fps: f32,
    /// CPU 使用率百分比（0.0 ~ 100.0，100% = 所有核心满载）
    pub cpu_usage: f32,
    /// 进程内存占用（MB）
    pub memory_mb: f32,
    /// GPU 帧耗时（ms）
    pub gpu_frame_time_ms: f32,
}

impl PerfData {
    pub fn new(fps: f32, cpu_usage: f32, memory_mb: f32, gpu_frame_time_ms: f32) -> Self {
        Self {
            fps,
            cpu_usage,
            memory_mb,
            gpu_frame_time_ms,
        }
    }
}

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

/// 渲染性能面板
pub fn performance_panel_view<'a>(data: &PerfData) -> Element<'a> {
    let fps_text = format!("{:.1}", data.fps);
    let cpu_text = format!("{:.1}%", data.cpu_usage);
    let mem_text = format!("{:.1} MB", data.memory_mb);
    let gpu_text = format!("{:.1} ms", data.gpu_frame_time_ms);

    let panel = column![
        metric_row("FPS", fps_text),
        metric_row("CPU", cpu_text),
        metric_row("MEM", mem_text),
        metric_row("GPU", gpu_text),
    ]
    .spacing(2)
    .padding([6, 10]);

    container(panel)
        .width(200)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default()
                .background(palette.background.neutral.color)
                .border(iced_core::Border {
                    color: palette.background.strong.color,
                    width: 1.0,
                    radius: 4.0.into(),
                })
        })
        .into()
}

fn metric_row<'a>(label: &'a str, value: String) -> Element<'a> {
    row![
        text(label).size(11).style(|theme: &Theme| {
            let palette = theme.extended_palette();
            text::Style {
                color: Some(palette.background.strong.text),
            }
        }),
        iced_widget::space(),
        text(value).size(11).style(|theme: &Theme| {
            let palette = theme.extended_palette();
            text::Style {
                color: Some(palette.primary.strong.color),
            }
        }),
    ]
    .align_y(Alignment::Center)
    .into()
}
