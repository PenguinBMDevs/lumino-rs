//! 弯音编辑器交互处理
//!
//! 在弯音编辑模式下拦截鼠标事件，处理锚点创建、移动、删除、
//! 控制柄拖拽（对称/非对称切换）和命中检测。

use crate::Editor;
use lumino_core::BendDrawMode;
use lumino_core::pitch_bend::{
    PITCH_BEND_MAX, PITCH_BEND_MIN, PITCH_BEND_RANGE_SEMITONES, PitchBendAnchor,
};
use lumino_ui_core::message::Point2;

/// 命中半径（屏幕像素，除以 zoom_y 后转为值空间）
const ANCHOR_HIT_RADIUS_PX: f32 = 8.0;
/// 控制柄命中半径
const HANDLE_HIT_RADIUS_PX: f32 = 6.0;
/// 控制柄默认展示位置（直线段手柄未定义时展示在段中点，便于拖出控制柄）
const HANDLE_DEFAULT_POS: f32 = 0.5;

/// 控制柄方向（贝塞尔段两侧）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleSide {
    /// 出控制柄：控制「本锚点 → 下一锚点」段
    Out,
    /// 入控制柄：控制「上一锚点 → 本锚点」段
    In,
}

/// 弯音拖拽状态
#[derive(Debug, Clone, Copy, Default)]
pub enum PitchBendDragState {
    /// 无拖拽
    #[default]
    Idle,
    /// 拖拽锚点（锚点索引）
    MoveAnchor(usize),
    /// 拖拽控制柄（锚点索引, 入/出, 是否Alt非对称）
    DragHandle(usize, HandleSide, bool),
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
            PitchBendHit::Handle(idx, side) => {
                self.pitch_bend_drag_state = PitchBendDragState::DragHandle(idx, side, shift);
            }
            PitchBendHit::None => {
                // 弯音模式：点击空白处创建锚点并立即进入拖拽
                if let Some(idx) = self.pitch_bend_try_create_anchor(pos.x, pos.y) {
                    self.pitch_bend_drag_state = PitchBendDragState::MoveAnchor(idx);
                }
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
            PitchBendDragState::DragHandle(idx, side, alt_asymmetric) => {
                let Some(curve) = &self.editor_state.pitch_bend_curve else {
                    return;
                };
                if idx >= curve.anchors.len() {
                    return;
                }
                let anchor = &curve.anchors[idx];
                let ax = self.tick_to_x(anchor.tick as f32);
                let ay = self.pitch_bend_value_to_y(anchor.value);
                // 弯音值空间 → 屏幕像素（±2 semitones 对应满量程）
                let range_px = (PITCH_BEND_MAX as f32 - PITCH_BEND_MIN as f32)
                    * self.editor_state.view.zoom_y
                    * PITCH_BEND_RANGE_SEMITONES as f32;
                let zoom_x = self.editor_state.view.zoom_x;

                // 控制柄跟随鼠标（scratch-paint 行为）：
                // hx = 手柄相对锚点的 x 偏移 / 段宽（归一化）
                // hy = 锚点与鼠标的 y 差 / 满量程（value 增大方向 = 屏幕上方）
                let (seg_px, is_out) = match side {
                    HandleSide::Out => {
                        let Some(next) = curve.anchors.get(idx + 1) else {
                            return;
                        };
                        let seg = (next.tick.saturating_sub(anchor.tick)).max(1) as f32 * zoom_x;
                        (seg, true)
                    }
                    HandleSide::In => {
                        if idx == 0 {
                            return;
                        }
                        let prev_tick = curve.anchors[idx - 1].tick;
                        let seg = (anchor.tick.saturating_sub(prev_tick)).max(1) as f32 * zoom_x;
                        (seg, false)
                    }
                };
                let hx = ((pos.x - ax) / seg_px.max(1.0)).clamp(-0.5, 0.5);
                let hy = ((ay - pos.y) / range_px.max(1.0)).clamp(-1.0, 1.0);

                // 写回 anchor
                if let Some(curve) = self.editor_state.pitch_bend_curve.as_mut()
                    && idx < curve.anchors.len()
                {
                    let anchor = &mut curve.anchors[idx];
                    if is_out {
                        anchor.handle_out_x = hx;
                        anchor.handle_out_y = hy;
                        if !alt_asymmetric {
                            anchor.symmetrize_in_from_out();
                        }
                    } else {
                        anchor.handle_in_x = hx;
                        anchor.handle_in_y = hy;
                        if !alt_asymmetric {
                            anchor.symmetrize_out_from_in();
                        }
                    }
                }
            }
        }
    }

    /// 弯音编辑模式：处理鼠标释放
    pub fn handle_pitch_bend_released(&mut self) {
        self.pitch_bend_drag_state = PitchBendDragState::Idle;
    }

    /// 尝试创建锚点，返回新锚点索引（供调用方进入拖拽状态）
    fn pitch_bend_try_create_anchor(&mut self, x: f32, y: f32) -> Option<usize> {
        let tick = self.x_to_tick(x).max(0.0) as u32;
        let value = self.pitch_bend_y_to_value(y);

        let anchor = PitchBendAnchor::new(tick, value);
        if let Some(curve) = self.editor_state.pitch_bend_curve.as_mut() {
            let idx = curve.insert_anchor(anchor);
            curve.selected_anchor = Some(idx);
            Some(idx)
        } else {
            None
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

        // 优先检测选中锚点的控制柄（出 → 入）
        if let Some(idx) = curve.selected_anchor {
            if let Some(handle_pos) = self.pitch_bend_handle_out_screen_pos(idx) {
                let dist_sq = (handle_pos.0 - x).powi(2) + (handle_pos.1 - y).powi(2);
                if dist_sq <= HANDLE_HIT_RADIUS_PX.powi(2) {
                    return PitchBendHit::Handle(idx, HandleSide::Out);
                }
            }
            if let Some(handle_pos) = self.pitch_bend_handle_in_screen_pos(idx) {
                let dist_sq = (handle_pos.0 - x).powi(2) + (handle_pos.1 - y).powi(2);
                if dist_sq <= HANDLE_HIT_RADIUS_PX.powi(2) {
                    return PitchBendHit::Handle(idx, HandleSide::In);
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

    /// 锚点出控制柄的屏幕位置（scratch-paint 风格）
    ///
    /// 返回 `None` 表示该锚点不显示出手柄：
    /// - 非曲线模式（直线模式无贝塞尔控制柄）
    /// - 该锚点是最后一个锚点（无「本锚点 → 下一锚点」段）
    ///
    /// 手柄参数为 (0,0)（直线段）时展示在段中点，便于拖出控制柄。
    pub fn pitch_bend_handle_out_screen_pos(&self, idx: usize) -> Option<(f32, f32)> {
        let curve = self.editor_state.pitch_bend_curve.as_ref()?;
        if curve.mode != BendDrawMode::Curve {
            return None;
        }
        let anchor = curve.anchors.get(idx)?;
        let next = curve.anchors.get(idx + 1)?;
        let ax = self.tick_to_x(anchor.tick as f32);
        let ay = self.pitch_bend_value_to_y(anchor.value);
        let seg_px =
            (next.tick.saturating_sub(anchor.tick)).max(1) as f32 * self.editor_state.view.zoom_x;
        let hx = if anchor.has_handle_out() {
            anchor.handle_out_x
        } else {
            HANDLE_DEFAULT_POS
        };
        let hy = anchor.handle_out_y;
        Some((ax + hx * seg_px, ay - hy * self.handle_range_px()))
    }

    /// 锚点入控制柄的屏幕位置（scratch-paint 风格）
    ///
    /// 返回 `None` 表示该锚点不显示入手柄：
    /// - 非曲线模式
    /// - 该锚点是第一个锚点（无「上一锚点 → 本锚点」段）
    ///
    /// 手柄参数为 (0,0)（直线段）时展示在段中点，便于拖出控制柄。
    pub fn pitch_bend_handle_in_screen_pos(&self, idx: usize) -> Option<(f32, f32)> {
        let curve = self.editor_state.pitch_bend_curve.as_ref()?;
        if curve.mode != BendDrawMode::Curve {
            return None;
        }
        if idx == 0 {
            return None;
        }
        let anchor = curve.anchors.get(idx)?;
        let prev_tick = curve.anchors[idx - 1].tick;
        let ax = self.tick_to_x(anchor.tick as f32);
        let ay = self.pitch_bend_value_to_y(anchor.value);
        let seg_px =
            (anchor.tick.saturating_sub(prev_tick)).max(1) as f32 * self.editor_state.view.zoom_x;
        let hx = if anchor.has_handle_in() {
            anchor.handle_in_x
        } else {
            -HANDLE_DEFAULT_POS
        };
        let hy = anchor.handle_in_y;
        Some((ax + hx * seg_px, ay - hy * self.handle_range_px()))
    }

    /// 弯音值满量程对应的屏幕像素高度（value 增大方向 = 屏幕上方）
    fn handle_range_px(&self) -> f32 {
        (PITCH_BEND_MAX as f32 - PITCH_BEND_MIN as f32)
            * self.editor_state.view.zoom_y
            * PITCH_BEND_RANGE_SEMITONES as f32
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
}

/// 命中检测结果
#[derive(Debug, Clone, Copy)]
enum PitchBendHit {
    None,
    Anchor(usize),
    Handle(usize, HandleSide),
}
