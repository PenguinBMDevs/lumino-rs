pub mod performance;

use iced_core::Length;
use iced_core::widget::text::Wrapping;
use iced_widget::{button, container, row, space, text};
use lumino_core::storage::config::GpuLoadDisplay;
use lumino_extras::i18n::{Language, main_translations};
use lumino_midi_io::GpuBufferStats;
use lumino_ui_core::button_descs::ButtonId;

use super::Element;
use crate::Theme;
use performance::PerfData;

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
pub(crate) enum QueueTrend {
    /// 上升（缓冲变深）
    Up,
    /// 下降（缓冲变浅，趋向欠载）
    Down,
    /// 持平
    #[default]
    Flat,
}

impl QueueTrend {
    /// 箭头显示字符。
    pub(crate) fn arrow(self) -> &'static str {
        match self {
            QueueTrend::Up => "↑",
            QueueTrend::Down => "↓",
            QueueTrend::Flat => "→",
        }
    }
}

/// 指标颜色档位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetricTone {
    /// 常规（强调色）
    Normal,
    /// 警告（黄）
    Warning,
    /// 危险（红）
    Danger,
}

/// 负载分级：<60% 常规；60–90% 警告；≥90% 危险（1.0 = 90% 实时预算上限）。
pub(crate) fn load_tone(load: f64) -> MetricTone {
    if load >= 0.9 {
        MetricTone::Danger
    } else if load >= 0.6 {
        MetricTone::Warning
    } else {
        MetricTone::Normal
    }
}

/// 欠载分级：0 = 常规；>0 = 危险（欠载即已静音丢音）。
pub(crate) fn underrun_tone(underruns: u64) -> MetricTone {
    if underruns == 0 {
        MetricTone::Normal
    } else {
        MetricTone::Danger
    }
}

/// 治理级别分级：0=常规 / 1=警告 / ≥2=危险（2=Overload / 3=Emergency）。
pub(crate) fn governor_tone(level: u64) -> MetricTone {
    match level {
        0 => MetricTone::Normal,
        1 => MetricTone::Warning,
        _ => MetricTone::Danger,
    }
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

    pub fn view<'a>(
        &'a self,
        language: Language,
        note_count: Option<usize>,
        gpu: GpuStatusFlags,
    ) -> Element<'a> {
        let translations = main_translations(language);

        // 左侧描述区：优先显示悬停按钮的 `按钮名 - {解释说明}`，
        // 其次为显式 left_text，再其次为音符总量（UI-015，下边栏模式），
        // 最后为"就绪"
        let left_text: String = if let Some(id) = self.hover_label {
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
        let use_fps_style = self.use_fps_style();

        // 左侧预留描述区（固定宽度，避免 hover 时布局跳动）
        // 说明文字强制单行显示：显式 `Wrapping::None` 关闭自动折行（iced 的
        // `Text` 默认 `Wrapping::Word`，长描述会在 220px 内折成多行，把 20px 高的
        // 状态栏“纵向拉长”），同时固定文本宽度为 220px 使其超出部分被裁剪。
        let left_section: Element<'a> = container(
            text(left_text)
                .size(12)
                .width(Length::Fixed(220.0))
                .wrapping(Wrapping::None)
                .style(move |theme: &Theme| {
                    if use_fps_style {
                        let palette = theme.extended_palette();
                        text::Style {
                            color: Some(palette.primary.strong.color),
                        }
                    } else {
                        text::Style::default()
                    }
                }),
        )
        .width(Length::Fixed(220.0))
        .into();

        // 右侧性能指标组：CPU / MEM / FPS（仿照 yinhe mode_bar 底部栏的 metric 设计：
        // label 用弱色、value 用强调色；MEM 可点击打开内存监控对话框）
        let perf_section: Element<'a> = if let Some(fps) = self.fps {
            row![
                metric_label("CPU"),
                metric_value(format!("{:.1}%", self.perf_data.cpu_usage)),
                space().width(12),
                metric_label("MEM"),
                metric_clickable_value(
                    format!("{:.1} MB", self.perf_data.memory_mb),
                    crate::toolbar::Event::open_memory_monitor_dialog(),
                ),
                space().width(12),
                metric_label("FPS"),
                metric_value(format!("{:.1}", fps)),
            ]
            .align_y(iced_core::Alignment::Center)
            .into()
        } else {
            iced_widget::Space::new().into()
        };

        // GPU (LGS) 缓冲状态组——仅当设置开关开启且存在快照
        // （LGS 后端）时显示；插在 CPU/MEM/FPS 组左侧。
        let gpu_section: Option<Element<'a>> = if gpu.enabled {
            self.gpu_stats
                .map(|stats| build_gpu_section(stats, self.queue_trend, gpu))
        } else {
            None
        };

        let mut metrics_row = row![left_section, iced_widget::space().width(Length::Fill)];
        if let Some(gpu_section) = gpu_section {
            metrics_row = metrics_row.push(gpu_section);
            metrics_row = metrics_row.push(iced_widget::space().width(12));
        }
        metrics_row = metrics_row.push(perf_section);
        metrics_row = metrics_row.push(text(&self.info.right_text).size(12));

        container(
            metrics_row
                .spacing(8)
                .padding([0, 8])
                .align_y(iced_core::Alignment::Center),
        )
        .width(Length::Fill)
        .height(20)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default().background(palette.background.weak.color)
        })
        .into()
    }
}

/// 构建 GPU 指标组（Q 队列毫秒 + 趋势箭头 / LOAD / V / UR /
/// 采样内存 GMEM/CMEM，异常时附 GOV、DROP）。
fn build_gpu_section<'a>(
    stats: GpuBufferStats,
    trend: QueueTrend,
    flags: GpuStatusFlags,
) -> Element<'a> {
    let load = match flags.load_mode {
        GpuLoadDisplay::ShortAverage => stats.average_render_load,
        GpuLoadDisplay::Instant => stats.render_load,
    };

    // Q 毫秒整数：量程 0–600ms（容量按时间封顶），毫秒分辨率才能区分
    // 「缓冲还稳」与「下一块就断」；数据源本身即块级/回调级粒度（个位数 ms）。
    let queue_metric = format!("Q {:.0}ms{}", stats.queue_ms, trend.arrow());

    let mut row = row![
        metric_label("GPU"),
        metric_value(queue_metric),
        space().width(8),
        metric_label("LOAD"),
        metric_value_toned(format!("{:.0}%", load * 100.0), load_tone(load)),
        space().width(8),
        metric_label("V"),
        metric_value(stats.voice_count.to_string()),
        space().width(8),
        metric_label("UR"),
        metric_value_toned(stats.underruns.to_string(), underrun_tone(stats.underruns)),
    ]
    .spacing(6)
    .align_y(iced_core::Alignment::Center);

    // REF-LGS-013：采样内存（GPU / CPU）。未加载音源时为 0，不占位。
    if stats.sample_gpu_bytes > 0 || stats.sample_cpu_bytes > 0 {
        row = row.push(space().width(8));
        row = row.push(metric_label("GMEM"));
        row = row.push(metric_value(mem_mb(stats.sample_gpu_bytes)));
        row = row.push(space().width(8));
        row = row.push(metric_label("CMEM"));
        row = row.push(metric_value(mem_mb(stats.sample_cpu_bytes)));
    }

    // 防爆治理指标：仅在异常（级别 >0 或发生过丢弃）时出现，
    // 且受「显示防爆治理指标」副开关控制。
    if flags.show_governor {
        if stats.governor_level > 0 {
            row = row.push(space().width(8));
            row = row.push(metric_label("GOV"));
            row = row.push(metric_value_toned(
                stats.governor_level.to_string(),
                governor_tone(stats.governor_level),
            ));
        }
        if stats.dropped_note_ons > 0 {
            row = row.push(space().width(8));
            row = row.push(metric_label("DROP"));
            row = row.push(metric_value_toned(
                stats.dropped_note_ons.to_string(),
                MetricTone::Danger,
            ));
        }
    }

    row.into()
}

/// 字节 → MB 文本（1 位小数；0 显示 "0"）。REF-LGS-013 状态栏用。
fn mem_mb(bytes: u64) -> String {
    if bytes == 0 {
        "0".to_string()
    } else {
        format!("{:.1}M", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// 指标标签（弱色小字，仿照 yinhe mode_bar 的 `metric` 标签）
fn metric_label<'a>(label: &'a str) -> Element<'a> {
    text(label)
        .size(12)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            text::Style {
                color: Some(palette.background.weak.text),
            }
        })
        .into()
}

/// 指标数值（强调色，仿照 yinhe mode_bar 的 `metric` 数值）
fn metric_value<'a>(value: String) -> Element<'a> {
    text(value)
        .size(12)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            text::Style {
                color: Some(palette.primary.strong.color),
            }
        })
        .into()
}

/// 按颜色档位渲染的指标数值（常规=强调色 / 警告=黄 / 危险=红）。
fn metric_value_toned<'a>(value: String, tone: MetricTone) -> Element<'a> {
    text(value)
        .size(12)
        .style(move |theme: &Theme| {
            let palette = theme.extended_palette();
            let color = match tone {
                MetricTone::Normal => palette.primary.strong.color,
                MetricTone::Warning => palette.warning.base.color,
                MetricTone::Danger => palette.danger.strong.color,
            };
            text::Style { color: Some(color) }
        })
        .into()
}

/// 可点击的指标数值（强调色，点击发送消息，仿照 yinhe `metric_clickable`）
fn metric_clickable_value<'a>(value: String, on_press: crate::Message) -> Element<'a> {
    button(text(value).size(12).style(|theme: &Theme| {
        let palette = theme.extended_palette();
        text::Style {
            color: Some(palette.primary.strong.color),
        }
    }))
    .on_press(on_press)
    .padding([0.0, 0.0])
    .style(|theme: &Theme, _status| {
        let palette = theme.extended_palette();
        button::Style {
            background: None,
            text_color: palette.primary.strong.color,
            border: iced_core::Border::default(),
            shadow: Default::default(),
            snap: false,
        }
    })
    .into()
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

    /// 负载颜色分级阈值（60% / 90%）。
    #[test]
    fn test_load_tone_thresholds() {
        assert_eq!(load_tone(0.0), MetricTone::Normal);
        assert_eq!(load_tone(0.59), MetricTone::Normal);
        assert_eq!(load_tone(0.6), MetricTone::Warning);
        assert_eq!(load_tone(0.89), MetricTone::Warning);
        assert_eq!(load_tone(0.9), MetricTone::Danger);
        assert_eq!(load_tone(2.5), MetricTone::Danger, ">100% 仍为危险档");
    }

    /// 欠载/治理级别分级。
    #[test]
    fn test_underrun_and_governor_tone() {
        assert_eq!(underrun_tone(0), MetricTone::Normal);
        assert_eq!(underrun_tone(3), MetricTone::Danger);
        assert_eq!(governor_tone(0), MetricTone::Normal);
        assert_eq!(governor_tone(1), MetricTone::Warning);
        assert_eq!(governor_tone(2), MetricTone::Danger);
        assert_eq!(governor_tone(3), MetricTone::Danger);
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

    /// REF-LGS-013：采样内存显示为 MB、1 位小数；0 不显示单位。
    #[test]
    fn test_mem_mb_format() {
        assert_eq!(mem_mb(0), "0");
        assert_eq!(mem_mb(1024 * 1024), "1.0M");
        assert_eq!(mem_mb(3 * 1024 * 1024 / 2), "1.5M");
        assert_eq!(mem_mb(17 * 1024 * 1024 + 1024 * 1024 / 10), "17.1M");
    }
}
