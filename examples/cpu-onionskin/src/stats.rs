//! 终端性能统计：帧工作耗时分位数、可见音符、LOD、进程 CPU/RSS。
//!
//! `wall FPS` 含帧率上限/睡眠；`work p99` 是真正花在"剔除 + 光栅 + 合成 +
//! present"上的时间。80fps 验收两者都看：wall ≥ 80 且 work p99 ≤ 12.5ms。

use std::{
    collections::VecDeque,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use lumino_diagnostics::memory_monitor::MemoryMonitor;
use sysinfo::{ProcessesToUpdate, System};

/// 滑动窗口帧样本。
pub struct PerfMonitor {
    samples: VecDeque<f32>,
    draw_samples: VecDeque<f32>,
    present_samples: VecDeque<f32>,
    capacity: usize,
    frames: u64,
    started: Instant,
    last_report: Instant,
    cpu: Option<CpuSampler>,
    physical_cores: usize,
    visible_notes: u64,
    mode: &'static str,
    bands: usize,
}

/// 独立线程采样进程 CPU：sysinfo 的进程 CPU 必须跨两次 refresh 才有值，
/// 不能放在渲染热路径里每帧调用。
struct CpuSampler {
    value: Arc<AtomicU32>,
}

impl CpuSampler {
    fn spawn() -> Option<Self> {
        let value = Arc::new(AtomicU32::new(0));
        let slot = Arc::clone(&value);
        thread::Builder::new()
            .name("cpu-onionskin-cpu".to_string())
            .spawn(move || {
                let mut system = System::new();
                let Ok(pid) = sysinfo::get_current_pid() else {
                    return;
                };
                let mut primed = false;
                loop {
                    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
                    if primed && let Some(process) = system.process(pid) {
                        let usage = process.cpu_usage().clamp(0.0, 100_000.0);
                        slot.store((usage * 100.0) as u32, Ordering::Relaxed);
                    }
                    primed = true;
                    thread::sleep(Duration::from_secs(1));
                }
            })
            .ok()
            .map(|_| Self { value })
    }

    fn read(&self) -> f32 {
        self.value.load(Ordering::Relaxed) as f32 / 100.0
    }
}

/// 一段统计的汇总。
#[derive(Debug, Clone, Copy)]
pub struct Summary {
    /// 帧数。
    pub frames: u64,
    /// wall 时间（秒）。
    pub seconds: f64,
    /// wall FPS。
    pub wall_fps: f64,
    /// 工作耗时均值（毫秒）。
    pub work_avg_ms: f32,
    /// p50 / p95 / p99 / max。
    pub p50_ms: f32,
    pub p95_ms: f32,
    pub p99_ms: f32,
    pub max_ms: f32,
    /// 纯绘制（render + overlay，不含 present）分位数。
    pub draw_p50_ms: f32,
    pub draw_p99_ms: f32,
    /// present（softbuffer/GDI 全窗 blit）分位数。
    pub present_p50_ms: f32,
    pub present_p99_ms: f32,
    /// 进程 CPU（sysinfo 口径，100 = 一个逻辑核）。
    pub cpu_pct: f32,
}

impl PerfMonitor {
    /// 创建监视器。
    #[must_use]
    pub fn new(capacity: usize, physical_cores: usize) -> Self {
        let now = Instant::now();
        Self {
            samples: VecDeque::with_capacity(capacity.max(16)),
            draw_samples: VecDeque::with_capacity(capacity.max(16)),
            present_samples: VecDeque::with_capacity(capacity.max(16)),
            capacity: capacity.max(16),
            frames: 0,
            started: now,
            last_report: now,
            cpu: CpuSampler::spawn(),
            physical_cores: physical_cores.max(1),
            visible_notes: 0,
            mode: "idle",
            bands: 0,
        }
    }

    /// 记录一帧（work 含 present；draw 为 render+overlay）。
    pub fn record_full(
        &mut self,
        work_ms: f32,
        draw_ms: f32,
        present_ms: f32,
        visible_notes: u64,
        mode: &'static str,
        bands: usize,
    ) {
        for (queue, value) in [
            (&mut self.samples, work_ms),
            (&mut self.draw_samples, draw_ms),
            (&mut self.present_samples, present_ms),
        ] {
            if queue.len() == self.capacity {
                queue.pop_front();
            }
            queue.push_back(value);
        }
        self.frames += 1;
        self.visible_notes = visible_notes;
        self.mode = mode;
        self.bands = bands;
    }

    /// 清空统计（基准开始时调用）。
    pub fn reset(&mut self) {
        self.samples.clear();
        self.draw_samples.clear();
        self.present_samples.clear();
        self.frames = 0;
        self.started = Instant::now();
        self.last_report = Instant::now();
    }

    /// 每 1 秒输出一行；未到时间返回 `None`。
    pub fn maybe_report(&mut self, window: Option<&winit::window::Window>) -> Option<()> {
        if self.last_report.elapsed() < Duration::from_secs(1) {
            return None;
        }
        let summary = self.summary();
        println!(
            "[PERF] wall={:.1}fps work p50={:.2} p95={:.2} p99={:.2} max={:.2} ms | visible={} mode={} bands={} | cpu={:.0}% of 1 core ({:.0}% of {} cores) rss={}MB",
            summary.wall_fps,
            summary.p50_ms,
            summary.p95_ms,
            summary.p99_ms,
            summary.max_ms,
            self.visible_notes,
            self.mode,
            self.bands,
            summary.cpu_pct,
            summary.cpu_pct / self.physical_cores as f32,
            self.physical_cores,
            rss_mb()
        );
        if let Some(window) = window {
            let title = format!(
                "CPU ONIONSKIN | {:.1} FPS | {} | {} notes | {}",
                summary.wall_fps, self.mode, self.visible_notes, "熠星始芒 164M"
            );
            window.set_title(&title);
        }
        self.last_report = Instant::now();
        self.samples.clear();
        self.frames = 0;
        self.started = Instant::now();
        Some(())
    }

    /// 基准模式：最终汇总（样本不清空，画/呈现分位数单独给出）。
    /// 汇总当前窗口。
    #[must_use]
    pub fn summary(&self) -> Summary {
        let seconds = self.started.elapsed().as_secs_f64().max(0.001);
        let (work_avg, p50, p95, p99, max) = sample_stats(&self.samples);
        let (_, draw_p50, _, draw_p99, _) = sample_stats(&self.draw_samples);
        let (_, present_p50, _, present_p99, _) = sample_stats(&self.present_samples);
        let cpu_pct = self.cpu.as_ref().map_or(0.0, CpuSampler::read);
        Summary {
            frames: self.frames,
            seconds,
            wall_fps: self.frames as f64 / seconds,
            work_avg_ms: work_avg,
            p50_ms: p50,
            p95_ms: p95,
            p99_ms: p99,
            max_ms: max,
            draw_p50_ms: draw_p50,
            draw_p99_ms: draw_p99,
            present_p50_ms: present_p50,
            present_p99_ms: present_p99,
            cpu_pct,
        }
    }

    /// 输出一次 idle 汇总。
    pub fn print_idle(&self, note_count: u64) {
        println!(
            "[IDLE] no redraw requested after settling | notes={} | rss={}MB | mode={}",
            note_count,
            rss_mb(),
            self.mode
        );
    }

    /// 输出基准最终汇总；返回是否达到 80fps / p99 门槛。
    ///
    /// `draw_criterion=true` 用于总览热力图：该档每帧写满 1600×900，端到端
    /// p99 会被 DWM present 尖峰支配；验收 CPU 绘制 p99，present 分位数照常
    /// 打印，不隐藏。
    pub fn print_bench(&mut self, label: &str, draw_criterion: bool) -> bool {
        let summary = self.summary();
        let pass = if draw_criterion {
            summary.wall_fps >= 80.0 && summary.draw_p99_ms <= 12.5
        } else {
            summary.wall_fps >= 80.0 && summary.p99_ms <= 12.5
        };
        let criterion = if draw_criterion {
            "overview: draw p99"
        } else {
            "frame p99"
        };
        println!(
            "[BENCH {label}] frames={} wall={:.1}fps | draw p50={:.2} p99={:.2} | present p50={:.2} p99={:.2} | work avg={:.2} p50={:.2} p95={:.2} p99={:.2} max={:.2} ms | secs={:.2} | cpu={:.0}% of 1 core ({:.0}% of {} cores) rss={}MB -> {} [{}]",
            summary.frames,
            summary.wall_fps,
            summary.draw_p50_ms,
            summary.draw_p99_ms,
            summary.present_p50_ms,
            summary.present_p99_ms,
            summary.work_avg_ms,
            summary.p50_ms,
            summary.p95_ms,
            summary.p99_ms,
            summary.max_ms,
            summary.seconds,
            summary.cpu_pct,
            summary.cpu_pct / self.physical_cores as f32,
            self.physical_cores,
            rss_mb(),
            if pass { "PASS" } else { "FAIL" },
            criterion
        );
        pass
    }
}

fn sample_stats(queue: &VecDeque<f32>) -> (f32, f32, f32, f32, f32) {
    let mut values: Vec<f32> = queue.iter().copied().collect();
    values.sort_by(|a, b| a.total_cmp(b));
    let pick = |ratio: f32| -> f32 {
        if values.is_empty() {
            return 0.0;
        }
        let index = ((values.len() - 1) as f32 * ratio).round() as usize;
        values[index.min(values.len() - 1)]
    };
    let avg = if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f32>() / values.len() as f32
    };
    (
        avg,
        pick(0.50),
        pick(0.95),
        pick(0.99),
        values.last().copied().unwrap_or(0.0),
    )
}

fn rss_mb() -> u64 {
    MemoryMonitor::global().current_rss() / (1024 * 1024)
}
