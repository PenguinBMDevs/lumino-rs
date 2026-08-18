//! 计数器模式 CPU 帧渲染
//!
//! 参考 Zenith-MIDI NoteCountRender（`Render.cs`）与 fmr NoteCounter（`Renderer.cs`）：
//! - 黑色背景（fmr 用 `FillRectangle(Black)` 清屏）
//! - 统计数值 + 模板替换（Zenith 全套占位符，见 `counter_template.rs`）
//! - 六种对齐方式（Zenith `Alignments`：TopLeft/TopRight/BottomLeft/BottomRight/
//!   TopSpread/BottomSpread），对齐布局见 `counter_frame_layout.rs`
//! - 白色前景文本（fmr 用 `DrawString(text, White)`），字号可配置
//!
//! 本模式不需要卷帘/键盘/标尺，仅绘制变化的数据文本。

use lumino_midi_loader::MidiDocument;

use super::counter_font::CounterFontRenderer;
use super::counter_frame_layout::draw_aligned_text;
use super::counter_stats::{CounterRenderConfig, CounterStats};
use super::counter_template::{TemplateContext, render_template};
use super::waterfall_frame::fill_bgra_black;

/// 单帧渲染输出。
pub struct CounterFrameOutput {
    /// 渲染到画面的文本（已替换占位符）
    pub text: String,
    /// CSV 行（启用 CSV 导出时由调用方写入文件）
    pub csv_line: Option<String>,
}

/// 绘制一个计数器帧（BGRA 像素，in-place 修改）。
///
/// 返回渲染文本与可选的 CSV 行；统计状态 `stats` 每帧推进一次。
/// `renderer` 为字体渲染器（TTF 后端内置 glyph 缓存，跨帧复用）。
#[allow(clippy::too_many_arguments)]
pub fn render_counter_frame(
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    document: &MidiDocument,
    tick: u32,
    ppq: u32,
    fps: u32,
    duration_secs: f64,
    config: &CounterRenderConfig,
    stats: &mut CounterStats,
    renderer: &mut CounterFontRenderer,
) -> CounterFrameOutput {
    stats.advance(document, tick, fps);

    let ctx = TemplateContext {
        config,
        stats,
        document,
        tick,
        ppq,
        fps,
        duration_secs,
    };
    let text = render_template(&ctx);
    let csv_line = if config.save_csv && !config.csv_output.as_os_str().is_empty() {
        Some(render_template(&TemplateContext {
            config: &CounterRenderConfig {
                text: config.csv_format.clone(),
                ..config.clone()
            },
            stats,
            document,
            tick,
            ppq,
            fps,
            duration_secs,
        }))
    } else {
        None
    };

    // 黑色背景（fmr：FillRectangle(Black)）
    fill_bgra_black(frame);
    if frame_width == 0 || frame_height == 0 || text.is_empty() {
        return CounterFrameOutput { text, csv_line };
    }

    // 白色前景（fmr：DrawString(text, White)），BGRA 白 = [255, 255, 255, 255]
    const WHITE: [u8; 4] = [255, 255, 255, 255];
    draw_aligned_text(
        frame,
        frame_width,
        frame_height,
        &text,
        config,
        renderer,
        WHITE,
    );

    CounterFrameOutput { text, csv_line }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_message::events::window::video::{CounterAlignment, CounterSeparator};
    use lumino_midi_loader::{NoteEvent, TrackManager};

    fn make_doc() -> MidiDocument {
        let notes = [(0u32, 480u32, 60u8), (480, 960, 62), (960, 1920, 64)];
        let mut list: Vec<NoteEvent> = notes
            .iter()
            .map(|&(s, e, k)| NoteEvent::new(s, e, k, 100, 0))
            .collect();
        list.sort_unstable_by_key(|n| n.start_tick);
        MidiDocument {
            notes: vec![lumino_midi_loader::ChunkedList::from_sorted(list)],
            tempo_changes: vec![(0, 120.0)],
            time_signatures: vec![(0, 4, 4)],
            key_signatures: vec![(0, 0, false)],
            control_events: lumino_midi_loader::ChunkedList::new(),
            lyrics: vec![],
            markers: vec![],
            sys_ex: vec![],
            track_names: vec![Some("T1".into())],
            total_ticks: 1920,
            track_count: 1,
            tracks: TrackManager::new(1),
            division: 480,
            track_ports: vec![],
            track_max_end_ticks: vec![],
        }
    }

    fn test_config() -> CounterRenderConfig {
        CounterRenderConfig {
            text: "Notes: {nc} / {tn}".to_string(),
            alignment: CounterAlignment::TopLeft,
            font_size: 14,
            font: lumino_message::events::window::video::CounterFont::Bitmap,
            separator: CounterSeparator::Nothing,
            padding_zeroes: false,
            bpm_int_pad: 3,
            bpm_dec_pad: 2,
            note_count_pad: 5,
            polyphony_pad: 3,
            nps_pad: 3,
            ticks_pad: 5,
            bars_pad: 3,
            frames_pad: 5,
            save_csv: false,
            csv_output: std::path::PathBuf::new(),
            csv_format: String::new(),
        }
    }

    /// 帧渲染：黑底 + 文本像素出现；统计推进生效
    #[test]
    fn test_render_counter_frame_basic() {
        let doc = make_doc();
        let config = test_config();
        let mut stats = CounterStats::default();
        stats.reset(&doc);

        let mut frame = vec![0u8; 200 * 40 * 4];
        let mut renderer =
            CounterFontRenderer::new(&config.font, config.font_size).expect("渲染器");
        let out = render_counter_frame(
            &mut frame,
            200,
            40,
            &doc,
            480,
            480,
            60,
            2.0,
            &config,
            &mut stats,
            &mut renderer,
        );
        // tick=480：0/480 两个音符已开始；test_config 未启用补零
        assert!(out.text.contains("Notes: 2 / 3"), "out={}", out.text);
        assert!(out.csv_line.is_none(), "未启用 CSV 时不应输出行");

        // 背景为黑色（取远离文本的右下角像素）
        let corner = ((39 * 200 + 199) * 4) as usize;
        assert_eq!(&frame[corner..corner + 4], &[0, 0, 0, 255]);
        // 文本像素存在（白色）
        assert!(frame.chunks_exact(4).any(|p| p == [255, 255, 255, 255]));
        // 统计推进
        assert_eq!(stats.frames, 1);
        assert_eq!(stats.note_count, 2);
    }

    /// CSV：启用时输出 csv_format 替换行
    #[test]
    fn test_render_counter_frame_csv() {
        let doc = make_doc();
        let mut config = test_config();
        config.save_csv = true;
        config.csv_output = std::path::PathBuf::from("out.csv");
        config.csv_format = "{nc},{nps}".to_string();

        let mut stats = CounterStats::default();
        stats.reset(&doc);
        let mut frame = vec![0u8; 200 * 40 * 4];
        let mut renderer =
            CounterFontRenderer::new(&config.font, config.font_size).expect("渲染器");
        let out = render_counter_frame(
            &mut frame,
            200,
            40,
            &doc,
            480,
            480,
            60,
            2.0,
            &config,
            &mut stats,
            &mut renderer,
        );
        let line = out.csv_line.expect("启用 CSV 应输出行");
        // 未补零：nc=2, nps=2
        assert_eq!(line, "2,2", "line={line}");
    }

    /// 对齐：TopRight 与 TopLeft 首行文本像素位置不同
    #[test]
    fn test_alignment_top_right_moves_text() {
        let doc = make_doc();
        let mut stats = CounterStats::default();
        stats.reset(&doc);

        // TopLeft
        let mut config = test_config();
        config.alignment = CounterAlignment::TopLeft;
        let mut frame_l = vec![0u8; 200 * 40 * 4];
        let mut renderer =
            CounterFontRenderer::new(&config.font, config.font_size).expect("渲染器");
        render_counter_frame(
            &mut frame_l,
            200,
            40,
            &doc,
            480,
            480,
            60,
            2.0,
            &config,
            &mut stats,
            &mut renderer,
        );

        // TopRight
        let mut frame_r = vec![0u8; 200 * 40 * 4];
        config.alignment = CounterAlignment::TopRight;
        render_counter_frame(
            &mut frame_r,
            200,
            40,
            &doc,
            480,
            480,
            60,
            2.0,
            &config,
            &mut stats,
            &mut renderer,
        );

        // 右上对齐：右侧区域应有像素，左上区域无
        assert!(
            frame_r[0..4] == [0, 0, 0, 255] || frame_r[0..4] == [0, 0, 0, 0],
            "右上对齐时左上角应为背景"
        );
        let right_has_pixel = (0..8).any(|y| {
            let idx = (y * 200 + 190) * 4;
            frame_r[idx..idx + 4] == [255, 255, 255, 255]
        });
        assert!(right_has_pixel, "右上对齐时右侧应有文本像素");
    }

    /// 空文本 / 空文档不 panic
    #[test]
    fn test_render_counter_frame_edge() {
        let doc = make_doc();
        let mut config = test_config();
        config.text = String::new();
        let mut stats = CounterStats::default();
        stats.reset(&doc);
        let mut frame = vec![0u8; 64 * 64 * 4];
        let mut renderer =
            CounterFontRenderer::new(&config.font, config.font_size).expect("渲染器");
        let out = render_counter_frame(
            &mut frame,
            64,
            64,
            &doc,
            0,
            480,
            60,
            2.0,
            &config,
            &mut stats,
            &mut renderer,
        );
        assert!(out.text.is_empty());
    }

    /// TTF 字体渲染中文模板端到端（系统微软雅黑；跳过条件：无系统字体）
    #[test]
    fn test_render_counter_frame_chinese_ttf() {
        let msyh = std::path::Path::new("C:\\Windows\\Fonts\\msyh.ttc");
        if !msyh.is_file() {
            eprintln!("跳过：系统缺少 msyh.ttc");
            return;
        }
        let doc = make_doc();
        let mut config = test_config();
        config.text = "音符: {nc} / {tn}".to_string();
        config.font = lumino_message::events::window::video::CounterFont::System {
            family: "微软雅黑".to_string(),
        };
        let mut stats = CounterStats::default();
        stats.reset(&doc);

        let mut frame = vec![0u8; 200 * 40 * 4];
        let mut renderer =
            CounterFontRenderer::new(&config.font, config.font_size).expect("渲染器");
        let out = render_counter_frame(
            &mut frame,
            200,
            40,
            &doc,
            480,
            480,
            60,
            2.0,
            &config,
            &mut stats,
            &mut renderer,
        );
        assert!(out.text.contains("音符: 2 / 3"), "out={}", out.text);
        // 中文模板渲染后应有非零前景像素
        assert!(
            frame.chunks_exact(4).any(|p| p[0] > 0),
            "TTF 渲染中文应有像素"
        );
    }

    /// TTF 字体文件不存在：渲染器构造失败（调用方回退）
    #[test]
    fn test_renderer_invalid_font_fails() {
        let missing = std::env::temp_dir().join("__lumino_no_such_font_9f3a.ttf");
        let res = CounterFontRenderer::new(
            &lumino_message::events::window::video::CounterFont::File {
                path: missing.to_string_lossy().to_string(),
            },
            40,
        );
        assert!(res.is_err(), "无效字体路径应报错");
    }
}
