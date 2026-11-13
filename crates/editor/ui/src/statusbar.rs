pub mod performance;

use lumino_core::storage::config::GpuLoadDisplay;
use lumino_extras::i18n::{Language, main_translations};
use lumino_midi_io::GpuBufferStats;
use lumino_ui_core::button_descs::ButtonId;

use performance::PerfData;

/// egui 叠加层用的状态栏快照（与 egui 绘制同源，避免两套规则漂移）。
#[derive(Debug, Clone)]
pub(crate) struct StatusSnapshot {
    /// 左侧描述文本。
    pub left_text: String,
    /// 左侧文本是否用强调色（FPS 高亮样式）。
    pub left_accent: bool,
    /// CPU 占用百分比。
    pub cpu_percent: f32,
    /// 进程内存（MB）。
    pub memory_mb: f32,
    /// 平滑后的 FPS；`None` = 尚未采样。
    pub fps: Option<f32>,
    /// 右侧补充文本。
    pub right_text: String,
    /// GPU (LGS) 指标组；`None` = 不显示。
    pub gpu: Option<GpuSnapshot>,
}

/// 状态栏 GPU 缓冲状态指示器的显示参数（由 Root 按用户设置构造）。
#[derive(Debug, Clone, Copy)]
pub struct GpuStatusFlags {
    /// 总开关（关闭 = 整个 GPU 指标组隐藏）
    pub enabled: bool,
    /// 是否显示 GOV/DROP 治理指标段（防爆治理配套）
    pub show_governor: bool,
    /// LOAD 显示口径（瞬时 / 3 秒短窗平均）
    pub load_mode: GpuLoadDisplay,
}

impl Default for GpuStatusFlags {
    fn default() -> Self {
        Self {
            enabled: true,
            show_governor: true,
            load_mode: GpuLoadDisplay::default(),
        }
    }
}

/// 队列深度趋势（与上一次 4Hz 采样比较，±10ms 内视为持平）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QueueTrend {
    /// 上升（缓冲变深）
    Up,
    /// 下降（缓冲变浅，趋向欠载）
    Down,
    /// 持平
    #[default]
    Flat,
}

/// 队列趋势：与上一采样比较（±10ms 内视为持平，抑制 4Hz 抖动）。
pub(crate) fn queue_trend(prev_ms: f64, cur_ms: f64) -> QueueTrend {
    const EPSILON_MS: f64 = 10.0;
    if cur_ms > prev_ms + EPSILON_MS {
        QueueTrend::Up
    } else if cur_ms < prev_ms - EPSILON_MS {
        QueueTrend::Down
    } else {
        QueueTrend::Flat
    }
}

/// 状态栏显示的信息
#[derive(Debug, Clone, Default)]
pub struct StatusInfo {
    /// 左侧状态文本
    pub left_text: String,
    /// 右侧状态文本
    pub right_text: String,
}

pub struct StatusBar {
    info: StatusInfo,
    /// FPS 值（显示在底部状态栏右侧）
    fps: Option<f32>,
    /// 性能监控数据
    perf_data: PerfData,
    /// 悬停工具栏按钮时显示的描述文字（左侧预留区）
    ///
    /// 由 `set_hover_label` 写入按钮角色标识：鼠标悬停时显示
    /// `按钮名 - {解释说明}`，离开时清空。
    hover_label: Option<ButtonId>,
    /// GPU (LGS) 缓冲状态快照（None = 非 LGS/未就绪，指示器隐藏）
    gpu_stats: Option<GpuBufferStats>,
    /// 队列趋势（与上一次注入的快照比较）
    queue_trend: QueueTrend,
}

/// GPU (LGS) 指标快照（UI 无关：供 egui 状态栏装配模型）。
#[derive(Debug, Clone, PartialEq)]
pub struct GpuSnapshot {
    /// 队列深度（毫秒）。
    pub queue_ms: f64,
    /// 队列趋势。
    pub trend: QueueTrend,
    /// 渲染负载（0..≈1+）。
    pub load: f64,
    /// 活跃声部数。
    pub voice_count: u64,
    /// 累计欠载次数。
    pub underruns: u64,
    /// 采样显存文本（`None` = 未加载音源）。
    pub sample_gpu: Option<String>,
    /// 采样内存文本（`None` = 未加载音源）。
    pub sample_cpu: Option<String>,
    /// 防爆治理级别（`None` = 未开启显示或为 0）。
    pub governor_level: Option<u32>,
    /// 被丢弃的过期 NoteOn 数（`None` = 未开启显示或为 0）。
    pub dropped: Option<u64>,
}

/// 字节 → MB 文本（1 位小数；0 显示 "0"）。REF-LGS-013 状态栏用。
fn mem_text(bytes: u64) -> String {
    if bytes == 0 {
        "0".to_string()
    } else {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    }
}

impl StatusBar {
    pub fn new() -> Self {
        Self {
            info: StatusInfo::default(),
            fps: None,
            perf_data: PerfData::default(),
            hover_label: None,
            gpu_stats: None,
            queue_trend: QueueTrend::Flat,
        }
    }

    /// 设置 FPS 值
    pub fn set_fps(&mut self, fps: f32) {
        self.fps = Some(fps);
    }

    /// 设置性能监控数据
    pub fn set_perf_data(&mut self, data: PerfData) {
        self.perf_data = data;
    }

    /// 注入 GPU 缓冲状态快照；返回是否有可见变化（供调用方标脏重绘）。
    ///
    /// `None` 表示当前不是 LGS（或后端未就绪），指示器隐藏。
    /// 队列趋势基于与上一次快照的比较更新（仅在两次都有快照时有意义）。
    pub fn set_gpu_stats(&mut self, stats: Option<GpuBufferStats>) -> bool {
        let new_trend = match (self.gpu_stats, stats) {
            (Some(old), Some(new)) => queue_trend(old.queue_ms, new.queue_ms),
            _ => QueueTrend::Flat,
        };
        let changed = self.gpu_stats != stats || self.queue_trend != new_trend;
        self.gpu_stats = stats;
        self.queue_trend = new_trend;
        changed
    }

    /// 设置/清除悬停描述文字。
    ///
    /// `Some(id)` 表示鼠标正悬停在标识为 `id` 的工具栏按钮上；
    /// `None` 表示鼠标已离开，清空描述区。
    pub fn set_hover_label(&mut self, label: Option<ButtonId>) {
        self.hover_label = label;
    }

    /// 设置左侧状态消息（如"文件已经保存"），`None` 恢复默认"就绪"。
    ///
    /// 悬停工具栏按钮时仍优先显示按钮解释文字（`set_hover_label`），
    /// 悬停离开后恢复显示此消息。
    pub fn set_status_message(&mut self, msg: Option<String>) {
        match msg {
            Some(msg) => self.info.left_text = msg,
            None => self.info.left_text.clear(),
        }
    }

    /// 是否处于 FPS 高亮样式（仅当左侧无描述文字且存在 FPS 值时）
    pub fn use_fps_style(&self) -> bool {
        self.hover_label.is_none() && self.info.left_text.is_empty() && self.fps.is_some()
    }

    /// egui 叠加层快照：左文本规则与 [`Self::view`] 完全一致。
    /// GPU 指标快照：设置关闭或后端未就绪时返回 `None`。
    pub(crate) fn gpu_snapshot(&self, flags: GpuStatusFlags) -> Option<GpuSnapshot> {
        if !flags.enabled {
            return None;
        }
        let stats = self.gpu_stats?;
        let load = match flags.load_mode {
            GpuLoadDisplay::ShortAverage => stats.average_render_load,
            GpuLoadDisplay::Instant => stats.render_load,
        };
        let has_samples = stats.sample_gpu_bytes > 0 || stats.sample_cpu_bytes > 0;
        Some(GpuSnapshot {
            queue_ms: stats.queue_ms,
            trend: self.queue_trend,
            load,
            voice_count: stats.voice_count,
            underruns: stats.underruns,
            sample_gpu: has_samples.then(|| mem_text(stats.sample_gpu_bytes)),
            sample_cpu: has_samples.then(|| mem_text(stats.sample_cpu_bytes)),
            governor_level: (flags.show_governor && stats.governor_level > 0)
                .then_some(stats.governor_level as u32),
            dropped: (flags.show_governor && stats.dropped_note_ons > 0)
                .then_some(stats.dropped_note_ons),
        })
    }

    pub(crate) fn egui_snapshot(
        &self,
        language: Language,
        note_count: Option<usize>,
        gpu: GpuStatusFlags,
    ) -> StatusSnapshot {
        let translations = main_translations(language);
        let left_text = if let Some(id) = self.hover_label {
            let (name, desc) = lumino_ui_core::button_descs::button_desc(id, language);
            format!("{} - {}", name, desc)
        } else if !self.info.left_text.is_empty() {
            self.info.left_text.clone()
        } else if let Some(count) = note_count {
            format!(
                "{}: {}",
                translations.note_count_label,
                lumino_extras::i18n::format_thousands(count as u64)
            )
        } else {
            translations.status_ready.to_string()
        };

        StatusSnapshot {
            left_text,
            left_accent: self.use_fps_style(),
            cpu_percent: self.perf_data.cpu_usage,
            memory_mb: self.perf_data.memory_mb,
            fps: self.fps,
            right_text: self.info.right_text.clone(),
            gpu: self.gpu_snapshot(gpu),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 状态消息：设置后显示，清除后恢复空（view 回退"就绪"）
    #[test]
    fn test_set_status_message() {
        let mut bar = StatusBar::new();
        assert!(bar.info.left_text.is_empty());

        bar.set_status_message(Some("文件已经保存".to_string()));
        assert_eq!(bar.info.left_text, "文件已经保存");

        bar.set_status_message(None);
        assert!(bar.info.left_text.is_empty());
    }

    /// 队列趋势判定带 ±10ms 死区（抑制 4Hz 采样抖动）。
    #[test]
    fn test_queue_trend_epsilon() {
        assert_eq!(queue_trend(100.0, 105.0), QueueTrend::Flat);
        assert_eq!(queue_trend(100.0, 90.0), QueueTrend::Flat);
        assert_eq!(queue_trend(100.0, 111.0), QueueTrend::Up);
        assert_eq!(queue_trend(100.0, 89.0), QueueTrend::Down);
        assert_eq!(queue_trend(0.0, 0.0), QueueTrend::Flat);
    }

    /// 快照注入的变更检测与趋势更新（相同快照不重复标脏）。
    #[test]
    fn test_set_gpu_stats_change_detection_and_trend() {
        let mut bar = StatusBar::new();
        let stats = GpuBufferStats {
            queue_ms: 100.0,
            ..GpuBufferStats::default()
        };
        assert!(bar.set_gpu_stats(Some(stats)), "首次注入应视为变化");
        assert!(!bar.set_gpu_stats(Some(stats)), "相同快照不应重复标脏");

        let deeper = GpuBufferStats {
            queue_ms: 200.0,
            ..stats
        };
        assert!(bar.set_gpu_stats(Some(deeper)), "队列变化应标脏");
        assert_eq!(bar.queue_trend, QueueTrend::Up, "队列变深 → 上升箭头");

        assert!(bar.set_gpu_stats(None), "清空（切非 LGS）应标脏");
        assert!(bar.gpu_stats.is_none(), "快照应已被清空");
    }
}
