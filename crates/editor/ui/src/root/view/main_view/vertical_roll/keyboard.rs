//! 纵向卷帘键盘 Canvas（编辑区底部，键沿 X 轴铺开）

use iced_core::mouse;
use iced_core::{Color, Event, Point, Rectangle, Size};
use iced_widget::canvas::{self, Action, Frame, Program, Stroke};
use lumino_gfx::grid::is_black_key;

use crate::{Message, Renderer, Theme};
use iced_wgpu::Geometry;
use lumino_core::view_state::ViewState;
use lumino_editor_state::CanvasState;
use lumino_ui_editor::grid::theme::ThemeExt;
use lumino_ui_editor::zoom::{fixed_ratio_from_viewport, zoom_factor_from_delta};

/// 纵向卷帘键盘
///
/// 键盘是「音高标尺」，与上方网格 X 轴（音高）严格对齐：网格中音高 `key` 的 X 坐标
/// 恰好落在键盘第 `key` 颗键上。音高轴缩放/平移复用横向卷帘同款语义 ——
/// `pitch_zoom`(= 横向 `zoom_y`，Pixels per Key) 控制键条宽窄，`pitch_scroll`(= 横向 `scroll_x`)
/// 控制横向滚动画面。播放时 `scroll_x` 驱动时间轴（Y）下落，键盘横向不随播放移动。
/// 配色复用横向键盘同款 `ThemeExt`（黑/白键、键盘底色、边框）。
pub struct VerticalKeyboardProgram {
    /// 总键数（音域宽度）
    pub key_count: u16,
    /// 音高轴缩放（Pixels per Key，复用横向 `zoom_y`）
    pub pitch_zoom: f32,
    /// 音高轴平移（复用横向 `scroll_x`），与网格 X 轴一致
    pub pitch_scroll: f32,
    /// 视图快照（供 update 读取缩放锚点所需的 canvas/view 尺寸）
    pub editor_view: ViewState,
    /// 画布状态快照（供 update 计算缩放视口宽度）
    pub editor_canvas: CanvasState,
    /// Ctrl 是否按下（host 通道注入，用于 Ctrl+滚轮缩放）
    pub ctrl_pressed: bool,
}

impl Program<Message, Theme, Renderer> for VerticalKeyboardProgram {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        _renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(_renderer, bounds.size());

        // 键盘底色
        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            theme.keyboard_background_color(),
        );

        let key_height = bounds.height;
        // 单键宽 = 画布宽 / 总键数 * pitch_zoom（pitch_zoom=1 时整排铺满）
        let key_width = bounds.width / self.key_count.max(1) as f32 * self.pitch_zoom;

        for i in 0..self.key_count {
            let keynum = i as isize;
            // 与网格 X 轴（音高）对齐：归一化位置 * 画布宽 - 横向滚动（低音在左、高音在右）
            let normalized = (keynum as f32) / (self.key_count.max(1) as f32);
            let screen_x = normalized * bounds.width - self.pitch_scroll;
            let is_black = is_black_key(keynum);
            let base_color = if is_black {
                theme.black_key_color()
            } else {
                theme.white_key_color()
            };
            // 256 键扩展区（128-255）颜色微调：高亮系压暗、暗色系提亮
            let key_color = if i >= 128 {
                let (r, g, b) = if theme.is_light() {
                    (
                        (base_color.r * 0.85).max(0.0),
                        (base_color.g * 0.85).max(0.0),
                        (base_color.b * 0.85).max(0.0),
                    )
                } else {
                    (
                        (base_color.r * 1.15).min(1.0),
                        (base_color.g * 1.15).min(1.0),
                        (base_color.b * 1.15).min(1.0),
                    )
                };
                Color::from_rgba(r, g, b, base_color.a)
            } else {
                base_color
            };

            let key_rect =
                Rectangle::new(Point::new(screen_x, 0.0), Size::new(key_width, key_height));
            let key_path =
                iced_widget::canvas::Path::rectangle(key_rect.position(), key_rect.size());
            frame.fill(&key_path, key_color);
            frame.stroke(
                &key_path,
                Stroke::default()
                    .with_width(1.0)
                    .with_color(theme.border_color()),
            );

            // 音符名称标签（键条较窄，仅白键标注音名以便定位）
            if !is_black {
                let label = canvas::Text {
                    content: note_label(i as u8),
                    position: Point::new(screen_x + key_width / 2.0, key_height / 2.0),
                    max_width: key_width,
                    line_height: iced_core::text::LineHeight::Relative(1.0),
                    size: iced_core::Pixels(9.0),
                    color: theme.text_color(),
                    font: iced_core::Font::DEFAULT,
                    align_x: iced_core::alignment::Horizontal::Center.into(),
                    align_y: iced_core::alignment::Vertical::Center,
                    shaping: iced_core::text::Shaping::Basic,
                };
                frame.fill_text(label);
            }
        }

        vec![frame.into_geometry()]
    }

    fn update(
        &self,
        _state: &mut (),
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        let Event::Mouse(mouse::Event::WheelScrolled { delta }) = event else {
            return None;
        };
        // Ctrl+滚轮：音高轴缩放（对应横向键盘区 Ctrl+滚轮→ZoomYChanged）
        if !self.ctrl_pressed {
            return None;
        }
        let factor = zoom_factor_from_delta(delta)?;
        let view = &self.editor_view;
        let canvas = &self.editor_canvas;
        let viewport_w = (canvas.size_x - view.keyboard_width).max(0.0);
        let local_pos = cursor
            .position()
            .map(|p| Point::new(p.x - bounds.x, p.y - bounds.y))?;
        Some(Action::publish(Message::ZoomYChanged {
            zoom: view.zoom_y * factor,
            fixed_ratio: fixed_ratio_from_viewport(local_pos.x, view.keyboard_width, viewport_w),
        }))
    }

    fn mouse_interaction(
        &self,
        _state: &(),
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::None
    }
}

/// 简化音符标签（取音名 + 八度，如 C4）；仅用于纵向键盘定位提示。
fn note_label(key: u8) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let name = NAMES[(key % 12) as usize];
    let octave = (key / 12).saturating_sub(1);
    format!("{name}{octave}")
}
