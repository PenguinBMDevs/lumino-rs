//! 计数器帧对齐布局（Zenith `Alignments` 六种语义）
//!
//! 从 `counter_frame.rs` 拆出，控制文件行数。行高统一由渲染器提供，
//! 使点阵（整数倍缩放）与 TTF（真实像素行高）两种字体后端行为一致。

use lumino_message::events::window::video::CounterAlignment as A;

use super::counter_font::CounterFontRenderer;
use super::counter_stats::CounterRenderConfig;

/// 按对齐方式绘制多行文本。
pub(super) fn draw_aligned_text(
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    text: &str,
    config: &CounterRenderConfig,
    renderer: &mut CounterFontRenderer,
    color: [u8; 4],
) {
    let lines: Vec<&str> = text.split('\n').collect();
    if lines.is_empty() {
        return;
    }
    let line_h = renderer.line_height();
    let n = lines.len() as u32;
    let w = frame_width;
    let h = frame_height;

    match config.alignment {
        // 左上：每行从 (0, i*line_h) 开始
        A::TopLeft => {
            for (i, line) in lines.iter().enumerate() {
                renderer.draw_line(frame, frame_width, line, 0, i as u32 * line_h, color);
            }
        }
        // 右上：每行右对齐
        A::TopRight => {
            for (i, line) in lines.iter().enumerate() {
                let lw = renderer.measure_line(line);
                let x = w.saturating_sub(lw);
                renderer.draw_line(frame, frame_width, line, x, i as u32 * line_h, color);
            }
        }
        // 左下：从底部向上
        A::BottomLeft => {
            for (i, line) in lines.iter().enumerate() {
                let y = h.saturating_sub((n - i as u32) * line_h);
                renderer.draw_line(frame, frame_width, line, 0, y, color);
            }
        }
        // 右下
        A::BottomRight => {
            for (i, line) in lines.iter().enumerate() {
                let lw = renderer.measure_line(line);
                let x = w.saturating_sub(lw);
                let y = h.saturating_sub((n - i as u32) * line_h);
                renderer.draw_line(frame, frame_width, line, x, y, color);
            }
        }
        // 顶部分散（Zenith 语义）：行从原始末行开始（p=1..n），
        // 水平中心分散于 dist*p*W 处，全部顶部对齐
        A::TopSpread => {
            let dist = 1.0f64 / (n as f64 + 1.0);
            for (i, line) in lines.iter().enumerate() {
                let p = (n - i as u32) as f64; // 原始末行 p=1
                let lw = renderer.measure_line(line) as f64;
                let x = (dist * p * w as f64 - lw / 2.0).max(0.0) as u32;
                renderer.draw_line(frame, frame_width, line, x, 0, color);
            }
        }
        // 底部分散（Zenith 语义）：行水平分散，全部底部对齐
        A::BottomSpread => {
            let dist = 1.0f64 / (n as f64 + 1.0);
            for (i, line) in lines.iter().enumerate() {
                let p = (n - i as u32) as f64;
                let lw = renderer.measure_line(line) as f64;
                let x = (dist * p * w as f64 - lw / 2.0).max(0.0) as u32;
                let y = h.saturating_sub(line_h);
                renderer.draw_line(frame, frame_width, line, x, y, color);
            }
        }
    }
}
