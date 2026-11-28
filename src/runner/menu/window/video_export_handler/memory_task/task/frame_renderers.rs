//! 导出期可选渲染器：计数器 / 数据曲线 / MidiConsole。
//!
//! 三条路径互斥且都可缺省；构造失败一律降级（内置点阵或不渲染），
//! 导出流程本身不因渲染器缺失而中断。

use super::*;

/// 可选渲染器与统计状态；`None` 表示该模式未启用或已降级。
pub(super) struct FrameRenderers {
    pub(super) counter_stats: Option<CounterStats>,
    pub(super) counter_renderer: Option<CounterFontRenderer>,
    pub(super) csv_writer: Option<std::io::BufWriter<std::fs::File>>,
    pub(super) data_curve_renderer: Option<DataCurveRenderer>,
    pub(super) midi_console_renderer: Option<MidiConsoleRenderer>,
}

impl FrameRenderers {
    /// 按配置装配渲染器；计数器与数据曲线共用同一份统计状态。
    pub(super) fn new(
        document: &lumino_midi_loader::MidiDocument,
        counter_config: Option<&CounterRenderConfig>,
        data_curve_config: Option<&DataCurveRenderConfig>,
        midi_console_config: Option<&MidiConsoleRenderConfig>,
        fps_f64: f64,
    ) -> Self {
        let mut renderers = Self {
            counter_stats: None,
            counter_renderer: None,
            csv_writer: None,
            data_curve_renderer: None,
            midi_console_renderer: None,
        };

        // 统计状态：计数器与数据曲线共用同一数据源
        if counter_config.is_some() || data_curve_config.is_some() {
            let mut stats = CounterStats::default();
            stats.reset(document);
            renderers.counter_stats = Some(stats);
        }
        if let Some(cfg) = counter_config {
            renderers.init_counter(cfg);
        }
        if let Some(cfg) = data_curve_config {
            renderers.init_data_curve(cfg, fps_f64);
        }

        // MidiConsole 风格渲染器（全文档模式，需完整 MIDI 数据）
        renderers.midi_console_renderer =
            midi_console_config.map(|cfg| MidiConsoleRenderer::new(document, cfg));
        renderers
    }

    /// 计数器：字体渲染器 + 可选 CSV 写入器。
    fn init_counter(&mut self, cfg: &CounterRenderConfig) {
        // 字体渲染器：TTF 加载失败时回退内置点阵（导出流程不中断）
        match CounterFontRenderer::new(&cfg.font, cfg.font_size) {
            Ok(r) => {
                tracing::info!("计数器字体加载成功：{}", r.describe());
                self.counter_renderer = Some(r);
            }
            Err(e) => {
                tracing::warn!("计数器字体加载失败（回退内置点阵）: {e}");
                self.counter_renderer = match CounterFontRenderer::new(
                    &lumino_message::events::window::video::CounterFont::Bitmap,
                    cfg.font_size,
                ) {
                    Ok(r) => Some(r),
                    Err(fallback_e) => {
                        // 内置点阵理论上不会失败；若异常（资源/渲染初始化问题），
                        // 降级为不渲染计数器，避免导出任务崩溃。
                        tracing::error!("内置点阵字体渲染器也失败（计数器将不渲染）: {fallback_e}");
                        None
                    }
                };
            }
        }
        if cfg.save_csv && !cfg.csv_output.as_os_str().is_empty() {
            match std::fs::File::create(&cfg.csv_output) {
                Ok(f) => self.csv_writer = Some(std::io::BufWriter::new(f)),
                Err(e) => tracing::warn!("计数器 CSV 文件创建失败: {e}"),
            }
        }
    }

    /// 数据曲线：渲染器（窗口帧数由 fps 决定），失败同样回退内置点阵。
    fn init_data_curve(&mut self, cfg: &DataCurveRenderConfig, fps_f64: f64) {
        let fps_u32 = fps_f64.max(1.0) as u32;
        match DataCurveRenderer::new(cfg, fps_u32) {
            Ok(r) => {
                tracing::info!("数据曲线渲染器就绪（窗口 {} 帧）", r.window_cap());
                self.data_curve_renderer = Some(r);
            }
            Err(e) => {
                tracing::warn!("数据曲线字体加载失败（回退内置点阵）: {e}");
                let fallback = DataCurveRenderConfig {
                    font: lumino_message::events::window::video::CounterFont::Bitmap,
                    ..cfg.clone()
                };
                self.data_curve_renderer = match DataCurveRenderer::new(&fallback, fps_u32) {
                    Ok(r) => Some(r),
                    Err(fallback_e) => {
                        // 内置点阵理论上不会失败；异常时降级为不渲染数据曲线。
                        tracing::error!(
                            "内置点阵数据曲线渲染器也失败（数据曲线将不渲染）: {fallback_e}"
                        );
                        None
                    }
                };
            }
        }
    }
}
