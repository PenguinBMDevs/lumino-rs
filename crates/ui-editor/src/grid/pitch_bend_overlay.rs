//! 弯音编辑模式遮罩层绘制
//!
//! WGPU 负责钢琴卷帘区域（音符区域）的半透明遮罩，
//! 本模块用 iced canvas 覆盖键盘区域、标尺区域和左上角空白区域，
//! 将两套遮罩拼接成完整覆盖。

use crate::{Editor, Renderer};
use iced_core::{Color, Point, Rectangle, Size};
use iced_widget::canvas::{Geometry, Path, Stroke};

impl Editor {
    /// 绘制弯音遮罩（键盘 + 标尺 + 左上角空白区域）
    ///
    /// 返回 None 表示非弯音模式或无 canvas。
    pub fn draw_pitch_bend_overlay(
        &self,
        renderer: &Renderer,
        bounds: Rectangle,
    ) -> Option<Geometry<Renderer>> {
        if !self.editor_state.is_pitch_bend_mode() {
            return None;
        }

        let v = &self.editor_state.view;
        let kw = v.keyboard_width;
        let rh = v.ruler_height;

        // 遮罩颜色：半透明灰
        let overlay_color = Color::from_rgba(0.5, 0.5, 0.5, 0.3);

        let geom = iced_widget::canvas::Cache::default().draw(renderer, bounds.size(), |frame| {
            // 1. 左上角空白区域 (0,0) -> (keyboard_width, ruler_height)
            let corner_rect = Path::rectangle(Point::ORIGIN, Size::new(kw, rh));
            frame.fill(&corner_rect, overlay_color);

            // 2. 键盘区域 (0, ruler_height) -> (keyboard_width, bounds.height)
            let keyboard_rect =
                Path::rectangle(Point::new(0.0, rh), Size::new(kw, bounds.height - rh));
            frame.fill(&keyboard_rect, overlay_color);

            // 3. 标尺区域 (keyboard_width, 0) -> (bounds.width, ruler_height)
            let ruler_rect = Path::rectangle(Point::new(kw, 0.0), Size::new(bounds.width - kw, rh));
            frame.fill(&ruler_rect, overlay_color);

            // 4. 选中音符基准线（Y 轴中心线，标识弯音中心位置）
            if let Some(curve) = self.editor_state.pitch_bend_curve.as_ref() {
                let base_y = self.key_to_y(curve.base_key);
                if base_y >= rh && base_y <= bounds.height {
                    let line_path =
                        Path::line(Point::new(kw, base_y), Point::new(bounds.width, base_y));
                    frame.stroke(
                        &line_path,
                        Stroke::default()
                            .with_color(Color::from_rgba(1.0, 0.8, 0.2, 0.6))
                            .with_width(1.0),
                    );
                }
            }
        });

        Some(geom)
    }
}
