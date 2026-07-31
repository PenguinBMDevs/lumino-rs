//! 弯音编辑器交互处理
//!
//! 在弯音编辑模式下拦截鼠标事件，处理锚点创建、移动、删除、
//! 控制柄拖拽（对称/非对称切换）和命中检测。

use crate::Editor;
use lumino_core::pitch_bend::{
    PITCH_BEND_MAX, PITCH_BEND_MIN, PITCH_BEND_RANGE_SEMITONES, PitchBendAnchor,
};
use lumino_ui_core::message::Point2;

/// 命中半径（屏幕像素，除以 zoom_y 后转为值空间）
const ANCHOR_HIT_RADIUS_PX: f32 = 8.0;
/// 控制柄命中半径
const HANDLE_HIT_RADIUS_PX: f32 = 6.0;

/// 弯音拖拽状态
#[derive(Debug, Clone, Copy, Default)]
pub enum PitchBendDragState {
    /// 无拖拽
    #[default]
    Idle,
    /// 拖拽锚点（锚点索引）
    MoveAnchor(usize),
    /// 拖拽出控制柄（锚点索引, 是否Alt非对称）
    DragHandle(usize, bool),
}

impl Editor {
    /// 弯音编辑模式：处理鼠标按下
    pub fn handle_pitch_bend_pressed(&mut self, pos: Point2, shift: bool) {
        // 弯音模式下不依赖 tool 枚举（工具栏切换可能覆盖），
        // 而是根据是否按住 Alt 键决定模式：
        // - 默认：创建/选中/移动锚点
        // - Alt：非对称控制柄拖拽
        // - 右键(Eraser 语义)：删除锚点
        let is_delete = self.editor_state.tool == lumino_core::Tool::Eraser;

        if is_delete {
            self.pitch_bend_try_delete_anchor(pos.x, pos.y);
            return;
        }

        // 命中检测
        let hit = self.pitch_bend_hit_test(pos.x, pos.y);

        match hit {
            PitchBendHit::Anchor(idx) => {
                if let Some(curve) = self.editor_state.pitch_bend_curve.as_mut() {
                    curve.selected_anchor = Some(idx);
                }
                self.pitch_bend_drag_state = PitchBendDragState::MoveAnchor(idx);
            }
            PitchBendHit::Handle(idx) => {
                self.pitch_bend_drag_state = PitchBendDragState::DragHandle(idx, shift);
            }
            PitchBendHit::None => {
                // 弯音模式：点击空白处直接创建锚点（不检查 tool 枚举）
                self.pitch_bend_try_create_anchor(pos.x, pos.y);
            }
        }
    }

    /// 弯音编辑模式：处理鼠标移动
    pub fn handle_pitch_bend_moved(&mut self, pos: Point2) {
        match self.pitch_bend_drag_state {
            PitchBendDragState::Idle => {}
            PitchBendDragState::MoveAnchor(idx) => {
                // 先计算新值（避免借用冲突）
                let tick = self.x_to_tick(pos.x).max(0.0) as u32;
                let value = self.pitch_bend_y_to_value(pos.y);

                if let Some(curve) = self.editor_state.pitch_bend_curve.as_mut()
                    && idx < curve.anchors.len()
                {
                    let mut anchor = curve.anchors.remove(idx);
                    anchor.tick = tick;
                    anchor.value = value;
                    let new_idx = curve.insert_anchor(anchor);
                    curve.selected_anchor = Some(new_idx);
                }
            }
            PitchBendDragState::DragHandle(idx, alt_asymmetric) => {
                // 先取 curve 引用计算邻居数据和基值
                let (span, zoom_x, zoom_y, next_screen_x, base_y, value_range) =
                    if let Some(curve) = &self.editor_state.pitch_bend_curve {
                        if idx >= curve.anchors.len() {
                            return;
                        }
                        let (prev_tick, _, next_tick, _) = get_neighbor_values(curve, idx);
                        let span = (next_tick.saturating_sub(prev_tick)).max(1) as f32;
                        let value_range = (PITCH_BEND_MAX as f32) - (PITCH_BEND_MIN as f32);
                        let prev_value = curve.anchors[idx].value;
                        let zoom_x = self.editor_state.view.zoom_x;
                        let zoom_y = self.editor_state.view.zoom_y;
                        let next_screen_x = self.tick_to_x(next_tick as f32);
                        let base_y = self.pitch_bend_value_to_y(prev_value);
                        (span, zoom_x, zoom_y, next_screen_x, base_y, value_range)
                    } else {
                        return;
                    };

                // 计算偏移量
                let out_x = ((next_screen_x - pos.x).max(0.0) / span / zoom_x).clamp(-0.5, 0.5);
                let out_y = ((pos.y - base_y)
                    / (value_range.abs() * zoom_y * PITCH_BEND_RANGE_SEMITONES as f32))
                    .clamp(-1.0, 1.0);

                // 写回 anchor
                if let Some(curve) = self.editor_state.pitch_bend_curve.as_mut()
                    && idx < curve.anchors.len()
                {
                    let anchor = &mut curve.anchors[idx];
                    anchor.handle_out_x = out_x;
                    anchor.handle_out_y = out_y;
                    if !alt_asymmetric {
                        anchor.symmetrize_in_from_out();
                    }
                }
            }
        }
    }

    /// 弯音编辑模式：处理鼠标释放
    pub fn handle_pitch_bend_released(&mut self) {
        self.pitch_bend_drag_state = PitchBendDragState::Idle;
    }

    /// 尝试创建锚点
    fn pitch_bend_try_create_anchor(&mut self, x: f32, y: f32) {
        let tick = self.x_to_tick(x).max(0.0) as u32;
        let value = self.pitch_bend_y_to_value(y);

        let anchor = PitchBendAnchor::new(tick, value);
        if let Some(curve) = self.editor_state.pitch_bend_curve.as_mut() {
            let idx = curve.insert_anchor(anchor);
            curve.selected_anchor = Some(idx);
        }
    }

    /// 尝试删除锚点
    fn pitch_bend_try_delete_anchor(&mut self, x: f32, y: f32) {
        // 先计算所有锚点的屏幕坐标和命中状态
        let hit_idx = if let Some(curve) = &self.editor_state.pitch_bend_curve {
            let mut found = None;
            for (i, anchor) in curve.anchors.iter().enumerate() {
                let ax = self.tick_to_x(anchor.tick as f32);
                let ay = self.pitch_bend_value_to_y(anchor.value);
                let dist_sq = (ax - x).powi(2) + (ay - y).powi(2);
                if dist_sq <= ANCHOR_HIT_RADIUS_PX.powi(2) {
                    found = Some(i);
                    break;
                }
            }
            found
        } else {
            None
        };

        // 再执行删除
        if let Some(i) = hit_idx
            && let Some(curve) = self.editor_state.pitch_bend_curve.as_mut()
        {
            curve.remove_anchor(i);
            if curve.selected_anchor == Some(i) {
                curve.selected_anchor = None;
            } else if let Some(s) = curve.selected_anchor
                && s > i
            {
                curve.selected_anchor = Some(s - 1);
            }
        }
    }

    /// 命中检测
    fn pitch_bend_hit_test(&self, x: f32, y: f32) -> PitchBendHit {
        let Some(curve) = self.editor_state.pitch_bend_curve.as_ref() else {
            return PitchBendHit::None;
        };

        // 优先检测选中锚点的控制柄
        if let Some(idx) = curve.selected_anchor {
            let anchor = &curve.anchors[idx];
            // 检测出控制柄位置
            if anchor.has_handle_out() {
                let handle_pos = self.pitch_bend_handle_out_screen_pos(anchor, curve, idx);
                let dist_sq = (handle_pos.0 - x).powi(2) + (handle_pos.1 - y).powi(2);
                if dist_sq <= HANDLE_HIT_RADIUS_PX.powi(2) {
                    return PitchBendHit::Handle(idx);
                }
            }
        }

        // 检测锚点
        for (i, anchor) in curve.anchors.iter().enumerate() {
            let ax = self.tick_to_x(anchor.tick as f32);
            let ay = self.pitch_bend_value_to_y(anchor.value);
            let dist_sq = (ax - x).powi(2) + (ay - y).powi(2);
            if dist_sq <= ANCHOR_HIT_RADIUS_PX.powi(2) {
                return PitchBendHit::Anchor(i);
            }
        }

        PitchBendHit::None
    }

    /// Y 坐标转弯音值（以选中音符为中心，±2 琴键高度映射 ±8192）
    pub fn pitch_bend_y_to_value(&self, y: f32) -> i16 {
        let Some(curve) = self.editor_state.pitch_bend_curve.as_ref() else {
            return 0;
        };

        let base_y = self.key_to_y(curve.base_key);
        // ±2 琴键高度对应 ±8192
        let semitone_height = self.editor_state.view.zoom_y;
        let two_semitones = semitone_height * PITCH_BEND_RANGE_SEMITONES as f32;

        // y 向下为正，弯音正值向上，所以反转
        let delta_y = base_y - y;
        let value = (delta_y / two_semitones * PITCH_BEND_MAX as f32).round() as i16;
        value.clamp(PITCH_BEND_MIN, PITCH_BEND_MAX)
    }

    /// 弯音值转 Y 坐标
    pub fn pitch_bend_value_to_y(&self, value: i16) -> f32 {
        let Some(curve) = self.editor_state.pitch_bend_curve.as_ref() else {
            return 0.0;
        };

        let base_y = self.key_to_y(curve.base_key);
        let semitone_height = self.editor_state.view.zoom_y;
        let two_semitones = semitone_height * PITCH_BEND_RANGE_SEMITONES as f32;

        // value 正 -> y 减小（向上）
        base_y - (value as f32 / PITCH_BEND_MAX as f32) * two_semitones
    }

    /// 计算出控制柄的屏幕位置
    fn pitch_bend_handle_out_screen_pos(
        &self,
        anchor: &PitchBendAnchor,
        curve: &lumino_core::pitch_bend::PitchBendCurve,
        idx: usize,
    ) -> (f32, f32) {
        let (prev_tick, _, next_tick, _next_value) = get_neighbor_values(curve, idx);

        let span = (next_tick.saturating_sub(prev_tick)).max(1) as f32;
        let ax = self.tick_to_x(anchor.tick as f32);
        let ay = self.pitch_bend_value_to_y(anchor.value);

        let handle_x = ax + anchor.handle_out_x * span * self.editor_state.view.zoom_x;
        let value_range = (PITCH_BEND_MAX as f32) - (PITCH_BEND_MIN as f32);
        let handle_y = ay
            + anchor.handle_out_y
                * value_range
                * self.editor_state.view.zoom_y
                * PITCH_BEND_RANGE_SEMITONES as f32;

        (handle_x, handle_y)
    }
}

/// 命中检测结果
#[derive(Debug, Clone, Copy)]
enum PitchBendHit {
    None,
    Anchor(usize),
    Handle(usize),
}

/// 获取锚点的邻居值（用于控制柄位置计算）
fn get_neighbor_values(
    curve: &lumino_core::pitch_bend::PitchBendCurve,
    idx: usize,
) -> (u32, i16, u32, i16) {
    let prev_tick = if idx > 0 {
        curve.anchors[idx - 1].tick
    } else {
        curve.anchors.first().map(|a| a.tick).unwrap_or(0)
    };
    let prev_value = if idx > 0 {
        curve.anchors[idx - 1].value
    } else {
        curve.anchors.first().map(|a| a.value).unwrap_or(0)
    };
    let next_tick = if idx + 1 < curve.anchors.len() {
        curve.anchors[idx + 1].tick
    } else {
        curve.anchors.last().map(|a| a.tick).unwrap_or(0)
    };
    let next_value = if idx + 1 < curve.anchors.len() {
        curve.anchors[idx + 1].value
    } else {
        curve.anchors.last().map(|a| a.value).unwrap_or(0)
    };
    (prev_tick, prev_value, next_tick, next_value)
}
