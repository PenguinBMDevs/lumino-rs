//! 基准支撑：进程级堆内存统计分配器、单操作统计、MIDI 加载与 200W 工作负载构建
//!
//! 由 `id_history_ops_bench.rs` 拆出（保持单文件 < 400 行）。

use std::alloc::{GlobalAlloc, Layout, System};
use std::env;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use lumino_midi_loader::{MidiDocument, NoteEvent};

/// 默认基准 MIDI（用户指定）。
pub const DEFAULT_MIDI: &str = r"D:\BM-DATA\MIDI File\Toilet Story 6 F2.mid";
/// 默认工作负载规模：200W 音符。
pub const DEFAULT_NOTES: usize = 2_000_000;
/// 默认连续循环轮数（另加 1 轮暖机）
pub const DEFAULT_CYCLES: usize = 7;
/// 单项操作耗时硬指标（ms）。
pub const TARGET_MS: f64 = 100.0;
/// 内存上升硬指标（MB，单次操作峰值堆增量）。
pub const TARGET_MEM_MB: f64 = 150.0;

// ── 进程级堆内存统计分配器 ─────────────────────────────────────────────

struct CountingAlloc;

/// 当前存活堆字节数
static LIVE: AtomicUsize = AtomicUsize::new(0);
/// 观测到的存活堆字节峰值（可在区间起点重置）
static PEAK: AtomicUsize = AtomicUsize::new(0);
/// 全程存活堆字节峰值（跨所有操作，不重置）
static RUN_PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let now = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
            RUN_PEAK.fetch_max(now, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

/// 当前存活堆字节
pub fn live() -> usize {
    LIVE.load(Ordering::Relaxed)
}

/// 重置区间峰值
pub fn reset_peak() {
    PEAK.store(live(), Ordering::Relaxed);
}

/// 重置全程峰值到当前存活量（加载等准备阶段结束后调用）
pub fn reset_run_peak() {
    RUN_PEAK.store(live(), Ordering::Relaxed);
}

/// 当前区间峰值
pub fn peak() -> usize {
    PEAK.load(Ordering::Relaxed)
}

/// 全程峰值（每次 alloc 后更新，等价于任意时刻存活堆的最大值）
pub fn run_peak() -> usize {
    RUN_PEAK.load(Ordering::Relaxed)
}

/// 字节 → MB
pub fn mb(bytes: i64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

// ── 单操作统计 ─────────────────────────────────────────────────────────

#[derive(Default)]
pub struct Stat {
    name: &'static str,
    times: Vec<f64>,
    peak_growth: Vec<i64>,
}

impl Stat {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            ..Default::default()
        }
    }

    /// 计时 + 单次操作内存增长测量一次执行。
    pub fn measure<T>(&mut self, f: impl FnOnce() -> T) -> T {
        let before = live();
        reset_peak();
        let t0 = Instant::now();
        let out = f();
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        self.times.push(ms);
        let op_peak = peak();
        self.peak_growth.push(op_peak as i64 - before as i64);
        out
    }

    fn max(&self) -> f64 {
        self.times.iter().copied().fold(0.0f64, f64::max)
    }

    /// 全部样本耗时（ms）——外部基准（`ui_boxselect_copy_bench`）复用统计结构时读取
    #[allow(dead_code)]
    pub fn times(&self) -> &[f64] {
        &self.times
    }

    /// 本操作历次执行的最大峰值堆增量（字节）
    #[allow(dead_code)]
    pub fn max_peak_growth(&self) -> i64 {
        self.peak_growth.iter().copied().max().unwrap_or(0)
    }

    /// 最小值（确定性代码成本的近似下界，供参考）
    fn min(&self) -> f64 {
        self.times.iter().copied().fold(f64::INFINITY, f64::min)
    }

    /// 中位数（判定基准：共享开发机的单轮最大受调度噪声支配，中位反映真实操作成本）
    fn median(&self) -> f64 {
        if self.times.is_empty() {
            return 0.0;
        }
        let mut sorted = self.times.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        sorted[sorted.len() / 2]
    }

    /// 输出一行报告，返回 (耗时达标, 内存达标)。
    ///
    /// 耗时判定取**中位数**（共享开发机单轮最大受调度噪声支配；最大/平均附于同行为参考），
    /// 内存判定取本操作自身的峰值堆增量（相对操作开始前）——对应性能线
    /// 「操作用时 ≤ 100ms、内存上升 ≤ 150MB」的**单次操作**预算；历史快照等
    /// 跨操作保留状态不计入单次操作预算（另在总览以绝对峰值信息性报道）。
    pub fn report(&self) -> (bool, bool) {
        if env::var_os("LUMINO_BENCH_VERBOSE").is_some() {
            let samples: Vec<String> = self.times.iter().map(|t| format!("{t:.1}")).collect();
            println!("  [明细] {}: {} ms", self.name, samples.join(", "));
        }
        let time_pass = self.median() <= TARGET_MS;
        let mem_pass = mb(self.max_peak_growth()) <= TARGET_MEM_MB;
        println!(
            "{:<28} | 中位 {:>8.2} ms | 最小 {:>8.2} ms | 最大 {:>8.2} ms | 内存峰值增量 {:>7.1} MB | {}",
            self.name,
            self.median(),
            self.min(),
            self.max(),
            mb(self.max_peak_growth()),
            if time_pass && mem_pass {
                "✓ 达标"
            } else if !time_pass {
                "✗ 耗时超标"
            } else {
                "✗ 内存超标"
            }
        );
        (time_pass, mem_pass)
    }
}

/// 读取正整数环境变量（缺失/非法/为 0 时回退默认值）。
pub fn env_usize(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|&v| v > 0)
        .unwrap_or(default)
}

// ── MIDI 加载与工作负载构建 ────────────────────────────────────────────

/// 加载真实 MIDI；文件缺失/失败回退合成数据。
pub fn load_doc(path: &str) -> (MidiDocument, String) {
    if Path::new(path).exists() {
        match MidiDocument::from_notes_file(path, None) {
            Ok(doc) => return (doc, format!("real:{path}")),
            Err(e) => eprintln!("⚠ 加载真实 MIDI 失败: {e}，回退合成数据"),
        }
    } else {
        eprintln!("⚠ 文件不存在: {path}，回退合成数据");
    }
    (synth_doc(DEFAULT_NOTES), "synthetic".into())
}

/// 合成单轨工作负载（升序密集排布，分配全局唯一 id）。
fn synth_doc(n: usize) -> MidiDocument {
    let mut doc = MidiDocument::empty_with_tracks(1, 960);
    let mut notes: Vec<NoteEvent> = Vec::with_capacity(n);
    for i in 0..n as u32 {
        notes.push(NoteEvent::new(
            i,
            i.saturating_add(2),
            (i % 128) as u8,
            100,
            0,
        ));
    }
    doc.batch_insert_sorted_notes_with_ids(0, notes);
    doc
}

/// 打印每轨音符规模。
pub fn print_track_stats(doc: &MidiDocument) {
    println!("音轨规模（音符数）:");
    for t in 0..doc.track_count() {
        let len = doc.track_notes(t).len();
        if len > 0 {
            println!("  轨 {t:>2}: {len:>12}");
        }
    }
}

/// 取音符最多音轨构建目标规模工作负载（均匀采样，tick 覆盖全曲）；
/// `full=true` 时保留整份文档。
pub fn build_workload(
    mut doc: MidiDocument,
    target: usize,
    full: bool,
) -> (MidiDocument, usize, usize) {
    let track = (0..doc.track_count())
        .max_by_key(|&t| doc.track_notes(t).len())
        .unwrap_or(0);
    let track_len = doc.track_notes(track).len();
    if full || track_len <= target {
        return (doc, track, track_len);
    }
    // 均匀采样 target 条（step_by 保持 tick 升序，覆盖全曲 tick 区间）
    let step = track_len.div_ceil(target);
    let kept: Vec<NoteEvent> = doc
        .track_notes(track)
        .iter()
        .step_by(step)
        .take(target)
        .copied()
        .collect();
    for t in 0..doc.track_count() {
        if t != track {
            let _ = doc.clear_track_notes(t);
        }
    }
    let _ = doc.replace_track_notes(track, kept);
    let len = doc.track_notes(track).len();
    (doc, track, len)
}
