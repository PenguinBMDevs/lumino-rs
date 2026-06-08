//! 力度/Tempo/CC 编辑面板 - 类 Cubase 的 Controller Lane
//!
//! 支持三种编辑模式：
//! - Velocity（力度）：显示当前音轨所有音符的力度值
//! - Tempo（速度）：Conductor 音轨上显示和编辑全局速度曲线
//! - CC（控制器）：显示和编辑指定 CC 控制器的控制点
//!
//! X 轴与钢琴卷帘对齐联动。力度/CC 的 Y 轴 0-127，Tempo 的 Y 轴为可视 BPM 范围。

pub mod widget;

pub use widget::TempoPoint;

use crate::Element;

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

/// 编辑模式
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    /// 力度编辑
    #[default]
    Velocity,
    /// 速度编辑（Conductor 音轨专用）
    Tempo,
    /// 弯音编辑（-8192 到 +8191）
    Bend,
    /// CC 控制器编辑
    Cc(u8),
}

impl EditMode {
    /// 所有可用的 EditMode 变体（用于切换）
    pub fn all_modes() -> Vec<Self> {
        vec![Self::Velocity]
    }

    /// 获取显示名称
    pub fn display_name(&self) -> &'static str {
        match self {
            EditMode::Velocity => "力度",
            EditMode::Tempo => "速度",
            EditMode::Bend => "Bend",
            EditMode::Cc(_) => "CC",
        }
    }

    /// 是否处于 CC 模式（包括 Bend）
    pub fn is_cc(&self) -> bool {
        matches!(self, EditMode::Cc(_) | EditMode::Bend)
    }

    /// 是否处于 Tempo 模式
    pub fn is_tempo(&self) -> bool {
        matches!(self, EditMode::Tempo)
    }
}

/// CC 控制点
#[derive(Debug, Clone, Copy)]
pub struct CcPoint {
    /// tick 位置
    pub tick: f32,
    /// 控制器值 (0-127)
    pub value: u8,
}

/// 弯音控制点
#[derive(Debug, Clone, Copy)]
pub struct BendPoint {
    /// tick 位置
    pub tick: f32,
    /// 弯音值 (-8192 到 +8191)
    pub value: i16,
}

/// 音轨 CC 数据
#[derive(Debug, Clone, Default)]
pub struct CcData {
    /// 控制器编号 → 控制点列表
    pub controllers: std::collections::HashMap<u8, Vec<CcPoint>>,
    /// 弯音点列表
    pub bend_points: Vec<BendPoint>,
}

/// 已知 CC 控制器名称（GM/GS/XG 标准）
pub const CC_CONTROLLER_NAMES: &[(u8, &str)] = &[
    (0, "Bank Select MSB"),
    (1, "Modulation Wheel"),
    (2, "Breath Controller"),
    (4, "Foot Controller"),
    (5, "Portamento Time"),
    (6, "Data Entry MSB"),
    (7, "Channel Volume"),
    (8, "Balance"),
    (10, "Pan"),
    (11, "Expression"),
    (12, "Effect Control 1"),
    (13, "Effect Control 2"),
    (16, "General Purpose 1"),
    (17, "General Purpose 2"),
    (18, "General Purpose 3"),
    (19, "General Purpose 4"),
    (32, "Bank Select LSB"),
    (33, "Modulation Wheel LSB"),
    (34, "Breath Controller LSB"),
    (36, "Foot Controller LSB"),
    (37, "Portamento Time LSB"),
    (38, "Data Entry LSB"),
    (39, "Channel Volume LSB"),
    (40, "Balance LSB"),
    (42, "Pan LSB"),
    (43, "Expression LSB"),
    (64, "Sustain Pedal"),
    (65, "Portamento On/Off"),
    (66, "Sostenuto Pedal"),
    (67, "Soft Pedal"),
    (68, "Legato Footswitch"),
    (69, "Hold 2"),
    (70, "Sound Variation"),
    (71, "Resonance"),
    (72, "Release Time"),
    (73, "Attack Time"),
    (74, "Brightness / Cutoff"),
    (75, "Sound Controller 6"),
    (76, "Sound Controller 7"),
    (77, "Sound Controller 8"),
    (78, "Sound Controller 9"),
    (79, "Sound Controller 10"),
    (80, "General Purpose 5"),
    (81, "General Purpose 6"),
    (82, "General Purpose 7"),
    (83, "General Purpose 8"),
    (84, "Portamento Control"),
    (91, "Reverb Depth"),
    (92, "Tremolo Depth"),
    (93, "Chorus Depth"),
    (94, "Celeste Depth"),
    (95, "Phaser Depth"),
    (96, "Data Increment"),
    (97, "Data Decrement"),
    (98, "NRPN LSB"),
    (99, "NRPN MSB"),
    (100, "RPN LSB"),
    (101, "RPN MSB"),
    (120, "All Sound Off"),
    (121, "Reset All Controllers"),
    (122, "Local Control On/Off"),
    (123, "All Notes Off"),
    (124, "Omni Off"),
    (125, "Omni On"),
    (126, "Mono On"),
    (127, "Poly On"),
];

/// CC 编号显示包装（下拉框显示 "编号: 名称"）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CcDisplay(pub u8);

/// 弯音显示包装（下拉框显示 "Bend: Pitch Bend (-8192..8191)"）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BendDisplay;

impl std::fmt::Display for BendDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bend: Pitch Bend (-8192..8191)")
    }
}

impl std::fmt::Display for CcDisplay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match CC_CONTROLLER_NAMES.iter().find(|(n, _)| *n == self.0) {
            Some((_, name)) => write!(f, "{}: {}", self.0, name),
            None => write!(f, "{}", self.0),
        }
    }
}

/// CC 或 Bend 下拉选项
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CcOption {
    /// 弯音
    Bend,
    /// CC 控制器
    Cc(u8),
}

impl std::fmt::Display for CcOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CcOption::Bend => write!(f, "Bend: Pitch Bend (-8192..8191)"),
            CcOption::Cc(n) => match CC_CONTROLLER_NAMES.iter().find(|(num, _)| *num == *n) {
                Some((_, name)) => write!(f, "{}: {}", n, name),
                None => write!(f, "{}", n),
            },
        }
    }
}

/// 力度点数据
#[derive(Debug, Clone, Copy)]
pub struct VelocityPoint {
    /// 在 notes 向量中的索引
    pub note_index: usize,
    /// 音符的起始 tick（用于排序）
    pub tick: f32,
    /// 力度值 0-127
    pub velocity: u8,
}

/// 力度/CC 编辑面板组件
pub struct VelocityPanel {
    /// 当前编辑模式
    pub edit_mode: EditMode,
    /// CC 模式下选择的控制器编号
    pub selected_cc: u8,
}

impl VelocityPanel {
    pub fn new() -> Self {
        Self {
            edit_mode: EditMode::Velocity,
            selected_cc: 1, // 默认调制轮
        }
    }

    /// 渲染编辑面板视图
    pub fn view<'a>(&'a self, editor: &'a crate::editor::Editor, panel_height: f32) -> Element<'a> {
        use iced_core::Alignment;
        use iced_widget::canvas::Canvas;
        use iced_widget::{button, column, container, pick_list, row, space, text};

        // 顶部工具栏：模式切换 + CC 选择器
        let toolbar_height = 28.0f32;
        let canvas_height = (panel_height - toolbar_height).max(10.0);

        let is_tempo = self.edit_mode == EditMode::Tempo;
        let is_velocity = self.edit_mode == EditMode::Velocity;

        // 模式切换按钮
        let mode_label = if is_tempo {
            "速度"
        } else if is_velocity {
            "力度"
        } else if self.edit_mode == EditMode::Bend {
            "Bend"
        } else {
            "CC"
        };
        let mode_btn = button(text(mode_label).size(12))
            .on_press(crate::message::Message::Velocity(
                crate::message::VelocityAction::ToggleMode,
            ))
            .padding([2, 8])
            .style(move |theme: &crate::Theme, status| {
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
            });

        // CC 控制器选择器（仅在 CC 模式显示，Tempo 模式也隐藏）
        let cc_selector: Element<'a> = if self.edit_mode.is_cc() {
            // 生成选项：Bend 在顶部，然后是 0-127 CC
            let mut cc_options: Vec<CcOption> = Vec::new();
            cc_options.push(CcOption::Bend);
            for n in 0..=127 {
                cc_options.push(CcOption::Cc(n));
            }
            let selected = if self.edit_mode == EditMode::Bend {
                CcOption::Bend
            } else {
                CcOption::Cc(self.selected_cc)
            };
            pick_list(cc_options, Some(selected), move |cc| {
                crate::message::Message::Velocity(crate::message::VelocityAction::CcOptionSelected(
                    cc,
                ))
            })
            .placeholder("Select CC/Bend")
            .padding([2, 6])
            .width(iced_core::Length::Fixed(170.0))
            .into()
        } else {
            space().width(0).into()
        };

        // 模式信息文字
        let info_text = if is_tempo {
            "速度 BPM".to_string()
        } else if is_velocity {
            "力度 0-127".to_string()
        } else if self.edit_mode == EditMode::Bend {
            "Bend: -8192..8191".to_string()
        } else {
            format!("{}", CcDisplay(self.selected_cc))
        };

        let toolbar = container(
            row![
                mode_btn,
                space().width(8),
                cc_selector,
                space().width(iced_core::Length::Fill),
                text(info_text)
                    .size(11)
                    .color(iced_core::Color::from_rgba(0.5, 0.5, 0.5, 0.7)),
            ]
            .align_y(Alignment::Center),
        )
        .height(toolbar_height)
        .width(iced_core::Length::Fill)
        .padding([2, 8])
        .style(|theme: &crate::Theme| {
            iced_widget::container::Style::default()
                .background(theme.extended_palette().background.weak.color)
        });

        let canvas = Canvas::new(widget::VelocityCanvas {
            editor,
            edit_mode: self.edit_mode,
            selected_cc: self.selected_cc,
        })
        .width(iced_core::Length::Fill)
        .height(canvas_height);

        let panel_content = column![toolbar, canvas]
            .width(iced_core::Length::Fill)
            .height(panel_height);

        iced_widget::container(panel_content)
            .width(iced_core::Length::Fill)
            .height(panel_height)
            .style(|theme: &crate::Theme| {
                iced_widget::container::Style::default()
                    .background(theme.extended_palette().background.weak.color)
            })
            .into()
    }

    /// 构建速度点数据（从 EditorData 的 tempo_points 读取，支持用户编辑后的实时反馈）
    pub fn build_tempo_points(editor: &crate::editor::Editor) -> Vec<TempoPoint> {
        editor.editor_state.data.tempo_points.clone()
    }

    /// 构建力度点数据
    pub fn build_velocity_points(notes: &im::Vector<crate::editor::Note>) -> Vec<VelocityPoint> {
        let mut points: Vec<VelocityPoint> = notes
            .iter()
            .enumerate()
            .map(|(i, note)| VelocityPoint {
                note_index: i,
                tick: note.tick,
                velocity: note.velocity,
            })
            .collect();

        points.sort_by(|a, b| {
            a.tick
                .partial_cmp(&b.tick)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.note_index.cmp(&b.note_index))
        });

        points
    }

    /// 构建 CC 数据（从编辑器中的 CC 数据获取）
    pub fn build_cc_points(editor: &crate::editor::Editor, cc_number: u8) -> Vec<CcPoint> {
        editor
            .editor_state
            .data
            .cc_data
            .controllers
            .get(&cc_number)
            .cloned()
            .unwrap_or_default()
    }

    /// 构建弯音数据（从编辑器中的 bend_points 获取）
    pub fn build_bend_points(editor: &crate::editor::Editor) -> Vec<BendPoint> {
        editor.editor_state.data.cc_data.bend_points.clone()
    }
}

impl Default for VelocityPanel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Note;

    // ===== Velocity 测试 =====

    #[test]
    fn test_build_velocity_points_empty() {
        let notes = im::Vector::new();
        let points = VelocityPanel::build_velocity_points(&notes);
        assert!(points.is_empty());
    }

    #[test]
    fn test_build_velocity_points_single_note() {
        let mut notes = im::Vector::new();
        notes.push_back(Note::new(0.0, 60, 480.0).with_velocity(100));
        let points = VelocityPanel::build_velocity_points(&notes);

        assert_eq!(points.len(), 1);
        assert_eq!(points[0].note_index, 0);
        assert_eq!(points[0].tick, 0.0);
        assert_eq!(points[0].velocity, 100);
    }

    #[test]
    fn test_build_velocity_points_multiple_notes() {
        let mut notes = im::Vector::new();
        notes.push_back(Note::new(480.0, 64, 240.0).with_velocity(80));
        notes.push_back(Note::new(0.0, 60, 480.0).with_velocity(100));
        notes.push_back(Note::new(960.0, 67, 240.0).with_velocity(120));
        notes.push_back(Note::new(480.0, 72, 120.0).with_velocity(60));

        let points = VelocityPanel::build_velocity_points(&notes);
        assert_eq!(points.len(), 4);
        assert_eq!(points[0].tick, 0.0);
        assert_eq!(points[0].note_index, 1);
        assert_eq!(points[1].tick, 480.0);
        assert_eq!(points[1].note_index, 0);
        assert_eq!(points[2].tick, 480.0);
        assert_eq!(points[2].note_index, 3);
        assert_eq!(points[3].tick, 960.0);
        assert_eq!(points[3].note_index, 2);
    }

    // ===== CC 数据测试 =====

    #[test]
    fn test_build_cc_points_empty() {
        use crate::editor::Editor;
        let editor = Editor::new();
        let points = VelocityPanel::build_cc_points(&editor, 1);
        assert!(points.is_empty());
    }

    #[test]
    fn test_build_cc_points_with_data() {
        use crate::editor::Editor;
        let mut editor = Editor::new();
        // 添加 CC 数据
        editor.editor_state.data.cc_data.controllers.insert(
            1,
            vec![
                CcPoint {
                    tick: 0.0,
                    value: 64,
                },
                CcPoint {
                    tick: 480.0,
                    value: 127,
                },
            ],
        );

        let points = VelocityPanel::build_cc_points(&editor, 1);
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].tick, 0.0);
        assert_eq!(points[0].value, 64);
        assert_eq!(points[1].tick, 480.0);
        assert_eq!(points[1].value, 127);
    }

    #[test]
    fn test_build_cc_points_wrong_number() {
        use crate::editor::Editor;
        let mut editor = Editor::new();
        editor.editor_state.data.cc_data.controllers.insert(
            1,
            vec![CcPoint {
                tick: 0.0,
                value: 64,
            }],
        );

        let points = VelocityPanel::build_cc_points(&editor, 7);
        assert!(points.is_empty(), "不同 CC 号应返回空");
    }

    // ===== EditMode 测试 =====

    #[test]
    fn test_edit_mode_default_is_velocity() {
        let mode = EditMode::default();
        assert_eq!(mode, EditMode::Velocity);
    }

    #[test]
    fn test_edit_mode_is_cc() {
        assert!(!EditMode::Velocity.is_cc());
        assert!(EditMode::Cc(1).is_cc());
        assert!(!EditMode::Tempo.is_cc());
    }

    #[test]
    fn test_edit_mode_is_tempo() {
        assert!(!EditMode::Velocity.is_tempo());
        assert!(!EditMode::Cc(1).is_tempo());
        assert!(EditMode::Tempo.is_tempo());
    }

    #[test]
    fn test_edit_mode_display_name() {
        assert_eq!(EditMode::Velocity.display_name(), "力度");
        assert_eq!(EditMode::Tempo.display_name(), "速度");
        assert_eq!(EditMode::Cc(1).display_name(), "CC");
    }

    // ===== Tempo 数据测试 =====

    #[test]
    fn test_build_tempo_points_no_document() {
        use crate::editor::Editor;
        let editor = Editor::new();
        let points = VelocityPanel::build_tempo_points(&editor);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].tick, 0.0);
        assert!((points[0].bpm - 120.0).abs() < 0.01);
    }

    #[test]
    fn test_build_tempo_points_from_editor_data() {
        use crate::editor::Editor;
        let mut editor = Editor::new();
        // 直接向 tempo_points 写入数据模拟已加载文档
        editor.editor_state.data.tempo_points = vec![
            TempoPoint {
                tick: 0.0,
                bpm: 120.0,
            },
            TempoPoint {
                tick: 480.0,
                bpm: 140.0,
            },
        ];

        let points = VelocityPanel::build_tempo_points(&editor);
        // 现在 build_tempo_points 从 data.tempo_points 读取，返回编辑后的数据
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].tick, 0.0);
        assert!((points[0].bpm - 120.0).abs() < 0.01);
        assert_eq!(points[1].tick, 480.0);
        assert!((points[1].bpm - 140.0).abs() < 0.01);
    }
}
