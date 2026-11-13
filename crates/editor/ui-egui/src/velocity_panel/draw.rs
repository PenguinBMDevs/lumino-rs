//! 力度面板绘制辅助：Tempo 折线与文本宽度测量。

use egui::{Align2, Color32, FontId, Pos2, Stroke};

use super::{VelocityPanelModel, input};

/// Tempo 折线图（gfx 只画力度/CC 曲线，Tempo 图由 egui 补）。
pub(super) fn tempo_graph(painter: &egui::Painter, model: &VelocityPanelModel) {
    let color = Color32::from_rgb(51, 140, 255);
    let mut points: Vec<(f32, f64)> = model.tempo_points.iter().map(|p| (p.tick, p.bpm)).collect();
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    let screen = |tick: f32, bpm: f64| {
        let x = model.canvas.left() + tick * model.view.zoom_x - model.view.scroll_x
            + model.view.keyboard_width;
        let y =
            model.canvas.top() + input::tempo_to_y(bpm, model.tempo_max_bpm, model.canvas.height());
        Pos2::new(x, y)
    };
    let visible: Vec<(Pos2, f64)> = points
        .iter()
        .filter(|(tick, _)| {
            let x = *tick * model.view.zoom_x - model.view.scroll_x + model.view.keyboard_width;
            x >= -50.0 && x <= model.canvas.width() + 50.0
        })
        .map(|(tick, bpm)| (screen(*tick, *bpm), *bpm))
        .collect();
    if visible.is_empty() {
        return;
    }
    let clip = painter.with_clip_rect(model.canvas);
    let mut line: Vec<Pos2> = visible.iter().map(|(p, _)| *p).collect();
    // 最后一个点向右无限延伸（BPM 保持恒定）。
    let last_tick = points.last().map(|p| p.0).unwrap_or(0.0);
    let last_bpm = points.last().map(|p| p.1).unwrap_or(120.0);
    let last_x = last_tick * model.view.zoom_x - model.view.scroll_x + model.view.keyboard_width;
    if last_x < model.canvas.width() + 50.0 {
        line.push(Pos2::new(
            model.canvas.right() + 50.0,
            screen(last_tick, last_bpm).y,
        ));
    }
    if line.len() >= 2 {
        clip.add(egui::Shape::line(
            line,
            Stroke::new(model.tempo_line_width, color),
        ));
    }
    for (pos, bpm) in &visible {
        clip.circle_filled(*pos, 4.0, color);
        clip.text(
            Pos2::new(pos.x, pos.y - 14.0),
            Align2::CENTER_TOP,
            format!("{bpm:.0}"),
            FontId::proportional(9.0),
            color.gamma_multiply(0.7),
        );
    }
}

/// 文本宽度（模式按钮自适应）。
pub(super) fn text_width(ui: &egui::Ui, text: &str, size: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(size), Color32::WHITE)
        .size()
        .x
}
