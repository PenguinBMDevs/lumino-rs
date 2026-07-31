//! 力度/Tempo 编辑面板 - 类 Cubase 的 Controller Lane
//!
//! X 轴与钢琴卷帘对齐联动。力度 Y 轴 0-127，Tempo Y 轴为可视 BPM 范围。
//! CC/Bend 控制器编辑已移除（数据层保留）。

pub mod widget;

pub use widget::TempoPoint;
pub use widget::VelocityCanvasState;

// 重新导出自 lumino-core 的数据类型
pub use lumino_core::{
    BendPoint, CC_CONTROLLER_NAMES, CcPoint, EditMode, EditorTransform, VelocityPoint,
};

use lumino_core::NoteStore;
use lumino_core::i18n::Language;
use lumino_ui_core::Element;

/// 面板高度（像素）
pub const VELOCITY_PANEL_HEIGHT: f32 = 150.0;
/// 面板最小高度
pub const VELOCITY_PANEL_MIN_HEIGHT: f32 = 60.0;
/// 面板最大高度
pub const VELOCITY_PANEL_MAX_HEIGHT: f32 = 400.0;
/// 点绘制半径
pub const POINT_RADIUS: f32 = 4.0;
/// 悬停高亮半径
pub const HOVER_RADIUS: f32 = 7.0;
/// 点击/拖拽检测半径
pub const HIT_RADIUS: f32 = 10.0;
/// 面板上下内边距
pub const PANEL_PADDING_Y: f32 = 12.0;
/// 面板左右内边距
pub const PANEL_PADDING_X: f32 = 8.0;
/// 顶部 resize 拖拽手柄高度
pub const RESIZE_HANDLE_HEIGHT: f32 = 5.0;
/// 工具栏高度
pub const TOOLBAR_HEIGHT: f32 = 28.0;

/// 力度/Tempo 编辑面板组件
pub struct VelocityPanel {
    /// 当前编辑模式
    pub edit_mode: EditMode,
    /// 自动化曲线垂直缩放。1.0 = 满量程映射到面板数据区高度。
    pub value_zoom: f32,
    /// 自动化曲线垂直滚动偏移（值空间单位）。
    pub value_scroll: f32,
    /// 自动化曲线连线粗细（像素，1-10，默认 2）。
    pub automation_line_thickness: f32,
}

impl VelocityPanel {
    pub fn new() -> Self {
        Self {
            edit_mode: EditMode::Velocity,
            value_zoom: 1.0,
            value_scroll: 0.0,
            automation_line_thickness: 2.0,
        }
    }

    /// 根据当前目标的最大值限制 value_scroll，避免滚出有效范围。
    pub fn clamp_value_scroll(&mut self, max_value: f32) {
        let visible_range = max_value / self.value_zoom.max(0.001);
        let max_scroll = (max_value - visible_range).max(0.0);
        self.value_scroll = self.value_scroll.clamp(0.0, max_scroll);
    }

    /// 渲染编辑面板视图
    pub fn view<'a>(
        &'a self,
        editor: &'a crate::Editor,
        panel_height: f32,
        language: Language,
    ) -> Element<'a> {
        use iced_core::Alignment;
        use iced_widget::canvas::Canvas;
        use iced_widget::{column, container, row, space, text};

        let t = lumino_core::i18n::main_translations(language);
        let toolbar_height = TOOLBAR_HEIGHT;
        let canvas_height = (panel_height - toolbar_height).max(10.0);

        let is_tempo = self.edit_mode == EditMode::Tempo;
        let is_velocity = self.edit_mode == EditMode::Velocity;

        let toolbar = container(
            row![
                self.build_mode_button(is_tempo, is_velocity, language, t),
                space().width(8),
                space().width(iced_core::Length::Fill),
                text(self.build_info_text(language))
                    .size(11)
                    .color(iced_core::Color::from_rgba(0.5, 0.5, 0.5, 0.7)),
            ]
            .align_y(Alignment::Center),
        )
        .height(toolbar_height)
        .width(iced_core::Length::Fill)
        .padding([2, 8])
        .style(|theme: &lumino_ui_core::Theme| {
            iced_widget::container::Style::default()
                .background(theme.extended_palette().background.weak.color)
        });

        let canvas = Canvas::new(widget::VelocityCanvas {
            editor,
            edit_mode: self.edit_mode,
        })
        .width(iced_core::Length::Fill)
        .height(canvas_height);

        let panel_content = column![toolbar, canvas]
            .width(iced_core::Length::Fill)
            .height(panel_height);

        iced_widget::container(panel_content)
            .width(iced_core::Length::Fill)
            .height(panel_height)
            .style(|_theme: &lumino_ui_core::Theme| {
                iced_widget::container::Style::default().background(iced_core::Color::TRANSPARENT)
            })
            .into()
    }

    /// 构建模式切换按钮（力度/速度/CC/Bend）
    fn build_mode_button<'a>(
        &'a self,
        is_tempo: bool,
        is_velocity: bool,
        _language: Language,
        t: &'static lumino_core::i18n::MainTranslations,
    ) -> Element<'a> {
        use iced_widget::{button, text};

        let mode_label = if is_tempo {
            t.velocity_panel_tempo
        } else {
            t.velocity_panel_velocity
        };

        button(text(mode_label).size(12))
            .on_press(lumino_ui_core::message::Message::Velocity(
                lumino_ui_core::message::VelocityAction::ToggleMode,
            ))
            .padding([2, 8])
            .style(move |theme: &lumino_ui_core::Theme, status| {
                let palette = theme.extended_palette();
                let bg = if is_tempo || is_velocity {
                    palette.primary.base.color
                } else if status == iced_widget::button::Status::Hovered {
                    palette.background.weak.color
                } else {
                    palette.background.weakest.color
                };
                iced_widget::button::Style {
                    border: iced_core::Border {
                        radius: 3.0.into(),
                        width: 0.0,
                        color: iced_core::Color::TRANSPARENT,
                    },
                    ..Default::default()
                }
                .with_background(bg)
            })
            .into()
    }

    /// 构建模式信息文字
    fn build_info_text(&self, language: Language) -> String {
        let t = lumino_core::i18n::main_translations(language);
        if self.edit_mode == EditMode::Tempo {
            t.velocity_panel_tempo_info.to_string()
        } else {
            t.velocity_panel_velocity_info.to_string()
        }
    }

    /// 构建速度点数据
    pub fn build_tempo_points(editor: &crate::Editor) -> Vec<TempoPoint> {
        editor
            .editor_state
            .data
            .tempo_points
            .iter()
            .map(|tp| TempoPoint {
                tick: tp.tick,
                bpm: tp.bpm,
            })
            .collect()
    }

    /// 构建力度点数据
    ///
    /// 直接遍历 notes 构建 VelocityPoint，避免创建 EditorData 及 notes.clone()。
    /// 10M 音符场景下，clone 开销可达百毫秒级。
    pub fn build_velocity_points(notes: &im::Vector<crate::Note>) -> Vec<VelocityPoint> {
        let mut points: Vec<VelocityPoint> = Vec::with_capacity(notes.len());
        for (i, n) in notes.iter().enumerate() {
            points.push(VelocityPoint {
                note_index: i,
                tick: n.tick,
                velocity: n.velocity,
                length: n.length,
            });
        }
        points.sort_by(|a, b| {
            a.tick
                .partial_cmp(&b.tick)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.note_index.cmp(&b.note_index))
        });
        points
    }

    /// 构建力度点数据（NoteStore 热路径，零 Note clone）
    ///
    /// **性能优化**：10M+ 音符场景下，比 `build_velocity_points(&im::Vector)` 节省
    /// 全部 Note 结构体 clone 开销（百毫秒级）。直接消费 SoA 数组，回调式遍历。
    ///
    /// 调用方需保证 `store` 与 `notes` 一致（NoteStore 启用时）。
    pub fn build_velocity_points_from_store(store: &NoteStore) -> Vec<VelocityPoint> {
        let mut points: Vec<VelocityPoint> = Vec::with_capacity(store.len());
        store.for_each_ref(|index, view| {
            points.push(VelocityPoint {
                note_index: index,
                tick: view.tick,
                velocity: view.velocity,
                length: view.length,
            });
        });
        points.sort_by(|a, b| {
            a.tick
                .partial_cmp(&b.tick)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.note_index.cmp(&b.note_index))
        });
        points
    }

    /// 构建 CC 数据（从 automation_lanes 读取当前音轨的 CC 事件）。
    pub fn build_cc_points(editor: &crate::Editor, cc_number: u8) -> Vec<CcPoint> {
        editor.editor_state.data.build_cc_points(cc_number)
    }

    /// 构建弯音数据（从 automation_lanes 读取当前音轨的 PitchBend 事件）。
    pub fn build_bend_points(editor: &crate::Editor) -> Vec<BendPoint> {
        editor.editor_state.data.build_bend_points()
    }
}

impl Default for VelocityPanel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
