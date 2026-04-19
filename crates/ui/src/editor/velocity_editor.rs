//! 力度编辑器 - Cubase风格条状力度条+瞄点控制

use iced_core::{Color, Point, Rectangle, Size};

use crate::editor::note::Note;
use crate::toolbar::Tool;

/// 力度编辑器状态
#[derive(Debug, Clone)]
pub struct VelocityEditor {
    /// 是否可见
    pub visible: bool,
    /// 面板高度
    pub height: f32,
    /// 当前工具
    pub current_tool: VelocityTool,
    /// 编辑状态
    pub edit_state: VelocityEditState,
    /// 瞄点半径
    pub handle_radius: f32,
    /// 力度条宽度
    pub bar_width: f32,
    /// 力度条间距
    pub bar_spacing: f32,
    /// 边距
    pub margin: Margin,
    /// 是否按住Shift
    pub shift_pressed: bool,
    /// 画笔绘制起点
    pub brush_start: Option<(f32, u8)>, // (x, velocity)
}

/// 边距
#[derive(Debug, Clone, Copy)]
pub struct Margin {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

impl Default for Margin {
    fn default() -> Self {
        Self {
            top: 40.0,
            bottom: 30.0,
            left: 50.0,
            right: 20.0,
        }
    }
}

/// 力度编辑工具
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VelocityTool {
    /// 光标工具 - 单点调整
    #[default]
    Pointer,
    /// 画笔工具 - 自由绘制
    Brush,
    /// 直线工具
    Line,
    /// 曲线工具
    Curve,
    /// 随机工具
    Random,
}

impl VelocityTool {
    /// 获取工具名称
    pub fn name(&self) -> &'static str {
        match self {
            VelocityTool::Pointer => "光标",
            VelocityTool::Brush => "画笔",
            VelocityTool::Line => "直线",
            VelocityTool::Curve => "曲线",
            VelocityTool::Random => "随机",
        }
    }

    /// 获取所有工具
    pub fn all() -> &'static [VelocityTool] {
        &[
            VelocityTool::Pointer,
            VelocityTool::Brush,
            VelocityTool::Line,
            VelocityTool::Curve,
            VelocityTool::Random,
        ]
    }
}

impl From<Tool> for VelocityTool {
    fn from(tool: Tool) -> Self {
        match tool {
            Tool::Pointer => VelocityTool::Pointer,
            Tool::Pencil => VelocityTool::Brush,
            Tool::Brush => VelocityTool::Brush,
            Tool::Pen => VelocityTool::Curve,
            _ => VelocityTool::Pointer,
        }
    }
}

/// 力度编辑状态
#[derive(Debug, Clone, Default, PartialEq)]
pub enum VelocityEditState {
    /// 空闲状态
    #[default]
    Idle,
    /// 拖拽瞄点
    DraggingHandle {
        note_index: usize,
        start_y: f32,
        start_velocity: u8,
    },
    /// 画笔绘制中
    Drawing {
        start_x: f32,
        start_velocity: u8,
        current_x: f32,
        current_velocity: u8,
    },
    /// 直线绘制中（Shift+画笔）
    DrawingLine {
        start_x: f32,
        start_velocity: u8,
        current_x: f32,
        current_velocity: u8,
    },
}

/// 瞄点信息
#[derive(Debug, Clone)]
pub struct VelocityHandle {
    /// 对应的音符索引
    pub note_index: usize,
    /// 屏幕位置
    pub center: Point,
    /// 当前力度值
    pub velocity: u8,
    /// 是否选中
    pub selected: bool,
}

impl VelocityEditor {
    /// 创建新的力度编辑器
    pub fn new() -> Self {
        Self {
            visible: true,
            height: 200.0,
            current_tool: VelocityTool::Pointer,
            edit_state: VelocityEditState::Idle,
            handle_radius: 8.0,
            bar_width: 28.0,
            bar_spacing: 40.0,
            margin: Margin::default(),
            shift_pressed: false,
            brush_start: None,
        }
    }

    /// 切换可见性
    pub fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
    }

    /// 设置当前工具
    pub fn set_tool(&mut self, tool: VelocityTool) {
        self.current_tool = tool;
        // 切换工具时重置编辑状态
        self.edit_state = VelocityEditState::Idle;
        self.brush_start = None;
    }

    /// 计算绘制区域
    pub fn draw_area(&self, total_width: f32) -> Rectangle {
        Rectangle::new(
            Point::new(self.margin.left, self.margin.top),
            Size::new(
                total_width - self.margin.left - self.margin.right,
                self.height - self.margin.top - self.margin.bottom,
            ),
        )
    }

    /// 力度值转换为Y坐标（从底部开始）
    pub fn velocity_to_y(&self, velocity: u8, draw_area: &Rectangle) -> f32 {
        let normalized = velocity as f32 / 127.0;
        draw_area.y + draw_area.height * (1.0 - normalized)
    }

    /// Y坐标转换为力度值
    pub fn y_to_velocity(&self, y: f32, draw_area: &Rectangle) -> u8 {
        let relative_y = (y - draw_area.y).clamp(0.0, draw_area.height);
        let normalized = 1.0 - (relative_y / draw_area.height);
        (normalized * 127.0).clamp(0.0, 127.0) as u8
    }

    /// 获取音符对应的X坐标
    pub fn note_to_x(&self, note_index: usize, _notes: &[Note], draw_area: &Rectangle) -> f32 {
        draw_area.x + (self.bar_spacing * note_index as f32) + (self.bar_width / 2.0)
    }

    /// 获取X坐标对应的音符索引
    pub fn x_to_note_index(&self, x: f32, _notes: &[Note], draw_area: &Rectangle) -> Option<usize> {
        let relative_x = x - draw_area.x;
        let index = (relative_x / self.bar_spacing).floor() as usize;
        // 检查是否在有效范围内
        if relative_x >= 0.0 && relative_x < draw_area.width {
            Some(index)
        } else {
            None
        }
    }

    /// 计算所有瞄点位置
    pub fn calculate_handles(&self, notes: &[Note], selected: &[usize], draw_area: &Rectangle) -> Vec<VelocityHandle> {
        notes
            .iter()
            .enumerate()
            .map(|(index, note)| {
                let x = self.note_to_x(index, notes, draw_area);
                let y = self.velocity_to_y(note.velocity, draw_area);
                VelocityHandle {
                    note_index: index,
                    center: Point::new(x, y),
                    velocity: note.velocity,
                    selected: selected.contains(&index),
                }
            })
            .collect()
    }

    /// 检测点击是否命中瞄点
    pub fn hit_test_handle(&self, pos: Point, handles: &[VelocityHandle]) -> Option<usize> {
        for (index, handle) in handles.iter().enumerate() {
            let dx = pos.x - handle.center.x;
            let dy = pos.y - handle.center.y;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance <= self.handle_radius * 1.5 {
                return Some(index);
            }
        }
        None
    }

    /// 处理鼠标按下事件
    pub fn handle_pressed(&mut self, pos: Point, notes: &[Note], selected: &[usize], shift: bool) -> Option<VelocityAction> {
        let draw_area = self.draw_area(800.0); // 临时使用800px宽度
        let handles = self.calculate_handles(notes, selected, &draw_area);

        self.shift_pressed = shift;

        match self.current_tool {
            VelocityTool::Pointer => {
                // 光标工具：检查是否点击了瞄点
                if let Some(handle_index) = self.hit_test_handle(pos, &handles) {
                    let handle = &handles[handle_index];
                    self.edit_state = VelocityEditState::DraggingHandle {
                        note_index: handle.note_index,
                        start_y: pos.y,
                        start_velocity: handle.velocity,
                    };
                    return Some(VelocityAction::SelectNote(handle.note_index));
                }
            }
            VelocityTool::Brush | VelocityTool::Line => {
                // 画笔/直线工具：开始绘制
                if let Some(_note_index) = self.x_to_note_index(pos.x, notes, &draw_area) {
                    let velocity = self.y_to_velocity(pos.y, &draw_area);
                    self.brush_start = Some((pos.x, velocity));

                    if shift {
                        // Shift+画笔 = 直线模式
                        self.edit_state = VelocityEditState::DrawingLine {
                            start_x: pos.x,
                            start_velocity: velocity,
                            current_x: pos.x,
                            current_velocity: velocity,
                        };
                    } else {
                        // 普通画笔模式
                        self.edit_state = VelocityEditState::Drawing {
                            start_x: pos.x,
                            start_velocity: velocity,
                            current_x: pos.x,
                            current_velocity: velocity,
                        };
                    }
                    return Some(VelocityAction::StartBatchEdit);
                }
            }
            _ => {}
        }

        None
    }

    /// 处理鼠标移动事件
    pub fn handle_moved(&mut self, pos: Point, notes: &[Note]) -> Option<VelocityAction> {
        let draw_area = self.draw_area(800.0);

        match &mut self.edit_state {
            VelocityEditState::DraggingHandle { note_index, start_y, start_velocity } => {
                let delta_y = pos.y - *start_y;
                let velocity_delta = -(delta_y / draw_area.height * 127.0) as i32;
                let new_velocity = (*start_velocity as i32 + velocity_delta).clamp(0, 127) as u8;

                return Some(VelocityAction::SetVelocity {
                    note_index: *note_index,
                    velocity: new_velocity,
                });
            }
            VelocityEditState::Drawing { current_x, current_velocity, .. } => {
                *current_x = pos.x;
                *current_velocity = self.y_to_velocity(pos.y, &draw_area);

                // 计算影响范围内的音符
                if let Some((start_x, start_velocity)) = self.brush_start {
                    let cx = *current_x;
                    let cv = *current_velocity;
                    return self.calculate_brush_effect(start_x, start_velocity, cx, cv, notes, &draw_area);
                }
            }
            VelocityEditState::DrawingLine { start_x, start_velocity, current_x, current_velocity } => {
                *current_x = pos.x;
                *current_velocity = self.y_to_velocity(pos.y, &draw_area);

                // 直线插值
                let sx = *start_x;
                let sv = *start_velocity;
                let cx = *current_x;
                let cv = *current_velocity;
                return self.calculate_line_effect(sx, sv, cx, cv, notes, &draw_area);
            }
            _ => {}
        }

        None
    }

    /// 处理鼠标释放事件
    pub fn handle_released(&mut self) -> Option<VelocityAction> {
        let action = match &self.edit_state {
            VelocityEditState::Drawing { .. } | VelocityEditState::DrawingLine { .. } => {
                Some(VelocityAction::EndBatchEdit)
            }
            _ => None,
        };

        self.edit_state = VelocityEditState::Idle;
        self.brush_start = None;
        action
    }

    /// 计算画笔效果
    fn calculate_brush_effect(
        &self,
        start_x: f32,
        start_velocity: u8,
        current_x: f32,
        current_velocity: u8,
        notes: &[Note],
        draw_area: &Rectangle,
    ) -> Option<VelocityAction> {
        let min_x = start_x.min(current_x);
        let max_x = start_x.max(current_x);

        let mut changes = Vec::new();

        for (index, _note) in notes.iter().enumerate() {
            let note_x = self.note_to_x(index, notes, draw_area);

            if note_x >= min_x && note_x <= max_x {
                // 计算该位置在绘制路径上的力度值
                let t = if max_x > min_x {
                    (note_x - min_x) / (max_x - min_x)
                } else {
                    0.0
                };

                let interpolated_velocity = if start_x < current_x {
                    start_velocity as f32 * (1.0 - t) + current_velocity as f32 * t
                } else {
                    current_velocity as f32 * (1.0 - t) + start_velocity as f32 * t
                };

                changes.push((index, interpolated_velocity.clamp(0.0, 127.0) as u8));
            }
        }

        if changes.is_empty() {
            None
        } else {
            Some(VelocityAction::BatchSetVelocity(changes))
        }
    }

    /// 计算直线效果（线性插值）
    fn calculate_line_effect(
        &self,
        start_x: f32,
        start_velocity: u8,
        end_x: f32,
        end_velocity: u8,
        notes: &[Note],
        draw_area: &Rectangle,
    ) -> Option<VelocityAction> {
        let min_x = start_x.min(end_x);
        let max_x = start_x.max(end_x);

        let mut changes = Vec::new();

        for (index, _note) in notes.iter().enumerate() {
            let note_x = self.note_to_x(index, notes, draw_area);

            if note_x >= min_x - self.bar_spacing / 2.0 && note_x <= max_x + self.bar_spacing / 2.0 {
                // 线性插值计算力度
                let t = if max_x > min_x {
                    ((note_x - min_x) / (max_x - min_x)).clamp(0.0, 1.0)
                } else {
                    0.0
                };

                let interpolated_velocity = if start_x < end_x {
                    start_velocity as f32 * (1.0 - t) + end_velocity as f32 * t
                } else {
                    end_velocity as f32 * (1.0 - t) + start_velocity as f32 * t
                };

                changes.push((index, interpolated_velocity.clamp(0.0, 127.0) as u8));
            }
        }

        if changes.is_empty() {
            None
        } else {
            Some(VelocityAction::BatchSetVelocity(changes))
        }
    }

    /// 设置Shift键状态
    pub fn set_shift_pressed(&mut self, pressed: bool) {
        self.shift_pressed = pressed;
    }
}

impl Default for VelocityEditor {
    fn default() -> Self {
        Self::new()
    }
}

/// 力度编辑器动作
#[derive(Debug, Clone)]
pub enum VelocityAction {
    /// 选择音符
    SelectNote(usize),
    /// 设置单个音符力度
    SetVelocity { note_index: usize, velocity: u8 },
    /// 批量设置力度
    BatchSetVelocity(Vec<(usize, u8)>),
    /// 开始批量编辑
    StartBatchEdit,
    /// 结束批量编辑
    EndBatchEdit,
}

/// 力度编辑器渲染器
pub struct VelocityEditorRenderer {
    /// 背景色
    pub background_color: Color,
    /// 力度条颜色
    pub bar_color: Color,
    /// 瞄点颜色
    pub handle_color: Color,
    /// 瞄点边框颜色
    pub handle_stroke_color: Color,
    /// 选中瞄点光晕颜色
    pub selected_glow_color: Color,
    /// 网格线颜色
    pub grid_line_color: Color,
    /// 文字颜色
    pub text_color: Color,
}

impl Default for VelocityEditorRenderer {
    fn default() -> Self {
        Self {
            background_color: Color::from_rgb(0.98, 0.98, 1.0),
            bar_color: Color::from_rgba(0.2, 0.6, 1.0, 0.9),
            handle_color: Color::WHITE,
            handle_stroke_color: Color::from_rgb(0.2, 0.6, 1.0),
            selected_glow_color: Color::from_rgba(0.2, 0.6, 1.0, 0.3),
            grid_line_color: Color::from_rgba(0.75, 0.75, 0.8, 0.5),
            text_color: Color::from_rgb(0.5, 0.5, 0.55),
        }
    }
}

impl VelocityEditorRenderer {
    /// 创建暗色主题渲染器
    pub fn dark_theme() -> Self {
        Self {
            background_color: Color::from_rgb(0.08, 0.08, 0.1),
            bar_color: Color::from_rgba(0.3, 0.7, 1.0, 0.85),
            handle_color: Color::from_rgb(0.15, 0.15, 0.18),
            handle_stroke_color: Color::from_rgb(0.3, 0.7, 1.0),
            selected_glow_color: Color::from_rgba(0.3, 0.7, 1.0, 0.2),
            grid_line_color: Color::from_rgba(0.3, 0.3, 0.35, 0.4),
            text_color: Color::from_rgb(0.6, 0.6, 0.65),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_velocity_to_y_conversion() {
        let editor = VelocityEditor::new();
        let draw_area = Rectangle::new(Point::new(0.0, 0.0), Size::new(100.0, 100.0));

        // 力度0应该在底部
        let y_0 = editor.velocity_to_y(0, &draw_area);
        assert!((y_0 - 100.0).abs() < 0.01);

        // 力度127应该在顶部
        let y_127 = editor.velocity_to_y(127, &draw_area);
        assert!(y_127.abs() < 0.01);

        // 力度64应该在中间
        let y_64 = editor.velocity_to_y(64, &draw_area);
        assert!((y_64 - 50.0).abs() < 1.0);
    }

    #[test]
    fn test_y_to_velocity_conversion() {
        let editor = VelocityEditor::new();
        let draw_area = Rectangle::new(Point::new(0.0, 0.0), Size::new(100.0, 100.0));

        // 底部应该是0
        let v_bottom = editor.y_to_velocity(100.0, &draw_area);
        assert_eq!(v_bottom, 0);

        // 顶部应该是127
        let v_top = editor.y_to_velocity(0.0, &draw_area);
        assert_eq!(v_top, 127);

        // 中间应该是约64
        let v_middle = editor.y_to_velocity(50.0, &draw_area);
        assert!((v_middle as i32 - 64).abs() <= 1);
    }
}
