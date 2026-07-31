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
                // 弯音值空间 → 屏幕像素（±满量程 = ±2 semitones = 2 个琴键高）
                let range_px = self.editor_state.view.zoom_y * PITCH_BEND_RANGE_SEMITONES as f32;
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

    /// 弯音值满量程对应的屏幕像素高度（±满量程 = ±2 semitones = 2 个琴键高）
    ///
    /// 注意：hy 的归一化语义是 value/MAX（±1.0 对应 ±8192），
    /// 所以基数只能是 `zoom_y * RANGE_SEMITONES`，**不能**再乘 (MAX-MIN)，
    /// 否则会放大 16382 倍把控制柄甩出屏幕（历史 bug：控制柄出现在 CC 面板区域）。
    fn handle_range_px(&self) -> f32 {
        self.editor_state.view.zoom_y * PITCH_BEND_RANGE_SEMITONES as f32
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

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_core::BendDrawMode;
    use lumino_core::pitch_bend::PitchBendAnchor;

    /// 构造处于弯音编辑模式的编辑器（base_key=60，两锚点 480/960，曲线模式）
    fn bend_editor() -> Editor {
        let mut editor = Editor::new();
        editor.editor_state.view.zoom_x = 0.1;
        editor.editor_state.view.zoom_y = 20.0;
        editor.editor_state.view.keyboard_width = 120.0;
        editor.editor_state.view.ruler_height = 24.0;
        editor.editor_state.enter_pitch_bend_mode(60, 0, 0);
        let curve = editor.editor_state.pitch_bend_curve.as_mut().unwrap();
        curve.mode = BendDrawMode::Curve;
        curve.insert_anchor(PitchBendAnchor::new(480, 0));
        curve.insert_anchor(PitchBendAnchor::new(960, 0));
        curve.selected_anchor = Some(0);
        editor
    }

    /// 回归测试：控制柄位置必须贴近锚点（±2 semitones = 2*zoom_y 像素内）。
    ///
    /// 历史 bug：基数误用 (MAX-MIN)*zoom_y*RANGE_SEMITONES（16382 倍放大），
    /// 控制柄被甩到 canvas 下方（用户看到的"CC 面板区域"）。
    #[test]
    fn test_handle_positions_stay_near_anchor() {
        let editor = bend_editor();
        let ax = editor.tick_to_x(480.0);
        let ay = editor.pitch_bend_value_to_y(0);

        // 默认手柄展示在段中点：x 偏移 = 0.5 * 段宽 = 0.5 * 480 * 0.1 = 24px
        let (hx, hy) = editor
            .pitch_bend_handle_out_screen_pos(0)
            .expect("选中锚点应显示出控制柄");
        assert!(
            (hx - ax - 24.0).abs() < 0.01,
            "控制柄 x 应在段中点，实际偏移 {}px",
            hx - ax
        );
        // y 必须贴近锚点（未拖拽时 = 锚点 y，误差 < 1px）
        assert!(
            (hy - ay).abs() < 1.0,
            "控制柄 y 应贴近锚点，实际偏移 {}px（历史 bug 会偏移数千像素）",
            hy - ay
        );

        // 入控制柄同样贴近锚点
        let (hx, hy) = editor
            .pitch_bend_handle_in_screen_pos(1)
            .expect("中间锚点应显示入控制柄");
        let ax1 = editor.tick_to_x(960.0);
        let ay1 = editor.pitch_bend_value_to_y(0);
        assert!(
            (hx - ax1 + 24.0).abs() < 0.01 && (hy - ay1).abs() < 1.0,
            "入控制柄应在锚点左侧段中点附近，实际 ({hx}, {hy}) vs 锚点 ({ax1}, {ay1})"
        );
    }

    /// 控制柄拖拽必须跟随鼠标（scratch-paint 行为），且对称模式下入控制柄镜像
    #[test]
    fn test_handle_drag_follows_mouse() {
        let mut editor = bend_editor();
        let ax = editor.tick_to_x(480.0);
        let ay = editor.pitch_bend_value_to_y(0);
        let (hx, _) = editor
            .pitch_bend_handle_out_screen_pos(0)
            .expect("应显示出控制柄");

        // 按下手柄（命中检测）
        editor.handle_pitch_bend_pressed(Point2::new(hx, ay), false);
        assert!(
            matches!(
                editor.pitch_bend_drag_state,
                PitchBendDragState::DragHandle(0, HandleSide::Out, false)
            ),
            "按下手柄应进入 DragHandle 状态"
        );

        // 拖动到锚点右上方：x +12px（段宽 48px → out_x=0.25），y 上移 10px（满量程 40px → out_y=0.25）
        editor.handle_pitch_bend_moved(Point2::new(ax + 12.0, ay - 10.0));

        let curve = editor.editor_state.pitch_bend_curve.as_ref().unwrap();
        let a = &curve.anchors[0];
        assert!(
            (a.handle_out_x - 0.25).abs() < 0.01 && (a.handle_out_y - 0.25).abs() < 0.01,
            "控制柄参数应跟随鼠标：out=({}, {})，期望 (0.25, 0.25)",
            a.handle_out_x,
            a.handle_out_y
        );
        // 对称模式：入控制柄镜像
        assert!(
            (a.handle_in_x + a.handle_out_x).abs() < 0.01
                && (a.handle_in_y + a.handle_out_y).abs() < 0.01,
            "对称模式下入控制柄应镜像出控制柄"
        );
        // 显示位置应与鼠标一致（±1px 内）
        let (hx2, hy2) = editor
            .pitch_bend_handle_out_screen_pos(0)
            .expect("拖拽后仍应显示控制柄");
        assert!(
            (hx2 - (ax + 12.0)).abs() < 1.0 && (hy2 - (ay - 10.0)).abs() < 1.0,
            "手柄应跟随鼠标：显示 ({hx2}, {hy2}) vs 鼠标 ({}, {})",
            ax + 12.0,
            ay - 10.0
        );
    }

    /// 边界锚点：第一个无入控制柄、最后一个无出控制柄；直线模式不显示手柄
    #[test]
    fn test_handle_boundaries_and_line_mode() {
        let editor = bend_editor();
        // 第一个锚点：无入
        assert!(editor.pitch_bend_handle_in_screen_pos(0).is_none());
        // 最后一个锚点：无出
        assert!(editor.pitch_bend_handle_out_screen_pos(1).is_none());

        // 直线模式：不显示任何手柄
        let mut editor = bend_editor();
        let curve = editor.editor_state.pitch_bend_curve.as_mut().unwrap();
        curve.mode = BendDrawMode::Line;
        assert!(editor.pitch_bend_handle_out_screen_pos(0).is_none());
        assert!(editor.pitch_bend_handle_in_screen_pos(1).is_none());
        // 直线模式点击段中点不应命中手柄
        let ax = editor.tick_to_x(480.0);
        let ay = editor.pitch_bend_value_to_y(0);
        editor.handle_pitch_bend_pressed(Point2::new(ax + 24.0, ay), false);
        assert!(
            matches!(editor.pitch_bend_drag_state, PitchBendDragState::Idle)
                || matches!(
                    editor.pitch_bend_drag_state,
                    PitchBendDragState::MoveAnchor(_)
                ),
            "直线模式不应进入 DragHandle 状态"
        );
    }
}
